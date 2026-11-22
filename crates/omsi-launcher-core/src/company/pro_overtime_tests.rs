//! Overtime on an open duty: a driver whose own duty ends twenty-five minutes before it begins goes
//! on with its first part - as much as his ten hours behind the wheel allow -, the duty cut there
//! for the date, the rest left open.

use super::super::market::{self, MarketBus, Payment};
use super::super::model::{CompanyLine, GameMode, Licence, Planner};
use super::super::network::{self, TourOfDay};
use super::super::plan::{self, DayPlan, Who};
use super::super::pro_ops as po;
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

/// Line 5: bus 1 06:00-10:50 (five trips, ending at B), bus 2 from B at 11:15 to 18:05 (seven).
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
    vec![LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: vec![tour("1", 360, false, 5), tour("2", 675, true, 7)] }]
}

fn company() -> Company {
    let mut c = found(&Founding { name: "Overtid".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
    c.cash += 5_000_000_00;
    for _ in 0..2 {
        market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
    }
    let id = c.id.clone();
    while c.staff.len() < 4 {
        for a in applicants(&c) {
            if c.staff.len() < 4 && a.licence == Licence::D {
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

#[test]
fn a_driver_goes_on_after_his_own_duty_as_far_as_his_ten_hours_allow() {
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    let date = "2024-03-05";
    let w: pt::Week = (0..pt::DAYS as i64).map(|k| duties(&c, &dates::add("2024-03-03", k))).collect();
    let ds = duties(&c, date);
    let (m, a) = (ds.iter().min_by_key(|d| (d.start - 360).abs()).unwrap().number, ds.iter().min_by_key(|d| (d.start - 675).abs()).unwrap().number);
    let rota = pt::add(&mut c, "Nord", false);
    pt::add_row(&mut c, rota).unwrap();
    pt::add_row(&mut c, rota).unwrap();
    for dd in 1..=5 {
        pt::set_cell(&mut c, rota, 0, dd, Cell::Duty { n: m }, &w).unwrap();
        pt::set_cell(&mut c, rota, 1, dd, Cell::Duty { n: a }, &w).unwrap();
    }
    pt::set_driver(&mut c, rota, 0, Some(ids[0])).unwrap();
    pt::set_driver(&mut c, rota, 1, Some(ids[1])).unwrap();
    // the afternoon's driver ill: his duty open
    pt::set_exception(&mut c, ids[1], date, Some((Cell::Off, "sick".into())));
    let p = day(&c, date);
    let open = po::open_duties(&p, &ds);
    let d = open.iter().find(|o| o.n == a).unwrap().clone();
    // the morning's driver ends at 10:50 at B, where the duty begins at 11:15: he goes on with as
    // much of it as his ten hours behind the wheel allow - not all of it
    let o = of_driver(&c, &p, &d, &ds, ids[0]).unwrap_or_else(|| {
        let mine: Vec<_> = p.duties_of(Who::Staff(ids[0])).into_iter().map(|(ti, k)| plan::duty_block(&p, ti, k)).collect();
        let units = pc::segs_of(&p, &d.pieces);
        panic!("no overtime: his blocks {mine:?}; the duty {d:?}; its stretches {:?}; cut points {:?}; offers {:?}", units.iter().map(|s| (s.from(&p), s.to(&p), s.block(&p).from_stop, s.block(&p).to_stop)).collect::<Vec<_>>(), pc::cut_points(&c, &p, &units), offers(&c, &p, &d, &ds))
    });
    assert!(!o.earlier && !o.whole, "{o:?}");
    assert_eq!((o.at, o.from), (650, 675));
    assert!(o.to > o.from && o.to < d.end, "{o:?}");
    assert!(290 + (o.to - o.from) <= staff::DAY_MAX, "{o:?}");
    // cut there for the date, the first part his
    let part = take(&mut c, &p, &d, &o).unwrap();
    assert_eq!(part, Some(0));
    po::give_part(&mut c, date, a, part, ids[0], None).unwrap();
    let p = day(&c, date);
    let s = pc::split_of(&c, date, a).unwrap().clone();
    assert_eq!(s.parts.len(), 2);
    let (ti, k) = pc::part_pieces(&p, &s.parts[0])[0];
    assert_eq!(p.tours[ti].duties[k].who, Some(Who::Staff(ids[0])));
    let (ti, k) = pc::part_pieces(&p, &s.parts[1])[0];
    assert_eq!(p.tours[ti].duties[k].who, None, "the rest stays open");
    // (a duty cut already: no more overtime offered on it)
    let ds = duties(&c, date);
    assert!(po::open_duties(&p, &ds).iter().filter(|x| x.n == a).all(|x| offers(&c, &p, x, &ds).is_empty()));
}
