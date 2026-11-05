//! RosterFlow Pro's daily operations (step 4): a driver's day on the calendar and the hours of his
//! week from Sunday, a day's missing duties, who could take one in the order an open duty is
//! filled in and what he costs (overtime, the hourly minimum), an AI driver's answer the same
//! every time, a duty given for a day (and a swap) run by the day's plan, a reserve still told of
//! a duty given late, a player's answer, and an older file.

use super::super::market::{self, MarketBus, Payment};
use super::super::model::{CompanyLine, Frame, GameMode, Licence, Planner};
use super::super::network::{self, TourOfDay};
use super::super::plan::{self, DayPlan, Problem, Who};
use super::super::pro_turnus::{self as pt, Cell};
use super::super::staff::{applicants, hire};
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

/// Line 5 every day: two buses, 06:00-10:50 and 14:00-18:50 (a duty each: 1001 and 1002 on a
/// weekday).
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

/// A company of line 5 planning with RosterFlow Pro (Monday 4 March 2024), two buses and five
/// drivers, its next two weeks quiet.
fn company() -> Company {
    let mut c = found(&Founding { name: "Daglig".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
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

fn week(c: &Company, from: &str) -> pt::Week {
    (0..pt::DAYS as i64).map(|k| duties(c, &dates::add(from, k))).collect()
}

/// Who drives duty `n` on the day.
fn driver_of(c: &Company, p: &DayPlan, n: u32) -> Option<Who> {
    let ds = duties(c, &p.date);
    let d = ds.iter().find(|x| x.number == n)?;
    let (ti, k) = *pieces_of(p, d).first()?;
    p.tours[ti].duties[k].who
}

/// A rota of one row a week: `a` the mornings (1001) Monday to Friday, `b` the afternoons (1002).
fn rota(c: &mut Company, a: u32, b: u32) {
    let w = week(c, "2024-03-03");
    let id = pt::add(c, "Nord", false);
    pt::add_row(c, id).unwrap();
    pt::add_row(c, id).unwrap();
    for dd in 1..=5 {
        pt::set_cell(c, id, 0, dd, Cell::Duty { n: 1001 }, &w).unwrap();
        pt::set_cell(c, id, 1, dd, Cell::Duty { n: 1002 }, &w).unwrap();
    }
    pt::set_driver(c, id, 0, Some(a)).unwrap();
    pt::set_driver(c, id, 1, Some(b)).unwrap();
}

#[test]
fn a_drivers_day_and_the_missing_duties_of_a_day() {
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1]);
    let date = "2024-03-05";
    let p = day(&c, date);
    let ds = duties(&c, date);
    // a's morning: 1001, signing on before 06:00 and off after 10:50, its minutes paid
    let a = c.employee(ids[0]).unwrap().clone();
    let dd = driver_day(&c, &a, &p, &ds);
    let d1001 = ds.iter().find(|d| d.number == 1001).unwrap();
    assert_eq!(dd.is, DayIs::Duties { numbers: vec![1001], start: d1001.start, end: d1001.end, frame: None, labels: vec!["1001".into()] });
    assert!(d1001.start < 360 && d1001.end > 650, "{d1001:?}");
    assert!((dd.minutes - d1001.paid as f64).abs() < 1.0, "{dd:?} {d1001:?}");
    // a driver without a duty: a day off; nothing missing
    let x = c.employee(ids[2]).unwrap().clone();
    assert_eq!(driver_day(&c, &x, &p, &ds).is, DayIs::Off);
    assert!(open_duties(&p, &ds).is_empty());
    // a ill that day (his rota's exception): his duty is missing, as the illness
    pt::set_exception(&mut c, ids[0], date, Some((Cell::Off, "sick".into())));
    let p = day(&c, date);
    let open = open_duties(&p, &ds);
    assert_eq!(open.iter().map(|o| o.n).collect::<Vec<_>>(), [1001]);
    assert_eq!(open[0].why, Some(Problem::Sick));
    assert_eq!(open[0].lines, ["5"]);
    assert_eq!(driver_day(&c, &a, &p, &ds).is, DayIs::Ill);
    // a reserve's frame, no duty in it: the frame, paid whether used or not
    c.staff[3].employment = Employment::Reserve(Frame::new(300, 780, 0x1f));
    let r = c.staff[3].clone();
    let dd = driver_day(&c, &r, &p, &ds);
    assert_eq!(dd.is, DayIs::Frame { from: 300, to: 780 });
    assert!((dd.minutes - 480.0).abs() < 0.5, "{dd:?}");
    // not employed yet
    c.staff[4].hired = "2024-04-01".into();
    let late = c.staff[4].clone();
    assert_eq!(driver_day(&c, &late, &p, &ds).is, DayIs::Away);
}

#[test]
fn the_hours_of_a_week_count_from_sunday() {
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1]);
    // the week of Monday 4 March 2024 began on Sunday the 3rd and ends on Saturday the 9th
    let dates = week_dates("2024-03-05");
    assert_eq!(dates.first().map(String::as_str), Some("2024-03-03"));
    assert_eq!(dates.last().map(String::as_str), Some("2024-03-09"));
    assert_eq!(week_dates("2024-03-03"), dates, "a Sunday begins its week");
    assert_eq!(week_dates("2024-03-10")[0], "2024-03-10", "the next Sunday a new one");
    // a's week: five mornings of 1001 (Sunday and Saturday off in his row)
    let paid = duties(&c, "2024-03-04").iter().find(|d| d.number == 1001).unwrap().paid as f64;
    let a = c.employee(ids[0]).unwrap().clone();
    let per_day = |date: &str| {
        let p = day(&c, date);
        driver_day(&c, &a, &p, &duties(&c, date)).minutes
    };
    // the company's week: the days gone (Sunday) are in his week's pay so far, the rest planned
    let mut a2 = a.clone();
    a2.week_paid = 123.0;
    let m = week_minutes(&c, &a2, "2024-03-03", &per_day);
    assert!((m - (123.0 + 5.0 * paid)).abs() < 1.0, "{m} vs {}", 123.0 + 5.0 * paid);
    // a week to come: every day planned, nothing of this week's pay
    let m = week_minutes(&c, &a2, "2024-03-10", &per_day);
    assert!((m - 5.0 * paid).abs() < 1.0, "{m}");
}

#[test]
fn who_could_take_a_duty_comes_in_the_order_it_is_filled_in_with_its_price() {
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1]);
    let date = "2024-03-05";
    pt::set_exception(&mut c, ids[0], date, Some((Cell::Off, "sick".into())));
    // ids[2] full time, ids[3] a reserve 05:00-13:00, ids[4] hourly paid
    c.staff[3].employment = Employment::Reserve(Frame::new(300, 780, 0x1f));
    c.staff[4].employment = Employment::Hourly;
    let p = day(&c, date);
    let ds = duties(&c, date);
    let open = open_duties(&p, &ds);
    let d = &open[0];
    assert_eq!(d.n, 1001);
    // ids[2] has worked an hour less than his week: the duty is overtime above his 39 (Normal: no Danish
    // price level)
    let contract = pay::contract_week_minutes(&c);
    let worked = |id: u32| if id == ids[2] { contract - 60.0 } else { 0.0 };
    let list = candidates(&c, &p, d, &worked);
    let can: Vec<u32> = list.iter().filter(|x| x.fit.is_ok()).map(|x| x.id).collect();
    // the reserve within his frame first, the colleagues (the standby first), the hourly paid
    assert_eq!(can.first(), Some(&ids[3]), "{list:?}");
    assert_eq!(can.last(), Some(&ids[4]), "{list:?}");
    let of = |id: u32| list.iter().find(|x| x.id == id).unwrap();
    assert_eq!(of(ids[3]).tier, plan::TIER_RESERVE);
    assert_eq!(of(ids[3]).price, Price::Frame);
    assert_eq!(of(ids[3]).cost, 0);
    // the overtime: his minutes over the week, at 1.5
    let e2 = c.employee(ids[2]).unwrap();
    let over = (contract - 60.0 + d.paid as f64 - contract).round() as i32;
    assert_eq!(of(ids[2]).price, Price::Overtime { minutes: over, rate: econ_uk::OVERTIME_RATE });
    let want = (pay::hour_wage(e2) * over as f64 * 1.5 / 60.0).round() as Cents;
    assert_eq!(of(ids[2]).cost, want);
    // the hourly paid: at least three hours (the duty pays more: its own), 10 % more an hour
    let e4 = c.employee(ids[4]).unwrap();
    assert_eq!(of(ids[4]).price, Price::Hourly { minutes: d.paid.max(pay::HOURLY_MIN_CALL) });
    assert!(pay::hour_wage(e4) > crate::company::staff::hourly(e4.wage));
    // b drives the afternoon: he could take the morning too (a colleague, not standby), within
    // his week
    assert_eq!(of(ids[1]).price, Price::Within);
    assert!(!of(ids[1]).standby);
    // a, ill, cannot - and comes after those who can, with why
    assert!(matches!(of(ids[0]).fit, Err(Cannot::Ill { .. })));
    assert_eq!(list.last().map(|x| x.id), Some(ids[0]));
    // the hourly minimum: a short duty of an hour is paid three
    let (price, _) = price_of(&c, e4, plan::TIER_HOURLY, 60, 0.0, false);
    assert_eq!(price, Price::Hourly { minutes: 180 });
    let (price, _) = price_of(&c, e4, plan::TIER_HOURLY, 60, 0.0, true);
    assert_eq!(price, Price::Hourly { minutes: 60 }, "called in already that day: no minimum again");
    // an agency driver on the day: the agency's price for the duty
    assert!(agency_on_day(&c, &p, d) > 0);
}

#[test]
fn an_ai_drivers_answer_is_human_and_the_same_every_time() {
    let mut c = company();
    let e = c.staff[2].clone();
    let free = Situation { fit: Ok(()), in_frame: false, week: 0.0, contract: Some(39.0 * 60.0), own: None, swap_with: None, can_from: None, can_until: None, tired: Default::default() };
    let r1 = ai_reply(&c, &e, "2024-03-05", 1001, 350, 660, &free);
    assert_eq!(ai_reply(&c, &e, "2024-03-05", 1001, 350, 660, &free), r1, "the same answer asked again");
    // within his frame: yes, his frame is the company's
    assert_eq!(ai_reply(&c, &e, "2024-03-05", 1001, 350, 660, &Situation { in_frame: true, ..free.clone() }), Reply::Frame);
    // (round F: within his frame but asked after noon the day before - short notice: mostly yes,
    // not always)
    assert!(!before_noon(&c, &c.date.clone()));
    let today = c.date.clone();
    let late: Vec<Reply> = (1001..1101).map(|n| ai_reply(&c, &e, &today, n, 900, 1300, &Situation { in_frame: true, ..free.clone() })).collect();
    let no = late.iter().filter(|r| !r.yes()).count();
    assert!(no > 3 && no < 35, "short notice: {no} of 100 said no");
    // his rest to his other work: too early, from when he can
    let rest = Situation { fit: Err(Cannot::Rest { minutes: 5 }), can_from: Some(510), ..free.clone() };
    assert!(matches!(ai_reply(&c, &e, "2024-03-05", 1001, 350, 660, &rest), Reply::Early { from: 510, .. }));
    let rest = Situation { fit: Err(Cannot::Rest { minutes: 5 }), can_until: Some(600), ..free.clone() };
    assert!(matches!(ai_reply(&c, &e, "2024-03-05", 1001, 350, 660, &rest), Reply::Late { until: 600, .. }));
    // ill: no, he is ill
    let ill = Situation { fit: Err(Cannot::Ill { until: "2024-03-06".into() }), ..free.clone() };
    assert!(matches!(ai_reply(&c, &e, "2024-03-05", 1001, 350, 660, &ill), Reply::No { why: NoWhy::Ill, .. }));
    // another duty then: a swap when a colleague could take his own, else busy
    let busy = Situation { fit: Err(Cannot::OnDuty { key: "x".into(), from: 400, to: 600 }), own: Some((1003, 390, 610)), swap_with: Some(77), ..free.clone() };
    let mut swaps = 0;
    for n in 1001..1041 {
        match ai_reply(&c, &e, "2024-03-05", n, 350, 660, &busy) {
            Reply::Swap { with: 77, theirs: 1003, .. } => swaps += 1,
            Reply::No { why: NoWhy::Busy, .. } => {}
            r => panic!("{r:?}"),
        }
    }
    assert!(swaps > 10 && swaps < 40, "{swaps}");
    let lonely = Situation { swap_with: None, ..busy.clone() };
    assert!(matches!(ai_reply(&c, &e, "2024-03-05", 1001, 350, 660, &lonely), Reply::No { why: NoWhy::Busy, .. }));
    // an hourly-paid driver's own hours of the day: before them too early, after them too late
    // (round F: drawn for the date - a day with a window that begins after 05:00 tried)
    c.staff[2].employment = Employment::Hourly;
    let h = c.staff[2].clone();
    let day = (0..60).map(|k| dates::add("2024-03-05", k)).find(|d| matches!(own_hours(&c, &h, d), Some(OwnHours::Window(f, _, _)) if f > 5 * 60)).unwrap();
    let Some(OwnHours::Window(from, until, _)) = own_hours(&c, &h, &day) else { unreachable!() };
    let r = ai_reply(&c, &h, &day, 1001, from - 60, from + 120, &free);
    assert!(matches!(r, Reply::Early { from: f, .. } if f == from), "{r:?}");
    if until < 1440 {
        assert!(matches!(ai_reply(&c, &h, &day, 1001, until - 60, until + 30, &free), Reply::Late { .. }));
    }
    // other days, other hours - most days some, now and then none ("I can't that day")
    let days: Vec<Option<OwnHours>> = (0..200).map(|k| own_hours(&c, &h, &dates::add("2024-03-05", k))).collect();
    let none = days.iter().filter(|d| **d == Some(OwnHours::NotThatDay)).count();
    let kinds: std::collections::HashSet<String> = days.iter().map(|d| format!("{d:?}")).collect();
    assert!(kinds.len() == 5 && none > 5 && none < 40, "{none} days off of 200, {} kinds", kinds.len());
    assert_eq!(own_hours(&c, &h, "2024-03-09"), own_hours(&c, &h, "2024-03-09"), "the same for the same day");
    let off = (0..200).map(|k| dates::add("2024-03-05", k)).find(|d| own_hours(&c, &h, d) == Some(OwnHours::NotThatDay)).unwrap();
    assert!(matches!(ai_reply(&c, &h, &off, 1001, 600, 900, &free), Reply::No { why: NoWhy::NotThatDay, .. }));
    assert!(own_hours(&c, &c.staff[3].clone(), "2024-03-05").is_none(), "only the hourly paid");
    // content and rested: mostly yes; over his week and stressed: mostly no, tired
    let full = c.staff[3].clone();
    let yes = (1001..1061).filter(|n| ai_reply(&c, &full, "2024-03-05", *n, 600, 900, &free).yes()).count();
    let mut tired = full.clone();
    (tired.stress, tired.satisfaction) = (80.0, 30.0);
    let over = Situation { week: 39.0 * 60.0, ..free.clone() };
    let no: Vec<Reply> = (1001..1061).map(|n| ai_reply(&c, &tired, "2024-03-05", n, 600, 900, &over)).collect();
    let tired_no = no.iter().filter(|r| matches!(r, Reply::No { why: NoWhy::Tired, .. })).count();
    assert!(yes > 35, "{yes} of 60 yes");
    assert!(tired_no > 40, "{tired_no} of 60 tired: {no:?}");
    // the words: a template of the translations, its places filled
    let (t, args) = words(&Reply::Early { from: 510, say: 0 }, &|_| String::new());
    assert_eq!(t, SAY_EARLY[0]);
    assert_eq!(args, [("time", "08:30".to_string())]);
    let (t, args) = words(&Reply::Swap { with: 77, theirs: 1003, say: 1 }, &|id| format!("D{id}"));
    assert_eq!(t, SAY_SWAP[1]);
    assert_eq!(args, [("name", "D77".to_string()), ("own", "1003".to_string())]);
}

#[test]
fn a_duty_given_for_a_day_and_a_swap_run_in_the_days_plan() {
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1]);
    let date = "2024-03-05";
    pt::set_exception(&mut c, ids[0], date, Some((Cell::Off, "sick".into())));
    assert_eq!(driver_of(&c, &day(&c, date), 1001), None);
    // asked: x says yes (an AI driver's answer comes on the company's clock)
    let x = ids[2];
    c.clock.minute = 9 * 60;
    let asked = ask(&mut c, date, 1001, 350, 660, vec![(Asked::Staff { id: x }, Some(Reply::Yes { say: 0 }))], 1000).unwrap();
    let a = c.planning.ops.asks.iter().find(|a| a.id == asked[0]).unwrap().clone();
    let game = crate::company::clock::now(&c);
    let due = a.due.unwrap();
    assert!(due > game && a.shown_at(game, 1000).is_none() && a.shown_at(due, 1000).is_some(), "{a:?}");
    assert_eq!(asks_of(&c, date, 1001).count(), 1);
    // given: his that day, the question closed - the rest of the week as the rota has it
    give(&mut c, date, 1001, x, None).unwrap();
    assert_eq!(asks_of(&c, date, 1001).count(), 0);
    assert_eq!(driver_of(&c, &day(&c, date), 1001), Some(Who::Staff(x)));
    assert_eq!(driver_of(&c, &day(&c, "2024-03-06"), 1001), Some(Who::Staff(ids[0])));
    // given to the afternoon's b instead, who swaps: b the morning, y his afternoon
    let y = ids[3];
    give(&mut c, date, 1001, ids[1], Some((y, 1002))).unwrap();
    let p = day(&c, date);
    assert_eq!(driver_of(&c, &p, 1001), Some(Who::Staff(ids[1])));
    assert_eq!(driver_of(&c, &p, 1002), Some(Who::Staff(y)));
    assert_eq!(c.planning.ops.calls.len(), 2);
    // taken back: the row's again (a ill: open)
    assert!(withdraw(&mut c, date, 1001));
    assert!(withdraw(&mut c, date, 1002));
    let p = day(&c, date);
    assert_eq!(driver_of(&c, &p, 1001), None);
    assert_eq!(driver_of(&c, &p, 1002), Some(Who::Staff(ids[1])));
    // a driver who is no driver cannot be given one
    assert_eq!(give(&mut c, date, 1001, 99_999, None), Err(NOT_A_DRIVER));
    // without RosterFlow Pro nothing of it runs
    give(&mut c, date, 1001, x, None).unwrap();
    autopilot::set_planner(&mut c, Planner::RosterFlow);
    assert_eq!(calls_on(&c, date).count(), 0);
    // the days gone dropped
    autopilot::set_planner(&mut c, Planner::Pro);
    c.date = "2024-03-06".into();
    tidy(&mut c);
    assert!(c.planning.ops.calls.is_empty() && c.planning.ops.asks.is_empty());
}

#[test]
fn a_reserve_given_a_duty_from_noon_the_day_before_is_told() {
    let mut c = company();
    let r = c.staff[3].id;
    c.staff[3].employment = Employment::Reserve(Frame::new(300, 780, 0x1f));
    let tomorrow = dates::add(&c.date, 1);
    c.clock.minute = 13 * 60;
    give(&mut c, &tomorrow, 1001, r, None).unwrap();
    let p = day(&c, &tomorrow);
    assert_eq!(driver_of(&c, &p, 1001), Some(Who::Staff(r)));
    let before = c.messages.list.len();
    assert_eq!(plan::tell_reserves(&mut c, &p), 1);
    assert_eq!(c.messages.list.len(), before + 1);
    assert_eq!(c.messages.list.last().map(|m| m.text.as_str()), Some(super::super::messages::RESERVE_DUTY));
    assert_eq!(plan::tell_reserves(&mut c, &p), 0, "told once");
}

#[test]
fn a_player_answers_for_himself() {
    let mut c = company();
    let date = "2024-03-05";
    let ids = ask(&mut c, date, 1002, 830, 1140, vec![(Asked::Guest { no: 3 }, None)], 500).unwrap();
    assert_eq!(asked_of(&c, 3).len(), 1);
    assert!(c.crew.outbox.iter().any(|n| n.template == ASKED_NOTICE && n.to == Some(Who::Guest(3))), "he is told");
    assert_eq!(answer(&mut c, ids[0], 4, Reply::Yes { say: 0 }, 510), Err(NOT_ASKED));
    answer(&mut c, ids[0], 3, Reply::Text { text: "  Kan godt\u{7}, men kun til 18  ".into(), yes: true }, 510).unwrap();
    let a = &c.planning.ops.asks[0];
    assert_eq!(a.shown(510), Some(&Reply::Text { text: "Kan godt, men kun til 18".into(), yes: true }));
    assert!(asked_of(&c, 3).is_empty());
    assert_eq!(answer(&mut c, 999, 3, Reply::Yes { say: 0 }, 510), Err(NO_SUCH_ASK));
    // nobody ticked
    assert_eq!(ask(&mut c, date, 1002, 830, 1140, Vec::new(), 500), Err(NOBODY));
}

#[test]
fn an_older_file_has_no_daily_operations_and_they_are_kept() {
    let old: plan::Planning = serde_json::from_str("{}").unwrap();
    assert!(old.ops.is_empty());
    let mut c = company();
    let v = serde_json::to_value(&c.planning).unwrap();
    assert!(v.get("ops").is_none(), "none kept when there are none");
    let r = c.staff[1].id;
    ask(&mut c, "2024-03-05", 1001, 350, 660, vec![(Asked::Staff { id: r }, Some(Reply::Late { until: 600, say: 2 }))], 1).unwrap();
    give(&mut c, "2024-03-05", 1001, r, None).unwrap();
    let s = serde_json::to_string(&c.planning).unwrap();
    let back: plan::Planning = serde_json::from_str(&s).unwrap();
    assert_eq!(back.ops, c.planning.ops);
}

/// The player's rules of asking (4 November 2026): a reserve within his frame is put on without a
/// question before 12:00 the day before (at 12:00 exactly he must be asked); everyone else is
/// asked, several at once - round G: the permanent drivers too; the duty is given only after a
/// yes (or so put on), and the others who said yes or have not answered are told it is taken.
#[test]
fn the_rules_of_asking_a_reserve_the_hourly_paid_and_a_permanent_driver() {
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    let tomorrow = dates::add(&c.date, 1);
    c.staff[0].employment = Employment::Reserve(Frame::new(300, 780, 0x7f));
    c.staff[3].employment = Employment::Hourly;
    c.staff[4].employment = Employment::Hourly;
    let r = c.staff[0].clone();
    // the day before: 11:59 still without a question, 12:00 asked
    c.clock.minute = 11 * 60 + 59;
    assert!(before_noon(&c, &tomorrow));
    assert_eq!(rule_of(&c, &r, &tomorrow, plan::TIER_RESERVE), Rule::Set);
    c.clock.minute = 12 * 60;
    assert!(!before_noon(&c, &tomorrow));
    assert_eq!(rule_of(&c, &r, &tomorrow, plan::TIER_RESERVE), Rule::Ask);
    assert_eq!(rule_of(&c, &c.staff[3].clone(), &tomorrow, plan::TIER_HOURLY), Rule::Ask);
    assert_eq!(rule_of(&c, &c.staff[1].clone(), &tomorrow, plan::TIER_COLLEAGUE), Rule::Ask);
    // round G: two permanent drivers at once, and the hourly paid alongside
    let yes = || Some(Reply::Yes { say: 0 });
    let date = tomorrow.as_str();
    ask(&mut c, date, 1001, 350, 660, vec![(Asked::Staff { id: ids[1] }, yes()), (Asked::Staff { id: ids[0] }, Some(Reply::No { why: NoWhy::Plans, say: 0 }))], 10).unwrap();
    ask(&mut c, date, 1001, 350, 660, vec![(Asked::Staff { id: ids[3] }, yes()), (Asked::Staff { id: ids[4] }, yes())], 10).unwrap();
    // given before his yes: refused; put on (a reserve's `set`): fine
    assert_eq!(give_asked(&mut c, date, 1001, None, ids[3], None, false, true, 10), Err(NEEDS_YES));
    // a question taken back, another permanent driver asked beside the first
    let first = c.planning.ops.asks.iter().find(|a| a.of_staff() == Some(ids[1])).unwrap().id;
    assert!(withdraw_ask(&mut c, first));
    assert!(!withdraw_ask(&mut c, first), "closed already");
    ask(&mut c, date, 1001, 350, 660, vec![(Asked::Staff { id: ids[2] }, yes())], 10).unwrap();
    ask(&mut c, date, 1001, 350, 660, vec![(Asked::Staff { id: ids[1] }, yes())], 10).unwrap();
    // the answers come: given to an hourly driver who said yes, the others told it is taken
    let latest = c.planning.ops.asks.iter().filter_map(|a| a.due).max().unwrap();
    c.clock.minute = (latest - crate::company::clock::moment(&c.date, 0)) as u32 + 1;
    let said_yes = c.planning.ops.asks.iter().find(|a| a.of_staff() == Some(ids[3])).unwrap().reply.clone().unwrap();
    if said_yes.yes() {
        let before = c.messages.list.len();
        let game = crate::company::clock::now(&c);
        // (those who said yes - ids[4], ids[2], ids[1], unless one of them did not answer - are
        // told; ids[0]'s no needs no word)
        let told = asks_of(&c, date, 1001).filter(|a| a.of_staff() != Some(ids[3]) && a.shown_at(game, 10).is_none_or(Reply::yes)).count();
        assert!(asks_of(&c, date, 1001).any(|a| a.of_staff() == Some(ids[0])) && told <= 3 && told >= 2);
        give_asked(&mut c, date, 1001, None, ids[3], None, false, true, 10).unwrap();
        assert_eq!(c.messages.list.len(), before + told);
        assert!(c.messages.list.iter().rev().take(told).all(|m| m.text == super::super::messages::DUTY_TAKEN));
        assert_eq!(asks_of(&c, date, 1001).count(), 0);
    }
    // the reserve put on without a question
    give_asked(&mut c, date, 1002, None, ids[0], None, true, false, 10).unwrap();
    assert!(c.planning.ops.calls.iter().any(|x| x.n == 1002 && x.driver == ids[0]));
}

/// An AI driver answers on the company's clock: 5-30 minutes, 2-10 when the duty begins within 3
/// hours; never between 23:00 and 06:00 (after 06:00, or not reachable when the duty begins
/// first); a tired driver slower, sometimes not at all ("no answer" after 2 hours); the same
/// every time.
#[test]
fn an_ai_driver_answers_in_game_time_and_not_at_night() {
    use crate::company::clock::{moment, DAY};
    let mut c = company();
    let e = c.staff[1].clone();
    let date = "2024-03-06";
    let yes = Reply::Yes { say: 0 };
    let at = |h: i64| moment("2024-03-05", h);
    let (r, due) = answer_time(&c, &e, date, 1001, 14 * 60, yes.clone(), at(10 * 60), 0.0);
    assert_eq!(answer_time(&c, &e, date, 1001, 14 * 60, yes.clone(), at(10 * 60), 0.0), (r.clone(), due), "the same every time");
    let mut normal = Vec::new();
    let mut urgent = Vec::new();
    for n in 1001..1101 {
        let (r, d) = answer_time(&c, &e, date, n, 14 * 60, yes.clone(), at(10 * 60), 0.0);
        if r != Reply::Silent {
            normal.push(d - at(10 * 60));
        }
        // (the duty at 08:00 the next day, asked at 06:00: within 3 hours)
        let (r, d) = answer_time(&c, &e, date, n, 8 * 60, yes.clone(), moment(date, 6 * 60), 0.0);
        if r != Reply::Silent {
            urgent.push(d - moment(date, 6 * 60));
        }
    }
    assert!(normal.iter().all(|m| (5..=30).contains(m)), "{normal:?}");
    assert!(urgent.iter().all(|m| (2..=10).contains(m)), "{urgent:?}");
    assert!(normal.len() > 85, "a rested driver mostly answers: {}", normal.len());
    // asked at 23:30 for the afternoon: answered between 06:00 and 06:20
    for n in 1001..1041 {
        let (r, d) = answer_time(&c, &e, date, n, 14 * 60, yes.clone(), at(23 * 60 + 30), 0.0);
        if r != Reply::Silent {
            assert!((moment(date, 6 * 60)..=moment(date, 6 * 60 + 20)).contains(&d), "{}", d.rem_euclid(DAY));
        }
    }
    // asked at 23:30 for a duty at 05:00: not reachable - no answer
    let (r, d) = answer_time(&c, &e, date, 1001, 5 * 60, yes.clone(), at(23 * 60 + 30), 0.0);
    assert_eq!((r, d), (Reply::Silent, at(23 * 60 + 30) + NO_ANSWER_AFTER));
    // tired: slower, and more often no answer
    c.staff[1].stress = 85.0;
    let t = c.staff[1].clone();
    let slow: Vec<(Reply, i64)> = (1001..1201).map(|n| answer_time(&c, &t, date, n, 14 * 60, yes.clone(), at(10 * 60), 0.0)).collect();
    assert!(slow.iter().filter(|x| x.0 != Reply::Silent).any(|x| x.1 - at(10 * 60) > 30), "slower");
    let silent = slow.iter().filter(|x| x.0 == Reply::Silent).count();
    assert!(silent > 10 && silent < 60, "{silent} of 200 no answer");
    // an hourly driver short of hours: quicker
    c.staff[2].employment = Employment::Hourly;
    let h = c.staff[2].clone();
    assert!((1001..1101).map(|n| answer_time(&c, &h, date, n, 14 * 60, yes.clone(), at(10 * 60), 0.0)).filter(|x| x.0 != Reply::Silent).all(|x| x.1 - at(10 * 60) <= 18));
    // "no answer" is no answer
    assert!(!Reply::Silent.yes());
    assert_eq!(words(&Reply::Silent, &|_| String::new()).0, SAY_SILENT);
}

// --- asked again the same day after a no (NexiconDK, 4 November 2026) -------------------------------

/// A question to driver `id` for duty `n` of the company's day, answered `reply` at the company's
/// minute `due`.
fn said(c: &mut Company, id: u32, n: u32, reply: Reply, due: i64) {
    c.planning.ops.last += 1;
    let k = c.planning.ops.last;
    c.planning.ops.asks.push(Ask { id: k, date: c.date.clone(), n, start: 600, end: 900, who: Asked::Staff { id }, at: 0, wait: 0, reply: Some(reply), closed: false, part: None, due: Some(due), closed_at: None, seen: false });
}

fn calm() -> Situation {
    Situation { fit: Ok(()), in_frame: false, week: 0.0, contract: Some(39.0 * 60.0), own: None, swap_with: None, can_from: None, can_until: None, tired: Default::default() }
}

#[test]
fn an_irritated_driver_says_yes_much_less_and_grudgingly() {
    let mut c = company();
    c.clock.minute = 10 * 60;
    let game = crate::company::clock::now(&c);
    let e = c.staff[0].clone();
    let yeses = |c: &Company| (2001..2401).filter(|n| ai_reply(c, &e, &c.date.clone(), *n, 600, 900, &calm()).yes()).count();
    let before = yeses(&c);
    assert_eq!(mood_of(&c, e.id, &c.date, game), Mood::Calm);
    said(&mut c, e.id, 1001, Reply::No { why: NoWhy::Plans, say: 0 }, game - 18);
    assert_eq!(mood_of(&c, e.id, &c.date, game), Mood::Irritated { at: game - 18 });
    let after = yeses(&c);
    assert!(before > 150 && (after as f64) < before as f64 * 0.6, "{before} -> {after}");
    let answers: Vec<Reply> = (2001..2101).map(|n| ai_reply(&c, &e, &c.date.clone(), n, 600, 900, &calm())).collect();
    assert!(answers.iter().all(|r| matches!(r, Reply::Grudging { .. } | Reply::No { why: NoWhy::Annoyed, .. })), "{answers:?}");
    assert!(answers.iter().any(|r| matches!(r, Reply::Grudging { .. })));
    // (a reserve within his frame, asked after noon: irritated too)
    let frame = Situation { in_frame: true, ..calm() };
    assert!((2001..2101).all(|n| matches!(ai_reply(&c, &e, &c.date.clone(), n, 600, 900, &frame), Reply::Grudging { .. } | Reply::No { why: NoWhy::Annoyed, .. })));
    // (the same every time; still what he cannot comes first: ill is ill)
    assert_eq!(ai_reply(&c, &e, &c.date.clone(), 2002, 600, 900, &calm()), ai_reply(&c, &e, &c.date.clone(), 2002, 600, 900, &calm()));
    let ill = Situation { fit: Err(Cannot::Ill { until: "2024-03-06".into() }), ..calm() };
    assert!(matches!(ai_reply(&c, &e, &c.date.clone(), 2002, 600, 900, &ill), Reply::No { why: NoWhy::Ill, .. }));
    // (asked again: a little less content)
    let s0 = c.employee(e.id).unwrap().satisfaction;
    let date = c.date.clone();
    ask_part(&mut c, &date, 1002, None, 600, 900, vec![(Asked::Staff { id: e.id }, Some(Reply::No { why: NoWhy::Annoyed, say: 1 }))], 0, &|_| 0.0).unwrap();
    assert_eq!(c.employee(e.id).unwrap().satisfaction, s0 - IRRITATED_COST);
    let (words, _) = words(&Reply::No { why: NoWhy::Annoyed, say: 1 }, &|_| String::new());
    assert_eq!(words, "Again? No.");
}

#[test]
fn an_angry_driver_always_says_no_and_then_answers_no_more_today() {
    let mut c = company();
    c.clock.minute = 11 * 60;
    let game = crate::company::clock::now(&c);
    let id = c.staff[1].id;
    let e = c.staff[1].clone();
    said(&mut c, id, 1001, Reply::No { why: NoWhy::Plans, say: 0 }, game - 60);
    said(&mut c, id, 1002, Reply::No { why: NoWhy::Annoyed, say: 0 }, game - 20);
    assert_eq!(mood_of(&c, id, &c.date, game), Mood::Angry { at: game - 20 });
    assert!((2001..2101).all(|n| matches!(ai_reply(&c, &e, &c.date.clone(), n, 600, 900, &calm()), Reply::No { why: NoWhy::Angry, .. })));
    let s0 = c.employee(id).unwrap().satisfaction;
    let date = c.date.clone();
    ask_part(&mut c, &date, 1003, None, 600, 900, vec![(Asked::Staff { id }, Some(Reply::No { why: NoWhy::Angry, say: 0 }))], 0, &|_| 0.0).unwrap();
    assert_eq!(c.employee(id).unwrap().satisfaction, s0 - ANGRY_COST);
    // (his angry no come: no more questions today)
    c.clock.minute += 120;
    let later = crate::company::clock::now(&c);
    assert!(matches!(mood_of(&c, id, &c.date, later), Mood::Done { .. }), "{:?}", mood_of(&c, id, &c.date, later));
    assert_eq!(ask_part(&mut c, &date, 1004, None, 600, 900, vec![(Asked::Staff { id }, Some(Reply::Yes { say: 0 }))], 0, &|_| 0.0), Err(ANSWERS_NO_MORE));
    // (never below nothing, and never gone for it)
    c.staff.iter_mut().find(|e| e.id == id).unwrap().satisfaction = 1.0;
    c.planning.ops.asks.retain(|a| a.n != 1003);
    ask_part(&mut c, &date, 1005, None, 600, 900, vec![(Asked::Staff { id }, Some(Reply::No { why: NoWhy::Angry, say: 0 }))], 0, &|_| 0.0).unwrap();
    assert_eq!(c.employee(id).unwrap().satisfaction, 0.0);
    assert!(c.employee(id).is_some());
}

#[test]
fn only_real_refusals_of_the_day_count_and_a_new_day_starts_calm() {
    let mut c = company();
    c.clock.minute = 12 * 60;
    let game = crate::company::clock::now(&c);
    let id = c.staff[2].id;
    said(&mut c, id, 1001, Reply::Early { from: 700, say: 0 }, game - 50);
    said(&mut c, id, 1002, Reply::Late { until: 700, say: 0 }, game - 40);
    said(&mut c, id, 1003, Reply::Silent, game - 30);
    said(&mut c, id, 1004, Reply::No { why: NoWhy::Ill, say: 0 }, game - 25);
    said(&mut c, id, 1005, Reply::No { why: NoWhy::Plans, say: 0 }, game + 30);
    said(&mut c, id, 1006, Reply::No { why: NoWhy::Plans, say: 0 }, game - 10);
    c.planning.ops.asks.last_mut().unwrap().closed_at = Some(game - 15);
    assert_eq!(mood_of(&c, id, &c.date, game), Mood::Calm, "too early, too late, no answer, ill, not come yet, taken back first");
    // (taken back after his no: it counts)
    said(&mut c, id, 1007, Reply::No { why: NoWhy::Tired, say: 0 }, game - 5);
    let k = c.planning.ops.last;
    assert!(withdraw_ask(&mut c, k));
    assert_eq!(mood_of(&c, id, &c.date, game), Mood::Irritated { at: game - 5 });
    // (asked the same duty again: the old no kept)
    let date = c.date.clone();
    ask_part(&mut c, &date, 1007, None, 600, 900, vec![(Asked::Staff { id }, Some(Reply::Yes { say: 0 }))], 0, &|_| 0.0).unwrap();
    assert_eq!(c.planning.ops.noes, vec![Refusal { driver: id, at: game - 5, date: c.date.clone() }]);
    assert_eq!(mood_of(&c, id, &c.date, game), Mood::Irritated { at: game - 5 });
    // (round F: a no for this date leaves him calm about a duty of another date)
    let tomorrow = dates::add(&c.date, 1);
    assert_eq!(mood_of(&c, id, &tomorrow, game), Mood::Calm);
    // (the next day: calm again; the day after the company's, the old noes dropped)
    use crate::company::clock::DAY;
    assert_eq!(mood_of(&c, id, &c.date, game + DAY), Mood::Calm);
    c.date = dates::add(&c.date, 1);
    tidy(&mut c);
    assert!(c.planning.ops.noes.is_empty());
}

/// (Round F, the player's report from beta) A driver's mood is about the duty's day: a no for the
/// 15th leaves him calm for the 16th; two noes for the 15th make him angry - for the 15th only.
#[test]
fn a_mood_is_about_the_duties_day_not_the_day_of_asking() {
    let mut c = company();
    c.clock.minute = 10 * 60;
    let game = crate::company::clock::now(&c);
    let id = c.staff[2].id;
    let (d15, d16) = (dates::add(&c.date, 1), dates::add(&c.date, 2));
    let no_for = |c: &mut Company, date: &str, n: u32, at: i64| {
        c.planning.ops.last += 1;
        let k = c.planning.ops.last;
        c.planning.ops.asks.push(Ask { id: k, date: date.to_string(), n, start: 600, end: 900, who: Asked::Staff { id }, at: 0, wait: 0, reply: Some(Reply::No { why: NoWhy::Plans, say: 0 }), closed: false, part: None, due: Some(at), closed_at: None, seen: false });
    };
    no_for(&mut c, &d15, 1001, game - 30);
    assert_eq!(mood_of(&c, id, &d15, game), Mood::Irritated { at: game - 30 });
    assert_eq!(mood_of(&c, id, &d16, game), Mood::Calm, "another day's duty: calm");
    let e = c.employee(id).unwrap().clone();
    let calm16 = (2001..2101).filter(|n| matches!(ai_reply(&c, &e, &d16, *n, 600, 900, &calm()), Reply::No { why: NoWhy::Annoyed | NoWhy::Angry, .. })).count();
    assert_eq!(calm16, 0, "no moody answers for the 16th");
    no_for(&mut c, &d15, 1002, game - 10);
    assert_eq!(mood_of(&c, id, &d15, game), Mood::Angry { at: game - 10 });
    assert_eq!(mood_of(&c, id, &d16, game), Mood::Calm);
    // (asked the 15th's duty again: the kept no remembers its date)
    ask_part(&mut c, &d15, 1001, None, 600, 900, vec![(Asked::Staff { id }, None)], 0, &|_| 0.0).unwrap();
    assert!(c.planning.ops.noes.iter().all(|x| x.date == d15));
    assert_eq!(mood_of(&c, id, &d16, game), Mood::Calm);
    // (an older file's kept no without a date: not counted)
    c.planning.ops.noes.push(Refusal { driver: id, at: game - 5, date: String::new() });
    assert_eq!(mood_of(&c, id, &d16, game), Mood::Calm);
}

// --- round A (NexiconDK, 4 November 2026): the sick call, the rest rules, the fair share --------

#[test]
fn a_driver_calls_in_sick_before_his_first_duty_and_the_player_is_told() {
    use super::super::pro_sick::{self as ps, SickCall};
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1]);
    // (a worn-out, unhappy, unreliable driver calls more often: the day's draws the same each time)
    let mut worn = c.employee(ids[0]).unwrap().clone();
    let calm = ps::chance(&c, &worn);
    (worn.stress, worn.satisfaction, worn.reliability) = (90.0, 20.0, 0.3);
    assert!(ps::chance(&c, &worn) > calm * 3.0, "{calm} {}", ps::chance(&c, &worn));
    for e in c.staff.iter_mut() {
        (e.stress, e.satisfaction, e.reliability) = (100.0, 0.0, 0.0);
    }
    let base = c.id.clone();
    let mut found = None;
    for k in 0..400 {
        c.id = format!("{base}s{k}");
        let p = day(&c, &c.date.clone());
        let ds = duties(&c, &c.date.clone());
        let d = ps::draws(&c, &p, &ds);
        assert_eq!(d, ps::draws(&c, &p, &ds), "the same every time");
        if let Some(x) = d.first().copied() {
            found = Some((x, p));
            break;
        }
    }
    let ((at, driver, n, start), p) = found.expect("a call in 400 companies");
    assert!(ids[..2].contains(&driver));
    assert!(at >= ps::EARLIEST && at < start, "{at} {start}");
    // (the day's script has it at its time)
    let s = super::super::clock::script(&c, &p);
    assert!(s.events.iter().any(|e| e.at == at as i64 && e.what == super::super::clock::What::SickCall { driver, n, start }));
    // the call: ill from today, his message to the control room, the player's notice waiting
    let moment = super::super::clock::moment(&c.date, at as i64);
    assert!(ps::call(&mut c, driver, n, start, moment));
    let today = c.date.clone();
    assert!(c.employee(driver).unwrap().absent(&today));
    assert!(c.messages.list.iter().any(|m| m.text == ps::SAYS && m.to == super::super::messages::To::Control));
    assert_eq!(ps::unseen(&c).map(|x| (x.driver, x.n)), Some((driver, n)));
    c.clock.minute = at as u32;
    let x = ps::unseen(&c).unwrap().clone();
    let (text, args) = ps::words(&c, &x);
    assert_eq!(text, ps::CALLED);
    assert_eq!(args.iter().find(|a| a.0 == "min").map(|a| a.1.clone()), Some((start - at).to_string()));
    // (his duty is open now - for his illness; the calendar shows him ill)
    let p = day(&c, &today);
    let ds = duties(&c, &today);
    let open = open_duties(&p, &ds);
    assert!(open.iter().any(|o| o.n == n && o.why == Some(Problem::Sick)), "{open:?}");
    assert_eq!(driver_day(&c, c.employee(driver).unwrap(), &p, &ds).is, DayIs::Ill);
    // seen: no longer shown; called again (ill already): nothing
    ps::seen(&mut c, Some(driver));
    assert!(ps::unseen(&c).is_none());
    assert!(!ps::call(&mut c, driver, n, start, moment));
    // (a duty begun: no call)
    let other = if driver == ids[0] { ids[1] } else { ids[0] };
    assert!(!ps::call(&mut c, other, 1002, 300, super::super::clock::moment(&today, 400)));
    // (an older file: no calls; a call of another day tidied away)
    let old: Ops = serde_json::from_str("{}").unwrap();
    assert!(old.sick.is_empty() && old.extras.is_empty() && old.rest_fine == 0);
    c.planning.ops.sick.push(SickCall { date: "2024-03-01".into(), driver, n, start, at: 0, seen: false });
    tidy(&mut c);
    assert!(c.planning.ops.sick.iter().all(|x| x.date == today));
}

#[test]
fn the_rest_rules_eleven_hours_a_day_of_thirteen_a_week_of_forty_eight() {
    use super::super::pro_rest::{breaches, reduced_in, Breach};
    // 11 hours after yesterday's 1005 (ends 22:00) and before tomorrow's: nothing broken
    assert!(breaches(9 * 60, 17 * 60, Some((14 * 60, 22 * 60, 1005)), Some((6 * 60, 14 * 60, 1001)), None, 0, 0.0, 480).is_empty());
    // 9 hours: allowed while fewer than three reduced rests this week, broken after
    assert!(breaches(7 * 60, 15 * 60, Some((14 * 60, 22 * 60, 1005)), None, None, 0, 0.0, 480).is_empty());
    assert_eq!(breaches(7 * 60, 15 * 60, Some((14 * 60, 22 * 60, 1005)), None, None, 3, 0.0, 480), vec![Breach::RestAfter { minutes: 9 * 60, n: 1005 }]);
    // 8 hours: always (14 hours before the day after: fine); before the day after too
    assert_eq!(breaches(6 * 60, 15 * 60, Some((14 * 60, 22 * 60, 1005)), Some((5 * 60, 12 * 60, 2001)), None, 0, 0.0, 540), vec![Breach::RestAfter { minutes: 8 * 60, n: 1005 }]);
    assert_eq!(breaches(14 * 60, 23 * 60, None, Some((6 * 60, 12 * 60, 2001)), None, 3, 0.0, 540), vec![Breach::RestBefore { minutes: 7 * 60, n: 2001 }]);
    // a day of more than 13 hours with his own morning duty
    assert_eq!(breaches(15 * 60, 20 * 60, None, None, Some((5 * 60, 11 * 60, 1001)), 0, 0.0, 300), vec![Breach::LongDay { minutes: 15 * 60 }]);
    // a week over 48 hours
    assert_eq!(breaches(6 * 60, 14 * 60, None, None, None, 0, 42.0 * 60.0, 480), vec![Breach::LongWeek { minutes: 50 * 60 }]);
    // the reduced rests of a week
    assert_eq!(reduced_in(&[Some((6 * 60, 22 * 60, 1)), Some((7 * 60, 15 * 60, 2)), None, Some((14 * 60, 23 * 60, 3)), Some((8 * 60, 16 * 60, 4))]), 2);
}

#[test]
fn breaking_the_rest_rules_anyway_costs_content_and_the_fine() {
    use super::super::pro_rest as pr;
    let mut c = company();
    let id = c.staff[0].id;
    let (s0, t0) = (c.staff[0].satisfaction, c.staff[0].stress);
    let cash = c.cash;
    pr::broke(&mut c, id);
    assert_eq!((c.staff[0].satisfaction, c.staff[0].stress, c.cash), (s0 - pr::BREAK_COST, t0 + pr::BREAK_STRESS, cash));
    // the owner's fine off the server; on the server only its admin's
    pr::set_fine(&mut c, 1_000_00).unwrap();
    assert_eq!(pr::fine(&c), 1_000_00);
    pr::broke(&mut c, id);
    assert!(c.cash < cash, "{} {cash}", c.cash);
    c.online.on_server = true;
    assert_eq!(pr::set_fine(&mut c, 0), Err(pr::SERVER_DECIDES));
    assert_eq!(pr::fine(&c), 0);
    c.online.live = Some(super::super::live_pay::LiveCfg { rest_fine: 2_500_00, ..Default::default() });
    assert_eq!(pr::fine(&c), 2_500_00);
    let kv: std::collections::HashMap<String, String> = [("rest_fine".to_string(), "750".to_string())].into_iter().collect();
    assert_eq!(super::super::live_pay::LiveCfg::of_keys(&kv).rest_fine, 750_00);
}

#[test]
fn the_extra_duties_of_the_month_are_counted_and_the_forgotten_grumble() {
    use super::super::pro_rest as pr;
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    c.staff.iter_mut().find(|e| e.id == ids[4]).unwrap().employment = Employment::Hourly;
    c.date = "2024-03-11".into();
    let date = c.date.clone();
    // given many: counted for the month, more stressed beyond `TIRED_FROM`
    let stress = c.employee(ids[0]).unwrap().stress;
    for _ in 0..pr::TIRED_FROM {
        pr::count_extra(&mut c, ids[0], &date);
    }
    assert_eq!(pr::extras_in_month(&c, ids[0], &date), pr::TIRED_FROM);
    assert_eq!(c.employee(ids[0]).unwrap().stress, stress);
    pr::count_extra(&mut c, ids[0], &date);
    assert_eq!(c.employee(ids[0]).unwrap().stress, stress + pr::TIRED_STRESS);
    assert_eq!(pr::extras_in_month(&c, ids[0], "2024-04-02"), 0, "another month");
    // the hourly-paid driver given none grumbles - once a week; the permanent ones do not
    let content = c.employee(ids[4]).unwrap().satisfaction;
    assert_eq!(pr::grumbles(&mut c), vec![ids[4]]);
    assert_eq!(c.employee(ids[4]).unwrap().satisfaction, content - pr::GRUMBLE_COST);
    assert!(c.messages.list.iter().any(|m| m.text == pr::NEVER_ASKED && matches!(&m.from, super::super::messages::From::Driver { name, .. } if *name == c.employee(ids[4]).unwrap().name)));
    assert!(pr::grumbles(&mut c).is_empty());
    c.date = "2024-03-18".into();
    assert_eq!(pr::grumbles(&mut c), vec![ids[4]], "a week on");
    // (given one, he no longer grumbles; the first week of a month nobody does)
    c.date = "2024-03-25".into();
    pr::count_extra(&mut c, ids[4], "2024-03-25");
    assert!(pr::grumbles(&mut c).is_empty());
    c.date = "2024-04-03".into();
    assert!(pr::grumbles(&mut c).is_empty());
    // a duty given and taken back: no longer counted
    c.date = date.clone();
    give_asked(&mut c, &date, 1001, None, ids[1], None, true, true, 10).unwrap();
    assert_eq!(pr::extras_in_month(&c, ids[1], &date), 1);
    assert!(withdraw(&mut c, &date, 1001));
    assert_eq!(pr::extras_in_month(&c, ids[1], &date), 0);
    // (a reserve put on within his frame is no extra duty)
    give_asked(&mut c, &date, 1001, None, ids[1], None, true, false, 10).unwrap();
    assert_eq!(pr::extras_in_month(&c, ids[1], &date), 0);
}

#[test]
fn a_late_duty_gets_its_sick_call_in_the_afternoon_shortly_before_it() {
    use super::super::pro_sick as ps;
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1]);
    for e in c.staff.iter_mut() {
        (e.stress, e.satisfaction, e.reliability) = (100.0, 0.0, 0.0);
    }
    let base = c.id.clone();
    let date = c.date.clone();
    let mut late = None;
    let mut early = None;
    for k in 0..2000 {
        c.id = format!("{base}l{k}");
        let p = day(&c, &date);
        let ds = duties(&c, &date);
        for (at, driver, n, start) in ps::draws(&c, &p, &ds) {
            assert!(at < start && at >= ps::EARLIEST);
            if start >= 12 * 60 && at >= start - 180 && at >= 11 * 60 {
                late.get_or_insert((at, driver, n, start));
            }
            if start <= 8 * 60 {
                assert!(at >= start - 75, "an early duty's call shortly before it: {at} {start}");
                early.get_or_insert(at);
            }
        }
        if late.is_some() && early.is_some() {
            break;
        }
    }
    let (at, driver, n, start) = late.expect("an afternoon call for the late duty");
    assert_eq!(driver, ids[1], "the afternoon's driver");
    assert!(early.is_some());
    // its notice: the minutes left until it signs on
    let moment = super::super::clock::moment(&date, at as i64);
    c.clock.minute = at as u32;
    assert!(ps::call(&mut c, driver, n, start, moment));
    let x = ps::unseen(&c).unwrap().clone();
    let (text, args) = ps::words(&c, &x);
    assert_eq!(text, ps::CALLED);
    assert_eq!(args.iter().find(|a| a.0 == "min").map(|a| a.1.clone()), Some((start - at).to_string()));
}

// --- round B (NexiconDK, 5 November 2026): the drivers' wishes, time off in lieu, the week's cost --

#[test]
fn drivers_send_wishes_now_and_then_the_same_every_time_and_unanswered_they_lapse() {
    use super::super::pro_wish::{self as pw, WishKind, WishState};
    let mut c = company();
    for e in c.staff.iter_mut() {
        e.satisfaction = 30.0;
    }
    // (less content: more often; friendlier: less)
    let e = c.staff[0].clone();
    let mut glad = e.clone();
    (glad.satisfaction, glad.friendliness) = (90.0, 100.0);
    assert!(pw::chance(&e) > pw::chance(&glad) * 1.5, "{} {}", pw::chance(&e), pw::chance(&glad));
    let base = c.id.clone();
    let mut sent = Vec::new();
    for k in 0..300 {
        c.id = format!("{base}w{k}");
        c.planning.ops.wished.clear();
        let mut again = c.clone();
        let got = pw::daily(&mut c);
        assert_eq!(got, pw::daily(&mut again), "the same every time");
        assert!(pw::daily(&mut c).is_empty(), "once a day");
        sent.extend(got);
        if sent.len() >= 3 {
            break;
        }
    }
    assert!(sent.len() >= 3, "wishes are sent");
    let w = c.planning.ops.wishes.iter().find(|w| w.id == sent[0]).unwrap().clone();
    assert_eq!(w.state, WishState::Open);
    assert!(c.messages.list.iter().any(|m| m.to == super::super::messages::To::Control && pw::texts().contains(&m.text.as_str())));
    if let WishKind::DayOff { date } = &w.wish {
        let ahead = dates::between(&c.date, date);
        assert!((4..=14).contains(&ahead), "{ahead}");
    }
    // left open: a day off lapses on its day, the others after a week - less content
    let s0 = c.employee(w.driver).unwrap().satisfaction;
    c.date = dates::add(&c.date, 15);
    pw::lapse(&mut c);
    let w2 = c.planning.ops.wishes.iter().find(|x| x.id == w.id).unwrap();
    assert_eq!(w2.state, WishState::Lapsed);
    assert!(c.employee(w.driver).unwrap().satisfaction < s0);
    // (an older file: none)
    let old: Ops = serde_json::from_str("{}").unwrap();
    assert!(old.wishes.is_empty() && !old.toil_default && old.is_empty());
}

/// A wish of driver `id` put in the company's list.
fn wish(c: &mut Company, id: u32, w: super::super::pro_wish::WishKind) -> u32 {
    c.planning.ops.last_wish += 1;
    let k = c.planning.ops.last_wish;
    let sent = c.date.clone();
    c.planning.ops.wishes.push(super::super::pro_wish::Wish { id: k, driver: id, wish: w, sent, state: Default::default(), decided: None });
    k
}

#[test]
fn a_day_off_granted_frees_the_driver_and_leaves_his_duty_open() {
    use super::super::pro_wish::{self as pw, WishKind};
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1]);
    let date = "2024-03-06".to_string();
    let k = wish(&mut c, ids[0], WishKind::DayOff { date: date.clone() });
    // (marked on his day, and in the popup by his name)
    assert!(pw::marked(&c, ids[0], &date).is_some());
    assert_eq!(pw::notes(&c, ids[0], &date, 360, 650), vec![pw::NOTE_DAY_OFF]);
    let s0 = c.employee(ids[0]).unwrap().satisfaction;
    assert_eq!(pw::grant(&mut c, k), Ok(Some(date.clone())));
    assert_eq!(c.employee(ids[0]).unwrap().satisfaction, s0 + pw::GRANTED);
    assert_eq!(pw::grant(&mut c, k), Err(pw::NO_SUCH_WISH));
    // free that day: absent, his rota's 1001 open, his day off; he cannot take a duty
    let e = c.employee(ids[0]).unwrap().clone();
    assert!(e.absent(&date));
    let p = day(&c, &date);
    let ds = duties(&c, &date);
    assert!(open_duties(&p, &ds).iter().any(|o| o.n == 1001));
    assert_eq!(driver_day(&c, &e, &p, &ds).is, DayIs::Off);
    assert_eq!(pt::decided(&c, ids[0], &date).and_then(|d| d.note).as_deref(), Some("wish"));
    let d = open_duties(&p, &ds).into_iter().find(|o| o.n == 1001).unwrap();
    let cand = candidates(&c, &p, &d, &|_| 0.0).into_iter().find(|x| x.id == ids[0]).unwrap();
    assert_eq!(cand.fit, Err(Cannot::DayOff { toil: false }));
    let sit = Situation { fit: cand.fit.clone(), ..calm() };
    assert!(matches!(ai_reply(&c, &e, &date, 1001, 360, 650, &sit), Reply::No { why: NoWhy::DayOff, .. }));
    // (the next day as planned)
    assert_eq!(driver_of(&c, &day(&c, "2024-03-07"), 1001), Some(Who::Staff(ids[0])));
    // refused three times in a row: more so
    let other = ids[2];
    let mut costs = Vec::new();
    for _ in 0..3 {
        let k = wish(&mut c, other, WishKind::Early);
        let before = c.employee(other).unwrap().satisfaction;
        pw::refuse(&mut c, k).unwrap();
        costs.push(before - c.employee(other).unwrap().satisfaction);
    }
    assert_eq!(costs, vec![pw::REFUSED, pw::REFUSED, pw::REFUSED + pw::IN_A_ROW]);
    assert_eq!(pw::refused_in_a_row(&c, other), 3);
}

#[test]
fn a_drivers_wishes_change_his_answer_and_being_asked_anyway_annoys_him() {
    use super::super::pro_wish::{self as pw, WishKind};
    let mut c = company();
    let e = c.staff[0].clone();
    let date = "2024-03-06".to_string();
    assert_eq!(pw::bias(&c, e.id, &date, 360, 650), 1.0);
    wish(&mut c, e.id, WishKind::DayOff { date: date.clone() });
    assert!(pw::bias(&c, e.id, &date, 360, 650) < 0.5);
    let yes = |c: &Company, d: &str| (3001..3301).filter(|n| ai_reply(c, &e, d, *n, 600, 900, &calm()).yes()).count();
    assert!(yes(&c, &date) * 2 < yes(&c, "2024-03-07"), "{} {}", yes(&c, &date), yes(&c, "2024-03-07"));
    // early duties wished: glad of an early one, rather not of a late one
    let x = c.staff[1].id;
    wish(&mut c, x, WishKind::Early);
    assert!(pw::bias(&c, x, &date, 300, 700) > 1.0 && pw::bias(&c, x, &date, 800, 1300) < 1.0);
    let y = c.staff[2].id;
    wish(&mut c, y, WishKind::NoSplit);
    assert!(pw::bias(&c, y, &date, 300, 1100) < 1.0 && pw::bias(&c, y, &date, 300, 700) == 1.0);
    // asked on the day he wished off: a little less content
    let s0 = c.employee(e.id).unwrap().satisfaction;
    ask_part(&mut c, &date, 1001, None, 360, 650, vec![(Asked::Staff { id: e.id }, Some(Reply::No { why: NoWhy::Plans, say: 0 }))], 0, &|_| 0.0).unwrap();
    assert_eq!(c.employee(e.id).unwrap().satisfaction, s0 - pw::ASKED_ANYWAY);
}

#[test]
fn overtime_saved_as_time_off_in_lieu_is_not_paid_and_a_day_of_it_is() {
    use super::super::pro_toil as ptl;
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1]);
    let date = c.date.clone();
    let contract = pay::contract_week_minutes(&c);
    // his week full already: the day's duty is all overtime
    for e in c.staff.iter_mut() {
        e.week_paid = contract;
    }
    let pieces = [(ids[0], 360, 650)];
    let mut paid = c.clone();
    let p1 = pay::day(&mut paid, &date, &pieces);
    assert!(p1.overtime > 0.0 && p1.saved == 0.0);
    give(&mut c, &date, 1001, ids[0], None).unwrap();
    assert!(set_toil(&mut c, &date, 1001, None, ids[0], true));
    assert!(c.planning.ops.calls[0].toil);
    let mut saved = c.clone();
    let p2 = pay::day(&mut saved, &date, &pieces);
    assert!(p2.cost < p1.cost, "{} {}", p2.cost, p1.cost);
    assert!((p2.saved - p1.overtime).abs() < 1e-9);
    let toil = saved.employee(ids[0]).unwrap().toil;
    assert!((toil - p1.overtime * 60.0).abs() < 1e-6, "{toil}");
    // (an hourly-paid driver saves none)
    c.staff.iter_mut().find(|e| e.id == ids[3]).unwrap().employment = Employment::Hourly;
    assert!(!ptl::can_save(c.employee(ids[3]).unwrap()));
    assert!(!set_toil(&mut c, &date, 1001, None, ids[3], true));
    // a day of it: the hours taken, off that day, paid at the normal rate
    let mut c = saved;
    c.staff.iter_mut().find(|e| e.id == ids[0]).unwrap().toil = 10.0 * 60.0;
    let day_m = ptl::day_of(&c, c.employee(ids[0]).unwrap());
    assert_eq!(day_m, (contract / 5.0).round());
    let off = "2024-03-06";
    assert_eq!(ptl::give_day(&mut c, ids[0], off), Ok(day_m));
    assert_eq!(ptl::give_day(&mut c, ids[0], off), Err(ptl::OFF_ALREADY));
    let e = c.employee(ids[0]).unwrap().clone();
    assert!((e.toil - (600.0 - day_m)).abs() < 1e-9 && e.absent(off));
    let p = day(&c, off);
    let ds = duties(&c, off);
    assert_eq!(driver_day(&c, &e, &p, &ds).is, DayIs::Toil);
    assert!(open_duties(&p, &ds).iter().any(|o| o.n == 1001));
    for e in c.staff.iter_mut() {
        (e.week_paid, e.week_due) = (0.0, 0.0);
    }
    let mut a = c.clone();
    let with = pay::day(&mut a, off, &[]).cost;
    assert!(with > 0, "the day of it is paid");
    assert_eq!(a.employee(ids[0]).unwrap().week_due, 0.0, "no guarantee for it");
    // taken back: the hours his again
    assert!(ptl::take_back(&mut c, ids[0], off));
    assert!((c.employee(ids[0]).unwrap().toil - 600.0).abs() < 1e-9 && !c.employee(ids[0]).unwrap().absent(off));
    assert_eq!(ptl::give_day(&mut c, ids[0], "2024-03-01"), Err(ptl::GONE));
    c.staff.iter_mut().find(|e| e.id == ids[0]).unwrap().toil = 30.0;
    assert_eq!(ptl::give_day(&mut c, ids[0], off), Err(ptl::TOO_FEW));
}

#[test]
fn too_many_hours_of_time_off_in_lieu_make_a_driver_ask_for_them() {
    use super::super::pro_toil as ptl;
    let mut c = company();
    let id = c.staff[0].id;
    c.staff[0].toil = 40.0 * 60.0;
    let base = c.id.clone();
    let mut asked = false;
    for k in 0..60 {
        c.id = format!("{base}t{k}");
        let s0 = c.employee(id).unwrap().satisfaction;
        let got = ptl::daily(&mut c);
        assert_eq!(c.employee(id).unwrap().satisfaction, s0 - ptl::TOO_MANY_COST, "too many: less content");
        if let Some((who, h)) = got.first() {
            assert_eq!((*who, h.as_str()), (id, "40"));
            asked = true;
            break;
        }
    }
    assert!(asked);
    assert!(c.messages.list.iter().any(|m| m.text == ptl::ASKS));
    // (at most once a week)
    for k in 0..30 {
        c.id = format!("{base}u{k}");
        assert!(ptl::daily(&mut c).is_empty());
    }
}

#[test]
fn the_evening_the_night_and_the_weekend_pay_a_supplement_with_rosterflow_pro() {
    // (dates::weekday: 0 Monday ... 5 Saturday, 6 Sunday)
    assert_eq!(pay::supplement_rate(0, 10 * 60), 0.0);
    assert_eq!(pay::supplement_rate(0, 19 * 60), pay::EVENING);
    assert_eq!(pay::supplement_rate(0, 23 * 60 + 30), pay::NIGHT);
    assert_eq!(pay::supplement_rate(0, 5 * 60), pay::NIGHT);
    assert_eq!(pay::supplement_rate(5, 13 * 60), 0.0);
    assert_eq!(pay::supplement_rate(5, 15 * 60), pay::SATURDAY);
    assert_eq!(pay::supplement_rate(6, 10 * 60), pay::SUNDAY);
    // (past midnight: the next day's - Saturday night into Sunday)
    assert_eq!(pay::supplement_rate(5, 24 * 60 + 60), pay::SUNDAY);
    // an hour of the evening is 9 minutes more; overlapping pieces once
    assert!((pay::supplement_minutes(0, &[(18 * 60, 19 * 60)]) - 9.0).abs() < 1e-9);
    assert!((pay::supplement_minutes(0, &[(18 * 60, 19 * 60), (18 * 60 + 30, 19 * 60)]) - 9.0).abs() < 1e-9);
    // paid with RosterFlow Pro only
    let c = company();
    let id = c.staff[0].id;
    let mut pro = c.clone();
    let evening = pay::day(&mut pro, "2024-03-04", &[(id, 15 * 60, 21 * 60)]).cost;
    let mut day_only = c.clone();
    let morning = pay::day(&mut day_only, "2024-03-04", &[(id, 8 * 60, 14 * 60)]).cost;
    assert!(evening > morning, "{evening} {morning}");
    let mut std = c.clone();
    autopilot::set_planner(&mut std, Planner::RosterFlow);
    assert!(!super::super::pro_duties::on(&std));
    let a = pay::day(&mut std.clone(), "2024-03-04", &[(id, 15 * 60, 21 * 60)]).cost;
    let b = pay::day(&mut std, "2024-03-04", &[(id, 8 * 60, 14 * 60)]).cost;
    assert_eq!(a, b, "no supplement without RosterFlow Pro");
}

#[test]
fn what_a_week_costs_as_planned() {
    use super::super::pro_cost::{self as pcost, WeekCost};
    let c = company();
    let e = c.staff[0].clone();
    let hour = pay::hour_wage(&e);
    let k = contract_minutes(&c, &e).unwrap();
    // under his contract: the guarantee pays the week
    let w = pcost::week(&c, &e, k - 120.0, &[], 0.0, 0.0);
    assert_eq!(w, WeekCost { wages: (hour * k / 60.0).round() as Cents, overtime: 0, supplements: 0, saved: 0.0, holidays: 0 });
    // two hours over it: overtime at its rate; saved as time off in lieu: not paid
    let o = pcost::week(&c, &e, k + 120.0, &[], 0.0, 0.0);
    assert_eq!(o.overtime, (hour * 120.0 * econ_uk::OVERTIME_RATE / 60.0).round() as Cents);
    let s = pcost::week(&c, &e, k + 120.0, &[], 0.0, 480.0);
    assert_eq!((s.overtime, s.saved), (0, 120.0));
    assert!(s.wages < o.wages);
    // a Sunday's pieces: supplements
    let sun = pcost::week(&c, &e, k, &[("2024-03-10".to_string(), vec![(8 * 60, 14 * 60)])], 0.0, 0.0);
    assert_eq!(sun.supplements, (hour * 360.0 * pay::SUNDAY / 60.0).round() as Cents);
    assert_eq!(sun.wages, w.wages + sun.supplements);
}

// --- round C: ill and on holiday one record, a swap of both drivers ----------------------------

#[test]
fn ill_set_in_the_turnus_tab_is_the_staff_cards_illness_and_the_other_way_round() {
    use super::super::pro_absence as pa;
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1]);
    let a = ids[0];
    // today and the next days: his illness of now, one spell
    pa::set_ill(&mut c, a, "2024-03-04").unwrap();
    pa::set_ill(&mut c, a, "2024-03-05").unwrap();
    pa::set_ill(&mut c, a, "2024-03-06").unwrap();
    let e = c.employee(a).unwrap();
    assert_eq!(e.sick_until.as_deref(), Some("2024-03-06"));
    assert!(e.sick_planned.is_empty() && c.planning.rotas.exceptions.is_empty());
    assert_eq!(pa::spells(e, &c.date), vec![("sick", "2024-03-04".to_string(), "2024-03-06".to_string())]);
    // the rota's day: off, ill - and his duty is missing, as the illness
    let d = pt::decided(&c, a, "2024-03-05").unwrap();
    assert_eq!((d.cell, d.note.as_deref()), (Cell::Off, Some("sick")));
    assert_eq!(driver_of(&c, &day(&c, "2024-03-05"), 1001), None);
    // a day further ahead: known ahead, his on the day it comes
    pa::set_ill(&mut c, a, "2024-03-11").unwrap();
    assert_eq!(c.employee(a).unwrap().sick_planned, vec!["2024-03-11".to_string()]);
    // (the week after he has the afternoons: a row down every week)
    assert_eq!(driver_of(&c, &day(&c, "2024-03-11"), 1002), None);
    assert!(c.employee(a).unwrap().ill_on("2024-03-11") && !c.employee(a).unwrap().ill_on("2024-03-08"));
    // back as in the rota: the 5th out - ill the 4th, the 6th still known ahead
    assert!(pa::clear_ill(&mut c, a, "2024-03-05"));
    let e = c.employee(a).unwrap();
    assert_eq!(e.sick_until.as_deref(), Some("2024-03-04"));
    assert_eq!(e.sick_planned, vec!["2024-03-06".to_string(), "2024-03-11".to_string()]);
    assert_eq!(driver_of(&c, &day(&c, "2024-03-05"), 1001), Some(Who::Staff(a)));
    // the night: the 6th has come - his illness of the day
    c.date = "2024-03-06".into();
    pt::tidy(&mut c);
    let e = c.employee(a).unwrap();
    assert_eq!(e.sick_until.as_deref(), Some("2024-03-06"));
    assert_eq!(e.sick_planned, vec!["2024-03-11".to_string()]);
    // the other way round: ill on the staff card - the rota's day says so
    c.staff.iter_mut().find(|e| e.id == ids[1]).unwrap().sick_until = Some("2024-03-07".into());
    assert_eq!(pt::decided(&c, ids[1], "2024-03-07").unwrap().note.as_deref(), Some("sick"));
    // a day gone cannot be set
    assert_eq!(pa::set_ill(&mut c, a, "2024-03-05"), Err(pa::GONE));
}

#[test]
fn a_holiday_set_in_the_turnus_tab_is_an_approved_holiday_of_his_and_counts_against_his_days() {
    use super::super::holidays::{self, State};
    use super::super::pro_absence as pa;
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1]);
    let a = ids[0];
    let left = c.employee(a).unwrap().holiday_left;
    for d in ["2024-03-11", "2024-03-12", "2024-03-13"] {
        pa::set_holiday(&mut c, a, d).unwrap();
    }
    let e = c.employee(a).unwrap();
    let mine: Vec<_> = e.leave.requests.iter().filter(|r| r.state == State::Approved).collect();
    assert_eq!(mine.len(), 1, "one holiday: {:?}", e.leave.requests);
    assert_eq!((mine[0].from.as_str(), mine[0].until.as_str()), ("2024-03-11", "2024-03-13"));
    assert_eq!(holidays::free_days(e, None), left - mine[0].working_days());
    assert_eq!(pt::decided(&c, a, "2024-03-12").unwrap().note.as_deref(), Some("holiday"));
    assert_eq!(pa::spells(e, &c.date), vec![("holiday", "2024-03-11".to_string(), "2024-03-13".to_string())]);
    // the middle day back: two holidays, its day not taken
    assert!(pa::clear_holiday(&mut c, a, "2024-03-12"));
    let e = c.employee(a).unwrap();
    assert_eq!(e.on_holiday("2024-03-12"), None);
    assert!(e.on_holiday("2024-03-11").is_some() && e.on_holiday("2024-03-13").is_some());
    // begun (the night before it): the days taken from his year's; one back gives it back
    holidays::night(&mut c, "2024-03-10");
    let taken = c.employee(a).unwrap().holiday_left;
    assert!(taken < left, "{taken} {left}");
    assert!(pa::clear_holiday(&mut c, a, "2024-03-11"));
    assert_eq!(c.employee(a).unwrap().holiday_left, taken + 1);
    // none left: refused
    c.staff.iter_mut().find(|e| e.id == ids[2]).unwrap().holiday_left = 0;
    assert_eq!(pa::set_holiday(&mut c, ids[2], "2024-03-20"), Err(pa::NO_DAYS));
}

#[test]
fn an_older_files_turnus_ill_and_holiday_days_move_into_the_staffs_records() {
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1]);
    let push = |c: &mut Company, id: u32, date: &str, note: &str| c.planning.rotas.exceptions.push(pt::Exception { driver: id, date: date.into(), cell: Cell::Off, note: note.into() });
    push(&mut c, ids[0], "2024-03-05", "sick");
    push(&mut c, ids[1], "2024-03-12", "holiday");
    push(&mut c, ids[1], "2024-03-13", "");
    super::super::pro_absence::migrate(&mut c);
    assert_eq!(c.employee(ids[0]).unwrap().sick_planned, vec!["2024-03-05".to_string()]);
    assert!(c.employee(ids[1]).unwrap().on_holiday("2024-03-12").is_some());
    assert_eq!(c.planning.rotas.exceptions.len(), 1, "the day off stays: {:?}", c.planning.rotas.exceptions);
    assert_eq!(pt::decided(&c, ids[0], "2024-03-05").unwrap().note.as_deref(), Some("sick"));
}

#[test]
fn a_swap_in_the_turnus_tab_changes_both_drivers_at_once_and_is_undone_before_its_day() {
    use super::super::pro_swap as ps;
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1]);
    let (a, b) = (ids[0], ids[1]);
    let w = week(&c, "2024-03-03");
    let date = "2024-03-06";
    let before = c.inbox.messages.len() + c.messages.list.len();
    // a takes b's afternoon: b takes a's morning - both days changed in one act
    let made = ps::swap_rota(&mut c, a, date, 1002, &w).unwrap();
    let s = made.swap.clone().unwrap();
    assert_eq!((s.a, s.b, s.a_gets, s.b_gets), (a, b, Some(1002), Some(1001)));
    assert_eq!(pt::decided(&c, a, date).unwrap().cell, Cell::Duty { n: 1002 });
    assert_eq!(pt::decided(&c, b, date).unwrap().cell, Cell::Duty { n: 1001 });
    let p = day(&c, date);
    assert_eq!(driver_of(&c, &p, 1001), Some(Who::Staff(b)));
    assert_eq!(driver_of(&c, &p, 1002), Some(Who::Staff(a)));
    // both told
    assert!(c.inbox.messages.len() + c.messages.list.len() >= before + 2);
    assert_eq!(ps::of_driver(&c, b, date).map(|x| x.1), Some(a));
    // the rest rules for both: an afternoon after a morning the day before is fine, a morning
    // after the afternoon is short of 11 hours only if it were the next day
    assert!(made.a_breaks.is_empty() && made.b_breaks.is_empty(), "{made:?}");
    // undone: both days as the rota has them, both told
    ps::undo(&mut c, s.no).unwrap();
    assert_eq!(pt::decided(&c, a, date).unwrap().cell, Cell::Duty { n: 1001 });
    assert_eq!(pt::decided(&c, b, date).unwrap().cell, Cell::Duty { n: 1002 });
    assert!(c.planning.ops.swaps.is_empty() && c.planning.rotas.exceptions.is_empty());
    // a duty on a's day off (Sunday): b has the day off instead
    let made = ps::swap_rota(&mut c, a, "2024-03-07", 1002, &w).unwrap();
    assert_eq!(made.swap.as_ref().unwrap().b_gets, Some(1001));
    // the day begun: no longer undone
    let s = made.swap.unwrap();
    c.date = "2024-03-07".into();
    c.clock.minute = 23 * 60;
    assert_eq!(ps::undo(&mut c, s.no).map(|_| ()), Err(ps::BEGUN));
    // the day gone: forgotten
    c.date = "2024-03-08".into();
    pt::tidy(&mut c);
    assert!(c.planning.ops.swaps.is_empty());
}

#[test]
fn a_swap_of_the_daily_operations_is_kept_told_and_undone_with_both_duties() {
    use super::super::pro_swap as ps;
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1]);
    let date = "2024-03-05";
    pt::set_exception(&mut c, ids[0], date, Some((Cell::Off, String::new())));
    // b takes the open morning, his afternoon goes to y: one act, both kept as a swap
    let y = ids[3];
    give(&mut c, date, 1001, ids[1], Some((y, 1002))).unwrap();
    let s = ps::after_give(&mut c, date, 1001, ids[1], y, 1002, 300);
    assert_eq!(ps::of_day(&c, date).count(), 1);
    pt::tidy(&mut c);
    assert_eq!(ps::of_day(&c, date).count(), 1, "it holds");
    ps::undo(&mut c, s.no).unwrap();
    let p = day(&c, date);
    assert_eq!(driver_of(&c, &p, 1001), None);
    assert_eq!(driver_of(&c, &p, 1002), Some(Who::Staff(ids[1])));
    assert!(c.planning.ops.calls.is_empty());
}


// --- round D: public holidays, early and late people, tired drivers ------------------------------

#[test]
fn the_public_holidays_have_their_names_and_are_paid_as_a_sunday() {
    use super::super::pro_holiday as ph;
    // (Denmark's: Maundy Thursday and Great Prayer Day until 2023, not the first of May)
    assert_eq!(ph::name_in(true, "2024-03-28"), Some(ph::MAUNDY_THURSDAY));
    assert_eq!(ph::name_in(true, "2024-03-31"), Some(ph::EASTER_SUNDAY));
    assert_eq!(ph::name_in(true, "2024-05-19"), Some(ph::WHIT_SUNDAY));
    assert_eq!(ph::name_in(true, "2023-05-05"), Some(ph::PRAYER_DAY));
    assert_eq!(ph::name_in(true, "2024-04-26"), None);
    assert_eq!(ph::name_in(true, "2024-05-01"), None);
    assert_eq!(ph::name_in(true, "2024-12-24"), None);
    assert_eq!(ph::name_in(true, "2024-12-26"), Some(ph::SECOND_CHRISTMAS));
    // (the common German and Dutch ones: the first of May, no Maundy Thursday)
    assert_eq!(ph::name_in(false, "2024-05-01"), Some(ph::LABOUR_DAY));
    assert_eq!(ph::name_in(false, "2024-03-28"), None);
    assert_eq!(ph::name_in(false, "2024-04-01"), Some(ph::EASTER_MONDAY));
    // (a holiday's hours as a Sunday's, the highest counting - not on top of the evening's)
    assert_eq!(pay::supplement_rate_on(0, [true, false], 10 * 60), pay::SUNDAY);
    assert_eq!(pay::supplement_rate_on(0, [true, false], 19 * 60), pay::SUNDAY);
    assert_eq!(pay::supplement_rate_on(0, [false, false], 10 * 60), 0.0);
    // (past midnight into a holiday: the next day's)
    assert_eq!(pay::supplement_rate_on(2, [false, true], 24 * 60 + 60), pay::SUNDAY);
    assert_eq!(pay::supplement_rate_on(2, [true, false], 24 * 60 + 60), pay::NIGHT);
    assert!((pay::supplement_minutes_on(0, [true, false], &[(8 * 60, 14 * 60)]) - 360.0 * pay::SUNDAY).abs() < 1e-9);
    // (a company of Pro: Easter Monday's duty pays the supplement, and the week tells it apart)
    use super::super::pro_cost as pcost;
    let c = company();
    let e = c.staff[0].clone();
    assert!(ph::is(&c, "2024-04-01") && !ph::is(&c, "2024-04-02"));
    let k = contract_minutes(&c, &e).unwrap();
    let w = pcost::week(&c, &e, k, &[("2024-04-01".to_string(), vec![(8 * 60, 14 * 60)]), ("2024-04-02".to_string(), vec![(8 * 60, 14 * 60)])], 0.0, 0.0);
    let hour = pay::hour_wage(&e);
    assert_eq!(w.holidays, (hour * 360.0 * pay::SUNDAY / 60.0).round() as Cents);
    assert_eq!(w.supplements, w.holidays);
    assert!(pcost::duty_supplement(&c, &e, "2024-04-01", &[(8 * 60, 14 * 60)]) > 0);
    assert_eq!(pcost::duty_supplement(&c, &e, "2024-04-02", &[(8 * 60, 14 * 60)]), 0);
    // (asked on a holiday: a yes less likely)
    assert_eq!(ph::bias(&c, "2024-04-01"), ph::YES);
    assert_eq!(ph::bias(&c, "2024-04-02"), 1.0);
}

#[test]
fn working_many_holidays_makes_a_driver_less_content() {
    use super::super::pro_holiday as ph;
    let mut c = company();
    let id = c.staff[0].id;
    let mut seen = Vec::new();
    for d in ["2024-03-29", "2024-04-01", "2024-05-01", "2024-05-09", "2024-05-20"] {
        c.staff[0].satisfaction = 70.0;
        ph::after_day(&mut c, d, &[(id, 6 * 60, 14 * 60)]);
        seen.push(c.staff[0].satisfaction);
    }
    // (the first three free of cost, from the fourth two points each)
    assert_eq!(seen, vec![70.0, 70.0, 70.0, 70.0 - ph::COST, 70.0 - ph::COST]);
    assert_eq!(ph::worked(&c.staff[0], "2024-05-20"), 5);
    // (a normal day is not counted; a year later they are let go)
    ph::after_day(&mut c, "2024-05-21", &[(id, 6 * 60, 14 * 60)]);
    assert_eq!(c.staff[0].holidays_worked.len(), 5);
    ph::after_day(&mut c, "2025-06-01", &[]);
    assert!(c.staff[0].holidays_worked.is_empty());
}

#[test]
fn early_and_late_people_like_their_duties() {
    use super::super::pro_pref::{self as pp, ShiftPref};
    assert_eq!((pp::kind(5 * 60), pp::kind(7 * 60 + 59), pp::kind(8 * 60), pp::kind(12 * 60 + 59), pp::kind(13 * 60), pp::kind(22 * 60)), (ShiftPref::Early, ShiftPref::Early, ShiftPref::Neither, ShiftPref::Neither, ShiftPref::Late, ShiftPref::Late));
    let mut c = company();
    // (each driver one, the same every time, kept)
    let drawn: Vec<ShiftPref> = c.staff.iter().map(pp::seed).collect();
    assert_eq!(drawn, c.staff.iter().map(pp::seed).collect::<Vec<_>>());
    assert!(c.staff.iter().all(|e| e.shift_pref.is_some()), "hired with one");
    c.staff[0].shift_pref = None;
    pp::seed_all(&mut c);
    assert_eq!(c.staff[0].shift_pref, Some(drawn[0]));
    // (a spread over many: all three)
    let mut e = c.staff[0].clone();
    let kinds: Vec<ShiftPref> = (0..200).map(|k| { e.id = 1000 + k; pp::seed(&e) }).collect();
    for want in [ShiftPref::Early, ShiftPref::Late, ShiftPref::Neither] {
        let n = kinds.iter().filter(|x| **x == want).count();
        assert!((40..=100).contains(&n), "{want:?} {n}");
    }
    // (liked: a yes more likely and a little more content; not: less of both)
    c.staff[0].shift_pref = Some(ShiftPref::Early);
    let e = c.staff[0].clone();
    assert_eq!((pp::fit(&e, 5 * 60), pp::fit(&e, 10 * 60), pp::fit(&e, 15 * 60)), (1, 0, -1));
    assert_eq!((pp::bias(&c, &e, 5 * 60), pp::bias(&c, &e, 15 * 60)), (pp::LIKED, pp::DISLIKED));
    let id = e.id;
    c.staff[0].satisfaction = 60.0;
    pp::after_day(&mut c, &[(id, 5 * 60, 13 * 60)]);
    assert!((c.staff[0].satisfaction - (60.0 + pp::GLAD)).abs() < 1e-9);
    pp::after_day(&mut c, &[(id, 15 * 60, 23 * 60)]);
    assert!((c.staff[0].satisfaction - (60.0 + pp::GLAD - pp::SORRY)).abs() < 1e-9);
    // (an older file: none, drawn when opened)
    let old: Employee = serde_json::from_value({
        let mut v = serde_json::to_value(&c.staff[1]).unwrap();
        v.as_object_mut().unwrap().remove("shift_pref");
        v
    })
    .unwrap();
    assert_eq!((old.shift_pref, old.worked.len(), old.holidays_worked.len()), (None, 0, 0));
    assert_eq!(pp::of(&old), pp::seed(&old));
}

#[test]
fn many_long_days_make_a_driver_tired_and_days_off_rest_him() {
    use super::super::pro_tired::{self as ptd, Level, Why};
    let day = Some((6 * 60, 14 * 60));
    let long = Some((5 * 60, 15 * 60));
    let level = |v: &[ptd::Day]| ptd::assess(v);
    assert_eq!(level(&[day; 5]).level, Level::Fresh);
    assert_eq!(level(&[day; 6]), ptd::Tired { level: Level::Tired, why: Why::Days(6) });
    assert_eq!(level(&[day; 8]).level, Level::Worn);
    assert_eq!(level(&[day, long, long, long, long]), ptd::Tired { level: Level::Tired, why: Why::LongInRow(4) });
    assert_eq!(level(&[long, long, day, long, long]), ptd::Tired { level: Level::Tired, why: Why::LongDays(4) });
    assert_eq!(level(&[long; 6]).level, Level::Worn);
    // (two short rests: a late duty, then an early one)
    let late = Some((14 * 60, 22 * 60));
    let early = Some((5 * 60, 13 * 60));
    assert_eq!(level(&[late, early, late, early]), ptd::Tired { level: Level::Tired, why: Why::ShortRests(2) });
    // (a day off: fresh that day; one alone after a hard streak a step less; two: rested)
    assert_eq!(level(&[long, long, long, long, long, long, None]).level, Level::Fresh);
    assert_eq!(level(&[long, long, long, long, long, long, None, day]), ptd::Tired { level: Level::Tired, why: Why::OneDayOff });
    assert_eq!(level(&[long, long, long, long, long, long, None, None, day]).level, Level::Fresh);
    // (tired: fewer yeses, more sick calls, slower)
    assert!(ptd::yes(Level::Worn) < ptd::yes(Level::Tired) && ptd::yes(Level::Tired) < 1.0);
    assert!(ptd::sick(Level::Worn) > ptd::sick(Level::Tired) && ptd::sick(Level::Tired) > 1.0);
    assert!(ptd::slow(Level::Tired) > 1.0);
    // (the night keeps the day as worked, two weeks)
    let mut c = company();
    let id = c.staff[0].id;
    for k in 0..16 {
        let d = dates::add("2024-03-04", k);
        ptd::after_day(&mut c, &d, &[(id, 5 * 60, 10 * 60), (id, 11 * 60, 15 * 60 + 30)]);
    }
    let e = &c.staff[0];
    assert_eq!(e.worked.len(), ptd::KEPT as usize);
    assert_eq!(ptd::worked_on(e, "2024-03-19"), Some((5 * 60, 15 * 60 + 30)));
    assert_eq!(ptd::worked_on(e, "2024-03-05"), None);
    assert_eq!(ptd::today(e, "2024-03-20", Some((5 * 60, 15 * 60))).level, Level::Worn);
    assert_eq!(ptd::driving(&c, e, "2024-03-20"), Level::Worn);
    assert_eq!(ptd::driving(&c, &c.staff[1], "2024-03-20"), Level::Fresh);
}

#[test]
fn a_tired_driver_or_a_duty_not_liked_gets_fewer_yeses() {
    use super::super::pro_pref::ShiftPref;
    use super::super::pro_tired::Level;
    let mut c = company();
    c.staff[0].shift_pref = Some(ShiftPref::Neither);
    let e = c.staff[0].clone();
    let yes = |c: &Company, e: &Employee, tired: Level, start: i32| (1..400u32).filter(|&n| matches!(ai_reply(c, e, "2024-03-06", n, start, start + 6 * 60, &Situation { tired, ..calm() }), Reply::Yes { .. })).count();
    let fresh = yes(&c, &e, Level::Fresh, 9 * 60);
    let worn = yes(&c, &e, Level::Worn, 9 * 60);
    assert!(worn + 40 < fresh, "{worn} {fresh}");
    let mut late = e.clone();
    late.shift_pref = Some(ShiftPref::Late);
    let liked = yes(&c, &late, Level::Fresh, 14 * 60);
    late.shift_pref = Some(ShiftPref::Early);
    let disliked = yes(&c, &late, Level::Fresh, 14 * 60);
    assert!(disliked + 30 < liked, "{disliked} {liked}");
}

// --- round E: the drivers' swap proposals, the seasons, the Monday report -------------------------

fn emp(c: &mut Company, id: u32) -> &mut super::super::model::Employee {
    c.staff.iter_mut().find(|e| e.id == id).unwrap()
}

/// A rota of three rows: `a` the mornings (1001) Monday to Friday, `b` the afternoons (1002),
/// `f` every day off - each keeps his row for the test's week.
fn rota3(c: &mut Company, a: u32, b: u32, f: u32) {
    let w = week(c, "2024-03-03");
    let id = pt::add(c, "Nord", false);
    for _ in 0..3 {
        pt::add_row(c, id).unwrap();
    }
    for dd in 1..=5 {
        pt::set_cell(c, id, 0, dd, Cell::Duty { n: 1001 }, &w).unwrap();
        pt::set_cell(c, id, 1, dd, Cell::Duty { n: 1002 }, &w).unwrap();
    }
    pt::set_driver(c, id, 0, Some(a)).unwrap();
    pt::set_driver(c, id, 1, Some(b)).unwrap();
    pt::set_driver(c, id, 2, Some(f)).unwrap();
}

/// The company's proposal of the day, drawn again with another company id until `ok` takes one.
fn proposal(c: &mut Company, w: &pt::Week, ok: &dyn Fn(&super::super::pro_trade::Trade) -> bool) -> super::super::pro_trade::Trade {
    use super::super::pro_trade as tr;
    let id = c.id.clone();
    for n in 0..400 {
        c.id = format!("{id}-t{n}");
        c.planning.ops.traded.clear();
        c.planning.ops.trades.clear();
        if let Some(k) = tr::daily(c, w) {
            let t = c.planning.ops.trades.iter().find(|t| t.id == k).unwrap().clone();
            if ok(&t) {
                return t;
            }
        }
    }
    panic!("no proposal");
}

#[test]
fn a_tired_driver_proposes_to_give_a_duty_to_a_fresh_colleague_and_approved_it_is_a_swap() {
    use super::super::pro_swap as ps;
    use super::super::pro_tired::Worked;
    use super::super::pro_trade::{self as tr, Offer, TradeState, Why};
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    let (a, b, f) = (ids[0], ids[1], ids[2]);
    rota3(&mut c, a, b, f);
    for e in c.staff.iter_mut() {
        e.shift_pref = Some(super::super::pro_pref::ShiftPref::Neither);
    }
    // a worked the six days before: tired from Tuesday on
    emp(&mut c, a).worked = (1..=6).map(|k| Worked { date: dates::add("2024-03-04", -k), from: 360, to: 660 }).collect();
    let w = week(&c, "2024-03-03");
    let t = proposal(&mut c, &w, &|t| t.why == Why::Tired);
    // a gives his morning to f, who is off that day and fresh
    assert_eq!((t.a, t.b), (a, f));
    let Offer::Give { a_n } = t.offer else { panic!("{t:?}") };
    assert_eq!(a_n, 1001);
    assert!(t.date > c.date && t.state == TradeState::Open);
    // his message to the control room
    assert!(c.messages.list.iter().any(|m| m.text == tr::SAYS_GIVE), "{:?}", c.messages.list.iter().map(|m| &m.text).collect::<Vec<_>>());
    assert_eq!(tr::open(&c).len(), 1);
    // approved: the swap of round C - f drives 1001, a has the day off, ⇄ for both
    let before = (c.employee(a).unwrap().satisfaction, c.employee(f).unwrap().satisfaction);
    let made = tr::approve(&mut c, t.id, &w).unwrap();
    assert!(made.swap.is_some());
    assert_eq!(pt::decided(&c, f, &t.date).unwrap().cell, Cell::Duty { n: 1001 });
    assert_eq!(pt::decided(&c, a, &t.date).unwrap().cell, Cell::Off);
    assert_eq!(ps::of_driver(&c, a, &t.date).map(|x| x.1), Some(f));
    assert_eq!(c.employee(a).unwrap().satisfaction, before.0 + tr::APPROVED);
    assert_eq!(c.employee(f).unwrap().satisfaction, before.1 + tr::APPROVED);
    assert!(tr::open(&c).is_empty());
    assert_eq!(tr::approve(&mut c, t.id, &w).map(|_| ()), Err(tr::NO_SUCH_TRADE));
    // "Fortryd bytte" of round C undoes it
    let no = c.planning.ops.trades.iter().find(|x| x.id == t.id).unwrap().swap.unwrap();
    ps::undo(&mut c, no).unwrap();
    assert_eq!(pt::decided(&c, a, &t.date).unwrap().cell, Cell::Duty { n: 1001 });
}

#[test]
fn early_and_late_people_propose_to_swap_and_refused_or_left_they_are_less_content() {
    use super::super::pro_pref::ShiftPref;
    use super::super::pro_trade::{self as tr, Offer, TradeState, Why};
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    let (a, b) = (ids[0], ids[1]);
    rota(&mut c, a, b);
    // a has the mornings and likes the late ones, b the other way round
    emp(&mut c, a).shift_pref = Some(ShiftPref::Late);
    emp(&mut c, b).shift_pref = Some(ShiftPref::Early);
    let w = week(&c, "2024-03-03");
    let t = proposal(&mut c, &w, &|t| t.why != Why::Errand);
    assert_eq!(t.offer, if t.a == a { Offer::Swap { a_n: 1001, b_n: 1002 } } else { Offer::Swap { a_n: 1002, b_n: 1001 } });
    assert!(t.both, "{t:?}");
    assert_eq!(t.why, if t.a == a { Why::Late } else { Why::Early });
    // legal for both under the rest rules (a reduced rest of 9 hours is allowed three times a week)
    let (x, y) = tr::breaches(&c, &t, &w);
    assert!(x.is_empty() && y.is_empty(), "{x:?} {y:?}");
    // refused: both less content
    let before = c.employee(a).unwrap().satisfaction;
    tr::refuse(&mut c, t.id).unwrap();
    assert_eq!(c.employee(a).unwrap().satisfaction, before - tr::REFUSED);
    assert_eq!(tr::refuse(&mut c, t.id), Err(tr::NO_SUCH_TRADE));
    assert_eq!(tr::decided_in(&c, &c.date.clone(), &dates::add(&c.date, 6)), (0, 1));
    // one left open three days lapses on the night's tidy
    let t = proposal(&mut c, &w, &|_| true);
    let before = c.employee(t.a).unwrap().satisfaction;
    c.date = dates::add(&c.date, tr::LAPSE_DAYS);
    tr::tidy(&mut c);
    let x = c.planning.ops.trades.iter().find(|x| x.id == t.id).unwrap();
    assert_eq!(x.state, TradeState::Lapsed);
    assert_eq!(c.employee(t.a).unwrap().satisfaction, before - tr::LAPSED);
}

#[test]
fn a_proposal_no_longer_as_made_lapses_when_approved() {
    use super::super::pro_trade::{self as tr, TradeState};
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    rota(&mut c, ids[0], ids[1]);
    let w = week(&c, "2024-03-03");
    let t = proposal(&mut c, &w, &|_| true);
    // the first driver given the day off in the Turnus tab since
    pt::put_exception(&mut c, t.a, &t.date, Some((Cell::Off, String::new())));
    assert!(tr::open(&c).is_empty());
    assert_eq!(tr::approve(&mut c, t.id, &w).map(|_| ()), Err(tr::CHANGED));
    assert!(c.planning.ops.swaps.is_empty());
    // the night's tidy: lapsed
    tr::tidy(&mut c);
    assert_eq!(c.planning.ops.trades.iter().find(|x| x.id == t.id).unwrap().state, TradeState::Lapsed);
}

#[test]
fn winter_brings_more_sick_calls_and_the_summer_more_holiday_requests_and_dearer_refusals() {
    use super::super::holidays::{self as hd, By, Reason, Request};
    use super::super::pro_season::{self as se, Season};
    let mut c = company();
    assert_eq!(se::of("2024-01-15"), Season::Winter);
    assert_eq!(se::of("2024-12-01"), Season::Winter);
    assert_eq!(se::of("2024-03-01"), Season::Spring);
    assert_eq!(se::of("2024-06-19"), Season::Spring);
    assert_eq!(se::of("2024-06-20"), Season::Summer);
    assert_eq!(se::of("2024-08-15"), Season::Summer);
    assert_eq!(se::of("2024-08-16"), Season::Other);
    assert_eq!(se::sick(&c, "2024-02-10"), se::WINTER_SICK);
    assert_eq!(se::sick(&c, "2024-07-10"), 1.0);
    assert_eq!(se::asks(&c, "2024-07-10"), se::SUMMER_ASKS);
    assert_eq!(se::hint(&c, "2024-07-10").map(|h| h.0), Some("summer"));
    assert_eq!(se::hint(&c, "2024-10-10"), None);
    // a summer holiday refused costs half as much more; another the usual
    let id = c.staff[0].id;
    let ask = |c: &mut Company, from: &str, until: &str, reason: Reason| {
        c.staff_rules.holidays.last += 1;
        let no = c.staff_rules.holidays.last;
        let r = Request { no, from: from.into(), until: until.into(), reason, asked: c.date.clone(), due: dates::add(&c.date, 7), ..Default::default() };
        emp(c, id).leave.requests.push(r);
        no
    };
    let no = ask(&mut c, "2024-07-08", "2024-07-12", Reason::Summer);
    emp(&mut c, id).satisfaction = 80.0;
    hd::answer(&mut c, no, false, By::Player).unwrap();
    let summer = 80.0 - c.employee(id).unwrap().satisfaction;
    assert_eq!(summer, hd::refusal_cost(1) * se::SUMMER_REFUSAL);
    let no = ask(&mut c, "2024-10-08", "2024-10-09", Reason::Break);
    emp(&mut c, id).satisfaction = 80.0;
    hd::answer(&mut c, no, false, By::Player).unwrap();
    assert_eq!(80.0 - c.employee(id).unwrap().satisfaction, hd::refusal_cost(2));
    // without RosterFlow Pro the year is as it was
    let mut o = c.clone();
    autopilot::set_planner(&mut o, Planner::RosterFlow);
    assert_eq!(se::sick(&o, "2024-02-10"), 1.0);
    assert_eq!(se::hint(&o, "2024-07-10"), None);
}

#[test]
fn the_monday_report_counts_the_week_before_and_is_made_once_on_monday_morning() {
    use super::super::pay::Paid;
    use super::super::pro_report as rp;
    use super::super::pro_tired::Worked;
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    // the nights of the week from Sunday 3 March: paid, one ill two days, a duty uncovered
    emp(&mut c, ids[1]).sick_until = Some("2024-03-06".into());
    for k in 0..7 {
        let date = dates::add("2024-03-03", k);
        let paid = Paid { cost: 1_000_00, overtime: 1.5, overtime_cost: 120_00, supplements: 80_00, ..Default::default() };
        c.date = date.clone();
        rp::after_day(&mut c, &date, &paid, if k == 2 { 1 } else { 0 });
    }
    // (two grumbles on the Saturday - `pro_rest::grumbles`)
    rp::complained(&mut c, 2);
    // a driver worn out by the Saturday, another the least content
    emp(&mut c, ids[2]).worked = (0..8).map(|k| Worked { date: dates::add("2024-03-09", -k), from: 300, to: 900 }).collect();
    emp(&mut c, ids[3]).satisfaction = 31.4;
    // Sunday: nothing; Monday before 06:00: nothing
    c.date = "2024-03-10".into();
    assert_eq!(rp::monday(&mut c, 9 * 60), None);
    c.date = "2024-03-11".into();
    assert_eq!(rp::monday(&mut c, 5 * 60), None);
    let n = rp::monday(&mut c, 6 * 60).unwrap();
    assert_eq!(n, pt::week_number("2024-03-04"));
    let r = c.planning.ops.reports.last().unwrap().clone();
    assert_eq!(r.sunday, "2024-03-03");
    assert_eq!((r.tally.pay, r.tally.overtime_cost, r.tally.supplements, r.tally.days), (7_000_00, 840_00, 560_00, 7));
    assert!((r.tally.overtime_h - 10.5).abs() < 1e-9);
    // ill Sunday 3 (the sick_until from before) to Wednesday 6
    assert_eq!(r.tally.sick_days, 4);
    assert_eq!((r.tally.uncovered, r.tally.complaints), (1, 2));
    assert_eq!(r.tired.as_ref().map(|t| (t.id, t.key.as_str())), Some((ids[2], "worn")));
    assert_eq!(r.unhappy.as_ref().map(|u| (u.id, u.value)), Some((ids[3], 31.0)));
    // told: the inbox has it, a notice waits until seen; made once
    assert!(c.inbox.messages.iter().any(|m| m.title == rp::READY));
    assert_eq!(rp::unseen(&c).map(|r| r.sunday.as_str()), Some("2024-03-03"));
    rp::seen(&mut c);
    assert!(rp::unseen(&c).is_none());
    assert_eq!(rp::monday(&mut c, 7 * 60), None);
    // a week with nothing counted has no report
    c.date = "2024-03-25".into();
    assert_eq!(rp::monday(&mut c, 6 * 60), None);
}

/// (Round G) The mark on a day's missing cell: grey while questions wait, green for a yes not
/// seen, red when every answer is a no; the planner opening the duty's box sees them.
#[test]
fn the_mark_on_the_missing_cell_grey_green_red_and_seen() {
    let mut c = company();
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    let date = dates::add(&c.date, 1);
    let d = date.as_str();
    assert_eq!(mark_of(&c, d, crate::company::clock::now(&c), 10).key(), None);
    let no = || Some(Reply::No { why: NoWhy::Plans, say: 0 });
    ask(&mut c, d, 1001, 350, 660, vec![(Asked::Staff { id: ids[1] }, no()), (Asked::Staff { id: ids[2] }, no())], 10).unwrap();
    let now = crate::company::clock::now(&c);
    let m = mark_of(&c, d, now, 10);
    assert_eq!((m.key(), m.waiting), (Some("waiting"), 2));
    // the answers come: both no - red
    let latest = c.planning.ops.asks.iter().filter_map(|a| a.due).max().unwrap();
    let m = mark_of(&c, d, latest, 10);
    assert_eq!((m.key(), m.duties.clone()), (Some("no"), vec![1001]));
    // a yes for another duty that day: green wins
    ask(&mut c, d, 1002, 700, 900, vec![(Asked::Staff { id: ids[3] }, Some(Reply::Yes { say: 0 }))], 10).unwrap();
    let later = c.planning.ops.asks.iter().filter_map(|a| a.due).max().unwrap();
    let m = mark_of(&c, d, later, 10);
    assert_eq!((m.key(), m.yes), (Some("yes"), 1));
    assert_eq!(m.duties, vec![1001, 1002]);
    // the planner opens 1002's box: its yes seen - the noes of 1001 still red? no: a yes is open
    assert!(mark_seen(&mut c, d, 1002, later, 10));
    assert!(!mark_seen(&mut c, d, 1002, later, 10), "seen already");
    let m = mark_of(&c, d, later, 10);
    assert_eq!((m.key(), m.duties.clone()), (None, vec![1001]), "a yes still open: not all no; 1001's noes not seen");
    assert!(mark_seen(&mut c, d, 1001, later, 10));
    assert_eq!(mark_of(&c, d, later, 10).key(), None);
    // kept through a save
    let back: Ops = serde_json::from_str(&serde_json::to_string(&c.planning.ops).unwrap()).unwrap();
    assert!(back.asks.iter().filter(|a| a.n == 1001).all(|a| a.seen));
}
