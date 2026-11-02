//! (NexiconDK, Bus Company Online, 2 November 2026) A free drive with a bus of the company: a
//! player of a company on the server takes one of its buses that is not in service out into the
//! map's world - from its garage - and drives where he likes. With a destination up, people get
//! on: their fares are the company's. A tour of the company's he picks in the game's "Line and
//! tour" he drives as a single trip taken over from the AI (`together::book::claim_driven_trip`),
//! or behind it as an extra bus (its passengers this drive's). The bus's odometer and tank go on
//! from the company's (`fuel`), and come back with what he drove; he is paid his share of the
//! fares (`online::pay_of`) and half a driver's wage for the time his bus was moving.
//!
//! The drive itself (`FreeDrive`) is the company's from when he takes the bus to when his game
//! ends; what it came to (`FreeRun`) comes from the world, which checked it against where it saw
//! his bus (a `TripRun` with `free`, no tour, `vehicle` and `span`), and the day's close books its
//! kilometres, the bus's wear and fuel and the fares.

use super::model::{BookingKind, Cents, Company};
use super::network::PlannedTrip;
use super::plan::{BusOf, DayPlan};
use crate::TripRun;
use serde::{Deserialize, Serialize};

/// How long before a trip of its tour a bus is not taken out any more (minutes).
pub const KEEP_FREE: i32 = 60;
/// The most of a drive's moving time paid (minutes), and the share of a driver's wage an hour.
pub const PAID_MOST: f64 = 180.0;
pub const HOURLY_SHARE: f64 = 0.5;

pub const NOT_ON_SERVER: &str = "A bus of the company is taken out only on Bus Company Online's server.";
pub const NO_SUCH_BUS: &str = "The company has no such bus.";
pub const NOT_FREE: &str = "That bus is not free now: it runs a tour soon, it is in the workshop or another player has it.";
pub const DRIVING: &str = "You drive a duty or another bus now: finish it first.";

/// A bus a player took out: who, which, since when (the company's day and minute; Unix seconds).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct FreeDrive {
    pub guest: u32,
    pub vehicle: u32,
    pub date: String,
    pub from: i32,
    pub since: u64,
    /// His game ended (the bus is free again); its run may still come from the world.
    pub done: bool,
}

/// What a free drive came to, as the world checked it and the company booked it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct FreeRun {
    pub guest: u32,
    pub vehicle: u32,
    pub date: String,
    pub km: f64,
    /// The minutes his bus was moving, and the people who got on outside the trips he took over.
    pub minutes: f64,
    pub passengers: i32,
    pub fares: Cents,
    pub pay: Cents,
    /// When it came (Unix seconds).
    pub time: u64,
    /// The day's close booked it.
    pub closed: bool,
}

/// A bus of the fleet as the player's page lists it: free (until a minute of the day: a trip of
/// its tour; None: the rest of the day), or not, and why.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct FreeBus {
    pub id: u32,
    pub until: Option<i32>,
    /// Why not: on line X (now or soon), in the workshop, out with a player.
    pub busy: Option<String>,
}

/// The trips of the day's tours bus `id` runs.
fn tours_of(p: &DayPlan, id: u32) -> impl Iterator<Item = (&str, &PlannedTrip)> {
    p.tours.iter().filter(move |t| t.bus == Some(BusOf::Own(id))).flat_map(|t| t.tour.trips.iter().map(move |x| (t.tour.number.as_str(), x)))
}

/// The company's buses as a player sees them to take one out at minute `minute`.
pub fn buses(c: &Company, p: &DayPlan, minute: i32) -> Vec<FreeBus> {
    c.fleet
        .iter()
        .filter(|v| v.held_on(&c.date))
        .map(|v| {
            let mut b = FreeBus { id: v.id, ..Default::default() };
            if v.in_workshop(&c.date) {
                b.busy = Some("workshop".into());
                return b;
            }
            if let Some(d) = c.together.free_drives.iter().find(|d| d.vehicle == v.id && !d.done) {
                b.busy = Some(format!("out:{}", c.together.name_of(d.guest)));
                return b;
            }
            // (a trip of its tour under way, or leaving within the hour: on that line)
            if let Some((line, _)) = tours_of(p, v.id).find(|(_, x)| x.arr > minute && x.dep < minute + KEEP_FREE) {
                b.busy = Some(format!("line:{line}"));
                return b;
            }
            b.until = tours_of(p, v.id).filter(|(_, x)| x.dep >= minute + KEEP_FREE).map(|(_, x)| x.dep - KEEP_FREE / 2).min();
            b
        })
        .collect()
}

/// The player's drive now, if he has one out.
pub fn out_of(c: &Company, guest: u32) -> Option<&FreeDrive> {
    c.together.free_drives.iter().find(|d| d.guest == guest && !d.done)
}

/// Player `guest` takes bus `vehicle` out at minute `minute` (Unix `now`).
pub fn take(c: &mut Company, p: &DayPlan, guest: u32, vehicle: u32, minute: i32, now: u64) -> Result<(), &'static str> {
    if !c.online.on_server {
        return Err(NOT_ON_SERVER);
    }
    if c.vehicle(vehicle).is_none() {
        return Err(NO_SUCH_BUS);
    }
    // (the same bus again: his drive goes on)
    if let Some(d) = out_of(c, guest) {
        return if d.vehicle == vehicle { Ok(()) } else { Err(DRIVING) };
    }
    if c.together.claims.iter().any(|x| x.guest == guest && x.state == super::together::book::ClaimState::Driving) {
        return Err(DRIVING);
    }
    if !buses(c, p, minute).iter().any(|b| b.id == vehicle && b.busy.is_none()) {
        return Err(NOT_FREE);
    }
    c.together.free_drives.push(FreeDrive { guest, vehicle, date: c.date.clone(), from: minute, since: now, done: false });
    Ok(())
}

/// His game ended: the bus is free again.
pub fn end(c: &mut Company, guest: u32) -> bool {
    let mut any = false;
    for d in c.together.free_drives.iter_mut().filter(|d| d.guest == guest && !d.done) {
        d.done = true;
        any = true;
    }
    any
}

/// Where the company's garage is for the game: a run out of the depot of its tours (its trip's
/// name: the game puts the bus at the entry point where its route begins).
pub fn garage_trip(p: &DayPlan) -> Option<String> {
    p.tours.iter().filter(|t| matches!(t.bus, Some(BusOf::Own(_)))).filter_map(|t| t.tour.trips.first()).find(|x| x.empty && !x.name.trim().is_empty()).map(|x| x.name.clone())
}

/// The kilometres players drove bus `id` today on free drives (the day's close books them).
pub fn km_today(c: &Company, id: u32) -> f64 {
    c.together.free_runs.iter().filter(|r| r.vehicle == id && r.date == c.date && !r.closed).map(|r| r.km).sum()
}

/// What a player's free drive brings: the fares of `passengers` over `km` at the company's first
/// line's rates (none where the authority takes them).
fn fares_of(c: &Company, passengers: i32, km: f64) -> Cents {
    let Some(l) = c.lines.first() else { return 0 };
    let rates = super::income::Rates::of(c, l, &c.date);
    if rates.to_authority {
        return 0;
    }
    rates.takings(passengers.max(0) as f64, km).total().round() as Cents
}

/// His pay for `minutes` of driving (half a driver's wage an hour, the first `PAID_MOST`).
fn hourly_pay(c: &Company, minutes: f64) -> Cents {
    let lv = super::econ_uk::Level::of(c);
    let hour = super::staff::hourly(super::economy::market_wage(45.0, &lv)) * HOURLY_SHARE;
    (hour * minutes.clamp(0.0, PAID_MOST) / 60.0).round() as Cents
}

/// The world's account of player `guest`'s free drive `run` (`free`, no tour; `vehicle`, `metres`,
/// `seconds` moving, `passengers`, `fuel`): booked once - the bus's tank, the fares to come at the
/// close, his pay (his share of them and the hourly part, booked as wages). His pay; None: no drive
/// of his with that bus today, or booked already.
pub fn book_run(c: &mut Company, guest: u32, run: &TripRun, now: u64) -> Option<Cents> {
    let d = c.together.free_drives.iter().find(|d| d.guest == guest && d.vehicle == run.vehicle && d.date == c.date)?.clone();
    if c.together.free_runs.iter().any(|r| r.guest == guest && r.time == run.time && r.vehicle == run.vehicle) {
        return None;
    }
    let km = (run.metres.max(0.0) / 1000.0).min(run.span.max(run.seconds).max(0.0) / 3600.0 * 90.0);
    let minutes = run.seconds.max(0.0) / 60.0;
    // (those on the trips he took over are booked with those trips)
    let taken: i32 = c.together.trips.iter().filter(|g| g.guest == guest && g.trip.vehicle == d.vehicle && g.trip.time >= d.since).map(|g| g.trip.passengers.max(0)).sum();
    let passengers = (run.passengers.max(0) - taken).clamp(0, 2000);
    let fares = fares_of(c, passengers, km);
    let pay = super::online::pay_of(c, fares) + hourly_pay(c, minutes);
    if let (Some(f), Some(v)) = (run.fuel, c.fleet.iter_mut().find(|v| v.id == d.vehicle)) {
        v.fuel = f.clamp(0.0, 1.0);
        v.fuel_at = Some(c.clock.minute as i32);
    }
    let name = c.together.name_of(guest);
    if pay > 0 {
        c.book(BookingKind::Wages, -pay, format!("{name}: free drive with bus {} (a player of the company)", c.vehicle(d.vehicle).map(|v| v.number.clone()).unwrap_or_default()), false);
    }
    c.together.free_runs.push(FreeRun { guest, vehicle: d.vehicle, date: c.date.clone(), km, minutes, passengers, fares, pay, time: if run.time > 0 { run.time } else { now }, closed: false });
    Some(pay)
}

/// The day's close: the free drives' kilometres on their buses (and their wear), the fuel they
/// burnt and the fares they brought. Their fares, kilometres.
pub fn close(c: &mut Company, date: &str) -> (Cents, f64) {
    let runs: Vec<FreeRun> = c.together.free_runs.iter().filter(|r| r.date == date && !r.closed).cloned().collect();
    let (mut fares, mut km_all) = (0, 0.0);
    let lv = super::econ_uk::Level::of(c);
    for r in &runs {
        let Some(v) = c.fleet.iter_mut().find(|v| v.id == r.vehicle) else { continue };
        v.km += r.km;
        let kind = v.kind;
        let energy = (r.km * super::economy::energy_per_km(kind, &lv)).round() as Cents;
        if energy > 0 {
            c.book(BookingKind::Energy, -energy, "Fuel and power: free drives of the players", true);
        }
        if r.fares > 0 {
            c.book(BookingKind::Fares, r.fares, "Fares: free drives of the players", true);
        }
        fares += r.fares;
        km_all += r.km;
    }
    for r in c.together.free_runs.iter_mut().filter(|r| r.date == date) {
        r.closed = true;
    }
    (fares, km_all)
}

/// After the close of `closed`: the drives and runs of that day and before go (a drive still out
/// stays until his game ends).
pub fn after_close(c: &mut Company, closed: &str) {
    c.together.free_runs.retain(|r| r.date.as_str() > closed);
    c.together.free_drives.retain(|d| d.date.as_str() > closed || !d.done);
}
