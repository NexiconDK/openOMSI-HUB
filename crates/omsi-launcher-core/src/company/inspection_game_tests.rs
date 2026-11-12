//! (round U) The company's ticket inspection in the player's game.
use super::*;
use crate::company::model::GameMode;
use crate::company::{found, Founding};

#[test]
fn the_clothes_are_named_by_the_look_and_the_company() {
    let mut c = found(&Founding { name: "Umove Bus A/S".into(), game_mode: GameMode::Normal, date: "2026-11-11".into(), ..Default::default() }, "Mathias");
    c.short = "Umove".into();
    assert_eq!(slug(&c), "umove");
    assert_eq!(variant(&c, 'a'), "~Inspector_A_umove");
    c.short = " ".into();
    assert_eq!(slug(&c), "umovebusas");
    assert_eq!(letters(&c), vec!['A'], "the vest by default");
    c.inspection_plan.look = Look::Mixed;
    assert_eq!(letters(&c), vec!['A', 'B', 'C']);
    assert_eq!((Look::Mixed.letter(0), Look::Mixed.letter(1), Look::Mixed.letter(5)), ('A', 'B', 'C'));
    assert_eq!(Look::of("company"), Some(Look::Company));
    assert!(Look::of("x").is_none());
}

#[test]
fn a_game_s_check_is_kept_for_its_day_and_bounded() {
    let mut c = found(&Founding { name: "Stadtbus".into(), game_mode: GameMode::Normal, date: "2026-11-11".into(), ..Default::default() }, "Luc");
    let g = GameCheck { line: "134".into(), dep: 600, date: "2026-11-11".into(), checked: 23, caught: 2, fled: 1, stops: 5, board_stop: "Markt".into(), leave_stop: "Zoo".into(), names: vec!["Lene".into()], ordered: true, driver: "Luc".into() };
    book_game(&mut c, &g).unwrap();
    // (the same report again: kept once)
    book_game(&mut c, &g).unwrap();
    let day = of_day(&c, "2026-11-11");
    assert_eq!(day.len(), 1);
    assert_eq!((day[0].checked, day[0].caught, day[0].game.as_str(), day[0].from.as_str(), day[0].to.as_str(), day[0].stops), (23, 2, "Luc", "Markt", "Zoo", 5));
    assert!(day[0].fine > 0);
    assert!(book_game(&mut c, &GameCheck { caught: 30, ..g.clone() }).is_err(), "more caught than checked");
    assert!(book_game(&mut c, &GameCheck { checked: 500, caught: 1, ..g.clone() }).is_err());
    assert!(book_game(&mut c, &GameCheck { date: "2026-11-20".into(), ..g.clone() }).is_err(), "not of today");
    assert!(book_game(&mut c, &GameCheck { line: " ".into(), ..g.clone() }).is_err());
    // (an order of the phone's strict shape)
    let o = serde_json::json!({ "do": "inspection", "check": g });
    assert_eq!(super::super::remote::parse(&o), Some(super::super::remote::Order::Inspection { check: g.clone() }));
    let bad = serde_json::json!({ "do": "inspection", "check": GameCheck { line: "x".repeat(100), ..g.clone() } });
    assert!(super::super::remote::parse(&bad).is_none());
}

use crate::company::inspection::{self as ins, Seen};
use crate::company::model::{CompanyLine, Role};
use crate::company::network::{PlannedTrip, TourOfDay};
use crate::company::plan::{DayPlan, DayTour};
use crate::company::research::keys;
use crate::company::{dates, staff};

fn company() -> Company {
    let mut c = found(&Founding { name: "Kontrol".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), ..Default::default() }, "Luc");
    c.country = "de".into();
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), km: 300.0, service_from: Some(0), ..Default::default() });
    c.lines.push(CompanyLine { name: "Linie7".into(), number: "7".into(), km: 100.0, service_from: Some(0), ..Default::default() });
    c
}

/// Hire `n` inspectors and make the company's day one all of them work.
fn with_inspectors(c: &mut Company, n: usize) {
    c.research.done.push(keys::TICKET_INSPECTORS.to_string());
    let (id0, mut round, mut hired) = (c.id.clone(), 0, 0);
    while hired < n {
        for a in staff::applicants_of(c, Role::Inspector) {
            if hired < n {
                staff::hire(c, &a).unwrap();
                hired += 1;
            }
        }
        round += 1;
        c.id = format!("{id0}{round}");
        c.taken = Default::default();
    }
    c.id = id0;
    for e in c.staff.iter_mut() {
        (e.sick_until, e.holiday_until) = (None, None);
    }
    let ids: Vec<u32> = c.staff.iter().filter(|e| e.role == Role::Inspector).map(|e| e.id).collect();
    c.date = (0..14).map(|k| dates::add(&c.date, k)).find(|d| ids.iter().all(|id| staff::at_work(c.employee(*id).unwrap(), d))).expect("a day all of them work");
}

/// The day's plan: four tours of each line, a trip every 20 minutes each, 30 minutes and 14 stops.
fn plan(c: &Company) -> DayPlan {
    let trip = |line: &str, dep: i32| PlannedTrip { name: format!("{line} {dep}"), line: line.into(), from: "Bahnhof".into(), to: "Markt".into(), dep, arr: dep + 30, km: 10.0, stops: 14, ..Default::default() };
    let tours = ["Linie5", "Linie7"]
        .iter()
        .flat_map(|l| (0..4).map(move |k| (l, k)))
        .map(|(l, k)| DayTour { tour: TourOfDay { line: l.to_string(), number: l.trim_start_matches("Linie").into(), tour: (k + 1).to_string(), trips: (0..14).map(|i| trip(l, 5 * 60 + k * 5 + i * 80)).collect(), ..Default::default() }, ..Default::default() })
        .collect();
    DayPlan { date: c.date.clone(), tours, ..Default::default() }
}

#[test]
fn the_duty_s_trips_have_their_dodgers_and_the_patrols_board_some_of_them() {
    let mut c = company();
    let dp = plan(&c);
    // no inspectors: the shares only, nobody to ask
    let g = for_duty(&c, &dp, "Linie5", "1").unwrap();
    assert!(!g.can_order && g.boardings.is_empty());
    assert_eq!(g.shares.len(), 14);
    assert!(g.shares.iter().all(|s| s.share > 0.0 && s.share <= 0.5));
    assert!(for_duty(&c, &dp, "Linie5", "9").is_none(), "not a tour of the day");
    // two inspectors: a pair on the busiest line boards some of its trips in their hours
    with_inspectors(&mut c, 2);
    let dp = plan(&c);
    let p = staff::patrols(&c, &c.date);
    assert!(!p.is_empty());
    let mut boarded = 0;
    for tour in ["1", "2", "3", "4"] {
        let g = for_duty(&c, &dp, "Linie5", tour).unwrap();
        assert!(g.can_order && g.variant == variant(&c, 'A'));
        for b in &g.boardings {
            assert!(p.iter().any(|x| x.line == "Linie5" && b.dep >= x.from && b.dep < x.to), "{b:?}");
            assert!(b.board >= 1 && b.stops >= 1 && b.board + b.stops <= 14 && b.n == 2 && b.names.len() == 2 && b.names.iter().all(|n| !n.is_empty()), "{b:?}");
        }
        boarded += g.boardings.len();
        // (the same day draws the same)
        assert_eq!(for_duty(&c, &dp, "Linie5", tour), Some(g));
    }
    assert!(boarded > 0 && boarded < 40, "{boarded}");
    // (their checks keep the dodgers down: the line they check has fewer)
    if !p.iter().any(|x| x.line == "Linie7") {
        assert!(for_duty(&c, &dp, "Linie7", "1").unwrap().boardings.is_empty());
    }
}

#[test]
fn control_sends_inspectors_near_once_a_trip_or_says_why_not() {
    let mut c = company();
    let dp = plan(&c);
    assert_eq!(answer(&c, &dp, "Linie5", "1", 8 * 60).unwrap_err(), NO_INSPECTORS);
    with_inspectors(&mut c, 2);
    let dp = plan(&c);
    let p = staff::patrols(&c, &c.date);
    let on5 = p.iter().find(|x| x.line == "Linie5").unwrap();
    // (a few moments of their hours: mostly near enough)
    let mut yes = 0;
    for k in 0..12 {
        let now = on5.from + 10 + k * 25;
        if now >= on5.to {
            break;
        }
        match answer(&c, &dp, "Linie5", "1", now) {
            Ok((o, at)) => {
                yes += 1;
                assert!(o.board >= 1 + AHEAD && o.board + o.stops <= 14 && o.n == 2, "{o:?}");
                assert_eq!(at, now + (AHEAD as f64 * STOP_MINUTES).round() as i32);
                // (the same moment answers the same)
                assert_eq!(answer(&c, &dp, "Linie5", "1", now).unwrap().0, o);
            }
            Err(e) => assert!(e == NOBODY_NEAR, "{e}"),
        }
    }
    assert!(yes > 0);
    // once a trip: asked already
    let now = on5.from + 10;
    let (o, _) = (0..30).find_map(|k| answer(&c, &dp, "Linie5", "1", now + k).ok()).unwrap();
    c.messages.list.push(crate::company::messages::Msg { to: crate::company::messages::To::Duty { line: "Linie5".into(), tour: "1".into() }, inspection: Some(o.clone()), ..Default::default() });
    let trip_now = dp.tours[0].tour.trips.iter().find(|t| t.dep == o.dep).unwrap().dep + 1;
    assert_eq!(answer(&c, &dp, "Linie5", "1", trip_now).unwrap_err(), ASKED_ALREADY);
    // (out of their hours: nobody)
    assert_eq!(answer(&c, &dp, "Linie5", "1", 4 * 60).unwrap_err(), if p.iter().any(|x| x.from <= 240 && 240 < x.to) { NOBODY_NEAR } else { NO_INSPECTORS });
}

#[test]
fn the_day_s_close_takes_the_game_s_check_instead_of_the_model_s() {
    let mut c = company();
    with_inspectors(&mut c, 2);
    let date = c.date.clone();
    let seen: Vec<Seen> = (0..112).map(|k| 5 * 60 + 10 * (k / 2)).enumerate().map(|(k, dep)| Seen { line: "Linie5".into(), number: "5".into(), dep, arr: dep + 28, from: if k % 2 == 0 { "Bahnhof" } else { "Markt" }.into(), to: if k % 2 == 0 { "Markt" } else { "Bahnhof" }.into(), pax: 40, share: 0.08, crowded: false, stops: 14 }).collect();
    // (the model's checks of the day without the game)
    let model = ins::close(&mut c.clone(), &date, Vec::new(), &seen);
    let k = model.checks.iter().find(|k| k.line == "Linie5").expect("a modelled check").clone();
    let g = GameCheck { line: "5".into(), dep: k.dep, date: date.clone(), checked: 31, caught: 3, fled: 1, stops: 6, board_stop: "Markt".into(), leave_stop: "Zoo".into(), names: vec!["Lene".into()], ordered: false, driver: "Luc".into() };
    book_game(&mut c, &g).unwrap();
    let day = ins::close(&mut c, &date, Vec::new(), &seen);
    let on: Vec<_> = day.checks.iter().filter(|x| x.line == "Linie5" && (x.dep - k.dep).abs() <= 1).collect();
    assert_eq!(on.len(), 1, "{on:?}");
    assert_eq!((on[0].checked, on[0].caught, on[0].game.as_str()), (31, 3, "Luc"));
    assert!(day.checks.len() <= model.checks.len());
    assert!(c.inspection.game.is_empty(), "taken by the close");
}

/// (NexiconDK, 12 November 2026) A joined player's start carries the company's inspection on his
/// duty (Bus Company Online's players saw inspectors only when they asked for them): sent and
/// read back as it was; a start of a boss's launcher of before (without it) reads as none.
#[test]
fn a_joined_player_s_start_carries_the_inspection_of_his_duty() {
    use crate::company::together::wire::DriveStart;
    let mut c = company();
    with_inspectors(&mut c, 2);
    let dp = plan(&c);
    let g = (1..=4).filter_map(|t| for_duty(&c, &dp, "Linie5", &t.to_string())).find(|g| !g.boardings.is_empty()).expect("a tour boarded");
    let s = DriveStart { line: "Linie5".into(), tour: "1".into(), inspection: Some(g.clone()), ..Default::default() };
    let back: DriveStart = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
    // (the shares as JSON reads them back: to the last digit or so)
    let b = back.inspection.expect("sent");
    assert_eq!((&b.boardings, b.can_order, &b.variant, b.shares.len()), (&g.boardings, g.can_order, &g.variant, g.shares.len()));
    assert!((b.share - g.share).abs() < 1e-9 && b.shares.iter().zip(&g.shares).all(|(x, y)| x.dep == y.dep && (x.share - y.share).abs() < 1e-9));
    let mut old = serde_json::to_value(&DriveStart { line: "Linie5".into(), ..Default::default() }).unwrap();
    assert!(old.get("inspection").is_none(), "none: not sent");
    old["unknown"] = serde_json::json!(1);
    assert_eq!(serde_json::from_value::<DriveStart>(old).unwrap().inspection, None);
}
