//! Bus Company Online's main server, its door for the players' launchers (NexiconDK, phase 2
//! step 2 of docs/omsihub/BUS_COMPANY_ONLINE.md): plain HTTP on the server's `api_port`
//! (`together::server::Server::start_at`, `POST /bco`), and - with `tunnel = 1` - a Cloudflare
//! quick tunnel in front of it, posted on the game's relay under the server's session as a
//! company's is.
//!
//! The players reach it with the **server's code** (`HUB-…`, the company codes' format: its
//! secret, the session of its key, its port and the machine's addresses; its company tag
//! `SERVER_TAG` tells it from a company's), which `status` shows: "Bus Company Online" in the
//! launcher takes it, tries the addresses and then the tunnel (`find_server`), and from then on
//! posts its questions there (`ask`), each with the code's secret:
//!
//! * `info` - the server's name, clock, maps (with their companies) and what founding costs;
//! * `login` - with his Omsi-Hub account, his Steam ID and key, or both (`server_players`): a
//!   session (a day, longer while it is used), the player, and a new key for this computer's
//!   Steam ID when it was given one;
//! * `companies` - every company of the server, with his role in each;
//! * `join` - a company's code and the server's ticket for him (`online::ticket`): his launcher
//!   joins the company with them as "Play together" does, no code to type;
//! * `found` - a company founded on a map from his own money (`found_price`), he its owner -
//!   (phase 6) once he has done what founding asks (`server_found`: trips, the leadership course,
//!   a driving test); `course` books the course, `test` a driving test (`class`; none: the one
//!   booked called off);
//! * `world` - (phase 3) the world of a company's map for one of its players, started when it
//!   does not run: its ports, its tunnel, and whether it answers yet - his game joins it to
//!   drive a duty of the company.
//!
//! Every answer is JSON, `{ok: true, …}` or `{ok: false, error}`, the error in English (a key of
//! the launcher's translations, with its numbers beside it).

use super::main_server::{find_map, game_mode_of, MainServer};
use super::online::{self, company_ids, map_folder, KeptSession};
use super::server_players::{Account, Login, Player};
use super::together::code::{self, CompanyCode};
use super::store;
use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// The version of the door's questions (raised when they change shape).
pub const API_VERSION: u8 = 1;
/// Where the door is (`POST`), and what a `GET` there says.
pub const PATH: &str = "/bco";
pub const HELLO: &str = r#"{"app":"bus-company-online","v":1}"#;
/// The server's kept session: the code's secret and key (`KeptSession`, its port the API's).
pub const SESSION_FILE: &str = "api.session";
/// How long a login lasts without a question (s).
pub const SESSION_FOR: i64 = 24 * 3600;

/// The company tag of a server's code (a company's code has its company's).
pub fn server_tag() -> u16 {
    code::tag_of("bus-company-online")
}

/// Why a question was turned away (English: keys of the translations).
pub const BAD_SECRET: &str = "The server's code is no longer valid: ask the server's owner for its code.";
pub const NEWER: &str = "The server runs another version of Omsi-Hub: both need the same version.";
pub const UNKNOWN: &str = "The server did not understand that: both need the same version of Omsi-Hub.";
pub const LOGGED_OUT: &str = "Your login on the server has run out: log in again.";
pub const NO_COMPANY: &str = "The server has no such company.";
pub const STARTING: &str = "The company is starting on the server: try again in a moment.";
pub const NO_NAME: &str = "Give the company a name.";
pub const NO_MAP: &str = "That map is not installed on the server.";
pub const NO_MODE: &str = "That is no game mode.";
pub const NOT_ENOUGH: &str = "Founding a company costs %{price}: you have %{money}.";
pub const NOT_FOUNDED: &str = "The server could not found the company: it says why in its log.";
pub const UNREACHED: &str = "The server does not answer. Is it running? Over the internet its tunnel must be up (tunnel = 1), or its port open.";
pub const NOT_MEMBER: &str = "You are not a player of this company: join it first.";
pub const NO_WORLD: &str = "The server could not start the map's world: it says why in its log.";

/// Every text of the door's (for the translations' test).
pub fn texts() -> Vec<&'static str> {
    let mut v = vec![BAD_SECRET, NEWER, UNKNOWN, LOGGED_OUT, NO_COMPANY, STARTING, NO_NAME, NO_MAP, NO_MODE, NOT_ENOUGH, NOT_FOUNDED, UNREACHED, NOT_MEMBER, NO_WORLD];
    v.extend(super::server_players::texts());
    v.extend(super::server_found::texts());
    v
}

/// A question of a launcher.
#[derive(Deserialize, Default, Debug)]
#[serde(default)]
pub struct Ask {
    pub v: u8,
    /// The server's code's secret (hex).
    pub secret: String,
    #[serde(rename = "do")]
    pub what: String,
    /// The session `login` gave.
    pub session: String,
    /// `login`'s.
    #[serde(flatten)]
    pub login: Login,
    /// `join`'s company; `found`'s company name, map and game mode (the login's `name` is his).
    pub company: String,
    pub map: String,
    pub mode: String,
    /// (Phase 6) `test`'s class of bus (`career::LicenceClass`: midi, solo, articulated,
    /// double); empty: the test booked called off.
    pub class: String,
    /// (Phase 7) `hq`'s and `hq_do`'s group (its head office's id), `hq_do`'s act and its
    /// arguments (`server_hq::act`; `found`: `map`, `start`).
    pub group: String,
    pub act: String,
    pub args: Value,
}

/// (NexiconDK, 31 October 2026) A player of company `cid` (`me`) comes into it: the world of its
/// map started now if it does not run, so that it has loaded by the time he drives - a big map
/// loads for minutes (Ahlheim six), and a single trip taken meanwhile had left before it ran
/// ("This trip leaves too soon"). Only for a member of the company; nothing said if it runs.
fn warm_world(m: &mut MainServer, cid: &str, me: &Player) {
    let Ok(c) = store::load(&m.data, cid) else { return };
    let member = c.online.owner_id == me.id || c.together.guests.iter().any(|g| g.id == me.id && !g.banned);
    if !member || m.worlds.iter().any(|w| map_folder(&w.map) == map_folder(&c.map)) {
        return;
    }
    match m.start_world(&c.map) {
        Ok(_) => m.say(&format!("world of {}: started as {} ({}) came into {}", super::main_server::map_name(&c.map), me.name, me.id, c.name)),
        Err(e) => m.say_warning(&format!("the world of {} for {}: {e:#}", super::main_server::map_name(&c.map), me.name)),
    }
}

fn refusal(text: &str) -> Value {
    json!({ "ok": false, "error": text })
}

/// A refusal with its numbers (`server_found::Why`): money (`price`, `money`, whole units) beside
/// the error as `NOT_ENOUGH`'s are, the rest under `args`.
fn refusal_with(w: super::server_found::Why) -> Value {
    let mut v = refusal(w.0);
    let mut args = serde_json::Map::new();
    for (k, x) in w.1 {
        match (k.as_str(), x.parse::<i64>()) {
            ("price" | "money", Ok(n)) => v[k.as_str()] = json!(n),
            _ => {
                args.insert(k, json!(x));
            }
        }
    }
    v["args"] = Value::Object(args);
    v
}

/// (Phase 7) The head office `group` for player `me` to run: a group of the server's, and his.
fn runs_group(m: &MainServer, me: &Player, group: &str) -> std::result::Result<super::model::Company, &'static str> {
    let gid = group.trim();
    if !super::server_hq::is_group_id(gid) {
        return Err(NO_COMPANY);
    }
    let g = store::load(&m.data, gid).map_err(|_| NO_COMPANY)?;
    if !super::server_hq::may_run(&g, &me.id) {
        return Err(super::server_hq::NOT_YOURS);
    }
    Ok(g)
}

/// (Phase 7) What a head office's page asks of the main server: which depots run, the price of
/// a depot and the maps a new one may go on, the server's time.
fn around(m: &MainServer, g: &super::model::Company) -> super::server_hq::Around {
    let (_, secs) = m.clock.at_real(unix_now() as f64);
    let s = secs as i64;
    super::server_hq::Around {
        running: m.companies.iter().filter(|p| online::code_file(&m.data, &p.id).is_file()).map(|p| p.id.clone()).collect(),
        depot_price: m.cfg.depot_price,
        maps: super::server_depots::maps_for(&m.data, &m.cfg.root, g),
        time: format!("{:02}:{:02}", s / 3600, (s / 60) % 60),
        bus_move: (m.cfg.bus_move_price, m.cfg.bus_move_days),
        players: m.players.players.iter().map(|p| (p.id.clone(), p.name.clone())).collect(),
    }
}

/// The server's kept session in its folder (made the first time: a new secret and key; its port
/// the API's as the settings have it).
pub fn kept(data: &Path, port: u16) -> Result<KeptSession> {
    let path = data.join(SESSION_FILE);
    if let Some(mut k) = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str::<KeptSession>(&t).ok()).filter(|k| k.host_key().is_some()) {
        k.port = port;
        return Ok(k);
    }
    let (_, pkcs8) = omsi_net::hostkey::HostKey::generate_kept().context("no key for the server's session")?;
    let secret: [u8; 8] = omsi_net::http::random_bytes();
    let k = KeptSession { port, secret: omsi_net::hostkey::hex(&secret), key: omsi_net::hostkey::hex(&pkcs8) };
    std::fs::create_dir_all(data).with_context(|| format!("cannot make {}", data.display()))?;
    std::fs::write(&path, serde_json::to_string_pretty(&k)?).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(k)
}

/// The server's code: where its door is (its port, the machine's addresses), its session (for
/// the tunnel) and its secret.
pub fn server_code(k: &KeptSession) -> Option<CompanyCode> {
    let key = k.host_key()?;
    let ips: Vec<std::net::Ipv4Addr> = omsi_net::code_ipv4s().into_iter().filter(|ip| !ip.is_loopback()).collect();
    Some(CompanyCode { secret: k.secret_bytes(), session: key.session(), version: code::VERSION, port: k.port, company: server_tag(), ips })
}

/// What checks an Omsi-Hub account's access token: its account, or why not (English).
pub type Verify = Box<dyn Fn(&str) -> std::result::Result<Account, &'static str> + Send + Sync>;

/// The real check: Omsi-Hub online's (Supabase's) word on the token.
pub fn omsihub_verify() -> Verify {
    Box::new(|token: &str| match omsihub_online::verify_access(&omsihub_online::Endpoints::supabase(), token) {
        Ok(v) => Ok(Account { user_id: v.user_id, name: v.display_name }),
        Err(omsihub_online::Error::Offline) => Err(super::server_players::ACCOUNT_OFFLINE),
        Err(_) => Err(super::server_players::ACCOUNT_REFUSED),
    })
}

/// The door: the main server it answers for, its secret, the players' sessions and the check of
/// an account.
pub struct Api {
    pub server: Arc<Mutex<MainServer>>,
    secret: [u8; 8],
    sessions: Mutex<HashMap<String, (String, i64)>>,
    verify: Verify,
}

/// The real time, in Unix seconds.
fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Cents as the launcher shows them (whole units: `20 000`).
fn whole(cents: i64) -> i64 {
    cents / 100
}

/// The server's day now (its clock's date).
fn server_day(m: &MainServer) -> String {
    m.clock.at_real(unix_now() as f64).0
}

/// (Phase 6) What founding asks of him, and how far he is (`server_found::status_json`).
fn founding(m: &MainServer, p: &Player) -> Value {
    super::server_found::status_json(p, &m.cfg.found, &server_day(m))
}

/// A player as his launcher shows him.
pub fn player_json(p: &Player) -> Value {
    json!({ "id": p.id, "name": p.name, "money": whole(p.money), "account": !p.account.is_empty(), "steam": !p.steam.is_empty() })
}

impl Api {
    pub fn new(server: Arc<Mutex<MainServer>>, secret: [u8; 8], verify: Verify) -> Api {
        Api { server, secret, sessions: Mutex::new(HashMap::new()), verify }
    }

    fn server(&self) -> std::sync::MutexGuard<'_, MainServer> {
        self.server.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The player of a session, which lasts on (`SESSION_FOR` from now).
    fn player_of(&self, session: &str, now: i64) -> Option<String> {
        let mut g = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        g.retain(|_, (_, until)| *until > now);
        let (id, until) = g.get_mut(session.trim())?;
        *until = now + SESSION_FOR;
        Some(id.clone())
    }

    /// The answer to a launcher's question (its body: an `Ask` as JSON), at `now`.
    pub fn answer(&self, body: &str, now: i64) -> Value {
        let Ok(ask) = serde_json::from_str::<Ask>(body) else { return refusal(UNKNOWN) };
        if ask.v != API_VERSION {
            return refusal(NEWER);
        }
        let want: String = self.secret.iter().map(|b| format!("{b:02x}")).collect();
        let got = ask.secret.trim().to_ascii_lowercase();
        if got.len() != want.len() || got.bytes().zip(want.bytes()).fold(0u8, |a, (x, y)| a | (x ^ y)) != 0 {
            return refusal(BAD_SECRET);
        }
        match ask.what.as_str() {
            "info" => self.info(),
            "login" => self.login(&ask, now),
            "companies" | "join" | "found" | "me" | "world" | "course" | "test" | "hq" | "hq_do" => {
                let Some(id) = self.player_of(&ask.session, now) else { return refusal(LOGGED_OUT) };
                match ask.what.as_str() {
                    "companies" => self.companies(&id),
                    "join" => self.join(&id, &ask.company),
                    "found" => self.found(&id, &ask),
                    "world" => self.world(&id, &ask.company),
                    "course" => self.course(&id),
                    "test" => self.test(&id, &ask.class, now),
                    "hq" => self.hq(&id, &ask.group),
                    "hq_do" => self.hq_do(&id, &ask, now),
                    _ => self.me(&id),
                }
            }
            _ => refusal(UNKNOWN),
        }
    }

    fn info(&self) -> Value {
        let m = self.server();
        let (date, secs) = m.clock.at_real(unix_now() as f64);
        let ids = company_ids(&m.data);
        let mut per_map: HashMap<String, usize> = HashMap::new();
        for id in &ids {
            if let Ok(c) = store::load(&m.data, id) {
                *per_map.entry(map_folder(&c.map).to_lowercase()).or_default() += 1;
            }
        }
        let maps: Vec<Value> = super::main_server::installed_maps(&m.cfg.root).into_iter().map(|f| json!({ "folder": f, "companies": per_map.get(&f.to_lowercase()).copied().unwrap_or(0) })).collect();
        let s = secs as i64;
        json!({
            "ok": true, "app": "bus-company-online", "v": API_VERSION, "name": m.cfg.name,
            "date": date, "time": format!("{:02}:{:02}", s / 3600, (s / 60) % 60),
            "maps": maps, "companies": ids.len(), "players": m.players.players.len(),
            "found_price": whole(m.cfg.found_price), "start_money": whole(m.cfg.start_money),
            "logins": { "account": m.cfg.logins.account, "steam": m.cfg.logins.steam },
        })
    }

    fn login(&self, ask: &Ask, now: i64) -> Value {
        // (the account checked before the server is held: it goes round the world)
        let account = (!ask.login.account.trim().is_empty()).then(|| (self.verify)(ask.login.account.trim()));
        let mut m = self.server();
        let (logins, start) = (m.cfg.logins, m.cfg.start_money);
        let r = m.players.login(&ask.login, account, logins, start, now);
        let done = match r {
            Ok(done) => done,
            Err(e) => return refusal(e),
        };
        if let Err(e) = m.players.save(&m.data) {
            m.say_warning(&format!("the players not saved: {e:#}"));
        }
        if done.new {
            m.say(&format!("player {} ({}) came for the first time ({})", done.player.name, done.player.id, if done.player.account.is_empty() { "Steam" } else if done.player.steam.is_empty() { "Omsi-Hub account" } else { "Omsi-Hub account and Steam" }));
        }
        drop(m);
        let b: [u8; 16] = omsi_net::http::random_bytes();
        let session = omsi_net::hostkey::hex(&b);
        self.sessions.lock().unwrap_or_else(|e| e.into_inner()).insert(session.clone(), (done.player.id.clone(), now + SESSION_FOR));
        json!({ "ok": true, "session": session, "player": player_json(&done.player), "new": done.new, "key": done.key, "note": done.note })
    }

    fn me(&self, id: &str) -> Value {
        let m = self.server();
        match m.players.get(id) {
            Some(p) => json!({ "ok": true, "player": player_json(p), "found": founding(&m, p) }),
            None => refusal(LOGGED_OUT),
        }
    }

    /// (Phase 6) The leadership course booked from his money.
    fn course(&self, id: &str) -> Value {
        let mut m = self.server();
        let (needs, today) = (m.cfg.found, server_day(&m));
        let Some(p) = m.players.get_mut(id) else { return refusal(LOGGED_OUT) };
        if let Err(w) = super::server_found::book_course(p, &needs, &today) {
            return refusal_with(w);
        }
        let (name, until) = (p.name.clone(), p.course_until.clone());
        if let Err(e) = m.players.save(&m.data) {
            m.say_warning(&format!("the players not saved: {e:#}"));
        }
        m.say(&format!("{name} is on the leadership course until {until}"));
        drop(m);
        self.me(id)
    }

    /// (Phase 6) A driving test booked for a class of bus (`class`), or the one booked called off.
    fn test(&self, id: &str, class: &str, now: i64) -> Value {
        let mut m = self.server();
        let Some(p) = m.players.get_mut(id) else { return refusal(LOGGED_OUT) };
        let said = if class.trim().is_empty() {
            super::server_found::cancel_test(p);
            format!("{}: driving test called off", p.name)
        } else {
            match super::server_found::book_test(p, class, now.max(0) as u64) {
                Ok(c) => format!("{}: driving test booked ({})", p.name, c.label()),
                Err(w) => return refusal_with(w),
            }
        };
        if let Err(e) = m.players.save(&m.data) {
            m.say_warning(&format!("the players not saved: {e:#}"));
        }
        m.say(&said);
        drop(m);
        self.me(id)
    }

    fn companies(&self, id: &str) -> Value {
        let m = self.server();
        let Some(me) = m.players.get(id).cloned() else { return refusal(LOGGED_OUT) };
        let mut list = Vec::new();
        let all: Vec<super::model::Company> = company_ids(&m.data).into_iter().filter_map(|cid| store::load(&m.data, &cid).ok()).collect();
        // (phase 7: the groups' head offices - no map, the overview of their depots)
        let groups: Vec<super::model::Company> = super::server_hq::group_ids(&m.data).into_iter().filter_map(|gid| store::load(&m.data, &gid).ok()).collect();
        for g in &groups {
            let mine = g.online.owner_id == me.id;
            list.push(json!({
                "id": g.id, "name": g.name, "short": g.short, "colours": g.colours, "map": "", "mode": g.game_mode, "date": g.date,
                "owner": g.online.owner, "mine": mine, "role": mine.then_some("co_owner"), "runs": true, "players": 0,
                "lines": 0, "buses": 0, "staff": 0, "hq": "", "hq_name": Value::Null, "line_list": [],
                "group": true, "may_run": super::server_hq::may_run(g, &me.id),
            }));
        }
        // (the lines' colours the line editor gave them on each map, read once a map)
        let mut map_colours: std::collections::HashMap<String, std::collections::HashMap<String, String>> = std::collections::HashMap::new();
        for c in &all {
            let cid = c.id.clone();
            // (phase 4: a depot of a head office on another map - shown under it)
            let hq_name = (!c.online.hq.is_empty()).then(|| all.iter().chain(groups.iter()).find(|x| x.id == c.online.hq).map(|x| x.name.clone()).unwrap_or_default());
            let runs = m.companies.iter().any(|p| p.id == cid) && online::code_file(&m.data, &cid).is_file();
            let guest = c.together.guests.iter().find(|g| g.id == me.id);
            let mine = c.online.owner_id == me.id;
            list.push(json!({
                "id": c.id, "name": c.name, "short": c.short, "colours": c.colours, "map": super::main_server::map_name(&c.map),
                "mode": c.game_mode, "date": c.date, "owner": c.online.owner, "mine": mine,
                "role": if mine { Some("co_owner") } else { guest.map(|g| g.role.key()) },
                "runs": runs, "players": c.together.guests.iter().filter(|g| !g.banned).count(),
                "lines": c.lines.len(), "buses": c.fleet.len(), "staff": c.staff.len(),
                "hq": c.online.hq, "hq_name": hq_name,
                "line_list": line_badges(c, map_colours.entry(c.map.clone()).or_insert_with(|| crate::lines::map_line_colours(&c.map))),
            }));
        }
        // (his own first, then by name - a depot right under its head office)
        let head = |c: &Value| if c["hq"].as_str().unwrap_or("").is_empty() { c["name"].as_str().unwrap_or("").to_lowercase() } else { format!("{}\u{1}{}", c["hq_name"].as_str().unwrap_or("").to_lowercase(), c["name"].as_str().unwrap_or("").to_lowercase()) };
        list.sort_by_key(|c| (!c["mine"].as_bool().unwrap_or(false), c["role"].is_null(), head(c)));
        json!({ "ok": true, "player": player_json(&me), "companies": list, "found": founding(&m, &me) })
    }

    /// (Phase 7) Head office `group`'s page for its owner (`server_hq::view`).
    fn hq(&self, id: &str, group: &str) -> Value {
        let m = self.server();
        let Some(me) = m.players.get(id).cloned() else { return refusal(LOGGED_OUT) };
        let g = match runs_group(&m, &me, group) {
            Ok(g) => g,
            Err(e) => return refusal(e),
        };
        json!({ "ok": true, "hq": super::server_hq::view(&m.data, &g, &me.id, &around(&m, &g)) })
    }

    /// (Phase 7) What the head office's owner does on its page (`server_hq::act`; `found`: a
    /// depot founded from its treasury on `map` with `start`, its process started at once): kept,
    /// said in the log, the page again.
    fn hq_do(&self, id: &str, ask: &Ask, now: i64) -> Value {
        let mut m = self.server();
        let Some(me) = m.players.get(id).cloned() else { return refusal(LOGGED_OUT) };
        let mut g = match runs_group(&m, &me, &ask.group) {
            Ok(g) => g,
            Err(e) => return refusal(e),
        };
        let done = if ask.act == "found" {
            let map = ask.args.get("map").and_then(Value::as_str).unwrap_or("").to_string();
            let start = ask.args.get("start").and_then(Value::as_i64).unwrap_or(0);
            super::server_depots::found(&m.data, &m.cfg.root, &mut g, &map, start, m.cfg.depot_price, now).map(|d| format!("depot {d} founded on {}", super::main_server::map_name(&map)))
        } else {
            super::server_hq::act(&m.data, &mut g, (&me.id, &me.name), &ask.act, &ask.args, now)
        };
        match done {
            Ok(said) => {
                if let Err(e) = store::save(&m.data, &g) {
                    m.say_warning(&format!("group {}: its head office not saved: {e:#}", g.id));
                    return refusal(super::server_hq::NOT_MADE);
                }
                m.say(&format!("group {} ({}): {said} - by {} ({})", g.id, g.name, me.name, me.id));
                if ask.act == "found" {
                    m.scan();
                }
                json!({ "ok": true, "hq": super::server_hq::view(&m.data, &g, &me.id, &around(&m, &g)) })
            }
            Err(e) => refusal(e),
        }
    }

    fn join(&self, id: &str, company: &str) -> Value {
        let mut m = self.server();
        let Some(me) = m.players.get(id).cloned() else { return refusal(LOGGED_OUT) };
        let cid = company.trim();
        if cid.is_empty() || !company_ids(&m.data).iter().any(|c| c == cid) {
            return refusal(NO_COMPANY);
        }
        let code = std::fs::read_to_string(online::code_file(&m.data, cid)).unwrap_or_default();
        if code.trim().is_empty() || !m.companies.iter().any(|p| p.id == cid) {
            return refusal(STARTING);
        }
        let Some(secret) = online::read_server_secret(&m.data) else { return refusal(STARTING) };
        warm_world(&mut m, cid, &me);
        json!({ "ok": true, "code": code.trim(), "ticket": online::ticket(&secret, cid, &me.id, &me.name), "player": me.id, "name": me.name })
    }

    /// (Phase 3) The world of company `company`'s map for its player `id`: started when it does
    /// not run; whether it answers yet (its web gateway on this machine), its ports, its tunnel.
    fn world(&self, id: &str, company: &str) -> Value {
        let (map, slot, port, web_port, since, running, (phase, expected)) = {
            let mut m = self.server();
            let Some(me) = m.players.get(id).cloned() else { return refusal(LOGGED_OUT) };
            let cid = company.trim();
            if cid.is_empty() || !company_ids(&m.data).iter().any(|c| c == cid) {
                return refusal(NO_COMPANY);
            }
            let Ok(c) = store::load(&m.data, cid) else { return refusal(NO_COMPANY) };
            let member = c.online.owner_id == me.id || c.together.guests.iter().any(|g| g.id == me.id && !g.banned);
            if !member {
                return refusal(NOT_MEMBER);
            }
            let fresh = !m.worlds.iter().any(|w| map_folder(&w.map) == map_folder(&c.map));
            match m.start_world(&c.map) {
                Ok(w) => {
                    let x = (w.map.clone(), w.slot, w.port, w.web_port, w.started.elapsed().as_secs(), w.running(), w.loading());
                    if fresh {
                        m.say(&format!("world of {}: started for {} ({}), to drive for {}", super::main_server::map_name(&c.map), me.name, me.id, c.name));
                    }
                    x
                }
                Err(e) => {
                    m.say_warning(&format!("the world of {} for {}: {e:#}", super::main_server::map_name(&c.map), me.name));
                    return refusal(NO_WORLD);
                }
            }
        };
        // (it answers once it runs - its map loaded, `main_server::WORLD_RUNNING` - and its
        // gateway is up: not held meanwhile)
        let ready = running && omsi_net::ws::query(&format!("http://127.0.0.1:{web_port}"), false).is_ok();
        let status = super::main_server::world_status(slot);
        let tunnel = status.as_ref().and_then(|s| s.get("tunnel")).and_then(Value::as_str).filter(|t| !t.is_empty()).map(str::to_string);
        // (30 October 2026: what it loads, and how long it took the last time - the players'
        // loading screen)
        json!({ "ok": true, "ready": ready, "map": super::main_server::map_name(&map), "port": port, "web_port": web_port, "tunnel": tunnel, "since": since, "phase": phase, "expected": expected })
    }

    fn found(&self, id: &str, ask: &Ask) -> Value {
        let name = ask.company.trim();
        if name.is_empty() {
            return refusal(NO_NAME);
        }
        let mut m = self.server();
        if find_map(&m.cfg.root, &ask.map).is_none() {
            return refusal(NO_MAP);
        }
        if game_mode_of(&ask.mode).is_err() {
            return refusal(NO_MODE);
        }
        let price = m.cfg.found_price;
        let Some(me) = m.players.get(id).cloned() else { return refusal(LOGGED_OUT) };
        // (phase 6: what founding asks besides its price)
        if let Some(w) = super::server_found::missing(&me, &m.cfg.found, &server_day(&m)) {
            return refusal_with(w);
        }
        if me.money < price {
            return json!({ "ok": false, "error": NOT_ENOUGH, "price": whole(price), "money": whole(me.money) });
        }
        match m.found_for(&me.id, name, &ask.map, &ask.mode) {
            Ok(cid) => {
                let money = m.players.get(id).map(|p| p.money).unwrap_or(0);
                json!({ "ok": true, "company": cid, "money": whole(money) })
            }
            Err(e) => {
                m.say_warning(&format!("{} could not found {name}: {e:#}", me.name));
                refusal(NOT_FOUNDED)
            }
        }
    }
}

/// The door of the main server opened on its `api_port` (the server's code: `server_code`), with
/// its tunnel when `tunnel` is on. It closes when dropped.
pub fn open(api: Arc<Api>, k: &KeptSession, tunnel: bool) -> std::io::Result<super::together::server::Server> {
    let a = api.clone();
    let server = super::together::server::Server::start_at(k.port, false, PATH, HELLO, Arc::new(move |body: &str| a.answer(body, unix_now()).to_string()))?;
    if tunnel {
        if let Some(key) = k.host_key() {
            server.open_tunnel(Arc::new(key));
        }
    }
    Ok(server)
}

// --- the launcher's side --------------------------------------------------------------------------

/// How long one question may take (the tunnel goes round the world; a login asks Omsi-Hub online).
pub const TIMEOUT: Duration = Duration::from_secs(20);
/// How long an address of the code is tried.
const TRY_FOR: Duration = Duration::from_secs(3);

/// The address of the door at `base` (`http://ip:port`, `https://…`).
pub fn url_of(base: &str) -> String {
    format!("{}{PATH}", base.trim_end_matches('/'))
}

/// Post `body` to the door at `base`; its answer.
pub fn ask(base: &str, body: &Value, timeout: Duration) -> std::result::Result<Value, String> {
    let text = omsi_net::http::post_json(&url_of(base), &body.to_string(), timeout)?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// Find the server of `code` and post `body` there: where it answered, and its answer. The code's
/// addresses first (all at once), then its tunnel.
pub fn find_server(code: &CompanyCode, body: &Value) -> std::result::Result<(String, Value), String> {
    let bases: Vec<String> = code.addrs().iter().map(|a| format!("http://{a}")).collect();
    if !bases.is_empty() {
        let (tx, rx) = std::sync::mpsc::channel();
        for b in &bases {
            let (tx, b, body) = (tx.clone(), b.clone(), body.clone());
            let _ = std::thread::Builder::new().name("bco find".into()).spawn(move || {
                if let Ok(v) = ask(&b, &body, TRY_FOR) {
                    let _ = tx.send((b, v));
                }
            });
        }
        drop(tx);
        if let Ok(found) = rx.recv_timeout(TRY_FOR + Duration::from_millis(500)) {
            return Ok(found);
        }
    }
    if let Some(url) = omsi_net::bridge::lookup_tunnel(code.session) {
        if let Ok(v) = ask(&url, body, TIMEOUT) {
            return Ok((url, v));
        }
    }
    Err(UNREACHED.to_string())
}

/// (NexiconDK) A company's lines as the server's list shows them, to choose a company by:
/// each its number and colour - the colour the line editor gave it on the map (`map`: by the
/// timetable's name, `lines::map_line_colours`), else the company's own, else the one its
/// pages give it (`rosterflow::line_colour`'s palette).
pub fn line_badges(c: &super::model::Company, map: &std::collections::HashMap<String, String>) -> Vec<Value> {
    c.lines
        .iter()
        .map(|l| {
            let colour = map.get(&l.name.trim().to_lowercase()).cloned().unwrap_or_else(|| super::rosterflow::hex(super::rosterflow::line_colour(c, &l.name)));
            let number = if l.number.trim().is_empty() { l.name.trim() } else { l.number.trim() };
            json!({ "number": number, "colour": colour })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::company::main_server::{MainCfg, MainServer};

    /// (NexiconDK) A company's lines on the server's list: the colour the line editor gave a line
    /// on the map first, else the company's own, else its pages' palette - and its number (its
    /// timetable name when it has none).
    #[test]
    fn a_companys_lines_show_in_the_maps_colours() {
        let mut c = super::super::found(&super::super::Founding { name: "Umove".into(), date: "2024-03-04".into(), ..Default::default() }, "Luc");
        let line = |name: &str, number: &str, colour: &str| super::super::model::CompanyLine { name: name.into(), number: number.into(), colour: colour.into(), ..Default::default() };
        c.lines = vec![line("Linie5", "5", "#112233"), line("Linie7", "7", "#445566"), line("Linie9", "", "")];
        let map: HashMap<String, String> = [("linie5".to_string(), "#e5484d".to_string())].into_iter().collect();
        let b = line_badges(&c, &map);
        assert_eq!(b[0], json!({ "number": "5", "colour": "#e5484d" }), "the map's");
        assert_eq!(b[1], json!({ "number": "7", "colour": "#445566" }), "the company's");
        let palette = super::super::rosterflow::hex(super::super::rosterflow::PALETTE[2]);
        assert_eq!(b[2], json!({ "number": "Linie9", "colour": palette }), "the palette's, by its place");
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("openomsi-bco-api-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn fake_verify() -> Verify {
        Box::new(|t: &str| match t {
            "token-mathias" => Ok(Account { user_id: "u-1".into(), name: "Mathias".into() }),
            "offline" => Err(crate::company::server_players::ACCOUNT_OFFLINE),
            _ => Err(crate::company::server_players::ACCOUNT_REFUSED),
        })
    }

    /// A main server on an OMSI folder with Lemmental, its door with a fake account check.
    fn api(dir: &Path) -> (Api, String) {
        let root = dir.join("omsi");
        std::fs::create_dir_all(root.join("maps/Lemmental")).unwrap();
        std::fs::write(root.join("maps/Lemmental/global.cfg"), "").unwrap();
        let data = dir.join("bco");
        let cfg = MainCfg::parse(&format!("root = {}\nreal_time = 0\ndate = 2024-03-04\ngame = {}\nstart_money = 20000\nfound_price = 15000\n", root.display(), dir.join("no-game").display()));
        let m = MainServer::open(&data, cfg).unwrap();
        let k = kept(&data, 0).unwrap();
        (Api::new(Arc::new(Mutex::new(m)), k.secret_bytes(), fake_verify()), k.secret)
    }

    /// (NexiconDK, 31 October 2026) A player who comes into his company has its map's world
    /// started then (a stand-in game program), once; one who is none of its players
    /// does not.
    #[cfg(unix)]
    #[test]
    fn a_player_coming_into_his_company_starts_its_maps_world() {
        use std::os::unix::fs::PermissionsExt;
        let dir = temp("warm");
        let root = dir.join("omsi");
        std::fs::create_dir_all(root.join("maps/Lemmental")).unwrap();
        std::fs::write(root.join("maps/Lemmental/global.cfg"), "").unwrap();
        let game = dir.join("fake-world.sh");
        std::fs::write(&game, "#!/bin/sh\nread line\n").unwrap();
        std::fs::set_permissions(&game, std::fs::Permissions::from_mode(0o755)).unwrap();
        let data = dir.join("bco");
        let cfg = MainCfg::parse(&format!("root = {}\nreal_time = 0\ndate = 2024-03-04\ngame = {}\nstart_money = 20000\nfound_price = 15000\nfirst_port = 47115\n", root.display(), game.display()));
        let mut m = MainServer::open(&data, cfg).unwrap();
        let p = m.players.login(&Login { steam: "76561198000000042".into(), name: "Mathias".into(), ..Default::default() }, None, m.cfg.logins, m.cfg.start_money, 1_800_000_000).unwrap().player;
        let cid = m.found_for(&p.id, "Lemtal Trafik", "Lemmental", "casual").unwrap();
        let eve = m.players.login(&Login { steam: "76561198000000077".into(), name: "Eve".into(), ..Default::default() }, None, m.cfg.logins, m.cfg.start_money, 1_800_000_000).unwrap().player;
        warm_world(&mut m, &cid, &eve);
        assert!(m.worlds.is_empty(), "not for one who is none of its players");
        warm_world(&mut m, &cid, &p);
        assert_eq!(m.worlds.iter().map(|w| map_folder(&w.map).to_lowercase()).collect::<Vec<_>>(), ["lemmental"]);
        warm_world(&mut m, &cid, &p);
        assert_eq!(m.worlds.len(), 1, "once");
        m.shutdown();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_player_logs_in_founds_and_joins() {
        let dir = temp("found");
        let (api, secret) = api(&dir);
        let ask = |v: Value| api.answer(&v.to_string(), 1_800_000_000);
        // (the code's secret and the version first)
        assert_eq!(ask(json!({ "v": API_VERSION, "secret": "00", "do": "info" }))["error"], BAD_SECRET);
        assert_eq!(ask(json!({ "v": 99, "secret": secret, "do": "info" }))["error"], NEWER);
        assert_eq!(api.answer("nonsense", 0)["error"], UNKNOWN);
        let info = ask(json!({ "v": API_VERSION, "secret": secret, "do": "info" }));
        assert_eq!((info["ok"].as_bool(), info["found_price"].as_i64(), info["maps"][0]["folder"].as_str()), (Some(true), Some(15000), Some("Lemmental")));
        // (logged out: nothing)
        assert_eq!(ask(json!({ "v": API_VERSION, "secret": secret, "do": "companies", "session": "x" }))["error"], LOGGED_OUT);
        // (his account refused, no Steam: turned away)
        assert_eq!(ask(json!({ "v": API_VERSION, "secret": secret, "do": "login", "account": "stolen" }))["error"], crate::company::server_players::ACCOUNT_REFUSED);
        let me = ask(json!({ "v": API_VERSION, "secret": secret, "do": "login", "account": "token-mathias", "steam": "76561198000000001", "name": "pc" }));
        assert_eq!((me["ok"].as_bool(), me["player"]["name"].as_str(), me["player"]["money"].as_i64(), me["new"].as_bool()), (Some(true), Some("Mathias"), Some(20000), Some(true)));
        assert!(me["key"].as_str().is_some_and(|k| k.len() == 32), "a key for this computer's Steam ID");
        let session = me["session"].as_str().unwrap().to_string();
        let pid = me["player"]["id"].as_str().unwrap().to_string();
        let q = |what: &str, extra: Value| {
            let mut v = json!({ "v": API_VERSION, "secret": secret, "do": what, "session": session });
            for (k, x) in extra.as_object().unwrap() {
                v[k] = x.clone();
            }
            ask(v)
        };
        assert_eq!(q("companies", json!({}))["companies"], json!([]));
        assert_eq!(q("found", json!({ "company": " ", "map": "Lemmental" }))["error"], NO_NAME);
        assert_eq!(q("found", json!({ "company": "Ny", "map": "Grundorf" }))["error"], NO_MAP);
        assert_eq!(q("found", json!({ "company": "Ny", "map": "Lemmental", "mode": "hardest" }))["error"], NO_MODE);
        // (phase 6: what founding asks besides its price - his trips first)
        let lemtal = json!({ "company": "Lemtal Trafik", "map": "lemmental", "mode": "casual" });
        let not_yet = q("found", lemtal.clone());
        assert_eq!((not_yet["error"].as_str(), not_yet["args"]["n"].as_str(), not_yet["args"]["need"].as_str()), (Some(crate::company::server_found::NEED_TRIPS), Some("20"), Some("20")));
        assert_eq!((q("me", json!({}))["found"]["trips"]["need"].as_u64(), q("me", json!({}))["found"]["can"].as_bool()), (Some(20), Some(false)));
        {
            // (twenty trips driven for a company, as the companies booked them; and the course's price)
            let mut m = api.server();
            let p = m.players.get_mut(&pid).unwrap();
            p.trips = 20;
            p.money += 5000_00;
        }
        assert_eq!(q("found", lemtal.clone())["error"], crate::company::server_found::NEED_COURSE);
        let on = q("course", json!({}));
        assert_eq!((on["player"]["money"].as_i64(), on["found"]["course"]["running"].as_bool(), on["found"]["course"]["until"].as_str()), (Some(20000), Some(true), Some("2024-03-07")));
        assert_eq!(q("course", json!({}))["error"], crate::company::server_found::COURSE_RUNNING);
        assert_eq!(q("found", lemtal.clone())["args"]["date"], "2024-03-07");
        // (its days gone by)
        api.server().players.get_mut(&pid).unwrap().course_until = "2024-03-04".into();
        assert_eq!(q("found", lemtal.clone())["error"], crate::company::server_found::NEED_TEST);
        // a driving test booked, called off, booked again - and passed on his next solo bus trip
        assert_eq!(q("test", json!({ "class": "tram" }))["error"], crate::company::server_found::NO_CLASS);
        assert_eq!(q("test", json!({ "class": "solo" }))["found"]["test"]["booked"], "solo");
        assert!(q("test", json!({ "class": "" }))["found"]["test"]["booked"].is_null());
        q("test", json!({ "class": "solo" }));
        {
            let mut m = api.server();
            let p = m.players.get_mut(&pid).unwrap();
            let judged = crate::company::online::Judged { passed: true, score: 100, criteria: Vec::new() };
            let pay = crate::company::online::Pay { class: Some(crate::company::career::LicenceClass::Solo), exam: Some(judged), at: 1_800_000_001, ..Default::default() };
            assert!(crate::company::server_found::take_trip(p, &pay).is_some_and(|t| t.passed));
        }
        assert_eq!(q("me", json!({}))["found"]["can"], true);
        let f = q("found", lemtal);
        assert_eq!((f["ok"].as_bool(), f["money"].as_i64()), (Some(true), Some(5000)), "{f}");
        let cid = f["company"].as_str().unwrap().to_string();
        // (he is its owner by the server's id for him)
        let c = store::load(&api.server().data, &cid).unwrap();
        assert_eq!((c.online.owner.as_str(), c.online.owner_id.as_str(), c.online.on_server), ("Mathias", pid.as_str(), true));
        assert_eq!(c.game_mode, crate::company::model::GameMode::Casual);
        // (not enough for a second)
        let poor = q("found", json!({ "company": "Endnu et", "map": "Lemmental" }));
        assert_eq!((poor["error"].as_str(), poor["price"].as_i64(), poor["money"].as_i64()), (Some(NOT_ENOUGH), Some(15000), Some(5000)));
        let list = q("companies", json!({}));
        assert_eq!(list["companies"].as_array().unwrap().len(), 1);
        let row = &list["companies"][0];
        assert_eq!((row["name"].as_str(), row["mine"].as_bool(), row["role"].as_str(), row["map"].as_str(), row["mode"].as_str(), row["runs"].as_bool()), (Some("Lemtal Trafik"), Some(true), Some("co_owner"), Some("Lemmental"), Some("casual"), Some(false)));
        // (its process does not run here: it is starting)
        assert_eq!(q("join", json!({ "company": "nope" }))["error"], NO_COMPANY);
        assert_eq!(q("join", json!({ "company": cid }))["error"], STARTING);
        assert_eq!(q("me", json!({}))["player"]["money"], 5000);
        // (phase 3: the world of its map for one of its players - no game program here to start
        // it; not for one who is none of its players)
        assert_eq!(q("world", json!({ "company": cid }))["error"], NO_WORLD);
        assert_eq!(q("world", json!({ "company": "nope" }))["error"], NO_COMPANY);
        let eve = ask(json!({ "v": API_VERSION, "secret": secret, "do": "login", "steam": "76561198000000077", "name": "Eve" }));
        let not_his = ask(json!({ "v": API_VERSION, "secret": secret, "do": "world", "session": eve["session"], "company": cid }));
        assert_eq!(not_his["error"], NOT_MEMBER);
        // (the players kept)
        let kept_players = crate::company::server_players::Players::load(&api.server().data).unwrap();
        assert_eq!(kept_players.players.len(), 2);
        assert_eq!(kept_players.players[0].money, 500_000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_servers_code_and_its_door() {
        let dir = temp("door");
        let (api, secret) = api(&dir);
        let data = api.server().data.clone();
        let k = kept(&data, 0).unwrap();
        assert_eq!(k.secret, secret, "kept across starts");
        let code = server_code(&kept(&data, 27040).unwrap()).unwrap();
        assert_eq!((code.port, code.company), (27040, server_tag()));
        assert_ne!(server_tag(), code::tag_of("lemtal-trafik"));
        assert_eq!(CompanyCode::decode(&code.encode()).unwrap(), code);
        // (over the loopback: a door on a port of its own)
        let api = Arc::new(api);
        let a = api.clone();
        let server = super::super::together::server::Server::start_at(0, true, PATH, HELLO, Arc::new(move |b: &str| a.answer(b, unix_now()).to_string())).unwrap();
        let base = format!("http://127.0.0.1:{}", server.port());
        let v = ask(&base, &json!({ "v": API_VERSION, "secret": secret, "do": "info" }), Duration::from_secs(5)).unwrap();
        assert_eq!(v["app"], "bus-company-online");
        // (a company's server is no door: its path is another)
        assert!(omsi_net::http::post_json(&format!("{base}/together"), "{}", Duration::from_secs(5)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
