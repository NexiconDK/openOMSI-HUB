//! Bus Company Online on a dedicated server (phase 1 of docs/omsihub/BUS_COMPANY_ONLINE.md):
//! the bus company `server.cfg` names (`company`) lives in the server's company folder
//! (`company_dir`) and runs there for good, with the real clock.
//!
//! The company's host (`omsi_launcher_lib::company::online::Host`) runs on a thread of its own
//! - a midnight costs it a second or more, which on the world's step would stall everybody's
//! world: every `STEP` it tells the company what the world reported of its buses, moves its
//! time on, closes its days and writes the day's plan for the world; it is saved as it goes
//! and when the server stops (`stop`). The world meets it through the files, as the game and
//! the launcher do on a player's computer: the plan the timetable takes in (`CompanyLive::
//! for_server`, again whenever the host wrote it anew: `take_new_plan`), and the trips the
//! company's buses drove (`store::append_live`).
//!
//! The world's date and clock are the company's (`start`, `clock`): its time runs 1:1 with
//! the real clock, so the server's clock follows it as a server on the real time follows this
//! machine's.

use omsi_launcher_lib::company::online::{self, Host};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How often the host steps (real time).
const STEP: Duration = Duration::from_secs(2);

/// The company the server runs, as the world sees it.
struct Running {
    /// The server's company folder and the company's id.
    data: PathBuf,
    id: String,
    /// The host's course (`Host::course`): where the company's time is at any moment.
    course: Mutex<(i64, i64)>,
    /// The host's `plan_gen`: one more each time the plan is written.
    plan_gen: AtomicU64,
    stop: AtomicBool,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

static RUNNING: OnceLock<Arc<Running>> = OnceLock::new();
/// The `plan_gen` the world took in last.
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

/// The company `id` of the server's folder `data` opened (caught up with the time it was
/// away, its day planned for the world, saved) and its host started on its thread. `map_dir`:
/// the map's folder as the server has it - it must be the company's map. The date and time
/// (`HH:MM:SS`) the world starts at: the company's.
pub(crate) fn start(data: &Path, id: &str, map_dir: &Path) -> anyhow::Result<(String, String)> {
    anyhow::ensure!(RUNNING.get().is_none(), "a company runs already");
    let real = unix_now() as i64;
    let (host, away) = Host::open(data, id, real, Some(map_dir))?;
    let here = map_dir.file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
    if map_folder(&host.company.map) != here {
        anyhow::bail!("the company {id} runs on {}, the server's map is {}: set map in server.cfg to the company's", host.company.map, map_dir.display());
    }
    if let Some(a) = away.as_ref() {
        let result: i64 = a.reports.iter().map(|r| r.result).sum();
        log::info!("company {id}: caught up the time it was away - {} days closed, a result of {:.2}", a.reports.len(), result as f64 / 100.0);
    }
    let course = host.course(real);
    let (date, secs) = online::clock_at(course, unix_now());
    log::info!("company {id} ({}, {}): on the server at {date} {}", host.company.name, host.company.game_mode.label(), hhmmss(secs));
    TAKEN.store(host.plan_gen, Ordering::Relaxed);
    let r = Arc::new(Running {
        data: data.to_path_buf(),
        id: id.to_string(),
        course: Mutex::new(course),
        plan_gen: AtomicU64::new(host.plan_gen),
        stop: AtomicBool::new(false),
        thread: Mutex::new(None),
    });
    let shared = r.clone();
    let thread = std::thread::Builder::new().name("company host".into()).spawn(move || run(host, &shared))?;
    *r.thread.lock().unwrap_or_else(|e| e.into_inner()) = Some(thread);
    let _ = RUNNING.set(r);
    Ok((date, hhmmss(secs)))
}

/// The host's thread: a step every `STEP`, until the server stops; then saved.
fn run(mut host: Host, r: &Running) {
    let mut next = Instant::now() + STEP;
    let mut last_error = String::new();
    while !r.stop.load(Ordering::Relaxed) {
        if Instant::now() < next {
            std::thread::sleep(Duration::from_millis(100));
            continue;
        }
        next = Instant::now() + STEP;
        let real = unix_now() as i64;
        match host.tick(real) {
            Ok(s) => {
                last_error.clear();
                for d in &s.closed {
                    log::info!("company {}: the day {} closed - {} of {} tours run, {} km, a result of {:.2}", r.id, d.date, d.covered, d.tours, d.km.round(), d.result as f64 / 100.0);
                }
                if s.planned {
                    log::info!("company {}: the plan of {} written for the world", r.id, host.company.date);
                }
                *r.course.lock().unwrap_or_else(|e| e.into_inner()) = host.course(real);
                r.plan_gen.store(host.plan_gen, Ordering::Relaxed);
            }
            Err(e) => {
                // (said once, not every step)
                let text = format!("{e:#}");
                if text != last_error {
                    log::warn!("company {}: {text}", r.id);
                    last_error = text;
                }
            }
        }
    }
    match host.save(unix_now() as i64) {
        Ok(()) => log::info!("company {}: saved ({} {})", r.id, host.company.date, hhmmss(host.company.clock.minute as f64 * 60.0)),
        Err(e) => log::warn!("company {}: not saved: {e:#}", r.id),
    }
}

/// The server's company folder, when a company runs on this server.
pub(crate) fn data_dir() -> Option<PathBuf> {
    RUNNING.get().map(|r| r.data.clone())
}

/// The company's date and the second of its day now, when a company runs on this server: the
/// world's clock.
pub(crate) fn clock() -> Option<(String, f64)> {
    let r = RUNNING.get()?;
    let course = *r.course.lock().unwrap_or_else(|e| e.into_inner());
    Some(online::clock_at(course, unix_now()))
}

/// Whether the host wrote the company's plan anew since the world took it in last (then the
/// world takes it in now).
pub(crate) fn take_new_plan() -> bool {
    let Some(r) = RUNNING.get() else { return false };
    let g = r.plan_gen.load(Ordering::Relaxed);
    TAKEN.swap(g, Ordering::Relaxed) != g
}

/// The server stops: the host steps no more, and the company is saved.
pub(crate) fn stop() {
    let Some(r) = RUNNING.get() else { return };
    r.stop.store(true, Ordering::Relaxed);
    let thread = r.thread.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let Some(t) = thread {
        let _ = t.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_folder_and_the_time_of_day() {
        assert_eq!(map_folder("maps/Ahlheim/global.cfg"), "ahlheim");
        assert_eq!(map_folder("maps\\Berlin-Spandau\\global.cfg"), "berlin-spandau");
        assert_eq!(map_folder("maps/Grundorf"), "grundorf");
        assert_eq!(hhmmss(6.0 * 3600.0 + 10.0 * 60.0 + 12.7), "06:10:12");
    }
}
