//! (NexiconDK, Bus Company Online: relief at the stop, decided 26 October 2026) A LAN player who
//! takes a tour over from the AI waits on foot at the stop his duty begins at - the first stop of
//! its trip, or a relief stop along it -, and the AI's bus of the tour comes there and holds with
//! its doors open (`ReliefHold`) until his game asks for it (`take_relief`: the bus leaves the
//! host's world, and his game puts his own down where it stood, the driver gets up and walks out)
//! or until `RELIEF_WAIT` past its departure there (a late bus a minute after it came,
//! `RELIEF_ARRIVED_WAIT`): then the AI drives that trip after all (`relief_kept`), and he waits for
//! the bus at the start of his next trip.
//!
//! His game asks for it again every few seconds (`want_relief`); one not heard from for
//! `RELIEF_STALE` is let go. While he waits his game does not claim that trip (`LanClaim`: the
//! AI's bus of the tour would go), and the departure is the AI's (`is_player_tour`) whatever his
//! claim says: its bus takes the trip on at the terminus rather than driving off, or one is put out
//! for it, and it is put no further than his stop when it is late. (31 October 2026) One put out
//! where he would see it appear - at his stop, or its stand in front of him - is put out back along
//! its way instead and drives up (`approach`); at a stand other lines share, a few minutes early
//! (`RELIEF_LAYOVER`).
//!
//! When another LAN player drives the bus up to his stop - his claim has the trip before (a relief
//! at the trip's first stop), or the trip itself (a relief on the way) - it is that player's to
//! hand over (`ReliefState::FromPlayer`): he gets out at the stop, and his game gives the bus on
//! (`gave_relief`). Not there within `RELIEF_WAIT` of the departure (or gone from the session),
//! the AI brings a bus instead.
//!
//! (27 October 2026) At the end of his duty, where the AI drives the tour on, the AI takes his bus
//! over (`want_back`): its departure after his is held back - no bus of the AI's put out at the stop
//! beside his - while his game says so, he gets out, the AI's driver walks up and gets in (his
//! game's figure), and his game gives the bus back (`gave_back`): it leaves his game, and the AI's
//! bus of that departure is put out at the stop where it stood, waiting for its time. Not given
//! back by its departure (or his game no longer asks), the AI's own bus comes, as before.
//!
//! (28 October 2026) He may hand his bus over at a stop on the way through his duty too
//! (`want_stop`, his game's "Hand the bus over here"): at his trip's terminus the tour's next
//! departure is the AI's with it, at a stop along his trip that trip itself, put out at that stop
//! - his own claim's then, held for him without its wait's limits. Given back, his company is told
//! (`take_handed`): the AI drives the rest of his duty, and he is free.

use super::*;
use crate::ai_traffic::bus_service::ReliefHold;

/// How long past its departure at the relief stop the bus holds for him (s): five minutes, then
/// the AI drives the trip (decided 26 October 2026).
pub const RELIEF_WAIT: f64 = 300.0;

/// A bus that comes late holds at least this long after it came (s): the player waiting there
/// gets it, however late it is.
pub const RELIEF_ARRIVED_WAIT: f64 = 60.0;

/// A relief whose bus has not come this long past its wait is the AI's (s: nothing came to
/// hold for him - not put out, or stuck).
pub const RELIEF_NO_BUS: f64 = 600.0;

/// A relief his game has not asked for again in this long is let go (s, the traffic's clock).
pub const RELIEF_STALE: f64 = 20.0;

/// (31 October 2026) The AI's bus of a departure a player gave his bus back for is put out where
/// his stood while it is put out within this long of it (s): later, where its timetable has it.
pub const BACK_STOOD_FOR: f64 = 120.0;

/// A departure a LAN player waits to take over.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Relief {
    /// His id in the LAN session.
    pub peer: u32,
    /// The stop of the trip he takes over at (its index among the trip's stations), its map
    /// object, the bus's departure there and how long it holds there at most (the traffic's
    /// clock).
    pub stop: usize,
    pub stop_id: i64,
    pub at: f64,
    pub until: f64,
    /// When his game asked for it last (the traffic's clock).
    pub seen: f64,
    /// The LAN player who drives the bus up to the stop and hands it over himself (None: the
    /// AI's bus).
    pub from: Option<u32>,
    /// When the AI's bus came to the stop (the traffic's clock).
    pub arrived: Option<f64>,
}

impl Relief {
    /// Until when its bus holds at the stop: its wait, and a minute after it came at least.
    fn hold_until(&self) -> f64 {
        self.arrived.map_or(self.until, |a| self.until.max(a + RELIEF_ARRIVED_WAIT))
    }
}

/// A relief the AI drove after all: the player it was his, the trip's line, tour and departure
/// (seconds of the company's day: a night trip of the day before past 24:00) - for his company's
/// books (`take_kept_reliefs`: his duty's trip the AI's).
#[derive(Clone, Debug, PartialEq)]
pub struct Kept {
    pub peer: u32,
    pub line: String,
    pub tour: String,
    pub departure: f64,
}

/// (27 October 2026) A departure after a LAN player's duty that the AI takes over with his bus,
/// held back while his game asks (`want_back`): who, when his game asked last, until when at most
/// (the traffic's clock: its departure).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Back {
    pub peer: u32,
    pub seen: f64,
    pub until: f64,
    /// (28 October 2026, `want_stop`) His trip itself, handed over at a stop along it: when the
    /// timetable has it leave that stop (the traffic's clock) - its bus is put out there.
    pub at: Option<f64>,
    /// His duty handed over on the way (`want_stop`): the trip he drove (a departure's index) and
    /// whether he ended it at its terminus - for his company's books (`Handed`).
    pub handed: Option<(usize, bool)>,
    /// The rest of his duty then (his claim's later departures): the AI's to drive once he gave
    /// the bus back, though his company drops what a player leaves (`set_lan_tours`).
    pub rest: Vec<usize>,
}

/// (28 October 2026) A LAN player handed his bus over to the AI on the way through his duty
/// (`want_stop`, `gave_back`): who, the trip he drove (its line and tour, its departure in seconds
/// of the company's day) and whether he ended it at its terminus - else the AI drives the rest of
/// it. For his company's books (`take_handed`): the AI drives the rest of his duty, and he did not
/// leave it.
#[derive(Clone, Debug, PartialEq)]
pub struct Handed {
    pub peer: u32,
    pub line: String,
    pub tour: String,
    pub departure: f64,
    pub done: bool,
}

/// What becomes of a LAN player's bus at the end of his duty (`want_back`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BackState {
    /// The AI drives the tour on with it: its departure then (minutes of the day), and the AI's
    /// bus of it that stood at the stop already, taken off the road for it (the caller takes it
    /// off).
    Ai(i32, Option<u64>),
    /// Given back already (a question still on its way when it was).
    Given(i32),
    /// Nothing to hand back: the tour ends, another player drives it on, or the AI's bus of it is
    /// on its way.
    No,
}

/// (30 October 2026) Where the bus a LAN player waits for at his stop is now (`relief_bus`): its
/// place and heading, how late it is (s, negative early), and whether it is still on the tour's
/// trip before his (it takes his on at the terminus).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReliefBus {
    pub pos: glam::DVec3,
    pub heading: f64,
    pub delay: f64,
    pub before: bool,
}

/// What became of a relief his game asks about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReliefState {
    /// The bus is on its way (or not out yet).
    Coming,
    /// It stands at his stop, holding for him: the car's id.
    Here(u64),
    /// Another LAN player drives it up to his stop and hands it over (his id).
    FromPlayer(u32),
    /// His already: handed over (a question still on its way when it was).
    Taken,
    /// He was not there in time: the AI drives the trip. His relief is at the start of his
    /// next trip.
    Kept,
    /// No such departure today, or another player's.
    Unknown,
}

impl ScheduleSim {
    /// Today's departure of `line`/`tour` leaving at `dep` (minutes of the day, a night trip's
    /// 24:xx as 0:xx too): of a tour that runs, the one nearest `day_time` (the traffic's clock) -
    /// a night trip asked for in the evening is tonight's, not the one of the night before.
    pub fn relief_departure(&self, line: &str, tour: &str, dep: i32, day_time: f64) -> Option<usize> {
        let claim = LanClaim { line: line.trim().to_lowercase(), tour: tour.trim().to_lowercase(), window: Some((dep, dep)) };
        (0..self.departures.len())
            .filter(|&i| self.runs(i))
            .filter(|&i| {
                let d = &self.departures[i];
                claim.has(&d.line, &d.tour, d.time)
            })
            .min_by(|&a, &b| (self.dep_time(a) - day_time).abs().total_cmp(&(self.dep_time(b) - day_time).abs()))
    }

    /// (30 October 2026) Where the bus of departure `dep` (minutes of the day) of `line`/`tour` is
    /// now, that a LAN player waits for at his stop: the AI's bus on that trip, or on the tour's
    /// trip before it - None while none is on the road (it comes for its time).
    pub fn relief_bus(&self, line: &str, tour: &str, dep: i32, traffic: &TrafficSim, day_time: f64) -> Option<ReliefBus> {
        let i = self.relief_departure(line, tour, dep, day_time)?;
        let (ci, before) = match self.tour_bus(i, traffic) {
            Some(ci) => (ci, false),
            None => (self.tour_prev.get(i).copied().flatten().and_then(|k| self.tour_bus(k, traffic))?, true),
        };
        let c = &traffic.cars[ci];
        let delay = c.bus.as_ref().map_or(0.0, |b| b.delay).clamp(-86_400.0, 86_400.0);
        Some(ReliefBus { pos: c.vehicle.position, heading: c.vehicle.heading.rem_euclid(360.0), delay: if delay.is_finite() { delay } else { 0.0 }, before })
    }

    /// Whether a LAN player waits to take departure `i` over from the AI (its bus is put out no
    /// further than his stop, and may come into view there).
    pub fn relief_wanted(&self, i: usize) -> bool {
        self.relief_ai(i)
    }

    /// Whether departure `i` is the AI's while a LAN player waits for its bus at his stop
    /// (`is_player_tour`): not one another player hands over.
    pub(super) fn relief_ai(&self, i: usize) -> bool {
        self.reliefs.get(&i).is_some_and(|r| r.from.is_none())
    }

    /// The LAN player of `others` (each with his claims) who drives departure `i`'s bus up to
    /// its relief at its stop `stop`: his claim has the trip before (a relief at the trip's first
    /// stop) or the trip itself (a relief on the way) - one the AI is not to drive.
    fn relief_giver(&self, i: usize, stop: usize, others: &[(u32, Vec<LanClaim>)]) -> Option<u32> {
        let k = if stop == 0 { self.tour_prev[i]? } else { i };
        // (nor one a player gave his bus back to the AI for: the AI's - 28 October 2026)
        if self.relief_ai(k) || self.relief_kept.contains(&k) || self.backed.contains(&k) {
            return None;
        }
        let d = &self.departures[k];
        others.iter().find(|(_, claims)| claims.iter().any(|c| c.has(&d.line, &d.tour, d.time))).map(|(id, _)| *id)
    }

    /// Where a relief's bus is put out at the latest (the traffic's clock): when it leaves his
    /// stop by the timetable (`place_departure`).
    pub(super) fn relief_place_time(&self, i: usize, day_time: f64) -> f64 {
        // (28 October 2026: his trip given back at a stop along it - its bus there)
        if let Some(&at) = self.back_at.get(&i) {
            return at;
        }
        self.reliefs.get(&i).filter(|r| r.from.is_none()).map_or(day_time, |r| day_time.min(r.at))
    }

    /// A relief the AI drove after all: the AI's for the day, and for his company's books.
    fn keep_relief(&mut self, i: usize, peer: u32) {
        if self.relief_kept.insert(i) {
            let d = &self.departures[i];
            self.kept_log.push(Kept { peer, line: d.line.clone(), tour: d.tour.clone(), departure: self.company_minute(i) as f64 * 60.0 });
        }
    }

    /// LAN player `peer` waits on foot at stop `stop` (an index among its trip's stations) to
    /// take departure `dep` (minutes of the day) of `line`/`tour` over, at `day_time`: asked
    /// again every few seconds while he waits. `others`: the other players of the session, each
    /// with his claims (one of them may drive the bus up to there). What has become of it.
    #[allow(clippy::too_many_arguments)]
    pub fn want_relief(&mut self, peer: u32, line: &str, tour: &str, dep: i32, stop: usize, others: &[(u32, Vec<LanClaim>)], traffic: &TrafficSim, day_time: f64) -> ReliefState {
        let Some(i) = self.relief_departure(line, tour, dep, day_time) else {
            return ReliefState::Unknown;
        };
        if self.relief_taken.get(&i) == Some(&peer) {
            return ReliefState::Taken;
        }
        if self.relief_kept.contains(&i) {
            return ReliefState::Kept;
        }
        let stations = trip_stations(&self.data.trips[self.departures[i].trip]);
        if stations.is_empty() {
            return ReliefState::Unknown;
        }
        let stop = stop.min(stations.len() - 1);
        match self.reliefs.get_mut(&i) {
            Some(r) if r.peer != peer => return ReliefState::Unknown,
            Some(r) => {
                r.seen = day_time;
                // (the player who was to hand it over left the session: the AI brings it)
                if r.from.is_some_and(|a| !others.iter().any(|o| o.0 == a)) {
                    log::info!("timetable: LAN player {:?} who was to hand line {line} tour {tour} over has gone: the AI brings the bus", r.from);
                    r.from = None;
                    r.until = r.until.max(day_time + RELIEF_WAIT);
                    self.relief_dispatch(i, traffic, day_time);
                }
            }
            None => {
                let at = self.dep_time(i) + self.times_of(i).stations.get(stop).map_or(0.0, |s| s.1);
                let from = self.relief_giver(i, stop, others);
                let r = Relief { peer, stop, stop_id: stations[stop], at, until: at + RELIEF_WAIT, seen: day_time, from, arrived: None };
                // (gone already, nobody to hand it over: the AI drove it - his books say so)
                if day_time >= r.until && from.is_none() {
                    self.keep_relief(i, peer);
                    return ReliefState::Kept;
                }
                match from {
                    Some(a) => log::info!("timetable: LAN player {peer} waits at stop {stop} ({}) to take over line {line} tour {tour} at {}: LAN player {a} hands the bus over", r.stop_id, hhmm(r.at - self.day_base)),
                    None => log::info!("timetable: LAN player {peer} waits at stop {stop} ({}) to take over line {line} tour {tour} at {}: its bus holds there for him until {}", r.stop_id, hhmm(r.at - self.day_base), hhmm(r.until - self.day_base)),
                }
                self.reliefs.insert(i, r);
                // (a hand-back held for it is void: another player waits for its bus)
                self.backs.remove(&i);
                match from {
                    // (the players': not the AI's to run)
                    Some(_) => self.departures[i].spawned = true,
                    None => self.relief_dispatch(i, traffic, day_time),
                }
            }
        }
        match self.reliefs.get(&i).and_then(|r| r.from) {
            Some(a) => ReliefState::FromPlayer(a),
            None => self.relief_state(i, traffic),
        }
    }

    /// LAN player `giver` got out of his bus at the relief stop of departure `dep` of
    /// `line`/`tour` and gives it on: the player who waits for it there (his id), the departure
    /// his now - or why not. Given again (an answer lost on the way): the same.
    pub fn gave_relief(&mut self, giver: u32, line: &str, tour: &str, dep: i32, day_time: f64) -> Result<u32, &'static str> {
        let i = self.relief_departure(line, tour, dep, day_time).ok_or(RELIEF_NO_TRIP)?;
        if let Some(&(g, taker)) = self.relief_given.get(&i) {
            if g == giver {
                return Ok(taker);
            }
        }
        let r = self.reliefs.get(&i).filter(|r| r.from == Some(giver)).ok_or(RELIEF_NOBODY)?;
        let taker = r.peer;
        self.reliefs.remove(&i);
        self.relief_given.insert(i, (giver, taker));
        self.relief_taken.insert(i, taker);
        self.departures[i].spawned = true;
        log::info!("timetable: LAN player {giver} handed line {line} tour {tour} at {} over to LAN player {taker}", hhmm(dep as f64 * 60.0));
        Ok(taker)
    }

    /// Departure `i` a LAN player waits for: the AI's to bring the bus up to his stop - its
    /// tour's bus takes it on at the terminus, or one is put out for it now when it is due
    /// already and nothing of it is on the road (a claim had taken it from the queues).
    fn relief_dispatch(&mut self, i: usize, traffic: &TrafficSim, day_time: f64) {
        if self.car_departure.values().any(|&k| k == i) || self.pending.contains(&i) || self.awaiting.contains(&i) || self.waiting.contains(&i) {
            return;
        }
        self.departures[i].spawned = false;
        let tod = day_time - self.day_base;
        // (not due yet: its tour's bus takes it on, or a layover bus is put out for it - as for
        // any trip of the AI's)
        if self.departures[i].time > tod {
            return;
        }
        self.departures[i].spawned = true;
        if self.tour_prev[i].and_then(|k| self.tour_bus(k, traffic)).is_some() {
            self.awaiting.insert(i);
        } else {
            self.pending.push_back(i);
        }
    }

    /// Whether the bus of departure `i` stands at the relief's stop.
    fn relief_bus_here(&self, i: usize, traffic: &TrafficSim) -> Option<u64> {
        let r = self.reliefs.get(&i)?;
        let c = &traffic.cars[self.tour_bus(i, traffic)?];
        c.bus.as_ref().is_some_and(|b| b.at_stop() && b.stops.front().is_some_and(|f| f.id == r.stop_id && (f.depart - r.at).abs() < 1.0)).then_some(c.id)
    }

    /// Whether departure `i`'s bus stands at the relief's stop: its car's id.
    fn relief_state(&self, i: usize, traffic: &TrafficSim) -> ReliefState {
        if !self.reliefs.contains_key(&i) {
            return ReliefState::Unknown;
        }
        match self.relief_bus_here(i, traffic) {
            Some(id) => ReliefState::Here(id),
            None => ReliefState::Coming,
        }
    }

    /// LAN player `peer`'s game takes departure `dep`'s bus over: it must stand at his stop,
    /// holding for him. The car's id - forgotten here, the departure his now (his claim from
    /// now on); the caller takes the car off the road - or why not.
    pub fn take_relief(&mut self, peer: u32, line: &str, tour: &str, dep: i32, traffic: &TrafficSim, day_time: f64) -> Result<u64, &'static str> {
        let i = self.relief_departure(line, tour, dep, day_time).ok_or(RELIEF_NO_TRIP)?;
        if self.relief_kept.contains(&i) {
            return Err(RELIEF_KEPT);
        }
        if self.reliefs.get(&i).is_none_or(|r| r.peer != peer || r.from.is_some()) {
            return Err(RELIEF_NOT_WAITING);
        }
        let ReliefState::Here(id) = self.relief_state(i, traffic) else {
            return Err(RELIEF_NOT_HERE);
        };
        self.reliefs.remove(&i);
        self.relief_taken.insert(i, peer);
        self.car_departure.remove(&id);
        self.running.retain(|r| r.car != id);
        self.departures[i].spawned = true;
        self.awaiting.remove(&i);
        self.pending.retain(|x| *x != i);
        self.waiting.retain(|x| *x != i);
        self.retry_at.remove(&i);
        log::info!("timetable: LAN player {peer} took over line {line} tour {tour} at {}: bus {id} is his", hhmm(dep as f64 * 60.0));
        Ok(id)
    }

    /// (27 October 2026) LAN player `peer`'s duty ends with departure `last` (minutes of the day)
    /// of `line`/`tour`, his bus at its terminus: the AI drives the tour on with it - the next
    /// departure of the tour is held back for it while his game asks, and the AI's bus of it that
    /// waited at the stop already goes (its car's id: the caller takes it off) - unless the tour
    /// ends there, another player has the next trip (or waits for it), it does not run, or its bus
    /// is on the road already. Only in a world of Bus Company Online, whose companies' tours the
    /// AI drives (`set_company`): in a player's own LAN session his bus stays his.
    pub fn want_back(&mut self, peer: u32, line: &str, tour: &str, last: i32, traffic: &TrafficSim, day_time: f64) -> BackState {
        if !self.online_world {
            return BackState::No;
        }
        let Some(i) = self.relief_departure(line, tour, last, day_time) else { return BackState::No };
        let Some(k) = self.tour_next.get(i).copied().flatten() else { return BackState::No };
        self.back_hold(peer, k, false, None, None, Vec::new(), traffic, day_time)
    }

    /// (28 October 2026) LAN player `peer` hands his bus over to the AI on the way through his
    /// duty, standing with it on his trip leaving at `dep` (minutes of the day) of `line`/`tour`:
    /// at its terminus (`stop` None, or its last), the tour's next departure is the AI's with his
    /// bus - as at the end of a duty (`want_back`), or his own next trip; at a stop along it (an
    /// index among its stations), that trip itself, its bus put out at that stop once given back
    /// (at its first stop, as a layover bus). His claims (`own`) and the other players' (`others`)
    /// say whose the departure is: not another's. Only in a world of Bus Company Online.
    #[allow(clippy::too_many_arguments)]
    pub fn want_stop(&mut self, peer: u32, line: &str, tour: &str, dep: i32, stop: Option<usize>, own: &[LanClaim], others: &[(u32, Vec<LanClaim>)], traffic: &TrafficSim, day_time: f64) -> BackState {
        if !self.online_world {
            return BackState::No;
        }
        let Some(i) = self.relief_departure(line, tour, dep, day_time) else { return BackState::No };
        let last = trip_stations(&self.data.trips[self.departures[i].trip]).len().saturating_sub(1);
        // (the departure the AI drives on, the stop of it his bus stands at, whether his trip is done)
        let (k, at_stop, done) = match stop.filter(|&s| s < last) {
            None => match self.tour_next.get(i).copied().flatten() {
                Some(k) => (k, 0, true),
                None => return BackState::No,
            },
            Some(s) => (i, s, false),
        };
        let has = |d: &Departure, claims: &[LanClaim]| claims.iter().any(|c| c.has(&d.line, &d.tour, d.time));
        let (mine, theirs) = (has(&self.departures[k], own), others.iter().any(|(_, c)| has(&self.departures[k], c)));
        // (another player's, or his trip itself not his)
        if theirs || (k == i && !mine) {
            return BackState::No;
        }
        // (his own: put out where his bus stands - that stop, or its first -, however late he is)
        let at = mine.then(|| self.dep_time(k) + self.times_of(k).stations.get(at_stop).map_or(0.0, |x| x.1));
        // (the rest of his duty: his claim's departures after these)
        let after = self.departures[i].time.max(self.departures[k].time);
        let rest: Vec<usize> = (0..self.departures.len()).filter(|&j| j != i && j != k && self.departures[j].time > after && has(&self.departures[j], own)).collect();
        self.back_hold(peer, k, mine, at, Some((i, done)), rest, traffic, day_time)
    }

    /// Departure `k` held back for LAN player `peer`'s bus (`want_back`, `want_stop`; `mine`: his
    /// own claim's, `at`: put out at a stop along it, `handed`: his duty handed over on the way).
    #[allow(clippy::too_many_arguments)]
    fn back_hold(&mut self, peer: u32, k: usize, mine: bool, at: Option<f64>, handed: Option<(usize, bool)>, rest: Vec<usize>, traffic: &TrafficSim, day_time: f64) -> BackState {
        let next = (self.departures[k].time / 60.0).round() as i32;
        // (his own held while he asks, however late: until its time - or five minutes on)
        let until = if mine { self.dep_time(k).max(day_time) + RELIEF_WAIT } else { self.dep_time(k) };
        if self.backed.contains(&k) {
            return BackState::Given(next);
        }
        // (one another player drives or waits for, or one its company's control room sends a
        // spare bus to: not his bus's)
        if !self.runs(k) || self.reliefs.contains_key(&k) || self.company_swap(k) || (!mine && (self.is_player_tour(k) || self.awaiting.contains(&k))) {
            if self.backs.contains_key(&k) {
                self.release_back(k, day_time);
            }
            return BackState::No;
        }
        // (held for him already: until it leaves - he is getting out)
        match self.backs.get_mut(&k) {
            Some(b) if b.peer != peer => return BackState::No,
            Some(b) => {
                b.seen = day_time;
                if mine {
                    b.until = b.until.max(until);
                }
                return BackState::Ai(next, None);
            }
            None => {}
        }
        let mut gone = None;
        // (the AI's: not one leaving now, nor after a break its bus would not wait at the stop for
        // - its own then -, and its bus on the road only when it waits at its first stop for its
        // time - one under way comes as it is; his own is off the road, his claim's, however late)
        if !mine {
            let wait = self.dep_time(k) - day_time;
            if !(30.0..=TOUR_LAYOVER_MAX).contains(&wait) {
                return BackState::No;
            }
            let car = self.tour_bus(k, traffic).map(|ci| &traffic.cars[ci]);
            if car.is_some_and(|c| day_time >= self.dep_time(k) || !c.bus.as_ref().is_some_and(|b| b.at_stop())) {
                return BackState::No;
            }
            gone = car.map(|c| c.id);
            if let Some(id) = gone {
                self.car_departure.remove(&id);
                self.running.retain(|r| r.car != id);
            }
            self.departures[k].spawned = true;
            self.pending.retain(|x| *x != k);
            self.waiting.retain(|x| *x != k);
            self.retry_at.remove(&k);
        }
        self.backs.insert(k, Back { peer, seen: day_time, until, at, handed, rest });
        let d = &self.departures[k];
        match handed {
            Some(_) => log::info!("timetable: LAN player {peer} hands his bus over on the way: the AI drives {} tour {} at {} on with it{}", d.line, d.tour, hhmm(d.time), if at.is_some() { " from his stop" } else { "" }),
            None => log::info!("timetable: LAN player {peer}'s duty ends: the AI drives {} tour {} at {} on with his bus", d.line, d.tour, hhmm(d.time)),
        }
        BackState::Ai(next, gone)
    }

    /// (27 October 2026) LAN player `peer` gave his bus back for departure `next` (minutes of the
    /// day; `want_back`): out of his game, the AI's bus of it is put out at its first stop, where his
    /// stood - or why not. Given again (an answer lost on the way): the same. (31 October 2026)
    /// `stood`: where his bus stands (its place and heading, as the session has it) - the AI's is put
    /// out just there and drives on from there (`back_stood`), rather than at the stop's own place.
    #[allow(clippy::too_many_arguments)]
    pub fn gave_back(&mut self, peer: u32, line: &str, tour: &str, next: i32, stood: Option<(glam::DVec3, f64)>, traffic: &TrafficSim, day_time: f64) -> Result<(), &'static str> {
        let k = self.relief_departure(line, tour, next, day_time).ok_or(RELIEF_NO_TRIP)?;
        if self.backed.contains(&k) {
            return Ok(());
        }
        self.backs.get(&k).filter(|b| b.peer == peer).ok_or(RELIEF_NOBODY)?;
        let Some(b) = self.backs.remove(&k) else { return Err(RELIEF_NOBODY) };
        self.backed.insert(k);
        if let Some((at, heading)) = stood.filter(|(at, h)| at.is_finite() && h.is_finite()) {
            self.back_stood.insert(k, (at, heading, day_time));
        }
        // (his trip itself, at a stop along it: its bus put out there; his duty handed over on the
        // way: his company told)
        if let Some(at) = b.at {
            self.back_at.insert(k, at);
        }
        if let Some((i, done)) = b.handed {
            let d = &self.departures[i];
            self.handed_log.push(Handed { peer, line: d.line.clone(), tour: d.tour.clone(), departure: self.company_minute(i) as f64 * 60.0, done });
            self.handed_runs.extend(b.rest);
        }
        // (a player waiting for his bus further on, to take it over from him: the AI brings it)
        let waiting: Vec<usize> = self.reliefs.iter().filter(|(_, r)| r.from == Some(peer)).map(|(j, _)| *j).collect();
        for j in waiting {
            if let Some(r) = self.reliefs.get_mut(&j) {
                r.from = None;
                r.until = r.until.max(day_time + RELIEF_WAIT);
            }
            log::info!("timetable: LAN player {peer} gave his bus to the AI: it brings the bus of {} tour {} at {} to the player who waits for it", self.departures[j].line, self.departures[j].tour, hhmm(self.departures[j].time));
            self.relief_dispatch(j, traffic, day_time);
        }
        // (put out now, waiting at its first stop for its time - as a layover bus; not when it has
        // a bus of its own meanwhile, or another player waits for it)
        let has_bus = self.car_departure.values().any(|&x| x == k) || self.pending.contains(&k) || self.awaiting.contains(&k);
        self.departures[k].spawned = true;
        if !has_bus && !self.reliefs.contains_key(&k) {
            self.pending.push_back(k);
        }
        log::info!("timetable: LAN player {peer} gave his bus back: the AI drives {line} tour {tour} at {} on", hhmm(next as f64 * 60.0));
        Ok(())
    }

    /// The departures held back for a bus given back that is not coming: his game no longer asks,
    /// or past its departure - the AI's own bus comes for them, as for any other trip of its
    /// (not one his claim has now: he drove on).
    fn backs_tick(&mut self, day_time: f64) {
        let over: Vec<usize> = self.backs.iter().filter(|(_, b)| day_time >= b.until || day_time - b.seen > RELIEF_STALE || day_time < b.seen - 60.0).map(|(k, _)| *k).collect();
        for k in over {
            self.release_back(k, day_time);
        }
    }

    /// Departure `k` no longer held back for a LAN player's bus: the AI's own bus comes for it - put
    /// out at once when it is due (the queue takes only a departure of the last moments) - unless his
    /// claim has it now (he drove on with it), another player waits for it, or it has a bus.
    fn release_back(&mut self, k: usize, day_time: f64) {
        self.backs.remove(&k);
        if self.is_player_tour(k) || self.reliefs.contains_key(&k) || self.car_departure.values().any(|&x| x == k) || self.pending.contains(&k) {
            return;
        }
        self.departures[k].spawned = false;
        if self.departures[k].time <= day_time - self.day_base {
            self.departures[k].spawned = true;
            self.pending.push_back(k);
        }
        log::info!("timetable: {} tour {} at {}: no bus given back, the AI's own comes", self.departures[k].line, self.departures[k].tour, hhmm(self.departures[k].time));
    }

    /// (27 October 2026) A LAN player gave his bus back for departure `i` (`gave_back`): its AI bus
    /// comes into view at its first stop, where his stood and he stands.
    pub fn back_given(&self, i: usize) -> bool {
        self.backed.contains(&i)
    }

    /// (31 October 2026) Where the bus a LAN player gave back for departure `i` stood (its place and
    /// heading): the AI's bus of it is put out just there, in its place (`schedule::spawn`,
    /// `stood_on`) - while it is put out within `BACK_STOOD_FOR` of it.
    pub fn back_stood(&self, i: usize, day_time: f64) -> Option<(glam::DVec3, f64)> {
        self.back_stood.get(&i).filter(|s| (0.0..BACK_STOOD_FOR).contains(&(day_time - s.2))).map(|s| (s.0, s.1))
    }

    /// (28 October 2026) The duties LAN players handed over to the AI on the way since asked last
    /// (a world of Bus Company Online writes them for the player's company:
    /// `company_server::handed_over`).
    pub fn take_handed(&mut self) -> Vec<Handed> {
        std::mem::take(&mut self.handed_log)
    }

    /// The reliefs the AI drove after all since asked last (a world of Bus Company Online writes
    /// them for the player's company: `company_server::relief_kept`).
    pub fn take_kept_reliefs(&mut self) -> Vec<Kept> {
        std::mem::take(&mut self.kept_log)
    }

    /// Once a tick (`queue_due`): the buses of the reliefs hold at their stops; a relief not
    /// asked for again is let go (the trip the AI's, or his claim's), one past its wait is the
    /// AI's (`relief_kept`).
    pub(super) fn relief_tick(&mut self, traffic: &mut TrafficSim, day_time: f64) {
        if !self.backs.is_empty() {
            self.backs_tick(day_time);
        }
        if self.reliefs.is_empty() {
            return;
        }
        // (a player who was to hand his bus over, not there in time: the AI brings one - to the
        // stop, where the relief waits)
        let late: Vec<usize> = self.reliefs.iter().filter(|(_, r)| r.from.is_some() && day_time >= r.until).map(|(i, _)| *i).collect();
        for i in late {
            if let Some(r) = self.reliefs.get_mut(&i) {
                log::info!("timetable: LAN player {:?} did not hand {} tour {} over in time: the AI brings a bus to LAN player {}", r.from, self.departures[i].line, self.departures[i].tour, r.peer);
                r.from = None;
                r.until = day_time + RELIEF_WAIT;
            }
            self.relief_dispatch(i, traffic, day_time);
        }
        // (the AI's bus at the stop: when it came)
        let came: Vec<usize> = self.reliefs.iter().filter(|(_, r)| r.from.is_none() && r.arrived.is_none()).map(|(i, _)| *i).filter(|&i| self.relief_bus_here(i, traffic).is_some()).collect();
        for i in came {
            if let Some(r) = self.reliefs.get_mut(&i) {
                r.arrived = Some(day_time);
            }
        }
        let mut gone: Vec<(usize, bool)> = Vec::new();
        for (&i, r) in &self.reliefs {
            // (past its wait: once its bus came and held its time, or when none came at all)
            let over = day_time >= r.hold_until() && (r.arrived.is_some() || day_time >= r.until + RELIEF_NO_BUS);
            if r.from.is_none() && over {
                gone.push((i, true));
            } else if day_time - r.seen > RELIEF_STALE || day_time < r.seen - 60.0 {
                gone.push((i, false));
            }
        }
        for (i, kept) in gone {
            let Some(r) = self.reliefs.remove(&i) else { continue };
            if let Some(b) = self.tour_bus(i, traffic).and_then(|ci| traffic.cars[ci].bus.as_mut()) {
                b.relief = None;
                if !kept && b.at_stop() && b.stops.front().is_some_and(|f| f.id == r.stop_id) {
                    b.leave_at = b.leave_at.min(r.at.max(day_time));
                }
            }
            if kept {
                self.keep_relief(i, r.peer);
                log::info!("timetable: LAN player {} did not take over {} tour {} at {}: the AI drives it", r.peer, self.departures[i].line, self.departures[i].tour, hhmm(r.at - self.day_base));
            } else {
                // (still his claim's: his - the AI's bus goes, and nobody puts another out; not:
                // the AI drives on)
                if self.is_player_tour(i) {
                    self.purge_player_tour = true;
                    self.departures[i].spawned = true;
                }
                log::info!("timetable: LAN player {} no longer waits for {} tour {} at {}", r.peer, self.departures[i].line, self.departures[i].tour, hhmm(r.at - self.day_base));
            }
        }
        for (&i, r) in self.reliefs.iter().filter(|(_, r)| r.from.is_none()) {
            let Some(ci) = self.tour_bus(i, traffic) else { continue };
            let Some(b) = traffic.cars[ci].bus.as_mut() else { continue };
            let hold = ReliefHold { stop: r.stop_id, depart: r.at, until: r.hold_until() };
            b.relief = Some(hold);
            // (at the stop already, before the hold was there: it holds now)
            if b.at_stop() && b.stops.front().is_some_and(|f| hold.is(f)) && b.phase != crate::ai_traffic::bus_service::Phase::Closing {
                b.leave_at = b.leave_at.max(hold.until);
            }
        }
    }
}

/// Why a relief's bus is not handed over (`take_relief`; English, the keys of the translations).
pub const RELIEF_NO_TRIP: &str = "No such trip today.";
pub const RELIEF_KEPT: &str = "You were not there in time: the AI drives this trip.";
pub const RELIEF_NOT_WAITING: &str = "You are not waiting for this bus.";
pub const RELIEF_NOT_HERE: &str = "The bus is not at your stop yet.";
pub const RELIEF_NOBODY: &str = "Nobody waits to take this bus over.";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_traffic::bus_service::{AiTimetable, Phase, Stop};
    use crate::traffic::{LaneBuilder, LaneKind, Network};

    struct World {
        path: PathBuf,
        ai: omsi_map::AiLists,
        positions: crate::world_data::ObjectPlaces,
    }
    impl TimetableWorld for World {
        fn root(&self) -> &Path {
            &self.path
        }
        fn map_dir(&self) -> &Path {
            &self.path
        }
        fn chrono_dirs(&self) -> Vec<PathBuf> {
            vec![]
        }
        fn ailists(&self) -> &omsi_map::AiLists {
            &self.ai
        }
        fn raw_tiles(&self) -> Vec<(i32, i32)> {
            vec![]
        }
        fn has_tile(&self, _: (i32, i32)) -> bool {
            false
        }
        fn date(&self) -> i32 {
            20221010
        }
        fn object_positions(&self) -> &crate::world_data::ObjectPlaces {
            &self.positions
        }
        fn stop_side(&self, _: i64) -> f32 {
            0.0
        }
    }

    /// A road with a bus of departure `dep` (s of the day) standing at its first stop (object 1)
    /// with its doors open.
    fn bus_at_stop(dir: &Path, dep: f64) -> (TrafficSim, u64) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("car.bus"), "[model]\nmodel.cfg\n").unwrap();
        std::fs::write(dir.join("model.cfg"), "").unwrap();
        let ty = Arc::new(VehicleType::load_ai(dir, &dir.join("car.bus")).unwrap());
        let mut net = Network { lanes: vec![LaneBuilder::arc(glam::DVec3::ZERO, 0.0, 1500.0, 0.0, 0.0, LaneKind::Street, 3.0)], ..Default::default() };
        net.link(1.5);
        net.build_grid();
        net.compute_reach();
        let random = crate::ai_traffic::setup::RandomTypes { types: Vec::new(), groups: Vec::new(), group_curves: false, group_uvg: Vec::new(), uvg_defaults: Vec::new() };
        let mut t = TrafficSim::assemble(dir, net, random, Vec::new(), HashMap::new(), (Vec::new(), Vec::new()), Vec::new(), (1.0, 0), 0);
        let stops = vec![Stop { ri: 0, s: 300.0, bay: 0.0, depart: dep, id: 1, side: 0.0 }, Stop { ri: 0, s: 1200.0, bay: 0.0, depart: dep + 600.0, id: 2, side: 0.0 }];
        let setup = crate::ai_traffic::model::BusSetup { route: vec![0], stops, number: None, hof: None, timetable: AiTimetable { line: String::new(), terminus: String::new(), stops: Vec::new() } };
        let vehicle = crate::VehicleInstance::new(ty.clone(), crate::VehicleHost::new(crate::SimClock::default()));
        let id = t.place_car(vehicle, LaneKind::Street, 0, 300.0, ty, 0x101, None, Some(5), Some(0.0), Some(setup));
        (t, id)
    }

    /// (NexiconDK, Bus Company Online: relief at the stop) A LAN player waiting on foot to take
    /// a tour over: while he waits its departure is the AI's - its bus takes the trip on at the
    /// terminus, though his claim has it -, the bus holds at his stop until he takes it (it is his
    /// then, the departure his claim's) or five minutes past its time (the AI drives it, and the
    /// next trip is the one to wait for); a relief not asked for again is let go.
    #[test]
    fn a_lan_player_takes_the_ais_bus_over_at_his_stop() {
        let dir = std::env::temp_dir().join(format!("omsi_relief_{}", std::process::id()));
        let w = World { path: dir.clone(), ai: Default::default(), positions: Default::default() };
        let mut s = ScheduleSim::new(&w.path, &w, &crate::SimClock::default());
        s.data.trips.push(omsi_timetable::Trip { name: "5a".into(), line: "Linie5".into(), stations: vec![1, 2], ..Default::default() });
        s.times = vec![vec![TripTimes { stations: vec![(0.0, 0.0), (600.0, 600.0)], stops: vec![true, true], duration: 600.0, ..Default::default() }]];
        let d = |h: f64| Departure { time: h * 3600.0, trip: 0, profile: 0, line: "Linie5".into(), ai_group: "depot".into(), tour: "3".into(), mask: -1, spawned: false, yesterday: false };
        s.departures = vec![d(8.0), d(9.0), d(10.0), d(11.0)];
        s.tour_prev = vec![None, Some(0), Some(1), Some(2)];
        s.tour_next = vec![Some(1), Some(2), Some(3), None];
        s.company_off = vec![false; 4];
        // his claim: the 9:00 and the 10:00 (his game sends it as soon as it has the duty)
        s.last_tod = 8.5 * 3600.0;
        s.set_lan_tours(lan_claims("linie5/3@540~600").into_iter().collect());
        assert!(s.is_player_tour(1) && s.departures[1].spawned);
        let (mut traffic, car) = bus_at_stop(&dir, 9.0 * 3600.0);
        let at = |h: f64| h * 3600.0;
        // he waits at the 9:00's first stop: the AI's to bring the bus there
        assert_eq!(s.want_relief(7, "Linie5", "3", 540, 0, &[], &traffic, at(8.8)), ReliefState::Coming);
        assert!(!s.is_player_tour(1) && !s.departures[1].spawned && s.may_take_on(1, at(8.95)), "the tour's bus takes it on at the terminus");
        assert!(s.is_player_tour(2), "his 10:00 stays his");
        assert_eq!(s.want_relief(8, "Linie5", "3", 540, 0, &[], &traffic, at(8.8)), ReliefState::Unknown, "another player's");
        assert_eq!(s.take_relief(7, "Linie5", "3", 540, &traffic, at(8.8)), Err(RELIEF_NOT_HERE));
        // (30 October 2026: where his bus is - none on the road yet, then on the tour's trip
        // before his, then on his)
        assert_eq!(s.relief_bus("Linie5", "3", 540, &traffic, at(8.8)), None);
        s.car_departure.insert(car, 0);
        let b = s.relief_bus("Linie5", "3", 540, &traffic, at(8.8)).unwrap();
        assert!(b.before && b.pos == traffic.cars[0].vehicle.position, "{b:?}");
        s.car_departure.insert(car, 1);
        assert!(!s.relief_bus("Linie5", "3", 540, &traffic, at(8.8)).unwrap().before);
        // the bus is there, holding for him
        s.car_departure.insert(car, 1);
        traffic.cars[0].bus.as_mut().unwrap().phase = Phase::Boarding;
        traffic.cars[0].bus.as_mut().unwrap().leave_at = at(9.0);
        assert_eq!(s.want_relief(7, "Linie5", "3", 540, 0, &[], &traffic, at(8.99)), ReliefState::Here(car));
        s.relief_tick(&mut traffic, at(8.99));
        let b = traffic.cars[0].bus.as_ref().unwrap();
        assert_eq!((b.relief.map(|r| r.until), b.leave_at), (Some(at(9.0) + RELIEF_WAIT), at(9.0) + RELIEF_WAIT));
        assert_eq!(s.take_relief(7, "Linie5", "3", 540, &traffic, at(8.99)), Ok(car));
        assert!(s.is_player_tour(1) && s.departures[1].spawned && !s.car_departure.contains_key(&car), "his now: the bus leaves the world");
        // (a question still on its way when it was handed over: his, nothing made of it)
        assert_eq!(s.want_relief(7, "Linie5", "3", 540, 0, &[], &traffic, at(8.995)), ReliefState::Taken);
        assert!(s.reliefs.is_empty() && s.departures[1].spawned);
        // not there in time for the 10:00: the AI drives it, and the 11:00 is the one to wait for
        assert_eq!(s.want_relief(7, "Linie5", "3", 600, 0, &[], &traffic, at(9.9)), ReliefState::Coming);
        // (no bus came at all: once its wait and ten minutes are over)
        assert_eq!(s.want_relief(7, "Linie5", "3", 600, 0, &[], &traffic, at(10.0) + RELIEF_WAIT + 1.0), ReliefState::Coming);
        s.relief_tick(&mut traffic, at(10.0) + RELIEF_WAIT + 2.0);
        assert!(!s.relief_kept.contains(&2), "not before its bus came or ten minutes are over");
        s.relief_tick(&mut traffic, at(10.0) + RELIEF_WAIT + RELIEF_NO_BUS);
        assert!(s.relief_kept.contains(&2) && !s.is_player_tour(2));
        assert_eq!(s.take_kept_reliefs(), vec![Kept { peer: 7, line: "Linie5".into(), tour: "3".into(), departure: at(10.0) }]);
        assert!(s.take_kept_reliefs().is_empty());
        assert_eq!(s.want_relief(7, "Linie5", "3", 600, 0, &[], &traffic, at(10.1)), ReliefState::Kept);
        assert_eq!(s.take_relief(7, "Linie5", "3", 600, &traffic, at(10.1)), Err(RELIEF_KEPT));
        // (asked for after its wait, nobody to hand it over: the AI drove it, his books told)
        assert_eq!(s.want_relief(7, "Linie5", "3", 480, 0, &[], &traffic, at(8.5)), ReliefState::Kept);
        assert_eq!(s.take_kept_reliefs(), vec![Kept { peer: 7, line: "Linie5".into(), tour: "3".into(), departure: at(8.0) }]);
        // asked for once only: let go
        s.set_lan_tours(lan_claims("linie5/3@660~660").into_iter().collect());
        assert_eq!(s.want_relief(7, "Linie5", "3", 660, 0, &[], &traffic, at(10.5)), ReliefState::Coming);
        s.relief_tick(&mut traffic, at(10.5) + RELIEF_STALE + 1.0);
        assert!(s.reliefs.is_empty() && s.is_player_tour(3) && s.purge_player_tour);
        // another player drives the bus up to his stop: his to hand over, not the AI's - the 11:00
        // is the relief, Eva (8) drives the 10:00 before it
        let mut s2 = ScheduleSim::new(&w.path, &w, &crate::SimClock::default());
        s2.data = s.data.clone();
        s2.times = s.times.clone();
        s2.departures = vec![d(8.0), d(9.0), d(10.0), d(11.0)];
        s2.tour_prev = vec![None, Some(0), Some(1), Some(2)];
        s2.tour_next = vec![Some(1), Some(2), Some(3), None];
        s2.company_off = vec![false; 4];
        s2.last_tod = 9.5 * 3600.0;
        s2.set_lan_tours(lan_claims("linie5/3@600~600;linie5/3@660~660").into_iter().collect());
        let eva = vec![(8, lan_claims("linie5/3@600~600"))];
        assert_eq!(s2.want_relief(7, "Linie5", "3", 660, 0, &eva, &traffic, at(10.5)), ReliefState::FromPlayer(8));
        assert!(s2.is_player_tour(3) && s2.departures[3].spawned && !s2.may_take_on(3, at(10.9)), "not the AI's to run");
        assert_eq!(s2.gave_relief(9, "Linie5", "3", 660, at(10.9)), Err(RELIEF_NOBODY));
        assert_eq!(s2.gave_relief(8, "Linie5", "3", 660, at(10.9)), Ok(7));
        assert!(s2.reliefs.is_empty() && s2.is_player_tour(3));
        assert_eq!(s2.gave_relief(8, "Linie5", "3", 660, at(10.91)), Ok(7), "given again (its answer lost): the same");
        assert_eq!(s2.want_relief(7, "Linie5", "3", 660, 0, &eva, &traffic, at(10.91)), ReliefState::Taken);
        s2.relief_given.clear();
        s2.relief_taken.clear();
        // (Eva late: five minutes past, the AI brings a bus to the stop; gone from the session: at once)
        assert_eq!(s2.want_relief(7, "Linie5", "3", 660, 0, &eva, &traffic, at(11.0) + RELIEF_WAIT - 1.0), ReliefState::FromPlayer(8));
        s2.relief_tick(&mut traffic, at(11.0) + RELIEF_WAIT);
        assert!(s2.relief_ai(3) && !s2.is_player_tour(3) && s2.pending.contains(&3), "the AI's bus is put out for him");
        s2.reliefs.clear();
        s2.pending.clear();
        s2.departures[3].spawned = true;
        assert_eq!(s2.want_relief(7, "Linie5", "3", 660, 0, &eva, &traffic, at(10.8)), ReliefState::FromPlayer(8));
        assert_eq!(s2.want_relief(7, "Linie5", "3", 660, 0, &[], &traffic, at(10.81)), ReliefState::Coming);
        assert!(s2.relief_ai(3));
        // a bus that comes late holds a minute after it came, however late: the 9:00 again, its
        // bus not there at 9:04 nor at its wait's end, coming at 9:07
        s.relief_kept.clear();
        s.relief_taken.clear();
        s.car_departure.insert(car, 1);
        traffic.cars[0].bus.as_mut().unwrap().phase = Phase::Running;
        assert_eq!(s.want_relief(7, "Linie5", "3", 540, 0, &[], &traffic, at(9.0) + 240.0), ReliefState::Coming);
        s.relief_tick(&mut traffic, at(9.0) + 240.0);
        assert_eq!(s.want_relief(7, "Linie5", "3", 540, 0, &[], &traffic, at(9.0) + 400.0), ReliefState::Coming);
        s.relief_tick(&mut traffic, at(9.0) + 400.0);
        assert!(!s.relief_kept.contains(&1), "its bus not there yet");
        traffic.cars[0].bus.as_mut().unwrap().phase = Phase::Boarding;
        assert_eq!(s.want_relief(7, "Linie5", "3", 540, 0, &[], &traffic, at(9.0) + 420.0), ReliefState::Here(car));
        s.relief_tick(&mut traffic, at(9.0) + 420.0);
        let b = traffic.cars[0].bus.as_ref().unwrap();
        assert_eq!(b.relief.map(|r| r.until), Some(at(9.0) + 420.0 + RELIEF_ARRIVED_WAIT));
        assert!(b.leave_at >= at(9.0) + 420.0 + RELIEF_ARRIVED_WAIT);
        assert_eq!(s.want_relief(7, "Linie5", "3", 540, 0, &[], &traffic, at(9.0) + 470.0), ReliefState::Here(car));
        s.relief_tick(&mut traffic, at(9.0) + 470.0);
        assert!(!s.relief_kept.contains(&1));
        s.relief_tick(&mut traffic, at(9.0) + 481.0);
        assert!(s.relief_kept.contains(&1), "then the AI drives it");
        // (the departure by its minute, the line and tour in any case)
        assert!(s.relief_departure("LINIE5", " 3", 660, at(10.0)) == Some(3) && s.relief_departure("linie5", "3", 661, at(10.0)).is_none() && s.relief_departure("linie7", "3", 660, at(10.0)).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A world of Bus Company Online, its companies' tours the AI's as ever.
    struct Online;
    impl TimetableCompany for Online {
        fn active(&self) -> bool {
            true
        }
        fn own_only(&self) -> bool {
            false
        }
        fn set_day(&mut self, _: i32) {}
        fn keeps_off(&self, _: &str, _: &str, _: bool) -> bool {
            false
        }
        fn runs_tour(&self, _: &str, _: &str) -> bool {
            true
        }
        fn report(&mut self, _: usize, _: &str, _: &str, _: i32, _: f64, _: usize, _: f64) {}
        fn bus(&mut self, _: &Path, _: &str, _: &str) -> Option<CompanyBus> {
            None
        }
    }

    /// (27 October 2026) At the end of a LAN player's duty the AI takes his bus over to drive the
    /// tour on: its next departure held back while his game asks - the AI's bus of it waiting at
    /// the stop makes way -, put out once he gave it back; not given back (his game no longer asks,
    /// or past its time), the AI's own bus comes - not for a trip he drove on with; nothing to hand
    /// back at the tour's end, nor to another player's game.
    #[test]
    fn at_the_end_of_his_duty_the_ai_takes_his_bus_over() {
        let dir = std::env::temp_dir().join(format!("omsi_relief_back_{}", std::process::id()));
        let w = World { path: dir.clone(), ai: Default::default(), positions: Default::default() };
        let mut s = ScheduleSim::new(&w.path, &w, &crate::SimClock::default());
        s.data.trips.push(omsi_timetable::Trip { name: "5a".into(), line: "Linie5".into(), stations: vec![1, 2], ..Default::default() });
        s.times = vec![vec![TripTimes { stations: vec![(0.0, 0.0), (600.0, 600.0)], stops: vec![true, true], duration: 600.0, ..Default::default() }]];
        let d = |h: f64| Departure { time: h * 3600.0, trip: 0, profile: 0, line: "Linie5".into(), ai_group: "depot".into(), tour: "3".into(), mask: -1, spawned: false, yesterday: false };
        s.departures = vec![d(8.0), d(9.0), d(10.0), d(11.0)];
        s.tour_prev = vec![None, Some(0), Some(1), Some(2)];
        s.tour_next = vec![Some(1), Some(2), Some(3), None];
        s.company_off = vec![false; 4];
        s.last_tod = 9.5 * 3600.0;
        let at = |h: f64| h * 3600.0;
        // (a player's own LAN session: his bus stays his)
        let (mut traffic, car) = bus_at_stop(&dir, at(10.0));
        assert_eq!(s.want_back(7, "Linie5", "3", 540, &traffic, at(9.6)), BackState::No);
        s.set_company(Box::new(Online));
        assert_eq!(s.want_back(7, "Linie5", "3", 540, &traffic, at(9.6)), BackState::No, "a company of his own: not the server's");
        s.set_online_world();
        // his duty ended with the 9:00; the 10:00's bus waits at the stop already: it makes way
        traffic.cars[0].bus.as_mut().unwrap().phase = Phase::Boarding;
        s.car_departure.insert(car, 2);
        s.departures[2].spawned = true;
        assert_eq!(s.want_back(7, "Linie5", "3", 540, &traffic, at(9.6)), BackState::Ai(600, Some(car)));
        assert!(s.departures[2].spawned && !s.car_departure.contains_key(&car) && !s.pending.contains(&2), "held back for his bus");
        assert_eq!(s.want_back(7, "Linie5", "3", 540, &traffic, at(9.62)), BackState::Ai(600, None), "asked again: held");
        assert_eq!(s.want_back(8, "Linie5", "3", 540, &traffic, at(9.62)), BackState::No, "another's");
        // given back: the 10:00 put out at once, where his bus stood
        assert_eq!(s.gave_back(8, "Linie5", "3", 600, None, &traffic, at(9.63)), Err(RELIEF_NOBODY));
        let stood = (glam::DVec3::new(0.5, 296.0, 0.0), 0.0);
        assert_eq!(s.gave_back(7, "Linie5", "3", 600, Some(stood), &traffic, at(9.63)), Ok(()));
        assert!(s.pending.contains(&2) && s.departures[2].spawned && s.backs.is_empty());
        assert_eq!(s.gave_back(7, "Linie5", "3", 600, None, &traffic, at(9.64)), Ok(()), "given again (its answer lost): the same");
        // (31 October 2026: just where his bus stands - put out within two minutes of it)
        assert_eq!(s.back_stood(2, at(9.64)), Some(stood));
        assert_eq!(s.back_stood(2, at(9.63) + BACK_STOOD_FOR + 1.0), None, "later: where its timetable has it");
        assert_eq!(s.want_back(7, "Linie5", "3", 540, &traffic, at(9.64)), BackState::Given(600));
        // the tour's last trip: nothing to hand back
        assert_eq!(s.want_back(7, "Linie5", "3", 660, &traffic, at(11.2)), BackState::No);
        // the 10:00 his last: not given back, his game no longer asking - the AI's own bus comes
        traffic.cars.clear();
        assert_eq!(s.want_back(7, "Linie5", "3", 600, &traffic, at(10.6)), BackState::Ai(660, None));
        assert!(s.departures[3].spawned);
        s.relief_tick(&mut traffic, at(10.6) + RELIEF_STALE + 1.0);
        assert!(s.backs.is_empty() && !s.departures[3].spawned);
        // (nor by its time, though his game asks: due then, put out at once)
        assert_eq!(s.want_back(7, "Linie5", "3", 600, &traffic, at(10.9)), BackState::Ai(660, None));
        assert_eq!(s.want_back(7, "Linie5", "3", 600, &traffic, at(10.995)), BackState::Ai(660, None), "held for him until it leaves, though that is soon: he is getting out");
        s.backs.get_mut(&3).unwrap().seen = at(11.0);
        s.relief_tick(&mut traffic, at(11.0));
        assert!(s.backs.is_empty() && s.departures[3].spawned && s.pending.contains(&3));
        assert_eq!(s.want_back(7, "Linie5", "3", 600, &traffic, at(10.995)), BackState::No, "too late to hold");
        assert_eq!(s.want_back(7, "Linie5", "3", 540, &traffic, at(9.2)), BackState::Given(600));
        s.pending.clear();
        s.departures[3].spawned = false;
        assert_eq!(s.want_back(7, "Linie5", "3", 600, &traffic, at(10.4)), BackState::No, "a break its bus would not wait for");
        // he drove on with it: his, not the AI's bus
        assert_eq!(s.want_back(7, "Linie5", "3", 600, &traffic, at(10.7)), BackState::Ai(660, None));
        s.set_lan_tours(lan_claims("linie5/3@660~660").into_iter().collect());
        assert_eq!(s.want_back(7, "Linie5", "3", 600, &traffic, at(10.71)), BackState::No);
        s.relief_tick(&mut traffic, at(10.71) + RELIEF_STALE + 1.0);
        assert!(s.backs.is_empty() && s.departures[3].spawned && s.is_player_tour(3));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A world of Bus Company Online whose companies drop what a LAN player leaves.
    struct Dropping;
    impl TimetableCompany for Dropping {
        fn active(&self) -> bool {
            true
        }
        fn own_only(&self) -> bool {
            false
        }
        fn set_day(&mut self, _: i32) {}
        fn keeps_off(&self, _: &str, _: &str, _: bool) -> bool {
            false
        }
        fn runs_tour(&self, _: &str, _: &str) -> bool {
            true
        }
        fn report(&mut self, _: usize, _: &str, _: &str, _: i32, _: f64, _: usize, _: f64) {}
        fn bus(&mut self, _: &Path, _: &str, _: &str) -> Option<CompanyBus> {
            None
        }
        fn drops_left(&self, _: &str) -> bool {
            true
        }
    }

    /// (28 October 2026) A LAN player hands his bus over to the AI on the way through his duty: at
    /// a stop along his trip the trip itself is the AI's with it - put out at that stop once given
    /// back -, at its terminus his next trip (his own: however long the break, and put out at its
    /// first stop however late); not one of another player's, nor his trip when it is not his.
    /// Given back, his company is told, the departure is the AI's though his claim still says so,
    /// and when his claims go the rest of his duty runs again - though his company drops what a
    /// player leaves -, not the one given back, twice. A player who waited for his bus further on
    /// gets the AI's.
    #[test]
    fn on_the_way_he_hands_his_bus_over_to_the_ai() {
        let dir = std::env::temp_dir().join(format!("omsi_relief_stop_{}", std::process::id()));
        let w = World { path: dir.clone(), ai: Default::default(), positions: Default::default() };
        let mut s = ScheduleSim::new(&w.path, &w, &crate::SimClock::default());
        s.data.trips.push(omsi_timetable::Trip { name: "5a".into(), line: "Linie5".into(), stations: vec![1, 2, 3], ..Default::default() });
        s.times = vec![vec![TripTimes { stations: vec![(0.0, 0.0), (300.0, 330.0), (600.0, 600.0)], stops: vec![true, true, true], duration: 600.0, ..Default::default() }]];
        let d = |h: f64| Departure { time: h * 3600.0, trip: 0, profile: 0, line: "Linie5".into(), ai_group: "depot".into(), tour: "3".into(), mask: -1, spawned: false, yesterday: false };
        s.departures = vec![d(8.0), d(9.0), d(10.0), d(11.0)];
        s.tour_prev = vec![None, Some(0), Some(1), Some(2)];
        s.tour_next = vec![Some(1), Some(2), Some(3), None];
        s.company_off = vec![false; 4];
        s.last_tod = 9.05 * 3600.0;
        let at = |h: f64| h * 3600.0;
        let (traffic, _) = bus_at_stop(&dir, at(12.0));
        let own = lan_claims("linie5/3@540~600");
        s.set_lan_tours(own.iter().cloned().collect());
        assert_eq!(s.want_stop(7, "Linie5", "3", 540, Some(1), &own, &[], &traffic, at(9.1)), BackState::No, "not in a player's own session");
        s.set_company(Box::new(Dropping));
        s.set_online_world();
        // at the middle stop of his 9:00: the trip itself, his claim's
        assert_eq!(s.want_stop(8, "Linie5", "3", 540, Some(1), &[], &[(7, own.clone())], &traffic, at(9.1)), BackState::No, "another's");
        assert_eq!(s.want_stop(7, "Linie5", "3", 540, Some(1), &[], &[], &traffic, at(9.1)), BackState::No, "his trip not his");
        assert_eq!(s.want_stop(7, "Linie5", "3", 540, Some(1), &own, &[], &traffic, at(9.1)), BackState::Ai(540, None));
        assert_eq!(s.backs[&1].rest, vec![2], "the rest of his duty");
        assert_eq!(s.want_stop(7, "Linie5", "3", 540, Some(1), &own, &[], &traffic, at(9.11)), BackState::Ai(540, None), "asked again: held");
        assert!(s.take_handed().is_empty(), "not handed over yet");
        assert_eq!(s.gave_back(7, "Linie5", "3", 540, None, &traffic, at(9.12)), Ok(()));
        assert!(s.pending.contains(&1) && s.departures[1].spawned && !s.is_player_tour(1) && s.back_given(1), "the AI's, though his claim says his");
        assert_eq!(s.relief_place_time(1, at(9.12)), at(9.0) + 330.0, "put out at his stop");
        s.started(1);
        assert_eq!(s.relief_place_time(1, at(9.13)), at(9.13), "once");
        assert_eq!(s.take_handed(), vec![Handed { peer: 7, line: "Linie5".into(), tour: "3".into(), departure: at(9.0), done: false }]);
        assert!(s.take_handed().is_empty());
        assert_eq!(s.want_stop(7, "Linie5", "3", 540, Some(1), &own, &[], &traffic, at(9.13)), BackState::Given(540));
        // his claims gone: his 10:00 runs again though his company drops what a player leaves - not
        // the 9:00, out already
        s.set_lan_tours(HashSet::new());
        assert!(!s.departures[2].spawned && s.departures[1].spawned && s.pending.iter().filter(|&&x| x == 1).count() == 1);
        // his 10:00, a player waiting at the first stop of the 11:00 for his bus: he hands it over at
        // the terminus, an hour before, and that player gets the AI's
        let own = lan_claims("linie5/3@600~600");
        s.set_lan_tours(own.iter().cloned().collect());
        assert_eq!(s.want_relief(8, "Linie5", "3", 660, 0, &[(7, own.clone())], &traffic, at(10.1)), ReliefState::FromPlayer(7));
        assert_eq!(s.want_stop(7, "Linie5", "3", 600, None, &own, &[], &traffic, at(10.15)), BackState::No, "another player waits for the 11:00");
        assert_eq!(s.want_stop(7, "Linie5", "3", 600, Some(1), &own, &[], &traffic, at(10.1)), BackState::Ai(600, None));
        assert_eq!(s.gave_back(7, "Linie5", "3", 600, None, &traffic, at(10.11)), Ok(()));
        assert!(s.reliefs[&3].from.is_none() && s.relief_giver(3, 0, &[(7, own.clone())]).is_none(), "the AI's bus comes to him");
        // at the terminus of his 10:00, his 11:00 next and late: put out at its first stop
        s.reliefs.clear();
        let own = lan_claims("linie5/3@600~660");
        s.set_lan_tours(own.iter().cloned().collect());
        s.backed.remove(&2);
        assert_eq!(s.want_stop(7, "Linie5", "3", 600, None, &own, &[], &traffic, at(11.05)), BackState::Ai(660, None));
        assert_eq!(s.gave_back(7, "Linie5", "3", 660, None, &traffic, at(11.06)), Ok(()));
        assert!(s.pending.contains(&3));
        assert_eq!(s.relief_place_time(3, at(11.06)), at(11.0), "at its first stop");
        assert_eq!(s.take_handed().last(), Some(&Handed { peer: 7, line: "Linie5".into(), tour: "3".into(), departure: at(10.0), done: true }));
        // (at its last stop: its terminus - the tour ends there)
        assert_eq!(s.want_stop(7, "Linie5", "3", 660, Some(2), &own, &[], &traffic, at(11.2)), BackState::No);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// (31 October 2026) The bus of a LAN player waiting at the first stop of his trip - a stand
    /// other buses stop at as well - is put out a few minutes before its departure, to drive up to
    /// the stop from back along the street (`relief_approach`) in time for it: another tour's bus
    /// there comes a minute before its own, as before.
    #[test]
    fn a_reliefs_bus_comes_to_a_shared_stand_in_time_for_its_departure() {
        let dir = std::env::temp_dir().join(format!("omsi_relief_stand_{}", std::process::id()));
        let w = World { path: dir.clone(), ai: Default::default(), positions: Default::default() };
        let mut s = ScheduleSim::new(&w.path, &w, &crate::SimClock::default());
        s.data.trips.push(omsi_timetable::Trip { name: "5a".into(), line: "Linie5".into(), stations: vec![1, 2], ..Default::default() });
        s.times = vec![vec![TripTimes { stations: vec![(0.0, 0.0), (600.0, 600.0)], stops: vec![true, true], duration: 600.0, ..Default::default() }]];
        let d = |tour: &str| Departure { time: 9.0 * 3600.0, trip: 0, profile: 0, line: "Linie5".into(), ai_group: "depot".into(), tour: tour.into(), mask: -1, spawned: false, yesterday: false };
        s.departures = vec![d("3"), d("4")];
        s.tour_prev = vec![None, None];
        s.tour_next = vec![None, None];
        s.company_off = vec![false; 2];
        s.shared_stand.insert(1, true);
        let at = |h: f64| h * 3600.0;
        let (mut traffic, _) = bus_at_stop(&dir, at(12.0));
        s.last_tod = at(8.8);
        assert_eq!(s.want_relief(7, "Linie5", "3", 540, 0, &[], &traffic, at(8.8)), ReliefState::Coming);
        assert!(!s.departures[0].spawned, "not due: put out as the AI's");
        let mut queued = |s: &mut ScheduleSim, t: f64| {
            let _ = s.want_relief(7, "Linie5", "3", 540, 0, &[], &traffic, t);
            s.queue_due(&w, &mut traffic, t, 1.0);
            (s.pending.contains(&0), s.pending.contains(&1))
        };
        assert_eq!(queued(&mut s, at(9.0) - 360.0), (false, false));
        assert_eq!(queued(&mut s, at(9.0) - 240.0), (true, false), "his comes in time");
        assert_eq!(queued(&mut s, at(9.0) - 50.0), (true, true), "the other a minute before its time");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
