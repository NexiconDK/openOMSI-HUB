//! RosterFlow Pro, round B (the player's, NexiconDK 5 November 2026: "Ønsker fra chaufførerne"):
//! the drivers send wishes of their own, the planner grants or refuses them.
//!
//! - **Sent** (`daily`, once a company day from 09:00 - `clock::on_minute`): each driver now and
//!   then, the same for the same company, day and driver (`Rng`) - more often when less content,
//!   less when friendlier; a handful a week in a company of ten. A day off on a date one to two
//!   weeks on ("Ønsker fri lørdag 9 Mar"), early duties, no split duty, more hours (the hourly paid
//!   and part-timers mostly). Each comes as his message to the control room, is listed in the
//!   calendar's "Ønsker" and marked ★ on his day in the calendar and the rota.
//! - **Granted** (`grant`): a day off becomes his free day (`model::FreeDay`: the plan keeps him
//!   off, his rota's duty open - the page says so and offers who can drive it); the others hold
//!   for `HOLDS_DAYS`. He is more content (`GRANTED`).
//! - **Refused** (`refuse`) or left open until its day or a week (`lapse`, `LAPSED`): less content,
//!   and three refusals in a row cost more (`IN_A_ROW`).
//! - **Asked anyway** (`bias`, `day_off_wished`): asked for a duty on the day he wished off, he says
//!   no more likely and is a little annoyed (`ASKED_ANYWAY`); one who wished early duties is glad of
//!   an early one and rather not of a late one, one who wished no split duties rather not of a
//!   split one (a duty spanning more than `SPLIT_SPAN`), one who wants hours glad of any.

use super::model::{Company, Employment, FreeDay, FreeWhy};
use super::rng::Rng;
use super::{dates, messages, staff};
use serde::{Deserialize, Serialize};

/// What a driver wishes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WishKind {
    /// The day off on `date`.
    DayOff { date: String },
    /// Early duties.
    Early,
    /// No split duty.
    NoSplit,
    /// More hours.
    MoreHours,
}

/// Where a wish stands.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum WishState {
    #[default]
    Open,
    Granted,
    Refused,
    /// Left open until its day (a day off) or a week (`LAPSE_DAYS`).
    Lapsed,
}

/// A driver's wish.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Wish {
    pub id: u32,
    pub driver: u32,
    pub wish: WishKind,
    /// The company's day he sent it, and the day it was decided (granted, refused, lapsed).
    pub sent: String,
    #[serde(default)]
    pub state: WishState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided: Option<String>,
}

/// A driver's chance a day of sending a wish, before his mood (a handful a week in ten drivers).
pub const CHANCE: f64 = 0.06;
/// The most wishes a driver has open at once.
pub const OPEN_MOST: usize = 2;
/// What a wish granted gives his content, and a refused or lapsed one costs; three in a row
/// refused or lapsed (`IN_A_ROW_FROM`) cost `IN_A_ROW` more each.
pub const GRANTED: f64 = 4.0;
pub const REFUSED: f64 = 3.0;
pub const LAPSED: f64 = 4.0;
pub const IN_A_ROW: f64 = 3.0;
pub const IN_A_ROW_FROM: usize = 3;
/// Asked for a duty on the day he wished off: what it costs his content.
pub const ASKED_ANYWAY: f64 = 1.0;
/// A wish other than a day off left open this many days lapses; granted, it holds this long.
pub const LAPSE_DAYS: i64 = 7;
pub const HOLDS_DAYS: i64 = 28;
/// A duty spanning more than this from sign-on to sign-off is taken for a split one (`bias`).
pub const SPLIT_SPAN: i32 = 11 * 60;

/// The wishes' words, as his message says them (English: keys of the translations; `%{date}` the
/// day).
pub const SAYS_DAY_OFF: &str = "I'd like the day off on %{date}, if possible.";
pub const SAYS_EARLY: &str = "I'd like early duties, if possible.";
pub const SAYS_NO_SPLIT: &str = "I'd rather not have split duties.";
pub const SAYS_MORE: &str = "I'd like more hours.";
/// The wishes in short, as the lists show them.
pub const DAY_OFF: &str = "Wants the day off %{date}";
pub const EARLY: &str = "Would like early duties";
pub const NO_SPLIT: &str = "Wants no split duty";
pub const MORE: &str = "Would like more hours";
/// Refusals of the acts.
pub const NO_SUCH_WISH: &str = "That wish is no longer open.";
pub const WISH_GONE: &str = "That day has gone.";
/// The feed's line when one comes.
pub const SENT_FEED: &str = "%{name} has sent a wish.";

/// The open or granted wish of driver `id` for the day off on `date`, if any.
pub fn day_off_wished<'a>(c: &'a Company, id: u32, date: &str) -> Option<&'a Wish> {
    c.planning.ops.wishes.iter().find(|w| w.driver == id && matches!(w.state, WishState::Open | WishState::Granted) && matches!(&w.wish, WishKind::DayOff { date: d } if d == date))
}

/// His wishes that hold (open, or granted within `HOLDS_DAYS`) other than a day off.
fn holding<'a>(c: &'a Company, id: u32) -> impl Iterator<Item = &'a Wish> + 'a {
    let today = c.date.clone();
    c.planning.ops.wishes.iter().filter(move |w| {
        w.driver == id
            && !matches!(w.wish, WishKind::DayOff { .. })
            && match w.state {
                WishState::Open => true,
                WishState::Granted => w.decided.as_deref().is_some_and(|d| dates::between(d, &today) < HOLDS_DAYS),
                _ => false,
            }
    })
}

/// How his wishes change a driver's yes to a duty of `date` signing on `start`, off `end` (a
/// factor on its chance: `pro_ops::ai_reply`).
pub fn bias(c: &Company, id: u32, date: &str, start: i32, end: i32) -> f64 {
    let mut f = 1.0;
    if day_off_wished(c, id, date).is_some() {
        f *= 0.35;
    }
    for w in holding(c, id) {
        f *= match w.wish {
            WishKind::Early if start <= 9 * 60 => 1.3,
            WishKind::Early if start >= 12 * 60 => 0.7,
            WishKind::NoSplit if end - start > SPLIT_SPAN => 0.5,
            WishKind::MoreHours => 1.4,
            _ => 1.0,
        };
    }
    f
}

/// The notes ★ of driver `id`'s wishes for a duty of `date` signing on `start`, off `end`, as the
/// popup shows them by his name (English: keys of the translations).
pub fn notes(c: &Company, id: u32, date: &str, start: i32, end: i32) -> Vec<&'static str> {
    let mut v = Vec::new();
    if day_off_wished(c, id, date).is_some() {
        v.push(NOTE_DAY_OFF);
    }
    for w in holding(c, id) {
        let note = match w.wish {
            WishKind::Early if start <= 9 * 60 || start >= 12 * 60 => Some(NOTE_EARLY),
            WishKind::NoSplit if end - start > SPLIT_SPAN => Some(NOTE_NO_SPLIT),
            WishKind::MoreHours => Some(NOTE_MORE),
            _ => None,
        };
        if let Some(n) = note.filter(|n| !v.contains(n)) {
            v.push(n);
        }
    }
    v
}

/// The notes of `notes`.
pub const NOTE_DAY_OFF: &str = "★ Wished the day off this day";
pub const NOTE_EARLY: &str = "★ Would like early duties";
pub const NOTE_NO_SPLIT: &str = "★ Wants no split duty";
pub const NOTE_MORE: &str = "★ Would like more hours";

/// A wish's text in short (English template, places), as the lists show it (`day`: the page's
/// words for a date).
pub fn short(w: &Wish, day: &dyn Fn(&str) -> String) -> (&'static str, Vec<(&'static str, String)>) {
    match &w.wish {
        WishKind::DayOff { date } => (DAY_OFF, vec![("date", day(date))]),
        WishKind::Early => (EARLY, Vec::new()),
        WishKind::NoSplit => (NO_SPLIT, Vec::new()),
        WishKind::MoreHours => (MORE, Vec::new()),
    }
}

/// "9/3": a date as a message says it (the same in every language).
fn numeric(date: &str) -> String {
    let d = date.get(8..10).and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
    let m = date.get(5..7).and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
    format!("{d}/{m}")
}

/// The wishes a driver of the company might send today: his chance (less content: more often;
/// friendlier: less).
pub fn chance(e: &super::model::Employee) -> f64 {
    (CHANCE * (1.3 - e.friendliness / 200.0) * (1.0 + (55.0 - e.satisfaction).max(0.0) / 80.0)).clamp(0.0, 0.3)
}

/// The company's day for wishes (`clock::on_minute`, once a day from 09:00): the wishes whose day
/// came or whose week went lapse; the drivers send new ones (each his message to the control
/// room). The wishes sent today.
pub fn daily(c: &mut Company) -> Vec<u32> {
    if !super::pro_duties::on(c) || c.planning.ops.wished == c.date {
        return Vec::new();
    }
    let today = c.date.clone();
    c.planning.ops.wished = today.clone();
    lapse(c);
    let mut sent = Vec::new();
    let drivers: Vec<u32> = c.staff.iter().filter(|e| staff::drives(e) && e.employed_on(&today) && e.notice_until.is_none() && e.agency_no == 0).map(|e| e.id).collect();
    for id in drivers {
        let Some(e) = c.employee(id).cloned() else { continue };
        let open: Vec<&Wish> = c.planning.ops.wishes.iter().filter(|w| w.driver == id && w.state == WishState::Open).collect();
        if open.len() >= OPEN_MOST {
            continue;
        }
        let mut rng = Rng::of(&[&c.id, &today, "pro-wish"], id as i64);
        if !rng.chance(chance(&e)) {
            continue;
        }
        let roll = rng.int(0, 99);
        let kind = match e.employment {
            Employment::Hourly if roll < 70 => WishKind::MoreHours,
            Employment::PartTime { .. } if roll < 30 => WishKind::MoreHours,
            Employment::FullTime | Employment::PartTime { .. } if roll >= 85 => WishKind::NoSplit,
            Employment::FullTime | Employment::PartTime { .. } if roll >= 65 => WishKind::Early,
            _ => {
                let date = dates::add(&today, rng.int(4, 14));
                if e.absent(&date) {
                    continue;
                }
                WishKind::DayOff { date }
            }
        };
        if open.iter().any(|w| std::mem::discriminant(&w.wish) == std::mem::discriminant(&kind)) || holding(c, id).any(|w| w.wish == kind) {
            continue;
        }
        c.planning.ops.last_wish += 1;
        let wid = c.planning.ops.last_wish;
        let (text, args) = match &kind {
            WishKind::DayOff { date } => (SAYS_DAY_OFF, vec![("date".to_string(), numeric(date))]),
            WishKind::Early => (SAYS_EARLY, Vec::new()),
            WishKind::NoSplit => (SAYS_NO_SPLIT, Vec::new()),
            WishKind::MoreHours => (SAYS_MORE, Vec::new()),
        };
        c.planning.ops.wishes.push(Wish { id: wid, driver: id, wish: kind, sent: today.clone(), state: WishState::Open, decided: None });
        let from = messages::From::Driver { line: String::new(), tour: String::new(), name: e.name.clone() };
        messages::send(c, from, messages::To::Control, messages::Priority::Info, text, args, false, None);
        sent.push(wid);
    }
    sent
}

/// The open wishes whose day came (a day off) or whose week went (the others) lapse: he is less
/// content.
pub fn lapse(c: &mut Company) {
    let today = c.date.clone();
    let gone: Vec<u32> = c
        .planning
        .ops
        .wishes
        .iter()
        .filter(|w| w.state == WishState::Open)
        .filter(|w| match &w.wish {
            WishKind::DayOff { date } => dates::between(&today, date) <= 0,
            _ => dates::between(&w.sent, &today) >= LAPSE_DAYS,
        })
        .map(|w| w.id)
        .collect();
    for id in gone {
        decide(c, id, WishState::Lapsed);
    }
}

/// Wish `id` decided (`state`): its driver's content by it, three refusals in a row more.
fn decide(c: &mut Company, id: u32, state: WishState) -> Option<u32> {
    let today = c.date.clone();
    let w = c.planning.ops.wishes.iter_mut().find(|w| w.id == id && w.state == WishState::Open)?;
    w.state = state;
    w.decided = Some(today);
    let driver = w.driver;
    let mut cost = match state {
        WishState::Granted => -GRANTED,
        WishState::Refused => REFUSED,
        WishState::Lapsed => LAPSED,
        WishState::Open => 0.0,
    };
    if matches!(state, WishState::Refused | WishState::Lapsed) && refused_in_a_row(c, driver) >= IN_A_ROW_FROM {
        cost += IN_A_ROW;
    }
    if let Some(e) = c.staff.iter_mut().find(|e| e.id == driver) {
        e.satisfaction = (e.satisfaction - cost).clamp(0.0, 100.0);
    }
    Some(driver)
}

/// How many of driver `id`'s last decided wishes in a row were refused or lapsed.
pub fn refused_in_a_row(c: &Company, id: u32) -> usize {
    let mut v: Vec<&Wish> = c.planning.ops.wishes.iter().filter(|w| w.driver == id && w.state != WishState::Open).collect();
    v.sort_by(|a, b| (a.decided.as_deref().unwrap_or(""), a.id).cmp(&(b.decided.as_deref().unwrap_or(""), b.id)));
    v.iter().rev().take_while(|w| matches!(w.state, WishState::Refused | WishState::Lapsed)).count()
}

/// Wish `id` granted: a day off becomes his free day (the date, for the page to say a duty is
/// open then); he is more content.
pub fn grant(c: &mut Company, id: u32) -> Result<Option<String>, &'static str> {
    let w = c.planning.ops.wishes.iter().find(|w| w.id == id && w.state == WishState::Open).cloned().ok_or(NO_SUCH_WISH)?;
    let date = match &w.wish {
        WishKind::DayOff { date } => {
            if dates::between(&c.date, date) < 0 {
                return Err(WISH_GONE);
            }
            Some(date.clone())
        }
        _ => None,
    };
    decide(c, id, WishState::Granted).ok_or(NO_SUCH_WISH)?;
    if let Some(date) = &date {
        if let Some(e) = c.staff.iter_mut().find(|e| e.id == w.driver) {
            if e.free_on(date).is_none() {
                e.free_days.push(FreeDay { date: date.clone(), why: FreeWhy::Wish, minutes: 0.0 });
                e.free_days.sort_by(|a, b| a.date.cmp(&b.date));
            }
        }
    }
    Ok(date)
}

/// Wish `id` refused: he is less content (three in a row: more).
pub fn refuse(c: &mut Company, id: u32) -> Result<(), &'static str> {
    decide(c, id, WishState::Refused).map(|_| ()).ok_or(NO_SUCH_WISH)
}

/// The open wishes, the earliest sent first.
pub fn open(c: &Company) -> Vec<&Wish> {
    let mut v: Vec<&Wish> = c.planning.ops.wishes.iter().filter(|w| w.state == WishState::Open).collect();
    v.sort_by(|a, b| (a.sent.as_str(), a.id).cmp(&(b.sent.as_str(), b.id)));
    v
}

/// The wishes of driver `id` to mark on `date` (★): a day off of that date, open or granted.
pub fn marked<'a>(c: &'a Company, id: u32, date: &str) -> Option<&'a Wish> {
    day_off_wished(c, id, date)
}

/// The wishes decided long ago dropped (a month and a half kept for the count in a row), and the
/// drivers gone.
pub fn tidy(c: &mut Company) {
    let today = c.date.clone();
    let drivers: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    c.planning.ops.wishes.retain(|w| drivers.contains(&w.driver) && w.decided.as_deref().is_none_or(|d| dates::between(d, &today) < 45));
}

/// Every English text of this module.
pub fn texts() -> Vec<&'static str> {
    vec![SAYS_DAY_OFF, SAYS_EARLY, SAYS_NO_SPLIT, SAYS_MORE, DAY_OFF, EARLY, NO_SPLIT, MORE, NO_SUCH_WISH, WISH_GONE, SENT_FEED, NOTE_DAY_OFF, NOTE_EARLY, NOTE_NO_SPLIT, NOTE_MORE]
}
