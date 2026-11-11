//! (NexiconDK, ticket inspection round U, 11 November 2026) The company's ticket inspection in
//! the game the player drives.
//!
//! - **The share without a ticket** of each trip of the player's duty (`for_duty`): the same as a
//!   modelled trip's (`dodgers::share`: its hour, the inspectors who checked the line lately) -
//!   the game draws who boards without one from it.
//! - **Inspectors boarding** some of the duty's trips (`for_duty`): where a team of the day's
//!   patrols (`staff::patrols`: the boss's plan or the automatic ones) checks the duty's line in
//!   its hours, a trip of the duty is boarded as often as a modelled bus of that line is - a team
//!   boards a bus about every `CYCLE` minutes, so a trip is boarded with about `CYCLE`'s share of
//!   the line's buses an hour -, at a stop on the way, for 3 to 8 stops (`inspection::STRETCH`); a
//!   storaktion's stop, when the trip passes it in its hours: one stop. In the clothes the boss
//!   chose (`Look`): the figures' generated `~Inspector_<look>_<company>` item (`variant`).
//! - **"Bestil kontrol"** (`answer`, the driver's quick button `messages::Quick::RequestInspection`,
//!   once a trip): a team on duty now on the same line, or on a line with a terminus of the
//!   driver's trip, can get on a few stops ahead - told back with the stop and the time (an
//!   inbox message carried out by itself, `Msg::inspection`) - or nobody is near.
//! - **What they did** (`book_game`: the plugin's report "inspection", for the company's own
//!   launcher a queued order, on Bus Company Online the player's launcher's request): a check of
//!   the day like the modelled ones, with "the player's trip" on it; the day's close takes it
//!   (`inspection::close`), fines and all, and the model does not check that trip again.

use super::inspection::{Check, ESCAPE, STRETCH};
use super::model::Company;
use super::plan::DayPlan;
use super::rng::Rng;
use serde::{Deserialize, Serialize};

/// How the inspectors look in the game, chosen by the boss (the Ticket inspection page).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum Look {
    /// A: a navy uniform and a neon-yellow vest, BILLETKONTROL on the back.
    #[default]
    Vest,
    /// B: their own clothes and an ID card on a lanyard.
    Plain,
    /// C: the company's uniform in its colour, its name on the back.
    Company,
    /// A, B and C, a team each.
    Mixed,
}

impl Look {
    pub const ALL: [Look; 4] = [Look::Vest, Look::Plain, Look::Company, Look::Mixed];

    pub fn key(self) -> &'static str {
        match self {
            Look::Vest => "vest",
            Look::Plain => "plain",
            Look::Company => "company",
            Look::Mixed => "mixed",
        }
    }

    pub fn of(key: &str) -> Option<Look> {
        Look::ALL.into_iter().find(|l| l.key() == key)
    }

    /// Its words (English, translated when shown).
    pub fn label(self) -> &'static str {
        match self {
            Look::Vest => "A: uniform and yellow vest",
            Look::Plain => "B: plain clothes",
            Look::Company => "C: the company's uniform",
            Look::Mixed => "Mixed: A, B and C",
        }
    }

    /// The letter of the clothes of a team (`seed`: which team, for the mixed look).
    pub fn letter(self, seed: u64) -> char {
        match self {
            Look::Vest => 'A',
            Look::Plain => 'B',
            Look::Company => 'C',
            Look::Mixed => ['A', 'B', 'C'][(seed % 3) as usize],
        }
    }
}

/// How often a team boards a bus (minutes: a few stops and the wait for the next).
pub const CYCLE: f64 = 13.0;
/// The most likely a trip of the duty is boarded.
pub const MOST: f64 = 0.6;
/// The stops ahead a team asked for gets on at, and the minutes a stop.
pub const AHEAD: u32 = 3;
pub const STOP_MINUTES: f64 = 2.0;

/// Refusals and answers (English: keys of the translations).
pub const ASKED_ALREADY: &str = "Ticket inspectors were asked for on this trip already.";
pub const NO_INSPECTORS: &str = "The company has no ticket inspectors at work now.";
pub const NOBODY_NEAR: &str = "No ticket inspectors are near you right now.";
pub const ON_THEIR_WAY: &str = "Ticket inspectors %{names} get on in about %{n} stops, at about %{time}.";

/// The company's short name made safe for a file name (the clothes' item names it).
pub fn slug(c: &Company) -> String {
    let base = if c.short.trim().is_empty() { c.name.as_str() } else { c.short.as_str() };
    let s: String = base.chars().filter(|ch| ch.is_ascii_alphanumeric()).map(|ch| ch.to_ascii_lowercase()).take(16).collect();
    if s.is_empty() {
        "company".into()
    } else {
        s
    }
}

/// The clothes' item of look `letter` for company `c` (the game's `inspect::variant_name`).
pub fn variant(c: &Company, letter: char) -> String {
    format!("~Inspector_{}_{}", letter.to_ascii_uppercase(), slug(c))
}

/// The letters of the clothes the company's inspectors wear (the figures to dress).
pub fn letters(c: &Company) -> Vec<char> {
    match c.inspection_plan.look {
        Look::Mixed => vec!['A', 'B', 'C'],
        l => vec![l.letter(0)],
    }
}

/// A trip of the duty and its share without a ticket.
#[derive(Clone, Debug, PartialEq)]
pub struct TripShare {
    pub dep: i32,
    pub share: f64,
}

/// Inspectors boarding a trip of the duty.
#[derive(Clone, Debug, PartialEq)]
pub struct Boarding {
    pub dep: i32,
    /// The stop they get on at (from 1), the stops they ride, how many.
    pub board: u32,
    pub stops: u32,
    pub n: u32,
    pub variant: String,
    pub names: Vec<String>,
}

/// The company's inspection on the player's duty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ForGame {
    pub share: f64,
    pub shares: Vec<TripShare>,
    pub boardings: Vec<Boarding>,
    pub can_order: bool,
    pub variant: String,
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// A trip's stops: its own, else about one every two minutes.
fn stops_of(t: &super::network::PlannedTrip) -> u32 {
    if t.stops >= 2 {
        t.stops
    } else {
        ((t.arr - t.dep) / 2).clamp(4, 40) as u32
    }
}

fn names_of(c: &Company, ids: &[u32]) -> Vec<String> {
    ids.iter().map(|id| c.employee(*id).map(|e| e.name.clone()).unwrap_or_default()).collect()
}

/// The company's inspection on the duty of tour `tour` of line `line` on `dp`'s date: the trips'
/// shares without a ticket and the inspectors boarding some of them. None: the duty is not in the
/// day's plan.
pub fn for_duty(c: &Company, dp: &DayPlan, line: &str, tour: &str) -> Option<ForGame> {
    let date = dp.date.as_str();
    let t = dp.tours.iter().find(|t| same(&t.tour.line, line) && same(&t.tour.tour, tour))?;
    let deter = super::dodgers::deterrence(c, date, &t.tour.line);
    let patrols = super::staff::patrols(c, date);
    let mut out = ForGame { share: super::dodgers::share(super::dodgers::BASE, deter, false), can_order: !patrols.is_empty(), variant: variant(c, c.inspection_plan.look.letter(0)), ..Default::default() };
    for (k, trip) in t.tour.trips.iter().enumerate().filter(|(_, x)| x.counts()) {
        out.shares.push(TripShare { dep: trip.dep, share: super::dodgers::share(super::dodgers::raw(trip.dep, false), deter, false) });
        let mut rng = Rng::of(&[&c.id, date, "game-inspection", &t.tour.line, &t.tour.tour], trip.dep as i64);
        for (pi, p) in patrols.iter().enumerate() {
            let look = variant(c, c.inspection_plan.look.letter(pi as u64));
            // (a storaktion at a stop the trip passes in its hours: they get on there for a stop)
            if let Some(stop) = &p.stop {
                let Some(duty) = super::inspection_plan::assigned(c, date).and_then(|a| a.into_iter().find(|x| x.duty.id == p.duty)).map(|x| x.duty) else { continue };
                let Some(at) = super::inspection_plan::passes_at(&duty, &t.tour.line, &trip.from, trip.dep, trip.arr) else { continue };
                if at < p.from || at >= p.to {
                    continue;
                }
                let n = stops_of(trip);
                let board = (1 + ((at - trip.dep) as f64 / (trip.arr - trip.dep).max(1) as f64 * (n - 1) as f64).round() as u32).clamp(1, n);
                let _ = stop;
                out.boardings.push(Boarding { dep: trip.dep, board, stops: 1, n: p.ids.len().min(4) as u32, variant: look, names: names_of(c, &p.ids) });
                break;
            }
            if !same(&p.line, &t.tour.line) || trip.dep < p.from || trip.dep >= p.to {
                continue;
            }
            // (the line's buses an hour in the team's hours: a trip of the duty is boarded as often
            // as one of them)
            let hours = ((p.to - p.from) as f64 / 60.0).max(1.0);
            let buses = dp.tours.iter().filter(|x| same(&x.tour.line, &t.tour.line)).flat_map(|x| x.tour.trips.iter()).filter(|x| x.counts() && x.dep >= p.from && x.dep < p.to).count() as f64;
            let chance = ((60.0 / CYCLE) / (buses / hours).max(1.0)).min(MOST);
            if !rng.chance(chance) {
                continue;
            }
            let n = stops_of(trip);
            let ride = rng.int(STRETCH.0, STRETCH.1) as u32;
            let board = if n > ride + 1 { 1 + rng.int(0, (n - ride - 1) as i64) as u32 } else { 1 };
            out.boardings.push(Boarding { dep: trip.dep, board, stops: ride.min(n.saturating_sub(board)).max(1), n: p.ids.len().min(4) as u32, variant: look, names: names_of(c, &p.ids) });
            let _ = k;
            break;
        }
    }
    Some(out)
}

/// Inspectors the driver asked for, as control answered (an inbox message carrying it out by
/// itself: `Msg::inspection`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Ordered {
    /// The trip (its departure, minutes of the day), the stop they get on at (from 1), the stops
    /// they ride, how many, their clothes and names.
    pub dep: i32,
    pub board: u32,
    pub stops: u32,
    pub n: u32,
    pub variant: String,
    pub names: Vec<String>,
}

/// The answer to the driver of tour `tour` of line `line` asking for inspectors now (`dp`: the
/// day's plan): `Ok` with who comes, where and when - or why not (`ASKED_ALREADY`,
/// `NO_INSPECTORS`, `NOBODY_NEAR`).
pub fn answer(c: &Company, dp: &DayPlan, line: &str, tour: &str, now: i32) -> Result<(Ordered, i32), &'static str> {
    let t = dp.tours.iter().find(|t| same(&t.tour.line, line) && same(&t.tour.tour, tour)).ok_or(NOBODY_NEAR)?;
    // the trip under way, else the next
    let trip = t.tour.trips.iter().filter(|x| x.counts()).find(|x| x.arr > now).ok_or(NOBODY_NEAR)?;
    let asked = c.messages.list.iter().any(|m| m.inspection.as_ref().is_some_and(|o| o.dep == trip.dep) && matches!(&m.to, super::messages::To::Duty { line: l, tour: tt } if same(l, line) && same(tt, tour)));
    let refused = c.messages.list.iter().any(|m| m.quick == Some(super::messages::Quick::RequestInspection) && m.args.iter().any(|(k, v)| k == "dep" && v == &trip.dep.to_string()));
    if asked || refused {
        return Err(ASKED_ALREADY);
    }
    let patrols: Vec<_> = super::staff::patrols(c, &dp.date).into_iter().filter(|p| p.stop.is_none() && p.from <= now && now < p.to).collect();
    if patrols.is_empty() {
        return Err(NO_INSPECTORS);
    }
    let mut rng = Rng::of(&[&c.id, &dp.date, "ordered", line, tour], now as i64);
    // (a team on the line: near enough mostly; one on a line with a terminus of the trip: half
    // the time)
    let ends = |l: &str| dp.tours.iter().filter(|x| same(&x.tour.line, l)).flat_map(|x| x.tour.trips.iter()).any(|x| [&x.from, &x.to].iter().any(|s| same(s, &trip.from) || same(s, &trip.to)));
    let team = patrols.iter().find(|p| same(&p.line, line) && rng.chance(0.85)).or_else(|| patrols.iter().find(|p| !same(&p.line, line) && ends(&p.line) && rng.chance(0.5))).ok_or(NOBODY_NEAR)?;
    let n = stops_of(trip);
    let done = ((now - trip.dep).max(0) as f64 / (trip.arr - trip.dep).max(1) as f64 * (n - 1) as f64).floor() as u32 + 1;
    let board = done + AHEAD;
    if board + 1 > n {
        return Err(NOBODY_NEAR);
    }
    let at = now + (AHEAD as f64 * STOP_MINUTES).round() as i32;
    let ride = (rng.int(STRETCH.0, STRETCH.1) as u32).min(n - board).max(1);
    let pi = super::staff::patrols(c, &dp.date).iter().position(|p| p.line == team.line && p.ids == team.ids).unwrap_or(0);
    Ok((Ordered { dep: trip.dep, board, stops: ride, n: team.ids.len().min(4) as u32, variant: variant(c, c.inspection_plan.look.letter(pi as u64)), names: names_of(c, &team.ids) }, at))
}

/// What the inspectors did on the player's trip, as the game reported it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct GameCheck {
    pub line: String,
    /// The trip's departure (minutes of the day) and the date.
    pub dep: i32,
    pub date: String,
    pub checked: u32,
    pub caught: u32,
    pub fled: u32,
    pub stops: u32,
    pub board_stop: String,
    pub leave_stop: String,
    pub names: Vec<String>,
    pub ordered: bool,
    /// Who drove (the player's name).
    pub driver: String,
}

/// The most a report may say (a report of a game is checked as strictly as a phone's order).
pub const MOST_CHECKED: u32 = 200;

/// Keep what the inspectors did on the player's trip for the day's close (`inspection::close`):
/// a check of that trip, by the game. Refused: a report of another day, or out of bounds.
pub fn book_game(c: &mut Company, g: &GameCheck) -> Result<(), &'static str> {
    if g.checked > MOST_CHECKED || g.caught > g.checked || g.stops > 40 || g.line.trim().is_empty() {
        return Err("That inspection report does not add up.");
    }
    if super::dates::parse(&g.date).is_none() || super::dates::between(&g.date, &c.date) > 1 || super::dates::between(&c.date, &g.date) > 0 {
        return Err("That inspection report is not of today.");
    }
    let number = c.lines.iter().find(|l| same(&l.name, &g.line) || same(&l.number, &g.line)).map(|l| (l.name.clone(), l.number.clone()));
    let (line, number) = number.unwrap_or((g.line.clone(), g.line.clone()));
    let fine = super::inspection::fine(c);
    let check = Check {
        line,
        number,
        dep: g.dep,
        // (about two minutes a stop: when they got off)
        arr: g.dep + (g.stops as i32 * 2).max(1),
        from: g.board_stop.clone(),
        to: g.leave_stop.clone(),
        team: Vec::new(),
        names: g.names.clone(),
        checked: g.checked,
        caught: g.caught,
        fine,
        duty: 0,
        stop: String::new(),
        at: g.dep,
        off: g.dep + (g.stops as i32 * 2).max(1),
        board: 0,
        alight: 0,
        stops: g.stops,
        game: if g.driver.trim().is_empty() { "-".into() } else { g.driver.trim().chars().take(40).collect() },
    };
    let day = g.date.clone();
    c.inspection.game.retain(|(d, k)| !(d == &day && k.line == check.line && k.dep == check.dep && k.game == check.game));
    c.inspection.game.push((day, check));
    // (a few days kept: the close of each takes its own)
    let keep = super::dates::add(&c.date, -3);
    c.inspection.game.retain(|(d, _)| super::dates::between(&keep, d) >= 0);
    Ok(())
}

/// The game's checks of `date` (the day's close takes them with the modelled ones; the modelled
/// trips they were on are not checked by the model as well).
pub fn of_day(c: &Company, date: &str) -> Vec<Check> {
    c.inspection.game.iter().filter(|(d, _)| d == date).map(|(_, k)| k.clone()).collect()
}

/// The share of the dodgers aboard who fled as inspectors came in (the game's draw: the model's
/// `inspection::ESCAPE`).
pub fn escape() -> f64 {
    ESCAPE
}

#[cfg(test)]
#[path = "inspection_game_tests.rs"]
mod tests;
