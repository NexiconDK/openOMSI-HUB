//! RosterFlow Pro: a driver calls in sick on the day itself (the player's, NexiconDK 4 November
//! 2026: "Sygemelding om morgenen" - the phone rings at 04:50, a duty is missing in 25 minutes).
//!
//! Besides the night's illnesses (`staff::after_day`, known in the morning), a driver planned
//! for the company's day can call in sick before his first duty of it: shortly before an early
//! duty, in the morning or an hour or two before a day duty, mostly in the early afternoon before
//! a late one (30 minutes to 3 hours before it). The day's
//! calls are drawn when the clock writes the day (`clock::script`, `draws`) - the same for the
//! same company, day and driver (`Rng`) -, more often for a stressed or unhappy driver and an
//! unreliable one, by the game mode's share of illness (`Rules::sickness`); a few a week in a
//! company of ten drivers. When one comes (`call`): he is ill (`Employee::sick_until`, the same
//! illness the staff card and the calendar show: from that day, one to three days), the control
//! room has his message, the feed says it, and the player is asked to fill the duty (the
//! notice's "Find a driver ›" opens the calendar's popup on it). A call not seen yet (`Ops::sick`,
//! `unseen`) shows also while the game's clock leads (the clock does not stop then).

use super::model::Company;
use super::plan::{DayPlan, Who};
use super::rng::Rng;
use super::{dates, staff};
use serde::{Deserialize, Serialize};

/// A driver's call of the company's day (kept until the player has seen it, and for the day).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct SickCall {
    pub date: String,
    pub driver: u32,
    /// His first duty of the day (its number) and when he would have signed on (minutes).
    pub n: u32,
    pub start: i32,
    /// When he called (the company's clock, `clock::now`).
    pub at: i64,
    /// The player has seen it (the notice closed).
    #[serde(default)]
    pub seen: bool,
}

/// The chance a planned driver calls in sick on a day, before the game mode's share and his
/// stress, mood and reliability: a few a week in a company of ten drivers.
pub const CHANCE: f64 = 0.025;
/// The earliest a driver calls (minutes of the day): the control room opens at 04:00.
pub const EARLIEST: i32 = 4 * 60;

/// The chance driver `e` calls in sick on a day he is planned (his stress and mood, his
/// reliability, the game mode's share of illness).
pub fn chance(c: &Company, e: &super::model::Employee) -> f64 {
    let stressed = 1.0 + (e.stress - 30.0).max(0.0) / 100.0 * 1.5;
    let unhappy = if e.satisfaction < 35.0 { 1.5 } else { 1.0 };
    (CHANCE * c.game_mode.rules().sickness * (1.6 - e.reliability).max(0.2) * stressed * unhappy).clamp(0.0, 0.5)
}

/// The calls of the day's plan `dp` (the company's day; RosterFlow Pro its planner; `duties` the
/// day's, as the Vagter tab numbers them): (when, driver, his first duty, its sign-on), the
/// earliest first. A driver ill or on holiday already, or without a numbered duty, does not call.
pub fn draws(c: &Company, dp: &DayPlan, duties: &[super::pro_turnus::ProDuty]) -> Vec<(i32, u32, u32, i32)> {
    if !super::pro_duties::on(c) || dp.date != c.date {
        return Vec::new();
    }
    let mut out = Vec::new();
    for e in c.staff.iter().filter(|e| staff::drives(e) && e.employed_on(&dp.date) && !e.absent(&dp.date)) {
        if dp.duties_of(Who::Staff(e.id)).is_empty() {
            continue;
        }
        let super::pro_ops::DayIs::Duties { numbers, start, end, .. } = super::pro_ops::driver_day(c, e, dp, duties).is else { continue };
        let Some(n) = numbers.first().copied().filter(|n| *n > 0) else { continue };
        let mut rng = Rng::of(&[&c.id, &dp.date, "pro-sick"], e.id as i64);
        // (round D: a tired driver calls more often - `pro_tired`)
        let tired = super::pro_tired::today(e, &dp.date, Some((start, end))).level;
        // (round E: more in winter - `pro_season`)
        if !rng.chance((chance(c, e) * super::pro_tired::sick(tired) * super::pro_season::sick(c, &dp.date)).min(0.6)) {
            continue;
        }
        // (spread before each kind of duty: an early one's driver calls shortly before it; a day
        // duty's in the morning or an hour or two before; a late one's mostly in the early
        // afternoon, 30 minutes to 3 hours before it - "Sofie calls at 14:05, 1006 at 14:45" -,
        // now and then already in the morning)
        let at = if start <= 8 * 60 {
            start - rng.int(20, 75) as i32
        } else if start < 12 * 60 {
            if rng.chance(0.5) { rng.int(5 * 60, 7 * 60 + 30) as i32 } else { start - rng.int(30, 120) as i32 }
        } else if rng.chance(0.25) {
            rng.int(6 * 60, 9 * 60) as i32
        } else {
            start - rng.int(30, 180) as i32
        };
        let at = (at.max(EARLIEST).min(start - 10).max(0) / 5) * 5;
        out.push((at, e.id, n, start));
    }
    out.sort();
    out
}

/// The texts of a call (English: keys of the translations).
pub const CALLED: &str = "%{name} has called in sick – duty %{n} is missing in %{min} min";
pub const CALLED_NOW: &str = "%{name} has called in sick – duty %{n} is missing now";
/// His message to the control room.
pub const SAYS: &str = "I'm ill and can't come in today. Sorry - duty %{n}.";

/// Driver `driver` calls in sick at the company's minute `at` (his first duty `n` signs on at
/// `start`): ill from today, one to three days (none if he is ill or away already, or the duty
/// began); his message to the control room, the call kept for the player's notice. Whether he
/// called.
pub fn call(c: &mut Company, driver: u32, n: u32, start: i32, at: i64) -> bool {
    let date = c.date.clone();
    let Some(e) = c.staff.iter().find(|e| e.id == driver) else { return false };
    if e.absent(&date) || !e.employed_on(&date) || (at.rem_euclid(super::clock::DAY) as i32) >= start {
        return false;
    }
    let mut rng = Rng::of(&[&c.id, &date, "pro-sick-days"], driver as i64);
    let until = dates::add(&date, rng.int(0, 2));
    let name = e.name.clone();
    if let Some(e) = c.staff.iter_mut().find(|e| e.id == driver) {
        e.sick_until = Some(until);
    }
    let from = super::messages::From::Driver { line: String::new(), tour: String::new(), name };
    super::messages::send(c, from, super::messages::To::Control, super::messages::Priority::Urgent, SAYS, vec![("n".into(), super::pro_duties::shown_number(c, &date, n))], false, None);
    c.planning.ops.sick.retain(|x| !(x.date == date && x.driver == driver));
    c.planning.ops.sick.push(SickCall { date, driver, n, start, at, seen: false });
    true
}

/// The feed's words for a call: its template and places.
pub fn words(c: &Company, x: &SickCall) -> (&'static str, Vec<(String, String)>) {
    let name = c.employee(x.driver).map(|e| e.name.clone()).unwrap_or_default();
    let left = super::clock::moment(&x.date, x.start as i64) - super::clock::now(c);
    let mut args = vec![("name".to_string(), name), ("n".to_string(), super::pro_duties::shown_number(c, &x.date, x.n))];
    if left > 0 {
        args.push(("min".into(), left.to_string()));
        (CALLED, args)
    } else {
        (CALLED_NOW, args)
    }
}

/// The first call of the company's day the player has not seen yet.
pub fn unseen(c: &Company) -> Option<&SickCall> {
    c.planning.ops.sick.iter().find(|x| !x.seen && x.date == c.date)
}

/// The call of driver `driver` today seen (every one, None).
pub fn seen(c: &mut Company, driver: Option<u32>) {
    let date = c.date.clone();
    for x in c.planning.ops.sick.iter_mut().filter(|x| x.date == date && driver.is_none_or(|d| d == x.driver)) {
        x.seen = true;
    }
}

/// Every English text of this module.
pub fn texts() -> Vec<&'static str> {
    vec![CALLED, CALLED_NOW, SAYS]
}
