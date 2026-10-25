//! (NexiconDK) Bus Company Online, phase 4 (docs/omsihub/BUS_COMPANY_ONLINE.md): a company's
//! depots on other maps of the server.
//!
//! A company of the server runs on one map for good; to run on another it founds a depot there
//! (`found`, its owner from the head office's settings): a company of its own on that map - its
//! own lines, buses, staff, tenders and books, its own process on the server as every company -
//! tied to the head office (`Online::hq`), with the head office's name, mark, game mode and owner.
//! The head office pays for it from its own money (`depot_price` in `company-server.cfg`) and
//! gives it its starting money; the owner gives a player the running of it (its depot leader: a
//! co-owner of the depot). Money, buses and staff moved between them, the head office's view of
//! its depots and a depot that goes bankrupt come in the next steps.

use super::main_server::{find_map, map_name};
use super::model::{BookingKind, Cents, Company};
use super::online::{self, company_ids, map_folder};
use super::store;
use super::Founding;
use std::path::Path;

/// Why a depot cannot be founded (English: keys of the translations).
pub const NOT_ON_SERVER: &str = "Depots are for the companies of a Bus Company Online server.";
pub const DEPOT_OF_DEPOT: &str = "A depot has no depots of its own: found them from the head office.";
pub const NO_MAP: &str = "That map is not installed on the server.";
pub const SAME_MAP: &str = "The head office runs this map: a depot goes on another.";
pub const HAS_DEPOT: &str = "The company has a depot on this map already.";
pub const NO_CASH: &str = "The head office has not the money for it: the depot and its starting money.";
pub const NO_CLOCK: &str = "The server's clock is not set: the main server keeps it.";
pub const NOT_FOUNDED: &str = "The depot could not be founded: the server says why in its log.";

/// Every text of this file (for the translations' check).
pub fn texts() -> Vec<&'static str> {
    vec![NOT_ON_SERVER, DEPOT_OF_DEPOT, NO_MAP, SAME_MAP, HAS_DEPOT, NO_CASH, NO_CLOCK, NOT_FOUNDED]
}

/// A depot as the head office sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Depot {
    pub id: String,
    pub name: String,
    pub map: String,
    pub cash: Cents,
    pub lines: usize,
    pub buses: usize,
    pub staff: usize,
}

/// The depots of head office `hq` in the server's folder `data`, by name.
pub fn depots_of(data: &Path, hq: &str) -> Vec<Depot> {
    let mut out: Vec<Depot> = company_ids(data)
        .into_iter()
        .filter_map(|id| store::load(data, &id).ok())
        .filter(|c| !hq.is_empty() && c.online.hq == hq)
        .map(|c| Depot { id: c.id.clone(), name: c.name.clone(), map: c.map.clone(), cash: c.cash, lines: c.lines.len(), buses: c.fleet.len(), staff: c.staff.len() })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// The maps of `root` a depot of `hq` may go on: not the head office's, nor one it has a depot on.
pub fn maps_for(data: &Path, root: &Path, hq: &Company) -> Vec<String> {
    let taken: Vec<String> = depots_of(data, &hq.id).iter().map(|d| map_folder(&d.map)).chain([map_folder(&hq.map)]).collect();
    super::main_server::installed_maps(root).into_iter().filter(|m| !taken.contains(&m.to_lowercase())).collect()
}

/// Head office `hq` founds a depot on map `map` (a map's folder or name in `root`), paying `price`
/// and giving it `start` (cents) from its own money: a company of the server on that map with the
/// head office's name and the map's, its mark, game mode, country and owner, on the server's clock
/// (`online::found_on_server`), tied to it (`Online::hq`). The main server starts its process at
/// its next look into the folder. The depot's id.
pub fn found(data: &Path, root: &Path, hq: &mut Company, map: &str, start: Cents, price: Cents, real: i64) -> Result<String, &'static str> {
    if !hq.online.on_server {
        return Err(NOT_ON_SERVER);
    }
    if !hq.online.hq.is_empty() {
        return Err(DEPOT_OF_DEPOT);
    }
    let map = find_map(root, map).ok_or(NO_MAP)?;
    if map_folder(&map) == map_folder(&hq.map) {
        return Err(SAME_MAP);
    }
    if depots_of(data, &hq.id).iter().any(|d| map_folder(&d.map) == map_folder(&map)) {
        return Err(HAS_DEPOT);
    }
    let start = start.max(0);
    if hq.cash < price + start {
        return Err(NO_CASH);
    }
    let clock = online::read_server_clock(data).ok_or(NO_CLOCK)?;
    let f = Founding {
        name: format!("{} {}", hq.name, map_name(&map)),
        short: hq.short.clone(),
        colours: hq.colours.clone(),
        logo: hq.logo.clone(),
        map: map.clone(),
        map_name: map_name(&map),
        game_mode: hq.game_mode,
        company_mode: hq.company_mode,
        country: hq.country.clone(),
        currency: hq.currency,
        ..Default::default()
    };
    let id = online::found_on_server(data, &f, super::main_server::SERVER_PROFILE, clock, real).map_err(|_| NOT_FOUNDED)?;
    let mut d = store::load(data, &id).map_err(|_| NOT_FOUNDED)?;
    d.online.hq = hq.id.clone();
    d.online.owner = hq.online.owner.clone();
    d.online.owner_id = hq.online.owner_id.clone();
    d.online.share = hq.online.share;
    // (its money the head office's: nothing of a founding's own capital)
    d.cash = 0;
    d.ledger.clear();
    d.months.clear();
    d.book(BookingKind::Capital, start, format!("{}: the head office's money", hq.name), false);
    store::save(data, &d).map_err(|_| NOT_FOUNDED)?;
    hq.book(BookingKind::Construction, -price, format!("Depot {}", d.name), false);
    hq.book(BookingKind::Capital, -start, format!("Depot {}: its starting money", d.name), false);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A head office founds a depot on another map: paid from its money, the depot with its starting
    /// money, its name, mark, mode and owner, tied to it; not on its own map, not twice on a map,
    /// not without the money, not from a depot.
    #[test]
    fn a_head_office_founds_a_depot_on_another_map() {
        let dir = std::env::temp_dir().join(format!("openomsi-server-depots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("omsi");
        for m in ["Grundorf", "Ahlheim", "Spandau"] {
            std::fs::create_dir_all(root.join("maps").join(m)).unwrap();
            std::fs::write(root.join("maps").join(m).join("global.cfg"), "").unwrap();
        }
        let data = dir.join("bco");
        online::server_clock(&data, 1_800_000_000, false, ("2024-03-04".into(), 6 * 3600)).unwrap();
        let mut hq = super::super::found(&Founding { name: "Umove".into(), short: "UMO".into(), date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "x");
        hq.id = "umove".into();
        hq.online.on_server = true;
        hq.online.owner = "Mathias".into();
        hq.online.owner_id = "p1".into();
        store::save(&data, &hq).unwrap();
        let cash = hq.cash;
        assert_eq!(maps_for(&data, &root, &hq).len(), 2, "not its own map");
        assert_eq!(found(&data, &root, &mut hq, "Grundorf", 100_000_00, 250_000_00, 1_800_000_000), Err(SAME_MAP));
        assert_eq!(found(&data, &root, &mut hq, "Nowhere", 100_000_00, 250_000_00, 1_800_000_000), Err(NO_MAP));
        assert_eq!(found(&data, &root, &mut hq, "Ahlheim", cash, 250_000_00, 1_800_000_000), Err(NO_CASH));
        let id = found(&data, &root, &mut hq, "Ahlheim", 100_000_00, 250_000_00, 1_800_000_000).unwrap();
        assert_eq!(hq.cash, cash - 350_000_00);
        let d = store::load(&data, &id).unwrap();
        assert_eq!((d.name.as_str(), d.short.as_str(), d.online.hq.as_str(), d.online.owner_id.as_str(), d.cash), ("Umove Ahlheim", "UMO", "umove", "p1", 100_000_00));
        assert!(d.online.on_server && map_folder(&d.map) == "ahlheim" && d.game_mode == hq.game_mode);
        assert_eq!(depots_of(&data, "umove").iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), ["Umove Ahlheim"]);
        assert_eq!(maps_for(&data, &root, &hq), ["Spandau"]);
        assert_eq!(found(&data, &root, &mut hq, "Ahlheim", 0, 250_000_00, 1_800_000_000), Err(HAS_DEPOT));
        let mut depot = d.clone();
        assert_eq!(found(&data, &root, &mut depot, "Spandau", 0, 0, 1_800_000_000), Err(DEPOT_OF_DEPOT));
        let mut own = hq.clone();
        own.online.on_server = false;
        assert_eq!(found(&data, &root, &mut own, "Spandau", 0, 0, 1_800_000_000), Err(NOT_ON_SERVER));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
