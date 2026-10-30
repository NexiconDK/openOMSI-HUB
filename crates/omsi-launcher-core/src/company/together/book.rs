//! What the company keeps of "Play together" (`Company::together`): the players who came, with
//! their roles and permissions (`roles`) and what they drove for the company, the duties of the
//! company's day they took (or were given, or asked for), and the trips they drove for it as
//! their games judged them. The plan puts a player on his duty before the roster
//! (`plan::day_plan`, `Who::Guest`); the day's close books his trips as the boss's own are booked
//! (`day::close_day`: measured, with his punctuality, passengers, fines and score) and the duty -
//! that duty alone, the rest of its tour runs as planned - counts as run. Every claim and report
//! is checked against the plan, or against the duty's trips as they were when it became his
//! (`Claim::sheet`) once its day has closed.

use super::super::clock;
use super::super::dates;
use super::super::duty_launch::DutyRef;
use super::super::model::{Cents, Company};
use super::super::network::PlannedTrip;
use super::super::plan::{BusOf, DayPlan, DayTour};
use super::roles::{self, Perm, Role};
use super::wire::DriveStart;
use crate::TripRun;
use serde::{Deserialize, Serialize};

/// At most this many players are kept (the oldest who has no duty goes first).
pub const MAX_GUESTS: usize = 64;

/// Why a claim or a report is refused (English: the keys of the translations).
pub const NO_DUTY: &str = "The planning no longer has this duty.";
pub const NOT_TODAY: &str = "Only duties of the company's day can be taken.";
pub const NOT_OPEN: &str = "Somebody drives this duty already.";
pub const NO_BUS: &str = "This duty has no bus of the fleet yet: the boss gives its tour one first.";
pub const BEGUN: &str = "This duty has begun already.";
pub const DRIVEN: &str = "This duty was driven already.";
pub const NOT_YOURS: &str = "This duty is not yours.";
pub const OVERLAP: &str = "You have another duty at that time.";
pub const UNDER_WAY: &str = "You are driving this duty: it stays yours until you have finished.";
pub const DONE: &str = "This duty is finished: what was driven stays.";
pub const TOO_MANY: &str = "The company has as many players as it can take.";
pub const CLOSED_DAY: &str = "The company's day closed before this duty could be driven.";
/// A duty the AI drives (its player left it) is taken back only from a trip it has not begun.
pub const AI_DRIVES: &str = "The AI drives the rest of this duty: take it back from the next trip.";
pub const AI_DONE: &str = "The AI has finished this duty: no trip is left to take back.";
pub const NO_AI: &str = "The AI does not drive this duty.";
/// (NexiconDK, Bus Company Online: single trips, 27 October 2026)
pub const NO_TRIPS: &str = "The company does not let its players take single trips.";
pub const NO_TRIP: &str = "This trip cannot be taken on its own.";
pub const TRIP_GONE: &str = "This trip leaves too soon: take a later one.";
pub const NO_FREE_TRIP: &str = "No free trip right now: try another line or stop, or a little later.";

/// (NexiconDK) A single trip is taken at least this many minutes before it leaves: his game starts
/// in the server's world and he walks to its stop, where the AI's bus is handed over to him.
pub const TRIP_LEAD: i64 = 4;
/// The next free trip (`take_next_trip`, the fleet map's bus) leaves at least this many minutes
/// from now: the game started at once, the world joined, his walk to the stop.
pub const NEXT_LEAD: i64 = 8;

/// How many days a duty whose day closed before it was driven stays its player's.
pub const LATE_DAYS: i64 = 3;

/// Every English text of this file (for the translations' check).
pub fn texts() -> [&'static str; 19] {
    [NO_DUTY, NOT_TODAY, NOT_OPEN, NO_BUS, BEGUN, DRIVEN, NOT_YOURS, OVERLAP, UNDER_WAY, DONE, TOO_MANY, CLOSED_DAY, AI_DRIVES, AI_DONE, NO_AI, NO_TRIPS, NO_TRIP, TRIP_GONE, NO_FREE_TRIP]
}

/// A player who came to the company: known by the id his launcher keeps, numbered by the
/// company (`Who::Guest`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Guest {
    pub no: u32,
    pub id: String,
    pub name: String,
    /// His role, and the permissions the boss gave (`grant`) and took (`deny`) on top of it.
    pub role: Role,
    pub grant: Vec<Perm>,
    pub deny: Vec<Perm>,
    /// Sent away for good: his launcher's messages are refused.
    pub banned: bool,
    /// What he drove for the company.
    pub stats: GuestStats,
}

impl Guest {
    /// What he may do.
    pub fn perms(&self) -> Vec<Perm> {
        roles::effective(self.role, &self.grant, &self.deny)
    }

    pub fn can(&self, p: Perm) -> bool {
        self.perms().contains(&p)
    }

    /// He keeps his place among the players when the company has too many (`guest_of`).
    fn kept(&self) -> bool {
        self.role != Role::Driver || self.banned || !self.grant.is_empty() || self.stats.trips > 0
    }
}

/// What a player drove for the company, as its books took it (`report`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct GuestStats {
    /// Duties finished, trips taken.
    pub duties: u32,
    pub trips: u32,
    pub passengers: i64,
    pub km: f64,
    /// The stops served, and those on time.
    pub stops: u32,
    pub on_time: u32,
    /// The trips' scores added up (their average: `score`).
    pub scores: u64,
    /// What his trips brought in (fares and the authority's payment) and cost in fines (cents).
    pub fares: Cents,
    pub fines: Cents,
    pub crashes: u32,
    /// The trips that came after their day had closed.
    pub late: u32,
    /// The duties he left halfway (or after they began, before his first trip): the AI drove
    /// the rest, or the company dropped it (`AiPart`; not when the boss ended the session or sent
    /// him away).
    pub left: u32,
}

impl GuestStats {
    /// The trips' average score (0 - 100).
    pub fn score(&self) -> u32 {
        if self.trips == 0 {
            0
        } else {
            (self.scores / self.trips as u64) as u32
        }
    }

    /// The share of the stops served on time (None: none served yet).
    pub fn punctuality(&self) -> Option<f64> {
        (self.stops > 0).then(|| 100.0 * self.on_time as f64 / self.stops as f64)
    }
}

/// Where a player's duty stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaimState {
    /// His: planned for him.
    #[default]
    Planned,
    /// Asked for: the boss says yes or no (the boss lets nobody take duties himself).
    Asked,
    /// His game drives it now.
    Driving,
    /// Driven and reported.
    Done,
}

/// A duty of the company's day for a player.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Claim {
    pub duty: DutyRef,
    pub guest: u32,
    pub state: ClaimState,
    /// The boss gave it him (he did not ask).
    pub given: bool,
    /// The duty as it was when it became his: its tour's number, its times, its trips and its
    /// bus's file (a report of a day closed since is checked against them: `report`).
    pub number: String,
    pub from: i32,
    pub to: i32,
    pub sheet: Vec<PlannedTrip>,
    pub bus: String,
    /// How his game starts on it (worked out by the boss's launcher: `host`).
    pub start: Option<DriveStart>,
    /// Its day closed before he drove it (`after_close`): the company had it covered, what he
    /// drives of it still counts for him and its fines and bonus for the company.
    pub late: bool,
    /// The parts of it the AI drives because he left it (Beta 63: "the route still carries on").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ai: Vec<AiPart>,
    /// (NexiconDK, Bus Company Online: single trips, 27 October 2026) Only some trips of the duty
    /// are his: from the first (an index into its trips) up to the second, not with it - when he
    /// drives on, it grows (`report`). The rest of the duty stays its driver's as the plan has it
    /// (the company's AI driver in the server's world): the plan, its pay and hours do not change,
    /// the trips he reported are booked as his (`part_drove`). Other players may take other trips
    /// of the duty (a claim each); not kept past its day.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<(usize, usize)>,
    /// (NexiconDK, single trips) Counted as left (`GuestStats::left`) before all his trips of it
    /// came: the world's check of them may still come, and then he did not leave it (`report`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub left: bool,
}

impl Claim {
    /// Trip `i` of the duty (an index into its trips) is his to drive: any of a whole duty, those
    /// of his part of a single trip's (`part`).
    pub fn his(&self, i: usize) -> bool {
        self.part.is_none_or(|(a, b)| i >= a && i < b)
    }

    /// The AI drives it now (from where he left it, not handed back yet).
    pub fn ai_now(&self) -> Option<&AiPart> {
        self.ai.last().filter(|a| a.until.is_none())
    }

    /// The AI drives (or drove) trip `i` of it (an index into its trips).
    pub fn ai_drives(&self, i: usize) -> bool {
        self.ai.iter().any(|a| a.has(i))
    }
}

/// Why the AI took over a player's duty.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LeftWhy {
    /// He left the session (or his launcher was closed).
    #[default]
    Left,
    /// His game ended before the duty did.
    Quit,
    /// His launcher was not heard from for `host::LEFT_AFTER`.
    Silent,
    /// He was sent away from the session.
    Removed,
    /// The boss's session ended.
    Closed,
    /// (NexiconDK, Bus Company Online) Given to him or taken by him after it began: the map's
    /// world drove it until then, and the AI drives it until he takes it over at its next trip.
    Begun,
    /// (NexiconDK, Bus Company Online, 28 October 2026) He handed his bus over to the AI at a stop
    /// on the way (his game's "Hand the bus over here", the map's world says so: `handed_over`):
    /// the AI drives the rest, and he did not leave it - he is free for another duty.
    HandedOver,
}

/// A part of a player's duty the AI drives: its trips from `from` (an index into the duty's
/// trips, deadheads counted) to `until` (None: to its end, or until he takes it back). The same
/// model as the company's own drivers drives it (`day::close_day_with`, `lateness::model`); the
/// trips he reported stay his.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct AiPart {
    pub from: usize,
    pub until: Option<usize>,
    pub why: LeftWhy,
    /// When it was handed over (Unix seconds) and the company's clock then (minutes of its day).
    pub at: u64,
    pub minute: i32,
    /// Nobody drives it: the company drops what a player leaves (`Together::left_dropped`; the
    /// day's close books its trips as dropped, the world keeps them off the road).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dropped: bool,
}

impl AiPart {
    pub fn has(&self, i: usize) -> bool {
        i >= self.from && self.until.is_none_or(|u| i < u)
    }
}

impl LeftWhy {
    /// He left it himself (counted in his figures, `GuestStats::left`): not the boss ending the
    /// session or sending him away.
    pub fn his(self) -> bool {
        matches!(self, LeftWhy::Left | LeftWhy::Quit | LeftWhy::Silent)
    }
}

/// A trip a player drove for the company, kept for the day's close.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct GuestTrip {
    pub guest: u32,
    pub duty: DutyRef,
    pub trip: TripRun,
    /// A trip he took on its own (`Claim::part`): the rest of its duty is its driver's.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub part: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Together {
    /// Players may take open duties themselves; else they ask and the boss says yes or no.
    pub open_claims: bool,
    pub guests: Vec<Guest>,
    pub claims: Vec<Claim>,
    pub trips: Vec<GuestTrip>,
    /// A duty its player left before driving a trip of it goes back open for another player;
    /// else (the default) the AI drives all of it.
    #[serde(default)]
    pub left_open: bool,
    /// What a player leaves of his duty nobody drives: its trips are dropped (the company loses
    /// their takings and pays the authority's penalties); else (the default) the AI drives them.
    /// The owner's choice on Bus Company Online (NexiconDK, 25 October 2026).
    #[serde(default)]
    pub left_dropped: bool,
    /// (NexiconDK, Bus Company Online) The delays its players reported to the control room
    /// (`server_fines::report_late`): their late departures not fined.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub late_reports: Vec<super::super::server_fines::LateReport>,
    /// (NexiconDK, Bus Company Online: single trips, 27 October 2026) Its players may take a
    /// single trip of a duty the company's AI driver has (`take_trip`): on, unless its owner
    /// switches it off. (A company saved before keeps it on: the default, `Together::default`.)
    pub trips_open: bool,
}

impl Default for Together {
    fn default() -> Self {
        Together { open_claims: true, guests: Vec::new(), claims: Vec::new(), trips: Vec::new(), left_open: false, left_dropped: false, late_reports: Vec::new(), trips_open: true }
    }
}

impl Together {
    pub fn guest(&self, no: u32) -> Option<&Guest> {
        self.guests.iter().find(|g| g.no == no)
    }

    /// A player's name ("?" for one no longer known).
    pub fn name_of(&self, no: u32) -> String {
        self.guest(no).map(|g| g.name.clone()).unwrap_or_else(|| "?".into())
    }

    pub fn claim_of(&self, r: &DutyRef) -> Option<&Claim> {
        self.claims.iter().find(|c| same_duty(&c.duty, r))
    }

    /// Player `guest`'s claim of duty `r`: the one he has not finished (a single trip of it he took
    /// after another, `Claim::part`), else his last.
    fn his_index(&self, r: &DutyRef, guest: u32) -> Option<usize> {
        let mine = || self.claims.iter().enumerate().filter(move |(_, x)| x.guest == guest && same_duty(&x.duty, r));
        mine().filter(|(_, x)| x.state != ClaimState::Done).map(|(k, _)| k).last().or_else(|| mine().map(|(k, _)| k).last())
    }

    /// Player `guest`'s claim of duty `r` (`his_index`).
    pub fn claim_for(&self, r: &DutyRef, guest: u32) -> Option<&Claim> {
        self.his_index(r, guest).map(|k| &self.claims[k])
    }

    /// (NexiconDK, single trips) Trip `i` of duty `r` (an index into its trips) is a player's: the
    /// whole duty is, or a single trip of it that he took (`Claim::part`).
    pub fn trip_taken(&self, r: &DutyRef, i: usize) -> bool {
        self.claims.iter().any(|x| same_duty(&x.duty, r) && x.his(i))
    }

    fn claim_mut(&mut self, r: &DutyRef) -> Option<&mut Claim> {
        self.claims.iter_mut().find(|c| same_duty(&c.duty, r))
    }

    /// The player known by `id` (his name kept as he says it now), added when new: his number.
    /// None: as many players as the company keeps, each with a duty.
    pub fn guest_of(&mut self, id: &str, name: &str) -> Option<u32> {
        let id = id.trim();
        if id.is_empty() {
            return None;
        }
        let name = clean(name, 32);
        let name = if name.is_empty() { "Driver".to_string() } else { name };
        if let Some(g) = self.guests.iter_mut().find(|g| g.id == id) {
            g.name = name;
            return Some(g.no);
        }
        if self.guests.len() >= MAX_GUESTS {
            let busy: Vec<u32> = self.claims.iter().map(|c| c.guest).chain(self.trips.iter().map(|t| t.guest)).collect();
            let k = self.guests.iter().position(|g| !busy.contains(&g.no) && !g.kept())?;
            self.guests.remove(k);
        }
        let no = self.guests.iter().map(|g| g.no).max().unwrap_or(0) + 1;
        self.guests.push(Guest { no, id: id.to_string(), name, ..Default::default() });
        Some(no)
    }

    pub fn guest_mut(&mut self, no: u32) -> Option<&mut Guest> {
        self.guests.iter_mut().find(|g| g.no == no)
    }

    /// What player `no` may do (nothing: no such player, or sent away).
    pub fn perms_of(&self, no: u32) -> Vec<Perm> {
        self.guest(no).filter(|g| !g.banned).map(Guest::perms).unwrap_or_default()
    }

    /// Player `no` gets `role` with the permissions `want` (what the role does not bring given,
    /// what it brings and he should not have taken).
    pub fn set_role(&mut self, no: u32, role: Role, want: &[Perm]) -> bool {
        let Some(g) = self.guest_mut(no) else { return false };
        let (grant, deny) = roles::overrides(role, want);
        (g.role, g.grant, g.deny) = (role, grant, deny);
        true
    }
}

/// Duties named alike (the line's case, the tour's spaces aside).
pub fn same_duty(a: &DutyRef, b: &DutyRef) -> bool {
    a.date == b.date && a.line.eq_ignore_ascii_case(&b.line) && a.tour.trim() == b.tour.trim() && a.duty == b.duty
}

/// A text a player sent, cut to `max` characters, without control characters.
pub fn clean(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control()).take(max).collect::<String>().trim().to_string()
}

/// The player on duty `k` of `line`/`tour` on `date`, when it is the company's day and he has
/// it (planned for him, driving it, or driven): `plan::day_plan` puts him on it first.
pub fn guest_on(c: &Company, date: &str, line: &str, tour: &str, k: usize) -> Option<u32> {
    if date != c.date {
        return None;
    }
    let r = DutyRef { date: date.to_string(), line: line.to_string(), tour: tour.to_string(), duty: k };
    // (a single trip of it a player took leaves the duty its driver's: `Claim::part`)
    c.together.claim_of(&r).filter(|x| x.state != ClaimState::Asked && x.part.is_none()).map(|x| x.guest)
}

/// A player's name as the pages show it.
pub fn guest_name(c: &Company, no: u32) -> String {
    c.together.name_of(no)
}

/// The AI drives a duty of player `no` of the company's day now: he left it (the pages say "AI
/// (his name left)" for him) - not one he handed over on the way (28 October 2026).
pub fn guest_left(c: &Company, no: u32) -> bool {
    c.together.claims.iter().any(|x| x.guest == no && x.duty.date == c.date && x.ai_now().is_some_and(|a| a.why != LeftWhy::HandedOver))
}

/// The tour and duty `r` names in `p`, with the tour.
fn find<'a>(p: &'a DayPlan, r: &DutyRef) -> Result<(usize, usize, &'a DayTour), &'static str> {
    let (ti, k) = r.find(p).ok_or(NO_DUTY)?;
    Ok((ti, k, &p.tours[ti]))
}

/// Whether duty `k` of `t` has begun on the company's clock (`now`, `clock::now`).
fn begun(p: &DayPlan, t: &DayTour, k: usize, now: i64) -> bool {
    now >= clock::moment(&p.date, t.duties[k].from as i64)
}

/// Who drives duty `k` of `t` by the plan - on Bus Company Online's server nobody is "you" of
/// the company (`Who::Player`): its world's AI drives such a duty, as an open one.
pub fn driver_of(c: &Company, t: &DayTour, k: usize) -> Option<super::super::plan::Who> {
    t.duties[k].who.filter(|w| !(c.online.on_server && *w == super::super::plan::Who::Player))
}

/// (NexiconDK, Bus Company Online) On the server a duty the plan gives the company's own driver
/// or an agency's - its world's AI drives it under their name - may be taken over by a player
/// (decided 26 October 2026): from its start, or from its next trip when it is under way
/// (`joined_part`), as an open one; the driver it was planned for is free for other work.
pub fn takes_over(c: &Company, t: &DayTour, k: usize) -> bool {
    c.online.on_server && matches!(driver_of(c, t, k), Some(super::super::plan::Who::Staff(_) | super::super::plan::Who::Agency))
}

/// (NexiconDK, Bus Company Online) A duty of the company's day that has begun is a player's on
/// the server from its next trip (`now`, the company's clock): the map's world drove it until
/// then, and the AI drives it until he takes it over (`take_back`) - the part the claim gets.
/// None: not on the server, or no trip of it is still to begin.
pub fn joined_part(c: &Company, p: &DayPlan, t: &DayTour, k: usize, now: i64) -> Option<AiPart> {
    if !c.online.on_server {
        return None;
    }
    let d = &t.duties[k];
    let trips = &t.tour.trips[d.start..d.end];
    trips.iter().any(|x| reportable(x) && clock::moment(&p.date, x.dep as i64) >= now).then(|| AiPart { from: 0, until: None, why: LeftWhy::Begun, at: 0, minute: (now - clock::moment(&p.date, 0)) as i32, dropped: false })
}

/// The minutes of the day `p` a claim of duty `r` takes: the duty's, or those of its trips a
/// player took on their own (`part`, `Claim::part`).
fn span_of(p: &DayPlan, r: &DutyRef, part: Option<(usize, usize)>) -> Option<(i32, i32)> {
    let (i, k) = r.find(p)?;
    let (t, d) = (&p.tours[i], &p.tours[i].duties[k]);
    match part {
        None => Some((d.from, d.to)),
        Some((a, b)) => {
            let trips = &t.tour.trips[d.start..d.end];
            Some((trips.get(a)?.dep, trips.get(b.max(a + 1) - 1)?.arr))
        }
    }
}

/// The minutes of the day `p` claim `x` keeps its player busy (`span_of`): not the rest the AI
/// drives that he left or handed over (28 October 2026: he is free for another duty then) - but
/// all of one the AI drives until he takes it over (`LeftWhy::Begun`).
fn busy_span(p: &DayPlan, x: &Claim) -> Option<(i32, i32)> {
    let (from, to) = span_of(p, &x.duty, x.part)?;
    let Some(a) = x.ai_now().filter(|a| a.why != LeftWhy::Begun) else { return Some((from, to)) };
    let (i, k) = x.duty.find(p)?;
    let (t, d) = (&p.tours[i], &p.tours[i].duties[k]);
    let Some(ai) = t.tour.trips[d.start..d.end].get(a.from) else { return Some((from, to)) };
    (ai.dep > from).then_some((from, ai.dep.min(to)))
}

/// The player's other duties (and single trips) of the day lie over `r` (its `part`).
fn overlapping(c: &Company, p: &DayPlan, r: &DutyRef, part: Option<(usize, usize)>, guest: u32) -> bool {
    let Some((from, to)) = span_of(p, r, part) else { return false };
    c.together.claims.iter().filter(|x| x.guest == guest && !same_duty(&x.duty, r) && x.duty.date == r.date).any(|x| busy_span(p, x).is_some_and(|(a, b)| a < to && from < b))
}

/// The bus file of a tour of the plan (empty: none of the fleet).
fn bus_file(c: &Company, t: &DayTour) -> String {
    match t.bus {
        Some(BusOf::Own(id)) => c.vehicle(id).map(|v| v.bus.clone()).unwrap_or_default(),
        _ => String::new(),
    }
}

/// A claim of duty `k` of `t` (its sheet as the plan has it now).
fn claim_of_plan(c: &Company, t: &DayTour, k: usize, r: &DutyRef, guest: u32, state: ClaimState, given: bool) -> Claim {
    let d = &t.duties[k];
    Claim { duty: r.clone(), guest, state, given, number: t.tour.number.clone(), from: d.from, to: d.to, sheet: t.tour.trips[d.start..d.end].to_vec(), bus: bus_file(c, t), start: None, late: false, ai: Vec::new(), part: None, left: false }
}

/// (NexiconDK, decided 26 October 2026) The duties of the company's day `p` the roster gives a
/// player of it (`Who::Guest`, planned for that weekday in the Roster) his - planned for him, as a
/// duty given - when nobody has them, he is not sent away, they have a bus of the company's and
/// lie over none of his: the day's begin (`clock::begin_day`). Whether any became his.
pub fn claims_from_roster(c: &mut Company, p: &DayPlan) -> bool {
    if !p.today || p.date != c.date {
        return false;
    }
    let mut any = false;
    for (ti, t) in p.tours.iter().enumerate() {
        if t.by_player || !matches!(t.bus, Some(BusOf::Own(_))) {
            continue;
        }
        let Some(r) = super::super::plan::roster(c, p.weekday, &t.tour.line, &t.tour.tour) else { continue };
        let wanted: Vec<(usize, u32)> = r.duties.iter().enumerate().filter_map(|(k, w)| match w {
            Some(super::super::plan::Who::Guest(g)) => Some((k, *g)),
            _ => None,
        }).collect();
        for (k, g) in wanted {
            let Some(duty) = DutyRef::of(p, ti, k) else { continue };
            if k >= t.duties.len() || c.together.claim_of(&duty).is_some() || !c.together.guest(g).is_some_and(|x| !x.banned) || overlapping(c, p, &duty, None, g) {
                continue;
            }
            let claim = claim_of_plan(c, t, k, &duty, g, ClaimState::Planned, true);
            c.together.claims.push(claim);
            any = true;
        }
    }
    any
}

/// A player takes duty `r` of the company's day (`p`, at `now` on the company's clock): his at
/// once when the boss lets players take open duties or he may approve requests himself
/// (`direct`), else asked for. What it is now.
pub fn take(c: &mut Company, p: &DayPlan, r: &DutyRef, guest: u32, now: i64, direct: bool) -> Result<ClaimState, &'static str> {
    if !p.today || r.date != c.date {
        return Err(NOT_TODAY);
    }
    let (_, k, t) = find(p, r)?;
    if let Some(x) = c.together.claim_of(r) {
        return if x.guest == guest && x.part.is_none() { Ok(x.state) } else { Err(NOT_OPEN) };
    }
    if t.by_player {
        return Err(DRIVEN);
    }
    if !matches!(t.bus, Some(BusOf::Own(_))) {
        return Err(NO_BUS);
    }
    if driver_of(c, t, k).is_some() && !takes_over(c, t, k) {
        return Err(NOT_OPEN);
    }
    // (on Bus Company Online's server: his from its next trip, the AI's until then - one the
    // company's AI drivers have too)
    let part = if begun(p, t, k, now) { Some(joined_part(c, p, t, k, now).ok_or(BEGUN)?) } else { None };
    if overlapping(c, p, r, None, guest) {
        return Err(OVERLAP);
    }
    let state = if c.together.open_claims || direct { ClaimState::Planned } else { ClaimState::Asked };
    let mut claim = claim_of_plan(c, t, k, r, guest, state, false);
    claim.ai.extend(part);
    c.together.claims.push(claim);
    Ok(state)
}

/// The boss (or a player who may) gives duty `r` to a player (whoever had it today gives it
/// up: the player drives it in his own game, at his own pace).
pub fn give(c: &mut Company, p: &DayPlan, r: &DutyRef, guest: u32, now: i64) -> Result<(), &'static str> {
    if !p.today || r.date != c.date {
        return Err(NOT_TODAY);
    }
    let (_, k, t) = find(p, r)?;
    if t.by_player {
        return Err(DRIVEN);
    }
    if !matches!(t.bus, Some(BusOf::Own(_))) {
        return Err(NO_BUS);
    }
    // (every claim of it: a single trip of it others took, too - `Claim::part`)
    for x in c.together.claims.iter().filter(|x| same_duty(&x.duty, r)) {
        match x.state {
            ClaimState::Driving if x.guest != guest || x.part.is_some() => return Err(UNDER_WAY),
            ClaimState::Done => return Err(DONE),
            _ => {}
        }
    }
    let others = !c.together.claims.iter().any(|x| same_duty(&x.duty, r) && x.guest == guest && x.part.is_none());
    if begun(p, t, k, now) && driver_of(c, t, k).is_some() && others && !takes_over(c, t, k) {
        return Err(BEGUN);
    }
    // (on Bus Company Online's server a duty under way is his from its next trip, the AI's until
    // then - not when no trip of it is left)
    let part = if begun(p, t, k, now) && others && c.online.on_server { Some(joined_part(c, p, t, k, now).ok_or(BEGUN)?) } else { None };
    if overlapping(c, p, r, None, guest) {
        return Err(OVERLAP);
    }
    let mut claim = claim_of_plan(c, t, k, r, guest, ClaimState::Planned, true);
    claim.ai.extend(part);
    c.together.claims.retain(|x| !same_duty(&x.duty, r));
    c.together.claims.push(claim);
    Ok(())
}

// --- single trips (NexiconDK, Bus Company Online, 27 October 2026) --------------------------------

/// A trip of block `t` a player may take on its own: one with passengers, whole (not cut at a
/// relief point on the way, which two duties share), of the block's own tour of the timetable (his
/// game starts on that tour).
fn single(t: &DayTour, x: &PlannedTrip) -> bool {
    reportable(x) && x.part.is_none() && (x.origin.trim().is_empty() || x.origin.trim() == t.tour.tour.trim())
}

/// What a player looks for single trips by (`free_trips`): a line - its name, or the number its
/// buses show (empty: any) - and the stop they leave from (empty: any); a bus on the fleet map
/// names its tour and the trip's departure (minutes of the day) besides.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct TripFilter {
    pub line: String,
    pub stop: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub tour: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i32>,
    /// The next free one, his game started on it at once (`take_next_trip`): `NEXT_LEAD` off.
    #[serde(skip)]
    pub next: bool,
}

impl TripFilter {
    /// How far off a trip found leaves at least (minutes).
    fn lead(&self) -> i64 {
        if self.next {
            NEXT_LEAD
        } else {
            TRIP_LEAD
        }
    }

    fn line_is(&self, t: &DayTour, x: &PlannedTrip) -> bool {
        let l = self.line.trim();
        let line = l.is_empty() || t.tour.line.eq_ignore_ascii_case(l) || t.tour.number.trim().eq_ignore_ascii_case(l) || x.line.trim().eq_ignore_ascii_case(l);
        let tour = self.tour.trim().is_empty() || t.tour.tour.trim() == self.tour.trim();
        line && tour && self.at.is_none_or(|m| m == x.dep)
    }

    fn stop_is(&self, x: &PlannedTrip) -> bool {
        let s = self.stop.trim();
        s.is_empty() || x.from.trim().eq_ignore_ascii_case(s)
    }
}

/// A trip of the company's day free to take on its own (`free_trips`): its duty, its place in the
/// duty's trips, its tour in the plan (an index into its tours) and the trip.
#[derive(Clone, Debug, PartialEq)]
pub struct FreeTrip {
    pub duty: DutyRef,
    pub trip: usize,
    pub tour: usize,
    pub x: PlannedTrip,
}

/// The trips of the company's day `p` a player may take on their own (`take_trip`), the soonest
/// first: on Bus Company Online's server, while its owner lets players take them
/// (`Together::trips_open`) - of the duties its AI drivers have (`takes_over`) that nobody took,
/// leaving `TRIP_LEAD` minutes from now (`now`, the company's clock) or later, as `f` asks.
pub fn free_trips(c: &Company, p: &DayPlan, now: i64, f: &TripFilter) -> Vec<FreeTrip> {
    let mut out = Vec::new();
    if !p.today || p.date != c.date || !c.online.on_server || !c.together.trips_open {
        return out;
    }
    for (ti, t) in p.tours.iter().enumerate() {
        if t.tour.unplanned || t.by_player || !matches!(t.bus, Some(BusOf::Own(_))) {
            continue;
        }
        for (k, d) in t.duties.iter().enumerate() {
            if !takes_over(c, t, k) {
                continue;
            }
            let Some(r) = DutyRef::of(p, ti, k) else { continue };
            for (i, x) in t.tour.trips[d.start..d.end].iter().enumerate() {
                if single(t, x) && !c.together.trip_taken(&r, i) && clock::moment(&p.date, x.dep as i64) >= now + f.lead() && f.line_is(t, x) && f.stop_is(x) {
                    out.push(FreeTrip { duty: r.clone(), trip: i, tour: ti, x: x.clone() });
                }
            }
        }
    }
    out.sort_by(|a, b| (a.x.dep, &a.duty.line, &a.duty.tour).cmp(&(b.x.dep, &b.duty.line, &b.duty.tour)));
    out
}

/// A player takes trip `i` of duty `r` (an index into its trips) of the company's day `p` on its
/// own (`now`: the company's clock): his at once when the boss lets players take duties, or he may
/// approve requests himself (`direct`), else asked for. The rest of the duty stays its driver's -
/// the company's AI driver - and the AI drives it on after his trip (`Claim::part`). What it is now.
pub fn take_trip(c: &mut Company, p: &DayPlan, r: &DutyRef, i: usize, guest: u32, now: i64, direct: bool) -> Result<ClaimState, &'static str> {
    if !p.today || r.date != c.date {
        return Err(NOT_TODAY);
    }
    if !c.online.on_server || !c.together.trips_open {
        return Err(NO_TRIPS);
    }
    let (_, k, t) = find(p, r)?;
    // (a trip of it he has: as it is; another trip of it before he finished that one, or one a
    // player has - the whole duty, or that trip: not his)
    if let Some(x) = c.together.claim_for(r, guest).filter(|x| x.state != ClaimState::Done) {
        return if x.part.is_some_and(|(a, _)| a == i) { Ok(x.state) } else { Err(NOT_OPEN) };
    }
    if c.together.trip_taken(r, i) {
        return Err(NOT_OPEN);
    }
    if t.by_player {
        return Err(DRIVEN);
    }
    if !matches!(t.bus, Some(BusOf::Own(_))) {
        return Err(NO_BUS);
    }
    if !takes_over(c, t, k) {
        return Err(NOT_OPEN);
    }
    let d = &t.duties[k];
    let x = t.tour.trips[d.start..d.end].get(i).filter(|x| single(t, x)).ok_or(NO_TRIP)?;
    if clock::moment(&p.date, x.dep as i64) < now + TRIP_LEAD {
        return Err(TRIP_GONE);
    }
    let part = (i, i + 1);
    if overlapping(c, p, r, Some(part), guest) {
        return Err(OVERLAP);
    }
    let state = if c.together.open_claims || direct { ClaimState::Planned } else { ClaimState::Asked };
    let mut claim = claim_of_plan(c, t, k, r, guest, state, false);
    (claim.from, claim.to, claim.part) = (x.dep, x.arr, Some(part));
    c.together.claims.push(claim);
    Ok(state)
}

/// A player takes the first free trip `f` asks for (`free_trips`) that fits his day: the trip
/// (its duty and its place in it) and what his claim is now.
pub fn take_next_trip(c: &mut Company, p: &DayPlan, f: &TripFilter, guest: u32, now: i64, direct: bool) -> Result<(DutyRef, usize, ClaimState), &'static str> {
    if !c.online.on_server || !c.together.trips_open {
        return Err(NO_TRIPS);
    }
    // (his game starts at once on the trip taken: one far enough off to get there)
    let f = TripFilter { next: true, ..f.clone() };
    for ft in free_trips(c, p, now, &f).into_iter().take(500) {
        if let Ok(state) = take_trip(c, p, &ft.duty, ft.trip, guest, now, direct) {
            return Ok((ft.duty, ft.trip, state));
        }
    }
    Err(NO_FREE_TRIP)
}

/// Player `guest`'s part of duty `r` (`Claim::part`, its trips `a..b`): whether he reported all
/// its trips, whether its first has left on the company's clock, whether he reported none.
fn part_state(c: &Company, p: Option<&DayPlan>, r: &DutyRef, guest: u32, (a, b): (usize, usize)) -> (bool, bool, bool) {
    let trips = trips_now(c, p, r);
    let deps: Vec<i32> = trips_of(c, r).filter(|g| g.guest == guest).map(|g| (g.trip.departure / 60.0).round() as i32).collect();
    let mine: Vec<bool> = trips.iter().enumerate().filter(|(i, x)| *i >= a && *i < b && reportable(x)).map(|(_, x)| deps.contains(&x.whole_dep())).collect();
    let begun = trips.get(a).is_none_or(|t| clock::now(c) >= clock::moment(&r.date, t.dep as i64));
    (mine.iter().all(|m| *m), begun, !mine.iter().any(|m| *m))
}

/// (NexiconDK, single trips) Where player `guest`'s game starts on his part of duty `r`: its first
/// trip with passengers he has not reported (an index into its trips) - not one that has left before
/// he started it (`now`, the company's clock).
pub fn part_from(c: &Company, p: Option<&DayPlan>, r: &DutyRef, guest: u32, now: i64) -> Result<usize, &'static str> {
    let x = c.together.claim_for(r, guest).ok_or(NOT_YOURS)?;
    let Some((a, b)) = x.part else { return Ok(0) };
    let trips = trips_now(c, p, r);
    let deps: Vec<i32> = trips_of(c, r).filter(|g| g.guest == guest).map(|g| (g.trip.departure / 60.0).round() as i32).collect();
    let from = (a..b.min(trips.len())).find(|&i| reportable(&trips[i]) && !deps.contains(&trips[i].whole_dep())).ok_or(DONE)?;
    if x.state == ClaimState::Planned && clock::moment(&r.date, trips[from].dep as i64) < now {
        return Err(TRIP_GONE);
    }
    Ok(from)
}

/// (NexiconDK, single trips) How far player `guest` may drive on from trip `b` of duty `r` (`n`
/// its trips): up to the first trip of it another claim has (a single trip another took).
pub fn free_until(c: &Company, r: &DutyRef, guest: u32, b: usize, n: usize) -> usize {
    let mine = c.together.his_index(r, guest);
    (b..n).find(|&j| c.together.claims.iter().enumerate().any(|(k, x)| Some(k) != mine && same_duty(&x.duty, r) && x.his(j))).unwrap_or(n)
}

/// (NexiconDK, single trips) The single trips taken that were not driven and have left more than
/// `grace` minutes ago on the company's clock (`now`): planned or asked for, none of their trips
/// reported - their players' games never came to the stop. Their places in the claims.
pub fn stale_trips(c: &Company, now: i64, grace: i64) -> Vec<usize> {
    c.together
        .claims
        .iter()
        .enumerate()
        .filter(|(_, x)| matches!(x.state, ClaimState::Planned | ClaimState::Asked) && !x.late && x.duty.date == c.date)
        .filter_map(|(k, x)| {
            let (a, b) = x.part?;
            let t = x.sheet.get(a)?;
            let none = !trips_of(c, &x.duty).any(|g| g.guest == x.guest && x.sheet.iter().take(b).skip(a).any(|y| (g.trip.departure / 60.0).round() as i32 == y.whole_dep()));
            (none && now >= clock::moment(&x.duty.date, t.dep as i64) + grace).then_some(k)
        })
        .collect()
}

/// The boss says yes (or no) to a duty a player asked for: whether there was such a request.
pub fn answer(c: &mut Company, r: &DutyRef, yes: bool) -> Option<u32> {
    // (the claim asked for: of a whole duty, or a single trip of it - `Claim::part`)
    let k = c.together.claims.iter().position(|x| same_duty(&x.duty, r) && x.state == ClaimState::Asked)?;
    let guest = c.together.claims[k].guest;
    if yes {
        c.together.claims[k].state = ClaimState::Planned;
    } else {
        c.together.claims.remove(k);
    }
    Some(guest)
}

/// A duty given back: by its player (`guest`; not while he drives it), or by the boss (None).
/// The player it was.
pub fn release(c: &mut Company, r: &DutyRef, guest: Option<u32>) -> Result<u32, &'static str> {
    // (his: of the duty, or of a single trip of it; the boss's: the duty's - or the first trip of
    // it a player took)
    let k = match guest {
        Some(g) => c.together.his_index(r, g),
        None => c.together.claims.iter().position(|x| same_duty(&x.duty, r)),
    }
    .ok_or(NOT_YOURS)?;
    let x = &c.together.claims[k];
    match x.state {
        ClaimState::Done => return Err(DONE),
        ClaimState::Driving if guest.is_some() => return Err(UNDER_WAY),
        _ => {}
    }
    let had = x.guest;
    c.together.claims.remove(k);
    Ok(had)
}

/// The duty a player drives now, or has driven.
pub fn set_state(c: &mut Company, r: &DutyRef, guest: u32, state: ClaimState) -> Result<(), &'static str> {
    let k = c.together.his_index(r, guest).ok_or(NOT_YOURS)?;
    let x = &c.together.claims[k];
    if x.state == ClaimState::Asked {
        return Err(NOT_YOURS);
    }
    if state == ClaimState::Done && x.state != ClaimState::Done && x.part.is_none() {
        if let Some(g) = c.together.guest_mut(guest) {
            g.stats.duties += 1;
        }
    }
    c.together.claims[k].state = state;
    Ok(())
}

/// How a player's game starts on his duty `r`, kept with his claim (a duty whose day closed
/// starts with what was kept).
pub fn keep_start(c: &mut Company, r: &DutyRef, guest: u32, start: &DriveStart) {
    if let Some(k) = c.together.his_index(r, guest) {
        c.together.claims[k].start = Some(start.clone());
    }
}

/// What a report is checked against: the tour as the timetable names it, its line's number,
/// the bus it is driven with and the duty's planned trips.
struct Sheet<'a> {
    tour: &'a str,
    number: &'a str,
    bus: String,
    planned: &'a [PlannedTrip],
}

/// The planned trip of the duty a trip report is of: a trip with passengers that leaves at the
/// report's minute, of its tour as the game knows it, on a line the trip shows or the tour's.
fn trip_of<'a>(s: &Sheet<'a>, run: &TripRun) -> Option<&'a PlannedTrip> {
    if !run.departure.is_finite() {
        return None;
    }
    let dep = (run.departure / 60.0).round() as i32;
    let line = run.line.trim();
    s.planned.iter().find(|x| {
        let tour = if x.origin.trim().is_empty() { s.tour.trim() } else { x.origin.trim() };
        x.counts() && !x.is_second() && x.whole_dep() == dep && tour == run.tour.trim() && (line.eq_ignore_ascii_case(x.line.trim()) || line.eq_ignore_ascii_case(s.number.trim()))
    })
}

/// A number in `0..=max`.
fn within(v: i32, max: i32) -> i32 {
    v.clamp(0, max.max(0))
}

fn finite(v: Option<f64>, max: f64) -> Option<f64> {
    v.filter(|x| x.is_finite()).map(|x| x.clamp(-max, max))
}

/// A player's trip made safe for the books: what the plan says of it (the company's map, its
/// line, the tour's bus) and his figures within what the trip can have had.
fn safe_trip(c: &Company, s: &Sheet, x: &PlannedTrip, run: &TripRun, name: &str, time: u64) -> TripRun {
    let stops = x.part.as_ref().map_or(x.stops, |p| p.stops) as i32;
    let km = x.part.as_ref().map_or(x.km, |p| p.km).max(0.0);
    let mut r = run.clone();
    r.time = time;
    r.driver = name.to_string();
    r.map = c.map.clone();
    r.bus = s.bus.clone();
    r.line = if x.line.trim().is_empty() { s.number.to_string() } else { x.line.clone() };
    r.free = false;
    r.planned = within(r.planned, stops);
    r.stops = within(r.stops, stops);
    r.early = within(r.early, r.stops);
    r.late = within(r.late, r.stops - r.early);
    r.worst = finite(r.worst, 7200.0);
    r.average = finite(r.average, 7200.0);
    r.seconds = if r.seconds.is_finite() { r.seconds.clamp(0.0, 6.0 * 3600.0) } else { 0.0 };
    r.metres = if r.metres.is_finite() { r.metres.clamp(0.0, km * 1500.0 + 2000.0) } else { 0.0 };
    r.jolts = r.jolts.max(0);
    r.crashes = r.crashes.max(0);
    r.passengers = within(r.passengers, 250);
    r.red_lights = r.red_lights.max(0);
    r.speeding = r.speeding.max(0);
    r.fines = r.fines.clamp(0, 2_000_00);
    r.hard_brakes = r.hard_brakes.max(0);
    r.hard_starts = r.hard_starts.max(0);
    r.rough_stops = r.rough_stops.max(0);
    // (NexiconDK: those he left at the stop - not more than he could have had)
    r.left_behind = within(r.left_behind, super::super::day::LEFT_MOST);
    r.left_full = within(r.left_full, super::super::day::LEFT_MOST);
    r.offences.truncate(50);
    r.visits.truncate(200);
    r
}

/// What a trip of a player brings the company (cents), as the day's close books it
/// (`day::close_day_with`): its passengers' takings (none on a gross-cost line: the authority's)
/// and a weekend line's tourism grant; on a bus contract its journey's share of the contract's
/// operating day (`bus_contracts`).
pub fn trip_money(c: &Company, t: &TripRun) -> Cents {
    use super::super::{bus_contracts as bc, ownline};
    let Some(l) = super::super::network::line_of_number(c, &t.line) else { return 0 };
    let km = super::super::career::believable_km(t);
    // (the takings of its passengers, `income`, and what the trip is paid besides)
    let ol = ownline::kind_of(l);
    let pax = t.passengers.max(0) as f64;
    let rates = super::super::income::Rates::of(c, l, &c.date);
    let mut take = if ol == crate::service::ServiceKind::School { Default::default() } else { rates.takings(pax, km) };
    ownline::kind_takings(ol, &mut take, pax, c.price_index);
    let comp = ownline::trip_payment(km, ownline::per_km_of(c, l));
    let fares = if rates.to_authority { 0 } else { take.total().round() as Cents };
    let contract = bc::on_line(c, &l.name, &c.date).map_or(0, |k| (bc::day_value(c, k) / k.journeys.max(1) as f64).round() as Cents);
    fares + comp + contract
}

/// A trip taken into a player's figures (`late`: after its day closed, when its fares were not
/// the company's).
fn count_trip(c: &mut Company, guest: u32, t: &TripRun, late: bool) {
    let money = if late { 0 } else { trip_money(c, t) };
    let score = super::super::career::evaluate(t).score;
    let km = super::super::career::believable_km(t);
    let Some(g) = c.together.guest_mut(guest) else { return };
    let s = &mut g.stats;
    s.trips += 1;
    s.passengers += t.passengers.max(0) as i64;
    s.km += km;
    s.stops += t.stops.max(0) as u32;
    s.on_time += t.on_time().max(0) as u32;
    s.scores += score as u64;
    s.fares += money;
    s.fines += t.fines.max(0);
    s.crashes += t.crashes.max(0) as u32;
    if late {
        s.late += 1;
    }
}

/// A player's trip reports of his duty `r`: those of its planned trips not reported yet are
/// kept for the day's close (made safe: `safe_trip`), the others refused. A duty whose day has
/// closed since (`Claim::late`) is checked against its sheet, and its trips are booked at once:
/// the company had the duty covered, so their fines, quality bonus and experience
/// (`levels::book_trip`) and the player's figures, not their fares. `now_unix`: when they came
/// (the close books trips that ended after its last one). How many were taken and refused.
pub fn report(c: &mut Company, p: Option<&DayPlan>, r: &DutyRef, guest: u32, runs: &[TripRun], now_unix: u64) -> Result<(usize, usize), &'static str> {
    let x = c.together.claim_for(r, guest).filter(|x| x.state != ClaimState::Asked).ok_or(NOT_YOURS)?.clone();
    let today = r.date == c.date && p.is_some_and(|p| p.today);
    if !today && !x.late {
        return Err(NOT_TODAY);
    }
    // (today: the plan as it is now; a closed day: the duty as it was)
    let (tour, planned, number, bus) = match p.filter(|_| today) {
        Some(p) => {
            let (_, k, t) = find(p, r)?;
            let d = &t.duties[k];
            (t.tour.tour.clone(), t.tour.trips[d.start..d.end].to_vec(), t.tour.number.clone(), bus_file(c, t))
        }
        None => (r.tour.clone(), x.sheet.clone(), x.number.clone(), x.bus.clone()),
    };
    let sheet = Sheet { tour: &tour, number: &number, bus, planned: &planned };
    let name = c.together.name_of(guest);
    let time = now_unix.max(c.trips_seen + 1);
    let (mut taken, mut refused) = (0, 0);
    for run in runs.iter().take(64) {
        let Some(px) = trip_of(&sheet, run) else {
            refused += 1;
            continue;
        };
        let dep = px.whole_dep();
        let again = c.together.trips.iter().any(|g| same_duty(&g.duty, r) && (g.trip.departure / 60.0).round() as i32 == dep);
        let i = planned.iter().position(|y| std::ptr::eq(y, px)).unwrap_or(usize::MAX);
        // (his claim the trip is of: the duty's, or a single trip of it he took - or the one he
        // drove on from, `trip_claim`)
        let Some((k, grows)) = trip_claim(&c.together, r, guest, &planned, i) else {
            refused += 1;
            continue;
        };
        // (a trip the AI drives since he left: the AI's, not his)
        let ai = c.together.claims[k].ai_drives(i);
        if again || ai {
            refused += 1;
            continue;
        }
        // (NexiconDK, single trips: one after his he drove on with - the next of the duty that
        // carries passengers - is his too, his part grows)
        if grows {
            let y = &mut c.together.claims[k];
            y.part = y.part.map(|(a, _)| (a, i + 1));
            y.to = planned[i].arr;
        }
        let part = c.together.claims[k].part;
        let safe = safe_trip(c, &sheet, px, run, &name, time);
        if !today {
            super::super::levels::book_trip(c, &safe, &number);
        }
        count_trip(c, guest, &safe, !today);
        c.together.trips.push(GuestTrip { guest, duty: r.clone(), trip: safe, part: part.is_some() });
        taken += 1;
        // (counted as left before the world's check of his trips came: all of them came, he did
        // not leave it)
        if let Some(part) = part.filter(|_| c.together.claims[k].left) {
            if part_state(c, p.filter(|_| today), r, guest, part).0 {
                c.together.claims[k].left = false;
                if let Some(g) = c.together.guest_mut(guest) {
                    g.stats.left = g.stats.left.saturating_sub(1);
                }
            }
        }
    }
    Ok((taken, refused))
}

/// The claim of player `guest`'s that trip `i` of duty `r` (`planned`: its trips) is of: the one
/// whose trips it is in (a whole duty's: any), or a single trip's he drove on from - the duty's
/// next trip with passengers after his part, no other player's (`grows`). Its place in the claims.
fn trip_claim(t: &Together, r: &DutyRef, guest: u32, planned: &[PlannedTrip], i: usize) -> Option<(usize, bool)> {
    if i >= planned.len() {
        return None;
    }
    let mine: Vec<usize> = t.claims.iter().enumerate().filter(|(_, x)| x.guest == guest && same_duty(&x.duty, r) && x.state != ClaimState::Asked).map(|(k, _)| k).collect();
    if let Some(&k) = mine.iter().find(|&&k| t.claims[k].his(i)) {
        return Some((k, false));
    }
    let taken = t.claims.iter().any(|x| same_duty(&x.duty, r) && x.his(i));
    let k = mine.into_iter().find(|&k| t.claims[k].part.is_some_and(|(_, b)| i >= b && planned[b.min(i)..i].iter().all(|y| !reportable(y))))?;
    (!taken).then_some((k, true))
}

/// The players' trips the day's close books with the boss's own (`store::close_day`).
pub fn trips_for_close(c: &Company) -> Vec<TripRun> {
    c.together.trips.iter().filter(|t| t.duty.date == c.date).map(|t| t.trip.clone()).collect()
}

/// A trip of the close's is one a player of "Play together" drove (`day::close_day_with`: it is
/// booked as measured, and his duty alone counts as driven - not the whole tour).
pub fn is_guest_trip(c: &Company, t: &TripRun) -> bool {
    c.together.trips.iter().any(|g| g.duty.date == c.date && g.trip == *t)
}

/// A player drove duty `k` of `line`/`tour` on `date`: his trips of it are in (the day's close
/// models the rest of the tour, not that duty).
pub fn drove(c: &Company, date: &str, line: &str, tour: &str, k: usize) -> bool {
    c.together.trips.iter().any(|g| !g.part && g.duty.date == date && g.duty.duty == k && g.duty.line.eq_ignore_ascii_case(line) && g.duty.tour.trim() == tour.trim())
}

/// (NexiconDK, single trips) The trip of `line`/`tour` leaving at `dep` (whole minutes) on `date` is
/// one a player took on its own and reported (`Claim::part`): booked as his, measured, with the
/// players' trips (`trips_for_close`) - the day's close runs the rest of its duty with its driver,
/// not that trip (whatever duty the plan has it in now).
pub fn part_drove(c: &Company, date: &str, line: &str, tour: &str, dep: i32) -> bool {
    c.together.trips.iter().any(|g| g.part && g.duty.date == date && g.duty.line.eq_ignore_ascii_case(line) && g.duty.tour.trim() == tour.trim() && (g.trip.departure / 60.0).round() as i32 == dep)
}

/// After the close of `closed`: its trips go (booked), and so do its claims - but a duty given
/// or taken that was not driven yet stays its player's for a few days (`Claim::late`: he drives
/// it at his own pace, the company had it covered); those older than that go.
pub fn after_close(c: &mut Company, closed: &str) {
    let driven: Vec<DutyRef> = c.together.trips.iter().filter(|t| t.duty.date.as_str() <= closed).map(|t| t.duty.clone()).collect();
    for x in c.together.claims.iter_mut().filter(|x| x.duty.date.as_str() <= closed) {
        // (one the AI drove for him since he left it was driven)
        // (a single trip is not kept past its day - but one his game drives as it closes: its trips
        // still come, `Claim::part`)
        let keep = x.part.is_none() || x.state == ClaimState::Driving;
        if matches!(x.state, ClaimState::Planned | ClaimState::Driving) && x.ai.is_empty() && keep && (x.part.is_some() || !driven.iter().any(|d| same_duty(d, &x.duty))) {
            x.late = true;
        }
    }
    let today = c.date.clone();
    c.together.claims.retain(|x| x.duty.date.as_str() > closed || (x.late && x.state != ClaimState::Done && dates::between(&x.duty.date, &today) <= LATE_DAYS));
    c.together.trips.retain(|x| x.duty.date.as_str() > closed);
}

/// A player's trips of a duty, as reported.
pub fn trips_of<'a>(c: &'a Company, r: &'a DutyRef) -> impl Iterator<Item = &'a GuestTrip> + 'a {
    c.together.trips.iter().filter(move |t| same_duty(&t.duty, r))
}

// --- the AI on a duty its player left (Beta 63) -------------------------------------------------

/// The trips of duty `r`: as the plan has them today, else as they were when it became its
/// player's (`Claim::sheet`).
fn trips_now(c: &Company, p: Option<&DayPlan>, r: &DutyRef) -> Vec<PlannedTrip> {
    let today = p.filter(|p| p.today && r.date == c.date).and_then(|p| find(p, r).ok()).map(|(_, k, t)| t.tour.trips[t.duties[k].start..t.duties[k].end].to_vec());
    today.or_else(|| c.together.claim_of(r).map(|x| x.sheet.clone())).unwrap_or_default()
}

/// The departures (whole minutes) of the trips its player reported of duty `r`.
fn reported(c: &Company, r: &DutyRef) -> Vec<i32> {
    trips_of(c, r).map(|t| (t.trip.departure / 60.0).round() as i32).collect()
}

/// A trip a player reports (with passengers, not a trip's second part).
fn reportable(x: &PlannedTrip) -> bool {
    x.counts() && !x.is_second()
}

/// Where its player got to on duty `r` (`trips`: its trips): the trip after the last he reported
/// (an index into them; 0: he reported none).
pub fn reached(c: &Company, trips: &[PlannedTrip], r: &DutyRef) -> usize {
    let deps = reported(c, r);
    trips.iter().rposition(|x| reportable(x) && deps.contains(&x.whole_dep())).map_or(0, |i| i + 1)
}

/// What became of a duty its player left.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handed {
    /// The AI drives its trips from this one on (an index into its trips).
    Ai(usize),
    /// He drove all of it: nothing is left.
    Over,
    /// He drove none of it, and it went back open for another player (`Together::left_open`).
    Open,
}

/// Player `guest` left his duty `r` (planned for him, or driving it) while it runs: the AI drives
/// the rest of it - from the trip after the last he reported, or all of it when he reported none
/// (or it goes back open, when the boss says so) -, with the same model as the company's own
/// drivers. Nothing of it is dropped, and the trips he reported stay his. `now_unix` and `minute`
/// (the company's clock) say when.
pub fn hand_to_ai(c: &mut Company, p: Option<&DayPlan>, r: &DutyRef, guest: u32, why: LeftWhy, now_unix: u64, minute: i32) -> Result<Handed, &'static str> {
    let k = c.together.his_index(r, guest).ok_or(NOT_YOURS)?;
    let x = &c.together.claims[k];
    match x.state {
        ClaimState::Asked => return Err(NOT_YOURS),
        ClaimState::Done => return Err(DONE),
        _ => {}
    }
    // (a duty whose day closed was covered by the company already)
    if x.late {
        return Err(CLOSED_DAY);
    }
    // (NexiconDK: a single trip he took - its duty is its driver's, who drives what he did not: it
    // goes back to the company before it left, else it is over - his trips reported stay his, and
    // those the world reports later still come)
    if let Some(part) = x.part {
        let (done, begun, none) = part_state(c, p, r, guest, part);
        if !done && !begun && none {
            c.together.claims.remove(k);
            return Ok(Handed::Open);
        }
        let y = &mut c.together.claims[k];
        y.state = ClaimState::Done;
        // (counted as left until the world's check of his trips says he drove them: `report`)
        if !done && begun && why.his() && !y.left {
            y.left = true;
            if let Some(g) = c.together.guest_mut(guest) {
                g.stats.left += 1;
            }
        }
        return Ok(Handed::Over);
    }
    if let Some(a) = x.ai_now() {
        return Ok(Handed::Ai(a.from));
    }
    let trips = trips_now(c, p, r);
    // (taken back from the AI before: from where he drove again)
    let from = reached(c, &trips, r).max(x.ai.last().and_then(|a| a.until).unwrap_or(0));
    if !trips.iter().skip(from).any(reportable) {
        return Ok(Handed::Over);
    }
    if from == 0 && x.ai.is_empty() && c.together.left_open {
        c.together.claims.retain(|x| !same_duty(&x.duty, r));
        return Ok(Handed::Open);
    }
    // (or nobody drives it, when the company drops what a player leaves - its owner's choice)
    let dropped = c.together.left_dropped;
    if let Some(x) = c.together.claim_mut(r) {
        x.ai.push(AiPart { from, until: None, why, at: now_unix, minute, dropped });
    }
    if why.his() {
        if let Some(g) = c.together.guest_mut(guest) {
            g.stats.left += 1;
        }
    }
    Ok(Handed::Ai(from))
}

/// Player `guest` did not leave his duty after all (his game drove on while his launcher was not
/// heard from, or he took it back before the AI drove a trip of it): the AI's part of it, from
/// where he got to, goes, and his figures forget that he left.
pub fn not_left(c: &mut Company, r: &DutyRef, guest: u32) {
    let Some(x) = c.together.claim_mut(r).filter(|x| x.guest == guest) else { return };
    let Some(a) = x.ai.pop() else { return };
    if a.why.his() {
        if let Some(g) = c.together.guest_mut(guest) {
            g.stats.left = g.stats.left.saturating_sub(1);
        }
    }
}

/// The world saw player `guest` drive trip `run` of his duty `r` (Bus Company Online): when its
/// report came after he had left the duty (his launcher said so first; the world checks a trip
/// before it is booked), the part the AI drives - or that is dropped - begins after it, and goes
/// when nothing is left of it (he drove all of it: he did not leave it). Whether it changed.
pub fn world_saw(c: &mut Company, p: Option<&DayPlan>, r: &DutyRef, guest: u32, run: &TripRun) -> bool {
    let Some(x) = c.together.claim_of(r).filter(|x| x.guest == guest && x.ai_now().is_some()) else { return false };
    let today = r.date == c.date && p.is_some_and(|p| p.today);
    let (tour, planned, number) = match p.filter(|_| today).and_then(|p| find(p, r).ok()) {
        Some((_, k, t)) => (t.tour.tour.clone(), t.tour.trips[t.duties[k].start..t.duties[k].end].to_vec(), t.tour.number.clone()),
        None => (r.tour.clone(), x.sheet.clone(), x.number.clone()),
    };
    let sheet = Sheet { tour: &tour, number: &number, bus: String::new(), planned: &planned };
    let Some(i) = trip_of(&sheet, run).and_then(|px| planned.iter().position(|y| std::ptr::eq(y, px))) else { return false };
    if x.ai_now().is_none_or(|a| a.from > i) {
        return false;
    }
    if planned.iter().skip(i + 1).any(reportable) {
        if let Some(a) = c.together.claim_mut(r).and_then(|x| x.ai.last_mut()) {
            a.from = i + 1;
        }
    } else {
        not_left(c, r, guest);
        let _ = set_state(c, r, guest, ClaimState::Done);
    }
    true
}

/// Player `guest` takes his duty `r` back from the AI, at a trip's end: from its next trip with
/// passengers the AI has not begun on the company's clock (`now`, `clock::now`) - the AI drives
/// those before it. The trip he drives from (an index into its trips).
pub fn take_back(c: &mut Company, p: Option<&DayPlan>, r: &DutyRef, guest: u32, now: i64) -> Result<usize, &'static str> {
    let x = c.together.claim_of(r).filter(|x| x.guest == guest).ok_or(NOT_YOURS)?;
    let a = x.ai_now().ok_or(NO_AI)?.clone();
    let trips = trips_now(c, p, r);
    let back = (a.from..trips.len()).find(|&i| reportable(&trips[i]) && clock::moment(&r.date, trips[i].dep as i64) >= now).ok_or(AI_DONE)?;
    // (28 October 2026: free since he left it or handed it over - not when he took another duty
    // for that time meanwhile)
    if a.why != LeftWhy::Begun && p.filter(|p| p.date == r.date).is_some_and(|p| overlapping(c, p, r, Some((back, trips.len())), guest)) {
        return Err(OVERLAP);
    }
    // (the AI drove none of it yet: as if it never had it - nor did he leave it)
    if back == a.from {
        not_left(c, r, guest);
    } else if let Some(last) = c.together.claim_mut(r).and_then(|x| x.ai.last_mut()) {
        last.until = Some(back);
    }
    Ok(back)
}

/// (NexiconDK, Bus Company Online: relief at the stop) Player `guest` was not at his stop in time
/// to take the bus of his duty `r` over at its trip leaving at `dep` (minutes of the day): the AI
/// drove that trip (the map's world says so: `store::WorldTrip::kept`). It is the AI's part of
/// the duty, up to his next trip - the part it drove until he takes over goes on to there, or one
/// begins (`LeftWhy::Begun`: he did not leave it). The trip's place in the duty.
pub fn ai_kept(c: &mut Company, p: Option<&DayPlan>, r: &DutyRef, guest: u32, dep: i32, now_unix: u64, minute: i32) -> Result<usize, &'static str> {
    c.together.his_index(r, guest).filter(|&k| c.together.claims[k].state != ClaimState::Asked).ok_or(NOT_YOURS)?;
    let trips = trips_now(c, p, r);
    let i = trips.iter().position(|t| reportable(t) && (t.whole_dep() - dep).abs() <= 1).ok_or(NO_DUTY)?;
    // (his claim of that trip: the duty's, or a single trip of it - else his last)
    let k = c
        .together
        .claims
        .iter()
        .position(|x| x.guest == guest && same_duty(&x.duty, r) && x.state != ClaimState::Asked && x.his(i))
        .or_else(|| c.together.his_index(r, guest))
        .ok_or(NOT_YOURS)?;
    let x = &c.together.claims[k];
    if x.ai_drives(i) {
        return Ok(i);
    }
    // (NexiconDK: a single trip he took - its driver drove it: he takes the bus over for the next
    // trip of his part, or it was the last, and it is over - the company's again when he drove none)
    if let Some((a, b)) = x.part {
        if !x.his(i) || i != a {
            return Ok(i);
        }
        let next = (i + 1..b.min(trips.len())).find(|&k| reportable(&trips[k]));
        let none = part_state(c, p, r, guest, (a, b)).2;
        match next {
            Some(n) => c.together.claims[k].part = Some((n, b)),
            None if none => {
                c.together.claims.remove(k);
            }
            None => c.together.claims[k].state = ClaimState::Done,
        }
        return Ok(i);
    }
    let next = trips.iter().enumerate().skip(i + 1).find(|(_, t)| reportable(t)).map(|(k, _)| k);
    let claim = &mut c.together.claims[k];
    match claim.ai.last_mut() {
        Some(a) if a.until == Some(i) => a.until = next,
        _ => claim.ai.push(AiPart { from: i, until: next, why: LeftWhy::Begun, at: now_unix, minute, dropped: false }),
    }
    Ok(i)
}

/// (NexiconDK, Bus Company Online, 28 October 2026) Player `guest` handed his bus over to the AI
/// at a stop on the way through his duty `r` (the map's world says so: `store::WorldTrip::handed`):
/// on his trip leaving at `dep` (minutes of the day), at its terminus (`done`) or at a stop along
/// it. The AI drives the rest of the duty from the trip after it, or that trip itself - and he did
/// not leave it (`LeftWhy::HandedOver`: not in his figures; counted already, when his launcher said
/// his game ended before the world did, no longer). A single trip he took: what he did not drive
/// of it is its driver's again (nothing of it: the company's). Where the AI drives on from (an
/// index into its trips; None: he drove all of it).
#[allow(clippy::too_many_arguments)]
pub fn handed_over(c: &mut Company, p: Option<&DayPlan>, r: &DutyRef, guest: u32, dep: i32, done: bool, now_unix: u64, minute: i32) -> Result<Option<usize>, &'static str> {
    let trips = trips_now(c, p, r);
    let i = trips.iter().position(|t| reportable(t) && (t.whole_dep() - dep).abs() <= 1).ok_or(NO_DUTY)?;
    let k = c.together.claims.iter().position(|x| x.guest == guest && same_duty(&x.duty, r) && x.state != ClaimState::Asked && x.his(i)).ok_or(NOT_YOURS)?;
    let from = if done { (i + 1..trips.len()).find(|&j| reportable(&trips[j])) } else { Some(i) };
    // (counted as left before the world said so: not left)
    let x = &mut c.together.claims[k];
    let counted = std::mem::take(&mut x.left) || x.ai_now().is_some_and(|a| a.why.his());
    if counted {
        if let Some(g) = c.together.guest_mut(guest) {
            g.stats.left = g.stats.left.saturating_sub(1);
        }
    }
    let x = &mut c.together.claims[k];
    if let Some((a, b)) = x.part {
        match from.filter(|&f| f < b) {
            Some(f) if f <= a => {
                c.together.claims.remove(k);
            }
            Some(f) => {
                x.part = Some((a, f));
                x.state = ClaimState::Done;
            }
            None => x.state = ClaimState::Done,
        }
        return Ok(from);
    }
    let Some(from) = from else {
        x.state = ClaimState::Done;
        return Ok(None);
    };
    match x.ai.last_mut().filter(|a| a.until.is_none()) {
        Some(a) => {
            a.from = from;
            a.why = LeftWhy::HandedOver;
            a.dropped = false;
        }
        None => x.ai.push(AiPart { from, until: None, why: LeftWhy::HandedOver, at: now_unix, minute, dropped: false }),
    }
    Ok(Some(from))
}

/// The next trip of duty `r` the AI would hand back at (`take_back`), if it drives the duty now
/// and a trip of it is still to begin on the company's clock (`now`).
pub fn back_at(c: &Company, p: Option<&DayPlan>, r: &DutyRef, now: i64) -> Option<usize> {
    let a = c.together.claim_of(r)?.ai_now()?;
    let trips = trips_now(c, p, r);
    (a.from..trips.len()).find(|&i| reportable(&trips[i]) && clock::moment(&r.date, trips[i].dep as i64) >= now)
}

fn duty_ref(date: &str, line: &str, tour: &str, k: usize) -> DutyRef {
    DutyRef { date: date.to_string(), line: line.to_string(), tour: tour.to_string(), duty: k }
}

/// The AI drives a part of duty `k` of `line`/`tour` on `date`, the company's day: its player
/// left it. The day's close and its model drive that part with `ai_driver` (`ai_runs`).
pub fn ai_on(c: &Company, date: &str, line: &str, tour: &str, k: usize) -> bool {
    date == c.date && c.together.claim_of(&duty_ref(date, line, tour, k)).is_some_and(|x| !x.ai.is_empty() && !x.late && x.state != ClaimState::Asked)
}

/// Trip `i` of that duty (an index into its trips, `dep` its departure) is the AI's: in a part
/// its player left (the AI drives it, or it is dropped: `dropped_runs`), and not one he reported.
pub fn ai_runs(c: &Company, date: &str, line: &str, tour: &str, k: usize, i: usize, dep: i32) -> bool {
    let r = duty_ref(date, line, tour, k);
    ai_on(c, date, line, tour, k) && c.together.claim_of(&r).is_some_and(|x| x.ai_drives(i)) && !reported(c, &r).contains(&dep)
}

/// That trip is one nobody drives: in a part its player left that the company drops
/// (`AiPart::dropped`), and not one he reported.
pub fn dropped_runs(c: &Company, date: &str, line: &str, tour: &str, k: usize, i: usize, dep: i32) -> bool {
    let r = duty_ref(date, line, tour, k);
    ai_runs(c, date, line, tour, k, i, dep) && c.together.claim_of(&r).is_some_and(|x| x.ai.iter().any(|a| a.dropped && a.has(i)))
}

/// The departures of today's duties their players left that nobody drives (`dropped_runs`), for
/// the world (`plan::LiveTour::off`): (the timetable's line, its tour as the timetable names it,
/// the minute of the day).
pub fn dropped_departures(c: &Company, p: &DayPlan) -> Vec<(String, String, i32)> {
    let mut out = Vec::new();
    if p.date != c.date {
        return out;
    }
    for x in c.together.claims.iter().filter(|x| x.duty.date == c.date && !x.late && x.ai.iter().any(|a| a.dropped)) {
        let Ok((_, k, t)) = find(p, &x.duty) else { continue };
        let d = &t.duties[k];
        let deps = reported(c, &x.duty);
        for (i, y) in t.tour.trips[d.start..d.end].iter().enumerate() {
            if x.ai.iter().any(|a| a.dropped && a.has(i)) && !deps.contains(&y.whole_dep()) {
                let tour = if y.origin.trim().is_empty() { t.tour.tour.clone() } else { y.origin.clone() };
                out.push((t.tour.line.clone(), tour, y.whole_dep()));
            }
        }
    }
    out
}

/// The AI as the day's model has it drive a duty its player left: a driver of average skills (as
/// an agency's), under `plan::AGENCY` - nobody's employee, paid by nobody.
pub fn ai_driver() -> super::super::model::Employee {
    let mut e = super::super::plan::agency_driver();
    e.name = "AI driver".into();
    e
}
