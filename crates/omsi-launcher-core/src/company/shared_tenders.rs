//! (NexiconDK) Bus Company Online's tenders (decided 25 October 2026; phase 5's tenders brought
//! forward, docs/omsihub/BUS_COMPANY_ONLINE.md): one market of tenders a map, shared by every
//! company of the map on the server - they bid live against each other and against the map's
//! other operators.
//!
//! The main server keeps it (`step`, `<folder>/market/<map>.json`): every four weeks of the
//! server's date some of the map's lines no company runs go out to tender, a concession of a
//! company ending soon goes out again - open to every company of the map: the winner runs the
//! line from the day its new term begins - and a line a company applies for on its Lines page
//! (`ask`) goes out at once. It closes each tender a minute after its last minute, when every bid
//! is in, and decides it by the auction of every company's bids against the map's operators
//! (`auction::replay_many`, the 70/30 score, the same for every company): won by a company, by an
//! operator (the map's own buses run the line until it goes out again), or by nobody. On the
//! server no line is bought outright.
//!
//! (31 October 2026) The bids are sealed (`sealed`): each company's is its own until the close,
//! then every bid is opened and shown with its points; an operator that wins holds the line for
//! a term like a company (`OperatorHold`) and may give it up early, having bid below its own cost.
//!
//! (1 November 2026) Special services: a chrono event of the map (`Chrono/<folder>`: a match day,
//! a fair) that brings lines of its own puts each of them out to tender for its days alone
//! (`MapEvent`) - from six days before its first day to three days before, as a renewal - and
//! the company that wins runs it on those days; nobody: the map's operators. Such a line is
//! never offered otherwise, nor renewed. A company whose line an event takes off
//! (`[deactivate_lines]`) is told in time.
//!
//! Each company's process keeps a copy of the market's open tenders among its own (`sync`,
//! `concessions::Tender::market`): it bids on them as ever (its pages, its clock telling the bids
//! as they come), its bid goes into the market's bids (`post_bid`,
//! `<folder>/market/<map>.bids.jsonl`) and every other company of the map sees it at its next
//! step; and it takes what the main server decided (won: the concession with its contract; lost:
//! who won). Its own tenders of before the market are withdrawn, their fees paid back.

use super::clock::{self, Level, Run};
use super::concessions::{self as cn, OtherBids, Rival, Shared, Standing, Tender};
use super::dates;
use super::model::{BookingKind, Cents, Company, Drive, GameMode};
use super::online;
use super::rng::Rng;
use crate::LineInfo;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A tender is decided this many minutes after its last minute (every bid in by then).
pub const CLOSE_AFTER: i64 = 1;
/// (Decided 25 October 2026) A tender of the market runs three days; a concession put out again
/// opens six days before its new term begins and is decided three days before it - the winner
/// has three days to make ready.
pub const TENDER_DAYS: i64 = 3;
pub const RENEW_OPENS: i64 = 6;
pub const RENEW_DECIDED: i64 = 3;

/// Decided tenders are kept this many days.
pub const KEEP_DAYS: i64 = 56;

/// A map's market (written by the main server alone).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Market {
    /// The map's folder (lower case), and the map as its companies name it.
    pub key: String,
    pub map: String,
    /// The round of four weeks it last offered lines in (`concessions::ROUND_WEEKS`), and the
    /// server's date it last looked at the rounds and the renewals.
    pub round: i64,
    pub day: String,
    pub counter: u32,
    /// The map's other operators (made once).
    pub rivals: Vec<Rival>,
    pub tenders: Vec<MarketTender>,
    /// How far the companies' applications were read (bytes of their file).
    pub asks_read: u64,
    /// The server's date the week of its tenders could not be read on (`fill_weeks`: tried
    /// again the next day).
    pub weeks_failed: String,
    /// (31 October 2026) The lines the map's operators won, for their terms.
    pub held: Vec<OperatorHold>,
    /// (1 November 2026) The map's chrono events not over yet, as the main server last read them:
    /// their own lines (never offered otherwise) and the lines they take off (the companies told).
    pub events: Vec<MarketEvent>,
    /// (1 November 2026) The map's lines with passengers of their own as the main server last read
    /// them (the admin panel's lines to put out to tender).
    pub lines: Vec<String>,
}

/// (NexiconDK, 1 November 2026) A chrono event of the map as the market keeps it (`MapEvent`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct MarketEvent {
    pub name: String,
    pub from: String,
    pub until: String,
    /// Its own lines (`.ttl` names), and those it takes off.
    pub lines: Vec<String>,
    pub off: Vec<String>,
}

/// (NexiconDK, 1 November 2026) A chrono event of the map (`Chrono/<folder>/Chrono.cfg` with a
/// first and a last day) as the main server reads it for its market: its name, its days, the
/// lines of its own (its `TTData`'s that the map's own has not) - with their timetable of its
/// first day once it is near (`lines`) - and the lines it takes off (`[deactivate_lines]`).
#[derive(Clone, Debug, Default)]
pub struct MapEvent {
    pub name: String,
    pub from: String,
    pub until: String,
    pub own: Vec<String>,
    pub off: Vec<String>,
    pub lines: Vec<LineInfo>,
}

/// The chrono events of the map in `map_dir` not over on the server's `date` (an open-ended
/// scenario - a timetable change for good - is none: its lines are the map's from its day): each
/// one's own lines read from the timetable of its first day by `read` when that is within a week
/// (None: not read - tried again the next day).
pub fn map_events(map_dir: &Path, date: &str, read: &mut dyn FnMut(&str) -> Option<Vec<LineInfo>>) -> Vec<MapEvent> {
    let ttl_names = |dir: &Path| -> Vec<String> {
        omsi_cfg::vfs::read_dir_paths(&omsi_cfg::resolve_path(dir, "TTData"))
            .into_iter()
            .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("ttl")))
            .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .collect()
    };
    let map_own = ttl_names(map_dir);
    let day = |code: i32| format!("{:04}-{:02}-{:02}", code / 10000, code / 100 % 100, code % 100);
    let mut out = Vec::new();
    for (folder, cfg) in omsi_map::all_chrono(map_dir) {
        if cfg.start_date <= 0 || cfg.end_date < cfg.start_date {
            continue;
        }
        let (from, until) = (day(cfg.start_date), day(cfg.end_date));
        if dates::parse(&from).is_none() || dates::parse(&until).is_none() || dates::between(&until, date) > 0 {
            continue;
        }
        let own: Vec<String> = ttl_names(&folder).into_iter().filter(|n| !map_own.iter().any(|m| m.eq_ignore_ascii_case(n))).collect();
        let name = if cfg.name.is_empty() { folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() } else { cfg.name.clone() };
        let near = dates::between(date, &from) <= RENEW_OPENS + 1;
        let lines = if near && !own.is_empty() {
            read(&from).unwrap_or_default().into_iter().filter(|l| own.iter().any(|o| o.eq_ignore_ascii_case(&l.name))).collect()
        } else {
            Vec::new()
        };
        out.push(MapEvent { name, from, until, own, off: cfg.deactivate_lines.clone(), lines });
    }
    out
}

/// Whether `line` is one of a chrono event's own (`Market::events`): never offered but for it.
fn event_line(events: &[MarketEvent], line: &str) -> bool {
    events.iter().any(|e| e.lines.iter().any(|l| l.eq_ignore_ascii_case(line)))
}

/// (NexiconDK, 31 October 2026) A line one of the map's operators won: it runs it for the term a
/// company would (two weeks: its buses are diesel ones), then it goes out to tender again - or,
/// having bid below its own cost, it gives the line up on `gives_up` (empty: it does not) and the
/// line goes out to tender at once.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct OperatorHold {
    pub line: String,
    pub number: String,
    pub name: String,
    pub from: String,
    pub until: String,
    pub amount: Cents,
    pub own_cost: Cents,
    pub gives_up: String,
}

/// A tender of the market: the tender (its market part: the market's id, the operators, the
/// holder of a concession put out again and the day its new term begins), the companies that
/// applied for it, and what the main server decided.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct MarketTender {
    pub tender: Tender,
    #[serde(default)]
    pub applied: Vec<String>,
    #[serde(default)]
    pub decided: Option<Decided>,
}

/// How a tender of the market was decided: the winning company's id (empty: an operator, or
/// nobody), the winner's name (empty: nobody bid), its ask, and every bidder's points.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Decided {
    pub winner: String,
    pub name: String,
    pub amount: Cents,
    pub standings: Vec<Standing>,
    /// The buses the winning company bid with (its contract's term: `concessions::server_term`).
    pub drive: Drive,
    /// (31 October 2026) An operator won: its own cost of the work a year (it may give the line
    /// up, having bid below it - `OperatorHold`).
    pub own_cost: Cents,
}

/// A company's bid in a tender of the market (a line of the bids' file).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct MarketBid {
    pub tender: u32,
    pub company: String,
    pub name: String,
    /// The company's quality in the authority's eyes when it bid (its first bid's counts).
    pub quality: f64,
    /// (3 November 2026) Its quality's parts then (`viborg::Parts::of`; its first bid's count).
    #[serde(default, skip_serializing_if = "no_parts")]
    pub parts: super::viborg::Parts,
    pub at: i64,
    pub amount: Cents,
    /// The buses it bids with (`concessions::Tender::drive`).
    pub drive: Drive,
}

fn no_parts(p: &super::viborg::Parts) -> bool {
    !p.given()
}

/// A company's application for a line of the map (its Lines page's "Apply").
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct MarketAsk {
    pub company: String,
    pub line: String,
    /// (NexiconDK, 3 November 2026) The server's admin's choices (`ServerAsk`; none on a
    /// company's application).
    #[serde(default, skip_serializing_if = "ServerAsk::is_none")]
    pub server: ServerAsk,
}

/// (NexiconDK, 3 November 2026) How the server's admin puts a line out (`ask_as_server`): when its
/// bidding opens and closes (the server's minutes, `clock::moment`; 0: now, and the usual three
/// days), its cap a timetable hour (euro cents; 0: the settings'), and - a direct award - the
/// company that runs it without bidding (its id and name) and the buses (its term).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ServerAsk {
    pub opens_at: i64,
    pub closes_at: i64,
    pub cap_hour: Cents,
    pub direct: String,
    pub direct_name: String,
    pub drive: Drive,
}

impl ServerAsk {
    fn is_none(&self) -> bool {
        *self == ServerAsk::default()
    }
}

/// (NexiconDK, 3 November 2026) The tenders' rules, added to a `company-server.cfg` written before
/// them (`main_server`). `cap_per_hour` is in the fines' part (`server_fines::FINES_CFG`): a cap
/// of its own there now counts as it stands, 0 is the cost price by `tender_cap_factor`.
pub const TENDER_CFG: &str = "
# the tenders: the cap is the authority's cost price by this when cap_per_hour is 0; a bid under
# this share of the cost price is abnormally low (per cent); the operators bidding in a tender
# (empty: as the rules draw them, 0: none); a new round of lines every so many weeks, and how many
# lines (empty: the rules' and one more a company)
tender_cap_factor = 1.5
tender_low_share = 85
tender_operators =
tender_round_weeks = 4
tender_lines_per_round =
# how a tender is awarded: viborg (price 50 %, quality 50 %: the operating quality 25 %, the
# vehicles 10 %, the implementation plan 10 %, the working environment 5 %) or lowest (price alone)
tender_award = viborg
";

/// (NexiconDK, 3 November 2026) The tenders' rules of the server's settings (`company-server.cfg`,
/// the admin panel's "Rules and economy"): the cap a timetable hour (euro cents; 0: the cost
/// price by `cap_factor`), the share of the cost price under which a bid is abnormally low, how
/// many of the map's operators bid in a tender (None: as Normal's rules draw them), and the
/// rounds of new lines - every so many weeks, so many lines (None: Normal's offers and one more a
/// company).
#[derive(Clone, Debug, PartialEq)]
pub struct TenderCfg {
    pub cap_hour: Cents,
    pub cap_factor: f64,
    pub low_share: f64,
    pub operators: Option<i64>,
    pub round_weeks: i64,
    pub per_round: Option<usize>,
    /// (3 November 2026) How the new tenders are awarded (`tender_award`).
    pub award: super::viborg::Award,
}

impl Default for TenderCfg {
    fn default() -> TenderCfg {
        TenderCfg { cap_hour: 0, cap_factor: super::sealed::CAP_SHARE, low_share: super::sealed::LOW_SHARE, operators: None, round_weeks: cn::ROUND_WEEKS, per_round: None, award: super::viborg::Award::Viborg }
    }
}

impl TenderCfg {
    /// The settings' keys: `cap_per_hour` (kroner, 0 or none: the cost price by
    /// `tender_cap_factor`), `tender_low_share` (per cent), `tender_operators` (a number; empty or
    /// "auto": the rules'), `tender_round_weeks`, `tender_lines_per_round` (empty: the rules').
    pub fn of_keys(kv: &std::collections::HashMap<String, String>) -> TenderCfg {
        let d = TenderCfg::default();
        let num = |k: &str| kv.get(k).and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok()).filter(|x| x.is_finite());
        TenderCfg {
            cap_hour: num("cap_per_hour").filter(|x| *x > 0.0).map_or(0, super::server_fines::Fines::cents),
            cap_factor: num("tender_cap_factor").filter(|x| *x >= 1.0).map_or(d.cap_factor, |x| x.min(10.0)),
            low_share: num("tender_low_share").filter(|x| *x > 0.0 && *x < 100.0).map_or(d.low_share, |x| x / 100.0),
            operators: num("tender_operators").map(|x| (x.round() as i64).clamp(0, 6)),
            round_weeks: num("tender_round_weeks").map_or(d.round_weeks, |x| (x.round() as i64).clamp(1, 52)),
            per_round: num("tender_lines_per_round").map(|x| (x.round() as i64).clamp(0, 50) as usize),
            award: kv.get("tender_award").map_or(d.award, |v| super::viborg::Award::of(v)),
        }
    }

    /// Of the server's folder `data`.
    pub fn of(data: &Path) -> TenderCfg {
        TenderCfg::of_keys(&super::server_fines::settings(data))
    }
}

/// The market of a map (`maps/<folder>/global.cfg`): its folder, lower case.
pub fn key_of(map: &str) -> String {
    online::map_folder(map)
}

pub fn dir(data: &Path) -> PathBuf {
    data.join("market")
}

fn file(data: &Path, key: &str) -> PathBuf {
    dir(data).join(format!("{key}.json"))
}

fn bids_file(data: &Path, key: &str) -> PathBuf {
    dir(data).join(format!("{key}.bids.jsonl"))
}

fn asks_file(data: &Path, key: &str) -> PathBuf {
    dir(data).join(format!("{key}.asks.jsonl"))
}

/// The market of map `key` (None: the main server has not made it yet).
pub fn load(data: &Path, key: &str) -> Option<Market> {
    serde_json::from_slice(&std::fs::read(file(data, key)).ok()?).ok()
}

/// The market written whole, through a file beside it.
pub fn save(data: &Path, m: &Market) -> Result<()> {
    let path = file(data, &m.key);
    std::fs::create_dir_all(dir(data)).with_context(|| format!("cannot make {}", dir(data).display()))?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(m)?).with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, &path).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

/// The weeks of the market's open tenders that have none (`Tender::week`: their tours Monday to
/// Friday, Saturday and Sunday on the companies' pages), from the map's timetable of the week of
/// the server's `date` - each day read by `read` (None: it could not be read; tried again the
/// next day). Whether the market changed.
pub fn fill_weeks(data: &Path, key: &str, date: &str, read: &mut dyn FnMut(&str) -> Option<Vec<LineInfo>>) -> bool {
    let Some(mut m) = load(data, key) else { return false };
    // (a special service's week is its event's days: made with it - `offer`)
    let missing: Vec<usize> = m.tenders.iter().enumerate().filter(|(_, t)| t.decided.is_none() && t.tender.week.is_none_or(|w| !w.by_day) && t.tender.market.as_ref().is_none_or(|x| x.event.is_empty())).map(|(i, _)| i).collect();
    if missing.is_empty() || m.weeks_failed == date {
        return false;
    }
    let Some(day) = dates::parse(date) else { return false };
    let monday = day - dates::weekday(day) as i64;
    let mut days = Vec::new();
    for k in 0..7 {
        match read(&dates::fmt(monday + k)) {
            Some(d) => days.push(d),
            None => {
                m.weeks_failed = date.to_string();
                return save(data, &m).is_ok();
            }
        }
    }
    for i in missing {
        let w = cn::week_of(&days, &m.tenders[i].tender.line);
        m.tenders[i].tender.week = Some(w);
    }
    save(data, &m).is_ok()
}

/// A JSON line added to a file of the market (made when missing).
fn append(path: &Path, line: &str) -> Result<()> {
    use std::io::Write;
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).with_context(|| format!("cannot make {}", d.display()))?;
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).with_context(|| format!("cannot write {}", path.display()))?;
    f.write_all(format!("{line}\n").as_bytes())?;
    Ok(())
}

/// The whole lines of a file of the market from byte `from` on, and where they end.
fn lines_from<T: serde::de::DeserializeOwned>(path: &Path, from: u64) -> (Vec<T>, u64) {
    let Ok(bytes) = std::fs::read(path) else { return (Vec::new(), from) };
    // (a file made anew, shorter: from its start)
    let start = if (from as usize) > bytes.len() { 0 } else { from as usize };
    let rest = &bytes[start..];
    let whole = rest.iter().rposition(|b| *b == b'\n').map_or(0, |k| k + 1);
    let items = String::from_utf8_lossy(&rest[..whole]).lines().filter_map(|l| serde_json::from_str::<T>(l).ok()).collect();
    (items, (start + whole) as u64)
}

/// Every bid of the market of map `key`, as they came.
pub fn bids(data: &Path, key: &str) -> Vec<MarketBid> {
    lines_from(&bids_file(data, key), 0).0
}

/// A company's bid into the market (`concessions::bid` took it first: it leads as the company
/// sees the auction).
pub fn post_bid(data: &Path, key: &str, b: &MarketBid) -> Result<()> {
    append(&bids_file(data, key), &serde_json::to_string(b)?)
}

/// Why an application is refused.
pub const HELD: &str = "Another company of the map runs this line: it goes out to tender again towards the end of its concession.";
pub const ASKED: &str = "The line goes out to tender in a moment: its auction is on the Concessions page.";
pub const OPERATOR_HELD: &str = "One of the map's operators runs this line on the contract it won: it goes out to tender again when that ends.";
/// (1 November 2026) A company told that an event takes a line of its off (`sync`).
pub const EVENT_OFF_DAY: &str = "%{event} takes your line %{n} off on %{date}: it does not run that day - nothing to drive, nothing paid, no fines.";
pub const EVENT_OFF_DAYS: &str = "%{event} takes your line %{n} off for %{days} days from %{date}: it does not run those days - nothing to drive, nothing paid, no fines.";
/// (1 November 2026) A tender the server's admin called off (the admin panel: `server_admin`).
pub const CALLED_OFF: &str = "The tender of line %{n} is called off by the server: its fee is paid back.";
/// (1 November 2026)
pub const EVENT_LINE: &str = "This line runs only on an event of the map's: it goes out to tender before the event, for its days.";

/// Company `c` applies for line `l` of its map (the Lines page's "Apply" on the server): the main
/// server puts it out to tender at once (`ASKED`), unless another company runs it (`HELD`) - its
/// plans for the world say which lines each runs (`plan::live_plans`) - or the company does.
pub fn ask(data: &Path, c: &Company, l: &LineInfo) -> Result<&'static str, &'static str> {
    if c.lines.iter().any(|x| x.name.eq_ignore_ascii_case(&l.name)) {
        return Err("The company runs this line already.");
    }
    let key = key_of(&c.map);
    if load(data, &key).is_some_and(|m| event_line(&m.events, &l.name)) {
        return Err(EVENT_LINE);
    }
    if !cn::is_line(c, l) {
        return Err("This timetable carries no passengers of its own: depot runs, empty runs or other traffic.");
    }
    if super::plan::live_plans(data).iter().any(|p| p.company != c.id && key_of(&p.map) == key && p.has_line(&l.name)) {
        return Err(HELD);
    }
    if load(data, &key).is_some_and(|m| m.held.iter().any(|h| h.line.eq_ignore_ascii_case(&l.name))) {
        return Err(OPERATOR_HELD);
    }
    let a = MarketAsk { company: c.id.clone(), line: l.name.clone(), server: ServerAsk::default() };
    append(&asks_file(data, &key), &serde_json::to_string(&a).map_err(|_| "The application could not be written.")?).map_err(|_| "The application could not be written.")?;
    Ok(ASKED)
}

/// (1 November 2026) The server's admin puts line `line` of map `key` out to tender now (the
/// admin panel): an application of the server's own, which the main server takes at its next step
/// as a company's (`offer`: not when a company or an operator runs it, nor an event's line).
///
/// (3 November 2026) With the admin's choices (`ServerAsk`): when it opens and closes, its cap, or
/// a direct award to a company of the map.
pub fn ask_as_server(data: &Path, key: &str, line: &str, server: ServerAsk) -> Result<()> {
    let a = MarketAsk { company: SERVER_ASKS.to_string(), line: line.trim().to_string(), server };
    append(&asks_file(data, key), &serde_json::to_string(&a)?)
}

/// Who asks for a line the server's admin put out (`ask_as_server`).
pub const SERVER_ASKS: &str = "(server)";

// --- the main server's side -------------------------------------------------------------------

/// What the main server reads of a map for its market (only when it is needed: a new day of the
/// server's, an application): the map's lines of the day, its companies, and a company of the map
/// to price the tenders with (`template`: founded on the server's date, of no one).
pub struct MapNow {
    pub lines: Vec<LineInfo>,
    pub companies: Vec<Company>,
    pub template: Company,
    /// (1 November 2026) The map's chrono events not over (`map_events`).
    pub events: Vec<MapEvent>,
}

/// A company to price a map's tenders with: new, of the map, on the server's date, Normal - in the
/// country most of the map's `companies` play in and at their prices (31 October 2026: the map's
/// guessed country - "International", a UK operator's costs, for a map it did not know - priced
/// the work below what the companies could run it for; none: the map's).
pub fn template(map: &str, date: &str, companies: &[Company]) -> Company {
    let mut countries: Vec<(&str, usize)> = Vec::new();
    for c in companies.iter().filter(|c| !c.country.is_empty()) {
        match countries.iter_mut().find(|x| x.0 == c.country) {
            Some(x) => x.1 += 1,
            None => countries.push((&c.country, 1)),
        }
    }
    countries.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let country = countries.first().map(|x| x.0.to_string()).unwrap_or_default();
    let mut t = super::found(&super::Founding { name: "Market".into(), date: date.to_string(), map: map.to_string(), game_mode: GameMode::Normal, country, ..Default::default() }, "");
    if !companies.is_empty() {
        t.price_index = (companies.iter().map(|c| c.price_index.max(0.1)).sum::<f64>() / companies.len() as f64).max(t.price_index);
    }
    t
}

/// The market of map `key` (`map`) a step on, at the server's `now` (minutes, `clock::moment`) on
/// its `date`: the companies' applications put out to tender at once, a new round every four
/// weeks, the renewals of the companies' concessions ending soon, and every tender whose last
/// minute is over decided (`decide`). `read` reads the map now (None: it could not be read - tried
/// again at the next step). What to say in the main server's log.
pub fn step(data: &Path, key: &str, map: &str, date: &str, now: i64, read: &mut dyn FnMut() -> Option<MapNow>) -> Vec<String> {
    let mut said = Vec::new();
    let mut m = load(data, key).unwrap_or_else(|| Market { key: key.to_string(), map: map.to_string(), round: -1, rivals: cn::make_rivals(Rng::of(&[key, "market rivals"], 0)), ..Default::default() });
    let before = m.clone();
    let (asks, asks_to): (Vec<MarketAsk>, u64) = lines_from(&asks_file(data, key), m.asks_read);
    if m.day != date || !asks.is_empty() {
        if let Some(now_map) = read() {
            offer(&mut m, &now_map, &asks, date, now, &mut said, &TenderCfg::of(data));
            m.asks_read = asks_to;
            m.day = date.to_string();
        }
    }
    // (the tenders whose last minute is over: decided, every bid in)
    let due: Vec<usize> = m.tenders.iter().enumerate().filter(|(_, t)| t.decided.is_none() && t.tender.closes_at + CLOSE_AFTER <= now).map(|(i, _)| i).collect();
    if !due.is_empty() {
        let all = bids(data, key);
        for i in due {
            let d = decide(&m.tenders[i].tender, &all);
            let t = &m.tenders[i].tender;
            let hour = |a: Cents| cn::per_hour(t, a) / 100.0;
            said.push(match (&d.name, d.winner.is_empty()) {
                (n, _) if n.is_empty() => format!("{map}: line {} - no valid bid", t.number),
                (n, false) => format!("{map}: line {} won by the company {n} with an ask of {:.2} a timetable hour", t.number, hour(d.amount)),
                (n, true) => format!("{map}: line {} went to the operator {n} ({:.2} a timetable hour, its own cost {:.2})", t.number, hour(d.amount), hour(d.own_cost)),
            });
            // (an operator's: it holds the line for its term)
            if d.winner.is_empty() && !d.name.is_empty() {
                let from = t.market.as_ref().map(|x| x.from.clone()).filter(|f| !f.is_empty()).unwrap_or_else(|| dates::add(&t.closes, 1));
                // (a special service: the event's days alone)
                let until = t.market.as_ref().map(|x| x.until.clone()).filter(|u| dates::parse(u).is_some()).unwrap_or_else(|| cn::server_term(Drive::Diesel, &from));
                let days = dates::between(&from, &until) + 1;
                // (a special service is run to its end: its days are few)
                let event = t.market.as_ref().is_some_and(|x| !x.event.is_empty());
                let gives = if event { String::new() } else { super::sealed::gives_up(d.amount, d.own_cost, days, &[key, "gives up", &t.line, &from]).map(|k| dates::add(&from, k)).unwrap_or_default() };
                m.held.retain(|h| !h.line.eq_ignore_ascii_case(&t.line));
                m.held.push(OperatorHold { line: t.line.clone(), number: t.number.clone(), name: d.name.clone(), from, until, amount: d.amount, own_cost: d.own_cost, gives_up: gives });
            }
            m.tenders[i].decided = Some(d);
        }
    }
    if m != before {
        if let Err(e) = save(data, &m) {
            said.push(format!("{map}: its tenders not written: {e:#}"));
        }
    }
    said
}

/// The open tender of a line in the market.
fn open_of<'a>(m: &'a Market, line: &str) -> Option<&'a MarketTender> {
    m.tenders.iter().find(|t| t.decided.is_none() && t.tender.line.eq_ignore_ascii_case(line))
}

/// A new tender of the market, its auction from `opens_at` to `closes_at`: a concession put out
/// again (`holder` its company, its new term from `from`), or a line no company runs.
fn add(m: &mut Market, c: &Company, l: &LineInfo, opens_at: i64, closes_at: i64, holder: Option<(&str, &str)>, cfg: &TenderCfg) -> u32 {
    m.counter += 1;
    let id = m.counter;
    let pool: Vec<u32> = m.rivals.iter().map(|r| r.id).collect();
    // (3 November 2026: as many of the map's operators as the server's admin wants)
    let rivals = cfg.operators.map_or(cn::rules(GameMode::Normal).rivals, |n| (n, n));
    let mut t = cn::make_tender(c, l, id, opens_at, holder.is_some(), &m.key, &pool, rivals);
    t.closes_at = closes_at.max(opens_at + 60);
    t.closes = clock::date_of(t.closes_at);
    // (the cap: the cost price by the settings' factor - `sealed::CAP_SHARE` - or, 3 November
    // 2026, the server's own a timetable hour when its admin set one: then that, whatever the cost
    // price - before, his was only a ceiling over the factor's and changed nothing)
    t.reference = super::sealed::cap_set(t.cost, cn::of_hour(&t, cfg.cap_hour as f64), cfg.cap_factor);
    t.market = Some(Shared { key: m.key.clone(), id, rivals: m.rivals.clone(), others: Vec::new(), holder: holder.map(|h| h.0.to_string()).unwrap_or_default(), from: holder.map(|h| h.1.to_string()).unwrap_or_default(), low_share: cfg.low_share, award: cfg.award, ..Default::default() });
    m.tenders.push(MarketTender { tender: t, applied: Vec::new(), decided: None });
    id
}

/// What the market offers now: the applications, a new round, the renewals.
fn offer(m: &mut Market, now_map: &MapNow, asks: &[MarketAsk], date: &str, now: i64, said: &mut Vec<String>, cfg: &TenderCfg) {
    let MapNow { lines, companies, template, events } = now_map;
    let held = |line: &str| companies.iter().find(|c| c.lines.iter().any(|x| x.name.eq_ignore_ascii_case(line)));
    // (1 November 2026: the chrono events' own lines go out with their events alone)
    m.events = events.iter().map(|e| MarketEvent { name: e.name.clone(), from: e.from.clone(), until: e.until.clone(), lines: e.own.clone(), off: e.off.clone() }).collect();
    m.lines = lines.iter().filter(|l| cn::is_line(template, l)).map(|l| l.name.clone()).collect();
    let of_event = |line: &str| events.iter().any(|e| e.own.iter().any(|o| o.eq_ignore_ascii_case(line)));
    // the applications: at once
    for a in asks {
        let Some(l) = lines.iter().find(|l| l.name.eq_ignore_ascii_case(&a.line)) else {
            if a.company == SERVER_ASKS {
                said.push(format!("{}: the server's admin asked for line {} - the map has no such line", m.map, a.line));
            }
            continue;
        };
        if of_event(&l.name) {
            said.push(format!("{}: the application of {} for line {} - an event's line", m.map, a.company, l.name));
            continue;
        }
        if let Some(c) = held(&l.name) {
            said.push(format!("{}: the application of {} for line {} - {} runs it", m.map, a.company, l.name, c.name));
            continue;
        }
        if let Some(h) = m.held.iter().find(|h| h.line.eq_ignore_ascii_case(&l.name)) {
            said.push(format!("{}: the application of {} for line {} - the operator {} runs it until {}", m.map, a.company, l.name, h.name, h.until));
            continue;
        }
        if !cn::is_line(template, l) {
            continue;
        }
        let x = &a.server;
        let id = match open_of(m, &l.name) {
            // (3 November 2026) a direct award of the server's admin: decided at once for the
            // company he named, at the cap - no bids, no operators; the line's open tender, if it
            // has one, ends so
            open if !x.direct.is_empty() => {
                let id = match open {
                    Some(t) => t.tender.id,
                    None => add(m, template, l, now, now + 1, None, cfg),
                };
                if let Some(t) = m.tenders.iter_mut().find(|t| t.tender.id == id) {
                    t.tender.closes_at = t.tender.opens_at.min(now) + 1;
                    t.tender.closes = clock::date_of(t.tender.closes_at);
                    if x.cap_hour > 0 {
                        t.tender.reference = cn::of_hour(&t.tender, x.cap_hour as f64);
                    }
                    t.tender.drive = x.drive;
                    t.tender.bidders.clear();
                    if let Some(s) = t.tender.market.as_mut() {
                        (s.direct, s.direct_name) = (x.direct.clone(), x.direct_name.clone());
                    }
                }
                said.push(format!("{}: line {} awarded directly to {} by the server's admin", m.map, l.name, if x.direct_name.is_empty() { &x.direct } else { &x.direct_name }));
                id
            }
            Some(t) => t.tender.id,
            None => {
                // (the admin's own times and cap, when he chose them)
                let opens = if x.opens_at > 0 { x.opens_at.max(now) } else { now };
                let closes = if x.closes_at > opens { x.closes_at } else { opens + TENDER_DAYS * clock::DAY };
                let id = add(m, template, l, opens, closes, None, cfg);
                if x.cap_hour > 0 {
                    if let Some(t) = m.tenders.iter_mut().find(|t| t.tender.id == id) {
                        t.tender.reference = cn::of_hour(&t.tender, x.cap_hour as f64);
                    }
                }
                said.push(format!("{}: line {} out to tender from {} {} to {} {}, on the application of {}", m.map, l.name, clock::date_of(opens), clock::hhmm(opens), clock::date_of(closes), clock::hhmm(closes), a.company));
                id
            }
        };
        if let Some(t) = m.tenders.iter_mut().find(|t| t.tender.id == id) {
            if !t.applied.contains(&a.company) {
                t.applied.push(a.company.clone());
            }
        }
    }
    if m.day == date {
        return;
    }
    // (31 October 2026) the operators' lines: one that gives its line up today (it bid below its
    // own cost) puts it out to tender at once; a term ending goes out again as a company's does
    for h in std::mem::take(&mut m.held) {
        if !h.gives_up.is_empty() && dates::between(&h.gives_up, date) >= 0 {
            match lines.iter().find(|l| l.name.eq_ignore_ascii_case(&h.line)).filter(|_| open_of(m, &h.line).is_none()) {
                Some(l) => {
                    let id = add(m, template, l, now, now + TENDER_DAYS * clock::DAY, None, cfg);
                    if let Some(x) = m.tenders.iter_mut().find(|t| t.tender.id == id).and_then(|t| t.tender.market.as_mut()) {
                        x.failed = h.name.clone();
                    }
                    said.push(format!("{}: the operator {} gives line {} up (it bid below its own cost): out to tender at once", m.map, h.name, h.line));
                }
                None => said.push(format!("{}: the operator {} gives line {} up", m.map, h.name, h.line)),
            }
            continue;
        }
        // (an event's line: its days over, it goes - never out again but with its next event)
        if of_event(&h.line) {
            if dates::between(&h.until, date) <= 0 {
                m.held.push(h);
            }
            continue;
        }
        if dates::between(&h.until, date) > 0 {
            continue;
        }
        let from = dates::add(&h.until, 1);
        let decided = m.tenders.iter().any(|t| t.tender.renewal && t.tender.line.eq_ignore_ascii_case(&h.line) && t.tender.market.as_ref().is_some_and(|x| x.from == from));
        if dates::between(date, &from) <= RENEW_OPENS + 1 && !decided && open_of(m, &h.line).is_none() {
            if let Some(l) = lines.iter().find(|l| l.name.eq_ignore_ascii_case(&h.line)) {
                let mut rng = Rng::of(&[&m.key, "renewal", &h.line], dates::parse(&from).unwrap_or(0));
                let minute = 9 * 60 + rng.int(0, 24) * 15;
                let opens = clock::moment(&dates::add(&from, -RENEW_OPENS), minute).max(now + 30);
                let closes = clock::moment(&dates::add(&from, -RENEW_DECIDED), minute).max(opens + 60);
                add(m, template, l, opens, closes, Some(("", &from)), cfg);
                said.push(format!("{}: the operator {}'s contract on line {} out to tender again from {} {}", m.map, h.name, l.name, clock::date_of(opens), clock::hhmm(opens)));
            }
        }
        m.held.push(h);
    }
    // a new round: lines no company runs
    let round = dates::week_of(date) / cfg.round_weeks.max(1);
    if m.round != round {
        m.round = round;
        m.tenders.retain(|t| t.decided.is_none() || dates::between(&t.tender.closes, date) < KEEP_DAYS);
        let mut free: Vec<&LineInfo> = lines
            .iter()
            .filter(|l| !crate::lines::is_own_file(&l.name))
            .filter(|l| !of_event(&l.name))
            .filter(|l| held(&l.name).is_none())
            .filter(|l| !m.held.iter().any(|h| h.line.eq_ignore_ascii_case(&l.name)))
            .filter(|l| open_of(m, &l.name).is_none())
            .filter(|l| cn::is_line(template, l))
            .collect();
        let mut rng = Rng::of(&[&m.key, "round"], round);
        let n = cfg.per_round.unwrap_or(cn::rules(GameMode::Normal).offers + companies.len());
        let mut chosen = Vec::new();
        while chosen.len() < n && !free.is_empty() {
            let k = rng.int(0, free.len() as i64 - 1) as usize;
            chosen.push(free.remove(k.min(free.len() - 1)).clone());
        }
        for (k, l) in chosen.into_iter().enumerate() {
            // (one a day, in the office hours, half an hour from now at the soonest; three days each)
            let at = clock::moment(&dates::add(date, k as i64), 9 * 60 + rng.int(0, 24) * 15).max(now + 30);
            add(m, template, &l, at, at + TENDER_DAYS * clock::DAY, None, cfg);
            said.push(format!("{}: line {} out to tender from {} {}", m.map, l.name, clock::date_of(at), clock::hhmm(at)));
        }
    }
    // the renewals: a company's concession whose new term begins within a week, open to every
    // company of the map - from six days before it to three days before (`RENEW_OPENS`,
    // `RENEW_DECIDED`), its last minutes that day's office hours
    for c in companies {
        for h in &c.concessions.held {
            let from = dates::add(&h.until, 1);
            if dates::between(date, &from) > RENEW_OPENS + 1 || open_of(m, &h.line).is_some() || of_event(&h.line) {
                continue;
            }
            // (decided already for this term)
            if m.tenders.iter().any(|t| t.tender.renewal && t.tender.line.eq_ignore_ascii_case(&h.line) && t.tender.market.as_ref().is_some_and(|x| x.from == from)) {
                continue;
            }
            let Some(l) = lines.iter().find(|l| l.name.eq_ignore_ascii_case(&h.line)) else { continue };
            let mut rng = Rng::of(&[&m.key, "renewal", &h.line], dates::parse(&from).unwrap_or(0));
            let minute = 9 * 60 + rng.int(0, 24) * 15;
            let opens = clock::moment(&dates::add(&from, -RENEW_OPENS), minute).max(now + 30);
            let closes = clock::moment(&dates::add(&from, -RENEW_DECIDED), minute);
            add(m, template, l, opens, closes, Some((&c.id, &from)), cfg);
            said.push(format!("{}: the concession of {} on line {} out to tender again from {} {} to {}, open to every company", m.map, c.name, l.name, clock::date_of(opens), clock::hhmm(opens), clock::date_of(closes)));
        }
    }
    // (1 November 2026) the special services: an event's own lines out to tender for its days -
    // from six days before its first to three days before, as a renewal; too late for that (the
    // server saw it late), the map's operators run them
    for e in events {
        if dates::between(date, &e.from) > RENEW_OPENS + 1 || dates::between(date, &e.from) < 0 {
            continue;
        }
        for l in e.lines.iter().filter(|l| cn::is_event_line(template, l)) {
            // (offered for this event already, or held over its first day by a company or an operator)
            if m.tenders.iter().any(|t| t.tender.line.eq_ignore_ascii_case(&l.name) && t.tender.market.as_ref().is_some_and(|x| x.from == e.from && x.event == e.name)) {
                continue;
            }
            let covers = |from: &str, until: &str| dates::between(from, &e.from) >= 0 && dates::between(&e.from, until) >= 0;
            if companies.iter().any(|c| c.concessions.held.iter().any(|h| h.line.eq_ignore_ascii_case(&l.name) && covers(&h.from, &h.until))) || m.held.iter().any(|h| h.line.eq_ignore_ascii_case(&l.name) && covers(&h.from, &h.until)) {
                continue;
            }
            let mut rng = Rng::of(&[&m.key, "event", &l.name], dates::parse(&e.from).unwrap_or(0));
            let minute = 9 * 60 + rng.int(0, 24) * 15;
            let opens = clock::moment(&dates::add(&e.from, -RENEW_OPENS), minute).max(now + 30);
            let closes = clock::moment(&dates::add(&e.from, -RENEW_DECIDED), minute);
            if closes < opens + 12 * 60 {
                said.push(format!("{}: the event '{}' too near for a tender of its line {}: the map's operators run it", m.map, e.name, l.name));
                continue;
            }
            let id = add(m, template, l, opens, closes, None, cfg);
            if let Some(t) = m.tenders.iter_mut().find(|t| t.tender.id == id) {
                t.tender.week = Some(cn::week_of(std::slice::from_ref(&e.lines), &l.name));
                if let Some(x) = t.tender.market.as_mut() {
                    (x.from, x.until, x.event) = (e.from.clone(), e.until.clone(), e.name.clone());
                }
            }
            said.push(format!("{}: the event '{}' ({} to {}): its line {} out to tender from {} {} to {}", m.map, e.name, e.from, e.until, l.name, clock::date_of(opens), clock::hhmm(opens), clock::date_of(closes)));
        }
    }
}

/// The companies' bids of a tender of the market in their one order (by their ids): (id, name,
/// quality at its first bid, bids).
fn humans(t: &Tender, all: &[MarketBid]) -> Vec<(String, String, f64, Vec<(i64, Cents)>)> {
    humans_with(t, all).into_iter().map(|h| (h.0, h.1, h.2, h.3)).collect()
}

/// A company's bids in a tender: its id, name, quality and its quality's parts (its first
/// bid's), its asks (when, how much) and the buses of its last.
type Human = (String, String, f64, Vec<(i64, Cents)>, Drive, super::viborg::Parts);

/// `humans`, each with the buses of his last bid and his quality's parts.
fn humans_with(t: &Tender, all: &[MarketBid]) -> Vec<Human> {
    let id = t.market.as_ref().map_or(t.id, |m| m.id);
    let mut out: Vec<Human> = Vec::new();
    for b in all.iter().filter(|b| b.tender == id) {
        match out.iter_mut().find(|h| h.0 == b.company) {
            Some(h) => {
                h.1 = b.name.clone();
                h.3.push((b.at, b.amount));
                h.4 = b.drive;
            }
            None => out.push((b.company.clone(), b.name.clone(), b.quality, vec![(b.at, b.amount)], b.drive, b.parts)),
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// A tender of the market decided by every company's bids (`all`) against its operators: sealed
/// (`sealed`) - each company's last bid before the close and each operator's, opened together and
/// scored; the best valid total wins.
pub fn decide(t: &Tender, all: &[MarketBid]) -> Decided {
    // (3 November 2026: awarded directly by the server's admin - no bids opened)
    if let Some(x) = t.market.as_ref().filter(|x| !x.direct.is_empty()) {
        let name = if x.direct_name.is_empty() { x.direct.clone() } else { x.direct_name.clone() };
        return Decided { winner: x.direct.clone(), name, amount: t.reference, drive: t.drive, ..Decided::default() };
    }
    let hs = humans_with(t, all);
    let ops = super::sealed::operator_bids(t);
    // (the companies' first, then the operators': an index into both)
    let mut opened: Vec<super::sealed::Opened> = Vec::new();
    let mut last: Vec<(usize, Cents, Drive)> = Vec::new();
    for (k, h) in hs.iter().enumerate() {
        if let Some(&(_, amount)) = h.3.iter().filter(|o| o.0 <= t.closes_at).last() {
            opened.push(super::sealed::Opened { who: k, name: h.1.clone(), quality: h.2, amount, parts: h.5 });
            last.push((k, amount, h.4));
        }
    }
    for (k, o) in ops.iter().enumerate() {
        opened.push(super::sealed::Opened { who: hs.len() + k, name: o.name.clone(), quality: o.quality, amount: o.amount, parts: o.parts });
    }
    let (winner, rows) = super::sealed::score(t, &opened);
    let standings: Vec<Standing> = rows.into_iter().map(|r| r.1).collect();
    match winner {
        Some(w) if w < hs.len() => Decided {
            winner: hs[w].0.clone(),
            name: hs[w].1.clone(),
            amount: last.iter().find(|l| l.0 == w).map_or(0, |l| l.1),
            standings,
            drive: last.iter().find(|l| l.0 == w).map_or(Drive::Diesel, |l| l.2),
            own_cost: 0,
        },
        Some(w) => {
            let o = &ops[w - hs.len()];
            Decided { winner: String::new(), name: o.name.clone(), amount: o.amount, standings, drive: Drive::Diesel, own_cost: o.own_cost }
        }
        None => Decided { standings, ..Decided::default() },
    }
}

// --- the server's admin and a company's contract (3 November 2026) ----------------------------------

/// The server's admin moves the last day of a company's contract on a line (the admin panel's
/// contracts): later (an extension), or earlier - its renewal then opens as any concession's does
/// (`offer`, `RENEW_OPENS` days before the new term), so the panel never asks for a day sooner
/// than that from the server's date. Taken by the company's process (`take_contract_orders`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct ContractOrder {
    pub line: String,
    pub until: String,
}

/// The admin's orders to company `id` (`<folder>/admin-orders/<id>.jsonl`).
pub fn contract_orders_file(data: &Path, id: &str) -> PathBuf {
    data.join("admin-orders").join(format!("{id}.jsonl"))
}

/// The admin orders company `id`'s contract on `line` to end on `until`.
pub fn order_contract(data: &Path, id: &str, line: &str, until: &str) -> Result<()> {
    let o = ContractOrder { line: line.trim().to_string(), until: until.trim().to_string() };
    append(&contract_orders_file(data, id), &serde_json::to_string(&o)?)
}

/// What the company is told of it.
pub const CONTRACT_MOVED: &str = "The authority moved the last day of your contract on line %{n} to %{date}.";

/// The admin's orders to company `c` since its last look: its concession on the line and the
/// contract that ended with it end on the day he chose, told in its news. Whether any came.
pub fn take_contract_orders(data: &Path, c: &mut Company) -> bool {
    let (orders, to): (Vec<ContractOrder>, u64) = lines_from(&contract_orders_file(data, &c.id), c.online.admin_read);
    if to == c.online.admin_read {
        return false;
    }
    c.online.admin_read = to;
    let mut run = Run::default();
    let at = clock::now(c);
    for o in orders.iter().filter(|o| dates::parse(&o.until).is_some()) {
        let Some(h) = c.concessions.held.iter_mut().find(|h| h.line.eq_ignore_ascii_case(&o.line)) else { continue };
        let (old, number) = (std::mem::replace(&mut h.until, o.until.clone()), h.number.clone());
        for k in c.concessions.contracts.iter_mut().filter(|k| k.line.eq_ignore_ascii_case(&o.line) && k.until == old) {
            k.until = o.until.clone();
        }
        clock::tell(c, &mut run, at, CONTRACT_MOVED, vec![clock::arg("n", &number), clock::arg("date", &o.until)], Level::Warn);
    }
    true
}

// --- the companies' side ----------------------------------------------------------------------

/// Company `c` and its map's market on the server's folder `data` (each step of its process): its
/// own tenders of before withdrawn (their fees paid back), the market's open tenders taken among
/// its own, the other companies' bids taken in, and what the main server decided taken (won: the
/// concession and its contract; lost: who won), told in its news. Whether it changed, and whether
/// it won a line (its lines to be taken on: `concessions::take_on`).
pub fn sync(data: &Path, c: &mut Company) -> (bool, bool) {
    if !c.online.on_server {
        return (false, false);
    }
    let key = key_of(&c.map);
    let Some(m) = load(data, &key) else { return (false, false) };
    let before = c.concessions.clone();
    let mut run = Run::default();
    let at = clock::now(c);
    // (its own tenders of before: withdrawn, their fees paid back)
    let own: Vec<Tender> = c.concessions.tenders.iter().filter(|t| t.open() && t.market.is_none()).cloned().collect();
    for t in &own {
        if t.fee_paid {
            let fee = cn::fee(c, t);
            if fee > 0 {
                c.book(BookingKind::Concession, fee, format!("Tender line {} withdrawn: its fee paid back", t.number), false);
            }
        }
        clock::tell(c, &mut run, at, "The tender of line %{n} is withdrawn: on the server the map's tenders are shared by its companies", vec![clock::arg("n", &t.number)], Level::Info);
    }
    c.concessions.tenders.retain(|t| !(t.open() && t.market.is_none()));
    let all = bids(data, &key);
    let mut won = false;
    for mt in &m.tenders {
        let id = mt.tender.id;
        let hs = humans(&mt.tender, &all);
        let others: Vec<OtherBids> = hs.iter().filter(|h| h.0 != c.id).map(|h| OtherBids { company: h.0.clone(), name: h.1.clone(), quality: h.2, offers: h.3.clone() }).collect();
        let mine: Option<Vec<(i64, Cents)>> = hs.iter().find(|h| h.0 == c.id).map(|h| h.3.clone());
        let local = c.concessions.tenders.iter().position(|t| t.market.as_ref().is_some_and(|x| x.key == key && x.id == id));
        let direct = mt.tender.market.as_ref().is_some_and(|x| x.direct == c.id);
        let k = match local {
            Some(k) => k,
            None if mt.decided.is_none() || direct => {
                let mut t = mt.tender.clone();
                c.concessions.counter += 1;
                t.id = c.concessions.counter;
                t.renewal = t.market.as_ref().is_some_and(|x| x.holder == c.id);
                t.fee_paid = mine.is_some();
                t.opened = false;
                t.told = 0;
                c.concessions.tenders.push(t);
                c.concessions.tenders.len() - 1
            }
            None => continue,
        };
        let t = &mut c.concessions.tenders[k];
        // (sealed: the other companies' bids only once opened at the close - 31 October 2026)
        if let Some(x) = t.market.as_mut() {
            x.others = if mt.decided.is_some() { others } else { Vec::new() };
            x.failed = mt.tender.market.as_ref().map(|y| y.failed.clone()).unwrap_or_default();
        }
        // (its times are the market's: the server's operator may move them in the market's file;
        // and its week, once the main server read it - `fill_weeks`)
        if t.open() {
            (t.opens_at, t.closes_at) = (mt.tender.opens_at, mt.tender.closes_at);
            // (3 November 2026: its cap too - the admin may change it - and its limits)
            t.reference = mt.tender.reference;
            if let (Some(x), Some(y)) = (t.market.as_mut(), mt.tender.market.as_ref()) {
                (x.low_share, x.direct, x.direct_name, x.award) = (y.low_share, y.direct.clone(), y.direct_name.clone(), y.award);
            }
            if mt.tender.week.is_some() {
                t.week = mt.tender.week;
            }
        }
        if let Some(mine) = mine {
            t.offers = mine;
        }
        t.applied |= mt.applied.contains(&c.id);
        // (decided by the main server for every company)
        let Some(d) = mt.decided.clone().filter(|_| t.open()) else { continue };
        let (lid, number, renewal, mine_bid) = (t.id, t.number.clone(), t.renewal, !t.offers.is_empty());
        let (line, from) = (t.line.clone(), t.market.as_ref().map(|m| m.from.clone()).unwrap_or_default());
        let me = d.winner == c.id;
        let Some((ev, amount)) = cn::decide_shared(c, lid, me, &d.name, d.amount, d.standings.clone(), d.drive) else { continue };
        let closes = c.concessions.tenders.iter().find(|t| t.id == lid).map_or(at, |t| t.closes_at);
        match ev {
            cn::Event::Won { until, .. } => {
                won = true;
                clock::tell(c, &mut run, closes, "Line %{n} won for %{amount}: the concession runs until %{date}", vec![clock::arg("n", &number), clock::arg("amount", amount), clock::arg("date", until)], Level::Good);
            }
            cn::Event::Lost { winner, .. } if winner.is_empty() => clock::tell(c, &mut run, closes, "Nobody bid for line %{n}", vec![clock::arg("n", &number)], Level::Minor),
            cn::Event::Lost { winner, .. } => {
                // (its concession won by another company: the buses it ran the line with offered
                // to it - `takeover`)
                if renewal && !d.winner.is_empty() && !from.is_empty() {
                    super::takeover::offer(data, c, &d.winner, &line, &number, &from);
                }
                let level = if mine_bid || renewal { Level::Bad } else { Level::Minor };
                let text = if renewal { "Line %{n} went to %{who} for %{amount}: your concession ends with its term" } else { "Line %{n} went to %{who} for %{amount}" };
                clock::tell(c, &mut run, closes, text, vec![clock::arg("n", &number), clock::arg("who", winner), clock::arg("amount", amount)], level);
            }
            cn::Event::Ended { .. } => {}
        }
    }
    // (1 November 2026) an event of the map that takes a line of the company's off: told once, a
    // week before its first day at the soonest
    let today = c.date.clone();
    for e in &m.events {
        if dates::between(&today, &e.from) > RENEW_OPENS + 1 || dates::between(&e.until, &today) > 0 {
            continue;
        }
        for off in &e.off {
            let Some((name, number)) = c.lines.iter().find(|x| x.name.trim().eq_ignore_ascii_case(off.trim())).map(|x| (x.name.clone(), x.number.clone())) else { continue };
            let key = format!("{}|{}|{}", e.name, e.from, name);
            if c.concessions.events_told.contains(&key) {
                continue;
            }
            let days = dates::between(&e.from, &e.until) + 1;
            if days > 1 {
                clock::tell(c, &mut run, at, EVENT_OFF_DAYS, vec![clock::arg("event", &e.name), clock::arg("n", &number), clock::arg("days", days), clock::arg("date", &e.from)], Level::Warn);
            } else {
                clock::tell(c, &mut run, at, EVENT_OFF_DAY, vec![clock::arg("event", &e.name), clock::arg("n", &number), clock::arg("date", &e.from)], Level::Warn);
            }
            c.concessions.events_told.push(key);
        }
    }
    c.concessions.events_told.retain(|k| k.split('|').nth(1).is_some_and(|f| dates::between(f, &today) < KEEP_DAYS));
    // (1 November 2026) an open tender the server's admin called off - gone from the market: gone
    // from the company's, its fee paid back
    let gone = |t: &Tender| t.open() && t.market.as_ref().is_some_and(|x| x.key == key && !m.tenders.iter().any(|mt| mt.tender.id == x.id));
    let off: Vec<Tender> = c.concessions.tenders.iter().filter(|t| gone(t)).cloned().collect();
    for t in &off {
        if t.fee_paid {
            let fee = cn::fee(c, t);
            if fee > 0 {
                c.book(BookingKind::Concession, fee, format!("Tender line {} called off: its fee paid back", t.number), false);
            }
        }
        clock::tell(c, &mut run, at, CALLED_OFF, vec![clock::arg("n", &t.number)], Level::Info);
    }
    c.concessions.tenders.retain(|t| !gone(t));
    // (the decided ones the market no longer keeps go)
    c.concessions.tenders.retain(|t| t.open() || t.market.as_ref().is_none_or(|x| x.key != key || m.tenders.iter().any(|mt| mt.tender.id == x.id)));
    (c.concessions != before || run.told > 0, won)
}

/// A company's bid on a tender (`concessions::bid`), into the market when it is one of the map's
/// (the other companies see it at their next step). The company's id, name and quality go with it.
pub fn bid(data: &Path, c: &mut Company, id: u32, amount: Cents) -> Result<(), &'static str> {
    cn::bid(c, id, amount)?;
    let Some(t) = c.concessions.tenders.iter().find(|t| t.id == id) else { return Ok(()) };
    let Some(m) = t.market.as_ref() else { return Ok(()) };
    let at = t.offers.last().map_or(clock::now(c), |o| o.0);
    // (its first bid's quality counts in the market's auction: `humans`)
    let b = MarketBid { tender: m.id, company: c.id.clone(), name: c.name.clone(), quality: cn::quality_in(c, t), parts: super::viborg::Parts::of(c, t), at, amount, drive: t.drive };
    post_bid(data, &m.key, &b).map_err(|_| "The bid could not be written to the server's tenders.")
}

#[cfg(test)]
mod tests;
