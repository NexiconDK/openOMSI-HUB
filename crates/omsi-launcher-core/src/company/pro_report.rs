//! RosterFlow Pro, round E (the player's, NexiconDK 5 November 2026: "Mandagsrapport"): every
//! Monday morning a short report of the week before.
//!
//! - **Counted** night by night (`after_day`, `day::close_day`): the wages paid, of them the
//!   overtime (hours and cost) and the supplements, the drivers' days ill, their calls in sick on
//!   the day, the duties left without a driver; in the day (`complained`) the drivers who
//!   grumbled of no extra duties (`pro_rest::grumbles`). Kept per week from Sunday (`Tally`).
//! - **Made** (`monday`, `clock::on_minute` from 06:00 on a Monday - or the first hour after, if
//!   the clock jumped it): the week's tally, the wishes granted and refused that week
//!   (`pro_wish`), the swap proposals approved and refused (`pro_trade`), the most tired driver
//!   that Saturday (`pro_tired`) and the least content one now. The last `KEPT` are kept.
//! - **Told**: the feed says it, the inbox has it, and a notice waits until seen ("📋
//!   Mandagsrapport for uge 12 er klar", "Åbn ›" - `unseen`, `seen`).

use super::inbox::{self, Kind, Message};
use super::model::{Cents, Company};
use super::pay::Paid;
use super::pro_turnus as pt;
use super::{dates, pro_tired, staff};
use serde::{Deserialize, Serialize};

/// A week's counts, from its Sunday (`sunday`), night by night.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Tally {
    pub sunday: String,
    pub pay: Cents,
    pub overtime_h: f64,
    pub overtime_cost: Cents,
    pub supplements: Cents,
    pub sick_days: u32,
    pub sick_calls: u32,
    pub uncovered: u32,
    pub complaints: u32,
    /// The nights counted.
    pub days: u32,
}

/// A driver the report names: who, and what (a level's key and its reason; a content's value).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Named {
    pub id: u32,
    pub name: String,
    /// The most tired: "tired" | "worn", and the reason's template and number (`pro_tired::Why`).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub key: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub why: String,
    pub n: u32,
    /// The least content: their content (0-100).
    pub value: f64,
}

/// A Monday report of the week from `sunday`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Report {
    /// Its week's number (of the Monday after `sunday`), and the day it was made.
    pub week: u32,
    pub sunday: String,
    pub made: String,
    pub tally: Tally,
    pub wishes_granted: u32,
    pub wishes_refused: u32,
    pub trades_approved: u32,
    pub trades_refused: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tired: Option<Named>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unhappy: Option<Named>,
    /// The player has seen it (its notice closed or the report opened).
    pub seen: bool,
}

/// The reports kept, and the tallies (this week's and the one before).
pub const KEPT: usize = 8;
const TALLIES: usize = 2;
/// The earliest minute of a Monday the report is made.
pub const FROM: i32 = 6 * 60;

/// The texts (English: keys of the translations).
pub const READY: &str = "📋 Monday report for week %{n} is ready";
pub const INBOX_TEXT: &str = "Week %{n}: wages %{amount} (of them overtime %{cost} for %{hours} h, supplements %{charge}), days ill: %{sick}, duties without a driver: %{uncovered}. Open it in RosterFlow's calendar: Reports.";

fn tally_of<'a>(c: &'a mut Company, sunday: &str) -> &'a mut Tally {
    let v = &mut c.planning.ops.tallies;
    if !v.iter().any(|t| t.sunday == sunday) {
        v.push(Tally { sunday: sunday.to_string(), ..Default::default() });
        v.sort_by(|a, b| a.sunday.cmp(&b.sunday));
        let extra = v.len().saturating_sub(TALLIES);
        v.drain(..extra);
    }
    v.iter_mut().find(|t| t.sunday == sunday).expect("just made")
}

/// The night after `date` (`day::close_day`, after the pay): its counts added to its week's.
pub fn after_day(c: &mut Company, date: &str, paid: &Paid, uncovered: u32) {
    if !super::pro_duties::on(c) {
        return;
    }
    let ill = c.staff.iter().filter(|e| staff::drives(e) && e.employed_on(date) && e.ill_on(date)).count() as u32;
    let calls = c.planning.ops.sick.iter().filter(|x| x.date == date).count() as u32;
    let t = tally_of(c, &pt::sunday_of(date));
    t.pay += paid.cost;
    t.overtime_h += paid.overtime;
    t.overtime_cost += paid.overtime_cost;
    t.supplements += paid.supplements;
    t.sick_days += ill;
    t.sick_calls += calls;
    t.uncovered += uncovered;
    t.days += 1;
}

/// `n` drivers grumbled today of no extra duties (`pro_rest::grumbles`).
pub fn complained(c: &mut Company, n: usize) {
    if n == 0 || !super::pro_duties::on(c) {
        return;
    }
    let sunday = pt::sunday_of(&c.date);
    tally_of(c, &sunday).complaints += n as u32;
}

/// The report of the week from `sunday`, as the company is now.
pub fn make(c: &Company, sunday: &str) -> Report {
    let saturday = dates::add(sunday, 6);
    let tally = c.planning.ops.tallies.iter().find(|t| t.sunday == sunday).cloned().unwrap_or_else(|| Tally { sunday: sunday.to_string(), ..Default::default() });
    let mut r = Report { week: pt::week_number(&dates::add(sunday, 1)), sunday: sunday.to_string(), made: c.date.clone(), tally, ..Default::default() };
    for w in c.planning.ops.wishes.iter() {
        let Some(d) = w.decided.as_deref() else { continue };
        if d < sunday || d > saturday.as_str() {
            continue;
        }
        match w.state {
            super::pro_wish::WishState::Granted => r.wishes_granted += 1,
            super::pro_wish::WishState::Refused | super::pro_wish::WishState::Lapsed => r.wishes_refused += 1,
            super::pro_wish::WishState::Open => {}
        }
    }
    (r.trades_approved, r.trades_refused) = super::pro_trade::decided_in(c, sunday, &saturday);
    let drivers: Vec<&super::model::Employee> = c.staff.iter().filter(|e| staff::drives(e) && e.employed_on(&c.date) && e.agency_no == 0).collect();
    // (the most tired that Saturday, as worked; the more tired, then the longer reason first)
    let mut tired: Option<(pro_tired::Tired, &super::model::Employee)> = None;
    for e in &drivers {
        let t = pro_tired::on_day(&saturday, &|d| pro_tired::worked_on(e, d));
        if t.level == pro_tired::Level::Fresh {
            continue;
        }
        let n = t.why.words().map_or(0, |w| w.1);
        if tired.as_ref().is_none_or(|(b, _)| (t.level, n) > (b.level, b.why.words().map_or(0, |w| w.1))) {
            tired = Some((t, e));
        }
    }
    r.tired = tired.map(|(t, e)| {
        let (why, n) = t.why.words().unwrap_or(("", 0));
        Named { id: e.id, name: e.name.clone(), key: t.level.key().to_string(), why: why.to_string(), n, value: 0.0 }
    });
    r.unhappy = drivers.iter().min_by(|a, b| a.satisfaction.total_cmp(&b.satisfaction)).map(|e| Named { id: e.id, name: e.name.clone(), value: e.satisfaction.round(), ..Default::default() });
    r
}

/// The Monday's report (`clock::on_minute`): on a Monday from `FROM` - or later in that week, if
/// the clock jumped it - the week before's report made, kept, told in the feed and the inbox. Its
/// week's number, when made now.
pub fn monday(c: &mut Company, minute: i32) -> Option<u32> {
    if !super::pro_duties::on(c) {
        return None;
    }
    let today = c.date.clone();
    let day = dates::parse(&today)?;
    // (0 Monday: from 06:00; later days of the week the same, if Monday went by)
    if dates::weekday(day) == 0 && minute < FROM {
        return None;
    }
    let sunday = dates::add(&pt::sunday_of(&today), -7);
    // (a week from Sunday: Sunday itself is not yet the Monday after)
    if dates::weekday(day) == 6 || c.planning.ops.reports.iter().any(|r| r.sunday == sunday) {
        return None;
    }
    // (nothing counted that week: a company new to RosterFlow Pro - no report)
    if !c.planning.ops.tallies.iter().any(|t| t.sunday == sunday && t.days > 0) {
        return None;
    }
    let r = make(c, &sunday);
    let week = r.week;
    let m = Message::new(Kind::Staff, READY, INBOX_TEXT)
        .arg("n", week.to_string())
        .arg("amount", r.tally.pay.to_string())
        .arg("cost", r.tally.overtime_cost.to_string())
        .arg("hours", format!("{:.1}", r.tally.overtime_h))
        .arg("charge", r.tally.supplements.to_string())
        .arg("sick", r.tally.sick_days.to_string())
        .arg("uncovered", r.tally.uncovered.to_string())
        .key(&format!("pro-report-{sunday}"));
    c.planning.ops.reports.push(r);
    c.planning.ops.reports.sort_by(|a, b| a.sunday.cmp(&b.sunday));
    let extra = c.planning.ops.reports.len().saturating_sub(KEPT);
    c.planning.ops.reports.drain(..extra);
    inbox::post(c, m);
    Some(week)
}

/// The newest report not seen yet.
pub fn unseen(c: &Company) -> Option<&Report> {
    c.planning.ops.reports.iter().rev().find(|r| !r.seen)
}

/// Every report seen.
pub fn seen(c: &mut Company) {
    for r in c.planning.ops.reports.iter_mut() {
        r.seen = true;
    }
}

/// Every English text of this module.
pub fn texts() -> Vec<&'static str> {
    vec![READY, INBOX_TEXT]
}
