//! Bus Company Online's main server (NexiconDK, phase 1 of docs/omsihub/BUS_COMPANY_ONLINE.md):
//! `omsi-hub-cli --company-server <folder>`, a program without a window, a map or graphics that
//! runs around the clock on the server's machine.
//!
//! It keeps every bus company of every map in its folder (`<folder>/companies`), each on its map
//! for good, on the server's clock (`online::ServerClock`, kept in `<folder>/clock.json`). Each
//! company runs in a process of its own (`omsi-hub-game --company-host`, phase 2: its boss
//! without a window - the launcher's session with the company open, shared with "Play together"
//! on the company's own port under a code that stays the same, its time run by `online::Host`):
//! the players come to it from their launchers with its code and use its pages as their roles
//! allow, its founder with every permission. The main server starts the processes, starts one
//! again that ended (after `HOLD`), takes on a company founded meanwhile, and tells them to quit
//! (each saves its company) when it ends.
//!
//! The players drive in a **world per map**: the dedicated server (`omsi-hub-game --server`) with
//! `companies = 1`, which runs the map with every company's buses in their paint on the server's
//! clock and reports their trips into the folder. The main server starts a map's world when it is
//! asked to (`start_world`; phase 1: the console's `start <map>`), each on ports of its own; a world
//! stops by itself a while after its last player has gone (`world_idle`). A map without a world:
//! its companies' trips are worked out at the day's close, as Bus Company does for the days not
//! played.
//!
//! The players come in through the server's door (phase 2, step 2: `server_api`, on `api_port`,
//! reached with the server's code that `status` shows): "Bus Company Online" in their launchers
//! logs them in with their Omsi-Hub accounts or their Steam IDs (`server_players`, kept in
//! `<folder>/players.json`, each with money of his own), lists the server's companies, joins one
//! with its code and the server's ticket for him, and founds one on a map from his money.
//!
//! The settings are `<folder>/company-server.cfg` (written with the defaults and a comment for
//! every key when it is missing); the console takes `help`, `status`, `maps`, `start`, `stop`,
//! `found`, `role`, `players`, `money` and `quit`. A company on the server is always founded
//! there, new: nothing of a player's own companies comes along.

use super::model::GameMode;
use super::online::{self, company_ids, map_folder, ServerClock};
use super::server_players::{Logins, Players};
use super::{store, Founding};
use anyhow::{Context, Result};
use std::collections::HashMap;
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

/// The main server's settings in its folder.
pub const CFG_FILE: &str = "company-server.cfg";

pub const DEFAULT_CFG: &str = "\
# Omsi-Hub: Bus Company Online, the main server
# (key = value; lines starting with # are comments)

# the server's name, as the players' launchers show it
name = Bus Company Online

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

# the companies' own ports, from this one on (the players' launchers come to a company there:
# \"Join a company\" with its code - status shows it). tunnel below starts a free Cloudflare
# tunnel for each company and world, for players across the internet
first_company_port = 27045

# the door for the players' launchers (\"Bus Company Online\" with the server's code - status
# shows it): its port, the ways to log in (account: an Omsi-Hub account, email or Google;
# steam: the Steam ID of the player's computer and a key the server gives it), the money a new
# player starts with and what founding a company costs (whole units of money)
api_port = 27040
logins = account steam
start_money = 20000
found_price = 10000

# what each world runs (as server.cfg's keys of the same names)
traffic = 30
passengers = 1
max_players = 16
tunnel = 0
share_positions = 1
admin_password =
";

/// The players' door's part of the settings (phase 2, step 2), added to a `company-server.cfg`
/// written before it had them.
pub const DOOR_CFG: &str = "
# the door for the players' launchers (\"Bus Company Online\" with the server's code - status
# shows it): its port, the ways to log in (account: an Omsi-Hub account, email or Google;
# steam: the Steam ID of the player's computer and a key the server gives it), the money a new
# player starts with and what founding a company costs (whole units of money)
api_port = 27040
logins = account steam
start_money = 20000
found_price = 10000
";

/// What `company-server.cfg` says (see `DEFAULT_CFG`).
#[derive(Debug, Clone, PartialEq)]
pub struct MainCfg {
    pub name: String,
    pub root: PathBuf,
    pub real_time: bool,
    pub date: Option<String>,
    pub time: String,
    pub game: Option<PathBuf>,
    pub first_port: u16,
    pub max_worlds: usize,
    pub world_idle: u32,
    pub first_company_port: u16,
    pub api_port: u16,
    pub logins: Logins,
    /// Cents.
    pub start_money: i64,
    pub found_price: i64,
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
        let mut text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        // (written before the players' door: its settings added, with their comment)
        if !text.lines().any(|l| l.trim_start().starts_with("api_port")) {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&path) {
                let _ = f.write_all(DOOR_CFG.as_bytes());
            }
            text.push_str(DOOR_CFG);
        }
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
            name: text("name").unwrap_or_else(|| "Bus Company Online".into()),
            root: text("root").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(crate::load_config().root)),
            real_time: flag("real_time", true),
            date: text("date"),
            time: text("time").unwrap_or_else(|| "06:00".into()),
            game: text("game").map(PathBuf::from),
            first_port: num("first_port", 27015).clamp(1, 65000) as u16,
            max_worlds: num("max_worlds", 4).clamp(1, 64) as usize,
            world_idle: num("world_idle", 10).clamp(0, 24 * 60) as u32,
            first_company_port: num("first_company_port", 27045).clamp(1, 65000) as u16,
            api_port: num("api_port", 27040).clamp(1, 65000) as u16,
            logins: Logins::parse(&text("logins").unwrap_or_default()),
            start_money: num("start_money", 20000).max(0) * 100,
            found_price: num("found_price", 10000).max(0) * 100,
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
    installed_maps(root).into_iter().find(|f| f.to_lowercase() == want).map(|f| format!("maps/{f}/global.cfg"))
}

/// The maps installed under `root` and the content roots (their folders, `Ahlheim 5`), sorted.
pub fn installed_maps(root: &Path) -> Vec<String> {
    let mut dirs = vec![root.join("maps")];
    dirs.extend(omsi_cfg::content_roots().into_iter().map(|r| r.join("maps")));
    let mut out: Vec<String> = Vec::new();
    for d in dirs {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let folder = e.file_name().to_string_lossy().to_string();
            if e.path().join("global.cfg").is_file() && !out.iter().any(|f| f.eq_ignore_ascii_case(&folder)) {
                out.push(folder);
            }
        }
    }
    out.sort_by_key(|f| f.to_lowercase());
    out
}

/// A map's folder as it is written (`maps/Ahlheim 5/global.cfg`: `Ahlheim 5`).
pub fn map_name(map: &str) -> String {
    map.replace('\\', "/").trim_end_matches("/global.cfg").rsplit('/').next().unwrap_or(map).to_string()
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
    let name = map_name(map);
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

/// A company's own process (`omsi-hub-game --company-host`): its boss on the server - the
/// launcher's session with the company open, shared with "Play together" on the company's port,
/// its time run on the server's clock (`omsi_launcher_api::omsihub::company_host`).
pub struct CompanyProc {
    pub id: String,
    pub port: u16,
    pub started: Instant,
    child: Child,
    stdin: Option<std::process::ChildStdin>,
}

/// The main server: the companies of its folder on its clock (each in a process of its own),
/// and the maps' worlds it started.
pub struct MainServer {
    pub data: PathBuf,
    pub cfg: MainCfg,
    pub clock: ServerClock,
    pub companies: Vec<CompanyProc>,
    /// A company whose process could not start or ended: not started again before this.
    held: HashMap<String, Instant>,
    /// What was said of a company that is not run (its map not installed), said once.
    said: HashMap<String, String>,
    pub worlds: Vec<World>,
    /// The players who logged in through the door (`server_api`), as `players.json` keeps them.
    pub players: Players,
    /// The server's code, its door open (`server_api::server_code`).
    pub code: Option<String>,
    /// The folder taken for this main server, as long as it runs.
    _lock: std::fs::File,
}

/// How long a company whose process ended waits before it is started again.
pub const HOLD: Duration = Duration::from_secs(30);

impl MainServer {
    /// The main server of the folder `data`: the folder taken, the clock set or taken up again,
    /// the launcher settings its companies' processes read (the OMSI folder, the game), the
    /// worlds told to take the plans and the clock in, and every company's process started.
    pub fn open(data: &Path, cfg: MainCfg) -> Result<MainServer> {
        let lock = online::lock_folder(data)?;
        let real = unix_now();
        let clock = online::server_clock(data, real, cfg.real_time, cfg.clock_start())?;
        let game = cfg.game.clone().or_else(crate::game_program).map(|g| g.to_string_lossy().to_string()).unwrap_or_default();
        let launcher = crate::Config { root: cfg.root.to_string_lossy().to_string(), game, profile: SERVER_PROFILE.to_string() };
        std::fs::write(data.join("launcher.json"), serde_json::to_vec_pretty(&launcher)?).with_context(|| format!("cannot write {}", data.join("launcher.json").display()))?;
        // (the tickets' secret: the companies' processes check the players with it)
        online::server_secret(data)?;
        let players = Players::load(data)?;
        let mut m = MainServer { data: data.to_path_buf(), cfg, clock, companies: Vec::new(), held: HashMap::new(), said: HashMap::new(), worlds: Vec::new(), players, code: None, _lock: lock };
        online::bump_plans(data)?;
        m.scan();
        Ok(m)
    }

    /// A line for the console and the log.
    pub fn say(&self, text: &str) {
        info!("{text}");
    }

    pub fn say_warning(&self, text: &str) {
        warn!("{text}");
    }

    /// The folder's own, absolute (the processes are handed it).
    fn data_abs(&self) -> PathBuf {
        std::path::absolute(&self.data).unwrap_or_else(|_| self.data.clone())
    }

    /// Every company of the folder whose process does not run (and is not held) started - one on
    /// a map that is not installed left in the folder. How many were started.
    pub fn scan(&mut self) -> usize {
        let mut n = 0;
        for id in company_ids(&self.data) {
            if self.companies.iter().any(|p| p.id == id) || self.held.get(&id).is_some_and(|t| Instant::now() < *t) {
                continue;
            }
            let why = match store::load(&self.data, &id) {
                Ok(c) if map_dir_of(&self.cfg.root, &c.map).join("global.cfg").is_file() => None,
                Ok(c) => Some(format!("its map {} is not installed under {} - left in the folder", c.map, self.cfg.root.display())),
                Err(e) => Some(format!("not opened: {e:#}")),
            };
            if let Some(why) = why {
                if self.said.get(&id) != Some(&why) {
                    warn!("company {id}: {why}");
                    self.said.insert(id.clone(), why);
                }
                continue;
            }
            self.said.remove(&id);
            match self.start_company(&id) {
                Ok(()) => n += 1,
                Err(e) => {
                    warn!("company {id}: its process would not start: {e:#}");
                    self.held.insert(id, Instant::now() + HOLD);
                }
            }
        }
        n
    }

    /// Company `id`'s own process started: its session kept (port, code), its output to
    /// `<folder>/hosts/<id>.out`.
    fn start_company(&mut self, id: &str) -> Result<()> {
        let kept = online::kept_session(&self.data, id, self.cfg.first_company_port)?;
        let game = self.cfg.game.clone().or_else(crate::game_program).context("no game program to run the company in (game in company-server.cfg)")?;
        let dir = self.data.join("hosts");
        std::fs::create_dir_all(&dir).with_context(|| format!("cannot make {}", dir.display()))?;
        let out = std::fs::OpenOptions::new().create(true).append(true).open(dir.join(format!("{id}.out"))).ok();
        let mut cmd = Command::new(&game);
        cmd.arg("--company-host").arg(self.data_abs()).arg("--company").arg(id).arg("--root").arg(&self.cfg.root).stdin(Stdio::piped());
        if !self.cfg.tunnel {
            cmd.env("OMSI_NO_TUNNEL", "1");
        }
        match out.and_then(|f| Some((f.try_clone().ok()?, f))) {
            Some((a, b)) => {
                cmd.stdout(a).stderr(b);
            }
            None => {
                cmd.stdout(Stdio::null()).stderr(Stdio::null());
            }
        }
        let mut child = cmd.spawn().with_context(|| format!("starting {}", game.display()))?;
        let stdin = child.stdin.take();
        info!("company {id}: its process started (players' port {})", kept.port);
        self.companies.push(CompanyProc { id: id.to_string(), port: kept.port, started: Instant::now(), child, stdin });
        Ok(())
    }

    /// The companies' processes that ended let go (started again after `HOLD`).
    pub fn reap_companies(&mut self) {
        let held = &mut self.held;
        self.companies.retain_mut(|p| match p.child.try_wait() {
            Ok(Some(status)) => {
                warn!("company {}: its process ended ({status}) - started again in {} s", p.id, HOLD.as_secs());
                held.insert(p.id.clone(), Instant::now() + HOLD);
                false
            }
            _ => true,
        });
    }

    /// A line for company `id`'s process (its console: `role …`). Whether it went.
    pub fn tell_company(&mut self, id: &str, line: &str) -> bool {
        use std::io::Write;
        let Some(p) = self.companies.iter_mut().find(|p| p.id == id) else { return false };
        p.stdin.as_mut().is_some_and(|s| writeln!(s, "{line}").and_then(|_| s.flush()).is_ok())
    }

    /// Every company's process told to quit (it saves its company), waited for a while, the rest
    /// ended.
    pub fn stop_companies(&mut self) {
        let ids: Vec<String> = self.companies.iter().map(|p| p.id.clone()).collect();
        for id in &ids {
            self.tell_company(id, "quit");
        }
        let until = Instant::now() + Duration::from_secs(20);
        for p in self.companies.iter_mut() {
            p.stdin = None;
            loop {
                match p.child.try_wait() {
                    Ok(Some(_)) => break,
                    _ if Instant::now() >= until => {
                        warn!("company {}: its process did not end - stopped", p.id);
                        let _ = p.child.kill();
                        let _ = p.child.wait();
                        break;
                    }
                    _ => std::thread::sleep(Duration::from_millis(100)),
                }
            }
            info!("company {}: stopped", p.id);
        }
        self.companies.clear();
    }

    /// A new company founded on the server: `name`, on the map `map` (a name `find_map` knows)
    /// for good, in the game mode `mode`, its owner the player called `owner` (his launcher's name:
    /// the first of that name to come is the owner); its process started at once. Its id.
    pub fn found(&mut self, name: &str, map: &str, mode: &str, owner: &str) -> Result<String> {
        self.found_as(name, map, mode, owner, "")
    }

    /// `found`, its owner's id given (empty: the first player of his name).
    fn found_as(&mut self, name: &str, map: &str, mode: &str, owner: &str, owner_id: &str) -> Result<String> {
        anyhow::ensure!(!name.trim().is_empty(), "the company needs a name");
        let game_mode = game_mode_of(mode)?;
        let map = find_map(&self.cfg.root, map).with_context(|| format!("no map '{map}' installed - maps lists them"))?;
        let f = Founding { name: name.trim().to_string(), map: map.clone(), map_name: map_name(&map), game_mode, ..Default::default() };
        let id = online::found_on_server(&self.data, &f, SERVER_PROFILE, self.clock, unix_now())?;
        // (its owner set before its process starts: the process keeps the company)
        if !owner.trim().is_empty() {
            let mut c = store::load(&self.data, &id)?;
            c.online.owner = owner.trim().to_string();
            c.online.owner_id = owner_id.to_string();
            store::save(&self.data, &c)?;
        }
        info!("company {id} ({}, {}, {}) founded{}", name.trim(), map_name(&map), game_mode.label(), if owner.trim().is_empty() { String::new() } else { format!(", its owner {}", owner.trim()) });
        self.held.remove(&id);
        self.scan();
        Ok(id)
    }

    /// A new company founded on the server by its player `player` (the server's id for him) from
    /// his own money (`found_price`): he its owner from the start (`Online::owner_id`). Its id.
    pub fn found_for(&mut self, player: &str, name: &str, map: &str, mode: &str) -> Result<String> {
        let p = self.players.get(player).cloned().context("no such player")?;
        anyhow::ensure!(p.money >= self.cfg.found_price, "{} has not money enough", p.name);
        let id = self.found_as(name, map, mode, &p.name, &p.id)?;
        let price = self.cfg.found_price;
        let _ = self.players.pay(&p.id, price);
        self.players.save(&self.data)?;
        info!("company {id}: founded by {} ({}) for {:.2} - {:.2} left", p.name, p.id, price as f64 / 100.0, (p.money - price) as f64 / 100.0);
        Ok(id)
    }

    /// The world of `map`, started if it is not running (on the first free ports). An error: no
    /// such map, all `max_worlds` running, or the game program would not start.
    pub fn start_world(&mut self, map: &str) -> Result<&World> {
        let map = find_map(&self.cfg.root, map).with_context(|| format!("no map '{map}' installed - maps lists them"))?;
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

    /// The worlds stopped and every company's process told to quit (each saves its company): the
    /// server goes down.
    pub fn shutdown(&mut self) {
        let maps: Vec<String> = self.worlds.iter().map(|w| w.map.clone()).collect();
        for m in maps {
            self.stop_world(&m);
        }
        self.stop_companies();
    }

    /// What `status` says.
    pub fn status(&self) -> String {
        let (date, secs) = self.clock.at_real(unix_now() as f64);
        let ids = company_ids(&self.data);
        let mut out = format!("server's clock: {date} {}\n", hhmmss(secs));
        match &self.code {
            Some(code) => out += &format!("server's code: {code} (the players' launchers: Bus Company Online; port {})\n", self.cfg.api_port),
            None => out += "server's code: none - its door is not open\n",
        }
        out += &format!("players: {}\ncompanies: {}\n", self.players.players.len(), ids.len());
        for id in ids {
            let Ok(c) = store::load(&self.data, &id) else { continue };
            let state = match self.companies.iter().find(|p| p.id == id) {
                Some(p) => format!("runs, port {}", p.port),
                None if self.said.contains_key(&id) => "not run (its map)".to_string(),
                None => "waits to start".to_string(),
            };
            let owner = if c.online.owner.is_empty() { String::new() } else { format!(", owner {}", c.online.owner) };
            out += &format!("  {id} \"{}\" on {} ({}{owner}): {} {}, cash {:.2}, {} lines, {} buses, {} staff - {state}\n", c.name, map_name(&c.map), c.game_mode.label(), c.date, hhmmss(c.clock.minute as f64 * 60.0), c.cash as f64 / 100.0, c.lines.len(), c.fleet.len(), c.staff.len());
            if let Ok(code) = std::fs::read_to_string(online::code_file(&self.data, &id)) {
                out += &format!("    code: {}\n", code.trim());
            }
        }
        out += &format!("worlds: {} of at most {}\n", self.worlds.len(), self.cfg.max_worlds);
        for w in &self.worlds {
            out += &format!("  {}: UDP {}, web {} (the launcher's server list: <this machine>:{}), for {} min\n", map_folder(&w.map), w.port, w.web_port, w.web_port, w.started.elapsed().as_secs() / 60);
        }
        out
    }
}

/// The profile the server's companies are founded under (`Company::profile`), and the one its
/// companies' processes read their companies as.
pub const SERVER_PROFILE: &str = "owner";

pub const HELP: &str = "\
help                          this list
status                        the clock, the companies, the worlds
maps                          the maps installed (by their folders' names)
start <map>                   start a map's world (Lemmental, Ahlheim 5, ...)
stop <map>                    stop a map's world
found <name>; <map>; [mode]; [owner]
                              found a company on a map (mode: casual, normal, realistic; owner:
                              the player's name in his launcher - he gets every permission)
role <company>; <player>; <role>
                              give a player of a company a role (driver, planner, manager, owner)
owner <company>; <player>      make a player of the server a company's owner (every permission)
players                       the players who logged in, with their money
money <player>; <amount>      set a player's money (+500 or -500: add or take)
quit                          stop the worlds, save every company and end";

/// What the console's line `line` does; the answer to print. None: `quit`.
pub fn command(m: &mut MainServer, line: &str) -> Option<String> {
    let line = line.trim();
    let (cmd, rest) = line.split_once(char::is_whitespace).map(|(a, b)| (a, b.trim())).unwrap_or((line, ""));
    Some(match cmd.to_ascii_lowercase().as_str() {
        "" => String::new(),
        "help" | "?" => HELP.to_string(),
        "status" => m.status(),
        "maps" => {
            let maps = installed_maps(&m.cfg.root);
            if maps.is_empty() {
                format!("no maps under {}", m.cfg.root.display())
            } else {
                maps.join("\n")
            }
        }
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
                (Some(name), Some(map)) => match m.found(name, map, parts.get(2).copied().unwrap_or(""), parts.get(3).copied().unwrap_or("")) {
                    Ok(id) => format!("founded: {id} - its code comes in status when its process runs"),
                    Err(e) => format!("not founded: {e:#}"),
                },
                _ => "found <name>; <map>; [mode]; [owner]".to_string(),
            }
        }
        "role" => match rest.split_once(';') {
            Some((company, who)) if !company.trim().is_empty() && who.contains(';') => {
                if m.tell_company(company.trim(), &format!("role {}", who.trim())) {
                    format!("told {} (its log: hosts\\{}.out)", company.trim(), company.trim())
                } else {
                    format!("company {} does not run", company.trim())
                }
            }
            _ => "role <company>; <player>; <role>".to_string(),
        },
        "owner" => match rest.split_once(';') {
            Some((company, who)) if !company.trim().is_empty() => match m.players.find(who).cloned() {
                Some(p) => {
                    if m.tell_company(company.trim(), &format!("owner {}; {}", p.id, p.name)) {
                        format!("told {}: {} is its owner (its log: hosts\\{}.out)", company.trim(), p.name, company.trim())
                    } else {
                        format!("company {} does not run", company.trim())
                    }
                }
                None => format!("no player '{}' - players lists them (he logs in from Bus Company Online first)", who.trim()),
            },
            _ => "owner <company>; <player>".to_string(),
        },
        "players" => {
            if m.players.players.is_empty() {
                "no player has logged in yet".to_string()
            } else {
                m.players
                    .players
                    .iter()
                    .map(|p| {
                        let owns: Vec<String> = company_ids(&m.data).into_iter().filter(|id| store::load(&m.data, id).is_ok_and(|c| c.online.owner_id == p.id)).collect();
                        let by = match (p.account.is_empty(), p.steam.is_empty()) {
                            (false, false) => "account and Steam",
                            (false, true) => "account",
                            _ => "Steam",
                        };
                        format!("  {} ({}, {by}): money {:.2}{}", p.name, p.id, p.money as f64 / 100.0, if owns.is_empty() { String::new() } else { format!(", owns {}", owns.join(", ")) })
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        "money" => match rest.split_once(';') {
            Some((who, amount)) => {
                let amount = amount.trim();
                let cents = amount.trim_start_matches(['+', '-']).replace([' ', ','], "").parse::<f64>().ok().map(|v| (v * 100.0).round() as i64);
                let id = m.players.find(who).map(|p| p.id.clone());
                match (id, cents) {
                    (Some(id), Some(c)) => {
                        let p = m.players.get_mut(&id).expect("found");
                        p.money = match amount.chars().next() {
                            Some('+') => p.money + c,
                            Some('-') => (p.money - c).max(0),
                            _ => c,
                        };
                        let said = format!("{} has {:.2} now", p.name, p.money as f64 / 100.0);
                        match m.players.save(&m.data) {
                            Ok(()) => said,
                            Err(e) => format!("{said} - not saved: {e:#}"),
                        }
                    }
                    (None, _) => format!("no player '{}' - players lists them", who.trim()),
                    (_, None) => format!("'{amount}' is no amount"),
                }
            }
            None => "money <player>; <amount>".to_string(),
        },
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

/// The main server, held (the door's threads hold it too).
fn lock(m: &std::sync::Mutex<MainServer>) -> std::sync::MutexGuard<'_, MainServer> {
    m.lock().unwrap_or_else(|e| e.into_inner())
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
    let (api_port, tunnel) = (cfg.api_port, cfg.tunnel);
    let m = std::sync::Arc::new(std::sync::Mutex::new(MainServer::open(dir, cfg)?));
    // (the players' door: the server's code, its tunnel)
    let kept = super::server_api::kept(dir, api_port)?;
    let api = std::sync::Arc::new(super::server_api::Api::new(m.clone(), kept.secret_bytes(), super::server_api::omsihub_verify()));
    let door = match super::server_api::open(api, &kept, tunnel) {
        Ok(d) => {
            lock(&m).code = super::server_api::server_code(&kept).map(|c| c.encode());
            Some(d)
        }
        Err(e) => {
            warn!("the players' door did not open on port {api_port}: {e} - nobody logs in from the launcher (api_port in {CFG_FILE})");
            None
        }
    };
    take_signals();
    println!("Bus Company Online - main server ({})", dir.display());
    print!("{}", lock(&m).status());
    println!("type help for the commands");
    let mut told_tunnel: Option<String> = None;
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
    let (mut scan, mut reap) = (Instant::now() + Duration::from_secs(5), Instant::now());
    'run: while !STOP.load(Ordering::SeqCst) {
        while let Ok(line) = rx.try_recv() {
            match command(&mut lock(&m), &line) {
                Some(answer) if !answer.is_empty() => println!("{}", answer.trim_end()),
                Some(_) => {}
                None => break 'run,
            }
        }
        let now = Instant::now();
        if now >= reap {
            reap = now + Duration::from_secs(1);
            let mut g = lock(&m);
            g.reap_worlds();
            g.reap_companies();
            drop(g);
            if let Some(super::together::server::TunnelState::Ready(url)) = door.as_ref().map(|d| d.tunnel()) {
                if told_tunnel.as_ref() != Some(&url) {
                    info!("the players' door is on the internet too: {url} (the server's code finds it)");
                    told_tunnel = Some(url);
                }
            }
        }
        if now >= scan {
            scan = now + Duration::from_secs(5);
            lock(&m).scan();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    println!("stopping the worlds and saving the companies ...");
    drop(door);
    lock(&m).shutdown();
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
        // (the players' door: its defaults, and its part added to a file written before it)
        assert_eq!((c.api_port, c.logins, c.start_money, c.found_price, c.name.as_str()), (27040, Logins::default(), 2_000_000, 1_000_000, "Bus Company Online"));
        assert!(DEFAULT_CFG.contains(DOOR_CFG.trim()), "the door's part is the defaults'");
        std::fs::write(dir.join(CFG_FILE), "root = /srv\nfirst_company_port = 27145\n").unwrap();
        let old = MainCfg::load(&dir).unwrap();
        assert_eq!((old.first_company_port, old.api_port), (27145, 27040));
        let text = std::fs::read_to_string(dir.join(CFG_FILE)).unwrap();
        assert!(text.contains("api_port = 27040") && text.contains("found_price = 10000"));
        MainCfg::load(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(CFG_FILE)).unwrap(), text, "added once");
        let c = MainCfg::parse("start_money = 500\nfound_price = 0\nlogins = steam\napi_port = 0\n");
        assert_eq!((c.start_money, c.found_price, c.logins.account, c.api_port), (50_000, 0, false, 1));
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
        // (no game program: the companies' processes do not start here, they wait)
        let cfg = MainCfg { root: root.clone(), game: Some(dir.join("no-game")), ..MainCfg::parse("real_time = 0\ndate = 2024-03-04\ntime = 06:00\n") };
        let mut m = MainServer::open(&data, cfg.clone()).unwrap();
        assert!(MainServer::open(&data, cfg.clone()).is_err(), "one main server per folder");
        let launcher: crate::Config = serde_json::from_str(&std::fs::read_to_string(data.join("launcher.json")).unwrap()).unwrap();
        assert_eq!((PathBuf::from(&launcher.root), launcher.profile.as_str()), (root.clone(), SERVER_PROFILE), "what the companies' processes read");
        assert_eq!(find_map(&root, "lemmental").as_deref(), Some("maps/Lemmental/global.cfg"));
        assert_eq!(find_map(&root, "maps/Ahlheim 5/global.cfg").as_deref(), Some("maps/Ahlheim 5/global.cfg"));
        assert!(find_map(&root, "Grundorf").is_none());
        assert_eq!(command(&mut m, "maps").unwrap(), "Ahlheim 5\nLemmental");
        let a = command(&mut m, "found Lemtal Trafik; Lemmental; casual; Mathias").unwrap();
        let b = command(&mut m, "found Bergbus; lemmental").unwrap();
        let c = command(&mut m, "found Lemtal Ahlheim; Ahlheim 5; normal").unwrap();
        assert!(a.starts_with("founded:") && b.starts_with("founded:") && c.starts_with("founded:"), "{a} / {b} / {c}");
        assert!(command(&mut m, "found X; Grundorf").unwrap().contains("no map"));
        assert!(command(&mut m, "found X; Lemmental; sandbox").unwrap().contains("no game mode"));
        assert!(command(&mut m, "found ; Lemmental").unwrap().starts_with("found <name>"));
        let ids = company_ids(&data);
        assert_eq!(ids.len(), 3);
        let all: Vec<crate::company::Company> = ids.iter().map(|id| store::load(&data, id).unwrap()).collect();
        let maps: std::collections::HashSet<String> = all.iter().map(|c| map_folder(&c.map)).collect();
        assert_eq!(maps, ["lemmental", "ahlheim 5"].map(String::from).into_iter().collect());
        assert!(all.iter().all(|c| c.date == "2024-03-04" && c.online.on_server && c.profile == SERVER_PROFILE));
        assert_eq!(store::load(&data, "lemtal-trafik").unwrap().online.owner, "Mathias");
        // (the processes would not start: held, said in status)
        assert!(m.companies.is_empty());
        let s = command(&mut m, "status").unwrap();
        assert!(s.contains("companies: 3") && s.contains("lemtal-trafik") && s.contains("owner Mathias") && s.contains("waits to start") && s.contains("worlds: 0"), "{s}");
        assert!(command(&mut m, "role lemtal-trafik; Jonas; planner").unwrap().contains("does not run"));
        assert!(command(&mut m, "role lemtal-trafik").unwrap().starts_with("role <company>"));
        assert!(command(&mut m, "stop Lemmental").unwrap().contains("no world"));
        assert!(command(&mut m, "fly").unwrap().contains("no command"));
        assert!(command(&mut m, "testcopy x").unwrap().contains("no command"), "nothing of a player's own companies comes along");
        assert!(world_cfg(&m.cfg, &data, "maps/Ahlheim 5/global.cfg", 1, 2).contains("name = Bus Company Online - Ahlheim 5"));
        assert!(command(&mut m, "quit").is_none());
        m.shutdown();
        drop(m);
        // started again: the companies there, their sessions (ports) kept apart
        let m = MainServer::open(&data, cfg).unwrap();
        assert_eq!(company_ids(&m.data).len(), 3);
        let ports: std::collections::HashSet<u16> = ids.iter().map(|id| online::kept_session(&data, id, 27045).unwrap().port).collect();
        assert_eq!(ports.len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A company's process started, told and ended (a stand-in for the game program: `sh`, which
    /// reads its console and quits on it). (Unix: the release's tests run on Windows without it.)
    #[cfg(unix)]
    #[test]
    fn a_companys_process_is_started_told_and_stopped() {
        let dir = temp("procs");
        let root = omsi_with_maps(&dir);
        let data = dir.join("bco");
        let game = dir.join("fake-game.sh");
        std::fs::write(&game, "#!/bin/sh\nwhile read line; do echo \"got $line\"; [ \"$line\" = quit ] && exit 0; done\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&game, std::fs::Permissions::from_mode(0o755)).unwrap();
        let cfg = MainCfg { root, game: Some(game), ..MainCfg::parse("real_time = 0\ndate = 2024-03-04\n") };
        let mut m = MainServer::open(&data, cfg).unwrap();
        command(&mut m, "found Lemtal Trafik; Lemmental; casual; Mathias").unwrap();
        assert_eq!(m.companies.len(), 1);
        assert!(command(&mut m, "status").unwrap().contains("runs, port 27045"));
        assert!(command(&mut m, "role lemtal-trafik; Jonas; planner").unwrap().starts_with("told"));
        m.shutdown();
        assert!(m.companies.is_empty());
        let out = std::fs::read_to_string(data.join("hosts").join("lemtal-trafik.out")).unwrap();
        assert!(out.contains("got role Jonas; planner") && out.contains("got quit"), "{out}");
        let _ = std::fs::remove_dir_all(&dir);
    }

}
