//! (NexiconDK, Bus Company Online, decided 3 November 2026) Doubling ("dublering"): an extra bus
//! right behind a departure that is full. As Midttrafik's contracts have it, the authority may
//! order extra journeys - doubling among them - and pays one run with a contract bus that stands
//! free anyway at the contract's own price, one with a bus of no contract by an agreement of its
//! own (Midttrafik's bus contract, 20.2). On the server:
//!
//! 1. **Full.** The day's close counts on each departure of a server contract's line how many
//!    wanted on and what its bus takes (`ownline::carried`, `income::trip_demand`): a departure
//!    is full when its load reaches the admin's share of the bus's places, seats and standing
//!    (`Rules::full_share`; 100 %: people were left at the stop) - a player's own trip when he
//!    left people behind because the bus was full (`TripRun::left_full`). The full departures
//!    are kept a few weeks (`Doubling::full`).
//! 2. **The order.** A departure full on `FULL_OF` of its last `LIKE_DAYS` like days (weekdays,
//!    Saturdays, Sundays) gets an order (`Order`): an extra bus behind it on its like days of the
//!    week from the day after tomorrow (`ORDER_FROM`, `ORDER_DAYS`). The company answers yes or
//!    no until 18:00 the day before it begins (the Concessions page, `answer`); left unanswered,
//!    the dispatcher says yes when the company has money in the bank, no otherwise. The server's
//!    admin may order one himself, for a concert say (`ordered_by_admin`): that one is to be run.
//! 3. **The extra tour.** On each of its days an order said yes to is a tour of its own on the
//!    line - the departure's trip copied, its tour's name with `MARK` and the departure's minute
//!    (`add_tours`) - planned, claimed and driven as any other (`network::tours_of_day`), not
//!    one of the contract's journeys: no timetable hour of the contract, no fine when it is not
//!    run. The departure and its double share its passengers (`share`).
//! 4. **Pay.** A run is paid for its timetabled minutes: at the contract's price a timetable hour
//!    with a bus that runs the line's own tours that day (a contract bus standing free then), at
//!    the admin's fixed price with any other (`Rules::price_hour`, 650 kr.). Each run is a
//!    quality point on the contract (`QUALITY_PER_RUN`: it offsets a default point).
//! 5. **Declined.** No fine; but each full departure of a day the order would have run, said no
//!    to or not run, is a complaint against the contract's quality bonus
//!    (`bus_contracts::BONUS_PER_COMPLAINT`).

use super::bus_contracts::{Journeys, PointsDay};
use super::clock;
use super::dates;
use super::day::DayReport;
use super::inbox::{self, Kind, Message};
use super::model::{BookingKind, BusSize, Cents, Company};
use super::network::TourOfDay;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A departure full on this many of its last like days gets an order.
pub const FULL_OF: usize = 3;
pub const LIKE_DAYS: usize = 5;
/// An order runs from the day after tomorrow on its like days of a week.
pub const ORDER_FROM: i64 = 2;
pub const ORDER_DAYS: i64 = 7;
/// The company answers until this minute of the day before an order begins.
pub const ANSWER_BY: i64 = 18 * 60;
/// The full departures are kept this many days.
pub const KEEP_DAYS: i64 = 28;
/// The admin's fixed price a timetable hour of a run with a bus of no contract (kroner), and
/// the share of a bus's places at which a departure is full.
pub const PRICE_HOUR_KR: f64 = 650.0;
pub const FULL_SHARE: f64 = 1.0;
/// The quality points a run brings its contract (one offsets a default point).
pub const QUALITY_PER_RUN: f64 = 1.0;
/// What marks an extra tour's name: `<tour>+D<minute of its departure>`.
pub const MARK: &str = "+D";

/// The kind of day a departure runs on.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DayKind {
    #[default]
    Weekday,
    Saturday,
    Sunday,
}

impl DayKind {
    pub fn of(date: &str) -> DayKind {
        match dates::parse(date).map(dates::weekday) {
            Some(5) => DayKind::Saturday,
            Some(6) => DayKind::Sunday,
            _ => DayKind::Weekday,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DayKind::Weekday => "weekdays",
            DayKind::Saturday => "Saturdays",
            DayKind::Sunday => "Sundays",
        }
    }
}

/// The server's rules of doubling (`company-server.cfg`: `doubling_price` kroner a timetable
/// hour, `doubling_full` per cent of a bus's places), set by the company's process.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    /// Euro cents a timetable hour of a run with a bus of no contract.
    pub price_hour: Cents,
    pub full_share: f64,
}

impl Default for Rules {
    fn default() -> Rules {
        Rules { price_hour: super::server_fines::Fines::cents(PRICE_HOUR_KR), full_share: FULL_SHARE }
    }
}

impl Rules {
    pub fn of_keys(kv: &HashMap<String, String>) -> Rules {
        let d = Rules::default();
        let num = |k: &str| kv.get(k).and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok()).filter(|x| x.is_finite() && *x > 0.0);
        Rules {
            price_hour: num("doubling_price").map_or(d.price_hour, super::server_fines::Fines::cents),
            full_share: num("doubling_full").map_or(d.full_share, |x| (x / 100.0).clamp(0.3, 2.0)),
        }
    }

    pub fn of(data: &Path) -> Rules {
        Rules::of_keys(&super::server_fines::settings(data))
    }
}

/// The doubling's part of the settings, added to a `company-server.cfg` written before it.
pub const DOUBLING_CFG: &str = "
# doubling: an extra bus behind a departure full on 3 of its last 5 like days - the price a
# timetable hour of a run with a bus of no contract (kroner; one of the line's own contract buses
# standing free is paid at the contract's price), and how full is full (per cent of a bus's
# places, seats and standing: 100 - people left at the stop)
doubling_price = 650
doubling_full = 100
";

/// A departure that was full: its day, line (timetable name), tour, its minutes and what was left.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Full {
    pub date: String,
    pub line: String,
    pub tour: String,
    pub dep: i32,
    pub arr: i32,
    pub size: BusSize,
    pub left: u32,
}

/// The company's answer to an order.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    #[default]
    Asked,
    Yes,
    No,
}

/// An order of an extra bus behind a departure.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Order {
    pub id: u32,
    pub line: String,
    pub number: String,
    pub tour: String,
    pub dep: i32,
    pub arr: i32,
    pub size: BusSize,
    /// Its kind of days, the first and last day (None: every day of them, an admin's order).
    pub day: Option<DayKind>,
    pub from: String,
    pub until: String,
    pub answer: Answer,
    /// Who said yes or no: the company, its dispatcher, the server's admin.
    pub by: String,
    /// Its runs, what they were paid, the full departures that went without it.
    pub runs: u32,
    pub paid: Cents,
    pub declined: u32,
}

impl Order {
    /// Whether it is to run on `date` (said yes to).
    pub fn runs_on(&self, date: &str) -> bool {
        self.answer == Answer::Yes && self.covers(date)
    }

    /// Whether `date` is one of its days.
    pub fn covers(&self, date: &str) -> bool {
        dates::between(&self.from, date) >= 0 && dates::between(date, &self.until) >= 0 && self.day.is_none_or(|k| k == DayKind::of(date))
    }

    /// The name of its extra tour.
    pub fn tour_name(&self) -> String {
        format!("{}{MARK}{}", self.tour.trim(), self.dep)
    }

    /// The last moment it may be answered: 18:00 the day before it begins.
    pub fn answer_by(&self) -> i64 {
        clock::moment(&dates::add(&self.from, -1), ANSWER_BY)
    }

    fn same(&self, line: &str, tour: &str, dep: i32) -> bool {
        self.line.eq_ignore_ascii_case(line) && self.tour.trim() == tour.trim() && (self.dep - dep).abs() <= 1
    }
}

/// What a company keeps of doubling (`online::Online::doubling`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Doubling {
    pub full: Vec<Full>,
    pub orders: Vec<Order>,
    pub counter: u32,
    /// How far the admin's orders were read (bytes of its file).
    pub admin_read: u64,
}

/// Whether a tour is an extra one of an order.
pub fn is_tour(tour: &str) -> bool {
    tour.contains(MARK)
}

/// The tour an extra one runs behind, and its departure's minute.
pub fn base_of(tour: &str) -> Option<(&str, i32)> {
    let (base, dep) = tour.rsplit_once(MARK)?;
    Some((base, dep.trim().parse().ok()?))
}

/// The order an extra tour of `line` is of, on `date`.
pub fn order_of<'a>(c: &'a Company, date: &str, line: &str, tour: &str) -> Option<&'a Order> {
    let (base, dep) = base_of(tour)?;
    c.online.doubling.orders.iter().find(|o| o.same(line, base, dep) && o.runs_on(date))
}

/// The share of a departure's passengers it carries: half with its extra bus running that day
/// (`doubled`: the extra tours run, by line, base tour and minute), all of them otherwise - the
/// extra one's half too.
pub fn share(doubled: &[(String, String, i32)], line: &str, tour: &str, dep: i32) -> f64 {
    let (base, dep) = base_of(tour).unwrap_or((tour, dep));
    if doubled.iter().any(|d| d.0.eq_ignore_ascii_case(line) && d.1.trim() == base.trim() && (d.2 - dep).abs() <= 1) {
        0.5
    } else {
        1.0
    }
}

/// Whether a departure of a load `load` (`ownline::load`) in a bus of `size` is full by the
/// rules: at the share of all its places, seats and standing.
pub fn is_full(load: f64, size: BusSize, rules: &Rules) -> bool {
    load >= super::ownline::capacity(size) * super::ownline::CRUSH * rules.full_share - 1e-9
}

/// The extra tours of `date` added to the day's tours (`network::tours_of_day`): each order said
/// yes to that runs that day, its departure's trip copied from its tour (none when the
/// timetable no longer has it).
pub fn add_tours(c: &Company, date: &str, out: &mut Vec<TourOfDay>) {
    if !c.online.on_server {
        return;
    }
    let mut extra = Vec::new();
    for o in c.online.doubling.orders.iter().filter(|o| o.runs_on(date)) {
        let Some(t) = out.iter().find(|t| t.line.eq_ignore_ascii_case(&o.line) && t.tour.trim() == o.tour.trim() && !is_tour(&t.tour)) else { continue };
        let Some(trip) = t.trips.iter().find(|x| x.counts() && (x.dep - o.dep).abs() <= 1) else { continue };
        let mut trip = trip.clone();
        trip.part = None;
        extra.push(TourOfDay { line: t.line.clone(), number: t.number.clone(), tour: o.tour_name(), ai_group: t.ai_group.clone(), trips: vec![trip], unplanned: false, cuts: None });
    }
    out.extend(extra);
}

/// The texts told of it.
pub const ORDERED: (&str, &str) = ("Doubling ordered on line %{n}", "The %{time} departure of tour %{tour} was full on %{full} of its last %{like} like days: the authority orders an extra bus right behind it on its like days from %{from} to %{until}. Paid by its timetable minutes: at your contract's price with one of the line's own buses standing free, at %{amount} an hour with another. Answer by 18:00 on %{by} on the Concessions page.");
pub const DECIDED_YES: (&str, &str) = ("Doubling on line %{n}: yes", "Your dispatcher said yes to the extra bus behind the %{time} departure from %{from}: the company has the money for it.");
pub const DECIDED_NO: (&str, &str) = ("Doubling on line %{n}: no", "Your dispatcher said no to the extra bus behind the %{time} departure from %{from}: the company is short of money. Its full departures count as complaints against the quality bonus.");
pub const BY_ADMIN: (&str, &str) = ("Doubling on line %{n} ordered", "The authority orders an extra bus right behind the %{time} departure of tour %{tour} from %{from} to %{until}: it is on your duty board, to be run.");
pub const TOO_LATE: &str = "Too late: an order is answered by 18:00 the day before it begins.";
pub const NO_ORDER: &str = "There is no such order to answer.";
pub const NOT_YOURS: &str = "The authority's own order: it is to be run.";

fn hhmm(minute: i32) -> String {
    let m = minute.rem_euclid(1440);
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// Company `c` answers order `id`: yes or no, until 18:00 the day before it begins.
pub fn answer(c: &mut Company, id: u32, yes: bool) -> Result<(), &'static str> {
    let now = clock::now(c);
    let o = c.online.doubling.orders.iter_mut().find(|o| o.id == id).ok_or(NO_ORDER)?;
    if o.by == "admin" {
        return Err(NOT_YOURS);
    }
    if now >= o.answer_by() {
        return Err(TOO_LATE);
    }
    o.answer = if yes { Answer::Yes } else { Answer::No };
    o.by = "company".into();
    Ok(())
}

/// The contract bus of `line` running `date` (its contract), when the company has one on it.
fn contract_of<'a>(c: &'a mut Company, line: &str, date: &str) -> Option<&'a mut super::bus_contracts::Contract> {
    c.concessions.contracts.iter_mut().find(|k| k.line.eq_ignore_ascii_case(line) && k.on(date))
}

/// The like days of `date` back from it, `date` first: its kind's last `n`.
fn like_days(date: &str, n: usize) -> Vec<String> {
    let kind = DayKind::of(date);
    let mut out = Vec::new();
    let mut d = date.to_string();
    for _ in 0..(n * 7 + 7) {
        if DayKind::of(&d) == kind {
            out.push(d.clone());
            if out.len() == n {
                break;
            }
        }
        d = dates::add(&d, -1);
    }
    out
}

/// The day's close of company `c` on the server (after its contracts', `bus_contracts::close_day`):
/// its extra runs paid, the full departures that went without one counted against the bonus,
/// kept, and those full on enough like days ordered; the orders left unanswered decided by the
/// dispatcher. `report`: the day's, its extra runs paid go into its contracts' money.
pub fn close_day(c: &mut Company, date: &str, journeys: &Journeys, report: &mut DayReport) {
    if !c.online.on_server {
        return;
    }
    let rules = c.online.doubling_rules.unwrap_or_default();
    let index_of = |c: &Company, k: &super::bus_contracts::Contract| c.contract_index / k.index.max(0.01);
    // 1. the extra runs: paid, a quality point each
    let mut ran: Vec<(String, String, i32)> = Vec::new();
    for d in journeys.doubled_runs().iter().filter(|d| d.run) {
        let Some((base, dep)) = base_of(&d.tour) else { continue };
        let Some(k) = c.online.doubling.orders.iter().position(|o| o.same(&d.line, base, dep) && o.runs_on(date)) else { continue };
        ran.push((d.line.clone(), base.to_string(), dep));
        let contract = c.concessions.contracts.iter().find(|k| k.line.eq_ignore_ascii_case(&d.line) && k.on(date)).map(|k| (k.per_hour, index_of(c, k), k.number.clone()));
        let price = match contract {
            Some((per_hour, index, _)) if d.contract_bus && per_hour > 0 => per_hour as f64 * index,
            _ => rules.price_hour as f64 * c.price_index,
        };
        let amount = (price * d.minutes.max(1) as f64 / 60.0).round() as Cents;
        let number = contract.map(|x| x.2).unwrap_or_else(|| c.online.doubling.orders[k].number.clone());
        let o = &mut c.online.doubling.orders[k];
        o.runs += 1;
        o.paid += amount;
        let time = hhmm(o.dep);
        if amount > 0 {
            c.book(BookingKind::Contract, amount, format!("Line {number}: doubling behind the {time}"), false);
            report.contracts.paid += amount;
        }
        if let Some(x) = contract_of(c, &d.line, date) {
            x.record.push(PointsDay { date: date.to_string(), quality: QUALITY_PER_RUN, why: vec!["doubling".into()], ..Default::default() });
        }
    }
    // 2. the full departures of the day: without the extra bus ordered, a complaint each
    let today: Vec<Full> = journeys.fulls().iter().filter(|f| !is_tour(&f.tour)).cloned().map(|mut f| {
        f.date = date.to_string();
        f
    }).collect();
    for f in &today {
        if contract_of(c, &f.line, date).is_none() {
            continue;
        }
        let without = c.online.doubling.orders.iter().position(|o| o.same(&f.line, &f.tour, f.dep) && o.covers(date) && !ran.iter().any(|r| o.same(&r.0, &r.1, r.2)));
        if let Some(k) = without {
            c.online.doubling.orders[k].declined += 1;
            if let Some(x) = contract_of(c, &f.line, date) {
                x.bonus.complaints += 1;
            }
        }
    }
    // 3. kept a few weeks, and those full often enough ordered
    let lines: Vec<String> = c.concessions.contracts.iter().filter(|k| k.on(date)).map(|k| k.line.to_lowercase()).collect();
    c.online.doubling.full.extend(today.iter().filter(|f| lines.contains(&f.line.to_lowercase())).cloned());
    c.online.doubling.full.retain(|f| dates::between(&f.date, date) < KEEP_DAYS);
    let like = like_days(date, LIKE_DAYS);
    let from = dates::add(date, ORDER_FROM);
    let until = dates::add(&from, ORDER_DAYS - 1);
    let kind = DayKind::of(date);
    let mut ordered: Vec<Order> = Vec::new();
    for f in today.iter().filter(|f| lines.contains(&f.line.to_lowercase())) {
        let full: std::collections::BTreeSet<&str> = c.online.doubling.full.iter().filter(|x| x.line.eq_ignore_ascii_case(&f.line) && x.tour.trim() == f.tour.trim() && (x.dep - f.dep).abs() <= 1 && like.contains(&x.date)).map(|x| x.date.as_str()).collect();
        if full.len() < FULL_OF {
            continue;
        }
        // (one order at a time on a departure: none while one still covers its days to come)
        if c.online.doubling.orders.iter().chain(ordered.iter()).any(|o| o.same(&f.line, &f.tour, f.dep) && dates::between(date, &o.until) >= 0) {
            continue;
        }
        c.online.doubling.counter += 1;
        let number = c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(&f.line)).map(|l| l.number.clone()).unwrap_or_else(|| f.line.clone());
        ordered.push(Order { id: c.online.doubling.counter, line: f.line.clone(), number, tour: f.tour.clone(), dep: f.dep, arr: f.arr, size: f.size, day: Some(kind), from: from.clone(), until: until.clone(), answer: Answer::Asked, by: String::new(), runs: 0, paid: 0, declined: 0 });
    }
    for o in ordered {
        let full = c.online.doubling.full.iter().filter(|x| o.same(&x.line, &x.tour, x.dep) && like.contains(&x.date)).map(|x| x.date.clone()).collect::<std::collections::BTreeSet<_>>().len();
        let m = Message::new(Kind::Contract, ORDERED.0, ORDERED.1)
            .arg("n", o.number.clone())
            .arg("time", hhmm(o.dep))
            .arg("tour", o.tour.clone())
            .arg("full", full.to_string())
            .arg("like", LIKE_DAYS.to_string())
            .arg("from", o.from.clone())
            .arg("until", o.until.clone())
            .arg("amount", ((rules.price_hour as f64 * c.price_index).round() as Cents).to_string())
            .arg("by", dates::add(&o.from, -1));
        inbox::post(c, m);
        c.online.doubling.orders.push(o);
    }
    // 4. the orders beginning tomorrow and not answered: the dispatcher decides
    let tomorrow = dates::add(date, 1);
    let cash = c.cash;
    let mut told = Vec::new();
    for o in c.online.doubling.orders.iter_mut().filter(|o| o.answer == Answer::Asked && dates::between(&o.from, &tomorrow) >= 0) {
        o.answer = if cash > 0 { Answer::Yes } else { Answer::No };
        o.by = "dispatcher".into();
        told.push((o.answer, o.number.clone(), hhmm(o.dep), o.from.clone()));
    }
    for (a, n, time, from) in told {
        let text = if a == Answer::Yes { DECIDED_YES } else { DECIDED_NO };
        inbox::post(c, Message::new(Kind::Contract, text.0, text.1).arg("n", n).arg("time", time).arg("from", from));
    }
    // 5. those long over go
    c.online.doubling.orders.retain(|o| dates::between(&o.until, date) < 14);
}

// --- the server's admin's own orders ---------------------------------------------------------------

/// The admin's order of an extra bus on company `id`'s line (a line of the admin panel's): behind
/// the departure of `tour` at `dep` (minute of the day), every day from `from` to `until`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct AdminOrder {
    pub line: String,
    pub tour: String,
    pub dep: i32,
    pub from: String,
    pub until: String,
}

pub fn admin_file(data: &Path, id: &str) -> PathBuf {
    data.join("admin-doubling").join(format!("{id}.jsonl"))
}

/// The admin orders an extra bus on company `id`'s line.
pub fn order_by_admin(data: &Path, id: &str, o: &AdminOrder) -> anyhow::Result<()> {
    super::shared_tenders::append(&admin_file(data, id), &serde_json::to_string(o)?)
}

/// The admin's orders to company `c` since its last look: each said yes to and told (it is on
/// its duty board on its days). Whether any came.
pub fn ordered_by_admin(data: &Path, c: &mut Company) -> bool {
    let (orders, to): (Vec<AdminOrder>, u64) = super::shared_tenders::lines_from(&admin_file(data, &c.id), c.online.doubling.admin_read);
    if to == c.online.doubling.admin_read {
        return false;
    }
    c.online.doubling.admin_read = to;
    for a in orders.into_iter().filter(|a| dates::parse(&a.from).is_some() && dates::parse(&a.until).is_some()) {
        c.online.doubling.counter += 1;
        let number = c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(&a.line)).map(|l| l.number.clone()).unwrap_or_else(|| a.line.clone());
        let o = Order { id: c.online.doubling.counter, line: a.line.clone(), number: number.clone(), tour: a.tour.clone(), dep: a.dep, arr: a.dep, size: BusSize::Solo, day: None, from: a.from.clone(), until: a.until.clone(), answer: Answer::Yes, by: "admin".into(), runs: 0, paid: 0, declined: 0 };
        inbox::post(c, Message::new(Kind::Contract, BY_ADMIN.0, BY_ADMIN.1).arg("n", number).arg("time", hhmm(a.dep)).arg("tour", a.tour.clone()).arg("from", a.from.clone()).arg("until", a.until.clone()));
        c.online.doubling.orders.push(o);
    }
    true
}

#[cfg(test)]
mod tests;
