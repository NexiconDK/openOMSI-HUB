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

//!
//! (NexiconDK, 22 November 2026: "tager 'systemet' ikke højde for om chaufføren foretrækker morgen,
//! dag eller aftendelen?" and "en der hedder 'Aften/Nat'") Five now: morning, day, evening,
//! evening and night, none - a duty is early (signing on before 08:00), a day one (before 13:00),
//! a late one (before 20:00) or a night one. A driver of before with "neither" or "late" may come
//! to "day" or "evening and night" (`seed_all`, once, the same every time). The rotas count too: a
//! rota's time (`slot_of_rota`), the drivers who suit it first when one is chosen for a week and
//! when the automatic rota fills its empty weeks (`pro_turnus::auto`); a driver who works week
//! after week on duties he does not like asks for another rota (`pro_wish::WishKind::OtherRota`).
use super::model::{Company, Employee};
use super::pro_duties;
use super::rng::Rng;
use serde::{Deserialize, Serialize};

/// A duty signing on before this (minutes of the day) is early.
pub const EARLY_BEFORE: i32 = 8 * 60;
/// A duty signing on at this or later is late.
pub const LATE_FROM: i32 = 13 * 60;
/// (22 November 2026) A duty signing on at this or later (or before `NIGHT_UNTIL`) is a night one.
pub const NIGHT_FROM: i32 = 20 * 60;
pub const NIGHT_UNTIL: i32 = 3 * 60;
/// The yes chance of a duty they like, and of one they do not, against another.
pub const LIKED: f64 = 1.25;
pub const DISLIKED: f64 = 0.7;
/// What a day on a duty they like, or one they do not, does to their satisfaction.
pub const GLAD: f64 = 0.4;
pub const SORRY: f64 = 0.6;
/// (22 November 2026) Days on duties he does not like, less those he likes, before a driver asks
/// for another rota (`Employee::disliked`).
pub const ASKS_ROTA_AFTER: u16 = 10;

/// What a driver likes - and (`kind`) what a duty is.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug, Default, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ShiftPref {
    /// Early duties: up early, home early.
    Early,
    /// (22 November 2026) Day duties: neither up early nor home late.
    Day,
    /// Late duties: a slow morning.
    Late,
    /// (22 November 2026) Late and night duties ("Aften/Nat"); of a duty: a night one.
    LateNight,
    #[default]
    Neither,
}

/// What kind a duty signing on at `start` is: early, a day one, late, or a night one.
pub fn kind(start: i32) -> ShiftPref {
    let m = start.rem_euclid(1440);
    if m < NIGHT_UNTIL || m >= NIGHT_FROM {
        ShiftPref::LateNight
    } else if m < EARLY_BEFORE {
        ShiftPref::Early
    } else if m < LATE_FROM {
        ShiftPref::Day
    } else {
        ShiftPref::Late
    }
}

/// The one drawn for a driver who has none yet: from their number and name, the same every time.
pub fn seed(e: &Employee) -> ShiftPref {
    let mut rng = Rng::of(&[&e.name, "shift-pref"], e.id as i64);
    match rng.f64() {
        x if x < 0.30 => ShiftPref::Early,
        x if x < 0.50 => ShiftPref::Day,
        x if x < 0.70 => ShiftPref::Late,
        x if x < 0.80 => ShiftPref::LateNight,
        _ => ShiftPref::Neither,
    }
}

/// What driver `e` likes.
pub fn of(e: &Employee) -> ShiftPref {
    e.shift_pref.unwrap_or_else(|| seed(e))
}

/// Every driver without one has theirs drawn and kept (`store`, when the company is opened; a
/// new driver's when hired); (22 November 2026) of the drivers of before, about half of those
/// with none come to day duties, a third of those who like the late ones to the late and night
/// ones - the same every time, so once.
pub fn seed_all(c: &mut Company) {
    for e in c.staff.iter_mut() {
        let mut rng = Rng::of(&[&e.name, "shift-pref-2"], e.id as i64);
        let x = rng.f64();
        e.shift_pref = Some(match e.shift_pref {
            None => seed(e),
            Some(ShiftPref::Neither) if x < 0.5 => ShiftPref::Day,
            Some(ShiftPref::Late) if x < 0.35 => ShiftPref::LateNight,
            Some(p) => p,
        });
    }
}

/// How a duty of kind `duty` (`kind`) fits what one who likes `pref` likes: 1 liked, -1 not, 0 all
/// the same.
pub fn fit_kind(pref: ShiftPref, duty: ShiftPref) -> i8 {
    use ShiftPref::*;
    match (pref, duty) {
        (Neither, _) | (_, Neither) => 0,
        (Early, Early) | (Day, Day) | (Late, Late) | (LateNight, Late) | (LateNight, LateNight) => 1,
        (Early, Late) | (Early, LateNight) | (Day, LateNight) | (Late, Early) | (Late, LateNight) | (LateNight, Early) => -1,
        _ => 0,
    }
}

/// How a duty signing on at `start` fits what `e` likes: 1 liked, -1 not, 0 all the same.
pub fn fit(e: &Employee, start: i32) -> i8 {
    fit_kind(of(e), kind(start))
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
/// each driver's day by its first sign-on - liked, a little more content; not, a little less (and
/// counted towards asking for another rota, `Employee::disliked`).
pub fn after_day(c: &mut Company, pieces: &[(u32, i32, i32)]) {
    if !pro_duties::on(c) {
        return;
    }
    for e in c.staff.iter_mut() {
        let Some(first) = pieces.iter().filter(|x| x.0 == e.id && x.2 > x.1).map(|x| x.1).min() else { continue };
        match fit(e, first) {
            1 => {
                e.satisfaction = (e.satisfaction + GLAD).min(100.0);
                e.disliked = e.disliked.saturating_sub(1);
            }
            -1 => {
                e.satisfaction = (e.satisfaction - SORRY).max(0.0);
                e.disliked = e.disliked.saturating_add(1);
            }
            _ => {}
        }
    }
}

/// (22 November 2026) The time of a rota's duties (`duty_start(day, n)`: when duty `n` of the
/// week's day `day` signs on): the kind three in four of its duties are, else None (a mixed one).
pub fn slot_of_rota(t: &super::pro_turnus::Turnus, duty_start: &dyn Fn(usize, u32) -> Option<i32>) -> Option<ShiftPref> {
    if t.reserve || t.mixed {
        return None;
    }
    let mut n = std::collections::HashMap::new();
    let mut all = 0usize;
    for row in &t.rows {
        for (d, cell) in row.days.iter().enumerate() {
            if let super::pro_turnus::Cell::Duty { n: num } = cell {
                if let Some(start) = duty_start(d, *num) {
                    *n.entry(kind(start)).or_insert(0usize) += 1;
                    all += 1;
                }
            }
        }
    }
    n.into_iter().find(|(_, k)| all > 0 && *k * 4 >= all * 3).map(|x| x.0)
}

/// What it is called on the staff card and in the popup (English, translated when shown).
pub fn label(p: ShiftPref) -> Option<&'static str> {
    match p {
        ShiftPref::Early => Some(PREFERS_EARLY),
        ShiftPref::Day => Some(PREFERS_DAY),
        ShiftPref::Late => Some(PREFERS_LATE),
        ShiftPref::LateNight => Some(PREFERS_NIGHT),
        ShiftPref::Neither => None,
    }
}

/// (22 November 2026) A rota's time in short (English, translated when shown).
pub fn rota_label(p: ShiftPref) -> &'static str {
    match p {
        ShiftPref::Early => ROTA_EARLY,
        ShiftPref::Day => ROTA_DAY,
        ShiftPref::Late => ROTA_LATE,
        ShiftPref::LateNight => ROTA_NIGHT,
        ShiftPref::Neither => ROTA_MIXED,
    }
}

pub const PREFERS_EARLY: &str = "🌅 Prefers early duties";
pub const PREFERS_DAY: &str = "☀ Prefers day duties";
pub const PREFERS_LATE: &str = "🌙 Prefers late duties";
pub const PREFERS_NIGHT: &str = "🌃 Prefers late and night duties";
pub const NO_PREFERENCE: &str = "No preference";
pub const SHIFTS: &str = "Preference";
pub const ROTA_EARLY: &str = "Morning rota";
pub const ROTA_DAY: &str = "Day rota";
pub const ROTA_LATE: &str = "Evening rota";
pub const ROTA_NIGHT: &str = "Night rota";
pub const ROTA_MIXED: &str = "Mixed rota";

/// Every English text this module hands the pages.
pub fn texts() -> Vec<&'static str> {
    vec![PREFERS_EARLY, PREFERS_DAY, PREFERS_LATE, PREFERS_NIGHT, NO_PREFERENCE, SHIFTS, ROTA_EARLY, ROTA_DAY, ROTA_LATE, ROTA_NIGHT, ROTA_MIXED]
}
