//! (NexiconDK, 15 November 2026: "Jeg har 3 ubesatte vagter i dag, de har kørt men jeg kan stadig
//! besætte dem ... Det burde man ikke kunne?") The duties that began on the company's day without a
//! driver. The day's close used to count a duty by its driver at midnight: one given a driver in the
//! evening, after it had run without one, ran all its trips - paid, on time, no fine.
//!
//! Now the clock looks (`control::watch_day`): a duty nobody drove when it began is kept with the
//! minute it was last seen so (`DayScript::unmanned`). A driver given later drives only what was
//! left: the trips that left before then are dropped (`day::why_not`). One that had nobody until
//! its end stays without a driver today, whatever is given afterwards (`plan::day_plan`), and the
//! pages no longer offer it (`ended`). One that had a driver when it began is never counted so,
//! even if the day's plan later shows it without one for a moment.

use super::clock;
use super::model::Company;
use super::plan::{duty_key, DayPlan};
use serde::{Deserialize, Serialize};

/// One duty of the day as the clock saw it when it began, and after.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Seen {
    /// `plan::duty_key`.
    pub key: String,
    /// Nobody drove it until this minute of the day (its beginning: somebody did from the start).
    pub until: i32,
    /// Still nobody when last seen.
    pub open: bool,
}

/// The clock's look at the company's day at minute `now` (`dp`: its plan now): each duty that has
/// begun kept as it began, with or without a driver, and one without kept until it gets one.
pub fn note(c: &mut Company, dp: &DayPlan, now: i32) {
    if !dp.today || c.clock.today.as_ref().is_none_or(|s| s.date != dp.date) {
        return;
    }
    let tp = dp.to_plan();
    let mut seen: Vec<(String, bool, i32, i32)> = Vec::new();
    for (ti, t) in dp.tours.iter().enumerate() {
        for (k, d) in t.duties.iter().enumerate() {
            if d.from > now {
                continue;
            }
            let (line, tour) = (&t.tour.line, &t.tour.tour);
            // (nobody: not the player's tour, not one the game runs live, not driven by the AI of
            // the server's world or for a player who left it - `day::why_not` runs those)
            let ai = tp.tours.get(ti).zip(tp.tours.get(ti).and_then(|x| x.duties.get(k))).is_some_and(|(x, y)| x.world_ai(c, y));
            let nobody = d.who.is_none() && !t.by_player && !t.live && !ai && !super::together::book::ai_on(c, &dp.date, line, tour, k);
            seen.push((duty_key(line, tour, k), nobody, d.from, d.to));
        }
    }
    let Some(s) = c.clock.today.as_mut() else { return };
    for (key, nobody, from, to) in seen {
        match s.unmanned.iter_mut().find(|x| x.key == key) {
            None => s.unmanned.push(Seen { key, until: if nobody { now.min(to) } else { from }, open: nobody }),
            Some(x) if x.open => {
                if nobody {
                    x.until = x.until.max(now.min(to));
                } else {
                    x.open = false;
                }
            }
            Some(_) => {}
        }
    }
}

/// Until when duty `k` of that tour had nobody today (None: it had a driver when it began, or the
/// clock has not seen it begin).
pub fn until(c: &Company, line: &str, tour: &str, k: usize) -> Option<i32> {
    let key = duty_key(line, tour, k);
    let s = c.clock.today.as_ref().filter(|s| s.date == c.date)?;
    // (one that had a driver from the start: until its beginning - no trip left before it)
    s.unmanned.iter().find(|x| x.key == key).map(|x| x.until)
}

/// That duty of `date` (ending at `to`) had nobody until its end: it stays without a driver today.
pub fn ran_without(c: &Company, date: &str, line: &str, tour: &str, k: usize, to: i32) -> bool {
    date == c.date && until(c, line, tour, k).is_some_and(|u| u >= to)
}

/// What ends at `to` (minutes of the plan's day) is over: on the company's day, its clock past it.
/// A duty without a driver that is over can no longer be given today.
pub fn ended(c: &Company, p: &DayPlan, to: i32) -> bool {
    p.today && clock::now(c) >= clock::moment(&p.date, to as i64)
}

// (its test: `plan`'s `a_duty_that_ran_without_a_driver_is_not_driven_by_one_given_afterwards`)
