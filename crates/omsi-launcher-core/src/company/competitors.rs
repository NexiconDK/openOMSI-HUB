//! (NexiconDK, Bus Company Online, decided 3 November 2026) The competitors the server's admin
//! makes: the AI operators that bid in the server's tenders on every map, each with a name, a
//! size, a style, its buses and a quality (the admin panel's tab "Competitors", `<data>/competitors.json`).
//!
//! * **The size** says which packages it bids on and how many contracts it holds at once on a map
//!   (`Size`): small - small packages, at most 2; medium - small and medium ones, at most 4; large
//!   - every package, at most 8 (by the work's timetable hours a week: `concessions::WorkSize`). A
//!   large one runs leaner: its own cost is the authority's estimate by `Size::own_share`.
//! * **The style** says what it puts on its own cost (`Style`): careful 10 to 15 %, normal 5 to 10 %,
//!   aggressive 0 to 5 % - never under the abnormally low line.
//! * **Its buses**: the fuels and floors it runs (none named: diesel, low floor). It bids only on a
//!   tender whose requirements its buses meet (`server_buses::Needs`), with the first of its fuels
//!   the tender allows.
//! * **Its quality** (0 to 100): its quality's parts in a bid lie around it
//!   (`viborg::operator_parts`); it rises a point with each contract of its that ran its term
//!   (`ENDED_WELL`) and falls five with each it gave up (`GAVE_UP`).
//!
//! A map's market takes the list at each step as its operators (`Market::rivals`, by their ids;
//! an open tender's copy is brought up to date, one deleted leaves its bidders); its contracts run
//! out. A server without a list keeps each map's operators as they were drawn (`concessions::make_rivals`).
//! The admin's "Operators bidding in a tender" (`tender_operators`) still says how many of them
//! are drawn for a tender; without it every competitor is.

use super::auction::Character;
use super::concessions::{Rival, Tender, WorkSize};
use super::model::Drive;
use super::server_buses::{Floor, Needs};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A contract run to its term raises a competitor's quality this much; one given up lowers it so.
pub const ENDED_WELL: f64 = 1.0;
pub const GAVE_UP: f64 = -5.0;

/// A new competitor's quality when the admin names none.
pub const QUALITY: f64 = 80.0;

/// A competitor's size.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Size {
    Small,
    #[default]
    Medium,
    Large,
}

impl Size {
    pub const ALL: [Size; 3] = [Size::Small, Size::Medium, Size::Large];

    /// The most contracts it holds at once on a map.
    pub fn most(self) -> usize {
        match self {
            Size::Small => 2,
            Size::Medium => 4,
            Size::Large => 8,
        }
    }

    /// Whether it bids on a work of this size.
    pub fn takes(self, w: WorkSize) -> bool {
        match self {
            Size::Small => w == WorkSize::Small,
            Size::Medium => w != WorkSize::Large,
            Size::Large => true,
        }
    }

    /// Its own cost against the authority's estimate of the work: a large one has its scale.
    pub fn own_share(self) -> f64 {
        match self {
            Size::Small => 1.00,
            Size::Medium => 0.97,
            Size::Large => 0.93,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Size::Small => "small",
            Size::Medium => "medium",
            Size::Large => "large",
        }
    }

    pub fn of_key(key: &str) -> Option<Size> {
        Size::ALL.into_iter().find(|s| s.key() == key.trim().to_lowercase())
    }

    pub fn label(self) -> &'static str {
        match self {
            Size::Small => "Small",
            Size::Medium => "Medium",
            Size::Large => "Large",
        }
    }
}

/// A competitor's style: what it puts on its own cost.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    Careful,
    #[default]
    Normal,
    Aggressive,
}

impl Style {
    pub const ALL: [Style; 3] = [Style::Careful, Style::Normal, Style::Aggressive];

    /// Its margin over its own cost (from, to).
    pub fn margin(self) -> (f64, f64) {
        match self {
            Style::Careful => (0.10, 0.15),
            Style::Normal => (0.05, 0.10),
            Style::Aggressive => (0.0, 0.05),
        }
    }

    /// The operators' character it is shown and timed as where one is asked (`auction::Character`).
    pub fn character(self, size: Size) -> Character {
        match (self, size) {
            (Style::Aggressive, _) => Character::Aggressive,
            (Style::Careful, _) => Character::Cautious,
            (Style::Normal, Size::Small) => Character::Small,
            (Style::Normal, _) => Character::Big,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Style::Careful => "careful",
            Style::Normal => "normal",
            Style::Aggressive => "aggressive",
        }
    }

    pub fn of_key(key: &str) -> Option<Style> {
        Style::ALL.into_iter().find(|s| s.key() == key.trim().to_lowercase())
    }

    pub fn label(self) -> &'static str {
        match self {
            Style::Careful => "Careful",
            Style::Normal => "Normal",
            Style::Aggressive => "Aggressive",
        }
    }
}

/// What the admin made of an operator, kept with it in a market (`concessions::Rival::made`): its
/// size, style and buses.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Made {
    pub size: Size,
    pub style: Style,
    pub fuels: Vec<Drive>,
    pub floors: Vec<Floor>,
}

impl Made {
    /// Its fuels (none named: diesel) and floors (none named: low floor).
    pub fn fuels(&self) -> Vec<Drive> {
        if self.fuels.is_empty() {
            vec![Drive::Diesel]
        } else {
            self.fuels.clone()
        }
    }

    pub fn floors(&self) -> Vec<Floor> {
        if self.floors.is_empty() {
            vec![Floor::LowFloor]
        } else {
            self.floors.clone()
        }
    }

    /// The buses it bids a tender asking `needs` with: the first of its fuels the tender allows,
    /// when one of its floors is allowed too (None: its buses do not meet them).
    pub fn drive_for(&self, needs: &Needs) -> Option<Drive> {
        if !self.floors().iter().any(|f| needs.floor.allows(*f)) {
            return None;
        }
        self.fuels().into_iter().find(|d| needs.drive_ok(*d))
    }

    /// Whether it bids on tender `t` at all: the size of its work, and its buses against what it
    /// asks of them (the drive it would bid with).
    pub fn bids_on(&self, t: &Tender) -> Option<Drive> {
        if !self.size.takes(WorkSize::of_hours(super::concessions::week_hours(t))) {
            return None;
        }
        let needs = t.market.as_ref().and_then(|m| m.needs).unwrap_or_default();
        self.drive_for(&needs)
    }
}

/// A competitor of the server's.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Competitor {
    pub id: u32,
    pub name: String,
    pub quality: f64,
    #[serde(flatten)]
    pub made: Made,
}

impl Competitor {
    /// As a map's operator.
    pub fn rival(&self) -> Rival {
        Rival { id: self.id, name: self.name.clone(), character: self.made.style.character(self.made.size), quality: self.quality.clamp(0.0, 100.0), made: Some(self.made.clone()) }
    }
}

/// The server's competitors (`<data>/competitors.json`): the list and the last id given.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Competitors {
    pub list: Vec<Competitor>,
    pub counter: u32,
}

/// Why the admin's competitor is refused.
pub const NO_NAME: &str = "A competitor needs a name.";
pub const NAME_TAKEN: &str = "Another competitor has that name.";
pub const NO_SUCH: &str = "There is no such competitor.";

impl Competitors {
    /// The map's operators by the list (None: no list - each map's own drawn ones).
    pub fn rivals(&self) -> Option<Vec<Rival>> {
        (!self.list.is_empty()).then(|| self.list.iter().map(Competitor::rival).collect())
    }

    /// The admin adds a competitor (`id` 0) or changes one; its id.
    pub fn put(&mut self, mut c: Competitor) -> Result<u32, &'static str> {
        c.name = c.name.trim().to_string();
        if c.name.is_empty() {
            return Err(NO_NAME);
        }
        if self.list.iter().any(|x| x.id != c.id && x.name.eq_ignore_ascii_case(&c.name)) {
            return Err(NAME_TAKEN);
        }
        c.quality = if c.quality.is_finite() { c.quality.clamp(0.0, 100.0) } else { QUALITY };
        c.made.fuels.dedup();
        c.made.floors.dedup();
        if c.id == 0 {
            self.counter = self.counter.max(self.list.iter().map(|x| x.id).max().unwrap_or(0)) + 1;
            c.id = self.counter;
            self.list.push(c);
            return Ok(self.counter);
        }
        let x = self.list.iter_mut().find(|x| x.id == c.id).ok_or(NO_SUCH)?;
        *x = c;
        Ok(x.id)
    }

    /// The admin deletes one (its contracts run out).
    pub fn delete(&mut self, id: u32) -> Result<String, &'static str> {
        let k = self.list.iter().position(|x| x.id == id).ok_or(NO_SUCH)?;
        Ok(self.list.remove(k).name)
    }

    /// A competitor's quality moved by `delta` (one of the list's by its name; another: nothing).
    pub fn moved(&mut self, name: &str, delta: f64) -> bool {
        match self.list.iter_mut().find(|x| x.name == name) {
            Some(x) => {
                x.quality = (x.quality + delta).clamp(0.0, 100.0);
                true
            }
            None => false,
        }
    }
}

fn file(data: &Path) -> PathBuf {
    data.join("competitors.json")
}

/// The server's competitors (none yet: an empty list).
pub fn load(data: &Path) -> Competitors {
    std::fs::read(file(data)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

/// The list written whole, through a file beside it.
pub fn save(data: &Path, c: &Competitors) -> Result<()> {
    let path = file(data);
    std::fs::create_dir_all(data).with_context(|| format!("cannot make {}", data.display()))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(c)?).with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

/// A map's market's operators brought up to the server's list (when it has one): the market's
/// own, and each open tender's copy - a competitor the admin changed taken as it is now, one he
/// deleted out of the tender's bidders. Whether anything changed.
pub fn bring_up(m: &mut super::shared_tenders::Market, list: &Competitors) -> bool {
    let Some(rivals) = list.rivals() else { return false };
    let mut changed = false;
    if m.rivals != rivals {
        m.rivals = rivals.clone();
        changed = true;
    }
    for mt in m.tenders.iter_mut().filter(|x| x.decided.is_none()) {
        let Some(x) = mt.tender.market.as_mut() else { continue };
        for r in x.rivals.iter_mut().filter(|r| r.made.is_some()) {
            if let Some(now) = rivals.iter().find(|n| n.id == r.id && n.name == r.name) {
                if r != now {
                    *r = now.clone();
                    changed = true;
                }
            }
        }
        let gone: Vec<u32> = x.rivals.iter().filter(|r| r.made.is_some() && !rivals.iter().any(|n| n.id == r.id && n.name == r.name)).map(|r| r.id).collect();
        if !gone.is_empty() {
            let before = mt.tender.bidders.len();
            mt.tender.bidders.retain(|b| !gone.contains(b));
            changed |= mt.tender.bidders.len() != before;
        }
        // (one of the list's not yet among the tender's operators: a competitor the admin added -
        // it may be drawn for the next tenders, not this one)
    }
    changed
}

/// The contracts a competitor named `name` holds on a map now (`Market::held`; a package's lines
/// one).
pub fn holds(m: &super::shared_tenders::Market, name: &str) -> usize {
    let mut seen: Vec<String> = Vec::new();
    for h in m.held.iter().filter(|h| h.name == name) {
        let key = if h.package.is_empty() { h.line.to_lowercase() } else { format!("package:{}", h.package.to_lowercase()) };
        if !seen.contains(&key) {
            seen.push(key);
        }
    }
    seen.len()
}

/// The bidders of tender `t` of market `m` at its close: an admin's competitor without room - as
/// many contracts on the map as its size allows - left out.
pub fn with_room(m: &super::shared_tenders::Market, t: &Tender) -> Vec<u32> {
    let Some(x) = t.market.as_ref() else { return t.bidders.clone() };
    t.bidders
        .iter()
        .copied()
        .filter(|id| match x.rivals.iter().find(|r| r.id == *id).and_then(|r| r.made.as_ref().map(|mk| (r, mk))) {
            Some((r, mk)) => holds(m, &r.name) < mk.size.most(),
            None => true,
        })
        .collect()
}

#[cfg(test)]
#[path = "competitors_tests.rs"]
mod tests;
