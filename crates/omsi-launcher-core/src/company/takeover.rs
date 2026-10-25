//! (NexiconDK) Bus Company Online: the buses of a line that changes company (decided 25 October
//! 2026, docs/omsihub/BUS_COMPANY_ONLINE.md). When a company's concession goes to another company
//! of the map in its renewal (`shared_tenders`), the company that lost it offers the winner the
//! buses of its own it ran the line with - the line's tours in its roster - at their book value
//! (`market::value_of`); the winner says yes to all of them or no. On the day the new term begins
//! they leave the loser's fleet (sold: what it books for a bus sold) and join the winner's, who
//! pays their value then.
//!
//! As the other ways between the companies' processes (`server_depots`), each step goes through
//! a file of JSON lines of the server's folder, read from where its reader got to
//! (`Takeover::offers_read`...): the offers to the winner (`<folder>/takeover/<id>.offers.jsonl`),
//! its answers to the loser (`<id>.answers.jsonl`), the buses delivered to it (`<id>.buses.jsonl`).

use super::inbox::{self, Kind, Message};
use super::market;
use super::model::{BookingKind, Cents, Company, Tenure, Vehicle};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What a company keeps of the buses of lines changing company (`online::Online::takeover`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Takeover {
    /// How far its files were read (bytes): the offers to it, the answers to its offers, the
    /// buses delivered to it.
    pub offers_read: u64,
    pub answers_read: u64,
    pub buses_read: u64,
    /// The offers it was made (`answer` None: not answered yet).
    pub offers: Vec<Offer>,
    /// The sales agreed, delivered on their day.
    pub sales: Vec<Sale>,
}

/// The buses a company ran a line with, offered to the company that won the line from it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Offer {
    /// The seller, the line and the new term's first day.
    pub id: String,
    pub from: String,
    pub name: String,
    pub line: String,
    pub number: String,
    pub date: String,
    pub buses: Vec<OfferBus>,
    pub answer: Option<bool>,
}

impl Offer {
    /// What its buses are worth together.
    pub fn total(&self) -> Cents {
        self.buses.iter().map(|b| b.value).sum()
    }
}

/// A bus of an offer: its id in the seller's fleet, its number and model, its book value then.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct OfferBus {
    pub id: u32,
    pub label: String,
    pub value: Cents,
}

/// A sale agreed: the offer, the buyer, the day, the buses.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Sale {
    pub id: String,
    pub to: String,
    pub name: String,
    pub date: String,
    pub buses: Vec<u32>,
}

/// The buyer's answer to an offer.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
struct Answer {
    id: String,
    from: String,
    name: String,
    yes: bool,
}

/// A bus delivered to the buyer, at its price.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Delivery {
    id: String,
    from: String,
    name: String,
    vehicle: Vehicle,
    price: Cents,
}

/// Why an offer cannot be taken.
pub const NO_OFFER: &str = "There is no such offer of buses.";
pub const NO_CASH: &str = "The company has not the money for these buses.";
pub const NOT_WRITTEN: &str = "The answer could not be written: the server says why in its log.";

pub fn texts() -> [&'static str; 3] {
    [NO_OFFER, NO_CASH, NOT_WRITTEN]
}

/// The messages it posts.
pub const OFFERED: (&str, &str) = ("Buses offered with line %{n}", "%{name} ran line %{n} with %{count} buses of its own. You may take them over at their book value, %{amount} together, on %{date}, the day your term begins: the Concessions page.");
pub const TAKEN: (&str, &str) = ("%{name} takes over your buses of line %{n}", "%{name} takes over the %{count} buses of yours that ran line %{n}, at their book value, on %{date}.");
pub const REFUSED: (&str, &str) = ("%{name} does not take over your buses of line %{n}", "%{name} won line %{n} and does not take over the buses of yours that ran it: they stay yours.");

fn dir(data: &Path) -> PathBuf {
    data.join("takeover")
}

fn offers_file(data: &Path, id: &str) -> PathBuf {
    dir(data).join(format!("{id}.offers.jsonl"))
}

fn answers_file(data: &Path, id: &str) -> PathBuf {
    dir(data).join(format!("{id}.answers.jsonl"))
}

fn buses_file(data: &Path, id: &str) -> PathBuf {
    dir(data).join(format!("{id}.buses.jsonl"))
}

/// A JSON line added to a file of the folder (made when missing).
fn append(path: &Path, value: &impl Serialize) -> std::io::Result<()> {
    use std::io::Write;
    let line = serde_json::to_string(value).map_err(std::io::Error::other)?;
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(format!("{line}\n").as_bytes())
}

/// The whole lines of a file from byte `from` on, each with the byte it ends at.
fn lines_after(path: &Path, from: u64) -> Vec<(String, u64)> {
    let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
    let mut at = if (from as usize) > bytes.len() { 0 } else { from as usize };
    let mut out = Vec::new();
    while let Some(k) = bytes[at..].iter().position(|b| *b == b'\n') {
        let end = at + k + 1;
        out.push((String::from_utf8_lossy(&bytes[at..end - 1]).to_string(), end as u64));
        at = end;
    }
    out
}

/// The buses of its own company `c` runs line `line` with (its roster's tours of the line).
pub fn buses_of_line(c: &Company, line: &str) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    for id in c.planning.week.iter().filter(|t| t.line.eq_ignore_ascii_case(line)).filter_map(|t| t.bus) {
        if !out.contains(&id) && c.vehicle(id).is_some_and(|v| matches!(v.tenure, Tenure::Owned { .. })) {
            out.push(id);
        }
    }
    out
}

/// Company `c` lost its line `line` (`number`) to company `winner` from `date`: the buses of its
/// own it ran the line with offered to it at their book value. How many.
pub fn offer(data: &Path, c: &Company, winner: &str, line: &str, number: &str, date: &str) -> usize {
    let buses: Vec<OfferBus> = buses_of_line(c, line)
        .into_iter()
        .filter_map(|id| c.vehicle(id))
        .map(|v| OfferBus { id: v.id, label: format!("{} {}", v.number, v.name).trim().to_string(), value: market::value_of(c, v) })
        .collect();
    if buses.is_empty() || winner == c.id {
        return 0;
    }
    let o = Offer { id: format!("{}/{}/{date}", c.id, line.to_lowercase()), from: c.id.clone(), name: c.name.clone(), line: line.to_string(), number: number.to_string(), date: date.to_string(), buses, answer: None };
    match append(&offers_file(data, winner), &o) {
        Ok(()) => o.buses.len(),
        Err(_) => 0,
    }
}

/// The buyer `c` answers offer `id`: yes, all its buses on its day (the money needed then is
/// there now), or no.
pub fn answer(data: &Path, c: &mut Company, id: &str, yes: bool) -> Result<(), &'static str> {
    let o = c.online.takeover.offers.iter().find(|o| o.id == id && o.answer.is_none()).cloned().ok_or(NO_OFFER)?;
    if yes && c.cash < o.total() {
        return Err(NO_CASH);
    }
    append(&answers_file(data, &o.from), &Answer { id: o.id.clone(), from: c.id.clone(), name: c.name.clone(), yes }).map_err(|_| NOT_WRITTEN)?;
    if let Some(x) = c.online.takeover.offers.iter_mut().find(|x| x.id == id) {
        x.answer = Some(yes);
    }
    Ok(())
}

/// Company `c`'s part of the buses changing company, at its date (each step of its process): the
/// offers made to it (told), the answers to its own offers (a yes kept as a sale), its sales
/// whose day has come delivered - each bus it still owns leaves its fleet, sold at its book value
/// then -, and the buses delivered to it on an offer it took, paid. Whether anything changed.
pub fn step(data: &Path, c: &mut Company) -> bool {
    if !c.online.on_server {
        return false;
    }
    let mut changed = false;
    // (the offers made to it)
    for (line, end) in lines_after(&offers_file(data, &c.id), c.online.takeover.offers_read) {
        c.online.takeover.offers_read = end;
        changed = true;
        let Ok(o) = serde_json::from_str::<Offer>(&line) else { continue };
        if c.online.takeover.offers.iter().any(|x| x.id == o.id) {
            continue;
        }
        let m = Message::new(Kind::Fleet, OFFERED.0, OFFERED.1).arg("name", o.name.clone()).arg("n", o.number.clone()).arg("count", o.buses.len().to_string()).arg("amount", o.total().to_string()).arg("date", o.date.clone());
        inbox::post(c, m);
        c.online.takeover.offers.push(Offer { answer: None, ..o });
    }
    // (the answers to its offers)
    for (line, end) in lines_after(&answers_file(data, &c.id), c.online.takeover.answers_read) {
        c.online.takeover.answers_read = end;
        changed = true;
        let Ok(a) = serde_json::from_str::<Answer>(&line) else { continue };
        let Some((_, number, date, buses)) = offered(c, &a.id) else { continue };
        let text = if a.yes { TAKEN } else { REFUSED };
        let m = Message::new(Kind::Fleet, text.0, text.1).arg("name", a.name.clone()).arg("n", number).arg("count", buses.len().to_string()).arg("date", date.clone());
        inbox::post(c, m);
        if a.yes && !c.online.takeover.sales.iter().any(|s| s.id == a.id) {
            c.online.takeover.sales.push(Sale { id: a.id.clone(), to: a.from.clone(), name: a.name.clone(), date, buses });
        }
    }
    // (its sales whose day has come: the buses it still owns leave, sold)
    let due: Vec<Sale> = c.online.takeover.sales.iter().filter(|s| super::dates::between(&s.date, &c.date) >= 0).cloned().collect();
    for s in due {
        let today = c.date.clone();
        for id in &s.buses {
            let Some(v) = c.vehicle(*id).filter(|v| matches!(v.tenure, Tenure::Owned { .. })).cloned() else { continue };
            let price = market::value_of(c, &v);
            let d = Delivery { id: s.id.clone(), from: c.id.clone(), name: c.name.clone(), vehicle: v.clone(), price };
            if append(&buses_file(data, &s.to), &d).is_err() {
                continue;
            }
            market::book_part_month(c, &v, &today);
            c.book(BookingKind::Sale, price, format!("{} {}: taken over by {}", v.number, v.name, s.name), false);
            c.fleet.retain(|x| x.id != *id);
            c.planning.week.iter_mut().filter(|t| t.bus == Some(*id)).for_each(|t| t.bus = None);
        }
        c.online.takeover.sales.retain(|x| x.id != s.id);
        changed = true;
    }
    // (the buses delivered to it, on an offer it took: paid now)
    for (line, end) in lines_after(&buses_file(data, &c.id), c.online.takeover.buses_read) {
        c.online.takeover.buses_read = end;
        changed = true;
        let Ok(d) = serde_json::from_str::<Delivery>(&line) else { continue };
        if !c.online.takeover.offers.iter().any(|o| o.id == d.id && o.answer == Some(true)) {
            continue;
        }
        let mut v = d.vehicle;
        c.counters.vehicle += 1;
        v.id = c.counters.vehicle;
        if c.fleet.iter().any(|x| x.number == v.number) || v.number.trim().is_empty() {
            v.number = c.next_fleet_number();
        }
        v.acquired = c.date.clone();
        v.workshop_until = None;
        c.book(BookingKind::Purchase, -d.price, format!("{} {}: taken over from {}", v.number, v.name, d.name), false);
        c.fleet.push(v);
    }
    changed
}

/// What company `c` offered with offer `id` (its own): the line, its number, the day and the
/// buses.
fn offered(c: &Company, id: &str) -> Option<(String, String, String, Vec<u32>)> {
    // (the id is "<seller>/<line>/<date>")
    let mut parts = id.splitn(3, '/');
    let (seller, line, date) = (parts.next()?, parts.next()?, parts.next()?);
    if seller != c.id {
        return None;
    }
    let number = c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(line)).map(|l| l.number.clone()).unwrap_or_else(|| line.to_string());
    let name = c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(line)).map(|l| l.name.clone()).unwrap_or_else(|| line.to_string());
    Some((name, number, date.to_string(), buses_of_line(c, line)))
}

#[cfg(test)]
mod tests;
