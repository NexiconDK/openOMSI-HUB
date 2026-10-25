//! (NexiconDK) Bus Company Online's fines on a line of a contract won in the map's market
//! (decided 25 October 2026, docs/omsihub/BUS_COMPANY_ONLINE.md): what the authority fines the
//! company for a journey not run (by the minute of its time), a tour run with a bus smaller than
//! its timetable asks for, a diesel bus on a tour of a contract won with electric buses (not the
//! other way round), each departure from a stop more than a minute early or - not reported to the
//! control room (`report_late`) - more than 15 minutes late, and each complaint about the
//! driving, the temperature in the bus, the route driven, a ticket sold wrong and a red light
//! run. And the most the authority pays a timetable hour (`cap_per_hour`): the companies bid by
//! the timetable hour on the server.
//!
//! The amounts are Danish kroner, in the main server's `company-server.cfg` (`FINES_CFG`), and
//! each company pays them in its own currency (`Fines::cents`).

use super::currency::Currency;
use super::model::{Cents, Company};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The fines, in kroner.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Fines {
    /// A journey not run (or not all of it): each minute of its timetabled time.
    pub dropped_minute: f64,
    /// A journey run with a bus smaller than its tour asks for.
    pub wrong_bus: f64,
    /// A journey of a contract won with electric buses run with a diesel bus.
    pub diesel_on_electric: f64,
    /// A complaint: the driving, the temperature in the bus, the route driven, a ticket sold
    /// wrong, a red light run.
    pub driving: f64,
    pub temperature: f64,
    pub route: f64,
    pub ticket: f64,
    pub red_light: f64,
    /// A departure from a stop more than `EARLY_SECONDS` early, or more than `LATE_MINUTES` late
    /// without the delay reported to the control room.
    pub early: f64,
    pub late: f64,
}

/// A departure is early from more than this many seconds before its time, and late - unless
/// reported - from more than this many minutes after it.
pub const EARLY_SECONDS: f64 = 60.0;
pub const LATE_MINUTES: i32 = 15;

/// The most the authority pays a timetable hour when the settings say nothing (kroner).
pub const CAP_PER_HOUR: f64 = 1500.0;

impl Default for Fines {
    fn default() -> Self {
        Fines { dropped_minute: 20.83, wrong_bus: 2500.0, diesel_on_electric: 2500.0, driving: 2000.0, temperature: 1500.0, route: 2500.0, ticket: 2500.0, red_light: 2500.0, early: 2500.0, late: 2500.0 }
    }
}

impl Fines {
    /// Kroner in euro cents (the books' money).
    pub fn cents(kr: f64) -> Cents {
        Currency::Dkk.cents(kr.max(0.0))
    }

    /// The fines of the settings' keys (`fine_…`), the defaults where one is missing or not a
    /// number (a decimal comma read as a point).
    pub fn of(kv: &HashMap<String, String>) -> Fines {
        let d = Fines::default();
        let f = |k: &str, d: f64| kv.get(k).and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok()).filter(|x| x.is_finite() && *x >= 0.0).unwrap_or(d);
        Fines {
            dropped_minute: f("fine_dropped_minute", d.dropped_minute),
            wrong_bus: f("fine_wrong_bus", d.wrong_bus),
            diesel_on_electric: f("fine_diesel_on_electric", d.diesel_on_electric),
            driving: f("fine_driving", d.driving),
            temperature: f("fine_temperature", d.temperature),
            route: f("fine_route", d.route),
            ticket: f("fine_ticket", d.ticket),
            red_light: f("fine_red_light", d.red_light),
            early: f("fine_early", d.early),
            late: f("fine_late", d.late),
        }
    }
}

/// The settings' keys and values of the main server's folder `data` (`company-server.cfg`).
fn settings(data: &std::path::Path) -> HashMap<String, String> {
    let text = std::fs::read_to_string(data.join(super::main_server::CFG_FILE)).unwrap_or_default();
    text.lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// The most the authority pays a timetable hour on the server of folder `data`, in euro cents
/// (`cap_per_hour` in kroner: `CAP_PER_HOUR` when it says nothing).
pub fn cap_per_hour(data: &std::path::Path) -> Cents {
    let kr = settings(data).get("cap_per_hour").and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok()).filter(|x| x.is_finite() && *x > 0.0).unwrap_or(CAP_PER_HOUR);
    Fines::cents(kr)
}

/// A player's delay reported to the control room (`report_late`): his tour, from that minute of
/// the company's day on.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct LateReport {
    pub date: String,
    pub line: String,
    pub tour: String,
    pub minute: i32,
}

/// Why a delay is not reported, and that it was.
pub const NOT_DRIVING: &str = "You are not driving a duty now.";
pub const REPORTED: &str = "Your delay is reported to the control room: the authority does not fine your late departures from now on.";

/// Player `guest` reports to the control room that he runs late on the duty his game drives now:
/// from now on his departures more than `LATE_MINUTES` late are not fined (`reported`).
pub fn report_late(c: &mut Company, guest: u32) -> Result<(), &'static str> {
    use super::together::book::ClaimState;
    let date = c.date.clone();
    let x = c.together.claims.iter().find(|x| x.guest == guest && x.state == ClaimState::Driving && x.duty.date == date).ok_or(NOT_DRIVING)?;
    let r = LateReport { date: date.clone(), line: x.duty.line.clone(), tour: x.duty.tour.clone(), minute: c.clock.minute as i32 };
    c.together.late_reports.retain(|x| super::dates::between(&x.date, &date) <= 2);
    c.together.late_reports.push(r);
    Ok(())
}

/// Whether a delay of tour `line`/`tour` on `date` was reported by `minute` of the day.
pub fn reported(c: &Company, date: &str, line: &str, tour: &str, minute: i32) -> bool {
    c.together.late_reports.iter().any(|r| r.date == date && r.line.eq_ignore_ascii_case(line) && r.tour.trim() == tour.trim() && r.minute <= minute)
}

/// A player's trip's departures from its stops more than `EARLY_SECONDS` early and more than
/// `LATE_MINUTES` late (not those of a delay reported by then: `reported`, by its tour).
pub fn departures(c: &Company, date: &str, t: &crate::TripRun, tour: Option<(&str, &str)>) -> (u32, u32) {
    let (mut early, mut late) = (0, 0);
    for v in &t.visits {
        let Some(left) = v.left else { continue };
        if left < v.dep - EARLY_SECONDS {
            early += 1;
        } else if left > v.dep + LATE_MINUTES as f64 * 60.0 && !tour.is_some_and(|(l, n)| reported(c, date, l, n, (left / 60.0) as i32)) {
            late += 1;
        }
    }
    (early, late)
}

/// The fines of the main server's folder `data` (its `company-server.cfg`, read as it is: the
/// defaults when it has none).
pub fn of_folder(data: &std::path::Path) -> Fines {
    Fines::of(&settings(data))
}

/// The fines' part of the settings, added to a `company-server.cfg` written before it.
pub const FINES_CFG: &str = "
# the authority's fines on a line of a contract won in the map's tenders (Danish kroner; each
# company pays them in its own currency): a journey not run, by the minute of its time; a tour
# run with a smaller bus than its timetable asks for, and a diesel bus on a tour of a contract
# won with electric buses, by the journey; and each complaint about the driving, the
# temperature in the bus, the route driven, a ticket sold wrong and a red light run
fine_dropped_minute = 20.83
fine_wrong_bus = 2500
fine_diesel_on_electric = 2500
fine_driving = 2000
fine_temperature = 1500
fine_route = 2500
fine_ticket = 2500
fine_red_light = 2500
# a departure from a stop more than a minute early, or more than 15 minutes late without the
# delay reported to the control room (by the departure)
fine_early = 2500
fine_late = 2500
# the companies bid by the timetable hour: the most the authority pays one (kroner)
cap_per_hour = 1500
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fines_are_read_in_kroner_with_a_decimal_comma_too() {
        let mut kv = HashMap::new();
        kv.insert("fine_dropped_minute".to_string(), "20,83".to_string());
        kv.insert("fine_driving".to_string(), "x".to_string());
        kv.insert("fine_route".to_string(), "3000".to_string());
        let f = Fines::of(&kv);
        assert_eq!((f.dropped_minute, f.driving, f.route, f.red_light), (20.83, 2000.0, 3000.0, 2500.0));
        // (2 500 kr in the books' euro cents, and back)
        let c = Fines::cents(2500.0);
        assert!((Currency::Dkk.units(c as f64) - 2500.0).abs() < 0.5, "{c}");
    }

    /// A player's trip's departures: more than a minute early, more than 15 minutes late - not
    /// those after his delay was reported to the control room; reported only while he drives.
    #[test]
    fn early_and_late_departures_and_a_reported_delay() {
        use crate::{StopVisit, TripRun};
        let mut c = super::super::found(&super::super::Founding { name: "Late".into(), date: "2024-03-04".into(), ..Default::default() }, "x");
        assert_eq!(report_late(&mut c, 1), Err(NOT_DRIVING));
        let stop = |i: usize, dep: f64, left: f64| StopVisit { index: i, name: format!("S{i}"), arr: dep, dep, arrived: Some(left), left: Some(left) };
        // (06:00: 90 s early; 06:10 on time; 06:20: 16 min late; 06:30: 20 min late)
        let t = TripRun { visits: vec![stop(0, 21600.0, 21510.0), stop(1, 22200.0, 22230.0), stop(2, 22800.0, 23760.0), stop(3, 23400.0, 24600.0)], ..Default::default() };
        assert_eq!(departures(&c, "2024-03-04", &t, Some(("Linie5", "1"))), (1, 2));
        // (reported at 06:40: the 06:30's departure at 06:50 is not fined, the 06:20's at 06:36 is)
        c.together.late_reports.push(LateReport { date: "2024-03-04".into(), line: "Linie5".into(), tour: "1".into(), minute: 400 });
        assert_eq!(departures(&c, "2024-03-04", &t, Some(("Linie5", "1"))), (1, 1));
        assert_eq!(departures(&c, "2024-03-04", &t, Some(("Linie5", "2"))), (1, 2), "another tour");
    }
}
