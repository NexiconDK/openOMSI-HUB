use super::*;
use crate::company::bus_contracts::Journeys;
use crate::company::concessions::tests::line;
use crate::company::concessions::Concession;
use crate::company::model::GameMode;
use crate::company::{found, network, store, Founding};

const MAP: &str = "maps/Grundorf/global.cfg";
const DATE: &str = "2024-03-04";

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("openomsi-subcontracts-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn company(name: &str, id: &str) -> Company {
    let mut c = found(&Founding { name: name.into(), date: DATE.into(), map: MAP.into(), game_mode: GameMode::Normal, ..Default::default() }, "owner");
    c.id = id.into();
    c.online.on_server = true;
    c
}

fn held(line: &str, until: &str) -> Concession {
    Concession { line: line.into(), number: line.trim_start_matches("Linie").into(), from: DATE.into(), until: until.into(), price: 1.0, direct: false, after: String::new(), options: 0, package: String::new(), with: Vec::new() }
}

fn told(c: &Company, text: &str) -> usize {
    c.clock.feed.iter().filter(|f| f.text == text).count()
}

/// A subcontracted line's day travels as it was run: its timetabled minutes, its journeys run
/// and not, the players' departures and red lights; its minutes run are the tours' less those not
/// run.
#[test]
fn a_days_run_goes_from_one_company_to_another() {
    let mut j = Journeys::default();
    j.timetabled("Linie5", 120);
    j.timetabled("Linie5", 60);
    j.timetabled("Linie7", 30);
    j.ran("Linie5", "1", 2, false, Some(3), None, None);
    j.missed("Linie5", "2", 25);
    j.excused("Linie5", "2", 10);
    j.departures("Linie5", 1, 2);
    j.red_lights("Linie5", 1);
    let d = j.export("linie5");
    assert_eq!((d.planned.len(), d.journeys.len(), d.early, d.late, d.red), (2, 3, 1, 2, 1));
    assert_eq!(d.minutes_run(), 180 - 25 - 10);
    let mut k = Journeys::default();
    k.import("Linie5", &d);
    assert_eq!(k.export("Linie5"), d);
    assert_eq!(k.len(), 3);
}

/// The main server takes the companies' asks: a proposal of lines the main company holds (refused
/// for one it does not, or another map's company), accepted by the other - running from its day
/// to the contract's end -, ended by either with two days' notice; one not answered lapses.
#[test]
fn proposed_accepted_ended_or_lapsed() {
    let data = temp("book");
    let mut a = company("Umove", "umove");
    a.concessions.held.push(held("Linie5", "2024-03-20"));
    let b = company("GoCollective", "gocollective");
    let mut far = company("Far", "far");
    far.map = "maps/Other/global.cfg".into();
    for c in [&a, &b, &far] {
        store::save(&data, c).unwrap();
    }
    let tomorrow = dates::add(DATE, 1);
    let propose = |sub: &str, lines: &[&str], hour: Cents| Ask { kind: AskKind::Propose, sub: sub.into(), lines: lines.iter().map(|l| l.to_string()).collect(), hour, from: tomorrow.clone(), ..Ask::default() };
    assert_eq!(ask(&data, &a, &propose("umove", &["Linie5"], 500_00)), Err(NO_SUB));
    assert_eq!(ask(&data, &a, &propose("gocollective", &["Linie5"], 0)), Err(NO_PRICE));
    assert_eq!(ask(&data, &a, &propose("gocollective", &["Linie7"], 500_00)), Err(NOT_HELD));
    ask(&data, &a, &propose("gocollective", &["Linie5"], 500_00)).unwrap();
    ask(&data, &a, &propose("far", &["Linie5"], 500_00)).unwrap();
    step(&data, DATE);
    let bk = load(&data);
    assert_eq!(bk.list.len(), 1, "not another map's");
    let x = bk.list[0].clone();
    assert_eq!((x.state, x.from.as_str(), x.until.as_str(), x.hour), (State::Proposed, tomorrow.as_str(), "2024-03-20", 500_00));
    // (accepted by the other: running from its day)
    ask(&data, &b, &Ask { kind: AskKind::Accept, id: x.id, ..Ask::default() }).unwrap();
    step(&data, DATE);
    let x = load(&data).list[0].clone();
    assert!(x.state == State::Running && !x.runs(DATE) && x.runs(&tomorrow) && x.runs("2024-03-20") && !x.runs("2024-03-21"));
    // (ended by the main company with notice)
    ask(&data, &a, &Ask { kind: AskKind::End, id: x.id, ..Ask::default() }).unwrap();
    step(&data, "2024-03-06");
    let x = load(&data).list[0].clone();
    assert_eq!(x.end(), "2024-03-08");
    step(&data, "2024-03-09");
    assert_eq!(load(&data).list[0].state, State::Ended);
    // (one not answered lapses)
    ask(&data, &a, &Ask { from: "2024-03-10".into(), ..propose("gocollective", &["Linie5"], 400_00) }).unwrap();
    step(&data, "2024-03-09");
    let id = load(&data).list.iter().find(|x| x.state == State::Proposed).unwrap().id;
    step(&data, "2024-03-13");
    assert_eq!(load(&data).list.iter().find(|x| x.id == id).unwrap().state, State::Declined);
    let _ = std::fs::remove_dir_all(&data);
}

/// The subcontractor takes the lines on (its own, `sub_for` the main company, their fares the
/// authority's), the main company runs none of them that day; at the day's close the
/// subcontractor is paid its timetable hours and sends its day, which the main company's
/// contract takes in, paying them.
#[test]
fn the_subcontractor_runs_the_line_and_both_are_paid() {
    let data = temp("run");
    let lines = vec![line("Linie5", "5", 2, true)];
    let mut a = company("Umove", "umove");
    network::add_line(&mut a, &lines[0], None).unwrap();
    a.concessions.held.push(held("Linie5", "2024-03-20"));
    let mut b = company("GoCollective", "gocollective");
    let x = Subcontract { id: 1, key: "grundorf".into(), main: "umove".into(), main_name: "Umove".into(), sub: "gocollective".into(), sub_name: "GoCollective".into(), lines: vec!["Linie5".into()], numbers: vec!["5".into()], hour: 600_00, from: DATE.into(), until: "2024-03-20".into(), last: String::new(), state: State::Running, joint: None, asked: DATE.into() };
    save(&data, &Book { counter: 1, list: vec![x], asks_read: 0 }).unwrap();
    assert!(!refresh(&data, &mut a), "the main company takes nothing on");
    assert!(refresh(&data, &mut b));
    assert_eq!((told(&a, STARTS), told(&b, STARTS)), (1, 1));
    assert!(take_on(&mut b, &lines));
    let cl = b.lines.iter().find(|l| l.name == "Linie5").unwrap();
    assert_eq!(cl.sub_for, "umove");
    assert!(crate::company::bus_contracts::fares_to_authority(&b, "Linie5"));
    assert!(!refresh(&data, &mut b), "taken on");
    // (the main company's tours of the day: none of the line)
    let mut tours = network::tours_of_day(&a, &lines, DATE);
    assert!(tours.is_empty(), "{:?}", tours.iter().map(|t| &t.tour).collect::<Vec<_>>());
    let mut own = network::tours_of_day(&b, &lines, DATE);
    assert_eq!(own.len(), 2);
    tours.append(&mut own);
    // (the subcontractor's close: paid, the day sent on)
    let mut j = Journeys::default();
    j.timetabled("Linie5", 120);
    j.timetabled("Linie5", 120);
    j.ran("Linie5", "1", 0, false, None, None, None);
    j.missed("Linie5", "2", 30);
    let cash = b.cash;
    close_day(&mut b, DATE, &mut j);
    assert_eq!(b.cash - cash, (210.0 / 60.0 * 600_00.0) as Cents);
    assert_eq!(b.online.sub_out.len(), 1);
    refresh(&data, &mut b);
    assert!(b.online.sub_out.is_empty(), "sent");
    // (the main company takes it in: paid out, judged as its own)
    refresh(&data, &mut a);
    assert_eq!(a.online.sub_in.len(), 1);
    let mut mine = Journeys::default();
    let cash = a.cash;
    close_day(&mut a, DATE, &mut mine);
    assert_eq!(cash - a.cash, (210.0 / 60.0 * 600_00.0) as Cents);
    assert_eq!(mine.export("Linie5").minutes_run(), 210);
    assert!(a.online.sub_in.is_empty());
    // (ended: the subcontractor gives the line up)
    let mut bk = load(&data);
    bk.list[0].state = State::Ended;
    save(&data, &bk).unwrap();
    assert!(refresh(&data, &mut b));
    assert!(take_on(&mut b, &lines));
    assert!(!b.lines.iter().any(|l| l.name == "Linie5"));
    let _ = std::fs::remove_dir_all(&data);
}

/// A joint bid: the partner's lines of a tender of the market, from its contract's first day;
/// accepted, it runs when the bidder wins and falls away when another does.
#[test]
fn a_joint_bid_runs_when_its_bidder_wins() {
    use crate::company::shared_tenders::{self as st, MapNow};
    let data = temp("joint");
    let lines = vec![line("Linie7", "7", 2, true), line("Linie9", "9", 2, true)];
    let (a, b) = (company("Umove", "umove"), company("GoCollective", "gocollective"));
    store::save(&data, &a).unwrap();
    store::save(&data, &b).unwrap();
    let key = st::key_of(MAP);
    let mut read = || Some(MapNow { lines: lines.clone(), companies: Vec::new(), template: st::template(MAP, DATE, &[]), events: Vec::new() });
    st::step(&data, &key, MAP, DATE, crate::company::clock::moment(DATE, 8 * 60), &mut read);
    let m = st::load(&data, &key).unwrap();
    let joint = |k: usize| {
        let t = &m.tenders[k].tender;
        Joint { key: key.clone(), tender: t.market.as_ref().unwrap().id, number: t.number.clone() }
    };
    let t0 = m.tenders[0].tender.clone();
    for k in 0..2 {
        let line = m.tenders[k].tender.line.clone();
        ask(&data, &a, &Ask { kind: AskKind::Propose, sub: "gocollective".into(), lines: vec![line], hour: 500_00, joint: Some(joint(k)), ..Ask::default() }).unwrap();
    }
    step(&data, DATE);
    let bk = load(&data);
    assert_eq!(bk.list.len(), 2);
    assert_eq!(bk.list[0].from, crate::company::concessions::server_start(&t0));
    for x in &bk.list {
        ask(&data, &b, &Ask { kind: AskKind::Accept, id: x.id, ..Ask::default() }).unwrap();
    }
    step(&data, DATE);
    assert!(load(&data).list.iter().all(|x| x.state == State::Accepted));
    // (the first won by the bidder, the second by another)
    let mut m = st::load(&data, &key).unwrap();
    m.tenders[0].decided = Some(st::Decided { winner: "umove".into(), name: "Umove".into(), ..Default::default() });
    m.tenders[1].decided = Some(st::Decided { winner: String::new(), name: "Op".into(), ..Default::default() });
    st::save(&data, &m).unwrap();
    step(&data, DATE);
    let states: Vec<State> = load(&data).list.iter().map(|x| x.state).collect();
    assert_eq!(states, vec![State::Running, State::Declined]);
    let _ = std::fs::remove_dir_all(&data);
}
