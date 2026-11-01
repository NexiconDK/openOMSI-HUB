//! (NexiconDK, 1 November 2026) A dedicated server's admin panel in a browser, on its web port:
//! `GET /admin` is the page (`admin_page.html` with the panel of `launcher/ui/js/omsihub/server/
//! panel.js`, the same the launcher's Multiplayer → My server shows), `/admin/api/…` what it asks:
//!
//! * `GET challenge` - a challenge, a salt and the rounds; `POST login` (`<challenge>\n<answer>`)
//!   with the answer the game's administration gives too (`omsi_lan::admin`: HMAC-SHA256 of
//!   `omsi2rw-admin` and the challenge under PBKDF2-SHA256 of the password): a token, kept while it
//!   is used (`X-Admin-Token`). The password never goes over the network; five wrong answers in
//!   two minutes close the door for a while, as the game's and `POST /admin`'s do.
//! * `GET state` - the server now: its status, its players, the clock, the weather, the traffic
//!   (`AdminWeb::state`, which the server puts together every second);
//! * `GET log?from=<n>` - its log since line n (`push_log`: the last thousand lines);
//! * `POST command` - admin commands, one a line, as the game's Administration menu sends them
//!   (`kick 3`, `clock 30600`, `weather next`, `say …`; and `stop`): done by the server's loop;
//! * `GET config` / `POST config` - its `server.cfg` as text, written whole (read at the next
//!   start).
//!
//! On this machine alone unless `server.cfg` says `web_admin = 1`: a tunnel or a proxy here
//! connects from the loopback too, and what came through one is from elsewhere (`from_here`).
//! The launcher on this machine asks with the password itself (`X-Admin-Password`).

use crate::ws::{header, json_str, players_json, ServerInfo};
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The rounds of the key's derivation, as the game's administration (`omsi_lan::admin`).
pub const KDF_ROUNDS: u32 = 600_000;
/// A token is kept this long unused.
const TOKEN_IDLE: Duration = Duration::from_secs(12 * 3600);
/// A challenge is answered within this.
const CHALLENGE_LIFE: Duration = Duration::from_secs(120);
/// The log's last lines kept.
const LOG_KEEP: usize = 1000;
/// The largest body taken (a `server.cfg`).
pub const BODY_MAX: usize = 64 * 1024;
/// Headers a tunnel or a proxy adds (what came through one is not from this machine).
pub(crate) const PROXY_HEADERS: [&str; 5] = ["cf-connecting-ip", "cf-ray", "x-forwarded-for", "forwarded", "x-real-ip"];

/// The page and the panel.
pub const PAGE: &str = include_str!("admin_page.html");
pub const PANEL_JS: &str = include_str!("../../../launcher/ui/js/omsihub/server/panel.js");

/// What the gateway keeps for the panel.
#[derive(Debug, Clone, Default)]
pub struct AdminWeb {
    /// `web_admin = 1`: from elsewhere than this machine too.
    pub remote: bool,
    /// The server's `server.cfg`.
    pub cfg_path: Option<PathBuf>,
    pub started: Option<Instant>,
    /// What the server put together last (a JSON object: `state_json` in omsi-app's server).
    pub state: Arc<String>,
    sessions: Vec<(String, Instant)>,
    challenges: Vec<(String, [u8; 16], Instant)>,
}

static LOG: Mutex<(u64, VecDeque<String>)> = Mutex::new((0, VecDeque::new()));

/// A line of the program's log, for the panel (the last `LOG_KEEP` kept).
pub fn push_log(line: String) {
    let mut l = LOG.lock().unwrap_or_else(|e| e.into_inner());
    l.0 += 1;
    l.1.push_back(line);
    while l.1.len() > LOG_KEEP {
        l.1.pop_front();
    }
}

/// The log's lines from line `from` on (those still kept), and the number of the next.
pub fn log_since(from: u64) -> (Vec<String>, u64) {
    let l = LOG.lock().unwrap_or_else(|e| e.into_inner());
    let next = l.0;
    let first = next - l.1.len() as u64;
    let skip = from.max(first).min(next) - first;
    (l.1.iter().skip(skip as usize).cloned().collect(), next)
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The key of `password` (PBKDF2-SHA256), as the game's administration derives it.
pub fn derive(password: &str, salt: &[u8], rounds: u32) -> [u8; 32] {
    let mut key = [0u8; 32];
    let rounds = std::num::NonZeroU32::new(rounds.max(1)).expect("not zero");
    ring::pbkdf2::derive(ring::pbkdf2::PBKDF2_HMAC_SHA256, rounds, salt, password.as_bytes(), &mut key);
    key
}

/// The answer to `challenge` under `key` (hex).
pub fn answer(key: &[u8; 32], challenge: &str) -> String {
    let mut ctx = ring::hmac::Context::with_key(&ring::hmac::Key::new(ring::hmac::HMAC_SHA256, key));
    ctx.update(b"omsi2rw-admin");
    ctx.update(challenge.as_bytes());
    hex(ctx.sign().as_ref())
}

/// A header of a request's head (`name`, any case).
pub fn header_of<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    header(head, name)
}

/// Whether the request came from this machine, not through a tunnel or a proxy here.
pub fn from_here(head: &str, peer: Option<SocketAddr>) -> bool {
    peer.is_some_and(|p| p.ip().is_loopback()) && !PROXY_HEADERS.iter().any(|h| header(head, h).is_some())
}

/// An answer: its status, its content type, its body.
pub type Answer = (&'static str, &'static str, Vec<u8>);

pub fn json(status: &'static str, body: String) -> Answer {
    (status, "application/json; charset=utf-8", body.into_bytes())
}

pub fn error(status: &'static str, text: &str) -> Answer {
    json(status, format!("{{\"error\":{}}}", json_str(text)))
}

/// A request of `/admin…` (`route` without its query `query`): the page, its script, the API.
pub(crate) fn serve(route: &str, query: &str, head: &str, body: &str, peer: Option<SocketAddr>, info: &Mutex<ServerInfo>) -> Answer {
    let post = head.starts_with("POST ");
    let here = from_here(head, peer);
    let remote = info.lock().unwrap_or_else(|e| e.into_inner()).admin.remote;
    if !here && !remote {
        let why = "The admin panel opens on the server's own machine only. web_admin = 1 in server.cfg opens it from elsewhere too, with the admin password.";
        return match route {
            "/admin" | "/admin/" => ("403 Forbidden", "text/html; charset=utf-8", format!("<!doctype html><meta charset=utf-8><title>Admin</title><body style=\"font-family:sans-serif;background:#16181c;color:#eee;padding:40px\"><p>{why}</p>").into_bytes()),
            _ => error("403 Forbidden", why),
        };
    }
    match route {
        "/admin" | "/admin/" => ("200 OK", "text/html; charset=utf-8", PAGE.as_bytes().to_vec()),
        "/admin/panel.js" => ("200 OK", "text/javascript; charset=utf-8", PANEL_JS.as_bytes().to_vec()),
        "/admin/api/challenge" => challenge(info),
        "/admin/api/login" if post => login(body, info),
        _ => {
            let Some(token) = authorized(head, here, info) else { return error("401 Unauthorized", "log in with the admin password") };
            match (route, post) {
                ("/admin/api/state", _) => json("200 OK", state(info)),
                ("/admin/api/log", _) => {
                    let from = query.split('&').find_map(|kv| kv.strip_prefix("from=")).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
                    let (lines, next) = log_since(from);
                    json("200 OK", format!("{{\"lines\":[{}],\"next\":{next}}}", lines.iter().map(|l| json_str(l)).collect::<Vec<_>>().join(",")))
                }
                ("/admin/api/command", true) => {
                    let commands: Vec<String> = body.lines().map(str::trim).filter(|l| !l.is_empty()).take(10).map(|l| l.chars().take(200).collect()).collect();
                    let n = commands.len();
                    info.lock().unwrap_or_else(|e| e.into_inner()).local_admin_queue.extend(commands);
                    json("202 Accepted", format!("{{\"taken\":{n}}}"))
                }
                ("/admin/api/config", false) => match info.lock().unwrap_or_else(|e| e.into_inner()).admin.cfg_path.clone() {
                    Some(p) => match std::fs::read_to_string(&p) {
                        Ok(t) => json("200 OK", format!("{{\"text\":{},\"path\":{}}}", json_str(&t), json_str(&p.to_string_lossy()))),
                        Err(e) => error("500 Internal Server Error", &e.to_string()),
                    },
                    None => error("404 Not Found", "no server.cfg"),
                },
                ("/admin/api/config", true) => {
                    let Some(p) = info.lock().unwrap_or_else(|e| e.into_inner()).admin.cfg_path.clone() else { return error("404 Not Found", "no server.cfg") };
                    if body.len() >= BODY_MAX || body.trim().is_empty() {
                        return error("400 Bad Request", "no server.cfg in the request, or too long");
                    }
                    let tmp = p.with_extension("cfg.tmp");
                    match std::fs::write(&tmp, body.replace("\r\n", "\n").replace('\n', "\r\n")).and_then(|_| std::fs::rename(&tmp, &p)) {
                        Ok(()) => {
                            log::info!("admin panel: server.cfg written (read at the next start)");
                            json("200 OK", "{\"ok\":true}".into())
                        }
                        Err(e) => error("500 Internal Server Error", &e.to_string()),
                    }
                }
                ("/admin/api/logout", true) => {
                    info.lock().unwrap_or_else(|e| e.into_inner()).admin.sessions.retain(|s| s.0 != token);
                    json("200 OK", "{\"ok\":true}".into())
                }
                _ => error("404 Not Found", "no such request"),
            }
        }
    }
}

/// The token the request gives (kept on), or the admin password from this machine (the
/// launcher's): None when neither is good.
fn authorized(head: &str, here: bool, info: &Mutex<ServerInfo>) -> Option<String> {
    let mut i = info.lock().unwrap_or_else(|e| e.into_inner());
    if i.local_admin_password.is_empty() {
        return None;
    }
    let now = Instant::now();
    i.admin.sessions.retain(|s| now.duration_since(s.1) < TOKEN_IDLE);
    if let Some(t) = header(head, "x-admin-token") {
        if let Some(s) = i.admin.sessions.iter_mut().find(|s| crate::ws::same_secret(&s.0, t)) {
            s.1 = now;
            return Some(s.0.clone());
        }
    }
    if here && header(head, "x-admin-password").is_some_and(|p| crate::ws::same_secret(p, &i.local_admin_password)) {
        return Some(String::new());
    }
    None
}

/// Too many wrong passwords lately (the game's and `POST /admin`'s counted with them).
fn locked(i: &mut ServerInfo) -> bool {
    i.local_admin_failures.retain(|t| t.elapsed() < crate::ws::ADMIN_LOCK_WINDOW);
    i.local_admin_failures.len() >= crate::ws::ADMIN_LOCK_AFTER
}

fn challenge(info: &Mutex<ServerInfo>) -> Answer {
    let mut i = info.lock().unwrap_or_else(|e| e.into_inner());
    if i.local_admin_password.is_empty() {
        return error("404 Not Found", "this server has no admin password (admin_password in server.cfg)");
    }
    if locked(&mut i) {
        return error("429 Too Many Requests", "too many wrong passwords: try again in two minutes");
    }
    let c = hex(&crate::http::random_bytes::<16>());
    let salt = crate::http::random_bytes::<16>();
    let now = Instant::now();
    i.admin.challenges.retain(|x| now.duration_since(x.2) < CHALLENGE_LIFE);
    while i.admin.challenges.len() >= 16 {
        i.admin.challenges.remove(0);
    }
    i.admin.challenges.push((c.clone(), salt, now));
    json("200 OK", format!("{{\"challenge\":{},\"salt\":{},\"rounds\":{KDF_ROUNDS}}}", json_str(&c), json_str(&hex(&salt))))
}

/// `<challenge>\n<answer>`: a token for a right answer.
fn login(body: &str, info: &Mutex<ServerInfo>) -> Answer {
    let mut it = body.lines().map(str::trim);
    let (c, a) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
    let (password, salt) = {
        let mut i = info.lock().unwrap_or_else(|e| e.into_inner());
        if locked(&mut i) {
            return error("429 Too Many Requests", "too many wrong passwords: try again in two minutes");
        }
        let Some(k) = i.admin.challenges.iter().position(|x| x.0 == c && x.2.elapsed() < CHALLENGE_LIFE) else { return error("401 Unauthorized", "the login took too long: try again") };
        let (_, salt, _) = i.admin.challenges.remove(k);
        (i.local_admin_password.clone(), salt)
    };
    // (the key derived without the lock: it takes a moment)
    let right = !password.is_empty() && crate::ws::same_secret(a, &answer(&derive(&password, &salt, KDF_ROUNDS), c));
    let mut i = info.lock().unwrap_or_else(|e| e.into_inner());
    if !right {
        i.local_admin_failures.push(Instant::now());
        log::warn!("admin panel: a wrong admin password");
        return error("401 Unauthorized", "wrong admin password");
    }
    let token = hex(&crate::http::random_bytes::<24>());
    while i.admin.sessions.len() >= 32 {
        i.admin.sessions.remove(0);
    }
    i.admin.sessions.push((token.clone(), Instant::now()));
    log::info!("admin panel: logged in");
    json("200 OK", format!("{{\"token\":{}}}", json_str(&token)))
}

/// The server now, as the panel shows it.
fn state(info: &Mutex<ServerInfo>) -> String {
    let i = info.lock().unwrap_or_else(|e| e.into_inner());
    let uptime = i.admin.started.map(|s| s.elapsed().as_secs()).unwrap_or(0);
    let admin = if i.admin.state.is_empty() { "null".to_string() } else { i.admin.state.as_str().to_string() };
    format!(
        "{{\"status\":{},\"players\":{},\"admin\":{admin},\"uptime\":{uptime},\"remote\":{},\"reached_at\":{},\"live_map\":{}}}",
        i.to_json(),
        players_json(&i.player_list),
        i.admin.remote,
        json_str(&i.reached_at),
        i.players_public
    )
}

// --- a panel's login of its own (Bus Company Online's main server) -----------------------------

/// (NexiconDK, 1 November 2026) The login of an admin panel that is not a game server's gateway's
/// (Bus Company Online's main server on its `api_port`): the same challenge, answer, tokens and
/// lock as the gateway's, on a password and a door of its own.
#[derive(Debug, Default)]
pub struct Auth {
    pub password: String,
    /// From elsewhere than this machine too.
    pub remote: bool,
    sessions: Vec<(String, Instant)>,
    challenges: Vec<(String, [u8; 16], Instant)>,
    failures: Vec<Instant>,
}

impl Auth {
    pub fn new(password: &str, remote: bool) -> Auth {
        Auth { password: password.to_string(), remote, ..Default::default() }
    }

    fn locked(&mut self) -> bool {
        self.failures.retain(|t| t.elapsed() < crate::ws::ADMIN_LOCK_WINDOW);
        self.failures.len() >= crate::ws::ADMIN_LOCK_AFTER
    }
}

/// Whether a request of the panel `route` may come in at all: None, it may; else the answer that
/// turns it away (from elsewhere, the panel not opened to it).
pub fn door(route: &str, here: bool, auth: &Mutex<Auth>) -> Option<Answer> {
    if here || auth.lock().unwrap_or_else(|e| e.into_inner()).remote {
        return None;
    }
    let why = "The admin panel opens on the server's own machine only. web_admin = 1 in its settings opens it from elsewhere too, with the admin password.";
    Some(match route {
        "/admin" | "/admin/" => ("403 Forbidden", "text/html; charset=utf-8", format!("<!doctype html><meta charset=utf-8><title>Admin</title><body style=\"font-family:sans-serif;background:#16181c;color:#eee;padding:40px\"><p>{why}</p>").into_bytes()),
        _ => error("403 Forbidden", why),
    })
}

/// The login's own requests (`/admin/api/challenge`, `login`, `logout`) for `auth`; None: another.
pub fn login_routes(route: &str, post: bool, head: &str, body: &str, auth: &Mutex<Auth>) -> Option<Answer> {
    Some(match (route, post) {
        ("/admin/api/challenge", _) => {
            let mut a = auth.lock().unwrap_or_else(|e| e.into_inner());
            if a.password.chars().count() < 8 {
                return Some(error("404 Not Found", "this server has no admin password (admin_password in its settings, at least 8 characters)"));
            }
            if a.locked() {
                return Some(error("429 Too Many Requests", "too many wrong passwords: try again in two minutes"));
            }
            let c = hex(&crate::http::random_bytes::<16>());
            let salt = crate::http::random_bytes::<16>();
            let now = Instant::now();
            a.challenges.retain(|x| now.duration_since(x.2) < CHALLENGE_LIFE);
            while a.challenges.len() >= 16 {
                a.challenges.remove(0);
            }
            a.challenges.push((c.clone(), salt, now));
            json("200 OK", format!("{{\"challenge\":{},\"salt\":{},\"rounds\":{KDF_ROUNDS}}}", json_str(&c), json_str(&hex(&salt))))
        }
        ("/admin/api/login", true) => {
            let mut it = body.lines().map(str::trim);
            let (c, ans) = (it.next().unwrap_or(""), it.next().unwrap_or(""));
            let (password, salt) = {
                let mut a = auth.lock().unwrap_or_else(|e| e.into_inner());
                if a.locked() {
                    return Some(error("429 Too Many Requests", "too many wrong passwords: try again in two minutes"));
                }
                let Some(k) = a.challenges.iter().position(|x| x.0 == c && x.2.elapsed() < CHALLENGE_LIFE) else { return Some(error("401 Unauthorized", "the login took too long: try again")) };
                let (_, salt, _) = a.challenges.remove(k);
                (a.password.clone(), salt)
            };
            let right = !password.is_empty() && crate::ws::same_secret(ans, &answer(&derive(&password, &salt, KDF_ROUNDS), c));
            let mut a = auth.lock().unwrap_or_else(|e| e.into_inner());
            if !right {
                a.failures.push(Instant::now());
                log::warn!("admin panel: a wrong admin password");
                return Some(error("401 Unauthorized", "wrong admin password"));
            }
            let token = hex(&crate::http::random_bytes::<24>());
            while a.sessions.len() >= 32 {
                a.sessions.remove(0);
            }
            a.sessions.push((token.clone(), Instant::now()));
            json("200 OK", format!("{{\"token\":{}}}", json_str(&token)))
        }
        ("/admin/api/logout", true) => {
            if let Some(t) = header(head, "x-admin-token") {
                auth.lock().unwrap_or_else(|e| e.into_inner()).sessions.retain(|s| s.0 != t);
            }
            json("200 OK", "{\"ok\":true}".into())
        }
        _ => return None,
    })
}

/// Whether the request is logged in: a token the panel was given (kept on), or the admin password
/// from this machine (the launcher's).
pub fn logged_in(head: &str, here: bool, auth: &Mutex<Auth>) -> bool {
    let mut a = auth.lock().unwrap_or_else(|e| e.into_inner());
    if a.password.chars().count() < 8 {
        return false;
    }
    let now = Instant::now();
    a.sessions.retain(|s| now.duration_since(s.1) < TOKEN_IDLE);
    if let Some(t) = header(head, "x-admin-token") {
        if let Some(s) = a.sessions.iter_mut().find(|s| crate::ws::same_secret(&s.0, t)) {
            s.1 = now;
            return true;
        }
    }
    here && header(head, "x-admin-password").is_some_and(|p| crate::ws::same_secret(p, &a.password))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(password: &str, remote: bool) -> Mutex<ServerInfo> {
        let mut i = ServerInfo { name: "Test".into(), local_admin_password: password.into(), ..Default::default() };
        i.admin.remote = remote;
        Mutex::new(i)
    }

    const LOCAL: &str = "GET /admin HTTP/1.1\r\nHost: x";
    fn here() -> Option<SocketAddr> {
        Some("127.0.0.1:5000".parse().unwrap())
    }
    fn lan() -> Option<SocketAddr> {
        Some("192.168.1.20:5000".parse().unwrap())
    }

    fn body(a: &Answer) -> String {
        String::from_utf8_lossy(&a.2).to_string()
    }

    fn value(text: &str, key: &str) -> String {
        crate::ws::json_value(text, key).unwrap_or_default().trim_matches('"').to_string()
    }

    /// The page on this machine; from elsewhere only with `web_admin = 1` - and a tunnel here is
    /// elsewhere.
    #[test]
    fn the_panel_opens_here_and_elsewhere_only_when_asked() {
        let i = info("secret-pass", false);
        assert_eq!(serve("/admin", "", LOCAL, "", here(), &i).0, "200 OK");
        assert_eq!(serve("/admin/panel.js", "", LOCAL, "", here(), &i).1, "text/javascript; charset=utf-8");
        assert_eq!(serve("/admin", "", LOCAL, "", lan(), &i).0, "403 Forbidden");
        let tunnel = "GET /admin HTTP/1.1\r\nHost: x\r\nCF-Connecting-IP: 1.2.3.4";
        assert_eq!(serve("/admin/api/challenge", "", tunnel, "", here(), &i).0, "403 Forbidden");
        let open = info("secret-pass", true);
        assert_eq!(serve("/admin", "", LOCAL, "", lan(), &open).0, "200 OK");
    }

    /// The login: the answer to the challenge gives a token that opens the API; a wrong one is
    /// refused and counted; the password from this machine opens it too, from elsewhere not.
    #[test]
    fn the_login_gives_a_token_and_a_wrong_answer_is_refused() {
        let i = info("secret-pass", true);
        assert_eq!(serve("/admin/api/state", "", LOCAL, "", lan(), &i).0, "401 Unauthorized");
        let c = body(&serve("/admin/api/challenge", "", LOCAL, "", lan(), &i));
        let (ch, salt) = (value(&c, "challenge"), value(&c, "salt"));
        let salt: Vec<u8> = (0..salt.len() / 2).map(|k| u8::from_str_radix(&salt[2 * k..2 * k + 2], 16).unwrap()).collect();
        let good = answer(&derive("secret-pass", &salt, KDF_ROUNDS), &ch);
        // (a wrong answer: refused, the challenge used up)
        let post = "POST /admin/api/login HTTP/1.1\r\nHost: x";
        assert_eq!(serve("/admin/api/login", "", post, &format!("{ch}\n{}", "0".repeat(64)), lan(), &i).0, "401 Unauthorized");
        assert_eq!(serve("/admin/api/login", "", post, &format!("{ch}\n{good}"), lan(), &i).0, "401 Unauthorized", "used up");
        assert_eq!(i.lock().unwrap().local_admin_failures.len(), 1);
        let c = body(&serve("/admin/api/challenge", "", LOCAL, "", lan(), &i));
        let (ch, s) = (value(&c, "challenge"), value(&c, "salt"));
        let salt: Vec<u8> = (0..s.len() / 2).map(|k| u8::from_str_radix(&s[2 * k..2 * k + 2], 16).unwrap()).collect();
        let good = answer(&derive("secret-pass", &salt, KDF_ROUNDS), &ch);
        let ok = serve("/admin/api/login", "", post, &format!("{ch}\n{good}"), lan(), &i);
        assert_eq!(ok.0, "200 OK", "{}", body(&ok));
        let token = value(&body(&ok), "token");
        let with = format!("GET /admin/api/state HTTP/1.1\r\nX-Admin-Token: {token}");
        let st = serve("/admin/api/state", "", &with, "", lan(), &i);
        assert_eq!(st.0, "200 OK");
        assert!(body(&st).contains("\"name\":\"Test\""), "{}", body(&st));
        // the password from this machine (the launcher's), not from elsewhere
        let pw = "GET /admin/api/state HTTP/1.1\r\nX-Admin-Password: secret-pass";
        assert_eq!(serve("/admin/api/state", "", pw, "", here(), &i).0, "200 OK");
        assert_eq!(serve("/admin/api/state", "", pw, "", lan(), &i).0, "401 Unauthorized");
        // a command queued for the server's loop; logged out, the token no longer opens it
        let cmd = format!("POST /admin/api/command HTTP/1.1\r\nX-Admin-Token: {token}");
        assert_eq!(serve("/admin/api/command", "", &cmd, "weather next\nsay hello\n", lan(), &i).0, "202 Accepted");
        assert_eq!(i.lock().unwrap().local_admin_queue, ["weather next", "say hello"]);
        let out = format!("POST /admin/api/logout HTTP/1.1\r\nX-Admin-Token: {token}");
        serve("/admin/api/logout", "", &out, "", lan(), &i);
        assert_eq!(serve("/admin/api/state", "", &with, "", lan(), &i).0, "401 Unauthorized");
        // no password on the server: no panel's API
        let none = info("", true);
        assert_eq!(serve("/admin/api/challenge", "", LOCAL, "", lan(), &none).0, "404 Not Found");
    }

    /// Through the gateway itself, over TCP as a browser asks: the page, its script, the login,
    /// the state and a command (what the routes in `ws::serve` hand on, bodies read whole).
    #[test]
    fn the_panel_through_the_gateway() {
        let gw = crate::ws::WsGateway::start("127.0.0.1:0".parse().unwrap(), "127.0.0.1:9".parse().unwrap(), ServerInfo { name: "Gate".into(), local_admin_password: "secret-pass".into(), ..Default::default() }).unwrap();
        let base = format!("http://{}", gw.addr);
        let t = Duration::from_secs(10);
        let get = |path: &str, h: &[(&str, &str)]| crate::http::request("GET", &format!("{base}{path}"), h, None, t).unwrap();
        let (code, page) = get("/admin", &[]);
        assert!(code == 200 && page.contains("Server admin"), "{code}");
        let (code, js) = get("/admin/panel.js", &[]);
        assert!(code == 200 && js.contains("export function mountPanel"), "{code}");
        let (_, c) = get("/admin/api/challenge", &[]);
        let (ch, s) = (value(&c, "challenge"), value(&c, "salt"));
        let salt: Vec<u8> = (0..s.len() / 2).map(|k| u8::from_str_radix(&s[2 * k..2 * k + 2], 16).unwrap()).collect();
        let good = answer(&derive("secret-pass", &salt, KDF_ROUNDS), &ch);
        let (code, l) = crate::http::request("POST", &format!("{base}/admin/api/login"), &[], Some(&format!("{ch}\n{good}")), t).unwrap();
        assert_eq!(code, 200, "{l}");
        let token = value(&l, "token");
        let (code, st) = get("/admin/api/state", &[("X-Admin-Token", &token)]);
        assert!(code == 200 && st.contains("\"name\":\"Gate\""), "{code} {st}");
        let (code, _) = crate::http::request("POST", &format!("{base}/admin/api/command"), &[("X-Admin-Token", &token)], Some("say hi\nstop"), t).unwrap();
        assert_eq!(code, 202);
        assert_eq!(gw.info.lock().unwrap().local_admin_queue, ["say hi", "stop"]);
        // (the old POST /admin for a tool on this machine still takes commands)
        let (code, _) = crate::http::request("POST", &format!("{base}/admin"), &[("X-Admin-Password", "secret-pass")], Some("weather next"), t).unwrap();
        assert_eq!(code, 202);
    }

    /// A panel's own login (the main server's): the same challenge and answer, a token, the door
    /// shut from elsewhere unless opened.
    #[test]
    fn a_login_of_its_own() {
        let auth = Mutex::new(Auth::new("secret-pass", false));
        assert!(door("/admin", true, &auth).is_none());
        assert_eq!(door("/admin/api/state", false, &auth).unwrap().0, "403 Forbidden");
        let c = body(&login_routes("/admin/api/challenge", false, LOCAL, "", &auth).unwrap());
        let (ch, s) = (value(&c, "challenge"), value(&c, "salt"));
        let salt: Vec<u8> = (0..s.len() / 2).map(|k| u8::from_str_radix(&s[2 * k..2 * k + 2], 16).unwrap()).collect();
        let good = answer(&derive("secret-pass", &salt, KDF_ROUNDS), &ch);
        let ok = login_routes("/admin/api/login", true, LOCAL, &format!("{ch}\n{good}"), &auth).unwrap();
        let token = value(&body(&ok), "token");
        assert!(logged_in(&format!("GET / HTTP/1.1\r\nX-Admin-Token: {token}"), false, &auth));
        assert!(!logged_in("GET / HTTP/1.1\r\nX-Admin-Token: nope", false, &auth));
        assert!(logged_in("GET / HTTP/1.1\r\nX-Admin-Password: secret-pass", true, &auth));
        assert!(!logged_in("GET / HTTP/1.1\r\nX-Admin-Password: secret-pass", false, &auth));
        assert!(login_routes("/admin/api/state", false, LOCAL, "", &auth).is_none());
    }

    /// The log's lines since a number; the config read and written whole.
    #[test]
    fn the_log_and_the_config() {
        let (_, start) = log_since(u64::MAX);
        push_log("one".into());
        push_log("two".into());
        let (lines, next) = log_since(start);
        assert!(lines.ends_with(&["one".to_string(), "two".to_string()]) && next >= start + 2);
        let dir = std::env::temp_dir().join(format!("omsi-admin-web-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("server.cfg");
        std::fs::write(&p, "name = A\r\n").unwrap();
        let i = info("secret-pass", false);
        i.lock().unwrap().admin.cfg_path = Some(p.clone());
        let h = "GET /admin/api/config HTTP/1.1\r\nX-Admin-Password: secret-pass";
        assert!(body(&serve("/admin/api/config", "", h, "", here(), &i)).contains("name = A"));
        let w = "POST /admin/api/config HTTP/1.1\r\nX-Admin-Password: secret-pass";
        assert_eq!(serve("/admin/api/config", "", w, "name = B\nport = 1\n", here(), &i).0, "200 OK");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "name = B\r\nport = 1\r\n");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
