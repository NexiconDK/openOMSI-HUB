//! Bus Company Online's main server (NexiconDK, phase 1 of docs/omsihub/BUS_COMPANY_ONLINE.md):
//! `omsi-hub-cli --company-server <folder>`, a program without a window, a map or graphics that
//! runs around the clock on the server's machine.
//!
//! It keeps every bus company of every map in its folder (`<folder>/companies`), each on its map
//! for good, on the server's clock (`online::ServerClock`, kept in `<folder>/clock.json`): their
//! hosts (`online::Host`) step every `STEP` - the worlds' reports told, the time moved on, the days
//! closed at midnight and planned, saved - and a company founded meanwhile is taken on (`SCAN`).
//! A midnight costs a company a second or more: nothing here answers a player in the meantime.
//!
//! The players drive in a **world per map**: the dedicated server (`omsi-hub-game --server`) with
//! `companies = 1`, which runs the map with every company's buses in their paint on the server's
//! clock and reports their trips into the folder. The main server starts a map's world when it is
//! asked to (`start_world`; phase 1: the console's `start <map>`), each on ports of its own; a world
//! stops by itself a while after its last player has gone (`world_idle`). A map without a world:
//! its companies' trips are worked out at the day's close, as Bus Company does for the days not
//! played.
//!
//! The settings are `<folder>/company-server.cfg` (written with the defaults and a comment for
//! every key when it is missing); the console takes `help`, `status`, `start`, `stop`, `found` and
//! `quit`. A company on the server is always founded there, new: nothing of a player's own
//! companies comes along.

use super::model::GameMode;
use super::online::{self, company_ids, map_folder, Host, ServerClock};
use super::{store, Founding};
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// The main server's log (`company-server.log` in its folder), beside the console.
static LOG: std::sync::Mutex<Option<std::fs::File>> = std::sync::Mutex::new(None);

/// A line for the console and the log, with the machine's date and time.
fn note(level: &str, text: String) {
    let at = crate::local_now().map(|(y, mo, d, h, mi)| format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02} ")).unwrap_or_default();
    let line = format!("{at}{level}{text}");
    println!("{line}");
    if let Some(f) = LOG.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
        use std::io::Write;
        let _ = writeln!(f, "{line}");
    }
}

macro_rules! info {
    ($($a:tt)*) => { note("", format!($($a)*)) };
}
macro_rules! warn {
    ($($a:tt)*) => { note("warning: ", format!($($a)*)) };
}

/// How often the companies step (real time).
pub const STEP: Duration = Duration::from_secs(2);
/// How often the folder is looked through for companies founded since (real time).
pub const SCAN: Duration = Duration::from_secs(30);
/// The main server's settings in its folder.
pub const CFG_FILE: &str = "company-server.cfg";

pub const DEFAULT_CFG: &str = "\
# Omsi-Hub: Bus Company Online, the main server
# (key = value; lines starting with # are comments)

# the OMSI 2 folder with the maps (empty: the one the launcher on this machine uses)
root =

# the server's clock: this machine's real date and time (1), or the date (YYYY-MM-DD, empty:
# today) and time below for the very first start - from then on it runs 1:1 with the real clock
# and goes on where it was after a restart, the time the server was off included
real_time = 1
date =
time = 06:00

# the maps' worlds: the dedicated server for each map in use. The game program they run in
# (empty: the one beside this program), the first of their ports (a world takes two: UDP and the
# web gateway), how many may run at once, and the minutes a world goes on without players
game =
first_port = 27015
max_worlds = 4
world_idle = 10

# what each world runs (as server.cfg's keys of the same names)
traffic = 30
passengers = 1
max_players = 16
tunnel = 0
share_positions = 1
admin_password =
";

/// What `company-server.cfg` says (see `DEFAULT_CFG`).
#[derive(Debug, Clone, PartialEq)]
pub struct MainCfg {
    pub root: PathBuf,
    pub real_time: bool,
    pub date: Option<String>,
    pub time: String,
    pub game: Option<PathBuf>,
    pub first_port: u16,
    pub max_worlds: usize,
    pub world_idle: u32,
    pub traffic: usize,
    pub passengers: bool,
    pub max_players: usize,
    pub tunnel: bool,
    pub share_positions: bool,
    pub admin_password: String,
}

impl MainCfg {
    /// The settings of the folder `dir` (written with the defaults when missing).
    pub fn load(dir: &Path) -> Result<MainCfg> {
        let path = dir.join(CFG_FILE);
        if !path.is_file() {
            std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
            std::fs::write(&path, DEFAULT_CFG).with_context(|| format!("writing {}", path.display()))?;
        }
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        Ok(MainCfg::parse(&text))
    }

    pub fn parse(text: &str) -> MainCfg {
        let mut kv: HashMap<String, String> = HashMap::new();
        for line in text.lines() {
            let l = line.trim();
            if l.is_empty() || l.starts_with('#') {
                continue;
            }
            if let Some((k, v)) = l.split_once('=') {
                kv.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
            }
        }
        let text = |k: &str| kv.get(k).cloned().filter(|v| !v.is_empty());
        let flag = |k: &str, d: bool| kv.get(k).map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")).unwrap_or(d);
        let num = |k: &str, d: i64| kv.get(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(d);
        MainCfg {
            root: text("root").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(crate::load_config().root)),
            real_time: flag("real_time", true),
            date: text("date"),
            time: text("time").unwrap_or_else(|| "06:00".into()),
            game: text("game").map(PathBuf::from),
            first_port: num("first_port", 27015).clamp(1, 65000) as u16,
            max_worlds: num("max_worlds", 4).clamp(1, 64) as usize,
            world_idle: num("world_idle", 10).clamp(0, 24 * 60) as u32,
            traffic: num("traffic", 30).max(0) as usize,
            passengers: flag("passengers", true),
            max_players: num("max_players", 16).clamp(1, 64) as usize,
            tunnel: flag("tunnel", false),
            share_positions: flag("share_positions", true),
            admin_password: text("admin_password").unwrap_or_default(),
        }
    }

    /// The date and second of the day the server's clock starts on (the first time, and on the
    /// real time every time: `online::server_clock`).
    pub fn clock_start(&self) -> (String, i64) {
        let now = crate::local_now();
        if self.real_time {
            if let Some((y, mo, d, h, mi)) = now {
                return (format!("{y:04}-{mo:02}-{d:02}"), (h * 3600 + mi * 60) as i64);
            }
        }
        let today = now.map(|(y, mo, d, _, _)| format!("{y:04}-{mo:02}-{d:02}")).unwrap_or_else(|| crate::DEFAULT_DATE.to_string());
        let mut parts = self.time.split(':').map(|p| p.trim().parse::<i64>().unwrap_or(0));
        let secs = parts.next().unwrap_or(0) * 3600 + parts.next().unwrap_or(0) * 60 + parts.next().unwrap_or(0);
        (self.date.clone().unwrap_or(today), secs.clamp(0, 86_399))
    }
}

/// The real time, in Unix seconds.
fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// `1234` s as `00:20:34`.
fn hhmmss(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{:02}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
}

/// The folder of a map (`maps/X/global.cfg`) under the OMSI folder `root` (a mod's copy in a
/// content root first, as the game finds it).
pub fn map_dir_of(root: &Path, map: &str) -> PathBuf {
    omsi_cfg::resolve_path(root, map).parent().map(Path::to_path_buf).unwrap_or_default()
}

/// The map a name means (`Lemmental`, `ahlheim 5`, `maps/Grundorf/global.cfg`): its path as a
/// company keeps it (`maps/<folder>/global.cfg`), when the map is installed under `root` (or a
/// content root).
pub fn find_map(root: &Path, name: &str) -> Option<String> {
    let want = map_folder(name.trim());
    let mut dirs = vec![root.join("maps")];
    dirs.extend(omsi_cfg::content_roots().into_iter().map(|r| r.join("maps")));
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let folder = e.file_name().to_string_lossy().to_string();
            if folder.to_lowercase() == want && e.path().join("global.cfg").is_file() {
                return Some(format!("maps/{folder}/global.cfg"));
            }
        }
    }
    None
}

/// A game mode by its name.
pub fn game_mode_of(s: &str) -> Result<GameMode> {
    match s.trim().to_ascii_lowercase().as_str() {
        "casual" | "easy" | "let" => Ok(GameMode::Casual),
        "" | "normal" => Ok(GameMode::Normal),
        "realistic" | "hard" | "hård" => Ok(GameMode::Realistic),
        other => anyhow::bail!("'{other}' is no game mode (casual, normal, realistic)"),
    }
}

/// The `server.cfg` of a map's world: the dedicated server running `map` on `port` and
/// `web_port`, its companies from the main server's folder `data`, on its clock, stopping by
/// itself `world_idle` minutes after its last player.
pub fn world_cfg(cfg: &MainCfg, data: &Path, map: &str, port: u16, web_port: u16) -> String {
    // (the map's folder as it is written: "Lemmental", "Ahlheim 5")
    let name = map.replace('\\', "/").trim_end_matches("/global.cfg").rsplit('/').next().unwrap_or(map).to_string();
    let b = |v: bool| if v { "1" } else { "0" };
    format!(
        "# Bus Company Online: the world of {map}, written by the main server at each start\n\
         name = Bus Company Online - {name}\n\
         motd = Bus Company Online\n\
         map = {map}\n\
         date =\n\
         time = 06:00\n\
         weather =\n\
         traffic = {traffic}\n\
         timetable = 1\n\
         passengers = {passengers}\n\
         port = {port}\n\
         web_port = {web_port}\n\
         max_players = {max_players}\n\
         tunnel = {tunnel}\n\
         radius = 0\n\
         admin_password = {admin}\n\
         real_time = 1\n\
         share_positions = {share}\n\
         share_timetable = 1\n\
         companies = 1\n\
         company_dir = {data}\n\
         idle_stop = {idle}\n",
        traffic = cfg.traffic,
        passengers = b(cfg.passengers),
        max_players = cfg.max_players,
        tunnel = b(cfg.tunnel),
        admin = cfg.admin_password,
        share = b(cfg.share_positions),
        data = data.display(),
        idle = cfg.world_idle,
    )
}

/// The ports of the `k`-th world (UDP, web).
pub fn ports_of(first: u16, k: usize) -> (u16, u16) {
    let p = first as usize + 2 * k;
    (p as u16, (p + 1) as u16)
}

/// A map's world the main server started.
pub struct World {
    /// The map (`maps/<folder>/global.cfg`).
    pub map: String,
    pub slot: usize,
    pub port: u16,
    pub web_port: u16,
    pub started: Instant,
    child: Child,
}

/// The main server: the companies of its folder on its clock, and the maps' worlds it started.
pub struct MainServer {
    pub data: PathBuf,
    pub cfg: MainCfg,
    pub clock: ServerClock,
    pub hosts: Vec<Host>,
    /// The companies looked at (opened, or left: another map not installed, would not open).
    seen: HashSet<String>,
    /// Each company's last error, said once.
    errors: HashMap<String, String>,
    pub worlds: Vec<World>,
    /// The folder taken for this main server, as long as it runs.
    _lock: std::fs::File,
}

impl MainServer {
    /// The main server of the folder `data`: the folder taken, the clock set or taken up again,
    /// every company opened on it (caught up with the time the server was off, planned, saved),
    /// and the worlds told to take the plans and the clock in.
    pub fn open(data: &Path, cfg: MainCfg) -> Result<MainServer> {
        let lock = online::lock_folder(data)?;
        let real = unix_now();
        let clock = online::server_clock(data, real, cfg.real_time, cfg.clock_start())?;
        let mut m = MainServer { data: data.to_path_buf(), cfg, clock, hosts: Vec::new(), seen: HashSet::new(), errors: HashMap::new(), worlds: Vec::new(), _lock: lock };
        m.open_new();
        online::bump_plans(data)?;
        Ok(m)
    }

    /// The companies of the folder not looked at yet, opened on the server's clock. How many.
    pub fn open_new(&mut self) -> usize {
        let mut n = 0;
        for id in company_ids(&self.data) {
            if !self.seen.insert(id.clone()) {
                continue;
            }
            let c = match store::load(&self.data, &id) {
                Ok(c) => c,
                Err(e) => {
                    warn!("company {id}: not opened: {e:#}");
                    continue;
                }
            };
            let dir = map_dir_of(&self.cfg.root, &c.map);
            if !dir.join("global.cfg").is_file() {
                warn!("company {id}: its map {} is not installed under {} - left in the folder", c.map, self.cfg.root.display());
                continue;
            }
            match Host::open(&self.data, &id, unix_now(), Some(&dir), Some(self.clock)) {
                Ok((h, away)) => {
                    if let Some(a) = away.as_ref() {
                        let result: i64 = a.reports.iter().map(|r| r.result).sum();
                        info!("company {id}: caught up the time the server was off - {} days closed, a result of {:.2}", a.reports.len(), result as f64 / 100.0);
                    }
                    info!("company {id} ({}, {}, {}): {} lines, {} buses, {} staff", h.company.name, map_folder(&h.company.map), h.company.game_mode.label(), h.company.lines.len(), h.company.fleet.len(), h.company.staff.len());
                    self.hosts.push(h);
                    n += 1;
                }
                Err(e) => warn!("company {id}: not opened: {e:#}"),
            }
        }
        n
    }

    /// Every company a step on (`Host::tick`); whether a plan was written (the worlds told).
    pub fn step(&mut self) -> bool {
        let mut planned = false;
        for h in self.hosts.iter_mut() {
            let id = h.company.id.clone();
            match h.tick(unix_now()) {
                Ok(s) => {
                    self.errors.remove(&id);
                    for d in &s.closed {
                        info!("company {id}: the day {} closed - {} of {} tours run, {} km, a result of {:.2}", d.date, d.covered, d.tours, d.km.round(), d.result as f64 / 100.0);
                    }
                    if s.planned {
                        info!("company {id}: the plan of {} written", h.company.date);
                        planned = true;
                    }
                }
                Err(e) => {
                    let text = format!("{e:#}");
                    if self.errors.get(&id) != Some(&text) {
                        warn!("company {id}: {text}");
                        self.errors.insert(id, text);
                    }
                }
            }
        }
        if planned {
            let _ = online::bump_plans(&self.data);
        }
        planned
    }

    /// The folder looked through: companies founded since taken on (the worlds told).
    pub fn scan(&mut self) -> usize {
        let n = self.open_new();
        if n > 0 {
            let _ = online::bump_plans(&self.data);
        }
        n
    }

    /// A new company founded on the server: `name`, on the map `map` (a name `find_map` knows)
    /// for good, in the game mode `mode`; opened at once. Its id.
    pub fn found(&mut self, name: &str, map: &str, mode: &str) -> Result<String> {
        anyhow::ensure!(!name.trim().is_empty(), "the company needs a name");
        let game_mode = game_mode_of(mode)?;
        let map = find_map(&self.cfg.root, map).with_context(|| format!("no map '{map}' under {}", self.cfg.root.display()))?;
        let f = Founding { name: name.trim().to_string(), map: map.clone(), map_name: map_folder(&map), game_mode, ..Default::default() };
        let id = online::found_on_server(&self.data, &f, "owner", self.clock, unix_now())?;
        self.scan();
        Ok(id)
    }

    /// The world of `map`, started if it is not running (on the first free ports). An error: no
    /// such map, all `max_worlds` running, or the game program would not start.
    pub fn start_world(&mut self, map: &str) -> Result<&World> {
        let map = find_map(&self.cfg.root, map).with_context(|| format!("no map '{map}' under {}", self.cfg.root.display()))?;
        if let Some(k) = self.worlds.iter().position(|w| map_folder(&w.map) == map_folder(&map)) {
            return Ok(&self.worlds[k]);
        }
        let slot = (0..self.cfg.max_worlds).find(|k| !self.worlds.iter().any(|w| w.slot == *k)).with_context(|| format!("{} worlds run already (max_worlds)", self.cfg.max_worlds))?;
        let (port, web_port) = ports_of(self.cfg.first_port, slot);
        let dir = self.data.join("worlds").join(map_folder(&map));
        std::fs::create_dir_all(&dir).with_context(|| format!("cannot make {}", dir.display()))?;
        let cfg_path = dir.join("server.cfg");
        let data = std::path::absolute(&self.data).unwrap_or_else(|_| self.data.clone());
        std::fs::write(&cfg_path, world_cfg(&self.cfg, &data, &map, port, web_port)).with_context(|| format!("cannot write {}", cfg_path.display()))?;
        let game = self.cfg.game.clone().or_else(crate::game_program).context("no game program to run the world in (game in company-server.cfg)")?;
        let out = std::fs::File::create(dir.join("world.out")).ok();
        let mut cmd = Command::new(&game);
        cmd.arg("--server").arg(&cfg_path).arg("--root").arg(&self.cfg.root).stdin(Stdio::null());
        match out.and_then(|f| Some((f.try_clone().ok()?, f))) {
            Some((a, b)) => {
                cmd.stdout(a).stderr(b);
            }
            None => {
                cmd.stdout(Stdio::null()).stderr(Stdio::null());
            }
        }
        let child = cmd.spawn().with_context(|| format!("starting {}", game.display()))?;
        info!("world of {map} started: UDP {port}, web {web_port} ({})", dir.display());
        self.worlds.push(World { map, slot, port, web_port, started: Instant::now(), child });
        Ok(self.worlds.last().expect("pushed"))
    }

    /// The world of `map` stopped. Whether one ran.
    pub fn stop_world(&mut self, map: &str) -> bool {
        let want = map_folder(map);
        let Some(k) = self.worlds.iter().position(|w| map_folder(&w.map) == want) else { return false };
        let mut w = self.worlds.remove(k);
        let _ = w.child.kill();
        let _ = w.child.wait();
        info!("world of {} stopped", w.map);
        true
    }

    /// The worlds that ended by themselves (no players for `world_idle` minutes, or a fault)
    /// let go.
    pub fn reap_worlds(&mut self) {
        self.worlds.retain_mut(|w| match w.child.try_wait() {
            Ok(Some(status)) => {
                info!("world of {} ended ({status}) after {} min", w.map, w.started.elapsed().as_secs() / 60);
                false
            }
            _ => true,
        });
    }

    /// Every company saved (and the worlds stopped): the server goes down.
    pub fn shutdown(&mut self) {
        let maps: Vec<String> = self.worlds.iter().map(|w| w.map.clone()).collect();
        for m in maps {
            self.stop_world(&m);
        }
        let real = unix_now();
        for h in self.hosts.iter_mut() {
            match h.save(real) {
                Ok(()) => info!("company {}: saved ({} {})", h.company.id, h.company.date, hhmmss(h.company.clock.minute as f64 * 60.0)),
                Err(e) => warn!("company {}: not saved: {e:#}", h.company.id),
            }
        }
    }

    /// What `status` says.
    pub fn status(&self) -> String {
        let (date, secs) = self.clock.at_real(unix_now() as f64);
        let mut out = format!("server's clock: {date} {}\ncompanies: {}\n", hhmmss(secs), self.hosts.len());
        for h in &self.hosts {
            let c = &h.company;
            out += &format!("  {} \"{}\" on {} ({}): {} {}, cash {:.2}, {} lines, {} buses, {} staff\n", c.id, c.name, map_folder(&c.map), c.game_mode.label(), c.date, hhmmss(c.clock.minute as f64 * 60.0), c.cash as f64 / 100.0, c.lines.len(), c.fleet.len(), c.staff.len());
        }
        out += &format!("worlds: {} of at most {}\n", self.worlds.len(), self.cfg.max_worlds);
        for w in &self.worlds {
            out += &format!("  {}: UDP {}, web {}, for {} min\n", map_folder(&w.map), w.port, w.web_port, w.started.elapsed().as_secs() / 60);
        }
        out
    }
}

pub const HELP: &str = "\
help                          this list
status                        the clock, the companies, the worlds
start <map>                   start a map's world (Lemmental, Ahlheim 5, ...)
stop <map>                    stop a map's world
found <name>; <map>; [mode]   found a company on a map (mode: casual, normal, realistic)
quit                          save every company, stop the worlds and end";

/// What the console's line `line` does; the answer to print. None: `quit`.
pub fn command(m: &mut MainServer, line: &str) -> Option<String> {
    let line = line.trim();
    let (cmd, rest) = line.split_once(char::is_whitespace).map(|(a, b)| (a, b.trim())).unwrap_or((line, ""));
    Some(match cmd.to_ascii_lowercase().as_str() {
        "" => String::new(),
        "help" | "?" => HELP.to_string(),
        "status" => m.status(),
        "start" => match m.start_world(rest) {
            Ok(w) => format!("world of {}: UDP {}, web {}", map_folder(&w.map), w.port, w.web_port),
            Err(e) => format!("not started: {e:#}"),
        },
        "stop" => {
            if m.stop_world(rest) {
                format!("world of {rest} stopped")
            } else {
                format!("no world of {rest} runs")
            }
        }
        "found" => {
            let parts: Vec<&str> = rest.split(';').map(str::trim).collect();
            match (parts.first().filter(|p| !p.is_empty()), parts.get(1).filter(|p| !p.is_empty())) {
                (Some(name), Some(map)) => match m.found(name, map, parts.get(2).copied().unwrap_or("")) {
                    Ok(id) => format!("founded: {id}"),
                    Err(e) => format!("not founded: {e:#}"),
                },
                _ => "found <name>; <map>; [mode]".to_string(),
            }
        }
        "quit" | "exit" => return None,
        other => format!("'{other}' is no command - help lists them"),
    })
}

/// Ctrl+C or a stop from the system asked the main server to end.
static STOP: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
extern "C" fn on_signal(_: libc::c_int) {
    STOP.store(true, Ordering::SeqCst);
}

/// Ctrl+C (and SIGTERM) end the main server the way `quit` does. (On Windows: `quit`; the
/// companies are saved every minute in any case.)
fn take_signals() {
    #[cfg(unix)]
    // SAFETY: the handler only stores into an atomic
    unsafe {
        libc::signal(libc::SIGINT, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t);
        libc::signal(libc::SIGTERM, on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t);
    }
}

/// The main server program: the folder `dir` (its settings, companies and worlds), run until
/// `quit` or Ctrl+C.
pub fn run(dir: &Path) -> Result<()> {
    let cfg = MainCfg::load(dir)?;
    *LOG.lock().unwrap_or_else(|e| e.into_inner()) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("company-server.log")).ok();
    // (the maps as the game finds them: its content folder - mods, a server's own TTData - first)
    if let Some(c) = omsi_cfg::content_dir() {
        omsi_cfg::add_content_root(c);
    }
    omsi_cfg::add_content_root(cfg.root.clone());
    let mut m = MainServer::open(dir, cfg)?;
    take_signals();
    println!("Bus Company Online - main server ({})", dir.display());
    print!("{}", m.status());
    println!("type help for the commands");
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::Builder::new().name("console".into()).spawn(move || {
        let stdin = std::io::stdin();
        let mut line = String::new();
        while stdin.read_line(&mut line).map(|n| n > 0).unwrap_or(false) {
            if tx.send(std::mem::take(&mut line)).is_err() {
                return;
            }
        }
    })?;
    let (mut next, mut scan, mut reap) = (Instant::now() + STEP, Instant::now() + SCAN, Instant::now());
    'run: while !STOP.load(Ordering::SeqCst) {
        while let Ok(line) = rx.try_recv() {
            match command(&mut m, &line) {
                Some(answer) if !answer.is_empty() => println!("{}", answer.trim_end()),
                Some(_) => {}
                None => break 'run,
            }
        }
        let now = Instant::now();
        if now >= reap {
            reap = now + Duration::from_secs(1);
            m.reap_worlds();
        }
        if now >= scan {
            scan = now + SCAN;
            m.scan();
        }
        if now >= next {
            next = now + STEP;
            m.step();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    println!("saving the companies and stopping the worlds ...");
    m.shutdown();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("openomsi-main-server-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// An OMSI folder with two maps (no timetables: their companies have no lines yet).
    fn omsi_with_maps(dir: &Path) -> PathBuf {
        let root = dir.join("OMSI 2");
        for m in ["Lemmental", "Ahlheim 5"] {
            std::fs::create_dir_all(root.join("maps").join(m)).unwrap();
            std::fs::write(root.join("maps").join(m).join("global.cfg"), "").unwrap();
        }
        root
    }

    #[test]
    fn the_settings_are_written_and_read() {
        let dir = temp("cfg");
        let c = MainCfg::load(&dir).unwrap();
        assert!(dir.join(CFG_FILE).is_file());
        assert!(c.real_time && c.first_port == 27015 && c.max_worlds == 4 && c.world_idle == 10 && !c.tunnel);
        let c = MainCfg::parse("root = /srv/OMSI 2\nreal_time = 0\ndate = 2024-03-04\ntime = 07:30\nmax_worlds = 2\ngame = /opt/omsi-hub-game\n");
        assert_eq!(c.root, PathBuf::from("/srv/OMSI 2"));
        assert_eq!(c.clock_start(), ("2024-03-04".to_string(), 7 * 3600 + 1800));
        assert_eq!((c.max_worlds, c.game.clone()), (2, Some(PathBuf::from("/opt/omsi-hub-game"))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A world's server.cfg: its map and ports, the companies from the main server's folder on
    /// its clock, the timetable on, and its own end when nobody drives.
    #[test]
    fn a_worlds_settings() {
        let c = MainCfg::parse("traffic = 12\nworld_idle = 15\ntunnel = 1\n");
        let t = world_cfg(&c, Path::new("/srv/bco"), "maps/Lemmental/global.cfg", 27017, 27018);
        for want in ["map = maps/Lemmental/global.cfg", "port = 27017", "web_port = 27018", "companies = 1", "company_dir = /srv/bco", "real_time = 1", "timetable = 1", "traffic = 12", "tunnel = 1", "idle_stop = 15", "radius = 0"] {
            assert!(t.lines().any(|l| l.trim() == want), "{want} in\n{t}");
        }
        assert_eq!((ports_of(27015, 0), ports_of(27015, 3)), ((27015, 27016), (27021, 27022)));
    }

    /// Companies on two maps on one main server, on its clock; founded from the console; a map
    /// that is not installed refused; one main server per folder.
    #[test]
    fn every_map_on_one_main_server() {
        let dir = temp("maps");
        let root = omsi_with_maps(&dir);
        let data = dir.join("bco");
        let cfg = MainCfg { root: root.clone(), ..MainCfg::parse("real_time = 0\ndate = 2024-03-04\ntime = 06:00\n") };
        let mut m = MainServer::open(&data, cfg.clone()).unwrap();
        assert!(MainServer::open(&data, cfg.clone()).is_err(), "one main server per folder");
        assert_eq!(find_map(&root, "lemmental").as_deref(), Some("maps/Lemmental/global.cfg"));
        assert_eq!(find_map(&root, "maps/Ahlheim 5/global.cfg").as_deref(), Some("maps/Ahlheim 5/global.cfg"));
        assert!(find_map(&root, "Grundorf").is_none());
        let gen = online::plans_gen(&data);
        let a = command(&mut m, "found Lemtal Trafik; Lemmental; casual").unwrap();
        let b = command(&mut m, "found Bergbus; lemmental").unwrap();
        let c = command(&mut m, "found Lemtal Ahlheim; Ahlheim 5; normal").unwrap();
        assert!(a.starts_with("founded:") && b.starts_with("founded:") && c.starts_with("founded:"), "{a} / {b} / {c}");
        assert!(command(&mut m, "found X; Grundorf").unwrap().contains("no map"));
        assert!(command(&mut m, "found X; Lemmental; sandbox").unwrap().contains("no game mode"));
        assert!(command(&mut m, "found ; Lemmental").unwrap().starts_with("found <name>"));
        assert_eq!(m.hosts.len(), 3);
        assert!(online::plans_gen(&data) > gen, "the worlds told");
        let maps: HashSet<String> = m.hosts.iter().map(|h| map_folder(&h.company.map)).collect();
        assert_eq!(maps, ["lemmental", "ahlheim 5"].map(String::from).into_iter().collect());
        assert!(m.hosts.iter().all(|h| h.company.date == "2024-03-04" && h.company.online.on_server));
        m.step();
        let s = command(&mut m, "status").unwrap();
        assert!(s.contains("companies: 3") && s.contains("lemtal-trafik") && s.contains("worlds: 0"), "{s}");
        assert!(command(&mut m, "stop Lemmental").unwrap().contains("no world"));
        assert!(command(&mut m, "fly").unwrap().contains("no command"));
        assert!(command(&mut m, "testcopy x").unwrap().contains("no command"), "nothing of a player's own companies comes along");
        assert!(world_cfg(&m.cfg, &data, "maps/Ahlheim 5/global.cfg", 1, 2).contains("name = Bus Company Online - Ahlheim 5"));
        assert!(command(&mut m, "quit").is_none());
        m.shutdown();
        drop(m);
        // started again: every company back
        let m = MainServer::open(&data, cfg).unwrap();
        assert_eq!(m.hosts.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
