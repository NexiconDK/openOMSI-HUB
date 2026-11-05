//! RosterFlow Pro, round D (the player's, NexiconDK 5 November 2026: "Morgenmenneske eller
//! aftenmenneske"): some drivers like the early duties, some the late ones, some do not mind.
//!
//! - **Who is which** (`of`, `Employee::shift_pref`): each driver's own, kept in the company's
//!   file; a driver of before has one drawn from their number and name (the same every time,
//!   `seed`) and kept when the company is opened (`seed_all`): about a third early, a third late,
//!   a third neither.
//! - **Early or late** (`kind`): a duty signing on before 08:00 (`EARLY_BEFORE`) is early, one
//!   signing on at 13:00 or later (`LATE_FROM`) late; the day duties between are neither.
//! - **What it does**: asked for a duty they like, a driver says yes more often (`bias`, ×1.25),
//!   for one they do not less often (×0.7); a day worked on a duty they like makes them a little
//!   more content (`after_day`, +0.4), one they do not a little less (−0.6). The dispatcher's own
//!   choice between two drivers otherwise equal takes the one who likes the duty
//!   (`plan::day_plan`, `fit`). The rotas are not changed: every driver goes through all the
//!   weeks of a rota.

use super::model::{Company, Employee};
use super::pro_duties;
use super::rng::Rng;
use serde::{Deserialize, Serialize};

/// A duty signing on before this (minutes of the day) is early.
pub const EARLY_BEFORE: i32 = 8 * 60;
/// A duty signing on at this or later is late.
pub const LATE_FROM: i32 = 13 * 60;
/// The yes chance of a duty they like, and of one they do not, against another.
pub const LIKED: f64 = 1.25;
pub const DISLIKED: f64 = 0.7;
/// What a day on a duty they like, or one they do not, does to their satisfaction.
pub const GLAD: f64 = 0.4;
pub const SORRY: f64 = 0.6;

/// What a driver likes.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ShiftPref {
    /// Early duties: up early, home early.
    Early,
    /// Late duties: a slow morning.
    Late,
    #[default]
    Neither,
}

/// What kind a duty signing on at `start` is: early, late, or neither (a day duty).
pub fn kind(start: i32) -> ShiftPref {
    let m = start.rem_euclid(1440);
    if m < EARLY_BEFORE {
        ShiftPref::Early
    } else if m >= LATE_FROM {
        ShiftPref::Late
    } else {
        ShiftPref::Neither
    }
}

/// The one drawn for a driver who has none yet: from their number and name, the same every time.
pub fn seed(e: &Employee) -> ShiftPref {
    let mut rng = Rng::of(&[&e.name, "shift-pref"], e.id as i64);
    match rng.f64() {
        x if x < 0.35 => ShiftPref::Early,
        x if x < 0.65 => ShiftPref::Late,
        _ => ShiftPref::Neither,
    }
}

/// What driver `e` likes.
pub fn of(e: &Employee) -> ShiftPref {
    e.shift_pref.unwrap_or_else(|| seed(e))
}

/// Every driver without one has theirs drawn and kept (`store`, when the company is opened; a
/// new driver's when hired).
pub fn seed_all(c: &mut Company) {
    for e in c.staff.iter_mut().filter(|e| e.shift_pref.is_none()) {
        e.shift_pref = Some(seed(e));
    }
}

/// How a duty signing on at `start` fits what `e` likes: 1 liked, -1 not, 0 all the same.
pub fn fit(e: &Employee, start: i32) -> i8 {
    match (of(e), kind(start)) {
        (ShiftPref::Neither, _) | (_, ShiftPref::Neither) => 0,
        (a, b) if a == b => 1,
        _ => -1,
    }
}

/// What it does to `e`'s yes chance of a duty signing on at `start` (RosterFlow Pro).
pub fn bias(c: &Company, e: &Employee, start: i32) -> f64 {
    if !pro_duties::on(c) {
        return 1.0;
    }
    match fit(e, start) {
        1 => LIKED,
        -1 => DISLIKED,
        _ => 1.0,
    }
}

/// The night after a day (`day::close_day`; `pieces` the drivers' pieces of work: id, from, to):
/// each driver's day by its first sign-on - liked, a little more content; not, a little less.
pub fn after_day(c: &mut Company, pieces: &[(u32, i32, i32)]) {
    if !pro_duties::on(c) {
        return;
    }
    for e in c.staff.iter_mut() {
        let Some(first) = pieces.iter().filter(|x| x.0 == e.id && x.2 > x.1).map(|x| x.1).min() else { continue };
        match fit(e, first) {
            1 => e.satisfaction = (e.satisfaction + GLAD).min(100.0),
            -1 => e.satisfaction = (e.satisfaction - SORRY).max(0.0),
            _ => {}
        }
    }
}

/// What it is called on the staff card and in the popup (English, translated when shown).
pub fn label(p: ShiftPref) -> Option<&'static str> {
    match p {
        ShiftPref::Early => Some(PREFERS_EARLY),
        ShiftPref::Late => Some(PREFERS_LATE),
        ShiftPref::Neither => None,
    }
}

pub const PREFERS_EARLY: &str = "🌅 Prefers early duties";
pub const PREFERS_LATE: &str = "🌙 Prefers late duties";
pub const NO_PREFERENCE: &str = "No preference";
pub const SHIFTS: &str = "Preference";

/// Every English text this module hands the pages.
pub fn texts() -> Vec<&'static str> {
    vec![PREFERS_EARLY, PREFERS_LATE, NO_PREFERENCE, SHIFTS]
}
