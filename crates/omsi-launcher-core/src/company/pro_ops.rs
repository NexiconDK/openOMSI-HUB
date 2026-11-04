//! RosterFlow Pro, step 4 (NexiconDK, 4 November 2026): the daily operations ("Daglig drift"),
//! the calendar of every driver's days ("Kalender") and asking a driver for a duty - HASTUS's
//! daily dispatch, the way a Danish bus company's planner works the phone.
//!
//! - **The calendar** (`driver_day`, `open_duties`): a driver's day as the day's plan has it - the
//!   duties of the Vagter tab he drives (their numbers, when he signs on and off), a day off, ill,
//!   on holiday, a reserve's frame - and the minutes it pays by his employment
//!   (`pay::planned_minutes`): the hours of his week from Sunday. A day's duties without a driver
//!   are its "missing" ones.
//! - **Who could take a duty** (`candidates`): every driver, in the order an open duty is filled
//!   in (`plan::fill_tier`: a reserve within his frame, a free colleague, an hourly-paid driver, an
//!   agency driver booked for the day), whether he can (the working-time rules for every piece of
//!   it: `plan::can_take`) and what he costs (`Price`): nothing more within a reserve's frame or a
//!   permanent driver's week, overtime above his week (`econ_uk::OVERTIME_RATE`), an hourly-paid
//!   driver's hours (at least `pay::HOURLY_MIN_CALL` on a day called in), an agency driver's.
//! - **Asking** (`ask`): the drivers ticked are asked. An AI driver's answer (`ai_reply`) is
//!   decided when he is asked - from the duty's times against his rest and his other work, his
//!   hours this week against his contract, how content and how stressed he is, an hourly-paid
//!   driver's own hours of the day - and shows after a few seconds (`Ask::shown`): yes, no, "too
//!   early" (from when he can), "too late" (until when), a swap (he takes it if a colleague takes
//!   his own duty). A player of "Play together" answers himself (`answer`).
//! - **Given** (`give`): the duty is the driver's on that date (`Call`), whoever's row or roster
//!   had it - the day's plan runs it (`pro_turnus::on_day`); a swap gives the other duty to the
//!   colleague. A reserve given a duty from noon the day before is told (`plan::tell_reserves`).
//!
//! Kept in the company's planning (`Planning::ops`), at once; an older file has none.

use super::model::{Cents, Company, Employee, Employment};
use super::plan::{self, Cannot, DayPlan, Problem, Who};
use super::pro_turnus::{self as pt, ProDuty};
use super::rng::Rng;
use super::staff::{self, Block};
use super::{dates, econ_uk, pay};
use serde::{Deserialize, Serialize};

// --- what is kept --------------------------------------------------------------------------------

/// A duty given to a driver for one date (`give`): his that day, whoever's row or roster had it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Call {
    pub date: String,
    /// The duty's number (the Vagter tab's).
    pub n: u32,
    pub driver: u32,
}

/// Who is asked.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Asked {
    /// A driver of the staff (an AI driver: he answers by himself).
    Staff { id: u32 },
    /// A player of "Play together" (his number): he answers himself.
    Guest { no: u32 },
}

/// Why a driver says no.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NoWhy {
    /// A long week, worn out.
    Tired,
    /// He drives another duty then.
    Busy,
    /// Plans of his own.
    Plans,
    Ill,
    Holiday,
    /// Not allowed to drive the bus.
    Licence,
    /// His day would be too long.
    Long,
}

/// A driver's answer.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    /// Yes (`say`: which of the ways to say it).
    Yes { say: u8 },
    /// Yes, the duty is within his frame (a reserve: the frame is the company's).
    Frame,
    No { why: NoWhy, say: u8 },
    /// Too early: he can from `from` (minutes of the day).
    Early { from: i32, say: u8 },
    /// Too late: he can until `until`.
    Late { until: i32, say: u8 },
    /// He takes it if colleague `with` takes his own duty `theirs` that day.
    Swap { with: u32, theirs: u32, say: u8 },
    /// A player's own words (`yes`: he says he can).
    Text { text: String, yes: bool },
}

impl Reply {
    /// The answer says he takes it (and the duty can be given him).
    pub fn yes(&self) -> bool {
        matches!(self, Reply::Yes { .. } | Reply::Frame | Reply::Swap { .. } | Reply::Text { yes: true, .. })
    }

    /// Its kind: "yes", "no", "early", "late", "swap", "text" (the page's chip).
    pub fn key(&self) -> &'static str {
        match self {
            Reply::Yes { .. } | Reply::Frame => "yes",
            Reply::No { .. } => "no",
            Reply::Early { .. } => "early",
            Reply::Late { .. } => "late",
            Reply::Swap { .. } => "swap",
            Reply::Text { .. } => "text",
        }
    }
}

/// A driver asked for a duty.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Ask {
    pub id: u32,
    pub date: String,
    pub n: u32,
    /// The duty's signing on and off (minutes of the day).
    pub start: i32,
    pub end: i32,
    pub who: Asked,
    /// When he was asked (seconds since 1970, the launcher's clock), and how many seconds an AI
    /// driver takes to answer.
    pub at: u64,
    #[serde(default)]
    pub wait: u64,
    /// His answer: an AI driver's decided when he is asked (shown after `wait`), a player's when
    /// he gives it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<Reply>,
    /// The duty was given (to him or to another), or the question taken back.
    #[serde(default)]
    pub closed: bool,
}

impl Ask {
    /// His answer as it is known at `now` (seconds since 1970): an AI driver's after his seconds.
    pub fn shown(&self, now: u64) -> Option<&Reply> {
        self.reply.as_ref().filter(|_| now >= self.at.saturating_add(self.wait))
    }
}

/// The daily operations' own: the duties given for a date, the drivers asked.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Ops {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<Call>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub asks: Vec<Ask>,
    /// The last number given to a question.
    pub last: u32,
}

impl Ops {
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty() && self.asks.is_empty()
    }
}

/// Refusals (English: keys of the translations).
pub const NO_SUCH_ASK: &str = "That question is no longer open.";
pub const NOT_ASKED: &str = "You were not asked for that duty.";
pub const NOT_A_DRIVER: &str = "That is not a driver of the company.";
pub const NOBODY: &str = "Tick the drivers to ask first.";

// --- a day: its duties, who drives them ----------------------------------------------------------

/// The pieces of duty `d` in the day's plan (tour, duty of the tour), the tours a player drove
/// left out.
pub fn pieces_of(dp: &DayPlan, d: &ProDuty) -> Vec<(usize, usize)> {
    let same = |a: &str, b: &str| a.trim().eq_ignore_ascii_case(b.trim());
    d.pieces
        .iter()
        .filter_map(|(line, tour, k)| {
            let ti = dp.tours.iter().position(|t| same(&t.tour.line, line) && same(&t.tour.tour, tour))?;
            (*k < dp.tours[ti].duties.len() && !dp.tours[ti].by_player).then_some((ti, *k))
        })
        .collect()
}

/// A duty of a day without a driver (or a piece of it without).
#[derive(Clone, Debug, PartialEq)]
pub struct OpenDuty {
    pub n: u32,
    /// Signing on and off, the minutes it pays, its type.
    pub start: i32,
    pub end: i32,
    pub paid: i32,
    pub kind: super::pro_duties::Kind,
    /// Its pieces in the day's plan, and why the first one open is (ill, on holiday, nobody).
    pub pieces: Vec<(usize, usize)>,
    pub why: Option<Problem>,
    /// Some of its pieces have a driver.
    pub part: bool,
    /// The numbers of the lines its pieces run on.
    pub lines: Vec<String>,
}

/// The duties of the day's plan `dp` without a driver (`duties`: the day's, as the Vagter tab
/// numbers them), in the order they begin.
pub fn open_duties(dp: &DayPlan, duties: &[ProDuty]) -> Vec<OpenDuty> {
    let mut out: Vec<OpenDuty> = duties
        .iter()
        .filter_map(|d| {
            let pieces = pieces_of(dp, d);
            let open: Vec<&(usize, usize)> = pieces.iter().filter(|(ti, k)| dp.tours[*ti].duties[*k].who.is_none()).collect();
            let first = open.first()?;
            let why = open.iter().find_map(|(ti, k)| dp.tours[*ti].duties[*k].problem).or(dp.tours[first.0].duties[first.1].problem);
            let mut lines: Vec<String> = Vec::new();
            for (ti, _) in &pieces {
                if !lines.contains(&dp.tours[*ti].tour.number) {
                    lines.push(dp.tours[*ti].tour.number.clone());
                }
            }
            Some(OpenDuty { n: d.number, start: d.start, end: d.end, paid: d.paid, kind: d.kind, part: open.len() < pieces.len(), why, pieces, lines })
        })
        .collect();
    out.sort_by_key(|x| (x.start, x.n));
    out
}

/// What a driver's day is on the calendar.
#[derive(Clone, Debug, PartialEq)]
pub enum DayIs {
    /// He drives these duties (their numbers; 0 a piece of no duty of the Vagter tab), signing on
    /// and off; `frame` his frame that day besides.
    Duties { numbers: Vec<u32>, start: i32, end: i32, frame: Option<(i32, i32)> },
    /// A reserve's frame, no duty in it (yet).
    Frame { from: i32, to: i32 },
    Off,
    Ill,
    Holiday,
    /// Not employed that day (not yet, or gone).
    Away,
}

/// Piece `k` of tour `ti` as the day's close pays it (`day::close_day`): the bus's check before a
/// tour's first piece and after its last with it (`Prefs::pre_trip`, `post_trip`).
pub fn pay_piece(c: &Company, dp: &DayPlan, ti: usize, k: usize) -> (i32, i32) {
    let p = &c.planning.flow.prefs;
    let (t, d) = (&dp.tours[ti], &dp.tours[ti].duties[k]);
    let from = if d.start == 0 { d.from - p.pre_trip } else { d.from };
    let to = if d.end >= t.tour.trips.len() { d.to + p.post_trip } else { d.to };
    (from, to)
}

/// A driver's day: what it is and the minutes it pays (`pay::planned_minutes`).
#[derive(Clone, Debug, PartialEq)]
pub struct DriverDay {
    pub is: DayIs,
    pub minutes: f64,
}

/// Driver `e`'s day as the plan `dp` of its date has it (`duties`: the day's, as the Vagter tab
/// numbers them).
pub fn driver_day(c: &Company, e: &Employee, dp: &DayPlan, duties: &[ProDuty]) -> DriverDay {
    let date = dp.date.as_str();
    if !e.employed_on(date) {
        return DriverDay { is: DayIs::Away, minutes: 0.0 };
    }
    let mine = dp.duties_of(Who::Staff(e.id));
    let pieces: Vec<(i32, i32)> = mine.iter().map(|&(ti, k)| pay_piece(c, dp, ti, k)).collect();
    let minutes = pay::planned_minutes(c, e, date, &pieces);
    let ill = e.sick_until.as_deref().is_some_and(|u| dates::between(date, u) >= 0);
    let holiday = e.on_holiday(date).is_some();
    let note = pt::decided(c, e.id, date).and_then(|d| d.note);
    let frame = pt::frame_on(c, e, date).map(|f| (f.from, f.to));
    if mine.is_empty() {
        let is = if ill || note.as_deref() == Some("sick") {
            DayIs::Ill
        } else if holiday || note.as_deref() == Some("holiday") {
            DayIs::Holiday
        } else if let Some((from, to)) = frame {
            DayIs::Frame { from, to }
        } else {
            DayIs::Off
        };
        return DriverDay { is, minutes };
    }
    let mut numbers: Vec<u32> = Vec::new();
    for &(ti, k) in &mine {
        let t = &dp.tours[ti].tour;
        let n = duties.iter().find(|d| d.pieces.iter().any(|(l, tt, kk)| *kk == k && l.trim().eq_ignore_ascii_case(t.line.trim()) && tt.trim() == t.tour.trim())).map_or(0, |d| d.number);
        if !numbers.contains(&n) {
            numbers.push(n);
        }
    }
    let p = &c.planning.flow.prefs;
    // (a duty of the Vagter tab driven whole: its own sign-on and sign-off)
    let whole = |n: u32| duties.iter().find(|d| d.number == n);
    let (start, end) = match (numbers.as_slice(), numbers.first().and_then(|n| whole(*n))) {
        ([_], Some(d)) => (d.start, d.end),
        _ => (pieces.iter().map(|x| x.0).min().unwrap_or(0) - p.sign_on, pieces.iter().map(|x| x.1).max().unwrap_or(0) + p.sign_off),
    };
    DriverDay { is: DayIs::Duties { numbers, start, end, frame }, minutes }
}

/// The seven dates of the bus company's week `date` is in, Sunday to Saturday (`pay::WEEK_BEGINS`).
pub fn week_dates(date: &str) -> Vec<String> {
    let sunday = pt::sunday_of(date);
    (0..pt::DAYS as i64).map(|k| dates::add(&sunday, k)).collect()
}

/// The minutes of driver `e`'s week that begins on Sunday `sunday` (`day(date)`: his day's minutes
/// as planned, `driver_day`): in the company's own week its days gone as paid so far
/// (`Employee::week_paid`), the rest as planned.
pub fn week_minutes(c: &Company, e: &Employee, sunday: &str, day: &dyn Fn(&str) -> f64) -> f64 {
    let current = pt::sunday_of(&c.date) == sunday;
    let planned: f64 = week_dates(sunday).iter().filter(|d| !current || dates::between(&c.date, d) >= 0).map(|d| day(d)).sum();
    planned + if current { e.week_paid } else { 0.0 }
}

// --- who could take a duty, and what he costs ------------------------------------------------------

/// What a driver costs for a duty, besides what he is paid anyway.
#[derive(Clone, Debug, PartialEq)]
pub enum Price {
    /// Within his frame (a reserve): paid anyway.
    Frame,
    /// Within his contract week (a permanent driver under his hours): paid anyway by the week's
    /// guarantee.
    Within,
    /// Overtime: these minutes above his week, at `econ_uk::OVERTIME_RATE`.
    Overtime { minutes: i32, rate: f64 },
    /// An hourly-paid driver's hours (at least `pay::HOURLY_MIN_CALL` on a day called in).
    Hourly { minutes: i32 },
    /// A reserve outside his frame: these minutes on top.
    Extra { minutes: i32 },
    /// An agency driver booked for the day: his hours at the agency's price.
    Agency { minutes: i32 },
}

/// A driver as the daily operations show him for an open duty.
#[derive(Clone, Debug, PartialEq)]
pub struct Cand {
    pub id: u32,
    /// Where he comes in the order the duty is filled in (`plan::fill_tier`).
    pub tier: u8,
    /// No other duty that day.
    pub standby: bool,
    /// Whether he can take every piece of it: or why not (the first piece he cannot).
    pub fit: Result<(), Cannot>,
    pub price: Price,
    /// What he costs more (cents, the wages without the on-costs).
    pub cost: Cents,
    /// The minutes of his week (Sunday on) without the duty.
    pub week: f64,
}

/// The minutes of driver `e`'s contract week (None: the hourly paid, no week).
pub fn contract_minutes(c: &Company, e: &Employee) -> Option<f64> {
    match e.employment {
        Employment::Hourly => None,
        Employment::Reserve(f) => Some(f.week_minutes() as f64),
        _ => staff::contract_hours(c, e).map(|h| h * 60.0),
    }
}

/// What driver `e` costs for a duty paying `paid` minutes, his week at `week` minutes without it
/// (`tier` where he comes in the order; `worked`: he works that day already).
pub fn price_of(c: &Company, e: &Employee, tier: u8, paid: i32, week: f64, worked: bool) -> (Price, Cents) {
    let hour = pay::hour_wage(e);
    let rate = if econ_uk::mode(c.game_mode).overtime { econ_uk::OVERTIME_RATE } else { 1.0 };
    let full = pay::contract_week_minutes(c);
    let over = |minutes: f64, limit: f64| ((week + minutes - limit).max(0.0) - (week - limit).max(0.0)).round() as i32;
    let cents = |x: f64| x.round() as Cents;
    if super::agency::is_booked(e.id) {
        let m = paid.max(0);
        return (Price::Agency { minutes: m }, cents(plan::agency_hour(c) * m as f64 / 60.0));
    }
    if tier == plan::TIER_RESERVE {
        return (Price::Frame, 0);
    }
    match e.employment {
        Employment::Hourly => {
            let m = if worked { paid } else { paid.max(pay::HOURLY_MIN_CALL) };
            let o = over(m as f64, full);
            (Price::Hourly { minutes: m }, cents(hour * ((m - o) as f64 + o as f64 * rate) / 60.0))
        }
        Employment::Reserve(f) => {
            let o = over(paid as f64, f.week_minutes() as f64);
            (Price::Extra { minutes: paid }, cents(hour * ((paid - o) as f64 + o as f64 * rate) / 60.0))
        }
        _ => {
            let week_of = contract_minutes(c, e).unwrap_or(full);
            match over(paid as f64, week_of) {
                0 => (Price::Within, 0),
                o => (Price::Overtime { minutes: o, rate }, cents(hour * o as f64 * rate / 60.0)),
            }
        }
    }
}

/// The duty's pieces as one piece of work (its first's beginning to its last's end).
fn span_block(dp: &DayPlan, pieces: &[(usize, usize)]) -> Block {
    let from = pieces.iter().map(|&(ti, k)| dp.tours[ti].duties[k].from).min().unwrap_or(0);
    let to = pieces.iter().map(|&(ti, k)| dp.tours[ti].duties[k].to).max().unwrap_or(0);
    Block { key: "pro-duty".into(), from, to, ..Default::default() }
}

/// Whether `e` can take every piece of an open duty beside his other work of the day (`plan::
/// can_take` piece after piece), or why not.
pub fn fit_of(c: &Company, dp: &DayPlan, e: &Employee, pieces: &[(usize, usize)]) -> Result<(), Cannot> {
    // (his rota's exception says him ill or on holiday that day)
    match pt::decided(c, e.id, &dp.date).and_then(|d| d.note).as_deref() {
        Some("sick") => return Err(Cannot::Ill { until: dp.date.clone() }),
        Some("holiday") => return Err(Cannot::Holiday { until: dp.date.clone() }),
        _ => {}
    }
    let mut work: Vec<Block> = dp.duties_of(Who::Staff(e.id)).into_iter().filter(|x| !pieces.contains(x)).map(|(ti, k)| plan::duty_block(dp, ti, k)).collect();
    let mut ps = pieces.to_vec();
    ps.sort_by_key(|&(ti, k)| dp.tours[ti].duties[k].from);
    for (ti, k) in ps {
        // (a piece he drives already is his)
        if dp.tours[ti].duties[k].who == Some(Who::Staff(e.id)) {
            continue;
        }
        let b = plan::duty_block(dp, ti, k);
        plan::can_take(e, &dp.date, dp.today, &dp.tours[ti].bus_of(c), &work, &b)?;
        work.push(b);
    }
    Ok(())
}

/// Every driver for open duty `d` of the day's plan `dp` - the company's and the agency drivers
/// booked for the day -, those who can first in the order an open duty is filled in
/// (`plan::fill_tier`), the standby drivers first within it, then the cheapest; then those who
/// cannot, each with why. `week(id)`: the minutes of his week without the duty.
pub fn candidates(c: &Company, dp: &DayPlan, d: &OpenDuty, week: &dyn Fn(u32) -> f64) -> Vec<Cand> {
    let span = span_block(dp, &d.pieces);
    let mut out: Vec<(String, Cand)> = c
        .staff
        .iter()
        .filter(|e| staff::drives(e))
        .chain(super::agency::drivers_on(c, &dp.date))
        .map(|e| {
            let others: Vec<(usize, usize)> = dp.duties_of(Who::Staff(e.id)).into_iter().filter(|x| !d.pieces.contains(x)).collect();
            let fit = fit_of(c, dp, e, &d.pieces);
            let tier = plan::fill_tier(c, e, &dp.date, &span);
            let w = week(e.id);
            let (price, cost) = price_of(c, e, tier, d.paid, w, !others.is_empty());
            (e.name.clone(), Cand { id: e.id, tier, standby: others.is_empty(), fit, price, cost, week: w })
        })
        .collect();
    out.sort_by(|a, b| {
        let rank = |x: &Cand| match &x.fit {
            Ok(()) => (0, x.tier, !x.standby, x.cost),
            Err(_) => (1, 0, false, 0),
        };
        rank(&a.1).cmp(&rank(&b.1)).then(a.0.cmp(&b.0))
    });
    out.into_iter().map(|x| x.1).collect()
}

/// What an agency driver sent on the day costs for the duty (cents): the agency's price for its
/// pieces (`plan::agency_cost`).
pub fn agency_on_day(c: &Company, dp: &DayPlan, d: &OpenDuty) -> Cents {
    d.pieces.iter().filter(|&&(ti, k)| dp.tours[ti].duties[k].who.is_none()).map(|&(ti, k)| plan::agency_cost(c, dp, ti, k)).sum()
}

// --- asking ----------------------------------------------------------------------------------------

/// What a driver's answer is decided from (the page works it out of the day's plan).
#[derive(Clone, Debug, PartialEq)]
pub struct Situation {
    /// Whether he can take it (`fit_of`), or why not.
    pub fit: Result<(), Cannot>,
    /// The duty is within his frame (a reserve's: the frame is the company's).
    pub in_frame: bool,
    /// The minutes of his week without the duty, and of his contract week (None: hourly paid).
    pub week: f64,
    pub contract: Option<f64>,
    /// His own duty that day (its number, signing on and off), and a free colleague who could
    /// take it (a swap).
    pub own: Option<(u32, i32, i32)>,
    pub swap_with: Option<u32>,
    /// From when, or until when, he could work (his rest to his other work, his own hours).
    pub can_from: Option<i32>,
    pub can_until: Option<i32>,
}

/// An hourly-paid driver's own hours of the day (from, to: minutes of the day), as he says them
/// ("mostly in the afternoon"): by his number, the same every day.
pub fn own_hours(e: &Employee) -> Option<(i32, i32, &'static str)> {
    (e.employment == Employment::Hourly).then(|| match e.id % 3 {
        0 => (5 * 60, 14 * 60, "can weekdays before 14:00"),
        1 => (11 * 60, 20 * 60, "can mostly in the afternoon"),
        _ => (14 * 60, 24 * 60, "can mostly in the evening"),
    })
}

/// When `e` could work around his other pieces of the day `dp` (the rest, `staff::REST`, to the
/// one before and the one after the duty `from..to`): from when, until when.
pub fn could(dp: &DayPlan, e: &Employee, skip: &[(usize, usize)], from: i32, to: i32) -> (Option<i32>, Option<i32>) {
    let work: Vec<(i32, i32)> = dp.duties_of(Who::Staff(e.id)).into_iter().filter(|x| !skip.contains(x)).map(|(ti, k)| (dp.tours[ti].duties[k].from, dp.tours[ti].duties[k].to)).collect();
    let before = work.iter().filter(|w| w.1 <= from + staff::REST).map(|w| w.1).max();
    let after = work.iter().filter(|w| w.0 + staff::REST >= to).map(|w| w.0).min();
    let up5 = |m: i32| (m + 4).div_euclid(5) * 5;
    let night = plan::free_from(e, dp.today).filter(|f| *f > from);
    let from_ = before.map(|b| up5(b + staff::REST)).filter(|f| *f > from).or(night.map(up5));
    let until = after.map(|a| (a - staff::REST).div_euclid(5) * 5).filter(|u| *u < to);
    (from_, until)
}

/// The ways to say each answer (English: keys of the translations; `%{time}` a time, `%{name}` a
/// colleague, `%{own}` his own duty).
pub const SAY_YES: [&str; 4] = ["Yes, that suits me fine.", "Yes, I can take it.", "Fine by me, put me down for it.", "Yes - I'll be there."];
pub const SAY_FRAME: &str = "Yes, it is within my frame.";
pub const SAY_TIRED: [&str; 3] = ["I've had a long week, can we do it another time?", "Sorry, I'm worn out this week.", "I need my days off this week, sorry."];
pub const SAY_BUSY: [&str; 2] = ["I'm driving %{own} then myself.", "I already have %{own} that day, sorry."];
pub const SAY_PLANS: [&str; 3] = ["Sorry, I have plans that day.", "That day doesn't work for me, sorry.", "No, I can't that day."];
pub const SAY_ILL: &str = "I'm still ill, sorry.";
pub const SAY_HOLIDAY: &str = "I'm on holiday then.";
pub const SAY_LICENCE: &str = "I'm not allowed to drive that bus.";
pub const SAY_LONG: &str = "That would make my day too long.";
pub const SAY_EARLY: [&str; 3] = ["That is too early for me, I can only from %{time}.", "Too early - I could from %{time}.", "I can't be there that early. From %{time} I can."];
pub const SAY_LATE: [&str; 3] = ["That is too late for me, I can until %{time}.", "Too late for me - I have to be off by %{time}.", "I can do it, but only until %{time}."];
pub const SAY_SWAP: [&str; 2] = ["I can, if %{name} takes my %{own}.", "Yes, if we swap: %{name} takes my %{own}?"];

/// The words of an answer: its English template (a key of the translations) and what fills it
/// (`name`: a colleague's name).
pub fn words(r: &Reply, name: &dyn Fn(u32) -> String) -> (String, Vec<(&'static str, String)>) {
    let pick = |list: &[&str], say: u8| list[say as usize % list.len()].to_string();
    let time = |m: i32| format!("{:02}:{:02}", m.rem_euclid(1440) / 60, m.rem_euclid(60));
    match r {
        Reply::Yes { say } => (pick(&SAY_YES, *say), Vec::new()),
        Reply::Frame => (SAY_FRAME.into(), Vec::new()),
        Reply::No { why, say } => match why {
            NoWhy::Tired => (pick(&SAY_TIRED, *say), Vec::new()),
            NoWhy::Busy => (pick(&SAY_BUSY, *say), Vec::new()),
            NoWhy::Plans => (pick(&SAY_PLANS, *say), Vec::new()),
            NoWhy::Ill => (SAY_ILL.into(), Vec::new()),
            NoWhy::Holiday => (SAY_HOLIDAY.into(), Vec::new()),
            NoWhy::Licence => (SAY_LICENCE.into(), Vec::new()),
            NoWhy::Long => (SAY_LONG.into(), Vec::new()),
        },
        Reply::Early { from, say } => (pick(&SAY_EARLY, *say), vec![("time", time(*from))]),
        Reply::Late { until, say } => (pick(&SAY_LATE, *say), vec![("time", time(*until))]),
        Reply::Swap { with, theirs, say } => (pick(&SAY_SWAP, *say), vec![("name", name(*with)), ("own", theirs.to_string())]),
        Reply::Text { text, .. } => (text.clone(), Vec::new()),
    }
}

/// An AI driver's answer to being asked for duty `n` of `date` (signing on `start`, off `end`),
/// in a human way: the same for the same company, day, duty and driver (`Rng`), from his
/// situation `s` - a reserve within his frame says yes; one who cannot says why (ill, on holiday,
/// his licence, a day too long, too early or too late for his rest, another duty then - or a swap
/// when a colleague could take that one); one who can: an hourly-paid driver too early or too late
/// for his own hours says so, a very early or late duty is sometimes too early or too late for
/// anyone, and then yes or no by how content and how stressed he is and his hours this week
/// against his contract (over it: tired), the hourly paid glad of the hours.
pub fn ai_reply(c: &Company, e: &Employee, date: &str, n: u32, start: i32, end: i32, s: &Situation) -> Reply {
    let mut rng = Rng::of(&[&c.id, date, "pro-ask"], ((n as i64) << 24) | e.id as i64);
    let say = rng.int(0, 11) as u8;
    if s.in_frame && s.fit.is_ok() {
        return Reply::Frame;
    }
    let no = |why: NoWhy| Reply::No { why, say };
    match &s.fit {
        Err(Cannot::Ill { .. }) => return no(NoWhy::Ill),
        Err(Cannot::Holiday { .. }) => return no(NoWhy::Holiday),
        Err(Cannot::NotDriver | Cannot::NotEmployed) => return no(NoWhy::Plans),
        Err(Cannot::WeekDone { .. }) => return no(NoWhy::Tired),
        Err(Cannot::Lacks(_)) => return no(NoWhy::Licence),
        Err(Cannot::TooLong { .. }) => return no(NoWhy::Long),
        Err(Cannot::OnDuty { .. }) => {
            return match (s.own, s.swap_with) {
                (Some(own), Some(with)) if rng.chance(0.7) => Reply::Swap { with, theirs: own.0, say },
                _ => no(NoWhy::Busy),
            };
        }
        Err(Cannot::Rest { .. } | Cannot::NightRest { .. }) => {
            return match (s.can_from, s.can_until) {
                (Some(from), _) => Reply::Early { from, say },
                (None, Some(until)) => Reply::Late { until, say },
                _ => no(NoWhy::Busy),
            };
        }
        Ok(()) => {}
    }
    // (an hourly-paid driver's own hours of the day)
    if let Some((from, until, _)) = own_hours(e) {
        if start < from {
            return Reply::Early { from, say };
        }
        if end > until {
            return Reply::Late { until, say };
        }
    }
    // (very early or very late: too early or too late for some)
    if start < 5 * 60 + 30 && rng.chance(0.25) {
        return Reply::Early { from: start + 30 * rng.int(2, 5) as i32, say };
    }
    if end > 23 * 60 && rng.chance(0.25) {
        return Reply::Late { until: end - 30 * rng.int(2, 4) as i32, say };
    }
    let paid = (end - start).max(0) as f64;
    let over = s.contract.is_some_and(|k| s.week + paid > k + 1.0);
    let mut p = 0.62 + (e.satisfaction - 50.0) / 160.0 - e.stress / 260.0;
    if over {
        p -= 0.35;
    }
    if e.employment == Employment::Hourly {
        p += 0.25 * (1.0 - (s.week / pay::contract_week_minutes(c)).min(1.0));
    }
    if rng.chance(p.clamp(0.08, 0.95)) {
        Reply::Yes { say }
    } else if over || e.stress > 60.0 {
        no(NoWhy::Tired)
    } else {
        no(NoWhy::Plans)
    }
}

/// The open questions for duty `n` of `date`.
pub fn asks_of<'a>(c: &'a Company, date: &'a str, n: u32) -> impl Iterator<Item = &'a Ask> + 'a {
    c.planning.ops.asks.iter().filter(move |a| a.date == date && a.n == n && !a.closed)
}

/// The template of the notice to a player asked for a duty.
pub const ASKED_NOTICE: &str = "The control room asks: can you drive duty %{n} on %{date}, signing on %{time}? Answer on your page.";

/// Ask the drivers `who` for duty `n` of `date` (signing on `start`, off `end`) at `now` (seconds
/// since 1970): each with his answer (an AI driver's, decided now and shown after a few seconds;
/// None a player's, to come - he is told). A driver asked already is asked again. Their numbers.
pub fn ask(c: &mut Company, date: &str, n: u32, start: i32, end: i32, who: Vec<(Asked, Option<Reply>)>, now: u64) -> Result<Vec<u32>, &'static str> {
    if who.is_empty() {
        return Err(NOBODY);
    }
    let mut ids = Vec::new();
    for (w, reply) in who {
        if let Asked::Staff { id } = w {
            if c.employee(id).is_none_or(|e| !staff::drives(e)) {
                return Err(NOT_A_DRIVER);
            }
        }
        c.planning.ops.asks.retain(|a| !(a.date == date && a.n == n && a.who == w));
        c.planning.ops.last += 1;
        let id = c.planning.ops.last;
        let wait = match &reply {
            Some(Reply::Frame) => 1,
            Some(_) => Rng::of(&[&c.id, date, "pro-wait"], ((n as i64) << 24) | id as i64).int(3, 14) as u64,
            None => 0,
        };
        if let Asked::Guest { no } = w {
            let time = format!("{:02}:{:02}", start.rem_euclid(1440) / 60, start.rem_euclid(60));
            super::crew::notify(c, Some(Who::Guest(no)), super::crew::Priority::High, ASKED_NOTICE, &[("n", n.to_string()), ("date", date.to_string()), ("time", time)], Some("phone"));
        }
        c.planning.ops.asks.push(Ask { id, date: date.to_string(), n, start, end, who: w, at: now, wait, reply, closed: false });
        ids.push(id);
    }
    Ok(ids)
}

/// Player `no` answers question `id` (`reply`: what he says), at `now`.
pub fn answer(c: &mut Company, id: u32, no: u32, reply: Reply, now: u64) -> Result<(), &'static str> {
    let a = c.planning.ops.asks.iter_mut().find(|a| a.id == id && !a.closed).ok_or(NO_SUCH_ASK)?;
    if a.who != (Asked::Guest { no }) {
        return Err(NOT_ASKED);
    }
    let reply = match reply {
        Reply::Text { text, yes } => Reply::Text { text: text.chars().filter(|ch| !ch.is_control()).take(140).collect::<String>().trim().to_string(), yes },
        r => r,
    };
    a.at = a.at.min(now);
    a.wait = 0;
    a.reply = Some(reply);
    Ok(())
}

/// The questions a player has open: asked of him, not answered, not closed.
pub fn asked_of(c: &Company, no: u32) -> Vec<&Ask> {
    c.planning.ops.asks.iter().filter(|a| a.who == (Asked::Guest { no }) && !a.closed && a.reply.is_none()).collect()
}

/// Duty `n` of `date` given to driver `driver` for that day (whoever's row or roster had it): the
/// day's plan runs it. `swap`: and his own duty `theirs` that day to colleague `with` (a swap he
/// asked for). The questions for it closed.
pub fn give(c: &mut Company, date: &str, n: u32, driver: u32, swap: Option<(u32, u32)>) -> Result<(), &'static str> {
    if c.employee(driver).is_none_or(|e| !staff::drives(e)) {
        return Err(NOT_A_DRIVER);
    }
    let set = |c: &mut Company, n: u32, driver: u32| {
        c.planning.ops.calls.retain(|x| !(x.date == date && x.n == n));
        c.planning.ops.calls.push(Call { date: date.to_string(), n, driver });
        for a in c.planning.ops.asks.iter_mut().filter(|a| a.date == date && a.n == n) {
            a.closed = true;
        }
    };
    set(c, n, driver);
    if let Some((with, theirs)) = swap {
        if c.employee(with).is_none_or(|e| !staff::drives(e)) {
            return Err(NOT_A_DRIVER);
        }
        set(c, theirs, with);
    }
    Ok(())
}

/// The questions for duty `n` of `date` closed (the duty given another way: to a player of "Play
/// together", `together::book::give`).
pub fn close(c: &mut Company, date: &str, n: u32) {
    for a in c.planning.ops.asks.iter_mut().filter(|a| a.date == date && a.n == n) {
        a.closed = true;
    }
}

/// The questions for duty `n` of `date` taken back, and the duty given for that day no longer
/// (it runs as the row or the roster has it again). Whether anything changed.
pub fn withdraw(c: &mut Company, date: &str, n: u32) -> bool {
    let before = c.planning.ops.clone();
    c.planning.ops.calls.retain(|x| !(x.date == date && x.n == n));
    for a in c.planning.ops.asks.iter_mut().filter(|a| a.date == date && a.n == n) {
        a.closed = true;
    }
    before != c.planning.ops
}

/// The duties given for `date` (`pro_turnus::on_day` runs them), RosterFlow Pro chosen.
pub fn calls_on<'a>(c: &'a Company, date: &'a str) -> impl Iterator<Item = &'a Call> + 'a {
    let on = super::pro_duties::on(c);
    c.planning.ops.calls.iter().filter(move |x| on && x.date == date)
}

/// The days gone dropped (before the company's day), and the drivers gone.
pub fn tidy(c: &mut Company) {
    let today = c.date.clone();
    let drivers: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    c.planning.ops.calls.retain(|x| dates::between(&today, &x.date) >= 0 && drivers.contains(&x.driver));
    c.planning.ops.asks.retain(|a| dates::between(&today, &a.date) >= 0 && !matches!(a.who, Asked::Staff { id } if !drivers.contains(&id)));
}

/// Every English text this module hands the pages besides its literals.
pub fn texts() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = vec![SAY_FRAME, SAY_ILL, SAY_HOLIDAY, SAY_LICENCE, SAY_LONG, NO_SUCH_ASK, NOT_ASKED, NOT_A_DRIVER, NOBODY, ASKED_NOTICE];
    for list in [&SAY_YES[..], &SAY_TIRED, &SAY_BUSY, &SAY_PLANS, &SAY_EARLY, &SAY_LATE, &SAY_SWAP] {
        v.extend(list.iter().copied());
    }
    v.extend(["can weekdays before 14:00", "can mostly in the afternoon", "can mostly in the evening"]);
    v
}

#[cfg(test)]
#[path = "pro_ops_tests.rs"]
mod tests;
