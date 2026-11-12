//! (NexiconDK, 12 November 2026) A company's own liveries shared through Bus Company Online's
//! server, the launcher's side (the server's: `omsi_launcher_lib::company::shared_livery`):
//!
//! - `share` - a livery the owner or a co-owner saved for a company of the server in the livery
//!   studio (`lak/place.rs`): its files sent up, those the server lacks, a part at a time;
//! - `share_kept` - when the server's companies are listed: the company's designs the server has
//!   no files of yet, sent up from this computer's studio projects (once a run of the launcher);
//! - `fetch_world` - before a game joins a map's world: the liveries of every company driven
//!   there that this computer lacks, or has an older one of, fetched into openOMSI's content
//!   folder - the textures first, the `.cti` last, each under its own name only once whole. What
//!   a livery's earlier version had and this one has not is taken away again (`shared_liveries.json`
//!   in the data folder keeps what was fetched).

use super::bco;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::shared_livery::{self as sl, File, Shared};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// How long a game's start waits for the liveries (fetched on meanwhile: seen from the next game).
const FETCH_WAIT: std::time::Duration = std::time::Duration::from_secs(60);

fn log(text: &str) {
    core::log_to_file(&format!("liveries: {text}"));
}

/// A question of the server's, its refusal's English as the error.
fn ask(what: &str, company: &str, args: Value) -> Result<Value, String> {
    bco::question(what, json!({ "company": company, "args": args })).map_err(|e| e.0)
}

/// Whether he is company `company`'s owner or a co-owner, as the server listed it.
fn owns(company: &str) -> bool {
    bco::company_row(company).is_some_and(|r| r.get("role").and_then(Value::as_str) == Some("co_owner"))
}

/// Livery `name` of company `company` painted for `bus` (its files: `files`, relative to the
/// content folder) sent to the server - on a thread of its own; a company not of the server, or
/// not his, nothing.
pub(crate) fn share(company: String, name: String, bus: String, files: Vec<String>) {
    if !owns(&company) {
        return;
    }
    let _ = std::thread::Builder::new().name("livery share".into()).spawn(move || match share_now(&company, &name, &bus, &files) {
        Ok(n) => log(&format!("'{name}' of {company} shared on the server ({n} files sent)")),
        Err(e) => log(&format!("'{name}' of {company} not shared: {e}")),
    });
}

/// The livery sent up now: how many files went.
fn share_now(company: &str, name: &str, bus: &str, files: &[String]) -> Result<usize, String> {
    let content = core::content_dir().ok_or("no content folder")?;
    let mut list = Vec::new();
    for f in files {
        let Some(p) = sl::clean_path(f) else { continue };
        let bytes = std::fs::read(content.join(&p)).map_err(|e| format!("{p}: {e}"))?;
        list.push(File { path: p, size: bytes.len() as u64, sha: sl::sha_of(&bytes) });
    }
    let livery = serde_json::to_value(Shared { name: name.to_string(), bus: bus.to_string(), files: list.clone(), ..Default::default() }).unwrap_or(Value::Null);
    let need: Vec<String> = ask("livery_begin", company, json!({ "livery": livery }))?.get("need").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
    for sha in &need {
        let Some(f) = list.iter().find(|f| &f.sha == sha) else { continue };
        let bytes = std::fs::read(content.join(&f.path)).map_err(|e| e.to_string())?;
        for (k, part) in bytes.chunks(sl::PART).enumerate() {
            ask("livery_part", company, json!({ "sha": sha, "size": bytes.len(), "offset": k * sl::PART, "data": sl::b64_encode(part) }))?;
        }
    }
    ask("livery_end", company, json!({ "livery": livery }))?;
    Ok(need.len())
}

/// The liveries of a company's row of the server's list.
fn liveries_of(row: &Value) -> Vec<Shared> {
    row.get("liveries").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default()
}

fn same_file(a: &str, b: &str) -> bool {
    a.trim().replace('\\', "/").eq_ignore_ascii_case(&b.trim().replace('\\', "/"))
}

/// The designs of his companies (`companies`: the server's list) the server has no files of,
/// sent up from this computer's studio projects - each once a run of the launcher.
pub(crate) fn share_kept(companies: Vec<Value>) {
    static TRIED: Mutex<Option<HashSet<String>>> = Mutex::new(None);
    let mut todo = Vec::new();
    {
        let mut tried = TRIED.lock().unwrap_or_else(|e| e.into_inner());
        let tried = tried.get_or_insert_with(HashSet::new);
        for row in companies.iter().filter(|r| r.get("role").and_then(Value::as_str) == Some("co_owner")) {
            let Some(cid) = row.get("id").and_then(Value::as_str) else { continue };
            let shared = liveries_of(row);
            for d in row.get("designs").and_then(Value::as_array).into_iter().flatten() {
                let (name, bus) = (d.get("name").and_then(Value::as_str).unwrap_or(""), d.get("bus").and_then(Value::as_str).unwrap_or(""));
                if name.trim().is_empty() || shared.iter().any(|s| s.named(name)) || !tried.insert(format!("{cid}|{}", name.trim().to_lowercase())) {
                    continue;
                }
                todo.push((cid.to_string(), name.to_string(), bus.to_string()));
            }
        }
    }
    if todo.is_empty() {
        return;
    }
    let _ = std::thread::Builder::new().name("livery share kept".into()).spawn(move || {
        let projects = omsihub_livery::projects();
        for (cid, name, bus) in todo {
            let Some(files) = projects.iter().filter_map(|p| p.placed.as_ref().filter(|pl| pl.name.trim().eq_ignore_ascii_case(name.trim()) && same_file(&p.bus, &bus) && !pl.files.is_empty())).map(|pl| pl.files.clone()).next() else { continue };
            match share_now(&cid, &name, &bus, &files) {
                Ok(n) => log(&format!("'{name}' of {cid}, saved before, shared on the server ({n} files sent)")),
                Err(e) => log(&format!("'{name}' of {cid}, saved before, not shared: {e}")),
            }
        }
    });
}

// --- fetching ----------------------------------------------------------------------------------

fn record_path() -> PathBuf {
    core::data_dir().join("shared_liveries.json")
}

/// What was fetched: per livery (`<company>|<name>`) its files.
fn record() -> HashMap<String, Vec<File>> {
    std::fs::read(record_path()).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn keep_record(r: &HashMap<String, Vec<File>>) {
    let _ = std::fs::write(record_path(), serde_json::to_vec_pretty(r).unwrap_or_default());
}

/// Whether the bus `bus` is on this computer (in the content folder or the OMSI 2 folder).
fn bus_here(content: &Path, bus: &str) -> bool {
    let root = core::load_config().root;
    omsi_cfg::resolve_path(content, bus).is_file() || (!root.trim().is_empty() && omsi_cfg::resolve_path(Path::new(&root), bus).is_file())
}

/// Whether `f` lies in the content folder as it should: its size, and its hash as fetched or
/// as read now.
fn have(content: &Path, f: &File, known: &HashSet<(String, String)>) -> bool {
    let p = content.join(&f.path);
    if std::fs::metadata(&p).map(|m| m.len()).ok() != Some(f.size) {
        return false;
    }
    known.contains(&(f.path.to_ascii_lowercase(), f.sha.clone())) || std::fs::read(&p).is_ok_and(|b| sl::sha_of(&b) == f.sha)
}

/// File `f` fetched from the server into the content folder (under a name of its own until it is
/// whole and checked).
fn fetch_file(content: &Path, f: &File) -> Result<(), String> {
    let mut bytes = Vec::with_capacity(f.size as usize);
    while (bytes.len() as u64) < f.size {
        let v = ask("livery_get", "", json!({ "sha": f.sha, "offset": bytes.len() }))?;
        let part = v.get("data").and_then(Value::as_str).and_then(sl::b64_decode).filter(|b| !b.is_empty()).ok_or("an empty part")?;
        bytes.extend(part);
    }
    if bytes.len() as u64 != f.size || sl::sha_of(&bytes) != f.sha {
        return Err(format!("{} did not arrive whole", f.path));
    }
    let to = content.join(&f.path);
    if !omsihub_livery::export::inside(&to, content) {
        return Err(format!("{} is not in the content folder", f.path));
    }
    if let Some(d) = to.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let part = to.with_extension("part");
    std::fs::write(&part, &bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&part, &to).map_err(|e| e.to_string())
}

/// The liveries `wanted` (each with its company) brought into the content folder: how many
/// files were fetched.
fn fetch_all(wanted: Vec<(String, Shared)>) -> usize {
    let Some(content) = core::content_dir() else { return 0 };
    let mut rec = record();
    let mut fetched = 0;
    for (cid, mut s) in wanted {
        if sl::check(&mut s).is_err() || !bus_here(&content, &s.bus) {
            continue;
        }
        let key = format!("{cid}|{}", s.name.trim().to_lowercase());
        let known: HashSet<(String, String)> = rec.values().flatten().map(|f| (f.path.to_ascii_lowercase(), f.sha.clone())).collect();
        let mut todo: Vec<&File> = s.files.iter().filter(|f| !have(&content, f, &known)).collect();
        // (the textures first, the .cti last: until it is there, the game knows nothing of them)
        todo.sort_by_key(|f| f.path.to_ascii_lowercase().ends_with(".cti"));
        let mut whole = true;
        for f in todo {
            match fetch_file(&content, f) {
                Ok(()) => fetched += 1,
                Err(e) => {
                    log(&format!("'{}' of {cid}: {e}", s.name));
                    whole = false;
                    break;
                }
            }
        }
        if !whole {
            continue;
        }
        // (what its version before had and this one has not)
        if let Some(old) = rec.get(&key) {
            for o in old.iter().filter(|o| !s.files.iter().any(|f| f.path.eq_ignore_ascii_case(&o.path))) {
                let p = content.join(&o.path);
                if sl::clean_path(&o.path).is_some() && omsihub_livery::export::inside(&p, &content) {
                    let _ = std::fs::remove_file(&p);
                }
            }
        }
        rec.insert(key, s.files.clone());
    }
    keep_record(&rec);
    if fetched > 0 {
        omsi_cfg::content_changed();
    }
    fetched
}

/// Before a game joins the map's world of company `company`: the liveries of every company
/// driven there that this computer lacks, fetched - waited for at most `FETCH_WAIT`.
pub(crate) fn fetch_world(company: &str) {
    if bco::company_row(company).is_none() {
        return;
    }
    bco::refresh_companies();
    let wanted: Vec<(String, Shared)> = bco::companies_of_world(company).iter().flat_map(|row| {
        let cid = row.get("id").and_then(Value::as_str).unwrap_or("").to_string();
        liveries_of(row).into_iter().map(move |s| (cid.clone(), s))
    }).collect();
    if wanted.is_empty() {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let _ = std::thread::Builder::new().name("livery fetch".into()).spawn(move || {
        let n = fetch_all(wanted);
        if n > 0 {
            log(&format!("{n} files of the world's companies' liveries fetched"));
        }
        let _ = tx.send(());
    });
    if rx.recv_timeout(FETCH_WAIT).is_err() {
        log("the world's companies' liveries not all fetched yet - fetched on meanwhile, seen from the next game on");
    }
}
