use super::*;
use crate::company::concessions::tests::line;
use crate::company::{found, network, Founding};

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("openomsi-shared-tenders-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

const MAP: &str = "maps/Grundorf/global.cfg";
const DATE: &str = "2024-03-04";

/// A company of the server on the map.
fn company(name: &str, id: &str) -> Company {
    let mut c = found(&Founding { name: name.into(), date: DATE.into(), map: MAP.into(), game_mode: GameMode::Normal, ..Default::default() }, "owner");
    c.id = id.into();
    c.online.on_server = true;
    c
}

/// The company's clock at minute `at` (`clock::moment`).
fn at(c: &mut Company, at: i64) {
    c.date = clock::date_of(at);
    c.clock.minute = clock::minute_of(at) as u32;
}

fn lines() -> Vec<LineInfo> {
    vec![line("Linie5", "5", 3, true), line("Linie7", "7", 2, true), line("oo_12", "12", 1, true), line("Linie9", "9", 4, true)]
}

/// Two companies of a map bid in the same tender of the map's market and see each other's bids
/// live; the main server decides it the same for both a minute after its close - the one won,
/// the other lost to it by name; a line one of them runs is not offered, and an application
/// for it is refused; their own tenders of before are withdrawn, the fee paid back.
#[test]
fn two_companies_bid_against_each_other_in_the_maps_tenders() {
    let data = temp("two");
    let lines = lines();
    let (mut a, mut b) = (company("Umove", "umove"), company("GoCollective", "gocollective"));
    network::add_line(&mut a, &lines[0], None).unwrap();
    // (a tender of B's own from before, its fee paid)
    cn::apply(&mut b, &lines[1]).unwrap();
    let own = b.concessions.tenders[0].id;
    let now = clock::moment(DATE, 8 * 60);
    let opens = b.concessions.tenders[0].opens_at;
    at(&mut b, opens + 5);
    let least = cn::min_bid(&b, &b.concessions.tenders[0]);
    cn::bid(&mut b, own, least).unwrap();
    let cash = b.cash;
    let key = key_of(MAP);
    let (ca, cb) = (a.clone(), b.clone());
    let mut read = || Some(MapNow { lines: lines.clone(), companies: vec![ca.clone(), cb.clone()], template: template(MAP, DATE) });
    let said = step(&data, &key, MAP, DATE, now, &mut read);
    let m = load(&data, &key).unwrap();
    // (Linie5 is A's, oo_12 a line of the player's own: Linie7 and Linie9 out to tender)
    let mut offered: Vec<&str> = m.tenders.iter().map(|t| t.tender.line.as_str()).collect();
    offered.sort();
    assert_eq!(offered, ["Linie7", "Linie9"], "{said:?}");
    assert!(m.tenders.iter().all(|t| t.tender.market.as_ref().is_some_and(|x| x.key == key && !x.rivals.is_empty())));
    // both take the market's tenders among their own; B's own of before goes, its fee back
    assert_eq!(sync(&data, &mut a), (true, false));
    assert!(sync(&data, &mut b).0);
    assert!(b.cash > cash, "the fee paid back");
    assert!(!b.concessions.tenders.iter().any(|t| t.id == own));
    assert_eq!((a.concessions.tenders.len(), b.concessions.tenders.len()), (2, 2));
    // (a close the server's operator moves in the market's file: the companies follow it)
    let mut moved = m.clone();
    let last = moved.tenders.iter_mut().max_by_key(|t| t.tender.opens_at).unwrap();
    let (mid_of, was) = (last.tender.market.as_ref().unwrap().id, last.tender.closes_at);
    last.tender.closes_at = was + 60;
    save(&data, &moved).unwrap();
    sync(&data, &mut a);
    let close_of = |c: &Company| c.concessions.tenders.iter().find(|t| t.market.as_ref().is_some_and(|x| x.id == mid_of)).unwrap().closes_at;
    assert_eq!(close_of(&a), was + 60);
    save(&data, &m).unwrap();
    sync(&data, &mut a);
    assert_eq!(close_of(&a), was);
    let first = m.tenders.iter().min_by_key(|t| t.tender.opens_at).unwrap().tender.clone();
    // the auction: A bids, B sees it and bids lower
    let mid = first.opens_at + 10;
    at(&mut a, mid);
    at(&mut b, mid);
    let ta = a.concessions.tenders.iter().find(|t| t.market.as_ref().unwrap().id == first.market.as_ref().unwrap().id).unwrap().id;
    let tb = b.concessions.tenders.iter().find(|t| t.market.as_ref().unwrap().id == first.market.as_ref().unwrap().id).unwrap().id;
    let ask_a = cn::min_bid(&a, a.concessions.tenders.iter().find(|t| t.id == ta).unwrap());
    bid(&data, &mut a, ta, ask_a).unwrap();
    sync(&data, &mut b);
    let t = b.concessions.tenders.iter().find(|t| t.id == tb).unwrap().clone();
    let placed = cn::bids(&b, &t, mid);
    let theirs = placed.iter().find(|p| matches!(p.who, Who::Other(_)) && p.amount == ask_a).expect("B sees A's bid");
    assert_eq!(cn::bidder_name(&b, &t, theirs.who), "Umove");
    assert!(cn::min_bid(&b, &t) < ask_a, "B has to ask less to lead");
    // (B goes low: below what any operator goes)
    let low = (t.cost / 2 / auction::ROUND) * auction::ROUND;
    at(&mut b, mid + 1);
    bid(&data, &mut b, tb, low).unwrap();
    sync(&data, &mut a);
    let t = a.concessions.tenders.iter().find(|t| t.id == ta).unwrap().clone();
    assert_eq!(cn::bids(&a, &t, mid + 1).last().map(|p| (p.who, p.amount)), Some((Who::Other(0), low)), "A sees B lead (gocollective comes first in their order)");
    // (A's clock past the close: it does not decide the tender itself)
    at(&mut a, first.closes_at + 5);
    assert!(cn::close(&mut a, ta).is_none());
    // the main server decides it a minute after its close, the same for both
    step(&data, &key, MAP, DATE, first.closes_at, &mut read);
    assert!(load(&data, &key).unwrap().tenders.iter().all(|t| t.decided.is_none()), "not before its last minute is over");
    step(&data, &key, MAP, DATE, first.closes_at + CLOSE_AFTER, &mut read);
    let d = load(&data, &key).unwrap().tenders.iter().find(|t| t.tender.id == first.market.as_ref().unwrap().id).unwrap().decided.clone().unwrap();
    assert_eq!((d.winner.as_str(), d.name.as_str(), d.amount), ("gocollective", "GoCollective", low));
    at(&mut b, first.closes_at + 5);
    assert_eq!(sync(&data, &mut b), (true, true));
    assert!(matches!(b.concessions.tenders.iter().find(|t| t.id == tb).unwrap().outcome, Some(cn::Outcome::Won { amount, .. }) if amount == low));
    let h = cn::of_line(&b, &first.line).expect("B holds the line");
    assert_eq!(dates::between(&h.from, &h.until), cn::SERVER_DIESEL_DAYS - 1, "two weeks with diesel buses");
    // (by the timetable hour: the cap 1 500 kr an hour, the contract paid its ask's hour)
    let cap_hour = cn::per_hour(&first, first.reference);
    assert!((cap_hour - crate::company::server_fines::Fines::cents(1500.0) as f64).abs() < 1.0, "{cap_hour}");
    let k = b.concessions.contracts.iter().find(|k| k.line == first.line).unwrap();
    assert_eq!(k.per_hour, cn::per_hour(&first, low).round() as Cents);
    sync(&data, &mut a);
    match &a.concessions.tenders.iter().find(|t| t.id == ta).unwrap().outcome {
        Some(cn::Outcome::Lost { winner, amount, .. }) => assert_eq!((winner.as_str(), *amount), ("GoCollective", low)),
        o => panic!("{o:?}"),
    }
    assert!(cn::of_line(&a, &first.line).is_none());
    // no buying outright on the server
    let other = a.concessions.tenders.iter().find(|t| t.open()).unwrap().id;
    assert_eq!(cn::buy_out(&mut a, other), Err(cn::NOT_ON_SERVER));
    // (the open tenders' weeks read by the main server - not again the day a day was not read -
    // their tours by the day on the companies' pages)
    let mut failing = |_: &str| None;
    assert!(fill_weeks(&data, &key, DATE, &mut failing));
    assert!(!fill_weeks(&data, &key, DATE, &mut failing), "not again that day");
    let mut week = |day: &str| Some(if dates::weekday(dates::parse(day).unwrap()) == 6 { Vec::new() } else { lines.clone() });
    assert!(fill_weeks(&data, &key, "2024-03-05", &mut week));
    let open = load(&data, &key).unwrap().tenders.iter().find(|t| t.decided.is_none()).unwrap().tender.clone();
    let w = open.week.expect("its week");
    assert_eq!((w.by_day, w.sun), (true, 0));
    assert!(w.mon_fri > 0 && w.sat == w.mon_fri);
    sync(&data, &mut a);
    assert_eq!(a.concessions.tenders.iter().find(|t| t.market.as_ref().is_some_and(|x| x.id == open.id)).unwrap().week, Some(w));
    let _ = std::fs::remove_dir_all(&data);
}

/// A company's concession ending soon goes out again in the map's market, open to every company:
/// won by another, its line is the winner's from the day the old term ends; an application for a
/// line another company runs is refused, one for a free line opens its tender at once. (Decided
/// 25 October 2026: the renewal opens six days before the new term and is decided three days
/// before it; won with an electric bid, the term is a month.)
#[test]
fn a_concession_ending_goes_out_to_every_company() {
    let data = temp("renewal");
    let lines = lines();
    let (mut a, mut b) = (company("Umove", "umove"), company("GoCollective", "gocollective"));
    network::add_line(&mut a, &lines[0], None).unwrap();
    let until = dates::add(DATE, 5);
    a.concessions.held.retain(|h| h.line != "Linie5");
    a.concessions.held.push(cn::Concession { line: "Linie5".into(), number: "5".into(), from: DATE.into(), until: until.clone(), price: 1.0, direct: false });
    let key = key_of(MAP);
    let (ca, cb) = (a.clone(), b.clone());
    let mut read = || Some(MapNow { lines: lines.clone(), companies: vec![ca.clone(), cb.clone()], template: template(MAP, DATE) });
    // (applications: B for A's line - refused -, B for Linie9)
    assert_eq!(ask(&data, &b, &lines[0]), Ok(ASKED), "B does not know A's plan here: the market refuses it");
    assert_eq!(ask(&data, &b, &lines[3]), Ok(ASKED));
    let now = clock::moment(DATE, 8 * 60);
    let said = step(&data, &key, MAP, DATE, now, &mut read);
    let m = load(&data, &key).unwrap();
    assert!(said.iter().any(|s| s.contains("Umove runs it")), "{said:?}");
    let renewal = m.tenders.iter().find(|t| t.tender.line == "Linie5").expect("the renewal").tender.clone();
    let x = renewal.market.as_ref().unwrap();
    assert!(renewal.renewal && x.holder == "umove" && x.from == dates::add(&until, 1));
    assert_eq!(clock::date_of(renewal.closes_at), dates::add(&until, 1 - RENEW_DECIDED), "decided three days before the new term");
    assert!(renewal.closes_at - renewal.opens_at > 2 * clock::DAY, "{renewal:?}");
    let applied = m.tenders.iter().find(|t| t.tender.line == "Linie9").unwrap();
    assert_eq!((applied.tender.opens_at, applied.applied.clone()), (now, vec!["gocollective".to_string()]), "opens at once");
    // (asked once: the next step offers nothing more)
    let n = m.tenders.len();
    step(&data, &key, MAP, DATE, now + 1, &mut read);
    assert_eq!(load(&data, &key).unwrap().tenders.len(), n);
    // B wins the renewal of A's line
    sync(&data, &mut a);
    sync(&data, &mut b);
    assert!(a.concessions.tenders.iter().find(|t| t.line == "Linie5").unwrap().renewal, "A's renewal");
    assert!(!b.concessions.tenders.iter().find(|t| t.line == "Linie5").unwrap().renewal);
    assert!(b.concessions.tenders.iter().find(|t| t.line == "Linie9").unwrap().applied);
    let tb = b.concessions.tenders.iter().find(|t| t.line == "Linie5").unwrap().clone();
    at(&mut b, tb.opens_at + 3);
    let low = (tb.cost / 2 / auction::ROUND) * auction::ROUND;
    // (with electric buses: where its level opens them)
    assert_eq!(cn::set_drive(&mut b, tb.id, Drive::Electric), Err(cn::NO_ELECTRIC));
    b.progress.xp = crate::company::levels::LEVEL_XP[2];
    cn::set_drive(&mut b, tb.id, Drive::Electric).unwrap();
    bid(&data, &mut b, tb.id, low).unwrap();
    step(&data, &key, MAP, &clock::date_of(tb.closes_at), tb.closes_at + CLOSE_AFTER, &mut read);
    at(&mut b, tb.closes_at + 2);
    assert_eq!(sync(&data, &mut b), (true, true));
    let h = cn::of_line(&b, "Linie5").unwrap();
    let from = dates::add(&until, 1);
    assert_eq!((h.from.clone(), h.until.clone()), (from.clone(), dates::add(&dates::add_month(&from), -1)), "from the day A's term ends, a month");
    let k = b.concessions.contracts.iter().find(|k| k.line == "Linie5").unwrap();
    assert!(k.electric && k.extensions_left == 0);
    at(&mut a, tb.closes_at + 2);
    sync(&data, &mut a);
    // (A did not bid on its own renewal: it went to B)
    assert!(matches!(&a.concessions.tenders.iter().find(|t| t.line == "Linie5").unwrap().outcome, Some(cn::Outcome::NoBid { winner, .. }) if winner == "GoCollective"));
    assert!(cn::of_line(&a, "Linie5").is_some(), "A runs it until its term ends");
    let _ = std::fs::remove_dir_all(&data);
}

/// A Casual company bids in the map's tenders as every company does (decided 25 October 2026: its
/// Concessions page on the server): its application opens the tender, and it bids nothing by
/// itself.
#[test]
fn an_easy_company_applies_and_bids_itself() {
    let data = temp("easy");
    let lines = lines();
    let mut e = company("Easy Bus", "easy");
    e.game_mode = GameMode::Casual;
    let key = key_of(MAP);
    assert_eq!(ask(&data, &e, &lines[1]), Ok(ASKED));
    let ce = e.clone();
    let mut read = || Some(MapNow { lines: lines.clone(), companies: vec![ce.clone()], template: template(MAP, DATE) });
    let now = clock::moment(DATE, 8 * 60);
    step(&data, &key, MAP, DATE, now, &mut read);
    at(&mut e, now + 2);
    sync(&data, &mut e);
    let t = e.concessions.tenders.iter().find(|t| t.line == "Linie7").unwrap().clone();
    assert!(t.applied && t.running(now + 2) && t.offers.is_empty(), "{t:?}");
    assert!(bids(&data, &key).is_empty(), "nothing bid by itself");
    let ask_e = cn::min_bid(&e, &t);
    bid(&data, &mut e, t.id, ask_e).unwrap();
    assert_eq!(bids(&data, &key).iter().filter(|b| b.company == "easy").count(), 1);
    let _ = std::fs::remove_dir_all(&data);
}
