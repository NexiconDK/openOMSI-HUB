//! (NexiconDK, round R, 9 November 2026: "Ønsker til vagter") The wishes of a player of Play
//! together (Bus Company Online's server) for the duties he is given: the kinds he would rather
//! drive and those he would rather not (early, day, evening, night, split), the weekdays and the
//! dates he would like off, and the most hours a week he would like to drive.
//!
//! - **Soft**: a wish is never a rule - a duty against it may still be given, taken or swapped; it
//!   is told ("ønske ikke opfyldt: undgår nat") where the duties are given (the Duties tab's
//!   "Give duties", RosterFlow Pro's "who can drive" box, which puts those it suits first) and to
//!   the player himself beside the duties he sees.
//! - **The days off** (`dates`) are said in RosterFlow Pro's "Kan arbejde" as well (`pro_avail`:
//!   not that day, by the driver), where the planner sees them with the hourly paid's.
//! - **Met**: `met` - of the duties of the players with wishes, the company's day's and those
//!   driven in the days before, how many keep them (RosterFlow Pro's "Ønsker" shows it).
//!
//! Kept as `Together::wishes`, one a player; dropped with the player (`tidy`).

use super::model::Company;
use super::plan::{DayPlan, Who};
use super::{dates, pro_avail};
use serde::{Deserialize, Serialize};

/// A kind of duty a wish names.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Signing on before 07:00.
    Early,
    /// Signing on from 07:00 to noon.
    Day,
    /// Signing on from noon.
    Evening,
    /// With night-bus work (a night line, or a trip arriving after midnight).
    Night,
    /// In pieces over the day (a split duty, or two duties of a day).
    Split,
}

impl Kind {
    pub const ALL: [Kind; 5] = [Kind::Early, Kind::Day, Kind::Evening, Kind::Night, Kind::Split];

    pub fn key(self) -> &'static str {
        match self {
            Kind::Early => "early",
            Kind::Day => "day",
            Kind::Evening => "evening",
            Kind::Night => "night",
            Kind::Split => "split",
        }
    }

    pub fn from_key(k: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|x| x.key() == k)
    }

    /// Its name (English: a key of the translations).
    pub fn label(self) -> &'static str {
        match self {
            Kind::Early => "early duties",
            Kind::Day => "day duties",
            Kind::Evening => "evening duties",
            Kind::Night => "night duties",
            Kind::Split => "split duties",
        }
    }
}

/// A player's wishes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct PlayerWish {
    pub guest: u32,
    pub prefer: Vec<Kind>,
    pub avoid: Vec<Kind>,
    /// The weekdays he would like off (0 Monday .. 6 Sunday).
    pub off: Vec<u8>,
    /// The dates he would like off.
    pub dates: Vec<String>,
    /// The most minutes a week he would like to drive (None: as many as come).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_week: Option<i32>,
    /// The company's day he said it.
    pub set: String,
}

impl PlayerWish {
    pub fn is_empty(&self) -> bool {
        self.prefer.is_empty() && self.avoid.is_empty() && self.off.is_empty() && self.dates.is_empty() && self.max_week.is_none()
    }
}

/// A day of work of a player, kept after its day closed (`together::book::after_close`): for the
/// rest before his next duty, his days in a row and his hours of the week.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct WorkDay {
    pub guest: u32,
    pub date: String,
    /// Signing on and off (minutes of the date; past 1440 into the next day).
    pub from: i32,
    pub to: i32,
    /// The minutes worked (pieces apart, their gaps not).
    pub minutes: i32,
}

/// How long a player's days of work are kept.
pub const KEEP_DAYS: i64 = 14;
/// The most dates off and weekdays off a player may wish at once, and how far ahead.
pub const DATES_MOST: usize = 10;
pub const DATES_AHEAD: i64 = 60;
pub const OFF_MOST: usize = 5;
/// The most hours a week a player may name: from 4 to 60.
pub const WEEK_LEAST: i32 = 4 * 60;
pub const WEEK_MOST: i32 = 60 * 60;
/// Signing on before this is early, before `DAY_TO` a day duty, after it an evening one.
pub const EARLY_TO: i32 = 7 * 60;
pub const DAY_TO: i32 = 12 * 60;

pub const BOTH: &str = "A kind cannot be both wished and avoided.";
pub const BAD_DAY: &str = "That date cannot be wished off.";
pub const TOO_MANY: &str = "Too many days off at once.";
pub const BAD_HOURS: &str = "The most hours a week must be from 4 to 60.";
pub const NO_PLAYER: &str = "There is no such player.";

/// Why a duty does not suit a player's wishes (English: keys of the translations).
pub const MISS_AVOIDS: &str = "avoids %{kind}";
pub const MISS_WISHES: &str = "wishes %{kinds}";
pub const MISS_DAY_OFF: &str = "wished %{day} off";
pub const MISS_HOURS: &str = "over %{max} a week (%{have})";

/// A wish a duty does not keep.
#[derive(Clone, Debug, PartialEq)]
pub enum Miss {
    /// He avoids a kind it is.
    Avoids(Kind),
    /// He wishes kinds it is none of.
    NotWished(Vec<Kind>),
    /// He wished its weekday or its date off (the weekday: 0 Monday .. 6 Sunday, None: the date).
    DayOff(Option<u8>),
    /// It takes his week past his most (`have` with it, `max`: minutes).
    Hours { have: i32, max: i32 },
}

impl Miss {
    /// Its words: the English template and what fills it (`kind`: a kind's name translated by
    /// `tr`; `day`: a weekday's name by `weekday`).
    pub fn words(&self, tr: &dyn Fn(&str) -> String, weekday: &dyn Fn(u8) -> String) -> (&'static str, Vec<(&'static str, String)>) {
        let hm = |m: i32| format!("{}:{:02}", m / 60, m % 60);
        match self {
            Miss::Avoids(k) => (MISS_AVOIDS, vec![("kind", tr(k.label()))]),
            Miss::NotWished(ks) => (MISS_WISHES, vec![("kinds", ks.iter().map(|k| tr(k.label())).collect::<Vec<_>>().join(", "))]),
            Miss::DayOff(Some(d)) => (MISS_DAY_OFF, vec![("day", weekday(*d))]),
            Miss::DayOff(None) => (MISS_DAY_OFF, vec![("day", tr("this day"))]),
            Miss::Hours { have, max } => (MISS_HOURS, vec![("max", hm(*max)), ("have", hm(*have))]),
        }
    }
}

/// The kinds a duty is: signing on at `from`, off at `to`, `split` (in pieces), `night` (with
/// night-bus work).
pub fn kinds(from: i32, split: bool, night: bool) -> Vec<Kind> {
    let mut out = vec![if from < EARLY_TO {
        Kind::Early
    } else if from < DAY_TO {
        Kind::Day
    } else {
        Kind::Evening
    }];
    if night {
        out.push(Kind::Night);
    }
    if split {
        out.push(Kind::Split);
    }
    out
}

/// Player `guest`'s wishes (None: none said).
pub fn of(c: &Company, guest: u32) -> Option<&PlayerWish> {
    c.together.wishes.iter().find(|w| w.guest == guest && !w.is_empty())
}

/// The minutes player `guest` worked in the week of `date` before it (Monday on, his days kept).
pub fn week_before(c: &Company, guest: u32, date: &str) -> i32 {
    let Some(d) = dates::parse(date) else { return 0 };
    let monday = d - dates::weekday(d) as i64;
    c.together.days.iter().filter(|w| w.guest == guest).filter(|w| dates::parse(&w.date).is_some_and(|x| x >= monday && x < d)).map(|w| w.minutes).sum()
}

/// The wishes of player `guest` a duty of `date` does not keep: signing on at `from`, `split`,
/// `night`, `minutes` long, with `week` minutes of his week besides it.
pub fn misses(c: &Company, guest: u32, date: &str, from: i32, split: bool, night: bool, minutes: i32, week: i32) -> Vec<Miss> {
    let Some(w) = of(c, guest) else { return Vec::new() };
    let ks = kinds(from, split, night);
    let mut out: Vec<Miss> = ks.iter().filter(|k| w.avoid.contains(k)).map(|k| Miss::Avoids(*k)).collect();
    if !w.prefer.is_empty() && !ks.iter().any(|k| w.prefer.contains(k)) {
        out.push(Miss::NotWished(w.prefer.clone()));
    }
    let wd = dates::parse(date).map(|d| dates::weekday(d) as u8);
    if w.dates.iter().any(|d| d == date) {
        out.push(Miss::DayOff(None));
    } else if let Some(wd) = wd.filter(|d| w.off.contains(d)) {
        out.push(Miss::DayOff(Some(wd)));
    }
    if let Some(max) = w.max_week {
        if week + minutes > max {
            out.push(Miss::Hours { have: week + minutes, max });
        }
    }
    out
}

/// What a duty of the day's plan is for the wishes: signing on, split, night, minutes worked.
pub fn shape(p: &DayPlan, pieces: &[(usize, usize)]) -> (i32, bool, bool, i32) {
    let from = pieces.iter().map(|&(ti, k)| p.tours[ti].duties[k].from).min().unwrap_or(0);
    let minutes = pieces.iter().map(|&(ti, k)| p.tours[ti].duties[k].to - p.tours[ti].duties[k].from).sum();
    let night = pieces.iter().any(|&(ti, k)| {
        let d = &p.tours[ti].duties[k];
        super::rosterflow::night_range(&p.tours[ti].tour, d.start..d.end)
    });
    let split = pieces.len() > 1 || pieces.iter().any(|&(ti, k)| {
        let d = &p.tours[ti].duties[k];
        p.tours[ti].tour.trips.get(d.start..d.end).is_some_and(|x| x.windows(2).any(|w| w[1].dep - w[0].arr >= super::pro_duties::SPLIT_GAP))
    });
    (from, split, night, minutes)
}

/// The wishes of player `guest` duty `(ti, k)` of the day's plan `p` would not keep, beside his
/// other duties of the day (with them: a split) and his week.
pub fn misses_of(c: &Company, p: &DayPlan, guest: u32, ti: usize, k: usize) -> Vec<Miss> {
    if of(c, guest).is_none() {
        return Vec::new();
    }
    let others: Vec<(usize, usize)> = p.duties_of(Who::Guest(guest)).into_iter().filter(|x| *x != (ti, k)).collect();
    let week = week_before(c, guest, &p.date) + others.iter().map(|&(t, j)| p.tours[t].duties[j].to - p.tours[t].duties[j].from).sum::<i32>();
    let mut all = others.clone();
    all.push((ti, k));
    let (from, split, night, _) = shape(p, &all);
    let d = &p.tours[ti].duties[k];
    misses(c, guest, &p.date, from, split, night, d.to - d.from, week)
}

/// Player `guest`'s wishes set (`w`, its `guest` and `set` filled in here): checked; the dates
/// off told RosterFlow Pro's "Kan arbejde" as well (those no longer wished cleared there).
pub fn set(c: &mut Company, guest: u32, mut w: PlayerWish) -> Result<(), &'static str> {
    if !c.together.guests.iter().any(|g| g.no == guest && !g.banned) {
        return Err(NO_PLAYER);
    }
    dedup(&mut w.prefer);
    dedup(&mut w.avoid);
    if w.prefer.iter().any(|k| w.avoid.contains(k)) {
        return Err(BOTH);
    }
    w.off.retain(|d| *d < 7);
    w.off.sort_unstable();
    w.off.dedup();
    w.dates.sort();
    w.dates.dedup();
    if w.off.len() > OFF_MOST || w.dates.len() > DATES_MOST {
        return Err(TOO_MANY);
    }
    let today = c.date.clone();
    if w.dates.iter().any(|d| dates::parse(d).is_none() || !(0..=DATES_AHEAD).contains(&dates::between(&today, d))) {
        return Err(BAD_DAY);
    }
    if let Some(m) = w.max_week {
        if !(WEEK_LEAST..=WEEK_MOST).contains(&m) {
            return Err(BAD_HOURS);
        }
    }
    let before: Vec<String> = of(c, guest).map(|x| x.dates.clone()).unwrap_or_default();
    // (the dates off in "Kan arbejde" - within its days ahead; the planner's own word kept)
    for d in w.dates.iter().filter(|d| !before.contains(d)) {
        if pro_avail::record(c, guest, true, d).is_none_or(|a| a.by == pro_avail::By::Driver) {
            let _ = pro_avail::set(c, guest, true, d, Some(pro_avail::Can::Not), pro_avail::By::Driver);
        }
    }
    for d in before.iter().filter(|d| !w.dates.contains(d)) {
        if pro_avail::record(c, guest, true, d).is_some_and(|a| a.by == pro_avail::By::Driver && a.can == Some(pro_avail::Can::Not)) {
            c.planning.ops.avail.retain(|a| !(a.guest && a.driver == guest && a.date == *d));
        }
    }
    w.guest = guest;
    w.set = today;
    c.together.wishes.retain(|x| x.guest != guest);
    if !w.is_empty() {
        c.together.wishes.push(w);
    }
    Ok(())
}

fn dedup(v: &mut Vec<Kind>) {
    let mut seen = Vec::new();
    v.retain(|k| {
        let new = !seen.contains(k);
        seen.push(*k);
        new
    });
}

/// Of the duties of the players with wishes - the company's day's (`p`) and those they drove in
/// the days kept -, how many keep them all: (kept, all).
pub fn met(c: &Company, p: Option<&DayPlan>) -> (usize, usize) {
    let mut kept = 0;
    let mut all = 0;
    for w in c.together.wishes.iter().filter(|w| !w.is_empty()) {
        if let Some(p) = p {
            for (ti, k) in p.duties_of(Who::Guest(w.guest)) {
                all += 1;
                if misses_of(c, p, w.guest, ti, k).is_empty() {
                    kept += 1;
                }
            }
        }
        for d in c.together.days.iter().filter(|d| d.guest == w.guest && dates::between(&w.set, &d.date) >= 0) {
            all += 1;
            let week = week_before(c, w.guest, &d.date);
            if misses(c, w.guest, &d.date, d.from, d.minutes + 30 < d.to - d.from, d.to > 1440, d.minutes, week).is_empty() {
                kept += 1;
            }
        }
    }
    (kept, all)
}

/// The days of work long gone dropped (`KEEP_DAYS`), the wishes of players gone and their dates
/// past.
pub fn tidy(c: &mut Company) {
    let today = c.date.clone();
    let players: Vec<u32> = c.together.guests.iter().filter(|g| !g.banned).map(|g| g.no).collect();
    c.together.days.retain(|d| dates::between(&d.date, &today) <= KEEP_DAYS);
    c.together.wishes.retain(|w| players.contains(&w.guest));
    for w in c.together.wishes.iter_mut() {
        w.dates.retain(|d| d.as_str() >= today.as_str());
    }
    c.together.wishes.retain(|w| !w.is_empty());
}

pub fn texts() -> Vec<&'static str> {
    let mut v = vec![BOTH, BAD_DAY, TOO_MANY, BAD_HOURS, NO_PLAYER, MISS_AVOIDS, MISS_WISHES, MISS_DAY_OFF, MISS_HOURS, "this day"];
    v.extend(Kind::ALL.iter().map(|k| k.label()));
    v
}
