//! RosterFlow Pro, round Q (the player's, NexiconDK 9 November 2026: "Advarsel om ledige vagter"
//! and "Tjek af turnus"): the planner's watch over the plan.
//!
//! - **A duty without a driver soon** (`unfilled`): a duty of the company's day nobody drives yet
//!   (`pro_ops::open_duties`: nobody given it - on Bus Company Online the AI drivers take it) that
//!   signs on within the warning time (`warn_before`, two hours unless the planner chose another,
//!   0 never) and has not begun. RosterFlow Pro's tabs show them in a red banner and the company's
//!   page on the server (the Duties tab) to whoever gives the duties; each is told once in the
//!   feed and the inbox (`note`, looked for every `EVERY` minutes of the company's clock -
//!   `clock::advance`).
//! - **The planner's limits** (`rest_min`, `in_a_row_max`): the least rest between two working
//!   days (11 hours unless chosen: 9 to 12) and the most days of work in a row (six unless chosen:
//!   4 to 7). The Turnus tab checks its rotas against them (`pro_turnus::check`), the calendar each
//!   driver's own days as they are planned now - a duty given, a swap, a frame's duty, a night duty
//!   past midnight its rest from when it really ends (`rest_issue`, `in_a_row`).

use super::clock::{FeedItem, Level};
use super::inbox::{self, Kind, Message};
use super::model::Company;
use super::plan::DayPlan;
use super::pro_ops;
use super::pro_turnus::ProDuty;
use super::rosterflow as rf;

/// The warning time unless the planner chose another (minutes before the sign-on), and the
/// choices (0: never).
pub const WARN_DEFAULT: i32 = 120;
pub const WARN_CHOICES: [i32; 6] = [0, 60, 120, 180, 240, 360];
/// The least rest unless chosen (the drivers' rules' 11 hours), and the range it may be set in.
pub const REST_DEFAULT: i32 = super::pro_turnus::MIN_REST;
pub const REST_LEAST: i32 = 9 * 60;
pub const REST_MOST: i32 = 12 * 60;
/// The most days of work in a row unless chosen, and the range.
pub const IN_A_ROW_DEFAULT: u32 = super::pro_turnus::MAX_IN_A_ROW as u32;
pub const IN_A_ROW_LEAST: u32 = 4;
pub const IN_A_ROW_MOST: u32 = 7;
/// How often the company's day is looked at for the duties without a driver (minutes).
pub const EVERY: i64 = 10;

pub const BAD_CHOICE: &str = "That is not one of the choices.";
/// Told once a duty (the feed; the inbox's title and text).
pub const FEED: &str = "⚠ Duty %{n} at %{time} has no driver yet (signs on in %{in})";
pub const INBOX_TITLE: &str = "Duty %{n} has no driver";
pub const INBOX_TEXT: &str = "Duty %{n} signs on at %{time} and nobody drives it yet. Give it a driver in RosterFlow Pro's Daily operations, or ask the drivers - else it is left to the dispatcher.";

/// How long before its sign-on a duty without a driver is warned of (minutes; 0 never).
pub fn warn_before(c: &Company) -> i32 {
    c.planning.ops.warn_before.unwrap_or(WARN_DEFAULT)
}

pub fn set_warn_before(c: &mut Company, minutes: i32) -> Result<(), &'static str> {
    if !WARN_CHOICES.contains(&minutes) {
        return Err(BAD_CHOICE);
    }
    c.planning.ops.warn_before = (minutes != WARN_DEFAULT).then_some(minutes);
    Ok(())
}

/// The least rest between two working days (minutes).
pub fn rest_min(c: &Company) -> i32 {
    c.planning.ops.rest_min.unwrap_or(REST_DEFAULT)
}

/// Set it (`minutes`: 9 to 12 hours, by the half hour).
pub fn set_rest_min(c: &mut Company, minutes: i32) -> Result<(), &'static str> {
    if !(REST_LEAST..=REST_MOST).contains(&minutes) || minutes % 30 != 0 {
        return Err(BAD_CHOICE);
    }
    c.planning.ops.rest_min = (minutes != REST_DEFAULT).then_some(minutes);
    Ok(())
}

/// The most days of work in a row.
pub fn in_a_row_max(c: &Company) -> u32 {
    c.planning.ops.in_a_row_max.unwrap_or(IN_A_ROW_DEFAULT)
}

pub fn set_in_a_row_max(c: &mut Company, n: u32) -> Result<(), &'static str> {
    if !(IN_A_ROW_LEAST..=IN_A_ROW_MOST).contains(&n) {
        return Err(BAD_CHOICE);
    }
    c.planning.ops.in_a_row_max = (n != IN_A_ROW_DEFAULT).then_some(n);
    Ok(())
}

// --- the duties without a driver ------------------------------------------------------------------

/// A duty of the day without a driver that signs on soon: its number, the name it is shown by,
/// when it signs on (minutes of the day) and in how many minutes from now.
#[derive(Clone, Debug, PartialEq)]
pub struct Unfilled {
    pub n: u32,
    pub label: String,
    pub start: i32,
    pub within: i32,
}

/// The duties of the company's day `dp` (`duties` its duties as the Vagter tab numbers them)
/// nobody drives yet that sign on within the warning time from `now` (minutes of the day) and
/// have not begun - the soonest first. None at all when the warning is off.
pub fn unfilled(c: &Company, dp: &DayPlan, duties: &[ProDuty], now: i32) -> Vec<Unfilled> {
    let warn = warn_before(c);
    if warn <= 0 || !super::pro_duties::on(c) {
        return Vec::new();
    }
    let mut out: Vec<Unfilled> = pro_ops::open_duties(dp, duties)
        .into_iter()
        // (a duty partly given counts too: a piece of it waits for nobody)
        .filter(|d| d.start > now && d.start - now <= warn)
        .map(|d| Unfilled { n: d.n, label: duties.iter().find(|x| x.number == d.n).map(|x| x.shown()).unwrap_or_else(|| d.n.to_string()), start: d.start, within: d.start - now })
        .collect();
    out.sort_by_key(|u| (u.start, u.n));
    out
}

/// "1:45" / "0:45": how long until a sign-on (the same in every language: the feed's words take
/// it as it is).
pub fn within_words(m: i32) -> String {
    rf::hm(m.max(0))
}

/// Look at the company's day now (`now` minutes of it): the duties without a driver signing on
/// soon not told yet are told - the feed's lines returned, the inbox's message posted - and kept
/// as told (the days gone forgotten).
pub fn note(c: &mut Company, dp: &DayPlan, duties: &[ProDuty], now: i32) -> Vec<FeedItem> {
    let date = c.date.clone();
    c.planning.ops.warned.retain(|(d, _)| d.as_str() >= date.as_str());
    let at = super::clock::moment(&date, now as i64);
    let mut out = Vec::new();
    for u in unfilled(c, dp, duties, now) {
        if c.planning.ops.warned.iter().any(|(d, n)| *d == date && *n == u.n) {
            continue;
        }
        c.planning.ops.warned.push((date.clone(), u.n));
        let time = rf::clock(u.start.rem_euclid(1440));
        let args = vec![("n".to_string(), u.label.clone()), ("time".to_string(), time.clone()), ("in".to_string(), within_words(u.within))];
        out.push(FeedItem { at, text: FEED.into(), args, level: Level::Warn });
        let m = Message::new(Kind::Staff, INBOX_TITLE, INBOX_TEXT).arg("n", u.label.clone()).arg("time", time).key(&format!("pro-unfilled-{date}-{}", u.n));
        inbox::post(c, m);
    }
    out
}

/// Whether the company's day is due a look (every `EVERY` minutes), and kept as looked at.
pub fn due(c: &mut Company, at: i64) -> bool {
    if !super::pro_duties::on(c) || warn_before(c) <= 0 {
        return false;
    }
    if c.planning.ops.watched > 0 && at < c.planning.ops.watched + EVERY && at >= c.planning.ops.watched {
        return false;
    }
    c.planning.ops.watched = at;
    true
}

// --- rest and days in a row ---------------------------------------------------------------------

/// The rest between the working day before (`prev`, its sign-on and sign-off - minutes of its day,
/// a night duty's sign-off past 1440) and the day's (`own`): its minutes, and whether it is below
/// the planner's least (`Some`; hard below the drivers' rules' reduced rest of 9 hours).
pub fn rest_issue(c: &Company, prev: Option<(i32, i32)>, own: Option<(i32, i32)>) -> Option<(i32, bool)> {
    let (_, end) = prev?;
    let (start, _) = own?;
    let rest = start + 1440 - end;
    (rest < rest_min(c)).then_some((rest.max(0), rest < super::pro_rest::REDUCED_REST))
}

/// Whether a run of `n` days of work in a row is more than the planner allows.
pub fn in_a_row(c: &Company, n: u32) -> bool {
    n > in_a_row_max(c)
}

/// Every English text this module writes, for the translations.
pub fn texts() -> Vec<&'static str> {
    vec![BAD_CHOICE, FEED, INBOX_TITLE, INBOX_TEXT]
}

