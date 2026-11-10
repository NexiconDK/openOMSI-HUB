//! RosterFlow Pro, round Q (the player's, NexiconDK 9 November 2026: "Overskud pr. linje"): what
//! each line earns and costs this week and this month.
//!
//! - **Booked** (`booked`): the company's ledger by line - every booking of a line names it
//!   ("Line 131: …", `line_of_text`): what it brings in (the contract's payments, the fares and
//!   passes, BSOG, the per-km payments: `INCOME`), what it is fined (the contract's penalties, a
//!   player's traffic fines on it: `PENALTIES`), what its buses
//!   burn and wear (diesel or power, the maintenance: `RUNNING`). The drivers' wages are booked for
//!   the company as a whole, so -
//! - **The drivers** (`duty_wages`): a day's duties at the Vagter tab's price (`pro_dutyplan::price`:
//!   the paid hours at the drivers' average wage, the supplements, the employer's on-costs), each
//!   duty's price shared out among the lines it works on by its minutes on each.
//! - **The contract's day** (`contract_day`): what the line's contract is worth an operating day
//!   (`bus_contracts::day_value`), for the days still to come.
//!
//! The page (`omsihub/rosterflow/ops.rs`, `econ`) puts them together: the days closed so far as
//! booked with their duties, and the whole week or month as it looks - the days to come at the
//! contract's day, the fares and the running at the line's average day of late, the duties as
//! planned.

use super::bus_contracts;
use super::model::{BookingKind, Cents, Company};
use super::pro_dutyplan as dp;
use super::rosterflow::Board;
use super::{dates, plan};

/// What a line brings in, is fined, and its buses cost to run, by the ledger's kinds.
pub const INCOME: [BookingKind; 9] = [BookingKind::Contract, BookingKind::Fares, BookingKind::ChildFares, BookingKind::NetworkPasses, BookingKind::SchoolPasses, BookingKind::Reimbursement, BookingKind::Bsog, BookingKind::Compensation, BookingKind::InspectionFines];
pub const PENALTIES: [BookingKind; 2] = [BookingKind::Penalty, BookingKind::Fine];
pub const RUNNING: [BookingKind; 2] = [BookingKind::Energy, BookingKind::Maintenance];
/// The days of late a line's average day of fares and running is taken from.
pub const AVERAGE_DAYS: i64 = 28;

/// A line's money over some days (cents): income in, the penalties and the running (negative, as
/// booked), the drivers' duties (positive, a cost) - and of the income the contract's.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Money {
    pub income: Cents,
    pub contract: Cents,
    pub penalties: Cents,
    pub running: Cents,
    pub wages: Cents,
}

impl Money {
    /// What is left: the income less the penalties, the running and the duties.
    pub fn result(&self) -> Cents {
        self.income + self.penalties + self.running - self.wages
    }

    pub fn add(&mut self, o: &Money) {
        self.income += o.income;
        self.contract += o.contract;
        self.penalties += o.penalties;
        self.running += o.running;
        self.wages += o.wages;
    }
}

/// The line number a booking's text names: "Line 131", "Line 131: …", "Line 131 (…)".
pub fn line_of_text(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("Line ")?;
    let end = rest.find([':', ' ', '(']).unwrap_or(rest.len());
    let n = rest[..end].trim();
    (!n.is_empty()).then_some(n)
}

/// The ledger by line from `from` to `to` (dates, both in): each line's number and its money (the
/// duties left at 0).
pub fn booked(c: &Company, from: &str, to: &str) -> Vec<(String, Money)> {
    let mut out: Vec<(String, Money)> = Vec::new();
    for b in c.ledger.iter().filter(|b| b.date.as_str() >= from && b.date.as_str() <= to) {
        let Some(n) = line_of_text(&b.text) else { continue };
        let m = match out.iter_mut().find(|x| x.0 == n) {
            Some(x) => &mut x.1,
            None => {
                out.push((n.to_string(), Money::default()));
                &mut out.last_mut().expect("just pushed").1
            }
        };
        if INCOME.contains(&b.kind) {
            m.income += b.amount;
            if b.kind == BookingKind::Contract {
                m.contract += b.amount;
            }
        } else if PENALTIES.contains(&b.kind) {
            m.penalties += b.amount;
        } else if RUNNING.contains(&b.kind) {
            m.running += b.amount;
        }
    }
    out
}

/// What the duties of `board` cost on `date` (`hour`: an hour's wage, `pro_dutyplan::hour_wage`),
/// by line number: each duty's price shared out by its minutes on each line.
pub fn duty_wages(c: &Company, board: &Board, date: &str, hour: f64) -> Vec<(String, Cents)> {
    let mut out: Vec<(String, f64)> = Vec::new();
    for s in board.shaped.iter().filter(|s| s.spare.is_none()) {
        let price = dp::price(c, s, date, hour).total as f64;
        let mut by: Vec<(String, i32)> = Vec::new();
        for sp in &s.spans {
            let Some(t) = board.tours.get(sp.tour) else { continue };
            let n = t.number.trim().to_string();
            let m = (sp.to - sp.from).max(1);
            match by.iter_mut().find(|x| x.0 == n) {
                Some(x) => x.1 += m,
                None => by.push((n, m)),
            }
        }
        let all: i32 = by.iter().map(|x| x.1).sum();
        if all <= 0 {
            continue;
        }
        for (n, m) in by {
            let share = price * m as f64 / all as f64;
            match out.iter_mut().find(|x| x.0 == n) {
                Some(x) => x.1 += share,
                None => out.push((n, share)),
            }
        }
    }
    out.into_iter().map(|(n, x)| (n, x.round() as Cents)).collect()
}

/// Whether the company ran line `number` (its number or timetable name) on `date`: the company and
/// the line there by then, the line in service that day (`network::in_service`) - a line not the
/// company's at all counts as running (its tours are on the board).
pub fn runs_on(c: &Company, number: &str, date: &str) -> bool {
    if date < c.founded.as_str() {
        return false;
    }
    let n = number.trim();
    match c.lines.iter().find(|l| l.number.trim() == n || l.name.trim() == n) {
        Some(l) => l.added.as_str() <= date && super::network::in_service(l, super::clock::moment(date, 1439)),
        None => true,
    }
}

/// What line `line`'s (timetable name) contract is worth on an operating day `date` (cents; 0 on
/// a commercial line).
pub fn contract_day(c: &Company, line: &str, date: &str) -> Cents {
    bus_contracts::on_line(c, line, date).map_or(0, |k| bus_contracts::day_value(c, k).round() as Cents)
}

/// The week of `date`, Monday to Sunday, and its month, the first to the last: (first, last).
pub fn week_of(date: &str) -> (String, String) {
    let monday = dates::add(date, -(plan::weekday_of(date) as i64));
    let sunday = dates::add(&monday, 6);
    (monday, sunday)
}

pub fn month_of(date: &str) -> (String, String) {
    let m = dates::month_of(date);
    let first = format!("{m}-01");
    let (y, mm) = (m[..4].parse::<i32>().unwrap_or(2024), m[5..7].parse::<u32>().unwrap_or(1));
    let last = format!("{m}-{:02}", dates::days_in_month(y, mm));
    (first, last)
}

/// The dates from `from` to `to`, both in.
pub fn dates_between(from: &str, to: &str) -> Vec<String> {
    let n = dates::between(from, to);
    (0..=n.max(-1)).map(|k| dates::add(from, k)).collect()
}

pub const TITLE: &str = "Profit by line";
pub const SO_FAR: &str = "So far (days closed)";
pub const WHOLE: &str = "The whole %{period} (forecast)";

/// Every English text this module writes, for the translations.
pub fn texts() -> Vec<&'static str> {
    vec![TITLE, SO_FAR, WHOLE]
}

