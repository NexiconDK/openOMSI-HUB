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
//! Each company's process keeps a copy of the market's open tenders among its own (`sync`,
//! `concessions::Tender::market`): it bids on them as ever (its pages, its clock telling the bids
//! as they come), its bid goes into the market's bids (`post_bid`,
//! `<folder>/market/<map>.bids.jsonl`) and every other company of the map sees it at its next
//! step; and it takes what the main server decided (won: the concession with its contract; lost:
//! who won). Its own tenders of before the market are withdrawn, their fees paid back.

use super::auction::{self, Human, Who};
use super::clock::{self, Level, Run};
use super::concessions::{self as cn, OtherBids, Rival, Shared, Standing, Tender};
use super::dates;
use super::model::{BookingKind, Cents, Company, GameMode};
use super::online;
use super::rng::Rng;
use crate::LineInfo;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A tender is decided this many minutes after its last minute (every bid in by then).
pub const CLOSE_AFTER: i64 = 1;
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
    pub at: i64,
    pub amount: Cents,
}

/// A company's application for a line of the map (its Lines page's "Apply").
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct MarketAsk {
    pub company: String,
    pub line: String,
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

/// Company `c` applies for line `l` of its map (the Lines page's "Apply" on the server): the main
/// server puts it out to tender at once (`ASKED`), unless another company runs it (`HELD`) - its
/// plans for the world say which lines each runs (`plan::live_plans`) - or the company does.
pub fn ask(data: &Path, c: &Company, l: &LineInfo) -> Result<&'static str, &'static str> {
    if c.lines.iter().any(|x| x.name.eq_ignore_ascii_case(&l.name)) {
        return Err("The company runs this line already.");
    }
    if !cn::is_line(c, l) {
        return Err("This timetable carries no passengers of its own: depot runs, empty runs or other traffic.");
    }
    let key = key_of(&c.map);
    if super::plan::live_plans(data).iter().any(|p| p.company != c.id && key_of(&p.map) == key && p.has_line(&l.name)) {
        return Err(HELD);
    }
    let a = MarketAsk { company: c.id.clone(), line: l.name.clone() };
    append(&asks_file(data, &key), &serde_json::to_string(&a).map_err(|_| "The application could not be written.")?).map_err(|_| "The application could not be written.")?;
    Ok(ASKED)
}

// --- the main server's side -------------------------------------------------------------------

/// What the main server reads of a map for its market (only when it is needed: a new day of the
/// server's, an application): the map's lines of the day, its companies, and a company of the map
/// to price the tenders with (`template`: founded on the server's date, of no one).
pub struct MapNow {
    pub lines: Vec<LineInfo>,
    pub companies: Vec<Company>,
    pub template: Company,
}

/// A company to price a map's tenders with: new, of the map, on the server's date, Normal.
pub fn template(map: &str, date: &str) -> Company {
    super::found(&super::Founding { name: "Market".into(), date: date.to_string(), map: map.to_string(), game_mode: GameMode::Normal, ..Default::default() }, "")
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
            offer(&mut m, &now_map, &asks, date, now, &mut said);
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
            said.push(match (&d.name, d.winner.is_empty()) {
                (n, _) if n.is_empty() => format!("{map}: line {} - nobody bid", m.tenders[i].tender.number),
                (n, false) => format!("{map}: line {} won by the company {n} with an ask of {:.2} a year", m.tenders[i].tender.number, d.amount as f64 / 100.0),
                (n, true) => format!("{map}: line {} went to the operator {n} ({:.2} a year)", m.tenders[i].tender.number, d.amount as f64 / 100.0),
            });
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

/// A new tender of the market, its auction opening at `opens_at`: a concession put out again
/// (`holder` its company, its new term from `from`), or a line no company runs.
fn add(m: &mut Market, c: &Company, l: &LineInfo, opens_at: i64, holder: Option<(&str, &str)>) -> u32 {
    m.counter += 1;
    let id = m.counter;
    let pool: Vec<u32> = m.rivals.iter().map(|r| r.id).collect();
    let mut t = cn::make_tender(c, l, id, opens_at, holder.is_some(), &m.key, &pool, cn::rules(GameMode::Normal).rivals);
    t.market = Some(Shared { key: m.key.clone(), id, rivals: m.rivals.clone(), others: Vec::new(), holder: holder.map(|h| h.0.to_string()).unwrap_or_default(), from: holder.map(|h| h.1.to_string()).unwrap_or_default() });
    m.tenders.push(MarketTender { tender: t, applied: Vec::new(), decided: None });
    id
}

/// What the market offers now: the applications, a new round, the renewals.
fn offer(m: &mut Market, now_map: &MapNow, asks: &[MarketAsk], date: &str, now: i64, said: &mut Vec<String>) {
    let MapNow { lines, companies, template } = now_map;
    let held = |line: &str| companies.iter().find(|c| c.lines.iter().any(|x| x.name.eq_ignore_ascii_case(line)));
    // the applications: at once
    for a in asks {
        let Some(l) = lines.iter().find(|l| l.name.eq_ignore_ascii_case(&a.line)) else { continue };
        if let Some(c) = held(&l.name) {
            said.push(format!("{}: the application of {} for line {} - {} runs it", m.map, a.company, l.name, c.name));
            continue;
        }
        if !cn::is_line(template, l) {
            continue;
        }
        let id = match open_of(m, &l.name) {
            Some(t) => t.tender.id,
            None => {
                let id = add(m, template, l, now, None);
                said.push(format!("{}: line {} out to tender at once, on the application of {}", m.map, l.name, a.company));
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
    // a new round: lines no company runs
    let round = dates::week_of(date) / cn::ROUND_WEEKS;
    if m.round != round {
        m.round = round;
        m.tenders.retain(|t| t.decided.is_none() || dates::between(&t.tender.closes, date) < KEEP_DAYS);
        let mut free: Vec<&LineInfo> = lines
            .iter()
            .filter(|l| !crate::lines::is_own_file(&l.name))
            .filter(|l| held(&l.name).is_none())
            .filter(|l| open_of(m, &l.name).is_none())
            .filter(|l| cn::is_line(template, l))
            .collect();
        let mut rng = Rng::of(&[&m.key, "round"], round);
        let n = cn::rules(GameMode::Normal).offers + companies.len();
        let mut chosen = Vec::new();
        while chosen.len() < n && !free.is_empty() {
            let k = rng.int(0, free.len() as i64 - 1) as usize;
            chosen.push(free.remove(k.min(free.len() - 1)).clone());
        }
        for (k, l) in chosen.into_iter().enumerate() {
            // (one a day, in the office hours, half an hour from now at the soonest)
            let at = clock::moment(&dates::add(date, k as i64), 9 * 60 + rng.int(0, 24) * 15).max(now + 30);
            add(m, template, &l, at, None);
            said.push(format!("{}: line {} out to tender from {} {}", m.map, l.name, clock::date_of(at), clock::hhmm(at)));
        }
    }
    // the renewals: a company's concession ending soon, open to every company of the map
    for c in companies {
        for h in c.concessions.held.iter().filter(|h| dates::between(date, &h.until) <= cn::RENEW_BEFORE) {
            if open_of(m, &h.line).is_some() {
                continue;
            }
            // (decided already in this term)
            if m.tenders.iter().any(|t| t.tender.renewal && t.tender.line.eq_ignore_ascii_case(&h.line) && dates::between(&t.tender.offered, &h.until) <= cn::RENEW_BEFORE) {
                continue;
            }
            let Some(l) = lines.iter().find(|l| l.name.eq_ignore_ascii_case(&h.line)) else { continue };
            // (a fortnight before it ends, a week from now at the soonest)
            let day = dates::add(&h.until, -14);
            let day = if dates::between(date, &day) < 7 { dates::add(date, 7) } else { day };
            let mut rng = Rng::of(&[&m.key, "renewal", &h.line], dates::parse(&day).unwrap_or(0));
            let from = dates::add(&h.until, 1);
            add(m, template, l, clock::moment(&day, 9 * 60 + rng.int(0, 24) * 15), Some((&c.id, &from)));
            said.push(format!("{}: the concession of {} on line {} out to tender again on {day}, open to every company", m.map, c.name, l.name));
        }
    }
}

/// The companies' bids of a tender of the market in their one order (by their ids): (id, name,
/// quality at its first bid, bids).
fn humans(t: &Tender, all: &[MarketBid]) -> Vec<(String, String, f64, Vec<(i64, Cents)>)> {
    let id = t.market.as_ref().map_or(t.id, |m| m.id);
    let mut out: Vec<(String, String, f64, Vec<(i64, Cents)>)> = Vec::new();
    for b in all.iter().filter(|b| b.tender == id) {
        match out.iter_mut().find(|h| h.0 == b.company) {
            Some(h) => {
                h.1 = b.name.clone();
                h.3.push((b.at, b.amount));
            }
            None => out.push((b.company.clone(), b.name.clone(), b.quality, vec![(b.at, b.amount)])),
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// A tender of the market decided by every company's bids (`all`) against its operators.
pub fn decide(t: &Tender, all: &[MarketBid]) -> Decided {
    let hs = humans(t, all);
    let rivals = cn::market_bidders(t);
    let humans: Vec<Human> = hs.iter().map(|h| Human { quality: h.2, offers: &h.3 }).collect();
    let placed = auction::replay_many(&t.lot(), &rivals, &humans, None, t.closes_at);
    let name = |who: Who| match who {
        Who::Other(k) => hs.get(k).map(|h| h.1.clone()).unwrap_or_default(),
        Who::Rival(i) => t.bidders.get(i).and_then(|id| t.market.as_ref()?.rivals.iter().find(|r| r.id == *id)).map(|r| r.name.clone()).unwrap_or_default(),
        Who::Player => String::new(),
    };
    let hq: Vec<f64> = hs.iter().map(|h| h.2).collect();
    let standings = auction::points_many(&placed, &rivals, &hq, None).into_iter().map(|p| Standing { name: name(p.who), amount: p.amount, price: p.price, quality: p.quality, total: p.total }).collect();
    match placed.last() {
        Some(p) => Decided {
            winner: match p.who {
                Who::Other(k) => hs.get(k).map(|h| h.0.clone()).unwrap_or_default(),
                _ => String::new(),
            },
            name: name(p.who),
            amount: p.amount,
            standings,
        },
        None => Decided::default(),
    }
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
        let k = match local {
            Some(k) => k,
            None if mt.decided.is_none() => {
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
        if let Some(x) = t.market.as_mut() {
            x.others = others;
        }
        if let Some(mine) = mine {
            t.offers = mine;
        }
        t.applied |= mt.applied.contains(&c.id);
        // (decided by the main server for every company)
        let Some(d) = mt.decided.clone().filter(|_| t.open()) else { continue };
        let (lid, number, renewal, mine_bid) = (t.id, t.number.clone(), t.renewal, !t.offers.is_empty());
        let me = d.winner == c.id;
        let Some((ev, amount)) = cn::decide_shared(c, lid, me, &d.name, d.amount, d.standings.clone()) else { continue };
        let closes = c.concessions.tenders.iter().find(|t| t.id == lid).map_or(at, |t| t.closes_at);
        match ev {
            cn::Event::Won { until, .. } => {
                won = true;
                clock::tell(c, &mut run, closes, "Line %{n} won for %{amount}: the concession runs until %{date}", vec![clock::arg("n", &number), clock::arg("amount", amount), clock::arg("date", until)], Level::Good);
            }
            cn::Event::Lost { winner, .. } if winner.is_empty() => clock::tell(c, &mut run, closes, "Nobody bid for line %{n}", vec![clock::arg("n", &number)], Level::Minor),
            cn::Event::Lost { winner, .. } => {
                let level = if mine_bid || renewal { Level::Bad } else { Level::Minor };
                let text = if renewal { "Line %{n} went to %{who} for %{amount}: your concession ends with its term" } else { "Line %{n} went to %{who} for %{amount}" };
                clock::tell(c, &mut run, closes, text, vec![clock::arg("n", &number), clock::arg("who", winner), clock::arg("amount", amount)], level);
            }
            cn::Event::Ended { .. } => {}
        }
    }
    // (the easy mode bids by itself: the recommended ask, once, on its own concession put out again
    // and on a line it applied for)
    if super::autopilot::on(c) {
        let now = clock::now(c);
        let due: Vec<u32> = c.concessions.tenders.iter().filter(|t| t.market.is_some() && (t.renewal || t.applied) && t.running(now) && t.offers.is_empty()).map(|t| t.id).collect();
        for id in due {
            let Some(t) = c.concessions.tenders.iter().find(|t| t.id == id).cloned() else { continue };
            let (ask, _) = cn::recommended(c, &t);
            let _ = bid(data, c, id, ask);
        }
    }
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
    let b = MarketBid { tender: m.id, company: c.id.clone(), name: c.name.clone(), quality: cn::quality_in(c, t), at, amount };
    post_bid(data, &m.key, &b).map_err(|_| "The bid could not be written to the server's tenders.")
}

#[cfg(test)]
mod tests;
