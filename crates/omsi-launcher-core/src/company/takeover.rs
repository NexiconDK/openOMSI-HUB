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
//!
//! (NexiconDK, 16 November 2026: "Det skal også gælde chaufførerne", and "Chaufførerne SKAL følge
//! med - uanset hvad - busserne er valgfrie") The drivers who drove the line most of their days
//! lately (`crew`, kept at each day's close: `note_crew`) go with it to the winner whatever it
//! answers - as a transfer of an undertaking: on the new term's first day each leaves the loser,
//! his week's guarantee paid, and joins the winner on his terms (wage, employment, seniority):
//! `<id>.drivers.jsonl`. The buses stay the winner's choice.

use super::inbox::{self, Kind, Message};
use super::market;
use super::model::{BookingKind, Cents, Company, Employee, Tenure, Vehicle};
use super::plan::{DayPlan, Who};
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
    /// (16 November 2026) How far the drivers delivered to it were read (bytes).
    pub drivers_read: u64,
    /// Who drove which line on its days of the last `CREW_DAYS` (`note_crew`).
    pub crew: Vec<Crew>,
}

/// (16 November 2026) A driver's days on a line lately: the dates he drove a duty of it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Crew {
    pub line: String,
    pub driver: u32,
    pub days: Vec<String>,
}

/// The days back the crew of a line is counted over.
pub const CREW_DAYS: i64 = 28;
/// A driver is of a line's crew with at least this many days on it (and none more on another).
pub const CREW_MIN_DAYS: usize = 3;

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
    /// (16 November 2026) The drivers who ran the line, who go with it (empty: an offer of before).
    pub drivers: Vec<OfferDriver>,
}

/// (16 November 2026) A driver of an offer: his id in the seller's staff, his name and wage.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct OfferDriver {
    pub id: u32,
    pub name: String,
    pub wage: Cents,
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
    /// (16 November 2026) The drivers taken.
    pub drivers: Vec<u32>,
}

/// The buyer's answer to an offer.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
struct Answer {
    id: String,
    from: String,
    name: String,
    /// The buses taken.
    yes: bool,
}

/// (16 November 2026) A driver handed to the winner of his line.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct Transfer {
    id: String,
    from: String,
    name: String,
    employee: Employee,
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
/// (16 November 2026) The drivers' messages.
pub const DRIVERS_OFFERED: (&str, &str) = ("Drivers come with line %{n}", "%{name} ran line %{n} with %{count} drivers. They come with the line, on their terms, on %{date}, the day your term begins: the Concessions page.");
pub const DRIVERS_TAKEN: (&str, &str) = ("Your drivers of line %{n} go to %{name}", "The %{count} drivers of yours that ran line %{n} go with it to %{name}, on their terms, on %{date}.");
pub const DRIVERS_CAME: (&str, &str) = ("%{count} drivers from %{name}", "%{count} drivers who drove line %{n} for %{name} work for you from today, on their terms: the Staff page.");

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

fn drivers_file(data: &Path, id: &str) -> PathBuf {
    dir(data).join(format!("{id}.drivers.jsonl"))
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

/// (16 November 2026) The day's drivers of each line, at the day's close (`day::close_day`): kept
/// for `CREW_DAYS` (a company of the server's only: the winner of a line may take them over).
pub fn note_crew(c: &mut Company, plan: &DayPlan) {
    if !c.online.on_server {
        return;
    }
    let today = c.date.clone();
    let mut driven: Vec<(String, u32)> = Vec::new();
    for t in &plan.tours {
        for d in &t.duties {
            if let Some(Who::Staff(id)) = d.who {
                if !driven.iter().any(|x| x.1 == id && x.0.eq_ignore_ascii_case(&t.tour.line)) {
                    driven.push((t.tour.line.clone(), id));
                }
            }
        }
    }
    let crew = &mut c.online.takeover.crew;
    for (line, id) in driven {
        match crew.iter_mut().find(|x| x.driver == id && x.line.eq_ignore_ascii_case(&line)) {
            Some(x) if !x.days.contains(&today) => x.days.push(today.clone()),
            Some(_) => {}
            None => crew.push(Crew { line, driver: id, days: vec![today.clone()] }),
        }
    }
    for x in crew.iter_mut() {
        x.days.retain(|d| (0..CREW_DAYS).contains(&super::dates::between(d, &today)));
    }
    crew.retain(|x| !x.days.is_empty());
}

/// (16 November 2026) The drivers company `c` runs line `line` with: those of its staff who drove
/// it on `CREW_MIN_DAYS` days or more of the last `CREW_DAYS`, and on no other line more - and those
/// its roster gives the line most of their duties (`planning::week`, a company planned so before
/// the days were counted). Not one who is leaving, nor one lent.
pub fn drivers_of_line(c: &Company, line: &str) -> Vec<u32> {
    let days = |id: u32, l: &str| c.online.takeover.crew.iter().find(|x| x.driver == id && x.line.eq_ignore_ascii_case(l)).map_or(0, |x| x.days.len());
    let duties = |id: u32, l: Option<&str>| c.planning.week.iter().filter(|t| l.is_none_or(|l| t.line.eq_ignore_ascii_case(l))).flat_map(|t| t.duties.iter()).filter(|w| **w == Some(Who::Staff(id))).count();
    let mut out: Vec<u32> = Vec::new();
    for e in c.staff.iter().filter(|e| super::staff::drives(e) && e.notice_until.is_none() && e.lent.is_none() && e.lend.is_none()) {
        let here = days(e.id, line);
        let most_elsewhere = c.online.takeover.crew.iter().filter(|x| x.driver == e.id && !x.line.eq_ignore_ascii_case(line)).map(|x| x.days.len()).max().unwrap_or(0);
        let by_days = here >= CREW_MIN_DAYS && here >= most_elsewhere;
        let on_line = duties(e.id, Some(line));
        let by_roster = on_line > 0 && on_line * 2 >= duties(e.id, None);
        if (by_days || by_roster) && !out.contains(&e.id) {
            out.push(e.id);
        }
    }
    out
}

/// (NexiconDK, 18 November 2026: "gøre så chaufførerne i hvert fald ryger videre, også til A.I
/// selskaber") Company `c` lost its line `line` (`number`) to a company of the game - an operator
/// or a rival, no player's (named `name`) - from `date`: the drivers who ran it go with the line
/// all the same, on that day (`leave_due`); its buses stay its own. Told. How many drivers.
pub fn to_ai(c: &mut Company, name: &str, line: &str, number: &str, date: &str) -> usize {
    let drivers = drivers_of_line(c, line);
    let id = format!("{}/{}/{date}", c.id, line.to_lowercase());
    if drivers.is_empty() || c.online.takeover.sales.iter().any(|s| s.id == id) {
        return 0;
    }
    let n = drivers.len();
    c.online.takeover.sales.push(Sale { id, to: String::new(), name: name.to_string(), date: date.to_string(), buses: Vec::new(), drivers });
    let m = Message::new(Kind::Staff, DRIVERS_TAKEN.0, DRIVERS_TAKEN.1).arg("name", name.to_string()).arg("n", number.to_string()).arg("count", n.to_string()).arg("date", date.to_string());
    inbox::post(c, m);
    n
}

/// The drivers going to a company of the game (`to_ai`) whose day has come: off the staff, their
/// week's guarantee paid - on the server or not. Whether any went.
pub fn leave_due(c: &mut Company) -> bool {
    let due: Vec<Sale> = c.online.takeover.sales.iter().filter(|s| s.to.is_empty() && super::dates::between(&s.date, &c.date) >= 0).cloned().collect();
    for s in &due {
        for id in &s.drivers {
            let _ = let_go(None, c, &s.id, &s.to, *id);
        }
        c.online.takeover.sales.retain(|x| x.id != s.id);
    }
    !due.is_empty()
}

/// Driver `id` of a sale (`sale`, to company `to`) leaves `c`: still here and not leaving
/// already, its week's guarantee paid, off the staff and the roster - to a player's company,
/// written to its file (`data`); to one of the game, gone. Whether it went.
fn let_go(data: Option<&Path>, c: &mut Company, sale: &str, to: &str, id: u32) -> bool {
    if !c.staff.iter().any(|e| e.id == id && e.notice_until.is_none() && e.lent.is_none() && e.lend.is_none()) {
        return false;
    }
    super::pay::settle(c, id);
    let Some(e) = c.staff.iter().find(|e| e.id == id).cloned() else { return false };
    if let (Some(data), false) = (data, to.is_empty()) {
        let t = Transfer { id: sale.to_string(), from: c.id.clone(), name: c.name.clone(), employee: e };
        if append(&drivers_file(data, to), &t).is_err() {
            return false;
        }
    }
    c.staff.retain(|x| x.id != id);
    c.staff_rules.asks.retain(|a| a.id != id);
    for t in c.planning.week.iter_mut() {
        for w in t.duties.iter_mut().filter(|w| **w == Some(Who::Staff(id))) {
            *w = None;
        }
    }
    c.online.takeover.crew.retain(|x| x.driver != id);
    true
}

/// Company `c` lost its line `line` (`number`) to company `winner` (named `winner_name`) from
/// `date`: the buses of its own it ran the line with offered to it at their book value, and (16
/// November 2026) the drivers who ran it going with the line - kept as a sale at once, told. How
/// many buses and drivers.
pub fn offer(data: &Path, c: &mut Company, winner: &str, winner_name: &str, line: &str, number: &str, date: &str) -> usize {
    let buses: Vec<OfferBus> = buses_of_line(c, line)
        .into_iter()
        .filter_map(|id| c.vehicle(id))
        .map(|v| OfferBus { id: v.id, label: format!("{} {}", v.number, v.name).trim().to_string(), value: market::value_of(c, v) })
        .collect();
    let drivers: Vec<OfferDriver> = drivers_of_line(c, line).into_iter().filter_map(|id| c.employee(id)).map(|e| OfferDriver { id: e.id, name: e.name.clone(), wage: e.wage }).collect();
    if (buses.is_empty() && drivers.is_empty()) || winner == c.id {
        return 0;
    }
    let o = Offer { id: format!("{}/{}/{date}", c.id, line.to_lowercase()), from: c.id.clone(), name: c.name.clone(), line: line.to_string(), number: number.to_string(), date: date.to_string(), buses, answer: None, drivers };
    if append(&offers_file(data, winner), &o).is_err() {
        return 0;
    }
    // (its drivers go with the line, whatever the winner answers)
    if !o.drivers.is_empty() && !c.online.takeover.sales.iter().any(|s| s.id == o.id) {
        c.online.takeover.sales.push(Sale { id: o.id.clone(), to: winner.to_string(), name: winner_name.to_string(), date: date.to_string(), buses: Vec::new(), drivers: o.drivers.iter().map(|d| d.id).collect() });
        let m = Message::new(Kind::Staff, DRIVERS_TAKEN.0, DRIVERS_TAKEN.1).arg("name", winner_name.to_string()).arg("n", number.to_string()).arg("count", o.drivers.len().to_string()).arg("date", date.to_string());
        inbox::post(c, m);
    }
    o.buses.len() + o.drivers.len()
}

/// The buyer `c` answers offer `id`: yes, all its buses on its day (the money needed then is
/// there now), or no. (Its drivers come in any case.)
pub fn answer(data: &Path, c: &mut Company, id: &str, yes: bool) -> Result<(), &'static str> {
    let o = c.online.takeover.offers.iter().find(|o| o.id == id && o.answer.is_none()).cloned().ok_or(NO_OFFER)?;
    let yes = yes && !o.buses.is_empty();
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
        if !o.buses.is_empty() {
            let m = Message::new(Kind::Fleet, OFFERED.0, OFFERED.1).arg("name", o.name.clone()).arg("n", o.number.clone()).arg("count", o.buses.len().to_string()).arg("amount", o.total().to_string()).arg("date", o.date.clone());
            inbox::post(c, m);
        }
        if !o.drivers.is_empty() {
            let m = Message::new(Kind::Staff, DRIVERS_OFFERED.0, DRIVERS_OFFERED.1).arg("name", o.name.clone()).arg("n", o.number.clone()).arg("count", o.drivers.len().to_string()).arg("date", o.date.clone());
            inbox::post(c, m);
        }
        c.online.takeover.offers.push(Offer { answer: None, ..o });
    }
    // (the answers to its offers)
    for (line, end) in lines_after(&answers_file(data, &c.id), c.online.takeover.answers_read) {
        c.online.takeover.answers_read = end;
        changed = true;
        let Ok(a) = serde_json::from_str::<Answer>(&line) else { continue };
        let Some((_, number, date, buses)) = offered(c, &a.id) else { continue };
        if buses.is_empty() {
            continue;
        }
        let text = if a.yes { TAKEN } else { REFUSED };
        let m = Message::new(Kind::Fleet, text.0, text.1).arg("name", a.name.clone()).arg("n", number).arg("count", buses.len().to_string()).arg("date", date.clone());
        inbox::post(c, m);
        if a.yes {
            // (with the drivers' sale of the same offer, or a sale of its own)
            match c.online.takeover.sales.iter_mut().find(|s| s.id == a.id) {
                Some(s) if s.buses.is_empty() => s.buses = buses,
                Some(_) => {}
                None => c.online.takeover.sales.push(Sale { id: a.id.clone(), to: a.from.clone(), name: a.name.clone(), date, buses, drivers: Vec::new() }),
            }
        }
    }
    // (its sales whose day has come: the buses it still owns leave, sold)
    // (a company of the game's: `leave_due`)
    let due: Vec<Sale> = c.online.takeover.sales.iter().filter(|s| !s.to.is_empty() && super::dates::between(&s.date, &c.date) >= 0).cloned().collect();
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
        // (16 November 2026: the drivers taken, still here and not leaving: their week's guarantee
        // paid, off the staff and the roster)
        for id in &s.drivers {
            let _ = let_go(Some(data), c, &s.id, &s.to, *id);
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
    // (16 November 2026: the drivers delivered to it, on an offer it took: its staff from today,
    // on their terms - a new number here, their seniority kept)
    let mut came: Vec<(String, String, usize)> = Vec::new();
    for (line, end) in lines_after(&drivers_file(data, &c.id), c.online.takeover.drivers_read) {
        c.online.takeover.drivers_read = end;
        changed = true;
        let Ok(t) = serde_json::from_str::<Transfer>(&line) else { continue };
        let Some(o) = c.online.takeover.offers.iter().find(|o| o.id == t.id).cloned() else { continue };
        let mut e = t.employee;
        c.counters.employee += 1;
        e.id = c.counters.employee;
        (e.week_paid, e.week_due, e.last_end, e.week_days) = (0.0, 0.0, None, 0);
        (e.notice_until, e.resigned, e.lent, e.lend, e.agency_no) = (None, false, None, None, 0);
        e.free_days.clear();
        e.sick_planned.clear();
        c.staff.push(e);
        match came.iter_mut().find(|x| x.0 == o.name && x.1 == o.number) {
            Some(x) => x.2 += 1,
            None => came.push((o.name.clone(), o.number.clone(), 1)),
        }
    }
    for (name, number, count) in came {
        inbox::post(c, Message::new(Kind::Staff, DRIVERS_CAME.0, DRIVERS_CAME.1).arg("name", name).arg("n", number).arg("count", count.to_string()));
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
