//! The vehicle's script variables and triggers as the game reads and sets them, and the
//! services (fuel, wash, repair) and the odometer.

use super::*;

/// The fuel in the tank as the bus scripts keep it (litres; OMSI 2's stock engine scripts).
const TANK_CONTENT: &str = "engine_tank_content";

impl VehicleInstance {
    pub(super) fn get(&self, id: Option<omsi_script::VarId>) -> f32 {
        id.map(|i| self.state.vars[i as usize]).unwrap_or(0.0)
    }

    pub(super) fn put(&mut self, id: Option<omsi_script::VarId>, v: f32) {
        if let Some(i) = id {
            self.state.vars[i as usize] = v;
        }
    }

    /// The bus's total mileage, as Omsi.exe 0x64207E writes it into a situation:
    /// the starting reading plus the kilometres driven since it was loaded.
    pub fn odometer_km(&self) -> f64 {
        self.host.km_base + self.driven_km
    }

    /// Continue a saved reading without giving the bus another random service history.
    /// Omsi.exe clears driven km at 0x643DBB and restores the total into the starting
    /// reading at 0x643DF3; subtract our already driven distance to keep the same total
    /// even when the caller has ticked the vehicle before restoring its situation.
    pub fn set_odometer_km(&mut self, km: f64) {
        self.host.km_base = km - self.driven_km;
        self.km_started = true;
    }

    /// Where variable `name` sits among the script's variables (`State::vars`).
    pub fn var_slot(&self, name: &str) -> Option<usize> {
        omsi_script::compile::with_lower(name, |k| self.var_index.get(k).map(|&i| i as usize))
    }

    pub fn var(&self, name: &str) -> Option<f32> {
        omsi_script::compile::with_lower(name, |k| self.var_index.get(k).copied())
            .map(|i| self.state.vars[i as usize])
    }

    /// The variables' names (lower case) and slots in `state.vars`, shared (the view reads a
    /// copy of the values by them: `crate::view_events::CarLooks`).
    pub fn var_index(&self) -> &std::sync::Arc<HashMap<String, omsi_script::VarId>> {
        &self.var_index
    }

    /// Whether variable `name` was declared in the vehicle's script set (as opposed to built-in host variables).
    pub fn has_script_var(&self, name: &str) -> bool {
        self.ty.program.has_script_var(name)
    }

    /// The door release to fire now: the bus's own, the C2's switch turned over, else `bus_dooraft`.
    pub fn door_release_trigger(&self) -> Option<&'static str> {
        let p = &self.ty.program;
        if let Some(t) = ["bus_doorall", "CG_active", "C2_CP_doorall_1"].into_iter().find(|t| p.trigger(t).is_some()) {
            return Some(t);
        }
        if p.trigger("C2_CP_Tuerfreigabe_1").is_some() && p.trigger("C2_CP_Tuerfreigabe_0").is_some() {
            return Some(if self.var("C2_CP_Tuerfreigabe").is_some_and(|x| x > 0.5) { "C2_CP_Tuerfreigabe_0" } else { "C2_CP_Tuerfreigabe_1" });
        }
        p.trigger("bus_dooraft").is_some().then_some("bus_dooraft")
    }

    /// The name of script variable `index` (lower case), for diagnostics and key helpers.
    pub fn var_name(&self, index: usize) -> Option<&str> {
        self.var_index
            .iter()
            .find(|(_, &i)| i as usize == index)
            .map(|(n, _)| n.as_str())
    }

    pub fn set_var(&mut self, name: &str, v: f32) -> bool {
        match omsi_script::compile::with_lower(name, |k| self.var_index.get(k).copied()) {
            Some(i) => {
                self.state.vars[i as usize] = v;
                true
            }
            None => false,
        }
    }

    /// Variables the engine feeds the model directly (`Dirt_Norm`, `AI`, …): OMSI's
    /// varlists rarely declare them, and an undeclared `[alphascale]` variable used to
    /// read as 1.0 - every bus wore its dirt film at full strength from the first frame.
    pub fn set_engine_var(&mut self, name: &str, v: f32) {
        if !self.set_var(name, v) {
            let id = self.state.vars.len() as omsi_script::VarId;
            self.state.vars.push(v);
            std::sync::Arc::make_mut(&mut self.var_index).insert(name.to_ascii_lowercase(), id);
            self.pax_vars = std::sync::OnceLock::new();
            self.engine_ids = std::sync::OnceLock::new();
        }
    }

    pub fn trigger(&mut self, name: &str) -> bool {
        let p = self.ty.program.clone();
        let rear_target_was_open = self.var("doorTarget_23").is_some_and(|target| target > 0.0);
        let rear_force_close_was_active = self.var("bdoor_embtn_cls").is_some_and(|close| close > 0.5);
        let rear_was_open = self.var("doorTarget_23").is_some_and(|target| target > 0.0)
            || self.var("door_2").is_some_and(|door| door > 0.05)
            || self.var("door_3").is_some_and(|door| door > 0.05);
        let mut fired = self
            .vm
            .run_trigger(&p, name, &mut self.state, &mut self.host);
        // Several Volvo Wright door scripts expose the actual force-close operation as
        // `bus_dooraft1_external_CL`, while the dashboard `bus_dooraftclose` trigger only
        // sounds the button when its handbrake guard rejects the request.  Use that explicit
        // close path when the dashboard request left the rear door open.  Other buses are
        // unaffected because the fallback trigger is only present in those scripts.
        // A toggle pressed while the leaves are already closing is an open request.  The
        // fallback must not immediately undo that request just because the leaves are still
        // physically open for a few frames.
        let toggle_is_reopen_request = name.eq_ignore_ascii_case("bus_dooraft")
            && !rear_target_was_open
            && (rear_force_close_was_active || self.var("doorTarget_23").is_some_and(|target| target > 0.0));
        if (name.eq_ignore_ascii_case("bus_dooraftclose")
            || (name.eq_ignore_ascii_case("bus_dooraft") && !toggle_is_reopen_request))
            && rear_was_open
            && (self.var("doorTarget_23").is_some_and(|target| target > 0.0)
                || self.var("door_2").is_some_and(|door| door > 0.05)
                || self.var("door_3").is_some_and(|door| door > 0.05))
            && p.trigger("bus_dooraft1_external_CL").is_some()
        {
            fired |= self.vm.run_trigger(
                &p,
                "bus_dooraft1_external_CL",
                &mut self.state,
                &mut self.host,
            );
        }
        fired
    }

    /// The script variables as `names` would leave them, run one after another, with the
    /// vehicle left exactly as it was: its variables, the machine's random numbers, the
    /// sounds and messages the triggers asked for.
    pub fn trial_triggers(&mut self, names: &[&str]) -> Vec<f32> {
        let (state, vm) = (self.state.clone(), self.vm.clone());
        let (fired, fired_files, messages, time_written) = (self.host.fired_triggers.len(), self.host.fired_file_triggers.len(), self.host.messages.clone(), self.host.time_written);
        let fired_vars = self.host.fired_trigger_vars.len();
        for n in names {
            self.trigger(n);
        }
        let out = self.state.vars.clone();
        self.state = state;
        self.vm = vm;
        self.host.fired_triggers.truncate(fired);
        self.host.fired_trigger_vars.truncate(fired_vars);
        self.host.fired_file_triggers.truncate(fired_files);
        self.host.messages = messages;
        self.host.time_written = time_written;
        out
    }

    /// Fire a trigger and read the number the script leaves on the stack. OMSI asks the
    /// bus how long its repairs take this way (`malfunction_gettime`): every damaged system
    /// adds its minutes to the value passing through the chain of macros.
    pub fn trigger_value(&mut self, name: &str) -> Option<f32> {
        let p = self.ty.program.clone();
        self.vm.stacks = Default::default();
        if !self
            .vm
            .run_trigger(&p, name, &mut self.state, &mut self.host)
        {
            return None;
        }
        Some(self.vm.stacks.st[0])
    }

    /// Run one of the engine's service triggers with `secs` on the clock: OMSI holds
    /// `veh_tank` / `veh_wash` down while the pump or the wash runs and the bus script
    /// decides what a second of it is worth (the SD202 takes 3 litres and caps at 250).
    /// One step of a depot service: its trigger (`veh_tank`, `veh_wash`) with `secs` of
    /// the script's clock. False when the bus has no such handling.
    pub fn service(&mut self, name: &str, secs: f32) -> bool {
        let keep = self.host.clock.timegap;
        self.host.clock.timegap = secs;
        let ok = self.trigger(name);
        self.host.clock.timegap = keep;
        ok
    }

    /// Refuel until the tank stops filling. Returns the tank content the script reports.
    pub fn refuel(&mut self) -> Option<f32> {
        let mut last = f32::NEG_INFINITY;
        for _ in 0..600 {
            if !self.service("veh_tank", 1.0) {
                return None;
            }
            let now = self.var(TANK_CONTENT)?;
            if now <= last + 1e-3 {
                break;
            }
            last = now;
        }
        self.var(TANK_CONTENT)
    }

    /// (NexiconDK, Bus Company Online, 2 November 2026) How full the tank (an electric bus's
    /// battery) is, 0 to 1: the content as the scripts keep it (`engine_tank_content`) of a full
    /// tank as the pump found it when the company's level was set (`set_fuel_level`) - else the
    /// share the scripts work out for the gauge (`tank_percent`, 0 to 1, OMSI 2's stock engine
    /// and the Solaris Urbino's), else of the scripts' `engine_tank_capacity`. None: the bus does
    /// not tell.
    pub fn fuel_level(&self) -> Option<f32> {
        let content = self.var(TANK_CONTENT).filter(|c| c.is_finite());
        let of = |full: f32| content.map(|c| (c / full).clamp(0.0, 1.0));
        if let Some(full) = self.tank_full {
            return of(full);
        }
        if let Some(p) = self.var("tank_percent").filter(|p| p.is_finite()) {
            return Some(p.clamp(0.0, 1.0));
        }
        self.var("engine_tank_capacity").filter(|f| f.is_finite() && *f > 0.0).and_then(of)
    }

    /// (NexiconDK, Bus Company Online) Fill the tank to `level` (0 to 1) of a full one: filled up
    /// at the pump first - the bus's own `veh_tank`, five seconds of it at a time, until it takes
    /// no more (the diesel Urbino's 250 l at 3 l a second, the electric one's battery at 1 a
    /// second) - to find what full holds, then left with that share of it. What a full tank
    /// holds; None: the bus has no tank the game can fill (left as its scripts have it).
    pub fn set_fuel_level(&mut self, level: f32) -> Option<f32> {
        let mut full = self.var(TANK_CONTENT).filter(|c| c.is_finite())?;
        for _ in 0..400 {
            if !self.service("veh_tank", 5.0) {
                return None;
            }
            let now = self.var(TANK_CONTENT)?;
            if now <= full + 1e-3 {
                break;
            }
            full = now;
        }
        if !(full.is_finite() && full > 0.0) {
            return None;
        }
        self.tank_full = Some(full);
        let level = if level.is_finite() { level.clamp(0.0, 1.0) } else { 1.0 };
        self.set_var(TANK_CONTENT, full * level);
        Some(full)
    }

    /// Run the bus through the wash until it stops getting cleaner.
    pub fn wash(&mut self) -> Option<f32> {
        self.dirt = 0.0;
        self.set_engine_var("Dirt_Norm", 0.0);
        let mut last = f32::INFINITY;
        for _ in 0..600 {
            if !self.service("veh_wash", 1.0) {
                return None;
            }
            let now = self.var("Dirt_Wiped")?;
            if now >= last - 1e-4 {
                break;
            }
            last = now;
        }
        self.var("Dirt_Wiped")
    }

    /// Minutes the workshop needs for the damage the bus has right now.
    pub fn repair_minutes(&mut self) -> Option<f32> {
        self.trigger_value("malfunction_gettime")
    }

    /// Repair everything (the engine fires this once the driver accepts the waiting time).
    pub fn repair(&mut self) -> bool {
        self.trigger("malfunction_reset")
    }
}

#[cfg(test)]
mod fuel_tests {
    use crate::timetable_run::tests::script_test_vehicle;

    /// A tank as the Solaris Urbino's engine script keeps it: 250 l, a random content at its
    /// start, the gauge's share worked out every frame, and the pump's 3 l a second.
    const URBINO: &str = "{init}\n250 random 250 random + 250 min (S.L.engine_tank_content)\n{end}\n{frame}\n(L.L.engine_tank_content) 250 / (S.L.tank_percent)\n{end}\n{trigger:veh_tank}\n(L.L.engine_tank_content) (L.S.Timegap) 3 * + 250 min (S.L.engine_tank_content)\n{end}\n";

    /// (NexiconDK, Bus Company Online) The company's level of the tank: filled up at the pump to
    /// find what full holds, then left at that share - and read back as that share, as the
    /// bus's gauge has it.
    #[test]
    fn the_tank_is_left_at_the_companys_level_of_a_full_one() {
        let mut v = script_test_vehicle(URBINO, "engine_tank_content\ntank_percent\n", "");
        assert_eq!(v.set_fuel_level(0.4), Some(250.0));
        assert!((v.var("engine_tank_content").unwrap() - 100.0).abs() < 1e-3);
        assert!((v.fuel_level().unwrap() - 0.4).abs() < 1e-6);
        v.update_scripts_only(0.0);
        assert!((v.var("tank_percent").unwrap() - 0.4).abs() < 1e-6, "the gauge's");
        // (out of range: full; empty stays empty)
        assert_eq!(v.set_fuel_level(7.0), Some(250.0));
        assert_eq!(v.fuel_level(), Some(1.0));
        v.set_fuel_level(0.0);
        assert_eq!(v.fuel_level(), Some(0.0));
    }

    /// A bus the game cannot fill (no `veh_tank`) is left as its scripts have it; one whose
    /// level was never set is read by its gauge's share.
    #[test]
    fn a_tank_without_a_pump_is_left_alone_and_the_gauge_is_read() {
        let mut v = script_test_vehicle("{frame}\n(L.L.engine_tank_content) 200 / (S.L.tank_percent)\n{end}\n", "engine_tank_content\ntank_percent\n", "");
        v.set_var("engine_tank_content", 50.0);
        assert_eq!(v.set_fuel_level(1.0), None);
        assert_eq!(v.var("engine_tank_content"), Some(50.0));
        v.update_scripts_only(0.0);
        assert_eq!(v.fuel_level(), Some(0.25));
        let none = script_test_vehicle("{frame}\n{end}\n", "x\n", "");
        assert_eq!(none.fuel_level(), None);
    }
}
