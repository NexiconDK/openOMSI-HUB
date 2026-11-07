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
use super::pro_cut::PieceKey;
use super::plan::{self, Cannot, DayPlan, Problem, Who};
use super::pro_turnus::{self as pt, ProDuty};
use super::rng::Rng;
use super::staff::{self, Block};
use super::{dates, econ_uk, pay};
use serde::{Deserialize, Serialize};

// --- what is kept --------------------------------------------------------------------------------

/// A duty given to a driver for one date (`give`): his that day, whoever's row or roster had it.
/// (A part of a cut duty: `part` its index in the duty's split, `pieces` its pieces - `pro_cut`.)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Call {
    pub date: String,
    /// The duty's number (the Vagter tab's).
    pub n: u32,
    pub driver: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<u8>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pieces: Vec<PieceKey>,
    /// (NexiconDK, 5 November 2026: round B) Its overtime saved as time off in lieu, not paid
    /// (`set_toil`, `pay::day`).
    #[serde(default, skip_serializing_if = "is_false")]
    pub toil: bool,
}

fn is_false(x: &bool) -> bool {
    !*x
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
    /// Irritated: he said no once today already (`Mood::Irritated`).
    Annoyed,
    /// Angry: he said no twice today already (`Mood::Angry`).
    Angry,
    /// (round B) His day off: a wish granted, time off in lieu (`model::FreeDay`).
    DayOff,
    /// (Round F) An hourly-paid driver who cannot that day at all (round H: as he said, `pro_avail`).
    NotThatDay,
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
    /// No answer: he did not pick up (shown as "no answer" after `NO_ANSWER_AFTER`).
    Silent,
    /// Yes, grudgingly: irritated, he said no once today already (`Mood::Irritated`).
    Grudging { say: u8 },
}

impl Reply {
    /// The answer says he takes it (and the duty can be given him).
    pub fn yes(&self) -> bool {
        matches!(self, Reply::Yes { .. } | Reply::Grudging { .. } | Reply::Frame | Reply::Swap { .. } | Reply::Text { yes: true, .. })
    }

    /// Its kind: "yes", "no", "early", "late", "swap", "text" (the page's chip).
    pub fn key(&self) -> &'static str {
        match self {
            Reply::Yes { .. } | Reply::Grudging { .. } | Reply::Frame => "yes",
            Reply::No { .. } => "no",
            Reply::Early { .. } => "early",
            Reply::Late { .. } => "late",
            Reply::Swap { .. } => "swap",
            Reply::Text { .. } => "text",
            Reply::Silent => "silent",
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
    /// The part of a cut duty he is asked for (`pro_cut`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<u8>,
    /// When his answer comes, on the company's clock (`clock::now`; None: an older question, by
    /// the launcher's seconds `at` + `wait`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<i64>,
    /// When it was closed, on the company's clock (`clock::now`): an answer due after it was never
    /// given (the duty given another, the question taken back).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_at: Option<i64>,
    /// (Round G) The planner has seen his answer: opened the duty's box after it came. The mark
    /// on the day's "missing" cell counts only answers not seen (`mark_of`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub seen: bool,
}

impl Ask {
    /// His answer as it is known at `now` (seconds since 1970): an AI driver's after his seconds.
    pub fn shown(&self, now: u64) -> Option<&Reply> {
        self.reply.as_ref().filter(|_| self.due.is_none() && now >= self.at.saturating_add(self.wait))
    }

    /// His answer as it is known at the company's minute `game` (`clock::now`) and the launcher's
    /// second `now`.
    pub fn shown_at(&self, game: i64, now: u64) -> Option<&Reply> {
        match self.due {
            Some(d) => self.reply.as_ref().filter(|_| game >= d),
            None => self.shown(now),
        }
    }

    /// Closed at the company's minute `game` (the first time: an answer due after it never came).
    pub fn shut(&mut self, game: i64) {
        if !self.closed {
            self.closed = true;
            self.closed_at = Some(game);
        }
    }

    /// A real refusal known at the company's minute `game`: an AI driver's no - tired, plans of
    /// his own, irritated, angry (not "too early", "too late", "no answer", nor a no for what he
    /// cannot: ill, on holiday, his licence, busy, a day too long) - come before the question was
    /// closed. When he said it.
    pub fn refused_at(&self, game: i64) -> Option<i64> {
        let due = self.due?;
        let real = matches!(self.reply, Some(Reply::No { why: NoWhy::Tired | NoWhy::Plans | NoWhy::Annoyed | NoWhy::Angry, .. }));
        (real && due <= game && self.closed_at.is_none_or(|x| due <= x) && self.of_staff().is_some()).then_some(due)
    }

    /// Asked of staff driver `id`.
    pub fn of_staff(&self) -> Option<u32> {
        match self.who {
            Asked::Staff { id } => Some(id),
            Asked::Guest { .. } => None,
        }
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
    /// The duties cut for their date (`pro_cut`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub splits: Vec<super::pro_cut::Split>,
    /// The refusals of questions asked again since (their question replaced): `refusals`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub noes: Vec<Refusal>,
    /// The drivers who called in sick on the company's day (`pro_sick`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sick: Vec<super::pro_sick::SickCall>,
    /// The extra duties given this month and the last (`pro_rest`: the fair share), and who
    /// grumbled of none lately.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extras: Vec<super::pro_rest::Extra>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub grumbled: Vec<super::pro_rest::Extra>,
    /// The owner's fine for a breach of the rest rules off the server (cents; `pro_rest::fine`).
    #[serde(skip_serializing_if = "is_zero")]
    pub rest_fine: Cents,
    /// (NexiconDK, 5 November 2026: round B) The drivers' wishes (`pro_wish`), the last number
    /// given one, and the company's day they were last drawn for.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub wishes: Vec<super::pro_wish::Wish>,
    #[serde(skip_serializing_if = "is_none_u32")]
    pub last_wish: u32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub wished: String,
    /// Who asked for their time off in lieu lately (`pro_toil::daily`), and whether overtime is
    /// saved as time off in lieu unless the player chooses otherwise (the company's setting).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub toil_asked: Vec<super::pro_rest::Extra>,
    #[serde(skip_serializing_if = "is_false")]
    pub toil_default: bool,
    /// (NexiconDK, 5 November 2026: round C) The swaps of two drivers at once, to undo before
    /// their day begins (`pro_swap`), and the last number given one.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub swaps: Vec<super::pro_swap::Swap>,
    #[serde(skip_serializing_if = "is_none_u32")]
    pub last_swap: u32,
    /// (NexiconDK, 5 November 2026: round E) The swaps the drivers propose themselves
    /// (`pro_trade`), the last number given one, and the company's day they were last drawn for.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub trades: Vec<super::pro_trade::Trade>,
    #[serde(skip_serializing_if = "is_none_u32")]
    pub last_trade: u32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub traded: String,
    /// The weeks counted for the Monday report, and the reports made (`pro_report`).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tallies: Vec<super::pro_report::Tally>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reports: Vec<super::pro_report::Report>,
    /// (NexiconDK, 5 November 2026: round H) When the hourly paid can work, a day at a time
    /// (`pro_avail`), and the company's day the AI drivers last filled theirs in.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub avail: Vec<super::pro_avail::Avail>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub avail_day: String,
    /// (NexiconDK, 5 November 2026: round I) The bus dispatch's choices of a bus for a block on a
    /// date (`pro_buses`), and the buses that fell out of the day (no start, the workshop) told.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub buses: Vec<super::pro_buses::BusPick>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub downs: Vec<super::pro_buses::BusDown>,
}

fn is_none_u32(x: &u32) -> bool {
    *x == 0
}

fn is_zero(x: &Cents) -> bool {
    *x == 0
}

impl Ops {
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty() && self.asks.is_empty() && self.splits.is_empty() && self.noes.is_empty() && self.sick.is_empty() && self.extras.is_empty() && self.grumbled.is_empty() && self.rest_fine == 0 && self.wishes.is_empty() && self.last_wish == 0 && self.wished.is_empty() && self.toil_asked.is_empty() && !self.toil_default && self.swaps.is_empty() && self.last_swap == 0 && self.trades.is_empty() && self.last_trade == 0 && self.traded.is_empty() && self.tallies.is_empty() && self.reports.is_empty() && self.avail.is_empty() && self.avail_day.is_empty() && self.buses.is_empty() && self.downs.is_empty()
    }
}

/// A driver's no, kept when his question was asked again (`ask_part`): `at` when he said it, on
/// the company's clock; `date` the duty's date (an older file has none: not counted).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub driver: u32,
    pub at: i64,
    #[serde(default)]
    pub date: String,
}

/// Refusals (English: keys of the translations).
pub const NO_SUCH_ASK: &str = "That question is no longer open.";
pub const NOT_ASKED: &str = "You were not asked for that duty.";
pub const NOT_A_DRIVER: &str = "That is not a driver of the company.";
pub const NOBODY: &str = "Tick the drivers to ask first.";
pub const NEEDS_YES: &str = "Ask him first: the duty can be given only to a driver who said yes.";
/// How long an unanswered question waits before it shows "no answer" (minutes of the company's
/// clock).
pub const NO_ANSWER_AFTER: i64 = 120;

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
    /// and off; `frame` his frame that day besides. `labels`: how the calendar names them - a part
    /// of a duty cut for the date by its letter ("1002C", `pro_cut`).
    Duties { numbers: Vec<u32>, start: i32, end: i32, frame: Option<(i32, i32)>, labels: Vec<String> },
    /// A reserve's frame, no duty in it (yet).
    Frame { from: i32, to: i32 },
    Off,
    Ill,
    Holiday,
    /// (round B) A day of time off in lieu (`model::FreeDay`; a wish's day off granted is `Off`).
    Toil,
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
    let ill = e.ill_on(date);
    let holiday = e.on_holiday(date).is_some();
    let note = pt::decided(c, e.id, date).and_then(|d| d.note);
    let frame = pt::frame_on(c, e, date).map(|f| (f.from, f.to));
    if mine.is_empty() {
        let is = if ill || note.as_deref() == Some("sick") {
            DayIs::Ill
        } else if holiday || note.as_deref() == Some("holiday") {
            DayIs::Holiday
        } else if pay::toil_day(e, date).is_some() {
            DayIs::Toil
        } else if e.free_on(date).is_some() {
            DayIs::Off
        } else if let Some((from, to)) = frame {
            DayIs::Frame { from, to }
        } else {
            DayIs::Off
        };
        return DriverDay { is, minutes };
    }
    let mut numbers: Vec<u32> = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    for &(ti, k) in &mine {
        let t = &dp.tours[ti].tour;
        let d = duties.iter().find(|d| d.pieces.iter().any(|(l, tt, kk)| *kk == k && l.trim().eq_ignore_ascii_case(t.line.trim()) && tt.trim() == t.tour.trim()));
        let n = d.map_or(0, |d| d.number);
        if !numbers.contains(&n) {
            numbers.push(n);
        }
        // (round K: a duty by its own name)
        let label = super::pro_cut::label_of(c, dp, ti, k).unwrap_or_else(|| match d {
            Some(d) => d.shown(),
            None => "+".to_string(),
        });
        if !labels.contains(&label) {
            labels.push(label);
        }
    }
    let p = &c.planning.flow.prefs;
    // (a duty of the Vagter tab driven whole: its own sign-on and sign-off)
    let whole = |n: u32| duties.iter().find(|d| d.number == n);
    let (start, end) = match (numbers.as_slice(), numbers.first().and_then(|n| whole(*n))) {
        ([_], Some(d)) => (d.start, d.end),
        _ => (pieces.iter().map(|x| x.0).min().unwrap_or(0) - p.sign_on, pieces.iter().map(|x| x.1).max().unwrap_or(0) + p.sign_off),
    };
    DriverDay { is: DayIs::Duties { numbers, start, end, frame, labels }, minutes }
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
    /// (round D) How tired he is that day with the duty (`pro_tired`).
    pub tired: super::pro_tired::Level,
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
            NoWhy::Annoyed => (pick(&SAY_ANNOYED, *say), Vec::new()),
            NoWhy::Angry => (pick(&SAY_ANGRY, *say), Vec::new()),
            NoWhy::DayOff => (SAY_DAY_OFF.into(), Vec::new()),
            NoWhy::NotThatDay => (SAY_NOT_THAT_DAY.into(), Vec::new()),
        },
        Reply::Grudging { say } => (pick(&SAY_GRUDGING, *say), Vec::new()),
        Reply::Early { from, say } => (pick(&SAY_EARLY, *say), vec![("time", time(*from))]),
        Reply::Late { until, say } => (pick(&SAY_LATE, *say), vec![("time", time(*until))]),
        Reply::Swap { with, theirs, say } => (pick(&SAY_SWAP, *say), vec![("name", name(*with)), ("own", theirs.to_string())]),
        Reply::Text { text, .. } => (text.clone(), Vec::new()),
        Reply::Silent => (SAY_SILENT.into(), Vec::new()),
    }
}

/// What "no answer" says.
pub const SAY_SILENT: &str = "No answer.";
/// (round B) A driver on his day off (a wish granted, time off in lieu).
pub const SAY_DAY_OFF: &str = "That's my day off - I'm not coming in.";
/// (Round F) An hourly-paid driver who cannot that day.
pub const SAY_NOT_THAT_DAY: &str = "I can't that day, sorry.";

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
    // (asked again the same day after a no: angry, he says no; irritated, rather no - `Mood`)
    let mood = mood_of(c, e.id, date, super::clock::now(c));
    if matches!(mood, Mood::Angry { .. } | Mood::Done { .. }) {
        return Reply::No { why: NoWhy::Angry, say };
    }
    let irritated = matches!(mood, Mood::Irritated { .. });
    let mut moody = Rng::of(&[&c.id, date, "pro-mood"], ((n as i64) << 24) | e.id as i64);
    if s.in_frame && s.fit.is_ok() {
        if irritated && !moody.chance(IRRITATED_YES) {
            return Reply::No { why: NoWhy::Annoyed, say };
        }
        // (round F: asked after noon the day before - short notice - a reserve mostly says yes,
        // not always: plans of his own)
        if !irritated && !before_noon(c, date) && !moody.chance(LATE_NOTICE_YES) {
            return Reply::No { why: NoWhy::Plans, say };
        }
        return if irritated { Reply::Grudging { say } } else { Reply::Frame };
    }
    let no = |why: NoWhy| Reply::No { why, say };
    match &s.fit {
        Err(Cannot::Ill { .. }) => return no(NoWhy::Ill),
        Err(Cannot::Holiday { .. }) => return no(NoWhy::Holiday),
        Err(Cannot::DayOff { .. }) => return no(NoWhy::DayOff),
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
    // (round H: an hourly-paid driver remembers what he said he can that day - `pro_avail`)
    let mut inside = false;
    match super::pro_avail::can_of(c, e, date) {
        Some(super::pro_avail::Can::Not) => return no(NoWhy::NotThatDay),
        Some(can) => {
            let (from, until) = can.span().unwrap_or((0, 30 * 60));
            if start < from {
                return Reply::Early { from, say };
            }
            if end > until {
                return Reply::Late { until, say };
            }
            inside = true;
        }
        None => {}
    }
    // (very early or very late: too early or too late for some - not for one who said he can)
    if !inside && start < 5 * 60 + 30 && rng.chance(0.25) {
        return Reply::Early { from: start + 30 * rng.int(2, 5) as i32, say };
    }
    if !inside && end > 23 * 60 && rng.chance(0.25) {
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
    // (round B: his wishes - the day off he asked for, early duties, no split duty, more hours)
    p *= super::pro_wish::bias(c, e.id, date, start, end);
    // (round D: early or late as he likes it, a public holiday, tired - `pro_pref`, `pro_holiday`,
    // `pro_tired`)
    p *= super::pro_pref::bias(c, e, start) * super::pro_holiday::bias(c, date) * super::pro_tired::yes(s.tired);
    // (round H: within what he said he can - glad of it)
    let p = if inside { (p.clamp(0.08, 0.95) * super::pro_avail::INSIDE_YES).min(super::pro_avail::INSIDE_MOST) } else { p.clamp(0.08, 0.95) };
    if irritated {
        return if moody.chance(p * IRRITATED_YES) { Reply::Grudging { say } } else { no(NoWhy::Annoyed) };
    }
    if rng.chance(p) {
        Reply::Yes { say }
    } else if over || e.stress > 60.0 {
        no(NoWhy::Tired)
    } else {
        no(NoWhy::Plans)
    }
}

// --- asked again after a no (the player's: NexiconDK, 4 November 2026) ----------------------------

/// How a driver takes being asked again for the same day after a no: from his real refusals today
/// of duties of that date (`refusals`; another date's duty is another matter - asked about the 16th
/// after a no for the 15th he is calm -, and a new day - the date changing at midnight - starts
/// calm).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Calm,
    /// One no today (`at`: when he said it): a yes much less likely (`IRRITATED_YES`), grudging;
    /// a no in irritated words; asked again he is a little less content (`IRRITATED_COST`).
    Irritated { at: i64 },
    /// Two: asked again he says no, angry, and is less content (`ANGRY_COST`).
    Angry { at: i64 },
    /// Three or more (the angry no among them): he answers no more questions today.
    Done { at: i64 },
}

/// How much less likely an irritated driver says yes.
pub const IRRITATED_YES: f64 = 0.4;
/// How much less content an irritated driver asked again is, and an angry one.
pub const IRRITATED_COST: f64 = 2.0;
pub const ANGRY_COST: f64 = 5.0;

/// When driver `id` said a real no (`Ask::refused_at`) to a duty of `date`, on the company's day of
/// the minute `game` (an old no for that date from last week not counted), known then, the
/// earliest first.
pub fn refusals(c: &Company, id: u32, date: &str, game: i64) -> Vec<i64> {
    use super::clock::DAY;
    let today = game.div_euclid(DAY);
    let mut v: Vec<i64> = c.planning.ops.asks.iter().filter(|a| a.of_staff() == Some(id) && a.date == date).filter_map(|a| a.refused_at(game)).chain(c.planning.ops.noes.iter().filter(|x| x.driver == id && x.date == date && x.at <= game).map(|x| x.at)).filter(|at| at.div_euclid(DAY) == today).collect();
    v.sort_unstable();
    v
}

/// Driver `id`'s mood about a duty of `date` at the company's minute `game`.
pub fn mood_of(c: &Company, id: u32, date: &str, game: i64) -> Mood {
    let r = refusals(c, id, date, game);
    match (r.len(), r.last()) {
        (1, Some(&at)) => Mood::Irritated { at },
        (2, Some(&at)) => Mood::Angry { at },
        (_, Some(&at)) => Mood::Done { at },
        _ => Mood::Calm,
    }
}

/// What an irritated, an angry driver says, and a grudging yes.
pub const SAY_ANNOYED: [&str; 3] = ["I told you no earlier today…", "Again? No.", "I've already said no today."];
pub const SAY_ANGRY: [&str; 2] = ["No! Leave me alone today.", "Come on - the answer is still no!"];
pub const SAY_GRUDGING: [&str; 2] = ["Fine… but that's the last time today.", "Okay then. But don't call me again today."];
/// Refusal: he answers no more questions about that day today (round F: another day's duty he is
/// asked about calmly).
pub const ANSWERS_NO_MORE: &str = "The driver answers no more questions about this day today.";

/// The open questions for duty `n` of `date`.
pub fn asks_of<'a>(c: &'a Company, date: &'a str, n: u32) -> impl Iterator<Item = &'a Ask> + 'a {
    c.planning.ops.asks.iter().filter(move |a| a.date == date && a.n == n && !a.closed)
}

/// (Round G, NexiconDK 5 November 2026) What the answers for a day's duties say on its "N
/// vagter mangler" cell: yes answers not seen yet (green - the planner can give a duty), every
/// answer a no (red - ask others), questions still waiting (grey). The duties with answers not
/// seen, for a click straight to one of them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mark {
    pub yes: usize,
    pub waiting: usize,
    /// Every question answered, none of them a yes - and some of those noes not seen yet.
    pub all_no: bool,
    pub duties: Vec<u32>,
}

impl Mark {
    /// Which it shows (green over red over grey): "yes", "no", "waiting" - None: nothing.
    pub fn key(&self) -> Option<&'static str> {
        if self.yes > 0 {
            Some("yes")
        } else if self.all_no {
            Some("no")
        } else if self.waiting > 0 {
            Some("waiting")
        } else {
            None
        }
    }
}

/// The mark of `date`'s missing cell at the company's minute `game` (the launcher's second `now`):
/// its open questions (not closed: the duty not given, the question not taken back).
pub fn mark_of(c: &Company, date: &str, game: i64, now: u64) -> Mark {
    let mut m = Mark::default();
    let (mut yes_any, mut no_new) = (false, false);
    for a in c.planning.ops.asks.iter().filter(|a| a.date == date && !a.closed) {
        match a.shown_at(game, now) {
            None => m.waiting += 1,
            Some(r) => {
                if r.yes() {
                    yes_any = true;
                    if !a.seen {
                        m.yes += 1;
                    }
                } else if !a.seen {
                    no_new = true;
                }
                if !a.seen && !m.duties.contains(&a.n) {
                    m.duties.push(a.n);
                }
            }
        }
    }
    m.all_no = no_new && !yes_any && m.waiting == 0;
    m
}

/// The planner opened duty `n` of `date`'s box: the answers come so far are seen. Whether any was
/// new.
pub fn mark_seen(c: &mut Company, date: &str, n: u32, game: i64, now: u64) -> bool {
    let mut any = false;
    for a in c.planning.ops.asks.iter_mut().filter(|a| a.date == date && a.n == n && !a.seen) {
        if a.shown_at(game, now).is_some() {
            a.seen = true;
            any = true;
        }
    }
    any
}

/// The template of the notice to a player asked for a duty.
pub const ASKED_NOTICE: &str = "The control room asks: can you drive duty %{n} on %{date}, signing on %{time}? Answer on your page.";

// --- the rules of asking (the player's: NexiconDK, 4 November 2026) -------------------------------

/// How a driver is given a duty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    /// A reserve whose frame holds it, before 12:00 the day before: put on without a question,
    /// he is told (`plan::tell_reserves`).
    Set,
    /// Asked first (round G: anyone else, several at once - the permanent drivers too; a reserve
    /// from 12:00 the day before on).
    Ask,
}

/// (Round F) A reserve asked for a duty in his frame after 12:00 the day before (short notice:
/// `before_noon` no more) says yes this often.
pub const LATE_NOTICE_YES: f64 = 0.85;

/// Whether it is still before 12:00 the day before `date` on the company's clock.
pub fn before_noon(c: &Company, date: &str) -> bool {
    super::clock::now(c) < super::clock::moment(date, 0) - super::clock::DAY + plan::RESERVE_TELL_FROM
}

/// How driver `e` is given a duty of `date` (`tier`: where he comes in its fill order).
pub fn rule_of(c: &Company, e: &Employee, date: &str, tier: u8) -> Rule {
    let _ = e;
    if tier == plan::TIER_RESERVE && before_noon(c, date) {
        Rule::Set
    } else {
        Rule::Ask
    }
}

/// When an AI driver's answer `reply` to duty `n` of `date` (signing on `start`) comes, asked at
/// the company's minute `game`: in game time - normally 5-30 minutes, 2-10 when the duty begins
/// within 3 hours; quicker for an hourly-paid driver short of hours, slower for a tired or
/// stressed one; never between 23:00 and 06:00 (he sleeps: he answers after 06:00, or cannot be
/// reached when the duty begins first); and sometimes no answer at all (`Reply::Silent`, shown
/// after `NO_ANSWER_AFTER`). The same for the same company, day, duty and driver.
#[allow(clippy::too_many_arguments)]
pub fn answer_time(c: &Company, e: &Employee, date: &str, n: u32, start: i32, end: i32, reply: Reply, game: i64, week: f64) -> (Reply, i64) {
    use super::clock::{moment, DAY};
    let mut rng = Rng::of(&[&c.id, date, "pro-answer"], ((n as i64) << 24) | e.id as i64);
    if reply == Reply::Frame {
        return (reply, game + rng.int(1, 3));
    }
    let begins = moment(date, start as i64);
    let soon = begins - game < 180;
    let mut wait = (if soon { rng.int(2, 10) } else { rng.int(5, 30) }) as f64;
    let contract = contract_minutes(c, e);
    let over = contract.is_some_and(|k| week > k);
    let tired = e.stress > 60.0 || e.satisfaction < 35.0 || over;
    if e.employment == Employment::Hourly && week < pay::contract_week_minutes(c) * 0.5 {
        wait *= 0.6;
    }
    if tired {
        wait *= 1.6;
    }
    // (round H: asked within what he said he can - he answers sooner)
    if super::pro_avail::inside(c, e, date, start, end) {
        wait *= super::pro_avail::INSIDE_WAIT;
    }
    let silent_chance = if tired { 0.15 } else { 0.03 };
    if rng.chance(silent_chance) {
        return (Reply::Silent, game + NO_ANSWER_AFTER);
    }
    let mut due = game + wait.round().max(1.0) as i64;
    // (the night: he sleeps from 23:00 to 06:00)
    let m = due.rem_euclid(DAY);
    if !(6 * 60..23 * 60).contains(&m) {
        let morning = if m >= 23 * 60 { due - m + DAY + 6 * 60 } else { due - m + 6 * 60 };
        if begins <= morning {
            return (Reply::Silent, game + NO_ANSWER_AFTER);
        }
        due = morning + rng.int(0, 20);
    }
    (reply, due)
}

/// Ask the drivers `who` for duty `n` (`part`: a part of it cut, `pro_cut`) of `date` (signing on
/// `start`, off `end`) at `now` (seconds since 1970): each with his answer (an AI driver's,
/// decided now and come on the company's clock - `answer_time`, `week(id)` the minutes of his
/// week; None a player's, to come - he is told). A driver asked already is asked again. Round G
/// (NexiconDK, 5 November 2026): any number at once, the permanent drivers too - the rule of one
/// permanent driver at a time is gone. Their numbers.
pub fn ask(c: &mut Company, date: &str, n: u32, start: i32, end: i32, who: Vec<(Asked, Option<Reply>)>, now: u64) -> Result<Vec<u32>, &'static str> {
    ask_part(c, date, n, None, start, end, who, now, &|_| 0.0)
}

#[allow(clippy::too_many_arguments)]
pub fn ask_part(c: &mut Company, date: &str, n: u32, part: Option<u8>, start: i32, end: i32, who: Vec<(Asked, Option<Reply>)>, now: u64, week: &dyn Fn(u32) -> f64) -> Result<Vec<u32>, &'static str> {
    if who.is_empty() {
        return Err(NOBODY);
    }
    let game = super::clock::now(c);
    let mut ids = Vec::new();
    for (w, reply) in who {
        if let Asked::Staff { id } = w {
            if c.employee(id).is_none_or(|e| !staff::drives(e)) {
                return Err(NOT_A_DRIVER);
            }
            // (asked again the same day after a no: no more after the angry no; less content)
            let cost = match mood_of(c, id, date, game) {
                Mood::Done { .. } => return Err(ANSWERS_NO_MORE),
                Mood::Angry { .. } => ANGRY_COST,
                Mood::Irritated { .. } => IRRITATED_COST,
                Mood::Calm => 0.0,
            };
            // (round B: asked on the day he wished to have off - it annoys him a little)
            let cost = cost + if super::pro_wish::day_off_wished(c, id, date).is_some() { super::pro_wish::ASKED_ANYWAY } else { 0.0 };
            if let Some(e) = c.staff.iter_mut().find(|e| e.id == id) {
                e.satisfaction = (e.satisfaction - cost).clamp(0.0, 100.0);
            }
        }
        // (the question asked again replaces the old one: its no kept)
        let kept: Vec<Refusal> = c.planning.ops.asks.iter().filter(|a| a.date == date && a.n == n && a.part == part && a.who == w).filter_map(|a| Some(Refusal { driver: a.of_staff()?, at: a.refused_at(game)?, date: a.date.clone() })).collect();
        c.planning.ops.noes.extend(kept);
        c.planning.ops.asks.retain(|a| !(a.date == date && a.n == n && a.part == part && a.who == w));
        c.planning.ops.last += 1;
        let id = c.planning.ops.last;
        let (reply, due) = match (w, reply) {
            (Asked::Staff { id: e }, Some(r)) => {
                let Some(emp) = c.employee(e).cloned() else { return Err(NOT_A_DRIVER) };
                let (r, due) = answer_time(c, &emp, date, n, start, end, r, game, week(e));
                (Some(r), Some(due))
            }
            (_, r) => (r, None),
        };
        if let Asked::Guest { no } = w {
            let time = format!("{:02}:{:02}", start.rem_euclid(1440) / 60, start.rem_euclid(60));
            super::crew::notify(c, Some(Who::Guest(no)), super::crew::Priority::High, ASKED_NOTICE, &[("n", super::pro_duties::shown_number(c, date, n)), ("date", date.to_string()), ("time", time)], Some("phone"));
        }
        c.planning.ops.asks.push(Ask { id, date: date.to_string(), n, start, end, who: w, at: now, wait: 0, reply, closed: false, part, due, closed_at: None, seen: false });
        ids.push(id);
    }
    Ok(ids)
}

/// Question `id` taken back (closed). Whether it was open.
pub fn withdraw_ask(c: &mut Company, id: u32) -> bool {
    let game = super::clock::now(c);
    match c.planning.ops.asks.iter_mut().find(|a| a.id == id && !a.closed) {
        Some(a) => {
            a.shut(game);
            true
        }
        None => false,
    }
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
    if a.due.is_some() {
        a.due = Some(i64::MIN);
    }
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
    give_part(c, date, n, None, driver, swap)
}

/// `give` for a part of a cut duty (`part`: its index in the duty's split, `pro_cut`).
pub fn give_part(c: &mut Company, date: &str, n: u32, part: Option<u8>, driver: u32, swap: Option<(u32, u32)>) -> Result<(), &'static str> {
    if c.employee(driver).is_none_or(|e| !staff::drives(e)) {
        return Err(NOT_A_DRIVER);
    }
    let pieces = match part {
        Some(i) => super::pro_cut::split_of(c, date, n).and_then(|s| s.parts.get(i as usize)).map(|p| p.pieces.clone()).ok_or(super::pro_cut::NO_SUCH_PART)?,
        None => Vec::new(),
    };
    let set = |c: &mut Company, n: u32, part: Option<u8>, pieces: Vec<PieceKey>, driver: u32| {
        let game = super::clock::now(c);
        c.planning.ops.calls.retain(|x| !(x.date == date && x.n == n && x.part == part));
        c.planning.ops.calls.push(Call { date: date.to_string(), n, driver, part, pieces, toil: false });
        for a in c.planning.ops.asks.iter_mut().filter(|a| a.date == date && a.n == n && a.part == part) {
            a.shut(game);
        }
    };
    set(c, n, part, pieces, driver);
    if let Some((with, theirs)) = swap {
        if c.employee(with).is_none_or(|e| !staff::drives(e)) {
            return Err(NOT_A_DRIVER);
        }
        set(c, theirs, None, Vec::new(), with);
    }
    Ok(())
}

/// `give_part` by the rules of asking (the page's): to a driver who said yes - or, `set`, to a
/// reserve put on within his frame before noon the day before (`Rule::Set`, the page decides it).
/// The others asked for it are told it is taken. `extra`: a duty beyond his own work - counted
/// for the fair share (`pro_rest::count_extra`).
#[allow(clippy::too_many_arguments)]
pub fn give_asked(c: &mut Company, date: &str, n: u32, part: Option<u8>, driver: u32, swap: Option<(u32, u32)>, set: bool, extra: bool, now: u64) -> Result<(), &'static str> {
    let game = super::clock::now(c);
    let yes = c.planning.ops.asks.iter().any(|a| a.date == date && a.n == n && a.part == part && !a.closed && a.of_staff() == Some(driver) && a.shown_at(game, now).is_some_and(Reply::yes));
    if !set && !yes {
        return Err(NEEDS_YES);
    }
    // (round G: those who said yes, or have not answered yet, are told it is taken - with thanks,
    // nothing off their content; a no needs no word)
    let others: Vec<u32> = c.planning.ops.asks.iter().filter(|a| a.date == date && a.n == n && a.part == part && !a.closed).filter(|a| a.shown_at(game, now).is_none_or(Reply::yes)).filter_map(Ask::of_staff).filter(|id| *id != driver).collect();
    give_part(c, date, n, part, driver, swap)?;
    if extra {
        super::pro_rest::count_extra(c, driver, date);
    }
    for id in others {
        let args = vec![("n".to_string(), super::pro_duties::shown_number(c, date, n)), ("date".to_string(), date.to_string())];
        super::messages::post(c, super::messages::To::Driver { employee: id }, super::messages::Priority::Info, super::messages::DUTY_TAKEN, args, None);
    }
    // (round F: put on in his frame before noon the day before - not asked, but told: "You drive
    // duty 1004 on 2024-03-05 in your RAM2")
    if set {
        let named = super::pro_turnus::named_on(c, driver, date);
        let mut args = vec![("n".to_string(), super::pro_duties::shown_number(c, date, n)), ("date".to_string(), date.to_string())];
        let template = match named {
            Some(f) => {
                args.push(("frame".to_string(), f.name));
                super::messages::RESERVE_SET_NAMED
            }
            None => super::messages::RESERVE_SET,
        };
        super::messages::post(c, super::messages::To::Driver { employee: driver }, super::messages::Priority::Instruction, template, args, None);
    }
    Ok(())
}

/// (round B) The overtime of duty `n` of `date` (`part`) given driver `driver` saved as time off
/// in lieu (`toil`) or paid: only a permanent driver's (full or part time). Whether it holds.
pub fn set_toil(c: &mut Company, date: &str, n: u32, part: Option<u8>, driver: u32, toil: bool) -> bool {
    let can = c.employee(driver).is_some_and(super::pro_toil::can_save);
    match c.planning.ops.calls.iter_mut().find(|x| x.date == date && x.n == n && x.part == part && x.driver == driver) {
        Some(x) => {
            x.toil = toil && can;
            x.toil == toil
        }
        None => false,
    }
}

/// The questions for duty `n` of `date` closed (the duty given another way: to a player of "Play
/// together", `together::book::give`).
pub fn close(c: &mut Company, date: &str, n: u32) {
    let game = super::clock::now(c);
    for a in c.planning.ops.asks.iter_mut().filter(|a| a.date == date && a.n == n) {
        a.shut(game);
    }
}

/// The questions for duty `n` of `date` taken back, and the duty given for that day no longer
/// (it runs as the row or the roster has it again). Whether anything changed.
pub fn withdraw(c: &mut Company, date: &str, n: u32) -> bool {
    let before = c.planning.ops.clone();
    let game = super::clock::now(c);
    // (taken back: no longer an extra duty of the driver's - `pro_rest`)
    let gone: Vec<u32> = c.planning.ops.calls.iter().filter(|x| x.date == date && x.n == n).map(|x| x.driver).collect();
    for d in gone {
        super::pro_rest::uncount_extra(c, d, date);
    }
    c.planning.ops.calls.retain(|x| !(x.date == date && x.n == n));
    for a in c.planning.ops.asks.iter_mut().filter(|a| a.date == date && a.n == n) {
        a.shut(game);
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
    c.planning.ops.splits.retain(|x| dates::between(&today, &x.date) >= 0);
    let day = super::clock::moment(&today, 0);
    c.planning.ops.noes.retain(|x| x.at >= day && drivers.contains(&x.driver));
    c.planning.ops.asks.retain(|a| dates::between(&today, &a.date) >= 0 && !matches!(a.who, Asked::Staff { id } if !drivers.contains(&id)));
    c.planning.ops.sick.retain(|x| x.date == today && drivers.contains(&x.driver));
    super::pro_rest::tidy(c);
    super::pro_wish::tidy(c);
    super::pro_toil::tidy(c);
    super::pro_trade::tidy(c);
    super::pro_avail::tidy(c);
}

/// Every English text this module hands the pages besides its literals.
pub fn texts() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = vec![SAY_FRAME, SAY_ILL, SAY_HOLIDAY, SAY_DAY_OFF, SAY_NOT_THAT_DAY, SAY_LICENCE, SAY_LONG, NO_SUCH_ASK, NOT_ASKED, NOT_A_DRIVER, NOBODY, ASKED_NOTICE, NEEDS_YES, SAY_SILENT, ANSWERS_NO_MORE];
    v.extend(super::pro_cut::texts());
    v.extend(super::pro_sick::texts());
    v.extend(super::pro_rest::texts());
    v.extend(super::pro_wish::texts());
    v.extend(super::pro_toil::texts());
    v.extend(super::pro_absence::texts());
    v.extend(super::pro_swap::texts());
    v.extend(super::pro_holiday::texts());
    v.extend(super::pro_pref::texts());
    v.extend(super::pro_tired::texts());
    v.extend(super::pro_trade::texts());
    v.extend(super::pro_season::texts());
    v.extend(super::pro_avail::texts());
    v.extend(super::pro_report::texts());
    for list in [&SAY_YES[..], &SAY_TIRED, &SAY_BUSY, &SAY_PLANS, &SAY_EARLY, &SAY_LATE, &SAY_SWAP, &SAY_ANNOYED, &SAY_ANGRY, &SAY_GRUDGING] {
        v.extend(list.iter().copied());
    }
    v
}

#[cfg(test)]
#[path = "pro_ops_tests.rs"]
mod tests;
