//! (NexiconDK, Bus Company Online, 2 November 2026) A bus's own faults, as its scripts keep them:
//! the variables whose names say fault - `door_failure`, `elec_failure_general`, the Urbino's
//! `ecas_awaria_*` and `actia_error_*`, the VDV display's `vdv_error_EDC_active`, and the wear
//! that brings them (`EDC_Error_dist`, `Door_Error_Count`). A company's bus keeps them from one
//! drive to the next until its workshop puts it right (`fault_state`, `set_fault_state`); a fault
//! that stays on while the engine runs is told to the company (`FaultWatch`) - a warning that
//! goes with the start (the brakes' air before the compressor fills it) is not.

use super::VehicleInstance;
use std::collections::{BTreeSet, HashMap};

/// How long a fault stays on with the engine running before it is the workshop's (s).
pub const HELD_AFTER: f32 = 120.0;

/// Words of a variable's name that say it is of the bus's faults (lower case: English, German,
/// Polish).
const FAULT_WORDS: [&str; 9] = ["fail", "error", "awaria", "defekt", "defect", "fehler", "stoer", "malfunction", "usterk"];
/// Of those, what is no fault itself: a counter towards one, a display's page, a choice of part.
const NOT_FLAGS: [&str; 11] = ["dist", "count", "toleranz", "anzahl", "time", "pixel", "scroll", "multiple", "display_error", "blink", "pressure"];

/// Whether variable `name` is of the bus's faults (its state kept from drive to drive).
pub fn is_fault_var(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    FAULT_WORDS.iter().any(|w| n.contains(w)) && !n.contains("lusterk") && !n.contains("display_error")
}

/// Whether variable `name` is a fault that is on when it is not 0 (not a counter towards one,
/// nor the air pressure's warning).
pub fn is_fault_flag(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    is_fault_var(&n) && !NOT_FLAGS.iter().any(|w| n.contains(w))
}

/// A fault's name as the company's workshop says it.
pub fn readable(name: &str) -> String {
    let n = name.to_ascii_lowercase();
    let part = [
        (&["door", "tuer", "drzwi"][..], "Doors"),
        (&["ecas"][..], "Suspension (ECAS)"),
        (&["ebs", "brake", "bremse"][..], "Brakes (EBS)"),
        (&["retarder"][..], "Retarder"),
        (&["gear", "getriebe"][..], "Gearbox"),
        (&["edc", "emr", "engine", "motor"][..], "Engine"),
        (&["antrieb"][..], "Drive"),
        (&["battery", "elec", "batter"][..], "Electrics"),
        (&["coolant", "kuehl"][..], "Cooling"),
        (&["diesel", "fuel"][..], "Fuel system"),
        (&["master"][..], "Warning lamp"),
    ]
    .iter()
    .find(|(words, _)| words.iter().any(|w| n.contains(w)))
    .map(|(_, part)| part.to_string());
    part.unwrap_or_else(|| name.trim().to_string())
}

impl VehicleInstance {
    /// The bus's faults as its scripts keep them now: each fault variable that is not 0.
    pub fn fault_state(&self) -> Vec<(String, f32)> {
        let p = &self.ty.program;
        p.var_names
            .iter()
            .enumerate()
            .filter(|(_, n)| is_fault_var(n))
            .filter_map(|(i, n)| self.state.vars.get(i).copied().filter(|v| v.is_finite() && *v != 0.0).map(|v| (n.clone(), v)))
            .collect()
    }

    /// The faults `state` (`fault_state` of a drive before) set again: how many of its variables
    /// the bus has.
    pub fn set_fault_state(&mut self, state: &[(String, f32)]) -> usize {
        state.iter().filter(|(n, v)| is_fault_var(n) && v.is_finite() && self.set_var(n, *v)).count()
    }
}

/// The faults of the player's bus that stay on while its engine runs: each one's time on, and
/// those on long enough to be the workshop's (`HELD_AFTER`).
#[derive(Debug, Clone, Default)]
pub struct FaultWatch {
    on: HashMap<String, f32>,
    held: BTreeSet<String>,
    /// The flags of the bus's program (its variable slots and names), found once.
    flags: Option<(usize, Vec<(usize, String)>)>,
}

impl FaultWatch {
    /// `dt` seconds more of bus `v` (`engine`: its engine runs).
    pub fn tick(&mut self, v: &VehicleInstance, engine: bool, dt: f32) {
        let key = std::sync::Arc::as_ptr(&v.ty.program) as usize;
        if self.flags.as_ref().is_none_or(|(k, _)| *k != key) {
            let flags = v.ty.program.var_names.iter().enumerate().filter(|(_, n)| is_fault_flag(n)).map(|(i, n)| (i, n.clone())).collect();
            self.flags = Some((key, flags));
            self.on.clear();
        }
        let Some((_, flags)) = self.flags.as_ref() else { return };
        for (i, name) in flags {
            let lit = v.state.vars.get(*i).is_some_and(|x| x.is_finite() && *x != 0.0);
            if !lit {
                self.on.remove(name);
                continue;
            }
            if !engine {
                continue;
            }
            let t = self.on.entry(name.clone()).or_insert(0.0);
            *t += dt.max(0.0);
            if *t >= HELD_AFTER {
                self.held.insert(name.clone());
            }
        }
    }

    /// The faults that stayed on long enough, by the variables' names.
    pub fn held(&self) -> Vec<String> {
        self.held.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timetable_run::tests::script_test_vehicle;

    #[test]
    fn the_scripts_faults_are_known_by_their_names() {
        for n in ["door_failure", "elec_failure_general", "ecas_awaria_0_L", "actia_error_3", "vdv_error_EDC_active", "cockpit_light_masterfailure"] {
            assert!(is_fault_flag(n), "{n}");
        }
        for n in ["EDC_Error_dist", "Door_Error_Count", "Fehleranzahl"] {
            assert!(is_fault_var(n) && !is_fault_flag(n), "{n}: kept, no fault itself");
        }
        for n in ["vdv_error_pressure_active", "vdv_display_error", "lusterko_zew_typ", "Velocity"] {
            assert!(!is_fault_flag(n), "{n}");
        }
        assert_eq!(readable("vdv_error_EDC_active"), "Engine");
        assert_eq!(readable("ecas_awaria_1_R"), "Suspension (ECAS)");
        assert_eq!(readable("door_failure"), "Doors");
    }

    /// A fault on for two minutes with the engine running is the workshop's; one on without the
    /// engine, or off again before, is not. The faults' state goes with the bus to the next drive.
    #[test]
    fn a_fault_on_with_the_engine_running_is_held_and_kept() {
        let mut v = script_test_vehicle("{frame}\n{end}\n", "door_failure\nEDC_Error_dist\nvdv_error_EDC_active\nVelocity_x\n", "");
        let mut w = FaultWatch::default();
        v.set_var("door_failure", 1.0);
        for _ in 0..200 {
            w.tick(&v, false, 1.0);
        }
        assert!(w.held().is_empty(), "the engine off");
        for _ in 0..119 {
            w.tick(&v, true, 1.0);
        }
        assert!(w.held().is_empty(), "not two minutes yet");
        w.tick(&v, true, 1.0);
        assert_eq!(w.held(), vec!["door_failure".to_string()]);
        v.set_var("vdv_error_EDC_active", 1.0);
        for _ in 0..60 {
            w.tick(&v, true, 1.0);
        }
        v.set_var("vdv_error_EDC_active", 0.0);
        for _ in 0..100 {
            w.tick(&v, true, 1.0);
        }
        assert_eq!(w.held().len(), 1, "off again before two minutes");
        v.set_var("EDC_Error_dist", 1234.5);
        let state = v.fault_state();
        assert_eq!(state, vec![("door_failure".to_string(), 1.0), ("EDC_Error_dist".to_string(), 1234.5)]);
        let mut next = script_test_vehicle("{frame}\n{end}\n", "door_failure\nEDC_Error_dist\nvdv_error_EDC_active\n", "");
        assert_eq!(next.set_fault_state(&state), 2);
        assert_eq!((next.var("door_failure"), next.var("EDC_Error_dist")), (Some(1.0), Some(1234.5)));
        assert_eq!(next.set_fault_state(&[("Velocity".into(), 3.0)]), 0, "only the faults");
    }
}
