//! (NexiconDK, 1 November 2026) Bus Company Online's main server administered from a browser, or
//! from the launcher's Multiplayer → My server: `GET /admin` on its `api_port` (the door the
//! players' launchers come to, its tunnel with it) is the login page the game servers' panel has
//! (`omsi_net::admin_web::PAGE`) with the main server's own panel (`bco-panel.js`), and
//! `/admin/api/…` what it asks:
//!
//! * `overview` - the clock, the code, the counts, the machine, and what wants looking at (a
//!   company in the red, a world that does not come up, a tender closing today);
//! * `worlds` - every map installed, its companies, its world (running, loading, its ports, its
//!   status: its code, its tunnel, its players);
//! * `companies`, `company?id=` - every company and group head office with its money and its size;
//! * `players` - every player with his money, what founding asks of him, his companies, whether he
//!   is on the server now, and whether he is banned; `POST player` bans, lets in again or forgets
//!   his computers' keys;
//! * `market` - each map's tenders: open ones (bids sealed: only how many), the last decided with
//!   every bid opened, the operators' lines, the chrono events; `POST tender` moves a close, calls
//!   one off, or puts a line out to tender now;
//! * `config` - `company-server.cfg` as text, written whole (read at the next start);
//! * `log?from=`, `log?world=<map>`, `log?company=<id>` - the main server's log (its last thousand
//!   lines), a world's output, a company process's;
//! * `POST command` - a line of the console (`status`, `start Ahlheim`, `money Luc; +500`, `owner
//!   …`, `grant …`, `clock now`): its answer; `POST stop` ends the main server as `quit` does.
//!
//! The login is the game servers' (`omsi_net::admin_web::Auth`: a challenge answered with the
//! admin password, which never goes over the network) with `admin_password` of company-server.cfg
//! (at least 8 characters; the worlds take it too); on this machine only unless `web_admin = 1`
//! there - what comes through the server's tunnel is from elsewhere.

use super::main_server::{self as ms, MainServer};
use super::server_api::Api;
use super::together::server::{Extra, Request};
use super::{clock, concessions as cn, dates, online, shared_tenders, store};
use omsi_net::admin_web::{self as aw, Answer, Auth};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, MutexGuard};

/// The main server's panel (the launcher's file; the page loads it from `/admin/bco-panel.js`).
pub const PANEL_JS: &str = include_str!("../../../../launcher/ui/js/omsihub/server/bco-panel.js");

/// The lines a log of a world or a company's process shows (its last).
const TAIL: usize = 400;

/// What answers the panel's requests on the door (`together::server::Server::start_with`).
pub fn extra(api: Arc<Api>, password: &str, remote: bool) -> Extra {
    let auth = Arc::new(Mutex::new(Auth::new(password, remote)));
    Arc::new(move |r: &Request| handle(&api, &auth, r))
}

fn server(api: &Api) -> MutexGuard<'_, MainServer> {
    api.server.lock().unwrap_or_else(|e| e.into_inner())
}

fn unix_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// A value of the query (`a=1&b=2`), its `%xx` and `+` undone.
fn param(query: &str, key: &str) -> String {
    let raw = query.split('&').find_map(|kv| kv.strip_prefix(&format!("{key}="))).unwrap_or("");
    let mut out = Vec::new();
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                if let Ok(v) = u8::from_str_radix(&raw[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
                out.push(b'%');
            }
            b'+' => out.push(b' '),
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The game servers' login page with this panel (`/admin/bco-panel.js`) and the main server's
/// name and settings.
fn page() -> String {
    aw::PAGE.replace("'/admin/panel.js'", "'/admin/bco-panel.js'").replace("admin_password in server.cfg", "admin_password in company-server.cfg").replace("Server admin", "Bus Company Online admin")
}

pub fn handle(api: &Api, auth: &Mutex<Auth>, r: &Request) -> Option<Answer> {
    let route = r.path.split('?').next().unwrap_or("");
    if route != "/admin" && !route.starts_with("/admin/") {
        return None;
    }
    let query = r.path.split_once('?').map(|x| x.1).unwrap_or("");
    let here = aw::from_here(r.head, r.peer);
    if let Some(a) = aw::door(route, here, auth) {
        return Some(a);
    }
    let post = r.method == "POST";
    let body = String::from_utf8_lossy(r.body).to_string();
    match route {
        "/admin" | "/admin/" => return Some(("200 OK", "text/html; charset=utf-8", page().into_bytes())),
        // (the game servers' panel: its pieces - the form, the buttons - are this one's too)
        "/admin/panel.js" => return Some(("200 OK", "text/javascript; charset=utf-8", aw::PANEL_JS.as_bytes().to_vec())),
        "/admin/bco-panel.js" => return Some(("200 OK", "text/javascript; charset=utf-8", PANEL_JS.as_bytes().to_vec())),
        _ => {}
    }
    if let Some(a) = aw::login_routes(route, post, r.head, &body, auth) {
        return Some(a);
    }
    if !aw::logged_in(r.head, here, auth) {
        return Some(aw::error("401 Unauthorized", "log in with the admin password"));
    }
    let ok = |v: Value| aw::json("200 OK", v.to_string());
    let asked: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
    let s = |k: &str| asked.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    Some(match (route.trim_start_matches("/admin/api/"), post) {
        ("overview", _) => ok(overview(api)),
        ("worlds", _) => ok(worlds(api)),
        ("companies", _) => ok(companies(api)),
        ("company", _) => match company(api, &param(query, "id")) {
            Some(v) => ok(v),
            None => aw::error("404 Not Found", "no such company"),
        },
        ("players", _) => ok(players(api)),
        ("market", _) => ok(market(api)),
        ("log", _) => ok(log(api, query)),
        ("config", false) => {
            let p = server(api).data.join(ms::CFG_FILE);
            ok(json!({ "text": std::fs::read_to_string(&p).unwrap_or_default(), "path": p.to_string_lossy() }))
        }
        ("config", true) => {
            if body.trim().is_empty() || body.len() >= aw::BODY_MAX {
                return Some(aw::error("400 Bad Request", "no settings in the request, or too long"));
            }
            let p = server(api).data.join(ms::CFG_FILE);
            match std::fs::write(&p, body.replace("\r\n", "\n").replace('\n', "\r\n")) {
                Ok(()) => {
                    server(api).say("admin panel: company-server.cfg written (read at the next start)");
                    ok(json!({ "ok": true }))
                }
                Err(e) => aw::error("500 Internal Server Error", &e.to_string()),
            }
        }
        ("command", true) => {
            let line = body.lines().next().unwrap_or("").trim().to_string();
            server(api).say(&format!("admin panel: {line}"));
            let answer = ms::command(&mut server(api), &line);
            match answer {
                Some(a) => ok(json!({ "answer": a })),
                None => {
                    ms::request_stop();
                    ok(json!({ "answer": "stopping: the worlds stop and every company is saved" }))
                }
            }
        }
        ("stop", true) => {
            server(api).say("admin panel: the main server stops");
            ms::request_stop();
            ok(json!({ "ok": true }))
        }
        ("player", true) => match player_do(api, &s("id"), &s("do"), &s("reason")) {
            Ok(text) => ok(json!({ "answer": text })),
            Err(e) => aw::error("400 Bad Request", &e),
        },
        ("tender", true) => match tender_do(api, &s("map"), asked.get("id").and_then(Value::as_u64).unwrap_or(0) as u32, &s("do"), &s("at"), &s("line")) {
            Ok(text) => ok(json!({ "answer": text })),
            Err(e) => aw::error("400 Bad Request", &e),
        },
        _ => aw::error("404 Not Found", "no such request"),
    })
}

/// Whole units of money.
fn whole(cents: i64) -> i64 {
    cents / 100
}

/// The map's folder as it is named (`maps/Ahlheim 5/global.cfg`: `Ahlheim 5`).
fn folder_name(map: &str) -> String {
    map.replace('\\', "/").trim_end_matches("/global.cfg").trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string()
}

/// The map's folder name (`maps/<folder>/global.cfg`), lower case.
fn folder(map: &str) -> String {
    online::map_folder(map)
}

fn overview(api: &Api) -> Value {
    let online_now = api.online().len();
    let m = server(api);
    let (date, secs) = m.clock.at_real(unix_now() as f64);
    let ids = online::company_ids(&m.data);
    let mut alerts: Vec<Value> = Vec::new();
    for id in &ids {
        if let Ok(c) = store::load(&m.data, id) {
            if c.cash < 0 {
                alerts.push(json!({ "level": "warn", "kind": "red", "name": c.name, "money": whole(c.cash), "company": id }));
            }
        }
    }
    for w in &m.worlds {
        let (loading, took) = w.loading();
        let mins = w.started.elapsed().as_secs() / 60;
        if !w.running() && took.map_or(mins >= 15, |t| w.started.elapsed().as_secs() > t * 2 + 300) {
            alerts.push(json!({ "level": "warn", "kind": "loading", "name": folder_name(&w.map), "minutes": mins, "what": loading }));
        }
    }
    // (the tenders that close today)
    let mut keys: Vec<String> = ids.iter().filter_map(|id| store::load(&m.data, id).ok()).map(|c| shared_tenders::key_of(&c.map)).collect();
    keys.sort();
    keys.dedup();
    for key in &keys {
        if let Some(mk) = shared_tenders::load(&m.data, key) {
            let n = mk.tenders.iter().filter(|t| t.decided.is_none() && clock::date_of(t.tender.closes_at) == date).count();
            if n > 0 {
                alerts.push(json!({ "level": "info", "kind": "closing", "name": folder_name(&mk.map), "n": n }));
            }
        }
    }
    let running = m.worlds.iter().filter(|w| w.running()).count();
    let s = secs as i64;
    json!({
        "name": m.cfg.name,
        "date": date,
        "time": format!("{:02}:{:02}", s / 3600, s / 60 % 60),
        "real_time": m.cfg.real_time,
        "code": m.code,
        "tunnel": m.tunnel_url,
        "api_port": m.cfg.api_port,
        "players": m.players.players.len(),
        "online": online_now,
        "companies": ids.len(),
        "groups": super::server_hq::group_ids(&m.data).len(),
        "worlds": { "running": running, "loading": m.worlds.len() - running, "max": m.cfg.max_worlds, "keep": m.cfg.keep.on },
        "machine": m.load.map(|l| l.text()),
        "alerts": alerts,
    })
}

fn worlds(api: &Api) -> Value {
    let m = server(api);
    let mut per_map: std::collections::HashMap<String, Vec<String>> = Default::default();
    for id in online::company_ids(&m.data) {
        if let Ok(c) = store::load(&m.data, &id) {
            per_map.entry(folder(&c.map).to_lowercase()).or_default().push(c.name.clone());
        }
    }
    let now = unix_now() as f64;
    let maps: Vec<Value> = ms::installed_maps(&m.cfg.root)
        .into_iter()
        .map(|f| {
            let w = m.worlds.iter().find(|w| folder(&w.map).eq_ignore_ascii_case(&f));
            let world = w.map(|w| {
                let (loading, took) = w.loading();
                json!({
                    "running": w.running(),
                    "loading": loading,
                    "load_secs": took,
                    "port": w.port,
                    "web_port": w.web_port,
                    "minutes": w.started.elapsed().as_secs() / 60,
                    "empty_minutes": w.empty_since.map(|s| ((now - s) / 60.0).max(0.0) as i64),
                    "status": ms::world_status(w.slot).map(|v| json!({
                        "code": v.get("code"),
                        "tunnel": v.get("tunnel"),
                        "players": v.get("players").and_then(Value::as_array).map(|ps| ps.iter().map(|p| json!({ "name": p.get("name"), "bus": p.get("bus"), "line": p.get("line") })).collect::<Vec<_>>()).unwrap_or_default(),
                    })),
                })
            });
            json!({ "folder": f, "companies": per_map.get(&f.to_lowercase()).cloned().unwrap_or_default(), "world": world })
        })
        .collect();
    json!({ "maps": maps, "max": m.cfg.max_worlds })
}

/// A company as the list shows it.
fn company_row(m: &MainServer, id: &str, c: &super::model::Company, group: bool) -> Value {
    let proc = m.companies.iter().find(|p| p.id == id);
    json!({
        "id": id,
        "name": c.name,
        "map": if group { String::new() } else { folder_name(&c.map) },
        "mode": c.game_mode.label(),
        "owner": c.online.owner,
        "owner_id": c.online.owner_id,
        "cash": whole(c.cash),
        "debt": whole(c.debt()),
        "lines": c.lines.len(),
        "buses": c.fleet.len(),
        "staff": c.staff.len(),
        "date": c.date,
        "hq": c.online.hq,
        "group": group,
        "runs": proc.is_some(),
        "port": proc.map(|p| p.port),
        "code": std::fs::read_to_string(online::code_file(&m.data, id)).ok().map(|c| c.trim().to_string()),
    })
}

fn companies(api: &Api) -> Value {
    let m = server(api);
    let mut out: Vec<Value> = online::company_ids(&m.data).into_iter().filter_map(|id| store::load(&m.data, &id).ok().map(|c| company_row(&m, &id, &c, false))).collect();
    out.extend(super::server_hq::group_ids(&m.data).into_iter().filter_map(|id| store::load(&m.data, &id).ok().map(|c| company_row(&m, &id, &c, true))));
    json!({ "companies": out })
}

fn company(api: &Api, id: &str) -> Option<Value> {
    let m = server(api);
    let group = super::server_hq::group_ids(&m.data).iter().any(|g| g == id);
    let c = store::load(&m.data, id).ok()?;
    let mut v = company_row(&m, id, &c, group);
    let depots: Vec<Value> = super::server_depots::depots_of(&m.data, id).iter().map(|d| json!({ "id": d.id, "name": d.name })).collect();
    let held: Vec<Value> = c.concessions.held.iter().map(|h| json!({ "line": h.line, "number": h.number, "from": h.from, "until": h.until })).collect();
    let o = v.as_object_mut()?;
    o.insert("depots".into(), json!(depots));
    o.insert("concessions".into(), json!(held));
    let members: Vec<Value> = c.together.guests.iter().map(|g| json!({ "id": g.id, "name": g.name, "role": g.role.key(), "banned": g.banned, "owner": g.id == c.online.owner_id })).collect();
    o.insert("members".into(), json!(members));
    o.insert("reputation".into(), json!((c.reputation * 10.0).round() / 10.0));
    Some(v)
}

fn players(api: &Api) -> Value {
    let online_now = api.online();
    let m = server(api);
    let today = m.clock.at_real(unix_now() as f64).0;
    let owners: Vec<(String, String)> = online::company_ids(&m.data).into_iter().filter_map(|id| store::load(&m.data, &id).ok()).map(|c| (c.online.owner_id.clone(), c.name.clone())).collect();
    let list: Vec<Value> = m
        .players
        .players
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "name": p.name,
                "account": !p.account.is_empty(),
                "steam": !p.steam.is_empty(),
                "keys": p.keys.len(),
                "money": whole(p.money),
                "trips": p.trips,
                "course": super::server_found::course_done(p, &today),
                "tests": p.tests.licences.iter().map(|c| c.label()).collect::<Vec<_>>(),
                "owns": owners.iter().filter(|o| o.0 == p.id).map(|o| o.1.clone()).collect::<Vec<_>>(),
                "since": p.since,
                "seen": p.seen,
                "online": online_now.contains(&p.id),
                "banned": p.banned,
            })
        })
        .collect();
    json!({ "players": list })
}

/// A price a year as the market's price a timetable hour (whole money).
fn hour(t: &cn::Tender, amount: i64) -> f64 {
    (cn::per_hour(t, amount) / 100.0 * 100.0).round() / 100.0
}

fn market(api: &Api) -> Value {
    let m = server(api);
    let mut keys: Vec<String> = online::company_ids(&m.data).into_iter().filter_map(|id| store::load(&m.data, &id).ok()).map(|c| shared_tenders::key_of(&c.map)).collect();
    keys.sort();
    keys.dedup();
    let at = |x: i64| format!("{} {}", clock::date_of(x), clock::hhmm(x));
    // (who runs each line of a map: its companies)
    let mut runs: Vec<(String, String, String)> = Vec::new();
    for id in online::company_ids(&m.data) {
        if let Ok(c) = store::load(&m.data, &id) {
            let key = shared_tenders::key_of(&c.map);
            runs.extend(c.lines.iter().map(|l| (key.clone(), l.name.to_lowercase(), c.name.clone())));
        }
    }
    let maps: Vec<Value> = keys
        .iter()
        .filter_map(|key| shared_tenders::load(&m.data, key))
        .map(|mk| {
            let bids = shared_tenders::bids(&m.data, &mk.key);
            let open: Vec<Value> = mk
                .tenders
                .iter()
                .filter(|t| t.decided.is_none())
                .map(|mt| {
                    let t = &mt.tender;
                    let x = t.market.as_ref();
                    let id = x.map_or(t.id, |x| x.id);
                    let mut who: Vec<&str> = bids.iter().filter(|b| b.tender == id).map(|b| b.company.as_str()).collect();
                    who.sort();
                    who.dedup();
                    json!({
                        "id": id, "line": t.line, "number": t.number, "caption": t.caption,
                        "opens": at(t.opens_at), "closes": at(t.closes_at),
                        "renewal": t.renewal, "event": x.map(|x| x.event.clone()).unwrap_or_default(),
                        "from": x.map(|x| x.from.clone()).unwrap_or_default(), "until": x.map(|x| x.until.clone()).unwrap_or_default(),
                        "cap_hour": hour(t, t.reference), "bidders": who.len(), "applied": mt.applied,
                    })
                })
                .collect();
            let mut decided: Vec<&shared_tenders::MarketTender> = mk.tenders.iter().filter(|t| t.decided.is_some()).collect();
            decided.sort_by_key(|t| std::cmp::Reverse(t.tender.closes_at));
            let decided: Vec<Value> = decided
                .into_iter()
                .take(12)
                .map(|mt| {
                    let t = &mt.tender;
                    let d = mt.decided.as_ref().expect("decided");
                    json!({
                        "id": t.market.as_ref().map_or(t.id, |x| x.id), "line": t.line, "number": t.number, "closed": at(t.closes_at),
                        "winner": d.name, "company": !d.winner.is_empty(), "hour": hour(t, d.amount),
                        "bids": d.standings.iter().map(|s| json!({ "name": s.name, "hour": hour(t, s.amount), "total": (s.total * 10.0).round() / 10.0, "rejected": s.rejected })).collect::<Vec<_>>(),
                    })
                })
                .collect();
            let held: Vec<Value> = mk.held.iter().map(|h| json!({ "line": h.line, "number": h.number, "name": h.name, "from": h.from, "until": h.until, "gives_up": h.gives_up })).collect();
            let lines: Vec<Value> = mk
                .lines
                .iter()
                .map(|l| {
                    let low = l.to_lowercase();
                    let company = runs.iter().find(|r| r.0 == mk.key && r.1 == low).map(|r| r.2.clone());
                    let operator = mk.held.iter().find(|h| h.line.eq_ignore_ascii_case(l)).map(|h| h.name.clone());
                    let open = mk.tenders.iter().any(|t| t.decided.is_none() && t.tender.line.eq_ignore_ascii_case(l));
                    let event = mk.events.iter().any(|e| e.lines.iter().any(|x| x.eq_ignore_ascii_case(l)));
                    json!({ "name": l, "company": company, "operator": operator, "open": open, "event": event })
                })
                .collect();
            json!({ "key": mk.key, "map": folder_name(&mk.map), "open": open, "decided": decided, "held": held, "events": mk.events, "lines": lines })
        })
        .collect();
    json!({ "maps": maps })
}

/// The last `TAIL` lines of a file.
fn tail(path: &std::path::Path) -> Vec<String> {
    let text = std::fs::read(path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(TAIL)..].iter().map(|l| l.to_string()).collect()
}

fn log(api: &Api, query: &str) -> Value {
    let (world, company) = (param(query, "world"), param(query, "company"));
    if !world.is_empty() || !company.is_empty() {
        let data = server(api).data.clone();
        // (only names of the folders' own: nothing outside them)
        let safe = |s: &str| !s.contains(['/', '\\']) && !s.contains("..");
        let path = if !world.is_empty() && safe(&world) {
            data.join("worlds").join(online::map_folder(&world)).join("world.out")
        } else if safe(&company) {
            data.join("hosts").join(format!("{company}.out"))
        } else {
            return json!({ "lines": [], "next": 0 });
        };
        return json!({ "lines": tail(&path), "next": 0 });
    }
    let from = param(query, "from").parse::<u64>().unwrap_or(0);
    let (lines, next) = aw::log_since(from);
    json!({ "lines": lines, "next": next })
}

/// A player banned (`ban`, with why), let in again (`unban`), or his computers' keys forgotten
/// (`keys`: a computer comes in with its Steam ID anew).
fn player_do(api: &Api, id: &str, what: &str, reason: &str) -> Result<String, String> {
    let mut m = server(api);
    let data = m.data.clone();
    let p = m.players.get_mut(id).ok_or("no such player")?;
    let said = match what {
        "ban" => {
            p.banned = if reason.is_empty() { "banned by the server's admin".to_string() } else { reason.chars().take(200).collect() };
            format!("{} is banned: {}", p.name, p.banned)
        }
        "unban" => {
            p.banned.clear();
            format!("{} may come in again", p.name)
        }
        "keys" => {
            p.keys.clear();
            format!("{}'s computers' keys are forgotten: his next login with Steam gets a new one", p.name)
        }
        _ => return Err("ban, unban or keys".into()),
    };
    m.players.save(&data).map_err(|e| format!("{e:#}"))?;
    m.say(&format!("admin panel: {said}"));
    drop(m);
    if what == "ban" {
        api.end_sessions_of(id);
    }
    Ok(said)
}

/// A tender of map `map` (its market's key): its close moved (`close` to `at`, `YYYY-MM-DD HH:MM`),
/// called off (`cancel`), or a line put out to tender now (`open`, `line`).
fn tender_do(api: &Api, map: &str, id: u32, what: &str, at: &str, line: &str) -> Result<String, String> {
    let m = server(api);
    let data = m.data.clone();
    let mut mk = shared_tenders::load(&data, map).ok_or("no such market")?;
    let said = match what {
        "open" => {
            if line.is_empty() {
                return Err("which line?".into());
            }
            shared_tenders::ask_as_server(&data, map, line).map_err(|e| format!("{e:#}"))?;
            format!("line {line} of {map}: out to tender at once, unless a company or an operator runs it")
        }
        "close" | "cancel" => {
            let k = mk.tenders.iter().position(|t| t.decided.is_none() && t.tender.market.as_ref().is_some_and(|x| x.id == id)).ok_or("no such open tender")?;
            if what == "cancel" {
                let t = mk.tenders.remove(k);
                format!("the tender of line {} of {map} is called off", t.tender.number)
            } else {
                let (day, hm) = at.split_once(' ').ok_or("the time as YYYY-MM-DD HH:MM")?;
                let (h, mi) = hm.split_once(':').ok_or("the time as YYYY-MM-DD HH:MM")?;
                let (h, mi) = (h.trim().parse::<i64>().map_err(|e| e.to_string())?, mi.trim().parse::<i64>().map_err(|e| e.to_string())?);
                if dates::parse(day).is_none() || !(0..24).contains(&h) || !(0..60).contains(&mi) {
                    return Err("the time as YYYY-MM-DD HH:MM".into());
                }
                let t = &mut mk.tenders[k].tender;
                let closes = clock::moment(day, h * 60 + mi);
                if closes <= t.opens_at {
                    return Err("it closes before it opens".into());
                }
                t.closes_at = closes;
                t.closes = day.to_string();
                format!("the tender of line {} of {map} closes {day} {hm} now", t.number)
            }
        }
        _ => return Err("open, close or cancel".into()),
    };
    if what != "open" {
        shared_tenders::save(&data, &mk).map_err(|e| format!("{e:#}"))?;
    }
    m.say(&format!("admin panel: {said}"));
    Ok(said)
}

#[cfg(test)]
mod tests;
