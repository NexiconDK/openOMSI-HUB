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
        s.linked = Some(Linked { text: text.trim().to_string(), code, base, session, info, player, companies: Vec::new() });
        s.error = None;
        s.note = note;
    }
    refresh()
}

/// A question of the logged-in player's (`what`, with `extra`): its answer, logged in again once
/// when the login ran out.
fn question(what: &str, extra: Value) -> Result<Value, (String, Vec<(String, String)>)> {
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

/// (Phase 3) The world of company `company`'s map, to drive a duty in: asked of the main server
/// (it starts the world when it does not run), and where this computer joins it - the server's
/// own address with the world's web port on a network, its tunnel over the internet. `{address}`
/// once it answers, `{waiting}` while it starts, `{error}`.
fn world(company: &str) -> Value {
    let base = state().linked.as_ref().map(|l| l.base.clone());
    let Some(base) = base else { return json!({ "error": tr(NOT_LINKED) }) };
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
                Some(a) => json!({ "address": a, "map": v.get("map") }),
                None => json!({ "waiting": tr("The map's world has no way in over the internet yet: its tunnel is starting.") }),
            }
        }
        Ok(v) => json!({ "waiting": trf("Starting the world of %{map} on the server… (about a minute)", &[("map", v.get("map").and_then(Value::as_str).unwrap_or(""))]) }),
        Err((e, a)) => json!({ "error": words_of(&e, &a) }),
    }
}

/// The page's words for a refusal and its arguments.
fn words_of(e: &str, args: &[(String, String)]) -> String {
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
        "join": tr("Join"), "found": tr("Found a company"), "refresh": tr("Refresh"), "forget": tr("Log out"),
        "money": tr("Your money"), "players": tr("Players"), "server_time": tr("Server time"),
        "found_title": tr("Found a company on the server"), "name": tr("Name"), "map": tr("Map"), "mode": tr("Game mode"),
        "found_text": tr("The company stays on its map for good. You are its owner, with every permission."),
        "price": tr("Founding costs %{price} of your own money: you have %{money}."), "found_go": tr("Found"), "cancel": tr("Cancel"),
        "mine": tr("Yours"), "your_role": tr("Your role"), "owner": tr("Owner"),
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
        "me": { "name": l.player.get("name").and_then(Value::as_str).unwrap_or(""), "money": euros(money * 100), "by": by.join(" + "), "can_found": money >= price },
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
        texts.push(super::together::SERVER_CODE.to_string());
        texts.push(omsi_launcher_lib::company::together::host::NO_TICKET.to_string());
        for lead in ["tr(\"", "trf(\""] {
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
