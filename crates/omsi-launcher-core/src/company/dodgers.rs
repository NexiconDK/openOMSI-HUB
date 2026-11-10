//! (NexiconDK, ticket inspection round S, 9 November 2026) Fare dodgers: the share of a line's
//! passengers who ride without a ticket. Every company has them (Bus Company Online's and the
//! others alike): about one in twenty by day (`BASE`), more in the evening and at night (`EVENING`,
//! `NIGHT`) and on a bus full enough that nobody gets to the driver (`FULL`); fewer on a line the
//! ticket inspectors checked today and in the last two weeks (`deterrence`: what was seen keeps
//! people honest for a while, fading day by day), more so with their researched working method
//! (`research::inspection_factor`), and fewer on a trip whose driver took the ticket sales course
//! (`TRAINED` - the course's own `incidents::TICKETING_GAIN` stays what its fares get: the share
//! the inspectors meet on such a trip is lower).
//!
//! The fares a trip brings were always worked out net of the usual day's dodgers, so a company
//! without inspectors earns what it did: `fare_factor` is 1 at `BASE`, more below it (the line
//! the inspectors keep honest - what `staff::INSPECT_GAIN` did before), a little less above it
//! (the night buses). The inspectors' checks of the day and their fines are `inspection`.

use super::dates;
use super::model::Company;

/// One passenger in twenty rides without a ticket by day: the fares were always net of these.
pub const BASE: f64 = 0.05;
/// More in the evening (20:00 to 22:00) and at night (22:00 to 05:00).
pub const EVENING: f64 = 0.02;
pub const NIGHT: f64 = 0.045;
/// More on a crowded bus: nobody gets to the driver.
pub const FULL: f64 = 0.015;
/// Never fewer, never more.
pub const MIN: f64 = 0.01;
pub const MAX: f64 = 0.15;
/// How long an inspection is remembered (days), and what it does: a line checked all day keeps
/// `TODAY` of its dodgers off that day, and each of the days before adds `PAST`, fading to nothing
/// over `MEMORY` days; together never more than `DETER_MAX`.
pub const MEMORY: i64 = 14;
pub const TODAY: f64 = 0.6;
pub const PAST: f64 = 0.12;
pub const DETER_MAX: f64 = 0.85;
/// A driver trained in ticket sales: the dodgers on the trip.
pub const TRAINED: f64 = 0.8;

/// What the hour of a departure adds to the share.
pub fn time_extra(dep: i32) -> f64 {
    let m = dep.rem_euclid(1440);
    if m >= 22 * 60 || m < 5 * 60 {
        NIGHT
    } else if m >= 20 * 60 {
        EVENING
    } else {
        0.0
    }
}

/// Whether a departure is a night one (22:00 to 05:00).
pub fn at_night(dep: i32) -> bool {
    let m = dep.rem_euclid(1440);
    m >= 22 * 60 || m < 5 * 60
}

/// The share of a trip's passengers without a ticket before the inspectors: its hour, and a full
/// bus.
pub fn raw(dep: i32, crowded: bool) -> f64 {
    BASE + time_extra(dep) + if crowded { FULL } else { 0.0 }
}

/// The share of the day a line (its name) was checked on `date`, as the day's close kept it.
pub fn checked_on(c: &Company, date: &str, line: &str) -> f64 {
    c.inspection.days.iter().find(|d| d.date == date).and_then(|d| d.lines.iter().find(|l| l.line.eq_ignore_ascii_case(line))).map_or(0.0, |l| l.checked)
}

/// How much of a line's dodgers the inspectors keep off on `date` (0 to `DETER_MAX`): `today` is
/// the share of the day they check it today (`staff::inspected`), the days before are kept.
pub fn deterrence_with(c: &Company, date: &str, line: &str, today: f64) -> f64 {
    let past: f64 = (1..=MEMORY).map(|k| checked_on(c, &dates::add(date, -k), line) * (1.0 - k as f64 / (MEMORY + 1) as f64)).sum();
    ((TODAY * today.clamp(0.0, 1.0) + PAST * past) * super::research::inspection_factor(c)).min(DETER_MAX)
}

/// `deterrence_with` with today's checks of the inspectors at work.
pub fn deterrence(c: &Company, date: &str, line: &str) -> f64 {
    let today = super::staff::inspected(c, date).iter().find(|x| x.0.eq_ignore_ascii_case(line)).map_or(0.0, |x| x.1);
    deterrence_with(c, date, line, today)
}

/// The share of a trip's passengers without a ticket: its raw share, what the inspectors keep off,
/// and its driver's course.
pub fn share(raw: f64, deterrence: f64, trained: bool) -> f64 {
    (raw * (1.0 - deterrence.clamp(0.0, 1.0)) * if trained { TRAINED } else { 1.0 }).clamp(MIN, MAX)
}

/// The factor on a trip's tickets for its share of dodgers: 1 at `BASE` (the fares were always net
/// of the usual day's).
pub fn fare_factor(share: f64) -> f64 {
    (1.0 - share) / (1.0 - BASE)
}

/// Why a line's share is what it is (for the page): its night and evening passengers, its full
/// buses, and what the inspectors keep off.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Why {
    /// The line's usual share before the inspectors (its passengers' hours, its full buses).
    pub raw: f64,
    /// What it is now.
    pub share: f64,
    pub night: f64,
    pub full: f64,
    pub deterrence: f64,
}

/// A line's share now, by its last days kept (`inspection::LineDay`: the share of its passengers
/// at night and in the evening, of its trips full) and the inspectors' deterrence today.
pub fn of_line(c: &Company, line: &str) -> Why {
    let kept: Vec<&super::inspection::LineDay> = c.inspection.days.iter().rev().take(7).filter_map(|d| d.lines.iter().find(|l| l.line.eq_ignore_ascii_case(line))).collect();
    let pax: f64 = kept.iter().map(|l| l.passengers as f64).sum();
    let (night, evening, full) = if pax > 0.0 {
        (kept.iter().map(|l| l.night).sum::<f64>() / pax, kept.iter().map(|l| l.evening).sum::<f64>() / pax, kept.iter().map(|l| l.full).sum::<f64>() / pax)
    } else {
        (0.0, 0.0, 0.0)
    };
    let raw = BASE + NIGHT * night + EVENING * evening + FULL * full;
    let deterrence = deterrence(c, &c.date, line);
    Why { raw, share: share(raw, deterrence, false), night: night + evening, full, deterrence }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hour_and_a_full_bus_raise_the_share_and_the_fares_are_net_of_the_base() {
        assert_eq!(raw(10 * 60, false), BASE);
        assert!(raw(23 * 60, false) > raw(21 * 60, false) && raw(21 * 60, false) > raw(10 * 60, false));
        assert!((raw(2 * 60 + 1440, false) - (BASE + NIGHT)).abs() < 1e-9);
        assert!(raw(10 * 60, true) > raw(10 * 60, false));
        assert!((fare_factor(BASE) - 1.0).abs() < 1e-12);
        assert!(fare_factor(share(BASE, 0.5, false)) > 1.0);
        assert!(fare_factor(share(raw(23 * 60, true), 0.0, false)) < 1.0);
        // (clamped)
        assert_eq!(share(0.5, 0.0, false), MAX);
        assert_eq!(share(BASE, 1.0, false), MIN);
        assert!(share(BASE, 0.0, true) < share(BASE, 0.0, false));
    }
}
