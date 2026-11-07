//! (NexiconDK, 6 November 2026) What the main server's admin gives a company of Bus Company Online
//! from the web admin's "Companies": its level, research done (or taken back), money given or
//! taken with a reason. (7 November 2026) And a contract's price set again (`price_of`), and its
//! depot's areas set to a level or their building work finished (`depot::Area`).
//!
//! A company of the server lives in its own process (`company_host`), which saves it over its file
//! now and then: a change written into the file while it runs would be lost at its next save. So
//! a grant goes the way the admin's other orders to a company go (`shared_tenders::
//! take_contract_orders`, `<data>/admin-orders/<id>.jsonl`, read from `Online::admin_read`): its
//! process takes it at its next step and saves it. When no process runs the company, the main
//! server takes it into the company's file at once (`take_now`, under the main server's lock - a
//! process started after reads the file with it, and `admin_read` past it: never taken twice).

use super::bus_contracts::Contract;
use super::currency::Currency;
use super::depot::Area;
use super::inbox::{self, Kind, Message};
use super::levels;
use super::model::{BookingKind, Cents, Company};
use super::research::{self, Running, TOPICS};
use super::store;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The most money given or taken at once (kroner).
pub const MOST_KR: f64 = 1_000_000_000.0;
/// The longest reason kept.
pub const REASON_MOST: usize = 120;
/// The text of the booking: "Bus Company Online: <reason>" (the company's accounts).
pub const BOOKED_AS: &str = "Bus Company Online";

/// One grant of the admin's.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Grant {
    /// The company's level: its experience set to the level's start (`levels::LEVEL_XP`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    /// A research topic and how many of its stages are done (0: none - taken back).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub research: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stages: Option<u32>,
    /// Money, euro cents (the company's books): given (more than 0) or taken.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub money: Option<i64>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reason: String,
    /// (7 November 2026) A contract's prices set again (`Contract::id`): what it pays as it pays
    /// today, euro cents (taken to its contract index when given: `price_of`); None: kept.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_hour: Option<Cents>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_bus_day: Option<Cents>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub buses: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year_price: Option<Cents>,
    /// (7 November 2026) A depot area (`Area::key`) set to a level, or its building work finished.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_level: Option<u32>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub finish: bool,
}

/// How a contract is paid, as the admin sets its price (`Prices::of`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Prices {
    /// A timetable hour run, a contract bus a day and its buses, a year - today's euro cents
    /// (None: not how it is paid).
    pub per_hour: Option<Cents>,
    pub per_bus_day: Option<Cents>,
    pub buses: Option<u32>,
    pub year: Option<Cents>,
}

impl Prices {
    /// What contract `k` pays today (the company's contract index `index`): by the timetable
    /// hour (and its contract buses a day, a bid in two parts), else by the year.
    pub fn of(k: &Contract, index: f64) -> Prices {
        let now = |x: Cents| (x as f64 * index / k.index.max(0.01)).round() as Cents;
        if k.per_hour > 0 {
            let two = k.per_bus_day > 0 || k.buses > 0;
            Prices { per_hour: Some(now(k.per_hour)), per_bus_day: two.then(|| now(k.per_bus_day)), buses: two.then_some(k.buses), year: None }
        } else {
            Prices { year: Some(now(k.year_price)), ..Default::default() }
        }
    }
}

/// What the admin can be told of a contract or an area (English: keys of the translations).
pub const NO_CONTRACT: &str = "The company has no such contract.";
pub const CONTRACT_ENDED: &str = "The contract has ended: its price is not changed.";
pub const NOT_PAID_SO: &str = "The contract is not paid that way.";
pub const NO_PRICE: &str = "A price is more than 0.";
pub const NO_BUSES: &str = "A contract in two parts has at least one bus.";
pub const NO_AREA: &str = "No such area of the depot.";
pub const NOT_BUILDING: &str = "Nothing is being built there.";
/// What the company is told (its inbox).
pub const PRICE_TITLE: &str = "Contract price changed";
pub const PRICE_HOUR: &str = "Bus Company Online changed the price of your contract on line %{n}: %{amount} a timetable hour %{reason}";
pub const PRICE_BUS_DAY: &str = "Bus Company Online changed the price of your contract on line %{n}: %{amount} a contract bus a day, %{count} buses %{reason}";
pub const PRICE_YEAR: &str = "Bus Company Online changed the price of your contract on line %{n}: %{amount} a year %{reason}";
pub const DEPOT_TITLE: &str = "Depot changed";
pub const DEPOT_SET: &str = "Bus Company Online set %{area} of your depot to level %{level}.";
pub const DEPOT_FINISHED: &str = "Bus Company Online finished the building work: %{area} level %{level} is in use now.";

/// Every text of this file (for the translations' check).
pub fn texts() -> Vec<&'static str> {
    vec![NO_CONTRACT, CONTRACT_ENDED, NOT_PAID_SO, NO_PRICE, NO_BUSES, NO_AREA, NOT_BUILDING, PRICE_TITLE, PRICE_HOUR, PRICE_BUS_DAY, PRICE_YEAR, DEPOT_TITLE, DEPOT_SET, DEPOT_FINISHED]
}

impl Grant {
    pub fn level(level: u32) -> Grant {
        Grant { level: Some(level), ..Default::default() }
    }

    pub fn research(key: &str, stages: u32) -> Grant {
        Grant { research: Some(key.to_string()), stages: Some(stages), ..Default::default() }
    }

    /// Money in kroner (the panel's money, `server_fines::Fines::cents`'s rate), with its reason.
    pub fn kroner(kr: f64, reason: &str) -> Grant {
        Grant { money: Some(Currency::Dkk.cents(kr)), reason: reason.trim().chars().take(REASON_MOST).collect(), ..Default::default() }
    }

    /// A contract's prices (today's euro cents) and buses, with the reason.
    pub fn contract(id: u32, p: Prices, reason: &str) -> Grant {
        Grant { contract: Some(id), per_hour: p.per_hour, per_bus_day: p.per_bus_day, buses: p.buses, year_price: p.year, reason: reason.trim().chars().take(REASON_MOST).collect(), ..Default::default() }
    }

    /// A depot area set to `level`.
    pub fn area(a: Area, level: u32) -> Grant {
        Grant { area: Some(a.key().to_string()), area_level: Some(level), ..Default::default() }
    }

    /// A depot area's building work finished.
    pub fn finish(a: Area) -> Grant {
        Grant { area: Some(a.key().to_string()), finish: true, ..Default::default() }
    }

    /// Whether it can be given to company `c` as it is: the contract live and paid the way its
    /// prices are, the area of the depot under way when finished. An error says why not
    /// (English: a key of the translations).
    pub fn fits(&self, c: &Company) -> Result<(), &'static str> {
        if let Some(id) = self.contract {
            let k = c.concessions.contracts.iter().find(|k| k.id == id).ok_or(NO_CONTRACT)?;
            if !k.live() || super::dates::between(&k.until, &c.date) > 0 {
                return Err(CONTRACT_ENDED);
            }
            let p = Prices::of(k, c.contract_index);
            if (self.per_hour.is_some() && p.per_hour.is_none()) || ((self.per_bus_day.is_some() || self.buses.is_some()) && p.buses.is_none()) || (self.year_price.is_some() && p.year.is_none()) {
                return Err(NOT_PAID_SO);
            }
        }
        if let Some(a) = &self.area {
            let a = Area::from_key(a).ok_or(NO_AREA)?;
            if self.finish && c.site.works_on(a).is_none() {
                return Err(NOT_BUILDING);
            }
        }
        Ok(())
    }

    /// Whether it can be given: a level of the ladder, a topic of the tree with its stages, an
    /// amount within `MOST_KR`. An error says why not.
    pub fn check(&self) -> Result<(), String> {
        if let Some(l) = self.level {
            if l < 1 || l > levels::max_level() {
                return Err(format!("the level is 1 to {}", levels::max_level()));
            }
        }
        if let Some(key) = &self.research {
            let Some(t) = TOPICS.iter().find(|t| t.key == key) else { return Err(format!("no research '{key}'")) };
            if self.stages.unwrap_or(t.stages) > t.stages {
                return Err(format!("{} has {} stages", t.title, t.stages));
            }
        }
        if let Some(m) = self.money {
            if m == 0 || (m.unsigned_abs() as f64) > Currency::Dkk.cents(MOST_KR) as f64 {
                return Err("no amount, or too much".into());
            }
        }
        if self.contract.is_some() {
            if [self.per_hour, self.per_bus_day, self.year_price].iter().flatten().any(|x| *x <= 0) {
                return Err(NO_PRICE.into());
            }
            if self.buses == Some(0) {
                return Err(NO_BUSES.into());
            }
            if self.per_hour.is_none() && self.per_bus_day.is_none() && self.buses.is_none() && self.year_price.is_none() {
                return Err(NO_PRICE.into());
            }
        }
        if let Some(a) = &self.area {
            let Some(area) = Area::from_key(a) else { return Err(NO_AREA.into()) };
            if !self.finish && self.area_level.is_none_or(|l| l > area.max()) {
                return Err(format!("{} is 0 to {}", area.label(), area.max()));
            }
        }
        if self.level.is_none() && self.research.is_none() && self.money.is_none() && self.contract.is_none() && self.area.is_none() {
            return Err("nothing to give".into());
        }
        Ok(())
    }

    /// What it gives, in words for the server's log.
    pub fn said(&self) -> String {
        let mut out = Vec::new();
        if let Some(l) = self.level {
            out.push(format!("level {l}"));
        }
        if let Some(k) = &self.research {
            out.push(format!("research {k}: {} stages done", self.stages.unwrap_or(0)));
        }
        if let Some(m) = self.money {
            let kr = Currency::Dkk.units(m as f64);
            out.push(format!("{}{:.0} kr. ({})", if m > 0 { "+" } else { "" }, kr, if self.reason.is_empty() { "no reason" } else { &self.reason }));
        }
        if let Some(id) = self.contract {
            let kr = |x: Cents| format!("{:.2} kr.", Currency::Dkk.units(x as f64));
            let mut p = Vec::new();
            if let Some(x) = self.per_hour {
                p.push(format!("{} a timetable hour", kr(x)));
            }
            if let Some(x) = self.per_bus_day {
                p.push(format!("{} a bus a day", kr(x)));
            }
            if let Some(b) = self.buses {
                p.push(format!("{b} buses"));
            }
            if let Some(x) = self.year_price {
                p.push(format!("{} a year", kr(x)));
            }
            out.push(format!("contract {id}: {} ({})", p.join(", "), if self.reason.is_empty() { "no reason" } else { &self.reason }));
        }
        if let Some(a) = &self.area {
            out.push(if self.finish { format!("depot {a}: building finished") } else { format!("depot {a}: level {}", self.area_level.unwrap_or(0)) });
        }
        out.join(", ")
    }
}

/// A contract's prices given (`Grant::contract`): set in its own euro cents at its contract index,
/// the company told. Nothing booked; the market's record of the tender stays as it was.
fn price_of(c: &mut Company, g: &Grant, id: u32) {
    let index = c.contract_index;
    let today = c.date.clone();
    let Some(k) = c.concessions.contracts.iter_mut().find(|k| k.id == id && k.live() && super::dates::between(&k.until, &today) <= 0) else { return };
    let was = Prices::of(k, index);
    // (today's euro cents to the contract's own, at its index)
    let at = k.index.max(0.01);
    let own = |x: Cents| (x as f64 * at / index.max(0.01)).round() as Cents;
    let reason = if g.reason.is_empty() { String::new() } else { format!("({})", g.reason) };
    let mut told: Vec<Message> = Vec::new();
    let tell = |text: &str, amount: Cents, number: &str| Message::new(Kind::Contract, PRICE_TITLE, text).arg("n", number).arg("amount", amount.to_string()).arg("reason", reason.clone());
    if let (Some(x), Some(_)) = (g.per_hour.filter(|x| *x > 0), was.per_hour) {
        k.per_hour = own(x);
        told.push(tell(PRICE_HOUR, x, &k.number));
    }
    if was.buses.is_some() && (g.per_bus_day.is_some() || g.buses.is_some()) {
        if let Some(x) = g.per_bus_day.filter(|x| *x > 0) {
            k.per_bus_day = own(x);
        }
        if let Some(b) = g.buses.filter(|b| *b > 0) {
            k.buses = b;
        }
        let day = g.per_bus_day.filter(|x| *x > 0).or(was.per_bus_day).unwrap_or(0);
        told.push(tell(PRICE_BUS_DAY, day, &k.number).arg("count", k.buses.to_string()));
    }
    if let (Some(x), Some(_)) = (g.year_price.filter(|x| *x > 0), was.year) {
        k.year_price = own(x);
        told.push(tell(PRICE_YEAR, x, &k.number));
    }
    for m in told {
        inbox::post(c, m);
    }
}

/// A depot area given (`Grant::area`, `Grant::finish`): its level set (what the level brings
/// follows from it, as built: `depot::Site`), work on it at or below that level done with; or its
/// building work finished now. Nothing paid; the company told.
fn area_of(c: &mut Company, g: &Grant, key: &str) {
    let Some(a) = Area::from_key(key) else { return };
    let level = if g.finish {
        let Some(w) = c.site.works_on(a).cloned() else { return };
        w.level
    } else {
        let Some(l) = g.area_level else { return };
        l.min(a.max())
    };
    c.site.set_level(a, level);
    // (the work for a level it has now: done with - for a higher one, it goes on)
    c.site.works.retain(|w| w.area != a || w.level > level);
    let text = if g.finish { DEPOT_FINISHED } else { DEPOT_SET };
    inbox::post(c, Message::new(Kind::Workshop, DEPOT_TITLE, text).arg("area", a.label()).arg("level", level.to_string()));
}

/// The grant given company `c` (its process's step, or `take_now`).
pub fn apply(c: &mut Company, g: &Grant) {
    if let Some(l) = g.level.filter(|l| (1..=levels::max_level()).contains(l)) {
        if levels::level(c) != l {
            c.progress.xp = levels::LEVEL_XP[l as usize - 1];
        }
    }
    if let Some(t) = g.research.as_deref().and_then(|k| TOPICS.iter().find(|t| t.key == k)) {
        let n = g.stages.unwrap_or(t.stages).min(t.stages);
        let keys: Vec<String> = (1..=t.stages).map(|s| research::stage_key(t, s)).collect();
        c.research.done.retain(|k| !keys.contains(k));
        c.research.done.extend(keys.into_iter().take(n as usize));
        // (its stage under way: given now, or no longer the next - stopped, all it cost back)
        if let Some(Running { money, trophies, .. }) = c.research.running.take_if(|r| r.topic == t.key && r.stage != n + 1) {
            c.book(BookingKind::Research, money, format!("{}: given by {BOOKED_AS}", t.title), false);
            c.trophies += trophies;
        }
        c.research.checked = true;
    }
    if let Some(m) = g.money.filter(|m| *m != 0) {
        let text = if g.reason.is_empty() { BOOKED_AS.to_string() } else { format!("{BOOKED_AS}: {}", g.reason) };
        c.book(BookingKind::Capital, m, text, false);
    }
    if let Some(id) = g.contract {
        price_of(c, g, id);
    }
    if let Some(key) = g.area.as_deref() {
        area_of(c, g, key);
    }
}

/// The admin gives company `id` the grant: into its orders, taken by its process; `runs`: whether
/// a process runs it now - when none does, taken into its file at once. An error: not written.
pub fn give(data: &Path, id: &str, g: &Grant, runs: bool) -> Result<()> {
    super::shared_tenders::order_grant(data, id, g)?;
    if !runs {
        take_now(data, id)?;
    }
    Ok(())
}

/// The admin's orders to company `id` taken into its file now (no process runs it). Whether any
/// came.
pub fn take_now(data: &Path, id: &str) -> Result<bool> {
    let mut c = store::load(data, id)?;
    if !super::shared_tenders::take_contract_orders(data, &mut c) {
        return Ok(false);
    }
    store::save(data, &c)?;
    Ok(true)
}

/// The admin's orders to company `c` not taken yet.
pub fn waiting(data: &Path, c: &Company) -> usize {
    super::shared_tenders::lines_from::<super::shared_tenders::ContractOrder>(&super::shared_tenders::contract_orders_file(data, &c.id), c.online.admin_read).0.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::company::research::keys;

    fn company() -> Company {
        let mut c = crate::company::found(&crate::company::Founding { name: "Givet".into(), date: "2024-03-04".into(), ..Default::default() }, "owner");
        c.online.on_server = true;
        c
    }

    #[test]
    fn a_level_research_and_money_given() {
        let mut c = company();
        c.progress.xp = 202;
        apply(&mut c, &Grant::level(3));
        assert_eq!((levels::level(&c), c.progress.xp), (3, 1_600));
        // (the level it has: its experience kept)
        c.progress.xp = 2_000;
        apply(&mut c, &Grant::level(3));
        assert_eq!(c.progress.xp, 2_000);
        // (lower, picked so)
        apply(&mut c, &Grant::level(1));
        assert_eq!(c.progress.xp, 0);
        // research: a single stage, and stages of one topic
        apply(&mut c, &Grant::research(keys::ARTICULATED, 1));
        assert!(research::allows(&c, crate::company::model::BusSize::Articulated));
        apply(&mut c, &Grant::research("fuel", 3));
        assert_eq!(c.research.done.iter().filter(|k| k.starts_with("fuel_")).count(), 3);
        apply(&mut c, &Grant::research("fuel", 1));
        assert_eq!(c.research.done.iter().filter(|k| k.starts_with("fuel_")).cloned().collect::<Vec<_>>(), ["fuel_1"]);
        apply(&mut c, &Grant::research(keys::ARTICULATED, 0));
        assert!(!c.research.has(keys::ARTICULATED));
        // a research under way, given: stopped, all it cost back
        c.research.running = Some(Running { topic: "dirt".into(), stage: 1, work: 4.0, done: 1.0, started: c.date.clone(), money: 10_000_00, trophies: 5, idle_told: false, paused: false });
        let (before, trophies) = (c.cash, c.trophies);
        apply(&mut c, &Grant::research("dirt", 1));
        assert!(c.research.running.is_none() && c.research.has("dirt_1"));
        assert_eq!((c.cash - before, c.trophies - trophies), (10_000_00, 5));
        // (another topic's under way: left alone)
        c.research.running = Some(Running { topic: "wear".into(), stage: 1, work: 4.0, ..Default::default() });
        apply(&mut c, &Grant::research("dirt", 3));
        assert!(c.research.running.is_some());
        // money in kroner, booked with its reason
        let at = c.cash;
        apply(&mut c, &Grant::kroner(7_450.0, "Kompensation for nedbrud"));
        assert_eq!(c.cash - at, Currency::Dkk.cents(7_450.0));
        let b = c.ledger.last().unwrap();
        assert_eq!((b.kind, b.text.as_str()), (BookingKind::Capital, "Bus Company Online: Kompensation for nedbrud"));
        apply(&mut c, &Grant::kroner(-7_450.0, ""));
        assert_eq!(c.cash, at);
        assert_eq!(c.ledger.last().unwrap().text, "Bus Company Online");
        // what cannot be given
        assert!(Grant::level(11).check().is_err() && Grant::level(0).check().is_err());
        assert!(Grant::research("nope", 1).check().is_err());
        assert!(Grant::research("fuel", 6).check().is_err());
        assert!(Grant::kroner(0.0, "").check().is_err());
        assert!(Grant::default().check().is_err());
        assert!(Grant::research("fuel", 5).check().is_ok() && Grant::kroner(-10.0, "x").check().is_ok());
    }

    /// A company with two contracts of the server's: line 131 by the timetable hour and its buses
    /// (a bid in two parts), agreed when the contract index was 1.0 and is 1.1 now; line 153 by
    /// the year; and one ended.
    fn with_contracts() -> Company {
        use crate::company::bus_contracts::{make, Kind as K, Period, Work};
        let mut c = company();
        c.contract_index = 1.0;
        let w = Work { hours: 10.0, km: 100.0, journeys: 20, buses: 3, days: 360 };
        let mut a = make(&mut c, "131", "131", K::Gross, Period::Day, 0, &w, "2024-03-01", "2025-02-28");
        a.per_hour = 60_00;
        a.per_bus_day = 200_00;
        a.buses = 3;
        let b = make(&mut c, "153", "153", K::Gross, Period::Week, 500_000_00, &w, "2024-03-01", "2025-02-28");
        let mut d = make(&mut c, "187", "187", K::Gross, Period::Week, 100_000_00, &w, "2023-03-01", "2024-02-28");
        d.ended = Some("2024-02-28".into());
        c.concessions.contracts.extend([a, b, d]);
        c.contract_index = 1.1;
        c
    }

    /// A contract's price set as it pays today, in kroner: kept in its own euro cents at its
    /// contract index, the company told - nothing booked.
    #[test]
    fn a_contract_price_set_in_kroner_as_it_pays_today() {
        let mut c = with_contracts();
        let p = Prices::of(&c.concessions.contracts[0], c.contract_index);
        assert_eq!((p.per_hour, p.per_bus_day, p.buses, p.year), (Some(66_00), Some(220_00), Some(3), None));
        assert_eq!(Prices::of(&c.concessions.contracts[1], c.contract_index).year, Some(550_000_00));
        // 550 kr. a timetable hour today, 1.800 kr. a bus a day, 4 buses
        let g = Grant::contract(1, Prices { per_hour: Some(Currency::Dkk.cents(550.0)), per_bus_day: Some(Currency::Dkk.cents(1_800.0)), buses: Some(4), year: None }, "For lav pris i buddet");
        assert!(g.check().is_ok() && g.fits(&c).is_ok());
        let (cash, books, mail) = (c.cash, c.ledger.len(), c.inbox.messages.len());
        apply(&mut c, &g);
        let k = &c.concessions.contracts[0];
        // (its own cents: today's taken back to the index it was agreed at)
        assert_eq!(k.per_hour, (Currency::Dkk.cents(550.0) as f64 / 1.1).round() as Cents);
        assert_eq!((k.buses, k.index), (4, 1.0));
        let now = Prices::of(k, c.contract_index);
        assert!((Currency::Dkk.units(now.per_hour.unwrap() as f64) - 550.0).abs() < 0.1);
        assert!((Currency::Dkk.units(now.per_bus_day.unwrap() as f64) - 1_800.0).abs() < 0.1);
        assert_eq!((c.cash, c.ledger.len()), (cash, books));
        let told: Vec<&Message> = c.inbox.messages[mail..].iter().collect();
        assert_eq!(told.len(), 2);
        assert_eq!((told[0].text.as_str(), told[0].value("n"), told[0].value("amount")), (PRICE_HOUR, Some("131"), Some(Currency::Dkk.cents(550.0).to_string().as_str())));
        assert_eq!((told[1].text.as_str(), told[1].value("count"), told[1].value("reason")), (PRICE_BUS_DAY, Some("4"), Some("(For lav pris i buddet)")));
        // by the year
        apply(&mut c, &Grant::contract(2, Prices { year: Some(Currency::Dkk.cents(4_500_000.0)), ..Default::default() }, ""));
        let y = Prices::of(&c.concessions.contracts[1], c.contract_index).year.unwrap();
        assert!((Currency::Dkk.units(y as f64) - 4_500_000.0).abs() < 1.0);
        assert_eq!((c.inbox.messages.last().unwrap().text.as_str(), c.inbox.messages.last().unwrap().value("reason")), (PRICE_YEAR, Some("")));
        // what is refused: no price, no bus, no such contract, an ended one, another way of paying
        assert!(Grant::contract(1, Prices { per_hour: Some(0), ..Default::default() }, "").check().is_err());
        assert!(Grant::contract(1, Prices { buses: Some(0), ..Default::default() }, "").check().is_err());
        assert!(Grant::contract(1, Prices::default(), "").check().is_err());
        assert_eq!(Grant::contract(9, Prices { year: Some(1), ..Default::default() }, "").fits(&c), Err(NO_CONTRACT));
        assert_eq!(Grant::contract(3, Prices { year: Some(1), ..Default::default() }, "").fits(&c), Err(CONTRACT_ENDED));
        assert_eq!(Grant::contract(2, Prices { per_hour: Some(1), ..Default::default() }, "").fits(&c), Err(NOT_PAID_SO));
        assert_eq!(Grant::contract(1, Prices { year: Some(1), ..Default::default() }, "").fits(&c), Err(NOT_PAID_SO));
        // (an ended one given all the same: left as it was)
        let before = c.concessions.contracts[2].clone();
        apply(&mut c, &Grant::contract(3, Prices { year: Some(1), ..Default::default() }, ""));
        assert_eq!(c.concessions.contracts[2], before);
    }

    /// A depot area set to a level (within its top), and its building work finished - nothing
    /// paid, what the level brings follows.
    #[test]
    fn a_depot_area_set_and_its_building_finished() {
        use crate::company::depot::{self, Works};
        let mut c = company();
        let cash = c.cash;
        assert_eq!(c.site.spaces(), 12);
        apply(&mut c, &Grant::area(Area::Parking, 5));
        assert_eq!((c.site.parking, c.site.spaces()), (5, 60));
        // (above its top: refused, and kept to it when given all the same)
        assert!(Grant::area(Area::Wash, 3).check().is_err() && Grant::area(Area::Wash, 2).check().is_ok());
        apply(&mut c, &Grant::area(Area::Wash, 9));
        assert_eq!((c.site.wash, c.site.washes()), (2, 40));
        apply(&mut c, &Grant::area(Area::Charging, 10));
        assert_eq!(c.site.electric_served(), 10 * depot::BUSES_PER_CHARGER);
        assert_eq!(c.cash, cash);
        // building work: finished now
        c.site.works.push(Works { area: Area::Workshop, level: 2, until: "2024-03-20".into() });
        assert_eq!(Grant::finish(Area::Offices).fits(&c), Err(NOT_BUILDING));
        assert!(Grant::finish(Area::Workshop).fits(&c).is_ok());
        apply(&mut c, &Grant::finish(Area::Workshop));
        assert_eq!((c.site.workshop, c.site.works.len()), (2, 0));
        assert_eq!(c.inbox.messages.last().unwrap().text, DEPOT_FINISHED);
        // a level set: work for it or below done with, for a higher one going on
        c.site.works.push(Works { area: Area::Fuel, level: 2, until: "2024-03-20".into() });
        c.site.works.push(Works { area: Area::Offices, level: 3, until: "2024-03-20".into() });
        apply(&mut c, &Grant::area(Area::Fuel, 2));
        apply(&mut c, &Grant::area(Area::Offices, 2));
        assert_eq!(c.site.works.iter().map(|w| w.area).collect::<Vec<_>>(), [Area::Offices]);
        assert_eq!(c.inbox.messages.last().unwrap().text, DEPOT_SET);
        assert_eq!(c.inbox.messages.last().unwrap().value("area"), Some("Offices and rest rooms"));
        assert!(Grant { area: Some("roof".into()), area_level: Some(1), ..Default::default() }.check().is_err());
        assert_eq!(c.cash, cash);
    }

    /// While its process runs the company, a grant waits in its orders: the process's own copy -
    /// saved over the file at its next save - takes it at its next step, and never twice.
    #[test]
    fn a_grant_made_while_the_company_runs_is_not_lost() {
        let dir = std::env::temp_dir().join(format!("openomsi-grants-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let data = dir.as_path();
        let c = company();
        store::save(data, &c).unwrap();
        // its process has it open (its own copy in memory)
        let mut live = store::load(data, &c.id).unwrap();
        let cash = live.cash;
        give(data, &c.id, &Grant::level(3), true).unwrap();
        give(data, &c.id, &Grant::research(keys::ARTICULATED, 1), true).unwrap();
        give(data, &c.id, &Grant::kroner(5_000.0, "Gave"), true).unwrap();
        give(data, &c.id, &Grant::area(Area::Parking, 4), true).unwrap();
        // (the file not touched: the process saves over it)
        assert_eq!(levels::level(&store::load(data, &c.id).unwrap()), levels::level(&c));
        assert_eq!(store::load(data, &c.id).unwrap().site.parking, 1);
        assert_eq!(waiting(data, &live), 4);
        // the process's step takes them, and saves
        assert!(super::super::shared_tenders::take_contract_orders(data, &mut live));
        store::save(data, &live).unwrap();
        let back = store::load(data, &c.id).unwrap();
        assert_eq!(levels::level(&back), 3);
        assert!(back.research.has(keys::ARTICULATED));
        assert_eq!(back.cash - cash, Currency::Dkk.cents(5_000.0));
        assert_eq!(back.site.parking, 4);
        assert_eq!(waiting(data, &back), 0);
        // (its next step: nothing again)
        assert!(!super::super::shared_tenders::take_contract_orders(data, &mut live));
        assert_eq!(live.cash - cash, Currency::Dkk.cents(5_000.0));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// When no process runs the company, the grant is in its file at once - and a process started
    /// after does not take it again.
    #[test]
    fn a_grant_made_while_the_company_does_not_run_is_in_its_file() {
        let dir = std::env::temp_dir().join(format!("openomsi-grants-off-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let data = dir.as_path();
        let c = with_contracts();
        store::save(data, &c).unwrap();
        give(data, &c.id, &Grant::kroner(-1_000.0, "Bøde"), false).unwrap();
        give(data, &c.id, &Grant::level(4), false).unwrap();
        give(data, &c.id, &Grant::contract(2, Prices { year: Some(Currency::Dkk.cents(3_000_000.0)), ..Default::default() }, "Rettet"), false).unwrap();
        let back = store::load(data, &c.id).unwrap();
        let y = Prices::of(&back.concessions.contracts[1], back.contract_index).year.unwrap();
        assert!((Currency::Dkk.units(y as f64) - 3_000_000.0).abs() < 1.0);
        assert_eq!(back.cash - c.cash, Currency::Dkk.cents(-1_000.0));
        assert_eq!(levels::level(&back), 4);
        assert_eq!(waiting(data, &back), 0);
        let mut started = back.clone();
        assert!(!super::super::shared_tenders::take_contract_orders(data, &mut started));
        assert_eq!(started.cash, back.cash);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
