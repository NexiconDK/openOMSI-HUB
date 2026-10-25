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
    assert_eq!(offer(&data, &a, "gocollective", "Linie5", "5", date), 2);
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
    offer(&data, &a, "gocollective", "Linie5", "5", "2024-03-05");
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
