//! (NexiconDK) Bus Company Online's fines on a line of a contract won in the map's market
//! (decided 25 October 2026, docs/omsihub/BUS_COMPANY_ONLINE.md): what the authority fines the
//! company for a journey not run (by the minute of its time), a tour run with a bus smaller than
//! its timetable asks for, a diesel bus on a tour of a contract won with electric buses (not the
//! other way round), and each complaint about the driving, the temperature in the bus, the route
//! driven, a ticket sold wrong and a red light run.
//!
//! The amounts are Danish kroner, in the main server's `company-server.cfg` (`FINES_CFG`), and
//! each company pays them in its own currency (`Fines::cents`).

use super::currency::Currency;
use super::model::Cents;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// The fines, in kroner.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Fines {
    /// A journey not run (or not all of it): each minute of its timetabled time.
    pub dropped_minute: f64,
    /// A journey run with a bus smaller than its tour asks for.
    pub wrong_bus: f64,
    /// A journey of a contract won with electric buses run with a diesel bus.
    pub diesel_on_electric: f64,
    /// A complaint: the driving, the temperature in the bus, the route driven, a ticket sold
    /// wrong, a red light run.
    pub driving: f64,
    pub temperature: f64,
    pub route: f64,
    pub ticket: f64,
    pub red_light: f64,
}

impl Default for Fines {
    fn default() -> Self {
        Fines { dropped_minute: 20.83, wrong_bus: 2500.0, diesel_on_electric: 2500.0, driving: 2000.0, temperature: 1500.0, route: 2500.0, ticket: 2500.0, red_light: 2500.0 }
    }
}

impl Fines {
    /// Kroner in euro cents (the books' money).
    pub fn cents(kr: f64) -> Cents {
        Currency::Dkk.cents(kr.max(0.0))
    }

    /// The fines of the settings' keys (`fine_…`), the defaults where one is missing or not a
    /// number (a decimal comma read as a point).
    pub fn of(kv: &HashMap<String, String>) -> Fines {
        let d = Fines::default();
        let f = |k: &str, d: f64| kv.get(k).and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok()).filter(|x| x.is_finite() && *x >= 0.0).unwrap_or(d);
        Fines {
            dropped_minute: f("fine_dropped_minute", d.dropped_minute),
            wrong_bus: f("fine_wrong_bus", d.wrong_bus),
            diesel_on_electric: f("fine_diesel_on_electric", d.diesel_on_electric),
            driving: f("fine_driving", d.driving),
            temperature: f("fine_temperature", d.temperature),
            route: f("fine_route", d.route),
            ticket: f("fine_ticket", d.ticket),
            red_light: f("fine_red_light", d.red_light),
        }
    }
}

/// The fines of the main server's folder `data` (its `company-server.cfg`, read as it is: the
/// defaults when it has none).
pub fn of_folder(data: &std::path::Path) -> Fines {
    let text = std::fs::read_to_string(data.join(super::main_server::CFG_FILE)).unwrap_or_default();
    let kv: HashMap<String, String> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect();
    Fines::of(&kv)
}

/// The fines' part of the settings, added to a `company-server.cfg` written before it.
pub const FINES_CFG: &str = "
# the authority's fines on a line of a contract won in the map's tenders (Danish kroner; each
# company pays them in its own currency): a journey not run, by the minute of its time; a tour
# run with a smaller bus than its timetable asks for, and a diesel bus on a tour of a contract
# won with electric buses, by the journey; and each complaint about the driving, the
# temperature in the bus, the route driven, a ticket sold wrong and a red light run
fine_dropped_minute = 20.83
fine_wrong_bus = 2500
fine_diesel_on_electric = 2500
fine_driving = 2000
fine_temperature = 1500
fine_route = 2500
fine_ticket = 2500
fine_red_light = 2500
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fines_are_read_in_kroner_with_a_decimal_comma_too() {
        let mut kv = HashMap::new();
        kv.insert("fine_dropped_minute".to_string(), "20,83".to_string());
        kv.insert("fine_driving".to_string(), "x".to_string());
        kv.insert("fine_route".to_string(), "3000".to_string());
        let f = Fines::of(&kv);
        assert_eq!((f.dropped_minute, f.driving, f.route, f.red_light), (20.83, 2000.0, 3000.0, 2500.0));
        // (2 500 kr in the books' euro cents, and back)
        let c = Fines::cents(2500.0);
        assert!((Currency::Dkk.units(c as f64) - 2500.0).abs() < 0.5, "{c}");
    }
}
