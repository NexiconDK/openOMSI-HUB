//! RosterFlow Pro's bus dispatch (round I): the day plan runs the bus chosen for a block on its
//! date (or none), the roster's for the others; who can take a block - the workshop, another block
//! then, the battery; "Fordel automatisk" (the hand's choices kept unless asked); a swap; a bus that
//! falls out told once, and its notice scheduled with the morning.

use super::super::market::{self, MarketBus, Payment};
use super::super::model::{CompanyLine, Drive, GameMode, Planner};
use super::super::network::{self, TourOfDay};
use super::super::plan::{self, BusOf, DayPlan, Problem, Source};
use super::super::{autopilot, found, Founding};
use super::*;
use crate::{LineInfo, StopInfo, TourInfo, TripInfo};

fn tt(name: &str, a: &str, b: &str, dep: i32, arr: i32) -> TripInfo {
    TripInfo {
        name: name.into(),
        index: 0,
        line: "5".into(),
        from: a.into(),
        terminus: format!("{b} Bf"),
        departure: dep as f64 * 60.0,
        arrival: arr as f64 * 60.0,
        stops: (0..6).map(|s| StopInfo { name: match s { 0 => a.to_string(), 5 => b.to_string(), _ => format!("S{s}") }, id: s, arr: 0.0, dep: 0.0 }).collect(),
        km: (arr - dep) as f64 * 0.4,
    }
}

/// Line 5 every day: three blocks, 06:00-10:50, 07:00-11:50 and 14:00-18:50 (100 km each).
fn lines() -> Vec<LineInfo> {
    let tour = |no: &str, from: i32| TourInfo {
        number: no.into(),
        ai_group: String::new(),
        first: 0.0,
        last: 0.0,
        days: "daily".into(),
        runs: true,
        today: true,
        yesterday_night: false,
        next_run: None,
        trips: (0..5).map(|k| if k % 2 == 0 { tt(&format!("5 {no}-{k}"), "A", "B", from + k * 60, from + k * 60 + 50) } else { tt(&format!("5 {no}-{k}"), "B", "A", from + k * 60, from + k * 60 + 50) }).collect(),
    };
    vec![LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: vec![tour("1", 360), tour("2", 420), tour("3", 840)] }]
}

/// A company of line 5 planning with RosterFlow Pro (Monday 4 March 2024), three buses, no
/// disruptions the next two weeks.
fn company() -> Company {
    let mut c = found(&Founding { name: "Disponering".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
    c.cash += 5_000_000_00;
    for _ in 0..3 {
        market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
    }
    for v in c.fleet.iter_mut() {
        (v.condition, v.clean, v.breakdowns, v.workshop_until) = (90.0, 90.0, 0, None);
        v.next_service_km = v.km + 20_000.0;
    }
    let id = c.id.clone();
    for n in 0.. {
        c.id = format!("{id}{n}");
        if (0..14).all(|k| plan::disruptions(&c, &dates::add(&c.date, k)).is_empty()) {
            break;
        }
    }
    assert_eq!(autopilot::set_planner(&mut c, Planner::Pro), Planner::Pro);
    c
}

fn tours(c: &Company, date: &str) -> Vec<TourOfDay> {
    network::tours_of_day(c, &lines(), date)
}

fn day(c: &Company, date: &str) -> DayPlan {
    plan::day_plan(c, date, tours(c, date), &[], &[], false)
}

fn at(p: &DayPlan, tour: &str) -> usize {
    p.tours.iter().position(|t| t.tour.tour.trim() == tour).unwrap()
}

#[test]
fn the_day_plan_runs_the_bus_chosen_for_the_date_and_the_rosters_for_the_rest() {
    let mut c = company();
    let date = dates::add(&c.date, 1);
    let ids: Vec<u32> = c.fleet.iter().map(|v| v.id).collect();
    // nothing planned: no block has a bus
    let p = day(&c, &date);
    assert!(p.tours.iter().all(|t| t.bus.is_none() && t.bus_problem == Some(Problem::Unassigned)), "{:?}", p.tours.iter().map(|t| t.bus_problem).collect::<Vec<_>>());
    // the roster's bus on block 1 every Tuesday, the dispatch's on block 3 this Tuesday only
    plan::set_bus(&mut c, p.weekday, "Linie5", "1", Some(ids[0]));
    set(&mut c, &date, "Linie5", "3", Some(ids[1]), true);
    let p = day(&c, &date);
    assert_eq!((p.tours[at(&p, "1")].bus, p.tours[at(&p, "1")].bus_from), (Some(BusOf::Own(ids[0])), Source::Roster));
    assert_eq!((p.tours[at(&p, "3")].bus, p.tours[at(&p, "3")].bus_from), (Some(BusOf::Own(ids[1])), Source::Dispatcher));
    let next = day(&c, &dates::add(&date, 7));
    assert_eq!(next.tours[at(&next, "3")].bus, None, "the dispatch's choice is for its date");
    // the dispatch over the roster: another bus, or none at all; cleared, the roster's again
    set(&mut c, &date, "Linie5", "1", Some(ids[2]), true);
    assert_eq!(day(&c, &date).tours[0].bus, Some(BusOf::Own(ids[2])));
    set(&mut c, &date, "Linie5", "1", None, true);
    let p = day(&c, &date);
    assert_eq!((p.tours[at(&p, "1")].bus, p.tours[at(&p, "1")].bus_problem), (None, Some(Problem::Unassigned)));
    clear(&mut c, &date, "Linie5", "1");
    assert_eq!(day(&c, &date).tours[at(&p, "1")].bus, Some(BusOf::Own(ids[0])));
    // a chosen bus in the workshop that day: none, and why
    c.fleet[1].workshop_until = Some(date.clone());
    let p = day(&c, &date);
    assert_eq!((p.tours[at(&p, "3")].bus, p.tours[at(&p, "3")].bus_problem), (None, Some(Problem::Workshop)));
    // choices of long gone dates go
    set(&mut c, "2024-01-01", "Linie5", "1", Some(ids[0]), true);
    prune(&mut c);
    assert!(pick_of(&c, "2024-01-01", "Linie5", "1").is_none());
    assert!(pick_of(&c, &date, "Linie5", "3").is_some());
}

#[test]
fn who_can_take_a_block_the_workshop_another_block_then_the_battery() {
    let mut c = company();
    let date = c.date.clone();
    let ids: Vec<u32> = c.fleet.iter().map(|v| v.id).collect();
    set(&mut c, &date, "Linie5", "1", Some(ids[0]), true);
    let p = day(&c, &date);
    let (t1, t2, t3) = (at(&p, "1"), at(&p, "2"), at(&p, "3"));
    assert!((km_of(&p.tours[t1].tour) - 100.0).abs() < 0.01, "{}", km_of(&p.tours[t1].tour));
    // bus 0 runs block 1 (06:00-10:50): not block 2 (07:00), block 3 (14:00) yes
    let v0 = c.vehicle(ids[0]).unwrap().clone();
    assert_eq!(check(&c, &p, t2, &v0), Err(Cannot::Busy { with: "5/01".into() }));
    assert_eq!(check(&c, &p, t3, &v0), Ok(Vec::new()));
    // in the workshop
    c.fleet[1].workshop_until = Some(dates::add(&date, 2));
    let p = day(&c, &date);
    assert!(matches!(check(&c, &p, t2, c.vehicle(ids[1]).unwrap()), Err(Cannot::Workshop { .. })));
    // warnings: a service soon, dirty, three breakdowns
    c.fleet[2].next_service_km = c.fleet[2].km + 300.0;
    (c.fleet[2].clean, c.fleet[2].breakdowns) = (20.0, 3);
    let w = check(&c, &p, t2, c.vehicle(ids[2]).unwrap()).unwrap();
    assert!(matches!(w[0], Warn::ServiceSoon { km_left } if (km_left - 200.0).abs() < 0.5), "{w:?}");
    assert_eq!(&w[1..], &[Warn::Dirty, Warn::Breakdowns { n: 3 }]);
    // an electric bus, its battery at a fifth since 05:00: not enough for 100 km
    c.fleet[2].kind.drive = Drive::Electric;
    (c.fleet[2].fuel, c.fleet[2].fuel_at) = (0.2, Some(300));
    let v = c.vehicle(ids[2]).unwrap().clone();
    assert!(matches!(check(&c, &p, t2, &v), Err(Cannot::Range { .. })), "{:?} km {}", check(&c, &p, t2, &v), range_km(&v, 0.2));
    // full, two blocks of 100 km a day; charged in the gap between (half an hour or more)
    (c.fleet[2].fuel, c.fleet[2].fuel_at) = (1.0, None);
    let v = c.vehicle(ids[2]).unwrap().clone();
    let runs = [(360, 650, 100.0)];
    let without = 1.0 - 100.0 * economy::consumption(v.kind) / 100.0 / fuel::tank(v.kind);
    assert!((level_before(&v, false, &runs, 660) - without).abs() < 1e-9, "a gap of 10 minutes charges nothing");
    assert!(level_before(&v, false, &runs, 840) > without + 0.1, "three hours at the charger");
    assert_eq!(charging(&v, &[(360, 650, 100.0), (840, 1130, 100.0)]), vec![(660, 830)]);
    assert!(charging(&v0, &[(360, 650, 100.0), (840, 1130, 100.0)]).is_empty(), "a diesel bus does not charge");
}

#[test]
fn fordel_automatisk_fills_the_blocks_without_a_bus_and_keeps_the_hands_unless_asked() {
    let mut c = company();
    let date = dates::add(&c.date, 1);
    // block 2 has no bus by hand: kept
    set(&mut c, &date, "Linie5", "2", None, true);
    let p = day(&c, &date);
    assert_eq!(auto(&mut c, &p, false), 2);
    let p = day(&c, &date);
    let (t1, t2, t3) = (at(&p, "1"), at(&p, "2"), at(&p, "3"));
    assert!(p.tours[t1].bus.is_some() && p.tours[t3].bus.is_some() && p.tours[t2].bus.is_none(), "{:?}", p.tours.iter().map(|t| t.bus).collect::<Vec<_>>());
    // the bus already on the line that day takes the afternoon too
    assert_eq!(p.tours[t1].bus, p.tours[t3].bus);
    assert!(!pick_of(&c, &date, "Linie5", "1").unwrap().hand);
    // again: nothing more; "Fordel alt igen": block 2 too, on another bus than block 1's
    let again = day(&c, &date);
    assert_eq!(auto(&mut c, &again, false), 0);
    let p = day(&c, &date);
    assert_eq!(auto(&mut c, &p, true), 3);
    let p = day(&c, &date);
    assert!(p.tours.iter().all(|t| t.bus.is_some()));
    assert_ne!(p.tours[t1].bus, p.tours[t2].bus);
    // a bus in the workshop is not given
    let mut c2 = company();
    for v in c2.fleet.iter_mut().skip(1) {
        v.workshop_until = Some(date.clone());
    }
    let p = day(&c2, &date);
    assert_eq!(auto(&mut c2, &p, false), 2, "one bus: blocks 1 and 3, not block 2 at the same time");
    let p = day(&c2, &date);
    let only = c2.fleet[0].id;
    assert!(p.tours.iter().filter_map(bus_of).all(|id| id == only), "{:?}", p.tours.iter().map(|t| t.bus).collect::<Vec<_>>());
}

#[test]
fn two_blocks_swap_their_buses() {
    let mut c = company();
    let date = dates::add(&c.date, 1);
    let ids: Vec<u32> = c.fleet.iter().map(|v| v.id).collect();
    set(&mut c, &date, "Linie5", "1", Some(ids[0]), true);
    set(&mut c, &date, "Linie5", "2", Some(ids[1]), true);
    let p = day(&c, &date);
    swap(&mut c, &p, at(&p, "1"), at(&p, "2"));
    let p = day(&c, &date);
    assert_eq!((p.tours[at(&p, "1")].bus, p.tours[at(&p, "2")].bus), (Some(BusOf::Own(ids[1])), Some(BusOf::Own(ids[0]))));
}

#[test]
fn a_bus_that_falls_out_is_told_once_with_its_first_block_and_scheduled_with_the_morning() {
    let mut c = company();
    let ids: Vec<u32> = c.fleet.iter().map(|v| v.id).collect();
    let date = c.date.clone();
    set(&mut c, &date, "Linie5", "1", Some(ids[0]), true);
    set(&mut c, &date, "Linie5", "3", Some(ids[0]), true);
    c.fleet[0].workshop_until = Some(date.clone());
    let p = day(&c, &date);
    let downs = downs_of(&c, &p);
    assert_eq!(downs.len(), 1, "{downs:?}");
    let (bus, ti, why, covered, blocks) = downs[0];
    assert_eq!((bus, label(&p.tours[ti].tour), why, covered, blocks), (ids[0], "5/01".to_string(), DownWhy::Workshop, None, 2));
    // the morning's notice in the day's script
    let script = super::super::clock::script(&c, &p);
    assert!(script.events.iter().any(|e| matches!(&e.what, super::super::clock::What::BusDown { bus, label, .. } if *bus == ids[0] && label == "5/01")), "{:?}", script.events.iter().map(|e| &e.what).collect::<Vec<_>>());
    // told once a day
    assert!(tell(&mut c, ids[0], "Linie5", "1", "5/01", DownWhy::Workshop, None, 2, 300));
    assert!(!tell(&mut c, ids[0], "Linie5", "1", "5/01", DownWhy::Workshop, None, 2, 310));
    let x = unseen(&c).unwrap().clone();
    let (text, args) = words(&c, &x);
    assert_eq!(text, DOWN_WORKSHOP);
    assert_eq!(args[1], ("block".to_string(), "5/01".to_string()));
    seen(&mut c, Some(ids[0]));
    assert!(unseen(&c).is_none());
}
