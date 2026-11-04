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
    let free = Situation { fit: Ok(()), in_frame: false, week: 0.0, contract: Some(39.0 * 60.0), own: None, swap_with: None, can_from: None, can_until: None };
    let r1 = ai_reply(&c, &e, "2024-03-05", 1001, 350, 660, &free);
    assert_eq!(ai_reply(&c, &e, "2024-03-05", 1001, 350, 660, &free), r1, "the same answer asked again");
    // within his frame: yes, his frame is the company's
    assert_eq!(ai_reply(&c, &e, "2024-03-05", 1001, 350, 660, &Situation { in_frame: true, ..free.clone() }), Reply::Frame);
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
    c.staff[2].employment = Employment::Hourly;
    let h = c.staff[2].clone();
    let (from, until, _) = own_hours(&h).unwrap();
    let r = ai_reply(&c, &h, "2024-03-05", 1001, from - 60, from + 120, &free);
    assert!(matches!(r, Reply::Early { from: f, .. } if f == from), "{r:?}");
    if until < 1440 {
        assert!(matches!(ai_reply(&c, &h, "2024-03-05", 1001, until - 60, until + 30, &free), Reply::Late { .. }));
    }
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
/// question before 12:00 the day before (at 12:00 exactly he must be asked); the hourly paid are
/// asked several at once, a permanent driver one at a time; the duty is given only after a yes
/// (or so put on), and the others asked are told it is taken.
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
    assert_eq!(rule_of(&c, &r, &tomorrow, plan::TIER_RESERVE), Rule::One);
    assert_eq!(rule_of(&c, &c.staff[3].clone(), &tomorrow, plan::TIER_HOURLY), Rule::Many);
    assert_eq!(rule_of(&c, &c.staff[1].clone(), &tomorrow, plan::TIER_COLLEAGUE), Rule::One);
    // a permanent driver one at a time; two at once refused too
    let yes = || Some(Reply::Yes { say: 0 });
    let date = tomorrow.as_str();
    ask(&mut c, date, 1001, 350, 660, vec![(Asked::Staff { id: ids[1] }, yes())], 10).unwrap();
    assert_eq!(ask(&mut c, date, 1001, 350, 660, vec![(Asked::Staff { id: ids[2] }, yes())], 10), Err(ONE_AT_A_TIME));
    // the hourly paid alongside, several at once
    ask(&mut c, date, 1001, 350, 660, vec![(Asked::Staff { id: ids[3] }, yes()), (Asked::Staff { id: ids[4] }, yes())], 10).unwrap();
    // given before his yes: refused; put on (a reserve's `set`): fine
    assert_eq!(give_asked(&mut c, date, 1001, None, ids[3], None, false, 10), Err(NEEDS_YES));
    // the first permanent driver's question taken back: the next may be asked
    let first = c.planning.ops.asks.iter().find(|a| a.of_staff() == Some(ids[1])).unwrap().id;
    assert!(withdraw_ask(&mut c, first));
    assert!(!withdraw_ask(&mut c, first), "closed already");
    ask(&mut c, date, 1001, 350, 660, vec![(Asked::Staff { id: ids[2] }, yes())], 10).unwrap();
    // the answers come: given to an hourly driver who said yes, the others told it is taken
    let latest = c.planning.ops.asks.iter().filter_map(|a| a.due).max().unwrap();
    c.clock.minute = (latest - crate::company::clock::moment(&c.date, 0)) as u32 + 1;
    let said_yes = c.planning.ops.asks.iter().find(|a| a.of_staff() == Some(ids[3])).unwrap().reply.clone().unwrap();
    if said_yes.yes() {
        let before = c.messages.list.len();
        give_asked(&mut c, date, 1001, None, ids[3], None, false, 10).unwrap();
        assert_eq!(c.messages.list.len(), before + 2, "ids[4] and ids[2] told");
        assert!(c.messages.list.iter().rev().take(2).all(|m| m.text == super::super::messages::DUTY_TAKEN));
        assert_eq!(asks_of(&c, date, 1001).count(), 0);
    }
    // the reserve put on without a question
    give_asked(&mut c, date, 1002, None, ids[0], None, true, 10).unwrap();
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
