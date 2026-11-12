//! (NexiconDK, ticket inspection round V, 12 November 2026) A player as one of the company's
//! ticket inspectors (Bus Company Online).
//!
//! - **A patrol taken** (`take`): under Duties a player of the company takes a ticket-inspection
//!   duty on one of its lines in service today (`offers`): where a patrol of the day's (the boss's
//!   plan or the automatic ones) checks the line, its hours from now; else `DEFAULT_LEN` from now.
//!   One a day at a time, at most `MAX_LEN`.
//! - **In the map's world** his game starts on foot at a stop of the line in the company's
//!   inspector's uniform. He walks into the buses there - the company's, another company's, a
//!   player's - and checks the riders one at a time (K by one): the game that runs a rider says
//!   whether they have a ticket (`omsi_sim::people::ticket`), the world sees every check.
//! - **What he did** (`PatrolSeen`, written by the world for the company: `store::WorldTrip::
//!   patrol`): each bus ridden with what he found (`book_seen`: a check of the day like the
//!   modelled ones, fines and all - `inspection_game::book_game`), and his time on patrol, paid as
//!   a free drive is (`live_pay::free_drive_pay`), within his patrol's hours.
//! - **"Bestil kontrol"** from a driver of the line while he is on patrol (`call`): told to him
//!   (his game's inbox) with the stop the bus is near and the time, and to the driver.

use super::inspection_game::{self as ig, GameCheck};
use super::model::{BookingKind, Cents, Company};
use serde::{Deserialize, Serialize};

/// A patrol's hours: the most, and those of one on a line no patrol of the day's checks (min).
pub const MAX_LEN: i32 = 8 * 60;
pub const DEFAULT_LEN: i32 = 4 * 60;
/// Minutes before his patrol begins and after it ends that still count as on it.
pub const SLACK: i32 = 30;
/// The most minutes of one report of his time.
pub const MOST_MINUTES: f64 = 12.0 * 60.0;

/// Refusals and words (English: keys of the translations).
pub const NO_LINE: &str = "The company runs no such line today.";
pub const HAVE_ONE: &str = "You have a ticket-inspection duty today already.";
pub const DAY_OVER: &str = "The day is nearly over: no ticket-inspection duty starts now.";
pub const NOT_ON_PATROL: &str = "You are on no ticket-inspection duty now.";
pub const CALLED: &str = "The driver of line %{line}, tour %{tour} (towards %{stop}), asks for ticket inspection at %{time}.";
pub const PLAYER_COMES: &str = "Ticket inspector %{name} works your line now and has been told.";

/// A player's ticket-inspection duty of a day.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct PlayerPatrol {
    pub date: String,
    /// The player (his number in the company's session) and his name.
    pub guest: u32,
    pub name: String,
    /// The line (the timetable's name) and its number as the company shows it.
    pub line: String,
    pub number: String,
    /// Its hours (minutes of the day; past midnight beyond 1440).
    pub from: i32,
    pub to: i32,
    /// What he was paid for so far (minutes on patrol).
    pub paid_minutes: f64,
    /// The drivers who asked for ticket inspection while he was on patrol.
    pub calls: Vec<Call>,
}

/// A driver's call for ticket inspection to a player on patrol.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Call {
    pub id: u64,
    pub line: String,
    pub tour: String,
    /// Where the bus goes (its trip's terminus), and the minute of the day.
    pub stop: String,
    pub at: i32,
}

/// A patrol a player may take now: the line, its number and colour, its hours, who of the staff
/// patrols it with him.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Offer {
    pub line: String,
    pub number: String,
    pub colour: String,
    pub from: i32,
    pub to: i32,
    pub team: Vec<String>,
}

/// What the world saw a player on patrol do (`store::WorldTrip::patrol`): a bus ridden (`ride`:
/// its line as it showed it, what he checked and caught, the stops he got on and off at, the
/// minute of the day he got on) or his time on patrol (`time`: the minutes).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct PatrolSeen {
    pub kind: String,
    /// The company the patrol is for (its id).
    pub company: String,
    pub line: String,
    pub checked: u32,
    pub caught: u32,
    pub board_stop: String,
    pub leave_stop: String,
    pub minute: i32,
    pub minutes: f64,
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// The company's lines in service on `date`.
fn lines_of<'a>(c: &'a Company, date: &str) -> Vec<&'a super::model::CompanyLine> {
    c.lines.iter().filter(|l| super::network::in_service(l, super::clock::moment(date, 24 * 60 - 1))).collect()
}

/// The patrols a player may take at minute `now` of `date`: each line in service, with the hours
/// of the day's patrol on it from now (or `DEFAULT_LEN` from now).
pub fn offers(c: &Company, date: &str, now: i32) -> Vec<Offer> {
    let patrols = super::staff::patrols(c, date);
    let mut out = Vec::new();
    for l in lines_of(c, date) {
        let p = patrols.iter().filter(|p| p.stop.is_none() && same(&p.line, &l.name) && p.to > now).min_by_key(|p| p.from);
        let (from, to, team) = match p {
            Some(p) => (p.from.max(now), p.to.min(now.max(p.from) + MAX_LEN), p.ids.iter().filter_map(|id| c.employee(*id).map(|e| e.name.clone())).collect()),
            None => (now, now + DEFAULT_LEN, Vec::new()),
        };
        if to - from >= 30 {
            out.push(Offer { line: l.name.clone(), number: l.number.clone(), colour: l.colour.clone(), from, to, team });
        }
    }
    out
}

/// Player `guest` (`name`) takes a patrol on line `line` at minute `now` of the company's day.
pub fn take(c: &mut Company, guest: u32, name: &str, line: &str, now: i32) -> Result<PlayerPatrol, &'static str> {
    let date = c.date.clone();
    if c.inspection_plan.players.iter().any(|p| p.guest == guest && p.date == date && p.to > now) {
        return Err(HAVE_ONE);
    }
    if now > 24 * 60 - 30 {
        return Err(DAY_OVER);
    }
    let o = offers(c, &date, now).into_iter().find(|o| same(&o.line, line)).ok_or(NO_LINE)?;
    let p = PlayerPatrol { date, guest, name: name.to_string(), line: o.line, number: o.number, from: o.from, to: o.to, ..Default::default() };
    c.inspection_plan.players.push(p.clone());
    Ok(p)
}

/// Player `guest`'s patrol at minute `now` of `date` (`SLACK` before and after it too).
pub fn of<'a>(c: &'a Company, guest: u32, date: &str, now: i32) -> Option<&'a PlayerPatrol> {
    c.inspection_plan.players.iter().filter(|p| p.guest == guest && p.date == date && now >= p.from - SLACK && now < p.to + SLACK).min_by_key(|p| p.from)
}

/// The player on patrol now on line `line`, if one is.
pub fn on_line<'a>(c: &'a Company, line: &str, now: i32) -> Option<&'a PlayerPatrol> {
    c.inspection_plan.players.iter().find(|p| p.date == c.date && same(&p.line, line) && now >= p.from && now < p.to)
}

/// A driver of `line`/`tour` asked for ticket inspection (message `id`; his bus towards `stop`) at minute
/// `now`: told the player on patrol on that line - his name, else None (the staff's turn).
pub fn call(c: &mut Company, line: &str, tour: &str, stop: &str, now: i32, id: u64) -> Option<String> {
    let date = c.date.clone();
    let p = c.inspection_plan.players.iter_mut().find(|p| p.date == date && same(&p.line, line) && now >= p.from && now < p.to)?;
    p.calls.push(Call { id, line: line.to_string(), tour: tour.to_string(), stop: stop.to_string(), at: now });
    if p.calls.len() > 20 {
        p.calls.remove(0);
    }
    Some(p.name.clone())
}

/// What the world saw player `guest` (`name`) on patrol do, booked: a bus ridden as a check of
/// the day, his time paid (within his patrol's hours, once). His pay; an error when he has no
/// patrol now, or the report does not add up.
pub fn book_seen(c: &mut Company, guest: u32, name: &str, s: &PatrolSeen) -> Result<Cents, &'static str> {
    let now = c.clock.minute as i32;
    let date = c.date.clone();
    let Some(k) = c.inspection_plan.players.iter().position(|p| p.guest == guest && p.date == date && now >= p.from - SLACK && now < p.to + 2 * SLACK) else { return Err(NOT_ON_PATROL) };
    match s.kind.as_str() {
        "ride" => {
            let line = if s.line.trim().is_empty() { c.inspection_plan.players[k].line.clone() } else { s.line.trim().chars().take(64).collect() };
            let g = GameCheck { line, dep: s.minute, date, checked: s.checked, caught: s.caught.min(s.checked), fled: 0, stops: 0, board_stop: s.board_stop.chars().take(64).collect(), leave_stop: s.leave_stop.chars().take(64).collect(), names: vec![name.to_string()], ordered: false, driver: name.to_string() };
            ig::book_game(c, &g)?;
            Ok(0)
        }
        "time" => {
            let p = &c.inspection_plan.players[k];
            let room = ((p.to - p.from) as f64 - p.paid_minutes).max(0.0);
            let minutes = s.minutes.clamp(0.0, MOST_MINUTES).min(room);
            if minutes < 1.0 {
                return Ok(0);
            }
            c.inspection_plan.players[k].paid_minutes += minutes;
            let player = c.together.guests.iter().find(|g| g.no == guest).map(|g| g.id.clone()).unwrap_or_default();
            let pay = if super::live_pay::drives_free(c, &player) { 0 } else { super::live_pay::free_drive_pay(c, minutes) };
            if pay > 0 {
                c.book(BookingKind::Wages, -pay, format!("{name}: ticket inspection, {:.0} min (a player of the company)", minutes), false);
            }
            Ok(pay)
        }
        _ => Err("That inspection report does not add up."),
    }
}

/// Days gone by let go (the day's close).
pub fn prune(c: &mut Company, today: &str) {
    let keep = super::dates::add(today, -2);
    c.inspection_plan.players.retain(|p| super::dates::between(&keep, &p.date) >= 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::company::{found, Founding};

    fn company() -> Company {
        let mut c = found(&Founding { name: "Umove".into(), date: "2024-03-04".into(), ..Default::default() }, "Mathias");
        c.lines.push(crate::company::model::CompanyLine { name: "Linie5".into(), number: "5".into(), service_from: Some(crate::company::model::LEGACY_SERVICE), ..Default::default() });
        c
    }

    #[test]
    fn a_player_takes_a_patrol_and_is_paid_for_his_time_on_it() {
        let mut c = company();
        let o = offers(&c, &c.date.clone(), 600);
        assert_eq!(o.len(), 1, "the line in service");
        assert_eq!(take(&mut c, 2, "Urban", "Linie14", 600), Err(NO_LINE));
        assert_eq!((o[0].line.as_str(), o[0].from, o[0].to), ("Linie5", 600, 600 + DEFAULT_LEN));
        let p = take(&mut c, 2, "Urban", "linie5", 600).unwrap();
        assert_eq!((p.line.as_str(), p.from, p.to), ("Linie5", 600, 840));
        assert_eq!(take(&mut c, 2, "Urban", "Linie5", 610), Err(HAVE_ONE));
        assert!(of(&c, 2, &c.date.clone(), 620).is_some() && of(&c, 3, &c.date.clone(), 620).is_none());
        // a driver's call reaches him
        c.clock.minute = 650;
        assert_eq!(call(&mut c, "Linie5", "3", "Markt", 650, 7).as_deref(), Some("Urban"));
        assert_eq!(call(&mut c, "Linie14", "1", "Markt", 650, 8), None);
        // his time, within his hours, once
        let s = PatrolSeen { kind: "time".into(), minutes: 300.0, ..Default::default() };
        let cash = c.cash;
        let pay = book_seen(&mut c, 2, "Urban", &s).unwrap();
        assert_eq!(c.inspection_plan.players[0].paid_minutes, 240.0);
        assert_eq!(c.cash, cash - pay);
        assert_eq!(book_seen(&mut c, 2, "Urban", &s), Ok(0), "paid once");
        // a bus ridden: a check of the day with his name on it
        let r = PatrolSeen { kind: "ride".into(), line: "5".into(), checked: 12, caught: 2, minute: 655, ..Default::default() };
        book_seen(&mut c, 2, "Urban", &r).unwrap();
        let k = ig::of_day(&c, &c.date.clone());
        assert_eq!((k.len(), k[0].checked, k[0].caught, k[0].game.as_str()), (1, 12, 2, "Urban"));
        // nobody else's
        assert_eq!(book_seen(&mut c, 3, "Eve", &r), Err(NOT_ON_PATROL));
    }
}
