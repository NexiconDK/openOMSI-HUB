//! (NexiconDK, ticket inspection round T, 10 November 2026) The boss plans the ticket inspection:
//! inspection duties ("kontrolvagter") for the days of the week, and single dates that differ.
//! A duty checks one or more lines (or the busiest line nobody else checks) within its hours
//! (2 to 10, past midnight too), with a team of a size or of inspectors named; or it is a
//! "storaktion": many inspectors at one stop, who board every bus of the company's lines passing
//! it within the hours and check everyone aboard - more are caught there, and every line through
//! the stop keeps fewer dodgers for the days after (`ACTION_SHARE`, `dodgers::deterrence`).
//!
//! A day with duties planned replaces the automatic patrols (`staff::patrols`: two by two, the
//! busiest lines first) for that date; a day without stays automatic. The inspectors' hours follow
//! the duties (`staff::shifts`): who is at work that day (employed, not away, on the rota), rested
//! (`REST` since yesterday's end) and not too many days in a row (`DAYS_IN_A_ROW`) - named ones
//! first, the free ones for the rest, the least stressed first; one duty a day each. What is
//! missing is said (`Assigned::missing`); a duty nobody can work is dropped. Inspectors at work by
//! the rota without a duty on a planned day (`spare`) work their role's core hours and patrol as
//! the automatic ones do - two by two, the busiest lines no duty checks that day first, a lone one
//! at `staff::LONE` - paid as any day's work.
//!
//! The stops a storaktion may stand at, and when the buses of each line pass them, come from the
//! timetable read (`stops_of`) and are kept with the duty (`Pass`): the day's close has no stops.

use super::dates;
use super::model::{Company, Employee, Role};
use super::staff::{self, Patrol};
use serde::{Deserialize, Serialize};

/// A duty's hours: two at least, ten at most.
pub const MIN_LEN: i32 = 120;
pub const MAX_LEN: i32 = 600;
/// A patrol's team, and a storaktion's.
pub const TEAM: (u8, u8) = (1, 4);
pub const ACTION_TEAM: (u8, u8) = (2, 8);
/// Rest between two working days (minutes), and the most days at work in a row.
pub const REST: i32 = 11 * 60;
pub const DAYS_IN_A_ROW: i64 = 6;
/// What a storaktion counts as on every line through its stop: most of a day checked (what the
/// lines keep off that day and the next two weeks, `dodgers::deterrence`).
pub const ACTION_SHARE: f64 = 0.8;
/// At a storaktion each inspector checks this many of a bus's passengers, and fewer get away.
pub const ACTION_EACH: f64 = 14.0;
pub const ACTION_ESCAPE: f64 = 0.05;

/// The plan kept (`Company::inspection_plan`; a file of before: nothing planned).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// The duties of the days of the week.
    #[serde(default)]
    pub duties: Vec<Duty>,
    /// Single dates planned otherwise (their duties replace the weekday's; none: automatic).
    #[serde(default)]
    pub dates: Vec<DateDuties>,
    /// The last duty number given.
    #[serde(default)]
    pub next: u32,    /// (round U) How the inspectors look in the game (`inspection_game::Look`).
    #[serde(default)]
    pub look: super::inspection_game::Look,
    /// (round V) The players' ticket-inspection duties (`inspection_patrol`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub players: Vec<super::inspection_patrol::PlayerPatrol>,
}

/// A date planned otherwise.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct DateDuties {
    pub date: String,
    #[serde(default)]
    pub duties: Vec<Duty>,
}

/// What a duty does.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// A team rides its lines' buses one after the other.
    #[default]
    Patrol,
    /// Many inspectors at one stop board every bus passing it.
    Action,
}

/// An inspection duty.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Duty {
    pub id: u32,
    /// The days of the week (0 Monday … 6 Sunday) - a weekly duty's; a date's has none.
    #[serde(default)]
    pub days: Vec<u8>,
    #[serde(default)]
    pub kind: Kind,
    /// The lines (`CompanyLine::name`) a patrol checks, one after the other; none: the busiest
    /// line nobody else checks that day.
    #[serde(default)]
    pub lines: Vec<String>,
    /// Its hours (minutes of the day; `to` past midnight is more than 1440).
    pub from: i32,
    pub to: i32,
    /// How many work it (when nobody is named).
    #[serde(default)]
    pub size: u8,
    /// The inspectors named for it (employee ids).
    #[serde(default)]
    pub named: Vec<u32>,
    /// A storaktion's stop, and when the buses of each line pass it.
    #[serde(default)]
    pub stop: String,
    #[serde(default)]
    pub through: Vec<Pass>,
}

/// When the buses of a line pass a stop: `offset` minutes after leaving `from` (a trip's first
/// stop; empty: any).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Pass {
    pub line: String,
    #[serde(default)]
    pub from: String,
    pub offset: i32,
}

/// A stop of the timetable read, the lines through it and when their buses pass (`stops_of`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StopLines {
    pub stop: String,
    pub passes: Vec<Pass>,
}

impl StopLines {
    /// The lines through it (`CompanyLine::name`), each once.
    pub fn lines(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for p in &self.passes {
            if !out.iter().any(|l| l.eq_ignore_ascii_case(&p.line)) {
                out.push(p.line.clone());
            }
        }
        out
    }
}

impl Duty {
    /// The lines a storaktion's buses come by (`through`), each once.
    pub fn action_lines(&self) -> Vec<String> {
        StopLines { stop: self.stop.clone(), passes: self.through.clone() }.lines()
    }

    /// How many it asks for: the named, else its size.
    pub fn wants(&self) -> usize {
        if self.named.is_empty() {
            self.size.max(1) as usize
        } else {
            self.named.len()
        }
    }
}

/// Why a duty cannot be kept as it is (English: keys of the translations).
pub const BAD_HOURS: &str = "An inspection duty lasts 2 to 10 hours.";
pub const BAD_TEAM: &str = "A patrol has 1 to 4 inspectors, a storaktion 2 to 8.";
pub const NO_DAYS: &str = "Choose the days of the week the duty is on.";
pub const NO_STOP: &str = "Choose the stop of the storaktion: one the company's lines pass.";
pub const NO_LINE: &str = "The company has no such line.";
pub const NO_INSPECTOR: &str = "That inspector does not work for the company.";
pub const NO_DUTY: &str = "There is no such inspection duty.";
pub const PAST_DATE: &str = "Only today and later dates can be planned.";
pub const DATE_TAKEN: &str = "That date is planned already.";

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// A duty as the page sent it, put right and checked (`weekly`: a duty of the week, which needs
/// its days).
fn checked(c: &Company, mut d: Duty, weekly: bool) -> Result<Duty, &'static str> {
    d.from = d.from.rem_euclid(1440);
    let len = d.to - d.from;
    let len = if len <= 0 { len + 1440 } else { len };
    if !(MIN_LEN..=MAX_LEN).contains(&len) {
        return Err(BAD_HOURS);
    }
    d.to = d.from + len;
    d.days.sort_unstable();
    d.days.dedup();
    d.days.retain(|x| *x < 7);
    if weekly && d.days.is_empty() {
        return Err(NO_DAYS);
    }
    if !weekly {
        d.days.clear();
    }
    let (lo, hi) = if d.kind == Kind::Action { ACTION_TEAM } else { TEAM };
    d.named.dedup();
    if d.named.is_empty() {
        if !(lo..=hi).contains(&d.size) {
            return Err(BAD_TEAM);
        }
    } else {
        if d.named.len() > hi as usize || (d.named.len() < lo as usize) {
            return Err(BAD_TEAM);
        }
        if d.named.iter().any(|id| c.employee(*id).is_none_or(|e| e.role != Role::Inspector)) {
            return Err(NO_INSPECTOR);
        }
        d.size = d.named.len() as u8;
    }
    match d.kind {
        Kind::Patrol => {
            d.stop.clear();
            d.through.clear();
            for l in d.lines.iter_mut() {
                let Some(cl) = c.lines.iter().find(|x| same(&x.name, l)) else { return Err(NO_LINE) };
                *l = cl.name.clone();
            }
            let mut kept: Vec<String> = Vec::new();
            for l in std::mem::take(&mut d.lines) {
                if !kept.contains(&l) {
                    kept.push(l);
                }
            }
            d.lines = kept;
        }
        Kind::Action => {
            d.lines.clear();
            d.through.retain(|p| c.lines.iter().any(|x| same(&x.name, &p.line)));
            if d.stop.trim().is_empty() || d.through.is_empty() {
                return Err(NO_STOP);
            }
            d.stop = d.stop.trim().to_string();
        }
    }
    Ok(d)
}

/// Keep duty `d` (its `id` 0: a new one): a weekly duty, or one of date `date`'s (a date planned
/// otherwise; it is so from now on). Returns its number.
pub fn save(c: &mut Company, date: Option<&str>, d: Duty) -> Result<u32, &'static str> {
    let mut d = checked(c, d, date.is_none())?;
    if let Some(date) = date {
        if date < c.date.as_str() {
            return Err(PAST_DATE);
        }
    }
    // (an edited duty leaves where it was)
    if d.id != 0 {
        let known = c.inspection_plan.duties.iter().any(|x| x.id == d.id) || c.inspection_plan.dates.iter().any(|x| x.duties.iter().any(|y| y.id == d.id));
        if !known {
            return Err(NO_DUTY);
        }
        c.inspection_plan.duties.retain(|x| x.id != d.id);
        for x in c.inspection_plan.dates.iter_mut() {
            x.duties.retain(|y| y.id != d.id);
        }
    } else {
        c.inspection_plan.next += 1;
        d.id = c.inspection_plan.next;
    }
    let id = d.id;
    match date {
        None => c.inspection_plan.duties.push(d),
        Some(date) => {
            let k = match c.inspection_plan.dates.iter().position(|x| x.date == date) {
                Some(k) => k,
                None => {
                    c.inspection_plan.dates.push(DateDuties { date: date.to_string(), duties: Vec::new() });
                    c.inspection_plan.dates.len() - 1
                }
            };
            c.inspection_plan.dates[k].duties.push(d);
        }
    }
    sort(&mut c.inspection_plan);
    Ok(id)
}

fn sort(p: &mut Plan) {
    p.duties.sort_by_key(|d| (d.days.first().copied().unwrap_or(9), d.from, d.id));
    for x in p.dates.iter_mut() {
        x.duties.sort_by_key(|d| (d.from, d.id));
    }
    p.dates.sort_by(|a, b| a.date.cmp(&b.date));
}

/// Take duty `id` out of the plan.
pub fn remove(c: &mut Company, id: u32) -> Result<(), &'static str> {
    let before = c.inspection_plan.duties.len() + c.inspection_plan.dates.iter().map(|x| x.duties.len()).sum::<usize>();
    c.inspection_plan.duties.retain(|x| x.id != id);
    for x in c.inspection_plan.dates.iter_mut() {
        x.duties.retain(|y| y.id != id);
    }
    let after = c.inspection_plan.duties.len() + c.inspection_plan.dates.iter().map(|x| x.duties.len()).sum::<usize>();
    if after == before {
        return Err(NO_DUTY);
    }
    Ok(())
}

/// Plan date `date` otherwise: its weekday's duties copied to it, to be changed there (none: the
/// day is automatic).
pub fn add_date(c: &mut Company, date: &str) -> Result<(), &'static str> {
    if dates::parse(date).is_none() || date < c.date.as_str() {
        return Err(PAST_DATE);
    }
    if c.inspection_plan.dates.iter().any(|x| x.date == date) {
        return Err(DATE_TAKEN);
    }
    let wd = weekday(date);
    let mut duties: Vec<Duty> = c.inspection_plan.duties.iter().filter(|d| d.days.contains(&wd)).cloned().collect();
    for d in duties.iter_mut() {
        c.inspection_plan.next += 1;
        d.id = c.inspection_plan.next;
        d.days.clear();
    }
    c.inspection_plan.dates.push(DateDuties { date: date.to_string(), duties });
    sort(&mut c.inspection_plan);
    Ok(())
}

/// Date `date` back to its weekday's duties.
pub fn remove_date(c: &mut Company, date: &str) -> Result<(), &'static str> {
    let n = c.inspection_plan.dates.len();
    c.inspection_plan.dates.retain(|x| x.date != date);
    if c.inspection_plan.dates.len() == n {
        return Err(NO_DUTY);
    }
    Ok(())
}

/// Dates gone by are let go (the day's close).
pub fn prune(c: &mut Company, today: &str) {
    c.inspection_plan.dates.retain(|x| x.date.as_str() >= today);
}

/// 0 Monday … 6 Sunday.
pub fn weekday(date: &str) -> u8 {
    dates::parse(date).map(dates::weekday).unwrap_or(0) as u8
}

/// The duties planned for `date`: a date planned otherwise, else its weekday's; None: none - the
/// day is automatic (`staff::patrols`).
pub fn duties_on(c: &Company, date: &str) -> Option<Vec<Duty>> {
    let p = &c.inspection_plan;
    let list: Vec<Duty> = match p.dates.iter().find(|x| x.date == date) {
        Some(x) => x.duties.clone(),
        None => {
            let wd = weekday(date);
            p.duties.iter().filter(|d| d.days.contains(&wd)).cloned().collect()
        }
    };
    (!list.is_empty()).then_some(list)
}

/// A duty with the inspectors who work it.
#[derive(Clone, Debug, PartialEq)]
pub struct Assigned {
    pub duty: Duty,
    pub ids: Vec<u32>,
    /// How many it lacks (named ones away, too few free).
    pub missing: u32,
}

impl Assigned {
    /// Nobody can work it: it is dropped.
    pub fn dropped(&self) -> bool {
        self.ids.is_empty()
    }
}

/// When someone's working day of `date` ends (minutes from its start; past midnight more than
/// 1440), by the plan without the rest's check, else by the role's hours; None: not at work.
fn day_end(c: &Company, e: &Employee, date: &str) -> Option<i32> {
    if !staff::at_work(e, date) {
        return None;
    }
    let h = c.staff_rules.hours_of(Role::Inspector);
    let core = h.from + h.length();
    match duties_on(c, date) {
        // (a duty's end; who had none worked the core hours)
        Some(duties) => Some(assign(c, date, &duties, false).into_iter().find(|a| a.ids.contains(&e.id)).map_or(core, |a| a.duty.to)),
        None => Some(core),
    }
}

/// Whether inspector `e` may work hours from `from` on `date`: at work, rested since yesterday
/// (`rest`: looked at), not too many days in a row.
fn free_for(c: &Company, e: &Employee, date: &str, from: i32, rest: bool) -> bool {
    if !staff::at_work(e, date) {
        return false;
    }
    if !rest {
        return true;
    }
    let yesterday = dates::add(date, -1);
    if let Some(end) = day_end(c, e, &yesterday) {
        if from + 1440 - end < REST {
            return false;
        }
    }
    let in_a_row = (1..=DAYS_IN_A_ROW).take_while(|k| staff::at_work(e, &dates::add(date, -k))).count() as i64;
    in_a_row < DAYS_IN_A_ROW
}

/// Who works each of `duties` on `date` (`rest`: yesterday's end and the days in a row looked at):
/// the duties by their start, the named first, the free ones - the least stressed - for the rest;
/// a storaktion with fewer than two is dropped.
fn assign(c: &Company, date: &str, duties: &[Duty], rest: bool) -> Vec<Assigned> {
    let mut order: Vec<&Duty> = duties.iter().collect();
    order.sort_by_key(|d| (d.from, d.id));
    let mut pool: Vec<&Employee> = c.staff.iter().filter(|e| e.role == Role::Inspector && e.employed_on(date)).collect();
    pool.sort_by(|a, b| a.stress.total_cmp(&b.stress).then(a.id.cmp(&b.id)));
    let mut used: Vec<u32> = Vec::new();
    let mut out = Vec::new();
    for d in order {
        let want = d.wants();
        let mut ids: Vec<u32> = Vec::new();
        for id in &d.named {
            if let Some(e) = c.employee(*id) {
                if !used.contains(id) && free_for(c, e, date, d.from, rest) {
                    ids.push(*id);
                }
            }
        }
        for e in &pool {
            if ids.len() >= want {
                break;
            }
            if !used.contains(&e.id) && !ids.contains(&e.id) && !d.named.contains(&e.id) && free_for(c, e, date, d.from, rest) {
                ids.push(e.id);
            }
        }
        // (a storaktion takes two at least: fewer, and it is dropped - they stay free for the rest)
        if d.kind == Kind::Action && ids.len() < ACTION_TEAM.0 as usize {
            ids.clear();
        }
        used.extend(ids.iter().copied());
        out.push(Assigned { duty: d.clone(), missing: want.saturating_sub(ids.len()) as u32, ids });
    }
    out
}

/// The duties of `date` with who works them; None: nothing planned (automatic).
pub fn assigned(c: &Company, date: &str) -> Option<Vec<Assigned>> {
    duties_on(c, date).map(|d| assign(c, date, &d, true))
}

/// The inspectors at work by the rota on a planned day without a duty (rested, not too many days in
/// a row for the core hours): they work the core hours and patrol automatically (`patrols`), the
/// least stressed first.
pub fn spare(c: &Company, date: &str, a: &[Assigned]) -> Vec<u32> {
    let from = c.staff_rules.hours_of(Role::Inspector).from;
    let mut people: Vec<&Employee> = c
        .staff
        .iter()
        .filter(|e| e.role == Role::Inspector && e.employed_on(date) && !a.iter().any(|x| x.ids.contains(&e.id)))
        .filter(|e| free_for(c, e, date, from, true))
        .collect();
    people.sort_by(|x, y| x.stress.total_cmp(&y.stress).then(x.id.cmp(&y.id)));
    people.into_iter().map(|e| e.id).collect()
}

/// The inspectors' hours on a planned day: each the hours of their duty, the spare ones the core
/// hours (`spare`).
pub fn shifts(c: &Company, date: &str, a: &[Assigned]) -> Vec<staff::Shift> {
    let h = c.staff_rules.hours_of(Role::Inspector);
    let mut out: Vec<staff::Shift> = a.iter().flat_map(|x| x.ids.iter().map(|id| staff::Shift { id: *id, role: Role::Inspector, from: x.duty.from, to: x.duty.to, extra: None })).collect();
    out.extend(spare(c, date, a).into_iter().map(|id| staff::Shift { id, role: Role::Inspector, from: h.from, to: h.from + h.length(), extra: None }));
    out.sort_by_key(|s| (s.from, s.id));
    out
}

/// The duty's lines that run: a patrol's chosen (in service), else the busiest line not taken.
fn lines_of(c: &Company, date: &str, d: &Duty, taken: &[String]) -> Vec<String> {
    let running = |name: &str| c.lines.iter().any(|l| same(&l.name, name) && super::network::in_service(l, super::clock::moment(date, 24 * 60 - 1)));
    if !d.lines.is_empty() {
        return d.lines.iter().filter(|l| running(l)).cloned().collect();
    }
    let mut lines: Vec<&super::model::CompanyLine> = c.lines.iter().filter(|l| running(&l.name)).collect();
    lines.sort_by(|a, b| b.km.total_cmp(&a.km).then(a.name.cmp(&b.name)));
    lines.iter().find(|l| !taken.contains(&l.name)).or(lines.first()).map(|l| vec![l.name.clone()]).unwrap_or_default()
}

/// The patrols of a planned day (`staff::patrols`): a patrol's team rides its lines one after the
/// other, its hours shared between them; a storaktion stands at its stop for every line through it.
pub fn patrols(c: &Company, date: &str, a: &[Assigned]) -> Vec<Patrol> {
    let mut out: Vec<Patrol> = Vec::new();
    let mut taken: Vec<String> = a.iter().flat_map(|x| x.duty.lines.clone()).collect();
    for x in a.iter().filter(|x| !x.dropped()) {
        let skill = x.ids.iter().map(|id| staff::skill_of(c, *id)).sum::<f64>() / x.ids.len() as f64;
        let d = &x.duty;
        match d.kind {
            Kind::Patrol => {
                let lines = lines_of(c, date, d, &taken);
                taken.extend(lines.iter().cloned());
                if lines.is_empty() {
                    continue;
                }
                let alone = if x.ids.len() >= 2 { 1.0 } else { staff::LONE };
                let seg = (d.to - d.from) / lines.len() as i32;
                for (k, l) in lines.iter().enumerate() {
                    let from = d.from + seg * k as i32;
                    let to = if k + 1 == lines.len() { d.to } else { from + seg };
                    out.push(Patrol { line: l.clone(), ids: x.ids.clone(), from, to, share: ((to - from) as f64 / 480.0 * skill * alone).min(1.0), duty: d.id, stop: None });
                }
            }
            Kind::Action => {
                for l in d.action_lines() {
                    if c.lines.iter().any(|cl| same(&cl.name, &l)) {
                        taken.push(l.clone());
                        out.push(Patrol { line: l, ids: x.ids.clone(), from: d.from, to: d.to, share: ACTION_SHARE, duty: d.id, stop: Some(d.stop.clone()) });
                    }
                }
            }
        }
    }
    // (the spare ones: two by two in the core hours, on the busiest lines no duty checks - when
    // every line is checked, the one whose cover in their hours is thinnest - a lone one at `LONE`)
    let spare = spare(c, date, a);
    if !spare.is_empty() {
        let mut lines: Vec<&super::model::CompanyLine> = c.lines.iter().filter(|l| super::network::in_service(l, super::clock::moment(date, 24 * 60 - 1))).collect();
        lines.sort_by(|a, b| b.km.total_cmp(&a.km).then(a.name.cmp(&b.name)));
        let h = c.staff_rules.hours_of(Role::Inspector);
        let (from, to) = (h.from, h.from + h.length());
        // (a line whose passengers ride at night - by its last days kept - has nothing to check in
        // core hours by day: the day's lines first, the ones no duty checks before the others)
        let by_day = from.rem_euclid(1440) < 20 * 60 && from.rem_euclid(1440) >= 5 * 60;
        let night_line = |name: &str| by_day && night_share(c, name) >= 0.8;
        let day: Vec<String> = lines.iter().filter(|l| !night_line(&l.name)).map(|l| l.name.clone()).collect();
        let pool: Vec<String> = if day.is_empty() { lines.iter().map(|l| l.name.clone()).collect() } else { day };
        let checked = staff::Shift { id: 0, role: Role::Inspector, from, to, extra: None }.within(5 * 60, 24 * 60) as f64 / 480.0;
        for team in spare.chunks(2) {
            // (a line no duty checks that day, the busiest first; when every line is checked, the
            // busiest whose planned cover within these hours is thinnest - the planned patrols' and
            // a storaktion's minutes in the hours, then the spare teams' before)
            let cover = |line: &str, planned: bool| -> i32 { out.iter().filter(|p| same(&p.line, line) && (p.duty != 0) == planned).map(|p| (p.to.min(to) - p.from.max(from)).max(0)).sum() };
            let line = match pool.iter().find(|l| !taken.iter().any(|t| same(t, l))) {
                Some(l) => l.clone(),
                // (the planned cover first: a spare team goes beside another spare one before it
                // goes beside a duty)
                None => match pool.iter().enumerate().min_by_key(|(k, l)| (cover(l, true), cover(l, false), *k)) {
                    Some((_, l)) => l.clone(),
                    None => break,
                },
            };
            taken.push(line.clone());
            let skill = team.iter().map(|id| staff::skill_of(c, *id)).sum::<f64>() / team.len() as f64;
            let alone = if team.len() >= 2 { 1.0 } else { staff::LONE };
            out.push(Patrol { line, ids: team.to_vec(), from, to, share: (checked * skill * alone).min(1.0), duty: 0, stop: None });
        }
    }
    out
}

/// The share of a line's passengers of its last days kept who rode in the evening or at night
/// (`inspection::LineDay`; 0 when nothing is kept) - read directly, not by `dodgers::of_line`,
/// which asks the patrols again.
fn night_share(c: &Company, line: &str) -> f64 {
    let kept: Vec<&super::inspection::LineDay> = c.inspection.days.iter().rev().take(7).filter_map(|d| d.lines.iter().find(|l| same(&l.line, line))).collect();
    let pax: f64 = kept.iter().map(|l| l.passengers as f64).sum();
    if pax <= 0.0 {
        return 0.0;
    }
    kept.iter().map(|l| l.night + l.evening).sum::<f64>() / pax
}

/// When a trip of `line` leaving `from` at `dep` (arriving `arr`) passes a storaktion's stop.
pub fn passes_at(d: &Duty, line: &str, from: &str, dep: i32, arr: i32) -> Option<i32> {
    let of_line: Vec<&Pass> = d.through.iter().filter(|p| same(&p.line, line)).collect();
    if of_line.is_empty() {
        return None;
    }
    let p = of_line.iter().find(|p| same(&p.from, from)).or_else(|| of_line.iter().find(|p| p.from.trim().is_empty()));
    Some(match p {
        Some(p) => dep + p.offset,
        // (another route of the line: halfway, as near as is known)
        None => dep + (arr - dep) / 2,
    })
}

/// The stops of the timetable read (`lines`) where the company's lines call, the busiest first:
/// each with when the buses of each line pass it (from each first stop).
pub fn stops_of(c: &Company, lines: &[crate::LineInfo]) -> Vec<StopLines> {
    let mut out: Vec<StopLines> = Vec::new();
    for cl in &c.lines {
        for li in lines.iter().filter(|l| same(&l.name, &cl.name)) {
            for y in li.tours.iter().flat_map(|t| t.trips.iter()) {
                for s in &y.stops {
                    let name = s.name.trim();
                    if name.is_empty() {
                        continue;
                    }
                    let offset = ((s.dep - y.departure) / 60.0).round() as i32;
                    let k = match out.iter().position(|x| same(&x.stop, name)) {
                        Some(k) => k,
                        None => {
                            out.push(StopLines { stop: name.to_string(), passes: Vec::new() });
                            out.len() - 1
                        }
                    };
                    let passes = &mut out[k].passes;
                    if !passes.iter().any(|p| same(&p.line, &cl.name) && same(&p.from, &y.from)) {
                        passes.push(Pass { line: cl.name.clone(), from: y.from.trim().to_string(), offset: offset.max(0) });
                    }
                }
            }
        }
    }
    out.sort_by(|a, b| b.lines().len().cmp(&a.lines().len()).then(b.passes.len().cmp(&a.passes.len())).then(a.stop.cmp(&b.stop)));
    out
}

#[cfg(test)]
mod tests {
    use super::super::model::{CompanyLine, GameMode};
    use super::super::research::keys;
    use super::super::{found, inspection, Founding};
    use super::*;

    fn company() -> Company {
        let mut c = found(&Founding { name: "Kontrol".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.country = "dk".into();
        c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), km: 300.0, service_from: Some(0), ..Default::default() });
        c.lines.push(CompanyLine { name: "Linie7".into(), number: "7".into(), km: 100.0, service_from: Some(0), ..Default::default() });
        c.lines.push(CompanyLine { name: "Linie9".into(), number: "9".into(), km: 50.0, service_from: Some(0), ..Default::default() });
        c
    }

    /// `n` inspectors, nobody away.
    fn with_inspectors(c: &mut Company, n: usize) -> Vec<u32> {
        c.research.done.push(keys::TICKET_INSPECTORS.to_string());
        let (id0, mut round, mut hired) = (c.id.clone(), 0, 0);
        while hired < n {
            for a in staff::applicants_of(c, Role::Inspector) {
                if hired < n {
                    staff::hire(c, &a).unwrap();
                    hired += 1;
                }
            }
            round += 1;
            c.id = format!("{id0}{round}");
            c.taken = Default::default();
        }
        c.id = id0;
        for e in c.staff.iter_mut() {
            (e.sick_until, e.holiday_until) = (None, None);
            e.stress = 0.0;
        }
        c.staff.iter().filter(|e| e.role == Role::Inspector).map(|e| e.id).collect()
    }

    /// The first date from today on that all of `ids` are at work, and the days after it too.
    fn all_at_work(c: &Company, ids: &[u32], days: i64) -> String {
        (0..40).map(|k| dates::add(&c.date, k)).find(|d| (0..days).all(|j| ids.iter().all(|id| staff::at_work(c.employee(*id).unwrap(), &dates::add(d, j))))).expect("such days")
    }

    fn patrol(days: &[u8], lines: &[&str], from: i32, to: i32, size: u8) -> Duty {
        Duty { days: days.to_vec(), lines: lines.iter().map(|x| x.to_string()).collect(), from, to, size, ..Default::default() }
    }

    fn trips(line: &str, from: &str) -> Vec<inspection::Seen> {
        (0..57).map(|k| 5 * 60 + 20 * k).map(|dep| inspection::Seen { line: line.into(), number: line.trim_start_matches("Linie").into(), dep, arr: dep + 30, from: from.into(), to: "Markt".into(), pax: 40, share: 0.05, crowded: false, stops: 0 }).collect()
    }

    #[test]
    fn an_empty_plan_is_the_automatic_one() {
        let mut c = company();
        let ids = with_inspectors(&mut c, 4);
        let date = all_at_work(&c, &ids, 1);
        assert!(duties_on(&c, &date).is_none() && assigned(&c, &date).is_none());
        let p = staff::patrols(&c, &date);
        assert_eq!(p.len(), 2);
        assert!(p.iter().all(|x| x.duty == 0 && x.stop.is_none() && x.ids.len() == 2));
        assert_eq!(staff::shifts(&c, &date, Role::Inspector).0.len(), 4);
        // (a file of before reads without a plan)
        let mut v = serde_json::to_value(&c).unwrap();
        v.as_object_mut().unwrap().remove("inspection_plan");
        let back: Company = serde_json::from_value(v).unwrap();
        assert_eq!(back.inspection_plan, Plan::default());
    }

    #[test]
    fn a_weekday_duty_picks_its_team_and_sets_their_hours() {
        let mut c = company();
        let ids = with_inspectors(&mut c, 4);
        let date = all_at_work(&c, &ids, 1);
        let wd = weekday(&date);
        save(&mut c, None, patrol(&[wd], &["Linie7"], 16 * 60, 22 * 60, 2)).unwrap();
        let a = assigned(&c, &date).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!((a[0].ids.len(), a[0].missing), (2, 0));
        // (the two work from 16:00 to 22:00; the other two their core hours, automatically on the
        // busiest line the duty does not check)
        let (sh, _) = staff::shifts(&c, &date, Role::Inspector);
        assert_eq!(sh.len(), 4);
        assert_eq!(sh.iter().filter(|s| (s.from, s.to) == (16 * 60, 22 * 60) && a[0].ids.contains(&s.id)).count(), 2);
        let p = staff::patrols(&c, &date);
        assert_eq!(p.len(), 2);
        let planned = p.iter().find(|x| x.duty != 0).unwrap();
        assert_eq!((planned.line.as_str(), planned.from, planned.to, planned.duty), ("Linie7", 16 * 60, 22 * 60, a[0].duty.id));
        let auto = p.iter().find(|x| x.duty == 0).unwrap();
        assert_eq!((auto.line.as_str(), auto.ids.len()), ("Linie5", 2));
        // (another weekday stays automatic)
        let other = (1..7).map(|k| dates::add(&date, k)).find(|d| weekday(d) != wd).unwrap();
        assert!(assigned(&c, &other).is_none());
        // (two lines: the hours shared between them; named inspectors first)
        let named = vec![ids[3], ids[1]];
        let d = Duty { days: vec![wd], lines: vec!["Linie5".into(), "Linie9".into()], from: 6 * 60, to: 14 * 60, named: named.clone(), ..Default::default() };
        let id2 = save(&mut c, None, d).unwrap();
        let a = assigned(&c, &date).unwrap();
        let x = a.iter().find(|x| x.duty.id == id2).unwrap();
        assert_eq!(x.ids, named);
        let p = staff::patrols(&c, &date);
        let two: Vec<&Patrol> = p.iter().filter(|x| x.duty == id2).collect();
        assert_eq!(two.len(), 2);
        assert_eq!((two[0].line.as_str(), two[0].from, two[0].to, two[1].line.as_str(), two[1].from, two[1].to), ("Linie5", 360, 600, "Linie9", 600, 840));
        // (the 16:00 duty takes the free ones, not the named)
        let late = a.iter().find(|x| x.duty.from == 16 * 60).unwrap();
        assert!(late.ids.iter().all(|i| !named.contains(i)));
    }

    #[test]
    fn a_date_planned_otherwise_wins_and_goes_back() {
        let mut c = company();
        let ids = with_inspectors(&mut c, 2);
        let date = all_at_work(&c, &ids, 1);
        let wd = weekday(&date);
        save(&mut c, None, patrol(&[wd], &["Linie5"], 8 * 60, 16 * 60, 2)).unwrap();
        add_date(&mut c, &date).unwrap();
        assert_eq!(add_date(&mut c, &date), Err(DATE_TAKEN));
        // (copied from the weekday, then changed)
        let copy = c.inspection_plan.dates[0].duties[0].clone();
        assert!(copy.days.is_empty() && copy.lines == vec!["Linie5".to_string()]);
        save(&mut c, Some(&date), Duty { lines: vec!["Linie9".into()], ..copy }).unwrap();
        let a = assigned(&c, &date).unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].duty.lines, vec!["Linie9".to_string()]);
        // (the week after, the weekday's)
        let next = dates::add(&date, 7);
        assert_eq!(duties_on(&c, &next).unwrap()[0].lines, vec!["Linie5".to_string()]);
        // (all its duties taken out: the date is automatic)
        let id = c.inspection_plan.dates[0].duties[0].id;
        remove(&mut c, id).unwrap();
        assert!(assigned(&c, &date).is_none());
        remove_date(&mut c, &date).unwrap();
        assert_eq!(duties_on(&c, &date).unwrap()[0].lines, vec!["Linie5".to_string()]);
        // (no planning of the past; a bad duty is refused)
        let yesterday = dates::add(&c.date, -1);
        assert_eq!(add_date(&mut c, &yesterday), Err(PAST_DATE));
        assert_eq!(save(&mut c, None, patrol(&[wd], &[], 8 * 60, 9 * 60, 2)), Err(BAD_HOURS));
        assert_eq!(save(&mut c, None, patrol(&[wd], &[], 8 * 60, 19 * 60, 2)), Err(BAD_HOURS));
        assert_eq!(save(&mut c, None, patrol(&[], &[], 8 * 60, 16 * 60, 2)), Err(NO_DAYS));
        assert_eq!(save(&mut c, None, patrol(&[wd], &[], 8 * 60, 16 * 60, 5)), Err(BAD_TEAM));
        assert_eq!(save(&mut c, None, patrol(&[wd], &["Linie42"], 8 * 60, 16 * 60, 2)), Err(NO_LINE));
        assert_eq!(save(&mut c, None, Duty { kind: Kind::Action, days: vec![wd], from: 480, to: 600, size: 4, ..Default::default() }), Err(NO_STOP));
    }

    #[test]
    fn rest_and_holidays_are_kept_and_what_is_missing_is_said() {
        let mut c = company();
        let ids = with_inspectors(&mut c, 2);
        let date = all_at_work(&c, &ids, 2);
        let next = dates::add(&date, 1);
        // (a night duty until 02:00, then an early one at 06:00 the next day: four hours' rest)
        add_date(&mut c, &date).unwrap();
        add_date(&mut c, &next).unwrap();
        save(&mut c, Some(&date), Duty { lines: vec!["Linie5".into()], from: 18 * 60, to: 26 * 60, size: 2, ..Default::default() }).unwrap();
        let early = save(&mut c, Some(&next), Duty { lines: vec!["Linie5".into()], from: 6 * 60, to: 12 * 60, size: 2, ..Default::default() }).unwrap();
        let a = assigned(&c, &next).unwrap();
        let x = a.iter().find(|x| x.duty.id == early).unwrap();
        assert!(x.dropped() && x.missing == 2, "{x:?}");
        assert!(staff::patrols(&c, &next).is_empty());
        // (from 14:00 they are rested: twelve hours)
        save(&mut c, Some(&next), Duty { id: early, lines: vec!["Linie5".into()], from: 14 * 60, to: 20 * 60, size: 2, ..Default::default() }).unwrap();
        assert_eq!(assigned(&c, &next).unwrap()[0].ids.len(), 2);
        // (one on holiday: the other works alone, one missing)
        c.staff.iter_mut().find(|e| e.id == ids[0]).unwrap().holiday_until = Some(dates::add(&next, 3));
        let a = assigned(&c, &next).unwrap();
        assert_eq!((a[0].ids.clone(), a[0].missing), (vec![ids[1]], 1));
        let p = staff::patrols(&c, &next);
        assert_eq!(p.len(), 1);
        assert!(p[0].share < staff::patrols(&c, &date)[0].share);
        // (named and away: missing; a stand-in is not taken for the named one when nobody is free)
        save(&mut c, Some(&next), Duty { id: early, lines: vec!["Linie5".into()], from: 14 * 60, to: 20 * 60, named: vec![ids[0]], ..Default::default() }).unwrap();
        let a = assigned(&c, &next).unwrap();
        assert_eq!((a[0].ids.clone(), a[0].missing), (vec![ids[1]], 0));
    }

    #[test]
    fn a_storaktion_checks_every_bus_through_its_stop_and_keeps_its_lines_honest() {
        let mut c = company();
        let ids = with_inspectors(&mut c, 4);
        let date = all_at_work(&c, &ids, 1);
        c.date = date.clone();
        let through = vec![Pass { line: "Linie5".into(), from: "Bahnhof".into(), offset: 10 }, Pass { line: "Linie7".into(), from: "Hafen".into(), offset: 4 }];
        add_date(&mut c, &date).unwrap();
        save(&mut c, Some(&date), Duty { kind: Kind::Action, stop: "Rathaus".into(), through: through.clone(), from: 15 * 60, to: 18 * 60, size: 4, ..Default::default() }).unwrap();
        let p = staff::patrols(&c, &date);
        assert_eq!(p.len(), 2);
        assert!(p.iter().all(|x| x.stop.as_deref() == Some("Rathaus") && x.ids.len() == 4 && x.share == ACTION_SHARE));
        let mut seen = trips("Linie5", "Bahnhof");
        seen.extend(trips("Linie7", "Hafen"));
        seen.extend(trips("Linie9", "Wald"));
        let lines = ["Linie5", "Linie7", "Linie9"].map(|l| inspection::LineDay { line: l.into(), passengers: 2280, dodgers: 114.0, ..Default::default() }).to_vec();
        let day = inspection::close(&mut c, &date, lines, &seen);
        // (every bus of both lines passing between 15:00 and 18:00, at the minute it passes)
        let of5: Vec<&inspection::Check> = day.checks.iter().filter(|k| k.line == "Linie5").collect();
        assert_eq!(of5.len(), 9);
        assert!(of5.iter().all(|k| k.stop == "Rathaus" && k.at == k.dep + 10 && (15 * 60..18 * 60).contains(&k.at)));
        assert_eq!(day.checks.iter().filter(|k| k.line == "Linie7").count(), 9);
        assert!(day.checks.iter().all(|k| k.line != "Linie9"));
        // (more of a bus checked than a patrol does)
        assert!(day.checks.iter().any(|k| k.checked as f64 > inspection::PAIR_CHECKS - 0.5) || day.checks.iter().all(|k| k.checked >= 18));
        // (the lines through it are kept honest the days after)
        let after = dates::add(&date, 1);
        assert!(super::super::dodgers::deterrence_with(&c, &after, "Linie5", 0.0) > 0.08);
        assert!(super::super::dodgers::deterrence_with(&c, &after, "Linie7", 0.0) > 0.08);
        assert_eq!(super::super::dodgers::deterrence_with(&c, &after, "Linie9", 0.0), 0.0);
        // (the day keeps what was planned)
        assert_eq!(day.planned.len(), 1);
        assert_eq!((day.planned[0].kind, day.planned[0].ids.len(), day.planned[0].missing), (Kind::Action, 4, 0));
        assert_eq!(passes_at(&Duty { through, ..Default::default() }, "Linie7", "Anderswo", 600, 640), Some(620));
    }

    #[test]
    fn a_storaktion_of_one_is_dropped() {
        let mut c = company();
        let ids = with_inspectors(&mut c, 3);
        let date = all_at_work(&c, &ids, 1);
        add_date(&mut c, &date).unwrap();
        save(&mut c, Some(&date), patrol(&[], &["Linie5"], 6 * 60, 14 * 60, 2)).unwrap();
        let through = vec![Pass { line: "Linie5".into(), from: String::new(), offset: 5 }];
        save(&mut c, Some(&date), Duty { kind: Kind::Action, stop: "Rathaus".into(), through, from: 15 * 60, to: 18 * 60, size: 4, ..Default::default() }).unwrap();
        let a = assigned(&c, &date).unwrap();
        let x = a.iter().find(|x| x.duty.kind == Kind::Action).unwrap();
        assert!(x.dropped() && x.missing == 4, "{x:?}");
        // (the one left over works the core hours, alone, automatically)
        assert_eq!(staff::shifts(&c, &date, Role::Inspector).0.len(), 3);
        let p = staff::patrols(&c, &date);
        assert!(p.iter().any(|x| x.duty == 0 && x.ids.len() == 1 && x.line != "Linie5"));
    }

    /// (10 November 2026, the user's choice) An inspector at work by the rota without a duty on a
    /// planned day is not sent home unpaid: the core hours, paid, patrolling a line no duty checks.
    #[test]
    fn a_spare_inspector_works_paid_and_checks_an_unplanned_line() {
        let mut c = company();
        let ids = with_inspectors(&mut c, 3);
        let date = all_at_work(&c, &ids, 1);
        c.date = date.clone();
        save(&mut c, None, patrol(&[weekday(&date)], &["Linie5"], 6 * 60, 14 * 60, 2)).unwrap();
        let a = assigned(&c, &date).unwrap();
        let spare_ids = spare(&c, &date, &a);
        assert_eq!(spare_ids.len(), 1);
        let h = c.staff_rules.hours_of(Role::Inspector);
        let (sh, _) = staff::shifts(&c, &date, Role::Inspector);
        let s = sh.iter().find(|s| s.id == spare_ids[0]).expect("a shift");
        assert_eq!((s.from, s.to), (h.from, h.from + h.length()));
        assert!(s.paid() >= staff::MIN_PAID);
        // (paid as any day's work: the wages of the day count the spare one's hours)
        let mut report = super::super::day::DayReport::default();
        staff::roles_day(&mut c, &date, &mut report);
        let day = c.staff_rules.last.clone().unwrap();
        let inspectors = day.roles.iter().find(|r| r.role == Role::Inspector).unwrap();
        assert_eq!(inspectors.at_work, 3);
        assert!(inspectors.wages > 0 && inspectors.paid >= 3.0 * staff::MIN_PAID as f64 / 60.0);
        // (automatic checks on a line the duty does not check)
        let mut seen = trips("Linie5", "Bahnhof");
        seen.extend(trips("Linie7", "Hafen"));
        let day = inspection::close(&mut c, &date, Vec::new(), &seen);
        assert!(day.checks.iter().any(|k| k.duty == 0 && k.line == "Linie7" && k.team == spare_ids));
        assert!(day.checks.iter().all(|k| k.duty != 0 || k.line != "Linie5"));
    }

    /// (10 November 2026) The spare ones never patrol beside a planned duty on its line: a line no
    /// duty checks first; when every line is checked, the busiest whose cover in their hours is
    /// thinnest - a storaktion's lines count as checked, but its hours outside theirs do not.
    #[test]
    fn spare_patrols_keep_off_the_lines_the_duties_check() {
        let mut c = company();
        let ids = with_inspectors(&mut c, 10);
        // (a day seven of them are at work: two on the patrol, two at the storaktion, three spare)
        let date = (0..14).map(|k| dates::add(&c.date, k)).find(|d| ids.iter().filter(|id| staff::at_work(c.employee(**id).unwrap(), d)).count() >= 7).expect("such a day");
        c.date = date.clone();
        let h = c.staff_rules.hours_of(Role::Inspector);
        let wd = weekday(&date);
        save(&mut c, None, patrol(&[wd], &["Linie5"], h.from, h.from + h.length(), 2)).unwrap();
        let through = vec![Pass { line: "Linie7".into(), from: String::new(), offset: 4 }, Pass { line: "Linie9".into(), from: String::new(), offset: 6 }];
        save(&mut c, None, Duty { days: vec![wd], kind: Kind::Action, stop: "Rathaus".into(), through, from: 15 * 60, to: 18 * 60, size: 2, ..Default::default() }).unwrap();
        let a = assigned(&c, &date).unwrap();
        let spare_ids = spare(&c, &date, &a);
        assert!(spare_ids.len() >= 2, "{spare_ids:?}");
        let p = staff::patrols(&c, &date);
        let auto: Vec<&Patrol> = p.iter().filter(|x| x.duty == 0).collect();
        // (every line is checked: not Linie5, whose duty covers the hours - Linie7 first, then Linie9)
        assert!(auto.iter().all(|x| x.line != "Linie5"), "{auto:?}");
        assert_eq!(auto[0].line, "Linie7");
        if auto.len() > 1 {
            assert_eq!(auto[1].line, "Linie9");
        }
        // (a line no duty checks comes first)
        let mut c2 = c.clone();
        c2.inspection_plan.duties.retain(|d| d.kind == Kind::Patrol);
        let p2 = staff::patrols(&c2, &date);
        assert_eq!(p2.iter().find(|x| x.duty == 0).unwrap().line, "Linie7");
        assert!(p2.iter().filter(|x| x.duty == 0).all(|x| x.line != "Linie5"));
    }

    #[test]
    fn night_duties_run_past_midnight() {
        let mut c = company();
        let ids = with_inspectors(&mut c, 2);
        let date = all_at_work(&c, &ids, 1);
        c.date = date.clone();
        save(&mut c, None, patrol(&[weekday(&date)], &["Linie5"], 22 * 60, 2 * 60, 2)).unwrap();
        let d = &c.inspection_plan.duties[0];
        assert_eq!((d.from, d.to), (22 * 60, 26 * 60));
        let (sh, _) = staff::shifts(&c, &date, Role::Inspector);
        assert!(sh.iter().all(|s| s.night() == 240));
        let seen: Vec<inspection::Seen> = (0..16).map(|k| 21 * 60 + 20 * k).map(|dep| inspection::Seen { line: "Linie5".into(), number: "5".into(), dep, arr: dep + 18, from: "Bahnhof".into(), to: "Markt".into(), pax: 30, share: 0.095, crowded: false, stops: 0 }).collect();
        let day = inspection::close(&mut c, &date, Vec::new(), &seen);
        assert!(!day.checks.is_empty());
        assert!(day.checks.iter().all(|k| k.at >= 22 * 60 && k.at < 26 * 60));
        assert!(day.checks.iter().any(|k| k.at >= 24 * 60));
    }

    #[test]
    fn the_same_plan_draws_the_same_day() {
        let mut a = company();
        let ids = with_inspectors(&mut a, 3);
        let date = all_at_work(&a, &ids, 1);
        a.date = date.clone();
        save(&mut a, None, patrol(&[weekday(&date)], &[], 7 * 60, 15 * 60, 3)).unwrap();
        let mut b = a.clone();
        let seen = trips("Linie5", "Bahnhof");
        assert_eq!(assigned(&a, &date), assigned(&b, &date));
        assert_eq!(inspection::close(&mut a, &date, Vec::new(), &seen), inspection::close(&mut b, &date, Vec::new(), &seen));
    }

    #[test]
    fn the_stops_of_the_timetable_say_when_each_line_passes() {
        let mut c = company();
        c.lines.truncate(2);
        let stop = |name: &str, s: f64| crate::StopInfo { name: name.into(), id: 0, arr: s, dep: s };
        let trip = |line: &str, from: &str, dep: f64, stops: Vec<crate::StopInfo>| crate::TripInfo { name: format!("{line} {from}"), index: 1, line: line.into(), from: from.into(), terminus: "X".into(), departure: dep, arrival: dep + 1800.0, stops, km: 10.0 };
        let info = |name: &str, trips: Vec<crate::TripInfo>| crate::LineInfo { name: name.into(), tours: vec![crate::TourInfo { number: "1".into(), ai_group: String::new(), first: 0.0, last: 0.0, days: "daily".into(), runs: true, today: true, yesterday_night: false, next_run: None, trips }], user_allowed: true, termini: Vec::new() };
        let lines = vec![
            info("Linie5", vec![trip("5", "Bahnhof", 36000.0, vec![stop("Bahnhof", 36000.0), stop("Rathaus", 36600.0), stop("Markt", 37200.0)])]),
            info("Linie7", vec![trip("7", "Hafen", 36000.0, vec![stop("Hafen", 36000.0), stop("Rathaus", 36240.0)])]),
        ];
        let s = stops_of(&c, &lines);
        assert_eq!(s[0].stop, "Rathaus");
        assert_eq!(s[0].lines(), vec!["Linie5".to_string(), "Linie7".to_string()]);
        assert_eq!(s[0].passes.iter().map(|p| p.offset).collect::<Vec<_>>(), vec![10, 4]);
    }
}
