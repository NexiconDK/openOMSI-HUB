//! Bus Company Online (NexiconDK, phase 1 of docs/omsihub/BUS_COMPANY_ONLINE.md): the bus
//! companies of every map live on one main server and run there for good, with the real clock -
//! their days, plans and books going on whether anybody is there or not - while a world per map
//! in use (the dedicated server) runs their buses for the players (`main_server`).
//!
//! `Host` is a company as the main server keeps it. Opened, it catches up with the real time it
//! was away (`clock::catch_up`: the days between closed, the dispatcher deciding), and from then
//! on each `tick` moves its time on to where the real clock has it (`clock::advance`, quick: nobody
//! is asked), tells what its map's world reported of its buses as it comes (`store::peek_live`,
//! `clock::tell_live`; the day's close books it), closes its days at their midnights, writes the
//! day's plan for the world (`plan::save_live_plan`: the tours with their buses, paints and
//! numbers - one more `plan_gen` each time) and saves itself now and then and after each close,
//! so that a server started again goes on where it was.
//!
//! Every company runs on the server's clock (`ServerClock`, the owner's choice: the real date and
//! time, or a date of his own, and from then on 1:1 with the real clock, going on where it was
//! after a restart): its course is pinned to it (`Host::open`'s `clock`), and the maps' worlds run
//! on it too (`read_server_clock`). Companies are founded on the server, on a map of their choice
//! for good, at the server's date and minute (`found_on_server`); on a map each line belongs to at
//! most one of them. The worlds take the plans in again whenever the main server says so
//! (`PLANS_FILE`).
//!
//! What the launcher's pages do besides on their tick (a line's change on its day, the dealer's
//! deliveries, the control room's messages) is not run here yet: the phases after this one.

use super::clock::{self, Away, Synced, World};
use serde::{Deserialize, Serialize};
use super::day::DayReport;
use super::model::Company;
use super::{dates, network, plan, store, Founding};
use crate::LineInfo;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// How often the company is saved while nothing closes (real seconds): a server stopped hard
/// loses at most this much of its day (the day's close saves at once).
pub const SAVE_EVERY: i64 = 60;

/// What the company keeps of its life on a server (`Company::online`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Online {
    /// It runs on a server. In Normal and Realistic its roster is its planners' alone: the day's
    /// repair of the roster (`plan::repair_day`: a driver ill, a bus broken) is not run for
    /// them, and what nobody filled does not run. The easy mode's own routine fills the roster
    /// as it does on a player's computer (`autopilot::morning`). The map's lines no company has
    /// run as the map has them.
    #[serde(default)]
    pub on_server: bool,
    /// The player who founded it, by his name: the first player of that name who comes to the
    /// company is its owner (every permission: `together::Role::CoOwner`), and from then on only
    /// he (`owner_id`: his id). Founded from the launcher (phase 2, step 2) the owner's id is the
    /// server's id for the player (`server_players::Player::id`) from the start, his name the
    /// server's for him.
    #[serde(default)]
    pub owner: String,
    #[serde(default)]
    pub owner_id: String,
}

/// What a company keeps on the main server for its players' launchers ("Play together": the
/// company's session, `together::code`): its port, the code's secret and the session's key, so
/// that its code stays the same across restarts (`<data>/companies/<id>.session`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct KeptSession {
    pub port: u16,
    /// The code's secret and the session key's PKCS#8 bytes (hex).
    pub secret: String,
    pub key: String,
}

impl KeptSession {
    /// The code's secret.
    pub fn secret_bytes(&self) -> [u8; 8] {
        let mut out = [0u8; 8];
        if let Some(b) = omsi_net::hostkey::unhex(&self.secret) {
            for (o, x) in out.iter_mut().zip(b) {
                *o = x;
            }
        }
        out
    }

    /// The session's key.
    pub fn host_key(&self) -> Option<omsi_net::hostkey::HostKey> {
        omsi_net::hostkey::HostKey::from_pkcs8(&omsi_net::hostkey::unhex(&self.key)?)
    }
}

/// The file of a company's kept session.
pub fn session_file(data: &Path, id: &str) -> PathBuf {
    store::dir(data).join(format!("{id}.session"))
}

/// The file the company's process writes its code to, for the main server's `status`.
pub fn code_file(data: &Path, id: &str) -> PathBuf {
    store::dir(data).join(format!("{id}.code"))
}

/// The main server's own secret in its folder (made the first time): the tickets it gives its
/// players are signed with it (`ticket`), and its companies' processes check them with it.
pub const SECRET_FILE: &str = "server.secret";

/// The main server's secret of the folder `data`, made the first time.
pub fn server_secret(data: &Path) -> Result<Vec<u8>> {
    if let Some(s) = read_server_secret(data) {
        return Ok(s);
    }
    let s: [u8; 32] = omsi_net::http::random_bytes();
    std::fs::create_dir_all(data).with_context(|| format!("cannot make {}", data.display()))?;
    let path = data.join(SECRET_FILE);
    std::fs::write(&path, omsi_net::hostkey::hex(&s)).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(s.to_vec())
}

/// The main server's secret, when it has made one.
pub fn read_server_secret(data: &Path) -> Option<Vec<u8>> {
    let text = std::fs::read_to_string(data.join(SECRET_FILE)).ok()?;
    omsi_net::hostkey::unhex(text.trim()).filter(|b| b.len() == 32)
}

fn ticket_text(company: &str, player: &str, name: &str) -> String {
    format!("bco-ticket\n{}\n{}\n{}", company.trim().to_lowercase(), player.trim(), name.trim())
}

/// Phase 2 (step 2): the ticket the main server gives a player it knows (logged in with his
/// Omsi-Hub account or his Steam ID and key: `server_players`) for company `company`: he shows it
/// with every message to the company's process (`wire::Request::ticket`), which lets nobody in
/// without one (`together::host::HostState::tickets`) - so a player is known to a company by the
/// server's id for him (`player`) and the server's name (`name`), not by what his launcher says.
pub fn ticket(secret: &[u8], company: &str, player: &str, name: &str) -> String {
    omsi_net::hostkey::hex(&omsi_net::http::hmac_sha256(secret, ticket_text(company, player, name).as_bytes()))
}

/// Whether `ticket` is the main server's for this player and company.
pub fn ticket_ok(secret: &[u8], company: &str, player: &str, name: &str, ticket: &str) -> bool {
    omsi_net::hostkey::unhex(ticket.trim()).is_some_and(|t| omsi_net::http::hmac_sha256_ok(secret, ticket_text(company, player, name).as_bytes(), &t))
}

/// Company `id`'s kept session in the folder `data`, made the first time: the first port from
/// `first_port` no other company of the folder has, a new secret and key.
pub fn kept_session(data: &Path, id: &str, first_port: u16) -> Result<KeptSession> {
    let path = session_file(data, id);
    if let Some(k) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<KeptSession>(&t).ok()).filter(|k| k.host_key().is_some()) {
        return Ok(k);
    }
    let taken: Vec<u16> = company_ids(data)
        .iter()
        .filter(|other| other.as_str() != id)
        .filter_map(|other| std::fs::read_to_string(session_file(data, other)).ok())
        .filter_map(|t| serde_json::from_str::<KeptSession>(&t).ok())
        .map(|k| k.port)
        .collect();
    let port = (first_port..=u16::MAX).find(|p| !taken.contains(p)).context("no port left for the company")?;
    let (_, pkcs8) = omsi_net::hostkey::HostKey::generate_kept().context("no key for the company's session")?;
    let secret: [u8; 8] = omsi_net::http::random_bytes();
    let k = KeptSession { port, secret: omsi_net::hostkey::hex(&secret), key: omsi_net::hostkey::hex(&pkcs8) };
    std::fs::create_dir_all(store::dir(data)).with_context(|| format!("cannot make {}", store::dir(data).display()))?;
    std::fs::write(&path, serde_json::to_string_pretty(&k)?).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(k)
}

/// The owner made so among the company's players (`Online::owner`): the first player of the
/// owner's name, from then on only the one with his id, gets every permission. Whether it
/// changed anything.
pub fn seat_owner(c: &mut Company) -> bool {
    use super::together::{Perm, Role};
    let name = c.online.owner.trim().to_lowercase();
    if name.is_empty() {
        return false;
    }
    let pinned = c.online.owner_id.clone();
    let Some(g) = c.together.guests.iter().find(|g| if pinned.is_empty() { g.name.trim().to_lowercase() == name } else { g.id == pinned }) else { return false };
    let (no, id) = (g.no, g.id.clone());
    let all: Vec<Perm> = Perm::ALL.to_vec();
    let done = g.role == Role::CoOwner && g.perms() == all && !g.banned && pinned == id;
    if done {
        return false;
    }
    c.online.owner_id = id;
    c.together.set_role(no, Role::CoOwner, &all);
    if let Some(g) = c.together.guest_mut(no) {
        g.banned = false;
    }
    true
}

/// A server's clock: every company on the server runs on it, and the server's world. Set once to
/// `at` (seconds of the companies' calendar: `dates::parse`'s day times 86 400, and the second of
/// that day) at the real time `real` (Unix seconds), it runs 1:1 with the real clock from there,
/// and goes on where it was after a restart - the time the server was off included
/// (`server_clock`).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerClock {
    pub real: i64,
    pub at: i64,
}

/// The server's clock in its company folder.
pub const CLOCK_FILE: &str = "clock.json";

const DAY_S: i64 = clock::DAY * 60;

impl ServerClock {
    /// Set to the second `secs` of `date` (YYYY-MM-DD) at the real time `real`.
    pub fn starting(date: &str, secs: i64, real: i64) -> Option<ServerClock> {
        Some(ServerClock { real, at: dates::parse(date)? * DAY_S + secs.clamp(0, DAY_S - 1) })
    }

    /// Its date and the second of that day at the real time `real` (Unix seconds, with their
    /// fraction): the server's world clock.
    pub fn at_real(&self, real: f64) -> (String, f64) {
        let at = self.at as f64 + (real - self.real as f64);
        let day = (at / DAY_S as f64).floor();
        (dates::fmt(day as i64), at - day * DAY_S as f64)
    }

    /// The companies' minute (`clock::moment`) at the real time `real`.
    pub fn minute(&self, real: i64) -> i64 {
        (self.at + (real - self.real)).div_euclid(60)
    }

    /// The course a company on this clock keeps (`clock::course`), to the second.
    fn synced(&self) -> Synced {
        Synced { real: self.real - self.at.rem_euclid(60), minute: self.at.div_euclid(60) }
    }
}

/// The server's clock kept in its company folder `data` (`CLOCK_FILE`), at the real time `real`:
/// as it was set the first time, going on 1:1. `start`: the date and second of the day the server
/// starts on - the first time, and on a server on the real time (`follow`) every time it starts,
/// unless that would take its clock back (the clocks set back an hour in the autumn: the
/// companies cannot go back, the server keeps its course).
pub fn server_clock(data: &Path, real: i64, follow: bool, start: (String, i64)) -> Result<ServerClock> {
    let path = data.join(CLOCK_FILE);
    let kept: Option<ServerClock> = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok());
    let fresh = ServerClock::starting(&start.0, start.1, real).with_context(|| format!("the server's date {} is no date (YYYY-MM-DD)", start.0))?;
    let sc = match kept {
        Some(k) if !follow || fresh.minute(real) < k.minute(real) => k,
        _ => fresh,
    };
    if kept != Some(sc) {
        std::fs::create_dir_all(data).with_context(|| format!("cannot make {}", data.display()))?;
        std::fs::write(&path, serde_json::to_string_pretty(&sc)?).with_context(|| format!("cannot write {}", path.display()))?;
    }
    Ok(sc)
}

/// The server's clock as its folder keeps it (`CLOCK_FILE`), not set nor moved: what a map's
/// world runs on (the main server keeps it).
pub fn read_server_clock(data: &Path) -> Option<ServerClock> {
    serde_json::from_str(&std::fs::read_to_string(data.join(CLOCK_FILE)).ok()?).ok()
}

/// Held by the main server that runs on the folder (`lock_folder`).
pub const LOCK_FILE: &str = "server.lock";
/// One more each time a company's plan was written, a company came on or the clock was set: the
/// maps' worlds take the plans (and the clock) in again (`bump_plans`, `plans_gen`).
pub const PLANS_FILE: &str = "plans.gen";

/// The plans' generation of the folder `data` (0: none written yet).
pub fn plans_gen(data: &Path) -> u64 {
    std::fs::read_to_string(data.join(PLANS_FILE)).ok().and_then(|t| t.trim().parse().ok()).unwrap_or(0)
}

/// One more plans' generation in the folder `data` (written whole: a world never reads half of
/// it). The new one.
pub fn bump_plans(data: &Path) -> Result<u64> {
    let g = plans_gen(data) + 1;
    std::fs::create_dir_all(data).with_context(|| format!("cannot make {}", data.display()))?;
    let tmp = data.join(format!("{PLANS_FILE}.tmp"));
    std::fs::write(&tmp, g.to_string()).with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, data.join(PLANS_FILE)).with_context(|| format!("cannot write {}", data.join(PLANS_FILE).display()))?;
    Ok(g)
}

/// A map's folder name from a map path (`maps/Ahlheim/global.cfg`: `ahlheim`).
pub fn map_folder(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches("/global.cfg").trim_end_matches('/').rsplit('/').next().unwrap_or("").to_lowercase()
}

/// The ids of the companies in a server's folder (`<id>.json` of its `companies`; a plan beside
/// one, `<id>.liveplan.json`, is none), sorted.
pub fn company_ids(data: &Path) -> Vec<String> {
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

/// The folder taken for this main server (`LOCK_FILE`, held as long as the file lives - the
/// process gone, the system lets it go): two main servers on one folder would write the same
/// companies. An error: another one runs on it.
pub fn lock_folder(data: &Path) -> Result<std::fs::File> {
    std::fs::create_dir_all(data).with_context(|| format!("cannot make {}", data.display()))?;
    let path = data.join(LOCK_FILE);
    let f = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&path).with_context(|| format!("cannot open {}", path.display()))?;
    match f.try_lock() {
        Ok(()) => Ok(f),
        Err(std::fs::TryLockError::WouldBlock) => anyhow::bail!("another main server runs on the company folder {} - one main server per folder", data.display()),
        Err(std::fs::TryLockError::Error(e)) => Err(e).with_context(|| format!("cannot lock {}", path.display())),
    }
}

/// A new company founded on the server (companies are founded there, never brought along): on
/// the server's clock - its first day the server's date, its minute the server's - and running on
/// a server (`Online::on_server`), saved in `data` under an id not taken yet. Its id.
pub fn found_on_server(data: &Path, f: &Founding, founder: &str, sc: ServerClock, real: i64) -> Result<String> {
    let (date, secs) = sc.at_real(real as f64);
    let mut c = super::found(&Founding { date, ..f.clone() }, founder);
    c.id = store::unused_id(data, &c.name);
    c.clock.minute = (secs / 60.0) as u32;
    c.online.on_server = true;
    c.clock.real.on = true;
    c.clock.real.lead = 0;
    c.clock.real.synced = Some(sc.synced());
    store::save(data, &c)?;
    Ok(c.id)
}

/// The company on a server.
pub struct Host {
    /// The server's company folder (`store::dir` in it): the company, its live file, its plan.
    pub data: PathBuf,
    /// The map's folder as the server has it (its timetables read from there; None: through the
    /// launcher's OMSI folder, as on a player's computer).
    pub map_dir: Option<PathBuf>,
    pub company: Company,
    /// The timetables read so far (`store::Disk::with_read`).
    read: Vec<(String, String, Vec<LineInfo>)>,
    /// The date the world's plan was written for.
    planned: Option<String>,
    /// One more each time the world's plan is written: the world takes it in anew.
    pub plan_gen: u64,
    /// The live events of the day told already: (the day, how many).
    seen: (String, usize),
    /// When it was saved last (real seconds), and whether it changed since.
    saved_at: i64,
    dirty: bool,
    /// The server's clock the company runs on (None: its own course, as on a player's computer).
    server: Option<ServerClock>,
}

/// What a step came to.
#[derive(Default)]
pub struct Stepped {
    /// The days closed on the way.
    pub closed: Vec<DayReport>,
    /// Live events told (the world's reports of the company's buses).
    pub told: usize,
    /// The world's plan was written (a new day, or the first).
    pub planned: bool,
    /// The company was saved.
    pub saved: bool,
}

impl Host {
    /// The company `id` of the server's folder `data`, caught up with the real clock (`real`,
    /// Unix seconds) and saved, its day's plan written. A company that did not run with the real
    /// clock does from now on (a server's always does), from where it is.
    /// `map_dir`: the map's folder as the server has it; `clock`: the server's clock, the
    /// company's course pinned to it.
    pub fn open(data: &Path, id: &str, real: i64, map_dir: Option<&Path>, clock: Option<ServerClock>) -> Result<(Host, Option<Away>)> {
        let c = store::load(data, id).with_context(|| format!("the company {id} in {}", data.display()))?;
        let mut h = Host::new(data, c, real, clock);
        h.map_dir = map_dir.map(Path::to_path_buf);
        let mut disk = store::Disk::with_read(data, std::mem::take(&mut h.read)).on_map(map_dir);
        let r = h.open_with(real, &mut disk);
        h.done_with(disk);
        r.map(|away| (h, away))
    }

    /// `open` with the world given (the timetable and the night: a test's own).
    pub fn open_in(data: &Path, id: &str, real: i64, w: &mut dyn World, clock: Option<ServerClock>) -> Result<(Host, Option<Away>)> {
        let c = store::load(data, id)?;
        let mut h = Host::new(data, c, real, clock);
        let away = h.open_with(real, w)?;
        Ok((h, away))
    }

    fn new(data: &Path, mut company: Company, real: i64, server: Option<ServerClock>) -> Host {
        if !company.clock.real.on {
            clock::set_realtime(&mut company, true);
        }
        company.online.on_server = true;
        let seen = (company.date.clone(), 0);
        let mut h = Host { data: data.to_path_buf(), map_dir: None, company, read: Vec::new(), planned: None, plan_gen: 0, seen, saved_at: real, dirty: false, server };
        h.pin();
        h
    }

    /// On the server's clock: the company's course is the server's (a lead it took - a step
    /// ahead of it - waits for the server to come up to it).
    fn pin(&mut self) -> bool {
        let Some(sc) = self.server else { return false };
        let r = &mut self.company.clock.real;
        let was = (r.on, r.synced, r.lead);
        (r.on, r.synced, r.lead) = (true, Some(sc.synced()), 0);
        was != (r.on, r.synced, r.lead)
    }

    fn open_with(&mut self, real: i64, w: &mut dyn World) -> Result<Option<Away>> {
        let away = clock::catch_up(&mut self.company, real, w).map_err(anyhow::Error::msg)?;
        self.pin();
        self.seen = (self.company.date.clone(), 0);
        self.write_plan(w)?;
        self.save(real)?;
        Ok(away)
    }

    /// The company's time moved on to the real clock's (`real`, Unix seconds), with the world's
    /// reports told, its days closed and planned, and saved as it is due.
    pub fn tick(&mut self, real: i64) -> Result<Stepped> {
        let (data, map_dir) = (self.data.clone(), self.map_dir.clone());
        let mut disk = store::Disk::with_read(&data, std::mem::take(&mut self.read)).on_map(map_dir.as_deref());
        let r = self.tick_in(real, &mut disk);
        self.done_with(disk);
        r
    }

    /// `tick` with the world given.
    pub fn tick_in(&mut self, real: i64, w: &mut dyn World) -> Result<Stepped> {
        let mut s = Stepped::default();
        let before = self.company.date.clone();
        if self.pin() {
            self.dirty = true;
        }
        let c = &mut self.company;
        // never in step yet: in step from now (a company that did not run with the real clock)
        if c.clock.real.synced.is_none() {
            clock::sync(c, real);
            self.dirty = true;
        }
        // what the world reported of the company's buses since: told as it comes
        if self.seen.0 != c.date {
            self.seen = (c.date.clone(), 0);
        }
        let live = store::peek_live(&self.data, &c.id, self.seen.1);
        if !live.is_empty() {
            self.seen.1 += live.len();
            s.told = clock::tell_live(c, &live);
            self.dirty = true;
        }
        // its time on to the real clock's: a long way as an absence is caught up, else step by
        // step (the dispatcher deciding: nobody is asked on a server)
        let now = clock::now(c);
        if let Some(to) = clock::course(c, real).filter(|to| *to > now) {
            if to - now >= clock::AWAY_TOLD {
                if let Some(away) = clock::catch_up(c, real, w).map_err(anyhow::Error::msg)? {
                    s.closed = away.reports;
                }
            } else {
                s.closed = clock::advance(c, to, w, true).map_err(anyhow::Error::msg)?.reports;
            }
            self.dirty = true;
        }
        let new_day = self.company.date != before;
        if new_day {
            self.seen = (self.company.date.clone(), 0);
        }
        if new_day || self.planned.as_deref() != Some(self.company.date.as_str()) {
            self.write_plan(w)?;
            s.planned = true;
        }
        if new_day || (self.dirty && real - self.saved_at >= SAVE_EVERY) {
            self.save(real)?;
            s.saved = true;
        }
        Ok(s)
    }

    /// The day's plan for the world: the company's tours of its date with their buses (or
    /// dropped), as the launcher writes it for a drive (`plan::live_plan`).
    fn write_plan(&mut self, w: &mut dyn World) -> Result<()> {
        // (the day entered first: its roster filled or repaired, as the clock does at its start)
        clock::enter_day(&mut self.company, w).map_err(anyhow::Error::msg)?;
        let c = &self.company;
        let date = c.date.clone();
        let lines = w.lines(c, &date).map_err(anyhow::Error::msg)?;
        let tours = network::tours_of_day(c, &lines, &date);
        let day = plan::day_plan(c, &date, tours, &[], &[], false);
        plan::save_live_plan(&self.data, &plan::live_plan(c, &day))?;
        self.planned = Some(date);
        self.plan_gen += 1;
        Ok(())
    }

    /// The company changed from outside (a page of its owner's): its day's plan for the world is
    /// written again at the next step.
    pub fn replan(&mut self) {
        self.planned = None;
        self.dirty = true;
    }

    /// Saved now.
    pub fn save(&mut self, real: i64) -> Result<()> {
        store::save(&self.data, &self.company)?;
        self.saved_at = real;
        self.dirty = false;
        Ok(())
    }

    /// The disk's timetables kept, and the live file a close took in let go (saved with it).
    fn done_with(&mut self, disk: store::Disk) {
        let (read, took) = disk.into_parts();
        self.read = read;
        if let Some(date) = took {
            // (a close saves the company: `tick_in`; saved again here should the save above
            // have been skipped by an error)
            if self.dirty {
                let _ = store::save(&self.data, &self.company);
                self.dirty = false;
            }
            store::forget_taken(&self.data, &self.company.id, &date);
        }
    }

    /// The company's date and minute of the day: the server's world runs on them.
    pub fn now(&self) -> (String, u32) {
        (self.company.date.clone(), self.company.clock.minute)
    }
}

#[cfg(test)]
mod tests {
    use super::super::concessions::tests::line;
    use super::super::market::{self, MarketBus, Payment};
    use super::super::model::{GameMode, Licence};
    use super::super::staff::{applicants, hire};
    use super::super::{day, depot, found, Founding};
    use super::*;
    use crate::LineInfo;

    /// The timetable of every day the same; the night as the store runs it.
    struct Fake(Vec<LineInfo>);

    impl World for Fake {
        fn lines(&mut self, _: &Company, _: &str) -> std::result::Result<Vec<LineInfo>, String> {
            Ok(self.0.clone())
        }

        fn close_day(&mut self, c: &mut Company, lines: &[LineInfo]) -> std::result::Result<DayReport, String> {
            network::refresh_lines(c, lines);
            let date = c.date.clone();
            let tours = network::tours_of_day(c, lines, &date);
            let r = day::close_day(c, tours, &[]);
            Ok(depot::after_close(c, r, lines))
        }
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("openomsi-online-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// A company of a line in service with its buses and drivers, saved in `data`; `planned`:
    /// its planners filled the roster of every weekday.
    fn company_of(data: &Path, mode: GameMode, planned: bool) -> (String, Fake) {
        let lines = vec![line("Linie5", "5", 3, true), line("Linie7", "7", 2, true)];
        let mut c = found(&Founding { name: "Online".into(), game_mode: mode, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.cash += 5_000_000_00;
        network::add_line(&mut c, &lines[0], None).unwrap();
        network::start_service(&mut c, "Linie5", 0).unwrap();
        c.planning.auto = true;
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        for _ in 0..4 {
            market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
        }
        let id = c.id.clone();
        while c.staff.len() < 8 {
            for a in applicants(&c) {
                if c.staff.len() < 8 && a.licence == Licence::D {
                    hire(&mut c, &a).unwrap();
                }
            }
            c.id.push('x');
            c.taken = Default::default();
        }
        c.id = id.clone();
        for e in c.staff.iter_mut() {
            e.sick_until = None;
            e.holiday_until = None;
        }
        if planned {
            for k in 0..7 {
                let date = super::super::dates::add("2024-03-04", k);
                let tours = network::tours_of_day(&c, &lines, &date);
                plan::fill_day(&mut c, &date, tours);
            }
        }
        // (the company's morning: in step with the real clock from when it is opened)
        c.clock.minute = 5 * 60;
        store::save(data, &c).unwrap();
        (id, Fake(lines))
    }

    fn company(data: &Path) -> (String, Fake) {
        company_of(data, GameMode::Normal, true)
    }

    /// Real time: Unix seconds `hours` after the moment the tests start from (to the minute).
    fn at(hours: f64) -> i64 {
        1_800_000_000 + (hours * 60.0).round() as i64 * 60
    }

    /// Opened, the company goes in step with the real clock and writes its day's plan for the
    /// world; its time then runs as the real time does, a minute a minute, and the plan is
    /// written once a day.
    #[test]
    fn a_server_company_runs_with_the_real_clock() {
        let data = temp("clock");
        let (id, mut w) = company(&data);
        let (mut h, away) = Host::open_in(&data, &id, at(0.0), &mut w, None).unwrap();
        assert!(away.is_none(), "never in step before: nothing caught up");
        assert_eq!(h.now(), ("2024-03-04".to_string(), 5 * 60));
        assert_eq!(h.plan_gen, 1);
        let p = plan::live_plans(&data).into_iter().find(|p| p.company == id).expect("the world's plan");
        assert_eq!(p.date, "2024-03-04");
        assert!(p.tours.iter().any(|t| t.vehicle.is_some()), "its buses on its tours: {:?}", p.tours);
        // ten minutes on: ten minutes of the company's day, the plan as it was
        let s = h.tick_in(at(10.0 / 60.0), &mut w).unwrap();
        assert_eq!(h.now().1, 5 * 60 + 10);
        assert!(!s.planned && s.closed.is_empty() && h.plan_gen == 1);
        // (an hour: as an absence - caught up the same)
        h.tick_in(at(1.0 + 10.0 / 60.0), &mut w).unwrap();
        assert_eq!(h.now().1, 6 * 60 + 10);
        let _ = std::fs::remove_dir_all(&data);
    }

    /// At midnight the day is closed and booked, the next day planned for the world and the
    /// company saved; a server away for two days catches them up, each closed.
    #[test]
    fn its_days_are_closed_planned_and_saved() {
        let data = temp("days");
        let (id, mut w) = company(&data);
        let (mut h, _) = Host::open_in(&data, &id, at(0.0), &mut w, None).unwrap();
        let history = h.company.history.len();
        // to 00:05 of the next day
        let s = h.tick_in(at(19.0 + 5.0 / 60.0), &mut w).unwrap();
        assert_eq!(s.closed.len(), 1, "the day closed");
        assert!(s.planned && s.saved);
        assert_eq!(h.now(), ("2024-03-05".to_string(), 5));
        assert_eq!(h.company.history.len(), history + 1);
        assert!(plan::live_plans(&data).iter().any(|p| p.company == id && p.date == "2024-03-05"), "the new day's plan");
        assert_eq!(store::load(&data, &id).unwrap().date, "2024-03-05", "saved");
        // two days on: both closed
        let s = h.tick_in(at(19.0 + 48.0 + 5.0 / 60.0), &mut w).unwrap();
        assert_eq!(s.closed.len(), 2);
        assert_eq!(h.now(), ("2024-03-07".to_string(), 5));
        let _ = std::fs::remove_dir_all(&data);
    }

    /// On a server the roster of a Normal or Realistic company is its planners': what they left
    /// open does not run. The easy mode's own routine fills it, as on a player's computer.
    #[test]
    fn what_the_planners_leave_open_does_not_run_but_the_easy_mode_fills_it() {
        for (mode, runs) in [(GameMode::Normal, false), (GameMode::Realistic, false), (GameMode::Casual, true)] {
            let data = temp("open");
            let (id, mut w) = company_of(&data, mode, false);
            let (h, _) = Host::open_in(&data, &id, at(0.0), &mut w, None).unwrap();
            assert!(h.company.online.on_server);
            let p = plan::live_plans(&data).into_iter().find(|p| p.company == id).unwrap();
            assert!(!p.tours.is_empty());
            assert_eq!(p.tours.iter().any(|t| t.vehicle.is_some() && !t.dropped), runs, "{mode:?}: {:?}", p.tours);
            let _ = std::fs::remove_dir_all(&data);
        }
    }

    /// The server's clock: set the first time, then 1:1 with the real clock, on where it was
    /// after a restart (the time it was off included); on the real time it follows the day it
    /// starts on - but never back.
    /// A company's session kept: its port its own, the same code after a restart; the owner
    /// seated by his name the first time, by his id from then on.
    #[test]
    fn the_session_kept_and_the_owner_seated() {
        let data = temp("kept");
        let (a, _) = company(&data);
        let mut c = store::load(&data, &a).unwrap();
        c.id = "second".into();
        store::save(&data, &c).unwrap();
        let ka = kept_session(&data, &a, 27045).unwrap();
        let kb = kept_session(&data, "second", 27045).unwrap();
        assert_eq!((ka.port, kb.port), (27045, 27046));
        assert_eq!(kept_session(&data, &a, 27045).unwrap(), ka, "kept");
        assert_eq!(ka.host_key().unwrap().session(), kept_session(&data, &a, 27045).unwrap().host_key().unwrap().session());
        assert_ne!(ka.secret_bytes(), [0u8; 8]);
        // the owner
        let mut c = store::load(&data, &a).unwrap();
        assert!(!seat_owner(&mut c), "no owner named");
        c.online.owner = "Mathias".into();
        let other = c.together.guest_of("pc-2", "Jonas").unwrap();
        assert!(!seat_owner(&mut c), "not here yet");
        let me = c.together.guest_of("pc-1", "mathias").unwrap();
        assert!(seat_owner(&mut c));
        assert_eq!(c.together.guest(me).unwrap().role, super::super::together::Role::CoOwner);
        assert_eq!(c.online.owner_id, "pc-1");
        assert!(!seat_owner(&mut c), "seated once");
        // another of the name later: not the owner
        let impostor = c.together.guest_of("pc-3", "Mathias").unwrap();
        seat_owner(&mut c);
        assert_eq!(c.together.guest(impostor).unwrap().role, super::super::together::Role::Driver);
        assert_eq!(c.together.guest(other).unwrap().role, super::super::together::Role::Driver);
        let _ = std::fs::remove_dir_all(&data);
    }

    /// One main server at a time on a folder; the plans' generation the worlds watch.
    #[test]
    fn one_main_server_on_a_folder_and_the_plans_generation() {
        let data = temp("folder");
        let held = lock_folder(&data).unwrap();
        assert!(format!("{:#}", lock_folder(&data).unwrap_err()).contains("another"));
        drop(held);
        lock_folder(&data).unwrap();
        assert_eq!(plans_gen(&data), 0);
        assert_eq!((bump_plans(&data).unwrap(), bump_plans(&data).unwrap()), (1, 2));
        assert_eq!(plans_gen(&data), 2);
        assert_eq!(map_folder("maps\\Ahlheim 5\\global.cfg"), "ahlheim 5");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn the_servers_clock_goes_on_where_it_was() {
        let data = temp("server-clock");
        let sc = server_clock(&data, at(0.0), false, ("2024-03-04".into(), 8 * 3600)).unwrap();
        assert_eq!(sc.at_real(at(0.0) as f64 + 0.5), ("2024-03-04".to_string(), 8.0 * 3600.0 + 0.5));
        assert_eq!(sc.minute(at(1.0)), clock::moment("2024-03-04", 9 * 60));
        // over midnight, to the second
        assert_eq!(sc.at_real(at(16.0) as f64 + 30.0), ("2024-03-05".to_string(), 30.0));
        // started again three hours later with another start in server.cfg: on where it was
        let again = server_clock(&data, at(3.0), false, ("2030-01-01".into(), 0)).unwrap();
        assert_eq!(again, sc);
        assert_eq!(again.at_real(at(3.0) as f64).1, 11.0 * 3600.0);
        // on the real time: the day it starts on - an hour on (spring) taken, an hour back not
        let real = server_clock(&data, at(3.0), true, ("2024-03-04".into(), 12 * 3600)).unwrap();
        assert_eq!(real.at_real(at(3.0) as f64).1, 12.0 * 3600.0);
        let back = server_clock(&data, at(3.0), true, ("2024-03-04".into(), 11 * 3600)).unwrap();
        assert_eq!(back, real, "never back");
        assert!(ServerClock::starting("not a date", 0, 0).is_none());
        let _ = std::fs::remove_dir_all(&data);
    }

    /// Every company of a server runs on the server's clock: one behind it catches up when it is
    /// opened, one ahead of it waits for it; and a company founded there starts on its date and
    /// minute, under an id of its own.
    #[test]
    fn the_companies_run_on_the_servers_clock() {
        let data = temp("on-the-clock");
        let (id, mut w) = company(&data);
        // the server at 07:30 when the company (at 05:00) comes onto it
        let sc = ServerClock::starting("2024-03-04", 7 * 3600 + 1800, at(0.0)).unwrap();
        let (mut h, away) = Host::open_in(&data, &id, at(0.0), &mut w, Some(sc)).unwrap();
        assert!(away.is_some(), "the two and a half hours caught up");
        assert_eq!(h.now(), ("2024-03-04".to_string(), 7 * 60 + 30));
        h.tick_in(at(1.0), &mut w).unwrap();
        assert_eq!(h.now().1, 8 * 60 + 30);
        // a server whose clock is behind the company's (04:00): the company waits for it
        let behind = ServerClock::starting("2024-03-04", 4 * 3600, at(2.0)).unwrap();
        h.save(at(2.0)).unwrap();
        drop(h);
        let (mut h, _) = Host::open_in(&data, &id, at(2.0), &mut w, Some(behind)).unwrap();
        h.tick_in(at(3.0), &mut w).unwrap();
        assert_eq!(h.now().1, 8 * 60 + 30, "waits");
        h.tick_in(at(7.0), &mut w).unwrap();
        assert_eq!(h.now().1, 9 * 60, "on with the server once it is there");
        // founded on the server: its date and minute, an id of its own, on a server
        let f = Founding { name: "Online".into(), map: "maps/Grundorf/global.cfg".into(), date: "1999-01-01".into(), ..Default::default() };
        let a = found_on_server(&data, &f, "owner", sc, at(1.0)).unwrap();
        let b = found_on_server(&data, &f, "owner", sc, at(1.0)).unwrap();
        assert_ne!(a, b);
        assert_ne!(a, id);
        let c = store::load(&data, &b).unwrap();
        assert_eq!((c.date.as_str(), c.clock.minute, c.online.on_server), ("2024-03-04", 8 * 60 + 30, true));
        let (h, away) = Host::open_in(&data, &b, at(1.0), &mut w, Some(sc)).unwrap();
        assert!(away.is_none(), "nothing to catch up");
        assert_eq!(h.now(), ("2024-03-04".to_string(), 8 * 60 + 30));
        let _ = std::fs::remove_dir_all(&data);
    }

    /// A server stopped and started again goes on where the company was saved: the time it was
    /// away caught up, nothing closed twice.
    #[test]
    fn a_server_started_again_goes_on_where_it_was() {
        let data = temp("restart");
        let (id, mut w) = company(&data);
        let (mut h, _) = Host::open_in(&data, &id, at(0.0), &mut w, None).unwrap();
        h.tick_in(at(20.0), &mut w).unwrap();
        assert_eq!(h.now(), ("2024-03-05".to_string(), 60));
        // (the minutes after the last save are the server's; saved on the way down)
        h.save(at(20.0)).unwrap();
        let days = h.company.history.len();
        drop(h);
        // five hours later
        let (h, away) = Host::open_in(&data, &id, at(25.0), &mut w, None).unwrap();
        let away = away.expect("the five hours caught up");
        assert_eq!((away.caught.from, away.caught.to), (clock::moment("2024-03-05", 60), clock::moment("2024-03-05", 6 * 60)));
        assert_eq!(h.now(), ("2024-03-05".to_string(), 6 * 60));
        assert_eq!(h.company.history.len(), days, "no day closed again");
        let _ = std::fs::remove_dir_all(&data);
    }

    /// What the world reports of the company's buses is told once, as it comes, and booked at
    /// the day's close.
    #[test]
    fn the_worlds_reports_are_told_once() {
        let data = temp("live");
        let (id, mut w) = company(&data);
        let (mut h, _) = Host::open_in(&data, &id, at(0.0), &mut w, None).unwrap();
        let trip = day::LiveEvent::Trip { line: "5".into(), tour: "1".into(), vehicle: Some(1), passengers: 31, km: 10.0, delay: 45.0, completed: true, dep: Some(6 * 60) };
        store::append_live(&data, &id, &trip).unwrap();
        store::append_live(&data, &id, &trip).unwrap();
        assert_eq!(h.tick_in(at(0.1), &mut w).unwrap().told, 2);
        assert_eq!(h.tick_in(at(0.2), &mut w).unwrap().told, 0, "told once");
        store::append_live(&data, &id, &trip).unwrap();
        assert_eq!(h.tick_in(at(0.3), &mut w).unwrap().told, 1);
        let _ = std::fs::remove_dir_all(&data);
    }

    /// A company on a real map, run on its server for a day and a half: how long its steps take
    /// (`OMSI_COMPANY_MAP_DIR=<a map's folder> cargo test -p omsi-launcher-core --lib
    /// online::tests::on_a_real_map -- --ignored --nocapture`).
    #[test]
    #[ignore = "needs a map's folder: OMSI_COMPANY_MAP_DIR"]
    fn on_a_real_map() {
        let Some(dir) = std::env::var_os("OMSI_COMPANY_MAP_DIR").map(PathBuf::from) else { return };
        let data = temp("real");
        let date = "2024-03-04";
        let t = std::time::Instant::now();
        let all = crate::lines_in_map_dir(&dir, date).unwrap();
        eprintln!("the map's lines read in {:.2} s: {} lines, {} tours today", t.elapsed().as_secs_f32(), all.len(), all.iter().map(|l| l.tours.iter().filter(|t| t.today).count()).sum::<usize>());
        let mut c = found(&Founding { name: "Server".into(), game_mode: GameMode::Normal, date: date.into(), map: "maps/X/global.cfg".into(), ..Default::default() }, "Server");
        c.cash += 50_000_000_00;
        // the three lines with the most tours today
        let mut lines: Vec<&LineInfo> = all.iter().filter(|l| l.user_allowed).collect();
        lines.sort_by_key(|l| std::cmp::Reverse(l.tours.iter().filter(|t| t.today).count()));
        let mut tours = 0;
        for l in lines.iter().take(3) {
            if network::add_line(&mut c, l, None).is_ok() && network::start_service(&mut c, &l.name, 0).is_ok() {
                tours += l.tours.iter().filter(|t| t.today).count();
            }
        }
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        // (as many as the depot has room for)
        for _ in 0..tours + 2 {
            if market::buy_new(&mut c, &bus, Payment::Cash, "").is_err() {
                break;
            }
        }
        let id = c.id.clone();
        while c.staff.len() < tours * 2 + 2 {
            for a in applicants(&c) {
                if c.staff.len() < tours * 2 + 2 && a.licence == Licence::D {
                    hire(&mut c, &a).unwrap();
                }
            }
            c.id.push('x');
            c.taken = Default::default();
        }
        c.id = id.clone();
        // (its planners fill the roster of the two days)
        for d in [date.to_string(), super::super::dates::add(date, 1)] {
            let lines = crate::lines_in_map_dir(&dir, &d).unwrap();
            let tours = network::tours_of_day(&c, &lines, &d);
            plan::fill_day(&mut c, &d, tours);
        }
        c.clock.minute = 5 * 60;
        store::save(&data, &c).unwrap();
        let t = std::time::Instant::now();
        let (mut h, _) = Host::open(&data, &id, at(0.0), Some(&dir), None).unwrap();
        eprintln!("opened in {:.2} s: {} lines in service, {} tours today, {} buses, {} staff", t.elapsed().as_secs_f32(), h.company.lines.len(), tours, h.company.fleet.len(), h.company.staff.len());
        let p = plan::live_plans(&data).into_iter().find(|p| p.company == id).unwrap();
        eprintln!("the world's plan: {} tours, {} with a bus, {} dropped", p.tours.len(), p.tours.iter().filter(|t| t.vehicle.is_some()).count(), p.tours.iter().filter(|t| t.dropped).count());
        // a step a minute through the morning
        let t = std::time::Instant::now();
        for m in 1..=60 {
            h.tick(at(m as f64 / 60.0)).unwrap();
        }
        eprintln!("an hour, a step a minute: {:.1} ms a step", t.elapsed().as_secs_f64() * 1000.0 / 60.0);
        let t = std::time::Instant::now();
        let s = h.tick(at(19.0 + 5.0 / 60.0)).unwrap();
        eprintln!("on to 00:05 of the next day: {:.2} s, {} day closed, result {:?}", t.elapsed().as_secs_f32(), s.closed.len(), s.closed.first().map(|r| r.result));
        assert_eq!(h.now(), ("2024-03-05".to_string(), 5));
        let p = plan::live_plans(&data).into_iter().find(|p| p.company == id && p.date == "2024-03-05").expect("the next day's plan");
        assert!(p.tours.iter().any(|t| t.vehicle.is_some()));
        let size = std::fs::metadata(store::path_of(&data, &id)).map(|m| m.len()).unwrap_or(0);
        eprintln!("the company's file: {} KB", size / 1024);
        let _ = std::fs::remove_dir_all(&data);
    }
}
