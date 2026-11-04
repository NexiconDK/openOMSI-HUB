//! RosterFlow Pro's duties (step 2): the types, the numbers by the day type and their staying
//! across edits, the pool, a duty of pieces of two buses, the marks, the fall-back, "Make duties
//! automatically", and an older file.

use super::super::{found, Founding};
use super::*;
use crate::company::model::CompanyLine;
use crate::company::rosterflow::{board, Span};
use crate::{StopInfo, TourInfo, TripInfo};

fn tt(name: &str, a: &str, b: &str, dep: i32, arr: i32) -> TripInfo {
    TripInfo {
        name: name.into(),
        index: 0,
        line: "5".into(),
        from: a.into(),
        terminus: format!("{b} Bf"),
        departure: dep as f64 * 60.0,
        arrival: arr as f64 * 60.0,
        stops: (0..6).map(|s| StopInfo { name: match s { 0 => a.to_string(), 5 => b.to_string(), _ => format!("S{s}") }, id: s, arr: 0.0, dep: 0.0 }).collect(),
        km: (arr - dep) as f64 * 0.4,
    }
}

fn tour(no: &str, trips: Vec<TripInfo>) -> TourInfo {
    TourInfo { number: no.into(), ai_group: String::new(), first: 0.0, last: 0.0, days: "daily".into(), runs: true, today: true, yesterday_night: false, next_run: None, trips }
}

/// Line 5 back and forth between A and B: tours (number, first departure, trips) of 50 minutes,
/// ten minutes at each end.
fn back_and_forth(tours: &[(&str, i32, i32)]) -> LineInfo {
    let t = tours
        .iter()
        .map(|&(no, from, n)| tour(no, (0..n).map(|k| if k % 2 == 0 { tt(&format!("5 {no}-{k}"), "A", "B", from + k * 60, from + k * 60 + 50) } else { tt(&format!("5 {no}-{k}"), "B", "A", from + k * 60, from + k * 60 + 50) }).collect()))
        .collect();
    LineInfo { name: "Linie5".into(), user_allowed: true, termini: vec!["A".into(), "B".into()], tours: t }
}

/// A company on 4 March 2024 (a Monday) planning with RosterFlow Pro.
fn company() -> Company {
    let mut c = found(&Founding { name: "Pro".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
    c.game_mode = super::super::GameMode::Normal;
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    assert_eq!(autopilot::set_planner(&mut c, Planner::Pro), Planner::Pro);
    c
}

/// Two buses: one from 06:00 to 13:50 (8 trips), one from 06:30 to 10:20 (4 trips).
fn lines() -> Vec<LineInfo> {
    vec![back_and_forth(&[("1", 360, 8), ("2", 390, 4)])]
}

fn numbers(b: &Board) -> Vec<String> {
    b.shaped.iter().enumerate().map(|(row, s)| s.label(row)).collect()
}

/// The block of the board that holds the tour `tour` of the timetable.
fn block(b: &Board, tour: &str) -> usize {
    b.tours.iter().position(|t| t.tour.trim() == tour).expect("the block")
}

/// The row of the duty piece `k` of block `i` is in.
fn row_of(b: &Board, i: usize, k: usize) -> usize {
    let di = st::duty_of(&b.duties, i, k).expect("a duty");
    b.shaped.iter().position(|s| s.duty == Some(di)).expect("its row")
}

/// An edit as the page makes it: on the company, then the numbers kept (`renumber`).
fn edit(c: &mut Company, f: impl FnOnce(&mut Company, u8, &[TourOfDay]) -> Result<(), &'static str>) -> Result<Board, &'static str> {
    let l = lines();
    let date = c.date.clone();
    let before = board(c, &l, &date);
    f(c, before.weekday, &before.tours)?;
    let after = board(c, &l, &date);
    renumber(c, before.weekday, &before, &after);
    tidy_pool(c, before.weekday, &after.tours);
    Ok(board(c, &l, &date))
}

fn shaped(spans: &[(i32, i32)]) -> Shaped {
    let spans: Vec<Span> = spans.iter().enumerate().map(|(k, &(from, to))| Span { tour: k, duty: 0, from, to }).collect();
    let work = spans.iter().map(|s| s.to - s.from).sum();
    Shaped { start: spans.first().map(|s| s.from).unwrap_or(0), end: spans.last().map(|s| s.to).unwrap_or(0), work, spans, ..Default::default() }
}

#[test]
fn the_types_of_a_duty() {
    // one stretch of eight hours, two with a break of an hour: whole
    assert_eq!(kind_of(&shaped(&[(300, 780)])), Kind::Whole);
    assert_eq!(kind_of(&shaped(&[(300, 540), (600, 840)])), Kind::Whole);
    // a gap of an hour and a half or more: split, even when short
    assert_eq!(kind_of(&shaped(&[(360, 540), (900, 1080)])), Kind::Split);
    assert_eq!(kind_of(&shaped(&[(360, 420), (510, 570)])), Kind::Split);
    // under four hours of work: short
    assert_eq!(kind_of(&shaped(&[(420, 580)])), Kind::Short);
    assert_eq!(Kind::Split.label(), "Split duty");
    assert_eq!(Kind::Short.key(), "short");
}

#[test]
fn a_day_nobody_built_runs_rosterflows_duties_numbered_by_the_day_type() {
    let mut c = company();
    let l = lines();
    let pro = board(&c, &l, &c.date.clone());
    // RosterFlow's own duties, numbered in their rows from 1001
    autopilot::set_planner(&mut c, Planner::RosterFlow);
    let plain = board(&c, &l, &c.date.clone());
    assert_eq!(pro.duties, plain.duties);
    assert_eq!(numbers(&pro), numbers(&plain));
    assert_eq!(numbers(&pro)[0], "1001");
    // a Saturday from 2001, a Sunday from 3001
    autopilot::set_planner(&mut c, Planner::Pro);
    assert_eq!(numbers(&board(&c, &l, "2024-03-09"))[0], "2001");
    assert_eq!(numbers(&board(&c, &l, "2024-03-10"))[0], "3001");
    assert_eq!((day_type(&c, 0), day_type(&c, 4), day_type(&c, 5), day_type(&c, 6)), (0, 0, 1, 2));
    assert_eq!(base(&c, 6), 3000);
}

#[test]
fn a_piece_cut_off_waits_in_the_pool_and_the_numbers_stay() {
    let mut c = company();
    let l = lines();
    let b = board(&c, &l, &c.date.clone());
    let (one, two) = (block(&b, "1"), block(&b, "2"));
    let n_two = b.shaped[row_of(&b, two, 0)].label(row_of(&b, two, 0));
    // cut bus 1 after its fourth trip: the piece from there on in the pool
    let at = 4;
    assert!(st::can_cut(&c, &b.tours, one, at).is_ok());
    let b = edit(&mut c, |c, wd, tours| cut_to_pool(c, wd, tours, one, at)).unwrap();
    let wd = b.weekday;
    assert_eq!(c.planning.flow.sheet.pool.len(), 1);
    let pieces = network::duties_of(&b.tours[one]).len();
    assert_eq!(pieces, 2);
    let rest = row_of(&b, one, 1);
    assert!(is_pool_duty(&c, wd, &b.tours, &b.shaped[rest]));
    // (it still runs: numbered among the pool's, from 1501)
    assert_eq!(b.shaped[rest].label(rest), "1501");
    // the other duties keep their numbers
    assert_eq!(b.shaped[row_of(&b, two, 0)].label(row_of(&b, two, 0)), n_two);
    assert_eq!(b.shaped[row_of(&b, one, 0)].label(row_of(&b, one, 0)), "1001");
    // twice in the pool: refused
    assert_eq!(to_pool(&mut c.clone(), wd, &b.tours, (one, 1)), Err(IN_POOL));
    // made a duty of its own: the lowest number free
    let b = edit(&mut c, |c, wd, tours| new_duty(c, wd, tours, (one, 1))).unwrap();
    assert!(c.planning.flow.sheet.pool.is_empty());
    let rest = row_of(&b, one, 1);
    assert_eq!(b.shaped[rest].label(rest), "1003");
    assert_eq!(new_duty(&mut c.clone(), wd, &b.tours, (one, 1)), Err(NOT_IN_POOL));
}

#[test]
fn a_duty_of_pieces_of_two_buses_keeps_its_number_and_overlaps_are_refused() {
    let mut c = company();
    let l = lines();
    let b = board(&c, &l, &c.date.clone());
    let (one, two) = (block(&b, "1"), block(&b, "2"));
    // bus 1 cut at 10:00: its afternoon piece in the pool
    let at = 4;
    let b = edit(&mut c, |c, wd, tours| cut_to_pool(c, wd, tours, one, at)).unwrap();
    let wd = b.weekday;
    let n_two = b.shaped[row_of(&b, two, 0)].label(row_of(&b, two, 0));
    // bus 1's morning piece overlaps bus 2's: refused, nothing changed
    let was = c.clone();
    assert_eq!(add_to_duty(&mut c, wd, &b.tours, (one, 0), (two, 0)), Err(OVERLAP));
    assert_eq!(c, was);
    // bus 2 (06:30-10:20) and the afternoon of bus 1 (10:00 on) overlap too; bus 1's afternoon
    // after bus 2's day does not: make bus 2 shorter first - cut it after its second trip
    edit(&mut c, |c, wd, tours| cut_to_pool(c, wd, tours, two, 2)).unwrap();
    assert_eq!(c.planning.flow.sheet.pool.len(), 2);
    // bus 2's morning (06:30-08:20) and bus 1's afternoon (10:00-13:50): one driver's duty
    let b = edit(&mut c, |c, wd, tours| add_to_duty(c, wd, tours, (one, 1), (two, 0))).unwrap();
    let row = row_of(&b, two, 0);
    assert_eq!(row, row_of(&b, one, 1), "one duty");
    assert_eq!(b.shaped[row].spans.len(), 2);
    assert_eq!(b.shaped[row].label(row), n_two, "the duty keeps its number");
    assert_eq!(kind_of(&b.shaped[row]), Kind::Split, "a gap of 1 h 40");
    assert_eq!(c.planning.flow.sheet.pool.len(), 1, "bus 2's second piece is still in the pool");
    // the same piece again: the same duty
    assert_eq!(add_to_duty(&mut c.clone(), wd, &b.tours, (one, 1), (two, 0)), Err(st::SAME_DUTY));
    // taken out again: back in the pool, the duty keeps its number
    let b = edit(&mut c, |c, wd, tours| to_pool(c, wd, tours, (one, 1))).unwrap();
    let row = row_of(&b, two, 0);
    assert_eq!(b.shaped[row].spans.len(), 1);
    assert_eq!(b.shaped[row].label(row), n_two);
    assert!(pooled(&c, wd, &st::piece_ref(&b.tours, one, 1).unwrap()));
    // dissolved: every piece of it in the pool
    let b = edit(&mut c, |c, wd, tours| dissolve(c, wd, tours, (two, 0))).unwrap();
    assert_eq!(c.planning.flow.sheet.pool.len(), 3);
    assert!(b.shaped.iter().enumerate().filter(|(_, s)| !is_pool_duty(&c, wd, &b.tours, s)).all(|(row, s)| s.label(row) == "1001"));
}

#[test]
fn a_piece_next_to_the_duty_on_its_bus_joins_it_again() {
    let mut c = company();
    let l = lines();
    let b = board(&c, &l, &c.date.clone());
    let one = block(&b, "1");
    let b = edit(&mut c, |c, wd, tours| cut_to_pool(c, wd, tours, one, 4)).unwrap();
    assert_eq!(network::duties_of(&b.tours[one]).len(), 2);
    let b = edit(&mut c, |c, wd, tours| add_to_duty(c, wd, tours, (one, 1), (one, 0))).unwrap();
    assert_eq!(network::duties_of(&b.tours[one]).len(), 1, "the cut is gone");
    assert!(c.planning.flow.sheet.pool.is_empty());
    assert!(c.planning.joined.is_empty());
    assert_eq!(b.shaped[row_of(&b, one, 0)].label(row_of(&b, one, 0)), "1001");
}

#[test]
fn the_marks_of_a_duty() {
    let mut c = company();
    let l = lines();
    let b = board(&c, &l, &c.date.clone());
    let (one, two) = (block(&b, "1"), block(&b, "2"));
    // bus 2's first trip alone (06:30-07:20, A to B), and bus 1 from its trip at 08:00 (A to B)
    edit(&mut c, |c, wd, tours| cut_to_pool(c, wd, tours, two, 1)).unwrap();
    edit(&mut c, |c, wd, tours| cut_to_pool(c, wd, tours, one, 2)).unwrap();
    let b = edit(&mut c, |c, wd, tours| add_to_duty(c, wd, tours, (one, 1), (two, 0))).unwrap();
    let row = row_of(&b, two, 0);
    // (bus 2 ends at B at 07:20, bus 1 leaves A at 08:00: 40 minutes, 10 needed - no mark)
    assert!(marks(&c, &b, row).is_empty(), "{:?}", marks(&c, &b, row));
    // ten minutes needed between two stops: more than the 40 there are
    c.planning.flow.prefs.change.other_stop = 45;
    let m = marks(&c, &b, row);
    assert_eq!(m.len(), 1);
    assert!(m[0].hard && m[0].text == M_CHANGE);
    assert!(m[0].args.contains(&("have", "40 min".to_string())) && m[0].args.contains(&("need", "45 min".to_string())));
    // a split duty spread over 12 h 30
    let s = shaped(&[(300, 540), (900, 1100)]);
    let mut b2 = b.clone();
    b2.shaped = vec![s];
    let m = marks(&c, &b2, 0);
    assert!(m.iter().any(|x| x.text == M_SPREAD && x.hard));
    assert!(marks(&c, &b2, 5).is_empty());
}

#[test]
fn make_duties_automatically_empties_the_pool() {
    let mut c = company();
    let l = lines();
    let date = c.date.clone();
    let b = board(&c, &l, &date);
    let one = block(&b, "1");
    edit(&mut c, |c, wd, tours| cut_to_pool(c, wd, tours, one, 4)).unwrap();
    assert!(!c.planning.flow.sheet.pool.is_empty());
    let before = board(&c, &l, &date);
    let n = auto(&mut c, &l, &date);
    assert!(c.planning.flow.sheet.pool.is_empty());
    let after = board(&c, &l, &date);
    renumber(&mut c, after.weekday, &before, &after);
    let after = board(&c, &l, &date);
    assert_eq!(n, after.shaped.len());
    assert_eq!(after.figures.assigned, 12, "every trip in a duty");
    // numbered from 1001 without a gap
    let mut got: Vec<u32> = after.shaped.iter().enumerate().map(|(row, s)| number_of(s, row)).collect();
    got.sort();
    assert_eq!(got, (1001..1001 + n as u32).collect::<Vec<_>>());
}

#[test]
fn an_older_file_loads_as_it_was() {
    // (no pool, no numbers, no change times: the defaults)
    let s: rf::Sheet = serde_json::from_str("{}").unwrap();
    assert!(s.pool.is_empty() && s.numbers.is_empty() && s.is_empty());
    let p: rf::Prefs = serde_json::from_str("{}").unwrap();
    assert_eq!(p.change, Change { same_stop: 5, other_stop: 10 });
    // and what is empty is left out of the saved file
    let text = serde_json::to_string(&rf::Sheet::default()).unwrap();
    assert!(!text.contains("pool") && !text.contains("numbers"));
    // without RosterFlow Pro the numbers are RosterFlow's: no kept numbers are used
    let mut c = company();
    autopilot::set_planner(&mut c, Planner::RosterFlow);
    let l = lines();
    let b = board(&c, &l, &c.date.clone());
    assert!(b.shaped.iter().all(|s| s.number.is_none()));
    assert!(!on(&c));
}

#[test]
fn the_texts_are_all_there() {
    let t = texts();
    assert!(t.contains(&M_CHANGE) && t.contains(&"Whole") && t.len() >= 10);
}
