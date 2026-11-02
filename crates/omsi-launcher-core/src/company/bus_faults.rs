//! (NexiconDK, Bus Company Online, 2 November 2026) What is wrong with a company's bus, from its
//! drives in the server's world: a fault of its own (its scripts') that stayed on while its engine
//! ran - a warning on its dashboard - and the defects its driver reported from the game menu. Each
//! goes to the workshop as a defect (`crew::report_defect`), and the bus keeps its scripts' faults
//! (`Vehicle::fault_state`) from drive to drive while one of its dashboard is open: the next driver
//! sees the warnings, and is told of its open defects, until the workshop puts it right.

use super::crew::{self, DefectKind};
use super::model::Company;
use crate::TripRun;

/// What a fault of the dashboard's defect says before its parts.
pub const DASHBOARD: &str = "Dashboard";

/// A drive's account of bus `vehicle` by player `by` (a trip of it, or a free drive's whole):
/// its faults and the driver's reports to the workshop, its scripts' faults kept with the bus.
/// On the server only.
pub fn take(c: &mut Company, vehicle: u32, run: &TripRun, by: &str) {
    if !c.online.on_server || vehicle == 0 || c.vehicle(vehicle).is_none() {
        return;
    }
    if !run.faults.is_empty() {
        let mut parts: Vec<&str> = run.faults.iter().map(|f| f.trim()).filter(|f| !f.is_empty()).collect();
        parts.dedup();
        let text = format!("{DASHBOARD}: {}", parts.join(", "));
        let _ = crew::report_defect(c, vehicle, DefectKind::Other, &text, by, true);
    }
    for key in &run.reported {
        if let Some(kind) = DefectKind::from_key(key.trim()) {
            let _ = crew::report_defect(c, vehicle, kind, "Reported by the driver", by, false);
        }
    }
    // (the scripts' faults as the drive left them: kept while one of the dashboard is open)
    if !run.fault_state.is_empty() || !run.faults.is_empty() {
        if let Some(v) = c.fleet.iter_mut().find(|v| v.id == vehicle) {
            v.fault_state = run.fault_state.iter().filter(|(n, x)| !n.trim().is_empty() && x.is_finite()).take(200).cloned().collect();
        }
    }
}

/// Bus `vehicle` as the next drive starts it: its scripts' faults (none once the workshop put its
/// dashboard's right), and its defects not repaired yet, as the game tells them.
pub fn at_start(c: &Company, vehicle: u32) -> (Vec<(String, f32)>, Vec<String>) {
    let Some(v) = c.vehicle(vehicle) else { return (Vec::new(), Vec::new()) };
    let open: Vec<&crew::Defect> = crew::defects_of(c, vehicle).into_iter().filter(|d| d.open(c)).collect();
    let dashboard = open.iter().any(|d| d.fault && d.text.starts_with(DASHBOARD));
    let texts = open.iter().map(|d| if d.text.trim().is_empty() || d.text == "Reported by the driver" { d.kind.label().to_string() } else { d.text.clone() }).collect();
    (if dashboard { v.fault_state.clone() } else { Vec::new() }, texts)
}
