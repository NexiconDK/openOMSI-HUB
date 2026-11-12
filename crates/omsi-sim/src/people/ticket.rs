//! (NexiconDK, ticket inspection round V, 12 November 2026) A player as a ticket inspector: on a
//! Bus Company Online map's world he walks into buses (`on_foot`) and checks the riders one at a
//! time (K by one). Which rider (`nearest_rider`) is decided in his game, among those drawn there;
//! whether that rider has a ticket (`check_ticket`) by the game that runs them - the host for a
//! timetable bus's, the driver's game for a player's bus's (`omsi-app`'s `patrol`).
//!
//! The riders of the player's own bus know it (`Pax::dodger`, drawn as they board). Those of a
//! timetable bus are drawn by the share `ai_share` when first checked, from their id (the same
//! answer whoever asks). A rider checked is checked (`Pax::checked`): asked again, the answer is
//! `Checked`; a dodger caught has a fine and rides on with it.

use super::*;

/// What checking a rider's ticket found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ticket {
    /// A valid ticket (or a pass).
    Valid,
    /// Without one: caught, fined.
    Caught,
    /// Checked before.
    Checked,
    /// Not a rider of a bus here (got off, gone).
    Gone,
}

impl Ticket {
    pub fn key(self) -> &'static str {
        match self {
            Ticket::Valid => "valid",
            Ticket::Caught => "caught",
            Ticket::Checked => "checked",
            Ticket::Gone => "gone",
        }
    }

    pub fn of(key: &str) -> Option<Ticket> {
        [Ticket::Valid, Ticket::Caught, Ticket::Checked, Ticket::Gone].into_iter().find(|t| t.key() == key.trim())
    }
}

/// Whether a timetable bus's rider `id` rides without a ticket, by the share `share` (0 to 0.5):
/// the same for the same rider, whoever asks.
pub fn ai_dodger(id: u32, share: f32) -> bool {
    let mut h = (id as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD6E8_FEB8_6659_FD93;
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    ((h % 10_000) as f32) < share.clamp(0.0, 0.5) * 10_000.0
}

impl PeopleSim {
    /// The rider nearest `at` (in the frame of bus `bus`) within `reach` metres: one drawn here or
    /// run here, not an inspector, not the player's own walker (a puppet) - and none of `but`.
    pub fn nearest_rider(&self, bus: BusId, at: Vec3, reach: f32, but: &[u32]) -> Option<u32> {
        self.people
            .iter()
            .filter(|p| p.puppet.is_none() && !but.contains(&p.id))
            .filter(|p| !super::inspect::is_inspector_variant(&p.variant.checked_sub(1).and_then(|v| p.ty.variants.get(v)).map(|v| v.name.clone()).unwrap_or_default()))
            .filter(|p| !matches!(&p.state, State::Pax(x) if x.task == Task::Inspecting))
            .filter_map(|p| match p.place {
                Place::Bus(b, l) if b == bus => Some((p.id, (l - at).length())),
                _ => None,
            })
            .filter(|(_, d)| *d <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|x| x.0)
    }

    /// The ticket of rider `id` - one run here (not drawn for another game), aboard a bus -
    /// checked: the player's bus's by what they boarded with, a timetable bus's by `ai_share`.
    pub fn check_ticket(&mut self, id: u32, ai_share: f32) -> Ticket {
        let Some(p) = self.people.iter_mut().find(|p| p.id == id && !p.remote && p.puppet.is_none()) else { return Ticket::Gone };
        let Place::Bus(bus, _) = p.place else { return Ticket::Gone };
        let State::Pax(x) = &mut p.state else { return Ticket::Gone };
        if x.task == Task::Inspecting {
            return Ticket::Gone;
        }
        if x.checked {
            return Ticket::Checked;
        }
        x.checked = true;
        let dodger = match bus {
            BusId::Player => std::mem::take(&mut x.dodger),
            BusId::Ai(_) => ai_dodger(id, ai_share),
        };
        if dodger {
            Ticket::Caught
        } else {
            Ticket::Valid
        }
    }

    /// Where avatar `key` is: aboard a bus (and where in it), else None.
    pub fn avatar_aboard(&self, key: u32) -> Option<(BusId, Vec3)> {
        let id = self.avatars.get(&key)?;
        match self.people.iter().find(|p| p.id == *id)?.place {
            Place::Bus(b, l) => Some((b, l)),
            Place::Ground => None,
        }
    }

    /// A grown-up figure (its index) that has the clothes `item`: the first of `prefer` that does,
    /// else any.
    pub fn type_wearing(&self, item: &str, prefer: Option<usize>) -> Option<usize> {
        let has = |t: &Arc<HumanType>| t.def.grown_up() && t.variants.iter().any(|v| v.name.trim().eq_ignore_ascii_case(item.trim()));
        prefer.filter(|k| self.types.get(*k).is_some_and(has)).or_else(|| self.types.iter().position(has))
    }

    /// Avatar `key` put where `cmd` says (`avatar`), made - on its first call - in the clothes
    /// `item` where its figure has them.
    pub fn avatar_in(&mut self, key: u32, world: &dyn World, cmd: AvatarCmd, kind: u64, item: &str) {
        let known = self.avatars.get(&key).is_some_and(|id| self.people.iter().any(|p| p.id == *id));
        if !known && !item.trim().is_empty() {
            let n = self.types.len().max(1) as u64;
            self.force_variant = self.types.get((kind % n) as usize).and_then(|t| t.variants.iter().position(|v| v.name.trim().eq_ignore_ascii_case(item.trim()))).map(|v| v + 1);
        }
        self.avatar(key, world, cmd, kind);
        self.force_variant = None;
    }

    /// The bus rider `id` (one run here) is aboard.
    pub fn rider_bus(&self, id: u32) -> Option<BusId> {
        match self.people.iter().find(|p| p.id == id && !p.remote)?.place {
            Place::Bus(b, _) => Some(b),
            Place::Ground => None,
        }
    }

    /// The riders run here aboard bus `bus` (their ids).
    pub fn riders_of(&self, bus: BusId) -> Vec<u32> {
        self.people.iter().filter(|p| !p.remote && p.puppet.is_none() && matches!(p.place, Place::Bus(b, _) if b == bus)).map(|p| p.id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timetable_bus_s_dodgers_are_the_same_whoever_asks() {
        let n = (0..20_000).filter(|i| ai_dodger(*i, 0.08)).count();
        assert!((1300..1900).contains(&n), "about 8 %: {n}");
        assert!((0..1000).all(|i| ai_dodger(i, 0.08) == ai_dodger(i, 0.08)));
        assert_eq!((0..1000).filter(|i| ai_dodger(*i, 0.0)).count(), 0);
        assert_eq!(Ticket::of("caught"), Some(Ticket::Caught));
        assert_eq!(Ticket::of(Ticket::Checked.key()), Some(Ticket::Checked));
        assert_eq!(Ticket::of("x"), None);
    }
}
