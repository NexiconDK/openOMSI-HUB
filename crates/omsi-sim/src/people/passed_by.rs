//! (NexiconDK, 30 October 2026) A player's bus that leaves people waiting for it at their stop:
//! it drove past without stopping, or it stood there and left without them (full, its doors
//! shut). Those who wanted it wave after it - an arm high, swinging, watching it go - and give
//! up (`human::wave_after_arm`, drawn by the view where the setting `passenger_wave_after`
//! has it); and each time is told (`LeftBehind`), for the player's trip and his company, which
//! counts the passengers left behind (a full bus excused).
//!
//! Only the players' buses - ours, and on the host the other LAN players' (`players_bus`):
//! the driver chose to drive on. The timetable's AI buses stop as their timetable has them.
//! A LAN client tells nothing itself: the host's people are drawn there, and the host says who
//! waves (`MirrorGesture`) and whose bus left whom.

use super::*;

/// How long somebody waves after the bus (s): the arm up and swinging, then down.
pub const WAVE_SECS: f64 = 3.6;
/// How often the stops are looked at for the players' buses (s).
pub const PASSED_EVERY: f64 = 0.25;
/// A stop a bus comes to: from this far before it (m, along the way its buses go) ...
pub const PASSED_FROM: f64 = -60.0;
/// ... to this far past it, where it is gone (a bus standing at the stop has its front at
/// most a bus length past the sign).
pub const PASSED_GONE: f64 = 12.0;
/// A bus standing in this stretch at the stop stood there (m along, before and past the sign).
pub const PASSED_STOOD: (f64, f64) = (-25.0, 12.0);
/// A bus that has not come near a stop it was noted at for this long is forgotten (s).
const PASSED_FORGET: f64 = 30.0;

/// A player's bus coming to a stop (`PeopleSim::passing`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Passing {
    pub bus: BusId,
    pub stop: i64,
    /// It stood at the stop.
    pub stood: bool,
    /// It was full as it came (every place taken).
    pub full: bool,
    /// When it was last seen coming (the people's clock).
    pub seen: f64,
}

/// Somebody waving after a bus that left them (`PeopleSim::waving`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaveAfter {
    pub person: u32,
    pub bus: BusId,
    /// When it began (the people's clock) and where the bus is (as last seen).
    pub since: f64,
    pub at: DVec3,
}

/// A player's bus left people waiting for it (`PeopleSim::take_left_behind`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeftBehind {
    pub bus: BusId,
    pub stop: i64,
    pub count: u32,
    /// It was full: they could not get on (excused).
    pub full: bool,
    /// It stood at the stop (else it drove past).
    pub stood: bool,
}

/// What one of the host's waiting people does for a bus, as a LAN client draws it: signals to
/// it, glances at it (full), waves after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    Hail,
    Full,
    Wave,
}

impl Gesture {
    pub fn word(self) -> &'static str {
        match self {
            Gesture::Hail => "hail",
            Gesture::Full => "full",
            Gesture::Wave => "wave",
        }
    }

    pub fn of_word(w: &str) -> Option<Gesture> {
        match w {
            "hail" => Some(Gesture::Hail),
            "full" => Some(Gesture::Full),
            "wave" => Some(Gesture::Wave),
            _ => None,
        }
    }
}

/// A gesture of one of the host's people on a LAN client: what, the bus (the one of ours
/// nearest where the host had it) and where that was.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MirrorGesture {
    pub gesture: Gesture,
    pub bus: Option<BusId>,
    pub at: DVec3,
    /// When it came (the people's clock): a wave ends by itself.
    pub since: f64,
}

/// Where bus `d` is from a stop (`forward` the way its buses go): metres along, across.
fn stop_frame(d: DVec3, forward: DVec2) -> (f64, f64) {
    (d.truncate().dot(forward), (d.x * forward.y - d.y * forward.x).abs())
}

impl PeopleSim {
    /// Whether bus `b` has every place taken (its seats and standing places the scripts left
    /// on, reserved or with somebody in them); a bus of no known places is never full.
    pub fn bus_full(&self, b: &BusNow) -> bool {
        let capacity = b.cabin.seats.iter().enumerate().filter(|(j, _)| !b.places_off.get(*j).copied().unwrap_or(false)).count();
        if capacity == 0 {
            return false;
        }
        let reserved = self.seats.get(&b.id).map_or(0, |places| places.iter().enumerate().filter(|(j, taken)| **taken && !b.places_off.get(*j).copied().unwrap_or(false)).count());
        reserved >= capacity || self.people.iter().filter(|p| matches!(p.place, Place::Bus(id, _) if id == b.id)).take(capacity).count() >= capacity
    }

    /// Four times a second: the players' buses at the stops near them - noted as they come,
    /// and as one goes on past a stop, those still waiting there who wanted it are left
    /// behind (`LeftBehind`) and wave after it.
    pub fn passed_tick(&mut self, buses: &[BusNow]) {
        if self.mirror || self.avatar_only {
            return;
        }
        // (a wave ends; and where its bus is now)
        let now = self.time;
        let at: HashMap<BusId, DVec3> = buses.iter().map(|b| (b.id, b.pos)).collect();
        self.waving.retain(|w| now - w.since < WAVE_SECS);
        for w in self.waving.iter_mut() {
            if let Some(p) = at.get(&w.bus) {
                w.at = *p;
            }
        }
        if now - self.passed_at < PASSED_EVERY && now >= self.passed_at {
            return;
        }
        self.passed_at = now;
        self.passing.retain(|p| now - p.seen < PASSED_FORGET && at.contains_key(&p.bus));
        if !buses.iter().any(|b| super::runners::players_bus(b.id)) {
            return;
        }
        let mut gone: Vec<(BusId, i64, bool, bool)> = Vec::new();
        for b in buses.iter().filter(|b| super::runners::players_bus(b.id)) {
            let full = self.bus_full(b);
            for (id, s) in &self.stops {
                let d = b.pos - s.pos;
                if d.length_squared() > 90.0 * 90.0 || d.z.abs() > 8.0 {
                    continue;
                }
                let h = s.heading.to_radians();
                let forward = DVec2::new(h.sin(), h.cos());
                let (along, across) = stop_frame(d, forward);
                if across > 14.0 || b.fwd().dot(forward) <= 0.0 {
                    continue;
                }
                let k = self.passing.iter().position(|p| p.bus == b.id && p.stop == *id);
                if (PASSED_FROM..=PASSED_GONE).contains(&along) {
                    let stood = b.speed.abs() < 0.5 && (PASSED_STOOD.0..=PASSED_STOOD.1).contains(&along);
                    match k {
                        Some(k) => {
                            let p = &mut self.passing[k];
                            p.stood |= stood;
                            p.full = full;
                            p.seen = now;
                        }
                        None => self.passing.push(Passing { bus: b.id, stop: *id, stood, full, seen: now }),
                    }
                } else if along > PASSED_GONE && b.speed.abs() > 1.0 {
                    if let Some(k) = k {
                        let p = self.passing.swap_remove(k);
                        gone.push((b.id, *id, p.stood, p.full));
                    }
                }
            }
        }
        for (bus, stop, stood, full) in gone {
            let Some(b) = buses.iter().find(|b| b.id == bus) else { continue };
            let Some(s) = self.stops.get(&stop) else { continue };
            let left: Vec<u32> = self
                .people
                .iter()
                .enumerate()
                .filter_map(|(i, person)| {
                    let State::Pax(p) = &person.state else { return None };
                    (p.task == Task::WaitingForBus && p.inside.is_none() && p.stop == Some(stop) && p.late.is_none() && self.takes_waiting(stop, s, b, p.dest.as_deref(), p.line)).then_some(self.people[i].id)
                })
                .collect();
            if left.is_empty() {
                continue;
            }
            for person in &left {
                self.waving.retain(|w| w.person != *person);
                self.waving.push(WaveAfter { person: *person, bus, since: now, at: b.pos });
            }
            self.left_behind.push(LeftBehind { bus, stop, count: left.len() as u32, full, stood });
        }
    }

    /// Whether bus `b` is one somebody waiting at stop `id` for `dest` (line record `line`)
    /// would take: as boarding chooses it (`runners::bus_takes`) - another LAN player's bus,
    /// which nobody boards here (its game does), by what its display shows
    /// (`set_remote_termini`).
    pub fn takes_waiting(&self, id: i64, s: &PaxStop, b: &BusNow, dest: Option<&str>, line: Option<usize>) -> bool {
        if let BusId::Ai(k) = b.id {
            if let Some(player) = remote_bus_player(k) {
                let Some(t) = self.remote_termini.get(&player).map(|t| t.trim()).filter(|t| !t.is_empty()) else { return false };
                let Some((_, termini)) = line.and_then(|k| s.lines.get(k)) else { return false };
                return !b.cabin.entries.is_empty() && fit(id, s, dest, termini, t, &Takes::Terminus).is_some();
            }
        }
        !matches!(b.takes, Takes::Nobody) && super::runners::bus_takes(id, s, b, dest, line)
    }

    /// (LAN host) What the other players' buses show: their destination, by player.
    pub fn set_remote_termini(&mut self, shown: impl Iterator<Item = (u32, String)>) {
        self.remote_termini.clear();
        self.remote_termini.extend(shown);
    }

    /// The players' buses that left people behind since the last call.
    pub fn take_left_behind(&mut self) -> Vec<LeftBehind> {
        std::mem::take(&mut self.left_behind)
    }

    /// Whom person `id` waves after now: the bus's place and how long it has waved (s).
    pub fn wave_of(&self, id: u32) -> Option<(DVec3, f32)> {
        if let Some(w) = self.waving.iter().find(|w| w.person == id) {
            return Some((w.at, (self.time - w.since) as f32));
        }
        let g = self.mirror_gestures.get(&id)?;
        (g.gesture == Gesture::Wave && self.time - g.since < WAVE_SECS).then(|| (self.mirror_bus_at(g), (self.time - g.since) as f32))
    }

    fn mirror_bus_at(&self, g: &MirrorGesture) -> DVec3 {
        g.bus.and_then(|id| self.last_buses.iter().find(|b| b.id == id)).map_or(g.at, |b| b.pos)
    }

    /// (LAN client) The host says what one of its people does for a bus near `at` (None: no
    /// more): the bus is ours nearest that place.
    pub fn set_mirror_gesture(&mut self, id: u32, g: Option<(Gesture, DVec3)>) {
        match g {
            None => {
                self.mirror_gestures.remove(&id);
            }
            Some((gesture, at)) => {
                let bus = self.last_buses.iter().filter(|b| (b.pos - at).truncate().length() < 30.0).min_by(|a, b| (a.pos - at).length_squared().total_cmp(&(b.pos - at).length_squared())).map(|b| b.id);
                self.mirror_gestures.insert(id, MirrorGesture { gesture, bus, at, since: self.time });
            }
        }
    }

    /// (LAN client) The bus one of the host's people signals to or glances at: its place,
    /// whether it moves, which bus and whether it is full - as `visual_hail_bus` has it here.
    pub fn mirror_hail(&self, id: u32) -> Option<(BusId, bool, DVec3, bool)> {
        let g = self.mirror_gestures.get(&id)?;
        let full = match g.gesture {
            Gesture::Hail => false,
            Gesture::Full => true,
            Gesture::Wave => return None,
        };
        // (a bus here the host's word fits; else a stand-in for the gate, that place)
        let bus = g.bus.unwrap_or(BusId::Ai(u64::MAX - id as u64));
        Some((bus, true, self.mirror_bus_at(g), full))
    }

    /// (LAN host) What each of our waiting people does for a bus now, for the clients: waves
    /// after one that left them, else signals to one coming (glances at it, full) - with where
    /// that bus is.
    pub fn lan_gestures(&self) -> Vec<(u32, Gesture, DVec3)> {
        let buses: &[BusNow] = &self.last_buses;
        let mut out = Vec::new();
        for (i, person) in self.people.iter().enumerate() {
            if person.remote || person.puppet.is_some() {
                continue;
            }
            if let Some(w) = self.waving.iter().find(|w| w.person == person.id) {
                out.push((person.id, Gesture::Wave, w.at));
                continue;
            }
            if let Some((id, moving, at)) = self.visual_hail_bus(i, buses) {
                if moving {
                    let full = buses.iter().find(|b| b.id == id).is_some_and(|b| self.bus_full(b));
                    out.push((person.id, if full { Gesture::Full } else { Gesture::Hail }, at));
                }
            }
        }
        out
    }
}

#[cfg(test)]
#[path = "passed_by_tests.rs"]
mod tests;
