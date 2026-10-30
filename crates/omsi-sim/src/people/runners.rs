//! Somebody late for the bus (from openOMSI PR #2073 by JDCustard, MIT; the fork's own
//! version). Only a player's bus - ours, or on the host another LAN player's - and only once
//! it stands at a stop: then, now and then (the setting `pax_runners`), somebody runs for it -
//! somebody walking near on the stop's side of the street, else somebody from 25 to 60 m along
//! the pavement out of the player's sight (behind the bus first, where the driver sees them
//! come in the mirror). They run to the doors; the bus leaving without them, they stand a
//! moment and walk on. (The pull request also put people out on the pavement while a bus was
//! still 150 m off, for the timetable's buses too: left out here, and all that looking every
//! frame with it.)

use super::*;

/// The chance per stop a player's bus stands at, by default (`PeopleSim::runner_chance`).
pub const RUNNER_CHANCE: f32 = 0.05;
/// How far along the pavement from the stop somebody comes from behind (m).
pub const RUNNER_START: (f32, f32) = (25.0, 60.0);
/// Somebody walking within this of the stop can run for the bus there (m).
pub const RUNNER_NEAR: f64 = 50.0;
/// The pace of somebody hurrying for the bus (m/s).
pub const RUNNER_SPEED: (f32, f32) = (3.0, 3.6);
/// Seconds somebody hurrying keeps a timetable bus at the stop, at most (`holds_bus`).
pub const RUNNER_HOLD_MAX: f32 = 12.0;
/// Seconds somebody left behind stands looking after the bus.
pub const RUNNER_MISSED: f32 = 2.0;
/// How often the stops are looked at for a player's bus standing there (s): four times a
/// second, not every frame.
pub const RUNNER_EVERY: f64 = 0.25;

/// Logs with `OMSI_DEBUG_PAX`.
macro_rules! trace {
    ($($arg:tt)*) => {
        if debug_pax() {
            log::info!($($arg)*)
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LatePhase {
    /// On the way to the stop at a run, for another player's bus (its own game takes them
    /// in once they wait there).
    Approach,
    /// Hurrying for the bus's doors.
    Hurry,
    /// The bus left without them: the seconds left standing there.
    Missed(f32),
}

/// What makes a passenger somebody late (`Pax::late`).
#[derive(Debug, Clone, Copy)]
pub struct Late {
    pub phase: LatePhase,
    /// Their own walking pace, and the pace they hurry at.
    pub walk: f32,
    pub run: f32,
    /// Seconds spent hurrying.
    pub hurried: f32,
    /// The bus has stood at the stop since they began to hurry.
    pub stood: bool,
}

impl Late {
    /// Whether they keep a timetable bus at the stop now.
    pub fn holds(&self) -> bool {
        self.phase == LatePhase::Hurry && self.hurried < RUNNER_HOLD_MAX
    }
}

/// The chance per bus: `OMSI_RUNNER_CHANCE` when set, else `setting` (`pax_runners`), 0..1.
pub fn runner_chance(setting: f32) -> f32 {
    omsi_cfg::flags::OMSI_RUNNER_CHANCE.parse::<f32>().unwrap_or(setting).clamp(0.0, 1.0)
}

/// Whether `b` is a player's bus: ours, or another LAN player's (`remote_bus_id`).
pub fn players_bus(b: BusId) -> bool {
    match b {
        BusId::Player => true,
        BusId::Ai(k) => remote_bus_player(k).is_some(),
    }
}

/// `t` (0..1) of the way through `range`.
fn lerp(range: (f32, f32), t: f32) -> f32 {
    range.0 + (range.1 - range.0) * t
}

/// How far the pavement (lane `lane`) 10 m from the stop at `s0` lies along `fwd`, that way
/// along the lane (`sign`): which way is behind the bus.
fn along(net: &Network, lane: usize, s0: f32, lane_len: f32, stop_pos: DVec3, fwd: DVec2, sign: f32) -> f64 {
    (net.lanes[lane].at((s0 + sign * 10.0).clamp(0.0, lane_len)).0 - stop_pos).truncate().dot(fwd)
}

/// Whether a bus listed at stop `id` takes somebody waiting there for `dest` with line record
/// `line` (as `bus_for` would choose it, `fit`).
pub(super) fn bus_takes(id: i64, stop: &PaxStop, bn: &BusNow, dest: Option<&str>, line: Option<usize>) -> bool {
    if bn.cabin.entries.is_empty() {
        return false;
    }
    match line.and_then(|k| stop.lines.get(k)) {
        None => stop.buses.first().is_some_and(|b| b.0 == bn.id),
        Some((_, termini)) => bn.terminus.as_deref().is_some_and(|t| fit(id, stop, dest, termini, t, &bn.takes).is_some()),
    }
}

impl PeopleSim {
    /// Four times a second: a player's bus standing in a stop's box near the player has its
    /// roll once per stop (`runner_chance`), and somebody runs for it when it falls so.
    pub fn runners_tick(&mut self, world: &dyn World, net: &Network, buses: &[BusNow], bus_ix: &HashMap<BusId, usize>) {
        if self.mirror || self.avatar_only {
            return;
        }
        if self.time >= self.runner_at && self.time - self.runner_at < RUNNER_EVERY {
            return;
        }
        self.runner_at = self.time;
        // (anybody still late, for `runner_step`: while nobody is, nobody looks)
        self.runners_live = self.people.iter().any(|p| matches!(&p.state, State::Pax(x) if x.late.is_some()));
        // (a bus no longer listed at the stop has left it: at the next stop a new roll)
        let stops = &self.stops;
        self.runner_rolls.retain(|(s, b)| stops.get(s).is_some_and(|st| st.buses.iter().any(|x| x.0 == *b)));
        let chance = runner_chance(self.runner_chance);
        // (no player's bus standing: nothing more to look at - the stops not at all)
        if chance <= 0.0 || !buses.iter().any(|b| players_bus(b.id) && b.speed.abs() < 0.5) {
            return;
        }
        // the players' buses standing in the box of a stop near the player
        let mut standing: Vec<(i64, BusId)> = self
            .stops
            .iter()
            .filter(|(_, s)| s.near)
            .flat_map(|(id, s)| s.buses.iter().filter(|(b, in_box)| *in_box && players_bus(*b)).map(move |(b, _)| (*id, *b)))
            .filter(|(_, b)| bus_ix.get(b).is_some_and(|k| buses[*k].speed.abs() < 0.5))
            .collect();
        standing.sort_unstable_by_key(|(id, b)| (*id, matches!(b, BusId::Ai(_))));
        for key in standing {
            if !self.runner_rolls.insert(key) {
                continue;
            }
            if (self.rand_f() as f32) < chance {
                self.spawn_runner(world, net, key.0, key.1, buses, bus_ix);
            }
        }
    }

    /// Somebody running for bus `bus` standing at stop `id`: somebody walking near, else
    /// somebody from behind the bus out of the player's sight (`start_behind`). For our bus
    /// they make for its doors; for another player's they run to a waiting place, where
    /// that player's game asks for them. Nobody when the stop is full, has no pavement, no
    /// destination drawn is one the bus goes to, or nobody is there to run.
    pub fn spawn_runner(&mut self, world: &dyn World, net: &Network, id: i64, bus: BusId, buses: &[BusNow], bus_ix: &HashMap<BusId, usize>) -> Option<usize> {
        let Some((lane, s0, stop_pos, full)) = self.stops.get(&id).and_then(|s| s.lane.map(|(l, s0)| (l, s0, s.pos, s.taken.iter().all(|t| *t)))) else {
            trace!("t={:.1} nobody late for {bus:?} at stop {id}: the stop has no pavement", self.time);
            return None;
        };
        if full {
            trace!("t={:.1} nobody late for {bus:?} at stop {id}: every waiting place is taken", self.time);
            return None;
        }
        let bn = bus_ix.get(&bus).map(|k| &buses[*k])?;
        // (a few draws: the stop's destinations go with every line calling there - and with
        // none of those for this bus, the first of the stop's line records it takes)
        let mut drawn = None;
        for _ in 0..4 {
            let (dest, line) = self.draw_dest(id);
            if bus_takes(id, &self.stops[&id], bn, dest.as_deref(), line) {
                drawn = Some((dest, line));
                break;
            }
        }
        if drawn.is_none() {
            let s = &self.stops[&id];
            drawn = (0..s.lines.len()).find(|&k| bus_takes(id, s, bn, Some(&s.lines[k].0), Some(k))).map(|k| (Some(s.lines[k].0.clone()), Some(k)));
        }
        let Some((dest, line)) = drawn else {
            trace!("t={:.1} nobody late for {bus:?} at stop {id}: no destination drawn is one this bus goes to", self.time);
            return None;
        };
        let lane_len = net.lanes.get(lane)?.length();
        let jitter = self.rand_f() as f32 * 5.0;
        // (anybody walking near, not right at the stop - as if they had been waiting there)
        let pave = net.lanes[lane].at(s0).0;
        let near = self.stroller_near(net, pave).filter(|&j| (self.people[j].position - pave).truncate().length() > 5.0);
        let start = match near {
            Some(_) => None,
            None => Some(self.start_behind(world, net, lane, s0, lane_len, stop_pos, bn.fwd(), jitter)?),
        };
        let k = self.take_spot(id)?;
        let walk = 1.1 + (self.rand_f() as f32 * 2.0 - 1.0) * 0.2;
        let run = lerp(RUNNER_SPEED, self.rand_f() as f32);
        // (another player's bus: to the waiting place at a run, that player's game takes them in)
        let ours = bus == BusId::Player;
        let mut pax = Pax::new(walk, self.rand_f());
        pax.stop = Some(id);
        pax.spot = Some(k);
        pax.dest = dest;
        pax.line = line;
        pax.bus = Some(bus);
        pax.late = Some(Late { phase: if ours { LatePhase::Hurry } else { LatePhase::Approach }, walk, run, hurried: 0.0, stood: true });
        pax.walk_speed = run;
        let i = match (near, start) {
            (Some(j), _) => {
                // (on from where they walk, the way they face)
                let p = &mut self.people[j];
                pax.pos = p.position;
                pax.yaw = p.heading.to_radians();
                p.state = State::Pax(Box::new(pax));
                j
            }
            (None, Some(start)) => {
                let yaw = yaw_of((stop_pos - start).truncate());
                pax.pos = start;
                pax.yaw = yaw;
                let Some(i) = self.spawn(world, start, yaw.to_degrees(), State::Pax(Box::new(pax))) else {
                    self.free_spot(id, k);
                    return None;
                };
                i
            }
            (None, None) => return None,
        };
        self.set_task(i, if ours { Task::ToBus } else { Task::WalkingToBusstop }, buses, bus_ix, world);
        self.runners_live = true;
        trace!(
            "t={:.1} pax {} late for {bus:?} at stop {id}, {}, {:.0} m away",
            self.time,
            self.people[i].label(),
            if near.is_some() { "walking near, runs" } else { "comes from behind" },
            (stop_pos - self.people[i].position).truncate().length()
        );
        Some(i)
    }

    /// The nearest person strolling the pavement within `RUNNER_NEAR` of `pave` (the point of
    /// the pavement at a stop), on its side of the street: somebody who could run for a bus.
    fn stroller_near(&self, net: &Network, pave: DVec3) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for (j, p) in self.people.iter().enumerate() {
            if p.puppet.is_some() || p.remote || !matches!(p.state, State::Strolling(_)) {
                continue;
            }
            let d = (p.position - pave).truncate().length();
            // (the cheap tests first: the street crossing only for the few within reach)
            if d >= RUNNER_NEAR || (p.position.z - pave.z).abs() >= 3.0 || best.is_some_and(|(_, b)| d >= b) {
                continue;
            }
            if !crosses_street(net, p.position.truncate(), pave.truncate()) {
                best = Some((j, d));
            }
        }
        best.map(|b| b.0)
    }

    /// A place 25 to 60 m along the stop's pavement (lane `lane` at `s0`) out of the
    /// player's sight, on the stop's side of the street: behind the bus first (against
    /// `fwd`), where the driver sees them come in the mirror.
    #[allow(clippy::too_many_arguments)]
    fn start_behind(&self, world: &dyn World, net: &Network, lane: usize, s0: f32, lane_len: f32, stop_pos: DVec3, fwd: DVec2, jitter: f32) -> Option<DVec3> {
        let probe = |sign| along(net, lane, s0, lane_len, stop_pos, fwd, sign);
        let signs = if probe(1.0) <= probe(-1.0) { [1.0f32, -1.0] } else { [-1.0, 1.0] };
        for sign in signs {
            for d in [25.0f32, 35.0, 45.0, 55.0] {
                let a = (s0 + sign * (d + jitter)).clamp(0.0, lane_len);
                // (the pavement ends sooner: too close to the stop to be late from)
                if (a - s0).abs() < RUNNER_START.0 * 0.6 {
                    break;
                }
                let (q, _) = net.lanes[lane].at(a);
                if !crosses_street(net, q.truncate(), net.lanes[lane].at(s0).0.truncate()) && world.has_ground(q.x, q.y) && !self.seen(q) {
                    return Some(q);
                }
            }
        }
        None
    }

    /// Somebody late, before their movement this frame: left behind when the bus goes, an
    /// ordinary passenger again once aboard or waiting at the stop.
    pub fn runner_step(&mut self, i: usize, dt: f32, world: &dyn World, buses: &[BusNow], bus_ix: &HashMap<BusId, usize>) {
        if !self.runners_live {
            return;
        }
        let Some(p) = self.pax(i) else { return };
        let Some(late) = p.late else { return };
        let (task, inside, stop, bus) = (p.task, p.inside, p.stop, p.bus);
        let Some(stop) = stop.filter(|_| inside.is_none()) else {
            self.end_late(i);
            return;
        };
        let bn = bus.and_then(|b| bus_ix.get(&b).map(|k| &buses[*k]));
        let listed = bus.is_some_and(|b| self.listed_at(stop, b));
        match late.phase {
            // (another player's bus: at the waiting place they wait as anybody)
            LatePhase::Approach if !listed || task == Task::WaitingForBus => self.end_late(i),
            LatePhase::Approach => {}
            LatePhase::Hurry => {
                if !listed || bn.is_none_or(|b| b.speed.abs() >= 3.0) {
                    let p = self.pax_mut(i).unwrap();
                    p.walk_speed = 0.0;
                    p.late = Some(Late { phase: LatePhase::Missed(RUNNER_MISSED), ..late });
                    trace!("t={:.1} pax {} missed {bus:?}", self.time, self.people[i].label());
                } else if task == Task::WaitingForBus {
                    // (gave up at its shut doors, back at the stop)
                    self.end_late(i);
                } else {
                    self.pax_mut(i).unwrap().late = Some(Late { hurried: late.hurried + dt, ..late });
                }
            }
            LatePhase::Missed(t) if t > dt => self.pax_mut(i).unwrap().late = Some(Late { phase: LatePhase::Missed(t - dt), ..late }),
            LatePhase::Missed(_) => {
                self.end_late(i);
                if !matches!(task, Task::WalkingToBusstop | Task::WaitingForBus) {
                    self.set_task(i, Task::WalkingToBusstop, buses, bus_ix, world);
                }
            }
        }
    }

    /// No longer late: their own pace again.
    fn end_late(&mut self, i: usize) {
        let Some(p) = self.pax_mut(i) else { return };
        if let Some(late) = p.late.take() {
            p.walk_speed = late.walk;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only a player's bus has anybody run for it: ours, or another LAN player's - not the
    /// timetable's or the traffic's.
    #[test]
    fn only_the_players_buses_have_runners() {
        assert!(players_bus(BusId::Player));
        assert!(players_bus(BusId::Ai(remote_bus_id(3))));
        assert!(!players_bus(BusId::Ai(17)));
        assert!(!players_bus(BusId::Ai(placed_bus_id(5))), "a bus the player placed is no player's");
    }

    #[test]
    fn the_setting_gives_the_chance_unless_the_variable_does() {
        if omsi_cfg::flags::OMSI_RUNNER_CHANCE.is_set() {
            return;
        }
        assert_eq!(runner_chance(0.15), 0.15);
        assert_eq!(runner_chance(2.0), 1.0);
        assert_eq!(runner_chance(0.0), 0.0);
    }
}
