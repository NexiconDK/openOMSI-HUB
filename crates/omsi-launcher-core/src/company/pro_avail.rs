//! RosterFlow Pro, round H (the player's, NexiconDK 5 November 2026: "en fane hvor de timelønnede
//! kan ønske arbejde"): the hourly paid say when they can work, a day at a time, and the planner
//! sees it in the tab "Kan arbejde" - and the drivers remember it when they are asked.
//!
//! - **What a day says** (`Can`): all day (any duty of the day), a window ("05:00–15:00", within
//!   the day: from 00:00 to 24:00, an hour at least), or not that day. A day nobody said anything
//!   of is "–".
//! - **Who says it** (`By`): an AI hourly-paid driver fills in the next `AHEAD` days himself every
//!   company night (`daily`, from 00:00 - `clock::on_minute`), each day drawn for the company, the
//!   driver and the date (`drawn`: the weights `DRAWN` - all day 35, the morning 05–14 20, the
//!   afternoon 11–20 20, the evening 14–24 15, not that day 10 of 100; one who wished for more
//!   hours, round B, `MORE`: all day 50 and not that day 4). A player of Play together who drives
//!   says his own in the tab; the planner may set, change or clear any driver's day (`set`). A day
//!   the planner cleared stays "–" (the driver does not fill it in again).
//! - **Remembered when asked** (`pro_ops::ai_reply`, `answer_time`): a duty within what he said -
//!   all of it - he says yes far more often (`INSIDE_YES`, at most `INSIDE_MOST`) and answers
//!   sooner (`INSIDE_WAIT`); outside it, too early or too late ("I can from 11:00"); a day he
//!   cannot, a gentle no. A day nobody filled in (past the days ahead): his day drawn the same way.
//! - **Kept** a week after its day (`KEEP_DAYS`), then dropped (`tidy`).
//!
//! Before round H (round F) the hourly paid had their day drawn the same way, kept nowhere
//! (`pro_ops::own_hours`, gone): an older company's days are filled in the first night with the same
//! draw, so its drivers keep saying what they said.

use super::model::{Company, Employee, Employment};
use super::rng::Rng;
use super::{clock, dates};
use serde::{Deserialize, Serialize};

/// What a driver can on a day.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Can {
    /// Any duty of the day.
    AllDay,
    /// From `from` to `to` (minutes of the day).
    Window { from: i32, to: i32 },
    /// Not that day.
    Not,
}

impl Can {
    /// The span of the day it holds (None: not that day).
    pub fn span(self) -> Option<(i32, i32)> {
        match self {
            Can::AllDay => Some((0, 30 * 60)),
            Can::Window { from, to } => Some((from, to)),
            Can::Not => None,
        }
    }

    /// Whether a duty from `start` to `end` lies within it, all of it.
    pub fn holds(self, start: i32, end: i32) -> bool {
        self.span().is_some_and(|(f, t)| start >= f && end <= t)
    }

    pub fn key(self) -> &'static str {
        match self {
            Can::AllDay => "all",
            Can::Window { .. } => "window",
            Can::Not => "not",
        }
    }
}

/// Who said it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum By {
    /// The driver (an AI driver's own, or a player's in the tab).
    #[default]
    Driver,
    /// The planner.
    Planner,
}

/// A driver's day: `driver` a driver of the staff, or with `guest` a player of Play together (his
/// number); `can` None - cleared by the planner, "–".
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Avail {
    pub driver: u32,
    #[serde(default, skip_serializing_if = "is_false")]
    pub guest: bool,
    pub date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub can: Option<Can>,
    #[serde(default)]
    pub by: By,
    /// When it was said, on the company's clock (minutes).
    #[serde(default)]
    pub at: i64,
}

fn is_false(x: &bool) -> bool {
    !*x
}

/// How many days ahead an AI driver fills in (today the first).
pub const AHEAD: i64 = 14;
/// How long a day is kept after it.
pub const KEEP_DAYS: i64 = 7;
/// How far ahead a day may be set in the tab.
pub const SET_AHEAD: i64 = 60;
/// A window an hour at least.
pub const LEAST: i32 = 60;
/// Within what he said: his yes this much more likely, at most `INSIDE_MOST`; he answers in this
/// share of the time.
pub const INSIDE_YES: f64 = 1.6;
pub const INSIDE_MOST: f64 = 0.97;
pub const INSIDE_WAIT: f64 = 0.5;

/// The days an AI driver draws, and their weights (of 100).
pub const DRAWN: [(Can, u32); 5] = [
    (Can::AllDay, 35),
    (Can::Window { from: 5 * 60, to: 14 * 60 }, 20),
    (Can::Window { from: 11 * 60, to: 20 * 60 }, 20),
    (Can::Window { from: 14 * 60, to: 24 * 60 }, 15),
    (Can::Not, 10),
];
/// The same for one who wished for more hours (round B).
pub const MORE: [(Can, u32); 5] = [
    (Can::AllDay, 50),
    (Can::Window { from: 5 * 60, to: 14 * 60 }, 16),
    (Can::Window { from: 11 * 60, to: 20 * 60 }, 16),
    (Can::Window { from: 14 * 60, to: 24 * 60 }, 14),
    (Can::Not, 4),
];

/// Why a day is refused (English: keys of the translations).
pub const NOT_HOURLY: &str = "Only the hourly paid say when they can work.";
pub const BAD_TIMES: &str = "Give the times as HH:MM, the end after the start and an hour at least.";
pub const BAD_DAY: &str = "That day cannot be set: from today and the next 60 days.";
pub const ONLY_OWN: &str = "You can only say your own days.";

/// Whether driver `e` says when he can (the hourly paid).
pub fn says(e: &Employee) -> bool {
    e.employment == Employment::Hourly
}

/// Driver `e`'s day `date` as he would fill it in (`DRAWN`, `MORE`): the same for the same
/// company, driver and date.
pub fn drawn(c: &Company, e: &Employee, date: &str) -> Can {
    let table = if super::pro_wish::wants_more(c, e.id) { &MORE } else { &DRAWN };
    let mut rng = Rng::of(&[&c.id, date, "pro-own-hours"], e.id as i64);
    let total: u32 = table.iter().map(|x| x.1).sum();
    let mut k = rng.int(0, total as i64 - 1) as u32;
    for (h, w) in table.iter() {
        if k < *w {
            return *h;
        }
        k -= w;
    }
    table[0].0
}

/// The record of a driver's day (`guest`: a player's), if any.
pub fn record<'a>(c: &'a Company, driver: u32, guest: bool, date: &str) -> Option<&'a Avail> {
    c.planning.ops.avail.iter().find(|a| a.driver == driver && a.guest == guest && a.date == date)
}

/// What driver `e` can on `date` (None: not an hourly-paid driver, or the planner cleared it): his
/// record, else his day as he would fill it in.
pub fn can_of(c: &Company, e: &Employee, date: &str) -> Option<Can> {
    if !says(e) {
        return None;
    }
    match record(c, e.id, false, date) {
        Some(a) => a.can,
        None => Some(drawn(c, e, date)),
    }
}

/// What a player of Play together said of `date` (None: nothing).
pub fn guest_can(c: &Company, no: u32, date: &str) -> Option<Can> {
    record(c, no, true, date).and_then(|a| a.can)
}

/// Whether a duty from `start` to `end` of `date` lies within what `e` said he can.
pub fn inside(c: &Company, e: &Employee, date: &str, start: i32, end: i32) -> bool {
    can_of(c, e, date).is_some_and(|x| x.holds(start, end))
}

/// The company's night for the hourly paid (`clock::on_minute`, once a company day): each AI
/// hourly-paid driver fills in the days ahead he has not said yet; the days long gone dropped.
/// How many days were filled in.
pub fn daily(c: &mut Company) -> usize {
    if !super::pro_duties::on(c) || c.planning.ops.avail_day == c.date {
        return 0;
    }
    let today = c.date.clone();
    c.planning.ops.avail_day = today.clone();
    tidy(c);
    let at = clock::now(c);
    let mut new = Vec::new();
    for e in c.staff.iter().filter(|e| says(e)) {
        for k in 0..AHEAD {
            let date = dates::add(&today, k);
            if record(c, e.id, false, &date).is_none() {
                new.push(Avail { driver: e.id, guest: false, date: date.clone(), can: Some(drawn(c, e, &date)), by: By::Driver, at });
            }
        }
    }
    let n = new.len();
    c.planning.ops.avail.extend(new);
    n
}

/// The days long gone dropped (`KEEP_DAYS`), and those of drivers gone or no longer hourly paid
/// and of players gone.
pub fn tidy(c: &mut Company) {
    let today = c.date.clone();
    let hourly: Vec<u32> = c.staff.iter().filter(|e| says(e)).map(|e| e.id).collect();
    let players: Vec<u32> = c.together.guests.iter().filter(|g| !g.banned).map(|g| g.no).collect();
    c.planning.ops.avail.retain(|a| dates::between(&a.date, &today) <= KEEP_DAYS && if a.guest { players.contains(&a.driver) } else { hourly.contains(&a.driver) });
}

/// "05:00" read as minutes of the day (24:00 the day's end).
pub fn minutes(s: &str) -> Option<i32> {
    let (h, m) = s.trim().split_once(':')?;
    let (h, m): (i32, i32) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    ((0..=24).contains(&h) && (0..60).contains(&m) && h * 60 + m <= 24 * 60).then_some(h * 60 + m)
}

/// A window checked: within the day, the end after the start, an hour at least.
pub fn window(from: i32, to: i32) -> Result<Can, &'static str> {
    if from < 0 || to > 24 * 60 || to - from < LEAST {
        return Err(BAD_TIMES);
    }
    Ok(Can::Window { from, to })
}

/// Driver `driver`'s (`guest`: a player's) day `date` set to `can` (None: cleared, "–") by `by`.
pub fn set(c: &mut Company, driver: u32, guest: bool, date: &str, can: Option<Can>, by: By) -> Result<(), &'static str> {
    if guest {
        if !c.together.guests.iter().any(|g| g.no == driver && !g.banned) {
            return Err(NOT_HOURLY);
        }
    } else if !c.employee(driver).is_some_and(says) {
        return Err(NOT_HOURLY);
    }
    let ahead = dates::between(&c.date, date);
    if dates::parse(date).is_none() || !(0..=SET_AHEAD).contains(&ahead) {
        return Err(BAD_DAY);
    }
    if let Some(Can::Window { from, to }) = can {
        window(from, to)?;
    }
    let at = clock::now(c);
    c.planning.ops.avail.retain(|a| !(a.driver == driver && a.guest == guest && a.date == date));
    c.planning.ops.avail.push(Avail { driver, guest, date: date.to_string(), can, by, at });
    Ok(())
}

/// "05–15", or "05:30–15:00" when either has minutes.
pub fn times(from: i32, to: i32) -> String {
    let hm = |m: i32| format!("{:02}:{:02}", m / 60, m % 60);
    if from % 60 == 0 && to % 60 == 0 {
        format!("{:02}–{:02}", from / 60, to / 60)
    } else {
        format!("{}–{}", hm(from), hm(to))
    }
}

/// The words of a day.
pub const ALL_DAY: &str = "All day";
pub const CANNOT: &str = "Cannot";
/// The popup's words by an hourly-paid driver's name for a duty: within, outside, not that day.
pub const CAN_IN: &str = "✓ Can %{t}";
pub const CAN_ALL: &str = "✓ Can all day";
pub const CAN_ONLY: &str = "Can only %{t}";
pub const CAN_NOT: &str = "Cannot that day";
/// The calendar's mark on an empty day, its words on hover.
pub const MARK: &str = "Can work %{t}";
pub const MARK_ALL: &str = "Can work all day";
/// Who said it, when.
pub const BY_DRIVER: &str = "Said by the driver at %{time}";
pub const BY_PLAYER: &str = "Said by the player at %{time}";
pub const BY_PLANNER: &str = "Set by the planner";

/// Every English text of this module.
pub fn texts() -> Vec<&'static str> {
    vec![NOT_HOURLY, BAD_TIMES, BAD_DAY, ONLY_OWN, ALL_DAY, CANNOT, CAN_IN, CAN_ALL, CAN_ONLY, CAN_NOT, MARK, MARK_ALL, BY_DRIVER, BY_PLAYER, BY_PLANNER]
}

