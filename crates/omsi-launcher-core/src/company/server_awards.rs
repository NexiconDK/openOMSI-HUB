//! (NexiconDK, Bus Company Online, decided 3 November 2026) Two of the plan's contract events
//! (`contract_events` has the others), both in the map's market (`shared_tenders`):
//!
//! * **H9 rail replacement**, only where a map has it: a map's timetable for rail replacement
//!   ("SEV", Schienenersatzverkehr: `specials::Kind::RailReplacement`, never a concession of its
//!   own) the admin orders for days of his (`order_rail`). It goes out as a short tender - bids for
//!   `RAIL_BID_DAYS`, at the admin's fixed price a timetable hour, the quality deciding
//!   (`viborg::Award::Fixed`) - or to a company he names directly, and the winner runs it on those
//!   days as an event's special service. A map without such a timetable has no rail replacement to
//!   order (`Market::rail_lines` empty: the admin panel shows none).
//! * **H11 a complaint about an award**: a company that bid and lost may complain in the days
//!   between the award and the start (`complain`, with its reason). The admin sees the complaint
//!   with the opened bids and their points, and lets the award stand or gives the contract to the
//!   next valid bidder (`rule`): the market's decision then names the new winner (`Decided::
//!   revision`), and every company of the map takes it at its next step (`take_revision`) - the
//!   old winner gives the contract back (before its start: as if never won; after it: it ends
//!   today), the new one wins it, its start kept when it is still ahead.

use super::clock::{self, Level, Run};
use super::concessions::{self as cn, Outcome, Tender};
use super::dates;
use super::model::{Cents, Company};
use super::shared_tenders::{self as st, Decided, Market};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The days a rail replacement takes bids.
pub const RAIL_BID_DAYS: i64 = 1;
/// The most days a rail replacement the admin orders runs.
pub const RAIL_MOST_DAYS: i64 = 31;
/// The event's name a rail replacement runs under (its concession's days alone).
pub const RAIL_EVENT: &str = "Rail replacement";

/// A rail replacement the admin ordered: the map's timetable, its days, the fixed price a
/// timetable hour (euro cents), and - given directly - the company (its id and name). `offered`:
/// put out (or given) already.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct RailOrder {
    pub line: String,
    pub from: String,
    pub until: String,
    pub hour: Cents,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub direct: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub direct_name: String,
    pub offered: bool,
}

/// Why the admin's rail replacement is refused.
pub const NO_RAIL: &str = "this map has no timetable for rail replacement";

/// The admin orders a rail replacement of the map's timetable `line` from `from` to `until` at
/// `hour` a timetable hour (euro cents), directly to a company or by a short tender
/// (`shared_tenders::offer` puts it out at the main server's next step). It starts no sooner
/// than the day after tomorrow (a day of bids, a day to make ready).
pub fn order_rail(m: &mut Market, line: &str, from: &str, until: &str, hour: Cents, direct: Option<(String, String)>, today: &str) -> Result<String, String> {
    if m.rail_lines.is_empty() {
        return Err(NO_RAIL.into());
    }
    let Some(line) = m.rail_lines.iter().find(|l| l.eq_ignore_ascii_case(line.trim())).cloned() else {
        return Err(format!("the map's timetables for rail replacement: {}", m.rail_lines.join(", ")));
    };
    let (Some(_), Some(_)) = (dates::parse(from), dates::parse(until)) else { return Err("the days as YYYY-MM-DD".into()) };
    if dates::between(today, from) < 2 {
        return Err(format!("it starts {} at the soonest: a day for the bids, a day to make ready", dates::add(today, 2)));
    }
    let days = dates::between(from, until) + 1;
    if !(1..=RAIL_MOST_DAYS).contains(&days) {
        return Err(format!("from 1 to {RAIL_MOST_DAYS} days"));
    }
    if hour <= 0 {
        return Err("the fixed price a timetable hour, in kroner".into());
    }
    let overlaps = |r: &RailOrder| r.line.eq_ignore_ascii_case(&line) && dates::between(&r.from, until) >= 0 && dates::between(from, &r.until) >= 0;
    if m.rails.iter().any(overlaps) {
        return Err(format!("line {line} has a rail replacement on some of those days already"));
    }
    let (direct, direct_name) = direct.unwrap_or_default();
    let said = if direct.is_empty() { format!("rail replacement {line} of {} from {from} to {until}: out to tender for a day at its next step, the quality deciding", m.map) } else { format!("rail replacement {line} of {} from {from} to {until}: given to {direct_name} at its next step", m.map) };
    m.rails.push(RailOrder { line, from: from.to_string(), until: until.to_string(), hour, direct, direct_name, offered: false });
    Ok(said)
}

// --- H11 complaints -------------------------------------------------------------------------------

/// A complaint about an award as the main server keeps it with its tender.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Complaint {
    pub company: String,
    pub name: String,
    pub reason: String,
    /// When it came (the server's minutes).
    pub at: i64,
    pub state: ComplaintState,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComplaintState {
    #[default]
    Open,
    /// The admin let the award stand.
    Stood,
    /// The admin gave the contract to the next valid bidder.
    Upheld,
}

/// A company's complaint on its way to the main server (a line of `complaints_file`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ComplaintAsk {
    /// The market's id of the tender.
    pub tender: u32,
    pub company: String,
    pub name: String,
    pub reason: String,
    pub at: i64,
}

/// The longest reason kept.
pub const REASON_MOST: usize = 600;

/// Why a complaint is refused.
pub const NOT_LOST: &str = "Only a company that bid and lost may complain about the award.";
pub const TOO_LATE: &str = "The contract has started: complaints are taken until the day before its start.";
pub const COMPLAINED: &str = "Your complaint about this award is with the authority already.";
pub const NO_REASON: &str = "Say why you complain.";
/// What the companies are told.
pub const COMPLAINT_SENT: &str = "Your complaint about the award of %{n} is with the authority: it decides before the start.";
pub const STOOD: &str = "The authority looked at your complaint about the award of %{n}: the award stands.";
pub const UPHELD_LOST: &str = "Upon a complaint the authority gives %{n} to %{who} instead: your contract on it is withdrawn.";
pub const UPHELD_WON: &str = "Upon a complaint the authority gives %{n} to you after all, for %{amount}: the concession runs until %{date}.";
pub const UPHELD_OTHER: &str = "Upon a complaint the authority gives %{n} to %{who} instead.";

pub fn complaints_file(data: &Path, key: &str) -> PathBuf {
    st::dir(data).join(format!("{key}.complaints.jsonl"))
}

/// The key a company keeps for what it was told of a complaint (its day second, as the
/// concessions' told events keep theirs: `shared_tenders::sync` lets them go after `KEEP_DAYS`).
fn told_key(date: &str, key: &str, id: u32, what: &str) -> String {
    format!("complaint|{date}|{key}|{id}|{what}")
}

/// Whether company `c` was told that of a complaint.
fn told(c: &Company, key: &str, id: u32, what: &str) -> bool {
    let tail = format!("|{key}|{id}|{what}");
    c.concessions.events_told.iter().any(|e| e.starts_with("complaint|") && e.ends_with(&tail))
}

/// The first day of tender `t`'s contract.
pub fn start_of(t: &Tender) -> String {
    cn::server_start(t)
}

/// Whether company `c` may complain about the award of its tender `t` today: one of the market's,
/// decided, lost by it with a bid of its own, before the contract's start, not complained of yet.
pub fn may_complain(c: &Company, t: &Tender) -> Result<(), &'static str> {
    let Some(x) = t.market.as_ref() else { return Err(NOT_LOST) };
    if t.open() || !matches!(t.outcome, Some(Outcome::Lost { .. })) || t.offers.is_empty() {
        return Err(NOT_LOST);
    }
    if dates::between(&c.date, &start_of(t)) < 1 {
        return Err(TOO_LATE);
    }
    if told(c, &x.key, x.id, "sent") {
        return Err(COMPLAINED);
    }
    Ok(())
}

/// Company `c` complains about the award of its tender `id` (`reason`): into the market's
/// complaints for the main server (the admin decides).
pub fn complain(data: &Path, c: &mut Company, id: u32, reason: &str) -> Result<(), &'static str> {
    let Some(t) = c.concessions.tenders.iter().find(|t| t.id == id).cloned() else { return Err(NOT_LOST) };
    may_complain(c, &t)?;
    let reason: String = reason.trim().chars().take(REASON_MOST).collect();
    if reason.is_empty() {
        return Err(NO_REASON);
    }
    let x = t.market.as_ref().expect("a market's");
    let ask = ComplaintAsk { tender: x.id, company: c.id.clone(), name: c.name.clone(), reason, at: clock::now(c) };
    st::append(&complaints_file(data, &x.key), &serde_json::to_string(&ask).map_err(|_| "The complaint could not be written.")?).map_err(|_| "The complaint could not be written.")?;
    let date = c.date.clone();
    c.concessions.events_told.push(told_key(&date, &x.key, x.id, "sent"));
    let mut run = Run::default();
    let at = clock::now(c);
    clock::tell(c, &mut run, at, COMPLAINT_SENT, vec![clock::arg("n", &t.number)], Level::Info);
    Ok(())
}

/// The main server reads the companies' complaints come since: each with its tender, once a
/// company (a tender not decided, or gone: none).
pub fn read_complaints(data: &Path, m: &mut Market) {
    let (asks, to): (Vec<ComplaintAsk>, u64) = st::lines_from(&complaints_file(data, &m.key), m.complaints_read);
    if to == m.complaints_read {
        return;
    }
    m.complaints_read = to;
    for a in asks {
        let Some(mt) = m.tenders.iter_mut().find(|t| t.decided.is_some() && t.tender.market.as_ref().is_some_and(|x| x.id == a.tender)) else { continue };
        if mt.complaints.iter().any(|x| x.company == a.company) {
            continue;
        }
        mt.complaints.push(Complaint { company: a.company, name: a.name, reason: a.reason, at: a.at, state: ComplaintState::Open });
    }
}

/// The next valid bidder of a decided tender: the best of the opened bids after the winner (and
/// those passed over before) - its name; None: no other valid bid.
pub fn next_of(d: &Decided) -> Option<String> {
    d.standings.iter().filter(|s| !s.rejected).map(|s| s.name.clone()).find(|n| *n != d.name && !d.passed.contains(n))
}

/// The admin decides company `company`'s complaint about tender `id` of market `m`: the award
/// stands, or (`uphold`) the contract goes to the next valid bidder - the bids of the market's
/// file opened again without the winner (`shared_tenders::decide_without`); an operator's hold
/// moves with it.
pub fn rule(data: &Path, m: &mut Market, id: u32, company: &str, uphold: bool) -> Result<String, String> {
    let k = m.tenders.iter().position(|t| t.tender.market.as_ref().is_some_and(|x| x.id == id)).ok_or("no such tender")?;
    let mt = &m.tenders[k];
    let Some(old) = mt.decided.clone() else { return Err("that tender is not decided".into()) };
    let c = mt.complaints.iter().position(|x| x.company == company && x.state == ComplaintState::Open).ok_or("no open complaint of that company")?;
    let number = mt.tender.number.clone();
    if !uphold {
        m.tenders[k].complaints[c].state = ComplaintState::Stood;
        return Ok(format!("the complaint of {} about {number}: the award to {} stands", m.tenders[k].complaints[c].name, old.name));
    }
    let mut passed = old.passed.clone();
    passed.push(old.name.clone());
    let all = st::bids(data, &m.key);
    let mut new = st::decide_without(&mt.tender, &all, &passed);
    if new.name.is_empty() {
        return Err(format!("no other valid bid for {number}: let the award to {} stand, or cancel and put it out again", old.name));
    }
    new.revision = old.revision + 1;
    new.passed = passed;
    let t = mt.tender.clone();
    // (the operator that held it gives it up; the one that wins now holds it)
    if old.winner.is_empty() {
        m.held.retain(|h| !(t.has_line(&h.line) && h.name == old.name));
    }
    if new.winner.is_empty() {
        let key = m.key.clone();
        st::hold_for(m, &t, &new, &key);
    }
    let said = format!("the complaint of {} about {number} upheld: it goes to {} instead of {}", m.tenders[k].complaints[c].name, new.name, old.name);
    m.tenders[k].decided = Some(new);
    m.tenders[k].complaints[c].state = ComplaintState::Upheld;
    Ok(said)
}

/// The lines of tender `t`: a package's every line, else its own.
fn lines_of(t: &Tender) -> Vec<String> {
    match t.package.as_ref().filter(|p| p.lines.len() > 1) {
        Some(p) => p.lines.iter().map(|l| l.line.clone()).collect(),
        None => vec![t.line.clone()],
    }
}

/// Company `c` gives back what it won in tender `t` (a complaint upheld): before the term's start
/// its contracts of it go and its concessions with them (a renewal's back to the term before);
/// started, they end today.
pub fn withdraw(c: &mut Company, t: &Tender) {
    let today = c.date.clone();
    for line in lines_of(t) {
        let same = |l: &str| l.eq_ignore_ascii_case(&line);
        let Some(from) = c.concessions.contracts.iter().filter(|k| same(&k.line)).map(|k| k.from.clone()).max() else { continue };
        if dates::between(&today, &from) > 0 {
            c.concessions.contracts.retain(|k| !(same(&k.line) && k.from == from));
            match c.concessions.contracts.iter().filter(|k| same(&k.line)).map(|k| k.until.clone()).max() {
                Some(until) => {
                    if let Some(h) = c.concessions.held.iter_mut().find(|h| same(&h.line)) {
                        h.until = until;
                    }
                }
                None => c.concessions.held.retain(|h| !same(&h.line)),
            }
        } else {
            for k in c.concessions.contracts.iter_mut().filter(|k| same(&k.line) && k.from == from) {
                if dates::between(&today, &k.until) > 0 {
                    k.until = today.clone();
                }
            }
            if let Some(h) = c.concessions.held.iter_mut().find(|h| same(&h.line)) {
                h.until = today.clone();
            }
        }
    }
}

/// Company `c` takes a changed award of its tender `lid` (`d`: the admin upheld a complaint): the
/// old winner gives it back (`withdraw`), the new one wins it (its contract from the tender's
/// start, or tomorrow when that is past), every other only hears of it.
pub fn take_revision(c: &mut Company, lid: u32, d: &Decided, run: &mut Run, at: i64) {
    let Some(t) = c.concessions.tenders.iter().find(|t| t.id == lid).cloned() else { return };
    let won = matches!(t.outcome, Some(Outcome::Won { .. }));
    let me = d.winner == c.id;
    if won && !me {
        withdraw(c, &t);
        if let Some(x) = c.concessions.tenders.iter_mut().find(|x| x.id == lid) {
            x.outcome = Some(Outcome::Lost { score: 0.0, winner: d.name.clone(), rivals: Vec::new(), amount: d.amount });
        }
        clock::tell(c, run, at, UPHELD_LOST, vec![clock::arg("n", &t.number), clock::arg("who", &d.name)], Level::Bad);
    } else if me && !won {
        if let Some(x) = c.concessions.tenders.iter_mut().find(|x| x.id == lid) {
            x.outcome = None;
            x.bus_day = d.bus_day;
        }
        if let Some((cn::Event::Won { until, .. }, amount)) = cn::decide_shared(c, lid, true, &d.name, d.amount, d.standings.clone(), d.drive) {
            clock::tell(c, run, at, UPHELD_WON, vec![clock::arg("n", &t.number), clock::arg("amount", amount), clock::arg("date", until)], Level::Good);
        }
    } else if !t.offers.is_empty() {
        clock::tell(c, run, at, UPHELD_OTHER, vec![clock::arg("n", &t.number), clock::arg("who", &d.name)], Level::Info);
    }
    if let Some(x) = c.concessions.tenders.iter_mut().find(|x| x.id == lid) {
        let name = c.name.clone();
        x.standings = d.standings.iter().cloned().map(|s| if s.name == name { cn::Standing { name: String::new(), ..s } } else { s }).collect();
        if let Some(m) = x.market.as_mut() {
            m.revision = d.revision;
        }
    }
}

/// Company `c` is told once what came of its complaints in market `m` (one upheld is told by
/// `take_revision`).
pub fn tell_complaints(c: &mut Company, m: &Market, run: &mut Run, at: i64) {
    for mt in &m.tenders {
        let Some(x) = mt.tender.market.as_ref() else { continue };
        if mt.complaints.iter().any(|k| k.company == c.id && k.state == ComplaintState::Stood) && !told(c, &m.key, x.id, "stood") {
            let date = c.date.clone();
            c.concessions.events_told.push(told_key(&date, &m.key, x.id, "stood"));
            clock::tell(c, run, at, STOOD, vec![clock::arg("n", &mt.tender.number)], Level::Info);
        }
    }
}

#[cfg(test)]
#[path = "server_awards_tests.rs"]
mod tests;
