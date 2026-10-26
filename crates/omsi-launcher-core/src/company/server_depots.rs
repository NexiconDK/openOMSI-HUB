//! (NexiconDK) Bus Company Online, phase 4 (docs/omsihub/BUS_COMPANY_ONLINE.md): a company's
//! depots on other maps of the server.
//!
//! A company of the server runs on one map for good; to run on another it founds a depot there
//! (`found`, its owner from the head office's settings): a company of its own on that map - its
//! own lines, buses, staff, tenders and books, its own process on the server as every company -
//! tied to the head office (`Online::hq`), with the head office's name, mark, game mode and owner.
//! The head office pays for it from its own money (`depot_price` in `company-server.cfg`) and
//! gives it its starting money; the owner gives a player the running of it (its depot leader: a
//! co-owner of the depot). Money moves between them and the head office sees its depots (step
//! 2: `send`), a bus of their own moves and staff are lent (step 3: `move_bus`, `lend`); a depot
//! that goes bankrupt comes in the next step.

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
    vec![NOT_ON_SERVER, DEPOT_OF_DEPOT, NO_MAP, SAME_MAP, HAS_DEPOT, NO_CASH, NO_CLOCK, NOT_FOUNDED, NOT_RELATED, NO_AMOUNT, NOT_SENT, NO_CASH_SEND, NO_SUCH_BUS, NOT_OWNED, NO_CASH_MOVE, NO_ROOM, NO_SUCH_EMPLOYEE, LEAVING, BORROWED, LENT_ALREADY, BAD_DAYS]
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

// --- step 3: buses moved between maps, staff lent ---------------------------------------------

/// The other companies of `c`'s owner on the server it may send money, buses and staff to: its
/// head office and the head office's other depots, or a head office's depots - (id, name).
pub fn relatives(data: &Path, c: &Company) -> Vec<(String, String)> {
    if !c.online.on_server {
        return Vec::new();
    }
    let hq = if c.online.hq.is_empty() { c.id.clone() } else { c.online.hq.clone() };
    let mut out: Vec<(String, String)> = company_ids(data)
        .into_iter()
        .filter(|id| *id != c.id)
        .filter_map(|id| store::load(data, &id).ok())
        .filter(|o| o.id == hq || o.online.hq == hq)
        .map(|o| (o.id, o.name))
        .collect();
    out.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
    out
}

/// A bus sent to another company of its owner's (`<folder>/buses/<to>.jsonl`), on its way until
/// `arrives` (the sender's date then).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct BusMove {
    pub from: String,
    pub name: String,
    pub vehicle: super::model::Vehicle,
    pub arrives: String,
}

/// Why a bus does not move.
pub const NO_SUCH_BUS: &str = "This bus is not in the fleet.";
pub const NOT_OWNED: &str = "Only a bus the company owns moves to another map: a leased or rented one stays with its contract.";
pub const NO_CASH_MOVE: &str = "The company has not the money for the move.";
pub const NO_ROOM: &str = "The other company's depot has no room for another bus.";

pub fn buses_file(data: &Path, id: &str) -> std::path::PathBuf {
    data.join("buses").join(format!("{id}.jsonl"))
}

/// A JSON line added to a file of the folder (made when missing).
fn append_line(path: &Path, line: &str) -> std::io::Result<()> {
    use std::io::Write;
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

/// Company `c` sends its bus `id` to `to` (its head office or another depot of the owner's,
/// `relatives`): it leaves the fleet now - the month's fixed costs up to today paid as a sale pays
/// them, the move's `price` paid - and joins `to`'s fleet `days` later (`take_buses`). Only a bus
/// it owns. The day it arrives.
pub fn move_bus(data: &Path, c: &mut Company, id: u32, to: &str, price: Cents, days: i64) -> Result<String, &'static str> {
    if !c.online.on_server {
        return Err(NOT_ON_SERVER);
    }
    let other = store::load(data, to).map_err(|_| NOT_RELATED)?;
    if other.id == c.id || !related(c, &other.id, &other.online.hq) {
        return Err(NOT_RELATED);
    }
    let v = c.vehicle(id).cloned().ok_or(NO_SUCH_BUS)?;
    if !matches!(v.tenure, super::model::Tenure::Owned { .. }) {
        return Err(NOT_OWNED);
    }
    if super::depot::room(&other).is_err() {
        return Err(NO_ROOM);
    }
    if c.cash < price {
        return Err(NO_CASH_MOVE);
    }
    let arrives = super::dates::add(&c.date, days.max(0));
    let m = BusMove { from: c.id.clone(), name: c.name.clone(), vehicle: v.clone(), arrives: arrives.clone() };
    append_line(&buses_file(data, to), &serde_json::to_string(&m).map_err(|_| NOT_SENT)?).map_err(|_| NOT_SENT)?;
    let today = c.date.clone();
    super::market::book_part_month(c, &v, &today);
    c.book(BookingKind::Services, -price, format!("{} {}: moved to {}", v.number, v.name, other.name), false);
    c.fleet.retain(|x| x.id != id);
    Ok(arrives)
}

/// The buses on their way to company `c` that have come by its date, into its fleet - with a new
/// id, its fleet number when that is free (else the next), from today its own - from a company of
/// its own only. How many came.
pub fn take_buses(data: &Path, c: &mut Company) -> usize {
    let mut n = 0;
    for (line, end) in lines_after(&buses_file(data, &c.id), c.online.buses_read) {
        let Ok(m) = serde_json::from_str::<BusMove>(&line) else {
            c.online.buses_read = end;
            continue;
        };
        // (on its way still: it and those after it wait)
        if super::dates::between(&m.arrives, &c.date) < 0 {
            break;
        }
        c.online.buses_read = end;
        if !store::load(data, &m.from).ok().is_some_and(|o| related(c, &o.id, &o.online.hq)) {
            continue;
        }
        let mut v = m.vehicle;
        c.counters.vehicle += 1;
        v.id = c.counters.vehicle;
        if c.fleet.iter().any(|x| x.number == v.number) || v.number.trim().is_empty() {
            v.number = c.next_fleet_number();
        }
        v.acquired = c.date.clone();
        v.workshop_until = None;
        c.fleet.push(v);
        n += 1;
    }
    n
}

/// The buses on their way from or to company `c`: (the bus's number and name, the other company,
/// the day it arrives, whether it leaves `c`).
pub fn buses_on_the_way(data: &Path, c: &Company) -> Vec<(String, String, String, bool)> {
    let mut out = Vec::new();
    for (line, _) in lines_after(&buses_file(data, &c.id), c.online.buses_read) {
        if let Ok(m) = serde_json::from_str::<BusMove>(&line) {
            out.push((format!("{} {}", m.vehicle.number, m.vehicle.name), m.name, m.arrives, false));
        }
    }
    for (id, name) in relatives(data, c) {
        let read = store::load(data, &id).map(|o| o.online.buses_read).unwrap_or(0);
        for (line, _) in lines_after(&buses_file(data, &id), read) {
            if let Some(m) = serde_json::from_str::<BusMove>(&line).ok().filter(|m| m.from == c.id) {
                out.push((format!("{} {}", m.vehicle.number, m.vehicle.name), name.clone(), m.arrives, true));
            }
        }
    }
    out
}

/// An employee lent by another company of the owner's (`Employee::lent`): from which, until when,
/// and his id there.
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Lent {
    pub from: String,
    pub name: String,
    pub until: String,
    pub home_id: u32,
}

/// An employee to be lent (`Employee::lend`): to which company, for how many days, asked on which
/// day (he goes at the company's next day).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct LendOut {
    pub to: String,
    pub name: String,
    pub days: i64,
    pub asked: String,
}

/// An employee on his way to another company of the owner's, lent or coming back
/// (`<folder>/people/<to>.jsonl`).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, PartialEq)]
pub struct PeopleMove {
    pub from: String,
    pub name: String,
    pub employee: super::model::Employee,
    /// Lent: his last day there. Coming back: empty.
    pub until: String,
}

/// Why an employee is not lent.
pub const NO_SUCH_EMPLOYEE: &str = "They do not work here.";
pub const LEAVING: &str = "They are leaving the company.";
pub const BORROWED: &str = "They are lent to the company: they go back first.";
pub const LENT_ALREADY: &str = "They are to be lent already.";
pub const BAD_DAYS: &str = "Lend them for 1 to 90 days.";

pub fn people_file(data: &Path, id: &str) -> std::path::PathBuf {
    data.join("people").join(format!("{id}.jsonl"))
}

/// Company `c` lends its employee `id` to `to` (another company of the owner's, `relatives`) for
/// `days` days: he goes at the company's next day (`people_step`) - paid by the company that
/// borrows him while he is there - and comes back after the last of them. The day he goes.
pub fn lend(data: &Path, c: &mut Company, id: u32, to: &str, days: i64) -> Result<String, &'static str> {
    if !c.online.on_server {
        return Err(NOT_ON_SERVER);
    }
    let other = store::load(data, to).map_err(|_| NOT_RELATED)?;
    if other.id == c.id || !related(c, &other.id, &other.online.hq) {
        return Err(NOT_RELATED);
    }
    if !(1..=90).contains(&days) {
        return Err(BAD_DAYS);
    }
    let today = c.date.clone();
    let e = c.staff.iter_mut().find(|e| e.id == id).ok_or(NO_SUCH_EMPLOYEE)?;
    if e.notice_until.is_some() {
        return Err(LEAVING);
    }
    if e.lent.is_some() {
        return Err(BORROWED);
    }
    if e.lend.is_some() {
        return Err(LENT_ALREADY);
    }
    e.lend = Some(LendOut { to: other.id.clone(), name: other.name.clone(), days, asked: today.clone() });
    Ok(super::dates::add(&today, 1))
}

/// Company `c`'s people between the companies of its owner, at its date (each step of its
/// process): who is to be lent goes (from the day after it was asked), who was lent to it goes
/// back after his last day - each with the guarantee of his week paid (`pay::settle`) - and who
/// came is taken into its staff (lent: with a new id, `Employee::lent`; back: with his own id).
/// Whether anything changed.
pub fn people_step(data: &Path, c: &mut Company) -> bool {
    if !c.online.on_server {
        return false;
    }
    let today = c.date.clone();
    let mut changed = false;
    // (those it lends, from the day after it was asked)
    let going: Vec<u32> = c.staff.iter().filter(|e| e.lend.as_ref().is_some_and(|l| super::dates::between(&l.asked, &today) >= 1)).map(|e| e.id).collect();
    for id in going {
        super::pay::settle(c, id);
        let Some(mut e) = c.staff.iter().find(|e| e.id == id).cloned() else { continue };
        let Some(l) = e.lend.take() else { continue };
        let until = super::dates::add(&today, l.days - 1);
        let m = PeopleMove { from: c.id.clone(), name: c.name.clone(), employee: e, until };
        if serde_json::to_string(&m).ok().is_some_and(|line| append_line(&people_file(data, &l.to), &line).is_ok()) {
            c.staff.retain(|x| x.id != id);
            c.staff_rules.asks.retain(|a| a.id != id);
            changed = true;
        }
    }
    // (those lent to it whose days are over: back home)
    let back: Vec<u32> = c.staff.iter().filter(|e| e.lent.as_ref().is_some_and(|l| super::dates::between(&l.until, &today) >= 1)).map(|e| e.id).collect();
    for id in back {
        super::pay::settle(c, id);
        let Some(mut e) = c.staff.iter().find(|e| e.id == id).cloned() else { continue };
        let Some(l) = e.lent.take() else { continue };
        e.id = l.home_id;
        let m = PeopleMove { from: c.id.clone(), name: c.name.clone(), employee: e, until: String::new() };
        if serde_json::to_string(&m).ok().is_some_and(|line| append_line(&people_file(data, &l.from), &line).is_ok()) {
            c.staff.retain(|x| x.id != id);
            c.staff_rules.asks.retain(|a| a.id != id);
            changed = true;
        }
    }
    // (those who came)
    for (line, end) in lines_after(&people_file(data, &c.id), c.online.people_read) {
        c.online.people_read = end;
        changed = true;
        let Ok(m) = serde_json::from_str::<PeopleMove>(&line) else { continue };
        if !store::load(data, &m.from).ok().is_some_and(|o| related(c, &o.id, &o.online.hq)) {
            continue;
        }
        let mut e = m.employee;
        (e.week_paid, e.week_due) = (0.0, 0.0);
        if m.until.is_empty() {
            // (back home: his own id, unless it was given since)
            if c.staff.iter().any(|x| x.id == e.id) || e.id == 0 {
                c.counters.employee += 1;
                e.id = c.counters.employee;
            }
        } else {
            c.counters.employee += 1;
            let home_id = e.id;
            e.id = c.counters.employee;
            e.lent = Some(Lent { from: m.from.clone(), name: m.name.clone(), until: m.until.clone(), home_id });
        }
        c.staff.push(e);
    }
    changed
}

// --- step 4: a depot that goes bankrupt -------------------------------------------------------

/// (Decided 25-26 October 2026) The days a depot may close in the red - warned, and its head
/// office, each day - before it goes bankrupt: it closes, and its head office covers its loss
/// (or, without the money, goes into the red itself - and sells, or closes).
pub const RED_DAYS: u32 = 2;

/// Something told to another company of the server (`<folder>/notices/<id>.jsonl`): a message
/// for its inbox (English, a key of the translations, with its places), and money booked with it
/// (a depot's loss covered: negative; what was left of it: positive).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Notice {
    pub title: String,
    pub text: String,
    pub args: Vec<(String, String)>,
    pub bad: bool,
    pub amount: Cents,
    pub booking: String,
}

pub fn notices_file(data: &Path, id: &str) -> std::path::PathBuf {
    data.join("notices").join(format!("{id}.jsonl"))
}

/// Tell company `to` (`Notice`).
pub fn tell(data: &Path, to: &str, n: &Notice) -> std::io::Result<()> {
    append_line(&notices_file(data, to), &serde_json::to_string(n).map_err(std::io::Error::other)?)
}

/// What was told to company `c` since: into its inbox, its money booked (as capital put in or
/// taken out). Whether anything came.
pub fn take_notices(data: &Path, c: &mut Company) -> bool {
    use super::inbox::{self, Kind, Message};
    let mut any = false;
    for (line, end) in lines_after(&notices_file(data, &c.id), c.online.notices_read) {
        c.online.notices_read = end;
        any = true;
        let Ok(n) = serde_json::from_str::<Notice>(&line) else { continue };
        if n.amount != 0 {
            c.book(BookingKind::Capital, n.amount, n.booking.clone(), false);
        }
        let mut m = Message::new(Kind::Finance, n.title.clone(), n.text.clone()).level(if n.bad { super::clock::Level::Bad } else { super::clock::Level::Warn });
        for (k, v) in &n.args {
            m = m.arg(k, v.clone());
        }
        inbox::post(c, m);
    }
    any
}

/// The messages of a depot in the red, and of its end.
pub const RED_DEPOT: (&str, &str) = ("The depot is in the red: %{n} days to go", "The depot has %{amount} in cash. Still in the red at the close of %{n} more days, it goes bankrupt: it closes, and its head office covers its loss. Sell buses, cut costs, or ask the head office for money.");
pub const RED_DEPOT_LAST: (&str, &str) = ("The depot goes bankrupt tomorrow", "The depot has %{amount} in cash. Still in the red at tomorrow's close, it goes bankrupt: it closes, and its head office covers its loss. Sell buses, cut costs, or ask the head office for money.");
pub const RED_HQ: (&str, &str) = ("Depot %{name} is in the red: %{n} days to go", "Depot %{name} has %{amount} in cash. Still in the red at the close of %{n} more days, it goes bankrupt and the head office covers its loss: send it money from the settings' Depots.");
pub const RED_HQ_LAST: (&str, &str) = ("Depot %{name} goes bankrupt tomorrow", "Depot %{name} has %{amount} in cash. Still in the red at tomorrow's close, it goes bankrupt and the head office covers its loss: send it money from the settings' Depots.");
pub const BANKRUPT: (&str, &str) = ("Depot %{name} is bankrupt", "Depot %{name} stayed in the red and is closed: its buses sold, its staff gone, its lines out to tender again. The head office covered its loss of %{amount}.");
pub const BANKRUPT_RED: (&str, &str) = ("Depot %{name} is bankrupt: the head office is in the red", "Depot %{name} stayed in the red and is closed: its buses sold, its staff gone, its lines out to tender again. The head office covered its loss of %{amount} and is in the red now: sell buses, or close the company.");
pub const BANKRUPT_LEFT: (&str, &str) = ("Depot %{name} is closed", "Depot %{name} stayed in the red and is closed: its buses sold, its staff gone, its lines out to tender again. What was left of it, %{amount}, is the head office's.");

pub fn red_texts() -> Vec<&'static str> {
    [RED_DEPOT, RED_DEPOT_LAST, RED_HQ, RED_HQ_LAST, BANKRUPT, BANKRUPT_RED, BANKRUPT_LEFT].iter().flat_map(|t| [t.0, t.1]).collect()
}

/// What a depot's closed day came to (`day_closed`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Red {
    Fine,
    /// In the red: this many days to go.
    Warned(u32),
    Bankrupt,
}

/// A depot's day closed: in the red - its money below nothing - it is warned, and its head office
/// (`RED_DAYS` times: the days to go); out of it, the count starts again; the day after the last
/// warning, still in the red, it goes bankrupt (`bankrupt`).
pub fn day_closed(data: &Path, c: &mut Company) -> Red {
    use super::inbox::{self, Kind, Message};
    if !c.online.on_server || c.online.hq.is_empty() {
        return Red::Fine;
    }
    if c.cash >= 0 {
        c.online.red_days = 0;
        return Red::Fine;
    }
    c.online.red_days += 1;
    if c.online.red_days > RED_DAYS {
        return Red::Bankrupt;
    }
    let left = RED_DAYS + 1 - c.online.red_days;
    let (own, hq) = if left <= 1 { (RED_DEPOT_LAST, RED_HQ_LAST) } else { (RED_DEPOT, RED_HQ) };
    let m = Message::new(Kind::Finance, own.0, own.1).arg("amount", c.cash.to_string()).arg("n", left.to_string()).level(super::clock::Level::Bad);
    inbox::post(c, m);
    let n = Notice { title: hq.0.into(), text: hq.1.into(), args: vec![("name".into(), c.name.clone()), ("amount".into(), c.cash.to_string()), ("n".into(), left.to_string())], bad: true, amount: 0, booking: String::new() };
    let _ = tell(data, &c.online.hq.clone(), &n);
    Red::Warned(left)
}

/// Depot `c` goes bankrupt: its buses sold (a leased one handed back early, a rented one given
/// back), its staff gone (each with the guarantee of his week; one lent to it back home), its
/// loans paid off, its lines, concessions and contracts given up - out to tender again - and
/// what is left handed to its head office: its loss covered there (the head office's money below
/// nothing if it has not enough), or what remains. The company's file is the caller's to remove.
/// What the head office was booked (negative: the loss it covered).
pub fn bankrupt(data: &Path, c: &mut Company) -> Cents {
    use super::model::Tenure;
    let today = c.date.clone();
    // (its buses)
    for v in std::mem::take(&mut c.fleet) {
        super::market::book_part_month(c, &v, &today);
        let text = format!("{} {}", v.number, v.name);
        match v.tenure {
            Tenure::Owned { .. } => {
                let price = super::market::sale_offer(c, &v);
                c.book(BookingKind::Sale, price, format!("{text} (bankruptcy)"), false);
            }
            Tenure::Leased { monthly, .. } => c.book(BookingKind::Lease, -3 * monthly, format!("{text} (returned early, bankruptcy)"), false),
            Tenure::Rented { .. } => {}
        }
    }
    // (its staff: one lent to it back home, the rest gone)
    let ids: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    for id in ids {
        super::pay::settle(c, id);
    }
    for mut e in std::mem::take(&mut c.staff) {
        if let Some(l) = e.lent.take() {
            e.id = l.home_id;
            let m = PeopleMove { from: c.id.clone(), name: c.name.clone(), employee: e, until: String::new() };
            if let Ok(line) = serde_json::to_string(&m) {
                let _ = append_line(&people_file(data, &l.from), &line);
            }
        }
    }
    // (its loans paid off out of what there is)
    let debt = c.debt();
    if debt > 0 {
        c.book(BookingKind::Repayment, -debt, "Loans paid off (bankruptcy)".to_string(), false);
        c.loans.clear();
    }
    // (its lines given up: out to tender again)
    c.concessions.held.clear();
    c.concessions.contracts.clear();
    c.lines.clear();
    // (the rest: the head office's)
    let left = c.cash;
    let hq = c.online.hq.clone();
    let hq_cash = store::load(data, &hq).map(|h| h.cash).unwrap_or(0);
    let (title, text) = if left >= 0 {
        BANKRUPT_LEFT
    } else if hq_cash + left < 0 {
        BANKRUPT_RED
    } else {
        BANKRUPT
    };
    let booking = if left >= 0 { format!("What was left of depot {}", c.name) } else { format!("Depot {} bankrupt: its loss covered", c.name) };
    let n = Notice { title: title.into(), text: text.into(), args: vec![("name".into(), c.name.clone()), ("amount".into(), left.abs().to_string())], bad: left < 0, amount: left, booking };
    let _ = tell(data, &hq, &n);
    if left != 0 {
        c.book(BookingKind::Capital, -left, "Handed to the head office (bankruptcy)".to_string(), false);
    }
    left
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

    /// (Step 3) A bus of the head office's moves to its depot: out of its fleet at once with the
    /// move paid, into the depot's on the day it arrives - once, with an id of the depot's; not a
    /// leased one, not to a stranger. A driver lent goes the next day, works at the depot with an
    /// id of its own for his days (not to be dismissed there) and comes back with his own id.
    #[test]
    fn a_bus_moves_and_a_driver_is_lent_between_a_head_office_and_its_depot() {
        use super::super::market::{self, MarketBus, Payment};
        use super::super::model::{BusKind, BusSize, Drive, Tenure};
        let dir = std::env::temp_dir().join(format!("openomsi-server-moves-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("omsi");
        for m in ["Grundorf", "Ahlheim"] {
            std::fs::create_dir_all(root.join("maps").join(m)).unwrap();
            std::fs::write(root.join("maps").join(m).join("global.cfg"), "").unwrap();
        }
        let data = dir.join("bco");
        online::server_clock(&data, 1_800_000_000, false, ("2024-03-04".into(), 6 * 3600)).unwrap();
        let mut hq = super::super::found(&Founding { name: "Umove".into(), short: "UMO".into(), date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "x");
        hq.id = "umove".into();
        hq.online.on_server = true;
        hq.online.owner_id = "p1".into();
        hq.cash += 2_000_000_00;
        store::save(&data, &hq).unwrap();
        let id = found(&data, &root, &mut hq, "Ahlheim", 100_000_00, 250_000_00, 1_800_000_000).unwrap();
        assert_eq!(relatives(&data, &hq), [(id.clone(), "Umove Ahlheim".to_string())]);
        // (a bus of its own, and one leased)
        let b = MarketBus { file: "Vehicles/MB_Citaro/Citaro.bus".into(), name: "Citaro".into(), kind: BusKind { size: BusSize::Solo, drive: Drive::Diesel }, ..Default::default() };
        let bus = market::buy_new(&mut hq, &b, Payment::Cash, "").unwrap();
        let mut leased = hq.vehicle(bus).unwrap().clone();
        hq.counters.vehicle += 1;
        (leased.id, leased.number, leased.tenure) = (hq.counters.vehicle, "999".into(), Tenure::Leased { monthly: 1000_00, until: "2030-01-01".into(), residual: 0 });
        hq.fleet.push(leased.clone());
        assert_eq!(move_bus(&data, &mut hq, leased.id, &id, 5000_00, 2), Err(NOT_OWNED));
        assert_eq!(move_bus(&data, &mut hq, 4711, &id, 5000_00, 2), Err(NO_SUCH_BUS));
        assert_eq!(move_bus(&data, &mut hq, bus, "nobody", 5000_00, 2), Err(NOT_RELATED));
        let (cash, n) = (hq.cash, hq.fleet.len());
        assert_eq!(move_bus(&data, &mut hq, bus, &id, 5000_00, 2).unwrap(), "2024-03-06");
        assert_eq!(hq.fleet.len(), n - 1);
        assert!(hq.cash <= cash - 5000_00, "the move paid");
        assert_eq!(buses_on_the_way(&data, &hq).len(), 1);
        let mut d = store::load(&data, &id).unwrap();
        let before = d.fleet.len();
        d.date = "2024-03-05".into();
        assert_eq!(take_buses(&data, &mut d), 0, "on its way still");
        assert_eq!(buses_on_the_way(&data, &d)[0].3, false);
        d.date = "2024-03-06".into();
        assert_eq!(take_buses(&data, &mut d), 1);
        assert_eq!(take_buses(&data, &mut d), 0, "once");
        assert_eq!(d.fleet.len(), before + 1);
        let got = d.fleet.last().unwrap();
        assert_eq!((got.name.as_str(), got.acquired.as_str(), got.id), ("Citaro", "2024-03-06", d.counters.vehicle));
        assert!(buses_on_the_way(&data, &d).is_empty());
        // (a driver lent for three days)
        let a = super::super::staff::applicants(&hq).into_iter().find(|a| a.role == super::super::model::Role::Driver).expect("a driver");
        let man = super::super::staff::hire(&mut hq, &a).unwrap();
        assert_eq!(lend(&data, &mut hq, man, &id, 0), Err(BAD_DAYS));
        assert_eq!(lend(&data, &mut hq, man, &id, 3).unwrap(), "2024-03-05");
        assert_eq!(lend(&data, &mut hq, man, &id, 3), Err(LENT_ALREADY));
        assert!(!people_step(&data, &mut hq), "not before the next day");
        hq.date = "2024-03-05".into();
        assert!(people_step(&data, &mut hq));
        assert!(hq.staff.iter().all(|e| e.id != man), "gone");
        store::save(&data, &hq).unwrap();
        d.date = "2024-03-05".into();
        assert!(people_step(&data, &mut d));
        let there = d.staff.iter().find(|e| e.lent.is_some()).expect("lent to the depot").clone();
        assert_eq!(there.lent.as_ref().map(|l| (l.from.as_str(), l.until.as_str(), l.home_id)), Some(("umove", "2024-03-07", man)));
        assert_eq!(super::super::staff::dismiss(&mut d, there.id), Err(BORROWED));
        d.date = "2024-03-07".into();
        people_step(&data, &mut d);
        assert!(d.staff.iter().any(|e| e.id == there.id), "his last day there");
        d.date = "2024-03-08".into();
        assert!(people_step(&data, &mut d));
        assert!(d.staff.iter().all(|e| e.lent.is_none()), "gone back");
        hq.date = "2024-03-08".into();
        assert!(people_step(&data, &mut hq));
        let back = hq.staff.iter().find(|e| e.id == man).expect("back with his own id");
        assert!(back.lent.is_none() && back.lend.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// (Step 4, decided 25-26 October 2026) A depot in the red is warned at each day's close, and
    /// its head office - two days to go, then one; out of the red the count starts again -, and
    /// the day after, still in the red, it goes bankrupt: its buses sold, its staff gone, its
    /// loans paid off, its lines given up, and its loss covered by its head office (into the red
    /// when it has not the money).
    #[test]
    fn a_depot_in_the_red_is_warned_twice_then_goes_bankrupt() {
        use super::super::market::{self, MarketBus, Payment};
        let dir = std::env::temp_dir().join(format!("openomsi-server-bankrupt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("omsi");
        for m in ["Grundorf", "Ahlheim"] {
            std::fs::create_dir_all(root.join("maps").join(m)).unwrap();
            std::fs::write(root.join("maps").join(m).join("global.cfg"), "").unwrap();
        }
        let data = dir.join("bco");
        online::server_clock(&data, 1_800_000_000, false, ("2024-03-04".into(), 6 * 3600)).unwrap();
        let mut hq = super::super::found(&Founding { name: "Umove".into(), short: "UMO".into(), date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "x");
        hq.id = "umove".into();
        hq.online.on_server = true;
        hq.cash += 1_000_000_00;
        store::save(&data, &hq).unwrap();
        let id = found(&data, &root, &mut hq, "Ahlheim", 100_000_00, 250_000_00, 1_800_000_000).unwrap();
        store::save(&data, &hq).unwrap();
        let mut d = store::load(&data, &id).unwrap();
        d.cash += 1_000_000_00;
        market::buy_new(&mut d, &MarketBus { name: "Citaro".into(), ..Default::default() }, Payment::Cash, "").unwrap();
        // (in the red: two days to go, then one; out of it, again from the start)
        d.cash = -500_000_00;
        assert_eq!(day_closed(&data, &mut d), Red::Warned(2));
        d.cash = 1_00;
        assert_eq!(day_closed(&data, &mut d), Red::Fine);
        d.cash = -500_000_00;
        assert_eq!(day_closed(&data, &mut d), Red::Warned(2));
        assert_eq!(day_closed(&data, &mut d), Red::Warned(1));
        assert!(d.inbox.messages.iter().any(|m| m.title == RED_DEPOT_LAST.0));
        assert!(take_notices(&data, &mut hq));
        assert!(hq.inbox.messages.iter().any(|m| m.title == RED_HQ_LAST.0));
        // (the next close, still in the red: bankrupt - its bus sold, its loss covered)
        assert_eq!(day_closed(&data, &mut d), Red::Bankrupt);
        let left = bankrupt(&data, &mut d);
        assert!(left < 0 && d.fleet.is_empty() && d.staff.is_empty() && d.lines.is_empty() && d.cash == 0, "{left} {}", d.cash);
        let before = hq.cash;
        assert!(take_notices(&data, &mut hq));
        assert_eq!(hq.cash, before + left);
        assert!(hq.inbox.messages.iter().any(|m| m.title == BANKRUPT.0));
        assert!(!take_notices(&data, &mut hq), "once");
        // (a head office without the money goes into the red)
        let mut d2 = d.clone();
        d2.cash = -(hq.cash + 1_000_00);
        hq.cash = 0;
        store::save(&data, &hq).unwrap();
        bankrupt(&data, &mut d2);
        take_notices(&data, &mut hq);
        assert!(hq.cash < 0 && hq.inbox.messages.iter().any(|m| m.title == BANKRUPT_RED.0));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
