//! Bus Company Online: a map's world (phase 1 of docs/omsihub/BUS_COMPANY_ONLINE.md). The
//! dedicated server with `companies = 1` in its server.cfg runs its map for the players, as a
//! dedicated server does, with every bus company of the main server's folder (`company_dir`) that
//! has the map: each line of theirs with its company's buses in its paint
//! (`company_live::ServerCompanies`), their trips reported into the folder (`store::append_live`),
//! on the server's clock.
//!
//! The companies themselves live in the main server (`omsi_launcher_lib::company::main_server`,
//! which starts the worlds): it keeps the clock (`online::read_server_clock`) and writes the
//! plans, and says so (`online::plans_gen`): the world takes the plans and the clock in again
//! (`take_new_plan`, every second from the server's step).

use omsi_launcher_lib::company::online::{self, ServerClock};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// The main server's folder and clock, as the world has them.
struct Companies {
    data: PathBuf,
    clock: Mutex<ServerClock>,
}

static COMPANIES: OnceLock<Companies> = OnceLock::new();
/// The plans' generation the world took in last.
static TAKEN: AtomicU64 = AtomicU64::new(0);

/// The real time, in Unix seconds with their fraction.
fn unix_now() -> f64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

/// `1234.5` s as `00:20:34`.
fn hhmmss(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// The world on the main server's folder `data`: its clock taken. The date and time
/// (`HH:MM:SS`) the world starts at. An error: the main server has not set its clock there.
pub(crate) fn start(data: &Path) -> anyhow::Result<(String, String)> {
    let clock = online::read_server_clock(data).ok_or_else(|| anyhow::anyhow!("no server's clock in {} - the main server keeps it (omsi-hub-cli --company-server {})", data.display(), data.display()))?;
    TAKEN.store(online::plans_gen(data), Ordering::Relaxed);
    let (date, secs) = clock.at_real(unix_now());
    let _ = COMPANIES.set(Companies { data: data.to_path_buf(), clock: Mutex::new(clock) });
    log::info!("server: the bus companies of {} on the server's clock, {date} {}", data.display(), hhmmss(secs));
    Ok((date, hhmmss(secs)))
}

/// The main server's folder, when the world runs its companies.
pub(crate) fn data_dir() -> Option<PathBuf> {
    COMPANIES.get().map(|c| c.data.clone())
}

/// The server's clock now, when the world runs the companies: the date and the second of the day
/// the world runs on.
pub(crate) fn clock() -> Option<(String, f64)> {
    let c = COMPANIES.get()?;
    let clock = *c.clock.lock().unwrap_or_else(|e| e.into_inner());
    Some(clock.at_real(unix_now()))
}

/// Whether the main server wrote plans anew (a company's day, a company new) since the world took
/// them in last: then the world takes them in now, and the clock with them.
pub(crate) fn take_new_plan() -> bool {
    let Some(c) = COMPANIES.get() else { return false };
    let Some(g) = changed(&c.data, TAKEN.load(Ordering::Relaxed)) else { return false };
    TAKEN.store(g, Ordering::Relaxed);
    if let Some(clock) = online::read_server_clock(&c.data) {
        *c.clock.lock().unwrap_or_else(|e| e.into_inner()) = clock;
    }
    true
}

/// The plans' generation of the folder, when it is not `taken`.
fn changed(data: &Path, taken: u64) -> Option<u64> {
    Some(online::plans_gen(data)).filter(|g| *g != taken)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The world takes the main server's plans in again when it says so, and its clock with them.
    #[test]
    fn the_world_follows_the_main_servers_folder() {
        let data = std::env::temp_dir().join(format!("openomsi-company-world-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        assert!(online::read_server_clock(&data).is_none(), "no clock before the main server's");
        let clock = online::server_clock(&data, 1_800_000_000, false, ("2024-03-04".into(), 6 * 3600)).unwrap();
        assert_eq!(online::read_server_clock(&data), Some(clock));
        assert_eq!(changed(&data, 0), None);
        let g = online::bump_plans(&data).unwrap();
        assert_eq!(changed(&data, 0), Some(g));
        assert_eq!(changed(&data, g), None);
        assert_eq!(hhmmss(6.0 * 3600.0 + 10.0 * 60.0 + 12.7), "06:10:12");
        let _ = std::fs::remove_dir_all(&data);
    }
}
