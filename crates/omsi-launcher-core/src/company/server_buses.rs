//! (NexiconDK, Bus Company Online, decided 3 November 2026) The buses of the server as its
//! tenders ask for them: each installed bus's floor (low floor, low entry, high floor), its fuel
//! (diesel/HVO, CNG, electric), its size and whether it has passenger information screens - the
//! game's guess from the bus's files and name (`guess_floor`, `market::guess_kind`), corrected by
//! the server's admin on the admin panel's "Buses" (`<data>/buses.json`, `Overrides`). His
//! corrections hold everywhere on the server: the companies' fleets (`apply`), the market's
//! buses, the requirements a tender sets (`Needs`), the vehicle points of the Viborg model and
//! the authority's fines.
//!
//! The floor is guessed from the words of the bus's name and file:
//! * **high floor**: a coach (`vehicles::guess_coach`), or "Reisebus", "Coach", "Tourismo",
//!   "Travego", "GT", "HD", "SHD", "UL" (Setra's high-floor intercity buses), "Überland", or an
//!   "Intercity" that does not say it is low entry;
//! * **low entry**: "LE", "Low entry" ("Lion's Intercity LE", "Citaro LE");
//! * **low floor**: every other bus - a city bus.

use super::market::MarketBus;
use super::model::{BusKind, BusSize, Company, Drive, Vehicle};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// How high a bus's floor is.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Floor {
    #[default]
    LowFloor,
    LowEntry,
    HighFloor,
}

impl Floor {
    pub const ALL: [Floor; 3] = [Floor::LowFloor, Floor::LowEntry, Floor::HighFloor];

    pub fn label(self) -> &'static str {
        match self {
            Floor::LowFloor => "Low floor",
            Floor::LowEntry => "Low entry",
            Floor::HighFloor => "High floor",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Floor::LowFloor => "low_floor",
            Floor::LowEntry => "low_entry",
            Floor::HighFloor => "high_floor",
        }
    }

    pub fn of_key(key: &str) -> Option<Floor> {
        Floor::ALL.into_iter().find(|f| f.key() == key.trim())
    }

    /// Its vehicle points in the Viborg model (`viborg::vehicles`).
    pub fn points(self) -> f64 {
        match self {
            Floor::LowFloor => 100.0,
            Floor::LowEntry => 70.0,
            Floor::HighFloor => 30.0,
        }
    }
}

/// A drive's key in the admin panel and the requirements (`diesel`, `cng`, `electric`).
pub fn drive_key(d: Drive) -> &'static str {
    match d {
        Drive::Diesel => "diesel",
        Drive::Cng => "cng",
        Drive::Electric => "electric",
    }
}

pub fn drive_of_key(key: &str) -> Option<Drive> {
    match key.trim() {
        "diesel" => Some(Drive::Diesel),
        "cng" => Some(Drive::Cng),
        "electric" => Some(Drive::Electric),
        _ => None,
    }
}

/// A drive's label: "Diesel/HVO", "CNG", "Electric".
pub fn drive_label(d: Drive) -> &'static str {
    match d {
        Drive::Diesel => "Diesel/HVO",
        Drive::Cng => "CNG",
        Drive::Electric => "Electric",
    }
}

/// A size's key (`midi`, `solo`, `articulated`, `double`).
pub fn size_key(s: BusSize) -> &'static str {
    match s {
        BusSize::Midi => "midi",
        BusSize::Solo => "solo",
        BusSize::Articulated => "articulated",
        BusSize::Double => "double",
    }
}

pub fn size_of_key(key: &str) -> Option<BusSize> {
    match key.trim() {
        "midi" => Some(BusSize::Midi),
        "solo" => Some(BusSize::Solo),
        "articulated" => Some(BusSize::Articulated),
        "double" => Some(BusSize::Double),
        _ => None,
    }
}

/// Lower-case words of a text.
fn words(s: &str) -> Vec<String> {
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// The words that make a bus high floor (with a coach).
const HIGH: [&str; 11] = ["reisebus", "coach", "tourismo", "travego", "gt", "hd", "shd", "hdh", "ul", "ueberland", "uberland"];

/// A bus's floor guessed from its name and file (`texts`) and whether it is a coach (see the
/// module's text).
pub fn guess_floor(texts: &[&str], coach: bool) -> Floor {
    let all = texts.join(" ").to_lowercase();
    let w: Vec<String> = texts.iter().flat_map(|t| words(t)).collect();
    let has = |x: &str| w.iter().any(|y| y == x);
    let low_entry = has("le") || all.contains("low entry") || all.contains("low-entry") || all.contains("lowentry");
    if low_entry {
        return Floor::LowEntry;
    }
    if coach || HIGH.iter().any(|k| has(k)) || all.contains("\u{fc}berland") || has("intercity") {
        return Floor::HighFloor;
    }
    Floor::LowFloor
}

/// The admin's corrections of a bus: each what he set (None: the game's guess).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(default)]
pub struct BusSet {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub floor: Option<Floor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drive: Option<Drive>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<BusSize>,
    /// Passenger information screens: the files do not tell, so only the admin says (None: no).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screens: Option<bool>,
}

impl BusSet {
    pub fn is_empty(&self) -> bool {
        *self == BusSet::default()
    }
}

/// The admin's corrections by the bus's file (`key`).
pub type Overrides = BTreeMap<String, BusSet>;

/// The file in the main server's folder.
pub const FILE: &str = "buses.json";

pub fn path(data: &Path) -> PathBuf {
    data.join(FILE)
}

/// A bus file as the corrections are kept by: forward slashes, lower case.
pub fn key(file: &str) -> String {
    file.trim().replace('\\', "/").to_lowercase()
}

/// The corrections of the server's folder `data` (none: empty).
pub fn load(data: &Path) -> Overrides {
    std::fs::read_to_string(path(data)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// The admin corrects bus `file` (an empty correction takes the bus back to the game's guess).
pub fn set(data: &Path, file: &str, s: BusSet) -> Result<()> {
    let mut o = load(data);
    if s.is_empty() {
        o.remove(&key(file));
    } else {
        o.insert(key(file), s);
    }
    std::fs::write(path(data), serde_json::to_string_pretty(&o)?)?;
    Ok(())
}

/// When the corrections were last written (seconds; 0: none).
fn written(data: &Path) -> u64 {
    std::fs::metadata(path(data)).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs().max(1))
}

/// The market's buses with the admin's drive and size (`market::market_of`'s guess otherwise).
pub fn apply_market(buses: &mut [MarketBus], o: &Overrides) {
    for b in buses {
        if let Some(s) = o.get(&key(&b.file)) {
            b.kind = BusKind { size: s.size.unwrap_or(b.kind.size), drive: s.drive.unwrap_or(b.kind.drive) };
        }
    }
}

/// Company `c` of a server with the corrections of its folder `data`: its buses' drive and size
/// as the admin set them, the corrections kept for their floors and screens (`floor_of`). Read
/// again when the file changed (its process asks at every step: `refresh`). Whether anything
/// changed.
pub fn refresh(data: &Path, c: &mut Company) -> bool {
    if !c.online.on_server {
        return false;
    }
    let at = written(data);
    if c.online.buses.is_some() && c.online.buses_at == at {
        return false;
    }
    let o = load(data);
    let old = c.online.buses.take().unwrap_or_default();
    c.online.buses_at = at;
    let mut changed = false;
    for v in c.fleet.iter_mut() {
        let kind = match o.get(&key(&v.bus)) {
            Some(s) => BusKind { size: s.size.unwrap_or(v.kind.size), drive: s.drive.unwrap_or(v.kind.drive) },
            // (a correction taken back while the company ran: the game's guess again)
            None if old.get(&key(&v.bus)).is_some_and(|s| s.drive.is_some() || s.size.is_some()) => super::market::kind_of(&v.bus, &v.name),
            None => continue,
        };
        changed |= kind != v.kind;
        v.kind = kind;
    }
    c.online.buses = Some(o);
    changed
}

/// The admin's correction of bus `file` for company `c` (none on a player's own computer).
fn set_of<'a>(c: &'a Company, file: &str) -> Option<&'a BusSet> {
    c.online.buses.as_ref().and_then(|o| o.get(&key(file)))
}

/// A bus's floor: the admin's, else guessed from its name and file.
pub fn floor_of(c: &Company, v: &Vehicle) -> Floor {
    set_of(c, &v.bus).and_then(|s| s.floor).unwrap_or_else(|| {
        let texts = [v.name.as_str(), v.bus.as_str()];
        guess_floor(&texts, super::vehicles::guess_coach(&texts))
    })
}

/// Whether a bus has passenger information screens (only the admin says so).
pub fn screens_of(c: &Company, v: &Vehicle) -> bool {
    set_of(c, &v.bus).and_then(|s| s.screens).unwrap_or(false)
}

// --- the requirements of a tender ------------------------------------------------------------------

/// What floor a tender asks for.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FloorRule {
    /// Low floor only (a town line).
    LowOnly,
    /// Low floor or low entry (a regional line).
    #[default]
    LowOrEntry,
    /// High floor allowed (a school run, an express).
    Any,
}

impl FloorRule {
    pub const ALL: [FloorRule; 3] = [FloorRule::LowOnly, FloorRule::LowOrEntry, FloorRule::Any];

    pub fn allows(self, f: Floor) -> bool {
        match self {
            FloorRule::LowOnly => f == Floor::LowFloor,
            FloorRule::LowOrEntry => f != Floor::HighFloor,
            FloorRule::Any => true,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            FloorRule::LowOnly => "low_only",
            FloorRule::LowOrEntry => "low_or_entry",
            FloorRule::Any => "any",
        }
    }

    pub fn of_key(key: &str) -> Option<FloorRule> {
        FloorRule::ALL.into_iter().find(|f| f.key() == key.trim())
    }

    pub fn label(self) -> &'static str {
        match self {
            FloorRule::LowOnly => "Low floor only",
            FloorRule::LowOrEntry => "Low floor or low entry",
            FloorRule::Any => "High floor allowed",
        }
    }
}

/// What fuel a tender asks for.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum FuelRule {
    #[default]
    Any,
    /// CNG or electric: no diesel.
    CngOrElectric,
    /// Electric only: zero emission.
    Electric,
}

impl FuelRule {
    pub const ALL: [FuelRule; 3] = [FuelRule::Any, FuelRule::CngOrElectric, FuelRule::Electric];

    pub fn allows(self, d: Drive) -> bool {
        match self {
            FuelRule::Any => true,
            FuelRule::CngOrElectric => d != Drive::Diesel,
            FuelRule::Electric => d.zero_emission(),
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            FuelRule::Any => "any",
            FuelRule::CngOrElectric => "cng_or_electric",
            FuelRule::Electric => "electric",
        }
    }

    pub fn of_key(key: &str) -> Option<FuelRule> {
        FuelRule::ALL.into_iter().find(|f| f.key() == key.trim())
    }

    pub fn label(self) -> &'static str {
        match self {
            FuelRule::Any => "Any fuel (diesel on HVO, CNG or electric)",
            FuelRule::CngOrElectric => "CNG or electric",
            FuelRule::Electric => "Electric only (zero emission)",
        }
    }
}

/// The oldest bus a tender takes by default (years).
pub const MAX_AGE: u32 = 12;

/// What a tender asks of the buses that run it: their floor, their fuel, their age and passenger
/// information screens. The admin sets them when he puts it out; the defaults otherwise.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Needs {
    pub floor: FloorRule,
    pub fuel: FuelRule,
    pub max_age: u32,
    pub screens: bool,
}

impl Default for Needs {
    fn default() -> Needs {
        Needs { floor: FloorRule::LowOrEntry, fuel: FuelRule::Any, max_age: MAX_AGE, screens: false }
    }
}

impl Needs {
    /// Whether bus `v` of company `c` meets them all.
    pub fn met_by(&self, c: &Company, v: &Vehicle) -> bool {
        self.floor.allows(floor_of(c, v)) && self.fuel.allows(v.kind.drive) && self.age_ok(c, v) && (!self.screens || screens_of(c, v))
    }

    pub fn age_ok(&self, c: &Company, v: &Vehicle) -> bool {
        self.max_age == 0 || v.age_years(&c.date) <= self.max_age as f64
    }

    /// Whether a bid with buses of drive `d` may be made.
    pub fn drive_ok(&self, d: Drive) -> bool {
        self.fuel.allows(d)
    }
}

/// The installed buses as the admin panel lists them: file, name, the game's guess and what holds
/// (the admin's correction over it), whether a coach, and the correction itself.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Listed {
    pub file: String,
    pub name: String,
    pub floor: Floor,
    pub drive: Drive,
    pub size: BusSize,
    pub coach: bool,
    pub screens: bool,
    pub guess_floor: Floor,
    pub guess_drive: Drive,
    pub guess_size: BusSize,
    pub set: BusSet,
}

/// The market's buses (as `market::market_of` guessed them) with the admin's corrections.
pub fn listed(buses: &[MarketBus], o: &Overrides) -> Vec<Listed> {
    buses
        .iter()
        .map(|b| {
            let texts = [b.name.as_str(), b.file.as_str()];
            let guess = guess_floor(&texts, b.coach);
            let s = o.get(&key(&b.file)).copied().unwrap_or_default();
            Listed {
                file: b.file.clone(),
                name: b.name.clone(),
                floor: s.floor.unwrap_or(guess),
                drive: s.drive.unwrap_or(b.kind.drive),
                size: s.size.unwrap_or(b.kind.size),
                coach: b.coach,
                screens: s.screens.unwrap_or(false),
                guess_floor: guess,
                guess_drive: b.kind.drive,
                guess_size: b.kind.size,
                set: s,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The floor guessed from the words: a city bus low floor, "LE" low entry, a coach or an
    /// "Intercity" / "UL" high floor.
    #[test]
    fn the_floor_is_guessed_from_the_words() {
        assert_eq!(guess_floor(&["Mercedes-Benz O530 Citaro", "Vehicles/Citaro/Citaro.bus"], false), Floor::LowFloor);
        assert_eq!(guess_floor(&["MAN Lion's Intercity LE", "Vehicles/MAN_LE/x.bus"], false), Floor::LowEntry);
        assert_eq!(guess_floor(&["Citaro Low Entry", "x.bus"], false), Floor::LowEntry);
        assert_eq!(guess_floor(&["Setra S 415 GT-HD", "x.bus"], false), Floor::HighFloor);
        assert_eq!(guess_floor(&["Setra S 415 UL", "x.bus"], false), Floor::HighFloor);
        assert_eq!(guess_floor(&["MAN Lion's Intercity", "x.bus"], false), Floor::HighFloor);
        assert_eq!(guess_floor(&["Anything", "x.bus"], true), Floor::HighFloor, "a coach");
        // ("Gelenk", "Lion's City" are no high floor; "le" only as a word)
        assert_eq!(guess_floor(&["MAN Lion's City G", "Vehicles/Lions City/lcg.bus"], false), Floor::LowFloor);
    }

    /// The requirements: the floor and the fuel rules, the defaults.
    #[test]
    fn the_rules_allow_and_refuse() {
        assert!(FloorRule::LowOnly.allows(Floor::LowFloor) && !FloorRule::LowOnly.allows(Floor::LowEntry));
        assert!(FloorRule::LowOrEntry.allows(Floor::LowEntry) && !FloorRule::LowOrEntry.allows(Floor::HighFloor));
        assert!(FloorRule::Any.allows(Floor::HighFloor));
        assert!(FuelRule::Any.allows(Drive::Diesel));
        assert!(!FuelRule::CngOrElectric.allows(Drive::Diesel) && FuelRule::CngOrElectric.allows(Drive::Cng));
        assert!(!FuelRule::Electric.allows(Drive::Cng) && FuelRule::Electric.allows(Drive::Electric));
        let n = Needs::default();
        assert_eq!((n.floor, n.fuel, n.max_age, n.screens), (FloorRule::LowOrEntry, FuelRule::Any, 12, false));
        let read: Needs = serde_json::from_str("{}").unwrap();
        assert_eq!(read, n);
        assert_eq!(FloorRule::of_key("low_only"), Some(FloorRule::LowOnly));
        assert_eq!(FuelRule::of_key("cng_or_electric"), Some(FuelRule::CngOrElectric));
        assert_eq!(drive_of_key("cng"), Some(Drive::Cng));
    }

    /// A bid's buses must be of a fuel the tender allows: CNG or electric - no diesel.
    #[test]
    fn a_bid_with_buses_the_tender_does_not_allow_is_refused() {
        use super::super::concessions::{self as cn, Shared};
        let mut c = super::super::found(&super::super::Founding { name: "Fuel".into(), date: "2026-03-04".into(), ..Default::default() }, "x");
        let l = cn::tests::line("Linie5", "5", 3, true);
        let mut t = cn::make_tender(&c, &l, 7, 0, false, "k", &[], (0, 0));
        t.market = Some(Shared { key: "k".into(), id: 7, needs: Some(Needs { fuel: FuelRule::CngOrElectric, ..Default::default() }), ..Default::default() });
        c.concessions.tenders.push(t);
        assert_eq!(cn::set_drive(&mut c, 7, Drive::Diesel), Err(cn::DRIVE_NOT_ALLOWED));
        assert_eq!(cn::set_drive(&mut c, 7, Drive::Cng), Ok(()));
        assert_eq!(c.concessions.tenders[0].drive, Drive::Cng);
    }

    /// The admin's corrections: written to the server's file, back to the guess when empty; a
    /// company of the server takes them for its buses (drive and size), its floors and screens,
    /// read again when the file changed; the market's buses too.
    #[test]
    fn the_admins_corrections_hold_on_the_server() {
        use super::super::market::{self, MarketBus};
        use super::super::model::Tenure;
        let data = std::env::temp_dir().join(format!("omsihub-server-buses-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data).unwrap();
        let file = "Vehicles\\MAN_Lion\\LionsCity.bus";
        let mut c = super::super::found(&super::super::Founding { name: "Buses".into(), date: "2026-03-04".into(), ..Default::default() }, "x");
        c.online.on_server = true;
        let b = MarketBus { file: file.into(), name: "MAN Lion's City".into(), kind: BusKind { size: BusSize::Solo, drive: Drive::Diesel }, ..Default::default() };
        market::add_vehicle(&mut c, &b, "2024-01-01".into(), 0.0, 1.0, Tenure::Owned { paid: 0, new_value: 0 }, "");
        assert!(!refresh(&data, &mut c), "no corrections");
        let v = c.fleet[0].clone();
        assert_eq!((floor_of(&c, &v), screens_of(&c, &v)), (Floor::LowFloor, false));
        set(&data, file, BusSet { floor: Some(Floor::LowEntry), drive: Some(Drive::Cng), size: None, screens: Some(true) }).unwrap();
        // (the file's time moves by whole seconds: forced again)
        c.online.buses_at = 0;
        assert!(refresh(&data, &mut c));
        let v = c.fleet[0].clone();
        assert_eq!((v.kind.drive, v.kind.size, floor_of(&c, &v), screens_of(&c, &v)), (Drive::Cng, BusSize::Solo, Floor::LowEntry, true));
        let mut m = vec![b.clone()];
        apply_market(&mut m, &load(&data));
        assert_eq!(m[0].kind.drive, Drive::Cng);
        let l = listed(&[b], &load(&data));
        assert_eq!((l[0].floor, l[0].guess_floor, l[0].drive, l[0].guess_drive), (Floor::LowEntry, Floor::LowFloor, Drive::Cng, Drive::Diesel));
        // (an empty correction: the guess again)
        set(&data, file, BusSet::default()).unwrap();
        assert!(load(&data).is_empty());
        let _ = std::fs::remove_dir_all(&data);
    }
}
