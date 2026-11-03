use super::*;
use crate::company::doubling::{Answer, Order};
use crate::company::together::book::Guest;
use crate::company::together::Role;
use crate::company::{found, Founding};
use crate::TripRun;

fn server() -> Company {
    let mut c = found(&Founding { name: "More".into(), date: "2026-03-04".into(), map: "maps/Ahlheim/global.cfg".into(), ..Default::default() }, "x");
    c.online.on_server = true;
    c.online.price_level = Some("dk".into());
    c.online.owner_id = "own".into();
    c.together.guests = vec![
        Guest { no: 1, id: "own".into(), name: "Owner".into(), ..Default::default() },
        Guest { no: 2, id: "co".into(), name: "Co".into(), role: Role::CoOwner, ..Default::default() },
        Guest { no: 3, id: "drv".into(), name: "Driver".into(), role: Role::Driver, ..Default::default() },
    ];
    c
}

/// A trip of an hour by its timetable, leaving at `hour`.
fn trip(line: &str, tour: &str, hour: f64) -> TripRun {
    TripRun { line: line.into(), tour: tour.into(), departure: hour * 3600.0, arrival: (hour + 1.0) * 3600.0, seconds: 3500.0, ..Default::default() }
}

/// K6: a doubling the admin ordered is urgent and the players' alone, paid 50 % more; one the
/// company said yes to is neither; rail replacement is urgent (the AI may still run it).
#[test]
fn urgent_tasks_for_players_only() {
    let mut c = server();
    let date = c.date.clone();
    let order = |by: &str| Order { id: 1, line: "Linie5".into(), number: "5".into(), tour: "3".into(), dep: 7 * 60, arr: 8 * 60, from: date.clone(), until: date.clone(), answer: Answer::Yes, by: by.into(), ..Default::default() };
    c.online.doubling.orders = vec![order("admin")];
    let extra = format!("3{}{}", crate::company::doubling::MARK, 7 * 60);
    assert!(urgent(&c, &date, "Linie5", &extra) && players_only(&c, &date, "Linie5", &extra));
    assert!(!urgent(&c, &date, "Linie5", "3"), "the departure itself is not");
    let base = crate::company::live_pay::duty_pay(&c, &trip("Linie5", &extra, 7.0));
    let paid = crate::company::live_pay::trip_pay(&c, "drv", &trip("Linie5", &extra, 7.0), 0);
    assert_eq!(paid, (base as f64 * 1.5).round() as Cents);
    c.online.doubling.orders = vec![order("company")];
    assert!(!urgent(&c, &date, "Linie5", &extra) && !players_only(&c, &date, "Linie5", &extra));
    c.online.on_server = false;
    c.online.doubling.orders = vec![order("admin")];
    assert!(!urgent(&c, &date, "Linie5", &extra), "only on the server");
}

/// K7: a passed live trip counts its minutes and score in its month, a failed one does not; the
/// month's ranking by hours, then by the average score; the company's share driven live.
#[test]
fn the_driver_of_the_month() {
    let at = |date: &str| (crate::company::dates::parse(date).unwrap() * 86_400 + 3600) as u64;
    assert_eq!(month_of(at("2026-11-03")), "2026-11");
    let mut a = Vec::new();
    count_trip(&mut a, at("2026-11-03"), 60.0, 90.0, true);
    count_trip(&mut a, at("2026-11-04"), 30.0, 70.0, true);
    count_trip(&mut a, at("2026-11-05"), 60.0, 40.0, false);
    count_trip(&mut a, at("2026-10-30"), 600.0, 99.0, true);
    assert_eq!(a.iter().find(|m| m.month == "2026-11").map(|m| (m.minutes, m.trips)), Some((90.0, 2)));
    let mut b = Vec::new();
    count_trip(&mut b, at("2026-11-02"), 90.0, 95.0, true);
    let rows = ranking([("Anna", a.as_slice()), ("Bo", b.as_slice())].into_iter(), "2026-11", 5);
    assert_eq!(rows.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["Bo", "Anna"], "same hours: the better score");
    assert!((rows[1].hours - 1.5).abs() < 1e-9 && (rows[1].score - 80.0).abs() < 1e-9);
    assert_eq!(ranking([("Anna", a.as_slice())].into_iter(), "2026-10", 5)[0].hours, 10.0);
    for k in 0..5 {
        count_trip(&mut a, at(&format!("2027-0{}-01", k + 1)), 1.0, 50.0, true);
    }
    assert_eq!(a.len(), MONTHS_KEPT);
    let mut c = server();
    let date = c.date.clone();
    c.online.live_days = vec![crate::company::live_pay::LiveDay { date, journeys: 40, live: 10, score: 800.0 }];
    assert!((live_share(&c) - 0.25).abs() < 1e-9);
}

/// K10: a duty given for good is the player's on its weekday (another player cannot have it), the
/// same duty on another weekday adds that day; it pays the loyalty bonus; given back, it is gone.
#[test]
fn a_fixed_duty() {
    let mut c = server();
    let date = c.date.clone();
    let wd = crate::company::plan::weekday_of(&date);
    let id = give_fixed(&mut c, 3, &date, "Linie5", "5", "3", 0).unwrap();
    assert_eq!(fixed_for(&c, wd, "Linie5", "3", 0), Some(3));
    assert_eq!(fixed_for(&c, (wd + 1) % 7, "Linie5", "3", 0), None);
    assert_eq!(give_fixed(&mut c, 2, &date, "Linie5", "5", "3", 0), Err(FIXED_TAKEN));
    let next = crate::company::dates::add(&date, 1);
    assert_eq!(give_fixed(&mut c, 3, &next, "Linie5", "5", "3", 0), Ok(id));
    assert_eq!(fixed_for(&c, (wd + 1) % 7, "Linie5", "3", 0), Some(3));
    assert!((pay_factor(&c, "drv", &date, "Linie5", "3") - 1.05).abs() < 1e-9);
    assert_eq!(pay_factor(&c, "co", &date, "Linie5", "3"), 1.0, "not his");
    assert_eq!(return_fixed(&mut c, 2, id, false), Err(NO_FIXED), "not hers");
    assert!(return_fixed(&mut c, 3, id, false).is_ok());
    assert!(c.online.fixed.is_empty());
    c.online.on_server = false;
    assert_eq!(give_fixed(&mut c, 3, &date, "Linie5", "5", "3", 0), Err(NOT_ON_SERVER));
}

/// K11: the admin's supplements by the clock and the day - weekday evenings, nights past midnight,
/// Saturdays, Sundays and holidays, snow -, each a share and kroner an hour; kept in the server's
/// file and read by the company's process.
#[test]
fn supplements_for_unsocial_hours() {
    let s = |when: When| Supplement { id: 1, name: "x".into(), when, percent: 10.0, per_hour: 0.0 };
    // (2026-11-03 a Tuesday, 2026-11-07 a Saturday, 2026-11-08 a Sunday)
    let evening = s(When::Evening { from: 18 * 60, to: 23 * 60 });
    assert!(applies(&evening, "2026-11-03", 19 * 60, false, false) && !applies(&evening, "2026-11-03", 17 * 60, false, false));
    assert!(!applies(&evening, "2026-11-07", 19 * 60, false, false), "not on a Saturday");
    let night = s(When::Night { from: 23 * 60, to: 5 * 60 });
    assert!(applies(&night, "2026-11-07", 60, false, false) && applies(&night, "2026-11-03", 23 * 60 + 30, false, false) && !applies(&night, "2026-11-03", 6 * 60, false, false));
    assert!(applies(&s(When::Saturday), "2026-11-07", 600, false, false) && !applies(&s(When::Saturday), "2026-11-08", 600, false, false));
    assert!(applies(&s(When::Sunday), "2026-11-08", 600, false, false) && applies(&s(When::Sunday), "2026-11-03", 600, true, false));
    assert!(applies(&s(When::Snow), "2026-11-03", 600, false, true) && !applies(&s(When::Snow), "2026-11-03", 600, false, false));
    let data = std::env::temp_dir().join(format!("omsihub-supplements-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    std::fs::create_dir_all(&data).unwrap();
    assert!(add_supplement(&data, "", When::Saturday, 10.0, 0.0).is_err() && add_supplement(&data, "Zero", When::Saturday, 0.0, 0.0).is_err());
    let a = add_supplement(&data, "Evening", When::Evening { from: 18 * 60, to: 23 * 60 }, 25.0, 0.0).unwrap();
    let b = add_supplement(&data, "Night", When::Night { from: 23 * 60, to: 5 * 60 }, 0.0, 40.0).unwrap();
    let mut c = server();
    c.date = "2026-11-03".into();
    assert!(refresh_supplements(&data, &mut c) && c.online.supplements.len() == 2);
    assert!(!refresh_supplements(&data, &mut c), "unchanged");
    assert_eq!(supplements_pay(&c, "2026-11-03", 19 * 60, 1000_00, 1.0), 250_00);
    let night_pay = supplements_pay(&c, "2026-11-03", 23 * 60 + 10, 1000_00, 0.5);
    assert_eq!(night_pay, (crate::company::server_fines::Fines::cents(40.0) as f64 * c.price_index * 0.5).round() as Cents);
    assert_eq!(supplements_pay(&c, "2026-11-03", 12 * 60, 1000_00, 1.0), 0);
    // (a player's evening trip: his pay with it)
    let base = crate::company::live_pay::duty_pay(&c, &trip("Linie5", "3", 19.0));
    assert_eq!(crate::company::live_pay::trip_pay(&c, "drv", &trip("Linie5", "3", 19.0), 0), base + (base as f64 * 0.25).round() as Cents);
    remove_supplement(&data, a).unwrap();
    remove_supplement(&data, b).unwrap();
    assert!(load_supplements(&data).is_empty() && remove_supplement(&data, a).is_err());
    let _ = std::fs::remove_dir_all(&data);
}

/// K12: the owner or a co-owner who drove two duties live in the last week raises the working
/// environment by 10; a driver's duties do not count, nor an old week's.
#[test]
fn the_boss_drives() {
    let mut c = server();
    let date = c.date.clone();
    let before = crate::company::viborg::work(&c);
    boss_drove(&mut c, 3, &date, "Linie5", "1");
    boss_drove(&mut c, 1, &date, "Linie5", "1");
    boss_drove(&mut c, 1, &date, "Linie5", "1");
    assert_eq!((boss_duties(&c), boss_points(&c)), (1, 0.0), "a driver's and the same duty again do not count");
    boss_drove(&mut c, 2, &date, "Linie5", "2");
    assert_eq!((boss_duties(&c), boss_points(&c)), (2, BOSS_POINTS));
    assert!((crate::company::viborg::work(&c) - (before + BOSS_POINTS).min(100.0)).abs() < 1e-9);
    c.date = crate::company::dates::add(&date, BOSS_DAYS);
    assert_eq!(boss_points(&c), 0.0, "a week on");
}

/// K13: one dispatcher at a time, on duty while online; a spare bus reaches its trips in 15
/// minutes then.
#[test]
fn the_live_dispatcher() {
    let mut c = server();
    let date = c.date.clone();
    let normal = crate::company::clock::spare_minutes(&c, &date, 600);
    assert!(normal > DISPATCHER_SPARE);
    set_dispatcher(&mut c, 2, "Co", true, 100).unwrap();
    assert!(dispatcher_on(&c));
    assert_eq!(set_dispatcher(&mut c, 3, "Driver", true, 101), Err(DISPATCH_TAKEN));
    assert_eq!(crate::company::clock::spare_minutes(&c, &date, 600), DISPATCHER_SPARE);
    assert!(!dispatcher_left(&mut c, &|g| g == 2), "online");
    assert!(dispatcher_left(&mut c, &|_| false) && !dispatcher_on(&c), "gone");
    set_dispatcher(&mut c, 2, "Co", true, 100).unwrap();
    set_dispatcher(&mut c, 3, "Driver", false, 102).unwrap();
    assert!(dispatcher_on(&c), "another cannot end it");
    set_dispatcher(&mut c, 2, "Co", false, 103).unwrap();
    assert!(!dispatcher_on(&c));
    assert_eq!(crate::company::clock::spare_minutes(&c, &date, 600), normal);
}

/// K4 (3 November 2026): the agency drivers run the duties nobody covers as the server's rules
/// say, unless the company's owner or a co-owner chose otherwise; a driver may not choose.
#[test]
fn the_agency_fallback_by_the_server_and_by_the_company() {
    use crate::company::live_pay::{agency_runs_open, set_agency, LiveCfg, NOT_OWNER_AGENCY};
    let mut c = server();
    assert!(agency_runs_open(&c), "the server's default");
    let mut kv = std::collections::HashMap::new();
    kv.insert("agency_fallback".to_string(), "0".to_string());
    c.online.live = Some(LiveCfg::of_keys(&kv));
    assert!(!agency_runs_open(&c), "the server's rule: dropped");
    assert_eq!(set_agency(&mut c, 3, Some(true)), Err(NOT_OWNER_AGENCY));
    set_agency(&mut c, 2, Some(true)).unwrap();
    assert!(agency_runs_open(&c), "the company's choice");
    set_agency(&mut c, 1, Some(false)).unwrap();
    c.online.live = None;
    assert!(!agency_runs_open(&c));
    set_agency(&mut c, 1, None).unwrap();
    assert!(agency_runs_open(&c), "as the server");
    c.online.on_server = false;
    assert!(!agency_runs_open(&c));
    let r = LiveCfg::of_keys(&std::collections::HashMap::new());
    assert!(r.agency && r.urgent == 0.5 && r.loyalty == 0.05 && r.boss_duties == 2);
    assert!(crate::company::live_pay::MORE_LIVE_CFG.contains("agency_fallback = 1"));
}
