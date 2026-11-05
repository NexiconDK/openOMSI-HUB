//! (NexiconDK, RosterFlow Pro round I, 5 November 2026) The bus dispatch ("Busdisponering"): which
//! bus of the fleet runs which block (vognløb) on a date.
//!
//! * **One truth.** The day plan (`plan::day_plan`) takes the dispatch's choice for a block on its
//!   date first - a bus (`BusPick::bus`), or none at all ("Fjern") - and the weekly roster's bus
//!   (`RosterTour::bus`) for the blocks the dispatch did not touch; the same checks either way (in
//!   the fleet, not in the workshop, one of the line's buses, free then). So the world drives the
//!   bus chosen here, the pages show it, the day's close books it. Choices are kept a week after
//!   their date (`KEEP_DAYS`).
//! * **Who can take a block** (`check`): still the company's; not in the workshop that day, not
//!   broken down this morning, fit to run (condition 20 or more); one of the line's buses
//!   (`ownline::line_allows`); as big as the tour asks for (`ownline::wanted`); what the line's
//!   contract asks of its buses (`bus_contracts::on_line`: zero emission, fuel, floor, age); free
//!   then, with the planning's ten minutes between two blocks (`staff::BUS_MARGIN`); and enough in
//!   the tank or the battery for the block's kilometres with a tenth to spare (`RANGE_MARGIN`).
//!   The level: full from the depot in the morning (`fuel`), less the bus's blocks before it that
//!   day at its fuel use (`economy::consumption`); an electric bus charges at the depot in a gap of
//!   half an hour or more (`CHARGE_GAP`) at 150 kW (`CHARGE_KW`). Warnings, not refusals: a service
//!   due within 500 km after the block (`SERVICE_SOON_KM`), dirty inside (under 40, `DIRTY_BELOW`),
//!   three breakdowns or more (`MANY_BREAKDOWNS`), a bigger bus than the tour asks for.
//! * **"Fordel automatisk"** (`auto`): every block without a bus (`redo`: every block, the hand's
//!   choices too) gets the best bus that can take it, the blocks in the order they begin: one
//!   already on the line that day first, the size asked for, an electric bus for a short block
//!   (at most 60 % of its range) and a diesel one for a long one, no service due, the fewest
//!   kilometres that day so far.
//! * **A bus that falls out** (`downs_of`, `tell`): one that did not start this morning (`plan::
//!   disruptions`) or is in the workshop (a repair, its service - the day's close sends it) leaves
//!   its blocks without a bus - or the central put another on one that came suddenly - and the
//!   player is told once a day for each bus, with "Find bus ›" (`BusDown`, like a sick call:
//!   `pro_sick`).

use super::dates;
use super::economy;
use super::fuel;
use super::model::{BusSize, Company, Vehicle};
use super::network::TourOfDay;
use super::plan::{BusOf, DayPlan, Problem, Source};
use super::staff::BUS_MARGIN;
use serde::{Deserialize, Serialize};

/// A choice is kept this many days after its date.
pub const KEEP_DAYS: i64 = 7;
/// A service due within this many kilometres after the block: a warning.
pub const SERVICE_SOON_KM: f64 = 500.0;
/// Dirtier inside than this (0 to 100): a warning.
pub const DIRTY_BELOW: f64 = 40.0;
/// This many breakdowns or more: a warning.
pub const MANY_BREAKDOWNS: u32 = 3;
/// A gap between two blocks of an electric bus this long (minutes) charges it at the depot.
pub const CHARGE_GAP: i32 = 30;
/// The depot's chargers (kW).
pub const CHARGE_KW: f64 = 150.0;
/// A block wants this share more than its kilometres in the tank.
pub const RANGE_MARGIN: f64 = 0.10;
/// "Fordel automatisk" gives an electric bus a block of at most this share of its range.
pub const SHORT_SHARE: f64 = 0.6;

/// The dispatch's choice for block `line`/`tour` on `date`: a bus, or none (`bus` None: no bus
/// at all, the block does not run); `hand` the planner's own (not "Fordel automatisk"'s).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BusPick {
    pub date: String,
    pub line: String,
    pub tour: String,
    #[serde(default)]
    pub bus: Option<u32>,
    #[serde(default)]
    pub hand: bool,
}

/// Why a bus fell out of its day.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownWhy {
    /// It did not start this morning.
    NoStart,
    /// In the workshop (a repair, its service).
    Workshop,
}

/// A bus that fell out of `date`, told the player: its first block without it, and the bus the
/// central put on it (None: none - the block is missing a bus).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BusDown {
    pub date: String,
    pub bus: u32,
    pub line: String,
    pub tour: String,
    /// Its first block's label ("5/01").
    #[serde(default)]
    pub label: String,
    pub why: DownWhy,
    #[serde(default)]
    pub covered: Option<u32>,
    /// The blocks of the day it leaves.
    #[serde(default)]
    pub blocks: u32,
    pub at: i64,
    #[serde(default)]
    pub seen: bool,
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// "5/01": the line's number and the tour (one digit made two).
pub fn label(t: &TourOfDay) -> String {
    let tour = t.tour.trim();
    let tour = if tour.len() == 1 && tour.chars().all(|c| c.is_ascii_digit()) { format!("0{tour}") } else { tour.to_string() };
    format!("{}/{}", t.number.trim(), tour)
}

/// The dispatch's choice for a block on a date (None: the roster's bus).
pub fn pick_of<'a>(c: &'a Company, date: &str, line: &str, tour: &str) -> Option<&'a BusPick> {
    c.planning.ops.buses.iter().find(|p| p.date == date && same(&p.line, line) && same(&p.tour, tour))
}

/// The dispatch chooses bus `bus` (None: no bus at all) for a block on a date; the choices of
/// long gone dates go.
pub fn set(c: &mut Company, date: &str, line: &str, tour: &str, bus: Option<u32>, hand: bool) {
    prune(c);
    c.planning.ops.buses.retain(|p| !(p.date == date && same(&p.line, line) && same(&p.tour, tour)));
    c.planning.ops.buses.push(BusPick { date: date.to_string(), line: line.to_string(), tour: tour.to_string(), bus, hand });
}

/// The block back to the roster's bus.
pub fn clear(c: &mut Company, date: &str, line: &str, tour: &str) {
    c.planning.ops.buses.retain(|p| !(p.date == date && same(&p.line, line) && same(&p.tour, tour)));
}

/// The choices of the dates more than `KEEP_DAYS` gone dropped, and the falls told.
pub fn prune(c: &mut Company) {
    let first = dates::add(&c.date, -KEEP_DAYS);
    c.planning.ops.buses.retain(|p| dates::between(&first, &p.date) >= 0);
    c.planning.ops.downs.retain(|p| dates::between(&first, &p.date) >= 0);
}

/// The kilometres of a block's trips.
pub fn km_of(t: &TourOfDay) -> f64 {
    t.trips.iter().map(|x| x.km.max(0.0)).sum()
}

/// The company's bus a block runs with in a plan.
pub fn bus_of(t: &super::plan::DayTour) -> Option<u32> {
    match t.bus {
        Some(BusOf::Own(id)) => Some(id),
        _ => None,
    }
}

/// How far a bus gets on `level` (0 to 1) of its tank, the depot's reserve kept (km).
pub fn range_km(v: &Vehicle, level: f64) -> f64 {
    let per_km = economy::consumption(v.kind) / 100.0;
    if per_km <= 0.0 {
        return f64::INFINITY;
    }
    ((level - fuel::RESERVE).max(0.0) * fuel::tank(v.kind)) / per_km
}

/// A block of a bus's day as the level is worked out: from, to, kilometres.
pub type Run = (i32, i32, f64);

/// What is in bus `v`'s tank (0 to 1) when a block beginning at `from` leaves, the bus's other
/// blocks that day `others` driven before it: full from the depot (today, as last known since
/// then), less what each used, an electric bus charged in the gaps (`CHARGE_GAP`, `CHARGE_KW`).
pub fn level_before(v: &Vehicle, today: bool, others: &[Run], from: i32) -> f64 {
    let (mut level, since) = match (today, v.fuel_at) {
        (true, Some(m)) => (v.fuel.clamp(0.0, 1.0), m),
        _ => (1.0, i32::MIN),
    };
    let tank = fuel::tank(v.kind).max(1.0);
    let per_km = economy::consumption(v.kind) / 100.0;
    let electric = v.kind.drive == super::model::Drive::Electric;
    let mut before: Vec<Run> = others.iter().copied().filter(|r| r.1 <= from && r.0 >= since).collect();
    before.sort_by_key(|r| r.0);
    let mut at = before.first().map(|r| r.0).unwrap_or(from);
    for r in &before {
        if electric && r.0 - at >= CHARGE_GAP {
            level = (level + (r.0 - at) as f64 / 60.0 * CHARGE_KW / tank).min(1.0);
        }
        level = (level - r.2 * per_km / tank).max(0.0);
        at = r.1;
    }
    if electric && from - at >= CHARGE_GAP && !before.is_empty() {
        level = (level + (from - at) as f64 / 60.0 * CHARGE_KW / tank).min(1.0);
    }
    level
}

/// The gaps an electric bus charges in between the blocks `runs` (from, to); none for another.
pub fn charging(v: &Vehicle, runs: &[Run]) -> Vec<(i32, i32)> {
    if v.kind.drive != super::model::Drive::Electric {
        return Vec::new();
    }
    let mut r: Vec<Run> = runs.to_vec();
    r.sort_by_key(|x| x.0);
    r.windows(2).filter(|w| w[1].0 - w[0].1 >= CHARGE_GAP).map(|w| (w[0].1 + BUS_MARGIN, w[1].0 - BUS_MARGIN)).collect()
}

/// Why a bus cannot take a block.
#[derive(Clone, Debug, PartialEq)]
pub enum Cannot {
    Gone,
    Workshop { until: String },
    NoStart,
    Unfit,
    NotLineBus,
    Small { wants: BusSize },
    ZeroEmission,
    Fuel,
    Floor,
    Age,
    /// On block `with` (a label) then.
    Busy { with: String },
    /// Its tank or battery takes it `has` km of the `needs` km.
    Range { needs: f64, has: f64 },
}

/// What a bus that can take a block warns of.
#[derive(Clone, Debug, PartialEq)]
pub enum Warn {
    ServiceSoon { km_left: f64 },
    Dirty,
    Breakdowns { n: u32 },
    Bigger,
}

/// Whether bus `v` can take block `t` of `date` beside its other blocks that day `others`
/// (label, from, to, km), and what it warns of.
pub fn fit(c: &Company, date: &str, today: bool, broken: &[u32], t: &TourOfDay, v: &Vehicle, others: &[(String, Run)]) -> Result<Vec<Warn>, Cannot> {
    if !v.held_on(date) {
        return Err(Cannot::Gone);
    }
    if v.in_workshop(date) {
        return Err(Cannot::Workshop { until: v.workshop_until.clone().unwrap_or_default() });
    }
    if today && broken.contains(&v.id) {
        return Err(Cannot::NoStart);
    }
    if v.condition < 20.0 {
        return Err(Cannot::Unfit);
    }
    if !super::ownline::line_allows(c, &t.line, v) {
        return Err(Cannot::NotLineBus);
    }
    let wants = super::ownline::wanted(c, t);
    let size = super::plan::size_fit(wants, v.kind.size);
    if size == 0 {
        return Err(Cannot::Small { wants: wants.unwrap_or_default() });
    }
    if let Some(k) = super::bus_contracts::on_line(c, &t.line, date) {
        if k.electric && !v.kind.drive.zero_emission() {
            return Err(Cannot::ZeroEmission);
        }
        if let Some(n) = k.needs {
            if !n.fuel.allows(v.kind.drive) {
                return Err(Cannot::Fuel);
            }
            if !n.floor.allows(super::server_buses::floor_of(c, v)) {
                return Err(Cannot::Floor);
            }
            if !n.age_ok(c, v) {
                return Err(Cannot::Age);
            }
        }
    }
    let (from, to) = (t.from(), t.to());
    if let Some((with, _)) = others.iter().find(|(_, r)| !(to + BUS_MARGIN <= r.0 || r.1 + BUS_MARGIN <= from)) {
        return Err(Cannot::Busy { with: with.clone() });
    }
    let runs: Vec<Run> = others.iter().map(|x| x.1).collect();
    let km = km_of(t);
    let has = range_km(v, level_before(v, today, &runs, from));
    let needs = km * (1.0 + RANGE_MARGIN);
    if has < needs {
        return Err(Cannot::Range { needs: km, has });
    }
    let mut warns = Vec::new();
    let driven: f64 = runs.iter().filter(|r| r.1 <= from).map(|r| r.2).sum();
    let left = v.next_service_km - (v.km + driven + km);
    if left < SERVICE_SOON_KM {
        warns.push(Warn::ServiceSoon { km_left: left.max(0.0) });
    }
    if v.clean < DIRTY_BELOW {
        warns.push(Warn::Dirty);
    }
    if v.breakdowns >= MANY_BREAKDOWNS {
        warns.push(Warn::Breakdowns { n: v.breakdowns });
    }
    if size == 1 {
        warns.push(Warn::Bigger);
    }
    Ok(warns)
}

/// The buses that did not start this morning (today's plan only).
pub fn broken(dp: &DayPlan) -> Vec<u32> {
    dp.disruptions.iter().filter_map(|d| if let super::plan::Disruption::Breakdown { vehicle, .. } = d { Some(*vehicle) } else { None }).collect()
}

/// Bus `id`'s blocks in plan `dp`, block `skip` left out: (label, from, to, km).
pub fn runs_of(dp: &DayPlan, id: u32, skip: Option<usize>) -> Vec<(String, Run)> {
    dp.tours.iter().enumerate().filter(|(i, t)| Some(*i) != skip && bus_of(t) == Some(id)).map(|(_, t)| (label(&t.tour), (t.tour.from(), t.tour.to(), km_of(&t.tour)))).collect()
}

/// Whether bus `v` can take block `ti` of plan `dp` (its other blocks in the plan as they are).
pub fn check(c: &Company, dp: &DayPlan, ti: usize, v: &Vehicle) -> Result<Vec<Warn>, Cannot> {
    let t = &dp.tours[ti];
    fit(c, &dp.date, dp.today, &broken(dp), &t.tour, v, &runs_of(dp, v.id, Some(ti)))
}

/// The blocks of a plan a dispatch can give a bus: the company's, not a player's own.
fn dispatchable(t: &super::plan::DayTour) -> bool {
    !t.by_player && !t.tour.unplanned
}

/// "Fordel automatisk": every block of plan `dp` without a bus (`redo`: every block, the
/// planner's own choices too) given the best bus that can take it, in the order the blocks
/// begin. How many got one.
pub fn auto(c: &mut Company, dp: &DayPlan, redo: bool) -> usize {
    let date = dp.date.clone();
    let gone = broken(dp);
    let moves: Vec<usize> = (0..dp.tours.len())
        .filter(|&i| dispatchable(&dp.tours[i]))
        .filter(|&i| {
            let t = &dp.tours[i];
            redo || (t.bus.is_none() && !pick_of(c, &date, &t.tour.line, &t.tour.tour).is_some_and(|p| p.hand && p.bus.is_none()))
        })
        .collect();
    // (what the buses run already: the blocks not moved)
    let mut busy: Vec<(u32, Vec<(String, Run)>)> = Vec::new();
    for (i, t) in dp.tours.iter().enumerate() {
        if moves.contains(&i) {
            continue;
        }
        if let Some(id) = bus_of(t) {
            let r = (label(&t.tour), (t.tour.from(), t.tour.to(), km_of(&t.tour)));
            match busy.iter_mut().find(|b| b.0 == id) {
                Some(b) => b.1.push(r),
                None => busy.push((id, vec![r])),
            }
        }
    }
    let mut order = moves;
    order.sort_by_key(|&i| dp.tours[i].tour.from());
    let mut given = Vec::new();
    for i in order {
        let t = &dp.tours[i].tour;
        let km = km_of(t);
        let wants = super::ownline::wanted(c, t);
        let mut best: Option<((bool, u8, bool, bool, i64, std::cmp::Reverse<u32>), u32)> = None;
        for v in &c.fleet {
            let others: Vec<(String, Run)> = busy.iter().find(|b| b.0 == v.id).map(|b| b.1.clone()).unwrap_or_default();
            let Ok(warns) = fit(c, &date, dp.today, &gone, t, v, &others) else { continue };
            let on_line = dp.tours.iter().any(|x| same(&x.tour.line, &t.line) && bus_of(x) == Some(v.id)) || given.iter().any(|(j, id): &(usize, u32)| *id == v.id && same(&dp.tours[*j].tour.line, &t.line));
            let electric = v.kind.drive == super::model::Drive::Electric;
            let runs: Vec<Run> = others.iter().map(|x| x.1).collect();
            let short = km <= SHORT_SHARE * range_km(v, level_before(v, dp.today, &runs, t.from()));
            let suits = if electric { short } else { !short || km >= 150.0 };
            let service = !warns.iter().any(|w| matches!(w, Warn::ServiceSoon { .. }));
            let driven: f64 = runs.iter().map(|r| r.2).sum();
            let score = (on_line, super::plan::size_fit(wants, v.kind.size), suits, service, -(driven.round() as i64), std::cmp::Reverse(v.id));
            if best.as_ref().is_none_or(|b| score > b.0) {
                best = Some((score, v.id));
            }
        }
        if let Some((_, id)) = best {
            let r = (label(t), (t.from(), t.to(), km));
            match busy.iter_mut().find(|b| b.0 == id) {
                Some(b) => b.1.push(r),
                None => busy.push((id, vec![r])),
            }
            given.push((i, id));
        }
    }
    for (i, id) in &given {
        let t = &dp.tours[*i].tour;
        set(c, &date, &t.line, &t.tour, Some(*id), false);
    }
    given.len()
}

/// Two blocks of plan `dp` swap their buses (either may have none).
pub fn swap(c: &mut Company, dp: &DayPlan, a: usize, b: usize) {
    let (x, y) = (bus_of(&dp.tours[a]), bus_of(&dp.tours[b]));
    let (ta, tb) = (dp.tours[a].tour.clone(), dp.tours[b].tour.clone());
    set(c, &dp.date, &ta.line, &ta.tour, y, true);
    set(c, &dp.date, &tb.line, &tb.tour, x, true);
}

/// The bus a block of plan `dp` was meant to run with: the dispatch's, else the roster's.
fn meant(c: &Company, dp: &DayPlan, t: &super::plan::DayTour) -> Option<u32> {
    match pick_of(c, &dp.date, &t.tour.line, &t.tour.tour) {
        Some(p) => p.bus,
        None => super::plan::roster(c, dp.weekday, &t.tour.line, &t.tour.tour).and_then(|r| r.bus),
    }
}

/// The buses that fell out of plan `dp`'s day - did not start, in the workshop - each with its
/// first block left, the bus the central put on it (None: none), and how many blocks it leaves.
pub fn downs_of(c: &Company, dp: &DayPlan) -> Vec<(u32, usize, DownWhy, Option<u32>, u32)> {
    let mut out: Vec<(u32, usize, DownWhy, Option<u32>, u32)> = Vec::new();
    let mut order: Vec<usize> = (0..dp.tours.len()).filter(|&i| dispatchable(&dp.tours[i])).collect();
    order.sort_by_key(|&i| dp.tours[i].tour.from());
    for i in order {
        let t = &dp.tours[i];
        let why = match t.bus_problem {
            Some(Problem::Breakdown) => DownWhy::NoStart,
            Some(Problem::Workshop) => DownWhy::Workshop,
            _ => continue,
        };
        let Some(id) = meant(c, dp, t) else { continue };
        let covered = (t.bus_from == Source::Central).then(|| bus_of(t)).flatten();
        match out.iter_mut().find(|x| x.0 == id) {
            Some(x) => x.4 += 1,
            None => out.push((id, i, why, covered, 1)),
        }
    }
    out
}

/// Bus `bus` fell out of the company's day: told once a day (whether it was told now).
pub fn tell(c: &mut Company, bus: u32, line: &str, tour: &str, label: &str, why: DownWhy, covered: Option<u32>, blocks: u32, at: i64) -> bool {
    let date = c.date.clone();
    if c.planning.ops.downs.iter().any(|x| x.date == date && x.bus == bus) {
        return false;
    }
    prune(c);
    c.planning.ops.downs.push(BusDown { date, bus, line: line.to_string(), tour: tour.to_string(), label: label.to_string(), why, covered, blocks, at, seen: false });
    true
}

/// The texts of a fall (English: keys of the translations).
pub const DOWN_NO_START: &str = "Bus %{bus} did not start this morning – block %{block} is missing a bus";
pub const DOWN_WORKSHOP: &str = "Bus %{bus} is in the workshop – block %{block} is missing a bus";
pub const DOWN_COVERED: &str = "Bus %{bus} did not start this morning – the central put bus %{other} on block %{block}";

/// The feed's words for a fall: its template and places.
pub fn words(c: &Company, x: &BusDown) -> (&'static str, Vec<(String, String)>) {
    let number = |id: u32| c.vehicle(id).map(|v| v.number.clone()).unwrap_or_else(|| id.to_string());
    let mut args = vec![("bus".to_string(), number(x.bus)), ("block".to_string(), x.label.clone())];
    match (x.why, x.covered) {
        (DownWhy::NoStart, Some(o)) => {
            args.push(("other".into(), number(o)));
            (DOWN_COVERED, args)
        }
        (DownWhy::NoStart, None) => (DOWN_NO_START, args),
        (DownWhy::Workshop, _) => (DOWN_WORKSHOP, args),
    }
}

/// The first fall of the company's day not seen yet.
pub fn unseen(c: &Company) -> Option<&BusDown> {
    c.planning.ops.downs.iter().find(|x| !x.seen && x.date == c.date)
}

/// Bus `bus`'s fall today seen (every one: None).
pub fn seen(c: &mut Company, bus: Option<u32>) {
    let date = c.date.clone();
    for x in c.planning.ops.downs.iter_mut().filter(|x| x.date == date && bus.is_none_or(|b| b == x.bus)) {
        x.seen = true;
    }
}

#[cfg(test)]
#[path = "pro_buses_tests.rs"]
mod tests;
