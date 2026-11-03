use super::*;
use crate::company::together::book::Guest;
use crate::company::together::Role;
use crate::company::{found, Founding};
use crate::TripRun;

fn server() -> Company {
    let mut c = found(&Founding { name: "Live".into(), date: "2026-03-04".into(), map: "maps/Ahlheim/global.cfg".into(), ..Default::default() }, "x");
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

/// A trip of an hour by its timetable (07:00 to 08:00).
fn hour_trip() -> TripRun {
    TripRun { departure: 7.0 * 3600.0, arrival: 8.0 * 3600.0, seconds: 3500.0, ..Default::default() }
}

/// The rules of the settings: per cent read as shares, the defaults where a key is missing or not
/// a number.
#[test]
fn the_rules_of_the_settings() {
    let mut kv = HashMap::new();
    kv.insert("player_pay_share".to_string(), "80".to_string());
    kv.insert("live_surcharge".to_string(), "7,5".to_string());
    kv.insert("driver_pool".to_string(), "".to_string());
    kv.insert("ai_quality_cap".to_string(), "x".to_string());
    let r = LiveCfg::of_keys(&kv);
    assert_eq!((r.pay_share, r.surcharge, r.pool, r.ai_cap), (0.8, 0.075, 0, 70.0));
    assert_eq!(LiveCfg::of_keys(&HashMap::new()), LiveCfg::default());
    assert!(LIVE_CFG.contains("player_pay_share = 90") && LIVE_CFG.contains("driver_pool ="));
}

/// K1: on the server a player's trip pays 90 % of what an AI driver costs for its paid hours -
/// about 500 kr. a timetable hour at the Danish level, less than the AI driver's ~550 -, a free
/// drive the same share of the time driven; off the server the company's share of the trip's
/// money as before.
#[test]
fn a_player_is_paid_less_than_an_ai_driver_costs() {
    let c = server();
    let t = hour_trip();
    let ai = ai_hour(&c) * crate::company::econ_uk::PAID_PER_TIMETABLED_HOUR;
    let pay = trip_pay(&c, "drv", &t, 1_000_00);
    assert_eq!(pay, (0.9 * ai).round() as Cents);
    assert!((pay as f64) < ai);
    let kr = pay as f64 / 100.0 * crate::company::currency::Currency::Dkk.rate();
    assert!((400.0..=650.0).contains(&kr), "{kr:.0} kr. a timetable hour");
    assert_eq!(free_drive_pay(&c, 30.0), (0.9 * ai_hour(&c) * 0.5).round() as Cents);
    // (a trip past midnight, and one with no timetable: its time)
    let night = TripRun { departure: 23.5 * 3600.0, arrival: 0.5 * 3600.0, ..Default::default() };
    assert!((trip_hours(&night) - 1.0).abs() < 1e-9);
    assert!((trip_hours(&TripRun { seconds: 1800.0, ..Default::default() }) - 0.5).abs() < 1e-9);
    let mut home = c.clone();
    home.online.on_server = false;
    assert_eq!(trip_pay(&home, "drv", &t, 1_000_00), crate::company::online::pay_of(&home, 1_000_00));
}

/// K5: the owner and a co-owner choose whether their own driving pays them; a driver may not; a
/// free one is paid nothing, a duty's trip nor a free drive's.
#[test]
fn the_owner_chooses_whether_his_driving_pays_him() {
    let mut c = server();
    assert!(may_drive_free(&c, 1) && may_drive_free(&c, 2) && !may_drive_free(&c, 3));
    assert_eq!(set_drives_free(&mut c, 3, true), Err(NOT_OWNER));
    set_drives_free(&mut c, 1, true).unwrap();
    set_drives_free(&mut c, 2, true).unwrap();
    assert_eq!(trip_pay(&c, "own", &hour_trip(), 1_000_00), 0);
    assert!(trip_pay(&c, "drv", &hour_trip(), 1_000_00) > 0);
    set_drives_free(&mut c, 2, false).unwrap();
    assert_eq!(c.online.drives_free, vec!["own".to_string()]);
    assert!(trip_pay(&c, "co", &hour_trip(), 1_000_00) > 0);
    let mut home = c.clone();
    home.online.on_server = false;
    assert!(!may_drive_free(&home, 1));
}

/// K2: the operating quality - no live trips: the track record, at most the AI's cap; half the
/// journeys run live and passed at 95: half and half; a live trip that failed counts no more than
/// the cap; days older than the window are forgotten; off the server nothing is recorded.
#[test]
fn live_trips_lift_the_operating_quality() {
    let mut c = server();
    let record = crate::company::bus_contracts::quality(&c);
    let ai = record.min(70.0);
    assert_eq!(operation(&c), ai);
    let date = c.date.clone();
    record_day(&mut c, &date, 100, &vec![(95.0, true); 50]);
    assert!((operation(&c) - (0.5 * ai + 0.5 * 95.0)).abs() < 1e-9, "{}", operation(&c));
    // (failed trips: at most the cap)
    record_day(&mut c, &date, 100, &vec![(95.0, false); 50]);
    assert!((operation(&c) - (0.5 * ai + 0.5 * 70.0)).abs() < 1e-9);
    assert_eq!(c.online.live_days.len(), 1, "one day each");
    // (more live trips than journeys counted: all of them live)
    record_day(&mut c, &date, 10, &vec![(90.0, true); 20]);
    assert!((operation(&c) - 90.0).abs() < 1e-9);
    // (the window)
    c.date = crate::company::dates::add(&date, WINDOW_DAYS + 1);
    assert_eq!(operation(&c), ai);
    // (the tenders' operating quality on the server is it)
    let mut home = server();
    home.online.on_server = false;
    record_day(&mut home, &date, 100, &[(95.0, true)]);
    assert!(home.online.live_days.is_empty());
}

/// K3: the live surcharge - 5 % of the hour price for the hours driven live and passed.
#[test]
fn the_authority_pays_a_surcharge_for_hours_driven_live() {
    let c = server();
    assert_eq!(surcharge(&c, 1_000_00.0, 120), 10_000);
    assert_eq!(surcharge(&c, 1_000_00.0, 0), 0);
    let mut none = c.clone();
    none.online.live = Some(LiveCfg { surcharge: 0.0, ..LiveCfg::default() });
    assert_eq!(surcharge(&none, 1_000_00.0, 120), 0);
}

/// K4: a map's pool of AI drivers - its companies' drivers counted, none left: no driver hired
/// (the staff's hiring says why), one hired: one fewer; no pool: as many as they like.
#[test]
fn the_ai_drivers_of_a_map_are_short() {
    use crate::company::model::Role as Job;
    let mut c = server();
    assert_eq!(pool_free(&c, 30), None);
    assert!(may_hire_driver(&c).is_ok());
    c.online.live = Some(LiveCfg { pool: 10, ..LiveCfg::default() });
    assert_eq!(pool_free(&c, 7), Some(3));
    c.online.pool_free = Some(1);
    assert!(may_hire_driver(&c).is_ok() && crate::company::staff::hireable(&c, Job::Driver).is_ok());
    took_driver(&mut c);
    assert_eq!(c.online.pool_free, Some(0));
    assert_eq!(may_hire_driver(&c), Err(DRIVERS_SHORT));
    assert_eq!(crate::company::staff::hireable(&c, Job::Driver), Err(DRIVERS_SHORT));
    assert!(crate::company::staff::hireable(&c, Job::Mechanic).is_ok(), "the other roles are not short");
    assert_eq!(pool_of(&c), Some((0, 10)));
    // (off the server: no pool)
    c.online.on_server = false;
    assert!(may_hire_driver(&c).is_ok());
}
