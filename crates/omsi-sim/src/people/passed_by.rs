//! (NexiconDK, 30 October 2026) A player's bus that leaves people waiting for it at their stop:
//! it drove past without stopping, or it stood there and left without them (full, its doors
//! shut). Those of them who wave (the setting `pax_wave`, `greetings`) wave after it - an arm
//! high, swinging, watching it go - and give up (`human::wave_after_arm`); and each time is
//! told (`LeftBehind`), for the player's trip and his company, which counts all the passengers
//! left behind (a full bus excused).
//!
//! Only the players' buses - ours, and on the host the other LAN players' (`players_bus`):
//! the driver chose to drive on. The timetable's AI buses stop as their timetable has them.
//! A LAN client draws the host's people as the host says (`MirrorGesture`), and the host tells
//! it whose bus left whom; (31 October 2026) of those its own bus took from the host at a stop
//! as it came - its own then - the client tells itself.

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
/// it, glances at it (full), waves after it - (31 October 2026) or waves to it as it comes
/// (`greetings`; a game without the word lets it go).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    Hail,
    Full,
    Wave,
    WaveTo,
}

impl Gesture {
    pub fn word(self) -> &'static str {
        match self {
            Gesture::Hail => "hail",
            Gesture::Full => "full",
            Gesture::Wave => "wave",
            Gesture::WaveTo => "waveto",
        }
    }

    pub fn of_word(w: &str) -> Option<Gesture> {
        match w {
            "hail" => Some(Gesture::Hail),
            "full" => Some(Gesture::Full),
            "wave" => Some(Gesture::Wave),
            "waveto" => Some(Gesture::WaveTo),
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

/// (31 October 2026) Whether place `at` - on a stop's platform - is on the side bus `b` opens
/// its doors to: its right where the traffic keeps right, its left on a map that drives on the
/// left (`LEFT_HAND`), a metre either way. A bus going the other way along the street has the
/// platform on its other side.
pub(super) fn door_side(b: &BusNow, at: DVec3) -> bool {
    let f = b.fwd();
    let x = (at - b.pos).truncate().dot(DVec2::new(f.y, -f.x));
    if LEFT_HAND.load(std::sync::atomic::Ordering::Relaxed) {
        x < 1.0
    } else {
        x > -1.0
    }
}

/// Where a stop's platform is: the middle of its waiting places, else where people gather.
pub(super) fn platform(s: &PaxStop) -> DVec3 {
    if s.spots.is_empty() {
        return s.gather;
    }
    s.spots.iter().map(|w| w.pos).sum::<DVec3>() / s.spots.len() as f64
}

/// Where bus `d` is from a stop (`forward` the way the bus goes): metres along, across.
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
        if self.avatar_only {
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
        // (NexiconDK, 31 October 2026: a LAN client, our own bus only - the waiting people it
        // takes from the host as it comes are ours then, and the host knows nothing of them;
        // the host tells it of its own that the bus left, `pax left`)
        let mirror = self.mirror;
        let counted = |id: BusId| super::runners::players_bus(id) && (!mirror || id == BusId::Player);
        if !buses.iter().any(|b| counted(b.id)) {
            return;
        }
        let mut gone: Vec<(BusId, i64, bool, bool)> = Vec::new();
        for b in buses.iter().filter(|b| counted(b.id)) {
            let full = self.bus_full(b);
            for (id, s) in &self.stops {
                let d = b.pos - s.pos;
                if d.length_squared() > 90.0 * 90.0 || d.z.abs() > 8.0 {
                    continue;
                }
                // (by the bus's own way: the stop's heading is the road's, and a stop on its other
                // side is served the other way round - `visual_hail_bus`)
                let (along, across) = stop_frame(d, b.fwd());
                if across > 14.0 || !door_side(b, platform(s)) {
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
            // (who was left, and whether they wave after it - those who wave, `greetings`)
            let left: Vec<(u32, bool)> = self
                .people
                .iter()
                .enumerate()
                .filter_map(|(i, person)| {
                    // (not another game's people drawn here: that game counts them)
                    if person.remote {
                        return None;
                    }
                    let State::Pax(p) = &person.state else { return None };
                    // (NexiconDK, 31 October 2026: and those who stepped up to it - they set off
                    // as it came within 60 m, and were still on their way to it, or at the
                    // gather point, as it went on)
                    // (NexiconDK, 1 November 2026: and somebody in a wheelchair at its door,
                    // waiting for the ramp it never folded out - `ramp`)
                    let ramp = p.ramp_wait == Some(bus) && p.inside.is_none() && p.stop == Some(stop);
                    (ramp || (match p.task { Task::WaitingForBus => true, Task::ToBus => p.bus == Some(bus), _ => false } && p.inside.is_none() && p.stop == Some(stop) && p.late.is_none() && self.takes_waiting(stop, s, b, p.dest.as_deref(), p.line))).then(|| (self.people[i].id, self.waves(i)))
                })
                .collect();
            // (NexiconDK, 1 November 2026) in a wheelchair or with a pram, with their place in it
            // taken: not left by its driver - as at a full bus (`mobility`)
            let no_room: Vec<u32> = self.people.iter().filter(|q| matches!(&q.state, State::Pax(x) if x.no_room == Some(bus))).map(|q| q.id).collect();
            let (excused, left): (Vec<(u32, bool)>, Vec<(u32, bool)>) = left.into_iter().partition(|(id, _)| no_room.contains(id));
            if !excused.is_empty() {
                self.left_behind.push(LeftBehind { bus, stop, count: excused.len() as u32, full: true, stood });
            }
            if left.is_empty() {
                continue;
            }
            for q in self.people.iter_mut().filter(|q| left.iter().any(|l| l.0 == q.id)) {
                if let State::Pax(x) = &mut q.state {
                    x.ramp_wait = None;
                }
            }
            for &(person, waves) in &left {
                self.waving.retain(|w| w.person != person);
                if waves {
                    self.waving.push(WaveAfter { person, bus, since: now, at: b.pos });
                }
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

    pub(crate) fn mirror_bus_at(&self, g: &MirrorGesture) -> DVec3 {
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

    /// (LAN host) What each of our people does for a bus now, for the clients - waves after one
    /// that left them, signals or waves to one coming, glances at it full (`gesture_of`) - with
    /// where that bus is.
    pub fn lan_gestures(&self) -> Vec<(u32, Gesture, DVec3)> {
        let buses: &[BusNow] = &self.last_buses;
        let mut full = |b: &BusNow| self.bus_full(b);
        (0..self.people.len())
            .filter_map(|i| self.gesture_of(i, buses, &mut full).map(|(g, _, at)| (self.people[i].id, g, at)))
            .collect()
    }
}

#[cfg(test)]
#[path = "passed_by_tests.rs"]
mod tests;
