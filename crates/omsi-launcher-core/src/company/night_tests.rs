//! (NexiconDK, 7 November 2026) A tour belongs to the day it begins on: the day before's night
//! tour, whose trips past 24:00 run that morning, is not one of the day's tours (it was planned,
//! staffed and booked a second time - a Sunday duty 00:40-05:12 of Saturday's night bus).

use super::super::model::{Company, CompanyLine};
use super::super::{day, found, plan, pro_turnus as pt, Founding};
use super::tours_of_day;
use super::super::dates;
use crate::{LineInfo, StopInfo, TourInfo, TripInfo};

/// Line TEST's timetable of `date` as the launcher reads it (`lines_on`; its flags are the
/// game's, `the_day_befores_night_tour_is_not_the_days_own`): a Friday tour, a Saturday night tour
/// (22:10 to 01:40, its last trips past midnight - on Sunday morning "running" as the day
/// before's night tour) and a Sunday tour.
fn lines(date: &str) -> Vec<LineInfo> {
    let trip = |k: usize, m: i32| {
        let dep = m as f64 * 60.0;
        let stops = (0..3).map(|i| StopInfo { name: format!("S{i}"), id: i, arr: dep + i as f64 * 1200.0, dep: dep + i as f64 * 1200.0 }).collect();
        TripInfo { name: "T".into(), index: k + 1, line: "T".into(), from: "S0".into(), terminus: "S2".into(), departure: dep, arrival: dep + 2400.0, stops, km: 12.0 }
    };
    // (the tour, its days, whether it runs past midnight, its departures)
    let all = [("TEST-FRE", "2026-10-09", [600, 660, 720, 780]), ("TEST-LOR", "2026-10-10", [1330, 1390, 1450, 1510]), ("TEST-SON", "2026-10-11", [600, 660, 720, 780])];
    let tours = all
        .iter()
        .map(|(name, day, times)| {
            let today = *day == date;
            let night = times.iter().any(|m| *m >= 24 * 60);
            let yesterday_night = night && dates::add(day, 1) == date;
            TourInfo { number: name.to_string(), ai_group: String::new(), first: 0.0, last: 0.0, days: String::new(), runs: today || yesterday_night, today, yesterday_night, next_run: None, trips: times.iter().enumerate().map(|(k, m)| trip(k, *m)).collect() }
        })
        .collect();
    vec![LineInfo { name: "TEST".into(), user_allowed: true, termini: Vec::new(), tours }]
}

fn company() -> Company {
    let mut c = found(&Founding { name: "Night".into(), date: "2026-10-10".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
    c.lines.push(CompanyLine { name: "TEST".into(), number: "T".into(), numbers: vec!["T".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    c
}

fn tours(c: &Company, date: &str) -> Vec<String> {
    tours_of_day(c, &lines(date), date).iter().map(|t| t.tour.clone()).collect()
}

#[test]
fn the_day_befores_night_tour_is_not_one_of_the_days_tours() {
    let c = company();
    // (the timetable says Saturday's night tour runs on Sunday morning - the launcher's "runs"
    // - but it is Saturday's, not Sunday's own)
    let sunday = lines("2026-10-11");
    let lor = sunday[0].tours.iter().find(|t| t.number == "TEST-LOR").unwrap();
    assert!(lor.runs && !lor.of_day());
    // Saturday has its night tour, its trips past midnight with it
    let sat = tours_of_day(&c, &lines("2026-10-10"), "2026-10-10");
    let night = sat.iter().find(|t| t.tour == "TEST-LOR").expect("Saturday's night tour on Saturday");
    assert!(night.trips.iter().any(|x| x.dep >= 24 * 60), "{:?}", night.trips);
    // Sunday has only its own tour, Saturday (Friday's night) not Friday's
    assert_eq!(tours(&c, "2026-10-11"), vec!["TEST-SON"]);
    assert_eq!(tours(&c, "2026-10-10"), vec!["TEST-LOR"]);
    assert_eq!(tours(&c, "2026-10-09"), vec!["TEST-FRE"]);
    // the rota's week: no Sunday duty of Saturday's night tour
    let sun_duties = pt::day_duties(&c, "2026-10-11", tours_of_day(&c, &sunday, "2026-10-11"));
    assert!(sun_duties.iter().all(|d| d.pieces.iter().all(|p| p.1 == "TEST-SON")), "{sun_duties:?}");
    assert!(sun_duties.iter().all(|d| d.start >= 9 * 60), "{sun_duties:?}");
    let sat_duties = pt::day_duties(&c, "2026-10-10", sat);
    assert!(sat_duties.iter().any(|d| d.end > 24 * 60), "Saturday's night duty ends past midnight: {sat_duties:?}");
}

#[test]
fn the_night_tour_runs_on_into_the_next_morning_with_its_bus_but_is_booked_once() {
    let mut c = company();
    // Saturday closed: its night tour booked with it, carried into Sunday for the game
    let sat = tours_of_day(&c, &lines("2026-10-10"), "2026-10-10");
    day::close_day(&mut c, sat, &[]);
    let carry = c.planning.night.clone().expect("Saturday's night tour carried");
    assert_eq!(carry.date, "2026-10-11");
    assert_eq!(carry.tours.iter().map(|t| t.tour.as_str()).collect::<Vec<_>>(), vec!["TEST-LOR"]);
    // Sunday's plan for the game runs it on (with Saturday's bus), its own tour besides; a trip
    // of it the game reports is not booked again
    let sun = tours_of_day(&c, &lines("2026-10-11"), "2026-10-11");
    assert!(sun.iter().all(|t| t.tour != "TEST-LOR"));
    let dp = plan::day_plan(&c, "2026-10-11", sun, &[], &[], false);
    let lp = plan::live_plan(&c, &dp);
    let lor = lp.tour("TEST", "TEST-LOR").expect("carried into Sunday's plan for the game");
    assert!(lor.carried);
    assert!(lp.tour("TEST", "TEST-SON").is_some_and(|t| !t.carried));
    assert!(lp.trip_event("TEST", "TEST-LOR", 30, 12.0, 5, 0.0).is_none(), "booked with Saturday");
    // a plan of another date carries nothing
    let mon = plan::day_plan(&c, "2026-10-12", Vec::new(), &[], &[], false);
    assert!(plan::live_plan(&c, &mon).tours.iter().all(|t| !t.carried));
}
