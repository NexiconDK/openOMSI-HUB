//! (NexiconDK, 1 November 2026) Multiplayer → My server: a dedicated server on this machine, run
//! from the launcher instead of a command window. Its folder (the one with its `server.cfg`) is
//! chosen once (`<data>/my-server.json`); the launcher starts the game program as the server
//! (`--server <folder>/server.cfg`, its log in `server.log` beside it), stops it, and asks it what
//! the admin panel shows (`omsi_net::admin_web`, `GET|POST /admin/api/…` on 127.0.0.1 with the
//! admin password - the launcher is on the server's machine). Its settings are read and written
//! here, the server stopped or not; the launcher gives a server without an admin password one,
//! so that it can administer it.

use crate::Answer;
use omsi_launcher_lib as core;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// The server the launcher started (its folder, its process).
static CHILD: Mutex<Option<(PathBuf, Child)>> = Mutex::new(None);

/// A new server's `server.cfg` (the game writes the same when it is missing).
const DEFAULT_CFG: &str = include_str!("../../../omsi-lan/src/default_server.cfg");

pub fn call(cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "srv_status" => Ok(status()),
        "srv_folder" => (|| {
            let folder = args.get("folder").and_then(Value::as_str).unwrap_or("").trim().to_string();
            if folder.is_empty() {
                return Err("No folder chosen.".to_string());
            }
            let _ = std::fs::create_dir_all(&folder);
            let _ = std::fs::write(store(), json!({ "folder": folder }).to_string());
            Ok(status())
        })(),
        "srv_start" => start(),
        "srv_stop" => stop(),
        "srv_api" => (|| {
            let path = args.get("path").and_then(Value::as_str).unwrap_or("").trim_start_matches('/').to_string();
            let body = args.get("body").and_then(Value::as_str);
            api(&path, body)
        })(),
        "srv_cfg" => (|| {
            let f = folder().ok_or("Choose the server's folder first.")?;
            let p = f.join("server.cfg");
            let text = std::fs::read_to_string(&p).unwrap_or_else(|_| DEFAULT_CFG.to_string());
            Ok(json!({ "text": text, "path": p.to_string_lossy() }))
        })(),
        "srv_cfg_save" => (|| {
            let f = folder().ok_or("Choose the server's folder first.")?;
            let text = args.get("text").and_then(Value::as_str).unwrap_or("");
            if text.trim().is_empty() {
                return Err("Nothing to save.".to_string());
            }
            std::fs::create_dir_all(&f).map_err(|e| e.to_string())?;
            std::fs::write(f.join("server.cfg"), text.replace("\r\n", "\n").replace('\n', "\r\n")).map_err(|e| e.to_string())?;
            Ok(json!({ "ok": true }))
        })(),
        "srv_log" => (|| {
            let f = folder().ok_or("Choose the server's folder first.")?;
            let text = std::fs::read_to_string(f.join("server.log")).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            let from = lines.len().saturating_sub(400);
            Ok(json!({ "lines": &lines[from..], "next": 0 }))
        })(),
        _ => return None,
    })
}

fn store() -> PathBuf {
    core::data_dir().join("my-server.json")
}

/// The server's folder, once chosen.
fn folder() -> Option<PathBuf> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(store()).ok()?).ok()?;
    v.get("folder").and_then(Value::as_str).filter(|s| !s.trim().is_empty()).map(PathBuf::from)
}

/// `server.cfg`'s `key = value` (the first of a key, lower case).
pub(crate) fn cfg_value(text: &str, key: &str) -> Option<String> {
    text.lines().map(str::trim).filter(|l| !l.starts_with('#')).find_map(|l| l.split_once('=').filter(|(k, _)| k.trim().eq_ignore_ascii_case(key)).map(|(_, v)| v.trim().to_string()))
}

/// `server.cfg` with `key = value`: its line changed where it is, else added at the end.
pub(crate) fn cfg_set(text: &str, key: &str, value: &str) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let line = format!("{key} = {value}");
    match lines.iter().position(|l| {
        let l = l.trim();
        !l.starts_with('#') && l.split_once('=').is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key))
    }) {
        Some(k) => lines[k] = line,
        None => {
            while lines.last().is_some_and(|l| l.trim().is_empty()) {
                lines.pop();
            }
            lines.push(line);
        }
    }
    lines.join("\n") + "\n"
}

/// The server's web port and admin password (`server.cfg`; its defaults).
fn ports_of(f: &Path) -> (u16, String, String) {
    let text = std::fs::read_to_string(f.join("server.cfg")).unwrap_or_else(|_| DEFAULT_CFG.to_string());
    let web = cfg_value(&text, "web_port").and_then(|v| v.parse().ok()).unwrap_or(27025);
    (web, cfg_value(&text, "admin_password").unwrap_or_default(), text)
}

/// Whether the launcher's server process still runs (one that ended is let go).
fn ours_running(f: &Path) -> bool {
    let mut c = CHILD.lock().unwrap_or_else(|e| e.into_inner());
    match c.as_mut() {
        Some((dir, child)) if dir == f => match child.try_wait() {
            Ok(None) => true,
            _ => {
                *c = None;
                false
            }
        },
        _ => false,
    }
}

/// Whether a server answers on the web port of this machine.
fn answers(web: u16) -> bool {
    matches!(omsi_net::http::request("GET", &format!("http://127.0.0.1:{web}/status"), &[], None, Duration::from_millis(800)), Ok((200, _)))
}

fn status() -> Value {
    let Some(f) = folder() else { return json!({ "folder": null }) };
    let (web, password, text) = ports_of(&f);
    let ours = ours_running(&f);
    let running = ours || answers(web);
    json!({
        "folder": f.to_string_lossy(),
        "cfg": f.join("server.cfg").is_file(),
        "name": cfg_value(&text, "name").unwrap_or_default(),
        "map": cfg_value(&text, "map").unwrap_or_default(),
        "port": cfg_value(&text, "port").and_then(|v| v.parse::<u16>().ok()).unwrap_or(27015),
        "web_port": web,
        "password": password.chars().count() >= 8,
        "web_admin": cfg_value(&text, "web_admin").is_some_and(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")),
        "running": running,
        "ours": ours,
        "admin_url": format!("http://127.0.0.1:{web}/admin"),
    })
}

/// A password of 16 letters and digits.
fn new_password() -> String {
    const CHARS: &[u8] = b"abcdefghijkmnpqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    omsi_net::http::random_bytes::<16>().iter().map(|b| CHARS[*b as usize % CHARS.len()] as char).collect()
}

fn start() -> Answer {
    let f = folder().ok_or("Choose the server's folder first.")?;
    let (web, password, text) = ports_of(&f);
    if ours_running(&f) || answers(web) {
        return Err(format!("A server runs already on web port {web}."));
    }
    std::fs::create_dir_all(&f).map_err(|e| e.to_string())?;
    let cfg = f.join("server.cfg");
    // (a server without an admin password: the launcher could not administer it)
    let made = password.chars().count() < 8;
    if made || !cfg.is_file() {
        let text = if made { cfg_set(&text, "admin_password", &new_password()) } else { text };
        std::fs::write(&cfg, text.replace("\r\n", "\n").replace('\n', "\r\n")).map_err(|e| format!("{}: {e}", cfg.display()))?;
    }
    let game = core::game_program().ok_or("The game program was not found (Setup).")?;
    let log = f.join("server.log");
    if log.is_file() {
        let _ = std::fs::rename(&log, f.join("server-prev.log"));
    }
    let out = std::fs::File::create(&log).map_err(|e| format!("{}: {e}", log.display()))?;
    let err = out.try_clone().map_err(|e| e.to_string())?;
    let mut c = Command::new(&game);
    c.arg("--server").arg(&cfg).current_dir(&f).stdin(Stdio::null()).stdout(out).stderr(err);
    let root = core::load_config().root;
    if !root.trim().is_empty() {
        c.arg("--root").arg(root.trim());
    }
    let child = c.spawn().map_err(|e| format!("{}: {e}", game.display()))?;
    *CHILD.lock().unwrap_or_else(|e| e.into_inner()) = Some((f, child));
    Ok(json!({ "ok": true, "password_made": made }))
}

fn stop() -> Answer {
    let f = folder().ok_or("Choose the server's folder first.")?;
    let (web, password, _) = ports_of(&f);
    // (asked to stop as the admin panel does; a server that does not, ended)
    if !password.is_empty() {
        let _ = omsi_net::http::request("POST", &format!("http://127.0.0.1:{web}/admin"), &[("X-Admin-Password", &password)], Some("stop"), Duration::from_secs(3));
    }
    let until = Instant::now() + Duration::from_secs(15);
    while Instant::now() < until {
        if !ours_running(&f) && !answers(web) {
            return Ok(json!({ "ok": true }));
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    let mut c = CHILD.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, child)) = c.as_mut() {
        let _ = child.kill();
        let _ = child.wait();
        *c = None;
        return Ok(json!({ "ok": true, "killed": true }));
    }
    Err("The server did not stop: it was not started from the launcher, and it has no admin password to be asked with.".into())
}

/// What the admin panel asks (`omsi_net::admin_web`), with the server's password from this machine.
fn api(path: &str, body: Option<&str>) -> Answer {
    let f = folder().ok_or("Choose the server's folder first.")?;
    let (web, password, _) = ports_of(&f);
    if password.is_empty() {
        return Err("The server has no admin password (Settings).".into());
    }
    let (code, text) = omsi_net::http::request("POST", &format!("http://127.0.0.1:{web}/admin/api/{path}"), &[("X-Admin-Password", &password)], body, Duration::from_secs(5))?;
    let v: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    if !(200..300).contains(&code) {
        return Err(v.get("error").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| format!("HTTP {code}")));
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_key_of_server_cfg_is_set_where_it_is() {
        let text = "# c\nname = A\nport = 1\n";
        assert_eq!(super::cfg_value(text, "NAME").as_deref(), Some("A"));
        assert_eq!(super::cfg_set(text, "name", "B"), "# c\nname = B\nport = 1\n");
        assert_eq!(super::cfg_set(text, "web_admin", "1"), "# c\nname = A\nport = 1\nweb_admin = 1\n");
        assert_eq!(super::cfg_value(super::DEFAULT_CFG, "web_admin").as_deref(), Some("0"));
        assert!(super::new_password().len() == 16);
    }
}
