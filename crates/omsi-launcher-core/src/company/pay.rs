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

/// The minutes of a reserve's pieces of work outside their frame, their signing on before it
/// and off after it with them.
fn outside_frame(p: &Prefs, f: &Frame, pieces: &[(i32, i32)]) -> i32 {
    let ps: Vec<(i32, i32)> = pieces.iter().copied().filter(|x| x.1 >= x.0).collect();
    let (Some(first), Some(last)) = (ps.iter().map(|x| x.0).min(), ps.iter().map(|x| x.1).max()) else { return 0 };
    let mut m: i32 = ps.iter().map(|&(a, b)| (b.min(f.from) - a).max(0) + (b - a.max(f.to)).max(0)).sum();
    if first < f.from {
        m += p.sign_on;
    }
    if last > f.to {
        m += p.sign_off;
    }
    m
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
        Employment::Reserve(f) => {
            let week = f.week_minutes() as f64;
            let paid = if f.on(weekday) && !away { f.minutes() + outside_frame(p, &f, pieces) } else { paid_minutes(p, pieces) };
            (paid as f64, 0.0, week)
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
fn framed_paid(p: &Prefs, e: &Employee, away: bool, framed: Option<Option<Frame>>, mine: &[(i32, i32)], paid: f64) -> f64 {
    match framed {
        None => paid,
        // (a frame of his rota: paid whether used or not, the work outside it on top)
        Some(Some(f)) if !away => (f.minutes() + outside_frame(p, &f, mine)).max(paid_minutes(p, mine)) as f64,
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
    framed_paid(p, e, away, super::pro_turnus::decides_frame(c, e.id, date), pieces, paid)
}

/// What an hour of driver `e` costs in wages (cents, without the on-costs): his wage's hour, the
/// hourly paid's premium on it (`rate_of`).
pub fn hour_wage(e: &Employee) -> f64 {
    staff::hourly(e.wage) * rate_of(e)
}

/// Away so that the day is paid through the allowances (ill or on holiday), not by the guarantee.
fn away_paid_otherwise(e: &Employee, date: &str) -> bool {
    e.sick_until.as_deref().is_some_and(|u| dates::between(date, u) >= 0) || e.on_holiday(date).is_some()
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
    let mut drivers = 0;
    // (RosterFlow Pro's step 3: the frame a driver's rota gives him that day - `pro_turnus`)
    let framed: Vec<(u32, Option<Frame>)> = c.staff.iter().filter_map(|e| super::pro_turnus::decides_frame(c, e.id, date).map(|f| (e.id, f))).collect();
    for e in c.staff.iter_mut().filter(|e| staff::drives(e) && e.employed_on(date)) {
        let mine: Vec<(i32, i32)> = pieces.iter().filter(|x| x.0 == e.id).map(|x| (x.1, x.2)).collect();
        // (NexiconDK, 4 November 2026: by their employment - `day_minutes`)
        let away = away_paid_otherwise(e, date);
        let (paid, due, contract) = day_minutes(&p, e, weekday, full, away, &mine);
        let paid = framed_paid(&p, e, away, framed.iter().find(|x| x.0 == e.id).map(|x| x.1), &mine, paid);
        e.week_due += due;
        if paid <= 0.0 {
            continue;
        }
        let before = e.week_paid;
        let after = before + paid;
        let over = if overtime { (after - contract).max(0.0) - (before - contract).max(0.0) } else { 0.0 };
        cost += cost_of(e, paid, over);
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
