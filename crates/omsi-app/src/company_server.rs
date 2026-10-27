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

/// (NexiconDK, 27 October 2026) The world runs - its map loaded, its timetable and companies taken
/// in: said in its folder beside its server.cfg (`cfg`), for the main server to let players in
/// (`main_server::WORLD_RUNNING`; its gateway answered minutes before).
pub(crate) fn mark_running(cfg: &Path) {
    if COMPANIES.get().is_none() {
        return;
    }
    let Some(dir) = cfg.parent() else { return };
    let path = dir.join(omsi_launcher_lib::company::main_server::WORLD_RUNNING);
    match std::fs::write(&path, std::process::id().to_string()) {
        Ok(()) => log::info!("server: running, said to the main server ({})", path.display()),
        Err(e) => log::warn!("server: running, but not said to the main server ({}): {e}", path.display()),
    }
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

// --- phase 3: the trips the players drive in the world ------------------------------------------

/// Where the world saw a player's bus: when (Unix seconds), where (the map's metres) and what he
/// drove (`Pose::tour`: `omsi_sim::timetable_run::lan_claims`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Seen {
    pub at: f64,
    pub x: f64,
    pub y: f64,
    pub tour: String,
}

/// How long the world keeps where it saw a player's bus (s): a long tour's trip and its posting.
const TRACK_FOR: f64 = 4.0 * 3600.0;
/// A trip's end may be this far off the world's clock (its game's machine's own) (s).
const SKEW: f64 = 600.0;

/// The players' buses as the world saw them, by their names (lower case).
static TRACKS: Mutex<Option<std::collections::HashMap<String, std::collections::VecDeque<Seen>>>> = Mutex::new(None);
/// The trips taken (the player, the tour, the departure in minutes): each once.
static TAKEN_TRIPS: Mutex<Vec<(String, String, i64)>> = Mutex::new(Vec::new());
/// The world's gateway takes trips (`POST /bco/trip`).
static DOOR_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Where the world sees the players' buses now (their names, places, tours), kept for the
/// trips they post (`take_trips`); at `now` (Unix seconds).
pub(crate) fn watch(players: impl Iterator<Item = (String, f64, f64, String)>, now: f64) {
    let mut g = TRACKS.lock().unwrap_or_else(|e| e.into_inner());
    let tracks = g.get_or_insert_with(Default::default);
    for (name, x, y, tour) in players {
        let t = tracks.entry(name.trim().to_lowercase()).or_default();
        t.push_back(Seen { at: now, x, y, tour });
        while t.front().is_some_and(|s| now - s.at > TRACK_FOR) {
            t.pop_front();
        }
    }
    tracks.retain(|_, t| t.back().is_some_and(|s| now - s.at <= TRACK_FOR));
}

/// Why a posted trip is not taken (the world's log says it).
pub(crate) fn check(track: &std::collections::VecDeque<Seen>, t: &omsi_launcher_lib::TripRun) -> Result<(), &'static str> {
    if t.free || t.tour.trim().is_empty() {
        return Err("not a trip of a tour");
    }
    let end = t.time as f64;
    let from = end - t.seconds.max(0.0) - SKEW;
    // (what his game says it drives: a tour, or of tours the trips of his duty - `LanClaim`)
    let on_tour = |s: &&Seen| s.at >= from && s.at <= end + SKEW && omsi_sim::timetable_run::lan_claims(&s.tour).iter().any(|c| c.tour.eq_ignore_ascii_case(t.tour.trim()));
    let seen: Vec<&Seen> = track.iter().filter(on_tour).collect();
    if seen.len() < 3 {
        return Err("the world did not see his bus on that tour then");
    }
    let driven: f64 = seen.windows(2).map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt()).filter(|d| *d < 2000.0).sum();
    if t.metres > 300.0 && driven < 0.5 * t.metres {
        return Err("his bus drove far less than the trip says");
    }
    Ok(())
}

/// The trips the players' launchers posted to the world's gateway since: each checked against
/// where the world saw the player's bus (`check`), taken once, and written for the company whose
/// line it is on the map `map` (`store::append_world_trip`: its process books it).
pub(crate) fn take_trips(map: &str) {
    let Some(c) = COMPANIES.get() else { return };
    if !DOOR_OPEN.load(Ordering::Relaxed) && crate::lan::open_bco_trips() {
        DOOR_OPEN.store(true, Ordering::Relaxed);
        log::info!("server: the players' trips are taken (POST /bco/trip)");
    }
    let posted = crate::lan::take_bco_trips();
    if posted.is_empty() {
        return;
    }
    let plans = omsi_launcher_lib::company::plan::live_plans(&c.data);
    let same_map = |m: &str| online::map_folder(m) == online::map_folder(map);
    for (source, body) in posted {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) else {
            log::warn!("server: a trip from {source} that is not one");
            continue;
        };
        let name = v.get("name").and_then(serde_json::Value::as_str).unwrap_or("").trim().to_string();
        let Some(t) = v.get("trip").and_then(|x| serde_json::from_value::<omsi_launcher_lib::TripRun>(x.clone()).ok()) else {
            log::warn!("server: a trip of {name} from {source} that is not one");
            continue;
        };
        let key = (name.to_lowercase(), t.tour.trim().to_string(), (t.departure / 60.0).round() as i64);
        if TAKEN_TRIPS.lock().unwrap_or_else(|e| e.into_inner()).contains(&key) {
            continue;
        }
        let verdict = {
            let g = TRACKS.lock().unwrap_or_else(|e| e.into_inner());
            match g.as_ref().and_then(|t| t.get(&name.to_lowercase())) {
                Some(track) => check(track, &t),
                None => Err("the world never saw him"),
            }
        };
        if let Err(why) = verdict {
            log::warn!("server: the trip of {name} on line {} tour {} not taken: {why}", t.line, t.tour);
            continue;
        }
        // (the company of the tour it is a trip of - a line it runs today, its timetable name or
        // the number the bus showed: the company's name for it from here on)
        let Some((company, line)) = plans.iter().filter(|p| same_map(&p.map)).find_map(|p| p.trip_line(&t.line, &t.tour).map(|l| (p.company.clone(), l))) else {
            log::warn!("server: the trip of {name} on line {} tour {}: no company of the map runs that tour", t.line, t.tour);
            continue;
        };
        let t = omsi_launcher_lib::TripRun { line, ..t };
        let w = omsi_launcher_lib::company::store::WorldTrip { name: name.clone(), trip: t.clone(), at: unix_now() as u64, kept: false, handed: false };
        match omsi_launcher_lib::company::store::append_world_trip(&c.data, &company, &w) {
            Ok(()) => {
                let mut taken = TAKEN_TRIPS.lock().unwrap_or_else(|e| e.into_inner());
                taken.push(key);
                if taken.len() > 5000 {
                    taken.drain(..1000);
                }
                log::info!("server: the trip of {name} on line {} tour {} taken for {company}", t.line, t.tour);
            }
            Err(e) => log::warn!("server: the trip of {name} not written for {company}: {e:#}"),
        }
    }
}

/// (NexiconDK, relief at the stop) Player `name` was not at his stop in time to take the bus of
/// line `line` tour `tour` over at `departure` (seconds of the day): the AI drove that trip of his
/// duty - written for the company whose line it is on the map `map` (`WorldTrip::kept`), whose
/// process books it as the AI's (`book::ai_kept`).
pub(crate) fn relief_kept(map: &str, name: &str, line: &str, tour: &str, departure: f64) {
    let Some(c) = COMPANIES.get() else { return };
    let plans = omsi_launcher_lib::company::plan::live_plans(&c.data);
    let same_map = |m: &str| online::map_folder(m) == online::map_folder(map);
    let Some((company, line)) = plans.iter().filter(|p| same_map(&p.map)).find_map(|p| p.trip_line(line, tour).map(|l| (p.company.clone(), l))) else {
        return;
    };
    let trip = omsi_launcher_lib::TripRun { line, tour: tour.trim().to_string(), departure, time: unix_now() as u64, ..Default::default() };
    let w = omsi_launcher_lib::company::store::WorldTrip { name: name.to_string(), trip, at: unix_now() as u64, kept: true, handed: false };
    match omsi_launcher_lib::company::store::append_world_trip(&c.data, &company, &w) {
        Ok(()) => log::info!("server: {name} was not at his stop in time: the AI drove line {} tour {tour} at {:.0} for {company}", w.trip.line, departure),
        Err(e) => log::warn!("server: the relief the AI drove not written for {company}: {e:#}"),
    }
}

/// (28 October 2026) Player `name` handed his bus over to the AI on the way through his duty, on
/// his trip of line `line` tour `tour` leaving at `departure` (seconds of the company's day) - at
/// its terminus (`done`), else at a stop along it: written for the company whose line it is on the
/// map `map` (`WorldTrip::handed`), whose process books the rest of his duty as the AI's
/// (`book::handed_over`) - he did not leave it, and is free for another.
pub(crate) fn handed_over(map: &str, name: &str, line: &str, tour: &str, departure: f64, done: bool) {
    let Some(c) = COMPANIES.get() else { return };
    let plans = omsi_launcher_lib::company::plan::live_plans(&c.data);
    let same_map = |m: &str| online::map_folder(m) == online::map_folder(map);
    let Some((company, line)) = plans.iter().filter(|p| same_map(&p.map)).find_map(|p| p.trip_line(line, tour).map(|l| (p.company.clone(), l))) else {
        return;
    };
    let trip = omsi_launcher_lib::TripRun { line, tour: tour.trim().to_string(), departure, completed: done, time: unix_now() as u64, ..Default::default() };
    let w = omsi_launcher_lib::company::store::WorldTrip { name: name.to_string(), trip, at: unix_now() as u64, kept: false, handed: true };
    match omsi_launcher_lib::company::store::append_world_trip(&c.data, &company, &w) {
        Ok(()) => log::info!("server: {name} handed his bus over to the AI on line {} tour {tour} at {:.0}: written for {company}", w.trip.line, departure),
        Err(e) => log::warn!("server: the hand-over of {name} not written for {company}: {e:#}"),
    }
}

// --- phase 5: each depot's control room --------------------------------------------------------

/// When the world last wrote what it sees for the control rooms.
static LIVE_AT: Mutex<Option<std::time::Instant>> = Mutex::new(None);
/// The world said once that it could not write it.
static LIVE_FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Every couple of seconds (`world_live::EVERY`): the companies' timetable buses on the world's
/// roads, and the players' buses on their tours (`players`, the poses the world has of them),
/// written for the companies' control rooms (`world_live`, `live/<map>.json` of the folder).
pub(crate) fn publish_live<'a>(map: &str, schedule: Option<&crate::schedule::Schedule>, traffic: Option<&crate::traffic::Traffic>, humans: Option<&crate::humans::Humans>, players: impl Iterator<Item = &'a omsi_net::Pose>) {
    use omsi_launcher_lib::company::world_live::{self, WorldBus, WorldLive, WorldPlayer};
    let Some(c) = COMPANIES.get() else { return };
    let (Some(s), Some(t)) = (schedule, traffic) else { return };
    {
        let mut at = LIVE_AT.lock().unwrap_or_else(|e| e.into_inner());
        if at.is_some_and(|t| t.elapsed().as_secs_f64() < world_live::EVERY) {
            return;
        }
        *at = Some(std::time::Instant::now());
    }
    let aboard = humans.map(|h| h.ai_aboard()).unwrap_or_default();
    let buses = s
        .sim
        .live_buses(&t.sim, &aboard)
        .into_iter()
        .map(|b| WorldBus { company: b.company, line: b.line, tour: b.tour, trip: b.trip, dep: b.dep, x: b.x, y: b.y, heading: b.heading, speed_kmh: b.speed_kmh, delay: b.delay, passengers: b.passengers, at_stop: b.at_stop, layover: b.layover, next: b.next, terminus: b.terminus, number: b.number })
        .collect();
    let players = players
        .filter_map(|p| {
            let tours: Vec<(String, String)> = omsi_sim::timetable_run::lan_claims(&p.tour).into_iter().map(|c| (c.line, c.tour)).collect();
            let company = tours.iter().find_map(|(line, _)| s.sim.line_owner(line))?;
            Some(WorldPlayer { name: p.name.trim().to_string(), company, tours, x: p.x, y: p.y, heading: (p.heading as f64).rem_euclid(360.0), speed_kmh: p.speed_kmh as f64, passengers: p.passengers, line: p.line.trim().to_string() })
        })
        .collect();
    let w = WorldLive { updated: unix_now(), map: map.to_string(), buses, players };
    match world_live::write(&c.data, &w) {
        Ok(()) => LIVE_FAILED.store(false, Ordering::Relaxed),
        Err(e) => {
            if !LIVE_FAILED.swap(true, Ordering::Relaxed) {
                log::warn!("server: the buses on the roads not written for the control rooms: {e:#}");
            }
        }
    }
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

    /// A player's trip is taken when the world saw his bus on its tour then, driving about as
    /// far as it says - not one it never saw, on another tour, or standing still.
    #[test]
    fn a_trip_is_taken_as_the_world_saw_it() {
        let t = omsi_launcher_lib::TripRun { time: 10_000, tour: "3".into(), line: "Linie5".into(), seconds: 900.0, metres: 6000.0, departure: 8.0 * 3600.0, ..Default::default() };
        // (its bus seen every 2 s on its tour, 7 m a second)
        let seen = |tour: &str, speed: f64| -> std::collections::VecDeque<Seen> { (0..=450).map(|k| Seen { at: 9_100.0 + 2.0 * k as f64, x: 2.0 * k as f64 * speed, y: 0.0, tour: tour.into() }).collect() };
        assert_eq!(check(&seen("Linie5/3", 7.0), &t), Ok(()));
        assert_eq!(check(&seen("linie5/3@480~540;linie7/2@560~600", 7.0), &t), Ok(()), "his duty's trips of that tour");
        assert!(check(&seen("Linie5/4", 7.0), &t).is_err(), "another tour");
        assert!(check(&seen("Linie5/3", 0.5), &t).is_err(), "hardly moved");
        assert!(check(&std::collections::VecDeque::new(), &t).is_err(), "never seen");
        // (seen hours before: not then)
        let mut early = seen("Linie5/3", 7.0);
        early.iter_mut().for_each(|s| s.at -= 5.0 * 3600.0);
        assert!(check(&early, &t).is_err());
        assert!(check(&seen("Linie5/3", 7.0), &omsi_launcher_lib::TripRun { free: true, ..t.clone() }).is_err(), "a free drive is no trip of a tour");
        // (the watch keeps hours, by name)
        watch([("Jonas".to_string(), 1.0, 2.0, "Linie5/3".to_string())].into_iter(), 100.0);
        watch([("jonas ".to_string(), 3.0, 4.0, "Linie5/3".to_string())].into_iter(), 102.0);
        assert_eq!(TRACKS.lock().unwrap().as_ref().unwrap().get("jonas").map(|t| t.len()), Some(2));
        watch(std::iter::empty(), 102.0 + TRACK_FOR + 1.0);
        assert!(TRACKS.lock().unwrap().as_ref().unwrap().get("jonas").is_none(), "gone after hours");
    }
}
