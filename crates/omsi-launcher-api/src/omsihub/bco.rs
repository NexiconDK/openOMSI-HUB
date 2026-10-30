//! Bus Company Online, the launcher's side (NexiconDK, phase 2 step 2 of
//! docs/omsihub/BUS_COMPANY_ONLINE.md; the server's side is `omsi_launcher_lib::company::server_api`):
//! the third way to play at the start of the bus company, beside one's own companies and a
//! friend's (`bco.js`'s page).
//!
//! The player enters the server's code once (`HUB-…`, its owner gives it: `status` on the main
//! server shows it; kept in `~/.openomsi/bco.json`). The launcher finds the server (its addresses,
//! then its tunnel) and logs him in with what this computer has: his Omsi-Hub account
//! (`online::account_for_server`: email or Google, the access token checked by the server once),
//! the Steam ID this computer's Steam was last signed in with (`omsi_cfg::install_search::steam_user`)
//! and the key the server gave this computer for it the first time (kept in `bco.json` beside the
//! server's session) - both together make them one player on the server. He sees the server's
//! companies, joins one with a click (the server hands the company's code and its ticket for him:
//! `together::join::join_server_company` - the company's menu is "Play together"'s), and founds
//! one on a map of the server's from his own money there.
//!
//! * `bco_view` - what the page shows (no network);
//! * `bco_connect {code}` - the server found, logged in, its companies;
//! * `bco_refresh` - its companies again (logged in again when the login ran out);
//! * `bco_join {company}` - the company joined (the page then shows "Play together"'s menu);
//! * `bco_found {name, map, mode}` - a company founded;
//! * `bco_forget` - the server's code forgotten, logged out;
//! * `bco_world {company}` - (phase 3) the world of the company's map, to drive a duty in: its
//!   address once it answers (Drive then joins it: `together::join`'s `drive` with `world`).
//!
//! Every text comes in the page's language (`lang`); the server's refusals are keys of the
//! translations (`server_api::texts`).

use super::company::words::{self, euros, tr, trf};
use super::online;
use super::together;
use crate::Answer;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::model::GameMode;
use omsi_launcher_lib::company::server_api::{self, API_VERSION};
use omsi_launcher_lib::company::together::code::CompanyCode;
use omsi_launcher_lib::company::together::Role;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

/// The kill switch: Bus Company Online's tile at the bus company's start is shown unless this is
/// set, or the launcher runs with `OMSI_BCO_OFF` (it is for the server's owner to try out now).
pub const KILLED: bool = false;

/// Bus Company Online is on.
pub fn enabled() -> bool {
    !KILLED && !omsi_cfg::flags::OMSI_BCO_OFF.is_set() && together::enabled()
}

/// The server the launcher is logged in to.
struct Linked {
    text: String,
    code: CompanyCode,
    base: String,
    session: String,
    info: Value,
    player: Value,
    companies: Vec<Value>,
    /// (Phase 6) What founding asks of him and how far he is (`server_found::status_json`).
    found: Value,
}

/// What the page shows: the server linked, and what was said last.
#[derive(Default)]
struct State {
    linked: Option<Linked>,
    /// What the server or the launcher said last (English, with its arguments), and a note of
    /// the login.
    error: Option<(String, Vec<(String, String)>)>,
    note: Option<String>,
}

static STATE: Mutex<State> = Mutex::new(State { linked: None, error: None, note: None });

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|e| e.into_inner())
}

/// What `bco.json` keeps: the server's code, and the keys the servers gave this computer for its
/// Steam IDs (by the server's session and the Steam ID).
fn kept_path() -> PathBuf {
    core::data_dir().join("bco.json")
}

fn kept() -> Value {
    std::fs::read(kept_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}))
}

fn keep(f: impl FnOnce(&mut Value)) {
    let mut v = kept();
    f(&mut v);
    if let Some(d) = kept_path().parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(kept_path(), serde_json::to_vec_pretty(&v).unwrap_or_default());
}

fn key_name(code: &CompanyCode, steam: &str) -> String {
    format!("{:012x}|{steam}", code.session)
}

/// This computer's Steam user (the OMSI folder's Steam first).
fn steam() -> Option<omsi_cfg::install_search::SteamUser> {
    let root = core::load_config().root;
    omsi_cfg::install_search::steam_user((!root.trim().is_empty()).then(|| std::path::Path::new(&root)))
}

fn ask(base: &str, mut body: Value, code: &CompanyCode) -> Result<Value, String> {
    body["v"] = json!(API_VERSION);
    body["secret"] = json!(code.secret_hex());
    server_api::ask(base, &body, server_api::TIMEOUT).map_err(|_| server_api::UNREACHED.to_string())
}

/// A refusal's English and its arguments (the price and money of `NOT_ENOUGH`).
fn refusal_of(v: &Value) -> (String, Vec<(String, String)>) {
    let e = v.get("error").and_then(Value::as_str).unwrap_or(server_api::UNKNOWN).to_string();
    let mut args = Vec::new();
    for k in ["price", "money"] {
        if let Some(n) = v.get(k).and_then(Value::as_i64) {
            args.push((k.to_string(), euros(n * 100)));
        }
    }
    // (phase 6: the rest of its numbers - trips, a date)
    if let Some(o) = v.get("args").and_then(Value::as_object) {
        for (k, x) in o {
            args.push((k.clone(), x.as_str().map(str::to_string).unwrap_or_else(|| x.to_string())));
        }
    }
    (e, args)
}

fn ok(v: &Value) -> bool {
    v.get("ok").and_then(Value::as_bool) == Some(true)
}

/// Log in at the server (`base`, of `code`) with what this computer has: the session, the
/// player, and a note of the server's. A new key for the Steam ID is kept.
fn login(base: &str, code: &CompanyCode) -> Result<(String, Value, Option<String>), (String, Vec<(String, String)>)> {
    let account = online::account_for_server();
    let steam = steam();
    let key = steam.as_ref().and_then(|s| kept().get("keys").and_then(|k| k.get(key_name(code, &s.id))).and_then(Value::as_str).map(str::to_string)).unwrap_or_default();
    let name = account.as_ref().map(|a| a.1.clone()).filter(|n| !n.trim().is_empty()).or_else(|| steam.as_ref().map(|s| s.name.clone())).unwrap_or_else(|| core::load_config().profile);
    let body = json!({ "do": "login", "name": name, "account": account.as_ref().map(|a| a.0.clone()).unwrap_or_default(), "steam": steam.as_ref().map(|s| s.id.clone()).unwrap_or_default(), "key": key });
    let v = ask(base, body, code).map_err(|e| (e, Vec::new()))?;
    if !ok(&v) {
        return Err(refusal_of(&v));
    }
    if let (Some(new), Some(s)) = (v.get("key").and_then(Value::as_str), steam.as_ref()) {
        let name = key_name(code, &s.id);
        keep(|k| {
            if !k.get("keys").is_some_and(Value::is_object) {
                k["keys"] = json!({});
            }
            k["keys"][name] = json!(new);
        });
    }
    let session = v.get("session").and_then(Value::as_str).unwrap_or("").to_string();
    Ok((session, v.get("player").cloned().unwrap_or(Value::Null), v.get("note").and_then(Value::as_str).map(str::to_string)))
}

/// The server of `text` found, logged in, its companies taken.
fn connect(text: &str) -> Value {
    let code = match CompanyCode::decode(text) {
        Ok(c) if c.company == server_api::server_tag() => c,
        Ok(_) => return fail(NOT_SERVER_CODE, Vec::new()),
        Err(e) => return fail(e, Vec::new()),
    };
    let (base, info) = match server_api::find_server(&code, &json!({ "v": API_VERSION, "secret": code.secret_hex(), "do": "info" })) {
        Ok(x) => x,
        Err(e) => return fail(&e, Vec::new()),
    };
    if !ok(&info) {
        let (e, a) = refusal_of(&info);
        return fail(&e, a);
    }
    keep(|k| k["server"] = json!(text.trim()));
    let (session, player, note) = match login(&base, &code) {
        Ok(x) => x,
        Err((e, a)) => return fail(&e, a),
    };
    {
        let mut s = state();
        s.linked = Some(Linked { text: text.trim().to_string(), code, base, session, info, player, companies: Vec::new(), found: Value::Null });
        s.error = None;
        s.note = note;
    }
    refresh()
}

/// A question of the logged-in player's (`what`, with `extra`): its answer, logged in again once
/// when the login ran out.
pub(super) fn question(what: &str, extra: Value) -> Result<Value, (String, Vec<(String, String)>)> {
    let Some((base, code, session)) = state().linked.as_ref().map(|l| (l.base.clone(), l.code.clone(), l.session.clone())) else { return Err((NOT_LINKED.to_string(), Vec::new())) };
    let body = |session: &str| {
        let mut b = extra.clone();
        b["do"] = json!(what);
        b["session"] = json!(session);
        b
    };
    let mut v = ask(&base, body(&session), &code).map_err(|e| (e, Vec::new()))?;
    if v.get("error").and_then(Value::as_str) == Some(server_api::LOGGED_OUT) {
        let (session, player, _) = login(&base, &code)?;
        if let Some(l) = state().linked.as_mut() {
            l.session = session.clone();
            l.player = player;
        }
        v = ask(&base, body(&session), &code).map_err(|e| (e, Vec::new()))?;
    }
    if !ok(&v) {
        return Err(refusal_of(&v));
    }
    Ok(v)
}

/// The server's companies again (and its news).
fn refresh() -> Value {
    let info = {
        let s = state();
        s.linked.as_ref().map(|l| (l.base.clone(), l.code.clone()))
    };
    if let Some((base, code)) = info {
        if let Ok(i) = ask(&base, json!({ "do": "info" }), &code) {
            if ok(&i) {
                if let Some(l) = state().linked.as_mut() {
                    l.info = i;
                }
            }
        }
    }
    match question("companies", json!({})) {
        Ok(v) => {
            let mut s = state();
            if let Some(l) = s.linked.as_mut() {
                l.companies = v.get("companies").and_then(Value::as_array).cloned().unwrap_or_default();
                l.player = v.get("player").cloned().unwrap_or(Value::Null);
                l.found = v.get("found").cloned().unwrap_or(Value::Null);
            }
            s.error = None;
            drop(s);
            view()
        }
        Err((e, a)) => fail(&e, a),
    }
}

fn fail(e: &str, args: Vec<(String, String)>) -> Value {
    state().error = Some((e.to_string(), args));
    view()
}

/// Join a company of the server: its code and the ticket from the server, joined as "Play
/// together" joins a friend's.
fn join(company: &str) -> Value {
    match question("join", json!({ "company": company })) {
        Ok(v) => {
            let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            let joined = together::join_server_company(&s("code"), &s("player"), &s("name"), &s("ticket"));
            if let Some(e) = joined.get("error").and_then(Value::as_str) {
                return json!({ "error": e });
            }
            json!({ "joined": true })
        }
        Err((e, a)) => json!({ "error": words_of(&e, &a) }),
    }
}

/// Found a company on a map of the server's.
fn found(args: &Value) -> Value {
    let s = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match question("found", json!({ "company": s("name"), "map": s("map"), "mode": s("mode") })) {
        Ok(_) => {
            let v = refresh();
            json!({ "ok": true, "message": trf("%{name} is founded: join it to run it.", &[("name", &s("name"))]), "view": v })
        }
        Err((e, a)) => json!({ "error": words_of(&e, &a) }),
    }
}

/// (Phase 6) The leadership course booked (`course`), or a driving test for a class of bus
/// (`test` with `class`; none: the one booked called off): the page again, or `{error}`.
fn needs(what: &str, extra: Value) -> Value {
    match question(what, extra) {
        Ok(v) => {
            if let Some(l) = state().linked.as_mut() {
                l.player = v.get("player").cloned().unwrap_or(Value::Null);
                l.found = v.get("found").cloned().unwrap_or(Value::Null);
            }
            view()
        }
        Err((e, a)) => json!({ "error": words_of(&e, &a) }),
    }
}

/// The bus classes a driving test is for (`career::LicenceClass`): their keys and names.
fn classes() -> Vec<Value> {
    omsi_launcher_lib::company::career::LicenceClass::ALL.iter().map(|c| json!({ "key": serde_json::to_value(c).unwrap_or(Value::Null), "label": tr(c.label()) })).collect()
}

/// The name of a class of bus by its key.
fn class_label(key: &Value) -> String {
    serde_json::from_value::<omsi_launcher_lib::company::career::LicenceClass>(key.clone()).map(|c| tr(c.label())).unwrap_or_default()
}

/// (Phase 6) What founding asks, each with how far he is and what he can do about it.
fn needs_json(f: &Value) -> Vec<Value> {
    let mut rows = Vec::new();
    let t = &f["trips"];
    let n = |v: &Value| v.as_u64().unwrap_or(0).to_string();
    if t["need"].as_u64().unwrap_or(0) > 0 {
        rows.push(json!({ "key": "trips", "ok": t["ok"], "text": trf("Trips driven for a company on the server: %{done} of %{need}", &[("done", &n(&t["done"])), ("need", &n(&t["need"]))]) }));
    }
    let c = &f["course"];
    if c["need"].as_bool() == Some(true) {
        let text = if c["ok"].as_bool() == Some(true) {
            tr("Leadership course: taken")
        } else if c["running"].as_bool() == Some(true) {
            trf("Leadership course: until %{date}", &[("date", c["until"].as_str().unwrap_or(""))])
        } else {
            trf("Leadership course: %{price}, %{days} days", &[("price", &euros(c["price"].as_i64().unwrap_or(0) * 100)), ("days", &n(&c["days"]))])
        };
        let book = c["ok"].as_bool() != Some(true) && c["running"].as_bool() != Some(true);
        rows.push(json!({ "key": "course", "ok": c["ok"], "text": text, "book": book.then(|| tr("Book the course")) }));
    }
    let x = &f["test"];
    if x["need"].as_bool() == Some(true) {
        let passed: Vec<String> = x["passed"].as_array().map(|a| a.iter().map(class_label).collect()).unwrap_or_default();
        let last = &x["last"];
        let text = if !passed.is_empty() {
            trf("Driving test: passed (%{classes})", &[("classes", &passed.join(", "))])
        } else if !x["booked"].is_null() {
            trf("Driving test booked: %{class} - your next trip with such a bus is the test", &[("class", &class_label(&x["booked"]))])
        } else if last["passed"].as_bool() == Some(false) {
            trf("Driving test: not passed (%{score} %) - book it again", &[("score", &n(&last["score"]))])
        } else {
            tr("Driving test: not taken")
        };
        let open = passed.is_empty() && x["booked"].is_null();
        rows.push(json!({ "key": "test", "ok": x["ok"], "text": text, "book": open.then(|| tr("Book the test")), "cancel": (!x["booked"].is_null()).then(|| tr("Call it off")), "classes": classes() }));
    }
    rows
}

/// (Phase 3) The world of company `company`'s map, to drive a duty in: asked of the main server
/// (it starts the world when it does not run), and where this computer joins it - the server's
/// own address with the world's web port on a network, its tunnel over the internet. `{address}`
/// once it answers, `{waiting}` while it starts (30 October 2026: with `{loading}` - the map, how
/// long it has loaded and took the last time, what it loads now - for the launcher's loading
/// screen, whose `{words}` every answer has), `{error}`.
fn world(company: &str) -> Value {
    let words = world_words();
    let base = state().linked.as_ref().map(|l| l.base.clone());
    let Some(base) = base else { return json!({ "error": tr(NOT_LINKED), "words": words }) };
    let loading = |v: &Value, phase: String| json!({ "map": v.get("map").and_then(Value::as_str).unwrap_or(""), "since": v.get("since").and_then(Value::as_u64).unwrap_or(0), "expected": v.get("expected").and_then(Value::as_u64), "phase": phase });
    match question("world", json!({ "company": company })) {
        Ok(v) if v.get("ready").and_then(Value::as_bool) == Some(true) => {
            let tunnel = v.get("tunnel").and_then(Value::as_str).map(str::to_string);
            let web = v.get("web_port").and_then(Value::as_u64).unwrap_or(0);
            // (the main server reached on a network: its machine, the world's web port; reached
            // through its tunnel: the world's own tunnel)
            let address = match base.strip_prefix("http://").and_then(|h| h.split(['/', ':']).next()).filter(|h| !h.is_empty()) {
                Some(host) if web > 0 => Some(format!("http://{host}:{web}")),
                _ => tunnel,
            };
            match address {
                Some(a) => json!({ "address": a, "map": v.get("map"), "words": words }),
                None => json!({ "waiting": tr("The map's world has no way in over the internet yet: its tunnel is starting."), "loading": loading(&v, tr("Opening its way in over the internet")), "words": words }),
            }
        }
        // (NexiconDK, 27 October 2026: until it runs - a big map loads for minutes -, said with how
        // long it has been loading, once a minute)
        Ok(v) => {
            let map = v.get("map").and_then(Value::as_str).unwrap_or("");
            let min = v.get("since").and_then(Value::as_u64).unwrap_or(0) / 60;
            let phase = match v.get("phase").and_then(Value::as_str).unwrap_or("") {
                "map" => tr("Loading the map: its tiles and objects"),
                "traffic" => tr("Laying out the roads and the traffic"),
                "timetable" => tr("Setting out the timetable and the companies' buses"),
                "people" => tr("Putting people at the stops"),
                _ => tr("Starting the world on the server"),
            };
            json!({ "waiting": if min == 0 { trf("Starting the world of %{map} on the server… (a big map takes a few minutes)", &[("map", map)]) } else { trf("The world of %{map} is still loading on the server: %{n} min so far.", &[("map", map), ("n", &min.to_string())]) }, "loading": loading(&v, phase), "words": words })
        }
        Err((e, a)) => json!({ "error": words_of(&e, &a), "words": words }),
    }
}

/// (30 October 2026) The words of the launcher's loading screen while a map's world starts.
fn world_words() -> Value {
    json!({
        "title": tr("Loading the world"),
        "of": tr("The world of %{map}"),
        "so_far": tr("%{t} so far"),
        "last": tr("about %{n} min the last time"),
        "note": tr("The game starts by itself as soon as the world is ready. You may leave this open."),
        "asking": tr("Asking the server for the world…"),
        "ready": tr("The world is ready: the game starts…"),
        "cancel": tr("Stop waiting"),
        "cancelled": tr("The world goes on loading on the server: press Drive again when you like."),
    })
}

/// (NexiconDK, phase 6 step 5) The players' guide to Bus Company Online, in sections: what the
/// founding asks as this server has it (its door's answers; the defaults before it is linked).
fn guide() -> Value {
    let (found, price) = {
        let s = state();
        let l = s.linked.as_ref();
        (l.map(|l| l.found.clone()).unwrap_or(Value::Null), l.and_then(|l| l.info.get("found_price").and_then(Value::as_i64)))
    };
    let d = omsi_launcher_lib::company::server_found::Needs::default();
    let trips = found["trips"]["need"].as_u64().unwrap_or(d.trips as u64);
    let course = found["course"]["need"].as_bool().unwrap_or(d.course);
    let course_price = found["course"]["price"].as_i64().unwrap_or(d.course_price / 100);
    let days = found["course"]["days"].as_i64().unwrap_or(d.course_days);
    let test = found["test"]["need"].as_bool().unwrap_or(d.test);
    let mut founding = vec![trf("Founding a company costs %{price} of your own money.", &[("price", &euros(price.unwrap_or(10_000) * 100))])];
    if trips > 0 {
        founding.push(trf("First you drive %{n} trips for a company on the server.", &[("n", &trips.to_string())]));
    }
    if course {
        founding.push(trf("You take the leadership course: %{price}, %{days} days on the server's clock.", &[("price", &euros(course_price * 100)), ("days", &days.to_string())]));
    }
    if test {
        founding.push(tr("And you pass a driving test on the server: book it in the found dialog - your next trip with a bus of that class is the test, to be driven to its end, on time, smoothly and safely."));
    }
    let sections = [
        (tr("What it is"), tr("Bus Company Online is one big server where the players' bus companies run around the clock, on the server's clock. Each map has a world of its own: everybody on a map drives in the same world, among every company's buses in their own colours. Where nobody drives, the AI drives the companies' tours.")),
        (tr("Getting in"), tr("Ask the server's owner for its code (HUB-…) and enter it here. You log in with your Omsi-Hub account, this computer's Steam account, or both as one player. Your money on the server is your own: you start with some and earn more by driving.")),
        (tr("Joining a company"), format!("{} {} {}", tr("Join opens a company's pages with the role its owner gave you: driver, planner, manager or co-owner. A driver gets his duties in the planning; Drive starts the map's world on the server if it is not running and puts you on your duty with the company's bus."), tr("Under Duties you take an open duty, or take one over from the company's AI drivers: from its start, or from its next trip when it is under way."), tr("A duty that begins with the bus in service starts on foot at its stop: the bus comes there, its driver gets out, and G at the front door takes the wheel. Not there within five minutes of the departure, the AI drives that trip and you take over at the next."))),
        (tr("Driving and pay"), tr("The world measures every trip you drive: it has to see your bus on that tour. The company books the trip and keeps its share (its owner sets it); the rest is your pay, into your money on the server. Running late? Report the delay on your duty: a departure more than 15 minutes late that was not reported is fined.")),
        (tr("Founding your own company"), founding.join(" ")),
        (tr("Lines and tenders"), tr("On each map a line belongs to one company at most, won in a tender. A tender is open for three days. You bid a price per timetable hour - at most the authority's cap - with diesel buses (a two-week contract) or electric ones (a month); the lowest price weighed with quality wins. A renewal is a new tender: it opens six days before the term ends and is decided three days before.")),
        (tr("The contract's rules"), tr("Each tour is run with the bus size its timetable asks for or a bigger one, and a contract won with electric buses with electric buses. The authority fines journeys not run, a bus too small, departures more than a minute early or more than 15 minutes late without a report, and complaints. A company that wins a line from another is offered the other's buses of it at their book value.")),
        (tr("The control room"), tr("While the map's world runs, the fleet map and the Operations page show every bus where it is in the world, with its real delay; otherwise where the timetable has it. Service control can run a late bus out of service to its terminus, cancel a trip, or swap in a spare bus at the terminus - and the bus in the world does it.")),
        (tr("Depots"), tr("A company that does well opens a depot on another map: its own lines, buses, staff and money there. The company becomes a group: a head office with no map of its own above its depots, the first map's included. The head office keeps the group's treasury and its loans - the bank lends on the whole group - sends money to the depots by hand or by their rules, moves buses and lends staff between the maps, and approves a depot's big purchases.")),
        (tr("In the red"), tr("A depot that closes its days in the red is warned twice, then goes bankrupt, and its head office covers its loss. A head office in the red gets the bank's emergency loan; in the red again while that loan runs, it is warned twice and then goes bankrupt, its depots with it.")),
        (tr("The server's clock"), tr("The server runs on its machine's own time, summer time too, and nobody can change it. A company's day closes at midnight; when the server was off, the time is caught up.")),
    ];
    json!({
        "title": tr("Bus Company Online: the guide"), "close": tr("Close"),
        "sections": sections.iter().map(|(t, x)| json!({ "title": t, "text": x })).collect::<Vec<_>>(),
    })
}

/// The page's words for a refusal and its arguments.
pub(super) fn words_of(e: &str, args: &[(String, String)]) -> String {
    let refs: Vec<(&str, &str)> = args.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    if refs.is_empty() {
        tr(e)
    } else {
        trf(e, &refs)
    }
}

/// Why the page cannot go on (English: keys of the translations).
pub const NOT_SERVER_CODE: &str = "This is not a Bus Company Online server's code. Ask the server's owner for its code (HUB-…): his main server shows it.";
pub const NOT_LINKED: &str = "Enter the server's code first.";

fn mode_label(m: &Value) -> String {
    let mode: GameMode = serde_json::from_value(m.clone()).unwrap_or_default();
    tr(mode.label())
}

fn company_json(c: &Value) -> Value {
    let s = |k: &str| c.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let n = |k: &str| c.get(k).and_then(Value::as_u64).unwrap_or(0);
    let role = c.get("role").and_then(Value::as_str).and_then(Role::of);
    let mine = c.get("mine").and_then(Value::as_bool).unwrap_or(false);
    let runs = c.get("runs").and_then(Value::as_bool).unwrap_or(false);
    // (no short name: its initials)
    let short = Some(s("short")).filter(|x| !x.trim().is_empty()).unwrap_or_else(|| s("name").split_whitespace().filter_map(|w| w.chars().next()).take(3).collect::<String>().to_uppercase());
    json!({
        "id": s("id"), "name": s("name"), "short": short, "colours": c.get("colours").cloned().unwrap_or(json!([])),
        "map": s("map"), "mode": mode_label(&c["mode"]), "date": s("date"),
        "owner": if mine { tr("You") } else if s("owner").is_empty() { "-".to_string() } else { s("owner") },
        "mine": mine, "role": role.map(|r| tr(r.label())), "runs": runs,
        "state": if runs { tr("Open") } else { tr("Starting on the server…") },
        "figures": trf("%{lines} lines · %{buses} buses · %{staff} staff · %{players} players", &[("lines", &n("lines").to_string()), ("buses", &n("buses").to_string()), ("staff", &n("staff").to_string()), ("players", &n("players").to_string())]),
        // (phase 4: a depot of a head office)
        "depot_of": (!s("hq").is_empty()).then(|| trf("Depot of %{hq}", &[("hq", &s("hq_name"))])),
        // (the head office's id: its depots are folded under it, and chosen from at Join - the
        // company's figures summed over them)
        "hq": s("hq"), "lines": n("lines"), "buses": n("buses"), "staff": n("staff"),
        // (its lines' numbers and colours: a driver chooses a company by them)
        "line_list": c.get("line_list").cloned().unwrap_or(json!([])),
        // (phase 7: a group's head office - no map, its depots under it; its owner opens it)
        "group": c.get("group").and_then(Value::as_bool).unwrap_or(false),
        "may_run": c.get("may_run").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// What the page shows.
fn view() -> Value {
    let s = state();
    let saved = kept().get("server").and_then(Value::as_str).unwrap_or("").to_string();
    let steam = steam();
    let ways = json!({ "steam": steam.as_ref().map(|u| trf("Steam: %{name}", &[("name", &u.name)])) });
    let words = json!({
        "title": tr("Bus Company Online"), "back": tr("Play mode"),
        "text": tr("Every company of a big server, on its maps, around the clock. Log in with your Omsi-Hub account or your Steam account, join a company or found your own."),
        "code_field": tr("The server's code (HUB-…)"), "connect": tr("Connect"), "connecting": tr("Connecting…"),
        "need": [tr("The server's owner gives you its code: his main server shows it."), tr("You log in with the Omsi-Hub account you are signed in with on the Hub page, or with this computer's Steam account - or with both, as one player.")],
        "companies_caps": tr("The server's companies"), "none": tr("No company on the server yet: found the first."),
        "join": tr("Join"), "found": tr("Found a company"), "refresh": tr("Refresh"), "forget": tr("Log out"), "guide": tr("The guide"),
        "money": tr("Your money"), "players": tr("Players"), "server_time": tr("Server time"),
        "found_title": tr("Found a company on the server"), "name": tr("Name"), "map": tr("Map"), "mode": tr("Game mode"),
        "found_text": tr("The company stays on its map for good. You are its owner, with every permission."),
        "price": tr("Founding costs %{price} of your own money: you have %{money}."), "found_go": tr("Found"), "cancel": tr("Cancel"),
        "mine": tr("Yours"), "your_role": tr("Your role"), "owner": tr("Owner"),
        // (a head office's depots, folded under it; Join asks which department)
        "show_depots": tr("Show depots"), "hide_depots": tr("Hide depots"),
        "which_title": tr("Which department?"), "which_text": tr("The company works on more than one map. Choose the department you join: the head office or one of its depots."),
        "head_office": tr("Head office"), "depot": tr("Depot"),
        "group_figures": tr("%{lines} lines · %{buses} buses · %{depots} depots"),
        // (phase 7: a group's head office, its owner's page)
        "hq": tr("Head office"),
        "modes": [{ "key": "casual", "label": tr("Casual") }, { "key": "normal", "label": tr("Normal") }, { "key": "realistic", "label": tr("Realistic") }],
    });
    let error = s.error.as_ref().map(|(e, a)| words_of(e, a));
    let Some(l) = s.linked.as_ref() else {
        return json!({ "words": words, "linked": false, "code": saved, "ways": ways, "problem": error });
    };
    let i = &l.info;
    let money = l.player.get("money").and_then(Value::as_i64).unwrap_or(0);
    let price = i.get("found_price").and_then(Value::as_i64).unwrap_or(0);
    let maps: Vec<Value> = i.get("maps").and_then(Value::as_array).cloned().unwrap_or_default();
    let by: Vec<String> = [l.player.get("account").and_then(Value::as_bool) == Some(true), l.player.get("steam").and_then(Value::as_bool) == Some(true)]
        .iter()
        .zip([tr("Omsi-Hub account"), tr("Steam")])
        .filter(|(on, _)| **on)
        .map(|(_, w)| w)
        .collect();
    json!({
        "words": words, "linked": true, "code": l.text, "ways": ways, "problem": error, "note": s.note.as_deref().map(tr),
        "server": { "name": i.get("name").and_then(Value::as_str).unwrap_or(""), "time": format!("{} {}", i.get("date").and_then(Value::as_str).unwrap_or(""), i.get("time").and_then(Value::as_str).unwrap_or("")), "players": i.get("players").and_then(Value::as_u64).unwrap_or(0) },
        "me": { "name": l.player.get("name").and_then(Value::as_str).unwrap_or(""), "money": euros(money * 100), "by": by.join(" + "), "can_found": money >= price && l.found.get("can").and_then(Value::as_bool) != Some(false) },
        // (phase 6: what founding asks besides its price)
        "needs": needs_json(&l.found), "needs_caps": tr("What founding asks"),
        "price": trf("Founding costs %{price} of your own money: you have %{money}.", &[("price", &euros(price * 100)), ("money", &euros(money * 100))]),
        "maps": maps.iter().map(|m| json!({ "key": m["folder"], "label": m["folder"] })).collect::<Vec<_>>(),
        "companies": l.companies.iter().map(company_json).collect::<Vec<_>>(),
    })
}

/// A command of Bus Company Online, or None when `cmd` is not one.
pub fn call(cmd: &str, args: &Value) -> Option<Answer> {
    words::set_language(args.get("lang").and_then(Value::as_str).unwrap_or(""));
    if !enabled() {
        return Some(Ok(json!({ "off": true })));
    }
    let s = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Some(Ok(match cmd {
        "bco_view" => view(),
        "bco_connect" => connect(&s("code")),
        "bco_refresh" => refresh(),
        "bco_join" => join(&s("company")),
        "bco_found" => found(args),
        "bco_world" => world(&s("company")),
        "bco_course" => needs("course", json!({})),
        "bco_guide" => guide(),
        "bco_test" => needs("test", json!({ "class": s("class") })),
        "bco_forget" => {
            *state() = State::default();
            keep(|k| {
                if let Some(o) = k.as_object_mut() {
                    o.remove("server");
                }
            });
            view()
        }
        _ => return None,
    }))
}

#[cfg(test)]
mod tests {
    /// Every text of Bus Company Online - the launcher's side and the server's refusals - in the
    /// six languages.
    #[test]
    fn every_online_server_text_is_translated() {
        let own = include_str!("bco.rs").split("#[cfg(test)]").next().unwrap_or("");
        let mut texts: Vec<String> = omsi_launcher_lib::company::server_api::texts().into_iter().map(String::from).collect();
        // (the map's shared tenders and the depots: refusals the pages say)
        use omsi_launcher_lib::company::{concessions, server_depots, shared_tenders};
        texts.extend(server_depots::texts().into_iter().chain([shared_tenders::HELD, shared_tenders::ASKED, concessions::NOT_ON_SERVER, concessions::NO_ELECTRIC]).map(String::from));
        // (the buses of a line changing company: its refusals and messages)
        use omsi_launcher_lib::company::takeover;
        texts.extend([omsi_launcher_lib::company::server_fines::NOT_DRIVING, omsi_launcher_lib::company::server_fines::REPORTED].map(String::from));
        texts.extend(server_depots::red_texts().into_iter().map(String::from));
        // (phase 6 step 3: a depot's big purchases, its head office's to approve)
        texts.extend(server_depots::approval_texts().into_iter().map(String::from));
        // (phase 7: a group's head office - its refusals, and its page's words)
        texts.extend(omsi_launcher_lib::company::server_hq::texts().into_iter().map(String::from));
        let hq = include_str!("bco_hq.rs").split("#[cfg(test)]").next().unwrap_or("");
        texts.extend(takeover::texts().into_iter().chain([takeover::OFFERED.0, takeover::OFFERED.1, takeover::TAKEN.0, takeover::TAKEN.1, takeover::REFUSED.0, takeover::REFUSED.1]).map(String::from));
        texts.push(super::together::SERVER_CODE.to_string());
        texts.push(omsi_launcher_lib::company::together::host::NO_TICKET.to_string());
        for (lead, own) in ["tr(\"", "trf(\""].into_iter().flat_map(|l| [(l, own), (l, hq)]) {
            let mut rest = own;
            while let Some(k) = rest.find(lead) {
                let after = &rest[k + lead.len()..];
                rest = after;
                let mut t = String::new();
                let mut chars = after.chars();
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => t.extend(chars.next()),
                        '"' => break,
                        c => t.push(c),
                    }
                }
                if t.chars().filter(|c| c.is_alphabetic()).count() >= 2 {
                    texts.push(t);
                }
            }
        }
        for t in [super::NOT_SERVER_CODE, super::NOT_LINKED, super::together::relay::SERVER_CLOCK] {
            texts.push(t.to_string());
        }
        assert!(texts.len() > 40, "only {} texts found", texts.len());
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        missing.sort();
        missing.dedup();
        assert!(missing.is_empty(), "{} missing (crates/omsihub-locales/locales/company-online.yml):\n{}", missing.len(), missing.join("\n"));
    }
}
