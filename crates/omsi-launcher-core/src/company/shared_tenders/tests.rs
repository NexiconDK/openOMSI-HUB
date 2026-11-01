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

/// The map's operators made dull (quality 60) in the market and its tenders: a company's valid
/// bid at the abnormally low line wins against any of theirs (their asks are above it).
fn dull_rivals(data: &Path, key: &str) {
    let mut m = load(data, key).unwrap();
    for r in m.rivals.iter_mut() {
        r.quality = 60.0;
    }
    for t in m.tenders.iter_mut() {
        if let Some(x) = t.tender.market.as_mut() {
            for r in x.rivals.iter_mut() {
                r.quality = 60.0;
            }
        }
    }
    save(data, &m).unwrap();
}

fn lines() -> Vec<LineInfo> {
    vec![line("Linie5", "5", 3, true), line("Linie7", "7", 2, true), line("oo_12", "12", 1, true), line("Linie9", "9", 4, true)]
}

/// Two companies of a map bid in the same tender of the map's market - sealed (31 October 2026):
/// neither sees the other's bid, an abnormally low one is refused; the main server opens every
/// bid a minute after its close and decides it the same for both - the one won, the other lost to
/// it by name, every bid then shown with its points; a line one of them runs is not offered, and
/// an application for it is refused; their own tenders of before are withdrawn, the fee paid back.
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
    let mut read = || Some(MapNow { lines: lines.clone(), companies: vec![ca.clone(), cb.clone()], template: template(MAP, DATE, &[]), events: Vec::new() });
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
    dull_rivals(&data, &key);
    sync(&data, &mut a);
    sync(&data, &mut b);
    let m = load(&data, &key).unwrap();
    let first = m.tenders.iter().min_by_key(|t| t.tender.opens_at).unwrap().tender.clone();
    // (the cap: the cost price by half again, at most the server's 1 500 kr an hour)
    let most = cn::of_hour(&first, crate::company::server_fines::Fines::cents(1500.0) as f64);
    assert_eq!(first.reference, crate::company::sealed::cap_of(first.cost, most));
    // sealed: A bids, B does not see it
    let mid = first.opens_at + 10;
    at(&mut a, mid);
    at(&mut b, mid);
    let ta = a.concessions.tenders.iter().find(|t| t.market.as_ref().unwrap().id == first.market.as_ref().unwrap().id).unwrap().id;
    let tb = b.concessions.tenders.iter().find(|t| t.market.as_ref().unwrap().id == first.market.as_ref().unwrap().id).unwrap().id;
    let ask_a = cn::min_bid(&a, a.concessions.tenders.iter().find(|t| t.id == ta).unwrap());
    assert_eq!(ask_a, first.reference, "anything up to the cap");
    bid(&data, &mut a, ta, ask_a).unwrap();
    sync(&data, &mut b);
    let t = b.concessions.tenders.iter().find(|t| t.id == tb).unwrap().clone();
    assert!(cn::bids(&b, &t, mid).is_empty() && t.market.as_ref().unwrap().others.is_empty(), "B sees nothing of A's");
    // (B goes too low: refused; then at the line, the least valid)
    let line = crate::company::sealed::low_line(&t);
    at(&mut b, mid + 1);
    assert_eq!(bid(&data, &mut b, tb, line - crate::company::auction::ROUND), Err(cn::ABNORMALLY_LOW));
    bid(&data, &mut b, tb, line).unwrap();
    let low = line;
    sync(&data, &mut a);
    let t = a.concessions.tenders.iter().find(|t| t.id == ta).unwrap().clone();
    assert_eq!(cn::bids(&a, &t, mid + 1).iter().map(|p| p.amount).collect::<Vec<_>>(), vec![ask_a], "A sees its own alone");
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
    // (every bid opened: both companies' and the operators', best first)
    assert!(d.standings.len() >= 2 && d.standings[0].name == "GoCollective" && d.standings.iter().any(|s| s.name == "Umove"), "{:?}", d.standings);
    let k = b.concessions.contracts.iter().find(|k| k.line == first.line).unwrap();
    assert_eq!(k.per_hour, cn::per_hour(&first, low).round() as Cents);
    sync(&data, &mut a);
    match &a.concessions.tenders.iter().find(|t| t.id == ta).unwrap().outcome {
        Some(cn::Outcome::Lost { winner, amount, .. }) => assert_eq!((winner.as_str(), *amount), ("GoCollective", low)),
        o => panic!("{o:?}"),
    }
    let t = a.concessions.tenders.iter().find(|t| t.id == ta).unwrap();
    assert!(t.standings.iter().any(|s| s.name == "GoCollective") && t.market.as_ref().unwrap().others.len() == 1, "opened: B's bid shown");
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
    a.concessions.held.push(cn::Concession { line: "Linie5".into(), number: "5".into(), from: DATE.into(), until: until.clone(), price: 1.0, direct: false, after: String::new() });
    let key = key_of(MAP);
    let (ca, cb) = (a.clone(), b.clone());
    let mut read = || Some(MapNow { lines: lines.clone(), companies: vec![ca.clone(), cb.clone()], template: template(MAP, DATE, &[]), events: Vec::new() });
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
    dull_rivals(&data, &key);
    sync(&data, &mut b);
    let tb = b.concessions.tenders.iter().find(|t| t.line == "Linie5").unwrap().clone();
    at(&mut b, tb.opens_at + 3);
    let low = crate::company::sealed::low_line(&tb);
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
    assert_eq!(h.after, "umove", "A runs it until then");
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
    let mut read = || Some(MapNow { lines: lines.clone(), companies: vec![ce.clone()], template: template(MAP, DATE, &[]), events: Vec::new() });
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

/// (31 October 2026) A line one of the map's operators won is its for a term: not offered again
/// meanwhile, an application for it refused; given up early (it bid below its own cost) it goes
/// out to tender at once, saying so; its term ending, it goes out again as a company's does.
#[test]
fn an_operator_holds_a_line_it_won_and_may_give_it_up() {
    let data = temp("operator");
    let lines = lines();
    let a = company("Umove", "umove");
    let key = key_of(MAP);
    let ca = a.clone();
    let mut read = || Some(MapNow { lines: lines.clone(), companies: vec![ca.clone()], template: template(MAP, DATE, &[]), events: Vec::new() });
    let now = clock::moment(DATE, 8 * 60);
    step(&data, &key, MAP, DATE, now, &mut read);
    // (nobody of the server bids: the operators' sealed bids decide)
    let m = load(&data, &key).unwrap();
    let t = m.tenders.iter().find(|t| !crate::company::sealed::operator_bids(&t.tender).is_empty()).expect("an operator bids").tender.clone();
    let closed = clock::date_of(t.closes_at);
    step(&data, &key, MAP, &closed, t.closes_at + CLOSE_AFTER, &mut read);
    let m = load(&data, &key).unwrap();
    let d = m.tenders.iter().find(|x| x.tender.id == t.id).unwrap().decided.clone().unwrap();
    assert!(d.winner.is_empty() && !d.name.is_empty() && d.own_cost > 0, "{d:?}");
    let h = m.held.iter().find(|h| h.line == t.line).expect("the operator holds it").clone();
    assert_eq!((h.from.clone(), h.name.clone()), (dates::add(&closed, 1), d.name.clone()));
    assert_eq!(dates::between(&h.from, &h.until), cn::SERVER_DIESEL_DAYS - 1);
    // (an application for it: refused)
    let l = lines.iter().find(|l| l.name == t.line).unwrap();
    assert_eq!(ask(&data, &a, l), Err(OPERATOR_HELD));
    // given up early: out to tender at once, the operator named
    let mut m = load(&data, &key).unwrap();
    m.held.iter_mut().find(|x| x.line == t.line).unwrap().gives_up = dates::add(&h.from, 3);
    save(&data, &m).unwrap();
    let day = dates::add(&h.from, 3);
    let said = step(&data, &key, MAP, &day, clock::moment(&day, 8 * 60), &mut read);
    let m = load(&data, &key).unwrap();
    assert!(said.iter().any(|s| s.contains("gives line") && s.contains("out to tender at once")), "{said:?}");
    assert!(!m.held.iter().any(|x| x.line == t.line));
    let again = m.tenders.iter().find(|x| x.decided.is_none() && x.tender.line == t.line).expect("out again").tender.clone();
    assert_eq!(again.market.as_ref().unwrap().failed, h.name);
    // its term ending: out again as a renewal, from the day after
    let mut m = load(&data, &key).unwrap();
    m.tenders.retain(|x| x.tender.line != t.line);
    m.held.push(OperatorHold { gives_up: String::new(), ..h.clone() });
    save(&data, &m).unwrap();
    let day = dates::add(&h.until, -RENEW_OPENS);
    step(&data, &key, MAP, &day, clock::moment(&day, 8 * 60), &mut read);
    let m = load(&data, &key).unwrap();
    let renewal = m.tenders.iter().find(|x| x.decided.is_none() && x.tender.line == t.line).expect("the renewal").tender.clone();
    let x = renewal.market.as_ref().unwrap();
    assert!(renewal.renewal && x.holder.is_empty() && x.from == dates::add(&h.until, 1), "{renewal:?}");
    let _ = std::fs::remove_dir_all(&data);
}

/// (31 October 2026) The market's work is priced in the country most of the map's companies play
/// in (a map whose country the game cannot tell priced a UK operator's work); none: the map's.
#[test]
fn the_market_prices_in_the_companies_country() {
    let mut dk = company("Umove", "umove");
    dk.country = "dk".into();
    let plain = template(MAP, DATE, &[]);
    let theirs = template(MAP, DATE, &[dk.clone(), dk]);
    assert_eq!(theirs.country, "dk");
    let l = &lines()[0];
    let cost = |c: &Company| cn::make_tender(c, l, 1, 0, false, "k", &[], (0, 0)).cost;
    assert!(cost(&theirs) > cost(&plain), "{} {}", cost(&theirs), cost(&plain));
}

/// (1 November 2026) A chrono event of the map that brings a line of its own - a match day's
/// shuttles - puts it out to tender a week before, from six days before its day to three days
/// before; never in the rounds, nor on an application. Won, the company holds it for the event's
/// day alone, and it is not renewed after it. A company whose line the event takes off is told
/// once, in time.
#[test]
fn a_chrono_events_line_goes_out_to_tender_for_its_days() {
    let data = temp("event");
    let shuttle = line("Sonderverkehr", "S", 2, true);
    let mut lines = lines();
    // (today an event day too: its line is among the map's lines of the day)
    lines.push(shuttle.clone());
    let (mut a, mut b) = (company("Umove", "umove"), company("GoCollective", "gocollective"));
    network::add_line(&mut a, &lines[0], None).unwrap();
    let key = key_of(MAP);
    // (DATE a Monday: the match a week from Tuesday)
    let event = MapEvent { name: "FCA Heimspiel".into(), from: "2024-03-12".into(), until: "2024-03-12".into(), own: vec!["Sonderverkehr".into()], off: vec!["Linie5".into()], lines: vec![shuttle.clone()] };
    let (ca, cb) = (a.clone(), b.clone());
    let mut read = || Some(MapNow { lines: lines.clone(), companies: vec![ca.clone(), cb.clone()], template: template(MAP, DATE, &[]), events: vec![event.clone()] });
    let said = step(&data, &key, MAP, DATE, clock::moment(DATE, 8 * 60), &mut read);
    let m = load(&data, &key).unwrap();
    assert!(!m.tenders.iter().any(|t| t.tender.line == "Sonderverkehr"), "not in the round, nor eight days before: {said:?}");
    assert_eq!(m.events.len(), 1);
    // (an application: refused)
    assert_eq!(ask(&data, &b, &shuttle), Err(EVENT_LINE));
    // a week before: out to tender for its day
    let day = "2024-03-05";
    let said = step(&data, &key, MAP, day, clock::moment(day, 8 * 60), &mut read);
    let m = load(&data, &key).unwrap();
    let t = m.tenders.iter().find(|t| t.tender.line == "Sonderverkehr").unwrap_or_else(|| panic!("{said:?}")).tender.clone();
    let x = t.market.as_ref().unwrap();
    assert_eq!((x.event.as_str(), x.from.as_str(), x.until.as_str()), ("FCA Heimspiel", "2024-03-12", "2024-03-12"));
    assert!(!t.renewal && x.holder.is_empty());
    assert_eq!((clock::date_of(t.opens_at), clock::date_of(t.closes_at)), ("2024-03-06".to_string(), "2024-03-09".to_string()));
    assert!(t.week.is_some_and(|w| w.tours == 2), "{:?}", t.week);
    // (A told its line 5 does not run that day - once)
    at(&mut a, clock::moment(day, 9 * 60));
    sync(&data, &mut a);
    let told = |c: &Company| c.clock.feed.iter().filter(|f| f.text == EVENT_OFF_DAY).count();
    assert_eq!(told(&a), 1);
    sync(&data, &mut a);
    assert_eq!(told(&a), 1, "told once");
    // B bids and wins it; the line is B's for the match day alone
    dull_rivals(&data, &key);
    at(&mut b, t.opens_at + 10);
    sync(&data, &mut b);
    let tb = b.concessions.tenders.iter().find(|x| x.line == "Sonderverkehr").unwrap().clone();
    bid(&data, &mut b, tb.id, crate::company::sealed::low_line(&tb)).unwrap();
    let closed = clock::date_of(t.closes_at);
    step(&data, &key, MAP, &closed, t.closes_at + CLOSE_AFTER, &mut read);
    at(&mut b, t.closes_at + 5);
    assert_eq!(sync(&data, &mut b), (true, true));
    let h = cn::of_line(&b, "Sonderverkehr").expect("B holds it").clone();
    assert_eq!((h.from.as_str(), h.until.as_str()), ("2024-03-12", "2024-03-12"));
    assert!(b.concessions.contracts.iter().any(|k| k.line == "Sonderverkehr" && k.per_hour > 0));
    // the days after: no renewal of it
    let (ca, cb) = (a.clone(), b.clone());
    let mut read = || Some(MapNow { lines: lines.clone(), companies: vec![ca.clone(), cb.clone()], template: template(MAP, DATE, &[]), events: vec![event.clone()] });
    for day in ["2024-03-07", "2024-03-10", "2024-03-12"] {
        step(&data, &key, MAP, day, clock::moment(day, 8 * 60), &mut read);
    }
    let m = load(&data, &key).unwrap();
    assert_eq!(m.tenders.iter().filter(|t| t.tender.line == "Sonderverkehr").count(), 1, "not out again");
    let _ = std::fs::remove_dir_all(&data);
}

/// (1 November 2026) The map's chrono events as the main server reads them: an event's own lines
/// are those of its TTData the map's own has not (a line of the map it replaces is the map's), an
/// open-ended change and an event over are none, and an event's own lines are read from the
/// timetable of its first day once it is a week off.
#[test]
fn the_maps_chrono_events_and_their_own_lines() {
    let map = temp("events-map");
    let ttl = "[newtour]\r\n1\r\nBus\r\n1023\r\n\r\n";
    std::fs::create_dir_all(map.join("TTData")).unwrap();
    std::fs::write(map.join("TTData/Linie5.ttl"), ttl).unwrap();
    for (folder, cfg, lines) in [
        ("1_FCA/12.03.24", "[name]\r\nFCA Heimspiel\r\n[startdate]\r\n20240312\r\n[enddate]\r\n20240312\r\n[deactivate_lines]\r\n1\r\nLinie7\r\n", vec!["Sonderverkehr", "Linie5"]),
        ("1_FCA/03.02.24", "[name]\r\nFCA alt\r\n[startdate]\r\n20240203\r\n[enddate]\r\n20240203\r\n", vec!["Sonderverkehr"]),
        ("Fahrplanwechsel", "[startdate]\r\n20240101\r\n", vec!["Linie9"]),
    ] {
        let d = map.join("Chrono").join(folder);
        std::fs::create_dir_all(d.join("TTData")).unwrap();
        std::fs::write(d.join("Chrono.cfg"), cfg).unwrap();
        for l in lines {
            std::fs::write(d.join("TTData").join(format!("{l}.ttl")), ttl).unwrap();
        }
    }
    let asked = std::cell::RefCell::new(Vec::<String>::new());
    let shuttle = line("Sonderverkehr", "S", 2, true);
    let mut read = |day: &str| {
        asked.borrow_mut().push(day.to_string());
        Some(vec![shuttle.clone(), line("Linie5", "5", 3, true)])
    };
    let far = map_events(&map, "2024-03-01", &mut read);
    assert_eq!(far.len(), 1, "the event over and the change for good are none: {far:?}");
    let e = &far[0];
    assert_eq!((e.name.as_str(), e.from.as_str(), e.until.as_str()), ("FCA Heimspiel", "2024-03-12", "2024-03-12"));
    assert_eq!((e.own.clone(), e.off.clone()), (vec!["Sonderverkehr".to_string()], vec!["Linie7".to_string()]));
    assert!(e.lines.is_empty() && asked.borrow().is_empty(), "not read eleven days before");
    let near = map_events(&map, "2024-03-05", &mut read);
    assert_eq!(near[0].lines.iter().map(|l| l.name.as_str()).collect::<Vec<_>>(), ["Sonderverkehr"]);
    assert_eq!(*asked.borrow(), ["2024-03-12"]);
    let _ = std::fs::remove_dir_all(&map);
}

/// (1 November 2026, the admin panel) The server's admin puts a line out to tender at once (an
/// application of the server's own: a line the map does not have is said, not offered), and calls
/// an open tender off: gone from the market, a company's copy goes at its next step, the fee it
/// paid back and the company told.
#[test]
fn the_servers_admin_opens_a_tender_and_calls_one_off() {
    let data = temp("admin");
    let lines = lines();
    let key = key_of(MAP);
    let now = clock::moment(DATE, 8 * 60);
    let mut a = company("Umove", "umove");
    let ca = a.clone();
    let mut read = || Some(MapNow { lines: lines.clone(), companies: vec![ca.clone()], template: template(MAP, DATE, &[]), events: Vec::new() });
    step(&data, &key, MAP, DATE, now, &mut read);
    let m = load(&data, &key).unwrap();
    // (the map's lines with passengers of their own, kept for the panel)
    assert!(m.lines.iter().any(|l| l == "Linie5") && m.lines.iter().any(|l| l == "Linie9"), "{:?}", m.lines);
    ask_as_server(&data, &key, "Linie5").unwrap();
    ask_as_server(&data, &key, "Linie99").unwrap();
    let said = step(&data, &key, MAP, DATE, now + 1, &mut read);
    assert!(said.iter().any(|s| s.contains("Linie99") && s.contains("no such line")), "{said:?}");
    let m = load(&data, &key).unwrap();
    let t = m.tenders.iter().find(|t| t.tender.line == "Linie5" && t.decided.is_none());
    assert!(t.is_some_and(|t| t.applied.contains(&SERVER_ASKS.to_string())), "{:?}", m.tenders.iter().map(|t| (&t.tender.line, &t.applied)).collect::<Vec<_>>());
    // the company takes it among its own and bids (the fee paid)
    at(&mut a, now + 2);
    sync(&data, &mut a);
    let (id, opens) = a.concessions.tenders.iter().find(|t| t.line == "Linie5").map(|t| (t.id, t.opens_at)).unwrap();
    at(&mut a, opens.max(now) + 5);
    let least = cn::min_bid(&a, a.concessions.tenders.iter().find(|t| t.id == id).unwrap());
    bid(&data, &mut a, id, least).unwrap();
    assert!(a.concessions.tenders.iter().find(|t| t.id == id).unwrap().fee_paid);
    let cash = a.cash;
    // called off by the admin: the market's copy goes
    let mut m = load(&data, &key).unwrap();
    m.tenders.retain(|t| t.tender.line != "Linie5");
    save(&data, &m).unwrap();
    sync(&data, &mut a);
    assert!(!a.concessions.tenders.iter().any(|t| t.line == "Linie5"), "gone from the company's");
    let fee = a.cash - cash;
    assert!(fee > 0, "its fee paid back");
    assert!(a.clock.feed.iter().any(|f| f.text == CALLED_OFF), "told");
    let _ = std::fs::remove_dir_all(&data);
}
