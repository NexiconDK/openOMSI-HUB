//! (NexiconDK, Bus Company Online, decided 3 November 2026) Driving yourself pays (the plan's
//! K1-K5). A player who drove a duty for a company was paid 75 % of what its trips brought in
//! (`online::pay_of`) while an AI driver costs only his wage: at the Danish price level a player
//! cost about 750 kr. a timetable hour, an AI driver about 550 - an owner did best to bid and let
//! the AI drive. Now, on the server:
//!
//! * **K1, a player is cheaper than an AI driver**: his pay for a trip of his duty is
//!   `player_pay_share` (90 %) of what an AI driver costs for the same time - a paid hour all in
//!   (`pay::driver_hour`) by the trip's timetabled time and the paid hours a timetabled hour
//!   takes (`econ_uk::PAID_PER_TIMETABLED_HOUR`); a free drive the same share of the hours driven.
//! * **K2, live driving scores better operating quality**: a live trip counts its judgement
//!   (`career::judge_exam`, 0 to 100 - punctual, smooth, safe) in the company's operating quality
//!   when it passed, at most the AI's cap when it did not; the AI's trips count the track record,
//!   at most `ai_quality_cap` (70). Over the last `WINDOW_DAYS` days: the share of the journeys run
//!   live by the live trips' average, the rest by the capped record (`operation`).
//! * **K3, a live surcharge**: the authority pays `live_surcharge` (5 %) of the hour price on top
//!   for the timetabled hours run live with a passed judgement (`bus_contracts::close_day`).
//! * **K4, drivers are short**: the AI drivers a map's companies may hire are `driver_pool` (none:
//!   as many as they like); none left, a company's open duties are run by agency drivers at the
//!   agency's price - the AI always drives what no player does (`plan`), nothing is dropped for
//!   want of a driver.
//! * **K5, the owner chooses**: the owner and the co-owners of a company choose whether their own
//!   live driving for it pays them or is free (`Online::drives_free`).
//!
//! A company off the server keeps the old pay.

use super::model::{Cents, Company};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// The days the operating quality looks back on.
pub const WINDOW_DAYS: i64 = 56;

/// The server's rules (`company-server.cfg`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct LiveCfg {
    /// A player's pay against an AI driver's cost for the same time (0.9: 90 %).
    pub pay_share: f64,
    /// The most an AI-driven trip counts in the operating quality (0 to 100).
    pub ai_cap: f64,
    /// The authority's surcharge on the hour price for the hours driven live (0.05: 5 %).
    pub surcharge: f64,
    /// The AI drivers a map's companies may hire (0: as many as they like).
    pub pool: u32,
    /// (K4, 3 November 2026) Whether a duty nobody covers is run by a free colleague or an agency
    /// driver (`agency_fallback`; each company's owner may choose otherwise: `Online::agency`).
    pub agency: bool,
    /// (K6) What an urgent task pays a player on top (0.5: 50 %).
    pub urgent: f64,
    /// (K10) What a fixed duty pays its player on top (0.05: 5 %).
    pub loyalty: f64,
    /// (K12) The duties a week the owner or a co-owner drives live for the working environment's
    /// points (`live_more::BOSS_POINTS`; 0: never).
    pub boss_duties: u32,
    /// (NexiconDK, 4 November 2026) The days of notice a change of a driver's employment takes on
    /// the server (`employment_notice_days`; 0: from the next company day - `staff::notice_day`).
    pub notice_days: i64,
    /// (NexiconDK, 4 November 2026) The fine a company pays when its planner breaks the rest rules
    /// (`rest_fine`, kroner; 0: none - `pro_rest::fine`). Cents.
    #[serde(default)]
    pub rest_fine: super::model::Cents,
}

impl Default for LiveCfg {
    fn default() -> LiveCfg {
        LiveCfg { pay_share: 0.9, ai_cap: 70.0, surcharge: 0.05, pool: 0, agency: true, urgent: 0.5, loyalty: 0.05, boss_duties: 2, notice_days: super::staff::SERVER_NOTICE_DAYS, rest_fine: 0 }
    }
}

impl LiveCfg {
    /// The rules of the settings' keys (`player_pay_share` and `live_surcharge` in per cent,
    /// `ai_quality_cap` 0 to 100, `driver_pool` a number), the defaults where one is missing.
    pub fn of_keys(kv: &HashMap<String, String>) -> LiveCfg {
        let d = LiveCfg::default();
        let num = |k: &str| kv.get(k).and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok()).filter(|x| x.is_finite() && *x >= 0.0);
        LiveCfg {
            pay_share: num("player_pay_share").map_or(d.pay_share, |x| x.min(200.0) / 100.0),
            ai_cap: num("ai_quality_cap").map_or(d.ai_cap, |x| x.min(100.0)),
            surcharge: num("live_surcharge").map_or(d.surcharge, |x| x.min(100.0) / 100.0),
            pool: num("driver_pool").map_or(d.pool, |x| x.round().min(100_000.0) as u32),
            agency: num("agency_fallback").map_or(d.agency, |x| x != 0.0),
            urgent: num("urgent_bonus").map_or(d.urgent, |x| x.min(500.0) / 100.0),
            loyalty: num("loyalty_bonus").map_or(d.loyalty, |x| x.min(100.0) / 100.0),
            boss_duties: num("boss_duties").map_or(d.boss_duties, |x| x.round().min(100.0) as u32),
            notice_days: num("employment_notice_days").map_or(d.notice_days, |x| x.round().min(365.0) as i64),
            rest_fine: num("rest_fine").map_or(d.rest_fine, |x| (x.min(1_000_000.0) * 100.0).round() as super::model::Cents),
        }
    }

    /// Of the main server's folder `data`.
    pub fn of(data: &Path) -> LiveCfg {
        LiveCfg::of_keys(&super::server_fines::settings(data))
    }
}

/// (3 November 2026) The second part of the rules (K4's choice, K6, K10, K12), added to a
/// `company-server.cfg` written before it.
pub const MORE_LIVE_CFG: &str = "
# a duty nobody covers: run by a free colleague or an agency driver (1) or dropped (0) - each
# company's owner may choose otherwise; an urgent task (short-notice doubling, rail replacement)
# pays a player this much more (per cent); a fixed duty pays its player this much more (per cent);
# the duties a week the owner or a co-owner drives live for the working environment's points
agency_fallback = 1
urgent_bonus = 50
loyalty_bonus = 5
boss_duties = 2
";

/// (NexiconDK, 4 November 2026) The notice of a change of a driver's employment, added to a
/// `company-server.cfg` written before it.
pub const NOTICE_CFG: &str = "
# the days of notice a change of a driver's employment takes (full time, part time, reserve,
# hourly paid; 0: from the next day) - from hourly paid to part or full time is always the next day
employment_notice_days = 2
";

/// (NexiconDK, 4 November 2026) The fine for a breach of the rest rules, added to a
/// `company-server.cfg` written before it.
pub const REST_CFG: &str = "
# the fine a company pays when its planner breaks the rest rules anyway (11 hours' rest, a day of
# at most 13 hours, a week of at most 48; kroner, 0: none)
rest_fine = 0
";

/// The rules' part of the settings, added to a `company-server.cfg` written before it.
pub const LIVE_CFG: &str = "
# driving yourself pays: a player's pay against what an AI driver costs for the same time (per
# cent); the most an AI-driven trip counts in the operating quality (0-100: a live trip that
# passes counts its judgement); the authority's surcharge on the hour price for the hours driven
# live (per cent, 0: none); the AI drivers a map's companies may hire (empty or 0: as many as they
# like - none left, agency drivers run their open duties at the agency's price)
player_pay_share = 90
ai_quality_cap = 70
live_surcharge = 5
driver_pool =
";

/// The company's rules: its server's as its process read them, the defaults elsewhere.
pub fn cfg(c: &Company) -> LiveCfg {
    c.online.live.unwrap_or_default()
}

/// What an AI driver costs the company a paid hour, all in, at its price level (cents).
pub fn ai_hour(c: &Company) -> f64 {
    super::pay::driver_hour(c)
}

/// A trip's timetabled time, hours: its first stop's departure to its last's arrival (past
/// midnight too); none known, the time it took.
pub fn trip_hours(t: &crate::TripRun) -> f64 {
    let mut s = t.arrival - t.departure;
    if s < 0.0 {
        s += 86_400.0;
    }
    if !(s > 0.0 && s < 86_400.0) {
        s = t.seconds.max(0.0);
    }
    s / 3600.0
}

/// K1: a player's pay for a trip of his duty on the server: the rules' share of what an AI driver
/// costs for its paid hours.
pub fn duty_pay(c: &Company, t: &crate::TripRun) -> Cents {
    (cfg(c).pay_share * ai_hour(c) * trip_hours(t) * super::econ_uk::PAID_PER_TIMETABLED_HOUR).round().max(0.0) as Cents
}

/// K1: a player's pay for `minutes` of a free drive on the server: the rules' share of an AI
/// driver's hour by the time driven (a free drive's hours are its own, no duty's).
pub fn free_drive_pay(c: &Company, minutes: f64) -> Cents {
    (cfg(c).pay_share * ai_hour(c) * minutes.max(0.0) / 60.0).round() as Cents
}

/// K5: whether player `player` (the server's id for him) drives for company `c` for nothing - the
/// owner or a co-owner who chose so.
pub fn drives_free(c: &Company, player: &str) -> bool {
    !player.is_empty() && c.online.drives_free.iter().any(|p| p == player)
}

/// What a player is paid for a trip of his duty that brought the company `money`: on the server
/// K1 (nothing when he drives for free, K5), elsewhere the company's share of it as before.
pub fn trip_pay(c: &Company, player: &str, t: &crate::TripRun, money: Cents) -> Cents {
    if !c.online.on_server {
        return super::online::pay_of(c, money);
    }
    if drives_free(c, player) {
        return 0;
    }
    // (3 November 2026: an urgent task's bonus and a fixed duty's - K6, K10 -, and the admin's
    // supplements for the hour it leaves - K11)
    let date = c.date.clone();
    let base = (duty_pay(c, t) as f64 * super::live_more::pay_factor(c, player, &date, &t.line, &t.tour)).round() as Cents;
    let minute = ((t.departure / 60.0).floor() as i32).rem_euclid(1440);
    base + super::live_more::supplements_pay(c, &date, minute, base, trip_hours(t))
}

/// K5: whether player `guest` (his number in the company) may choose to drive for nothing: the
/// owner of a company on the server, or a co-owner.
pub fn may_drive_free(c: &Company, guest: u32) -> bool {
    if !c.online.on_server {
        return false;
    }
    super::together::host::server_owner(c) == Some(guest) || c.together.guests.iter().any(|g| g.no == guest && g.role == super::together::Role::CoOwner)
}

/// Why a player may not choose.
pub const NOT_OWNER: &str = "Only the owner and the co-owners of the company choose whether their own driving is paid.";

/// K5: player `guest` chooses whether his own live driving for the company pays him (`free`
/// false) or is free.
pub fn set_drives_free(c: &mut Company, guest: u32, free: bool) -> Result<(), &'static str> {
    if !may_drive_free(c, guest) {
        return Err(NOT_OWNER);
    }
    let id = c.together.guests.iter().find(|g| g.no == guest).map(|g| g.id.clone()).filter(|id| !id.is_empty()).ok_or(NOT_OWNER)?;
    c.online.drives_free.retain(|p| p != &id);
    if free {
        c.online.drives_free.push(id);
    }
    Ok(())
}

/// K3: the authority's surcharge on an hour price of `hour` (cents, at today's contract index)
/// for `minutes` of timetabled time driven live and passed.
pub fn surcharge(c: &Company, hour: f64, minutes: i64) -> Cents {
    if minutes <= 0 {
        return 0;
    }
    (cfg(c).surcharge * hour.max(0.0) * minutes as f64 / 60.0).round() as Cents
}

// --- K2: the operating quality ---------------------------------------------------------------------

/// A day of the company's on the server: the journeys run of its lines, those of them its
/// players drove live, and what their judgements counted together (each 0 to 100).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct LiveDay {
    pub date: String,
    pub journeys: u32,
    pub live: u32,
    pub score: f64,
}

/// What a live trip counts in the operating quality: its judgement's score when it passed, at
/// most the AI's cap when it did not.
pub fn trip_counts(score: f64, passed: bool, cap: f64) -> f64 {
    let s = score.clamp(0.0, 100.0);
    if passed {
        s
    } else {
        s.min(cap)
    }
}

/// K2: day `date` closed with `journeys` journeys run and these live trips (score, passed): kept
/// with the company for `WINDOW_DAYS`.
pub fn record_day(c: &mut Company, date: &str, journeys: u32, live: &[(f64, bool)]) {
    if !c.online.on_server {
        return;
    }
    let cap = cfg(c).ai_cap;
    let day = LiveDay { date: date.to_string(), journeys: journeys.max(live.len() as u32), live: live.len() as u32, score: live.iter().map(|(s, p)| trip_counts(*s, *p, cap)).sum() };
    c.online.live_days.retain(|d| d.date != date && super::dates::between(&d.date, date) < WINDOW_DAYS);
    c.online.live_days.push(day);
}

/// K2: the company's operating quality (0 to 100) on the server: over the last `WINDOW_DAYS` days
/// the journeys its players ran live count their judgements, the rest the track record at most
/// the AI's cap - `(1 - s) × min(record, cap) + s × live`, `s` the live share of the journeys run,
/// `live` the live trips' average. No day known yet: the capped record.
pub fn operation(c: &Company) -> f64 {
    let record = super::bus_contracts::quality(c).clamp(0.0, 100.0);
    let cap = cfg(c).ai_cap.clamp(0.0, 100.0);
    let ai = record.min(cap);
    let days: Vec<&LiveDay> = c.online.live_days.iter().filter(|d| super::dates::between(&d.date, &c.date) < WINDOW_DAYS).collect();
    let journeys: u32 = days.iter().map(|d| d.journeys).sum();
    let live: u32 = days.iter().map(|d| d.live).sum();
    if journeys == 0 || live == 0 {
        return ai;
    }
    let share = (live as f64 / journeys as f64).min(1.0);
    let avg = days.iter().map(|d| d.score).sum::<f64>() / live as f64;
    ((1.0 - share) * ai + share * avg).clamp(0.0, 100.0)
}

// --- K4: the drivers of a map --------------------------------------------------------------------

/// The AI drivers company `c` employs (those under notice too, until they go).
pub fn drivers_of(c: &Company) -> u32 {
    c.staff.iter().filter(|e| e.role == super::model::Role::Driver).count() as u32
}

/// Why no AI driver can be hired.
pub const DRIVERS_SHORT: &str = "No AI driver is left to hire on this map: drivers are short. Your duties nobody drives are run by agency drivers at the agency's price - or by players.";

/// K4: the AI drivers of the map's pool the companies of the server do not employ, company `c`'s
/// own counted as it is now (None: no pool, as many as they like). `others`: the drivers the
/// map's other companies employ.
pub fn pool_free(c: &Company, others: u32) -> Option<i64> {
    let pool = cfg(c).pool;
    (pool > 0).then(|| pool as i64 - others as i64 - drivers_of(c) as i64)
}

/// The drivers the other companies of `c`'s map on the server employ (their files).
pub fn others_drivers(data: &Path, c: &Company) -> u32 {
    let key = super::shared_tenders::key_of(&c.map);
    super::store::list(data, "").iter().filter(|o| o.id != c.id && o.online.on_server && super::shared_tenders::key_of(&o.map) == key).map(drivers_of).sum()
}

/// K4: the map's pool as the company's process last counted it: (free, the pool); None without
/// one.
pub fn pool_of(c: &Company) -> Option<(i64, u32)> {
    let pool = cfg(c).pool;
    (pool > 0).then(|| (c.online.pool_free.unwrap_or(pool as i64), pool))
}

/// K4: whether an AI driver may be hired: a pool with none left refuses.
pub fn may_hire_driver(c: &Company) -> Result<(), &'static str> {
    match pool_of(c) {
        Some((free, _)) if c.online.on_server && free <= 0 => Err(DRIVERS_SHORT),
        _ => Ok(()),
    }
}

/// K4: an AI driver hired from the map's pool: one fewer free until it is counted again.
pub fn took_driver(c: &mut Company) {
    if let Some(f) = c.online.pool_free.as_mut() {
        *f -= 1;
    }
}

/// K4: the open duties of a company are run by agency drivers (or a free colleague) when nobody
/// drives them, or dropped. On the server only its admin decides (`agency_fallback`, the server's
/// rules - NexiconDK, 4 November 2026: "it is the server's admin, not the company's owner");
/// off the server, where there is no admin, the company's owner (`Online::agency`; none chosen:
/// dropped - agency drivers are hardly ever used in Denmark).
pub fn agency_runs_open(c: &Company) -> bool {
    if c.online.on_server {
        cfg(c).agency
    } else {
        c.online.agency.unwrap_or(false)
    }
}

/// Why a player may not choose.
pub const NOT_OWNER_AGENCY: &str = "Only the owner and the co-owners of the company choose what happens to the duties nobody covers.";
/// Why nobody of the company chooses it on the server.
pub const ADMIN_DECIDES: &str = "On the server the admin decides whether agency drivers run the duties nobody covers.";

/// K4: what happens to a duty nobody covers, chosen for the company: run by agency drivers
/// (true), dropped (false or None). Off the server only (`guest` ignored): on the server the
/// admin decides (`ADMIN_DECIDES`).
pub fn set_agency(c: &mut Company, _guest: u32, agency: Option<bool>) -> Result<(), &'static str> {
    if c.online.on_server {
        return Err(ADMIN_DECIDES);
    }
    c.online.agency = agency;
    Ok(())
}

#[cfg(test)]
#[path = "live_pay_tests.rs"]
mod tests;
