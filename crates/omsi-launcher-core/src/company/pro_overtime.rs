//! RosterFlow Pro: overtime on an open duty (NexiconDK, 22 November 2026: "Kan vi lave en funktion
//! der gør at en chauffør som allerede har en vagt den dag at tage overarbejde? Fx hvis chaufføren
//! har fri kl. 14:45 og en vagt skal starte kl. 15:00 - Så evt. tage et par timer? Så skal vagten
//! 'klippes' for en chauffør kan ikke køre 2 vagter i træk", then "En vagt må MAKS være 15 timer men
//! chaufføren må kun 'KØRE' i 10 timer", "det skal også gælde den anden vej med at møde tidligere").
//!
//! A driver whose own duty ends just before an open duty begins (at most `NEAR_MINUTES` before,
//! at a stop near where it begins, his rest between two pieces of work kept) can go on with its first part as
//! overtime; one whose own duty begins just after it ends can begin earlier with its last part. How
//! much: the most of it - the whole duty, or the duty cut at one of its relief points - his day
//! takes: at most `staff::DAY_MAX` behind the wheel (`plan::can_take`), at most
//! `pro_rest::SPREAD_MAX` from his first sign-on to his last sign-off, his rest kept. `offers` finds
//! them; `take` cuts the duty there for that date (`pro_cut::cut`) and says which part is his - the
//! rest stays open, for another driver.

use super::model::Company;
use super::plan::{self, DayPlan, Who};
use super::pro_cut::{self as pc, Seg};
use super::pro_ops::OpenDuty;
use super::pro_turnus::ProDuty;
use super::staff;

/// Overtime a driver can do on an open duty.
#[derive(Clone, Debug, PartialEq)]
pub struct Overtime {
    pub driver: u32,
    /// He begins earlier (the duty's last part); else he goes on (its first part).
    pub earlier: bool,
    /// His own duty's number, and when it ends (going on) or begins (beginning earlier).
    pub own: u32,
    pub at: i32,
    /// The part of the duty that would be his, when it begins and ends.
    pub segs: Vec<Seg>,
    pub from: i32,
    pub to: i32,
    /// It is the whole duty: no cut.
    pub whole: bool,
}

impl Overtime {
    /// The minutes of the duty he takes.
    pub fn minutes(&self) -> i32 {
        self.to - self.from
    }
}

/// The overtime the company's drivers who work that day can do on open duty `d` of the plan `dp`
/// (`duties` the date's): the most each can take, the longest first. None for a part of a cut duty
/// or a duty cut already.
pub fn offers(c: &Company, dp: &DayPlan, d: &OpenDuty, duties: &[ProDuty]) -> Vec<Overtime> {
    if d.part || pc::split_of(c, &dp.date, d.n).is_some() {
        return Vec::new();
    }
    let units = pc::segs_of(dp, &d.pieces);
    let (Some(first), Some(last)) = (units.first(), units.last()) else { return Vec::new() };
    let (from, to) = (first.from(dp), last.to(dp));
    let from_stop = dp.tours[first.ti].tour.trips[first.a].from.clone();
    let to_stop = dp.tours[last.ti].tour.trips[last.b - 1].to.clone();
    // (the ways to cut it in two: the first parts, the longest first; the last parts, the
    // longest first - each carrying passengers)
    let points = pc::cut_points(c, dp, &units);
    let carries = |segs: &Vec<Seg>| segs.iter().any(|s| s.counts(dp));
    let mut heads: Vec<(Vec<Seg>, bool)> = vec![(units.clone(), true)];
    let mut tails: Vec<(Vec<Seg>, bool)> = vec![(units.clone(), true)];
    for p in points.iter().rev() {
        let parts = pc::parts_at(&units, &[*p]);
        if parts.len() == 2 && parts.iter().all(carries) {
            heads.push((parts[0].clone(), false));
        }
    }
    for p in points.iter() {
        let parts = pc::parts_at(&units, &[*p]);
        if parts.len() == 2 && parts.iter().all(carries) {
            tails.push((parts[1].clone(), false));
        }
    }
    let prefs = &c.planning.flow.prefs;
    let need = |a: &str, b: &str| if pc::same(a, b) { prefs.change.same_stop } else { prefs.change.other_stop };
    let mut out = Vec::new();
    for e in c.staff.iter().filter(|e| staff::drives(e)) {
        let mine = dp.duties_of(Who::Staff(e.id));
        if mine.is_empty() {
            continue;
        }
        let blocks: Vec<((usize, usize), staff::Block)> = mine.iter().map(|&(ti, k)| ((ti, k), plan::duty_block(dp, ti, k))).collect();
        let (first_on, last_off) = (blocks.iter().map(|b| b.1.from).min().unwrap_or(from), blocks.iter().map(|b| b.1.to).max().unwrap_or(to));
        let spread_ok = |a: i32, b: i32| last_off.max(b) - first_on.min(a) <= super::pro_rest::SPREAD_MAX;
        // (at most ten hours behind the wheel with his own; the game's other rules - his rest
        // between two pieces of work (`staff::REST`, twenty minutes), the bus - as for any piece:
        // "fri kl. 14:45 og en vagt skal starte kl. 15:00" is a quarter of an hour, too short by
        // the game's rest; 14:40 to 15:00 does)
        let own_minutes = staff::work_minutes(&blocks.iter().map(|b| b.1.clone()).collect::<Vec<_>>());
        let fits = |segs: &Vec<Seg>| -> bool {
            let more: i32 = segs.iter().map(|s| s.to(dp) - s.from(dp)).sum();
            own_minutes + more <= staff::DAY_MAX && fits_with(c, dp, e, blocks.iter().map(|b| b.1.clone()).collect(), segs)
        };
        // (going on after his own duty)
        if let Some(((ti, k), b)) = blocks.iter().filter(|(_, b)| b.to <= from).max_by_key(|(_, b)| b.to) {
            let gap = from - b.to;
            if pc::nearby(&b.to_stop, &from_stop) && gap >= need(&b.to_stop, &from_stop).max(staff::REST) && gap <= pc::NEAR_MINUTES {
                if let Some((segs, whole)) = heads.iter().find(|(segs, _)| spread_ok(segs[0].from(dp), segs.last().unwrap().to(dp)) && fits(segs)) {
                    let (f, t) = (segs[0].from(dp), segs.last().unwrap().to(dp));
                    out.push(Overtime { driver: e.id, earlier: false, own: pc::number_at(dp, duties, *ti, *k), at: b.to, segs: segs.clone(), from: f, to: t, whole: *whole });
                    continue;
                }
            }
        }
        // (beginning before his own duty)
        if let Some(((ti, k), b)) = blocks.iter().filter(|(_, b)| b.from >= to).min_by_key(|(_, b)| b.from) {
            let gap = b.from - to;
            if pc::nearby(&to_stop, &b.from_stop) && gap >= need(&to_stop, &b.from_stop).max(staff::REST) && gap <= pc::NEAR_MINUTES {
                if let Some((segs, whole)) = tails.iter().find(|(segs, _)| spread_ok(segs[0].from(dp), segs.last().unwrap().to(dp)) && fits(segs)) {
                    let (f, t) = (segs[0].from(dp), segs.last().unwrap().to(dp));
                    out.push(Overtime { driver: e.id, earlier: true, own: pc::number_at(dp, duties, *ti, *k), at: b.from, segs: segs.clone(), from: f, to: t, whole: *whole });
                }
            }
        }
    }
    out.sort_by_key(|o| (std::cmp::Reverse(o.minutes()), o.driver));
    out
}

/// Whether `e` can take the stretches `segs` beside his work `work` by the game's rules (`plan::can_take`).
fn fits_with(c: &Company, dp: &DayPlan, e: &super::model::Employee, mut work: Vec<staff::Block>, segs: &[Seg]) -> bool {
    if matches!(super::pro_turnus::decided(c, e.id, &dp.date).and_then(|d| d.note).as_deref(), Some("sick") | Some("holiday")) {
        return false;
    }
    for s in segs {
        let b = s.block(dp);
        if plan::can_take(e, &dp.date, dp.today, &dp.tours[s.ti].bus_of(c), &work, &b).is_err() {
            return false;
        }
        work.push(b);
    }
    true
}

/// The overtime of driver `driver` on open duty `d` (`offers`).
pub fn of_driver(c: &Company, dp: &DayPlan, d: &OpenDuty, duties: &[ProDuty], driver: u32) -> Option<Overtime> {
    offers(c, dp, d, duties).into_iter().find(|o| o.driver == driver)
}

/// Overtime `o` on open duty `d` made ready: the duty cut in two there for its date, unless it is
/// the whole duty. The part that is his (None: the whole duty).
pub fn take(c: &mut Company, dp: &DayPlan, d: &OpenDuty, o: &Overtime) -> Result<Option<u8>, &'static str> {
    if o.whole {
        return Ok(None);
    }
    let units = pc::segs_of(dp, &d.pieces);
    // (the rest: the stretches of the duty not his)
    let starts_of = |segs: &[Seg]| segs.iter().map(|s| (s.ti, s.a, s.b)).collect::<Vec<_>>();
    let mine = starts_of(&o.segs);
    let mut rest: Vec<Seg> = Vec::new();
    for u in &units {
        // (a stretch of the duty, less the part of it that is his)
        let cut = mine.iter().find(|(ti, a, b)| *ti == u.ti && *a < u.b && *b > u.a);
        match cut {
            None => rest.push(*u),
            Some(&(_, a, b)) => {
                if u.a < a {
                    rest.push(Seg { ti: u.ti, a: u.a, b: a });
                }
                if b < u.b {
                    rest.push(Seg { ti: u.ti, a: b, b: u.b });
                }
            }
        }
    }
    if rest.is_empty() {
        return Ok(None);
    }
    let plan_of = |segs: Vec<Seg>| pc::PartPlan { from: segs[0].from(dp), to: segs.last().unwrap().to(dp), segs, sugg: None };
    let parts = if o.earlier { vec![plan_of(rest), plan_of(o.segs.clone())] } else { vec![plan_of(o.segs.clone()), plan_of(rest)] };
    pc::cut(c, dp, d, &pc::Proposal { parts })?;
    Ok(Some(if o.earlier { 1 } else { 0 }))
}

#[cfg(test)]
#[path = "pro_overtime_tests.rs"]
mod tests;
