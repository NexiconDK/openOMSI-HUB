//! Round J: a duty's price, the day in figures against the game's plan, the checklist, the short
//! names, and "Make duties automatically" with the planner's goals - numbered as they sign on.

use super::super::{autopilot, found, Founding};
use super::*;
use crate::company::model::{CompanyLine, Planner};
use crate::company::pro_duties::{auto, number_of, Goals};
use crate::company::rosterflow::{board, Span};
use crate::{StopInfo, TourInfo, TripInfo};

fn tt(name: &str, line: &str, a: &str, b: &str, dep: i32, arr: i32) -> TripInfo {
    TripInfo {
        name: name.into(),
        index: 0,
        line: line.into(),
        from: a.into(),
        terminus: format!("{b} Bf"),
        departure: dep as f64 * 60.0,
        arrival: arr as f64 * 60.0,
        stops: (0..6).map(|s| StopInfo { name: match s { 0 => a.to_string(), 5 => b.to_string(), _ => format!("S{s}") }, id: s, arr: 0.0, dep: 0.0 }).collect(),
        km: (arr - dep) as f64 * 0.4,
    }
}

/// Line 5 back and forth between A and B: tours (number, first departure, trips) of 50 minutes,
/// ten minutes at each end.
fn back_and_forth(tours: &[(&str, i32, i32)]) -> LineInfo {
    let t = tours
        .iter()
        .map(|&(no, from, n)| TourInfo {
            number: no.into(),
            ai_group: String::new(),
            first: 0.0,
            last: 0.0,
            days: "daily".into(),
            runs: true,
            today: true,
            yesterday_night: false,
            next_run: None,
            trips: (0..n).map(|k| if k % 2 == 0 { tt(&format!("5 {no}-{k}"), "5", "A", "B", from + k * 60, from + k * 60 + 50) } else { tt(&format!("5 {no}-{k}"), "5", "B", "A", from + k * 60, from + k * 60 + 50) }).collect(),
        })
        .collect();
    LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: t }
}

/// A company on 4 March 2024 (a Monday) planning line 5 with RosterFlow Pro.
fn company() -> Company {
    let mut c = found(&Founding { name: "Pro".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
    c.game_mode = super::super::GameMode::Normal;
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    assert_eq!(autopilot::set_planner(&mut c, Planner::Pro), Planner::Pro);
    c
}

/// A duty of pieces (from, to), as `shape` would make it (signing in).
fn shaped(spans: &[(i32, i32)]) -> Shaped {
    let start = spans[0].0 - 10;
    let end = spans[spans.len() - 1].1 + 5;
    Shaped { duty: Some(0), spans: spans.iter().map(|&(from, to)| Span { tour: 0, duty: 0, from, to }).collect(), start, end, work: end - start, paid: (end - start).max(240), ..Default::default() }
}

#[test]
fn a_duty_costs_its_hours_its_supplements_and_the_employers_part() {
    let c = company();
    let hour = hour_wage(&c);
    assert!(hour > 0.0, "a company without drivers prices at the market's wage");
    // 06:00 to 14:00 on a Monday: no supplement
    let day = shaped(&[(370, 835)]);
    let p = price(&c, &day, "2024-03-04", hour);
    assert_eq!(p.supplements, 0);
    assert_eq!(p.wages, (hour * day.paid as f64 / 60.0).round() as Cents);
    assert_eq!(p.total, economy::employer_cost(p.wages));
    assert_eq!(p.total, p.wages + p.supplements + p.oncosts);
    // 17:00 to 00:30: the evening's and the night's supplements on top
    let late = shaped(&[(1030, 1465)]);
    let q = price(&c, &late, "2024-03-04", hour);
    assert!(q.supplements > 0 && q.supplement_minutes >= 6 * 60, "{q:?}");
    assert!(q.total > p.total * late.paid as Cents / day.paid as Cents);
    // a split duty is not paid for its gap, nor its supplements
    let mut split = shaped(&[(360, 600), (900, 1100)]);
    split.splits = vec![(605, 890)];
    assert_eq!(worked(&split), vec![(350, 605), (890, 1105)]);
}

#[test]
fn the_day_in_figures_and_the_games_plan() {
    let mut c = company();
    let lines = vec![back_and_forth(&[("1", 360, 14), ("2", 390, 6), ("3", 900, 7)])];
    let date = c.date.clone();
    // nothing built: the day is the game's plan
    let b = board(&c, &lines, &date);
    let (g, gb) = game_plan(&c, &lines, &date);
    assert_eq!(summary(&c, &b, &date), summary(&g, &gb, &date));
    let s = summary(&c, &b, &date);
    assert_eq!(s.duties, counted(&c, &b).len());
    assert_eq!(s.ok + s.warn + s.bad, s.duties);
    assert!(s.cost > 0 && s.paid > 0 && s.driving > 0 && s.driving_share() <= 100.0);
    // made automatically: the game's plan stays what it was, the day's figures change
    auto(&mut c, &lines, &date);
    let b2 = board(&c, &lines, &date);
    let (g2, gb2) = game_plan(&c, &lines, &date);
    assert_eq!(summary(&g2, &gb2, &date), s, "the game's plan is the day without what was built");
    let s2 = summary(&c, &b2, &date);
    assert!(s2.bad <= s.bad, "{s2:?} against {s:?}");
}

#[test]
fn make_duties_automatically_keeps_to_the_longest_duty_and_numbers_by_sign_on() {
    let mut c = company();
    // one bus all day, one in the morning, one in the afternoon: many ways to pair the pieces
    let lines = vec![back_and_forth(&[("1", 330, 17), ("2", 390, 6), ("3", 840, 8), ("4", 420, 9)])];
    let date = c.date.clone();
    c.planning.flow.prefs.goals = Goals { target_from: 360, target_to: 450, longest: 480, same_line: true, ..Goals::default() };
    let n = auto(&mut c, &lines, &date);
    let b = board(&c, &lines, &date);
    assert_eq!(b.figures.assigned, 40, "every trip in a duty");
    let longest_piece = b.shaped.iter().flat_map(|s| s.spans.iter().map(|x| x.to - x.from)).max().unwrap_or(0);
    for s in b.shaped.iter().filter(|s| s.spans.len() > 1) {
        assert!(s.work <= 480, "a duty of several pieces works {} min, the longest allowed 8:00", s.work);
    }
    assert!(longest_piece <= 480);
    // numbered 1001 on in the order they sign on
    let mut rows: Vec<(i32, u32)> = b.shaped.iter().enumerate().filter(|(_, s)| s.spare.is_none()).map(|(row, s)| (s.start, number_of(s, row))).collect();
    assert_eq!(rows.len(), n);
    rows.sort();
    let numbers: Vec<u32> = rows.iter().map(|r| r.1).collect();
    assert_eq!(numbers, (1001..1001 + n as u32).collect::<Vec<_>>());
    // and the goals loosened: no duty longer than ten hours either way
    c.planning.flow.prefs.goals = Goals::default();
    auto(&mut c, &lines, &date);
    let b = board(&c, &lines, &date);
    assert!(b.shaped.iter().filter(|s| s.spans.len() > 1).all(|s| s.work <= 600));
}

#[test]
fn the_checklist_tells_every_rule_and_the_rules_count() {
    let c = company();
    let lines = vec![back_and_forth(&[("1", 360, 8), ("2", 390, 4)])];
    let date = c.date.clone();
    let b = board(&c, &lines, &date);
    for row in counted(&c, &b) {
        let list = checklist(&c, &b, row);
        let titles: Vec<&str> = list.iter().map(|k| k.title).collect();
        for t in [T_DRIVING, T_DAY, T_TARGET, T_CHANGES, T_PIECE, T_PLACES] {
            assert!(titles.contains(&t), "{t} missing in {titles:?}");
        }
        // each issue of the duty is in the list, and the chip counts them
        let (soft, hard) = rule_counts(&c, &b, row);
        let bad = list.iter().filter(|k| k.level == Level::Bad).count();
        assert!(bad <= hard + soft);
        if hard == 0 {
            assert!(!list.iter().any(|k| k.level == Level::Bad && k.title != T_DAY), "{list:?}");
        }
        assert!(list.iter().all(|k| !k.text.is_empty()));
    }
    // a duty of eight hours of work without a break: the meal break missing, in plain words
    let mut b2 = b.clone();
    let row = counted(&c, &b)[0];
    let s = &b.shaped[row];
    let list = checklist(&c, &b2, row);
    assert!(list.iter().any(|k| k.title == T_MEAL));
    b2.shaped[row].work = 1000;
    assert!(checklist(&c, &b2, row).iter().any(|k| k.title == T_DAY && k.level == Level::Bad), "{s:?}");
}

#[test]
fn the_names_are_short_and_say_the_other_lines() {
    assert_eq!(short_tour("131", "131H02"), "H02");
    assert_eq!(short_tour("131", "131/H02"), "H02");
    assert_eq!(short_tour("5", "1"), "01");
    assert_eq!(short_tour("131", "131"), "131");
    assert_eq!(short_tour("NE2", "ne2-07"), "07");
    assert_eq!(short_tour("153", "A12"), "A12");
    let trip = |line: &str, dep: i32, empty: bool| PlannedTrip { name: format!("{line}-{dep}"), line: line.into(), from: "A".into(), to: "B".into(), dep, arr: dep + 30, stops: 8, empty, ..Default::default() };
    let t = TourOfDay { line: "131".into(), number: "131".into(), tour: "131H09".into(), trips: vec![trip("131", 1200, false), trip("NE02", 1450, false), trip("NE10", 1520, false), trip("", 1600, true)], ..Default::default() };
    assert_eq!(short_block(&t), "131 · H09");
    let (mine, other) = piece_lines(&t, &t.trips);
    assert!(mine);
    assert_eq!(other.iter().map(|l| (l.number.as_str(), l.night)).collect::<Vec<_>>(), vec![("NE2", true), ("NE10", true)]);
    // a day line's trips in another number by day: no moon
    let d = TourOfDay { line: "153".into(), number: "153".into(), tour: "153H02".into(), trips: vec![trip("154", 600, false)], ..Default::default() };
    let (mine, other) = piece_lines(&d, &d.trips);
    assert!(!mine && other.len() == 1 && !other[0].night);
}
