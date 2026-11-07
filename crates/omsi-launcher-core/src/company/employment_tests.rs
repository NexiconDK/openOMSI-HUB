//! (NexiconDK, 4 November 2026) RosterFlow Pro's plan, step 1 "Ansættelse og rammer": the four
//! kinds of a driver's employment and how each is paid over a week, the Danish 37-hour week, the
//! order an open duty is filled in, a reserve's frame, the message a reserve gets of a duty given
//! from noon the day before, the bus company's week from Sunday, and a change of employment
//! given notice.

use crate::company::market::{self, MarketBus, Payment};
use crate::company::model::*;
use crate::company::network::tests::trip;
use crate::company::network::TourOfDay;
use crate::company::plan::{self, Cover, Who};
use crate::company::staff::{self, applicants, hire, Block};
use crate::company::{agency, dates, econ_uk, found, messages, pay, Founding};

fn tour(no: &str, from: i32, n: i32) -> TourOfDay {
    TourOfDay { line: "Linie5".into(), number: "5".into(), tour: no.into(), ai_group: String::new(), trips: (0..n).map(|k| trip(from + k * 60, from + 50 + k * 60, 12)).collect(), unplanned: false, cuts: None }
}

/// A company of line 5 (on Grundorf: a German company, the 39-hour week) founded on `date`, with
/// `buses` buses and `people` drivers, its mornings quiet today and tomorrow.
fn company(date: &str, buses: usize, people: usize) -> Company {
    let mut c = found(&Founding { name: "Rammer".into(), game_mode: GameMode::Normal, date: date.into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    c.planning.auto = true;
    let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
    c.cash += 5_000_000_00;
    for _ in 0..buses {
        market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
    }
    let id = c.id.clone();
    while c.staff.len() < people {
        for a in applicants(&c) {
            if c.staff.len() < people && a.licence == Licence::D {
                hire(&mut c, &a).unwrap();
            }
        }
        c.id.push('x');
        c.taken = Default::default();
    }
    for e in c.staff.iter_mut() {
        (e.sick_until, e.holiday_until) = (None, None);
        (e.satisfaction, e.reliability, e.experience) = (80.0, 1.0, 50.0);
    }
    for n in 0.. {
        c.id = format!("{id}{n}");
        if plan::disruptions(&c, &c.date).is_empty() && plan::disruptions(&c, &dates::add(&c.date, 1)).is_empty() {
            break;
        }
    }
    c
}

/// 05:00 to 13:00, Monday to Friday.
fn early() -> Frame {
    Frame::new(5 * 60, 13 * 60, 0b001_1111)
}

#[test]
fn a_reserves_frame_holds_its_hours_on_its_weekdays() {
    let f = early();
    assert_eq!((f.minutes(), f.day_count(), f.week_minutes()), (480, 5, 2400));
    assert!(f.on(0) && f.on(4) && !f.on(5) && !f.on(6));
    assert!(f.holds(6 * 60, 12 * 60) && f.holds(5 * 60, 13 * 60));
    assert!(!f.holds(4 * 60 + 50, 9 * 60) && !f.holds(12 * 60, 14 * 60));
    // made sound: past midnight, at least an hour, a weekday at least (none: Monday to Friday)
    let night = Frame::new(22 * 60, 6 * 60, 0b100_0000);
    assert_eq!((night.from, night.to, night.on(6)), (22 * 60, 30 * 60, true));
    assert_eq!(Frame::new(600, 610, 0).minutes(), 60);
    assert_eq!(Frame::new(600, 900, 0).days, 0b001_1111);
    // an older file's driver is full time; the kinds read and write themselves
    let e: Employment = serde_json::from_str(r#"{"kind":"reserve","from":300,"to":780,"days":31}"#).unwrap();
    assert_eq!(e, Employment::Reserve(early()));
    for x in [Employment::FullTime, Employment::PartTime { hours: 25.0 }, Employment::Reserve(early()), Employment::Hourly] {
        assert_eq!(serde_json::from_str::<Employment>(&serde_json::to_string(&x).unwrap()).unwrap(), x);
    }
    let mut v = serde_json::to_value(crate::company::plan::agency_driver()).unwrap();
    v.as_object_mut().unwrap().remove("employment");
    assert_eq!(serde_json::from_value::<Employee>(v).unwrap().employment, Employment::FullTime);
}

#[test]
fn each_kind_is_paid_its_own_week() {
    // 3 March 2024 is a Sunday: the bus company's week, Sunday to Saturday
    let mut c = company("2024-03-03", 0, 4);
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    c.staff[1].employment = Employment::PartTime { hours: 25.0 };
    c.staff[2].employment = Employment::Reserve(early());
    c.staff[3].employment = Employment::Hourly;
    let p = c.planning.flow.prefs.clone();
    let paid = |pieces: &[(i32, i32)]| pay::paid_minutes(&p, pieces) as f64;
    // the week's pieces of work, Sunday (0) to Saturday (6)
    let work = |d: i64, id: u32| -> Vec<(i32, i32)> {
        match (ids.iter().position(|x| *x == id).unwrap(), d) {
            // full time: 06:00-13:00 Monday to Friday
            (0, 1..=5) => vec![(360, 780)],
            // part time: 06:00-11:00 Monday to Friday
            (1, 1..=5) => vec![(360, 660)],
            // the reserve: inside the frame on Tuesday, partly after it on Wednesday
            (2, 2) => vec![(360, 600)],
            (2, 3) => vec![(720, 900)],
            // hourly paid: an hour and a half on Monday, six hours on Tuesday
            (3, 1) => vec![(420, 510)],
            (3, 2) => vec![(360, 720)],
            _ => Vec::new(),
        }
    };
    let mut guaranteed = Vec::new();
    for d in 0..7 {
        let date = dates::add(&c.date, d);
        let pieces: Vec<(u32, i32, i32)> = ids.iter().flat_map(|&id| work(d, id).into_iter().map(move |(a, b)| (id, a, b))).collect();
        if d == 6 {
            // (the week ends on Saturday night: before it is settled, each driver's week)
            let full = paid(&[(360, 780)]) * 5.0;
            let part = paid(&[(360, 660)]) * 5.0;
            assert_eq!((c.staff[0].week_paid, c.staff[0].week_due.round()), (full, (39.0 * 60.0 / 7.0 * 6.0_f64).round()));
            assert_eq!(c.staff[1].week_paid, part);
            assert!((c.staff[1].week_due - 25.0 * 60.0 / 7.0 * 6.0).abs() < 1e-6, "pro rata: {}", c.staff[1].week_due);
            // the reserve (round L): each frame day the hours driven, at least 7:24 - the frame
            // 05-13 is the window, not the hours -, and a contract week as a permanent driver's
            let least = crate::company::pro_turnus::NAMED_LEAST as f64;
            assert_eq!(c.staff[2].week_paid, least * 5.0);
            assert!((c.staff[2].week_due - 39.0 * 60.0 / 7.0 * 6.0).abs() < 1e-6, "{}", c.staff[2].week_due);
            // hourly paid: three hours called in on Monday (not the duty's floor), Tuesday's six
            assert_eq!(c.staff[3].week_paid, (180 + 360 + p.sign_on + p.sign_off) as f64);
            assert_eq!(c.staff[3].week_due, 0.0);
        }
        let r = pay::day(&mut c, &date, &pieces);
        guaranteed.push(r.guaranteed);
        if d == 5 {
            // (part time above 25 hours: overtime that week)
            assert!(c.staff[1].week_paid > 25.0 * 60.0);
        }
    }
    // the guarantee is paid on Saturday night alone: the full-time driver's hours not driven of
    // 39, and the reserve's (round L: their contract week - the frame's 40 hours at most a full
    // week, 39 -, five days of 7:24 short of it); not the part-timer's (more than 25), not the
    // hourly paid (none)
    assert!(guaranteed[..6].iter().all(|g| *g == 0.0), "{guaranteed:?}");
    let full_short = (39.0 * 60.0 - paid(&[(360, 780)]) * 5.0) / 60.0;
    let reserve_short = (39.0 * 60.0 - 5.0 * crate::company::pro_turnus::NAMED_LEAST as f64) / 60.0;
    assert!((guaranteed[6] - full_short - reserve_short).abs() < 1e-6, "{} {full_short} {reserve_short}", guaranteed[6]);
    assert!(c.staff.iter().all(|e| e.week_paid == 0.0 && e.week_due == 0.0));
    // overtime above each one's own week: the part-timer's above 25 hours
    let part = &c.staff[1];
    let (_, _, week) = pay::day_minutes(&p, part, 1, 39.0 * 60.0, false, &[]);
    assert_eq!(week, 25.0 * 60.0);
    // the hourly paid earn 10 % more an hour, the others their wage's
    assert_eq!(pay::rate_of(&c.staff[3]), 1.1);
    assert_eq!(pay::rate_of(&c.staff[0]), 1.0);
    // ill on a frame day: the allowances pay it, not the frame
    let reserve = c.staff[2].clone();
    assert_eq!(pay::day_minutes(&p, &reserve, 0, 2340.0, true, &[]).0, 0.0);
    assert_eq!(pay::day_minutes(&p, &reserve, 0, 2340.0, false, &[]).0, crate::company::pro_turnus::NAMED_LEAST as f64);
    assert_eq!(pay::day_minutes(&p, &reserve, 5, 2340.0, false, &[]).0, 0.0);
}

#[test]
fn the_hourly_paid_cost_ten_percent_more_for_the_same_hours() {
    let mut c = company("2024-03-03", 0, 2);
    let wage = c.staff[0].wage;
    c.staff[1].wage = wage;
    c.staff[1].employment = Employment::Hourly;
    let (a, b) = (c.staff[0].id, c.staff[1].id);
    let mut one = c.clone();
    let full = pay::day(&mut one, "2024-03-04", &[(a, 360, 840)]).cost;
    let mut two = c.clone();
    let hourly = pay::day(&mut two, "2024-03-04", &[(b, 360, 840)]).cost;
    assert!((hourly as f64 / full as f64 - 1.1).abs() < 0.001, "{full} {hourly}");
}

#[test]
fn a_danish_week_is_37_hours() {
    let mut c = company("2024-03-03", 0, 1);
    assert_eq!(econ_uk::contract_week_hours(&c), 39.0);
    let wage = c.staff[0].wage;
    assert_eq!(staff::contract_month(&c, &c.staff[0]), wage);
    // at the Danish price level (a server's settings): 37 hours, guaranteed a seventh a day
    c.online.price_level = Some("dk".into());
    assert_eq!(econ_uk::contract_week_hours(&c), 37.0);
    assert_eq!(staff::contract_hours(&c, &c.staff[0]), Some(37.0));
    assert_eq!(staff::contract_month(&c, &c.staff[0]), (wage as f64 * 37.0 / 39.0).round() as Cents);
    pay::day(&mut c, "2024-03-04", &[]);
    assert!((c.staff[0].week_due - 37.0 * 60.0 / 7.0).abs() < 1e-9);
    // a part-time week of 37 hours or more there is full time
    assert_eq!(staff::sound_employment(&c, Employment::PartTime { hours: 37.0 }), Employment::FullTime);
    assert_eq!(staff::sound_employment(&c, Employment::PartTime { hours: 3.0 }), Employment::PartTime { hours: staff::PART_TIME_MIN });
    // any other price level: 39
    c.online.price_level = Some("gb".into());
    assert_eq!(econ_uk::contract_week_hours(&c), 39.0);
}

#[test]
fn the_bus_companys_week_begins_on_sunday() {
    assert!(pay::week_ends("2024-03-09"), "a Saturday ends it");
    assert!(!pay::week_ends("2024-03-10") && !pay::week_ends("2024-03-04"));
    // a week without work: the guarantee on Saturday night, for the days of the week
    let mut c = company("2024-03-04", 0, 1);
    let mut got = Vec::new();
    for d in 0..7 {
        got.push(pay::day(&mut c, &dates::add("2024-03-04", d), &[]).guaranteed);
    }
    // Monday to Saturday (six days of the company's first week), then Sunday begins the next
    assert!(got[..5].iter().all(|g| *g == 0.0) && got[6] == 0.0, "{got:?}");
    assert!((got[5] - 39.0 * 6.0 / 7.0).abs() < 1e-9, "{got:?}");
    assert!((c.staff[0].week_due - 39.0 * 60.0 / 7.0).abs() < 1e-9);
}

#[test]
fn an_open_duty_is_filled_by_a_reserve_a_colleague_the_hourly_paid_then_the_agency() {
    let mut c = company("2024-03-04", 1, 3);
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    let (reserve, colleague, hourly) = (ids[0], ids[1], ids[2]);
    c.staff[0].employment = Employment::Reserve(early());
    c.staff[2].employment = Employment::Hourly;
    // (the colleague the more experienced: of two colleagues the dispatcher takes him)
    c.staff[1].experience = 60.0;
    let tomorrow = dates::add(&c.date, 1);
    let tours = vec![tour("1", 6 * 60, 3)];
    // the agency driver booked for tomorrow, the whole day
    agency::book(&mut c, &tomorrow, 1, None, &[]).unwrap();
    let booked = agency::drivers_on(&c, &tomorrow)[0].id;
    // the dispatcher's own: the reserve whose frame holds the duty, without asking
    let who = |c: &Company| plan::day_plan(c, &tomorrow, tours.clone(), &[], &[], false).tours[0].duties[0].who;
    assert_eq!(who(&c), Some(Who::Staff(reserve)));
    let p = plan::day_plan(&c, &tomorrow, tours.clone(), &[], &[], false);
    let order: Vec<(u32, u8)> = plan::candidates(&c, &p, 0, 0).into_iter().filter(|x| x.fit.is_ok() || x.id == reserve).map(|x| (x.id, x.tier)).collect();
    assert_eq!(order, vec![(reserve, plan::TIER_RESERVE), (colleague, plan::TIER_COLLEAGUE), (hourly, plan::TIER_HOURLY), (booked, plan::TIER_BOOKED)]);
    // the ways to cover it: the four in that order, then the agency sending one on the day
    let covers = plan::covers_of_duty(&c, &p, 0, 0);
    let ids_of: Vec<Option<u32>> = covers.iter().map(|x| if let Cover::Colleague { id, .. } = x { Some(*id) } else { None }).collect();
    assert_eq!(ids_of[..4], [Some(reserve), Some(colleague), Some(hourly), Some(booked)]);
    assert!(matches!(covers[4], Cover::Agency { .. }));
    // a frame that does not hold it: a colleague as the others (the more experienced first);
    // then the hourly paid; then the booked
    c.staff[0].employment = Employment::Reserve(Frame::new(9 * 60, 17 * 60, 0b111_1111));
    assert_eq!(who(&c), Some(Who::Staff(colleague)));
    c.staff[1].sick_until = Some(tomorrow.clone());
    c.staff[0].sick_until = Some(tomorrow.clone());
    assert_eq!(who(&c), Some(Who::Staff(hourly)));
    c.staff[2].sick_until = Some(tomorrow.clone());
    assert_eq!(who(&c), Some(Who::Staff(booked)));
    // a frame on other weekdays does not hold either
    let b = Block { key: "x".into(), from: 6 * 60, to: 9 * 60, ..Default::default() };
    let mut e = c.staff[0].clone();
    e.employment = Employment::Reserve(Frame::new(5 * 60, 13 * 60, 0b110_0000));
    assert_eq!(plan::fill_tier(&c, &e, &tomorrow, &b), plan::TIER_COLLEAGUE);
    e.employment = Employment::Reserve(early());
    assert_eq!(plan::fill_tier(&c, &e, &tomorrow, &b), plan::TIER_RESERVE);
}

#[test]
fn a_duty_that_falls_out_today_goes_to_the_reserve_first_and_the_rest_checks_hold() {
    let mut c = company("2024-03-04", 1, 3);
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    c.planning.auto = false;
    let wd = plan::weekday_of(&c.date);
    let b = c.fleet[0].id;
    plan::set_bus(&mut c, wd, "Linie5", "1", Some(b));
    plan::set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(ids[1])));
    c.staff[0].employment = Employment::Reserve(early());
    // the rostered driver falls ill this morning: the central covers it with the reserve
    c.staff[1].sick_until = Some(c.date.clone());
    let tours = vec![tour("1", 6 * 60, 3)];
    let p = plan::day_plan(&c, &c.date, tours.clone(), &[], &[], false);
    assert_eq!(p.tours[0].duties[0].who, Some(Who::Staff(ids[0])));
    // the night's rest still holds: back at 23:30 yesterday, the reserve may not begin at 06:00
    c.staff[0].last_end = Some(23 * 60 + 30);
    let p = plan::day_plan(&c, &c.date, tours, &[], &[], false);
    assert_eq!(p.tours[0].duties[0].who, Some(Who::Staff(ids[2])));
}

#[test]
fn a_reserve_given_a_duty_from_noon_the_day_before_is_told() {
    let mut c = company("2024-03-04", 2, 2);
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    let reserve = ids[0];
    c.planning.auto = false;
    c.staff[0].employment = Employment::Reserve(Frame::new(5 * 60, 20 * 60, 0b111_1111));
    let tomorrow = dates::add(&c.date, 1);
    let wd = plan::weekday_of(&tomorrow);
    let b = c.fleet[0].id;
    plan::set_bus(&mut c, wd, "Linie5", "1", Some(b));
    let b = c.fleet[1].id;
    plan::set_bus(&mut c, wd, "Linie5", "2", Some(b));
    plan::set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(reserve)));
    let tours = vec![tour("1", 6 * 60, 3), tour("2", 14 * 60, 3)];
    let told = |c: &Company| c.messages.list.iter().filter(|m| m.text == messages::RESERVE_DUTY && m.to == messages::To::Driver { employee: reserve }).count();
    // 11:00 the day before: given before noon - nothing to tell (known)
    c.clock.minute = 11 * 60;
    let p = plan::day_plan(&c, &tomorrow, tours.clone(), &[], &[], false);
    assert_eq!(plan::tell_reserves(&mut c, &p), 0);
    // 13:00: that one still his - not told; one given now is
    c.clock.minute = 13 * 60;
    assert_eq!(plan::tell_reserves(&mut c, &p), 0);
    plan::set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Staff(reserve)));
    let p = plan::day_plan(&c, &tomorrow, tours.clone(), &[], &[], false);
    assert_eq!(plan::tell_reserves(&mut c, &p), 1);
    assert_eq!(told(&c), 1);
    let m = c.messages.list.last().unwrap();
    let arg = |k: &str| m.args.iter().find(|x| x.0 == k).map(|x| x.1.clone()).unwrap_or_default();
    let sign_on = 14 * 60 - c.planning.flow.prefs.sign_on;
    assert_eq!((arg("date"), arg("n"), arg("tour"), arg("time")), (tomorrow.clone(), "5".to_string(), "2".to_string(), format!("{:02}:{:02}", sign_on / 60, sign_on % 60)));
    // once
    assert_eq!(plan::tell_reserves(&mut c, &p), 0);
    // taken from him and given again: told again
    plan::set_driver(&mut c, wd, "Linie5", "2", 0, None);
    let p0 = plan::day_plan(&c, &tomorrow, tours.clone(), &[], &[], false);
    plan::tell_reserves(&mut c, &p0);
    plan::set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Staff(reserve)));
    let p = plan::day_plan(&c, &tomorrow, tours.clone(), &[], &[], false);
    assert_eq!(plan::tell_reserves(&mut c, &p), 1);
    // a colleague who is no reserve is not told; two days ahead nobody yet
    plan::set_driver(&mut c, wd, "Linie5", "1", 0, Some(Who::Staff(ids[1])));
    let p = plan::day_plan(&c, &tomorrow, tours.clone(), &[], &[], false);
    assert_eq!(plan::tell_reserves(&mut c, &p), 0);
    let later = dates::add(&c.date, 2);
    let wd2 = plan::weekday_of(&later);
    let b = c.fleet[0].id;
    plan::set_bus(&mut c, wd2, "Linie5", "1", Some(b));
    plan::set_driver(&mut c, wd2, "Linie5", "1", 0, Some(Who::Staff(reserve)));
    let p = plan::day_plan(&c, &later, tours.clone(), &[], &[], false);
    assert_eq!(plan::tell_reserves(&mut c, &p), 0);
    assert!(c.planning.reserve_told.iter().all(|k| !k.starts_with(&later)));
}

#[test]
fn a_change_of_employment_applies_from_the_next_sunday() {
    // (4 March 2024 is a Monday)
    let mut c = company("2024-03-04", 0, 2);
    let id = c.staff[0].id;
    // hired today: at once
    assert_eq!(staff::change_employment(&mut c, id, Employment::Hourly), Ok(None));
    assert_eq!(c.staff[0].employment, Employment::Hourly);
    // later, from hourly paid up to part or full time: from the next company day
    c.date = dates::add(&c.date, 3);
    let from = staff::change_employment(&mut c, id, Employment::FullTime).unwrap().unwrap();
    assert_eq!(from, dates::add(&c.date, 1));
    assert_eq!(c.staff[0].employment, Employment::Hourly);
    c.date = from.clone();
    assert_eq!(staff::employment_due(&mut c), 1);
    assert_eq!(c.staff[0].employment, Employment::FullTime);
    // any other change: from the next Sunday - the old one until then (Friday 8 March: Sunday 10)
    assert_eq!(c.date, "2024-03-08");
    let from = staff::change_employment(&mut c, id, Employment::Reserve(early())).unwrap().unwrap();
    assert_eq!(from, "2024-03-10");
    assert_eq!(c.staff[0].employment, Employment::FullTime);
    c.date = dates::add(&from, -1);
    assert_eq!(staff::employment_due(&mut c), 0);
    c.date = from.clone();
    assert_eq!(staff::employment_due(&mut c), 1);
    assert_eq!((c.staff[0].employment, c.staff[0].employment_next.clone()), (Employment::Reserve(early()), None));
    // made on a Sunday: the Sunday after; on a Saturday: the next day, a Sunday
    let from = staff::change_employment(&mut c, id, Employment::PartTime { hours: 25.0 }).unwrap().unwrap();
    assert_eq!(from, "2024-03-17");
    c.date = "2024-03-16".into();
    assert_eq!(staff::change_employment(&mut c, id, Employment::PartTime { hours: 25.0 }), Ok(Some("2024-03-17".into())));
    // down from full time to hourly paid is no upgrade: the next Sunday too
    assert!(!staff::is_upgrade(&Employment::FullTime, &Employment::Hourly) && staff::is_upgrade(&Employment::Hourly, &Employment::PartTime { hours: 20.0 }));
    assert!(!staff::is_upgrade(&Employment::Hourly, &Employment::Reserve(early())));
    // the one they have takes a change back
    staff::change_employment(&mut c, id, Employment::FullTime).unwrap();
    assert!(c.staff[0].employment_next.is_some());
    assert_eq!(staff::change_employment(&mut c, id, Employment::Reserve(early())), Ok(None));
    assert!(c.staff[0].employment_next.is_none());
    // a change given a month's notice in an older file still applies on its day
    c.staff[0].employment_next = Some(EmploymentChange { to: Employment::Hourly, from: dates::add(&c.date, staff::EMPLOYMENT_NOTICE_DAYS) });
    c.date = dates::add(&c.date, staff::EMPLOYMENT_NOTICE_DAYS);
    assert_eq!(staff::employment_due(&mut c), 1);
    assert_eq!(c.staff[0].employment, Employment::Hourly);
    // only a driver's
    c.staff[1].role = Role::Mechanic;
    let mechanic = c.staff[1].id;
    assert!(staff::change_employment(&mut c, mechanic, Employment::Hourly).is_err());
    // hired as a kind; anyone but a driver full time
    let a = applicants(&c).into_iter().next().unwrap();
    let new = staff::hire_as(&mut c, &a, Employment::PartTime { hours: 20.0 }).unwrap();
    assert_eq!(c.employee(new).unwrap().employment, Employment::PartTime { hours: 20.0 });
}

#[test]
fn on_the_server_the_admin_sets_the_notice() {
    let mut c = company("2024-03-04", 0, 1);
    let id = c.staff[0].id;
    c.date = dates::add(&c.date, 1);
    c.online.on_server = true;
    // none read: the default, 2 days
    assert_eq!(staff::change_employment(&mut c, id, Employment::Hourly), Ok(Some(dates::add(&c.date, staff::SERVER_NOTICE_DAYS))));
    // the admin's: 5 days; 0: the next company day
    let mut kv = std::collections::HashMap::new();
    kv.insert("employment_notice_days".to_string(), "5".to_string());
    c.online.live = Some(crate::company::live_pay::LiveCfg::of_keys(&kv));
    assert_eq!(staff::change_employment(&mut c, id, Employment::Hourly), Ok(Some(dates::add(&c.date, 5))));
    kv.insert("employment_notice_days".to_string(), "0".to_string());
    c.online.live = Some(crate::company::live_pay::LiveCfg::of_keys(&kv));
    assert_eq!(staff::change_employment(&mut c, id, Employment::Hourly), Ok(Some(dates::add(&c.date, 1))));
    assert_eq!(staff::server_notice_days(&c), 0);
    // from hourly paid up: the next day, whatever the admin set
    kv.insert("employment_notice_days".to_string(), "9".to_string());
    c.online.live = Some(crate::company::live_pay::LiveCfg::of_keys(&kv));
    c.staff[0].employment = Employment::Hourly;
    c.staff[0].employment_next = None;
    assert_eq!(staff::change_employment(&mut c, id, Employment::PartTime { hours: 30.0 }), Ok(Some(dates::add(&c.date, 1))));
    assert_eq!(staff::change_employment(&mut c, id, Employment::Reserve(early())), Ok(Some(dates::add(&c.date, 9))));
    // the server's settings: the key, and the part added to an older file
    assert!(crate::company::live_pay::NOTICE_CFG.contains("employment_notice_days = 2"));
    assert_eq!(crate::company::live_pay::LiveCfg::default().notice_days, staff::SERVER_NOTICE_DAYS);
}

#[test]
fn the_player_hires_hourly_paid_and_the_game_full_time() {
    // the hiring window's choice to start from: hourly paid
    assert_eq!(staff::HIRED_AS, Employment::Hourly);
    let mut c = company("2024-03-04", 0, 0);
    let a = applicants(&c).into_iter().find(|a| a.role == Role::Driver).unwrap();
    let id = staff::hire_as(&mut c, &a, staff::HIRED_AS).unwrap();
    assert_eq!(c.employee(id).unwrap().employment, Employment::Hourly);
    // the game's own hiring (the autopilot, an AI company): full time as before
    let b = applicants(&c).into_iter().find(|b| b.role == Role::Driver && b.no != a.no).unwrap();
    let id = hire(&mut c, &b).unwrap();
    assert_eq!(c.employee(id).unwrap().employment, Employment::FullTime);
    c.cash += 1_000_000_00;
    c.taken = Default::default();
    if let Some(name) = staff::hire_best(&mut c, Role::Driver) {
        let e = c.staff.iter().find(|e| e.name == name).unwrap();
        assert_eq!(e.employment, Employment::FullTime);
    }
}

// --- (round L, NexiconDK 7 November 2026) a reserve's frame is a window, their week a contract ----

/// A frame of a file of before - five days of 12 hours - is a full week, not 60 hours; a shorter
/// frame week is that many hours (part time). One chosen is the contract.
#[test]
fn a_reserves_week_is_their_contract_not_the_frames_window() {
    let long = Frame::new(3 * 60, 15 * 60, 0b001_1111);
    assert_eq!(long.week_minutes(), 60 * 60);
    assert_eq!(Employment::Reserve(long).week_hours(37.0), Some(37.0));
    let short = Frame::new(5 * 60, 9 * 60, 0b001_1111);
    assert_eq!(Employment::Reserve(short).week_hours(37.0), Some(20.0));
    let chosen = long.with_contract(30 * 60, 0);
    assert_eq!(Employment::Reserve(chosen).week_hours(37.0), Some(30.0));
    assert_eq!(Employment::Reserve(long.with_contract(50 * 60, 0)).week_hours(37.0), Some(37.0), "at most a full week");
    // an employment of an older file loads as it was: no contract chosen, no named frame
    let old: Employment = serde_json::from_str(r#"{"kind":"reserve","from":180,"to":900,"days":31}"#).unwrap();
    assert_eq!(old, Employment::Reserve(long));
    let back = serde_json::to_string(&old).unwrap();
    assert!(!back.contains("contract") && !back.contains("ram"), "{back}");
    let kept: Employment = serde_json::from_str(&serde_json::to_string(&Employment::Reserve(long.with_contract(37 * 60, 2))).unwrap()).unwrap();
    assert_eq!(kept, Employment::Reserve(long.with_contract(37 * 60, 2)));
    // the hiring's choice made sound: a half hour's multiple, at most a full week
    let c = company("2024-03-04", 0, 0);
    let full = econ_uk::contract_week_hours(&c);
    assert_eq!(staff::sound_employment(&c, Employment::Reserve(long.with_contract(1_000 * 60, 1))), Employment::Reserve(long.with_contract((full * 60.0) as i32, 1)));
    assert_eq!(staff::sound_employment(&c, Employment::Reserve(long.with_contract(25 * 60 + 10, 0))), Employment::Reserve(long.with_contract(25 * 60, 0)));
}

/// A frame day pays the hours driven, at least 7:24: three hours driven are 7:24, nine are nine;
/// a day off the frame the duties; overtime above the contract week.
#[test]
fn a_frame_day_pays_the_hours_driven_at_least_7_24() {
    let c = company("2024-03-03", 0, 1);
    let p = c.planning.flow.prefs.clone();
    let least = crate::company::pro_turnus::NAMED_LEAST as f64;
    let mut e = c.staff[0].clone();
    e.employment = Employment::Reserve(Frame::new(3 * 60, 15 * 60, 0b001_1111).with_contract(37 * 60, 0));
    let full = 39.0 * 60.0;
    // Monday (0): nothing driven, three hours, nine hours
    assert_eq!(pay::day_minutes(&p, &e, 0, full, false, &[]).0, least);
    assert_eq!(pay::day_minutes(&p, &e, 0, full, false, &[(6 * 60, 9 * 60)]).0, least);
    let nine = pay::paid_minutes(&p, &[(5 * 60, 14 * 60)]) as f64;
    assert!(nine > least);
    assert_eq!(pay::day_minutes(&p, &e, 0, full, false, &[(5 * 60, 14 * 60)]).0, nine);
    // work past the window counts too
    assert_eq!(pay::day_minutes(&p, &e, 0, full, false, &[(10 * 60, 18 * 60)]).0, pay::paid_minutes(&p, &[(10 * 60, 18 * 60)]) as f64);
    // a Saturday, off the frame: the duties only; ill: nothing (the allowances)
    assert_eq!(pay::day_minutes(&p, &e, 5, full, false, &[]).0, 0.0);
    assert_eq!(pay::day_minutes(&p, &e, 0, full, true, &[]).0, 0.0);
    // the contract week is the week overtime is above, and is guaranteed as a permanent driver's
    let (_, due, week) = pay::day_minutes(&p, &e, 0, full, false, &[]);
    assert_eq!(week, 37.0 * 60.0);
    assert!((due - 37.0 * 60.0 / 7.0).abs() < 1e-9);
    // a duty outside the frame with the week near its contract: overtime above it, not "extra"
    let (price, _) = crate::company::pro_ops::price_of(&c, &e, plan::TIER_COLLEAGUE, 120, 37.0 * 60.0 - 60.0, false);
    assert!(matches!(price, crate::company::pro_ops::Price::Overtime { minutes: 60, .. }), "{price:?}");
    let (price, _) = crate::company::pro_ops::price_of(&c, &e, plan::TIER_COLLEAGUE, 120, 20.0 * 60.0, false);
    assert_eq!(price, crate::company::pro_ops::Price::Within);
    // a part-timer of 30 hours over five frame days: at least 6:00 a day, not 7:24 - five empty
    // frame days are their week, no overtime
    e.employment = Employment::Reserve(Frame::new(3 * 60, 15 * 60, 0b001_1111).with_contract(30 * 60, 0));
    assert_eq!(pay::day_minutes(&p, &e, 0, full, false, &[]).0, 360.0);
    assert_eq!(pay::day_minutes(&p, &e, 0, full, false, &[(5 * 60, 14 * 60)]).0, nine);
}

/// Hired as a reserve in RAM2 with a full contract and put on the reserve rota's first free row
/// (a new one when it has none); RAM2's times changed, the reserve's window follows; the rota aims
/// at its drivers' contract weeks.
#[test]
fn a_reserve_hired_in_ram2_is_put_on_the_reserve_rota() {
    use crate::company::pro_turnus as pt;
    let mut c = company("2024-03-04", 0, 0);
    let ram2 = pt::named_frames(&c).into_iter().find(|f| f.name == "RAM2").unwrap();
    let rota = pt::add(&mut c, "Reserve", true);
    let a = applicants(&c).into_iter().find(|a| a.role == Role::Driver).unwrap();
    let f = Frame::new(ram2.from, ram2.to, 0b001_1111).with_contract(37 * 60, ram2.id);
    let id = staff::hire_as(&mut c, &a, Employment::Reserve(f)).unwrap();
    assert_eq!(pt::place(&mut c, rota, id), Ok(0));
    assert_eq!(c.planning.rotas.list[0].rows.len(), 1);
    assert_eq!(c.planning.rotas.list[0].rows[0].driver, Some(id));
    assert_eq!(pt::named_of(&c, &c.employee(id).unwrap().employment.frame().unwrap()).map(|n| n.name), Some("RAM2".to_string()));
    // a permanent driver is refused on a reserve rota, and no row is left behind for them
    let b = applicants(&c).into_iter().find(|b| b.role == Role::Driver && b.no != a.no).unwrap();
    let other = staff::hire_as(&mut c, &b, Employment::FullTime).unwrap();
    assert_eq!(pt::place(&mut c, rota, other), Err(pt::RESERVES_ONLY));
    assert_eq!(c.planning.rotas.list[0].rows.len(), 1);
    // RAM2 now 06:00-17:00: the reserve's window with it, the contract kept
    pt::put_frame(&mut c, ram2.id, "RAM2", 6 * 60, 17 * 60).unwrap();
    let now = c.employee(id).unwrap().employment.frame().unwrap();
    assert_eq!((now.from, now.to, now.contract, now.ram), (6 * 60, 17 * 60, 37 * 60, ram2.id));
    // the rota aims at its drivers' contract weeks: 37 and 25 hours, 31 on average
    let c2 = c.employee(other).unwrap().id;
    c.staff.iter_mut().find(|e| e.id == c2).unwrap().employment = Employment::Reserve(Frame::new(300, 780, 0b001_1111).with_contract(25 * 60, 0));
    assert_eq!(pt::place(&mut c, rota, c2), Ok(1));
    let t = c.planning.rotas.list[0].clone();
    assert!((pt::target_hours(&c, &t) - 31.0).abs() < 1e-9, "{}", pt::target_hours(&c, &t));
    // RAM2 deleted: the window kept as one of its own
    pt::remove_frame(&mut c, ram2.id).unwrap();
    let now = c.employee(id).unwrap().employment.frame().unwrap();
    assert_eq!((now.from, now.to, now.ram), (6 * 60, 17 * 60, 0));
}
