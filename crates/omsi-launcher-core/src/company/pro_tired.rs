//! RosterFlow Pro, round D (the player's, NexiconDK 5 November 2026: "Træthed"): a driver who
//! works many long days in a row gets tired.
//!
//! - **How tired** (`assess`): from a driver's working days up to the day - each its first
//!   sign-on and last sign-off -, those since the last day off (the streak): how many, how many
//!   of them long (a working day over 9 hours from sign-on to sign-off, `LONG`), and how many
//!   rests between them short (under 11 hours, `pro_rest::DAILY_REST`).
//!   * **Tired** ("😮‍💨 Tired"): 6 working days in a row, 4 long days, or 2 short rests.
//!   * **Worn out** ("🥱 Worn out"): 8 working days in a row, 6 long days, 4 short rests, or 6
//!     days in a row of which 4 long.
//!   * **A day off** ends the streak - but one day off alone after a hard streak leaves them a
//!     step less tired, not rested ("only one day off after a hard week"); two days off and they
//!     are fresh.
//! - **The days** (`on_day`): the days gone as they were worked (`Employee::worked`, kept for two
//!   weeks by the night, `after_day`), the day itself and the days ahead as planned.
//! - **What it does**: a tired driver says yes to an extra duty less often (`yes`: ×0.8, worn
//!   out ×0.55), calls in sick on the day more often (`sick`, `pro_sick::draws`: ×1.6, ×2.5) and
//!   drives more slowly, late more often (`slow`, `lateness`: ×1.4, ×1.8).

use super::model::{Company, Employee};
use super::{dates, pro_duties, pro_rest};
use serde::{Deserialize, Serialize};

/// A long working day (minutes from its first sign-on to its last sign-off).
pub const LONG: i32 = 9 * 60;
/// The days of work kept (`Employee::worked`) and looked at (`on_day`).
pub const KEPT: i64 = 14;

/// A working day gone: its first sign-on and last sign-off (minutes of the day).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Worked {
    pub date: String,
    pub from: i32,
    pub to: i32,
}

/// A day's work: its first sign-on and last sign-off (None: no work that day).
pub type Day = Option<(i32, i32)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Default, Hash)]
pub enum Level {
    #[default]
    Fresh,
    Tired,
    Worn,
}

/// Why they are tired.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Why {
    #[default]
    Nothing,
    /// `n` long days, the last of them in a row.
    LongInRow(u32),
    /// `n` long days since the last day off.
    LongDays(u32),
    /// `n` working days in a row.
    Days(u32),
    /// `n` short rests since the last day off.
    ShortRests(u32),
    /// One day off only after a hard streak.
    OneDayOff,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Tired {
    pub level: Level,
    pub why: Why,
}

impl Level {
    fn down(self) -> Level {
        match self {
            Level::Worn => Level::Tired,
            _ => Level::Fresh,
        }
    }

    /// Its words (English, translated when shown): None when fresh.
    pub fn label(self) -> Option<&'static str> {
        match self {
            Level::Fresh => None,
            Level::Tired => Some(TIRED),
            Level::Worn => Some(WORN),
        }
    }

    /// Its key for the page.
    pub fn key(self) -> &'static str {
        match self {
            Level::Fresh => "fresh",
            Level::Tired => "tired",
            Level::Worn => "worn",
        }
    }
}

impl Why {
    /// Its words (English with `%{n}`, translated when shown) and the number.
    pub fn words(self) -> Option<(&'static str, u32)> {
        match self {
            Why::Nothing => None,
            Why::LongInRow(n) => Some((LONG_IN_ROW, n)),
            Why::LongDays(n) => Some((LONG_DAYS, n)),
            Why::Days(n) => Some((DAYS_IN_ROW, n)),
            Why::ShortRests(n) => Some((SHORT_RESTS, n)),
            Why::OneDayOff => Some((ONE_DAY_OFF, 0)),
        }
    }
}

/// How tired a driver is on the last of `days` (oldest first, the day itself last; None a day
/// without work). Fresh on a day without work.
pub fn assess(days: &[Day]) -> Tired {
    let Some(&Some(_)) = days.last() else { return Tired::default() };
    let n = days.len();
    let streak = days.iter().rev().take_while(|d| d.is_some()).count();
    let start = n - streak;
    let run: Vec<(i32, i32)> = days[start..].iter().flatten().copied().collect();
    let long_of = |x: &(i32, i32)| x.1 - x.0 > LONG;
    let long = run.iter().filter(|x| long_of(x)).count() as u32;
    let long_run = run.iter().rev().take_while(|x| long_of(x)).count() as u32;
    let short = run.windows(2).filter(|w| w[1].0 + 1440 - w[0].1 < pro_rest::DAILY_REST).count() as u32;
    let s = streak as u32;
    let level = if s >= 8 || long >= 6 || short >= 4 || (s >= 6 && long >= 4) {
        Level::Worn
    } else if s >= 6 || long >= 4 || short >= 2 {
        Level::Tired
    } else {
        Level::Fresh
    };
    let why = if level == Level::Fresh {
        Why::Nothing
    } else if long_run >= 4 {
        Why::LongInRow(long_run)
    } else if long >= 4 {
        Why::LongDays(long)
    } else if s >= 6 {
        Why::Days(s)
    } else {
        Why::ShortRests(short)
    };
    let mut out = Tired { level, why };
    // (one day off alone after a hard streak: a step less tired, not rested)
    if start >= 2 && days[start - 1].is_none() && days[start - 2].is_some() {
        let before = assess(&days[..start - 1]).level.down();
        if before > out.level {
            out = Tired { level: before, why: Why::OneDayOff };
        }
    }
    out
}

/// Driver `e`'s working day `date` gone, as it was worked (`Employee::worked`).
pub fn worked_on(e: &Employee, date: &str) -> Day {
    e.worked.iter().find(|w| w.date == date).map(|w| (w.from, w.to))
}

/// How tired driver `e` is on `date`: `span(d)` their work of each day of the two weeks before
/// and of the day itself (the days gone as worked - `worked_on` -, the others as planned).
pub fn on_day(date: &str, span: &dyn Fn(&str) -> Day) -> Tired {
    let days: Vec<Day> = (0..KEPT).rev().map(|k| span(&dates::add(date, -k))).collect();
    assess(&days)
}

/// How tired driver `e` is on the company's day `date` working `today` (the days before as
/// worked).
pub fn today(e: &Employee, date: &str, today: Day) -> Tired {
    on_day(date, &|d| if d == date { today } else { worked_on(e, d) })
}

/// What it does to the yes chance of an extra duty.
pub fn yes(l: Level) -> f64 {
    match l {
        Level::Fresh => 1.0,
        Level::Tired => 0.8,
        Level::Worn => 0.55,
    }
}

/// What it does to the chance of calling in sick on the day.
pub fn sick(l: Level) -> f64 {
    match l {
        Level::Fresh => 1.0,
        Level::Tired => 1.6,
        Level::Worn => 2.5,
    }
}

/// What it does to the minutes a driver loses on the way.
pub fn slow(l: Level) -> f64 {
    match l {
        Level::Fresh => 1.0,
        Level::Tired => 1.4,
        Level::Worn => 1.8,
    }
}

/// How tired driver `e` is driving on the company's day `date` (RosterFlow Pro; Fresh without
/// it): the days before as worked, the day itself a working day.
pub fn driving(c: &Company, e: &Employee, date: &str) -> Level {
    if !pro_duties::on(c) || e.worked.is_empty() {
        return Level::Fresh;
    }
    // (the day's own hours not known here: a working day, neither long nor after a short rest)
    let at = worked_on(e, &dates::add(date, -1)).map_or(8 * 60, |p| (p.1 + pro_rest::DAILY_REST - 1440).max(0));
    today(e, date, Some((at, at))).level
}

/// The night after `date` (`day::close_day`; `pieces` the drivers' pieces of work: id, from,
/// to): each driver's working day kept, the days older than two weeks let go.
pub fn after_day(c: &mut Company, date: &str, pieces: &[(u32, i32, i32)]) {
    if !pro_duties::on(c) {
        return;
    }
    for e in c.staff.iter_mut() {
        let mine = pieces.iter().filter(|x| x.0 == e.id && x.2 > x.1);
        let (from, to) = mine.fold((i32::MAX, i32::MIN), |a, x| (a.0.min(x.1), a.1.max(x.2)));
        e.worked.retain(|w| w.date != date && dates::between(&w.date, date) < KEPT);
        if from < to {
            e.worked.push(Worked { date: date.to_string(), from, to });
        }
    }
}

pub const TIRED: &str = "😮‍💨 Tired";
pub const WORN: &str = "🥱 Worn out";
pub const LONG_IN_ROW: &str = "%{n} long days in a row";
pub const LONG_DAYS: &str = "%{n} long days without a day off";
pub const DAYS_IN_ROW: &str = "%{n} working days in a row";
pub const SHORT_RESTS: &str = "%{n} short rests without a day off";
pub const ONE_DAY_OFF: &str = "only one day off after a hard week";

/// Every English text this module hands the pages.
pub fn texts() -> Vec<&'static str> {
    vec![TIRED, WORN, LONG_IN_ROW, LONG_DAYS, DAYS_IN_ROW, SHORT_RESTS, ONE_DAY_OFF]
}
