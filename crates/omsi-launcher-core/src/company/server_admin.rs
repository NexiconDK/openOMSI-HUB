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
//!   one off, or puts a line out to tender now; (3 November 2026) the tenders' rules as they stand,
//!   the money in kroner, the companies' contracts - and `POST tender` sets an open tender's cap,
//!   puts lines out with the admin's own times and cap, awards a line directly to a company, moves
//!   an operator's or a company's last day, or ends an operator's term (`tender_do`);
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
use super::currency::Currency;
use super::model::{Cents, Drive};
use super::server_fines::Fines;
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
        ("tender", true) => match tender_do(api, &s("map"), &asked) {
            Ok(text) => ok(json!({ "answer": text })),
            Err(e) => aw::error("400 Bad Request", &e),
        },
        // (3 November 2026) the installed buses, and the admin's corrections of one
        ("events", true) => match events_do(api, &asked) {
            Ok(text) => ok(json!({ "answer": text })),
            Err(e) => aw::error("400 Bad Request", &e),
        },
        ("buses", _) => ok(buses(api)),
        // (3 November 2026) the server's competitors, and the admin's changes of them
        ("competitors", _) => ok(competitors_view(api)),
        ("competitor", true) => match competitor_do(api, &asked) {
            Ok(text) => ok(json!({ "answer": text })),
            Err(e) => aw::error("400 Bad Request", &e),
        },
        ("bus", true) => match bus_do(api, &asked) {
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
/// A tender's amount a year as kroner a timetable hour (3 November 2026: the panel's money is
/// kroner, as the settings' - it said euros here).
fn hour(t: &cn::Tender, amount: i64) -> f64 {
    (Currency::Dkk.units(cn::per_hour(t, amount)) * 100.0).round() / 100.0
}

/// Kroner of euro cents, whole.
fn kr(cents: Cents) -> i64 {
    Currency::Dkk.units(cents as f64).round() as i64
}

/// The server's date and minute now (`clock::moment`).
fn server_now(m: &MainServer) -> (String, i64) {
    let (day, secs) = m.clock.at_real(unix_now() as f64);
    let now = clock::moment(&day, (secs / 60.0).floor() as i64);
    (day, now)
}

/// A moment typed `YYYY-MM-DD HH:MM` (or with a `T`), as the server's minutes.
fn moment_of(text: &str) -> Result<i64, String> {
    let text = text.trim().replace('T', " ");
    let bad = || "the time as YYYY-MM-DD HH:MM".to_string();
    let (day, hm) = text.split_once(' ').ok_or_else(bad)?;
    let (h, mi) = hm.split_once(':').ok_or_else(bad)?;
    let (h, mi) = (h.trim().parse::<i64>().map_err(|_| bad())?, mi.trim().parse::<i64>().map_err(|_| bad())?);
    if dates::parse(day).is_none() || !(0..24).contains(&h) || !(0..60).contains(&mi) {
        return Err(bad());
    }
    Ok(clock::moment(day, h * 60 + mi))
}

fn market(api: &Api) -> Value {
    let m = server(api);
    let mut keys: Vec<String> = online::company_ids(&m.data).into_iter().filter_map(|id| store::load(&m.data, &id).ok()).map(|c| shared_tenders::key_of(&c.map)).collect();
    keys.sort();
    keys.dedup();
    let at = |x: i64| format!("{} {}", clock::date_of(x), clock::hhmm(x));
    let all: Vec<super::model::Company> = online::company_ids(&m.data).into_iter().filter_map(|id| store::load(&m.data, &id).ok()).collect();
    // (who runs each line of a map: its companies)
    let mut runs: Vec<(String, String, String)> = Vec::new();
    for c in &all {
        let key = shared_tenders::key_of(&c.map);
        runs.extend(c.lines.iter().map(|l| (key.clone(), l.name.to_lowercase(), c.name.clone())));
    }
    let (today, _) = server_now(&m);
    let cfg = shared_tenders::TenderCfg::of(&m.data);
    let doubling = super::doubling::Rules::of(&m.data);
    let live = super::live_pay::LiveCfg::of(&m.data);
    // (3 November 2026: the tenders' rules as they stand, in kroner)
    let rules = json!({
        "cap_hour": kr(cfg.cap_hour), "cap_bus_day": kr(cfg.cap_bus_day), "cap_factor": cfg.cap_factor, "low_share": (cfg.low_share * 1000.0).round() / 10.0,
        "operators": cfg.operators, "round_weeks": cfg.round_weeks, "per_round": cfg.per_round, "award": cfg.award.key(),
        // (the timeline, the contracts' days by size - diesel, electric - and the option: 3 November 2026)
        "prep": shared_tenders::PREP_DAYS, "days": shared_tenders::TENDER_DAYS, "start": shared_tenders::START_AFTER,
        "renew": [shared_tenders::RENEW_ANNOUNCED, shared_tenders::RENEW_OPENS, shared_tenders::RENEW_DECIDED],
        "lengths": cn::SERVER_DAYS.iter().map(|x| [x.0, x.2]).collect::<Vec<_>>(),
        "options": shared_tenders::OPTIONS, "option_days": shared_tenders::OPTION_DAYS,
        // (doubling: the price of a run with a bus of no contract, how full is full - 3 November 2026)
        "doubling_price": kr(doubling.price_hour), "doubling_full": (doubling.full_share * 100.0).round(),
        // (driving yourself pays: a player's pay against an AI driver's, the AI's cap in the
        // operating quality, the live surcharge, the AI drivers a map - 3 November 2026, `live_pay`)
        "player_pay": (live.pay_share * 100.0).round(), "ai_cap": live.ai_cap.round(), "live_surcharge": (live.surcharge * 1000.0).round() / 10.0, "driver_pool": live.pool,
    });
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
                    let (bus_cap, bus_cost) = x.map_or((0, 0), |x| (x.bus_cap, x.bus_cost));
                    let id = x.map_or(t.id, |x| x.id);
                    let mut who: Vec<&str> = bids.iter().filter(|b| b.tender == id).map(|b| b.company.as_str()).collect();
                    who.sort();
                    who.dedup();
                    json!({
                        "id": id, "line": t.line, "number": t.number, "caption": t.caption,
                        "opens": at(t.opens_at), "closes": at(t.closes_at),
                        "renewal": t.renewal, "event": x.map(|x| x.event.clone()).unwrap_or_default(),
                        "from": x.map(|x| x.from.clone()).unwrap_or_default(), "until": x.map(|x| x.until.clone()).unwrap_or_default(),
                        "cap_hour": hour(t, (t.reference - cn::bus_year(t, bus_cap)).max(0)), "bidders": who.len(), "applied": mt.applied,
                        // (3 November 2026: the authority's estimate and the abnormally low line, in kroner -
                        // a bid in two parts: the timetable hour's, and a contract bus a day's)
                        "cost_hour": hour(t, (t.cost - cn::bus_year(t, bus_cost)).max(0)), "low_hour": hour(t, super::sealed::low_line(t)),
                        "bus_cap": kr(bus_cap), "bus_cost": kr(bus_cost), "buses": cn::contract_buses(t),
                        "direct": x.map(|x| x.direct_name.clone()).unwrap_or_default(),
                        // (3 November 2026: a package's name and lines, and the work's reserve buses)
                        "package": t.package.as_ref().filter(|p| p.lines.len() > 1).map(|p| p.name.clone()).unwrap_or_default(),
                        "lines": t.lines(), "reserve": x.map_or(0, |x| x.reserve),
                        // (3 November 2026: what it asks of the buses)
                        "needs": x.and_then(|x| x.needs).map(|n| needs_json(&n)),
                    })
                })
                .collect();
            let mut decided: Vec<&shared_tenders::MarketTender> = mk.tenders.iter().filter(|t| t.decided.is_some()).collect();
            decided.sort_by_key(|t| std::cmp::Reverse(t.tender.closes_at));
            let decided: Vec<Value> = decided
                .into_iter()
                .take(40)
                .map(|mt| {
                    let t = &mt.tender;
                    let d = mt.decided.as_ref().expect("decided");
                    json!({
                        "id": t.market.as_ref().map_or(t.id, |x| x.id), "line": t.line, "number": t.number, "closed": at(t.closes_at),
                        "winner": d.name, "company": !d.winner.is_empty(), "hour": hour(t, d.amount - cn::bus_year(t, d.bus_day)), "bus_day": kr(d.bus_day),
                        "bids": d.standings.iter().map(|s| json!({ "name": s.name, "hour": hour(t, s.amount - cn::bus_year(t, s.bus_day)), "bus_day": kr(s.bus_day), "total": (s.total * 10.0).round() / 10.0, "rejected": s.rejected })).collect::<Vec<_>>(),
                    })
                })
                .collect();
            let next = |until: &str| dates::add(until, 1 - shared_tenders::RENEW_OPENS);
            let held: Vec<Value> = mk.held.iter().map(|h| json!({ "line": h.line, "number": h.number, "name": h.name, "from": h.from, "until": h.until, "gives_up": h.gives_up, "next": next(&h.until), "package": h.package })).collect();
            // (3 November 2026: the companies of the map and their contracts - the admin moves a
            // contract's last day, awards a line directly)
            let here: Vec<&super::model::Company> = all.iter().filter(|c| shared_tenders::key_of(&c.map) == mk.key).collect();
            let contracts: Vec<Value> = here
                .iter()
                .flat_map(|c| c.concessions.held.iter().map(move |h| json!({ "company": c.id, "name": c.name, "line": h.line, "number": h.number, "from": h.from, "until": h.until, "next": next(&h.until), "package": h.package, "lines": if h.with.len() > 1 { h.with.clone() } else { vec![h.line.clone()] } })))
                .collect();
            let companies: Vec<Value> = here.iter().map(|c| json!({ "id": c.id, "name": c.name })).collect();
            let operators: Vec<Value> = mk.rivals.iter().map(|r| json!(r.name)).collect();
            let lines: Vec<Value> = mk
                .lines
                .iter()
                .map(|l| {
                    let low = l.to_lowercase();
                    let company = runs.iter().find(|r| r.0 == mk.key && r.1 == low).map(|r| r.2.clone());
                    let operator = mk.held.iter().find(|h| h.line.eq_ignore_ascii_case(l)).map(|h| h.name.clone());
                    let open = mk.tenders.iter().any(|t| t.decided.is_none() && t.tender.has_line(l));
                    let event = mk.events.iter().any(|e| e.lines.iter().any(|x| x.eq_ignore_ascii_case(l)));
                    json!({ "name": l, "company": company, "operator": operator, "open": open, "event": event })
                })
                .collect();
            // (3 November 2026: its roadworks - `contract_events`)
            let roadworks: Vec<super::contract_events::Roadwork> = super::contract_events::load_roadworks(&m.data).into_iter().filter(|r| r.map == mk.key).collect();
            // (3 November 2026: the rail replacements the map has and the admin ordered - H9 -, and
            // the open complaints about awards with their tenders' bids - H11)
            let rails: Vec<Value> = mk.rails.iter().map(|r| json!({ "line": r.line, "from": r.from, "until": r.until, "price": kr(r.hour), "direct": r.direct_name, "offered": r.offered })).collect();
            let complaints: Vec<Value> = mk
                .tenders
                .iter()
                .filter_map(|mt| mt.decided.as_ref().map(|d| (mt, d)))
                .flat_map(|(mt, d)| {
                    let t = &mt.tender;
                    mt.complaints.iter().filter(|k| k.state == super::server_awards::ComplaintState::Open).map(move |k| {
                        json!({
                            "id": t.market.as_ref().map_or(t.id, |x| x.id), "number": t.number, "company": k.company, "name": k.name, "reason": k.reason,
                            "winner": d.name, "next": super::server_awards::next_of(d), "start": cn::server_start(t),
                            "bids": d.standings.iter().map(|s| json!({ "name": s.name, "hour": hour(t, s.amount - cn::bus_year(t, s.bus_day)), "total": (s.total * 10.0).round() / 10.0, "rejected": s.rejected })).collect::<Vec<_>>(),
                        })
                    })
                })
                .collect();
            // (the AI drivers its companies employ, against the pool - `live_pay`, K4)
            let hired: u32 = all.iter().filter(|c| c.online.on_server && shared_tenders::key_of(&c.map) == mk.key).map(super::live_pay::drivers_of).sum();
            let drivers = json!({ "pool": live.pool, "hired": hired });
            json!({ "key": mk.key, "map": folder_name(&mk.map), "drivers": drivers, "open": open, "decided": decided, "held": held, "events": mk.events, "lines": lines, "contracts": contracts, "companies": companies, "operators": operators, "roadworks": roadworks, "rail_lines": mk.rail_lines, "rails": rails, "complaints": complaints })
        })
        .collect();
    // (3 November 2026: the server's calendar and the admin's holidays - `contract_events`)
    let calendar = if super::contract_events::danish(&m.data) { "dk" } else { "map" };
    json!({ "maps": maps, "rules": rules, "today": today, "earliest_end": dates::add(&today, shared_tenders::RENEW_ANNOUNCED), "calendar": calendar, "holidays": super::contract_events::load_periods(&m.data), "must_accept": super::contract_events::must_accept(&m.data), "change_days": super::contract_events::CHANGE_DAYS, "force_majeure": super::contract_events::load_majeure(&m.data), "weather_files": super::contract_events::installed_weather(&m.cfg.root),
        // (NexiconDK, 3 November 2026: the supplements for unsocial hours - K11 -, and the month's
        // ranking of live driving - K7)
        "supplements": super::live_more::load_supplements(&m.data).iter().map(|x| json!({ "id": x.id, "name": x.name, "when": x.when, "percent": x.percent, "per_hour": x.per_hour })).collect::<Vec<_>>(),
        "ranking": super::live_more::ranking(m.players.players.iter().map(|p| (p.name.as_str(), p.months.as_slice())), today.get(..7).unwrap_or(""), 10),
    })
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

/// (3 November 2026) The installed buses as the market reads them: each bus file read once, on a
/// thread of its own (the panel says it is reading meanwhile).
type Catalogue = (bool, Option<Arc<Vec<super::market::MarketBus>>>);
static CATALOGUE: Mutex<Catalogue> = Mutex::new((false, None));

/// The installed buses for the panel's "Buses" (`server_buses::listed`): each with its floor,
/// fuel and size as they hold on the server, the game's guess and the admin's correction.
fn buses(api: &Api) -> Value {
    use super::server_buses::{self as sb, Floor, FloorRule, FuelRule};
    let data = server(api).data.clone();
    let list = {
        let mut g = CATALOGUE.lock().unwrap_or_else(|e| e.into_inner());
        if g.1.is_none() && !g.0 {
            g.0 = true;
            std::thread::spawn(|| {
                let list = crate::list_vehicles().map(|v| super::market::market_of(&v)).unwrap_or_default();
                *CATALOGUE.lock().unwrap_or_else(|e| e.into_inner()) = (false, Some(Arc::new(list)));
            });
        }
        g.1.clone()
    };
    let floors: Vec<Value> = Floor::ALL.iter().map(|f| json!([f.key(), f.label()])).collect();
    let drives: Vec<Value> = [Drive::Diesel, Drive::Cng, Drive::Electric].iter().map(|d| json!([sb::drive_key(*d), sb::drive_label(*d)])).collect();
    let floor_rules: Vec<Value> = FloorRule::ALL.iter().map(|f| json!([f.key(), f.label()])).collect();
    let fuel_rules: Vec<Value> = FuelRule::ALL.iter().map(|f| json!([f.key(), f.label()])).collect();
    let choices = json!({
        "floors": floors, "drives": drives,
        "sizes": [["midi", "Midibus"], ["solo", "Solo bus"], ["articulated", "Articulated bus"], ["double", "Double-decker"]],
        "floor_rules": floor_rules, "fuel_rules": fuel_rules, "max_age": sb::MAX_AGE,
    });
    let Some(list) = list else { return json!({ "reading": true, "buses": [], "choices": choices }) };
    let rows: Vec<Value> = sb::listed(&list, &sb::load(&data))
        .into_iter()
        .map(|b| {
            json!({
                "file": b.file, "name": b.name, "coach": b.coach, "screens": b.screens,
                "floor": b.floor.key(), "drive": sb::drive_key(b.drive), "size": sb::size_key(b.size),
                "guess": { "floor": b.guess_floor.key(), "drive": sb::drive_key(b.guess_drive), "size": sb::size_key(b.guess_size) },
                "set": { "floor": b.set.floor.is_some(), "drive": b.set.drive.is_some(), "size": b.set.size.is_some(), "screens": b.set.screens.is_some() },
            })
        })
        .collect();
    json!({ "reading": false, "buses": rows, "choices": choices })
}

/// The admin corrects a bus (`file`): its `floor`, `drive`, `size` and `screens` - each a key, or
/// empty for the game's guess (`server_buses::set`); the companies take it at their next step.
fn bus_do(api: &Api, a: &Value) -> Result<String, String> {
    use super::server_buses::{self as sb, BusSet, Floor};
    let s = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let file = s("file");
    if file.is_empty() {
        return Err("which bus?".into());
    }
    let screens = match a.get("screens") {
        Some(Value::Bool(b)) => Some(*b),
        Some(Value::String(t)) if t == "1" || t == "true" => Some(true),
        Some(Value::String(t)) if t == "0" || t == "false" => Some(false),
        _ => None,
    };
    let set = BusSet { floor: Floor::of_key(&s("floor")), drive: sb::drive_of_key(&s("drive")), size: sb::size_of_key(&s("size")), screens };
    let m = server(api);
    sb::set(&m.data, &file, set).map_err(|e| format!("{e:#}"))?;
    let said = format!("the bus {file}: {}", if set.is_empty() { "as the game guesses it".to_string() } else { format!("{:?}", set) });
    m.say(&format!("admin panel: {said}"));
    Ok(said)
}

/// (3 November 2026) The maps' markets as the main server keeps them (`<data>/market/<key>.json`).
fn markets_of(data: &std::path::Path) -> Vec<shared_tenders::Market> {
    let Ok(rd) = std::fs::read_dir(shared_tenders::dir(data)) else { return Vec::new() };
    let mut keys: Vec<String> = rd.flatten().filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".json").filter(|k| !k.contains('.')).map(str::to_string)).collect();
    keys.sort();
    keys.into_iter().filter_map(|k| shared_tenders::load(data, &k)).collect()
}

/// (3 November 2026) The server's competitors for the panel's "Competitors" (`competitors`): each
/// with its size, style, buses, quality and the contracts it holds now (on every map; at most its
/// size's on each); without a list the maps' drawn operators, to take over.
fn competitors_view(api: &Api) -> Value {
    use super::competitors::{self as cp, Size, Style};
    use super::server_buses::{self as sb, Floor};
    let data = server(api).data.clone();
    let list = cp::load(&data);
    let markets = markets_of(&data);
    let rows: Vec<Value> = list
        .list
        .iter()
        .map(|c| {
            let now: usize = markets.iter().map(|m| cp::holds(m, &c.name)).sum();
            json!({
                "id": c.id, "name": c.name, "size": c.made.size.key(), "style": c.made.style.key(), "quality": c.quality.round(),
                "fuels": c.made.fuels().iter().map(|d| sb::drive_key(*d)).collect::<Vec<_>>(),
                "floors": c.made.floors().iter().map(|f| f.key()).collect::<Vec<_>>(),
                "contracts": now, "most": c.made.size.most(), "maps": markets.len(),
            })
        })
        .collect();
    let drawn: Vec<Value> = markets
        .iter()
        .map(|m| json!({ "map": folder_name(&m.map), "key": m.key, "operators": m.rivals.iter().filter(|r| r.made.is_none()).map(|r| json!({ "name": r.name, "kind": r.character.label(), "quality": r.quality.round(), "contracts": cp::holds(m, &r.name) })).collect::<Vec<_>>() }))
        .collect();
    let drives: Vec<Value> = [Drive::Diesel, Drive::Cng, Drive::Electric].iter().map(|d| json!([sb::drive_key(*d), sb::drive_label(*d)])).collect();
    json!({
        "competitors": rows, "drawn": drawn,
        "choices": {
            "sizes": Size::ALL.iter().map(|s| json!([s.key(), s.label(), s.most()])).collect::<Vec<_>>(),
            "styles": Style::ALL.iter().map(|s| { let (a, b) = s.margin(); json!([s.key(), s.label(), (a * 100.0).round(), (b * 100.0).round()]) }).collect::<Vec<_>>(),
            "drives": drives,
            "floors": Floor::ALL.iter().map(|f| json!([f.key(), f.label()])).collect::<Vec<_>>(),
            "quality": cp::QUALITY,
        },
    })
}

/// (3 November 2026) The admin makes, changes or deletes a competitor (`do`: `put` - `id` 0 a new
/// one, `name`, `size`, `style`, `fuels` and `floors` (keys, a list or comma between), `quality`;
/// `delete` - `id`; `take_over` - every map's drawn operators into the list, by their characters).
/// The maps' markets take the list at their next step.
fn competitor_do(api: &Api, a: &Value) -> Result<String, String> {
    use super::competitors::{self as cp, Competitor, Made, Size, Style};
    use super::server_buses::{self as sb, Floor};
    let s = |k: &str| a.get(k).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).unwrap_or_default().trim().to_string();
    let keys = |k: &str| -> Vec<String> {
        match a.get(k) {
            Some(Value::Array(xs)) => xs.iter().filter_map(Value::as_str).map(|x| x.trim().to_string()).collect(),
            Some(Value::String(t)) => t.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(),
            _ => Vec::new(),
        }
    };
    let m = server(api);
    let mut list = cp::load(&m.data);
    let said = match s("do").as_str() {
        "put" => {
            let id = s("id").parse::<u32>().unwrap_or(0);
            let quality = s("quality").replace(',', ".").parse::<f64>().unwrap_or(cp::QUALITY);
            let made = Made {
                size: Size::of_key(&s("size")).unwrap_or_default(),
                style: Style::of_key(&s("style")).unwrap_or_default(),
                fuels: keys("fuels").iter().filter_map(|k| sb::drive_of_key(k)).collect(),
                floors: keys("floors").iter().filter_map(|k| Floor::of_key(k)).collect(),
            };
            let c = Competitor { id, name: s("name"), quality, made };
            let name = c.name.trim().to_string();
            list.put(c).map_err(str::to_string)?;
            format!("the competitor {name}: {}", if id == 0 { "made" } else { "changed" })
        }
        "delete" => {
            let id = s("id").parse::<u32>().unwrap_or(0);
            let name = list.delete(id).map_err(str::to_string)?;
            format!("the competitor {name} deleted: its contracts run out")
        }
        "take_over" => {
            let mut n = 0;
            for mk in markets_of(&m.data) {
                for r in mk.rivals.iter().filter(|r| r.made.is_none()) {
                    if list.list.iter().any(|x| x.name.eq_ignore_ascii_case(&r.name)) {
                        continue;
                    }
                    let (size, style) = match r.character {
                        super::auction::Character::Big => (Size::Large, Style::Normal),
                        super::auction::Character::Small => (Size::Small, Style::Normal),
                        super::auction::Character::Aggressive => (Size::Medium, Style::Aggressive),
                        super::auction::Character::Cautious => (Size::Medium, Style::Careful),
                    };
                    let made = Made { size, style, fuels: vec![Drive::Diesel], floors: vec![Floor::LowFloor, Floor::LowEntry] };
                    if list.put(Competitor { id: 0, name: r.name.clone(), quality: r.quality, made }).is_ok() {
                        n += 1;
                    }
                }
            }
            format!("{n} of the maps' operators taken over as competitors")
        }
        _ => return Err("put, delete or take_over".into()),
    };
    cp::save(&m.data, &list).map_err(|e| format!("{e:#}"))?;
    m.say(&format!("admin panel: {said}"));
    Ok(said)
}

/// (3 November 2026) What the admin asks of a tender's buses (`floor`, `fuel`, `max_age`,
/// `screens` of the request; none of them: None, the defaults).
fn needs_of(a: &Value) -> Option<super::server_buses::Needs> {
    use super::server_buses::{FloorRule, FuelRule, Needs};
    let s = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let given = ["floor", "fuel", "max_age", "screens"].iter().any(|k| a.get(*k).is_some_and(|v| !v.is_null() && v.as_str() != Some("")));
    if !given {
        return None;
    }
    let d = Needs::default();
    let max_age = a.get("max_age").and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|t| t.trim().parse::<f64>().ok()))).map_or(d.max_age, |x| x.clamp(0.0, 40.0) as u32);
    let screens = a.get("screens").is_some_and(|v| v.as_bool().unwrap_or(false) || v.as_str().is_some_and(|t| t == "1" || t == "true"));
    Some(Needs { floor: FloorRule::of_key(&s("floor")).unwrap_or(d.floor), fuel: FuelRule::of_key(&s("fuel")).unwrap_or(d.fuel), max_age, screens })
}

/// A tender's requirements for the panel: their keys and the words.
fn needs_json(n: &super::server_buses::Needs) -> Value {
    json!({ "floor": n.floor.key(), "fuel": n.fuel.key(), "max_age": n.max_age, "screens": n.screens, "text": format!("{} · {} · ≤ {} y{}", n.floor.label(), n.fuel.label(), n.max_age, if n.screens { " · screens" } else { "" }) })
}

/// A tender of map `map` (its market's key) as the panel asks (`a`: `do` and its arguments): its
/// close moved (`close`, `id`, `at` = `YYYY-MM-DD HH:MM`), called off (`cancel`, `id`), its cap set
/// (`cap`, `id`, `cap` in kroner a timetable hour), or lines put out to tender (`open`, `line` or
/// `lines`, and as he chooses `opens`, `closes` or `days`, `cap`). (3 November 2026) Also: a line
/// awarded directly to a company of the map (`direct`, `line`, `company`, `cap`, `drive`), an
/// operator's term moved (`operator_until`, `line`, `until`) or ended with the line out to tender
/// (`operator_end`, `line`), and a company's contract moved to end on another day
/// (`contract_until`, `company`, `line`, `until`: not before `RENEW_ANNOUNCED` days from today).
fn tender_do(api: &Api, map: &str, a: &Value) -> Result<String, String> {
    use shared_tenders::ServerAsk;
    let s = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let num = |k: &str| a.get(k).and_then(|v| v.as_f64().or_else(|| v.as_str().and_then(|t| t.trim().replace(',', ".").parse::<f64>().ok()))).filter(|x| x.is_finite() && *x > 0.0);
    let id = a.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
    let what = s("do");
    let m = server(api);
    let data = m.data.clone();
    let (today, now) = server_now(&m);
    let mut mk = shared_tenders::load(&data, map).ok_or("no such market")?;
    let open_at = |mk: &shared_tenders::Market| mk.tenders.iter().position(|t| t.decided.is_none() && t.tender.market.as_ref().is_some_and(|x| x.id == id)).ok_or_else(|| "no such open tender".to_string());
    let mut changed = true;
    let said = match what.as_str() {
        "open" => {
            changed = false;
            let mut lines: Vec<String> = a.get("lines").and_then(Value::as_array).map(|l| l.iter().filter_map(|x| x.as_str()).map(|x| x.trim().to_string()).collect()).unwrap_or_default();
            if lines.is_empty() {
                lines.push(s("line"));
            }
            lines.retain(|l| !l.is_empty());
            if lines.is_empty() {
                return Err("which line?".into());
            }
            let opens = if s("opens").is_empty() { 0 } else { moment_of(&s("opens"))? };
            let start = opens.max(now);
            let closes = if !s("closes").is_empty() {
                moment_of(&s("closes"))?
            } else {
                num("days").map_or(0, |d| start + (d.min(60.0) * clock::DAY as f64).round() as i64)
            };
            if closes > 0 && closes <= start {
                return Err("it closes before it opens".into());
            }
            // (3 November 2026: the reserve buses' share for it, per cent; 0 or none: the rules')
            let reserve = num("reserve").filter(|x| *x > 0.0).map_or(0.0, |x| x.min(100.0) / 100.0);
            // (3 November 2026: what it asks of the buses - floor, fuel, age, screens; none: the defaults)
            let needs = needs_of(a);
            let ask = ServerAsk { opens_at: opens, closes_at: closes, cap_hour: num("cap").map_or(0, Fines::cents), reserve, needs, ..ServerAsk::default() };
            // (3 November 2026: several lines with a package's name - one tender for them all)
            let package = s("package");
            if !package.is_empty() && lines.len() > 1 {
                let ask = ServerAsk { package: package.clone(), lines: lines.clone(), ..ask };
                shared_tenders::ask_as_server(&data, map, &lines[0], ask).map_err(|e| format!("{e:#}"))?;
                format!("the package {package} of {map} ({}): out to tender in a moment, unless a company or an operator runs one of its lines", lines.join(", "))
            } else {
                for l in &lines {
                    shared_tenders::ask_as_server(&data, map, l, ask.clone()).map_err(|e| format!("{e:#}"))?;
                }
                format!("line(s) {} of {map}: out to tender in a moment, unless a company or an operator runs them", lines.join(", "))
            }
        }
        "direct" => {
            changed = false;
            let (line, company) = (s("line"), s("company"));
            let cap = num("cap").ok_or("the price a timetable hour, in kroner")?;
            let c = store::load(&data, &company).map_err(|_| "no such company".to_string())?;
            if shared_tenders::key_of(&c.map) != map {
                return Err("that company is not on this map".into());
            }
            if line.is_empty() {
                return Err("which line?".into());
            }
            let drive = super::server_buses::drive_of_key(&s("drive")).unwrap_or(Drive::Diesel);
            let ask = ServerAsk { cap_hour: Fines::cents(cap), direct: c.id.clone(), direct_name: c.name.clone(), drive, ..ServerAsk::default() };
            shared_tenders::ask_as_server(&data, map, &line, ask).map_err(|e| format!("{e:#}"))?;
            format!("line {line} of {map}: awarded directly to {} at {cap:.2} kr. a timetable hour, at the next step", c.name)
        }
        "cap" => {
            let k = open_at(&mk)?;
            let cap = num("cap").ok_or("the cap a timetable hour, in kroner")?;
            let t = &mut mk.tenders[k].tender;
            t.reference = cn::cap_of_hour(t, Fines::cents(cap) as f64);
            format!("the tender of line {} of {map}: its cap is {cap:.2} kr. a timetable hour now", t.number)
        }
        "close" | "cancel" => {
            let k = open_at(&mk)?;
            if what == "cancel" {
                let t = mk.tenders.remove(k);
                format!("the tender of line {} of {map} is called off", t.tender.number)
            } else {
                let closes = moment_of(&s("at"))?;
                let t = &mut mk.tenders[k].tender;
                if closes <= t.opens_at {
                    return Err("it closes before it opens".into());
                }
                t.closes_at = closes;
                t.closes = clock::date_of(closes);
                format!("the tender of line {} of {map} closes {} {} now", t.number, t.closes, clock::hhmm(closes))
            }
        }
        "operator_until" => {
            let (line, until) = (s("line"), s("until"));
            if dates::parse(&until).is_none() || dates::between(&today, &until) < 0 {
                return Err("the last day as YYYY-MM-DD, today or later".into());
            }
            let h = mk.held.iter_mut().find(|h| h.line.eq_ignore_ascii_case(&line)).ok_or("no operator runs that line")?;
            h.until = until.clone();
            // (a day it gives the line up on after its new last day: none)
            if !h.gives_up.is_empty() && dates::between(&until, &h.gives_up) > 0 {
                h.gives_up.clear();
            }
            format!("the operator {} runs line {line} of {map} until {until} now", h.name)
        }
        "operator_end" => {
            let line = s("line");
            let k = mk.held.iter().position(|h| h.line.eq_ignore_ascii_case(&line)).ok_or("no operator runs that line")?;
            let h = mk.held.remove(k);
            shared_tenders::ask_as_server(&data, map, &h.line, ServerAsk::default()).map_err(|e| format!("{e:#}"))?;
            format!("the operator {} no longer runs line {line} of {map}: out to tender in a moment", h.name)
        }
        "contract_until" => {
            changed = false;
            let (company, line, until) = (s("company"), s("line"), s("until"));
            let earliest = dates::add(&today, shared_tenders::RENEW_ANNOUNCED);
            if dates::parse(&until).is_none() || dates::between(&earliest, &until) < 0 {
                return Err(format!("the last day as YYYY-MM-DD, {earliest} or later: the line's next tender needs its days"));
            }
            let c = store::load(&data, &company).map_err(|_| "no such company".to_string())?;
            if !c.concessions.held.iter().any(|h| h.line.eq_ignore_ascii_case(&line)) {
                return Err("the company holds no contract on that line".into());
            }
            shared_tenders::order_contract(&data, &company, &line, &until).map_err(|e| format!("{e:#}"))?;
            format!("the contract of {} on line {line} of {map} ends {until} (taken at the company's next step)", c.name)
        }
        // (3 November 2026: an extra bus ordered behind a departure of a company's contract line,
        // for a concert say - taken at its next step, on its duty board from its first day)
        "doubling" => {
            changed = false;
            let (company, line, tour, at, from, until) = (s("company"), s("line"), s("tour"), s("at"), s("from"), s("until"));
            let dep = at.split_once(':').and_then(|(h, m)| Some(h.trim().parse::<i32>().ok()? * 60 + m.trim().parse::<i32>().ok()?)).filter(|x| (0..1440).contains(x)).ok_or("the departure as HH:MM")?;
            if tour.trim().is_empty() {
                return Err("which tour?".into());
            }
            let from = if from.trim().is_empty() { today.clone() } else { from };
            let until = if until.trim().is_empty() { from.clone() } else { until };
            if dates::parse(&from).is_none() || dates::parse(&until).is_none() || dates::between(&from, &until) < 0 || dates::between(&today, &from) < 0 {
                return Err(format!("the days as YYYY-MM-DD, from {today} on, the last not before the first"));
            }
            let c = store::load(&data, &company).map_err(|_| "no such company".to_string())?;
            if !c.concessions.contracts.iter().any(|k| k.line.eq_ignore_ascii_case(&line) && k.live()) {
                return Err("the company holds no contract on that line".into());
            }
            super::doubling::order_by_admin(&data, &company, &super::doubling::AdminOrder { line: line.clone(), tour: tour.trim().to_string(), dep, from: from.clone(), until: until.clone() }).map_err(|e| format!("{e:#}"))?;
            format!("an extra bus behind the {at} of tour {} on line {line} of {}, {from} to {until} (taken at the company's next step)", tour.trim(), c.name)
        }
        // (3 November 2026) a rail replacement of the map's ordered (H9), a complaint decided (H11)
        "rail" => {
            let price = num("price").ok_or("the fixed price a timetable hour, in kroner")?;
            let direct = match s("company") {
                x if x.is_empty() => None,
                x => {
                    let c = store::load(&data, &x).map_err(|_| "no such company".to_string())?;
                    if shared_tenders::key_of(&c.map) != map {
                        return Err("that company is not on this map".into());
                    }
                    Some((c.id.clone(), c.name.clone()))
                }
            };
            super::server_awards::order_rail(&mut mk, &s("line"), &s("from"), &s("until"), Fines::cents(price), direct, &today)?
        }
        "complaint" => super::server_awards::rule(&data, &mut mk, id, &s("company"), s("verdict") == "uphold")?,
        _ => return Err("open, direct, cap, close, cancel, operator_until, operator_end, contract_until, doubling, rail or complaint".into()),
    };
    if changed {
        shared_tenders::save(&data, &mk).map_err(|e| format!("{e:#}"))?;
    }
    m.say(&format!("admin panel: {said}"));
    Ok(said)
}

/// (NexiconDK, 3 November 2026) What may happen to a contract, from the admin panel
/// (`contract_events`): a company's contract changed (`change`: the map's timetable taken into
/// use, a line added or taken out - measured on the map's week), the server's own holidays
/// (`holiday_add`, `holiday_remove`) and roadworks on a line (`roadworks`, `roadworks_end`).
fn events_do(api: &Api, a: &Value) -> Result<String, String> {
    use super::contract_events as ev;
    let s = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
    let int = |k: &str| a.get(k).and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|t| t.trim().parse::<i64>().ok()))).unwrap_or(0);
    let m = server(api);
    let data = m.data.clone();
    let (today, _) = server_now(&m);
    let said = match s("do").as_str() {
        "holiday_add" => {
            let p = ev::add_period(&data, &s("name"), &s("from"), &s("until"))?;
            format!("holidays {}: {} to {}", p.name, p.from, p.until)
        }
        "holiday_remove" => {
            let p = ev::remove_period(&data, int("id") as u32)?;
            format!("holidays {} taken away", p.name)
        }
        "roadworks" => {
            let map = s("map");
            let mk = shared_tenders::load(&data, &map).ok_or("no such market")?;
            let from = if s("from").is_empty() { today.clone() } else { s("from") };
            let lines = crate::lines_in_map_dir(&ms::map_dir_of(&m.cfg.root, &mk.map), &from).map_err(|e| format!("{e:#}"))?;
            let rw = ev::Roadwork { map: map.clone(), line: s("line"), from_stop: s("from_stop"), to_stop: s("to_stop"), from, until: s("until"), minutes: int("minutes") as i32, ..Default::default() };
            let rw = ev::add_roadwork(&data, rw, &lines)?;
            format!("roadworks on line {} of {map}, {} to {}, from {} to {}: {} stops closed, +{} minutes", rw.number, rw.from_stop, rw.to_stop, rw.from, rw.until, rw.closed.len(), rw.minutes)
        }
        "roadworks_end" => {
            let rw = ev::end_roadwork(&data, int("id") as u32)?;
            format!("the roadworks on line {} ended", rw.number)
        }
        // (H5: snow, a storm or a strike - force majeure)
        "majeure" => {
            let kind = ev::Majeure::of_key(&s("kind")).ok_or("snow, storm or strike")?;
            let from = if s("from").is_empty() { today.clone() } else { s("from") };
            let fm = ev::add_majeure(&data, ev::ForceMajeure { kind, from, until: s("until"), map: s("map"), company: s("company"), weather: s("weather"), ..Default::default() })?;
            let whose = if fm.kind == ev::Majeure::Strike { if fm.company.is_empty() { ", every company".to_string() } else { format!(", company {}", fm.company) } } else { String::new() };
            format!("force majeure: {} from {} to {}{}{whose}", fm.kind.key(), fm.from, fm.until, if fm.map.is_empty() { String::new() } else { format!(" on {}", fm.map) })
        }
        "majeure_end" => {
            let fm = ev::end_majeure(&data, int("id") as u32)?;
            format!("the force majeure ({}, {} to {}) ended", fm.kind.key(), fm.from, fm.until)
        }
        "change" => {
            let (company, line, other) = (s("company"), s("line"), s("other"));
            let kind = match s("kind").as_str() {
                "add" => ev::ChangeKind::AddLine,
                "remove" => ev::ChangeKind::RemoveLine,
                _ => ev::ChangeKind::Timetable,
            };
            let c = store::load(&data, &company).map_err(|_| "no such company".to_string())?;
            let dir = ms::map_dir_of(&m.cfg.root, &c.map);
            let day = dates::parse(&today).ok_or("the server's date")?;
            let monday = day - dates::weekday(day) as i64;
            let mut days = Vec::new();
            for k in 0..7 {
                days.push(crate::lines_in_map_dir(&dir, &dates::fmt(monday + k)).map_err(|e| format!("{e:#}"))?);
            }
            let ch = ev::measure(&c, kind, &line, &other, &days, &today, ev::must_accept(&data))?;
            shared_tenders::order_change(&data, &company, &ch).map_err(|e| format!("{e:#}"))?;
            let may = if ch.decline_by.is_empty() { String::new() } else { format!(", the company may decline it until {}", ch.decline_by) };
            format!("the contract {} of {} changes from {}: {:.0} → {:.0} timetable hours a week, {} → {} buses{may} (taken at its next step)", ch.name, c.name, ch.from, ch.hours_before, ch.hours_after, ch.buses_before, ch.buses_after)
        }
        // (NexiconDK, 3 November 2026, K11: the supplements for unsocial hours)
        "supplement_add" => {
            let hm = |k: &str| {
                let v = s(k);
                let (h, mi) = v.split_once(':').unwrap_or((v.as_str(), "0"));
                h.trim().parse::<i32>().unwrap_or(0).clamp(0, 23) * 60 + mi.trim().parse::<i32>().unwrap_or(0).clamp(0, 59)
            };
            let num = |k: &str| s(k).replace(',', ".").trim().parse::<f64>().unwrap_or(0.0);
            let when = super::live_more::When::of(&s("when"), hm("from"), hm("to")).ok_or("evening, night, saturday, sunday or snow")?;
            let id = super::live_more::add_supplement(&data, &s("name"), when, num("percent"), num("per_hour"))?;
            format!("supplement {} ({}): {} % and {} kr. an hour (#{id})", s("name").trim(), when.key(), num("percent"), num("per_hour"))
        }
        "supplement_remove" => {
            let x = super::live_more::remove_supplement(&data, int("id") as u32)?;
            format!("supplement {} taken away", x.name)
        }
        _ => return Err("change, holiday_add, holiday_remove, roadworks, roadworks_end, majeure, majeure_end, supplement_add or supplement_remove".into()),
    };
    m.say(&format!("admin panel: {said}"));
    Ok(said)
}

#[cfg(test)]
mod tests;
