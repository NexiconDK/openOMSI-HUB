//! (NexiconDK) Bus Company Online phase 6 (docs/omsihub/BUS_COMPANY_ONLINE.md): what a player must
//! have done to found a company on the server besides paying for it (decided 26 October 2026) -
//! each asked or not by `company-server.cfg` (`FOUND_CFG`):
//!
//! - **trips** driven for a company of the server, as its world measured them (`found_trips`):
//!   every trip of his a company booked (`online::Pay`, paid or not) counts once, as the main
//!   server takes the pay in (`take_trip`);
//! - the **leadership course** on the server (`found_course`): booked for `course_price` of his
//!   own money, done `course_days` server days later;
//! - a **driving test** passed on the server (`found_test`): booked for a class of bus, his next
//!   trip with a bus of that class is the test, judged as the career's test is
//!   (`career::judge_exam`: to its end, punctual, smooth, safe) by his company's process from the
//!   trip the world measured.

use super::career::{Booking, ExamRecord, LicenceClass};
use super::dates;
use super::model::Cents;
use super::online::Pay;
use super::server_players::Player;
use serde_json::{json, Value};
use std::collections::HashMap;

/// What founding asks (`company-server.cfg`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Needs {
    /// Trips driven for a company of the server (0: none).
    pub trips: u32,
    /// The leadership course, what it costs (cents) and its days.
    pub course: bool,
    pub course_price: Cents,
    pub course_days: i64,
    /// A driving test passed on the server.
    pub test: bool,
}

impl Default for Needs {
    fn default() -> Needs {
        Needs { trips: 20, course: true, course_price: 5_000_00, course_days: 3, test: true }
    }
}

impl Needs {
    /// The settings' keys (`company-server.cfg`, lower case).
    pub fn of(kv: &HashMap<String, String>) -> Needs {
        let d = Needs::default();
        let num = |k: &str| kv.get(k).and_then(|v| v.trim().parse::<i64>().ok());
        let flag = |k: &str, d: bool| kv.get(k).map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")).unwrap_or(d);
        Needs {
            trips: num("found_trips").map_or(d.trips, |n| n.clamp(0, 100_000) as u32),
            course: flag("found_course", d.course),
            course_price: num("course_price").map_or(d.course_price, |n| n.max(0) * 100),
            course_days: num("course_days").map_or(d.course_days, |n| n.clamp(0, 60)),
            test: flag("found_test", d.test),
        }
    }
}

/// The founding's part of the settings, added to a `company-server.cfg` written before it.
pub const FOUND_CFG: &str = "
# what a player must have done to found a company, besides its price: trips driven for a company
# of the server (0: none), the leadership course (1: asked; its price in whole units and its
# days) and a driving test passed on the server (1: asked)
found_trips = 20
found_course = 1
course_price = 5000
course_days = 3
found_test = 1
";

/// Why founding or a booking is refused (English: keys of the translations).
pub const NEED_TRIPS: &str = "Drive %{n} more trips for a company on the server first (%{done} of %{need}).";
pub const NEED_COURSE: &str = "Take the leadership course first.";
pub const COURSE_RUNNING: &str = "Your leadership course ends on %{date}.";
pub const NEED_TEST: &str = "Pass a driving test on the server first.";
pub const COURSE_HAVE: &str = "You have taken the leadership course already.";
pub const COURSE_MONEY: &str = "The leadership course costs %{price}: you have %{money}.";
pub const TEST_BOOKED: &str = "A driving test is booked already.";
pub const TEST_HELD: &str = "You passed this test already.";
pub const NO_CLASS: &str = "There is no such class of bus.";

pub fn texts() -> Vec<&'static str> {
    vec![NEED_TRIPS, NEED_COURSE, COURSE_RUNNING, NEED_TEST, COURSE_HAVE, COURSE_MONEY, TEST_BOOKED, TEST_HELD, NO_CLASS]
}

/// A refusal: its words and their arguments.
pub type Why = (&'static str, Vec<(String, String)>);

fn arg(k: &str, v: impl ToString) -> (String, String) {
    (k.to_string(), v.to_string())
}

/// Whole units of money, for the words.
fn whole(c: Cents) -> String {
    format!("{}", c / 100)
}

/// Whether his leadership course is done on the server's day `today`.
pub fn course_done(p: &Player, today: &str) -> bool {
    !p.course_until.is_empty() && dates::between(&p.course_until, today) >= 0
}

/// Whether he passed a driving test on the server.
pub fn test_passed(p: &Player) -> bool {
    !p.tests.licences.is_empty()
}

/// What he still lacks to found a company on `today` (the first of it), None: nothing.
pub fn missing(p: &Player, n: &Needs, today: &str) -> Option<Why> {
    if p.trips < n.trips {
        return Some((NEED_TRIPS, vec![arg("n", n.trips - p.trips), arg("done", p.trips), arg("need", n.trips)]));
    }
    if n.course && !course_done(p, today) {
        if !p.course_until.is_empty() {
            return Some((COURSE_RUNNING, vec![arg("date", &p.course_until)]));
        }
        return Some((NEED_COURSE, Vec::new()));
    }
    if n.test && !test_passed(p) {
        return Some((NEED_TEST, Vec::new()));
    }
    None
}

/// The leadership course booked on `today`: paid from his money, done `course_days` later.
pub fn book_course(p: &mut Player, n: &Needs, today: &str) -> Result<(), Why> {
    if course_done(p, today) {
        return Err((COURSE_HAVE, Vec::new()));
    }
    if !p.course_until.is_empty() {
        return Err((COURSE_RUNNING, vec![arg("date", &p.course_until)]));
    }
    if p.money < n.course_price {
        return Err((COURSE_MONEY, vec![arg("price", whole(n.course_price)), arg("money", whole(p.money))]));
    }
    p.money -= n.course_price;
    p.course_until = dates::add(today, n.course_days);
    Ok(())
}

/// A driving test booked for buses of `class` (`now`: Unix seconds): his next trip with one is it.
pub fn book_test(p: &mut Player, class: &str, now: u64) -> Result<LicenceClass, Why> {
    let Some(class) = LicenceClass::ALL.into_iter().find(|c| serde_json::to_value(c).ok().and_then(|v| v.as_str().map(str::to_string)).is_some_and(|k| k.eq_ignore_ascii_case(class.trim()))) else {
        return Err((NO_CLASS, Vec::new()));
    };
    if p.tests.licences.contains(&class) {
        return Err((TEST_HELD, Vec::new()));
    }
    if p.tests.booked.is_some() {
        return Err((TEST_BOOKED, Vec::new()));
    }
    p.tests.booked = Some(Booking { class, since: now });
    Ok(class)
}

/// The test booked, called off.
pub fn cancel_test(p: &mut Player) {
    p.tests.booked = None;
}

/// A trip of his a company booked (its pay, `online::Pay`): counted; the test he booked taken
/// when it is the first trip since, with a bus of the test's class. The test taken.
pub fn take_trip(p: &mut Player, pay: &Pay) -> Option<ExamRecord> {
    p.trips += 1;
    let b = p.tests.booked.clone()?;
    let judged = pay.exam.as_ref()?;
    if pay.class != Some(b.class) || pay.at < b.since {
        return None;
    }
    let rec = ExamRecord { class: b.class, time: pay.at, line: pay.line.clone(), passed: judged.passed, score: judged.score, criteria: judged.criteria.clone() };
    if rec.passed && !p.tests.licences.contains(&b.class) {
        p.tests.licences.push(b.class);
    }
    p.tests.booked = None;
    p.tests.exams.insert(0, rec.clone());
    p.tests.exams.truncate(10);
    Some(rec)
}

/// What the launcher shows of it (`server_api`'s `me`, `companies`): each need with how far he is.
pub fn status_json(p: &Player, n: &Needs, today: &str) -> Value {
    let key = |c: LicenceClass| serde_json::to_value(c).unwrap_or(Value::Null);
    let last = p.tests.exams.first().map(|e| json!({ "class": key(e.class), "passed": e.passed, "score": e.score, "criteria": e.criteria }));
    json!({
        "trips": { "need": n.trips, "done": p.trips, "ok": p.trips >= n.trips },
        "course": { "need": n.course, "price": whole(n.course_price), "days": n.course_days, "until": p.course_until, "ok": !n.course || course_done(p, today), "running": !p.course_until.is_empty() && !course_done(p, today) },
        "test": { "need": n.test, "booked": p.tests.booked.as_ref().map(|b| key(b.class)), "passed": p.tests.licences.iter().map(|c| key(*c)).collect::<Vec<_>>(), "last": last, "ok": !n.test || test_passed(p) },
        "can": missing(p, n, today).is_none(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::company::career::Criterion;
    use crate::company::online::Judged;

    fn kv(text: &str) -> HashMap<String, String> {
        text.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
    }

    /// What founding asks, step by step: the trips, the course (paid, done days later), a test
    /// passed - a trip with a bus of another class is no test, a failed one can be booked again.
    #[test]
    fn a_player_founds_after_his_trips_the_course_and_a_test() {
        let n = Needs::of(&kv(FOUND_CFG));
        assert_eq!(n, Needs::default());
        assert_eq!(Needs::of(&kv("found_trips = 0\nfound_course = 0\nfound_test = no")), Needs { trips: 0, course: false, test: false, ..Needs::default() });
        let mut p = Player { money: 6_000_00, ..Default::default() };
        let today = "2026-10-26";
        assert_eq!(missing(&p, &n, today).map(|w| w.0), Some(NEED_TRIPS));
        // (trips counted as the companies book them)
        let pay = |class: Option<LicenceClass>, passed: bool, at: u64| Pay { class, exam: Some(Judged { passed, score: if passed { 100 } else { 50 }, criteria: vec![Criterion { rule: "finish".into(), passed: true, value: 1.0, limit: 1.0 }] }), at, line: "5".into(), ..Default::default() };
        for _ in 0..20 {
            assert!(take_trip(&mut p, &pay(Some(LicenceClass::Solo), true, 1)).is_none(), "no test booked");
        }
        assert_eq!(p.trips, 20);
        assert_eq!(missing(&p, &n, today).map(|w| w.0), Some(NEED_COURSE));
        // the course: paid, done three days later
        assert!(book_course(&mut Player { money: 100_00, ..p.clone() }, &n, today).is_err_and(|w| w.0 == COURSE_MONEY));
        book_course(&mut p, &n, today).unwrap();
        assert_eq!((p.money, p.course_until.as_str()), (1_000_00, "2026-10-29"));
        assert!(book_course(&mut p, &n, today).is_err_and(|w| w.0 == COURSE_RUNNING));
        assert_eq!(missing(&p, &n, "2026-10-28").map(|w| w.0), Some(COURSE_RUNNING));
        assert_eq!(missing(&p, &n, "2026-10-29").map(|w| w.0), Some(NEED_TEST));
        assert!(book_course(&mut p, &n, "2026-10-29").is_err_and(|w| w.0 == COURSE_HAVE));
        // the test: booked for a solo bus at 1000 s; an articulated bus's trip is not it, an
        // earlier trip neither; failed, booked again, passed
        assert!(book_test(&mut p, "tram", 1000).is_err_and(|w| w.0 == NO_CLASS));
        assert_eq!(book_test(&mut p, "Solo", 1000), Ok(LicenceClass::Solo));
        assert!(book_test(&mut p, "midi", 1000).is_err_and(|w| w.0 == TEST_BOOKED));
        assert!(take_trip(&mut p, &pay(Some(LicenceClass::Articulated), true, 1100)).is_none());
        assert!(take_trip(&mut p, &pay(Some(LicenceClass::Solo), true, 900)).is_none());
        let failed = take_trip(&mut p, &pay(Some(LicenceClass::Solo), false, 1200)).expect("taken");
        assert!(!failed.passed && p.tests.booked.is_none() && !test_passed(&p));
        book_test(&mut p, "solo", 1300).unwrap();
        assert!(take_trip(&mut p, &pay(Some(LicenceClass::Solo), true, 1400)).expect("taken").passed);
        assert!(test_passed(&p) && missing(&p, &n, "2026-10-29").is_none());
        assert!(book_test(&mut p, "solo", 1500).is_err_and(|w| w.0 == TEST_HELD));
        assert_eq!(p.trips, 24);
        let st = status_json(&p, &n, "2026-10-29");
        assert_eq!((st["can"].as_bool(), st["test"]["passed"][0].as_str(), st["test"]["last"]["passed"].as_bool()), (Some(true), Some("solo"), Some(true)));
        // (a booking called off)
        book_test(&mut p, "articulated", 1600).unwrap();
        cancel_test(&mut p);
        assert!(p.tests.booked.is_none());
    }
}
