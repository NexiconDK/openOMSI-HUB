//! (the fork, from openOMSI_DK) Where passengers sit, as people choose (the setting
//! `pax_sit`, the wish to sit; 0 leaves the pick of `reserve_place`, a free seat at random
//! and a standing place by `stand_chance`):
//!
//! - a seat is wanted by the chance to stand although one is free (`stand_chance`, the
//!   setting "Standing passengers") and the wish to sit (`wants_seat`); the elderly always
//!   want one;
//! - a window seat before the one by the aisle, a double seat to themselves before one
//!   beside a stranger;
//! - an elderly passenger (`ELDERLY` years and older, the `.hum`'s `[age]`) often goes for
//!   the pilot's seat - across the aisle from the driver, a little behind the cab, on
//!   whichever side the wheel is not - where the bus has one; the others leave it to them
//!   while they have another;
//! - a group (two to four with one destination) takes seats together: two a double seat,
//!   three or four a "four" (two double seats facing each other), else seats near one
//!   another, else standing places near one another;
//! - in the player's bus a rider standing for a longer ride takes a seat that comes free
//!   (not one who chose to stand),
//!   and now and then (at most once in `GIVE_SEAT_EVERY`) somebody stands up for an elderly
//!   passenger who found no seat.
//!
//! All of it from where the cabin's places are (bus frame: x to the right, y forward):
//! two seats side by side are a pair (the same row, a seat's width apart), the outer one of
//! a pair the window seat, a seat with no other beside it a single (by the window too).

use super::cabin::Seat;
use glam::Vec3;

/// From this age (years) a passenger counts as elderly: the pilot's seat, and somebody
/// stands up for them.
pub const ELDERLY: f32 = 65.0;
/// Below this age (years) a passenger is a child: travels with an adult in a group.
pub const CHILD: f32 = 14.0;
/// How often an elderly passenger goes for the pilot's seat.
pub const PILOT_SHARE: f64 = 0.7;
/// Of the people waiting at a stop, about this share wait and travel in groups.
pub const GROUP_SHARE: f64 = 0.2;
/// Somebody stands up for an elderly passenger at most once in this many seconds a bus.
pub const GIVE_SEAT_EVERY: f64 = 150.0;
/// A rider standing changes to a free seat only with at least this far to go (km).
const LONG_RIDE_KM: f64 = 1.0;
/// Places kept for a group's members still to get in are given back after this (s).
pub const GROUP_KEEP: f64 = 90.0;

/// Two seats are in one row when their fronts are this close along the bus (m).
const ROW: f32 = 0.35;
/// Two seats of a row are side by side when their centres are between these apart across
/// the bus (m): further apart, the aisle lies between them.
const BESIDE: (f32, f32) = (0.3, 0.75);
/// Two pairs facing each other make a "four" when they are this far apart along the bus.
const FACING: (f32, f32) = (0.8, 2.0);
/// The pilot's seat: across the aisle from the driver, from a little ahead of the
/// driver's place to this far behind it (m).
const PILOT_BEHIND: f32 = 3.0;

/// What the passengers know of a cabin's seats.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SeatMap {
    /// Per place: a seat by the window (or a single seat).
    pub window: Vec<bool>,
    /// Per place: the seat beside it, the two making a pair.
    pub beside: Vec<Option<usize>>,
    /// The pilot's seat, where the bus has one.
    pub pilot: Option<usize>,
    /// The "fours": two pairs facing each other (four seats).
    pub fours: Vec<[usize; 4]>,
}

impl SeatMap {
    /// The map of `places`, the driver at `driver` (bus frame) when known.
    pub fn of(places: &[Seat], driver: Option<Vec3>) -> SeatMap {
        let n = places.len();
        let mut beside = vec![None; n];
        for i in 0..n {
            if !places[i].seated {
                continue;
            }
            // the nearest seat of the same row a seat's width away, on the same side of the
            // aisle (they face the same way)
            let mut best: Option<(usize, f32)> = None;
            for j in 0..n {
                if j == i || !places[j].seated || places[j].group != places[i].group {
                    continue;
                }
                let (a, b) = (places[i].pos, places[j].pos);
                let across = (a.x - b.x).abs();
                if (a.y - b.y).abs() < ROW && across > BESIDE.0 && across < BESIDE.1 && same_way(places[i].rot, places[j].rot) && best.is_none_or(|(_, d)| across < d) {
                    best = Some((j, across));
                }
            }
            beside[i] = best.map(|b| b.0);
        }
        // (only pairs both ends agree on: a seat between two others is in no pair)
        for i in 0..n {
            if let Some(j) = beside[i] {
                if beside[j] != Some(i) {
                    beside[i] = None;
                }
            }
        }
        let centre = centre_x(places);
        let window = (0..n)
            .map(|i| {
                places[i].seated
                    && match beside[i] {
                        Some(j) => (places[i].pos.x - centre).abs() > (places[j].pos.x - centre).abs(),
                        None => true,
                    }
            })
            .collect();
        // the pilot's seat: the front row across the aisle from the driver, its seat nearer
        // the driver
        let pilot = driver.and_then(|d| {
            let driver_left = d.x < centre;
            let candidates: Vec<usize> = (0..n)
                .filter(|&i| {
                    let p = places[i].pos;
                    places[i].seated && places[i].group == 0 && (p.x < centre) != driver_left && p.y <= d.y + 1.0 && p.y >= d.y - PILOT_BEHIND
                })
                .collect();
            let front = candidates.iter().map(|&i| places[i].pos.y).fold(f32::MIN, f32::max);
            candidates
                .into_iter()
                .filter(|&i| places[i].pos.y > front - ROW)
                .min_by(|&a, &b| (places[a].pos.x - d.x).abs().total_cmp(&(places[b].pos.x - d.x).abs()))
        });
        // the fours: two pairs of one side facing each other
        let mut fours = Vec::new();
        for a in 0..n {
            let Some(a2) = beside[a].filter(|b| *b > a) else { continue };
            for b in 0..n {
                let Some(b2) = beside[b].filter(|x| *x > b) else { continue };
                if b <= a {
                    continue;
                }
                let (pa, pb) = (places[a].pos, places[b].pos);
                let along = (pa.y - pb.y).abs();
                let same_side = (pa.x < centre) == (pb.x < centre);
                let facing = !same_way(places[a].rot, places[b].rot);
                // (the pair in front faces back, the one behind forward: they look at each other)
                let face_each_other = facing && {
                    let (front, back) = if pa.y > pb.y { (a, b) } else { (b, a) };
                    faces_back(places[front].rot) && !faces_back(places[back].rot)
                };
                if same_side && along > FACING.0 && along < FACING.1 && face_each_other {
                    fours.push([a, a2, b, b2]);
                }
            }
        }
        SeatMap { window, beside, pilot, fours }
    }

    /// The place somebody takes of the `free` ones (`taken`: every place of the bus, true
    /// where somebody is): a seat when `seat` (and one is free), else a standing place;
    /// `pilot`: the pilot's seat first (an elderly passenger who goes for it); `elderly`:
    /// the pilot's seat is not left to others. Among the seats: one of a pair with nobody
    /// in the other first, by the window first; then a single; the pilot's seat last but
    /// for the elderly. `pick` breaks ties.
    #[allow(clippy::too_many_arguments)]
    pub fn choose(&self, free: &[usize], taken: &[bool], places: &[Seat], seat: bool, pilot: bool, elderly: bool, pick: usize) -> Option<usize> {
        if free.is_empty() {
            return None;
        }
        let seats: Vec<usize> = free.iter().copied().filter(|k| places.get(*k).is_some_and(|p| p.seated)).collect();
        let standing: Vec<usize> = free.iter().copied().filter(|k| places.get(*k).is_some_and(|p| !p.seated)).collect();
        if (seat || elderly) && !seats.is_empty() {
            if let Some(d) = self.pilot.filter(|d| pilot && seats.contains(d)) {
                return Some(d);
            }
            let top = seats.iter().map(|&k| self.score(k, taken, elderly)).max()?;
            let best: Vec<usize> = seats.into_iter().filter(|&k| self.score(k, taken, elderly) == top).collect();
            return Some(best[pick % best.len()]);
        }
        // a standing place: any of them, as Omsi.exe takes it
        let from = if standing.is_empty() { free.to_vec() } else { standing };
        Some(from[pick % from.len()])
    }

    /// How much a passenger likes seat `k`: a pair to themselves, by the window; not the
    /// pilot's seat but for the elderly.
    fn score(&self, k: usize, taken: &[bool], elderly: bool) -> i32 {
        let alone = match self.beside.get(k).copied().flatten() {
            Some(j) => !taken.get(j).copied().unwrap_or(false),
            None => true,
        };
        let window = self.window.get(k).copied().unwrap_or(false);
        let pilot = self.pilot == Some(k) && !elderly;
        (alone as i32) * 4 + (window as i32) * 2 - (pilot as i32) * 8
    }

    /// The places a group of `n` takes of the `free` ones: a four for three or four, a pair
    /// for two, else seats near the first one; then standing places near them. As many as
    /// there are (fewer than `n` when the bus is that full). `seat`: seats wanted.
    pub fn choose_group(&self, free: &[usize], taken: &[bool], places: &[Seat], n: usize, seat: bool, pick: usize) -> Vec<usize> {
        if n <= 1 {
            return self.choose(free, taken, places, seat, false, false, pick).into_iter().collect();
        }
        let is_free = |k: &usize| free.contains(k);
        let mut out: Vec<usize> = Vec::new();
        if seat {
            if n >= 3 {
                if let Some(f) = self.fours.iter().find(|f| f.iter().all(is_free)) {
                    out.extend(f.iter().take(n));
                }
            }
            if out.is_empty() {
                let pairs: Vec<(usize, usize)> = (0..self.beside.len()).filter_map(|k| self.beside[k].filter(|j| *j > k).map(|j| (k, j))).filter(|(a, b)| is_free(a) && is_free(b)).collect();
                if let Some(&(a, b)) = pairs.get(pick % pairs.len().max(1)) {
                    out.extend([a, b]);
                }
            }
            if out.is_empty() {
                if let Some(k) = self.choose(free, taken, places, true, false, false, pick).filter(|k| places[*k].seated) {
                    out.push(k);
                }
            }
            // more seats near them
            while out.len() < n {
                let first = out.first().copied();
                let next = free
                    .iter()
                    .copied()
                    .filter(|k| places[*k].seated && !out.contains(k) && self.pilot != Some(*k))
                    .min_by(|a, b| dist(places, first, *a).total_cmp(&dist(places, first, *b)));
                match next {
                    Some(k) if first.is_none_or(|f| (places[k].pos - places[f].pos).length() < 2.5) => out.push(k),
                    _ => break,
                }
            }
        }
        // standing places near them (or near the door's end of the bus: the first free)
        while out.len() < n {
            let first = out.first().copied();
            let next = free.iter().copied().filter(|k| !places[*k].seated && !out.contains(k)).min_by(|a, b| dist(places, first, *a).total_cmp(&dist(places, first, *b)));
            match next {
                Some(k) => out.push(k),
                None => break,
            }
        }
        out
    }
}

/// How far place `k` is from place `from` (0 with none).
fn dist(places: &[Seat], from: Option<usize>, k: usize) -> f32 {
    from.map(|f| (places[k].pos - places[f].pos).length()).unwrap_or(0.0)
}

/// Whether two seats face the same way (their headings within 30°).
fn same_way(a: f32, b: f32) -> bool {
    let d = (a - b).rem_euclid(360.0);
    d.min(360.0 - d) < 30.0
}

/// Whether a seat faces the back of the bus.
fn faces_back(rot: f32) -> bool {
    let d = rot.rem_euclid(360.0);
    d > 90.0 && d < 270.0
}

/// The middle of the cabin across: halfway between its outermost places.
fn centre_x(places: &[Seat]) -> f32 {
    let (lo, hi) = places.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.pos.x), hi.max(p.pos.x)));
    if lo <= hi {
        (lo + hi) * 0.5
    } else {
        0.0
    }
}

/// (the fork) Seconds between two looks round the player's bus for a seat to change to, at
/// the wish to sit 1.
const RESEAT_EVERY: f32 = 2.0;

impl super::PeopleSim {
    /// (the fork) Whether passengers choose their places as people do (`sit_wish` above 0).
    pub fn seating(&self) -> bool {
        self.sit_wish > 0.0
    }

    /// (the fork) Whether a passenger getting in wants a seat (while one is free): the
    /// chance to stand although a seat is free (`stand_chance`, the setting "Standing
    /// passengers"), less with a greater wish to sit; with a little wish (under 1) only that
    /// share of them wants a seat at all.
    pub fn wants_seat(&mut self) -> bool {
        let wish = self.sit_wish.clamp(0.0, 2.0);
        let stand = 1.0 - (1.0 - self.stand_chance.clamp(0.0, 1.0) / wish.max(1.0)) * wish.min(1.0);
        self.rand_f() as f32 >= stand
    }

    /// (the fork) A free place of the bus as passenger `i` chooses it: a seat when
    /// `prefer_seat` (or they are elderly), else a standing place - with their group, the
    /// places for all of them at once (the others take theirs as they get in). None:
    /// nothing free.
    pub fn reserve_place_as_people_do(&mut self, i: usize, bus: super::BusId, cabin: &super::cabin::Cabin, off: &[bool], prefer_seat: Option<bool>) -> Option<usize> {
        let places = &cabin.seats;
        let n = places.len();
        let group = self.pax(i).and_then(|p| p.group);
        // a place kept for them by the group's first
        if let Some(g) = group {
            if let Some((kept, _)) = self.group_places.get_mut(&(g, bus)) {
                if let Some(k) = kept.pop() {
                    if kept.is_empty() {
                        self.group_places.remove(&(g, bus));
                    }
                    return Some(k);
                }
            }
        }
        let taken = self.seats.entry(bus).or_insert_with(|| vec![false; n]);
        if taken.len() < n {
            taken.resize(n, false);
        }
        let taken = taken.clone();
        let free: Vec<usize> = (0..n).filter(|k| !taken[*k] && !off.get(*k).copied().unwrap_or(false)).collect();
        let elderly = self.people.get(i).is_some_and(|p| p.age >= ELDERLY);
        let pilot = self.wants_pilot(i);
        let map = SeatMap::of(places, cabin.data.driver_positions.first().map(|d| Vec3::from(d.pos)));
        let pick = self.rand() as usize;
        let seat = prefer_seat.unwrap_or(true);
        // the group's members still to get in: their places together
        let members = group.map(|g| self.group_waiting(g, i)).unwrap_or(0);
        let chosen: Vec<usize> = if members > 0 {
            map.choose_group(&free, &taken, places, members + 1, seat || elderly, pick)
        } else {
            map.choose(&free, &taken, places, seat, pilot, elderly, pick).into_iter().collect()
        };
        let (&k, rest) = chosen.split_first()?;
        let seats = self.seats.get_mut(&bus).unwrap();
        for &c in &chosen {
            seats[c] = true;
        }
        if let (Some(g), false) = (group, rest.is_empty()) {
            self.group_places.insert((g, bus), (rest.to_vec(), self.time));
        }
        if bus == super::BusId::Player {
            log::info!(
                "player bus boarding (as people do): {}{}wants {}; took {} place {k}{}",
                if elderly { "an elderly passenger " } else { "" },
                if members > 0 { format!("a group of {} ", members + 1) } else { String::new() },
                if seat || elderly { "a seat" } else { "to stand" },
                if places[k].seated { "seated" } else { "standing" },
                if map.pilot == Some(k) { " (the pilot's seat)" } else if map.window.get(k).copied().unwrap_or(false) { " (by the window)" } else { "" },
            );
        }
        Some(k)
    }

    /// (the fork) Whether person `i` goes for the pilot's seat: an elderly passenger, often
    /// (`PILOT_SHARE`) - the same one always.
    fn wants_pilot(&self, i: usize) -> bool {
        self.people.get(i).is_some_and(|p| p.age >= ELDERLY && (p.id.wrapping_mul(2_654_435_761) >> 8) % 1000 < (PILOT_SHARE * 1000.0) as u32)
    }

    /// (the fork) The ages of group `g`'s members besides person `i` still to get in (whose
    /// tickets `i` buys).
    pub fn group_ages(&self, g: u32, i: usize) -> Vec<f32> {
        use super::{pax::Task, State};
        self.people
            .iter()
            .enumerate()
            .filter(|(j, p)| *j != i && matches!(&p.state, State::Pax(x) if x.group == Some(g) && x.inside.is_none() && x.seat.is_none() && matches!(x.task, Task::WaitingForBus | Task::ToBus | Task::WalkingToBusstop)))
            .map(|(_, p)| p.age)
            .collect()
    }

    /// (the fork) How many of group `g` besides person `i` are still to get in (waiting or
    /// walking up to the bus, no place yet).
    fn group_waiting(&self, g: u32, i: usize) -> usize {
        use super::{pax::Task, State};
        self.people
            .iter()
            .enumerate()
            .filter(|(j, p)| *j != i && matches!(&p.state, State::Pax(x) if x.group == Some(g) && x.inside.is_none() && x.seat.is_none() && matches!(x.task, Task::WaitingForBus | Task::ToBus | Task::WalkingToBusstop)))
            .count()
    }

    /// (the fork) The places kept for a group's members who never came are given back.
    pub fn group_places_tick(&mut self) {
        let now = self.time;
        let old: Vec<(u32, super::BusId)> = self.group_places.iter().filter(|(_, (_, t))| now - *t > GROUP_KEEP).map(|(k, _)| *k).collect();
        for key in old {
            if let Some((kept, _)) = self.group_places.remove(&key) {
                for k in kept {
                    self.free_seat(key.1, k);
                }
            }
        }
    }

    /// (the fork) In the player's bus, while it is not boarding (rolling, or its doors
    /// shut), every couple of seconds: an elderly passenger standing takes a free seat - and
    /// with none free, now and then a younger one sitting stands up for them; else one
    /// passenger standing for a longer ride takes a seat somebody got up from. One at a
    /// time, so they do not all rush at once.
    pub fn reseat_player_bus(&mut self, dt: f32, buses: &[super::BusNow], bus_ix: &hashbrown::HashMap<super::BusId, usize>) {
        use super::{pax::Task, BusId, State};
        if !self.seating() {
            return;
        }
        self.reseat_t -= dt;
        if self.reseat_t > 0.0 {
            return;
        }
        self.reseat_t = RESEAT_EVERY / self.sit_wish.clamp(0.25, 4.0);
        let Some(bn) = bus_ix.get(&BusId::Player).map(|k| &buses[*k]) else { return };
        if bn.speed.abs() < 1.0 && bn.entry_open.iter().chain(&bn.exit_open).any(|o| *o) {
            return;
        }
        let places = &bn.cabin.seats;
        let n = places.len();
        let mut taken = self.seats.get(&BusId::Player).cloned().unwrap_or_default();
        taken.resize(n, false);
        let off = |k: usize| bn.places_off.get(k).copied().unwrap_or(false);
        let free: Vec<usize> = (0..n).filter(|&k| !taken[k] && !off(k)).collect();
        let km = self.odometer.get(&BusId::Player).copied().unwrap_or(0.0);
        let next = bn.next_stop.clone();
        // the riders at their places: (person, place, elderly, a longer ride ahead)
        let riders: Vec<(usize, usize, bool, bool)> = self
            .people
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match &p.state {
                State::Pax(x) if x.inside == Some(BusId::Player) && x.task == Task::SittingInBus => {
                    let off_next = x.dest.as_ref().is_some_and(|d| next.as_ref().is_some_and(|s| s.is_named(d)));
                    let left = x.km_start + x.ride_km as f64 - km;
                    let long = !off_next && (x.dest.is_some() || left >= LONG_RIDE_KM);
                    x.seat.filter(|k| *k < n).map(|k| (i, k, p.age >= ELDERLY, long))
                }
                _ => None,
            })
            .collect();
        let map = SeatMap::of(places, bn.cabin.data.driver_positions.first().map(|d| Vec3::from(d.pos)));
        let pick = self.rand() as usize;
        // an elderly passenger standing
        if let Some(&(old, at, _, _)) = riders.iter().find(|(_, k, e, _)| *e && !places[*k].seated) {
            let pilot = self.wants_pilot(old);
            if let Some(k) = map.choose(&free, &taken, places, true, pilot, true, pick).filter(|k| places[*k].seated) {
                self.move_rider(old, k, bn);
                return;
            }
            // nothing free: now and then the younger one sitting nearest stands up
            if self.time - self.gave_seat_at < GIVE_SEAT_EVERY {
                return;
            }
            let standing: Vec<usize> = free.iter().copied().filter(|k| !places[*k].seated).collect();
            let near = |k: usize| (places[k].pos - places[at].pos).length();
            let young = riders.iter().filter(|(_, k, e, _)| !*e && places[*k].seated).min_by(|a, b| near(a.1).total_cmp(&near(b.1))).copied();
            if let (Some((y, seat, _, _)), false) = (young, standing.is_empty()) {
                // (the standing place nearest the seat)
                let to = standing.iter().copied().min_by(|a, b| (places[*a].pos - places[seat].pos).length().total_cmp(&(places[*b].pos - places[seat].pos).length())).unwrap();
                self.move_rider(y, to, bn);
                self.move_rider(old, seat, bn);
                self.gave_seat_at = self.time;
                log::info!("player bus: a passenger stands up for an elderly one (seat {seat})");
            }
            return;
        }
        // somebody standing for a longer ride takes a free seat
        let standing_riders: Vec<usize> = riders.iter().filter(|(i, k, _, long)| !places[*k].seated && *long && !self.pax(*i).is_some_and(|x| x.stands_by_choice)).map(|r| r.0).collect();
        if standing_riders.is_empty() {
            return;
        }
        if let Some(k) = map.choose(&free, &taken, places, true, false, false, pick).filter(|k| places[*k].seated) {
            let who = standing_riders[pick % standing_riders.len()];
            self.move_rider(who, k, bn);
        }
    }

    /// (the fork) Rider `i` of the player's bus goes from their place to place `to`: the old
    /// one is free, the new one theirs, and they walk there along the cabin's paths.
    fn move_rider(&mut self, i: usize, to: usize, bn: &super::BusNow) {
        use super::pax::{Task, TICKET_NONE};
        let Some(old) = self.pax(i).and_then(|x| x.seat) else { return };
        if let Some(t) = self.seats.get_mut(&super::BusId::Player).and_then(|v| v.get_mut(to)) {
            *t = true;
        }
        self.free_seat(super::BusId::Player, old);
        let from = bn.cabin.seats.get(old).map(|s| s.floor).unwrap_or(Vec3::ZERO);
        let all = bn.cabin.all_points();
        let start = bn.cabin.omsi_nearest(from, &all, false, true, None, None);
        let at = start.and_then(|k| bn.cabin.graph.points.get(k)).copied();
        let Some(p) = self.pax_mut(i) else { return };
        p.seat = Some(to);
        p.pt = start;
        if let Some(q) = at {
            p.pos = q.as_dvec3();
        }
        p.seat_h = 0.0;
        p.pax_state = 1.0;
        p.ticket = TICKET_NONE;
        p.task = Task::InBusToPlace;
        self.route_to_place(i, bn);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seat(x: f32, y: f32, rot: f32) -> Seat {
        Seat { pos: Vec3::new(x, y, 0.9), floor: Vec3::new(x, y, 0.4), rot, seated: true, height: 0.45, omsi_seat: 0, switch_var: None, taken_var: None, group: 0 }
    }

    fn stand(x: f32, y: f32) -> Seat {
        Seat { seated: false, height: 0.0, ..seat(x, y, 0.0) }
    }

    /// A small bus, the driver front left: the front pair right (the pilot's seat its aisle
    /// one), a four on the left (a pair facing back, a pair behind it facing forward), a pair
    /// right further back, a single at the back; standing places in the aisle.
    fn bus() -> (Vec<Seat>, Vec3) {
        let places = vec![
            seat(1.0, 3.6, 0.0),     // 0: front right, window
            seat(0.55, 3.6, 0.0),    // 1: front right, aisle: the pilot's seat
            seat(-1.0, 2.0, 180.0),  // 2: four, front pair (facing back), window
            seat(-0.55, 2.0, 180.0), // 3
            seat(-1.0, 0.6, 0.0),    // 4: four, back pair (facing forward), window
            seat(-0.55, 0.6, 0.0),   // 5
            seat(1.0, -1.0, 0.0),    // 6: right pair, window
            seat(0.55, -1.0, 0.0),   // 7
            seat(-1.0, -3.0, 0.0),   // 8: a single
            stand(0.0, 3.0),         // 9
            stand(0.0, 1.0),         // 10
            stand(0.0, 0.5),         // 11
        ];
        (places, Vec3::new(-0.8, 4.5, 1.0))
    }

    fn free(taken: &[bool]) -> Vec<usize> {
        (0..taken.len()).filter(|k| !taken[*k]).collect()
    }

    #[test]
    fn the_cabin_knows_its_pairs_windows_fours_and_the_pilots_seat() {
        let (places, driver) = bus();
        let m = SeatMap::of(&places, Some(driver));
        assert_eq!(m.beside[0], Some(1));
        assert_eq!(m.beside[2], Some(3));
        assert_eq!(m.beside[8], None, "a single seat");
        assert!(m.window[0] && !m.window[1] && m.window[2] && !m.window[3] && m.window[8]);
        assert!(!m.window[9], "a standing place is no window seat");
        assert_eq!(m.pilot, Some(1), "across the aisle from the driver, the seat nearer the driver");
        assert_eq!(m.fours, vec![[2, 3, 4, 5]]);
        // a right-hand drive: the pilot's seat on the left
        let m = SeatMap::of(&places, Some(Vec3::new(0.8, 4.5, 1.0)));
        assert_eq!(m.pilot, Some(3), "{:?}", m.pilot);
    }

    #[test]
    fn people_take_a_window_seat_of_an_empty_pair_and_leave_the_pilots_seat_to_the_elderly() {
        let (places, driver) = bus();
        let m = SeatMap::of(&places, Some(driver));
        let mut taken = vec![false; places.len()];
        for pick in 0..8 {
            let k = m.choose(&free(&taken), &taken, &places, true, false, false, pick).unwrap();
            assert!([0, 2, 4, 6, 8].contains(&k), "{k}");
        }
        // an elderly passenger who goes for it: the pilot's seat
        assert_eq!(m.choose(&free(&taken), &taken, &places, true, true, true, 3), Some(1));
        // one who does not: a window seat like anybody
        assert!(m.window[m.choose(&free(&taken), &taken, &places, true, false, true, 3).unwrap()]);
        // every pair has somebody: beside a stranger, the pilot's seat last
        for k in [0, 2, 4, 6, 8] {
            taken[k] = true;
        }
        let k = m.choose(&free(&taken), &taken, &places, true, false, false, 0).unwrap();
        assert!([3, 5, 7].contains(&k), "{k}");
        for k in [3, 5, 7] {
            taken[k] = true;
        }
        assert_eq!(m.choose(&free(&taken), &taken, &places, true, false, false, 0), Some(1));
        // standing wanted: a standing place
        let k = m.choose(&free(&taken), &taken, &places, false, false, false, 1).unwrap();
        assert!(!places[k].seated);
    }

    /// The wish to sit on top of the chance to stand although a seat is free: at 1 the
    /// chance as set; a little wish (0.5) leaves half of the others standing too; a great one
    /// (2) halves the chance.
    #[test]
    fn the_wish_to_sit_on_top_of_the_chance_to_stand() {
        let mut h = super::super::PeopleSim::new(std::path::Path::new("/nonexistent"), 200);
        let mut stands = |wish: f32, chance: f32| {
            h.sit_wish = wish;
            h.stand_chance = chance;
            (0..4000).filter(|_| !h.wants_seat()).count() as f32 / 4000.0
        };
        assert!((stands(1.0, 0.25) - 0.25).abs() < 0.03);
        assert!((stands(0.5, 0.05) - 0.525).abs() < 0.04);
        assert!((stands(2.0, 0.2) - 0.1).abs() < 0.03);
        assert_eq!(stands(1.0, 0.0), 0.0);
        assert_eq!(stands(1.5, 1.0) > 0.6, true);
    }

    #[test]
    fn a_group_sits_together() {
        let (places, driver) = bus();
        let m = SeatMap::of(&places, Some(driver));
        let mut taken = vec![false; places.len()];
        let mut four = m.choose_group(&free(&taken), &taken, &places, 4, true, 0);
        four.sort_unstable();
        assert_eq!(four, vec![2, 3, 4, 5], "four in a four");
        let three = m.choose_group(&free(&taken), &taken, &places, 3, true, 0);
        assert!(three.iter().all(|k| [2, 3, 4, 5].contains(k)) && three.len() == 3);
        for k in [2, 3, 4, 5] {
            taken[k] = true;
        }
        let mut two = m.choose_group(&free(&taken), &taken, &places, 2, true, 0);
        two.sort_unstable();
        assert!(two == vec![0, 1] || two == vec![6, 7], "{two:?}");
        // the four taken: three get a pair and the seat nearest it
        let three = m.choose_group(&free(&taken), &taken, &places, 3, true, 1);
        assert_eq!(three.len(), 3, "{three:?}");
        // standing: places near one another
        let st = m.choose_group(&free(&taken), &taken, &places, 2, false, 0);
        assert!(st.iter().all(|k| !places[*k].seated) && st.len() == 2, "{st:?}");
    }
}
