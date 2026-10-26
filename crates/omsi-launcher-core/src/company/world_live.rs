//! (NexiconDK) Bus Company Online phase 5 (docs/omsihub/BUS_COMPANY_ONLINE.md): each depot's
//! control room. A map's world writes what it sees on its roads every couple of seconds
//! (`EVERY`) - the companies' timetable buses, by the line and tour their plans name them, and
//! the players' buses, by the tours their games say they drive - into the server's folder,
//! `live/<map>.json`, whole each time. A company's fleet map (and with it the Operations page)
//! puts its buses where the world has them while the file is fresh (`FRESH`), else where the
//! timetable has them: the world is not running, or the bus is not on its roads.

use super::online;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// How often the world writes the file (s).
pub const EVERY: f64 = 2.0;
/// How long a company believes it (s): a world that stopped writing has stopped.
pub const FRESH: f64 = 15.0;

/// A company's timetable bus on the world's roads.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorldBus {
    /// The company whose line it is.
    pub company: String,
    pub line: String,
    pub tour: String,
    pub trip: String,
    /// Its trip's departure, minutes of the day (a night trip after midnight past 1440).
    pub dep: i32,
    /// World metres (x east, y north), as the game's telemetry has them.
    pub x: f64,
    pub y: f64,
    pub heading: f64,
    pub speed_kmh: f64,
    /// Seconds late (negative: early).
    pub delay: f64,
    pub passengers: usize,
    pub at_stop: bool,
    pub layover: bool,
    pub next: String,
    pub terminus: String,
    /// Its fleet number.
    pub number: String,
}

/// A player's bus on the world's roads, on a company's tour.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorldPlayer {
    pub name: String,
    /// The company whose tour he drives.
    pub company: String,
    /// The tours his game says he drives (`omsi_sim`'s `LanClaim`): line and tour, lower case.
    pub tours: Vec<(String, String)>,
    pub x: f64,
    pub y: f64,
    pub heading: f64,
    pub speed_kmh: f64,
    pub passengers: u32,
    /// The line his bus shows.
    pub line: String,
}

/// What a map's world sees on its roads (`live/<map>.json`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorldLive {
    /// When the world wrote it (Unix seconds).
    pub updated: f64,
    /// The map, as the companies name it (`maps/<name>/global.cfg`).
    pub map: String,
    pub buses: Vec<WorldBus>,
    pub players: Vec<WorldPlayer>,
}

/// The world's file of map `map` in the server's folder `data`.
pub fn file(data: &Path, map: &str) -> PathBuf {
    data.join("live").join(format!("{}.json", online::map_folder(map)))
}

/// The world's file written whole (through a file beside it, so a company never reads half).
pub fn write(data: &Path, w: &WorldLive) -> Result<()> {
    let path = file(data, &w.map);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("cannot make {}", dir.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec(w)?).with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

/// What the world of map `map` sees of company `id` now (`now`: Unix seconds), while it is fresh.
pub fn read(data: &Path, map: &str, id: &str, now: f64) -> Option<WorldLive> {
    if online::map_folder(map).is_empty() {
        return None;
    }
    let text = std::fs::read_to_string(file(data, map)).ok()?;
    let w: WorldLive = serde_json::from_str(&text).ok()?;
    Some(w).filter(|w| (now - w.updated).abs() <= FRESH).map(|w| w.of(id))
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

impl WorldLive {
    /// Only company `id`'s buses and players.
    pub fn of(mut self, id: &str) -> WorldLive {
        self.buses.retain(|b| same(&b.company, id));
        self.players.retain(|p| same(&p.company, id));
        self
    }

    /// The timetable bus of tour `tour` of line `line`, on the roads now.
    pub fn bus(&self, line: &str, tour: &str) -> Option<&WorldBus> {
        self.buses.iter().find(|b| same(&b.line, line) && same(&b.tour, tour))
    }

    /// The player who drives tour `tour` of line `line` now.
    pub fn player(&self, line: &str, tour: &str) -> Option<&WorldPlayer> {
        self.players.iter().find(|p| p.tours.iter().any(|(l, t)| same(l, line) && same(t, tour)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The world writes what it sees whole; a company reads only its own, and only while the
    /// world keeps writing.
    #[test]
    fn a_company_sees_its_own_buses_while_the_world_writes() {
        let data = std::env::temp_dir().join(format!("openomsi-world-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let bus = |company: &str, line: &str, tour: &str| WorldBus { company: company.into(), line: line.into(), tour: tour.into(), x: 100.0, y: 200.0, delay: 90.0, ..Default::default() };
        let w = WorldLive {
            updated: 1000.0,
            map: "maps\\Grundorf\\global.cfg".into(),
            buses: vec![bus("alpha", "Linie5", "1"), bus("beta", "Linie6", "1")],
            players: vec![WorldPlayer { name: "Luc".into(), company: "alpha".into(), tours: vec![("linie5".into(), "2".into())], ..Default::default() }],
        };
        write(&data, &w).unwrap();
        assert!(file(&data, "maps/Grundorf/global.cfg").exists());
        let a = read(&data, "maps/Grundorf/global.cfg", "alpha", 1005.0).expect("fresh");
        assert_eq!(a.buses.len(), 1);
        assert_eq!(a.bus("linie5", "1").map(|b| b.delay), Some(90.0));
        assert!(a.bus("Linie6", "1").is_none(), "beta's is not alpha's");
        assert_eq!(a.player("Linie5", "2").map(|p| p.name.as_str()), Some("Luc"));
        assert!(a.player("Linie5", "1").is_none());
        let b = read(&data, "maps/Grundorf/global.cfg", "beta", 1005.0).expect("fresh");
        assert!(b.bus("Linie6", "1").is_some() && b.players.is_empty());
        assert!(read(&data, "maps/Grundorf/global.cfg", "alpha", 1000.0 + FRESH + 1.0).is_none(), "a world that stopped writing");
        assert!(read(&data, "maps/Ahlheim/global.cfg", "alpha", 1005.0).is_none(), "another map's world");
        let _ = std::fs::remove_dir_all(&data);
    }
}
