//! (NexiconDK, 3 November 2026) Doubling: a departure full on three of its last five like days
//! is ordered, answered or decided by the dispatcher, runs as an extra tour, is paid by the bus
//! it runs with, and a full departure without it is a complaint against the bonus.

use super::*;
use crate::company::bus_contracts::{self as bc, Doubled, Journeys, Kind, Period, Work};
use crate::company::model::{CompanyLine, GameMode};
use crate::company::network::PlannedTrip;
use crate::company::{found, Founding};

/// A server company with a two-part contract on "Linie5" (100 € a timetable hour) from today.
fn company() -> Company {
    let mut c = found(&Founding { name: "Dublering".into(), game_mode: GameMode::Normal, date: "2026-11-02".into(), ..Default::default() }, "x");
    c.online.on_server = true;
    let from = c.date.clone();
    let mut k = bc::make(&mut c, "Linie5", "5", Kind::Gross, Period::Day, 300_000_00, &Work { hours: 14.0, km: 280.0, journeys: 20, buses: 1, days: 312 }, &from, &dates::add(&from, 30));
    k.per_hour = 100_00;
    c.concessions.contracts.push(k);
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), ..Default::default() });
    c.cash = 10_000_00;
    c
}

/// The day's close of doubling with these journeys on the company's day.
fn close(c: &mut Company, j: &Journeys) -> DayReport {
    let mut r = DayReport::default();
    let date = c.date.clone();
    close_day(c, &date, j, &mut r);
    r
}

fn full_at(dep: i32) -> Journeys {
    let mut j = Journeys::default();
    j.full("Linie5", "3", dep, dep + 40, BusSize::Solo, 12);
    j
}

/// A bus is full at the share of all its places, seats and standing: a solo bus 91 (70 by 1.3);
/// at the admin's 90 % sooner.
#[test]
fn a_bus_is_full_at_its_places() {
    let r = Rules::default();
    assert!(is_full(91.0, BusSize::Solo, &r) && !is_full(90.0, BusSize::Solo, &r));
    assert!(is_full(82.0, BusSize::Solo, &Rules { full_share: 0.9, ..r }));
    let mut kv = HashMap::new();
    kv.insert("doubling_price".to_string(), "700".to_string());
    kv.insert("doubling_full".to_string(), "90".to_string());
    let x = Rules::of_keys(&kv);
    assert_eq!((x.price_hour, x.full_share), (super::super::server_fines::Fines::cents(700.0), 0.9));
    assert_eq!(Rules::of_keys(&HashMap::new()), Rules::default());
    assert_eq!((DayKind::of("2026-11-02"), DayKind::of("2026-11-07"), DayKind::of("2026-11-08")), (DayKind::Weekday, DayKind::Saturday, DayKind::Sunday));
}

/// Full on three of its last five weekdays: ordered from the day after tomorrow for a week of
/// weekdays, told; answered until 18:00 the day before; left unanswered the dispatcher says yes
/// with money in the bank. One order at a time on a departure.
#[test]
fn a_departure_full_on_three_of_five_like_days_is_ordered() {
    let mut c = company();
    // (Monday, Tuesday full; Wednesday not; Thursday full: ordered)
    for (k, full) in [true, true, false, true].into_iter().enumerate() {
        c.date = dates::add("2026-11-02", k as i64);
        close(&mut c, &if full { full_at(432) } else { Journeys::default() });
        assert_eq!(c.online.doubling.orders.len(), (k == 3) as usize, "day {k}");
    }
    let o = c.online.doubling.orders[0].clone();
    assert_eq!((o.from.as_str(), o.until.as_str(), o.day, o.answer, o.dep), ("2026-11-07", "2026-11-13", Some(DayKind::Weekday), Answer::Asked, 432));
    assert!(o.covers("2026-11-09") && !o.covers("2026-11-07"), "its weekdays");
    assert!(c.inbox.messages.iter().any(|m| m.title == ORDERED.0), "told");
    // (full again on Friday: no second order)
    c.date = "2026-11-06".into();
    close(&mut c, &full_at(432));
    assert_eq!(c.online.doubling.orders.len(), 1);
    // (answered: yes; after 18:00 the day before, too late)
    c.clock.minute = 600;
    answer(&mut c, o.id, true).unwrap();
    assert_eq!(c.online.doubling.orders[0].answer, Answer::Yes);
    c.date = "2026-11-06".into();
    c.clock.minute = 18 * 60;
    assert_eq!(answer(&mut c, o.id, false), Err(TOO_LATE));
    // (another, not answered: the dispatcher decides the night before it begins)
    let mut d = company();
    for day in ["2026-11-02", "2026-11-03", "2026-11-04"] {
        d.date = day.into();
        close(&mut d, &full_at(500));
    }
    assert_eq!(d.online.doubling.orders[0].answer, Answer::Asked);
    d.date = "2026-11-05".into();
    close(&mut d, &Journeys::default());
    assert_eq!((d.online.doubling.orders[0].answer, d.online.doubling.orders[0].by.as_str()), (Answer::Yes, "dispatcher"));
    // (and no money: no)
    let mut e = company();
    e.cash = -1;
    for day in ["2026-11-02", "2026-11-03", "2026-11-04", "2026-11-05"] {
        e.date = day.into();
        close(&mut e, &full_at(500));
    }
    assert_eq!(e.online.doubling.orders[0].answer, Answer::No);
}

/// Said yes to, the order is an extra tour on its days - its departure's trip copied - and the
/// two share the departure's passengers.
#[test]
fn the_extra_tour_runs_behind_its_departure() {
    let mut c = company();
    c.online.doubling.orders.push(Order { id: 1, line: "Linie5".into(), number: "5".into(), tour: "3".into(), dep: 432, arr: 472, day: Some(DayKind::Weekday), from: "2026-11-02".into(), until: "2026-11-08".into(), answer: Answer::Yes, ..Default::default() });
    let trip = |dep: i32| PlannedTrip { name: format!("t{dep}"), line: "5".into(), from: "A".into(), to: "B".into(), dep, arr: dep + 40, km: 12.0, stops: 10, empty: false, origin: "3".into(), last: "B".into(), part: None };
    let tour = TourOfDay { line: "Linie5".into(), number: "5".into(), tour: "3".into(), ai_group: String::new(), trips: vec![trip(400), trip(432), trip(480)], unplanned: false, cuts: None };
    let mut out = vec![tour.clone()];
    add_tours(&c, "2026-11-03", &mut out);
    assert_eq!(out.len(), 2);
    assert_eq!((out[1].tour.as_str(), out[1].trips.len(), out[1].trips[0].dep), ("3+D432", 1, 432));
    assert!(is_tour(&out[1].tour) && base_of(&out[1].tour) == Some(("3", 432)));
    let mut sat = vec![tour.clone()];
    add_tours(&c, "2026-11-07", &mut sat);
    assert_eq!(sat.len(), 1, "not on a Saturday");
    // (not said yes to: none)
    c.online.doubling.orders[0].answer = Answer::No;
    let mut no = vec![tour];
    add_tours(&c, "2026-11-03", &mut no);
    assert_eq!(no.len(), 1);
    let doubled = vec![("Linie5".to_string(), "3".to_string(), 432)];
    assert_eq!((share(&doubled, "Linie5", "3", 432), share(&doubled, "Linie5", "3+D432", 432), share(&doubled, "Linie5", "3", 480)), (0.5, 0.5, 1.0));
}

/// A run is paid by its minutes: at the contract's price with one of the line's own buses, at the
/// admin's with another, a quality point each; a full departure without the bus ordered - said
/// no to, or not run - is a complaint against the bonus.
#[test]
fn the_runs_are_paid_and_a_full_departure_without_one_is_a_complaint() {
    let mut c = company();
    c.date = "2026-11-03".into();
    c.online.doubling.orders.push(Order { id: 1, line: "Linie5".into(), number: "5".into(), tour: "3".into(), dep: 432, arr: 472, day: None, from: "2026-11-02".into(), until: "2026-11-08".into(), answer: Answer::Yes, ..Default::default() });
    let index = c.contract_index / c.concessions.contracts[0].index;
    let mut j = Journeys::default();
    j.doubled(Doubled { line: "Linie5".into(), tour: "3+D432".into(), run: true, minutes: 30, contract_bus: true });
    close(&mut c, &j);
    let paid: Cents = c.ledger.iter().filter(|b| b.text.contains("doubling")).map(|b| b.amount).sum();
    assert_eq!(paid, (100_00.0 * index * 0.5).round() as Cents, "half an hour at the contract's price");
    assert_eq!((c.online.doubling.orders[0].runs, c.online.doubling.orders[0].paid), (1, paid));
    assert!(c.concessions.contracts[0].record.iter().any(|d| d.quality == QUALITY_PER_RUN && d.why == vec!["doubling".to_string()]));
    // (another bus: the admin's price)
    c.date = "2026-11-04".into();
    let mut j = Journeys::default();
    j.doubled(Doubled { line: "Linie5".into(), tour: "3+D432".into(), run: true, minutes: 60, contract_bus: false });
    close(&mut c, &j);
    let today: Cents = c.ledger.iter().filter(|b| b.date == "2026-11-04" && b.text.contains("doubling")).map(|b| b.amount).sum();
    assert_eq!(today, (Rules::default().price_hour as f64 * c.price_index).round() as Cents);
    // (full and the extra bus not run: a complaint; run, none)
    c.date = "2026-11-05".into();
    close(&mut c, &full_at(432));
    assert_eq!((c.concessions.contracts[0].bonus.complaints, c.online.doubling.orders[0].declined), (1, 1));
    c.date = "2026-11-06".into();
    let mut j = full_at(432);
    j.doubled(Doubled { line: "Linie5".into(), tour: "3+D432".into(), run: true, minutes: 40, contract_bus: true });
    close(&mut c, &j);
    assert_eq!(c.concessions.contracts[0].bonus.complaints, 1);
    // (said no to: each full departure of its days a complaint)
    c.online.doubling.orders[0].answer = Answer::No;
    c.date = "2026-11-07".into();
    close(&mut c, &full_at(432));
    assert_eq!(c.concessions.contracts[0].bonus.complaints, 2);
}

/// The server's admin orders an extra bus himself - on the company's duty board from its day,
/// every day, to be run (the company cannot say no).
#[test]
fn the_admin_orders_an_extra_bus() {
    let data = std::env::temp_dir().join(format!("omsihub-doubling-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    let mut c = company();
    order_by_admin(&data, &c.id, &AdminOrder { line: "Linie5".into(), tour: "3".into(), dep: 1200, from: "2026-11-07".into(), until: "2026-11-08".into() }).unwrap();
    assert!(ordered_by_admin(&data, &mut c));
    assert!(!ordered_by_admin(&data, &mut c), "read once");
    let o = c.online.doubling.orders[0].clone();
    assert_eq!((o.answer, o.by.as_str(), o.day, o.dep), (Answer::Yes, "admin", None, 1200));
    assert!(o.runs_on("2026-11-07") && o.runs_on("2026-11-08"));
    assert_eq!(answer(&mut c, o.id, false), Err(NOT_YOURS));
    assert!(c.inbox.messages.iter().any(|m| m.title == BY_ADMIN.0));
    let _ = std::fs::remove_dir_all(&data);
}
