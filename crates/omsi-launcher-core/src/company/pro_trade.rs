//! RosterFlow Pro, round E (the player's, NexiconDK 5 November 2026: "Chaufførerne foreslår selv
//! bytte"): two drivers agree between themselves and ask the planner to let them swap.
//!
//! - **Proposed** (`daily`, once a company day from 10:00 - `clock::advance`, which reads the
//!   week's duties for it): at most one a day, with a chance of `CHANCE` and `PER_REASON` more for
//!   each driver with a reason in the days ahead, `MOST_CHANCE` at most - two to four a week in a
//!   company of ten. Never more than `OPEN_MOST` open at once.
//! - **What**: two drivers on their rotas swap their duties of a day (`Offer::Swap`), or one gives
//!   a duty to a colleague who has the day off (`Offer::Give`) - one to six days ahead.
//! - **Why** they want it, as the round D's drivers are: a tired driver (`pro_tired`) wants to give
//!   a duty away to a fresh colleague off that day ("Lone er træt og vil gerne have fri"); an early
//!   person with a late duty swaps with a late person with an early one (`pro_pref`); now and then
//!   one has an appointment (`Why::Errand`). A swap legal for both under the rest rules
//!   (`pro_swap::breaches`) is chosen when there is one; else the page shows the ⚠.
//! - **Approved** (`approve`): made through round C's swap (`pro_swap::swap_rota`) - ⇄ in the
//!   calendar, "Fortryd bytte" until its first duty begins; both are a little more content
//!   (`APPROVED`). **Refused** (`refuse`): both a little less (`REFUSED`). Left open until its first
//!   duty begins or `LAPSE_DAYS` (`lapse`): less (`LAPSED`).

use super::model::Company;
use super::pro_rest::Breach;
use super::pro_turnus::{self as pt, Cell, Week};
use super::rng::Rng;
use super::{dates, messages, pro_pref, pro_tired, staff};
use serde::{Deserialize, Serialize};

/// What two drivers propose.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Offer {
    /// `a` drives `b`'s duty `b_n` of the date, `b` drives `a`'s `a_n`.
    Swap { a_n: u32, b_n: u32 },
    /// `b`, off that day, drives `a`'s duty `a_n`; `a` has the day off.
    Give { a_n: u32 },
}

/// Why they want it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Why {
    /// `a` is tired (`pro_tired`) and would like the day off.
    Tired,
    /// `a` would rather drive late (b early) - `pro_pref`.
    Late,
    /// `a` would rather drive early (b late).
    Early,
    /// `a` has an appointment that day.
    Errand,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TradeState {
    #[default]
    Open,
    Approved,
    Refused,
    /// Left open until its first duty began or `LAPSE_DAYS`, or no longer as proposed.
    Lapsed,
}

/// A swap two drivers propose.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Trade {
    pub id: u32,
    pub date: String,
    pub a: u32,
    pub b: u32,
    pub offer: Offer,
    pub why: Why,
    /// Whether `b` (a swap: also) likes it the same way (`Why::Late`/`Early`: both gain).
    #[serde(default)]
    pub both: bool,
    /// The company's day it was sent, and decided.
    pub sent: String,
    #[serde(default)]
    pub state: TradeState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided: Option<String>,
    /// The minute of the date its first duty begins (it lapses then).
    pub starts: i32,
    /// Approved: the swap made (`pro_swap::Swap::no`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub swap: Option<u32>,
}

/// The chance of a proposal on a company day, and more for each driver with a reason, at most.
pub const CHANCE: f64 = 0.12;
pub const PER_REASON: f64 = 0.1;
pub const MOST_CHANCE: f64 = 0.6;
/// Of a day's proposal, the share for a reason (tired, preference) when there is one; else an
/// appointment.
pub const REASONED: f64 = 0.8;
/// The most proposals open at once.
pub const OPEN_MOST: usize = 3;
/// How far ahead (days: tomorrow to this).
pub const AHEAD: i64 = 6;
/// What approving gives both drivers' content, refusing costs, and leaving it open costs.
pub const APPROVED: f64 = 2.0;
pub const REFUSED: f64 = 2.0;
pub const LAPSED: f64 = 1.0;
/// An open proposal lapses after this many days (or when its first duty begins).
pub const LAPSE_DAYS: i64 = 3;

/// The texts (English: keys of the translations).
pub const SAYS_SWAP: &str = "%{b} and I would like to swap duties on %{date}: I drive %{bn}, %{b} drives %{an}. Is that all right?";
pub const SAYS_GIVE: &str = "%{b} has offered to take my duty %{an} on %{date}. Is that all right?";
pub const SENT_FEED: &str = "%{a} and %{b} propose a swap.";
pub const SWAP_TEXT: &str = "%{a} and %{b} want to swap on %{date}: %{an} ⇄ %{bn}";
pub const GIVE_TEXT: &str = "%{a} wants to give duty %{an} on %{date} to %{b}";
pub const WHY_TIRED: &str = "%{a} is tired and would like the day off";
pub const WHY_LATE: &str = "%{a} would rather drive late";
pub const WHY_EARLY: &str = "%{a} would rather drive early";
pub const WHY_LATE_BOTH: &str = "%{a} would rather drive late, %{b} early";
pub const WHY_EARLY_BOTH: &str = "%{a} would rather drive early, %{b} late";
pub const WHY_ERRAND: &str = "%{a} has an appointment that day";
pub const NO_SUCH_TRADE: &str = "That swap proposal is no longer open.";
pub const CHANGED: &str = "The duties have changed since: the swap no longer fits.";

fn on_rota(c: &Company, id: u32, date: &str) -> bool {
    c.employee(id).is_some_and(|e| staff::drives(e) && e.employed_on(date) && e.notice_until.is_none() && e.agency_no == 0 && !e.absent(date))
}

/// Driver `id`'s duty of `date` by his rota (None: no duty, away, a day not his rota's own).
fn duty_of(c: &Company, id: u32, date: &str) -> Option<u32> {
    let d = pt::decided(c, id, date)?;
    if d.note.as_deref().is_some_and(|n| n != "swap") {
        return None;
    }
    match d.cell {
        Cell::Duty { n } => Some(n),
        _ => None,
    }
}

/// Whether driver `id` is off on `date` by his rota alone (not ill, away, a free day of his).
fn off_on(c: &Company, id: u32, date: &str) -> bool {
    pt::decided(c, id, date).is_some_and(|d| d.cell == Cell::Off && d.note.is_none())
}

/// Driver `id`'s work of `date`: as worked before the company's day, as his rota has it from it.
fn span(c: &Company, week: &Week, id: u32, date: &str) -> pro_tired::Day {
    if date < c.date.as_str() {
        return c.employee(id).and_then(|e| pro_tired::worked_on(e, date));
    }
    let d = pt::decided(c, id, date)?;
    pt::times(week, pt::day_of(date), d.cell).map(|t| (t.0, t.1))
}

fn start_of(week: &Week, date: &str, n: u32) -> Option<i32> {
    pt::times(week, pt::day_of(date), Cell::Duty { n }).map(|t| t.0)
}

/// What the trade would change on its date, as two rota exceptions: (driver, cell).
fn cells(t: &Trade) -> [(u32, Cell); 2] {
    match t.offer {
        Offer::Swap { a_n, b_n } => [(t.a, Cell::Duty { n: b_n }), (t.b, Cell::Duty { n: a_n })],
        Offer::Give { a_n } => [(t.a, Cell::Off), (t.b, Cell::Duty { n: a_n })],
    }
}

/// The rest rules each of the two would break with the trade (`pro_swap::breaches`, on a copy).
pub fn breaches(c: &Company, t: &Trade, week: &Week) -> (Vec<Breach>, Vec<Breach>) {
    let mut x = c.clone();
    breaches_on(&mut x, t, week)
}

/// As `breaches`, on `x` (the exceptions put for the trade and back as they were).
fn breaches_on(x: &mut Company, t: &Trade, week: &Week) -> (Vec<Breach>, Vec<Breach>) {
    let was: Vec<_> = [t.a, t.b].iter().map(|id| x.planning.rotas.exceptions.iter().find(|e| e.driver == *id && e.date == t.date).cloned()).collect();
    for (id, cell) in cells(t) {
        pt::put_exception(x, id, &t.date, Some((cell, "swap".to_string())));
    }
    let out = (super::pro_swap::breaches(x, t.a, &t.date, week), super::pro_swap::breaches(x, t.b, &t.date, week));
    for (id, w) in [t.a, t.b].iter().zip(was) {
        pt::put_exception(x, *id, &t.date, w.map(|e| (e.cell, e.note)));
    }
    out
}

/// Whether trade `t` is still as proposed: both drivers' days of its date as they were.
pub fn fits(c: &Company, t: &Trade) -> bool {
    on_rota(c, t.a, &t.date)
        && on_rota(c, t.b, &t.date)
        && super::pro_swap::of_driver(c, t.a, &t.date).is_none()
        && super::pro_swap::of_driver(c, t.b, &t.date).is_none()
        && match t.offer {
            Offer::Swap { a_n, b_n } => duty_of(c, t.a, &t.date) == Some(a_n) && duty_of(c, t.b, &t.date) == Some(b_n),
            Offer::Give { a_n } => duty_of(c, t.a, &t.date) == Some(a_n) && off_on(c, t.b, &t.date),
        }
}

/// Whether open trade `t` is still live: its first duty not begun, not `LAPSE_DAYS` old.
pub fn live(c: &Company, t: &Trade) -> bool {
    t.state == TradeState::Open
        && dates::between(&t.sent, &c.date) < LAPSE_DAYS
        && match dates::between(&c.date, &t.date) {
            d if d > 0 => true,
            0 => (c.clock.minute as i32) < t.starts,
            _ => false,
        }
}

/// The open proposals still as proposed, the earliest date first.
pub fn open(c: &Company) -> Vec<&Trade> {
    let mut v: Vec<&Trade> = c.planning.ops.trades.iter().filter(|t| live(c, t) && fits(c, t)).collect();
    v.sort_by(|a, b| (a.date.as_str(), a.starts, a.id).cmp(&(b.date.as_str(), b.starts, b.id)));
    v
}

/// A candidate: the trade, its weight.
struct Cand {
    t: Trade,
    w: f64,
}

/// The trades the drivers could propose for the days ahead: those with a reason (tired, a duty not
/// liked) and, apart, any other.
fn candidates(c: &Company, week: &Week) -> (Vec<Cand>, Vec<Cand>) {
    let today = c.date.clone();
    let mut reasoned = Vec::new();
    let mut others = Vec::new();
    let busy = |id: u32, date: &str| c.planning.ops.trades.iter().any(|t| live(c, t) && t.date == date && [t.a, t.b].contains(&id));
    for k in 1..=AHEAD {
        let date = dates::add(&today, k);
        let ids: Vec<u32> = c.staff.iter().map(|e| e.id).filter(|id| on_rota(c, *id, &date) && pt::decided(c, *id, &date).is_some() && super::pro_swap::of_driver(c, *id, &date).is_none() && !busy(*id, &date)).collect();
        let on: Vec<(u32, u32, i32)> = ids.iter().filter_map(|id| duty_of(c, *id, &date).and_then(|n| start_of(week, &date, n).map(|s| (*id, n, s)))).collect();
        let off: Vec<u32> = ids.iter().copied().filter(|id| off_on(c, *id, &date)).collect();
        let tired = |id: u32| pro_tired::on_day(&date, &|d| span(c, week, id, d)).level;
        for &(a, an, sa) in &on {
            let Some(ea) = c.employee(a) else { continue };
            let ta = tired(a);
            // (a duty given away to a colleague off that day)
            for &b in &off {
                let starts = sa;
                let t = Trade { id: 0, date: date.clone(), a, b, offer: Offer::Give { a_n: an }, why: Why::Errand, both: false, sent: today.clone(), state: TradeState::Open, decided: None, starts, swap: None };
                // (the one taking it: fresh, it would not make him tired)
                let tb = pro_tired::on_day(&date, &|d| if d == date { Some((sa, sa + 8 * 60)) } else { span(c, week, b, d) }).level;
                if ta != pro_tired::Level::Fresh && tb == pro_tired::Level::Fresh {
                    let w = if ta == pro_tired::Level::Worn { 5.0 } else { 3.0 };
                    reasoned.push(Cand { t: Trade { why: Why::Tired, ..t.clone() }, w });
                }
                others.push(Cand { t, w: 1.0 });
            }
            // (two duties of the day swapped)
            for &(b, bn, sb) in &on {
                if b == a || bn == an {
                    continue;
                }
                let Some(eb) = c.employee(b) else { continue };
                let starts = sa.min(sb);
                let t = Trade { id: 0, date: date.clone(), a, b, offer: Offer::Swap { a_n: an, b_n: bn }, why: Why::Errand, both: false, sent: today.clone(), state: TradeState::Open, decided: None, starts, swap: None };
                let (fa_now, fa_then) = (pro_pref::fit(ea, sa), pro_pref::fit(ea, sb));
                let (fb_now, fb_then) = (pro_pref::fit(eb, sb), pro_pref::fit(eb, sa));
                if fa_then > fa_now && fb_then >= fb_now {
                    let why = if pro_pref::kind(sb) == pro_pref::ShiftPref::Late { Why::Late } else { Why::Early };
                    let both = fb_then > fb_now;
                    reasoned.push(Cand { t: Trade { why, both, ..t.clone() }, w: if both { 3.0 } else { 2.0 } });
                }
                if a < b {
                    others.push(Cand { t, w: 1.0 });
                }
            }
        }
    }
    (reasoned, others)
}

/// The candidates in the order drawn: by weight, the same for the same company and day.
fn drawn(mut v: Vec<Cand>, rng: &mut Rng) -> Vec<Trade> {
    let mut out = Vec::new();
    while !v.is_empty() {
        let all: f64 = v.iter().map(|x| x.w).sum();
        let mut x = rng.range(0.0, all);
        let k = v.iter().position(|c| {
            x -= c.w;
            x < 0.0
        });
        out.push(v.remove(k.unwrap_or(0)).t);
    }
    out
}

/// The company's day for the drivers' proposals (`clock::advance` from 10:00, once a day; `week`
/// the duties of the days ahead): the old ones lapse; maybe one new - legal for both under the
/// rest rules when one is - proposed by its first driver's message to the control room. The new
/// one's number.
pub fn daily(c: &mut Company, week: &Week) -> Option<u32> {
    if !super::pro_duties::on(c) || c.planning.ops.traded == c.date {
        return None;
    }
    let today = c.date.clone();
    c.planning.ops.traded = today.clone();
    lapse(c);
    if open(c).len() >= OPEN_MOST {
        return None;
    }
    let (reasoned, others) = candidates(c, week);
    let mut reasons: Vec<u32> = reasoned.iter().map(|x| x.t.a).collect();
    reasons.sort();
    reasons.dedup();
    let mut rng = Rng::of(&[&c.id, &today, "pro-trade"], 0);
    if !rng.chance((CHANCE + PER_REASON * reasons.len() as f64).min(MOST_CHANCE)) {
        return None;
    }
    let pool = if !reasoned.is_empty() && rng.chance(REASONED) { reasoned } else { others };
    let order = drawn(pool, &mut rng);
    // (the first legal for both under the rest rules; none: the first, the page warns)
    let mut x = c.clone();
    let pick = order.iter().take(12).find(|t| {
        let (a, b) = breaches_on(&mut x, t, week);
        a.is_empty() && b.is_empty()
    });
    let mut t = pick.or(order.first())?.clone();
    c.planning.ops.last_trade += 1;
    t.id = c.planning.ops.last_trade;
    let name = |id: u32| c.employee(id).map(|e| e.name.clone()).unwrap_or_default();
    let (na, nb) = (name(t.a), name(t.b));
    let day = numeric(&t.date);
    let (text, args) = match t.offer {
        Offer::Swap { a_n, b_n } => (SAYS_SWAP, vec![("b".to_string(), nb), ("date".to_string(), day), ("an".to_string(), a_n.to_string()), ("bn".to_string(), b_n.to_string())]),
        Offer::Give { a_n } => (SAYS_GIVE, vec![("b".to_string(), nb), ("date".to_string(), day), ("an".to_string(), a_n.to_string())]),
    };
    let id = t.id;
    c.planning.ops.trades.push(t);
    let from = messages::From::Driver { line: String::new(), tour: String::new(), name: na };
    messages::send(c, from, messages::To::Control, messages::Priority::Info, text, args, false, None);
    Some(id)
}

/// "9/3": a date as a message says it.
fn numeric(date: &str) -> String {
    let d = date.get(8..10).and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
    let m = date.get(5..7).and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
    format!("{d}/{m}")
}

/// Both drivers' content by `by` (+ more content).
fn content(c: &mut Company, t: &Trade, by: f64) {
    for e in c.staff.iter_mut().filter(|e| e.id == t.a || e.id == t.b) {
        e.satisfaction = (e.satisfaction + by).clamp(0.0, 100.0);
    }
}

fn decide(c: &mut Company, id: u32, state: TradeState) -> Option<Trade> {
    let today = c.date.clone();
    let t = c.planning.ops.trades.iter_mut().find(|t| t.id == id && t.state == TradeState::Open)?;
    t.state = state;
    t.decided = Some(today);
    let t = t.clone();
    let by = match state {
        TradeState::Approved => APPROVED,
        TradeState::Refused => -REFUSED,
        TradeState::Lapsed => -LAPSED,
        TradeState::Open => 0.0,
    };
    content(c, &t, by);
    Some(t)
}

/// The open proposals no longer live or no longer as proposed lapse: both a little less content.
pub fn lapse(c: &mut Company) {
    let gone: Vec<u32> = c.planning.ops.trades.iter().filter(|t| t.state == TradeState::Open && (!live(c, t) || !fits(c, t))).map(|t| t.id).collect();
    for id in gone {
        decide(c, id, TradeState::Lapsed);
    }
}

/// Proposal `id` approved: the swap made (`pro_swap::swap_rota`: both told, ⇄, undone in the
/// day's box until its first duty); both more content. The rest rules either breaks.
pub fn approve(c: &mut Company, id: u32, week: &Week) -> Result<super::pro_swap::Made, &'static str> {
    let t = c.planning.ops.trades.iter().find(|t| t.id == id && live(c, t)).cloned().ok_or(NO_SUCH_TRADE)?;
    // (no longer as proposed: no longer listed, it lapses on the day's or the night's `lapse`)
    if !fits(c, &t) {
        return Err(CHANGED);
    }
    let made = match t.offer {
        Offer::Swap { b_n, .. } => super::pro_swap::swap_rota(c, t.a, &t.date, b_n, week)?,
        Offer::Give { a_n } => super::pro_swap::swap_rota(c, t.b, &t.date, a_n, week)?,
    };
    decide(c, id, TradeState::Approved);
    if let Some(x) = c.planning.ops.trades.iter_mut().find(|x| x.id == id) {
        x.swap = made.swap.as_ref().map(|s| s.no);
    }
    Ok(made)
}

/// Proposal `id` refused: both less content.
pub fn refuse(c: &mut Company, id: u32) -> Result<(), &'static str> {
    if !c.planning.ops.trades.iter().any(|t| t.id == id && live(c, t)) {
        return Err(NO_SUCH_TRADE);
    }
    decide(c, id, TradeState::Refused).map(|_| ()).ok_or(NO_SUCH_TRADE)
}

/// A trade's words (English templates, places): what they propose, and why (`day`: the page's
/// words for a date).
pub fn words(c: &Company, t: &Trade, day: &dyn Fn(&str) -> String) -> ((&'static str, Vec<(&'static str, String)>), (&'static str, Vec<(&'static str, String)>)) {
    let name = |id: u32| c.employee(id).map(|e| e.name.clone()).unwrap_or_default();
    let (a, b) = (name(t.a), name(t.b));
    let what = match t.offer {
        Offer::Swap { a_n, b_n } => (SWAP_TEXT, vec![("a", a.clone()), ("b", b.clone()), ("date", day(&t.date)), ("an", a_n.to_string()), ("bn", b_n.to_string())]),
        Offer::Give { a_n } => (GIVE_TEXT, vec![("a", a.clone()), ("b", b.clone()), ("date", day(&t.date)), ("an", a_n.to_string())]),
    };
    let why = match (t.why, t.both) {
        (Why::Tired, _) => WHY_TIRED,
        (Why::Late, false) => WHY_LATE,
        (Why::Late, true) => WHY_LATE_BOTH,
        (Why::Early, false) => WHY_EARLY,
        (Why::Early, true) => WHY_EARLY_BOTH,
        (Why::Errand, _) => WHY_ERRAND,
    };
    (what, (why, vec![("a", a), ("b", b)]))
}

/// How many proposals were approved and refused (refused or left) on the days `from` to `until`.
pub fn decided_in(c: &Company, from: &str, until: &str) -> (u32, u32) {
    let mut out = (0, 0);
    for t in c.planning.ops.trades.iter() {
        let Some(d) = t.decided.as_deref() else { continue };
        if d < from || d > until {
            continue;
        }
        match t.state {
            TradeState::Approved => out.0 += 1,
            TradeState::Refused | TradeState::Lapsed => out.1 += 1,
            TradeState::Open => {}
        }
    }
    out
}

/// The proposals decided long ago dropped (a month and a half, for the Monday report), and the
/// drivers gone (the night's, `pro_ops::tidy`).
pub fn tidy(c: &mut Company) {
    lapse(c);
    let today = c.date.clone();
    let drivers: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    c.planning.ops.trades.retain(|t| drivers.contains(&t.a) && drivers.contains(&t.b) && t.decided.as_deref().is_none_or(|d| dates::between(d, &today) < 45));
}

/// Every English text of this module.
pub fn texts() -> Vec<&'static str> {
    vec![SAYS_SWAP, SAYS_GIVE, SENT_FEED, SWAP_TEXT, GIVE_TEXT, WHY_TIRED, WHY_LATE, WHY_EARLY, WHY_LATE_BOTH, WHY_EARLY_BOTH, WHY_ERRAND, NO_SUCH_TRADE, CHANGED]
}
