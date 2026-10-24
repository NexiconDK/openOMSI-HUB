//! Bus Company Online on a dedicated server (phase 1 of docs/omsihub/BUS_COMPANY_ONLINE.md):
//! every bus company of the server's company folder (`company_dir` of `server.cfg`, with
//! `companies = 1`) lives there and runs for good, on the server's clock, in the one world the
//! players share as in a LAN session.
//!
//! The server's clock is the owner's choice (`online::server_clock`: the real date and time with
//! `real_time = 1`, else `date` and `time` the first time), and from then on 1:1 with the real
//! clock, going on where it was after a restart; the world runs on it, and every company's course
//! is pinned to it.
//!
//! The companies' hosts (`omsi_launcher_lib::company::online::Host`) run on a thread of their own
//! - a midnight costs a company a second or more, which on the world's step would stall
//! everybody's world: every `STEP` each tells its company what the world reported of its buses,
//! moves its time on, closes its days and writes the day's plan for the world; a company founded
//! on the server meanwhile is taken on (`SCAN`); they are saved as they go and when the server
//! stops (`stop`). The world meets them through the files, as the game and the launcher do on a
//! player's computer: the plans the timetable takes in (`ServerCompanies`, again whenever a host
//! wrote one anew: `take_new_plan`), and the trips the companies' buses drove
//! (`store::append_live`).

use omsi_launcher_lib::company::online::{self, Host, ServerClock};
use omsi_launcher_lib::company::store;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How often the hosts step (real time).
const STEP: Duration = Duration::from_secs(2);
/// How often the folder is looked through for companies founded since (real time).
const SCAN: Duration = Duration::from_secs(30);

/// The server's companies, as the world sees them.
struct Running {
    /// The server's company folder.
    data: PathBuf,
    clock: ServerClock,
    /// One more each time a host wrote its plan or a company came on: the world takes the
    /// plans in again.
    plans: AtomicU64,
    stop: AtomicBool,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

static RUNNING: OnceLock<Arc<Running>> = OnceLock::new();
/// The `plans` the world took in last.
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

/// A map's folder name from a map path (`maps/Ahlheim/global.cfg`: `ahlheim`).
fn map_folder(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches("/global.cfg").trim_end_matches('/').rsplit('/').next().unwrap_or("").to_lowercase()
}

/// The ids of the companies in the server's folder (`<id>.json` of its `companies`; a plan
/// beside one, `<id>.liveplan.json`, is none).
fn company_ids(data: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(store::dir(data)) else { return Vec::new() };
    let mut ids: Vec<String> = rd
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.strip_suffix(".json").filter(|id| !id.contains('.') && !id.is_empty()).map(str::to_string)
        })
        .collect();
    ids.sort();
    ids
}

/// The server's clock (`online::server_clock` in the folder `data`; `follow`: on the real time,
/// `start` the date and second it starts on) and the companies of the folder opened on it - caught
/// up with the time the server was off, their days planned for the world, saved - and their hosts
/// started on their thread. `map_dir`: the map's folder as the server has it (a company of
/// another map stays in the folder, not in this world). The date and time (`HH:MM:SS`) the world
/// starts at: the server's clock's.
pub(crate) fn start(data: &Path, map_dir: &Path, follow: bool, start: (String, i64)) -> anyhow::Result<(String, String)> {
    anyhow::ensure!(RUNNING.get().is_none(), "the companies run already");
    let real = unix_now() as i64;
    let clock = online::server_clock(data, real, follow, start)?;
    let mut seen = HashSet::new();
    let hosts = open_new(data, map_dir, clock, &mut seen);
    let (date, secs) = clock.at_real(unix_now());
    log::info!("server: {} bus companies on the server's clock, {date} {} ({})", hosts.len(), hhmmss(secs), data.display());
    TAKEN.store(0, Ordering::Relaxed);
    let r = Arc::new(Running { data: data.to_path_buf(), clock, plans: AtomicU64::new(0), stop: AtomicBool::new(false), thread: Mutex::new(None) });
    let shared = r.clone();
    let map_dir = map_dir.to_path_buf();
    let thread = std::thread::Builder::new().name("company hosts".into()).spawn(move || run(hosts, seen, &map_dir, &shared))?;
    *r.thread.lock().unwrap_or_else(|e| e.into_inner()) = Some(thread);
    let _ = RUNNING.set(r);
    Ok((date, hhmmss(secs)))
}

/// The companies of the folder not seen yet, opened on the server's clock (those of another map,
/// and those that would not open, are seen and left).
fn open_new(data: &Path, map_dir: &Path, clock: ServerClock, seen: &mut HashSet<String>) -> Vec<Host> {
    let here = map_dir.file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut out = Vec::new();
    for id in company_ids(data) {
        if !seen.insert(id.clone()) {
            continue;
        }
        // (a company of another map is not opened: its time would run on this map's timetable)
        match store::load(data, &id) {
            Ok(c) if map_folder(&c.map) != here => {
                log::warn!("company {id}: on {}, not this server's map - left in the folder", c.map);
                continue;
            }
            Ok(_) => {}
            Err(e) => {
                log::warn!("company {id}: not opened: {e:#}");
                continue;
            }
        }
        let real = unix_now() as i64;
        match Host::open(data, &id, real, Some(map_dir), Some(clock)) {
            Ok((h, away)) => {
                if let Some(a) = away.as_ref() {
                    let result: i64 = a.reports.iter().map(|r| r.result).sum();
                    log::info!("company {id}: caught up the time the server was off - {} days closed, a result of {:.2}", a.reports.len(), result as f64 / 100.0);
                }
                log::info!("company {id} ({}, {}): on the server, {} lines, {} buses, {} staff", h.company.name, h.company.game_mode.label(), h.company.lines.len(), h.company.fleet.len(), h.company.staff.len());
                out.push(h);
            }
            Err(e) => log::warn!("company {id}: not opened: {e:#}"),
        }
    }
    out
}

/// The hosts' thread: a step of each every `STEP`, the folder looked through every `SCAN`,
/// until the server stops; then each saved.
fn run(mut hosts: Vec<Host>, mut seen: HashSet<String>, map_dir: &Path, r: &Running) {
    let mut next = Instant::now() + STEP;
    let mut scan = Instant::now() + SCAN;
    let mut errors: Vec<String> = vec![String::new(); hosts.len()];
    while !r.stop.load(Ordering::Relaxed) {
        if Instant::now() < next {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        next = Instant::now() + STEP;
        if Instant::now() >= scan {
            scan = Instant::now() + SCAN;
            let new = open_new(&r.data, map_dir, r.clock, &mut seen);
            if !new.is_empty() {
                errors.resize(hosts.len() + new.len(), String::new());
                hosts.extend(new);
                r.plans.fetch_add(1, Ordering::Relaxed);
            }
        }
        for (h, last_error) in hosts.iter_mut().zip(errors.iter_mut()) {
            match h.tick(unix_now() as i64) {
                Ok(s) => {
                    last_error.clear();
                    for d in &s.closed {
                        log::info!("company {}: the day {} closed - {} of {} tours run, {} km, a result of {:.2}", h.company.id, d.date, d.covered, d.tours, d.km.round(), d.result as f64 / 100.0);
                    }
                    if s.planned {
                        log::info!("company {}: the plan of {} written for the world", h.company.id, h.company.date);
                        r.plans.fetch_add(1, Ordering::Relaxed);
                    }
                }
                Err(e) => {
                    // (said once, not every step)
                    let text = format!("{e:#}");
                    if text != *last_error {
                        log::warn!("company {}: {text}", h.company.id);
                        *last_error = text;
                    }
                }
            }
        }
    }
    let real = unix_now() as i64;
    for h in hosts.iter_mut() {
        match h.save(real) {
            Ok(()) => log::info!("company {}: saved ({} {})", h.company.id, h.company.date, hhmmss(h.company.clock.minute as f64 * 60.0)),
            Err(e) => log::warn!("company {}: not saved: {e:#}", h.company.id),
        }
    }
}

/// The server's company folder, when its companies run (`companies = 1`).
pub(crate) fn data_dir() -> Option<PathBuf> {
    RUNNING.get().map(|r| r.data.clone())
}

/// The server's clock now, when its companies run: the date and the second of the day the world
/// runs on.
pub(crate) fn clock() -> Option<(String, f64)> {
    Some(RUNNING.get()?.clock.at_real(unix_now()))
}

/// Whether a host wrote its company's plan anew, or a company came on, since the world took the
/// plans in last (then the world takes them in now).
pub(crate) fn take_new_plan() -> bool {
    let Some(r) = RUNNING.get() else { return false };
    let g = r.plans.load(Ordering::Relaxed);
    TAKEN.swap(g, Ordering::Relaxed) != g
}

/// The server stops: the hosts step no more, and every company is saved.
pub(crate) fn stop() {
    let Some(r) = RUNNING.get() else { return };
    r.stop.store(true, Ordering::Relaxed);
    let thread = r.thread.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(t) = thread {
        let _ = t.join();
    }
}

/// `--found-company <name>` with `--server server.cfg`: a new company founded in the server's
/// company folder, on its map and its clock (companies are founded on the server, never brought
/// along), in the game mode `mode` (`casual`, `normal`, `realistic`; default normal). Its id.
pub(crate) fn found(data: &Path, map: &str, name: &str, mode: &str, follow: bool, start: (String, i64)) -> anyhow::Result<String> {
    use omsi_launcher_lib::company::{model::GameMode, Founding};
    let game_mode = match mode.trim().to_ascii_lowercase().as_str() {
        "casual" | "easy" => GameMode::Casual,
        "" | "normal" => GameMode::Normal,
        "realistic" | "hard" => GameMode::Realistic,
        other => anyhow::bail!("'{other}' is no game mode (casual, normal, realistic)"),
    };
    anyhow::ensure!(!name.trim().is_empty(), "the company needs a name");
    let real = unix_now() as i64;
    let clock = online::server_clock(data, real, follow, start)?;
    let f = Founding { name: name.trim().to_string(), map: map.to_string(), game_mode, ..Default::default() };
    online::found_on_server(data, &f, "owner", clock, real)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_folder_the_time_of_day_and_the_companies_of_the_folder() {
        assert_eq!(map_folder("maps/Ahlheim/global.cfg"), "ahlheim");
        assert_eq!(map_folder("maps\\Berlin-Spandau\\global.cfg"), "berlin-spandau");
        assert_eq!(map_folder("maps/Grundorf"), "grundorf");
        assert_eq!(hhmmss(6.0 * 3600.0 + 10.0 * 60.0 + 12.7), "06:10:12");
        let data = std::env::temp_dir().join(format!("openomsi-company-server-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let a = found(&data, "maps/Grundorf/global.cfg", "Alpha Bus", "casual", false, ("2024-03-04".into(), 6 * 3600)).unwrap();
        let b = found(&data, "maps/Grundorf/global.cfg", "Beta", "", false, ("2030-01-01".into(), 0)).unwrap();
        assert!(found(&data, "maps/Grundorf/global.cfg", "Gamma", "sandbox", false, ("2024-03-04".into(), 0)).is_err());
        // (the plans beside them are no companies)
        omsi_launcher_lib::company::plan::save_live_plan(&data, &omsi_launcher_lib::company::plan::LivePlan { company: a.clone(), ..Default::default() }).unwrap();
        assert_eq!(company_ids(&data), vec![a.clone(), b.clone()]);
        let c = store::load(&data, &b).unwrap();
        assert_eq!((c.date.as_str(), c.game_mode), ("2024-03-04", omsi_launcher_lib::company::model::GameMode::Normal), "on the server's clock, set by the first");
        assert_eq!(store::load(&data, &a).unwrap().game_mode, omsi_launcher_lib::company::model::GameMode::Casual);
        let _ = std::fs::remove_dir_all(&data);
    }
}
