//! RosterFlow Pro, round C (the player's, NexiconDK 5 November 2026): a swap changes both drivers
//! at once.
//!
//! - **In the Turnus tab** (`swap_rota`): driver A given duty N on a date of his next weeks takes
//!   it from its driver B that day, and B takes A's day - his duty, his frame or his day off: two
//!   exceptions "Swapped" made together.
//! - **In the who-can-drive box** (`after_give`): "Give the duty, with the swap" gives the open duty
//!   to A and A's own duty to colleague B in one act (`pro_ops::give_part`); the swap is kept here.
//! - Both are told (`SWAPPED`), the calendar shows both days changed (⇄), each driver's hours and
//!   costs follow from them. The rest rules are checked for both (`breaches`): broken, the page
//!   says so in red - it is the planner's to decide.
//! - **Undone** (`undo`, "Fortryd bytte") until the first of the two duties begins: the exceptions
//!   as they were, the duties given taken back; both are told (`UNSWAPPED`).

use super::model::Company;
use super::pro_rest::{self as pr, Breach, Span};
use super::pro_turnus::{self as pt, Cell, Exception, Week};
use super::{dates, messages};
use serde::{Deserialize, Serialize};

/// A swap of two drivers on a date: `a` takes `a_gets`, `b` takes `b_gets` (a duty's number, or
/// None: the day off).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Swap {
    pub no: u32,
    pub date: String,
    pub a: u32,
    pub b: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub a_gets: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub b_gets: Option<u32>,
    pub how: How,
    /// The minute of the date the first of the two duties begins (undone only before it).
    pub starts: i32,
}

/// Where a swap was made, and what undoing it puts back.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum How {
    /// The rota's days: the two drivers' exceptions of the date before it.
    Rota { a_before: Option<Exception>, b_before: Option<Exception> },
    /// The daily operations: the duties given for the date (the open one to `a`, `a`'s own to `b`).
    Ops { n: u32, theirs: u32 },
}

/// The texts (English: keys of the translations).
pub const SWAPPED: &str = "Swap on %{date}: you drive %{mine} instead of %{was} - %{name} takes yours";
pub const SWAPPED_OFF: &str = "Swap on %{date}: you drive %{mine} on your day off - %{name} has the day off instead";
pub const SWAPPED_TO_OFF: &str = "Swap on %{date}: you have the day off instead of %{was} - %{name} drives it";
pub const UNSWAPPED: &str = "The swap on %{date} is undone: you drive as planned";
pub const NOT_ON_A_ROTA: &str = "The driver is not on a rota.";
pub const SAME: &str = "The driver has that duty already.";
pub const NO_SUCH_SWAP: &str = "That swap is no longer there.";
pub const BEGUN: &str = "The swap's day has begun: it can no longer be undone.";
pub const AWAY: &str = "The driver is ill or on holiday that day.";
pub const NOBODY: &str = "Nobody's rota has that duty that day: it is given to the driver alone.";

fn number(c: Cell) -> Option<u32> {
    match c {
        Cell::Duty { n } => Some(n),
        _ => None,
    }
}

fn next_no(c: &mut Company) -> u32 {
    c.planning.ops.last_swap += 1;
    c.planning.ops.last_swap
}

/// What the Turnus tab's swap made: the swap kept (None: nobody's rota had the duty - it is the
/// driver's alone), and the rest rules each of the two would break.
#[derive(Clone, Debug, PartialEq)]
pub struct Made {
    pub swap: Option<Swap>,
    pub a_breaks: Vec<Breach>,
    pub b_breaks: Vec<Breach>,
}

/// Driver `a` given duty `n` on `date` in the Turnus tab: its driver by his rota that day (`b`)
/// takes `a`'s day instead - both exceptions "Swapped" at once, both told. `week`: the week's
/// duties (their times).
pub fn swap_rota(c: &mut Company, a: u32, date: &str, n: u32, week: &Week) -> Result<Made, &'static str> {
    if date < c.date.as_str() {
        return Err(super::pro_absence::GONE);
    }
    let da = pt::decided(c, a, date).ok_or(NOT_ON_A_ROTA)?;
    if matches!(da.note.as_deref(), Some("sick" | "holiday")) {
        return Err(AWAY);
    }
    if da.cell == (Cell::Duty { n }) {
        return Err(SAME);
    }
    // (whose rota has it that day - not ill, not on holiday: he is the one who would drive it)
    let b = c.staff.iter().map(|e| e.id).filter(|id| *id != a).find(|id| pt::decided(c, *id, date).is_some_and(|d| d.cell == Cell::Duty { n }));
    let before = |c: &Company, id: u32| c.planning.rotas.exceptions.iter().find(|x| x.driver == id && x.date == date).cloned();
    let a_before = before(c, a);
    pt::put_exception(c, a, date, Some((Cell::Duty { n }, "swap".to_string())));
    let Some(b) = b else {
        let a_breaks = breaches(c, a, date, week);
        return Ok(Made { swap: None, a_breaks, b_breaks: Vec::new() });
    };
    let b_before = before(c, b);
    pt::put_exception(c, b, date, Some((da.cell, "swap".to_string())));
    let day = pt::day_of(date);
    let starts = [Cell::Duty { n }, da.cell].iter().filter_map(|x| pt::times(week, day, *x)).map(|t| t.0).min().unwrap_or(0);
    let s = Swap { no: next_no(c), date: date.to_string(), a, b, a_gets: Some(n), b_gets: number(da.cell), how: How::Rota { a_before, b_before }, starts };
    c.planning.ops.swaps.push(s.clone());
    tell(c, &s);
    let (a_breaks, b_breaks) = (breaches(c, a, date, week), breaches(c, b, date, week));
    Ok(Made { swap: Some(s), a_breaks, b_breaks })
}

/// The swap of the daily operations' "Give the duty, with the swap" kept and told: duty `n` of
/// `date` given `a`, his own duty `theirs` given `b` (`starts`: the first of the two begins).
pub fn after_give(c: &mut Company, date: &str, n: u32, a: u32, b: u32, theirs: u32, starts: i32) -> Swap {
    let s = Swap { no: next_no(c), date: date.to_string(), a, b, a_gets: Some(n), b_gets: Some(theirs), how: How::Ops { n, theirs }, starts };
    c.planning.ops.swaps.push(s.clone());
    tell(c, &s);
    s
}

/// Both drivers of swap `s` told what they drive now.
fn tell(c: &mut Company, s: &Swap) {
    let name = |c: &Company, id: u32| c.employee(id).map(|e| e.name.clone()).unwrap_or_default();
    let (na, nb) = (name(c, s.a), name(c, s.b));
    let date = s.date.clone();
    let to = |id| messages::To::Driver { employee: id };
    // (a: the duty given him; what he had goes to b)
    let a_args = vec![("date".to_string(), date.clone()), ("mine".to_string(), s.a_gets.map(|x| x.to_string()).unwrap_or_default()), ("was".to_string(), s.b_gets.map(|x| x.to_string()).unwrap_or_default()), ("name".to_string(), nb)];
    let a_text = if s.b_gets.is_some() { SWAPPED } else { SWAPPED_OFF };
    messages::post(c, to(s.a), messages::Priority::Info, a_text, a_args, None);
    let b_args = vec![("date".to_string(), date), ("mine".to_string(), s.b_gets.map(|x| x.to_string()).unwrap_or_default()), ("was".to_string(), s.a_gets.map(|x| x.to_string()).unwrap_or_default()), ("name".to_string(), na)];
    let b_text = if s.b_gets.is_some() { SWAPPED } else { SWAPPED_TO_OFF };
    messages::post(c, to(s.b), messages::Priority::Info, b_text, b_args, None);
}

/// Whether swap `s` can still be undone: its date ahead, or the company's day before its first
/// duty begins.
pub fn can_undo(c: &Company, s: &Swap) -> bool {
    match dates::between(&c.date, &s.date) {
        d if d > 0 => true,
        0 => (c.clock.minute as i32) < s.starts,
        _ => false,
    }
}

/// Swap `no` undone ("Fortryd bytte"): the exceptions of the rota as they were, or the two duties
/// given taken back. Both told.
pub fn undo(c: &mut Company, no: u32) -> Result<Swap, &'static str> {
    let k = c.planning.ops.swaps.iter().position(|s| s.no == no).ok_or(NO_SUCH_SWAP)?;
    let s = c.planning.ops.swaps[k].clone();
    if !can_undo(c, &s) {
        return Err(BEGUN);
    }
    match &s.how {
        How::Rota { a_before, b_before } => {
            for (id, was) in [(s.a, a_before), (s.b, b_before)] {
                pt::put_exception(c, id, &s.date, was.as_ref().map(|x| (x.cell, x.note.clone())));
            }
        }
        How::Ops { n, theirs } => {
            super::pro_ops::withdraw(c, &s.date, *n);
            super::pro_ops::withdraw(c, &s.date, *theirs);
        }
    }
    c.planning.ops.swaps.remove(k);
    for id in [s.a, s.b] {
        messages::post(c, messages::To::Driver { employee: id }, messages::Priority::Info, UNSWAPPED, vec![("date".to_string(), s.date.clone())], None);
    }
    Ok(s)
}

/// The swaps of `date`.
pub fn of_day<'a>(c: &'a Company, date: &'a str) -> impl Iterator<Item = &'a Swap> + 'a {
    c.planning.ops.swaps.iter().filter(move |s| s.date == date)
}

/// The swap driver `id` is in on `date`, and the other driver of it.
pub fn of_driver<'a>(c: &'a Company, id: u32, date: &str) -> Option<(&'a Swap, u32)> {
    c.planning.ops.swaps.iter().find(|s| s.date == date && (s.a == id || s.b == id)).map(|s| (s, if s.a == id { s.b } else { s.a }))
}

/// A swap no longer as it was made (a duty given another way since, an exception changed) is
/// forgotten: the days as they are now.
fn holds(c: &Company, s: &Swap) -> bool {
    match &s.how {
        How::Rota { .. } => {
            let ex = |id: u32| c.planning.rotas.exceptions.iter().find(|x| x.driver == id && x.date == s.date && x.note == "swap");
            ex(s.a).is_some_and(|x| number(x.cell) == s.a_gets) && ex(s.b).is_some()
        }
        How::Ops { n, theirs } => {
            let given = |n: u32, id: u32| c.planning.ops.calls.iter().any(|x| x.date == s.date && x.n == n && x.driver == id && x.part.is_none());
            given(*n, s.a) && given(*theirs, s.b)
        }
    }
}

/// The swaps of days gone, and those no longer as made, dropped (the night's, `pro_turnus::tidy`;
/// and before the page shows them).
pub fn tidy(c: &mut Company) {
    let today = c.date.clone();
    let keep: Vec<bool> = c.planning.ops.swaps.iter().map(|s| dates::between(&today, &s.date) >= 0 && holds(c, s)).collect();
    let mut k = 0;
    c.planning.ops.swaps.retain(|_| {
        k += 1;
        keep[k - 1]
    });
}

/// The rest rules driver `id`'s day of `date` breaks as the rotas have it now: rest after the day
/// before, before the day after, the day's span (`pro_rest::breaches`, the week's working time
/// left out: the calendar's box shows it).
pub fn breaches(c: &Company, id: u32, date: &str, week: &Week) -> Vec<Breach> {
    let span = |date: &str| -> Span {
        let d = pt::decided(c, id, date)?;
        let (from, to, _) = pt::times(week, pt::day_of(date), d.cell)?;
        Some((from, to, number(d.cell).unwrap_or(0)))
    };
    let Some((start, end, _)) = span(date) else { return Vec::new() };
    let days: Vec<Span> = super::pro_ops::week_dates(date).iter().map(|x| if x == date { None } else { span(x) }).collect();
    let mut out = pr::breaches(start, end, span(&dates::add(date, -1)), span(&dates::add(date, 1)), None, pr::reduced_in(&days), 0.0, 0);
    out.retain(|b| !matches!(b, Breach::LongWeek { .. }));
    out
}

/// Every English text this module writes, for the translations.
pub fn texts() -> Vec<&'static str> {
    vec![SWAPPED, SWAPPED_OFF, SWAPPED_TO_OFF, UNSWAPPED, NOT_ON_A_ROTA, SAME, NO_SUCH_SWAP, BEGUN, NOBODY, AWAY]
}
