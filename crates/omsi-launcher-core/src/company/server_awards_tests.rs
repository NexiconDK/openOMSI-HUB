use super::*;
use crate::company::concessions::tests::line;
use crate::company::model::GameMode;
use crate::company::shared_tenders::{bid, key_of, load, save, step, sync, template, MapNow, CLOSE_AFTER};
use crate::company::{found, Founding};
use crate::LineInfo;

const MAP: &str = "maps/Grundorf/global.cfg";
const DATE: &str = "2024-03-04";

fn temp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("openomsi-server-awards-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

fn company(name: &str, id: &str) -> Company {
    let mut c = found(&Founding { name: name.into(), date: DATE.into(), map: MAP.into(), game_mode: GameMode::Normal, ..Default::default() }, "owner");
    c.id = id.into();
    c.online.on_server = true;
    c
}

fn at(c: &mut Company, at: i64) {
    c.date = clock::date_of(at);
    c.clock.minute = clock::minute_of(at) as u32;
}

/// The market's tenders without the map's operators (the companies alone bid).
fn no_operators(data: &Path, key: &str) {
    let mut m = load(data, key).unwrap();
    for t in m.tenders.iter_mut() {
        t.tender.bidders.clear();
    }
    save(data, &m).unwrap();
}

fn told(c: &Company, text: &str) -> usize {
    c.clock.feed.iter().filter(|f| f.text == text).count()
}

/// H11: a company that bid and lost complains in the days before the start (once); the admin
/// upholds it - the contract goes to it, the winner gives it back before its start - and a
/// complaint of the other let stand is told so. The winner may not complain, nor anyone once the
/// contract has started.
#[test]
fn a_complaint_upheld_gives_the_contract_to_the_next_bidder() {
    let data = temp("complaint");
    let lines: Vec<LineInfo> = vec![line("Linie7", "7", 2, true)];
    let (mut a, mut b) = (company("Umove", "umove"), company("GoCollective", "gocollective"));
    let key = key_of(MAP);
    let now = clock::moment(DATE, 8 * 60);
    let (ca, cb) = (a.clone(), b.clone());
    let mut read = || Some(MapNow { lines: lines.clone(), companies: vec![ca.clone(), cb.clone()], template: template(MAP, DATE, &[]), events: Vec::new() });
    step(&data, &key, MAP, DATE, now, &mut read);
    no_operators(&data, &key);
    sync(&data, &mut a);
    sync(&data, &mut b);
    let t = load(&data, &key).unwrap().tenders[0].tender.clone();
    let mid = t.opens_at + 10;
    at(&mut a, mid);
    at(&mut b, mid);
    let local = |c: &Company| c.concessions.tenders.iter().find(|x| x.market.as_ref().is_some_and(|m| m.id == t.market.as_ref().unwrap().id)).unwrap().id;
    let (ta, tb) = (local(&a), local(&b));
    let low = crate::company::sealed::low_line(&t);
    bid(&data, &mut a, ta, t.reference).unwrap();
    bid(&data, &mut b, tb, low).unwrap();
    step(&data, &key, MAP, DATE, t.closes_at + CLOSE_AFTER, &mut read);
    at(&mut a, t.closes_at + 5);
    at(&mut b, t.closes_at + 5);
    sync(&data, &mut a);
    sync(&data, &mut b);
    assert!(b.concessions.held.iter().any(|h| h.line == "Linie7") && !a.concessions.held.iter().any(|h| h.line == "Linie7"));
    // (the winner may not complain; the loser once, with a reason)
    assert_eq!(complain(&data, &mut b, tb, "no"), Err(NOT_LOST));
    assert_eq!(complain(&data, &mut a, ta, "  "), Err(NO_REASON));
    complain(&data, &mut a, ta, "The winner has no buses for it.").unwrap();
    assert_eq!(complain(&data, &mut a, ta, "again"), Err(COMPLAINED));
    assert_eq!(told(&a, COMPLAINT_SENT), 1);
    // (the main server takes it with its tender)
    step(&data, &key, MAP, DATE, t.closes_at + 10, &mut read);
    let mut m = load(&data, &key).unwrap();
    let mid_id = t.market.as_ref().unwrap().id;
    assert_eq!(m.tenders[0].complaints.len(), 1);
    assert_eq!(next_of(m.tenders[0].decided.as_ref().unwrap()).as_deref(), Some("Umove"));
    // (upheld: Umove wins, GoCollective is passed over)
    let said = rule(&data, &mut m, mid_id, "umove", true).unwrap();
    assert!(said.contains("Umove"), "{said}");
    let d = m.tenders[0].decided.clone().unwrap();
    assert_eq!((d.winner.as_str(), d.revision, d.passed.clone()), ("umove", 1, vec!["GoCollective".to_string()]));
    assert_eq!(rule(&data, &mut m, mid_id, "umove", true).unwrap_err(), "no open complaint of that company");
    save(&data, &m).unwrap();
    sync(&data, &mut a);
    sync(&data, &mut b);
    assert!(a.concessions.held.iter().any(|h| h.line == "Linie7") && a.concessions.contracts.iter().any(|k| k.line == "Linie7"), "Umove has it");
    assert!(!b.concessions.held.iter().any(|h| h.line == "Linie7") && !b.concessions.contracts.iter().any(|k| k.line == "Linie7"), "GoCollective gave it back");
    assert_eq!((told(&a, UPHELD_WON), told(&b, UPHELD_LOST)), (1, 1));
    // (taken once)
    sync(&data, &mut a);
    assert_eq!(told(&a, UPHELD_WON), 1);
    // (GoCollective complains in turn: let stand, it is told so)
    complain(&data, &mut b, tb, "We were cheaper.").unwrap();
    step(&data, &key, MAP, DATE, t.closes_at + 20, &mut read);
    let mut m = load(&data, &key).unwrap();
    rule(&data, &mut m, mid_id, "gocollective", false).unwrap();
    save(&data, &m).unwrap();
    sync(&data, &mut b);
    sync(&data, &mut b);
    assert_eq!(told(&b, STOOD), 1);
    // (none once the contract has started)
    let start = start_of(a.concessions.tenders.iter().find(|x| x.id == ta).unwrap());
    b.date = start;
    let tb_t = b.concessions.tenders.iter().find(|x| x.id == tb).unwrap().clone();
    b.concessions.events_told.clear();
    assert_eq!(may_complain(&b, &tb_t), Err(TOO_LATE));
    let _ = std::fs::remove_dir_all(&data);
}

/// H9: a map's timetable for rail replacement is the admin's to order (none on a map without):
/// out to tender for a day at his fixed price a timetable hour, every valid bid at it and the
/// quality deciding, the winner's contract its days alone; or given directly.
#[test]
fn rail_replacement_only_where_the_map_has_it() {
    let data = temp("rail");
    let key = key_of(MAP);
    let now = clock::moment(DATE, 8 * 60);
    let plain: Vec<LineInfo> = vec![line("Linie7", "7", 2, true)];
    let mut read = || Some(MapNow { lines: plain.clone(), companies: Vec::new(), template: template(MAP, DATE, &[]), events: Vec::new() });
    step(&data, &key, MAP, DATE, now, &mut read);
    let mut m = load(&data, &key).unwrap();
    assert!(m.rail_lines.is_empty());
    assert_eq!(order_rail(&mut m, "SEV", "2024-03-07", "2024-03-08", 900_00, None, DATE).unwrap_err(), NO_RAIL);
    // (a map with one)
    let lines: Vec<LineInfo> = vec![line("Linie7", "7", 2, true), line("SEV", "SEV", 2, true)];
    let mut a = company("Umove", "umove");
    let ca = a.clone();
    let mut read = || Some(MapNow { lines: lines.clone(), companies: vec![ca.clone()], template: template(MAP, DATE, &[]), events: Vec::new() });
    let tomorrow = dates::add(DATE, 1);
    step(&data, &key, MAP, &tomorrow, now + clock::DAY, &mut read);
    let mut m = load(&data, &key).unwrap();
    assert_eq!(m.rail_lines, vec!["SEV".to_string()]);
    assert!(!m.tenders.iter().any(|t| t.tender.line == "SEV"), "never a concession of its own");
    assert!(order_rail(&mut m, "SEV", &dates::add(&tomorrow, 1), &dates::add(&tomorrow, 2), 900_00, None, &tomorrow).is_err(), "a day of bids, a day to make ready");
    let (from, until) = (dates::add(&tomorrow, 3), dates::add(&tomorrow, 4));
    order_rail(&mut m, "sev", &from, &until, 900_00, None, &tomorrow).unwrap();
    assert!(order_rail(&mut m, "SEV", &until, &until, 900_00, None, &tomorrow).is_err(), "overlapping");
    save(&data, &m).unwrap();
    let mut m2 = load(&data, &key).unwrap();
    m2.day = String::new();
    save(&data, &m2).unwrap();
    let now2 = now + clock::DAY + 60;
    step(&data, &key, MAP, &tomorrow, now2, &mut read);
    let m = load(&data, &key).unwrap();
    let t = m.tenders.iter().find(|t| t.tender.line == "SEV").expect("out to tender").tender.clone();
    let x = t.market.clone().unwrap();
    assert_eq!((x.event.as_str(), x.from.as_str(), x.until.as_str(), x.award), (RAIL_EVENT, from.as_str(), until.as_str(), crate::company::viborg::Award::Fixed));
    assert!(t.closes_at <= t.opens_at + RAIL_BID_DAYS * clock::DAY);
    assert_eq!(t.reference, cn::of_hour(&t, 900_00.0), "the fixed price");
    // (a bid is at the fixed price, whatever it asks)
    no_operators(&data, &key);
    sync(&data, &mut a);
    at(&mut a, t.opens_at + 5);
    let lid = a.concessions.tenders.iter().find(|y| y.line == "SEV").unwrap().id;
    bid(&data, &mut a, lid, t.reference / 2).unwrap();
    let bids = crate::company::shared_tenders::bids(&data, &key);
    assert_eq!(bids.last().map(|b| b.amount), Some(t.reference));
    step(&data, &key, MAP, &tomorrow, t.closes_at + CLOSE_AFTER, &mut read);
    let d = load(&data, &key).unwrap().tenders.iter().find(|y| y.tender.line == "SEV").unwrap().decided.clone().unwrap();
    assert_eq!(d.winner, "umove");
    at(&mut a, t.closes_at + 5);
    sync(&data, &mut a);
    let h = a.concessions.held.iter().find(|h| h.line == "SEV").expect("held");
    assert_eq!((h.from.as_str(), h.until.as_str()), (from.as_str(), until.as_str()), "its days alone");
    let _ = std::fs::remove_dir_all(&data);
}
