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
    vec![NOT_ON_SERVER, DEPOT_OF_DEPOT, NO_MAP, SAME_MAP, HAS_DEPOT, NO_CASH, NO_CLOCK, NOT_FOUNDED, NOT_RELATED, NO_AMOUNT, NOT_SENT, NO_CASH_SEND]
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

// --- step 2: money between the head office and its depots ---------------------------------------

/// Money a company sent another of its own (a head office and its depot), as the receiving
/// company's process takes it (`<folder>/transfers/<to>.jsonl`).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Transfer {
    pub from: String,
    pub name: String,
    pub amount: Cents,
    /// When it was sent (Unix seconds).
    pub at: i64,
}

/// Why money cannot be sent.
pub const NOT_RELATED: &str = "Money goes only between a head office and its own depots.";
pub const NO_AMOUNT: &str = "Send at least 100.";
pub const NOT_SENT: &str = "The money could not be sent: the server says why in its log.";

/// The money sent to company `id`.
pub fn transfers_file(data: &Path, id: &str) -> std::path::PathBuf {
    data.join("transfers").join(format!("{id}.jsonl"))
}

/// Whether companies `a` and `b` are a head office and its depot (either way round), or two depots
/// of one head office (`hq` of each).
fn related(a: &Company, b_id: &str, b_hq: &str) -> bool {
    a.online.hq == b_id || b_hq == a.id || (!a.online.hq.is_empty() && a.online.hq == b_hq)
}

/// Company `c` sends `amount` of its money to `to` (its head office, one of its depots, or another
/// depot of its head office): booked out of its books now, into `to`'s at its process's next step
/// (`take_transfers`). An error: no such company of the server, not one of its own, no money.
pub fn send(data: &Path, c: &mut Company, to: &str, amount: Cents, real: i64) -> Result<(), &'static str> {
    if !c.online.on_server {
        return Err(NOT_ON_SERVER);
    }
    let other = store::load(data, to).map_err(|_| NOT_RELATED)?;
    if other.id == c.id || !related(c, &other.id, &other.online.hq) {
        return Err(NOT_RELATED);
    }
    if amount < 100_00 {
        return Err(NO_AMOUNT);
    }
    if c.cash < amount {
        return Err(NO_CASH_SEND);
    }
    let t = Transfer { from: c.id.clone(), name: c.name.clone(), amount, at: real };
    let line = serde_json::to_string(&t).map_err(|_| NOT_SENT)?;
    let path = transfers_file(data, to);
    let written = (|| -> std::io::Result<()> {
        use std::io::Write;
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&path)?;
        f.write_all(format!("{line}\n").as_bytes())
    })();
    written.map_err(|_| NOT_SENT)?;
    c.book(BookingKind::Capital, -amount, format!("To {}", other.name), false);
    Ok(())
}

/// The company has not the money to send.
pub const NO_CASH_SEND: &str = "The company has not that much money.";

/// The money sent to company `c` since its last look (`Online::transfers_read`), booked into its
/// books - from a company of its own only. How much came.
pub fn take_transfers(data: &Path, c: &mut Company) -> Cents {
    let Ok(bytes) = std::fs::read(transfers_file(data, &c.id)) else { return 0 };
    let from = c.online.transfers_read as usize;
    let start = if from > bytes.len() { 0 } else { from };
    let rest = &bytes[start..];
    let whole = rest.iter().rposition(|b| *b == b'\n').map_or(0, |k| k + 1);
    let mut total = 0;
    for t in String::from_utf8_lossy(&rest[..whole]).lines().filter_map(|l| serde_json::from_str::<Transfer>(l).ok()) {
        let known = store::load(data, &t.from).ok().is_some_and(|o| related(c, &o.id, &o.online.hq));
        if !known || t.amount <= 0 {
            continue;
        }
        c.book(BookingKind::Capital, t.amount, format!("From {}", t.name), false);
        total += t.amount;
    }
    c.online.transfers_read = (start + whole) as u64;
    total
}

/// A depot of a head office as its view shows it: this month's result besides its figures.
pub fn month_result(data: &Path, id: &str, month: &str) -> Option<Cents> {
    store::load(data, id).ok().map(|d| d.month(month).result())
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
        // (step 2) money goes between them: out of the sender's books at once, into the other's
        // at its next step - once; not to a company of another's, not more than there is
        store::save(&data, &hq).unwrap();
        let mut d = store::load(&data, &id).unwrap();
        let (hq_cash, d_cash) = (hq.cash, d.cash);
        send(&data, &mut hq, &id, 50_000_00, 1).unwrap();
        assert_eq!(hq.cash, hq_cash - 50_000_00);
        assert_eq!(take_transfers(&data, &mut d), 50_000_00);
        assert_eq!(take_transfers(&data, &mut d), 0, "once");
        assert_eq!(d.cash, d_cash + 50_000_00);
        send(&data, &mut d, "umove", 20_000_00, 2).unwrap();
        assert_eq!(take_transfers(&data, &mut hq), 20_000_00);
        let more = d.cash + 1;
        assert_eq!(send(&data, &mut d, "umove", more, 3), Err(NO_CASH_SEND));
        assert_eq!(send(&data, &mut d, "umove", 50, 3), Err(NO_AMOUNT));
        let mut stranger = super::super::found(&Founding { name: "Other".into(), date: "2024-03-04".into(), map: "maps/Spandau/global.cfg".into(), ..Default::default() }, "y");
        stranger.id = "other".into();
        stranger.online.on_server = true;
        store::save(&data, &stranger).unwrap();
        assert_eq!(send(&data, &mut stranger, "umove", 100_00, 4), Err(NOT_RELATED));
        assert_eq!(send(&data, &mut hq, "other", 100_00, 4), Err(NOT_RELATED));
        // (money a stranger wrote into the file himself is not taken)
        let forged = Transfer { from: "other".into(), name: "Other".into(), amount: 1_000_000_00, at: 5 };
        std::fs::write(transfers_file(&data, "umove"), format!("{}\n", serde_json::to_string(&forged).unwrap())).unwrap();
        let before = hq.cash;
        hq.online.transfers_read = 0;
        assert_eq!(take_transfers(&data, &mut hq), 0);
        assert_eq!(hq.cash, before);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
