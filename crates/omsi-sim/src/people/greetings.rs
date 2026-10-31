//! (NexiconDK, 31 October 2026) What people do for a bus that takes them, as it comes: a mix,
//! by the settings (`GestureShares`). Waiting at the stop, some signal to it with an arm out to
//! the street, some wave to it with an arm up high, and they stay at their place till it stands;
//! the others step up to where it will stand as it comes within 60 m, as in Omsi.exe. On their
//! way to the stop - walking up, or late and running for it - some wave to it, so that the
//! driver sees them. A full bus gets a glance from some, and nothing from the others. In the
//! dark some of those signalling or waving hold up their phone's light (the view,
//! `omsi_scene::lights::phone_lights`). Those it leaves behind are counted all the same; some of
//! them wave after it (`passed_by`).
//!
//! Who does what is each one's own lot (`lot`): the same for every bus, and in every LAN game
//! (by their id) - a group's all the same.

use super::*;

/// What share of the people do what for a bus (0: nobody ... 1: everybody), as the settings
/// have it: `pax_signal`, `pax_wave`, `pax_full`, `pax_phone`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GestureShares {
    /// Those waiting who signal to a coming bus (and stay at their place till it stands).
    pub signal: f32,
    /// Those who wave: to a coming bus while they wait (staying at their place), on their way
    /// to the stop, and after a bus that left them.
    pub wave: f32,
    /// Those signalling or waving who glance at a full bus instead.
    pub full: f32,
    /// Those signalling or waving who hold up their phone's light in the dark.
    pub phone: f32,
}

/// The settings' steps (`pax_signal` and the others): every time, sometimes, rarely, never.
pub const SHARE_STEPS: [f32; 4] = [1.0, 0.3, 0.1, 0.0];

impl Default for GestureShares {
    /// As the settings have them by default: "Sometimes" every one.
    fn default() -> Self {
        GestureShares { signal: 0.3, wave: 0.3, full: 0.3, phone: 0.3 }
    }
}

impl GestureShares {
    /// Nobody does anything (the original animations, which have none of it).
    pub const NONE: GestureShares = GestureShares { signal: 0.0, wave: 0.0, full: 0.0, phone: 0.0 };
}

/// How somebody waiting greets a bus that takes them as it comes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Greet {
    /// An arm out to the street.
    Signal,
    /// An arm up high, swinging.
    Wave,
}

/// The lots (`lot`): what each is for.
const LOT_SIGNAL: u32 = 1;
const LOT_WAVE: u32 = 2;
const LOT_PICK: u32 = 3;
const LOT_FULL: u32 = 4;
const LOT_PHONE: u32 = 5;

/// One's own lot for `what` (0 to 1): the same every time for the same `key` (a person's id, or
/// a group's), and in every game.
pub fn lot(key: u32, what: u32) -> f32 {
    let mut h = key ^ what.wrapping_mul(0x9E37_79B9);
    h = (h ^ (h >> 16)).wrapping_mul(0x85EB_CA6B);
    h = (h ^ (h >> 13)).wrapping_mul(0xC2B2_AE35);
    h ^= h >> 16;
    (h >> 8) as f32 / (1u32 << 24) as f32
}

/// Whether person `id` holds up their phone's light when they signal or wave in the dark, by
/// the share of those who do.
pub fn holds_phone(id: u32, share: f32) -> bool {
    lot(id, LOT_PHONE) < share
}

/// How far somebody on their way to the stop may be from the bus they wave to (m).
pub const WAVE_TO_REACH: f64 = 80.0;

impl PeopleSim {
    /// The key of person `i`'s lots: their group's (all of them do the same), else their own.
    fn lot_key(&self, i: usize) -> u32 {
        self.pax(i).and_then(|p| p.group).map_or(self.people[i].id, |g| g ^ 0x5bd1_e995)
    }

    /// How person `i`, waiting, greets a bus that takes them as it comes - signals or waves,
    /// staying at their place till it stands - or nothing: they step up to where it will
    /// stand (`stays_till_it_stands`).
    pub fn greets(&self, i: usize) -> Option<Greet> {
        let k = self.lot_key(i);
        let s = lot(k, LOT_SIGNAL) < self.gestures.signal;
        let w = lot(k, LOT_WAVE) < self.gestures.wave;
        match (s, w) {
            (true, true) => Some(if lot(k, LOT_PICK) < 0.5 { Greet::Signal } else { Greet::Wave }),
            (true, false) => Some(Greet::Signal),
            (false, true) => Some(Greet::Wave),
            (false, false) => None,
        }
    }

    /// Whether person `i` waves: on their way to the stop, and after a bus that left them.
    pub fn waves(&self, i: usize) -> bool {
        lot(self.lot_key(i), LOT_WAVE) < self.gestures.wave
    }

    /// Whether person `i` glances at a full bus.
    pub fn glances(&self, i: usize) -> bool {
        lot(self.lot_key(i), LOT_FULL) < self.gestures.full
    }

    /// Whether person `i`, waiting, stays at their place while a bus that takes them rolls in
    /// - signalling or waving to it - and sets off to it only once it (nearly) stands. The
    /// others step up to where it will stand as it comes within 60 m, as in Omsi.exe: before a
    /// signal could be seen.
    pub(crate) fn stays_till_it_stands(&self, i: usize) -> bool {
        self.greets(i).is_some()
    }

    /// What person `i` (one of ours, not another game's drawn here) does for a bus now: the
    /// gesture, the bus and where it is. `full`: whether a bus is full (asked once a bus is
    /// in question).
    pub fn gesture_of(&self, i: usize, buses: &[BusNow], full: &mut dyn FnMut(&BusNow) -> bool) -> Option<(Gesture, BusId, DVec3)> {
        let person = &self.people[i];
        if person.remote || person.puppet.is_some() {
            return None;
        }
        // waving after a bus that left them
        if let Some(w) = self.waving.iter().find(|w| w.person == person.id) {
            return Some((Gesture::Wave, w.bus, w.at));
        }
        let p = self.pax(i)?;
        if p.inside.is_some() {
            return None;
        }
        // late, hurrying for a player's bus: waving to it to wait for them - till they are at it
        if let Some(late) = p.late {
            if !matches!(late.phase, super::runners::LatePhase::Approach | super::runners::LatePhase::Hurry) || !self.waves(i) {
                return None;
            }
            let b = p.bus.and_then(|id| buses.iter().find(|b| b.id == id))?;
            let d = (b.pos - person.position).length_squared();
            return (d > 8.0 * 8.0 && d < WAVE_TO_REACH * WAVE_TO_REACH && !full(b)).then_some((Gesture::WaveTo, b.id, b.pos));
        }
        match p.task {
            Task::WaitingForBus => {
                let (id, moving, at) = self.visual_hail_bus(i, buses)?;
                if !moving {
                    return None;
                }
                let b = buses.iter().find(|b| b.id == id)?;
                if full(b) {
                    return self.glances(i).then_some((Gesture::Full, id, at));
                }
                self.greets(i).map(|g| (if g == Greet::Signal { Gesture::Hail } else { Gesture::WaveTo }, id, at))
            }
            // on their way to the stop: waving to the bus coming to it, or standing there
            Task::WalkingToBusstop => {
                if !self.waves(i) {
                    return None;
                }
                let b = self.coming_bus(p.stop?, person.position, p, buses, WAVE_TO_REACH)?;
                (!full(b)).then_some((Gesture::WaveTo, b.id, b.pos))
            }
            _ => None,
        }
    }

    /// (LAN) What one of another game's people drawn here does for a bus, as that game said -
    /// what this game's settings let be seen (none of a gesture they share nobody with): the
    /// gesture, the bus here and where it is.
    pub fn mirror_gesture_now(&self, id: u32) -> Option<(Gesture, Option<BusId>, DVec3)> {
        let g = self.mirror_gestures.get(&id)?;
        let shown = match g.gesture {
            Gesture::Hail => self.gestures.signal > 0.0,
            Gesture::Full => self.gestures.full > 0.0,
            Gesture::Wave => self.gestures.wave > 0.0 && self.time - g.since < WAVE_SECS,
            Gesture::WaveTo => self.gestures.wave > 0.0,
        };
        shown.then(|| (g.gesture, g.bus, self.mirror_bus_at(g)))
    }
}
