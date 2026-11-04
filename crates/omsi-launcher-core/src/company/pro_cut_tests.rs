//! RosterFlow Pro: an open duty cut for its date ("Klip vagten"): the cut that covers most, a
//! driver going on after his own duty at the same stop, one who said "I can until 08:00", a part
//! given for the date and run by the day's plan, the calendar's names of the parts, joined again,
//! and an older file.

use super::super::market::{self, MarketBus, Payment};
use super::super::model::{CompanyLine, GameMode, Licence, Planner};
use super::super::network::{self, TourOfDay};
use super::super::plan::{self, DayPlan, Who};
use super::super::pro_ops::{self as po, DayIs, NoWhy};
use super::super::pro_turnus::{self as pt, Cell};
use super::super::staff::{applicants, hire};
use super::super::{autopilot, dates, found, Founding};
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

/// Line 5: bus 1 06:00-10:50 (A, B, A, B, A: five trips), bus 2 14:00-18:50, bus 3 an early
/// one 04:40-07:30 that ends at A.
fn lines() -> Vec<LineInfo> {
    let tour = |no: &str, from: i32, start_b: bool, n: i32| TourInfo {
        number: no.into(),
        ai_group: String::new(),
        first: 0.0,
        last: 0.0,
        days: "daily".into(),
        runs: true,
        today: true,
        yesterday_night: false,
        next_run: None,
        trips: (0..n).map(|k| if (k % 2 == 0) != start_b { tt(&format!("5 {no}-{k}"), "A", "B", from + k * 60, from + k * 60 + 50) } else { tt(&format!("5 {no}-{k}"), "B", "A", from + k * 60, from + k * 60 + 50) }).collect(),
    };
    vec![LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: vec![tour("1", 360, false, 5), tour("2", 840, false, 5), tour("3", 280, true, 3)] }]
}

fn company() -> Company {
    let mut c = found(&Founding { name: "Klip".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
    c.cash += 5_000_000_00;
    for _ in 0..3 {
        market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
    }
    let id = c.id.clone();
    while c.staff.len() < 5 {
        for a in applicants(&c) {
            if c.staff.len() < 5 && a.licence == Licence::D {
                hire(&mut c, &a).unwrap();
            }
        }
        c.id.push('x');
        c.taken = Default::default();
    }
    for e in c.staff.iter_mut() {
        (e.sick_until, e.holiday_until) = (None, None);
        (e.satisfaction, e.reliability, e.experience, e.stress) = (80.0, 1.0, 50.0, 10.0);
        e.hired = "2024-01-01".into();
    }
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

fn duties(c: &Company, date: &str) -> Vec<ProDuty> {
    pt::day_duties(c, date, tours(c, date))
}

/// The number of the duty that begins about `at` (minutes) on `date`.
fn number_from(c: &Company, date: &str, at: i32) -> u32 {
    duties(c, date).iter().min_by_key(|d| (d.start - at).abs()).unwrap().number
}

/// A rota: the morning bus's duty `a`, the afternoon's `b`, the early bus's `e`, Monday to Friday.
fn rota(c: &mut Company, a: u32, b: u32, e: u32) {
    let w: pt::Week = (0..pt::DAYS as i64).map(|k| duties(c, &dates::add("2024-03-03", k))).collect();
    let date = "2024-03-05";
    let (m, af, ea) = (number_from(c, date, 360), number_from(c, date, 840), number_from(c, date, 280));
    let id = pt::add(c, "Nord", false);
    for _ in 0..3 {
        pt::add_row(c, id).unwrap();
    }
    for dd in 1..=5 {
        pt::set_cell(c, id, 0, dd, Cell::Duty { n: m }, &w).unwrap();
        pt::set_cell(c, id, 1, dd, Cell::Duty { n: af }, &w).unwrap();
        pt::set_cell(c, id, 2, dd, Cell::Duty { n: ea }, &w).unwrap();
    }
    pt::set_driver(c, id, 0, Some(a)).unwrap();
    pt::set_driver(c, id, 1, Some(b)).unwrap();
    pt::set_driver(c, id, 2, Some(e)).unwrap();
}

#[test]
fn an_open_duty_is_cut_so_that_most_of_it_is_driven() {
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1], ids[2]);
    let date = "2024-03-05";
    let morning = number_from(&c, date, 360);
    // the morning's driver ill: his duty open
    pt::set_exception(&mut c, ids[0], date, Some((Cell::Off, "sick".into())));
    let p = day(&c, date);
    let ds = duties(&c, date);
    let open = po::open_duties(&p, &ds);
    let d = open.iter().find(|o| o.n == morning).unwrap().clone();
    // nobody takes all of it: the afternoon's driver and ids[3] said no, ids[4] can until 08:00
    let said = vec![
        (ids[1], Reply::No { why: NoWhy::Plans, say: 0 }),
        (ids[3], Reply::No { why: NoWhy::Tired, say: 0 }),
        (ids[4], Reply::Late { until: 8 * 60, say: 0 }),
    ];
    let prop = propose(&c, &p, &d, &ds, &|_| 0.0, &said);
    assert_eq!(prop.parts.len(), 2, "{prop:#?}");
    assert_eq!(prop.covered(), prop.parts.iter().map(|x| x.to - x.from).sum::<i32>(), "all of it covered");
    // A 06:00-07:50: ids[4], who said he can until 08:00; B from 08:00 at A: the early bus's
    // driver goes on after his own duty, which ends 07:30 at A (half an hour to change)
    let (a, b) = (&prop.parts[0], &prop.parts[1]);
    assert_eq!((a.from, a.to, b.from), (360, 470, 480));
    assert_eq!(a.sugg.as_ref().map(|s| (s.driver, s.how.clone())), Some((ids[4], How::Said { late: true, time: 480 })));
    let sb = b.sugg.as_ref().unwrap();
    assert_eq!(sb.driver, ids[2]);
    assert!(matches!(&sb.how, How::Continues { at: 450, stop, .. } if stop.starts_with('A')), "{:?}", sb.how);
    assert_eq!(sb.price, Price::Within, "within his week: nothing more");
    // his week full: the part is overtime
    let full = suggest(&c, &p, &b.segs, &ds, &|_| 39.0 * 60.0, &said, &[ids[4]]).unwrap();
    assert_eq!(full.driver, ids[2]);
    assert!(matches!(full.price, Price::Overtime { minutes, .. } if minutes == b.to - 450), "{:?}", full.price);
    assert!(full.cost > 0);
    // cut for the date: the morning bus cut before its 08:00 trip on that date only
    cut(&mut c, &p, &d, &prop).unwrap();
    let s = split_of(&c, date, morning).unwrap().clone();
    assert_eq!(s.parts.len(), 2);
    assert_eq!(s.inner.len(), 1, "cut inside the piece");
    let p = day(&c, date);
    let t1 = p.tours.iter().find(|t| t.tour.tour == "1").unwrap();
    assert_eq!(t1.duties.len(), 2, "the bus cut in two pieces that day");
    assert_eq!(day(&c, "2024-03-06").tours.iter().find(|t| t.tour.tour == "1").unwrap().duties.len(), 1, "not the next day");
    assert_eq!(s.parts.iter().map(|x| part_pieces(&p, x).len()).collect::<Vec<_>>(), [1, 1]);
    // the day's duties keep their numbers (the rotas name them so); the part cut off has its own
    let before: Vec<u32> = { let mut v: Vec<u32> = ds.iter().map(|d| d.number).collect(); v.sort(); v };
    let after = duties(&c, date);
    assert!(before.iter().all(|n| after.iter().any(|d| d.number == *n)), "{before:?} {:?}", after.iter().map(|d| d.number).collect::<Vec<_>>());
    assert!(after.iter().any(|d| d.number == morning * 10 + 1), "part B is {}1", morning);
    let af = before.iter().copied().find(|n| ds.iter().any(|d| d.number == *n && d.start > 13 * 60)).unwrap();
    match po::driver_day(&c, &c.employee(ids[1]).unwrap().clone(), &p, &after).is {
        DayIs::Duties { numbers, .. } => assert_eq!(numbers, [af], "the afternoon's driver still drives his own"),
        x => panic!("{x:?}"),
    }
    // the parts given for the date: the day's plan runs them
    po::give_part(&mut c, date, morning, Some(0), ids[4], None).unwrap();
    po::give_part(&mut c, date, morning, Some(1), ids[2], None).unwrap();
    let p = day(&c, date);
    let (pa, pb) = (part_pieces(&p, &s.parts[0])[0], part_pieces(&p, &s.parts[1])[0]);
    assert_eq!(p.tours[pa.0].duties[pa.1].who, Some(Who::Staff(ids[4])));
    assert_eq!(p.tours[pb.0].duties[pb.1].who, Some(Who::Staff(ids[2])));
    // the calendar names them by the duty and the part: "1001 + 1002B"
    let ds = duties(&c, date);
    let e2 = c.employee(ids[2]).unwrap().clone();
    match po::driver_day(&c, &e2, &p, &ds).is {
        DayIs::Duties { labels, .. } => assert_eq!(labels.last(), Some(&format!("{morning}B")), "{labels:?}"),
        x => panic!("{x:?}"),
    }
    // joined again: the bus as it was, the parts given gone
    assert!(uncut(&mut c, date, morning));
    assert!(!uncut(&mut c, date, morning));
    assert!(c.planning.ops.calls.iter().all(|x| x.part.is_none()));
    let p = day(&c, date);
    assert_eq!(p.tours.iter().find(|t| t.tour.tour == "1").unwrap().duties.len(), 1);
}

#[test]
fn a_part_goes_to_one_who_begins_earlier_and_the_rules_hold() {
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1], ids[2]);
    let date = "2024-03-05";
    let p = day(&c, date);
    let ds = duties(&c, date);
    // a stretch of the morning bus 06:00-07:50: the early bus's driver drives then - he cannot
    let t1 = p.tours.iter().position(|t| t.tour.tour == "1").unwrap();
    let segs = [Seg { ti: t1, a: 0, b: 2 }];
    let s = suggest(&c, &p, &segs, &ds, &|_| 0.0, &[], &[]).unwrap();
    assert_ne!(s.driver, ids[2]);
    // 12:00-12:50 at A (the afternoon bus begins 14:00 at A): too far from it to begin earlier
    // - a free colleague then; with ids[3] and ids[4] skipped, the afternoon's driver who begins
    // at 14:00 is no "begins earlier" either (an hour and ten)
    let s = suggest(&c, &p, &segs, &ds, &|_| 0.0, &[], &[ids[3], ids[4]]);
    assert!(s.is_none_or(|s| !matches!(s.how, How::Earlier { .. })));
    // nearby stops: named alike or the same first word
    assert!(nearby("Bytorvet", "bytorvet") && nearby("Bytorvet Nord", "Bytorvet") && !nearby("Station", "Bytorvet"));
    assert_eq!(letter(0), 'A');
    assert_eq!(letter(2), 'C');
}

#[test]
fn an_older_file_has_no_cuts_and_they_are_kept() {
    let old: po::Ops = serde_json::from_str("{}").unwrap();
    assert!(old.splits.is_empty() && old.is_empty());
    let s = Split { date: "2024-03-05".into(), n: 1001, parts: vec![Part { pieces: vec![PieceKey { line: "Linie5".into(), tour: "1".into(), dep: 360, name: "5 1-0".into() }], from: 360, to: 470 }], inner: Vec::new() };
    let ops = po::Ops { splits: vec![s], ..Default::default() };
    let back: po::Ops = serde_json::from_str(&serde_json::to_string(&ops).unwrap()).unwrap();
    assert_eq!(back, ops);
    // a call of an older file: no part, no pieces
    let call: po::Call = serde_json::from_str(r#"{"date":"2024-03-05","n":1001,"driver":3}"#).unwrap();
    assert_eq!((call.part, call.pieces.len()), (None, 0));
    let ask: po::Ask = serde_json::from_str(r#"{"id":1,"date":"2024-03-05","n":1001,"start":350,"end":660,"who":{"kind":"staff","id":3},"at":5}"#).unwrap();
    assert_eq!((ask.part, ask.due), (None, None));
}
