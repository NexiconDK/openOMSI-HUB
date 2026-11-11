//! (NexiconDK, ticket inspection round U) Ticket inspectors aboard the player's bus.
//!
//! The bus company's inspectors (its launcher's `inspection`) board the bus the player drives:
//! at a stop of a trip of the duty the company planned it for, or one the driver asked for
//! ("Bestil kontrol"). The plugin tells the people (`inspect`): one or two inspectors - in the
//! company's uniform, a generated clothing variant of a figure (`variant_name`, never worn by
//! anybody else: `is_inspector_variant`) - come in by the front door, and some of the riders
//! without a ticket (`Pax::dodger`, drawn at boarding from the company's share,
//! `set_dodger_share`) get off before they are reached (`ESCAPE`). The inspectors walk the
//! aisle's paths from rider to rider (`Phase::Walking`, the cabin's path network as the people
//! walk it), stand by each with a hand out for the ticket for a few seconds (`Phase::Checking`,
//! the pay pose) and catch the dodgers; one they cannot reach in `WALK_GIVE_UP` is passed by.
//! After the stops they ride (`Run::stops_left`, counted as the bus stands at a stop with a door
//! open after it moved) they get off through the nearest exit as any rider does, and what they
//! did is kept for the plugin (`take_inspections`: the `inspection_done` event).

use super::*;

/// The `.cti` items of the inspectors' clothes begin so (`~Inspector_<look>_<company>`).
pub const INSPECTOR_PREFIX: &str = "~Inspector_";
/// The share of the dodgers aboard who get off as the inspectors come in (the company's
/// `inspection::ESCAPE`).
pub const ESCAPE: f64 = 0.15;
/// Seconds standing at a rider for the ticket.
pub const CHECK_SECS: (f64, f64) = (2.0, 4.0);
/// Seconds walking to a rider before the inspector passes them by (somebody in the way).
pub const WALK_GIVE_UP: f32 = 12.0;
/// Seconds of the people's clock after which the inspectors get off at the next stop whatever
/// (a bus standing for good, a driver who never stops).
pub const RUN_GIVE_UP: f64 = 30.0 * 60.0;
/// The most inspectors and stops of a run.
pub const MOST_TEAM: u8 = 4;
pub const MOST_STOPS: u32 = 12;

/// Whether a clothing item is an inspector's uniform (worn only by them).
pub fn is_inspector_variant(name: &str) -> bool {
    name.trim_start().starts_with(INSPECTOR_PREFIX)
}

/// The clothing item of the inspectors' look (`A`, `B`, `C`) for a company (`tag`: its short
/// name made safe for a file name).
pub fn variant_name(look: char, tag: &str) -> String {
    format!("{INSPECTOR_PREFIX}{}_{}", look.to_ascii_uppercase(), tag)
}

/// The ticket inspection of the player's bus.
#[derive(Debug, Clone, Default)]
pub struct InspectState {
    /// The share of the people boarding the player's bus who ride without a ticket.
    pub dodger_share: f32,
    pub run: Option<Run>,
    /// The runs ended since the plugin last looked (`take_inspections`).
    pub done: Vec<Outcome>,
    pub last_id: u32,
}

/// Inspectors aboard.
#[derive(Debug, Clone)]
pub struct Run {
    pub id: u32,
    pub team: Vec<Inspector>,
    /// The stops still to ride, the stops ridden, whether the bus moved since the last stop.
    pub stops_left: u32,
    pub stops: u32,
    pub moved: bool,
    /// When they came in (the people's clock).
    pub since: f64,
    pub checked: u32,
    pub caught: u32,
    pub fled: u32,
    /// The riders checked (or passed by), by person id.
    pub visited: Vec<u32>,
    pub board_stop: String,
    pub leave_stop: String,
    /// Getting off at this stop.
    pub leaving: bool,
    /// The dodgers near a door still to send off (the first frame after they came in).
    pub flee: bool,
}

/// One inspector of a run.
#[derive(Debug, Clone)]
pub struct Inspector {
    pub person: u32,
    pub target: Option<u32>,
    pub phase: Phase,
    pub timer: f32,
    pub out: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Free,
    Walking,
    Checking,
    Leaving,
}

/// What a run came to.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outcome {
    pub id: u32,
    pub checked: u32,
    pub caught: u32,
    pub fled: u32,
    pub stops: u32,
    pub board_stop: String,
    pub leave_stop: String,
    pub team: u32,
}

/// A stop served by the bus: it stands (under 0.5 m/s) with a door open after it moved (over 3
/// m/s) - `moved` kept between the frames.
pub fn stop_served(moved: &mut bool, speed: f64, doors_open: bool) -> bool {
    if speed.abs() > 3.0 {
        *moved = true;
        return false;
    }
    if *moved && doors_open && speed.abs() < 0.5 {
        *moved = false;
        return true;
    }
    false
}

/// The rider an inspector at `from` goes to next: the nearest of `riders` (person id, place in
/// the bus) neither checked nor another inspector's.
pub fn next_rider(from: DVec3, riders: &[(u32, DVec3)], visited: &[u32], taken: &[u32]) -> Option<u32> {
    riders
        .iter()
        .filter(|(id, _)| !visited.contains(id) && !taken.contains(id))
        .min_by(|a, b| (a.1 - from).length().total_cmp(&(b.1 - from).length()))
        .map(|x| x.0)
}

impl PeopleSim {
    /// The share of the people boarding the player's bus who ride without a ticket (the
    /// company's, for the line and the hour; 0 none).
    pub fn set_dodger_share(&mut self, share: f32) {
        self.inspect.dodger_share = if share.is_finite() { share.clamp(0.0, 0.5) } else { 0.0 };
    }

    /// Whether somebody boarding now rides without a ticket.
    pub fn draw_dodger(&mut self) -> bool {
        let s = self.inspect.dodger_share;
        s > 0.0 && (self.rand_f() as f32) < s
    }

    /// Inspectors aboard the player's bus now.
    pub fn inspecting(&self) -> bool {
        self.inspect.run.is_some()
    }

    /// The runs ended since the last look.
    pub fn take_inspections(&mut self) -> Vec<Outcome> {
        std::mem::take(&mut self.inspect.done)
    }

    /// The riders of the player's bus without a ticket now.
    pub fn dodgers_aboard(&self) -> u32 {
        self.people.iter().filter(|p| matches!(&p.state, State::Pax(x) if x.inside == Some(BusId::Player) && x.dodger)).count() as u32
    }

    /// `n` inspectors come into the player's bus (standing at `stop`) by its front door and ride
    /// `stops` stops, in the clothes `variant` (a figure's item of that name; none of the
    /// figures has it: their own clothes). The run's number.
    pub fn inspect(&mut self, world: &dyn World, n: u8, stops: u32, variant: &str, stop: &str) -> Result<u32, &'static str> {
        if self.inspect.run.is_some() {
            return Err("The inspectors are aboard already.");
        }
        let Some(bn) = self.last_buses.iter().find(|b| b.id == BusId::Player).cloned() else { return Err("No bus.") };
        let Some(door) = bn.cabin.entries.first().or(bn.cabin.exits.first()).cloned() else { return Err("The bus has no door.") };
        let n = n.clamp(1, MOST_TEAM);
        // the figures that have the clothes, a different one for each where there are
        let mut wearing: Vec<(usize, usize)> = Vec::new();
        if !variant.trim().is_empty() {
            for (k, t) in self.types.iter().enumerate() {
                if let Some(v) = t.variants.iter().position(|s| s.name.trim().eq_ignore_ascii_case(variant.trim())) {
                    wearing.push((k, v + 1));
                }
            }
        }
        let mut team = Vec::new();
        for k in 0..n as usize {
            let mut pax = Pax::new(1.2, 0.5);
            pax.bus = Some(BusId::Player);
            pax.inside = Some(BusId::Player);
            pax.task = Task::Inspecting;
            pax.st = 0;
            // (side by side in the doorway, a little apart)
            let local = door.inside + Vec3::new(0.0, -0.45 * k as f32, 0.0);
            pax.pos = local.as_dvec3();
            pax.pt = door.point;
            let w = bn.world(local);
            let (kind, var) = match wearing.get(k % wearing.len().max(1)) {
                Some(&(t, v)) => (Some(t), Some(v)),
                None => (None, None),
            };
            self.force_variant = var;
            let i = self.spawn_as(world, w, bn.heading, State::Pax(Box::new(pax)), kind);
            self.force_variant = None;
            let Some(i) = i else { continue };
            self.people[i].place = Place::Bus(BusId::Player, local);
            team.push(Inspector { person: self.people[i].id, target: None, phase: Phase::Free, timer: 0.0, out: false });
        }
        if team.is_empty() {
            return Err("No inspector could come in.");
        }
        self.inspect.last_id += 1;
        let id = self.inspect.last_id;
        self.inspect.run = Some(Run {
            id,
            team,
            stops_left: stops.clamp(1, MOST_STOPS),
            stops: 0,
            moved: false,
            since: self.time,
            checked: 0,
            caught: 0,
            fled: 0,
            visited: Vec::new(),
            board_stop: stop.trim().to_string(),
            leave_stop: String::new(),
            leaving: false,
            flee: true,
        });
        Ok(id)
    }

    fn index_of(&self, id: u32) -> Option<usize> {
        self.people.iter().position(|p| p.id == id)
    }

    /// The inspectors' frame (before the passengers'): the dodgers who flee, the stops ridden,
    /// each inspector's rider, the checks, getting off.
    pub fn inspect_tick(&mut self, dt: f32, buses: &[BusNow], bus_ix: &HashMap<BusId, usize>, world: &dyn World) {
        let Some(mut run) = self.inspect.run.take() else { return };
        let bn = bus_ix.get(&BusId::Player).map(|k| &buses[*k]);
        let Some(bn) = bn else {
            // (the bus is gone: so is the run)
            for m in &run.team {
                if let Some(i) = self.index_of(m.person) {
                    let p = self.people.swap_remove(i);
                    self.retire(&p);
                }
            }
            self.finish(run);
            return;
        };
        let doors_open = bn.entry_open.iter().chain(bn.exit_open.iter()).any(|o| *o);
        // the dodgers near a door get off as the inspectors come in
        if run.flee {
            run.flee = false;
            let ids: Vec<usize> = (0..self.people.len()).filter(|i| matches!(&self.people[*i].state, State::Pax(x) if x.inside == Some(BusId::Player) && x.dodger && matches!(x.task, Task::SittingInBus | Task::InBusToPlace))).collect();
            for i in ids {
                if self.rand_f() < ESCAPE {
                    self.set_task(i, Task::InBusToExit, buses, bus_ix, world);
                    if let Some(p) = self.pax_mut(i) {
                        p.dodger = false;
                    }
                    run.fled += 1;
                }
            }
        }
        if stop_served(&mut run.moved, bn.speed, doors_open) {
            run.stops += 1;
            run.stops_left = run.stops_left.saturating_sub(1);
            if run.stops_left == 0 {
                run.leaving = true;
                run.leave_stop = bn.next_stop.as_ref().map(|s| s.name.trim().to_string()).unwrap_or_default();
            }
        }
        if !run.leaving && self.time - run.since > RUN_GIVE_UP {
            run.stops_left = run.stops_left.min(1);
        }
        let standing = bn.speed.abs() < 0.5 && doors_open;
        // the riders to check: seated or standing in their places
        let riders: Vec<(u32, DVec3)> = self
            .people
            .iter()
            .filter_map(|p| match &p.state {
                State::Pax(x) if x.inside == Some(BusId::Player) && x.task == Task::SittingInBus => Some((p.id, x.pos)),
                _ => None,
            })
            .collect();
        for k in 0..run.team.len() {
            let m = run.team[k].clone();
            if m.out {
                continue;
            }
            let Some(i) = self.index_of(m.person) else {
                run.team[k].out = true;
                continue;
            };
            let here = self.pax(i).map(|p| p.pos).unwrap_or_default();
            if m.phase == Phase::Leaving {
                if self.pax(i).is_none_or(|p| p.inside.is_none()) {
                    run.team[k].out = true;
                }
                continue;
            }
            if run.leaving && standing {
                if let Some(p) = self.pax_mut(i) {
                    p.reach = false;
                    p.talking = false;
                }
                self.set_task(i, Task::InBusToExit, buses, bus_ix, world);
                run.team[k].phase = Phase::Leaving;
                continue;
            }
            match m.phase {
                Phase::Free => {
                    let taken: Vec<u32> = run.team.iter().filter_map(|x| x.target).collect();
                    let Some(t) = next_rider(here, &riders, &run.visited, &taken) else { continue };
                    let goal = riders.iter().find(|r| r.0 == t).map(|r| r.1).unwrap_or(here);
                    let seat = self.index_of(t).and_then(|j| self.pax(j)).and_then(|x| x.seat);
                    let all = bn.cabin.all_points();
                    let to = match seat.and_then(|s| bn.cabin.seats.get(s)) {
                        Some(s) => bn.cabin.omsi_nearest(s.floor, &all, false, true, None, None),
                        None => bn.cabin.omsi_nearest(goal.as_vec3(), &all, false, true, None, None),
                    };
                    let from = bn.cabin.omsi_nearest(here.as_vec3(), &all, false, true, None, None);
                    let m = &mut run.team[k];
                    m.target = Some(t);
                    m.timer = 0.0;
                    if let Some(p) = self.pax_mut(i) {
                        p.reach = false;
                        p.talking = false;
                        if from.is_some() && to.is_some() && from != to {
                            p.pt = from;
                            p.pt_target = to;
                            p.short = false;
                            p.st = 5;
                            p.pax_state = 1.0;
                            m.phase = Phase::Walking;
                        } else {
                            p.st = 0;
                            m.phase = Phase::Checking;
                            m.timer = 0.0;
                        }
                    }
                }
                Phase::Walking => {
                    run.team[k].timer += dt;
                    let st = self.pax(i).map(|p| p.st).unwrap_or(0);
                    let gone = run.team[k].target.is_none_or(|t| self.index_of(t).is_none());
                    if gone || run.team[k].timer > WALK_GIVE_UP {
                        if let Some(t) = run.team[k].target.take() {
                            run.visited.push(t);
                        }
                        run.team[k].phase = Phase::Free;
                        if let Some(p) = self.pax_mut(i) {
                            p.st = 0;
                            p.pax_state = 0.0;
                        }
                    } else if st == 7 || st == 0 {
                        let secs = CHECK_SECS.0 + self.rand_f() * (CHECK_SECS.1 - CHECK_SECS.0);
                        run.team[k].phase = Phase::Checking;
                        run.team[k].timer = secs as f32;
                        let at = run.team[k].target.and_then(|t| riders.iter().find(|r| r.0 == t)).map(|r| r.1);
                        if let Some(p) = self.pax_mut(i) {
                            p.st = 0;
                            p.pax_state = 0.0;
                            if let Some(at) = at {
                                let d = at - p.pos;
                                p.yaw = d.x.atan2(d.y);
                                p.reach = true;
                                p.reach_at = (at + DVec3::Z * 0.9).as_vec3();
                            }
                            p.talking = true;
                        }
                    }
                }
                Phase::Checking => {
                    if run.team[k].timer <= 0.0 && !self.pax(i).is_some_and(|p| p.reach) {
                        // (came to the rider at once: the check's seconds now)
                        let secs = CHECK_SECS.0 + self.rand_f() * (CHECK_SECS.1 - CHECK_SECS.0);
                        run.team[k].timer = secs as f32;
                        if let Some(p) = self.pax_mut(i) {
                            p.reach = true;
                            p.talking = true;
                        }
                        continue;
                    }
                    run.team[k].timer -= dt;
                    if run.team[k].timer > 0.0 {
                        continue;
                    }
                    if let Some(t) = run.team[k].target.take() {
                        run.visited.push(t);
                        if let Some(j) = self.index_of(t) {
                            run.checked += 1;
                            if let Some(x) = self.pax_mut(j) {
                                if x.dodger {
                                    x.dodger = false;
                                    run.caught += 1;
                                }
                            }
                        }
                    }
                    run.team[k].phase = Phase::Free;
                    if let Some(p) = self.pax_mut(i) {
                        p.reach = false;
                        p.talking = false;
                    }
                }
                Phase::Leaving => {}
            }
        }
        if run.team.iter().all(|m| m.out) {
            self.finish(run);
        } else {
            self.inspect.run = Some(run);
        }
    }

    fn finish(&mut self, run: Run) {
        self.inspect.done.push(Outcome { id: run.id, checked: run.checked, caught: run.caught, fled: run.fled, stops: run.stops, board_stop: run.board_stop, leave_stop: run.leave_stop, team: run.team.len() as u32 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stop_is_served_when_the_bus_stands_with_a_door_open_after_it_moved() {
        let mut moved = false;
        assert!(!stop_served(&mut moved, 0.0, true), "standing at the stop they boarded at");
        assert!(!stop_served(&mut moved, 8.0, false));
        assert!(!stop_served(&mut moved, 0.2, false), "a red light: doors shut");
        assert!(stop_served(&mut moved, 0.1, true));
        assert!(!stop_served(&mut moved, 0.1, true), "the same stop once");
        assert!(!stop_served(&mut moved, 5.0, false));
        assert!(stop_served(&mut moved, 0.0, true));
    }

    #[test]
    fn the_nearest_rider_not_checked_nor_taken_comes_next() {
        let riders = [(1, DVec3::new(0.0, 4.0, 0.0)), (2, DVec3::new(0.0, 1.0, 0.0)), (3, DVec3::new(0.0, -2.0, 0.0))];
        let at = DVec3::new(0.0, 5.0, 0.0);
        assert_eq!(next_rider(at, &riders, &[], &[]), Some(1));
        assert_eq!(next_rider(at, &riders, &[1], &[]), Some(2));
        assert_eq!(next_rider(at, &riders, &[1], &[2]), Some(3));
        assert_eq!(next_rider(at, &riders, &[1, 2, 3], &[]), None);
    }

    #[test]
    fn the_inspectors_clothes_are_theirs_alone() {
        assert!(is_inspector_variant(&variant_name('a', "umove")));
        assert_eq!(variant_name('c', "umove"), "~Inspector_C_umove");
        assert!(!is_inspector_variant("Variante1"));
    }

    #[test]
    fn about_the_share_board_without_a_ticket() {
        let mut h = PeopleSim::new(std::path::Path::new("/nonexistent"), 10);
        assert!(!(0..100).any(|_| h.draw_dodger()), "none without a share");
        h.set_dodger_share(0.1);
        let n = (0..4000).filter(|_| h.draw_dodger()).count();
        assert!((300..500).contains(&n), "{n} of 4000");
        h.set_dodger_share(f32::NAN);
        assert_eq!(h.inspect.dodger_share, 0.0);
        h.set_dodger_share(3.0);
        assert_eq!(h.inspect.dodger_share, 0.5);
    }
}

#[cfg(test)]
mod run_tests {
    use super::*;

    /// A solo bus of three seats along the aisle: the front door's threshold (x 1.1, y 4), the
    /// aisle from y 4 to y -3, the rear door's (y -3).
    fn cabin() -> Arc<Cabin> {
        let dir = std::env::temp_dir().join(format!("omsi-inspect-cabin-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("test.bus"), "[boundingbox]\n2.5\n12\n3\n0\n0\n1.5\n\n[passengercabin]\ncabin.cfg\n\n[paths]\npaths.cfg\n").unwrap();
        std::fs::write(dir.join("paths.cfg"), "[pathpnt]\n1.1\n4\n0.4\n\n[pathpnt]\n0\n4\n0.5\n\n[pathpnt]\n0\n0\n0.5\n\n[pathpnt]\n0\n-3\n0.5\n\n[pathpnt]\n1.1\n-3\n0.4\n\n[pathlink]\n0\n1\n\n[pathlink]\n1\n2\n\n[pathlink]\n2\n3\n\n[pathlink]\n3\n4\n").unwrap();
        std::fs::write(dir.join("cabin.cfg"), "[entry]\n0\n\n[exit]\n4\n\n[passpos]\n-0.5\n3\n0.9\n0.45\n0\n\n[passpos]\n-0.5\n0\n0.9\n0.45\n0\n\n[passpos]\n-0.5\n-2\n0.9\n0.45\n0\n").unwrap();
        let def = omsi_vehicle::Vehicle::load(&dir.join("test.bus")).unwrap();
        let c = Arc::new(Cabin::load_train(&[(&def, Vec3::ZERO, f32::INFINITY)]).expect("cabin"));
        std::fs::remove_dir_all(&dir).ok();
        c
    }

    /// A figure with its own clothes, another item and the inspectors' uniform.
    fn human() -> Arc<HumanType> {
        let dir = std::env::temp_dir().join(format!("omsi-inspect-hum-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("tex")).unwrap();
        std::fs::write(dir.join("person.hum"), "[model]\nmodel.cfg\n").unwrap();
        std::fs::write(dir.join("model.cfg"), "[CTC]\nColorscheme\ntex\n0\n\n[CTCTexture]\nfarbschema\nbody.jpg\n").unwrap();
        std::fs::write(dir.join("tex/texvarianten.cti"), "[item]\nVariante1\nfarbschema\nbody_b.jpg\n").unwrap();
        std::fs::write(dir.join("tex/~Inspector_A_test.cti"), "; Omsi-Hub inspector clothes v1 A #f58220 Test | BILLETKONTROL | person.hum\r\n\r\n[item]\r\n~Inspector_A_test\r\nfarbschema\r\n~Inspector\\body_A_test.dds\r\n").unwrap();
        let ty = Arc::new(HumanType::load(&dir.join("person.hum")).unwrap());
        std::fs::remove_dir_all(&dir).ok();
        ty
    }

    fn bus(c: &Arc<Cabin>, speed: f64, open: bool) -> BusNow {
        BusNow {
            id: BusId::Player,
            cabin: c.clone(),
            pos: DVec3::new(100.0, 200.0, 0.0),
            rot: Mat4::IDENTITY,
            heading: 0.0,
            speed,
            entry_open: vec![open],
            exit_open: vec![open],
            walk_open: None,
            interior: 0.0,
            air: CabinAir::default(),
            half: DVec2::new(1.25, 6.0),
            centre: DVec2::ZERO,
            accel: DVec2::ZERO,
            trailers: Vec::new(),
            terminus: None,
            takes: Takes::Terminus,
            next_stop: Some(RequestStop { id: 9, name: "Rådhuspladsen".into(), alias: String::new(), pos: DVec3::ZERO }),
            places_off: Vec::new(),
            served: None,
        }
    }

    /// A rider on seat `k`, with or without a ticket.
    fn rider(h: &mut PeopleSim, world: &dyn World, c: &Arc<Cabin>, k: usize, dodger: bool) {
        let s = c.seats[k].clone();
        let mut p = Pax::new(1.1, 0.5);
        p.task = Task::SittingInBus;
        p.inside = Some(BusId::Player);
        p.bus = Some(BusId::Player);
        p.seat = Some(k);
        p.pos = s.pos.as_dvec3();
        p.pax_state = 2.0;
        p.dodger = dodger;
        let i = h.spawn_as(world, DVec3::new(100.0, 200.0, 0.0), 0.0, State::Pax(Box::new(p)), Some(0)).unwrap();
        h.people[i].place = Place::Bus(BusId::Player, s.pos);
    }

    fn frame(h: &mut PeopleSim, world: &dyn World, b: BusNow, dt: f32) {
        // (standing at the stop: its stop is the people's next, as `register_buses` finds it)
        let next = (b.speed.abs() < 1.0).then_some(9);
        let buses = vec![b];
        h.last_buses = Arc::new(buses.clone());
        let bus_ix: HashMap<BusId, usize> = [(BusId::Player, 0)].into_iter().collect();
        let at: HashMap<BusId, BusAtStops> = [(BusId::Player, BusAtStops { next, request_next: None, near: vec![9], all_exit: false })].into_iter().collect();
        let mut taken = false;
        let mut remove = Vec::new();
        h.pax_frame(dt, world, &buses, &bus_ix, &at, None, &mut taken, &mut remove);
        remove.sort_unstable();
        remove.dedup();
        for i in remove.into_iter().rev() {
            let p = h.people.swap_remove(i);
            h.retire(&p);
        }
    }

    /// Two inspectors come in at a stop, check the three riders - catching the two without a
    /// ticket who did not flee - ride two stops and get off; what they did is told once.
    #[test]
    fn inspectors_check_the_riders_and_get_off_after_their_stops() {
        let world = crate::world_data::WorldData::default();
        let c = cabin();
        let mut h = PeopleSim::new(std::path::Path::new("/nonexistent"), 50);
        h.types = vec![human()];
        h.pool_types = 1;
        h.last_buses = Arc::new(vec![bus(&c, 0.0, true)]);
        rider(&mut h, &world, &c, 0, true);
        rider(&mut h, &world, &c, 1, false);
        rider(&mut h, &world, &c, 2, true);
        assert_eq!(h.dodgers_aboard(), 2);
        let id = h.inspect(&world, 2, 2, "~Inspector_A_test", "Banegården").unwrap();
        assert!(h.inspect(&world, 2, 2, "", "").is_err(), "one run at a time");
        // in the uniform (the item after the figure's own two: variant 2)
        let team: Vec<&Person> = h.people.iter().filter(|p| matches!(&p.state, State::Pax(x) if x.task == Task::Inspecting)).collect();
        assert_eq!(team.len(), 2);
        assert!(team.iter().all(|p| p.variant == 2), "{:?}", team.iter().map(|p| p.variant).collect::<Vec<_>>());
        // the bus stands with its doors open for 25 s, drives on, stops, drives, stops
        let mut t = 0.0;
        let dt = 0.1;
        let mut steps = 0;
        while h.inspecting() && steps < 20000 {
            let (speed, open) = match t {
                x if x < 25.0 => (0.0, true),
                x if x < 60.0 => (10.0, false),
                x if x < 75.0 => (0.0, true),
                x if x < 110.0 => (10.0, false),
                _ => (0.0, true),
            };
            frame(&mut h, &world, bus(&c, speed, open), dt);
            t += dt as f64;
            steps += 1;
        }
        assert!(!h.inspecting(), "the run ended (t {t:.0} s)");
        let out = h.take_inspections();
        assert_eq!(out.len(), 1);
        let o = &out[0];
        assert_eq!((o.id, o.stops, o.team, o.board_stop.as_str(), o.leave_stop.as_str()), (id, 2, 2, "Banegården", "Rådhuspladsen"));
        assert!(o.checked + o.fled >= 2 && o.checked <= 3, "{o:?}");
        assert_eq!(o.caught + o.fled, 2, "every dodger caught or fled: {o:?}");
        assert_eq!(h.dodgers_aboard(), 0);
        assert!(h.take_inspections().is_empty(), "told once");
        // (nobody of the run is left inside the bus)
        assert!(!h.people.iter().any(|p| matches!(&p.state, State::Pax(x) if x.task == Task::Inspecting)));
    }

    /// Some of the riders without a ticket get off as the inspectors come in (`ESCAPE` of
    /// them); the others stay.
    #[test]
    fn some_dodgers_flee_as_the_inspectors_come_in() {
        let world = crate::world_data::WorldData::default();
        let c = cabin();
        let (mut fled, mut stayed) = (0, 0);
        for seed in 0..60u64 {
            let mut h = PeopleSim::new(std::path::Path::new("/nonexistent"), 50);
            h.rng = h.rng.wrapping_add(seed * 7919);
            h.types = vec![human()];
            h.pool_types = 1;
            h.last_buses = Arc::new(vec![bus(&c, 0.0, true)]);
            for k in 0..3 {
                rider(&mut h, &world, &c, k, true);
            }
            h.inspect(&world, 1, 1, "", "").unwrap();
            frame(&mut h, &world, bus(&c, 0.0, true), 0.1);
            let leaving = h.people.iter().filter(|p| matches!(&p.state, State::Pax(x) if x.task == Task::InBusToExit)).count();
            fled += leaving;
            stayed += 3 - leaving;
            assert_eq!(h.inspect.run.as_ref().map(|r| r.fled as usize), Some(leaving));
        }
        let share = fled as f64 / (fled + stayed) as f64;
        assert!((0.05..0.3).contains(&share), "{fled} fled, {stayed} stayed");
    }

    /// The inspectors' uniform is never drawn for anybody else, and a figure asked for in it
    /// wears it.
    #[test]
    fn nobody_else_wears_the_uniform() {
        let world = crate::world_data::WorldData::default();
        let mut h = PeopleSim::new(std::path::Path::new("/nonexistent"), 400);
        h.types = vec![human()];
        h.pool_types = 1;
        let mut seen = std::collections::HashSet::new();
        for k in 0..200 {
            let i = h.spawn_as(&world, DVec3::new(k as f64 * 40.0, 0.0, 0.0), 0.0, State::Idle, None).unwrap();
            seen.insert(h.people[i].variant);
        }
        assert!(seen.contains(&0) && seen.contains(&1) && !seen.contains(&2), "{seen:?}");
        h.force_variant = Some(2);
        let i = h.spawn_as(&world, DVec3::ZERO, 0.0, State::Idle, Some(0)).unwrap();
        assert_eq!(h.people[i].variant, 2);
    }
}
