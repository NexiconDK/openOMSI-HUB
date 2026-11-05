//! RosterFlow Pro: the rest rules when a duty is put on, asked for or given (the player's,
//! NexiconDK 4 November 2026: "Hviletid"), and the fair share of the extra duties ("Retfærdig
//! fordeling").
//!
//! - **Rest** (`breaches`): the Danish and EU drivers' rules against a driver's other days -
//!   11 hours' rest between two working days (`DAILY_REST`; reduced to 9, `REDUCED_REST`, at most
//!   `REDUCED_TIMES` times a week), a working day of at most 13 hours from its first sign-on to its
//!   last sign-off (`SPREAD_MAX`), at most 48 hours a week (`WEEK_MAX`). The game's own hard rules
//!   (`plan::can_take`: the pause between a day's pieces, a working day's driving, the night's
//!   rest on the company's day) keep a driver out as before; these are the planner's to keep or
//!   to break: shown in red, the duty given "anyway" after a confirmation costs the driver's
//!   content and raises his stress (`broke`), and a fine when one is set (`fine`: on Bus Company
//!   Online the server's admin's, `rest_fine` - off it the owner's).
//! - **Fair share** (`Ops::extras`): every duty given a driver beyond his own work (overtime, a day
//!   off, an hourly-paid driver's extra hours - not a reserve within his frame) is counted for its
//!   month (`extras_in_month`). A driver given many gets tired faster (`TIRED_FROM`); one who
//!   wants hours and was not given any while others were given many says so to the control room
//!   once a week and is a little less content (`grumbles`).

use super::model::{Cents, Company, Employment};
use super::{dates, messages, staff};
use serde::{Deserialize, Serialize};

// --- rest -----------------------------------------------------------------------------------------

/// The daily rest between two working days (minutes), and the reduced rest allowed a few times a
/// week.
pub const DAILY_REST: i32 = 11 * 60;
pub const REDUCED_REST: i32 = 9 * 60;
pub const REDUCED_TIMES: usize = 3;
/// A working day's span, its first sign-on to its last sign-off (24 hours less the daily rest).
pub const SPREAD_MAX: i32 = 13 * 60;
/// A week's working time.
pub const WEEK_MAX: f64 = 48.0 * 60.0;

/// A driver's working day: its first sign-on, its last sign-off (minutes of the day) and its first
/// duty's number. None: no work that day.
pub type Span = Option<(i32, i32, u32)>;

/// A rule a duty would break for a driver.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Breach {
    /// Only `minutes` of rest after his duty `n` of the day before.
    RestAfter { minutes: i32, n: u32 },
    /// Only `minutes` of rest before his duty `n` of the day after.
    RestBefore { minutes: i32, n: u32 },
    /// His working day would span `minutes` (over `SPREAD_MAX`).
    LongDay { minutes: i32 },
    /// His week would come to `minutes` (over `WEEK_MAX`).
    LongWeek { minutes: i32 },
}

/// The rests shorter than the daily rest between the working days of a week (`days`: Sunday to
/// Saturday, each its span): the reduced rests taken.
pub fn reduced_in(days: &[Span]) -> usize {
    days.windows(2).filter(|w| matches!((w[0], w[1]), (Some(a), Some(b)) if b.0 + 1440 - a.1 < DAILY_REST)).count()
}

/// The rules a duty signing on `start` and off `end` would break for a driver: `prev` and `next`
/// his working days before and after (`prev` None on the company's day - the night's rest of the
/// game's own rules counts there: `plan::can_take`), `own` his other work that day, `reduced` the
/// reduced rests of his week so far, `week` the minutes of his week without it, `paid` its
/// minutes.
#[allow(clippy::too_many_arguments)]
pub fn breaches(start: i32, end: i32, prev: Span, next: Span, own: Span, reduced: usize, week: f64, paid: i32) -> Vec<Breach> {
    let mut out = Vec::new();
    let first = own.map_or(start, |o| o.0.min(start));
    let last = own.map_or(end, |o| o.1.max(end));
    let mut reduced = reduced;
    let mut short = |rest: i32| -> bool {
        if rest >= DAILY_REST {
            return false;
        }
        if rest >= REDUCED_REST && reduced < REDUCED_TIMES {
            reduced += 1;
            return false;
        }
        true
    };
    if let Some((_, end_before, n)) = prev {
        let rest = first + 1440 - end_before;
        if short(rest) {
            out.push(Breach::RestAfter { minutes: rest.max(0), n });
        }
    }
    if let Some((start_after, _, n)) = next {
        let rest = start_after + 1440 - last;
        if short(rest) {
            out.push(Breach::RestBefore { minutes: rest.max(0), n });
        }
    }
    if last - first > SPREAD_MAX {
        out.push(Breach::LongDay { minutes: last - first });
    }
    if week + paid as f64 > WEEK_MAX + 0.5 {
        out.push(Breach::LongWeek { minutes: (week + paid as f64).round() as i32 });
    }
    out
}

/// What breaking the rest rules costs the driver (content, stress).
pub const BREAK_COST: f64 = 4.0;
pub const BREAK_STRESS: f64 = 6.0;

/// The fine for a breach of the rest rules: on Bus Company Online's server its admin's
/// (`rest_fine`, `live_pay::LiveCfg`), off it the owner's (`Ops::rest_fine`). 0: none.
pub fn fine(c: &Company) -> Cents {
    if c.online.on_server {
        super::live_pay::cfg(c).rest_fine
    } else {
        c.planning.ops.rest_fine
    }
    .max(0)
}

/// The owner's fine off the server (cents; refused on the server, where its admin sets it).
pub const SERVER_DECIDES: &str = "Decided by the server's admin";
pub fn set_fine(c: &mut Company, cents: Cents) -> Result<(), &'static str> {
    if c.online.on_server {
        return Err(SERVER_DECIDES);
    }
    c.planning.ops.rest_fine = cents.clamp(0, 1_000_000_00);
    Ok(())
}

/// Driver `driver` given a duty breaking the rest rules anyway: less content, more stressed, and
/// the fine booked.
pub fn broke(c: &mut Company, driver: u32) {
    let fine = fine(c);
    let Some(e) = c.staff.iter_mut().find(|e| e.id == driver) else { return };
    e.satisfaction = (e.satisfaction - BREAK_COST).clamp(0.0, 100.0);
    e.stress = (e.stress + BREAK_STRESS).clamp(0.0, 100.0);
    let name = e.name.clone();
    if fine > 0 {
        c.book(super::model::BookingKind::Fine, -fine, name, false);
    }
}

// --- the fair share of the extra duties -----------------------------------------------------------

/// An extra duty given a driver (its date).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Extra {
    pub driver: u32,
    pub date: String,
}

/// A driver given more extra duties in a month than this gets tired faster (`TIRED_STRESS` each).
pub const TIRED_FROM: usize = 4;
pub const TIRED_STRESS: f64 = 3.0;

/// The extra duties driver `driver` was given for the month `date` is in.
pub fn extras_in_month(c: &Company, driver: u32, date: &str) -> usize {
    let month = dates::month_of(date);
    c.planning.ops.extras.iter().filter(|x| x.driver == driver && dates::month_of(&x.date) == month).count()
}

/// An extra duty of `date` given driver `driver` (counted; given many this month, more stressed).
pub fn count_extra(c: &mut Company, driver: u32, date: &str) {
    c.planning.ops.extras.push(Extra { driver, date: date.to_string() });
    if extras_in_month(c, driver, date) > TIRED_FROM {
        if let Some(e) = c.staff.iter_mut().find(|e| e.id == driver) {
            e.stress = (e.stress + TIRED_STRESS).clamp(0.0, 100.0);
        }
    }
}

/// An extra duty given back (taken back on the page): no longer counted.
pub fn uncount_extra(c: &mut Company, driver: u32, date: &str) {
    if let Some(i) = c.planning.ops.extras.iter().rposition(|x| x.driver == driver && x.date == date) {
        c.planning.ops.extras.remove(i);
    }
}

/// What a driver says who is never asked.
pub const NEVER_ASKED: &str = "I never get asked about extra duties.";
/// The content a grumble costs.
pub const GRUMBLE_COST: f64 = 2.0;
/// Others given this many this month make the one given none grumble.
pub const OTHERS_MANY: usize = 3;

/// Whether driver `e` wants more hours (and could have taken an extra duty): hourly paid, or part
/// time.
pub fn wants_hours(e: &super::model::Employee) -> bool {
    matches!(e.employment, Employment::Hourly | Employment::PartTime { .. })
}

/// The company's day: a driver who wants hours, given none this month while another was given
/// `OTHERS_MANY` or more, says so to the control room - once a week at most (`Ops::grumbled`),
/// from the month's second week - and is a little less content. Who grumbled.
pub fn grumbles(c: &mut Company) -> Vec<u32> {
    let today = c.date.clone();
    if !super::pro_duties::on(c) || today.get(8..10).and_then(|d| d.parse::<u32>().ok()).unwrap_or(1) < 8 {
        return Vec::new();
    }
    let drivers: Vec<u32> = c.staff.iter().filter(|e| staff::drives(e) && e.employed_on(&today) && !e.absent(&today)).map(|e| e.id).collect();
    let most = drivers.iter().map(|id| extras_in_month(c, *id, &today)).max().unwrap_or(0);
    if most < OTHERS_MANY {
        return Vec::new();
    }
    let mut out = Vec::new();
    for id in drivers {
        let Some(e) = c.employee(id) else { continue };
        if !wants_hours(e) || extras_in_month(c, id, &today) > 0 {
            continue;
        }
        if c.planning.ops.grumbled.iter().any(|g| g.driver == id && dates::between(&g.date, &today) < 7) {
            continue;
        }
        let name = e.name.clone();
        if let Some(e) = c.staff.iter_mut().find(|e| e.id == id) {
            e.satisfaction = (e.satisfaction - GRUMBLE_COST).clamp(0.0, 100.0);
        }
        let from = messages::From::Driver { line: String::new(), tour: String::new(), name };
        messages::send(c, from, messages::To::Control, messages::Priority::Info, NEVER_ASKED, Vec::new(), false, None);
        c.planning.ops.grumbled.retain(|g| g.driver != id);
        c.planning.ops.grumbled.push(Extra { driver: id, date: today.clone() });
        out.push(id);
    }
    out
}

/// The counts of months gone dropped (this month and the one before kept), and the drivers gone.
pub fn tidy(c: &mut Company) {
    let month = dates::month_of(&c.date);
    let before = dates::month_of(&dates::add(&format!("{month}-01"), -1));
    let drivers: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    c.planning.ops.extras.retain(|x| drivers.contains(&x.driver) && (dates::month_of(&x.date) == month || dates::month_of(&x.date) == before || dates::between(&c.date, &x.date) > 0));
    c.planning.ops.grumbled.retain(|x| drivers.contains(&x.driver) && dates::between(&x.date, &c.date) < 14);
}

/// Every English text of this module.
pub fn texts() -> Vec<&'static str> {
    vec![SERVER_DECIDES, NEVER_ASKED, "%{name}: I never get asked about extra duties."]
}
