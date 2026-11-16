use super::*;
use crate::company::market::{self, MarketBus, Payment};
use crate::company::model::CompanyLine;
use crate::company::{found, Founding};

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("openomsi-takeover-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// A company of the server with line 5, two buses of its own on its tours and one leased.
fn company(name: &str, id: &str) -> Company {
    let mut c = found(&Founding { name: name.into(), date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "owner");
    c.id = id.into();
    c.online.on_server = true;
    c.cash += 5_000_000_00;
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), ..Default::default() });
    c
}

/// (Decided 25 October 2026) A line won from another company: the loser offers the buses of its
/// own it ran the line with at their book value - not a leased one -, the winner takes them, and
/// on the day its term begins they leave the loser's fleet, sold, and join the winner's, paid;
/// a no keeps them where they are.
#[test]
fn the_winner_takes_over_the_losers_buses_of_the_line() {
    let data = temp("yes");
    let (mut a, mut b) = (company("Umove", "umove"), company("GoCollective", "gocollective"));
    let wd = crate::company::plan::weekday_of(&a.date);
    let bus = MarketBus { name: "Citaro".into(), ..Default::default() };
    let x = market::buy_new(&mut a, &bus, Payment::Cash, "").unwrap();
    let y = market::buy_new(&mut a, &bus, Payment::Cash, "").unwrap();
    crate::company::plan::set_bus(&mut a, wd, "Linie5", "1", Some(x));
    crate::company::plan::set_bus(&mut a, (wd + 1) % 7, "Linie5", "2", Some(y));
    crate::company::plan::set_bus(&mut a, wd, "Linie5", "3", Some(y));
    assert_eq!(buses_of_line(&a, "Linie5"), [x, y]);
    let date = "2024-03-08";
    assert_eq!(offer(&data, &mut a, "gocollective", "GoCollective", "Linie5", "5", date), 2);
    assert!(step(&data, &mut b));
    let o = b.online.takeover.offers[0].clone();
    assert_eq!((o.buses.len(), o.answer, o.total() > 0), (2, None, true));
    assert!(b.inbox.messages.iter().any(|m| m.title == OFFERED.0));
    // (yes: told to the seller, kept as a sale)
    answer(&data, &mut b, &o.id, true).unwrap();
    assert_eq!(answer(&data, &mut b, &o.id, true), Err(NO_OFFER), "answered");
    assert!(step(&data, &mut a));
    assert_eq!(a.online.takeover.sales.len(), 1);
    // (not before its day)
    let (fleet_a, fleet_b, cash_b) = (a.fleet.len(), b.fleet.len(), b.cash);
    step(&data, &mut a);
    assert_eq!(a.fleet.len(), fleet_a);
    a.date = date.into();
    assert!(step(&data, &mut a));
    assert_eq!(a.fleet.len(), fleet_a - 2);
    assert!(a.ledger.iter().any(|l| l.kind == BookingKind::Sale && l.text.contains("taken over by GoCollective")));
    assert!(a.planning.week.iter().all(|t| t.bus.is_none()), "off the roster");
    b.date = date.into();
    assert!(step(&data, &mut b));
    assert_eq!(b.fleet.len(), fleet_b + 2);
    assert!(b.cash < cash_b && cash_b - b.cash <= o.total() + 2, "paid about their value");
    assert!(!step(&data, &mut b), "once");
    let _ = std::fs::remove_dir_all(&data);
}

#[test]
fn a_no_keeps_the_buses_where_they_are() {
    let data = temp("no");
    let (mut a, mut b) = (company("Umove", "umove"), company("GoCollective", "gocollective"));
    let wd = crate::company::plan::weekday_of(&a.date);
    let x = market::buy_new(&mut a, &MarketBus { name: "Citaro".into(), ..Default::default() }, Payment::Cash, "").unwrap();
    crate::company::plan::set_bus(&mut a, wd, "Linie5", "1", Some(x));
    offer(&data, &mut a, "gocollective", "GoCollective", "Linie5", "5", "2024-03-05");
    step(&data, &mut b);
    let id = b.online.takeover.offers[0].id.clone();
    answer(&data, &mut b, &id, false).unwrap();
    step(&data, &mut a);
    assert!(a.online.takeover.sales.is_empty());
    assert!(a.inbox.messages.iter().any(|m| m.title == REFUSED.0));
    a.date = "2024-03-05".into();
    step(&data, &mut a);
    assert!(a.vehicle(x).is_some(), "still its own");
    let _ = std::fs::remove_dir_all(&data);
}

/// Three drivers of company `c` (hired from its applicants).
fn drivers(c: &mut Company) -> Vec<u32> {
    let mut out = Vec::new();
    while out.len() < 3 {
        let apps: Vec<_> = crate::company::staff::applicants(c).into_iter().filter(|a| a.role == crate::company::model::Role::Driver).collect();
        for a in apps {
            if out.len() < 3 {
                if let Ok(id) = crate::company::staff::hire(c, &a) {
                    out.push(id);
                }
            }
        }
        c.taken = Default::default();
        c.id.push('x');
    }
    c.id.truncate(c.id.trim_end_matches('x').len());
    out
}

/// (NexiconDK, 16 November 2026: "Chaufførerne SKAL følge med - uanset hvad - busserne er
/// valgfrie") The drivers who drove the line most of their days lately - or whom the roster gives
/// it - go with it to the winner whatever it answers (told to the loser at once); the winner says
/// no to the buses. On the new term's first day the drivers leave the loser - off its roster - and
/// join the winner on their terms, with numbers of its own; the loser keeps its buses.
#[test]
fn the_drivers_who_ran_the_line_go_with_it_the_buses_are_a_choice() {
    let data = temp("drivers");
    let (mut a, mut b) = (company("Umove", "umove"), company("GoCollective", "gocollective"));
    let ids = drivers(&mut a);
    let wd = crate::company::plan::weekday_of(&a.date);
    let x = market::buy_new(&mut a, &MarketBus { name: "Citaro".into(), ..Default::default() }, Payment::Cash, "").unwrap();
    crate::company::plan::set_bus(&mut a, wd, "Linie5", "1", Some(x));
    // (the first drove line 5 on four days lately; the second its roster's duties; the third
    // drove line 5 twice and line 7 more)
    let day = |k: i64| crate::company::dates::add("2024-03-04", -k);
    a.online.takeover.crew = vec![
        Crew { line: "Linie5".into(), driver: ids[0], days: (0..4).map(day).collect() },
        Crew { line: "Linie5".into(), driver: ids[2], days: (0..2).map(day).collect() },
        Crew { line: "Linie7".into(), driver: ids[2], days: (0..5).map(day).collect() },
    ];
    crate::company::plan::set_driver(&mut a, wd, "Linie5", "1", 0, Some(Who::Staff(ids[1])));
    assert_eq!(drivers_of_line(&a, "Linie5"), [ids[0], ids[1]]);
    let date = "2024-03-08";
    assert_eq!(offer(&data, &mut a, "gocollective", "GoCollective", "Linie5", "5", date), 3);
    // (the loser knows at once: its drivers go)
    assert_eq!(a.online.takeover.sales[0].drivers, [ids[0], ids[1]]);
    assert!(a.inbox.messages.iter().any(|m| m.title == DRIVERS_TAKEN.0));
    assert!(step(&data, &mut b));
    let o = b.online.takeover.offers[0].clone();
    assert_eq!((o.buses.len(), o.drivers.len()), (1, 2));
    assert!(b.inbox.messages.iter().any(|m| m.title == DRIVERS_OFFERED.0) && b.inbox.messages.iter().any(|m| m.title == OFFERED.0));
    // no to the buses
    answer(&data, &mut b, &o.id, false).unwrap();
    assert!(step(&data, &mut a));
    assert!(a.inbox.messages.iter().any(|m| m.title == REFUSED.0));
    assert!(a.online.takeover.sales[0].buses.is_empty());
    let (staff_a, staff_b) = (a.staff.len(), b.staff.len());
    a.date = date.into();
    assert!(step(&data, &mut a));
    assert_eq!(a.staff.len(), staff_a - 2);
    assert!(a.employee(ids[2]).is_some() && a.employee(ids[0]).is_none() && a.vehicle(x).is_some());
    assert!(a.planning.week.iter().all(|t| !t.duties.contains(&Some(Who::Staff(ids[1])))), "off the roster");
    b.date = date.into();
    assert!(step(&data, &mut b));
    assert_eq!(b.staff.len(), staff_b + 2);
    let names: Vec<&str> = o.drivers.iter().map(|d| d.name.as_str()).collect();
    let came: Vec<&crate::company::model::Employee> = b.staff.iter().filter(|e| names.contains(&e.name.as_str())).collect();
    assert_eq!(came.len(), 2);
    assert!(came.iter().all(|e| e.notice_until.is_none() && b.staff.iter().filter(|x| x.id == e.id).count() == 1));
    assert_eq!(came.iter().map(|e| e.wage).sum::<Cents>(), o.drivers.iter().map(|d| d.wage).sum::<Cents>(), "on their terms");
    assert!(b.inbox.messages.iter().any(|m| m.title == DRIVERS_CAME.0));
    assert!(!step(&data, &mut b), "once");
    let _ = std::fs::remove_dir_all(&data);
}
