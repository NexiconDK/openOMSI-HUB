//! (NexiconDK, Bus Company Online, 2 November 2026) What is in a bus's tank (an electric bus's
//! battery) during the day, as the company keeps it.
//!
//! The depot fills every bus at night (`day::close_day_with`). A player's game measures what it
//! uses and tells at each trip's end (`together::book::report`: `Vehicle::fuel`, at the trip's
//! arrival `Vehicle::fuel_at`). The world's AI buses burn nothing in the game - their scripts run
//! without a tank - so what the company's AI drivers use is worked out: the kilometres of the
//! day's trips they drive the bus on since its level was known, at the fuel use of its kind
//! (`economy::consumption`, what the books pay for), of a full tank of its kind (`tank`). A bus a
//! player takes over from the AI in the afternoon may be well down; never below the reserve.
//! On the server alone: elsewhere the level is the one the company last knew.

use super::economy;
use super::model::{BusKind, BusSize, Company, Drive};
use super::network::TourOfDay;
use super::together::book::ClaimState;

/// The least a bus is left with: the reserve a depot keeps it to (a share of a full tank).
pub const RESERVE: f64 = 0.05;

/// What a full tank (battery) of a bus of `kind` holds, in what `economy::consumption` counts:
/// litres of diesel, kWh. About what the makers fit: a 12 m Citaro or Urbino 250-300 l, an
/// electric one 300-500 kWh.
pub fn tank(kind: BusKind) -> f64 {
    match (kind.size, kind.drive) {
        (BusSize::Midi, Drive::Diesel) => 200.0,
        (BusSize::Solo, Drive::Diesel) => 250.0,
        (BusSize::Articulated, Drive::Diesel) => 300.0,
        (BusSize::Double, Drive::Diesel) => 330.0,
        (BusSize::Midi, Drive::Electric) => 220.0,
        (BusSize::Solo, Drive::Electric) => 400.0,
        (BusSize::Articulated, Drive::Electric) => 550.0,
        (BusSize::Double, Drive::Electric) => 470.0,
    }
}

/// A bus's fuel use as a page says it: "29 l / 100 km", "120 kWh / 100 km".
pub fn use_text(kind: BusKind) -> String {
    let unit = if kind.drive == Drive::Electric { "kWh" } else { "l" };
    format!("{:.0} {unit} / 100 km", economy::consumption(kind))
}

/// A tour of the day as the level is worked out of it: the fleet's bus it runs with, the tour,
/// and its duties' trips (indices into its trips, one range a duty).
pub struct BusDay<'a> {
    pub bus: Option<u32>,
    pub tour: &'a TourOfDay,
    pub duties: Vec<(usize, usize)>,
}

/// The tours of a day's plan as the level is worked out of them.
pub fn of_day_plan(p: &super::plan::DayPlan) -> Vec<BusDay<'_>> {
    p.tours
        .iter()
        .map(|t| BusDay {
            bus: match t.bus {
                Some(super::plan::BusOf::Own(id)) => Some(id),
                _ => None,
            },
            tour: &t.tour,
            duties: t.duties.iter().map(|d| (d.start, d.end)).collect(),
        })
        .collect()
}

/// `of_day_plan` of the plan the pages keep (`day::Plan`).
pub fn of_plan(p: &super::day::Plan) -> Vec<BusDay<'_>> {
    p.tours.iter().map(|t| BusDay { bus: t.bus, tour: &t.tour, duties: t.duties.iter().map(|d| (d.start, d.end)).collect() }).collect()
}

/// Whether a player's game drives (or drove) trip `j` of `day`'s tour today - his, not the AI's.
fn players(c: &Company, day: &BusDay, j: usize) -> bool {
    let Some(k) = day.duties.iter().position(|&(a, b)| j >= a && j < b) else { return false };
    let i = j - day.duties[k].0;
    c.together.claims.iter().any(|x| {
        x.duty.date == c.date
            && x.duty.duty == k
            && x.duty.line.eq_ignore_ascii_case(&day.tour.line)
            && x.duty.tour.trim() == day.tour.tour.trim()
            && matches!(x.state, ClaimState::Driving | ClaimState::Done)
            && x.his(i)
            && !x.ai_drives(i)
    })
}

/// The kilometres the company's AI drivers drive bus `id` today from minute `from` to `to`: the
/// trips of the tours it runs with, a trip under way by the share of its time gone.
pub fn ai_km(c: &Company, days: &[BusDay], id: u32, from: i32, to: i32) -> f64 {
    if id == 0 || to <= from {
        return 0.0;
    }
    let mut km = 0.0;
    for day in days.iter().filter(|d| d.bus == Some(id)) {
        for (j, t) in day.tour.trips.iter().enumerate() {
            let (a, b) = (t.dep.max(from), t.arr.min(to));
            if b <= a || players(c, day, j) {
                continue;
            }
            let share = if t.arr > t.dep { (b - a) as f64 / (t.arr - t.dep) as f64 } else { 1.0 };
            km += t.km.max(0.0) * share;
        }
    }
    km
}

/// What is in bus `id`'s tank at minute `minute` of the company's day, 0 to 1: as the company
/// last knew it, less what its AI drivers used since - on the server; elsewhere as it knew it.
pub fn level_at(c: &Company, days: &[BusDay], id: u32, minute: i32) -> f64 {
    let Some(v) = c.vehicle(id) else { return 1.0 };
    let known = v.fuel.clamp(0.0, 1.0);
    if !c.online.on_server {
        return known;
    }
    let used = ai_km(c, days, id, v.fuel_at.unwrap_or(i32::MIN), minute) * economy::consumption(v.kind) / 100.0 / tank(v.kind);
    if used <= 0.0 {
        return known;
    }
    (known - used).max(RESERVE.min(known))
}

/// Bus `id`'s odometer at minute `minute` of the company's day: the fleet's reading, with what
/// players drove it today and - on the server - what its AI drivers have driven it so far (the
/// day's close books them).
pub fn odometer_at(c: &Company, days: &[BusDay], id: u32, minute: i32) -> f64 {
    let Some(v) = c.vehicle(id) else { return 0.0 };
    let ai = if c.online.on_server { ai_km(c, days, id, i32::MIN, minute) } else { 0.0 };
    v.km + super::together::book::km_today(c, id) + ai
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_tank_lasts_as_far_as_a_bus_of_its_kind_goes() {
        let km = |size, drive| tank(BusKind { size, drive }) / economy::consumption(BusKind { size, drive }) * 100.0;
        // a diesel solo bus some 850 km, an electric one some 330: a long day's service
        assert!((800.0..900.0).contains(&km(BusSize::Solo, Drive::Diesel)), "{}", km(BusSize::Solo, Drive::Diesel));
        assert!((300.0..360.0).contains(&km(BusSize::Solo, Drive::Electric)), "{}", km(BusSize::Solo, Drive::Electric));
        assert_eq!(use_text(BusKind { size: BusSize::Articulated, drive: Drive::Diesel }), "40 l / 100 km");
        assert_eq!(use_text(BusKind { size: BusSize::Solo, drive: Drive::Electric }), "120 kWh / 100 km");
    }
}
