//! The drivers' pay (the UK model, `econ_uk`): by the paid hour of duty, no longer a flat monthly
//! wage. A driver's day is paid from the duties the plan gave them - RosterFlow's or the standard
//! planning's (`day::close_day`): from signing on before the first piece of work to signing off
//! after the last, a split unpaid, at least the least a duty pays (the company's RosterFlow
//! settings: `rosterflow::Prefs`, `paid_minutes`). An hour is the driver's contract wage by the
//! hour (`staff::hourly`), and above the contracted hours of the week (`CONTRACT_WEEK_HOURS`) half
//! as much again (`OVERTIME_RATE`; not in Casual). The week guarantees its hours for the days a
//! driver was there: whoever drove less is paid the rest when the week ends (`week`). Holidays and
//! sick days are paid through the allowances on every hour (`econ_uk::ALLOWANCES`), so a day away
//! adds nothing to the guarantee; a day on a course does.
//!
//! The day's hours are booked with the day (`Wages`), the week's guarantee on its last day, with
//! the on-costs and allowances (`economy::employer_cost`'s share) on both.
//!
//! (NexiconDK, 4 November 2026) Each driver by their employment (`model::Employment`): a full
//! week is 37 hours at the Danish price level, else 39 (`econ_uk::contract_week_hours`); a
//! part-time driver's guarantee and overtime follow their own week (pro rata); a reserve is paid
//! their frame on its days whether used or not, and the work outside it on top; the hourly paid
//! have no guarantee, only the hours worked - at least three a day called in, without the duty's
//! floor - at 10 % more an hour, overtime above a full week. The bus company's week begins on
//! Sunday: the guarantee is settled on Saturday night (`week_ends`).
//!
//! (NexiconDK, 5 November 2026: RosterFlow Pro, round B) With RosterFlow Pro the planner the hours
//! of the evening, the night and the weekend pay a supplement (`supplement_rate`: Danish-style -
//! evening 18-23 15 %, night 23-06 25 %, Saturday from 14:00 25 %, Sunday all day 35 %, the highest
//! for each minute; round D: a public holiday pays as a Sunday, `pro_holiday`); the
//! overtime of a duty given as time off in lieu (`pro_ops::Call::toil`) is not paid but saved
//! (`Employee::toil`, hour for hour), and a day of time off in lieu (`model::FreeDay`) pays its
//! hours at the normal rate from the saved ones (`pro_toil`). A company planning otherwise keeps
//! the pay above.

use super::dates;
use super::econ_uk;
use super::economy::{self, EMPLOYER_SHARE};
use super::model::{BookingKind, Cents, Company, Employee, Employment, Frame};
use super::rosterflow::{self, Prefs};
use super::staff;

/// The minutes a driver's day is paid, from its pieces of work (minutes of the day, the bus's
/// checks before and after it included): as RosterFlow reckons a duty - signing on before the
/// first piece and off after the last, a gap of `Prefs::split_min` or more a split (unpaid, its
/// sign-off and sign-on paid), a shorter one paid, and at least the paid floor
/// (`rosterflow::paid_floor`). Nothing without a piece.
pub fn paid_minutes(p: &Prefs, pieces: &[(i32, i32)]) -> i32 {
    match span_minutes(p, pieces) {
        0 => 0,
        m => m.max(rosterflow::paid_floor(p)),
    }
}

/// (NexiconDK, 4 November 2026) The minutes of a day's pieces from signing on to signing off,
/// a split unpaid (`paid_minutes` without the duty's floor). Nothing without a piece.
pub fn span_minutes(p: &Prefs, pieces: &[(i32, i32)]) -> i32 {
    let mut ps: Vec<(i32, i32)> = pieces.iter().copied().filter(|x| x.1 >= x.0).collect();
    if ps.is_empty() {
        return 0;
    }
    ps.sort_unstable();
    // (pieces that overlap or touch are one)
    let mut merged: Vec<(i32, i32)> = Vec::new();
    for x in ps {
        match merged.last_mut() {
            Some(m) if x.0 <= m.1 => m.1 = m.1.max(x.1),
            _ => merged.push(x),
        }
    }
    let start = merged[0].0 - p.sign_on;
    let end = merged[merged.len() - 1].1 + p.sign_off;
    let unpaid: i32 = merged.windows(2).map(|w| w[1].0 - w[0].1).filter(|g| *g >= p.split_min).map(|g| (g - p.sign_off_split - p.sign_on_split).max(0)).sum();
    (end - start - unpaid).max(1)
}

/// The contracted minutes of a full week in a company (37 hours at the Danish price level, else
/// 39: `econ_uk::contract_week_hours`).
pub fn contract_week_minutes(c: &Company) -> f64 {
    econ_uk::contract_week_hours(c) * 60.0
}

/// (NexiconDK, 4 November 2026) The bus company's week begins on Sunday (`dates::weekday`'s 6):
/// the contract week - its guarantee, its overtime, the hours "this week" - runs Sunday to
/// Saturday, for every company (the drivers' five working days, `staff::WEEK_DAYS`, still count
/// from Monday).
pub const WEEK_BEGINS: u32 = 6;

/// Whether the contract week ends with `date` (a Saturday).
pub fn week_ends(date: &str) -> bool {
    dates::parse(&dates::add(date, 1)).map(dates::weekday) == Some(WEEK_BEGINS)
}

/// (NexiconDK, 4 November 2026) What the hourly paid earn more an hour (no guarantee in return).
pub const HOURLY_PREMIUM: f64 = 0.10;
/// The least a day an hourly-paid driver is called in for pays (minutes).
pub const HOURLY_MIN_CALL: i32 = 180;

/// A driver's hour against their wage's (`staff::hourly`): the hourly paid's premium.
pub fn rate_of(e: &Employee) -> f64 {
    if e.employment == Employment::Hourly {
        1.0 + HOURLY_PREMIUM
    } else {
        1.0
    }
}

/// (Round L, NexiconDK 7 November 2026) A day of a frame - a reserve's own, or one of his rota,
/// named (RAM1 ...) or not - pays the hours driven, at least `least` (`frame_least`): the frame is
/// the window a duty may come in, not work.
pub fn frame_day_minutes(p: &Prefs, least: i32, pieces: &[(i32, i32)]) -> i32 {
    paid_minutes(p, pieces).max(least.max(0))
}

/// (Round L) The least a frame day of `window` minutes pays driver `e`: 7:24
/// (`pro_turnus::NAMED_LEAST`, a fifth of 37 hours), a part-timer's contract week shared over
/// their frame's days when that is less (30 hours over five days: 6:00), never more than the
/// window. `full`: a full week's minutes.
pub fn frame_least(e: &Employee, window: i32, full: f64) -> i32 {
    let mut least = super::pro_turnus::NAMED_LEAST;
    if let Employment::Reserve(f) = e.employment {
        let days = f.day_count().max(1) as f64;
        least = least.min((f.contract_minutes(full) / days).round() as i32);
    }
    least.min(window.max(0))
}

/// (NexiconDK, 4 November 2026) A driver's day by their employment: the minutes paid, the
/// minutes their week's guarantee owes for the day, and the minutes of their contract week
/// (overtime above them). `full`: a full week's minutes; `away`: ill or on holiday (paid through
/// the allowances).
pub fn day_minutes(p: &Prefs, e: &Employee, weekday: u8, full: f64, away: bool, pieces: &[(i32, i32)]) -> (f64, f64, f64) {
    match e.employment {
        Employment::FullTime => (paid_minutes(p, pieces) as f64, if away { 0.0 } else { full / 7.0 }, full),
        Employment::PartTime { hours } => {
            let week = (hours * 60.0).clamp(60.0, full);
            (paid_minutes(p, pieces) as f64, if away { 0.0 } else { week / 7.0 }, week)
        }
        // (round L: a contract week as a permanent driver's, guaranteed for the days they are
        // there; a day of the frame the hours driven, at least 7:24 - `frame_day_minutes`)
        Employment::Reserve(f) => {
            let week = f.contract_minutes(full);
            let paid = if f.on(weekday) && !away { frame_day_minutes(p, frame_least(e, f.minutes(), full), pieces) } else { paid_minutes(p, pieces) };
            (paid as f64, if away { 0.0 } else { week / 7.0 }, week)
        }
        Employment::Hourly => {
            let paid = match span_minutes(p, pieces) {
                0 => 0,
                m => m.max(HOURLY_MIN_CALL),
            };
            (paid as f64, 0.0, full)
        }
    }
}

/// A day's paid minutes (`paid`, by the employment) as a frame of the driver's rota changes them
/// (`framed`: what his rota decides of the day's frame, None when no rota decides it).
fn framed_paid(p: &Prefs, e: &Employee, away: bool, framed: Option<Option<(Frame, bool)>>, mine: &[(i32, i32)], paid: f64, full: f64) -> f64 {
    match framed {
        None => paid,
        // (round F: a named frame of his rota - RAM1 ... - pays the hours driven, at least 7.4:
        // its window is not work; round L: a frame of his rota of its own times the same)
        Some(Some((f, _))) if !away => frame_day_minutes(p, frame_least(e, f.minutes(), full), mine) as f64,
        // (a reserve whose rota has no frame that day: the duties he drives)
        Some(_) if matches!(e.employment, Employment::Reserve(_)) => paid_minutes(p, mine) as f64,
        Some(_) => paid,
    }
}

/// (NexiconDK, 4 November 2026: RosterFlow Pro's calendar, step 4) The minutes driver `e`'s day
/// of `date` is paid with the pieces of work `pieces` (minutes of the day), as `day` pays it: by
/// his employment, a frame of his rota, nothing for a day ill or on holiday (the allowances pay
/// it) - the hours of his week the calendar adds up.
pub fn planned_minutes(c: &Company, e: &Employee, date: &str, pieces: &[(i32, i32)]) -> f64 {
    let p = &c.planning.flow.prefs;
    let weekday = dates::parse(date).map(|d| dates::weekday(d) as u8).unwrap_or(0);
    let away = away_paid_otherwise(e, date);
    if away {
        return 0.0;
    }
    let (paid, _, _) = day_minutes(p, e, weekday, contract_week_minutes(c), away, pieces);
    framed_paid(p, e, away, super::pro_turnus::decides_frame_named(c, e.id, date), pieces, paid, contract_week_minutes(c))
}

/// What an hour of driver `e` costs in wages (cents, without the on-costs): his wage's hour, the
/// hourly paid's premium on it (`rate_of`).
pub fn hour_wage(e: &Employee) -> f64 {
    staff::hourly(e.wage) * rate_of(e)
}

/// Away so that the day is paid through the allowances (ill or on holiday), not by the guarantee.
fn away_paid_otherwise(e: &Employee, date: &str) -> bool {
    e.ill_on(date) || e.on_holiday(date).is_some()
}

// --- RosterFlow Pro: supplements, time off in lieu (round B) ------------------------------------

/// The supplements on an hour's pay (shares of it), by when it is worked.
pub const EVENING: f64 = 0.15;
pub const NIGHT: f64 = 0.25;
pub const SATURDAY: f64 = 0.25;
pub const SUNDAY: f64 = 0.35;

/// The supplement on minute `m` of a day (0 = midnight; past 24:00 the next day's) of weekday
/// `weekday` (`dates::weekday`: 0 Monday ... 6 Sunday): the highest of evening (18-23), night
/// (23-06), Saturday from 14:00, Sunday all day.
pub fn supplement_rate(weekday: u32, m: i32) -> f64 {
    supplement_rate_on(weekday, [false; 2], m)
}

/// (round D) As `supplement_rate`, with the public holidays (`holiday`: the day and the next one,
/// `pro_holiday::days`): a holiday is paid as a Sunday all day.
pub fn supplement_rate_on(weekday: u32, holiday: [bool; 2], m: i32) -> f64 {
    let next = m.div_euclid(1440);
    let wd = (weekday as i32 + next).rem_euclid(7);
    let m = m.rem_euclid(1440);
    let mut r: f64 = 0.0;
    if (18 * 60..23 * 60).contains(&m) {
        r = r.max(EVENING);
    }
    if !(6 * 60..23 * 60).contains(&m) {
        r = r.max(NIGHT);
    }
    if wd == 5 && m >= 14 * 60 {
        r = r.max(SATURDAY);
    }
    if wd == 6 || (matches!(next, 0 | 1) && holiday[next as usize]) {
        r = r.max(SUNDAY);
    }
    r
}

/// The supplements of a day's pieces of work (minutes of the day) on weekday `weekday`: the
/// minutes worked, each by its supplement (`supplement_rate`) - an hour of 15 % is 9 minutes.
pub fn supplement_minutes(weekday: u32, pieces: &[(i32, i32)]) -> f64 {
    supplement_minutes_on(weekday, [false; 2], pieces)
}

/// (round D) As `supplement_minutes`, with the public holidays (`supplement_rate_on`).
pub fn supplement_minutes_on(weekday: u32, holiday: [bool; 2], pieces: &[(i32, i32)]) -> f64 {
    let mut ps: Vec<(i32, i32)> = pieces.iter().copied().filter(|x| x.1 > x.0).collect();
    ps.sort_unstable();
    let mut done = i32::MIN;
    let mut out = 0.0;
    for (a, b) in ps {
        // (pieces that overlap counted once; by quarters of an hour is near enough and quick)
        let mut m = a.max(done);
        while m < b {
            let step = (b - m).min(15);
            out += supplement_rate_on(weekday, holiday, m) * step as f64;
            m += step;
        }
        done = done.max(b);
    }
    out
}

/// The supplements of a day (`date`) of pieces `pieces`, in cents of driver `e`'s wages (without
/// the on-costs): none when RosterFlow Pro does not plan the company.
pub fn supplement_cost(c: &Company, e: &Employee, date: &str, pieces: &[(i32, i32)]) -> f64 {
    if !super::pro_duties::on(c) {
        return 0.0;
    }
    let wd = dates::parse(date).map(dates::weekday).unwrap_or(0);
    hour_wage(e) * supplement_minutes_on(wd, super::pro_holiday::days(c, date), pieces) / 60.0
}

/// Driver `e`'s day of time off in lieu on `date`: its minutes (paid from the saved hours).
pub fn toil_day(e: &Employee, date: &str) -> Option<f64> {
    e.free_on(date).filter(|f| f.why == super::model::FreeWhy::Toil).map(|f| f.minutes)
}

/// What the drivers' pay of a day came to (the day's report shows it).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Paid {
    /// Hours paid for duties, of them overtime, and hours paid by the week's guarantee.
    pub hours: f64,
    pub overtime: f64,
    pub guaranteed: f64,
    /// What it cost, with the on-costs and allowances.
    pub cost: Cents,
    /// (RosterFlow Pro) Hours of overtime saved as time off in lieu, not paid (`Call::toil`).
    pub saved: f64,
    /// (RosterFlow Pro, round E: the Monday report) Of the cost, the supplements and the hours of
    /// overtime paid (cents, rounded).
    pub supplements: Cents,
    pub overtime_cost: Cents,
}

/// What a driver's minutes cost with the on-costs and allowances (`over`: of them overtime).
fn cost_of(e: &Employee, minutes: f64, over: f64) -> f64 {
    staff::hourly(e.wage) * rate_of(e) * (minutes - over + over * econ_uk::OVERTIME_RATE) / 60.0 * (1.0 + EMPLOYER_SHARE)
}

/// The drivers' day (`day::close_day`, before the night): `pieces` are the pieces of work each
/// driver of the staff had (id, from, to: minutes of the day). Every driver there that day earns
/// a seventh of the week's guarantee; the hours of their duties are paid and booked, overtime
/// above the contracted week; and on the week's last day the guarantee pays who drove less
/// (`week`).
pub fn day(c: &mut Company, date: &str, pieces: &[(u32, i32, i32)]) -> Paid {
    let p = c.planning.flow.prefs.clone();
    let overtime = econ_uk::mode(c.game_mode).overtime;
    let full = contract_week_minutes(c);
    let weekday = dates::parse(date).map(|d| dates::weekday(d) as u8).unwrap_or(0);
    let mut out = Paid::default();
    let mut cost = 0.0;
    let (mut supp, mut over_cost) = (0.0, 0.0);
    let mut drivers = 0;
    // (RosterFlow Pro's step 3: the frame a driver's rota gives him that day - `pro_turnus`)
    let framed: Vec<(u32, Option<(Frame, bool)>)> = c.staff.iter().filter_map(|e| super::pro_turnus::decides_frame_named(c, e.id, date).map(|f| (e.id, f))).collect();
    // (RosterFlow Pro, round B: the supplements, the overtime of a duty given as time off in lieu
    // saved, a day of time off in lieu paid from the saved hours)
    let pro = super::pro_duties::on(c);
    let saving: Vec<u32> = if pro { c.planning.ops.calls.iter().filter(|x| x.date == date && x.toil).map(|x| x.driver).collect() } else { Vec::new() };
    let wd = dates::parse(date).map(dates::weekday).unwrap_or(0);
    // (round D: a public holiday paid as a Sunday - `pro_holiday`)
    let hol = super::pro_holiday::days(c, date);
    for e in c.staff.iter_mut().filter(|e| staff::drives(e) && e.employed_on(date)) {
        let mine: Vec<(i32, i32)> = pieces.iter().filter(|x| x.0 == e.id).map(|x| (x.1, x.2)).collect();
        // (NexiconDK, 4 November 2026: by their employment - `day_minutes`)
        let toil = if pro { toil_day(e, date) } else { None };
        let away = away_paid_otherwise(e, date) || toil.is_some();
        if let Some(m) = toil.filter(|m| *m > 0.0) {
            cost += cost_of(e, m, 0.0);
            out.hours += m / 60.0;
        }
        let (paid, due, contract) = day_minutes(&p, e, weekday, full, away, &mine);
        let paid = framed_paid(&p, e, away, framed.iter().find(|x| x.0 == e.id).map(|x| x.1), &mine, paid, full);
        e.week_due += due;
        if paid <= 0.0 {
            continue;
        }
        let before = e.week_paid;
        let after = before + paid;
        let over = if overtime { (after - contract).max(0.0) - (before - contract).max(0.0) } else { 0.0 };
        if pro {
            let x = staff::hourly(e.wage) * rate_of(e) * supplement_minutes_on(wd, hol, &mine) / 60.0 * (1.0 + EMPLOYER_SHARE);
            cost += x;
            supp += x;
        }
        if over > 0.0 && saving.contains(&e.id) && matches!(e.employment, Employment::FullTime | Employment::PartTime { .. }) {
            // (saved: the overtime's hours not paid now, his to take off later)
            cost += cost_of(e, paid - over, 0.0);
            e.toil += over;
            out.saved += over / 60.0;
        } else {
            cost += cost_of(e, paid, over);
            over_cost += cost_of(e, over, over);
        }
        e.week_paid = after;
        out.hours += paid / 60.0;
        out.overtime += over / 60.0;
        drivers += 1;
    }
    let booked = cost.round() as Cents;
    if booked > 0 {
        c.book(BookingKind::Wages, -booked, format!("Drivers: paid hours of the day ({drivers}, {:.0} h)", out.hours), false);
    }
    out.cost += booked;
    (out.supplements, out.overtime_cost) = (supp.round() as Cents, over_cost.round() as Cents);
    // (the week ends tonight - a Saturday: `week_ends` -: its guarantee)
    if week_ends(date) {
        let w = week(c);
        out.guaranteed += w.guaranteed;
        out.cost += w.cost;
    }
    out
}

/// The week's end: every driver paid the guaranteed hours they did not drive (booked together),
/// and a new week begun for everyone.
pub fn week(c: &mut Company) -> Paid {
    let mut out = Paid::default();
    let mut cost = 0.0;
    let mut drivers = 0;
    for e in c.staff.iter_mut() {
        let short = (e.week_due - e.week_paid).max(0.0);
        if staff::drives(e) && short > 0.5 {
            cost += cost_of(e, short, 0.0);
            out.guaranteed += short / 60.0;
            drivers += 1;
        }
        e.week_paid = 0.0;
        e.week_due = 0.0;
    }
    let booked = cost.round() as Cents;
    if booked > 0 {
        c.book(BookingKind::Wages, -booked, format!("Drivers: guaranteed hours of the week ({drivers}, {:.0} h)", out.guaranteed), false);
    }
    out.cost = booked;
    out
}

/// A driver who leaves: the guaranteed hours of their week not driven, paid (booked with their
/// name). Returns what it cost.
pub fn settle(c: &mut Company, id: u32) -> Cents {
    let Some(e) = c.staff.iter_mut().find(|e| e.id == id) else { return 0 };
    let short = (e.week_due - e.week_paid).max(0.0);
    let cost = if staff::drives(e) { cost_of(e, short, 0.0).round() as Cents } else { 0 };
    (e.week_paid, e.week_due) = (0.0, 0.0);
    let name = e.name.clone();
    if cost > 0 {
        c.book(BookingKind::Wages, -cost, name, false);
    }
    cost
}

/// What a paid hour of a driver costs the company today, all in: the market's wage of a driver
/// of some experience (`economy::market_wage`) by the hour, with the on-costs and allowances.
pub fn driver_hour(c: &Company) -> f64 {
    let wage = economy::market_wage(45.0, &econ_uk::Level::of(c));
    staff::hourly(economy::employer_cost(wage))
}

/// An agency driver's hour today: a premium on a driver's (`econ_uk::AGENCY_AHEAD` booked ahead,
/// `AGENCY_SAME_DAY` sent on the day itself).
pub fn agency_hour(c: &Company, ahead: bool) -> f64 {
    driver_hour(c) * if ahead { econ_uk::AGENCY_AHEAD } else { econ_uk::AGENCY_SAME_DAY }
}

#[cfg(test)]
#[path = "employment_tests.rs"]
mod employment_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_is_paid_from_signing_on_to_signing_off() {
        let p = Prefs::default();
        // one piece 06:00-13:50: signed on 10 minutes before, off 5 after
        assert_eq!(paid_minutes(&p, &[(360, 830)]), 470 + p.sign_on + p.sign_off);
        // a short one is paid the floor
        assert_eq!(paid_minutes(&p, &[(600, 660)]), rosterflow::paid_floor(&p));
        // two pieces with a short break between: the break is paid
        assert_eq!(paid_minutes(&p, &[(360, 600), (630, 840)]), 480 + p.sign_on + p.sign_off);
        // a split: the gap unpaid but its signing
        let split = paid_minutes(&p, &[(360, 540), (900, 1080)]);
        assert_eq!(split, 360 + p.sign_on + p.sign_off + p.sign_off_split + p.sign_on_split);
        assert_eq!(paid_minutes(&p, &[]), 0);
    }
}
