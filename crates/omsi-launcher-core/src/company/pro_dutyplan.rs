//! RosterFlow Pro, round J (NexiconDK, 7 November 2026: "Vagter-fanen bedre"): what the Vagter
//! tab tells of the day's duties besides drawing them.
//!
//! - **The price of a duty** (`price`): its paid hours at what an hour of the company's drivers
//!   costs (their wages on average; the market's for a company without drivers), the supplements
//!   of the evening, the night and the weekend on the time it works (`pay::supplement_minutes_on`,
//!   a public holiday as a Sunday), and the employer's on-costs (`economy::employer_cost`).
//! - **The day in figures** (`summary`): duties, paid hours, the driving's share of them, the
//!   price, the average duty, and how many keep every rule, break a soft one or a hard one - and
//!   the same of the game's own plan (`game_plan`: the day as the timetable's blocks and the
//!   automatic duties have it, nothing built on it), so the page shows what the planner's duties
//!   make better or worse.
//! - **The checklist of a duty** (`checklist`): each rule a line, kept or broken, in plain words
//!   - the meal break, driving without a break, the working day, the target length, the changes of
//!   bus, the shortest piece, a split's spread, where it begins and ends - every issue of the duty
//!   in one of them or a line of its own.
//! - **Shorter names** (`short_tour`, `piece_lines`): "131 · H02" for the tour "131H02" of line
//!   131, and the other lines a piece's trips run ("NE2", a night line).

use super::model::{Cents, Company};
use super::network::{PlannedTrip, TourOfDay};
use super::pro_duties::{self as pd, Kind};
use super::rosterflow::{self as rf, Board, Shaped};
use super::specials::{self, ShownLine};
use super::{dates, economy, pay, staff, studio as st};
use crate::LineInfo;

/// What an hour of a driver costs the company in wages (cents, without the on-costs): its
/// drivers' on average, else the market's wage of a driver of some experience.
pub fn hour_wage(c: &Company) -> f64 {
    let drivers: Vec<f64> = c.staff.iter().filter(|e| staff::drives(e)).map(pay::hour_wage).collect();
    if drivers.is_empty() {
        staff::hourly(economy::market_wage(45.0, &super::econ_uk::Level::of(c)))
    } else {
        drivers.iter().sum::<f64>() / drivers.len() as f64
    }
}

/// The price of a duty, in cents.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DutyPrice {
    /// An hour's wage, and the minutes paid.
    pub hour: f64,
    pub paid: i32,
    pub wages: Cents,
    /// The supplements, and the minutes worked they are on (any rate).
    pub supplements: Cents,
    pub supplement_minutes: i32,
    /// The employer's on-costs on the wages and supplements.
    pub oncosts: Cents,
    pub total: Cents,
}

/// The stretches duty `s` works: sign-on to sign-off, a split's unpaid gap out.
pub fn worked(s: &Shaped) -> Vec<(i32, i32)> {
    let mut out = vec![(s.start, s.end)];
    let mut splits = s.splits.clone();
    splits.sort();
    for (a, z) in splits {
        if let Some(last) = out.pop() {
            if a > last.0 && z < last.1 {
                out.push((last.0, a));
                out.push((z, last.1));
            } else {
                out.push(last);
            }
        }
    }
    out
}

/// What duty `s` costs on `date`, `hour` an hour's wage (`hour_wage`).
pub fn price(c: &Company, s: &Shaped, date: &str, hour: f64) -> DutyPrice {
    let wd = dates::parse(date).map(dates::weekday).unwrap_or(0);
    let ps = worked(s);
    let holiday = super::pro_holiday::days(c, date);
    let supp = pay::supplement_minutes_on(wd, holiday, &ps);
    let supplement_minutes: i32 = ps.iter().map(|&(a, z)| (a..z).step_by(15).filter(|&m| pay::supplement_rate_on(wd, holiday, m) > 0.0).map(|m| (z - m).min(15)).sum::<i32>()).sum();
    let wages = (hour * s.paid as f64 / 60.0).round() as Cents;
    let supplements = (hour * supp / 60.0).round() as Cents;
    let oncosts = economy::employer_cost(wages + supplements) - wages - supplements;
    DutyPrice { hour, paid: s.paid, wages, supplements, supplement_minutes, oncosts, total: wages + supplements + oncosts }
}

/// How a rule stands on a duty: kept, only told (the target length), broken soft or hard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Ok,
    Info,
    Warn,
    Bad,
}

impl Level {
    pub fn key(self) -> &'static str {
        match self {
            Level::Ok => "ok",
            Level::Info => "info",
            Level::Warn => "warn",
            Level::Bad => "bad",
        }
    }

    fn of(hard: bool) -> Level {
        if hard { Level::Bad } else { Level::Warn }
    }
}

/// A line of a duty's checklist: its rule (`title`), how it stands, and in words (`text` with its
/// places; English, translated where shown).
#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub title: &'static str,
    pub level: Level,
    pub text: &'static str,
    pub args: Vec<(&'static str, String)>,
}

pub const T_MEAL: &str = "Meal break";
pub const T_DRIVING: &str = "Driving without a break";
pub const T_DAY: &str = "Working day";
pub const T_OVERTIME: &str = "Overtime";
pub const T_TARGET: &str = "Target length";
pub const T_CHANGES: &str = "Changes of bus";
pub const T_PIECE: &str = "Shortest piece";
pub const T_SPLIT: &str = "Split duty";
pub const T_PLACES: &str = "Where it begins and ends";
pub const T_REST: &str = "Rest between pieces";
pub const T_TYPE: &str = "Duty type";
pub const T_OTHER: &str = "Other";
pub const TITLES: [&str; 12] = [T_MEAL, T_DRIVING, T_DAY, T_OVERTIME, T_TARGET, T_CHANGES, T_PIECE, T_SPLIT, T_PLACES, T_REST, T_TYPE, T_OTHER];

pub const C_MEAL_NONE_NEEDED: &str = "Not needed: works %{time}, a break is due after %{after}";
pub const C_MEAL_AT: &str = "%{min} at %{at}, after %{worked} of work";
pub const C_MEAL_LATE: &str = "Too late: %{time} of work before it, at most %{after}";
pub const C_MEAL_MISSING: &str = "None: a duty working over %{after} needs a break of %{min}";
pub const C_DRIVING_OK: &str = "Longest stretch %{time}, at most %{max}";
pub const C_DRIVING_OVER: &str = "%{time} without a break, at most %{max} before a break of %{min}";
pub const C_DAY_OK: &str = "%{time} of at most %{max}";
pub const C_DAY_OVER: &str = "%{time}, longer than %{max}";
pub const C_OVERTIME: &str = "%{time} more than %{day}, paid as overtime";
pub const C_TARGET: &str = "%{time}, the target is %{from}–%{to}";
pub const C_CHANGES_NONE: &str = "None: one bus all the duty";
pub const C_CHANGES_OK: &str = "%{n} changes, at least %{min} for each";
pub const C_CHANGES_MANY: &str = "%{n} changes of bus, at most %{max}";
pub const C_CHANGE_SHORT: &str = "%{have} to get from %{from} to %{to}, %{need} needed";
pub const C_CHANGE_SAME: &str = "%{have} to change buses at %{stop}, %{need} needed";
pub const C_CHANGE_STOPS: &str = "Only %{time} to get to another stop between two pieces";
pub const C_PIECE_OK: &str = "%{time} (at least %{min})";
pub const C_PIECE_SHORT: &str = "A piece of %{time}, shorter than %{min}";
pub const C_SPLIT_OK: &str = "Spread %{time}, at most %{max}";
pub const C_SPLIT_OVER: &str = "Spread %{time}, more than %{max}";
pub const C_SPLIT_WEEKEND: &str = "Split duties do not run at the weekend";
pub const C_SPLIT_LONG: &str = "A gap of %{time} between two pieces, longer than %{max}";
pub const C_PLACES_OK: &str = "Signs on at %{on}, off at %{off}";
pub const C_REST_SHORT: &str = "Only %{time} between two pieces, at least %{min}";
pub const C_OVERLAP: &str = "Two pieces at the same time";
pub const C_TYPE: &str = "Fits no duty type allowed: it is %{type}";
/// (Round O) A night duty's target, and a split duty with night-bus work.
pub const C_TARGET_NIGHT: &str = "%{time}, a night duty's target is %{from}–%{to}";
pub const C_SPLIT_NIGHT: &str = "Split duties may not have night-bus work";
/// (Round P) A split duty judged part by part: each part's meal break and driving, and the gap
/// between the parts told as what it is (time off, not a meal break).
pub const C_PART_MEAL_NONE: &str = "Part %{n}: %{time}, none needed (only after %{after})";
pub const C_PART_MEAL_AT: &str = "Part %{n}: %{min} at %{at}";
pub const C_PART_MEAL_MISSING: &str = "Part %{n}: %{time} with no meal break, one needed after %{after}";
pub const C_PART_DRIVING: &str = "Part %{n}: at most %{time} of %{max}";
pub const C_SPLIT_GAP: &str = "%{time} off between the parts (%{from}–%{to})";
pub const CHECK_TEXTS: [&str; 33] = [
    C_MEAL_NONE_NEEDED, C_MEAL_AT, C_MEAL_LATE, C_MEAL_MISSING, C_DRIVING_OK, C_DRIVING_OVER, C_DAY_OK, C_DAY_OVER, C_OVERTIME, C_TARGET, C_CHANGES_NONE, C_CHANGES_OK, C_CHANGES_MANY, C_CHANGE_SHORT,
    C_CHANGE_SAME, C_CHANGE_STOPS, C_PIECE_OK, C_PIECE_SHORT, C_SPLIT_OK, C_SPLIT_OVER, C_SPLIT_WEEKEND, C_SPLIT_LONG, C_PLACES_OK, C_REST_SHORT, C_OVERLAP, C_TYPE,
    C_TARGET_NIGHT, C_SPLIT_NIGHT, C_PART_MEAL_NONE, C_PART_MEAL_AT, C_PART_MEAL_MISSING, C_PART_DRIVING, C_SPLIT_GAP,
];

fn arg<'a>(i: &'a rf::Issue, k: &str) -> &'a str {
    i.args.iter().find(|a| a.0 == k).map(|a| a.1.as_str()).unwrap_or("")
}

/// The issues of duty `row` not dismissed.
fn issues_of<'a>(c: &Company, b: &'a Board, s: &Shaped) -> Vec<&'a rf::Issue> {
    s.issues.iter().filter_map(|&i| b.issues.get(i)).filter(|i| !rf::dismissed(c, b.weekday, &i.key)).collect()
}

/// How many soft and hard rules duty `row` breaks (RosterFlow's issues not dismissed, and Pro's
/// marks): (soft, hard).
pub fn rule_counts(c: &Company, b: &Board, row: usize) -> (usize, usize) {
    let Some(s) = b.shaped.get(row) else { return (0, 0) };
    let issues = issues_of(c, b, s);
    let marks = pd::marks(c, b, row);
    let hard = issues.iter().filter(|i| i.hard).count() + marks.iter().filter(|m| m.hard).count();
    let soft = issues.iter().filter(|i| !i.hard).count() + marks.iter().filter(|m| !m.hard).count();
    (soft, hard)
}

/// The time a stop's name is (its place, trimmed).
fn stop(x: Option<&PlannedTrip>, end: bool) -> String {
    x.map(|t| if end { t.end().trim().to_string() } else { t.from.trim().to_string() }).unwrap_or_default()
}

/// The checklist of duty `row` of board `b` (the company `c` it is of).
pub fn checklist(c: &Company, b: &Board, row: usize) -> Vec<Check> {
    let Some(s) = b.shaped.get(row) else { return Vec::new() };
    let p = &c.planning.flow.prefs;
    let goals = p.goals.sane();
    let issues = issues_of(c, b, s);
    let marks = pd::marks(c, b, row);
    let hm = rf::hm;
    let mut used: Vec<usize> = Vec::new();
    let mut take = |texts: &[&str]| -> Vec<&rf::Issue> {
        let mut out = Vec::new();
        for (k, i) in issues.iter().enumerate() {
            if texts.contains(&i.text) && !used.contains(&k) {
                used.push(k);
                out.push(*i);
            }
        }
        out
    };
    let rule_of = |i: &rf::Issue| -> Option<usize> { st::Rule::LABELS.iter().position(|l| *l == arg(i, "rule")) };
    let mut out: Vec<Check> = Vec::new();
    let mut push = |title, level, text, args: Vec<(&'static str, String)>| out.push(Check { title, level, text, args });
    // the meal break (round P: a split duty's part by part - the gap between them is time off,
    // not a meal break)
    let split = pd::kind_of(s) == Kind::Split;
    let parts = pd::parts(s);
    let meal = take(&[rf::I_MEAL, rf::I_MEAL_LATE]);
    if split {
        // (RosterFlow's own meal issue, when it has one, sets how bad a part without one is)
        let missing = if meal.iter().any(|i| i.hard) { Level::Bad } else { Level::Warn };
        let mut told = false;
        for (n, &(a, z, worked)) in parts.iter().enumerate() {
            let n = (n + 1).to_string();
            if !p.meal.on || worked <= p.meal_after {
                push(T_MEAL, Level::Ok, C_PART_MEAL_NONE, vec![("n", n), ("time", hm(worked)), ("after", hm(p.meal_after))]);
            } else if let Some(m) = pd::part_meal(s, (a, z)) {
                push(T_MEAL, Level::Ok, C_PART_MEAL_AT, vec![("n", n), ("min", format!("{} min", m.to - m.from)), ("at", rf::clock(m.from))]);
            } else {
                push(T_MEAL, missing, C_PART_MEAL_MISSING, vec![("n", n), ("time", hm(worked)), ("after", hm(p.meal_after))]);
                told = true;
            }
        }
        // (its issue told as RosterFlow tells it when no part is without one: a meal break late)
        if let (Some(i), false) = (meal.first(), told) {
            let text = if i.text == rf::I_MEAL_LATE { C_MEAL_LATE } else { C_MEAL_MISSING };
            push(T_MEAL, Level::of(i.hard), text, vec![("time", arg(i, "time").into()), ("after", hm(p.meal_after)), ("min", hm(p.meal.value))]);
        }
    } else if let Some(i) = meal.first() {
        if i.text == rf::I_MEAL_LATE {
            push(T_MEAL, Level::of(i.hard), C_MEAL_LATE, vec![("time", arg(i, "time").into()), ("after", hm(p.meal_after))]);
        } else {
            push(T_MEAL, Level::of(i.hard), C_MEAL_MISSING, vec![("after", hm(p.meal_after)), ("min", hm(p.meal.value))]);
        }
    } else if let Some(m) = s.plan.breaks.iter().find(|x| x.meal && x.kind != rf::BreakKind::Split).filter(|_| p.meal.on && s.work > p.meal_after) {
        let before: i32 = s.splits.iter().filter(|x| x.1 <= m.from).map(|x| x.1 - x.0).sum();
        push(T_MEAL, Level::Ok, C_MEAL_AT, vec![("min", format!("{} min", m.to - m.from)), ("at", rf::clock(m.from)), ("worked", hm(m.from - s.start - before))]);
    } else if s.split() {
        // (an unpaid gap not long enough to split the duty: its break)
    } else {
        push(T_MEAL, Level::Ok, C_MEAL_NONE_NEEDED, vec![("time", hm(s.work)), ("after", hm(p.meal_after))]);
    }
    // driving without a break
    let cont = take(&[rf::I_CONTINUOUS]);
    match cont.first() {
        Some(i) => push(T_DRIVING, Level::of(i.hard), C_DRIVING_OVER, vec![("time", arg(i, "time").into()), ("max", hm(p.continuous.value)), ("min", hm(p.break_min.max(p.break_from)))]),
        // (round P: a split duty's each part)
        None if p.continuous.on && split => {
            for (n, &(a, z, _)) in parts.iter().enumerate() {
                let longest = s.plan.stretches.iter().map(|x| x.to.min(z) - x.from.max(a)).max().unwrap_or(0).max(0);
                push(T_DRIVING, Level::Ok, C_PART_DRIVING, vec![("n", (n + 1).to_string()), ("time", hm(longest)), ("max", hm(p.continuous.value))]);
            }
        }
        None if p.continuous.on => push(T_DRIVING, Level::Ok, C_DRIVING_OK, vec![("time", hm(s.plan.longest())), ("max", hm(p.continuous.value))]),
        None => {}
    }
    // the working day (the goals' longest; a working day too long by the staff rules too)
    let rules = take(&[rf::I_RULE, rf::I_RULE_TIME]);
    let too_long = rules.iter().find(|i| rule_of(i) == Some(2));
    if s.work > goals.longest || too_long.is_some() {
        push(T_DAY, Level::Bad, C_DAY_OVER, vec![("time", hm(s.work)), ("max", hm(goals.longest.min(staff::DAY_MAX)))]);
    } else {
        push(T_DAY, Level::Ok, C_DAY_OK, vec![("time", hm(s.work)), ("max", hm(goals.longest))]);
    }
    if let Some(i) = rules.iter().find(|i| rule_of(i) == Some(3)) {
        push(T_OVERTIME, Level::of(i.hard), C_OVERTIME, vec![("time", arg(i, "time").into()), ("day", hm(staff::DAY_TARGET))]);
    }
    // the target length (told, not a rule broken; round O: a night duty's its own)
    let night = pd::on(c) && rf::spans_night(&b.tours, &s.spans);
    let (from, to) = goals.target(night);
    let aimed = s.work >= from && s.work <= to;
    push(T_TARGET, if aimed { Level::Ok } else { Level::Info }, if night { C_TARGET_NIGHT } else { C_TARGET }, vec![("time", hm(s.work)), ("from", hm(from)), ("to", hm(to))]);
    // the changes of bus
    let many = take(&[rf::I_CHANGEOVERS]);
    let mut changes: Vec<Check> = Vec::new();
    for m in marks.iter().filter(|m| m.text == pd::M_CHANGE || m.text == pd::M_CHANGE_SAME) {
        let text = if m.text == pd::M_CHANGE { C_CHANGE_SHORT } else { C_CHANGE_SAME };
        changes.push(Check { title: T_CHANGES, level: Level::of(m.hard), text, args: m.args.clone() });
    }
    for i in rules.iter().filter(|i| rule_of(i) == Some(4)) {
        changes.push(Check { title: T_CHANGES, level: Level::of(i.hard), text: C_CHANGE_STOPS, args: vec![("time", arg(i, "time").into())] });
    }
    if let Some(i) = many.first() {
        changes.push(Check { title: T_CHANGES, level: Level::of(i.hard), text: C_CHANGES_MANY, args: vec![("n", arg(i, "n").into()), ("max", arg(i, "max").into())] });
    }
    if changes.is_empty() {
        if s.changeovers == 0 {
            push(T_CHANGES, Level::Ok, C_CHANGES_NONE, Vec::new());
        } else {
            push(T_CHANGES, Level::Ok, C_CHANGES_OK, vec![("n", s.changeovers.to_string()), ("min", format!("{} min", p.change.same_stop.min(p.change.other_stop)))]);
        }
    }
    out.extend(changes);
    // the shortest piece
    let short = take(&[rf::I_SHORT_PIECE]);
    let shortest = s.spans.iter().map(|x| x.to - x.from).min().unwrap_or(0);
    match short.first() {
        Some(i) => out.push(Check { title: T_PIECE, level: Level::of(i.hard), text: C_PIECE_SHORT, args: vec![("time", arg(i, "time").into()), ("min", arg(i, "min").into())] }),
        None => out.push(Check { title: T_PIECE, level: Level::Ok, text: C_PIECE_OK, args: vec![("time", hm(shortest)), ("min", hm(p.min_piece.value))] }),
    }
    // (round O: a split duty with night-bus work - never)
    if marks.iter().any(|m| m.text == pd::M_NIGHT_SPLIT) {
        out.push(Check { title: T_SPLIT, level: Level::Bad, text: C_SPLIT_NIGHT, args: Vec::new() });
    }
    // a split duty
    if split {
        // (round P: the gap between the parts, time off - its own line, never the meal break)
        for w in parts.windows(2) {
            out.push(Check { title: T_SPLIT, level: Level::Info, text: C_SPLIT_GAP, args: vec![("time", hm(w[1].0 - w[0].1)), ("from", rf::clock(w[0].1)), ("to", rf::clock(w[1].0))] });
        }
        let over = marks.iter().any(|m| m.text == pd::M_SPREAD);
        out.push(Check { title: T_SPLIT, level: if over { Level::Bad } else { Level::Ok }, text: if over { C_SPLIT_OVER } else { C_SPLIT_OK }, args: vec![("time", hm(s.spread())), ("max", hm(pd::SPLIT_SPREAD))] });
        if marks.iter().any(|m| m.text == pd::M_WEEKEND_SPLIT) {
            out.push(Check { title: T_SPLIT, level: Level::Warn, text: C_SPLIT_WEEKEND, args: Vec::new() });
        }
    }
    for i in rules.iter().filter(|i| rule_of(i) == Some(5)) {
        out.push(Check { title: T_SPLIT, level: Level::of(i.hard), text: C_SPLIT_LONG, args: vec![("time", arg(i, "time").into()), ("max", hm(st::LONG_SPLIT))] });
    }
    // rest between pieces, pieces at once
    for i in rules.iter().filter(|i| matches!(rule_of(i), Some(0) | Some(1))) {
        if rule_of(i) == Some(0) {
            out.push(Check { title: T_REST, level: Level::of(i.hard), text: C_OVERLAP, args: Vec::new() });
        } else {
            out.push(Check { title: T_REST, level: Level::of(i.hard), text: C_REST_SHORT, args: vec![("time", arg(i, "time").into()), ("min", hm(staff::REST))] });
        }
    }
    // where it begins and ends
    let mut spans = s.spans.clone();
    spans.sort_by_key(|x| x.from);
    let first = spans.first().and_then(|sp| b.tours.get(sp.tour).map(|t| stop(t.trips.get(super::network::duties_of(t).get(sp.duty).map(|r| r.start).unwrap_or(0)), false)));
    let last = spans.last().and_then(|sp| b.tours.get(sp.tour).and_then(|t| super::network::duties_of(t).get(sp.duty).map(|r| stop(t.trips.get(r.end.saturating_sub(1)), true))));
    let places = take(&[rf::I_SIGN_ON, rf::I_SIGN_OFF, rf::I_BREAK_PLACE, rf::I_RELIEF_PLACE, rf::I_TRAVEL]);
    if places.is_empty() {
        out.push(Check { title: T_PLACES, level: Level::Ok, text: C_PLACES_OK, args: vec![("on", first.unwrap_or_default()), ("off", last.unwrap_or_default())] });
    }
    for i in places {
        out.push(Check { title: T_PLACES, level: Level::of(i.hard), text: i.text, args: i.args.clone() });
    }
    // (round K: a bus taken over or handed over where no relief point is - built in "Afgange")
    for m in marks.iter().filter(|m| m.text == pd::M_NOT_RELIEF_ON || m.text == pd::M_NOT_RELIEF_OFF) {
        out.push(Check { title: T_PLACES, level: Level::Warn, text: m.text, args: m.args.clone() });
    }
    // the duty's type
    for i in take(&[rf::I_TYPE]) {
        out.push(Check { title: T_TYPE, level: Level::of(i.hard), text: C_TYPE, args: i.args.clone() });
    }
    // every other issue a line of its own (the staff rules' are told above)
    for (k, i) in issues.iter().enumerate() {
        if used.contains(&k) || i.text == rf::I_RULE || i.text == rf::I_RULE_TIME {
            continue;
        }
        out.push(Check { title: T_OTHER, level: Level::of(i.hard), text: i.text, args: i.args.clone() });
    }
    out
}

/// A day of duties in figures (`summary`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Summary {
    pub duties: usize,
    /// Minutes paid, minutes driven (every trip, with passengers or not), the average duty's paid.
    pub paid: i32,
    pub driving: i32,
    pub average: i32,
    pub cost: Cents,
    /// Duties that keep every rule, break a soft one (at worst), break a hard one.
    pub ok: usize,
    pub warn: usize,
    pub bad: usize,
}

impl Summary {
    /// The driving's share of the paid time, in per cent.
    pub fn driving_share(&self) -> f64 {
        if self.paid > 0 { self.driving as f64 * 100.0 / self.paid as f64 } else { 0.0 }
    }
}

/// The duties of board `b` the tab counts: not a spare's, not a piece in the pool.
pub fn counted(c: &Company, b: &Board) -> Vec<usize> {
    (0..b.shaped.len()).filter(|&row| b.shaped[row].spare.is_none() && !pd::is_pool_duty(c, b.weekday, &b.tours, &b.shaped[row])).collect()
}

/// Board `b` (of `date`, the company `c` as the board was made of) in figures.
pub fn summary(c: &Company, b: &Board, date: &str) -> Summary {
    let hour = hour_wage(c);
    let mut out = Summary::default();
    for row in counted(c, b).into_iter().chain((0..b.shaped.len()).filter(|&r| b.shaped[r].spare.is_none() && pd::is_pool_duty(c, b.weekday, &b.tours, &b.shaped[r]))) {
        let s = &b.shaped[row];
        out.duties += 1;
        out.paid += s.paid;
        out.driving += s.driving;
        out.cost += price(c, s, date, hour).total;
        match rule_counts(c, b, row) {
            (_, h) if h > 0 => out.bad += 1,
            (w, _) if w > 0 => out.warn += 1,
            _ => out.ok += 1,
        }
    }
    out.average = if out.duties > 0 { out.paid / out.duties as i32 } else { 0 };
    out
}

/// The company as the game's own plan has `date`'s weekday: its blocks, joined duties, pool and
/// kept numbers left out - the timetable's blocks and the duties RosterFlow makes of them by itself.
pub fn game_plan_company(c: &Company, date: &str) -> Company {
    let wd = super::plandays::slot_of(c, date);
    let mut g = c.clone();
    g.planning.layouts.retain(|l| l.weekday != wd);
    g.planning.joined.retain(|j| j.weekday != wd);
    let sh = &mut g.planning.flow.sheet;
    sh.pool.retain(|x| x.weekday != wd);
    sh.numbers.retain(|x| x.weekday != wd);
    sh.rows.retain(|x| x.weekday != wd);
    sh.names.retain(|x| x.weekday != wd);
    g
}

/// The game's own plan of `date` (`game_plan_company`), as RosterFlow shows a day.
pub fn game_plan(c: &Company, lines: &[LineInfo], date: &str) -> (Company, Board) {
    let g = game_plan_company(c, date);
    let b = rf::board(&g, lines, date);
    (g, b)
}

/// A tour's name without its line's number before it: "H02" of "131H02" on line 131 ("1" stays
/// "01", a name that is only the number stays).
pub fn short_tour(number: &str, tour: &str) -> String {
    let (n, t) = (number.trim(), tour.trim());
    let rest = if !n.is_empty() && t.len() > n.len() && t[..n.len()].eq_ignore_ascii_case(n) { t[n.len()..].trim_start_matches(['/', '-', '_', ' ']) } else { t };
    let rest = if rest.is_empty() { t } else { rest };
    if rest.len() == 1 && rest.chars().all(|c| c.is_ascii_digit()) { format!("0{rest}") } else { rest.to_string() }
}

/// A tour's short name with its line: "131 · H02".
pub fn short_block(t: &TourOfDay) -> String {
    format!("{} · {}", t.number.trim(), short_tour(&t.number, &t.tour))
}

/// Whether a trip leaves at night (22:00 to 05:00).
fn at_night(m: i32) -> bool {
    let h = m.rem_euclid(1440);
    h >= 22 * 60 || h < 5 * 60
}

/// The lines trips `trips` of tour `t` run besides its own (their displays' numbers; depot and
/// empty runs not), in the order they come - each a night line when its name says so or all its
/// trips leave between 22:00 and 05:00 - and whether any trip runs the tour's own line.
pub fn piece_lines(t: &TourOfDay, trips: &[PlannedTrip]) -> (bool, Vec<ShownLine>) {
    let own = specials::display_number(&t.number).to_lowercase();
    let mut mine = false;
    let mut seen: Vec<(ShownLine, bool)> = Vec::new();
    for x in trips.iter().filter(|x| x.counts()) {
        let n = specials::display_number(&x.line);
        if n.is_empty() || n.to_lowercase() == own {
            mine = true;
            continue;
        }
        let night = at_night(x.dep);
        match seen.iter_mut().find(|y| y.0.number.eq_ignore_ascii_case(&n)) {
            Some(y) => y.1 &= night,
            None => seen.push((ShownLine { number: n, night: false }, night)),
        }
    }
    let lines = seen.into_iter().map(|(mut s, all)| {
        s.night = all || specials::night_name(&s.number);
        s
    }).collect();
    (mine, lines)
}

/// Every English text of round J's (for the translations' check).
pub fn texts() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = TITLES.to_vec();
    v.extend(CHECK_TEXTS);
    v
}

#[cfg(test)]
#[path = "pro_dutyplan_tests.rs"]
mod tests;
