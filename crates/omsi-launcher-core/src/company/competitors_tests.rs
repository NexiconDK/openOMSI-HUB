use super::*;
use crate::company::concessions::{self as cn, Shared, Week};
use crate::company::sealed;
use crate::company::server_buses::{FloorRule, FuelRule};
use crate::company::shared_tenders::{self as st, Market, MarketTender, OperatorHold};
use crate::company::{found, Founding};

fn made(size: Size, style: Style, fuels: &[Drive], floors: &[Floor]) -> Made {
    Made { size, style, fuels: fuels.to_vec(), floors: floors.to_vec() }
}

fn competitor(id: u32, name: &str, m: Made) -> Competitor {
    Competitor { id, name: name.into(), quality: 80.0, made: m }
}

/// A tender of a map's market of cost price `cost` a year, its work `week_hours` timetable hours
/// a week, asking `needs`, every rival of `rivals` bidding.
fn tender(cost: i64, week_hours: f64, needs: Needs, rivals: Vec<Rival>) -> cn::Tender {
    let c = found(&Founding { name: "T".into(), date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "");
    let l = cn::tests::line("Linie5", "5", 3, true);
    let mut t = cn::make_tender(&c, &l, 7, 0, false, "k", &[], (0, 0));
    t.cost = cost;
    t.reference = sealed::cap_of(cost, 0);
    t.week = Some(Week { hours: week_hours, ..Default::default() });
    t.bidders = rivals.iter().map(|r| r.id).collect();
    t.market = Some(Shared { key: "grundorf".into(), id: 7, rivals, needs: Some(needs), ..Default::default() });
    t
}

/// The sizes take their packages and hold their contracts; the styles put their margins; the
/// buses meet a tender's requirements by fuel and floor - with the first fuel it allows.
#[test]
fn sizes_styles_and_buses() {
    use cn::WorkSize::*;
    assert!(Size::Small.takes(Small) && !Size::Small.takes(Medium) && !Size::Small.takes(Large));
    assert!(Size::Medium.takes(Medium) && !Size::Medium.takes(Large) && Size::Large.takes(Large));
    assert_eq!([Size::Small.most(), Size::Medium.most(), Size::Large.most()], [2, 4, 8]);
    assert_eq!([Style::Careful.margin(), Style::Normal.margin(), Style::Aggressive.margin()], [(0.10, 0.15), (0.05, 0.10), (0.0, 0.05)]);
    let any = Needs { floor: FloorRule::Any, fuel: FuelRule::Any, ..Default::default() };
    let electric = Needs { fuel: FuelRule::Electric, ..any };
    let clean = Needs { fuel: FuelRule::CngOrElectric, ..any };
    let low = Needs { floor: FloorRule::LowOnly, ..any };
    let gas = made(Size::Medium, Style::Normal, &[Drive::Diesel, Drive::Cng], &[Floor::LowEntry]);
    assert_eq!(gas.drive_for(&any), Some(Drive::Diesel));
    assert_eq!(gas.drive_for(&clean), Some(Drive::Cng), "the first fuel allowed");
    assert_eq!(gas.drive_for(&electric), None);
    assert_eq!(gas.drive_for(&low), None, "low entry on a low floor tender");
    let plain = Made::default();
    assert_eq!((plain.fuels(), plain.floors()), (vec![Drive::Diesel], vec![Floor::LowFloor]), "none named: diesel, low floor");
    assert_eq!(Style::Aggressive.character(Size::Large), crate::company::auction::Character::Aggressive);
    assert_eq!(Style::Normal.character(Size::Large), crate::company::auction::Character::Big);
}

/// A competitor bids its size's own cost by its style's margin, never under the abnormally low
/// line, with the buses it bids with; on work too big for it, or with buses the tender does not
/// allow, it does not bid. A drawn operator bids as before, with diesel.
#[test]
fn a_competitor_bids_by_its_size_style_and_buses() {
    let rivals = vec![
        competitor(1, "Big Aggro", made(Size::Large, Style::Aggressive, &[Drive::Electric], &[Floor::LowFloor])).rival(),
        competitor(2, "Small Care", made(Size::Small, Style::Careful, &[Drive::Diesel], &[Floor::LowFloor])).rival(),
        competitor(3, "Gas Normal", made(Size::Medium, Style::Normal, &[Drive::Diesel, Drive::Cng], &[Floor::LowFloor])).rival(),
        Rival { id: 4, name: "Drawn".into(), character: crate::company::auction::Character::Cautious, quality: 90.0, made: None },
    ];
    let cost = 100_000_00;
    // (a small work, any fuel: every one bids)
    let t = tender(cost, 500.0, Needs::default(), rivals.clone());
    let bids = sealed::operator_bids(&t);
    let of = |name: &str| bids.iter().find(|b| b.name == name).cloned();
    let big = of("Big Aggro").unwrap();
    assert!(big.amount as f64 >= sealed::low_line(&t) as f64 && big.amount as f64 <= cost as f64 * 0.93 * 1.05 + 100_00.0, "{big:?}");
    assert_eq!((big.own_cost, big.drive), (93_000_00, Drive::Electric));
    let small = of("Small Care").unwrap();
    assert!(small.amount >= 110_000_00 && small.amount <= 115_000_00 + 100_00, "{small:?}");
    assert_eq!(of("Gas Normal").unwrap().drive, Drive::Diesel);
    assert_eq!(of("Drawn").unwrap().drive, Drive::Diesel);
    // (a large work: only the large one and the drawn operator)
    let large = tender(cost, 3500.0, Needs::default(), rivals.clone());
    let names: Vec<String> = sealed::operator_bids(&large).into_iter().map(|b| b.name).collect();
    assert_eq!(names, ["Big Aggro", "Drawn"]);
    // (CNG or electric: the gas one with CNG, the small diesel one not at all)
    let clean = tender(cost, 500.0, Needs { fuel: FuelRule::CngOrElectric, ..Default::default() }, rivals);
    let bids = sealed::operator_bids(&clean);
    assert!(!bids.iter().any(|b| b.name == "Small Care"));
    assert_eq!(bids.iter().find(|b| b.name == "Gas Normal").unwrap().drive, Drive::Cng);
}

/// The list: a new one gets the next id, a name is needed and is the list's once, a change and a
/// delete by the id; quality moves within 0 to 100.
#[test]
fn the_admins_list() {
    let mut list = Competitors::default();
    assert!(list.rivals().is_none(), "no list: the maps' own");
    assert_eq!(list.put(competitor(0, "Nordbus", Made::default())), Ok(1));
    assert_eq!(list.put(competitor(0, " ", Made::default())), Err(NO_NAME));
    assert_eq!(list.put(competitor(0, "nordbus", Made::default())), Err(NAME_TAKEN));
    assert_eq!(list.put(competitor(0, "Citybus", made(Size::Large, Style::Careful, &[], &[]))), Ok(2));
    let mut changed = competitor(1, "Nordbus", made(Size::Small, Style::Aggressive, &[Drive::Cng], &[]));
    changed.quality = 140.0;
    assert_eq!(list.put(changed), Ok(1));
    assert_eq!((list.list[0].made.size, list.list[0].quality), (Size::Small, 100.0));
    assert!(list.moved("Nordbus", GAVE_UP) && list.list[0].quality == 95.0);
    assert!(!list.moved("Nobody", ENDED_WELL));
    assert_eq!(list.delete(2), Ok("Citybus".to_string()));
    assert_eq!(list.delete(2), Err(NO_SUCH));
    assert_eq!(list.put(competitor(0, "Third", Made::default())), Ok(3), "ids never given again");
    let rivals = list.rivals().unwrap();
    assert_eq!(rivals.iter().map(|r| (r.id, r.made.is_some())).collect::<Vec<_>>(), [(1, true), (3, true)]);
    // (kept in its file)
    let data = std::env::temp_dir().join(format!("openomsi-competitors-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&data);
    assert_eq!(load(&data), Competitors::default());
    save(&data, &list).unwrap();
    assert_eq!(load(&data), list);
    let _ = std::fs::remove_dir_all(&data);
}

/// A map's market takes the list as its operators; an open tender's copy of one the admin changed
/// is brought up to date, one he deleted leaves its bidders; a drawn operator of a tender before
/// stays. Holding as many contracts on the map as its size allows (a package one), a competitor
/// is left out of a tender's bidders at its close.
#[test]
fn a_market_takes_the_list_and_room_counts() {
    let mut list = Competitors::default();
    list.put(competitor(0, "Nordbus", made(Size::Small, Style::Normal, &[], &[]))).unwrap();
    list.put(competitor(0, "Citybus", made(Size::Medium, Style::Normal, &[], &[]))).unwrap();
    let drawn = Rival { id: 9, name: "Drawn".into(), character: crate::company::auction::Character::Big, quality: 70.0, made: None };
    let mut rivals = list.rivals().unwrap();
    rivals.push(drawn.clone());
    let t = tender(100_000_00, 500.0, Needs::default(), rivals);
    let mut m = Market { key: "grundorf".into(), rivals: vec![drawn], tenders: vec![MarketTender { tender: t, applied: Vec::new(), decided: None, complaints: Vec::new() }], ..Default::default() };
    // (the admin changes Nordbus's quality and deletes Citybus)
    list.list[0].quality = 55.0;
    list.delete(2).unwrap();
    assert!(bring_up(&mut m, &list));
    assert_eq!(m.rivals.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["Nordbus"]);
    let x = m.tenders[0].tender.market.as_ref().unwrap();
    assert_eq!(x.rivals.iter().find(|r| r.name == "Nordbus").unwrap().quality, 55.0);
    assert_eq!(m.tenders[0].tender.bidders, [1, 9], "Citybus gone, the drawn one stays");
    assert!(!bring_up(&mut m, &list), "nothing more");
    // (room: Nordbus is small - two contracts a map; a package's lines one)
    let hold = |line: &str, package: &str| OperatorHold { line: line.into(), name: "Nordbus".into(), package: package.into(), ..Default::default() };
    m.held = vec![hold("L1", "P"), hold("L2", "P")];
    assert_eq!(holds(&m, "Nordbus"), 1);
    assert_eq!(with_room(&m, &m.tenders[0].tender), [1, 9]);
    m.held.push(hold("L3", ""));
    assert_eq!(holds(&m, "Nordbus"), 2);
    assert_eq!(with_room(&m, &m.tenders[0].tender), [9], "no room: left out");
    // (no list: the market keeps its own)
    let mut plain = Market { rivals: cn::make_rivals(crate::company::rng::Rng::of(&["k"], 0)), ..Default::default() };
    let before = plain.rivals.clone();
    assert!(!bring_up(&mut plain, &Competitors::default()) && plain.rivals == before);
}

/// An operator's contract that ran its term raises its quality a point, one given up lowers it
/// five; a package's lines once.
#[test]
fn quality_moves_with_the_contracts() {
    let hold = |line: &str, until: &str, gives: &str, with: &[&str]| OperatorHold {
        line: line.into(),
        name: "Nordbus".into(),
        from: "2024-03-01".into(),
        until: until.into(),
        gives_up: gives.into(),
        with: with.iter().map(|w| w.to_string()).collect(),
        ..Default::default()
    };
    let before = vec![hold("A", "2024-03-10", "", &[]), hold("B", "2024-03-20", "2024-03-11", &[]), hold("C", "2024-03-10", "", &["C", "D"]), hold("D", "2024-03-10", "", &["C", "D"]), hold("E", "2024-03-30", "", &[])];
    let after = vec![before[4].clone()];
    let moves = st::quality_moves(&before, &after, "2024-03-11");
    assert_eq!(moves, [("Nordbus".to_string(), ENDED_WELL), ("Nordbus".to_string(), GAVE_UP), ("Nordbus".to_string(), ENDED_WELL)]);
}
