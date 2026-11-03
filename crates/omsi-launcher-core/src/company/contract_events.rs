//! (NexiconDK, Bus Company Online, decided 3 November 2026) What may happen to a contract during
//! its term (the plan's "Det kan også ske undervejs", H1-H3):
//!
//! * **H1 the timetable changes** (`ContractChange`): the admin changes a company's running
//!   contract (the admin panel's "Change") - a line of the map added to it or taken out of it, or
//!   the map's new timetable taken into use for its lines (after a map update, say). The main
//!   server measures it on the map's week (`measure`: a week's timetable hours and kilometres, the
//!   buses at the peak, the stops - what `concessions::week_of` gives a tender), the company's
//!   process takes it (`received`) and it applies `CHANGE_DAYS` later (`step`). The payment follows
//!   by itself - the timetable hours run at the contract's hour price, an added line's contract
//!   at the package's prices, its contract buses at its bus-day amount, the quality bonus 5 % of
//!   it all. With `change_must_accept` set, a company may decline more service than that share of
//!   its hours within `DECLINE_DAYS` (`decline`): an added line then goes out to tender on its
//!   own, a new timetable ends the contract the day before it and its lines go out to tender
//!   again. Less service it cannot decline.
//! * **H2 holidays** (`refresh_calendar`): the server's calendar - each map's own
//!   (`Holidays.txt`) or Denmark's (`calendar = dk`), and the admin's own periods
//!   (`holidays.json`) as school holidays on top - decides which of the maps' tours run on a day
//!   (`service::ServerCalendar`, read by `lines_in_map_dir`). A contract pays the timetable hours
//!   run that day and its buses every day: fewer hours on a holiday, the buses as ever.
//! * **H3 roadworks** (`Roadwork`): the admin closes a stretch of a line, from stop to stop, for
//!   some days, with the minutes it adds. OMSI cannot draw a road round it: the stops between are
//!   closed - each company running the line gets them as a diversion of its own (`ontime`, its
//!   drivers told before them) and the inbox's "roadworks" event (fewer passengers) -, a journey
//!   of the line is not fined nor unpaid for being late up to those minutes, a player's departure
//!   after the stretch neither (`allowance`, `server_fines::departures`), and every journey run is
//!   paid the minutes on top as timetable time (`extra_minutes`).

use super::bus_contracts::{self as bc, Work};
use super::clock::{self, Level, Run};
use super::concessions::{self as cn, Concession};
use super::dates;
use super::model::{Cents, Company};
use crate::LineInfo;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

// --- H1: the timetable changes -----------------------------------------------------------------

/// A change applies this many days after the admin ordered it.
pub const CHANGE_DAYS: i64 = 2;
/// A company may decline a change it may decline this many days after it came.
pub const DECLINE_DAYS: i64 = 1;

/// The change's part of the settings (`company-server.cfg`).
pub const CHANGE_CFG: &str = "
# a change of a running contract (the admin panel's Change): more service a company must run
# without a say, per cent of its timetable hours - above it the company may decline within a day
# (empty or 0: every change must be run)
change_must_accept =
";

/// The share of its timetable hours a company must run more without a say (`change_must_accept`,
/// per cent); None: every change.
pub fn must_accept(data: &Path) -> Option<f64> {
    super::server_fines::settings(data).get("change_must_accept").and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok()).filter(|x| x.is_finite() && *x > 0.0)
}

/// How the authority changes a running contract.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// The map's timetable as it is now taken into use for its lines.
    #[default]
    Timetable,
    /// A line of the map added to it.
    AddLine,
    /// One of its lines taken out of it.
    RemoveLine,
}

/// A line as the map's timetable has it now: a week's timetable hours and kilometres, the buses
/// at its peak, its stops, and its work an operating day (a contract's `Work`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct LineNow {
    pub line: String,
    pub number: String,
    pub hours: f64,
    pub km: f64,
    pub buses: u32,
    pub stops: u32,
    pub work: Work,
}

/// The authority's change of a company's running contract (`ChangeKind`), as the main server
/// measured it: from when, the week's timetable hours and kilometres and the contract buses
/// before and after, the lines as they run now, and until when the company may decline it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ContractChange {
    /// The company's own number of it (`received`).
    pub id: u32,
    pub kind: ChangeKind,
    /// The line added or taken out (a new timetable: the contract's first line).
    pub line: String,
    pub number: String,
    /// The contract's lines (a package's every line, else its one) and its name (a package's, else
    /// the line's number).
    pub lines: Vec<String>,
    pub name: String,
    /// The day it applies, and the day it was ordered.
    pub from: String,
    pub ordered: String,
    pub hours_before: f64,
    pub hours_after: f64,
    pub km_before: f64,
    pub km_after: f64,
    pub buses_before: u32,
    pub buses_after: u32,
    pub stops_before: u32,
    pub stops_after: u32,
    /// The lines as the timetable has them now: the one added, or each of the contract's.
    pub now: Vec<LineNow>,
    /// The last day the company may decline it (empty: it must be run).
    pub decline_by: String,
    /// What came of it: empty while it waits, `applied`, `declined`.
    pub state: String,
}

impl ContractChange {
    /// The change of the week's timetable hours, a share of the hours before.
    pub fn share(&self) -> f64 {
        if self.hours_before > 0.0 {
            (self.hours_after - self.hours_before) / self.hours_before
        } else if self.hours_after > 0.0 {
            1.0
        } else {
            0.0
        }
    }

    /// The company may still decline it on `today`.
    pub fn declinable(&self, today: &str) -> bool {
        self.state.is_empty() && !self.decline_by.is_empty() && dates::between(today, &self.decline_by) >= 0
    }
}

/// A week's timetable hours, kilometres and contract buses of contract `k` as it was agreed.
fn agreed(c: &Company, k: &bc::Contract) -> (f64, f64, u32) {
    let w = super::variations::work_of(c, k);
    let week = w.days.max(1) as f64 / 52.0;
    let buses = if k.buses > 0 { k.buses } else { w.buses };
    (w.hours * week, w.km * week, buses)
}

/// Line `line` as the map's week has it (`days`: its seven days' lines, Monday first).
pub fn line_now(days: &[Vec<LineInfo>], line: &str) -> Option<LineNow> {
    let l = days.iter().flat_map(|d| d.iter()).find(|l| l.name.eq_ignore_ascii_case(line))?;
    let w = cn::week_of(days, line);
    let run = w.days.max(1) as f64;
    let mut stops: Vec<String> = days.iter().flat_map(|d| d.iter()).filter(|l| l.name.eq_ignore_ascii_case(line)).flat_map(|l| l.tours.iter()).flat_map(|t| t.trips.iter()).flat_map(|t| t.stops.iter()).map(|s| s.name.trim().to_lowercase()).collect();
    stops.sort();
    stops.dedup();
    let number = super::specials::number_of(l, &[] as &[&str]);
    let work = Work { hours: w.hours / run, km: w.km / run, journeys: (w.trips as f64 / run).round() as u32, buses: w.peak, days: (w.days * 52).max(1) };
    Some(LineNow { line: l.name.clone(), number, hours: w.hours, km: w.km, buses: w.peak, stops: stops.len() as u32, work })
}

/// Why a change cannot be made.
pub const NO_CONTRACT: &str = "the company holds no contract on that line";

/// The main server measures the admin's change of company `c`'s contract on `line` (`kind`;
/// `other`: the line added or taken out) on the map's week `days`, ordered on `today`: the
/// contract's lines and name, the week's hours, kilometres, buses and stops before (as agreed)
/// and after (as the timetable runs now), and - with `must`, the share a company must run more
/// without a say (`must_accept`) - until when it may decline more than that.
pub fn measure(c: &Company, kind: ChangeKind, line: &str, other: &str, days: &[Vec<LineInfo>], today: &str, must: Option<f64>) -> Result<ContractChange, String> {
    let h = c.concessions.held.iter().find(|h| h.line.eq_ignore_ascii_case(line)).ok_or(NO_CONTRACT)?;
    let lines: Vec<String> = if h.with.len() > 1 { h.with.clone() } else { vec![h.line.clone()] };
    let name = if h.package.is_empty() { h.number.clone() } else { h.package.clone() };
    let live = |l: &str| c.concessions.contracts.iter().rev().find(|k| k.live() && k.line.eq_ignore_ascii_case(l));
    let (mut hb, mut kb, mut bb) = (0.0, 0.0, 0u32);
    for l in &lines {
        if let Some(k) = live(l) {
            let (a, b, n) = agreed(c, k);
            (hb, kb, bb) = (hb + a, kb + b, bb + n);
        }
    }
    let stops_of = |ls: &[String]| -> u32 { ls.iter().filter_map(|l| line_now(days, l)).map(|n| n.stops).sum() };
    let mut ch = ContractChange { kind, lines: lines.clone(), name, ordered: today.to_string(), from: dates::add(today, CHANGE_DAYS), hours_before: hb, km_before: kb, buses_before: bb, stops_before: stops_of(&lines), ..Default::default() };
    match kind {
        ChangeKind::Timetable => {
            ch.now = lines.iter().filter_map(|l| line_now(days, l)).collect();
            if ch.now.is_empty() {
                return Err("the map's timetable has none of the contract's lines".into());
            }
            (ch.line, ch.number) = (h.line.clone(), h.number.clone());
            ch.hours_after = ch.now.iter().map(|n| n.hours).sum();
            ch.km_after = ch.now.iter().map(|n| n.km).sum();
            ch.buses_after = ch.now.iter().map(|n| n.buses).sum();
            ch.stops_after = ch.now.iter().map(|n| n.stops).sum();
        }
        ChangeKind::AddLine => {
            if lines.iter().any(|l| l.eq_ignore_ascii_case(other)) {
                return Err("the line is the contract's already".into());
            }
            if c.concessions.held.iter().any(|x| x.line.eq_ignore_ascii_case(other)) {
                return Err("the company holds that line on another contract".into());
            }
            let n = line_now(days, other).ok_or("the map's timetable has no such line")?;
            (ch.line, ch.number) = (n.line.clone(), n.number.clone());
            (ch.hours_after, ch.km_after, ch.buses_after, ch.stops_after) = (hb + n.hours, kb + n.km, bb + n.buses, ch.stops_before + n.stops);
            ch.now = vec![n];
        }
        ChangeKind::RemoveLine => {
            if lines.len() < 2 {
                return Err("the contract has that line alone: end it instead (Move the end)".into());
            }
            let l = lines.iter().find(|l| l.eq_ignore_ascii_case(other)).ok_or("that line is none of the contract's")?;
            let x = c.concessions.held.iter().find(|x| x.line.eq_ignore_ascii_case(l)).map(|x| x.number.clone()).unwrap_or_else(|| l.clone());
            let (a, b, n) = live(l).map(|k| agreed(c, k)).unwrap_or_default();
            (ch.line, ch.number) = (l.clone(), x);
            (ch.hours_after, ch.km_after, ch.buses_after) = ((hb - a).max(0.0), (kb - b).max(0.0), bb.saturating_sub(n));
            ch.stops_after = ch.stops_before.saturating_sub(line_now(days, l).map_or(0, |n| n.stops));
        }
    }
    if let Some(m) = must.filter(|_| kind != ChangeKind::RemoveLine) {
        if ch.share() * 100.0 > m + 1e-9 {
            ch.decline_by = dates::add(today, DECLINE_DAYS);
        }
    }
    Ok(ch)
}

/// What the company is told of a change, and when it applies.
pub const CHANGE_TOLD: &str = "The authority changes your contract %{name} from %{date}: %{hours} timetable hours a week (%{before} → %{after}), %{buses} contract buses, %{stops} stops. Plan your duties for it.";
pub const CHANGE_MAY_DECLINE: &str = "It is more than you must run without a say: you may decline it until %{date} on the Concessions page.";
pub const CHANGE_APPLIED: &str = "Your contract %{name} runs changed from today: %{hours} timetable hours a week, %{buses} contract buses.";
pub const DECLINED_ADD: &str = "You declined the change of your contract %{name}: line %{n} goes out to tender on its own.";
pub const DECLINED_TIMETABLE: &str = "You declined the change of your contract %{name}: it ends on %{date}, and its lines go out to tender again.";
pub const NO_CHANGE: &str = "There is no such change to decline.";
pub const MUST_RUN: &str = "This change is within what you must run: it cannot be declined.";

/// A signed whole number ("+120", "-2", "0").
fn signed(x: f64) -> String {
    let r = x.round();
    if r > 0.0 {
        format!("+{r:.0}")
    } else {
        format!("{r:.0}")
    }
}

/// The company's process takes a change the admin ordered (`shared_tenders::take_contract_orders`):
/// kept with the company (`Online::changes`), told in its news; a line taken out ends the day
/// before the change at once (it cannot be declined).
pub fn received(c: &mut Company, mut ch: ContractChange, run: &mut Run, at: i64) {
    ch.id = c.online.changes.iter().map(|x| x.id).max().unwrap_or(0) + 1;
    ch.state.clear();
    let last = dates::add(&ch.from, -1);
    if ch.kind == ChangeKind::RemoveLine {
        for h in c.concessions.held.iter_mut().filter(|h| h.line.eq_ignore_ascii_case(&ch.line) && dates::between(&last, &h.until) > 0) {
            h.until = last.clone();
        }
        for k in c.concessions.contracts.iter_mut().filter(|k| k.live() && k.line.eq_ignore_ascii_case(&ch.line) && dates::between(&last, &k.until) > 0) {
            k.until = last.clone();
        }
    }
    let args = vec![
        clock::arg("name", &ch.name),
        clock::arg("date", &ch.from),
        clock::arg("hours", signed(ch.hours_after - ch.hours_before)),
        clock::arg("before", format!("{:.0}", ch.hours_before)),
        clock::arg("after", format!("{:.0}", ch.hours_after)),
        clock::arg("buses", signed(ch.buses_after as f64 - ch.buses_before as f64)),
        clock::arg("stops", signed(ch.stops_after as f64 - ch.stops_before as f64)),
    ];
    clock::tell(c, run, at, CHANGE_TOLD, args, Level::Warn);
    if !ch.decline_by.is_empty() {
        clock::tell(c, run, at, CHANGE_MAY_DECLINE, vec![clock::arg("date", &ch.decline_by)], Level::Warn);
    }
    c.online.changes.push(ch);
    // (those over for four weeks go)
    let today = c.date.clone();
    c.online.changes.retain(|x| x.state.is_empty() || dates::between(&x.from, &today) <= 28);
}

/// The changes due by the company's day, applied: an added line's concession and contract at the
/// package's prices from the change's day to the contract's end (the line taken on with the
/// others: `concessions::take_on`), a new timetable's work and contract buses kept with the
/// contracts, told in its news. Whether anything changed, and whether a line is to be taken on.
pub fn step(c: &mut Company) -> (bool, bool) {
    let today = c.date.clone();
    let due: Vec<usize> = c.online.changes.iter().enumerate().filter(|(_, x)| x.state.is_empty() && dates::between(&x.from, &today) >= 0).map(|(i, _)| i).collect();
    if due.is_empty() {
        return (false, false);
    }
    let mut run = Run::default();
    let at = clock::now(c);
    let mut take = false;
    for i in due {
        let ch = c.online.changes[i].clone();
        match ch.kind {
            ChangeKind::Timetable => {
                for n in &ch.now {
                    for k in c.concessions.contracts.iter_mut().filter(|k| k.live() && k.line.eq_ignore_ascii_case(&n.line)) {
                        k.work = Some(n.work);
                        k.journeys = n.work.journeys.max(1);
                        if k.per_bus_day > 0 {
                            k.buses = n.buses;
                        }
                    }
                }
            }
            ChangeKind::AddLine => take |= add_line(c, &ch),
            ChangeKind::RemoveLine => {
                for h in c.concessions.held.iter_mut() {
                    h.with.retain(|l| !l.eq_ignore_ascii_case(&ch.line));
                }
            }
        }
        c.online.changes[i].state = "applied".into();
        let args = vec![clock::arg("name", &ch.name), clock::arg("hours", format!("{:.0}", ch.hours_after)), clock::arg("buses", ch.buses_after)];
        clock::tell(c, &mut run, at, CHANGE_APPLIED, args, Level::Info);
    }
    (true, take)
}

/// Change `ch`'s line added to its contract: its concession (in the package - one made of a line
/// that ran alone, named by its number) and its contract at the package's prices.
fn add_line(c: &mut Company, ch: &ContractChange) -> bool {
    let Some(n) = ch.now.first().cloned() else { return false };
    let first = ch.lines.first().cloned().unwrap_or_default();
    let Some(base) = c.concessions.contracts.iter().rev().find(|k| k.live() && k.line.eq_ignore_ascii_case(&first)).cloned() else { return false };
    let Some(held) = c.concessions.held.iter().find(|h| h.line.eq_ignore_ascii_case(&first)).cloned() else { return false };
    if dates::between(&ch.from, &base.until) < 0 {
        return false;
    }
    let package = if held.package.is_empty() { held.number.clone() } else { held.package.clone() };
    let mut with: Vec<String> = if held.with.len() > 1 { held.with.clone() } else { vec![held.line.clone()] };
    with.push(n.line.clone());
    let year = (base.per_hour as f64 * n.work.hours * n.work.days.max(1) as f64).round() as Cents + base.per_bus_day * n.buses as Cents * 365;
    let mut k = bc::make(c, &n.line, &n.number, base.kind, base.period, year, &n.work, &ch.from, &base.until);
    (k.index, k.extensions_left, k.electric, k.needs, k.per_hour, k.per_bus_day) = (base.index, 0, base.electric, base.needs, base.per_hour, base.per_bus_day);
    if base.per_bus_day > 0 {
        k.buses = n.buses;
    }
    c.concessions.contracts.push(k);
    for h in c.concessions.held.iter_mut().filter(|h| with.iter().any(|l| l.eq_ignore_ascii_case(&h.line))) {
        (h.package, h.with) = (package.clone(), with.clone());
    }
    c.concessions.held.push(Concession { line: n.line.clone(), number: n.number.clone(), from: ch.from.clone(), until: base.until.clone(), price: held.price, direct: held.direct, after: String::new(), options: held.options, package, with });
    true
}

/// Company `c` declines its change `id` (the Concessions page), while it may: an added line goes
/// out to tender on its own; a new timetable ends the contract the day before it - its lines go
/// out to tender again as any ending contract's (`shared_tenders::offer`).
pub fn decline(data: &Path, c: &mut Company, id: u32) -> Result<(), &'static str> {
    let today = c.date.clone();
    let k = c.online.changes.iter().position(|x| x.id == id && x.state.is_empty()).ok_or(NO_CHANGE)?;
    if !c.online.changes[k].declinable(&today) {
        return Err(MUST_RUN);
    }
    let ch = c.online.changes[k].clone();
    let what = match ch.kind {
        ChangeKind::AddLine => {
            if c.online.on_server {
                super::shared_tenders::ask_as_server(data, &super::shared_tenders::key_of(&c.map), &ch.line, Default::default()).map_err(|_| "The line could not be put out to tender.")?;
            }
            (DECLINED_ADD, vec![clock::arg("name", &ch.name), clock::arg("n", &ch.number)])
        }
        _ => {
            let last = dates::add(&ch.from, -1);
            for h in c.concessions.held.iter_mut().filter(|h| ch.lines.iter().any(|l| l.eq_ignore_ascii_case(&h.line)) && dates::between(&last, &h.until) > 0) {
                h.until = last.clone();
            }
            for x in c.concessions.contracts.iter_mut().filter(|x| x.live() && ch.lines.iter().any(|l| l.eq_ignore_ascii_case(&x.line)) && dates::between(&last, &x.until) > 0) {
                x.until = last.clone();
            }
            (DECLINED_TIMETABLE, vec![clock::arg("name", &ch.name), clock::arg("date", &last)])
        }
    };
    c.online.changes[k].state = "declined".into();
    let mut run = Run::default();
    let at = clock::now(c);
    clock::tell(c, &mut run, at, what.0, what.1, Level::Warn);
    Ok(())
}

// --- H2: holidays ------------------------------------------------------------------------------

/// The calendar's part of the settings (`company-server.cfg`).
pub const CALENDAR_CFG: &str = "
# the calendar the maps run by: map (each map's own Holidays.txt) or dk (Denmark's school
# holidays and public holidays); the admin's own holiday periods: the admin panel's Holidays
calendar = map
";

/// One of the admin's holiday periods (`holidays.json`): its name and its first and last day.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Period {
    pub id: u32,
    pub name: String,
    pub from: String,
    pub until: String,
}

pub fn periods_file(data: &Path) -> PathBuf {
    data.join("holidays.json")
}

pub fn load_periods(data: &Path) -> Vec<Period> {
    std::fs::read_to_string(periods_file(data)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save_json<T: Serialize>(path: &Path, x: &T) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(x)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Whether the server's calendar is Denmark's (`calendar = dk`).
pub fn danish(data: &Path) -> bool {
    super::server_fines::settings(data).get("calendar").is_some_and(|v| matches!(v.trim().to_lowercase().as_str(), "dk" | "da" | "denmark" | "danish"))
}

/// The server's calendar of its folder `data` (None: each map's own, no period of the admin's).
pub fn calendar_of(data: &Path) -> Option<crate::service::ServerCalendar> {
    let periods: Vec<(i32, i32, String)> = load_periods(data)
        .into_iter()
        .filter_map(|p| Some((omsi_map::date_code(&p.from)?, omsi_map::date_code(if p.until.trim().is_empty() { &p.from } else { &p.until })?, p.name)))
        .filter(|p| p.0 <= p.1)
        .collect();
    let danish = danish(data);
    (danish || !periods.is_empty()).then_some(crate::service::ServerCalendar { danish, periods })
}

/// When the settings and the admin's periods were last written (seconds, both).
fn calendar_stamp(data: &Path) -> (u64, u64) {
    let at = |p: PathBuf| std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs().max(1));
    (at(data.join(super::main_server::CFG_FILE)), at(periods_file(data)))
}

static CALENDAR_READ: std::sync::Mutex<Option<(PathBuf, (u64, u64))>> = std::sync::Mutex::new(None);

/// The server's calendar for this process (`service::set_server_calendar`), read again when its
/// settings or the admin's periods changed. Whether it was read.
pub fn refresh_calendar(data: &Path) -> bool {
    let stamp = calendar_stamp(data);
    let mut seen = CALENDAR_READ.lock().unwrap_or_else(|e| e.into_inner());
    if seen.as_ref().is_some_and(|s| s.0 == data && s.1 == stamp) {
        return false;
    }
    *seen = Some((data.to_path_buf(), stamp));
    crate::service::set_server_calendar(calendar_of(data));
    true
}

/// The admin adds a holiday period (`name`, `from` to `until`, YYYY-MM-DD).
pub fn add_period(data: &Path, name: &str, from: &str, until: &str) -> Result<Period, String> {
    let until = if until.trim().is_empty() { from } else { until };
    if dates::parse(from).is_none() || dates::parse(until).is_none() || dates::between(from, until) < 0 {
        return Err("the days as YYYY-MM-DD, the last not before the first".into());
    }
    let name = name.trim();
    if name.is_empty() {
        return Err("name the holidays".into());
    }
    let mut all = load_periods(data);
    let p = Period { id: all.iter().map(|p| p.id).max().unwrap_or(0) + 1, name: name.to_string(), from: from.trim().to_string(), until: until.trim().to_string() };
    all.push(p.clone());
    save_json(&periods_file(data), &all).map_err(|e| format!("{e:#}"))?;
    Ok(p)
}

/// The admin takes holiday period `id` away.
pub fn remove_period(data: &Path, id: u32) -> Result<Period, String> {
    let mut all = load_periods(data);
    let k = all.iter().position(|p| p.id == id).ok_or("no such holidays")?;
    let p = all.remove(k);
    save_json(&periods_file(data), &all).map_err(|e| format!("{e:#}"))?;
    Ok(p)
}

// --- H3: roadworks -----------------------------------------------------------------------------

/// The ids of the diversions and the inbox's events the server's roadworks are kept as by each
/// company (its own ids lie below).
pub const ROADWORK_IDS: u64 = 9_000_000;
/// Passengers while a line's roadworks last, against the usual.
pub const ROADWORK_DEMAND: f64 = 0.93;

/// Roadworks the admin set on a line of a map (`roadworks.json`): the stretch from stop to stop,
/// the stops between closed, its days and the minutes it adds to a journey.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Roadwork {
    pub id: u32,
    /// The map's market key (`shared_tenders::key_of`), its line (timetable name) and number.
    pub map: String,
    pub line: String,
    pub number: String,
    pub from_stop: String,
    pub to_stop: String,
    pub closed: Vec<String>,
    pub from: String,
    pub until: String,
    pub minutes: i32,
}

impl Roadwork {
    pub fn on(&self, date: &str) -> bool {
        dates::between(&self.from, date) >= 0 && dates::between(date, &self.until) >= 0
    }

    /// Stop `name` is on the stretch: an end of it or one closed.
    pub fn on_stretch(&self, name: &str) -> bool {
        let same = |a: &str| a.trim().eq_ignore_ascii_case(name.trim());
        same(&self.from_stop) || same(&self.to_stop) || self.closed.iter().any(|s| same(s))
    }
}

pub fn roadworks_file(data: &Path) -> PathBuf {
    data.join("roadworks.json")
}

pub fn load_roadworks(data: &Path) -> Vec<Roadwork> {
    std::fs::read_to_string(roadworks_file(data)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// The stops between `a` and `b` on line `l`'s first trip that serves both (either way), and the
/// two as the timetable names them. None: no trip serves both.
pub fn stretch(l: &LineInfo, a: &str, b: &str) -> Option<(String, String, Vec<String>)> {
    let same = |x: &str, y: &str| x.trim().eq_ignore_ascii_case(y.trim());
    for t in l.tours.iter().flat_map(|t| t.trips.iter()) {
        let (Some(i), Some(j)) = (t.stops.iter().position(|s| same(&s.name, a)), t.stops.iter().position(|s| same(&s.name, b))) else { continue };
        if i == j {
            continue;
        }
        let (lo, hi) = (i.min(j), i.max(j));
        let between: Vec<String> = t.stops[lo + 1..hi].iter().map(|s| s.name.clone()).collect();
        return Some((t.stops[i].name.clone(), t.stops[j].name.clone(), between));
    }
    None
}

/// The admin sets roadworks on `rw.line` of map `rw.map` (the line as the map's timetable of its
/// first day has it: `lines`): its stretch's stops closed, kept with the server's.
pub fn add_roadwork(data: &Path, mut rw: Roadwork, lines: &[LineInfo]) -> Result<Roadwork, String> {
    let until = if rw.until.trim().is_empty() { rw.from.clone() } else { rw.until.clone() };
    if dates::parse(&rw.from).is_none() || dates::parse(&until).is_none() || dates::between(&rw.from, &until) < 0 {
        return Err("the days as YYYY-MM-DD, the last not before the first".into());
    }
    if !(0..=120).contains(&rw.minutes) {
        return Err("the minutes it adds: 0 to 120".into());
    }
    let l = lines.iter().find(|l| l.name.eq_ignore_ascii_case(rw.line.trim())).ok_or("the map's timetable has no such line")?;
    let Some((a, b, closed)) = stretch(l, &rw.from_stop, &rw.to_stop) else {
        let mut names: Vec<String> = l.tours.iter().flat_map(|t| t.trips.iter()).flat_map(|t| t.stops.iter()).map(|s| s.name.clone()).collect();
        names.sort();
        names.dedup();
        return Err(format!("no trip of line {} serves both stops - its stops: {}", l.name, names.join(", ")));
    };
    let mut all = load_roadworks(data);
    rw.id = all.iter().map(|x| x.id).max().unwrap_or(0) + 1;
    (rw.line, rw.number, rw.from_stop, rw.to_stop, rw.closed, rw.until) = (l.name.clone(), super::specials::number_of(l, &[] as &[&str]), a, b, closed, until);
    all.push(rw.clone());
    save_json(&roadworks_file(data), &all).map_err(|e| format!("{e:#}"))?;
    Ok(rw)
}

/// The admin ends roadworks `id` (taken away from the companies at their next step).
pub fn end_roadwork(data: &Path, id: u32) -> Result<Roadwork, String> {
    let mut all = load_roadworks(data);
    let k = all.iter().position(|x| x.id == id).ok_or("no such roadworks")?;
    let rw = all.remove(k);
    save_json(&roadworks_file(data), &all).map_err(|e| format!("{e:#}"))?;
    Ok(rw)
}

fn roadworks_stamp(data: &Path) -> u64 {
    std::fs::metadata(roadworks_file(data)).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs().max(1))
}

/// Company `c` takes the server's roadworks on the lines it runs, read again when the admin
/// changed them (`Online::roadworks`): each a diversion of its own - its stops closed, its drivers
/// told - and the inbox's "roadworks" event on the line (fewer riders); those the admin ended go.
/// Whether anything changed.
pub fn roadworks_step(data: &Path, c: &mut Company) -> bool {
    if !c.online.on_server {
        return false;
    }
    let at = roadworks_stamp(data);
    if c.online.roadworks_at == at && at != 0 {
        return false;
    }
    c.online.roadworks_at = at;
    let key = super::shared_tenders::key_of(&c.map);
    let mine: Vec<Roadwork> = load_roadworks(data).into_iter().filter(|r| r.map == key && c.lines.iter().any(|l| l.name.eq_ignore_ascii_case(&r.line))).collect();
    let before = (c.ontime.diversions.clone(), c.inbox.events.clone(), c.online.roadworks.clone());
    let ids: Vec<u64> = mine.iter().map(|r| ROADWORK_IDS + r.id as u64).collect();
    c.ontime.diversions.retain(|d| d.id < ROADWORK_IDS || ids.contains(&d.id));
    c.inbox.events.retain(|e| e.id < ROADWORK_IDS || ids.contains(&e.id));
    for r in &mine {
        let id = ROADWORK_IDS + r.id as u64;
        let number = c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(&r.line)).map(|l| l.number.clone()).unwrap_or_else(|| r.number.clone());
        let d = super::ontime::Diversion { id, from: r.from.clone(), until: r.until.clone(), lines: vec![r.line.clone()], closed: r.closed.clone(), via: format!("{} - {}", r.from_stop, r.to_stop), note: format!("+{} min", r.minutes) };
        match c.ontime.diversions.iter_mut().find(|x| x.id == id) {
            Some(x) => *x = d,
            None => c.ontime.diversions.push(d),
        }
        if !c.inbox.events.iter().any(|e| e.id == id) {
            c.inbox.events.push(super::inbox::Event { id, key: "roadworks".into(), from: r.from.clone(), until: r.until.clone(), fuel: 1.0, demand: ROADWORK_DEMAND, late: 0.0, line: Some(number), started: false, ended: false });
        }
    }
    c.online.roadworks = mine;
    (c.ontime.diversions.clone(), c.inbox.events.clone(), c.online.roadworks.clone()) != before
}

/// The minutes the server's roadworks add to a journey of `line` (timetable name) on `date`.
pub fn extra_minutes(c: &Company, date: &str, line: &str) -> i32 {
    c.online.roadworks.iter().filter(|r| r.on(date) && r.line.eq_ignore_ascii_case(line)).map(|r| r.minutes.max(0)).sum()
}

/// The roadworks on `line` on `date` (the first).
pub fn on_line<'a>(c: &'a Company, date: &str, line: &str) -> Option<&'a Roadwork> {
    c.online.roadworks.iter().find(|r| r.on(date) && r.line.eq_ignore_ascii_case(line))
}

/// A journey `minutes` off its timetable as the authority judges it with the roadworks' `extra`
/// minutes allowed: late by so much less, never early by them.
pub fn judged(minutes: i32, extra: i32) -> i32 {
    if minutes > 0 {
        (minutes - extra.max(0)).max(0)
    } else {
        minutes
    }
}

#[cfg(test)]
#[path = "contract_events_tests.rs"]
mod tests;
