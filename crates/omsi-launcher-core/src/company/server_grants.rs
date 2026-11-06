//! (NexiconDK, 6 November 2026) What the main server's admin gives a company of Bus Company Online
//! from the web admin's "Companies": its level, research done (or taken back), money given or
//! taken with a reason.
//!
//! A company of the server lives in its own process (`company_host`), which saves it over its file
//! now and then: a change written into the file while it runs would be lost at its next save. So
//! a grant goes the way the admin's other orders to a company go (`shared_tenders::
//! take_contract_orders`, `<data>/admin-orders/<id>.jsonl`, read from `Online::admin_read`): its
//! process takes it at its next step and saves it. When no process runs the company, the main
//! server takes it into the company's file at once (`take_now`, under the main server's lock - a
//! process started after reads the file with it, and `admin_read` past it: never taken twice).

use super::currency::Currency;
use super::levels;
use super::model::{BookingKind, Company};
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
        if self.level.is_none() && self.research.is_none() && self.money.is_none() {
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
        out.join(", ")
    }
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
        // (the file not touched: the process saves over it)
        assert_eq!(levels::level(&store::load(data, &c.id).unwrap()), levels::level(&c));
        assert_eq!(waiting(data, &live), 3);
        // the process's step takes them, and saves
        assert!(super::super::shared_tenders::take_contract_orders(data, &mut live));
        store::save(data, &live).unwrap();
        let back = store::load(data, &c.id).unwrap();
        assert_eq!(levels::level(&back), 3);
        assert!(back.research.has(keys::ARTICULATED));
        assert_eq!(back.cash - cash, Currency::Dkk.cents(5_000.0));
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
        let c = company();
        store::save(data, &c).unwrap();
        give(data, &c.id, &Grant::kroner(-1_000.0, "Bøde"), false).unwrap();
        give(data, &c.id, &Grant::level(4), false).unwrap();
        let back = store::load(data, &c.id).unwrap();
        assert_eq!(back.cash - c.cash, Currency::Dkk.cents(-1_000.0));
        assert_eq!(levels::level(&back), 4);
        assert_eq!(waiting(data, &back), 0);
        let mut started = back.clone();
        assert!(!super::super::shared_tenders::take_contract_orders(data, &mut started));
        assert_eq!(started.cash, back.cash);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
