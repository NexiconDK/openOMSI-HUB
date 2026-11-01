//! (NexiconDK, 1 November 2026) People in a wheelchair or with a pram (`[openomsi_mobility]` of
//! their `.hum`, `omsi_content::human::Mobility`).
//!
//! * They wait standing (never on a bench) and are in no group, never run for a bus and make
//!   no gestures to one (the wheelchair's figure would tip with the turn, the pram's let go of
//!   the handle).
//! * They board only where the bus's place for them (`cabin::Seat::mobility`: a `[passpos]`
//!   marked `{mobility}`, else the standing place the cabin chose by its second door) is free -
//!   for a wheelchair, by a door without steps to it - else they wait for the next one, which
//!   counts as at a full bus, not as left behind. They buy no ticket (a pass), get in by the door
//!   nearest the place, stand there facing as it says, are never moved to another place, and
//!   get out by the door nearest it, pressing the bus's button for them
//!   (`int_kinderwagenwunsch`) where it has one.
//! * The others take that place only when every other one is taken.

use super::*;
use omsi_content::human::Mobility;

impl PeopleSim {
    /// How person `i` gets about: in a wheelchair, with a pram, or on foot (None).
    pub fn mobility(&self, i: usize) -> Option<Mobility> {
        self.people.get(i).and_then(|p| p.ty.def.mobility)
    }

    /// The places of bus cabin `cabin` person `i` may not take: those its scripts switched off
    /// (`off`, #721); for one in a wheelchair or with a pram every place but theirs (a
    /// wheelchair's also where no door without steps leads); for the others theirs, unless
    /// every other place is taken.
    pub fn places_off_for(&self, i: usize, bus: BusId, cabin: &Cabin, off: &[bool]) -> Vec<bool> {
        let n = cabin.seats.len();
        let base: Vec<bool> = (0..n).map(|k| off.get(k).copied().unwrap_or(false)).collect();
        match self.mobility(i) {
            Some(m) => (0..n).map(|k| base[k] || !cabin.seats[k].mobility || (m == Mobility::Wheelchair && cabin.mobility_door(&cabin.entries, k, true).is_none())).collect(),
            None => {
                let kept: Vec<bool> = (0..n).map(|k| base[k] || cabin.seats[k].mobility).collect();
                let taken = self.seats.get(&bus);
                let free_else = (0..n).any(|k| !kept[k] && !taken.and_then(|t| t.get(k)).copied().unwrap_or(false));
                if free_else || !cabin.seats.iter().any(|s| s.mobility) {
                    kept
                } else {
                    base
                }
            }
        }
    }

    /// A figure of the pool that walks (no wheelchair, no pram), drawn at random - for the
    /// riders seeded into a bus, a group's members and the people late for a bus. None (the
    /// usual draw) where the pool has none of those others.
    pub fn walker_kind(&mut self) -> Option<usize> {
        let n = self.pool_types.clamp(1, self.types.len().max(1)).min(self.types.len());
        if !self.types[..n].iter().any(|t| t.def.mobility.is_some()) {
            return None;
        }
        let walkers: Vec<usize> = (0..n).filter(|&k| self.types[k].def.mobility.is_none()).collect();
        if walkers.is_empty() {
            return None;
        }
        Some(walkers[(self.rand() as usize) % walkers.len()])
    }

    /// `spawn` with a figure that walks (`walker_kind`).
    pub fn spawn_walker(&mut self, world: &dyn World, position: DVec3, heading: f64, state: State) -> Option<usize> {
        self.use_map_humans(world);
        let kind = self.walker_kind();
        if kind.is_none() {
            return self.spawn(world, position, heading, state);
        }
        if !self.pool_room() {
            return None;
        }
        self.spawn_as(world, position, heading, state, kind)
    }

    /// Whether the waiting place `k` of stop `stop` suits person `i`: a bench none in a
    /// wheelchair or with a pram.
    pub fn spot_suits(&self, i: usize, stop: i64, k: usize) -> bool {
        self.mobility(i).is_none() || self.stops.get(&stop).and_then(|s| s.spots.get(k)).is_some_and(|sp| sp.height == 0.0)
    }
}

#[cfg(test)]
#[path = "mobility_tests.rs"]
mod tests;
