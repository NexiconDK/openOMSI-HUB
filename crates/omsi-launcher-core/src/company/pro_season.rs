//! RosterFlow Pro, round E (the player's, NexiconDK 5 November 2026: "Årstider"): the time of year
//! makes the planning harder in two seasons.
//!
//! - **Winter** (1 December to the end of February): more drivers call in sick on the day
//!   (`pro_sick::draws`, `WINTER_SICK`: 40 % more).
//! - **Spring** (1 March to 19 June): the summer holiday is planned - more holiday requests
//!   (`holidays::ask`, `SPRING_ASKS`).
//! - **Summer holidays** (20 June to 15 August, July most of all): more requests still
//!   (`SUMMER_ASKS`), and a summer holiday refused costs more content (`SUMMER_REFUSAL` on
//!   `holidays::refusal_cost`).
//! - The calendar's head says which (`hint`), its tooltip the effect.
//!
//! RosterFlow Pro only (`pro_duties::on`); the other planners keep the year as it was.

use super::holidays::{Reason, Request};
use super::model::Company;

/// The seasons that change the planning.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Season {
    Winter,
    Spring,
    Summer,
    /// The rest of the year: nothing changes.
    Other,
}

impl Season {
    pub fn key(self) -> &'static str {
        match self {
            Season::Winter => "winter",
            Season::Spring => "spring",
            Season::Summer => "summer",
            Season::Other => "",
        }
    }
}

/// Of the day sick calls, the more in winter.
pub const WINTER_SICK: f64 = 1.4;
/// Of the holiday requests, the more in spring and in the summer holidays.
pub const SPRING_ASKS: f64 = 1.5;
pub const SUMMER_ASKS: f64 = 1.8;
/// What refusing a summer holiday costs of content, times the usual.
pub const SUMMER_REFUSAL: f64 = 1.5;

/// The season of `date` (yyyy-mm-dd).
pub fn of(date: &str) -> Season {
    let m = date.get(5..7).and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
    let d = date.get(8..10).and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
    match (m, d) {
        (12 | 1 | 2, _) => Season::Winter,
        (6, 20..) | (7, _) | (8, ..=15) => Season::Summer,
        (3..=5, _) | (6, _) => Season::Spring,
        _ => Season::Other,
    }
}

/// The season of `date` for the company: Other without RosterFlow Pro.
pub fn of_company(c: &Company, date: &str) -> Season {
    if super::pro_duties::on(c) {
        of(date)
    } else {
        Season::Other
    }
}

/// What the season does to the chance of a sick call on the day.
pub fn sick(c: &Company, date: &str) -> f64 {
    if of_company(c, date) == Season::Winter {
        WINTER_SICK
    } else {
        1.0
    }
}

/// What the season does to the chance of a holiday request.
pub fn asks(c: &Company, date: &str) -> f64 {
    match of_company(c, date) {
        Season::Spring => SPRING_ASKS,
        Season::Summer => SUMMER_ASKS,
        _ => 1.0,
    }
}

/// Whether a request is for the summer holiday: its reason, or one of its days in the summer
/// holidays.
pub fn summer(r: &Request) -> bool {
    r.reason == Reason::Summer || super::holidays::days_of(&r.from, &r.until).iter().any(|d| of(d) == Season::Summer)
}

/// What refusing request `r` costs of content, times the usual (RosterFlow Pro: a summer holiday
/// more).
pub fn refusal(c: &Company, r: &Request) -> f64 {
    if super::pro_duties::on(c) && summer(r) {
        SUMMER_REFUSAL
    } else {
        1.0
    }
}

/// The calendar's words for the season of `date` (None: nothing to say): its key, its words and
/// the tooltip (English: keys of the translations).
pub fn hint(c: &Company, date: &str) -> Option<(&'static str, &'static str, &'static str)> {
    let s = of_company(c, date);
    let (text, tip) = match s {
        Season::Winter => (WINTER, WINTER_TIP),
        Season::Spring => (SPRING, SPRING_TIP),
        Season::Summer => (SUMMER, SUMMER_TIP),
        Season::Other => return None,
    };
    Some((s.key(), text, tip))
}

pub const WINTER: &str = "❄ Winter – more call in sick";
pub const WINTER_TIP: &str = "December to February: about 40 % more drivers call in sick on the day.";
pub const SPRING: &str = "🌷 The summer holiday is planned – more holiday requests";
pub const SPRING_TIP: &str = "March to mid-June: the drivers ask for their summer holiday – 50 % more holiday requests.";
pub const SUMMER: &str = "☀ Summer holidays – many want time off";
pub const SUMMER_TIP: &str = "20 June to 15 August: 80 % more holiday requests, and refusing a summer holiday costs 50 % more contentment.";

/// Every English text of this module.
pub fn texts() -> Vec<&'static str> {
    vec![WINTER, WINTER_TIP, SPRING, SPRING_TIP, SUMMER, SUMMER_TIP]
}
