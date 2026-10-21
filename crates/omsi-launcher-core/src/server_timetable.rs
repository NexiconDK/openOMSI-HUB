//! Omsi-Hub: the timetable of the server a player joins, in the launcher and the game.
//!
//! A dedicated server shares the timetable it runs (`share_timetable`): its map's `TTData` as a
//! zip of `maps/<map>/TTData/<file>` (`GET /ttdata.zip`), with an id in its status (`"ttdata"`,
//! the zip's hash). The launcher of a player who picks the server fetches it once into
//! `~/.openomsi/server-timetables/<id>.zip` and mounts it as the content root searched first:
//! the drive pages (Tour, Work shift) then list the server's lines and tours, whatever the
//! player's own `TTData` holds. The game gets the same zip (`--content-zip`, see `drive_args`),
//! so the duty it starts is one of the server's too.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The server timetable mounted now: the server (as the player gave it), the zip and its mount.
struct Mounted {
    target: String,
    zip: PathBuf,
    mount: PathBuf,
    id: String,
}

static MOUNTED: Mutex<Option<Mounted>> = Mutex::new(None);

/// Its greatest size (the zips of the stock maps' timetables are a few MB).
const MAX_BYTES: u64 = 64_000_000;

/// What `fetch` did.
#[derive(Debug, Clone, PartialEq)]
pub struct Fetched {
    pub id: String,
    /// Fetched now (not from the cache).
    pub downloaded: bool,
    pub bytes: u64,
}

fn cache_dir() -> PathBuf {
    crate::data_dir().join("server-timetables")
}

/// The server timetable's zip for `target` when it is the one mounted.
pub fn zip_for(target: &str) -> Option<PathBuf> {
    let m = MOUNTED.lock().unwrap_or_else(|e| e.into_inner());
    m.as_ref().filter(|m| same_target(&m.target, target)).map(|m| m.zip.clone())
}

/// The id of the server timetable mounted, if any.
pub fn mounted_id() -> Option<String> {
    MOUNTED.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|m| m.id.clone())
}

fn same_target(a: &str, b: &str) -> bool {
    a.trim().trim_end_matches('/').eq_ignore_ascii_case(b.trim().trim_end_matches('/'))
}

/// Fetch the timetable of server `target` (an address as the player gave it) and mount it:
/// from the cache when this version of it is there. Err: the server shares none, or it could
/// not be reached.
pub fn fetch(target: &str) -> Result<Fetched, String> {
    let info = omsi_net::ws::query(target, false)?;
    let id = info.ttdata_id.trim().to_string();
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_hexdigit()) {
        clear();
        return Err("the server shares no timetable".into());
    }
    let dir = cache_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let zip = dir.join(format!("{id}.zip"));
    let mut downloaded = false;
    if !zip.is_file() {
        let data = omsi_net::ws::download(&info.reached_at, "/ttdata.zip", MAX_BYTES)?;
        // (written beside and renamed: a broken download never stands as the cached one)
        let part = dir.join(format!("{id}.part"));
        std::fs::write(&part, &data).map_err(|e| e.to_string())?;
        std::fs::rename(&part, &zip).map_err(|e| e.to_string())?;
        downloaded = true;
    }
    let bytes = std::fs::metadata(&zip).map(|m| m.len()).unwrap_or(0);
    if let Err(e) = mount(target, &zip, &id) {
        let _ = std::fs::remove_file(&zip);
        return Err(e);
    }
    prune(&dir, &zip);
    eprintln!("server timetable {id} of {target}: {} ({bytes} bytes)", if downloaded { "fetched" } else { "from the cache" });
    Ok(Fetched { id, downloaded, bytes })
}

fn mount(target: &str, zip: &Path, id: &str) -> Result<(), String> {
    let mut m = MOUNTED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(old) = m.as_ref() {
        if old.zip == zip {
            *m = Some(Mounted { target: target.to_string(), zip: old.zip.clone(), mount: old.mount.clone(), id: id.to_string() });
            return Ok(());
        }
        omsi_cfg::remove_content_root(&old.mount);
    }
    let mount = omsi_cfg::vfs::mount_zip(zip).map_err(|e| format!("{}: {e}", zip.display()))?;
    omsi_cfg::add_content_root_first(mount.clone());
    *m = Some(Mounted { target: target.to_string(), zip: zip.to_path_buf(), mount, id: id.to_string() });
    Ok(())
}

/// No server timetable any more: the player's own again.
pub fn clear() {
    if let Some(old) = MOUNTED.lock().unwrap_or_else(|e| e.into_inner()).take() {
        omsi_cfg::remove_content_root(&old.mount);
        eprintln!("server timetable {} put away", old.id);
    }
}

/// The cache keeps the newest few versions.
fn prune(dir: &Path, keep: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut zips: Vec<(std::time::SystemTime, PathBuf)> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x.eq_ignore_ascii_case("zip")) && p != keep)
        .filter_map(|p| std::fs::metadata(&p).and_then(|m| m.modified()).ok().map(|t| (t, p)))
        .collect();
    zips.sort();
    let extra = zips.len().saturating_sub(4);
    for (_, p) in zips.into_iter().take(extra) {
        let _ = std::fs::remove_file(p);
    }
}
