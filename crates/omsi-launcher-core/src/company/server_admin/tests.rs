use super::*;
use crate::company::main_server::{MainCfg, MainServer};
use crate::company::server_api::{self, kept, Verify, API_VERSION, HELLO, PATH};
use crate::company::server_players::Account;
use std::time::Duration;

fn temp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("openomsi-bco-admin-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn verify() -> Verify {
    Box::new(|t: &str| match t {
        "token-mathias" => Ok(Account { user_id: "u-1".into(), name: "Mathias".into() }),
        _ => Err(crate::company::server_players::ACCOUNT_REFUSED),
    })
}

/// A main server on an OMSI folder with Lemmental (no game program: no world starts), its door.
fn api(dir: &std::path::Path) -> (Arc<Api>, String) {
    let root = dir.join("omsi");
    std::fs::create_dir_all(root.join("maps/Lemmental")).unwrap();
    std::fs::write(root.join("maps/Lemmental/global.cfg"), "").unwrap();
    let data = dir.join("bco");
    // (a settings file of before the panel: its part added)
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(data.join(ms::CFG_FILE), format!("root = {}\r\nreal_time = 0\r\ndate = 2024-03-04\r\ngame = {}\r\nadmin_password = geheim-123\r\nmax_worlds = 4\r\n", root.display(), dir.join("no-game").display())).unwrap();
    let cfg = MainCfg::load(&data).unwrap();
    assert!(!cfg.web_admin);
    let m = MainServer::open(&data, cfg).unwrap();
    let k = kept(&data, 0).unwrap();
    (Arc::new(Api::new(Arc::new(Mutex::new(m)), k.secret_bytes(), verify())), k.secret)
}

fn get(base: &str, path: &str, headers: &[(&str, &str)]) -> (u16, String) {
    omsi_net::http::request("GET", &format!("{base}{path}"), headers, None, Duration::from_secs(5)).unwrap()
}

fn post(base: &str, path: &str, headers: &[(&str, &str)], body: &str) -> (u16, String) {
    omsi_net::http::request("POST", &format!("{base}{path}"), headers, Some(body), Duration::from_secs(5)).unwrap()
}

/// The panel on the players' door: its page and script for anyone here, its questions only with
/// the admin password (the launcher's, from this machine) - from elsewhere not at all unless
/// `web_admin = 1`; the door's own questions answered as ever.
#[test]
fn the_panel_on_the_players_door() {
    let dir = temp("door");
    let (api, secret) = api(&dir);
    let a = api.clone();
    let server = super::super::together::server::Server::start_with(0, true, PATH, HELLO, Arc::new(move |b: &str| a.answer(b, unix_now()).to_string()), Some(extra(api.clone(), "geheim-123", false))).unwrap();
    let base = format!("http://127.0.0.1:{}", server.port());
    let pw = [("X-Admin-Password", "geheim-123")];
    // the page: the game servers' login with this panel
    let (code, page) = get(&base, "/admin", &[]);
    assert_eq!(code, 200);
    assert!(page.contains("'/admin/bco-panel.js'") && page.contains("company-server.cfg") && !page.contains("'/admin/panel.js'"));
    let (code, js) = get(&base, "/admin/bco-panel.js", &[]);
    assert!(code == 200 && js.contains("export function mountPanel") && js.contains("from './panel.js'"));
    let (code, js) = get(&base, "/admin/panel.js", &[]);
    assert!(code == 200 && js.contains("export function settingsView"));
    // nothing without the password; nothing from elsewhere
    assert_eq!(get(&base, "/admin/api/overview", &[]).0, 401);
    assert_eq!(get(&base, "/admin/api/overview", &[("X-Admin-Password", "falsch-123")]).0, 401);
    assert_eq!(get(&base, "/admin/api/overview", &[("X-Admin-Password", "geheim-123"), ("CF-Connecting-IP", "203.0.113.9")]).0, 403);
    assert_eq!(get(&base, "/admin", &[("X-Forwarded-For", "203.0.113.9")]).0, 403);
    // the overview, the worlds
    let (code, o) = get(&base, "/admin/api/overview", &pw);
    let o: Value = serde_json::from_str(&o).unwrap();
    assert_eq!(code, 200, "{o}");
    assert_eq!((o["date"].as_str(), o["companies"].as_u64(), o["players"].as_u64()), (Some("2024-03-04"), Some(0), Some(0)));
    let (_, w) = get(&base, "/admin/api/worlds", &pw);
    let w: Value = serde_json::from_str(&w).unwrap();
    assert_eq!(w["maps"][0]["folder"], "Lemmental");
    assert!(w["maps"][0]["world"].is_null());
    // a console line
    let (code, c) = post(&base, "/admin/api/command", &pw, "maps");
    assert_eq!(code, 200);
    assert!(c.contains("Lemmental"), "{c}");
    // its log: what the main server said, the panel's line among it
    let (_, l) = get(&base, "/admin/api/log?from=0", &pw);
    assert!(l.contains("admin panel: maps"), "{l}");
    // the settings: written whole, read back
    let (_, cfg) = get(&base, "/admin/api/config", &pw);
    let cfg: Value = serde_json::from_str(&cfg).unwrap();
    assert!(cfg["text"].as_str().unwrap().contains("web_admin = 0"), "the panel's setting added to an older file");
    let text = cfg["text"].as_str().unwrap().replace("max_worlds = 4", "max_worlds = 6");
    assert_eq!(post(&base, "/admin/api/config", &pw, &text).0, 200);
    assert!(std::fs::read_to_string(dir.join("bco").join(ms::CFG_FILE)).unwrap().contains("max_worlds = 6\r\n"));
    // the door's own questions as before
    let v = server_api::ask(&base, &json!({ "v": API_VERSION, "secret": secret, "do": "info" }), Duration::from_secs(5)).unwrap();
    assert_eq!(v["app"], "bus-company-online");
    drop(server);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A player banned from the panel is not let in again (told why), his session ended; let in
/// again, he logs in as before; his computers' keys forgotten. Players online: those with a
/// session.
#[test]
fn a_banned_player_is_not_let_in() {
    let dir = temp("ban");
    let (api, secret) = api(&dir);
    let login = || api.answer(&json!({ "v": API_VERSION, "secret": secret, "do": "login", "account": "token-mathias", "name": "Mathias" }).to_string(), unix_now());
    let v = login();
    assert_eq!(v["ok"], true, "{v}");
    let id = v["player"]["id"].as_str().unwrap().to_string();
    assert_eq!(api.online(), [id.clone()]);
    let p = players(&api);
    assert_eq!(p["players"][0]["online"], true);
    assert!(player_do(&api, &id, "ban", "cheating on the fines").unwrap().contains("banned"));
    assert!(api.online().is_empty(), "his session ended");
    let v = login();
    assert_eq!((v["ok"].as_bool(), v["error"].as_str()), (Some(false), Some(server_api::BANNED)));
    assert_eq!(v["args"]["why"], "cheating on the fines");
    let p = players(&api);
    assert_eq!(p["players"][0]["banned"], "cheating on the fines");
    player_do(&api, &id, "unban", "").unwrap();
    assert_eq!(login()["ok"], true);
    // (kept in players.json; not written for a player never banned)
    let file = std::fs::read_to_string(dir.join("bco").join(crate::company::server_players::PLAYERS_FILE)).unwrap();
    assert!(!file.contains("banned"), "{file}");
    api.server.lock().unwrap().players.get_mut(&id).unwrap().keys = vec!["k1".into(), "k2".into()];
    player_do(&api, &id, "keys", "").unwrap();
    assert!(api.server.lock().unwrap().players.get(&id).unwrap().keys.is_empty());
    assert!(player_do(&api, "p-nobody", "ban", "").is_err());
    assert!(player_do(&api, &id, "explode", "").is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A map's open tender: its close moved (not before it opens), called off; a line put out to
/// tender now goes in as the server's application.
#[test]
fn a_tender_moved_called_off_and_opened() {
    let dir = temp("tender");
    let (api, _) = api(&dir);
    let data = dir.join("bco");
    let opens = clock::moment("2024-03-04", 8 * 60);
    let c = crate::company::found(&crate::company::Founding { name: "Umove".into(), date: "2024-03-04".into(), map: "maps/Lemmental/global.cfg".into(), ..Default::default() }, "owner");
    let mut t = cn::make_tender(&c, &crate::company::concessions::tests::line("Linie5", "5", 3, true), 3, opens, false, "lemmental", &[], (1, 2));
    t.closes_at = opens + 3 * clock::DAY;
    t.market = Some(cn::Shared { key: "lemmental".into(), id: 3, ..Default::default() });
    let m = shared_tenders::Market { key: "lemmental".into(), map: "maps/Lemmental/global.cfg".into(), tenders: vec![shared_tenders::MarketTender { tender: t, applied: Vec::new(), decided: None, complaints: Vec::new() }], ..Default::default() };
    shared_tenders::save(&data, &m).unwrap();
    assert!(tender_do(&api, "lemmental", &json!({ "id": 3, "do": "close", "at": "2024-03-09 12:30", "line": "" })).unwrap().contains("2024-03-09 12:30"));
    let m = shared_tenders::load(&data, "lemmental").unwrap();
    assert_eq!((m.tenders[0].tender.closes_at, m.tenders[0].tender.closes.as_str()), (clock::moment("2024-03-09", 12 * 60 + 30), "2024-03-09"));
    assert!(tender_do(&api, "lemmental", &json!({ "id": 3, "do": "close", "at": "2024-03-01 12:30", "line": "" })).is_err(), "before it opens");
    assert!(tender_do(&api, "lemmental", &json!({ "id": 3, "do": "close", "at": "9 March", "line": "" })).is_err());
    assert!(tender_do(&api, "lemmental", &json!({ "id": 4, "do": "cancel", "at": "", "line": "" })).is_err(), "no such tender");
    // the market as the panel shows it: the bids sealed, how many alone
    let v = market(&api);
    assert!(v["maps"].as_array().unwrap().is_empty(), "no company of the map: its market not listed");
    tender_do(&api, "lemmental", &json!({ "id": 3, "do": "cancel", "at": "", "line": "" })).unwrap();
    assert!(shared_tenders::load(&data, "lemmental").unwrap().tenders.is_empty());
    tender_do(&api, "lemmental", &json!({ "id": 0, "do": "open", "at": "", "line": "Linie7" })).unwrap();
    let asks = std::fs::read_to_string(shared_tenders::dir(&data).join("lemmental.asks.jsonl")).unwrap();
    assert!(asks.contains("Linie7") && asks.contains(shared_tenders::SERVER_ASKS), "{asks}");
    assert!(tender_do(&api, "nowhere", &json!({ "do": "open", "line": "Linie7" })).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_query_value_undone() {
    assert_eq!(param("world=Ahlheim%205&x=1", "world"), "Ahlheim 5");
    assert_eq!(param("company=a+b&from=3", "from"), "3");
    assert_eq!(param("company=a+b", "company"), "a b");
    assert_eq!(param("x=%zz%", "x"), "%zz%");
    assert_eq!(param("", "x"), "");
}

/// (3 November 2026) The admin's competitors: the maps' drawn operators shown and taken over by
/// their characters, one added with its size, style and buses, changed and deleted; the view
/// counts each one's contracts on the maps.
#[test]
fn the_admin_makes_takes_over_changes_and_deletes_competitors() {
    let dir = temp("competitors");
    let (api, _) = api(&dir);
    let data = dir.join("bco");
    let mut m = shared_tenders::Market { key: "lemmental".into(), map: "maps/Lemmental/global.cfg".into(), rivals: cn::make_rivals(super::super::rng::Rng::of(&["lemmental"], 0)), ..Default::default() };
    let first = m.rivals[0].name.clone();
    m.held.push(shared_tenders::OperatorHold { line: "Linie5".into(), name: first.clone(), ..Default::default() });
    shared_tenders::save(&data, &m).unwrap();
    let v = competitors_view(&api);
    assert!(v["competitors"].as_array().unwrap().is_empty());
    let drawn = &v["drawn"][0];
    assert_eq!((drawn["map"].as_str(), drawn["operators"].as_array().unwrap().len()), (Some("Lemmental"), m.rivals.len()));
    assert!(competitor_do(&api, &json!({ "do": "take_over" })).unwrap().starts_with(&format!("{} of the maps' operators", m.rivals.len())));
    let v = competitors_view(&api);
    let rows = v["competitors"].as_array().unwrap();
    assert_eq!(rows.len(), m.rivals.len());
    let row = rows.iter().find(|r| r["name"] == first.as_str()).unwrap();
    assert_eq!((row["contracts"].as_u64(), row["fuels"][0].as_str()), (Some(1), Some("diesel")));
    assert!(competitor_do(&api, &json!({ "do": "take_over" })).unwrap().starts_with("0 of"), "not twice");
    // a new one, changed and deleted
    competitor_do(&api, &json!({ "do": "put", "id": 0, "name": "Nordbus", "size": "large", "style": "aggressive", "fuels": ["cng", "electric"], "floors": "low_floor", "quality": "72" })).unwrap();
    let list = super::super::competitors::load(&data);
    let n = list.list.iter().find(|c| c.name == "Nordbus").unwrap().clone();
    assert_eq!((n.made.size, n.made.style, n.made.fuels.clone(), n.quality), (super::super::competitors::Size::Large, super::super::competitors::Style::Aggressive, vec![Drive::Cng, Drive::Electric], 72.0));
    assert!(competitor_do(&api, &json!({ "do": "put", "id": 0, "name": "nordbus" })).is_err(), "the name once");
    competitor_do(&api, &json!({ "do": "put", "id": n.id, "name": "Nordbus", "size": "small", "style": "careful", "fuels": [], "floors": [], "quality": 50 })).unwrap();
    assert_eq!(super::super::competitors::load(&data).list.iter().find(|c| c.id == n.id).unwrap().made.size, super::super::competitors::Size::Small);
    assert!(competitor_do(&api, &json!({ "do": "delete", "id": n.id })).unwrap().contains("Nordbus deleted"));
    assert!(competitor_do(&api, &json!({ "do": "delete", "id": n.id })).is_err());
    assert!(competitor_do(&api, &json!({ "do": "fly" })).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
