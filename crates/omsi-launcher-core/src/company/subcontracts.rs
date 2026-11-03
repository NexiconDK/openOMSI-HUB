//! (NexiconDK, Bus Company Online, decided 3 November 2026) The plan's H10: companies of the server
//! working together on a contract.
//!
//! * **A subcontractor.** A company holding a contract may give one or more of its lines to another
//!   company of the server - a player's company of the same map - for a price a timetable hour they
//!   agree: the main company proposes (`propose`), the other accepts or declines on its Concessions
//!   page. From its first day the subcontractor runs the lines (they are its lines, its tours,
//!   `sub_for` the main company), the main company none of them (`drop_lent`). The authority pays
//!   the main company as before: its contract is judged on the subcontractor's day, which the
//!   subcontractor's process sends on (`LineDayRun`, a day later at the latest) - its fines, its
//!   bonus and its watch stay the main company's. The main company pays the subcontractor every day
//!   for the timetable hours it ran (booked on both sides: the subcontractor's as it closes its day,
//!   the main company's as its contract takes it). Either may end it with `NOTICE_DAYS`' notice; it
//!   ends with the contract's term at the latest. The fares of the lines go to the authority (a gross
//!   contract), not to the subcontractor.
//! * **A joint bid**, in its least form: the company bidding on a tender of the market invites a
//!   partner for some of its lines at a price a timetable hour (`propose` with `joint`); accepted by
//!   the partner before the award, it becomes a running subcontract from the contract's first day
//!   when the bidder wins, and is dropped when it loses. The bid is scored on the bidding company's
//!   quality alone (the partner's is not counted); its contracts are the bidder's.
//!
//! The main server keeps every subcontract (`<data>/subcontracts.json`), from the companies' asks
//! (`<data>/subcontract-asks.jsonl`); each company's process reads them (`refresh`) and sends its
//! subcontracted days (`<data>/subcontracts/<main company>.jsonl`).

use super::bus_contracts::{Journeys, LineDayRun};
use super::dates;
use super::model::{BookingKind, Cents, Company, CompanyLine};
use crate::LineInfo;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Either side ends a running subcontract with this many days' notice.
pub const NOTICE_DAYS: i64 = 2;
/// A proposal not answered within this many days lapses.
pub const ANSWER_DAYS: i64 = 3;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    #[default]
    Proposed,
    /// A joint bid's, accepted by the partner: running from the contract's first day when the
    /// bidder wins.
    Accepted,
    Running,
    Declined,
    Ended,
}

/// A joint bid's tender: its market and the market's id of it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Joint {
    pub key: String,
    pub tender: u32,
    pub number: String,
}

/// A subcontract (or a joint bid's) as the main server keeps it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Subcontract {
    pub id: u32,
    /// The map's market (its folder, lower case).
    pub key: String,
    pub main: String,
    pub main_name: String,
    pub sub: String,
    pub sub_name: String,
    /// The lines (their timetables' names) and the numbers shown.
    pub lines: Vec<String>,
    pub numbers: Vec<String>,
    /// The price a timetable hour the main company pays (euro cents).
    pub hour: Cents,
    /// Its first day, and the contract's last (it ends then at the latest).
    pub from: String,
    pub until: String,
    /// Ended with notice: its last day (empty: `until`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub last: String,
    pub state: State,
    /// A joint bid's tender (None: a subcontract of a contract held).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joint: Option<Joint>,
    /// The server's date it was proposed on.
    pub asked: String,
}

impl Subcontract {
    /// Its last day: ended with notice, or the contract's.
    pub fn end(&self) -> &str {
        if self.last.is_empty() {
            &self.until
        } else {
            &self.last
        }
    }

    /// Running on `date`.
    pub fn runs(&self, date: &str) -> bool {
        self.state == State::Running && dates::between(&self.from, date) >= 0 && dates::between(date, self.end()) >= 0
    }

    pub fn has_line(&self, line: &str) -> bool {
        self.lines.iter().any(|l| l.eq_ignore_ascii_case(line))
    }
}

/// Every subcontract of the server, and how far the companies' asks were read.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Book {
    pub counter: u32,
    pub list: Vec<Subcontract>,
    pub asks_read: u64,
}

/// What a company asks the main server.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AskKind {
    #[default]
    Propose,
    Accept,
    Decline,
    End,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Ask {
    pub kind: AskKind,
    /// The company asking.
    pub by: String,
    /// The subcontract (accept, decline, end).
    pub id: u32,
    /// A proposal: the other company, the lines, the price a timetable hour, the first day.
    pub sub: String,
    pub lines: Vec<String>,
    pub hour: Cents,
    pub from: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub joint: Option<Joint>,
}

/// A subcontracted line's day sent to the main company.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SubDay {
    pub id: u32,
    pub sub: String,
    pub sub_name: String,
    pub line: String,
    pub number: String,
    pub date: String,
    pub hour: Cents,
    pub run: LineDayRun,
}

pub fn book_file(data: &Path) -> PathBuf {
    data.join("subcontracts.json")
}

pub fn asks_file(data: &Path) -> PathBuf {
    data.join("subcontract-asks.jsonl")
}

pub fn days_file(data: &Path, main: &str) -> PathBuf {
    data.join("subcontracts").join(format!("{main}.jsonl"))
}

pub fn load(data: &Path) -> Book {
    std::fs::read_to_string(book_file(data)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn save(data: &Path, b: &Book) -> Result<()> {
    let path = book_file(data);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(b)?)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

/// Why an ask is refused (by the company's page before it is sent).
pub const NOT_HELD: &str = "Your company holds no contract on that line for those days.";
pub const NO_SUB: &str = "Choose another company of the map.";
pub const NO_PRICE: &str = "Set the price a timetable hour.";
pub const SOONEST: &str = "A subcontract starts tomorrow at the soonest.";
pub const LENT: &str = "That line has a subcontract of those days already.";
/// What the companies are told.
pub const PROPOSED_TO: &str = "%{main} proposes that you run its lines %{lines} as its subcontractor from %{from} to %{until}, at %{price} a timetable hour: accept or decline on the Concessions page.";
pub const STARTS: &str = "Your subcontract on %{lines} with %{who} runs from %{from} to %{until}, at %{price} a timetable hour.";
pub const DECLINED: &str = "%{who} declined the subcontract on %{lines}.";
pub const ENDS: &str = "The subcontract on %{lines} with %{who} ends with notice on %{date}.";
pub const JOINT_LOST: &str = "The joint bid on %{n} lost: the subcontract with %{who} on %{lines} falls away.";

/// Company `c` asks the main server (a proposal checked first as far as the company can).
pub fn ask(data: &Path, c: &Company, a: &Ask) -> Result<(), &'static str> {
    if a.kind == AskKind::Propose {
        if a.sub.is_empty() || a.sub == c.id {
            return Err(NO_SUB);
        }
        if a.hour <= 0 {
            return Err(NO_PRICE);
        }
        if a.joint.is_none() {
            if dates::between(&c.date, &a.from) < 1 {
                return Err(SOONEST);
            }
            if a.lines.is_empty() || !a.lines.iter().all(|l| c.concessions.held.iter().any(|h| h.line.eq_ignore_ascii_case(l) && dates::between(&a.from, &h.until) >= 0)) {
                return Err(NOT_HELD);
            }
        }
    }
    let mut a = a.clone();
    a.by = c.id.clone();
    super::shared_tenders::append(&asks_file(data), &serde_json::to_string(&a).map_err(|_| "Not written.")?).map_err(|_| "The server's subcontracts could not be written.")
}

/// The main server a step on, on its `date`: the companies' asks taken (each checked against the
/// companies' files), proposals not answered in time lapse, joint bids settled by their awards,
/// subcontracts past their end ended. What to say in its log.
pub fn step(data: &Path, date: &str) -> Vec<String> {
    let mut said = Vec::new();
    let mut b = load(data);
    let before = b.clone();
    let (asks, to): (Vec<Ask>, u64) = super::shared_tenders::lines_from(&asks_file(data), b.asks_read);
    b.asks_read = to;
    let company = |id: &str| super::store::load(data, id).ok();
    for a in asks {
        match a.kind {
            AskKind::Propose => {
                let (Some(m), Some(s)) = (company(&a.by), company(&a.sub)) else { continue };
                if super::shared_tenders::key_of(&m.map) != super::shared_tenders::key_of(&s.map) || a.hour <= 0 {
                    said.push(format!("subcontract of {} for {} refused: not of the same map", m.name, s.name));
                    continue;
                }
                let (lines, numbers, from, until) = match &a.joint {
                    // (a joint bid: the tender's lines it names, from the contract's first day)
                    Some(j) => {
                        let Some(mk) = super::shared_tenders::load(data, &j.key) else { continue };
                        let Some(mt) = mk.tenders.iter().find(|t| t.tender.market.as_ref().is_some_and(|x| x.id == j.tender) && t.decided.is_none()) else {
                            said.push(format!("joint bid of {} with {} refused: the tender is closed", m.name, s.name));
                            continue;
                        };
                        let t = &mt.tender;
                        let ls: Vec<String> = a.lines.iter().filter(|l| t.has_line(l)).cloned().collect();
                        if ls.is_empty() {
                            continue;
                        }
                        let from = super::concessions::server_start(t);
                        let until = super::concessions::server_term(t.drive, super::concessions::week_hours(t), &from);
                        (ls.clone(), ls, from, until)
                    }
                    None => {
                        let held: Vec<&super::concessions::Concession> = a.lines.iter().filter_map(|l| m.concessions.held.iter().find(|h| h.line.eq_ignore_ascii_case(l) && dates::between(&a.from, &h.until) >= 0)).collect();
                        if held.len() != a.lines.len() || held.is_empty() || dates::between(date, &a.from) < 1 {
                            said.push(format!("subcontract of {} for {} refused: the lines held, from tomorrow", m.name, s.name));
                            continue;
                        }
                        let until = held.iter().map(|h| h.until.clone()).min().unwrap_or_default();
                        (held.iter().map(|h| h.line.clone()).collect(), held.iter().map(|h| h.number.clone()).collect(), a.from.clone(), until)
                    }
                };
                let clash = b.list.iter().any(|x| matches!(x.state, State::Proposed | State::Accepted | State::Running) && x.key == super::shared_tenders::key_of(&m.map) && lines.iter().any(|l| x.has_line(l)) && dates::between(&x.from, &until) >= 0 && dates::between(&from, x.end()) >= 0);
                if clash {
                    said.push(format!("subcontract of {} for {} refused: a line of it has one of those days", m.name, s.name));
                    continue;
                }
                b.counter += 1;
                b.list.push(Subcontract { id: b.counter, key: super::shared_tenders::key_of(&m.map), main: m.id.clone(), main_name: m.name.clone(), sub: s.id.clone(), sub_name: s.name.clone(), lines, numbers, hour: a.hour, from, until, last: String::new(), state: State::Proposed, joint: a.joint.clone(), asked: date.to_string() });
                said.push(format!("subcontract {} proposed: {} to {}", b.counter, m.name, s.name));
            }
            AskKind::Accept | AskKind::Decline => {
                let Some(x) = b.list.iter_mut().find(|x| x.id == a.id && x.state == State::Proposed) else { continue };
                // (the subcontractor answers; the main company may withdraw its proposal)
                if a.by != x.sub && !(a.kind == AskKind::Decline && a.by == x.main) {
                    continue;
                }
                x.state = match (a.kind, x.joint.is_some()) {
                    (AskKind::Decline, _) => State::Declined,
                    (_, true) => State::Accepted,
                    _ => State::Running,
                };
                said.push(format!("subcontract {}: {:?}", x.id, x.state));
            }
            AskKind::End => {
                let Some(x) = b.list.iter_mut().find(|x| x.id == a.id && matches!(x.state, State::Running | State::Accepted)) else { continue };
                if a.by != x.main && a.by != x.sub {
                    continue;
                }
                if x.state == State::Accepted || dates::between(date, &x.from) > NOTICE_DAYS {
                    x.state = State::Declined;
                } else {
                    let last = dates::add(date, NOTICE_DAYS);
                    if dates::between(&last, x.end()) > 0 {
                        x.last = last;
                    }
                }
                said.push(format!("subcontract {} ends on {}", x.id, x.end()));
            }
        }
    }
    for x in b.list.iter_mut() {
        // (a proposal not answered in time lapses)
        if x.state == State::Proposed && dates::between(&x.asked, date) > ANSWER_DAYS {
            x.state = State::Declined;
        }
        // (a joint bid settled by the award: the bidder won it - its concessions of the lines from
        // the contract's first day - or not)
        if x.state == State::Accepted {
            if let Some(j) = x.joint.clone() {
                let decided = super::shared_tenders::load(data, &j.key).and_then(|mk| mk.tenders.iter().find(|t| t.tender.market.as_ref().is_some_and(|y| y.id == j.tender)).map(|t| t.decided.clone()));
                match decided {
                    Some(Some(d)) if d.winner == x.main => x.state = State::Running,
                    Some(Some(_)) | None => x.state = State::Declined,
                    Some(None) => {}
                }
            }
        }
        if x.state == State::Running && dates::between(x.end(), date) > 0 {
            x.state = State::Ended;
        }
    }
    // (the ended and declined ones go after a while)
    b.list.retain(|x| !matches!(x.state, State::Declined | State::Ended) || dates::between(x.end(), date) < 56);
    if b != before {
        if let Err(e) = save(data, &b) {
            said.push(format!("subcontracts not written: {e:#}"));
        }
    }
    said
}

/// The subcontracts of company `c` (main or sub) as the main server keeps them.
pub fn of(b: &Book, c: &Company) -> Vec<Subcontract> {
    b.list.iter().filter(|x| x.main == c.id || x.sub == c.id).cloned().collect()
}

/// The running subcontract of `line` on `date` that company `c` gives (as the main company).
pub fn lent<'a>(c: &'a Company, line: &str, date: &str) -> Option<&'a Subcontract> {
    c.online.subs.iter().find(|x| x.main == c.id && x.has_line(line) && x.runs(date))
}

/// The running subcontract of `line` on `date` that company `c` runs (as the subcontractor).
pub fn borrowed<'a>(c: &'a Company, line: &str, date: &str) -> Option<&'a Subcontract> {
    c.online.subs.iter().find(|x| x.sub == c.id && x.has_line(line) && x.runs(date))
}

/// The main company runs none of its lines given to a subcontractor that day.
pub fn drop_lent(c: &Company, date: &str, tours: &mut Vec<super::network::TourOfDay>) {
    if c.online.subs.is_empty() {
        return;
    }
    tours.retain(|t| lent(c, &t.line, date).is_none());
}

/// Company `c`'s process reads the server's subcontracts (`c.online.subs`), tells what is new,
/// sends the days it ran for others and reads those run for it. Whether its lines must be
/// taken on again (`take_on`: a subcontract begun or ended).
pub fn refresh(data: &Path, c: &mut Company) -> bool {
    let b = load(data);
    let mine = of(&b, c);
    let today = c.date.clone();
    let mut run = super::clock::Run::default();
    let at = super::clock::now(c);
    let price = |x: &Subcontract| x.hour;
    for x in &mine {
        let key = |what: &str| format!("sub|{today}|{}|{what}", x.id);
        let told = |c: &Company, what: &str| c.concessions.events_told.iter().any(|e| e.starts_with("sub|") && e.ends_with(&format!("|{}|{what}", x.id)));
        let lines = x.numbers.join(", ");
        let other = if x.main == c.id { x.sub_name.clone() } else { x.main_name.clone() };
        let (what, text, level) = match x.state {
            State::Proposed if x.sub == c.id => ("proposed", PROPOSED_TO, super::clock::Level::Warn),
            State::Running => ("running", STARTS, super::clock::Level::Good),
            State::Declined if x.main == c.id && x.joint.is_none() => ("declined", DECLINED, super::clock::Level::Minor),
            State::Declined if x.joint.is_some() => ("lost", JOINT_LOST, super::clock::Level::Minor),
            _ => ("", "", super::clock::Level::Info),
        };
        if !what.is_empty() && !told(c, what) {
            c.concessions.events_told.push(key(what));
            let args = vec![
                super::clock::arg("main", &x.main_name),
                super::clock::arg("who", &other),
                super::clock::arg("lines", &lines),
                super::clock::arg("from", &x.from),
                super::clock::arg("until", x.end()),
                super::clock::arg("price", price(x)),
                super::clock::arg("n", x.joint.as_ref().map_or(String::new(), |j| j.number.clone())),
            ];
            super::clock::tell(c, &mut run, at, text, args, level);
        }
        if !x.last.is_empty() && x.state == State::Running && !told(c, "ends") {
            c.concessions.events_told.push(key("ends"));
            super::clock::tell(c, &mut run, at, ENDS, vec![super::clock::arg("lines", &lines), super::clock::arg("who", &other), super::clock::arg("date", &x.last)], super::clock::Level::Info);
        }
    }
    c.online.subs = mine;
    // (the days it ran for others, to them; those run for it, in)
    for d in std::mem::take(&mut c.online.sub_out) {
        if let Ok(line) = serde_json::to_string(&d) {
            if super::shared_tenders::append(&days_file(data, &main_of(&c.online.subs, d.id)), &line).is_err() {
                c.online.sub_out.push(d);
            }
        }
    }
    let (days, to): (Vec<SubDay>, u64) = super::shared_tenders::lines_from(&days_file(data, &c.id), c.online.sub_read);
    c.online.sub_read = to;
    c.online.sub_in.extend(days);
    // (its lines to run for others: taken on, or given up)
    let want: Vec<String> = c.online.subs.iter().filter(|x| x.sub == c.id && x.runs(&today)).flat_map(|x| x.lines.clone()).collect();
    let has: Vec<String> = c.lines.iter().filter(|l| !l.sub_for.is_empty()).map(|l| l.name.clone()).collect();
    want.iter().any(|w| !has.iter().any(|h| h.eq_ignore_ascii_case(w))) || has.iter().any(|h| !want.iter().any(|w| w.eq_ignore_ascii_case(h)))
}

fn main_of(subs: &[Subcontract], id: u32) -> String {
    subs.iter().find(|x| x.id == id).map(|x| x.main.clone()).unwrap_or_default()
}

/// The subcontractor takes on the lines it runs for others today (`lines`: the map's timetable of
/// the day) and gives up those whose subcontract ended.
pub fn take_on(c: &mut Company, lines: &[LineInfo]) -> bool {
    let today = c.date.clone();
    let mut changed = false;
    let ended: Vec<String> = c.lines.iter().filter(|l| !l.sub_for.is_empty() && borrowed(c, &l.name, &today).is_none()).map(|l| l.name.clone()).collect();
    if !ended.is_empty() {
        c.lines.retain(|l| !ended.contains(&l.name));
        changed = true;
    }
    let want: Vec<(String, String)> = c.online.subs.iter().filter(|x| x.sub == c.id && x.runs(&today)).flat_map(|x| x.lines.iter().map(move |l| (l.clone(), x.main.clone()))).collect();
    for (line, main) in want {
        if c.lines.iter().any(|l| l.name.eq_ignore_ascii_case(&line)) {
            continue;
        }
        let Some(l) = lines.iter().find(|l| l.name.eq_ignore_ascii_case(&line)) else { continue };
        if super::network::add_line(c, l, None).is_ok() {
            if let Some(cl) = c.lines.iter_mut().find(|x| x.name.eq_ignore_ascii_case(&line)) {
                cl.sub_for = main;
                cl.added = today.clone();
            }
            changed = true;
        }
    }
    changed
}

/// The day's close (before the contracts judge it): the subcontractor sends each line it ran for
/// another company and is paid its timetable hours; the main company takes the days run for it
/// into its contracts' journeys and pays them.
pub fn close_day(c: &mut Company, date: &str, journeys: &mut Journeys) {
    // (run for others)
    let lines: Vec<CompanyLine> = c.lines.iter().filter(|l| !l.sub_for.is_empty()).cloned().collect();
    for l in lines {
        let Some(x) = borrowed(c, &l.name, date).cloned() else { continue };
        let run = journeys.export(&l.name);
        let hours = run.minutes_run() as f64 / 60.0;
        let pay = (hours * x.hour as f64).round() as Cents;
        if pay > 0 {
            c.book(BookingKind::Contract, pay, format!("{}: line {} run as its subcontractor, {hours:.1} timetable hours", x.main_name, l.number), false);
        }
        c.online.sub_out.push(SubDay { id: x.id, sub: c.id.clone(), sub_name: c.name.clone(), line: l.name.clone(), number: l.number.clone(), date: date.to_string(), hour: x.hour, run });
    }
    // (run for it)
    let days = std::mem::take(&mut c.online.sub_in);
    for d in days {
        if dates::between(&d.date, date) < 0 {
            c.online.sub_in.push(d);
            continue;
        }
        journeys.import(&d.line, &d.run);
        let hours = d.run.minutes_run() as f64 / 60.0;
        let pay = (hours * d.hour as f64).round() as Cents;
        if pay > 0 {
            c.book(BookingKind::Contract, -pay, format!("Subcontractor {}: line {}, {hours:.1} timetable hours", d.sub_name, d.number), false);
        }
    }
}

#[cfg(test)]
#[path = "subcontracts_tests.rs"]
mod tests;
