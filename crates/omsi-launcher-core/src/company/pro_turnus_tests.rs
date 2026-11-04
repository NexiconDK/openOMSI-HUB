//! RosterFlow Pro's rota (step 3): the calendar of a rota (the week from Sunday, a row down each
//! week and round again), the checks over the whole cycle (the step from the last row back to the
//! first included), the rota made automatically, the editing rules, the exceptions, the day run
//! by the rota with the roster's fall-back, a frame of a rota, and an older file.

use super::super::market::{self, MarketBus, Payment};
use super::super::model::{CompanyLine, GameMode, Licence};
use super::super::plan::{self, DayPlan, Who};
use super::super::staff::{applicants, hire};
use super::super::{autopilot, found, network, Founding};
use super::*;
use crate::company::model::Planner;
use crate::{LineInfo, StopInfo, TourInfo, TripInfo};

// --- a week of duties made up ---------------------------------------------------------------------

fn d(number: u32, start: i32, end: i32) -> ProDuty {
    ProDuty { number, start, end, work: end - start, paid: end - start, kind: pd::Kind::Whole, pieces: Vec::new() }
}

/// Monday to Friday two early duties (05:00-13:00, 06:00-14:00) and two late (13:00-21:00,
/// 14:00-22:00), Saturday and Sunday one early and one late each.
fn week() -> Week {
    let weekday = || vec![d(1001, 300, 780), d(1002, 360, 840), d(1003, 780, 1260), d(1004, 840, 1320)];
    let mut w: Week = vec![vec![d(3001, 360, 840), d(3002, 840, 1320)]];
    for _ in 0..5 {
        w.push(weekday());
    }
    w.push(vec![d(2001, 360, 840), d(2002, 840, 1320)]);
    w
}

fn bare() -> Company {
    let mut c = found(&Founding { name: "Turnus".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
    assert_eq!(autopilot::set_planner(&mut c, Planner::Pro), Planner::Pro);
    c
}

fn row(cells: [Cell; DAYS]) -> Row {
    Row { days: cells, driver: None }
}

const F: Cell = Cell::Off;
const fn n(x: u32) -> Cell {
    Cell::Duty { n: x }
}

#[test]
fn a_rotas_week_begins_on_sunday_and_its_driver_moves_a_row_down_every_week() {
    // 2024-03-04 is a Monday: its week began on Sunday 3 March
    assert_eq!(day_of("2024-03-03"), 0);
    assert_eq!(day_of("2024-03-04"), 1);
    assert_eq!(day_of("2024-03-09"), 6);
    assert_eq!(sunday_of("2024-03-09"), "2024-03-03");
    assert_eq!(sunday_of("2024-03-03"), "2024-03-03");
    assert_eq!(week_number("2026-11-04"), 45);
    assert_eq!(week_number("2026-11-08"), 45, "a Sunday is in the week of the Monday before it (ISO)");
    let t = Turnus { id: 1, name: "Nord".into(), from: "2024-03-03".into(), rows: vec![Row::default(); 3], ..Default::default() };
    assert_eq!(week_of(&t, "2024-03-02"), None, "before it begins");
    assert_eq!(week_of(&t, "2024-03-03"), Some(0));
    assert_eq!(week_of(&t, "2024-03-09"), Some(0));
    assert_eq!(week_of(&t, "2024-03-10"), Some(1));
    // row 1's driver: row 1, 2, 3, then row 1 again
    let rows: Vec<Option<usize>> = ["2024-03-04", "2024-03-11", "2024-03-18", "2024-03-25"].iter().map(|x| row_on(&t, 0, x)).collect();
    assert_eq!(rows, [Some(0), Some(1), Some(2), Some(0)]);
    assert_eq!(row_on(&t, 2, "2024-03-11"), Some(0), "the last row's driver goes to the first");
    // and who is on a row: the driver who began a row up per week
    let mut t = t;
    t.rows[0].driver = Some(7);
    t.rows[2].driver = Some(9);
    assert_eq!(driver_on(&t, 0, "2024-03-04"), Some(7));
    assert_eq!(driver_on(&t, 1, "2024-03-11"), Some(7));
    assert_eq!(driver_on(&t, 0, "2024-03-11"), Some(9));
    assert_eq!(driver_on(&t, 1, "2024-03-04"), None);
}

#[test]
fn the_checks_go_round_the_whole_cycle() {
    let mut c = bare();
    let w = week();
    let id = add(&mut c, "Nord fuldtid", false);
    // row 1 the afternoons, row 2 the mornings: row 1's Saturday (2002 to 22:00) is followed by
    // row 2's Sunday (3001 from 06:00) - 8 hours of rest, across the step from row 1 to row 2;
    // row 2's Saturday off and row 1's Sunday off (round again) are two days off together
    c.planning.rotas.list[0].rows = vec![row([F, n(1003), n(1003), F, n(1004), n(1004), n(2002)]), row([n(3001), F, n(1001), n(1001), F, n(1002), F])];
    set_hours(&mut c, id, Some(30.0)).unwrap();
    let k = 0;
    let ch = check(&c, k, &w);
    let rest: Vec<(Option<usize>, Option<usize>)> = ch.marks.iter().filter(|m| m.rule == Rule::Rest).map(|m| (m.row, m.day)).collect();
    assert_eq!(rest, [(Some(1), Some(0))], "{:?}", ch.marks);
    assert!(ch.kept(Rule::InARow), "{:?}", ch.marks);
    assert!(ch.kept(Rule::Duty), "{:?}", ch.marks);
    // the hours: row 1 5 duties of 8 hours (40), row 2 4 (32), the rota aims at 30: row 1 over
    // by more than 20 %, row 2 within; and the cycle's average (36) over by more than 5 %
    assert_eq!(ch.minutes, [40 * 60, 32 * 60]);
    assert!((ch.average - 36.0).abs() < 0.01);
    assert!(ch.of(0, None).any(|m| m.text == M_OVER), "{:?}", ch.marks);
    assert!(!ch.of(1, None).any(|m| m.rule == Rule::Hours));
    assert!(ch.marks.iter().any(|m| m.row.is_none() && m.text == M_AVERAGE));
    // two days off together: row 2 has its Saturday and row 1's Sunday; row 1 none
    assert!(ch.of(0, None).any(|m| m.rule == Rule::TwoOff));
    assert!(!ch.of(1, None).any(|m| m.rule == Rule::TwoOff));
    assert!(ch.kept(Rule::Weekend));
    // seven days of work in a row: row 1's Thursday to Saturday and row 2's Sunday to Wednesday
    c.planning.rotas.list[0].rows[1].days[1] = n(1002);
    let ch = check(&c, k, &w);
    let seven: Vec<(Option<usize>, Option<usize>)> = ch.marks.iter().filter(|m| m.rule == Rule::InARow).map(|m| (m.row, m.day)).collect();
    assert_eq!(seven, [(Some(1), Some(3))], "{:?}", ch.marks);
    // a duty that does not run that day, and one in two rows the same day
    c.planning.rotas.list[0].rows[1].days[4] = n(1009);
    c.planning.rotas.list[0].rows[1].days[1] = n(1003);
    let ch = check(&c, k, &w);
    assert!(ch.of(1, Some(4)).any(|m| m.text == M_MISSING));
    assert!(ch.of(0, Some(1)).any(|m| m.text == M_TWICE) && ch.of(1, Some(1)).any(|m| m.text == M_TWICE));
    // no weekend day off in the whole cycle
    c.planning.rotas.list[0].rows[0].days[0] = n(3002);
    c.planning.rotas.list[0].rows[1].days[6] = n(2001);
    assert!(!check(&c, k, &w).kept(Rule::Weekend));
    let _ = id;
}

#[test]
fn a_rota_made_automatically_places_every_duty_once_and_keeps_the_rules() {
    let mut c = bare();
    let w = week();
    let id = add(&mut c, "Nord fuldtid", false);
    let n_rows = auto(&mut c, id, &w).unwrap();
    // 24 duties of 8 hours, 39 hours a row: 5 rows would do (24 duties over 5 days, 192 hours /
    // 39 = 4.9), but two days off together in 5 rows leave a weekday a row short: 6 rows
    assert_eq!(n_rows, 6);
    let t = &c.planning.rotas.list[0];
    for (dd, ds) in w.iter().enumerate() {
        for x in ds {
            assert_eq!(placed_count(&c, dd, x.number), 1, "duty {} on day {dd}", x.number);
        }
    }
    assert!(unplaced(&c, &w).iter().all(Vec::is_empty));
    let ch = check(&c, 0, &w);
    assert!(ch.kept(Rule::Rest), "{:?}\n{:?}", ch.marks, t.rows);
    assert!(ch.kept(Rule::InARow), "{:?}", ch.marks);
    assert!(ch.kept(Rule::Duty), "{:?}", ch.marks);
    assert!(ch.kept(Rule::TwoOff), "{:?}\n{:?}", ch.marks, t.rows);
    assert!(ch.kept(Rule::Weekend));
    for r in &t.rows {
        assert!(r.days.iter().filter(|x| x.works()).count() <= 5, "{:?}", r.days);
    }
    // a second rota takes what the first left: nothing
    let other = add(&mut c, "Nord deltid", false);
    assert_eq!(auto(&mut c, other, &w), Err(NOTHING_TO_PLACE));
    // made again, the drivers stay on their rows
    let mut c2 = c.clone();
    c2.planning.rotas.list[0].rows[0].driver = Some(42);
    auto(&mut c2, id, &w).unwrap();
    assert_eq!(c2.planning.rotas.list[0].rows[0].driver, Some(42));
}

#[test]
fn a_reserve_rota_is_made_of_frames() {
    let mut c = bare();
    let id = add(&mut c, "Reserve", true);
    add_row(&mut c, id).unwrap();
    add_row(&mut c, id).unwrap();
    assert_eq!(auto(&mut c, id, &week()), Ok(2));
    let t = &c.planning.rotas.list[0];
    assert!(t.rows.iter().all(|r| r.days.iter().filter(|x| x.works()).count() == 5));
    assert_eq!(t.rows[0].days.iter().find(|x| x.works()), Some(&Cell::Frame { from: 300, to: 780 }));
    assert_eq!(t.rows[1].days.iter().find(|x| x.works()), Some(&Cell::Frame { from: 780, to: 1260 }));
}

#[test]
fn editing_a_rota_keeps_a_duty_to_its_day_and_to_one_row() {
    let mut c = bare();
    let w = week();
    let id = add(&mut c, "", false);
    assert_eq!(c.planning.rotas.list[0].name, "Rota 1");
    assert_eq!(c.planning.rotas.list[0].from, "2024-03-03");
    add_row(&mut c, id).unwrap();
    add_row(&mut c, id).unwrap();
    assert_eq!(set_cell(&mut c, id, 0, 1, n(1001), &w), Ok(()));
    assert_eq!(set_cell(&mut c, id, 0, 1, n(1001), &w), Ok(()), "set again where it is");
    assert_eq!(set_cell(&mut c, id, 1, 1, n(1001), &w), Err(TAKEN));
    assert_eq!(set_cell(&mut c, id, 1, 0, n(1001), &w), Err(NOT_THAT_DAY), "a weekday's duty on a Sunday");
    assert_eq!(set_cell(&mut c, id, 1, 0, n(3001), &w), Ok(()));
    assert_eq!(set_cell(&mut c, id, 5, 0, n(3002), &w), Err(NO_ROW));
    assert_eq!(set_cell(&mut c, id, 1, 6, Cell::Frame { from: 300, to: 200 }, &w), Ok(()));
    assert_eq!(c.planning.rotas.list[0].rows[1].days[6], Cell::Frame { from: 300, to: 1640 }, "a frame made sound: past midnight");
    set_hours(&mut c, id, Some(25.2)).unwrap();
    assert_eq!(c.planning.rotas.list[0].hours, Some(25.0));
    set_from(&mut c, id, "2024-03-14").unwrap();
    assert_eq!(c.planning.rotas.list[0].from, "2024-03-10");
    remove_row(&mut c, id, 0).unwrap();
    assert_eq!(c.planning.rotas.list[0].rows.len(), 1);
    assert!(remove(&mut c, id));
    assert!(c.planning.rotas.is_empty());
}

// --- a company that runs it -----------------------------------------------------------------------

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

/// Line 5 every day: two buses, 06:00-10:50 and 14:00-18:50 (a duty each).
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

/// A company of line 5 planning with RosterFlow Pro, two buses and four drivers, its mornings
/// quiet the next two weeks.
fn company() -> Company {
    let mut c = found(&Founding { name: "Turnus".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
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
        (e.satisfaction, e.reliability, e.experience) = (80.0, 1.0, 50.0);
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

fn day(c: &Company, date: &str) -> DayPlan {
    let tours = network::tours_of_day(c, &lines(), date);
    plan::day_plan(c, date, tours, &[], &[], false)
}

fn real_week(c: &Company, from: &str) -> Week {
    (0..DAYS as i64).map(|k| {
        let date = dates::add(from, k);
        day_duties(c, &date, network::tours_of_day(c, &lines(), &date))
    }).collect()
}

/// Who drives the duty numbered `n` on the day.
fn driver_of(c: &Company, p: &DayPlan, n: u32) -> Option<Who> {
    let tours: Vec<TourOfDay> = p.tours.iter().map(|t| t.tour.clone()).collect();
    let ds = day_duties(c, &p.date, tours);
    let (line, tour, k) = ds.iter().find(|x| x.number == n)?.pieces.first()?.clone();
    p.tours.iter().find(|t| t.tour.line == line && t.tour.tour == tour)?.duties.get(k)?.who
}

fn problem_of(c: &Company, p: &DayPlan, n: u32) -> Option<Problem> {
    let tours: Vec<TourOfDay> = p.tours.iter().map(|t| t.tour.clone()).collect();
    let ds = day_duties(c, &p.date, tours);
    let (line, tour, k) = ds.iter().find(|x| x.number == n)?.pieces.first()?.clone();
    p.tours.iter().find(|t| t.tour.line == line && t.tour.tour == tour)?.duties.get(k)?.problem
}

#[test]
fn the_day_runs_the_rota_and_the_roster_where_no_row_decides() {
    let mut c = company();
    let w = real_week(&c, "2024-03-03");
    // the duties as the Vagter tab numbers them: 1001 the morning, 1002 the afternoon, the
    // weekend's 2001 / 3001 on
    assert_eq!(w[1].iter().map(|x| x.number).collect::<Vec<_>>(), [1001, 1002]);
    assert_eq!(w[0].iter().map(|x| x.number).collect::<Vec<_>>(), [3001, 3002]);
    assert_eq!(w[6].iter().map(|x| x.number).collect::<Vec<_>>(), [2001, 2002]);
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    let (a, b, x, y) = (ids[0], ids[1], ids[2], ids[3]);
    // nothing decided without a driver on a row
    let id = add(&mut c, "Nord", false);
    add_row(&mut c, id).unwrap();
    add_row(&mut c, id).unwrap();
    assert!(on_day(&c, "2024-03-04", &network::tours_of_day(&c, &lines(), "2024-03-04")).is_none());
    // row 1 the mornings Monday to Friday, row 2 the afternoons
    for dd in 1..=5 {
        set_cell(&mut c, id, 0, dd, n(1001), &w).unwrap();
        set_cell(&mut c, id, 1, dd, n(1002), &w).unwrap();
    }
    set_driver(&mut c, id, 0, Some(a)).unwrap();
    set_driver(&mut c, id, 1, Some(b)).unwrap();
    // the weekly roster has another driver on the morning, and the rota's driver of the
    // afternoon on the other bus's duty on Saturday - neither runs where a row decides
    let wd = plan::weekday_of("2024-03-04");
    let p0 = day(&c, "2024-03-04");
    let morning = p0.tours.iter().find(|t| t.tour.tour == "1").unwrap();
    plan::set_driver(&mut c, wd, &morning.tour.line, "1", 0, Some(Who::Staff(x)));
    let p = day(&c, "2024-03-04");
    assert_eq!(driver_of(&c, &p, 1001), Some(Who::Staff(a)), "the rota's, not the roster's");
    assert_eq!(driver_of(&c, &p, 1002), Some(Who::Staff(b)));
    // a week later they have changed rows
    let next = "2024-03-11";
    plan::set_driver(&mut c, plan::weekday_of(next), &morning.tour.line, "1", 0, Some(Who::Staff(x)));
    let p = day(&c, next);
    assert_eq!(driver_of(&c, &p, 1001), Some(Who::Staff(b)));
    assert_eq!(driver_of(&c, &p, 1002), Some(Who::Staff(a)));
    // on a Saturday the rota has days off: the roster's driver of a duty runs where his row does
    // not decide (y has no row); a's day off is his - the roster's duty for him does not run
    let sat = "2024-03-09";
    let ps = day(&c, sat);
    let t1 = ps.tours.iter().find(|t| t.tour.tour == "1").unwrap().tour.line.clone();
    plan::set_driver(&mut c, plan::weekday_of(sat), &t1, "1", 0, Some(Who::Staff(y)));
    plan::set_driver(&mut c, plan::weekday_of(sat), &t1, "2", 0, Some(Who::Staff(a)));
    let ps = day(&c, sat);
    assert_eq!(driver_of(&c, &ps, 2001), Some(Who::Staff(y)), "no row decides y's day: the roster's");
    assert_eq!(driver_of(&c, &ps, 2002), None, "a's Saturday is off in his row");
    // an exception: a ill on 5 March - his row's duty open as the illness; b swaps to the
    // morning on 6 March (and his afternoon is nobody's then)
    set_exception(&mut c, a, "2024-03-05", Some((Cell::Off, "sick".into())));
    let p = day(&c, "2024-03-05");
    assert_eq!(driver_of(&c, &p, 1001), None);
    assert_eq!(problem_of(&c, &p, 1001), Some(Problem::Sick));
    assert_eq!(driver_of(&c, &p, 1002), Some(Who::Staff(b)));
    set_exception(&mut c, b, "2024-03-06", Some((n(1001), "swap".into())));
    set_exception(&mut c, a, "2024-03-06", Some((n(1002), "swap".into())));
    let p = day(&c, "2024-03-06");
    assert_eq!(driver_of(&c, &p, 1001), Some(Who::Staff(b)));
    assert_eq!(driver_of(&c, &p, 1002), Some(Who::Staff(a)));
    // a swap made on one side only: the duty is the swapper's, its row's driver not on it
    set_exception(&mut c, a, "2024-03-06", None);
    let p = day(&c, "2024-03-06");
    assert_eq!(driver_of(&c, &p, 1001), Some(Who::Staff(b)));
    assert_eq!(driver_of(&c, &p, 1002), None);
    // back to the row
    set_exception(&mut c, a, "2024-03-05", None);
    assert_eq!(driver_of(&c, &day(&c, "2024-03-05"), 1001), Some(Who::Staff(a)));
    // without RosterFlow Pro the rota decides nothing: the roster's driver again
    autopilot::set_planner(&mut c, Planner::RosterFlow);
    assert!(decided(&c, a, "2024-03-04").is_none());
    assert_eq!(p0.date, "2024-03-04");
}

#[test]
fn drivers_go_on_rows_by_their_employment() {
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    c.staff[2].employment = Employment::Hourly;
    c.staff[3].employment = Employment::Reserve(Frame::new(300, 780, 0x1f));
    let id = add(&mut c, "Nord", false);
    let res = add(&mut c, "Reserve", true);
    add_row(&mut c, id).unwrap();
    add_row(&mut c, res).unwrap();
    assert_eq!(set_driver(&mut c, id, 0, Some(ids[2])), Err(NO_HOURLY));
    assert_eq!(set_driver(&mut c, id, 0, Some(ids[3])), Err(NO_RESERVES));
    assert_eq!(set_driver(&mut c, res, 0, Some(ids[0])), Err(RESERVES_ONLY));
    assert_eq!(set_driver(&mut c, res, 0, Some(ids[3])), Ok(()));
    assert_eq!(set_driver(&mut c, id, 3, Some(ids[0])), Err(NO_ROW));
    assert_eq!(set_driver(&mut c, id, 0, Some(9999)), Err(NOT_A_DRIVER));
    // a driver is on one row only
    add_row(&mut c, id).unwrap();
    set_driver(&mut c, id, 0, Some(ids[0])).unwrap();
    set_driver(&mut c, id, 1, Some(ids[0])).unwrap();
    assert_eq!(c.planning.rotas.list[0].rows.iter().map(|r| r.driver).collect::<Vec<_>>(), [None, Some(ids[0])]);
    assert_eq!(place_of(&c, ids[0]), Some((0, 1)));
}

#[test]
fn a_frame_of_a_rota_makes_its_driver_a_reserve_that_day() {
    let mut c = company();
    let w = real_week(&c, "2024-03-03");
    let a = c.staff[0].id;
    let id = add(&mut c, "Nord", false);
    add_row(&mut c, id).unwrap();
    set_cell(&mut c, id, 0, 1, Cell::Frame { from: 5 * 60, to: 13 * 60 }, &w).unwrap();
    set_driver(&mut c, id, 0, Some(a)).unwrap();
    let e = c.staff[0].clone();
    assert_eq!(frame_on(&c, &e, "2024-03-04"), Some(Frame::new(300, 780, 0x7f)));
    assert_eq!(frame_on(&c, &e, "2024-03-05"), None, "a day off of his row");
    let b = staff::Block { key: "x".into(), from: 6 * 60, to: 9 * 60, ..Default::default() };
    assert_eq!(plan::fill_tier(&c, &e, "2024-03-04", &b), plan::TIER_RESERVE);
    assert_eq!(plan::fill_tier(&c, &e, "2024-03-05", &b), plan::TIER_COLLEAGUE);
    assert_eq!(plan::frames_on(&c, "2024-03-04").iter().map(|x| x.0.id).collect::<Vec<_>>(), [a]);
    // a reserve whose rota says a day off has no frame that day, his own weekdays or not
    c.staff[1].employment = Employment::Reserve(Frame::new(300, 780, 0x7f));
    let r = c.staff[1].id;
    let res = add(&mut c, "Reserve", true);
    add_row(&mut c, res).unwrap();
    set_driver(&mut c, res, 0, Some(r)).unwrap();
    let e = c.staff[1].clone();
    assert_eq!(frame_on(&c, &e, "2024-03-04"), None);
    assert_eq!(decides_frame(&c, r, "2024-03-04"), Some(None));
    // the day's pay: a's frame paid whether used or not
    let paid = super::super::pay::day(&mut c, "2024-03-04", &[]);
    assert!((paid.hours - 8.0).abs() < 0.01, "{paid:?}");
}

#[test]
fn an_older_file_has_no_rotas_and_the_rotas_are_kept() {
    let c = bare();
    let v = serde_json::to_value(&c.planning).unwrap();
    assert!(v.get("rotas").is_none(), "none kept when there are none");
    let mut c = c;
    let id = add(&mut c, "Nord", false);
    add_row(&mut c, id).unwrap();
    set_exception(&mut c, 3, "2024-03-05", Some((Cell::Frame { from: 300, to: 780 }, "nonsense".into())));
    assert_eq!(c.planning.rotas.exceptions[0].note, "", "a reason not known: none");
    let s = serde_json::to_string(&c.planning).unwrap();
    let back: plan::Planning = serde_json::from_str(&s).unwrap();
    assert_eq!(back.rotas, c.planning.rotas);
    let old: plan::Planning = serde_json::from_str("{}").unwrap();
    assert!(old.rotas.is_empty());
    // an exception of a day gone is dropped
    c.staff.clear();
    tidy(&mut c);
    assert!(c.planning.rotas.exceptions.is_empty());
}

#[test]
fn a_rota_prints_as_a_pdf() {
    let mut c = bare();
    let w = week();
    let id = add(&mut c, "Nord", false);
    auto(&mut c, id, &w).unwrap();
    let words = PrintWords { title: "Rota".into(), days: std::array::from_fn(|d| ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][d].to_string()), ..Default::default() };
    let bytes = pdf(&c, 0, &w, "2024-03-04", &words);
    assert!(bytes.starts_with(b"%PDF"));
}

