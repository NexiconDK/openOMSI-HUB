//! The operations assistants (15 November 2026): their shift plan by its rules, filled at once,
//! who is at the desk when (the night past midnight, one ill not), that he runs the day only on a
//! server while no boss is online, and a sick driver's duty given to a colleague by him - told once.

use super::super::market::{self, MarketBus, Payment};
use super::super::model::{CompanyLine, GameMode, Licence, Planner};
use super::super::network::{self, TourOfDay};
use super::super::plan::{self, Who};
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

/// Line 5 every day: two buses, 06:00-10:50 and 14:00-18:50 (duties 1001 and 1002 on a weekday).
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
    vec![LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: vec![tour("1", 360), tour("2", 840)] }]
}

/// A company of line 5 on a server, planning with RosterFlow Pro (Monday 4 March 2024): two
/// buses, five drivers, five operations assistants, its next two weeks quiet.
fn company() -> Company {
    let mut c = found(&Founding { name: "Drift".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
    c.cash += 5_000_000_00;
    for _ in 0..2 {
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
    for k in 0..5u32 {
        let mut e = c.staff[0].clone();
        (e.id, e.name, e.role) = (900 + k, format!("Assistant {k}"), Role::Operations);
        c.staff.push(e);
    }
    for n in 0.. {
        c.id = format!("{id}{n}");
        if (0..14).all(|k| plan::disruptions(&c, &dates::add(&c.date, k)).is_empty()) {
            break;
        }
    }
    assert_eq!(autopilot::set_planner(&mut c, Planner::Pro), Planner::Pro);
    c.online.on_server = true;
    c
}

fn tours(c: &Company, date: &str) -> Vec<TourOfDay> {
    network::tours_of_day(c, &lines(), date)
}

#[test]
fn the_shift_plan_keeps_its_rules_and_fills_itself() {
    let mut c = company();
    // (Monday's late, then Tuesday's early: eight hours of rest)
    set_slot(&mut c, 1, 0, 900).unwrap();
    assert_eq!(set_slot(&mut c, 0, 1, 900), Err(NO_REST));
    // (two shifts on one day)
    assert_eq!(set_slot(&mut c, 2, 0, 900), Err(NO_REST));
    // (Sunday's night, then Monday's early: none at all - the week goes round)
    set_slot(&mut c, 0, 0, 901).unwrap();
    assert_eq!(set_slot(&mut c, 2, 6, 901), Err(NO_REST));
    // (a driver is no assistant; no such shift)
    let driver = c.staff[0].id;
    assert_eq!(set_slot(&mut c, 0, 2, driver), Err(NOT_ASSISTANT));
    assert_eq!(set_slot(&mut c, 3, 0, 900), Err(BAD_SHIFT));
    // (five lates a week, not a sixth)
    for wd in 1..5 {
        set_slot(&mut c, 1, wd, 900).unwrap();
    }
    assert_eq!(set_slot(&mut c, 1, 5, 900), Err(TOO_MANY));
    set_slot(&mut c, 1, 0, 0).unwrap();
    set_slot(&mut c, 1, 5, 900).unwrap();
    // filled at once: every shift of the week by five assistants, each by the rules
    assert_eq!(auto_fill(&mut c), 21);
    for id in 900..905 {
        let mine = shifts_of(&c, id);
        assert!(mine.len() <= MOST_A_WEEK, "{id}: {mine:?}");
        for a in &mine {
            for b in mine.iter().filter(|b| *b != a) {
                assert!(a.1 != b.1 && rest_between(*a, *b) >= REST, "{id}: {a:?} {b:?}");
            }
        }
    }
    // (with one assistant only, a part of the week)
    c.staff.retain(|e| e.role != Role::Operations || e.id == 900);
    assert_eq!(auto_fill(&mut c), MOST_A_WEEK);
}

#[test]
fn who_is_at_the_desk_and_when_he_runs_the_day() {
    let mut c = company();
    // Monday's night (20:00-04:00) is 902's, Tuesday's early 903's
    set_slot(&mut c, 2, 0, 902).unwrap();
    set_slot(&mut c, 0, 1, 903).unwrap();
    assert_eq!(on_shift(&c, "2024-03-04", 22 * 60), Some((902, 2)));
    assert_eq!(on_shift(&c, "2024-03-05", 2 * 60), Some((902, 2)));
    assert_eq!(on_shift(&c, "2024-03-05", 4 * 60), Some((903, 0)));
    assert_eq!(on_shift(&c, "2024-03-05", 13 * 60), None);
    // (ill: not at his desk)
    c.staff.iter_mut().find(|e| e.id == 903).unwrap().sick_until = Some("2024-03-05".into());
    assert_eq!(on_shift(&c, "2024-03-05", 5 * 60), None);
    // he runs the day while no boss is online, on a server only
    c.clock.minute = 22 * 60;
    assert_eq!(covering(&c), Some(902));
    c.ops_desk.chiefs_online = true;
    assert_eq!(covering(&c), None);
    assert!(!super::super::live_more::dispatcher_on(&c));
    c.ops_desk.chiefs_online = false;
    assert!(super::super::live_more::dispatcher_on(&c), "the spare bus sent as by a dispatcher on duty");
    c.online.on_server = false;
    assert_eq!(covering(&c), None);
    assert_eq!(hireable(&c), Err(ON_SERVER_ONLY));
}

#[test]
fn a_sick_drivers_duty_goes_to_a_colleague_by_the_assistant() {
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().filter(|e| e.role == Role::Driver).map(|e| e.id).collect();
    // a rota: ids[0] the mornings (1001), ids[1] the afternoons (1002)
    let date = c.date.clone();
    let w: pt::Week = (0..pt::DAYS as i64).map(|k| pt::day_duties(&c, &dates::add("2024-03-03", k), tours(&c, &dates::add("2024-03-03", k)))).collect();
    let rota = pt::add(&mut c, "Nord", false);
    pt::add_row(&mut c, rota).unwrap();
    pt::add_row(&mut c, rota).unwrap();
    for dd in 1..=5 {
        pt::set_cell(&mut c, rota, 0, dd, Cell::Duty { n: 1001 }, &w).unwrap();
        pt::set_cell(&mut c, rota, 1, dd, Cell::Duty { n: 1002 }, &w).unwrap();
    }
    pt::set_driver(&mut c, rota, 0, Some(ids[0])).unwrap();
    pt::set_driver(&mut c, rota, 1, Some(ids[1])).unwrap();
    // the morning's driver ill; the early shift 900's
    c.staff.iter_mut().find(|e| e.id == ids[0]).unwrap().sick_until = Some(date.clone());
    set_slot(&mut c, 0, 0, 900).unwrap();
    let dp = plan::day_plan(&c, &date, tours(&c, &date), &[], &[], false);
    let duties = pt::day_duties(&c, &date, tours(&c, &date));
    let d1001 = duties.iter().find(|d| d.number == 1001).unwrap().clone();
    assert!(super::super::pro_ops::open_duties(&dp, &duties).iter().any(|d| d.n == 1001));
    // a boss online: the assistant leaves it to him
    let now = d1001.start - 60;
    c.clock.minute = now as u32;
    c.ops_desk.chiefs_online = true;
    assert!(look(&mut c, &dp, &duties, now).is_empty());
    // nobody online: given to a colleague, told in the feed and his log
    c.ops_desk.chiefs_online = false;
    let told = look(&mut c, &dp, &duties, now);
    assert_eq!(told.len(), 1);
    assert_eq!(told[0].text, COVERED);
    let dp2 = plan::day_plan(&c, &date, tours(&c, &date), &[], &[], false);
    let (ti, k) = *super::super::pro_ops::pieces_of(&dp2, &d1001).first().unwrap();
    assert!(matches!(dp2.tours[ti].duties[k].who, Some(Who::Staff(id)) if id != ids[0] && c.employee(id).is_some_and(|e| e.role == Role::Driver)));
    assert_eq!(c.ops_desk.log.len(), 1);
    // (once: the duty is his now, and told already)
    assert!(look(&mut c, &dp, &duties, now + 5).is_empty());
    // (too far ahead: not yet)
    let mut c2 = company();
    set_slot(&mut c2, 0, 0, 900).unwrap();
    c2.clock.minute = 4 * 60;
    let dp = plan::day_plan(&c2, &date, tours(&c2, &date), &[], &[], false);
    let duties = pt::day_duties(&c2, &date, tours(&c2, &date));
    assert!(look(&mut c2, &dp, &duties, 4 * 60).iter().all(|f| f.args.iter().all(|(k, v)| k != "n" || v != "1002")));
}
