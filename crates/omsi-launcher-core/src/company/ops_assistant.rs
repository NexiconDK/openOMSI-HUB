//! (NexiconDK, 15 November 2026: "at man kan ansætte en Drifts Assistent som kan overvåge
//! sygemeldinger osv. og kører biksen mens man er offline ... Evt. en vagtplan over Drift
//! Assistenter") The operations assistants of a company on Bus Company Online.
//!
//! - **Who**: employees of their own role (`Role::Operations`), hired as the other staff are, on a
//!   server's company only (off the server the player himself is there whenever the clock runs).
//! - **The shift plan** (`Desk::grid`): three shifts a day - early 04:00-12:00, late 12:00-20:00,
//!   night 20:00-04:00 - for each day of the week, an assistant's name in each or nobody. Set by
//!   hand (`set_slot`) or filled at once (`auto_fill`): at most `MOST_A_WEEK` shifts each, eleven
//!   hours of rest between two of his shifts. An assistant ill or on holiday is not at his desk.
//! - **When he acts** (decided 15 November 2026: "Når ingen chef er online"): while he is on shift
//!   and none of the company's bosses - its owner, a co-owner, a manager - is online
//!   (`Desk::chiefs_online`, the company's process tells it, `together::host::watch`).
//! - **What he does** (decided the same day: sick calls and open duties, breakdowns, the agency):
//!   * the duties of the day without a driver that sign on within `AHEAD` (a driver called in
//!     sick, nobody given it): one at a time, every `EVERY` minutes, to the first driver of the
//!     company who can take it in the order a duty is filled in (`pro_ops::candidates`: a reserve
//!     within his frame, a free colleague, an hourly-paid driver) - RosterFlow Pro's daily
//!     operations; the sick call seen;
//!   * nobody of the company can: an agency driver sent (`plan::Fill::Agency`) where the server
//!     lets the agency run them (`live_pay::agency_runs_open`) and the desk allows it
//!     (`Desk::no_agency` not set); else told - the duty falls out;
//!   * the standard planner: the morning's repair of the roster (`plan::repair_day`) as off the
//!     server, when an assistant works the day's early shift or is on at midnight;
//!   * breakdowns: at his desk the spare bus is sent as soon as a dispatcher on duty sends it
//!     (`live_more::dispatcher_on`).
//! - **What he did** (`Desk::log`) shows on his page and in the feed.

use super::clock::{self, FeedItem, Level};
use super::model::{Company, Employee, Role};
use super::plan::{self, DayPlan};
use super::pro_turnus::ProDuty;
use serde::{Deserialize, Serialize};

/// The shifts of a day (minutes of the day; the night's ends the next morning).
pub const SHIFTS: [(i32, i32); 3] = [(4 * 60, 12 * 60), (12 * 60, 20 * 60), (20 * 60, 28 * 60)];
/// Their names (English, translated when shown).
pub const SHIFT_NAMES: [&str; 3] = ["Early shift", "Late shift", "Night shift"];
/// The most shifts an assistant works a week.
pub const MOST_A_WEEK: usize = 5;
/// The least rest between two of his shifts (minutes).
pub const REST: i32 = 11 * 60;
/// How often an assistant at his desk looks at the day (minutes of the company's clock).
pub const EVERY: i64 = 5;
/// How far ahead he covers a duty without a driver (minutes before its sign-on).
pub const AHEAD: i32 = 180;
/// The entries of his log kept.
pub const LOG_KEPT: usize = 80;

/// Why a shift cannot be given (English, translated when shown).
pub const NOT_ASSISTANT: &str = "Only an operations assistant of the company can take a shift.";
pub const BAD_SHIFT: &str = "There is no such shift.";
pub const TOO_MANY: &str = "An operations assistant works at most five shifts a week.";
pub const NO_REST: &str = "Too little rest: an operations assistant needs eleven hours off between two shifts.";
pub const ON_SERVER_ONLY: &str = "Operations assistants work for companies on Bus Company Online: here you run the day yourself.";

/// What an assistant did (the feed's words and the log's).
pub const COVERED: &str = "%{who} gave duty %{n} (%{time}) to %{driver}";
/// (22 November 2026) A part of it as overtime: going on until, or beginning earlier from, `until`.
pub const OVERTIME: &str = "%{who} gave duty %{n} (%{time}) to %{driver} as overtime until %{until}; the rest is open";
pub const OVERTIME_EARLIER: &str = "%{who} gave duty %{n} (%{time}) to %{driver} as overtime from %{until}; the rest is open";
pub const AGENCY: &str = "%{who} sent an agency driver for duty %{n} (%{time})";
pub const NOBODY: &str = "%{who} found nobody for duty %{n} (%{time}): it falls out";
pub const REPAIRED: &str = "%{who} repaired today's roster: %{n} places given anew";

/// Every English text of the desk (for the translations' check).
pub fn texts() -> Vec<&'static str> {
    let mut v = SHIFT_NAMES.to_vec();
    v.extend([NOT_ASSISTANT, BAD_SHIFT, TOO_MANY, NO_REST, ON_SERVER_ONLY, COVERED, OVERTIME, OVERTIME_EARLIER, AGENCY, NOBODY, REPAIRED]);
    v
}

/// Something an assistant did: when (the company's moment), who, the feed's words and their
/// places.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Done {
    pub at: i64,
    pub who: u32,
    pub text: String,
    pub args: Vec<(String, String)>,
}

/// The operations assistants' desk (`Company::ops_desk`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Desk {
    /// The shift plan: `grid[shift][weekday]` (Monday 0) the assistant's id, 0 nobody.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub grid: Vec<[u32; 7]>,
    /// The agency is not to be sent for (the desk's choice; the server's rules first).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub no_agency: bool,
    /// What they did, the latest last.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<Done>,
    /// The company's moment the day was last looked at.
    #[serde(skip_serializing_if = "is_zero")]
    pub looked: i64,
    /// A boss of the company is online now (the company's process says it; not kept).
    #[serde(skip)]
    pub chiefs_online: bool,
}

fn is_zero(x: &i64) -> bool {
    *x == 0
}

impl Desk {
    /// Who works shift `shift` on weekday `wd` (Monday 0): 0 nobody.
    pub fn who(&self, shift: usize, wd: usize) -> u32 {
        self.grid.get(shift).and_then(|r| r.get(wd)).copied().unwrap_or(0)
    }

    fn grid_mut(&mut self) -> &mut Vec<[u32; 7]> {
        self.grid.resize(SHIFTS.len(), [0; 7]);
        &mut self.grid
    }
}

/// The company's operations assistants employed on `date`.
pub fn assistants<'a>(c: &'a Company, date: &str) -> Vec<&'a Employee> {
    c.staff.iter().filter(|e| e.role == Role::Operations && e.employed_on(date)).collect()
}

/// The shifts assistant `id` works a week by the plan: (shift, weekday).
pub fn shifts_of(c: &Company, id: u32) -> Vec<(usize, usize)> {
    (0..7).flat_map(|wd| (0..SHIFTS.len()).map(move |s| (s, wd))).filter(|&(s, wd)| id != 0 && c.ops_desk.who(s, wd) == id).collect()
}

/// When shift `s` of weekday `wd` begins and ends, in minutes of the week (Monday 00:00 is 0).
fn span(s: usize, wd: usize) -> (i32, i32) {
    let day = wd as i32 * 1440;
    (day + SHIFTS[s].0, day + SHIFTS[s].1)
}

/// The rest between two shifts of the week, either way round (the week goes round: Sunday's
/// night is followed by Monday's early).
fn rest_between(a: (usize, usize), b: (usize, usize)) -> i32 {
    const WEEK: i32 = 7 * 1440;
    let (a0, a1) = span(a.0, a.1);
    let (b0, b1) = span(b.0, b.1);
    let after = (b0 - a1).rem_euclid(WEEK);
    let before = (a0 - b1).rem_euclid(WEEK);
    after.min(before)
}

/// Whether assistant `id` may take shift `(s, wd)` besides those he has (`but` left out of them).
fn may_take(c: &Company, id: u32, s: usize, wd: usize) -> Result<(), &'static str> {
    let mine: Vec<(usize, usize)> = shifts_of(c, id).into_iter().filter(|x| *x != (s, wd)).collect();
    if mine.len() >= MOST_A_WEEK {
        return Err(TOO_MANY);
    }
    if mine.iter().any(|x| x.1 == wd || rest_between(*x, (s, wd)) < REST) {
        return Err(NO_REST);
    }
    Ok(())
}

/// Shift `s` of weekday `wd` given to assistant `who` (0: nobody).
pub fn set_slot(c: &mut Company, s: usize, wd: usize, who: u32) -> Result<(), &'static str> {
    if s >= SHIFTS.len() || wd >= 7 {
        return Err(BAD_SHIFT);
    }
    if who != 0 {
        if !c.employee(who).is_some_and(|e| e.role == Role::Operations) {
            return Err(NOT_ASSISTANT);
        }
        may_take(c, who, s, wd)?;
    }
    c.ops_desk.grid_mut()[s][wd] = who;
    Ok(())
}

/// The shift plan filled anew with the assistants employed today: as many shifts of the week as
/// the rules allow (a search, the nights first, each shift first to the one with the fewest - the
/// one who worked it the day before first, so each keeps to his shift). Returns how many shifts
/// have somebody.
pub fn auto_fill(c: &mut Company) -> usize {
    let today = c.date.clone();
    let ids: Vec<u32> = assistants(c, &today).iter().map(|e| e.id).collect();
    let slots: Vec<(usize, usize)> = [2, 0, 1].into_iter().flat_map(|s| (0..7).map(move |wd| (s, wd))).collect();
    let mut f = Fill { ids, slots, grid: vec![[0; 7]; SHIFTS.len()], best: vec![[0; 7]; SHIFTS.len()], best_n: 0, nodes: 0 };
    f.search(0, 0);
    let n = f.best_n;
    c.ops_desk.grid = f.best;
    n
}

/// `auto_fill`'s search over the week's shifts.
struct Fill {
    ids: Vec<u32>,
    slots: Vec<(usize, usize)>,
    grid: Vec<[u32; 7]>,
    best: Vec<[u32; 7]>,
    best_n: usize,
    nodes: u32,
}

impl Fill {
    /// The most nodes searched (the best plan found by then is kept).
    const MOST_NODES: u32 = 200_000;

    fn mine(&self, id: u32) -> Vec<(usize, usize)> {
        self.slots.iter().copied().filter(|&(s, wd)| self.grid[s][wd] == id).collect()
    }

    fn may(&self, id: u32, s: usize, wd: usize) -> bool {
        let mine = self.mine(id);
        mine.len() < MOST_A_WEEK && !mine.iter().any(|x| x.1 == wd || rest_between(*x, (s, wd)) < REST)
    }

    fn search(&mut self, k: usize, filled: usize) {
        if filled > self.best_n {
            self.best_n = filled;
            self.best = self.grid.clone();
        }
        self.nodes += 1;
        if k == self.slots.len() || filled + (self.slots.len() - k) <= self.best_n || self.nodes > Self::MOST_NODES {
            return;
        }
        let (s, wd) = self.slots[k];
        let mut order: Vec<u32> = self.ids.iter().copied().filter(|id| self.may(*id, s, wd)).collect();
        order.sort_by_key(|id| (self.mine(*id).len(), self.grid[s][(wd + 6) % 7] != *id, *id));
        for id in order {
            self.grid[s][wd] = id;
            self.search(k + 1, filled + 1);
            self.grid[s][wd] = 0;
        }
        self.search(k + 1, filled);
    }
}

/// The weekday of `date` (Monday 0).
fn weekday(date: &str) -> usize {
    plan::weekday_of(date) as usize
}

/// Who is at the desk at minute `minute` of `date` by the plan (an assistant employed then, not
/// ill, on holiday or on a course): his id and his shift. (Before 04:00, the night shift of the day
/// before.)
pub fn on_shift(c: &Company, date: &str, minute: i32) -> Option<(u32, usize)> {
    let (day, s) = if minute < SHIFTS[0].0 {
        (super::dates::add(date, -1), 2)
    } else {
        (date.to_string(), SHIFTS.iter().position(|x| minute < x.1).unwrap_or(2))
    };
    let id = c.ops_desk.who(s, weekday(&day));
    let e = c.employee(id)?;
    (e.role == Role::Operations && e.employed_on(&day) && !e.absent(&day)).then_some((id, s))
}

/// The assistant who runs the day now: at his desk, the company on a server, no boss online.
pub fn covering(c: &Company) -> Option<u32> {
    if !c.online.on_server || c.ops_desk.chiefs_online {
        return None;
    }
    on_shift(c, &c.date, c.clock.minute as i32).map(|x| x.0)
}

/// The assistants' shifts of `date` as the staff's pay counts them (`staff::shifts`): each shift
/// of the plan that begins that day, by an assistant employed and at work then.
pub fn day_shifts(c: &Company, date: &str) -> Vec<super::staff::Shift> {
    let wd = weekday(date);
    (0..SHIFTS.len())
        .filter_map(|s| {
            let e = c.employee(c.ops_desk.who(s, wd))?;
            (e.role == Role::Operations && e.employed_on(date) && !e.absent(date)).then(|| super::staff::Shift { id: e.id, role: Role::Operations, from: SHIFTS[s].0, to: SHIFTS[s].1, extra: None })
        })
        .collect()
}

/// The company may employ operations assistants (on a server only).
pub fn hireable(c: &Company) -> Result<(), &'static str> {
    if c.online.on_server {
        Ok(())
    } else {
        Err(ON_SERVER_ONLY)
    }
}

/// Whether the day is due a look at moment `at` (every `EVERY` minutes).
pub fn due(c: &Company, at: i64) -> bool {
    at - c.ops_desk.looked >= EVERY || at < c.ops_desk.looked
}

fn log(c: &mut Company, at: i64, who: u32, text: &str, args: Vec<(String, String)>) -> FeedItem {
    c.ops_desk.log.push(Done { at, who, text: text.to_string(), args: args.clone() });
    if c.ops_desk.log.len() > LOG_KEPT {
        let extra = c.ops_desk.log.len() - LOG_KEPT;
        c.ops_desk.log.drain(..extra);
    }
    FeedItem { at, text: text.to_string(), args, level: Level::Info }
}

/// Whether the log tells already of duty `label` on `date` (one word of each duty).
fn told(c: &Company, date: &str, label: &str) -> bool {
    let day0 = clock::moment(date, 0);
    c.ops_desk.log.iter().any(|d| d.at >= day0 && d.args.iter().any(|(k, v)| k == "n" && v == label))
}

/// The assistant at the desk looks at the company's day `dp` (`duties` its duties as RosterFlow
/// Pro's Vagter tab numbers them) at minute `now`: the first duty without a driver signing on
/// within `AHEAD` given to a driver of the company who can take it, else to the agency, else told.
/// One duty a look. What he did, for the feed.
pub fn look(c: &mut Company, dp: &DayPlan, duties: &[ProDuty], now: i32) -> Vec<FeedItem> {
    let date = c.date.clone();
    let at = clock::moment(&date, now as i64);
    c.ops_desk.looked = at;
    let Some(who) = covering(c) else { return Vec::new() };
    if !super::pro_duties::on(c) || dp.date != date {
        return Vec::new();
    }
    let name = c.employee(who).map(|e| e.name.clone()).unwrap_or_default();
    let open = super::pro_ops::open_duties(dp, duties);
    for d in open.iter().filter(|d| d.start > now && d.start - now <= AHEAD) {
        let label = duties.iter().find(|x| x.number == d.n).map(|x| x.shown()).unwrap_or_else(|| d.n.to_string());
        if told(c, &date, &label) {
            continue;
        }
        let time = super::rosterflow::clock(d.start.rem_euclid(1440));
        let mut args = vec![("who".to_string(), name.clone()), ("n".to_string(), label.clone()), ("time".to_string(), time)];
        // (the company's own drivers who can, in the order a duty is filled in - the agency
        // drivers booked for the day among them)
        // (NexiconDK, 22 November 2026: he rings them - each takes it as an assistant asks,
        // `pro_ops::ASSISTANT_CALL_YES`, the next one rung after a no)
        let best = super::pro_ops::candidates(c, dp, d, &|_| 0.0).into_iter().filter(|x| x.fit.is_ok()).find(|x| {
            let mut rng = super::rng::Rng::of(&[&c.id, &date, "assistant-call"], ((d.n as i64) << 24) | x.id as i64);
            rng.chance(super::pro_ops::ASSISTANT_CALL_YES)
        });
        if let Some(x) = best {
            if super::pro_ops::give(c, &date, d.n, x.id, None).is_ok() {
                let driver = c.employee(x.id).map(|e| e.name.clone()).unwrap_or_default();
                // (the sick driver's call is seen: his duty is covered)
                let sick: Vec<u32> = c.planning.ops.sick.iter().filter(|s| s.date == date && s.n == d.n).map(|s| s.driver).collect();
                for id in sick {
                    super::pro_sick::seen(c, Some(id));
                }
                args.push(("driver".to_string(), driver));
                return vec![log(c, at, who, COVERED, args)];
            }
        }
        // (NexiconDK, 22 November 2026: nobody free said yes - a driver who works that day rung
        // about overtime, its part his, the rest of the duty left open for the next look)
        let extra = super::pro_overtime::offers(c, dp, d, duties).into_iter().find(|o| {
            let mut rng = super::rng::Rng::of(&[&c.id, &date, "assistant-overtime"], ((d.n as i64) << 24) | o.driver as i64);
            rng.chance(super::pro_ops::ASSISTANT_CALL_YES)
        });
        if let Some(o) = extra {
            if let Ok(part) = super::pro_overtime::take(c, dp, d, &o) {
                if super::pro_ops::give_part(c, &date, d.n, part, o.driver, None).is_ok() {
                    let driver = c.employee(o.driver).map(|e| e.name.clone()).unwrap_or_default();
                    args.push(("driver".to_string(), driver));
                    args.push(("until".to_string(), super::rosterflow::clock((if o.earlier { o.from } else { o.to }).rem_euclid(1440))));
                    return vec![log(c, at, who, if o.earlier { OVERTIME_EARLIER } else { OVERTIME }, args)];
                }
            }
        }
        if !c.ops_desk.no_agency && super::live_pay::agency_runs_open(c) {
            for &(ti, k) in d.pieces.iter().filter(|&&(ti, k)| dp.tours[ti].duties[k].who.is_none()) {
                let t = &dp.tours[ti].tour;
                plan::set_fill(c, &plan::duty_key(&t.line, &t.tour, k), Some(plan::Fill::Agency));
            }
            return vec![log(c, at, who, AGENCY, args)];
        }
        return vec![log(c, at, who, NOBODY, args)];
    }
    Vec::new()
}

/// The company's morning on a server with the standard planner: an assistant who works the day's
/// early shift (or is at the desk at midnight) repairs the roster as the dispatcher does off the
/// server (`plan::repair_day`), when no boss is online. Whether he did.
pub fn morning(c: &mut Company, date: &str, tours: Vec<super::network::TourOfDay>) -> bool {
    if !c.online.on_server || c.ops_desk.chiefs_online || super::pro_duties::on(c) {
        return false;
    }
    let early = c.ops_desk.who(0, weekday(date));
    let who = on_shift(c, date, 0).map(|x| x.0).or_else(|| c.employee(early).filter(|e| e.role == Role::Operations && e.employed_on(date) && !e.absent(date)).map(|e| e.id));
    let Some(who) = who else { return false };
    let r = plan::repair_day(c, date, tours);
    if r.broken == 0 {
        return false;
    }
    plan::tell_repaired(c, &r);
    let name = c.employee(who).map(|e| e.name.clone()).unwrap_or_default();
    let item = log(c, clock::moment(date, 0), who, REPAIRED, vec![("who".to_string(), name), ("n".to_string(), (r.duties + r.buses).to_string())]);
    c.clock.feed.push(item);
    true
}

#[cfg(test)]
#[path = "ops_assistant_tests.rs"]
mod tests;
