//! (NexiconDK, Bus Company Online, decided 3 November 2026) Driving yourself pays, the rest (the
//! plan's K6, K7 and K10-K13; K1-K5 are `live_pay`'s):
//!
//! * **K6, urgent tasks for players only**: a doubling the server's admin ordered (short notice,
//!   binding) and rail replacement are *urgent*: listed apart to the company's players, paid
//!   `urgent_bonus` (50 %) on top. An admin's doubling is the players' alone - no AI driver, no
//!   agency driver takes it (`players_only`): taken by none, it is not run and its full departures
//!   count as complaints (the doubling's rules). Rail replacement is planned as ever (the AI runs
//!   what no player takes) - a player taking it over earns the bonus.
//! * **K7, the ranking**: each trip a player drove live with a passed judgement counts its
//!   timetabled minutes and its score for him in its month (`LiveMonth`, kept with the server's
//!   player); the driver of the month has the most minutes (ties: the better average score). A
//!   company's share of its journeys driven live (`live_share`) is shown on its row of the server
//!   and with its bids.
//! * **K10, fixed duties**: a company may give a player a duty for good - the same duty of the same
//!   tour on that weekday (`FixedDuty`) -, booked for him as each such day begins (as the roster's
//!   duties of a player are) and paid `loyalty_bonus` (5 %) on top; he may give it back.
//! * **K11, supplements for unsocial hours**: the server's admin sets them (`Supplement`: evenings
//!   and nights by the clock, Saturdays, Sundays and holidays, snow days of force majeure), each a
//!   share of the pay or kroner an hour; a player's trip earns those of its departure. AI drivers'
//!   wages keep the economy's own rates (their duties are paid as the model books them).
//! * **K12, the boss drives**: when the owner or a co-owner drove at least `boss_duties` (2) duties
//!   live in the last seven days, the working environment's part (`viborg::work`) has
//!   `BOSS_POINTS` (10) more.
//! * **K13, the live dispatcher**: a player who may run service control takes the dispatcher's
//!   duty while he is online (`OnDuty`): a spare bus sent then reaches its trips in
//!   `DISPATCHER_SPARE` (15) minutes, Casual's controller leaves the late buses to him (he is
//!   shown them and the control room's page), and he answers the doubling orders.

use super::model::{Cents, Company};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// --- K6: urgent tasks ----------------------------------------------------------------------------

/// Whether tour `tour` of `line` is a doubling the server's admin ordered, running on `date`.
fn admin_doubling(c: &Company, date: &str, line: &str, tour: &str) -> bool {
    super::doubling::order_of(c, date, line, tour).is_some_and(|o| o.by == "admin")
}

/// Whether `line` is a rail replacement the company won (its tender's event is the rail's).
pub fn rail_line(c: &Company, line: &str) -> bool {
    c.concessions.tenders.iter().any(|t| {
        t.line.eq_ignore_ascii_case(line) && matches!(t.outcome, Some(super::concessions::Outcome::Won { .. })) && t.market.as_ref().is_some_and(|m| m.event == super::server_awards::RAIL_EVENT)
    })
}

/// K6: whether tour `tour` of `line` is urgent on `date` (a company of the server): an admin's
/// doubling, or rail replacement.
pub fn urgent(c: &Company, date: &str, line: &str, tour: &str) -> bool {
    c.online.on_server && (admin_doubling(c, date, line, tour) || rail_line(c, line))
}

/// K6: whether only a player may drive tour `tour` of `line` on `date`: an admin's doubling (no
/// AI or agency driver takes it).
pub fn players_only(c: &Company, date: &str, line: &str, tour: &str) -> bool {
    c.online.on_server && admin_doubling(c, date, line, tour)
}

/// K6, K10: what a player's pay for a trip of tour `tour` of `line` on `date` is multiplied by: the
/// urgent bonus, the fixed duty's loyalty bonus.
pub fn pay_factor(c: &Company, player: &str, date: &str, line: &str, tour: &str) -> f64 {
    let rules = super::live_pay::cfg(c);
    let mut f = 1.0;
    if urgent(c, date, line, tour) {
        f *= 1.0 + rules.urgent.max(0.0);
    }
    let wd = super::plan::weekday_of(date);
    if let Some(x) = c.online.fixed.iter().find(|x| x.player == player && x.on(wd) && x.line.eq_ignore_ascii_case(line) && x.tour.trim() == tour.trim()) {
        f *= 1.0 + x.bonus.max(0.0);
    }
    f
}

// --- K7: the ranking -----------------------------------------------------------------------------

/// A player's month of live driving: the timetabled minutes of his trips that passed, their scores
/// together and how many.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct LiveMonth {
    pub month: String,
    pub minutes: f64,
    pub score: f64,
    pub trips: u32,
}

/// The months a player keeps.
pub const MONTHS_KEPT: usize = 3;

/// The month (`YYYY-MM`) of a moment (Unix seconds).
pub fn month_of(at: u64) -> String {
    let date = super::dates::fmt((at / 86_400) as i64);
    date.get(..7).unwrap_or("").to_string()
}

/// K7: a trip a player drove (its timetabled `minutes`, judged `score`, `passed`) at `at` (Unix
/// seconds) counted in his months: only a passed one with minutes.
pub fn count_trip(months: &mut Vec<LiveMonth>, at: u64, minutes: f64, score: f64, passed: bool) {
    if !passed || !(minutes > 0.0) {
        return;
    }
    let month = month_of(at);
    match months.iter_mut().find(|m| m.month == month) {
        Some(m) => {
            m.minutes += minutes;
            m.score += score;
            m.trips += 1;
        }
        None => months.push(LiveMonth { month, minutes, score, trips: 1 }),
    }
    months.sort_by(|a, b| b.month.cmp(&a.month));
    months.truncate(MONTHS_KEPT);
}

/// A row of the month's ranking: the player's name, his live hours and his average score.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Ranked {
    pub name: String,
    pub hours: f64,
    pub score: f64,
}

/// K7: the month's ranking of the server's players (`players`: their names and months), the most
/// live hours first, on equal hours the better average score; at most `most`.
pub fn ranking<'a>(players: impl Iterator<Item = (&'a str, &'a [LiveMonth])>, month: &str, most: usize) -> Vec<Ranked> {
    let mut out: Vec<Ranked> = players
        .filter_map(|(name, months)| months.iter().find(|m| m.month == month).map(|m| Ranked { name: name.to_string(), hours: m.minutes / 60.0, score: if m.trips > 0 { m.score / m.trips as f64 } else { 0.0 } }))
        .filter(|r| r.hours > 0.0)
        .collect();
    out.sort_by(|a, b| b.hours.total_cmp(&a.hours).then(b.score.total_cmp(&a.score)).then(a.name.cmp(&b.name)));
    out.truncate(most);
    out
}

/// K7: the share of company `c`'s journeys of the last weeks its players drove live (0 to 1).
pub fn live_share(c: &Company) -> f64 {
    let days: Vec<&super::live_pay::LiveDay> = c.online.live_days.iter().filter(|d| super::dates::between(&d.date, &c.date) < super::live_pay::WINDOW_DAYS).collect();
    let journeys: u32 = days.iter().map(|d| d.journeys).sum();
    let live: u32 = days.iter().map(|d| d.live).sum();
    if journeys == 0 {
        0.0
    } else {
        (live as f64 / journeys as f64).min(1.0)
    }
}

// --- K10: fixed duties ---------------------------------------------------------------------------

/// A duty given to a player for good: duty `duty` (0 the first) of tour `tour` of `line` on the
/// weekdays of `days` (bit 0 Monday), paid `bonus` on top; who (the server's id for him, his number
/// in the company and his name) and since when.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct FixedDuty {
    pub id: u32,
    pub player: String,
    pub guest: u32,
    pub name: String,
    pub line: String,
    pub number: String,
    pub tour: String,
    pub duty: usize,
    pub days: u8,
    pub bonus: f64,
    pub since: String,
}

impl FixedDuty {
    /// Whether it is his on weekday `wd` (0 Monday).
    pub fn on(&self, wd: u8) -> bool {
        wd < 7 && self.days & (1 << wd) != 0
    }
}

/// Why a duty cannot be given for good, or given back.
pub const NOT_ON_SERVER: &str = "Fixed duties are for a company of the server.";
pub const NO_SUCH_PLAYER: &str = "There is no such player in the company.";
pub const FIXED_TAKEN: &str = "This duty is fixed for another player on that weekday already.";
pub const NO_FIXED: &str = "There is no such fixed duty of yours.";

/// K10: duty `duty` of tour `tour` of `line` (its number `number`) given to player `guest` for good
/// on the weekday of `date`. Its id.
pub fn give_fixed(c: &mut Company, guest: u32, date: &str, line: &str, number: &str, tour: &str, duty: usize) -> Result<u32, &'static str> {
    if !c.online.on_server {
        return Err(NOT_ON_SERVER);
    }
    let g = c.together.guests.iter().find(|g| g.no == guest && !g.banned).ok_or(NO_SUCH_PLAYER)?;
    let (player, name) = (g.id.clone(), g.name.clone());
    let wd = super::plan::weekday_of(date);
    if let Some(x) = c.online.fixed.iter_mut().find(|x| x.line.eq_ignore_ascii_case(line) && x.tour.trim() == tour.trim() && x.duty == duty && x.on(wd)) {
        if x.guest != guest {
            return Err(FIXED_TAKEN);
        }
        return Ok(x.id);
    }
    // (the same duty on another weekday of his: that weekday too)
    if let Some(x) = c.online.fixed.iter_mut().find(|x| x.guest == guest && x.line.eq_ignore_ascii_case(line) && x.tour.trim() == tour.trim() && x.duty == duty) {
        x.days |= 1 << wd;
        return Ok(x.id);
    }
    let id = c.online.fixed.iter().map(|x| x.id).max().unwrap_or(0) + 1;
    let bonus = super::live_pay::cfg(c).loyalty;
    c.online.fixed.push(FixedDuty { id, player, guest, name, line: line.to_string(), number: number.to_string(), tour: tour.trim().to_string(), duty, days: 1 << wd, bonus, since: c.date.clone() });
    Ok(id)
}

/// K10: player `guest` gives his fixed duty `id` back (or one who may give duties takes it off him:
/// `any`).
pub fn return_fixed(c: &mut Company, guest: u32, id: u32, any: bool) -> Result<FixedDuty, &'static str> {
    let k = c.online.fixed.iter().position(|x| x.id == id && (any || x.guest == guest)).ok_or(NO_FIXED)?;
    Ok(c.online.fixed.remove(k))
}

/// K10: the player a duty (`duty` of tour `tour` of `line`) is fixed for on weekday `wd`.
pub fn fixed_for(c: &Company, wd: u8, line: &str, tour: &str, duty: usize) -> Option<u32> {
    c.online.fixed.iter().find(|x| x.on(wd) && x.duty == duty && x.line.eq_ignore_ascii_case(line) && x.tour.trim() == tour.trim()).map(|x| x.guest)
}

// --- K11: supplements for unsocial hours ---------------------------------------------------------

/// When a supplement is paid.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum When {
    /// Monday to Friday from `from` to `to` (minutes of the day; past midnight when `to` is less).
    Evening { from: i32, to: i32 },
    /// Every day from `from` to `to`.
    Night { from: i32, to: i32 },
    Saturday,
    /// Sundays and public holidays.
    Sunday,
    /// A day of force majeure for snow (`contract_events`).
    Snow,
}

impl When {
    pub fn key(self) -> &'static str {
        match self {
            When::Evening { .. } => "evening",
            When::Night { .. } => "night",
            When::Saturday => "saturday",
            When::Sunday => "sunday",
            When::Snow => "snow",
        }
    }

    /// The admin panel's words (`when` key, its from and to as `HH:MM`).
    pub fn of(key: &str, from: i32, to: i32) -> Option<When> {
        Some(match key.trim() {
            "evening" => When::Evening { from, to },
            "night" => When::Night { from, to },
            "saturday" => When::Saturday,
            "sunday" => When::Sunday,
            "snow" => When::Snow,
            _ => return None,
        })
    }
}

/// A supplement of the server's admin: its name, when it is paid and how much - a share of the
/// trip's pay (`percent`) and kroner an hour (`per_hour`), either or both.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Supplement {
    pub id: u32,
    pub name: String,
    pub when: When,
    #[serde(default)]
    pub percent: f64,
    #[serde(default)]
    pub per_hour: f64,
}

/// The server's supplements (`<data>/supplements.json`).
pub fn supplements_file(data: &Path) -> PathBuf {
    data.join("supplements.json")
}

pub fn load_supplements(data: &Path) -> Vec<Supplement> {
    std::fs::read_to_string(supplements_file(data)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save_supplements(data: &Path, list: &[Supplement]) -> std::io::Result<()> {
    std::fs::write(supplements_file(data), serde_json::to_string_pretty(list).unwrap_or_else(|_| "[]".into()))
}

/// Why the admin's supplement is refused.
pub const BAD_SUPPLEMENT: &str = "a name, when, and a share or kroner an hour above 0";

/// K11: the admin adds a supplement (its id), or takes one off (`remove`).
pub fn add_supplement(data: &Path, name: &str, when: When, percent: f64, per_hour: f64) -> Result<u32, String> {
    let name = name.trim();
    if name.is_empty() || !(percent >= 0.0 && per_hour >= 0.0) || (percent <= 0.0 && per_hour <= 0.0) || percent > 500.0 || per_hour > 100_000.0 {
        return Err(BAD_SUPPLEMENT.into());
    }
    let mut list = load_supplements(data);
    let id = list.iter().map(|s| s.id).max().unwrap_or(0) + 1;
    list.push(Supplement { id, name: name.chars().take(60).collect(), when, percent, per_hour });
    save_supplements(data, &list).map_err(|e| e.to_string())?;
    Ok(id)
}

pub fn remove_supplement(data: &Path, id: u32) -> Result<Supplement, String> {
    let mut list = load_supplements(data);
    let k = list.iter().position(|s| s.id == id).ok_or("no such supplement")?;
    let s = list.remove(k);
    save_supplements(data, &list).map_err(|e| e.to_string())?;
    Ok(s)
}

/// The supplements file's change stamp (its length and time: 0 none).
fn stamp(data: &Path) -> u64 {
    std::fs::metadata(supplements_file(data)).ok().map_or(0, |m| m.len() ^ m.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs()))
}

/// The company's process reads the admin's supplements again when he changed them. Whether it did.
pub fn refresh_supplements(data: &Path, c: &mut Company) -> bool {
    if !c.online.on_server {
        return false;
    }
    let at = stamp(data);
    if at == c.online.supplements_at && (at != 0 || c.online.supplements.is_empty()) {
        return false;
    }
    c.online.supplements_at = at;
    c.online.supplements = load_supplements(data);
    true
}

/// Whether minute `m` of the day lies in `from`..`to` (past midnight when `to` is less).
fn in_span(m: i32, from: i32, to: i32) -> bool {
    let m = m.rem_euclid(1440);
    if from <= to {
        (from..to).contains(&m)
    } else {
        m >= from || m < to
    }
}

/// Whether supplement `s` is paid for a trip leaving at minute `minute` of `date` (`holiday`: a
/// public holiday, `snow`: a day of force majeure for snow).
pub fn applies(s: &Supplement, date: &str, minute: i32, holiday: bool, snow: bool) -> bool {
    let wd = super::plan::weekday_of(date);
    match s.when {
        When::Evening { from, to } => wd < 5 && !holiday && in_span(minute, from, to),
        When::Night { from, to } => in_span(minute, from, to),
        When::Saturday => wd == 5 && !holiday,
        When::Sunday => wd == 6 || holiday,
        When::Snow => snow,
    }
}

/// Whether `date` is a public holiday for the company (Denmark's at the Danish price level, the
/// common German and Dutch ones elsewhere).
pub fn holiday(c: &Company, date: &str) -> bool {
    let code = date.replace('-', "").parse::<i32>().unwrap_or(0);
    if c.online.price_level.as_deref() == Some("dk") {
        crate::service::danish_public_holiday(code)
    } else {
        crate::service::common_public_holiday(code)
    }
}

/// K11: what the supplements add to a player's pay `base` (cents) for a trip of `hours` leaving at
/// minute `minute` of `date`.
pub fn supplements_pay(c: &Company, date: &str, minute: i32, base: Cents, hours: f64) -> Cents {
    if !c.online.on_server || c.online.supplements.is_empty() || base <= 0 {
        return 0;
    }
    let hol = holiday(c, date);
    let snow = super::contract_events::majeure_on(c, date).is_some_and(|f| f.kind == super::contract_events::Majeure::Snow);
    let per_hour = |kr: f64| super::server_fines::Fines::cents(kr) as f64 * c.price_index.max(0.1);
    c.online
        .supplements
        .iter()
        .filter(|s| applies(s, date, minute, hol, snow))
        .map(|s| (base as f64 * s.percent / 100.0 + per_hour(s.per_hour) * hours.max(0.0)).round() as Cents)
        .sum()
}

// --- K12: the boss drives --------------------------------------------------------------------------

/// A duty the owner or a co-owner drove live: its day, its line and tour.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct BossDrive {
    pub date: String,
    pub line: String,
    pub tour: String,
}

/// The working environment's points the boss's driving brings.
pub const BOSS_POINTS: f64 = 10.0;
/// The days the boss's duties are counted over.
pub const BOSS_DAYS: i64 = 7;

/// K12: player `guest` drove a trip of tour `tour` of `line` on `date` live: kept when he is the
/// owner or a co-owner (a duty a day once).
pub fn boss_drove(c: &mut Company, guest: u32, date: &str, line: &str, tour: &str) {
    if !super::live_pay::may_drive_free(c, guest) {
        return;
    }
    let today = c.date.clone();
    c.online.boss_drives.retain(|d| super::dates::between(&d.date, &today) < 2 * BOSS_DAYS);
    if !c.online.boss_drives.iter().any(|d| d.date == date && d.line.eq_ignore_ascii_case(line) && d.tour.trim() == tour.trim()) {
        c.online.boss_drives.push(BossDrive { date: date.to_string(), line: line.to_string(), tour: tour.trim().to_string() });
    }
}

/// K12: the duties the owner and the co-owners drove live in the last `BOSS_DAYS` days.
pub fn boss_duties(c: &Company) -> usize {
    c.online.boss_drives.iter().filter(|d| (0..BOSS_DAYS).contains(&super::dates::between(&d.date, &c.date))).count()
}

/// K12: the working environment's points on top: `BOSS_POINTS` when the boss drove the rules'
/// duties a week, else none.
pub fn boss_points(c: &Company) -> f64 {
    let need = super::live_pay::cfg(c).boss_duties;
    if c.online.on_server && need > 0 && boss_duties(c) >= need as usize {
        BOSS_POINTS
    } else {
        0.0
    }
}

// --- K13: the live dispatcher --------------------------------------------------------------------

/// The player on the dispatcher's duty: his number in the company, his name, since (Unix seconds).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct OnDuty {
    pub guest: u32,
    pub name: String,
    pub since: u64,
}

/// A spare bus sent while a dispatcher is on duty reaches its trips in this many minutes.
pub const DISPATCHER_SPARE: i32 = 15;

/// Why the duty is refused.
pub const DISPATCH_TAKEN: &str = "Another player is on the dispatcher's duty.";

/// K13: player `guest` (`name`) takes the dispatcher's duty (`on`) or ends it, at `now` (Unix
/// seconds); one at a time.
pub fn set_dispatcher(c: &mut Company, guest: u32, name: &str, on: bool, now: u64) -> Result<(), &'static str> {
    if !c.online.on_server {
        return Err(NOT_ON_SERVER);
    }
    match (&c.online.dispatcher, on) {
        (Some(d), true) if d.guest != guest => Err(DISPATCH_TAKEN),
        (_, true) => {
            c.online.dispatcher = Some(OnDuty { guest, name: name.to_string(), since: now });
            Ok(())
        }
        (Some(d), false) if d.guest == guest => {
            c.online.dispatcher = None;
            Ok(())
        }
        (_, false) => Ok(()),
    }
}

/// K13: whether a player is on the dispatcher's duty.
pub fn dispatcher_on(c: &Company) -> bool {
    c.online.on_server && c.online.dispatcher.is_some()
}

/// K13: the dispatcher's duty ends when he is no longer online (`online`: is he?). Whether it did.
pub fn dispatcher_left(c: &mut Company, online: &dyn Fn(u32) -> bool) -> bool {
    match &c.online.dispatcher {
        Some(d) if !online(d.guest) => {
            c.online.dispatcher = None;
            true
        }
        _ => false,
    }
}

#[cfg(test)]
#[path = "live_more_tests.rs"]
mod tests;
