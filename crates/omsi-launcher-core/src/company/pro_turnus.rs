//! RosterFlow Pro, step 3 (NexiconDK, 4 November 2026): the rota ("Turnus"), HASTUS's way. A
//! rota is a named group of rows ("Nord fuldtid", "Nord deltid", "Reserve"); a row is one week
//! from Sunday to Saturday - the bus company's week begins on Sunday (`pay::WEEK_BEGINS`) - and
//! each of its days a duty of the Vagter tab (its number: 1001 on a weekday, 2001 on a Saturday,
//! 3001 on a Sunday, `pro_duties`), a day off ("F"), or a frame ("R 05:00-13:00": at the
//! company's disposal within it, as a reserve's). A driver is put on a row, and every week he
//! moves a row down, from the last row to the first again: the cycle is as many weeks as the
//! rota has rows (`row_on`). A day of a driver can be changed for one date without changing the
//! rota (`Exception`: a day off, a holiday, ill, a duty swapped).
//!
//! The rota is checked over the whole cycle, the step from the last row back to the first
//! included (`check`): 11 hours of rest between two duties, at most 6 days of work in a row, a
//! row's hours against the hours the rota aims at (the contract week, 37 hours at the Danish price
//! level, else 39, or the rota's own), and a driver's contract against the rota's average. The
//! marks show, they do not refuse. "Make the rota automatically" (`auto`) spreads the duties of
//! the week no other rota has over enough rows, two days off together where it can and the days
//! off where the week has the fewest duties - the weekend, mostly.
//!
//! The day runs it (`on_day`, from `plan::day_plan`): with RosterFlow Pro chosen, a driver with a
//! row drives the duty of his row that day (or of his exception), nothing else of the weekly
//! roster; a driver without a row, and a duty no row has, run as the roster has them. A frame of
//! his row makes him a reserve within it that day (`frame_on`: the first to cover an open duty in
//! it, paid the frame). Kept in the company's planning (`Planning::rotas`), saved at once - not in
//! RosterFlow's draft. An older file has none.

use super::model::{Company, Employee, Employment, Frame};
use super::network::TourOfDay;
use super::plan::Problem;
use super::pro_duties as pd;
use super::rosterflow::{self as rf, Board};
use super::{dates, econ_uk, plan, plandays, staff};
use serde::{Deserialize, Serialize};

/// The days of a row: Sunday first.
pub const DAYS: usize = 7;
/// The least rest between two duties (11 hours).
pub const MIN_REST: i32 = 660;
/// The most days of work in a row.
pub const MAX_IN_A_ROW: usize = 6;
/// A row's week this far from the hours aimed at (a share of them) is marked: a week may be a
/// day longer or shorter, the cycle's average is what counts.
pub const HOURS_SLACK: f64 = 0.20;
/// The cycle's average this far from the hours aimed at, and a driver's contract this far from
/// the average (shares of them), are marked.
pub const CYCLE_SLACK: f64 = 0.05;
/// The frames "Make the rota automatically" gives a reserve rota's rows without a driver's own:
/// early and late.
pub const RESERVE_FRAMES: [(i32, i32); 2] = [(5 * 60, 13 * 60), (13 * 60, 21 * 60)];

/// A day of a row.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Cell {
    /// A day off ("F").
    #[default]
    Off,
    /// A duty of the Vagter tab, by its number.
    Duty { n: u32 },
    /// At the company's disposal from `from` to `to` (minutes of the day): a reserve's day.
    Frame { from: i32, to: i32 },
}

impl Cell {
    pub fn works(&self) -> bool {
        !matches!(self, Cell::Off)
    }
}

/// A row of a rota: a week, Sunday to Saturday, and the driver on it in the rota's first week.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Row {
    pub days: [Cell; DAYS],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver: Option<u32>,
}

/// What a rota holds (NexiconDK, 4 November 2026, after the player's comments): an ordinary
/// rota of whole and short duties all week - its drivers at work every other weekend -, a rota
/// of split duties Monday to Friday - its weekends always off -, or a reserve rota of frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotaKind {
    Normal,
    Split,
    Reserve,
}

impl RotaKind {
    pub const ALL: [RotaKind; 3] = [RotaKind::Normal, RotaKind::Split, RotaKind::Reserve];
    pub const LABELS: [&'static str; 3] = ["Ordinary", "Split duties", "Reserve"];

    pub fn key(self) -> &'static str {
        match self {
            RotaKind::Normal => "normal",
            RotaKind::Split => "split",
            RotaKind::Reserve => "reserve",
        }
    }

    /// The kind of a key ("normal", "split", "reserve"; anything else ordinary).
    pub fn of_key(k: &str) -> RotaKind {
        Self::ALL.into_iter().find(|x| x.key() == k).unwrap_or(RotaKind::Normal)
    }

    pub fn label(self) -> &'static str {
        Self::LABELS[Self::ALL.iter().position(|x| *x == self).unwrap_or(0)]
    }
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// A rota.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Turnus {
    pub id: u32,
    pub name: String,
    /// A rota of reserve drivers (`Employment::Reserve`): its rows' days are frames.
    #[serde(default)]
    pub reserve: bool,
    /// A rota of split duties, Monday to Friday, its weekends off (an older file has none).
    #[serde(default, skip_serializing_if = "is_false")]
    pub split: bool,
    /// The Sunday its first week begins (the driver of row 1 on row 1 then).
    pub from: String,
    /// The hours a week a row aims at (None: the contract week, `econ_uk::contract_week_hours`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hours: Option<f64>,
    pub rows: Vec<Row>,
}

impl Turnus {
    pub fn kind(&self) -> RotaKind {
        if self.reserve {
            RotaKind::Reserve
        } else if self.split {
            RotaKind::Split
        } else {
            RotaKind::Normal
        }
    }
}

/// A weekend of a rota: the Saturday of one week and the Sunday of the next (the bus company's
/// week begins on Sunday) - both off, both at work, or one of each.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weekend {
    Off,
    Work,
    Half,
}

/// The weekend after week `k` of rota `t` (0 its first): week `k`'s Saturday and week `k + 1`'s
/// Sunday - the last week's Saturday with the first week's Sunday, round the cycle.
pub fn weekend(t: &Turnus, k: usize) -> Option<Weekend> {
    let n = t.rows.len();
    if k >= n {
        return None;
    }
    let sat = t.rows[k].days[DAYS - 1].works();
    let sun = t.rows[(k + 1) % n].days[0].works();
    Some(match (sat, sun) {
        (false, false) => Weekend::Off,
        (true, true) => Weekend::Work,
        _ => Weekend::Half,
    })
}

/// What a driver's day is on one date instead of his row's.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Exception {
    pub driver: u32,
    pub date: String,
    pub cell: Cell,
    /// Why (`NOTES`' keys): "" a day off, "holiday", "sick", "swap".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// The reasons of an exception, and their words.
pub const NOTES: [(&str, &str); 4] = [("", "Day off"), ("holiday", "On holiday"), ("sick", "Ill"), ("swap", "Swapped")];

/// The company's rotas and the exceptions of their drivers.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Rotas {
    pub list: Vec<Turnus>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub exceptions: Vec<Exception>,
}

impl Rotas {
    pub fn is_empty(&self) -> bool {
        self.list.is_empty() && self.exceptions.is_empty()
    }
}

// --- the calendar --------------------------------------------------------------------------------

/// The day of a row `date` is: 0 Sunday … 6 Saturday.
pub fn day_of(date: &str) -> usize {
    (plan::weekday_of(date) as usize + 1) % DAYS
}

/// The Sunday on or before `date`.
pub fn sunday_of(date: &str) -> String {
    dates::add(date, -(day_of(date) as i64))
}

/// The week number of `date` (ISO 8601: the week of its Monday's Thursday).
pub fn week_number(date: &str) -> u32 {
    let Some(d) = dates::parse(date) else { return 0 };
    let thursday = d - dates::weekday(d) as i64 + 3;
    let (y, _, _) = dates::civil_from_days(thursday);
    ((thursday - dates::days_from_civil(y, 1, 1)) / 7 + 1) as u32
}

/// The week of rota `t` that `date` is in (0 its first), None before it begins.
pub fn week_of(t: &Turnus, date: &str) -> Option<i64> {
    let from = dates::parse(&t.from)?;
    let d = dates::parse(date)?;
    (d >= from).then(|| (d - from).div_euclid(7))
}

/// The row of rota `t` its row `start`'s driver is on in the week of `date`: a row down every
/// week, from the last to the first again.
pub fn row_on(t: &Turnus, start: usize, date: &str) -> Option<usize> {
    let n = t.rows.len();
    let w = week_of(t, date)?;
    (n > 0).then(|| (start as i64 + w).rem_euclid(n as i64) as usize)
}

/// Whose row `row` of rota `t` is in the week of `date` (the driver who began on the row `week`
/// rows up).
pub fn driver_on(t: &Turnus, row: usize, date: &str) -> Option<u32> {
    let n = t.rows.len() as i64;
    let w = week_of(t, date)?;
    (n > 0).then(|| t.rows[(row as i64 - w).rem_euclid(n) as usize].driver).flatten()
}

/// The rota and the row driver `id` began on (where he is in a rota).
pub fn place_of(c: &Company, id: u32) -> Option<(usize, usize)> {
    c.planning.rotas.list.iter().enumerate().find_map(|(k, t)| t.rows.iter().position(|r| r.driver == Some(id)).map(|r| (k, r)))
}

/// A driver's day as his rota decides it.
#[derive(Clone, Debug, PartialEq)]
pub struct Decided {
    /// The rota (its id) and the row he is on that week.
    pub turnus: u32,
    pub row: usize,
    /// What he does that day, and what his row says (they differ for an exception).
    pub cell: Cell,
    pub planned: Cell,
    /// The exception's reason (`NOTES`), when one holds.
    pub note: Option<String>,
}

/// Driver `id`'s day on `date` as his rota has it (with RosterFlow Pro chosen, he on a row of a
/// rota begun by then), an exception of the date first. None: no rota decides it.
pub fn decided(c: &Company, id: u32, date: &str) -> Option<Decided> {
    if !pd::on(c) {
        return None;
    }
    let (k, start) = place_of(c, id)?;
    let t = &c.planning.rotas.list[k];
    let row = row_on(t, start, date)?;
    let planned = t.rows[row].days[day_of(date)];
    let ex = c.planning.rotas.exceptions.iter().find(|x| x.driver == id && x.date == date);
    Some(Decided { turnus: t.id, row, cell: ex.map_or(planned, |x| x.cell), planned, note: ex.map(|x| x.note.clone()) })
}

/// The frame driver `e` stands in on `date`: his rota's frame that day (with RosterFlow Pro, a
/// driver on a row), else a reserve's own frame on its weekdays.
pub fn frame_on(c: &Company, e: &Employee, date: &str) -> Option<Frame> {
    match decided(c, e.id, date) {
        Some(d) => match d.cell {
            Cell::Frame { from, to } => Some(Frame::new(from, to, 0x7f)),
            _ => None,
        },
        None => e.employment.frame().filter(|f| f.on(plan::weekday_of(date))),
    }
}

/// When the rota decides driver `id`'s day on `date`: the frame it gives him (None: none).
pub fn decides_frame(c: &Company, id: u32, date: &str) -> Option<Option<Frame>> {
    decided(c, id, date).map(|d| match d.cell {
        Cell::Frame { from, to } => Some(Frame::new(from, to, 0x7f)),
        _ => None,
    })
}

// --- the duties of a day -------------------------------------------------------------------------

/// A duty of a day as the rota knows it.
#[derive(Clone, Debug, PartialEq)]
pub struct ProDuty {
    pub number: u32,
    /// Signing on and off (minutes of the day).
    pub start: i32,
    pub end: i32,
    pub work: i32,
    pub paid: i32,
    pub kind: pd::Kind,
    /// Its pieces: the block's line, tour, and the piece of it.
    pub pieces: Vec<(String, String, usize)>,
}

/// The duties of a board, by their numbers (the spare duties left out).
pub fn duties_of_board(b: &Board) -> Vec<ProDuty> {
    b.shaped
        .iter()
        .enumerate()
        .filter(|(_, s)| s.spare.is_none())
        .map(|(row, s)| ProDuty {
            number: pd::number_of(s, row),
            start: s.start,
            end: s.end,
            work: s.work,
            paid: s.paid,
            kind: pd::kind_of(s),
            pieces: s.spans.iter().filter_map(|sp| b.tours.get(sp.tour).map(|t| (t.line.clone(), t.tour.clone(), sp.duty))).collect(),
        })
        .collect()
}

/// The duties of `date` (`tours` its blocks, as `network::tours_of_day` gives them).
pub fn day_duties(c: &Company, date: &str, tours: Vec<TourOfDay>) -> Vec<ProDuty> {
    let slot = plandays::slot_of(c, date);
    duties_of_board(&rf::board_of(c, slot, tours, Vec::new()))
}

/// The duties of a week, Sunday first (`week[day]`).
pub type Week = Vec<Vec<ProDuty>>;

fn duty<'a>(week: &'a Week, day: usize, n: u32) -> Option<&'a ProDuty> {
    week.get(day)?.iter().find(|d| d.number == n)
}

/// Signing on, off and the minutes paid of a cell on `day` (None: a day off, or a duty that does
/// not run that day).
pub fn times(week: &Week, day: usize, cell: Cell) -> Option<(i32, i32, i32)> {
    match cell {
        Cell::Off => None,
        Cell::Duty { n } => duty(week, day, n).map(|d| (d.start, d.end, d.paid)),
        Cell::Frame { from, to } => Some((from, to, (to - from).max(0))),
    }
}

/// The minutes a row pays a week.
pub fn row_minutes(week: &Week, row: &Row) -> i32 {
    (0..DAYS).filter_map(|d| times(week, d, row.days[d])).map(|x| x.2).sum()
}

/// The hours a row of rota `t` aims at: its own, else a reserve rota's drivers' frames (their
/// average), else the contract week.
pub fn target_hours(c: &Company, t: &Turnus) -> f64 {
    if let Some(h) = t.hours {
        return h;
    }
    let frames: Vec<f64> = t.rows.iter().filter_map(|r| r.driver).filter_map(|id| c.employee(id)).filter_map(|e| e.employment.frame()).map(|f| f.week_minutes() as f64 / 60.0).collect();
    if t.reserve && !frames.is_empty() {
        return frames.iter().sum::<f64>() / frames.len() as f64;
    }
    econ_uk::contract_week_hours(c)
}

// --- the checks ----------------------------------------------------------------------------------

/// Which rule a mark is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    Rest,
    InARow,
    TwoOff,
    Hours,
    Weekend,
    Duty,
    /// (An ordinary rota) every other weekend off.
    EveryOther,
    /// (A split-duty rota) split duties only, its weekends off; (an ordinary rota) no split duty.
    Split,
}

pub const M_REST: &str = "%{have} rest before this day, at least %{min}";
pub const M_IN_A_ROW: &str = "%{n} days of work in a row, at most %{max}";
pub const M_TWO_OFF: &str = "No two days off together in this week - a Saturday and the next week's Sunday count as two together";
pub const M_OVER: &str = "%{h} h a week, %{d} h more than the %{t} h aimed at";
pub const M_UNDER: &str = "%{h} h a week, %{d} h less than the %{t} h aimed at";
pub const M_CONTRACT: &str = "%{name} has a contract of %{c} h a week, the rota averages %{avg} h";
pub const M_AVERAGE: &str = "The rota averages %{avg} h a week over its cycle, %{t} h aimed at";
pub const M_WEEKEND: &str = "No weekend day off in the whole cycle";
pub const M_MISSING: &str = "Duty %{n} does not run on this day";
pub const M_TWICE: &str = "Duty %{n} is in another row on this day too";
pub const M_HALF_WEEKEND: &str = "Half a weekend: the Saturday of week %{a} and the Sunday of week %{b} should both be off or both at work";
pub const M_TWO_FREE: &str = "Two weekends off in a row: the drivers should work every other weekend";
pub const M_TWO_WORKED: &str = "Two weekends at work in a row: the drivers should have every other weekend off";
pub const M_ODD_WEEKS: &str = "%{n} weeks: with an odd number of weeks the weekends cannot take turns - add a week";
pub const M_SPLIT_HERE: &str = "Duty %{n} is a split duty: it belongs in a split-duty rota";
pub const M_NOT_SPLIT: &str = "Duty %{n} is not a split duty: a split-duty rota holds split duties only";
pub const M_SPLIT_WEEKEND: &str = "A split-duty rota is off at the weekend";

/// A mark of a rota: its row and day (None: the rota's), red (`hard`) or yellow.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub row: Option<usize>,
    pub day: Option<usize>,
    pub rule: Rule,
    pub hard: bool,
    pub text: &'static str,
    pub args: Vec<(&'static str, String)>,
}

/// A rota checked: each row's minutes a week, the cycle's average and the hours aimed at, and
/// the marks.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Checked {
    pub minutes: Vec<i32>,
    pub average: f64,
    pub target: f64,
    pub marks: Vec<Mark>,
}

impl Checked {
    /// Whether rule `r` is kept (no mark of it).
    pub fn kept(&self, r: Rule) -> bool {
        !self.marks.iter().any(|m| m.rule == r)
    }

    /// The marks of a row's day (or of the row as a whole: `day` None).
    pub fn of(&self, row: usize, day: Option<usize>) -> impl Iterator<Item = &Mark> {
        self.marks.iter().filter(move |m| m.row == Some(row) && m.day == day)
    }
}

fn h1(h: f64) -> String {
    format!("{:.1}", h).replace('.', ",")
}

/// Check rota `k` of the company over its whole cycle (`week` the duties of a week, Sunday
/// first): rest, days in a row, two days off together, the hours, a weekend day off, and the
/// duties of its days - each once a day among all the rotas.
pub fn check(c: &Company, k: usize, week: &Week) -> Checked {
    let Some(t) = c.planning.rotas.list.get(k) else { return Checked::default() };
    let n = t.rows.len();
    let target = target_hours(c, t);
    let mut out = Checked { target, ..Default::default() };
    if n == 0 {
        return out;
    }
    let cell = |p: usize| t.rows[p / DAYS].days[p % DAYS];
    let len = n * DAYS;
    // the duties: running that day, and once a day among all the rotas
    for (r, row) in t.rows.iter().enumerate() {
        for (d, x) in row.days.iter().enumerate() {
            let Cell::Duty { n: num } = *x else { continue };
            if duty(week, d, num).is_none() {
                out.marks.push(Mark { row: Some(r), day: Some(d), rule: Rule::Duty, hard: true, text: M_MISSING, args: vec![("n", num.to_string())] });
            } else if placed_count(c, d, num) > 1 {
                out.marks.push(Mark { row: Some(r), day: Some(d), rule: Rule::Duty, hard: true, text: M_TWICE, args: vec![("n", num.to_string())] });
            }
        }
    }
    // the rest between two days of work: a row's Saturday is followed by the next row's Sunday
    for p in 0..len {
        let q = (p + 1) % len;
        let (Some(a), Some(b)) = (times(week, p % DAYS, cell(p)), times(week, q % DAYS, cell(q))) else { continue };
        let rest = b.0 + 1440 - a.1;
        if rest < MIN_REST {
            out.marks.push(Mark { row: Some(q / DAYS), day: Some(q % DAYS), rule: Rule::Rest, hard: true, text: M_REST, args: vec![("have", rf::hm(rest.max(0))), ("min", rf::hm(MIN_REST))] });
        }
    }
    // days of work in a row (over the whole cycle, round it)
    if (0..len).all(|p| cell(p).works()) {
        out.marks.push(Mark { row: Some(0), day: Some(0), rule: Rule::InARow, hard: true, text: M_IN_A_ROW, args: vec![("n", len.to_string()), ("max", MAX_IN_A_ROW.to_string())] });
    } else {
        let first_off = (0..len).find(|&p| !cell(p).works()).unwrap_or(0);
        let mut run = 0;
        for s in 1..=len {
            let p = (first_off + s) % len;
            if cell(p).works() {
                run += 1;
                if run == MAX_IN_A_ROW + 1 {
                    out.marks.push(Mark { row: Some(p / DAYS), day: Some(p % DAYS), rule: Rule::InARow, hard: true, text: M_IN_A_ROW, args: vec![("n", run.to_string()), ("max", MAX_IN_A_ROW.to_string())] });
                }
            } else {
                run = 0;
            }
        }
    }
    // two days off together in each week: two days running of which one at least is the week's -
    // the Saturday before it and its Sunday count, its Saturday and the next week's Sunday too (a
    // weekend off is the two days off together of both weeks it touches)
    for r in 0..n {
        let base = r * DAYS;
        let together = (0..=DAYS).any(|i| {
            let p = (base + len + i - 1) % len;
            !cell(p).works() && !cell((p + 1) % len).works()
        });
        if !together {
            out.marks.push(Mark { row: Some(r), day: None, rule: Rule::TwoOff, hard: false, text: M_TWO_OFF, args: Vec::new() });
        }
    }
    let kind = t.kind();
    // the duties' kinds: a split-duty rota has split duties only and its weekends off, an
    // ordinary rota no split duty
    for (r, row) in t.rows.iter().enumerate() {
        for (d, x) in row.days.iter().enumerate() {
            if kind == RotaKind::Split && (d == 0 || d == DAYS - 1) && x.works() {
                out.marks.push(Mark { row: Some(r), day: Some(d), rule: Rule::Split, hard: true, text: M_SPLIT_WEEKEND, args: Vec::new() });
                continue;
            }
            let Cell::Duty { n: num } = *x else { continue };
            let Some(dk) = duty(week, d, num).map(|x| x.kind) else { continue };
            match kind {
                RotaKind::Split if dk != pd::Kind::Split => out.marks.push(Mark { row: Some(r), day: Some(d), rule: Rule::Split, hard: false, text: M_NOT_SPLIT, args: vec![("n", num.to_string())] }),
                RotaKind::Normal if dk == pd::Kind::Split => out.marks.push(Mark { row: Some(r), day: Some(d), rule: Rule::Split, hard: false, text: M_SPLIT_HERE, args: vec![("n", num.to_string())] }),
                _ => {}
            }
        }
    }
    match kind {
        // every other weekend off, round the cycle: no half weekend, and off and at work by turns
        RotaKind::Normal => {
            let ws: Vec<Weekend> = (0..n).filter_map(|k| weekend(t, k)).collect();
            for (k, w) in ws.iter().enumerate() {
                if *w == Weekend::Half {
                    out.marks.push(Mark { row: Some(k), day: Some(DAYS - 1), rule: Rule::EveryOther, hard: true, text: M_HALF_WEEKEND, args: vec![("a", (k + 1).to_string()), ("b", ((k + 1) % n + 1).to_string())] });
                }
            }
            if n % 2 == 1 {
                out.marks.push(Mark { row: None, day: None, rule: Rule::EveryOther, hard: false, text: M_ODD_WEEKS, args: vec![("n", n.to_string())] });
            } else {
                for k in 0..n {
                    let j = (k + 1) % n;
                    if ws[k] == ws[j] && ws[k] != Weekend::Half {
                        let text = if ws[k] == Weekend::Off { M_TWO_FREE } else { M_TWO_WORKED };
                        out.marks.push(Mark { row: Some(j), day: Some(DAYS - 1), rule: Rule::EveryOther, hard: true, text, args: Vec::new() });
                    }
                }
            }
        }
        // (a split-duty rota's weekends are off: its marks above)
        RotaKind::Split => {}
        // a weekend day off somewhere in the cycle
        RotaKind::Reserve => {
            if !t.rows.iter().any(|row| !row.days[0].works() || !row.days[6].works()) {
                out.marks.push(Mark { row: None, day: None, rule: Rule::Weekend, hard: false, text: M_WEEKEND, args: Vec::new() });
            }
        }
    }
    // the hours: each row's against the hours aimed at, the drivers' contracts against the average
    out.minutes = t.rows.iter().map(|row| row_minutes(week, row)).collect();
    out.average = out.minutes.iter().sum::<i32>() as f64 / 60.0 / n as f64;
    for (r, m) in out.minutes.iter().enumerate() {
        let h = *m as f64 / 60.0;
        let d = h - target;
        if d.abs() > target * HOURS_SLACK {
            let text = if d > 0.0 { M_OVER } else { M_UNDER };
            out.marks.push(Mark { row: Some(r), day: None, rule: Rule::Hours, hard: false, text, args: vec![("h", h1(h)), ("d", h1(d.abs())), ("t", h1(target))] });
        }
    }
    if (out.average - target).abs() > target * CYCLE_SLACK {
        out.marks.push(Mark { row: None, day: None, rule: Rule::Hours, hard: false, text: M_AVERAGE, args: vec![("avg", h1(out.average)), ("t", h1(target))] });
    }
    for (r, row) in t.rows.iter().enumerate() {
        let Some(e) = row.driver.and_then(|id| c.employee(id)) else { continue };
        let Some(hours) = staff::contract_hours(c, e) else { continue };
        if (hours - out.average).abs() > hours * CYCLE_SLACK {
            out.marks.push(Mark { row: Some(r), day: None, rule: Rule::Hours, hard: false, text: M_CONTRACT, args: vec![("name", e.name.clone()), ("c", h1(hours)), ("avg", h1(out.average))] });
        }
    }
    out
}

/// How many rows of all the rotas have duty `n` on `day`.
pub fn placed_count(c: &Company, day: usize, n: u32) -> usize {
    c.planning.rotas.list.iter().flat_map(|t| t.rows.iter()).filter(|r| r.days[day] == Cell::Duty { n }).count()
}

/// The duties of each day of `week` no row of any rota has ("Not in a rota").
pub fn unplaced(c: &Company, week: &Week) -> Vec<Vec<u32>> {
    week.iter().enumerate().map(|(d, ds)| ds.iter().filter(|x| placed_count(c, d, x.number) == 0).map(|x| x.number).collect()).collect()
}

// --- editing -------------------------------------------------------------------------------------

pub const NO_ROTA: &str = "There is no such rota.";
pub const NO_ROW: &str = "There is no such row.";
pub const NOT_THAT_DAY: &str = "That duty does not run on this day.";
pub const TAKEN: &str = "That duty is in another row on this day already.";
pub const NOT_A_DRIVER: &str = "Only a driver of the company can be put on a row.";
pub const NO_HOURLY: &str = "An hourly-paid driver has no rota: he is called in when needed.";
pub const RESERVES_ONLY: &str = "A reserve rota is for the reserve drivers.";
pub const NO_RESERVES: &str = "A reserve driver goes in a reserve rota.";
pub const NOTHING_TO_PLACE: &str = "Every duty of the week is in a rota already.";
pub const WEEKEND_OFF: &str = "A split-duty rota is off on Saturday and Sunday.";
pub const ONLY_SPLIT: &str = "A split-duty rota holds split duties only.";
pub const SPLIT_ELSEWHERE: &str = "A split duty goes in a split-duty rota.";
pub const NO_SPLIT: &str = "The week has no split duty from Monday to Friday that no rota has.";

fn rota_mut(c: &mut Company, id: u32) -> Result<&mut Turnus, &'static str> {
    c.planning.rotas.list.iter_mut().find(|t| t.id == id).ok_or(NO_ROTA)
}

/// A new rota (`reserve`: of reserve drivers), beginning the Sunday of the company's week:
/// its id.
pub fn add(c: &mut Company, name: &str, reserve: bool) -> u32 {
    add_kind(c, name, if reserve { RotaKind::Reserve } else { RotaKind::Normal }, 0)
}

/// The most weeks a rota is made with.
pub const MAX_WEEKS: usize = 104;

/// A new rota of `kind` and `weeks` weeks (days off all week), beginning the Sunday of the
/// company's week: its id.
pub fn add_kind(c: &mut Company, name: &str, kind: RotaKind, weeks: usize) -> u32 {
    let id = c.planning.rotas.list.iter().map(|t| t.id).max().unwrap_or(0) + 1;
    let fallback = match kind {
        RotaKind::Normal => "Rota",
        RotaKind::Split => "Split duties",
        RotaKind::Reserve => "Reserve",
    };
    let name = if name.trim().is_empty() { format!("{fallback} {id}") } else { name.trim().chars().take(40).collect() };
    let rows = vec![Row::default(); weeks.min(MAX_WEEKS)];
    c.planning.rotas.list.push(Turnus { id, name, reserve: kind == RotaKind::Reserve, split: kind == RotaKind::Split, from: sunday_of(&c.date), hours: None, rows });
    id
}

/// Rota `id` gone (its drivers' exceptions with it).
pub fn remove(c: &mut Company, id: u32) -> bool {
    let Some(k) = c.planning.rotas.list.iter().position(|t| t.id == id) else { return false };
    let t = c.planning.rotas.list.remove(k);
    let drivers: Vec<u32> = t.rows.iter().filter_map(|r| r.driver).collect();
    c.planning.rotas.exceptions.retain(|x| !drivers.contains(&x.driver));
    true
}

pub fn rename(c: &mut Company, id: u32, name: &str) -> Result<(), &'static str> {
    let t = rota_mut(c, id)?;
    if !name.trim().is_empty() {
        t.name = name.trim().chars().take(40).collect();
    }
    Ok(())
}

/// The hours a row of rota `id` aims at (None: the contract week).
pub fn set_hours(c: &mut Company, id: u32, hours: Option<f64>) -> Result<(), &'static str> {
    rota_mut(c, id)?.hours = hours.map(|h| (h * 2.0).round().clamp(2.0, 120.0) / 2.0);
    Ok(())
}

/// The week rota `id` begins (the Sunday of `date`'s week): its row 1 for row 1's driver then.
pub fn set_from(c: &mut Company, id: u32, date: &str) -> Result<(), &'static str> {
    rota_mut(c, id)?.from = sunday_of(date);
    Ok(())
}

/// A row more at the bottom of rota `id`: days off all week.
pub fn add_row(c: &mut Company, id: u32) -> Result<usize, &'static str> {
    let t = rota_mut(c, id)?;
    t.rows.push(Row::default());
    Ok(t.rows.len())
}

/// Row `row` of rota `id` gone (its driver off the rota).
pub fn remove_row(c: &mut Company, id: u32, row: usize) -> Result<(), &'static str> {
    let t = rota_mut(c, id)?;
    if row >= t.rows.len() {
        return Err(NO_ROW);
    }
    t.rows.remove(row);
    Ok(())
}

/// Day `day` (0 Sunday) of row `row` of rota `id` set to `cell`: a duty only of those that day
/// (`week`), and only one row of all the rotas has it.
pub fn set_cell(c: &mut Company, id: u32, row: usize, day: usize, cell: Cell, week: &Week) -> Result<(), &'static str> {
    if day >= DAYS {
        return Err(NO_ROW);
    }
    // (a split-duty rota: split duties only, Monday to Friday; an ordinary one: no split duty)
    let kind = c.planning.rotas.list.iter().find(|t| t.id == id).ok_or(NO_ROTA)?.kind();
    if kind == RotaKind::Split && (day == 0 || day == DAYS - 1) && cell.works() {
        return Err(WEEKEND_OFF);
    }
    if let Cell::Duty { n } = cell {
        let Some(dk) = duty(week, day, n).map(|x| x.kind) else { return Err(NOT_THAT_DAY) };
        match kind {
            RotaKind::Split if dk != pd::Kind::Split => return Err(ONLY_SPLIT),
            RotaKind::Normal if dk == pd::Kind::Split => return Err(SPLIT_ELSEWHERE),
            _ => {}
        }
        let mine = c.planning.rotas.list.iter().find(|t| t.id == id).and_then(|t| t.rows.get(row)).is_some_and(|r| r.days[day] == cell);
        if !mine && placed_count(c, day, n) > 0 {
            return Err(TAKEN);
        }
    }
    let cell = match cell {
        Cell::Frame { from, to } => {
            let f = Frame::new(from, to, 0x7f);
            Cell::Frame { from: f.from, to: f.to }
        }
        x => x,
    };
    let t = rota_mut(c, id)?;
    let r = t.rows.get_mut(row).ok_or(NO_ROW)?;
    r.days[day] = cell;
    Ok(())
}

/// Driver `who` put on row `row` of rota `id` (off any row he had); None: the row without one.
/// Full- and part-time drivers go in a rota, reserves in a reserve rota, the hourly paid in none.
pub fn set_driver(c: &mut Company, id: u32, row: usize, who: Option<u32>) -> Result<(), &'static str> {
    let reserve = c.planning.rotas.list.iter().find(|t| t.id == id).ok_or(NO_ROTA)?.reserve;
    if let Some(w) = who {
        let e = c.employee(w).filter(|e| staff::drives(e)).ok_or(NOT_A_DRIVER)?;
        match (e.employment, reserve) {
            (Employment::Hourly, _) => return Err(NO_HOURLY),
            (Employment::Reserve(_), false) => return Err(NO_RESERVES),
            (Employment::FullTime | Employment::PartTime { .. }, true) => return Err(RESERVES_ONLY),
            _ => {}
        }
    }
    if c.planning.rotas.list.iter().find(|t| t.id == id).is_some_and(|t| row >= t.rows.len()) {
        return Err(NO_ROW);
    }
    if let Some(w) = who {
        for t in c.planning.rotas.list.iter_mut() {
            for r in t.rows.iter_mut().filter(|r| r.driver == Some(w)) {
                r.driver = None;
            }
        }
    }
    rota_mut(c, id)?.rows[row].driver = who;
    Ok(())
}

/// Driver `id`'s day on `date` set to `cell` for that date only (`note` why: `NOTES`), or back to
/// his row's (None).
pub fn set_exception(c: &mut Company, driver: u32, date: &str, cell: Option<(Cell, String)>) {
    let ex = &mut c.planning.rotas.exceptions;
    ex.retain(|x| !(x.driver == driver && x.date == date));
    if let Some((cell, note)) = cell {
        let note = if NOTES.iter().any(|n| n.0 == note) { note } else { String::new() };
        ex.push(Exception { driver, date: date.to_string(), cell, note });
        ex.sort_by(|a, b| (a.date.as_str(), a.driver).cmp(&(b.date.as_str(), b.driver)));
    }
}

/// The exceptions of days gone (before `today`) dropped.
pub fn tidy(c: &mut Company) {
    // (step 4: the daily operations' days gone too)
    super::pro_ops::tidy(c);
    let today = c.date.clone();
    let drivers: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    c.planning.rotas.exceptions.retain(|x| dates::between(&today, &x.date) >= 0 && drivers.contains(&x.driver));
    for t in c.planning.rotas.list.iter_mut() {
        for r in t.rows.iter_mut() {
            if r.driver.is_some_and(|id| !drivers.contains(&id)) {
                r.driver = None;
            }
        }
    }
}

// --- made automatically --------------------------------------------------------------------------

/// The days off of rows working `work[r]` days each, `need[d]` rows at least at work on day `d`:
/// for each row a run of days off together - begun at most a day after the row before's ended,
/// so that from one row's days off to the next row's are at most 6 days of work (the last row's
/// and the first's too) -, where the week has the most rows to spare - the weekend, mostly -,
/// the rows' runs spread over the week. Searched for runs that keep both (`search`), else the
/// best a row at a time.
fn days_off(work: &[usize], need: &[usize; DAYS]) -> Vec<[bool; DAYS]> {
    search(work, need).unwrap_or_else(|| days_off_greedy(work, need))
}

/// Runs of days off for every row that keep the days of work in a row to 6 (round the cycle)
/// and leave each day `need[d]` rows at work: a search, the likeliest runs first (where the
/// week has the most rows to spare, the weekend). None when there are none (or the search took
/// too long).
fn search(work: &[usize], need: &[usize; DAYS]) -> Option<Vec<[bool; DAYS]>> {
    let n = work.len();
    let cap: [i64; DAYS] = std::array::from_fn(|d| n as i64 - need[d] as i64);
    struct S<'a> {
        work: &'a [usize],
        cap: [i64; DAYS],
        off: [i64; DAYS],
        starts: Vec<(usize, usize)>,
        nodes: usize,
    }
    fn go(st: &mut S, r: usize) -> bool {
        st.nodes += 1;
        let n = st.work.len();
        if st.nodes > 40_000 {
            return false;
        }
        if r == n {
            return true;
        }
        let off_n = DAYS - st.work[r].min(DAYS);
        if off_n == 0 {
            st.starts.push((DAYS, DAYS));
            if go(st, r + 1) {
                return true;
            }
            st.starts.pop();
            return false;
        }
        // (the room left must hold the days off of the rows left)
        let left: i64 = (0..DAYS).map(|d| st.cap[d] - st.off[d]).sum();
        let want: i64 = st.work[r..].iter().map(|w| (DAYS - (*w).min(DAYS)) as i64).sum();
        if left < want {
            return false;
        }
        let last = st.starts.iter().rev().find(|x| x.0 < DAYS).copied();
        let first = st.starts.iter().find(|x| x.0 < DAYS).map(|x| x.0);
        let mut tries: Vec<(i64, i64, usize)> = Vec::new();
        for s in 0..=DAYS - off_n {
            let e = s + off_n - 1;
            if (s..=e).any(|d| st.off[d] + 1 > st.cap[d]) {
                continue;
            }
            if last.is_some_and(|(_, le)| (DAYS - 1 - le) + s > MAX_IN_A_ROW) {
                continue;
            }
            if r + 1 == n && first.or((r == 0).then_some(s)).is_some_and(|fs| (DAYS - 1 - e) + fs > MAX_IN_A_ROW) {
                continue;
            }
            let room: i64 = (s..=e).map(|d| st.cap[d] - st.off[d]).sum();
            let weekend = (s..=e).any(|d| d == 0 || d == DAYS - 1) as i64;
            tries.push((room + weekend * 2, (last.is_none_or(|(ls, _)| ls != s)) as i64, s));
        }
        tries.sort_by(|a, b| (b.0, b.1).cmp(&(a.0, a.1)));
        for (_, _, s) in tries {
            for d in s..s + off_n {
                st.off[d] += 1;
            }
            st.starts.push((s, s + off_n - 1));
            if go(st, r + 1) {
                return true;
            }
            st.starts.pop();
            for d in s..s + off_n {
                st.off[d] -= 1;
            }
        }
        false
    }
    let mut st = S { work, cap, off: [0; DAYS], starts: Vec::new(), nodes: 0 };
    if !go(&mut st, 0) {
        return None;
    }
    Some(
        st.starts
            .iter()
            .map(|&(s, e)| {
                let mut row = [false; DAYS];
                if s < DAYS {
                    for d in s..=e {
                        row[d] = true;
                    }
                }
                row
            })
            .collect(),
    )
}

/// `days_off` a row at a time, where the search finds none.
fn days_off_greedy(work: &[usize], need: &[usize; DAYS]) -> Vec<[bool; DAYS]> {
    let n = work.len();
    let mut off = [0usize; DAYS];
    let mut out = Vec::with_capacity(n);
    // (where the row before's days off began and ended, and the first row's began)
    let mut last: Option<(usize, usize)> = None;
    let mut first: Option<usize> = None;
    for (r, w) in work.iter().enumerate() {
        let off_n = DAYS - (*w).min(DAYS);
        let mut row = [false; DAYS];
        if off_n > 0 {
            let spare = |d: usize, off: &[usize; DAYS]| n as i64 - need[d] as i64 - off[d] as i64;
            // (a run within the week; Saturday and Sunday of one row are no run, but both are
            // the weekend)
            let mut best: Option<(i64, i64, i64, i64, usize)> = None;
            for s in 0..=DAYS - off_n {
                let e = s + off_n - 1;
                let ds: Vec<usize> = (s..=e).collect();
                let least = ds.iter().map(|&d| spare(d, &off)).min().unwrap_or(0);
                let sum: i64 = ds.iter().map(|&d| spare(d, &off)).sum();
                let weekend = ds.iter().any(|&d| d == 0 || d == DAYS - 1) as i64;
                // (at most 6 days of work from the row before's days off to these, and from
                // the last row's to the first's)
                let after = last.is_none_or(|(_, le)| (DAYS - 1 - le) + s <= MAX_IN_A_ROW);
                let round = r + 1 < n || first.is_none_or(|fs| (DAYS - 1 - e) + fs <= MAX_IN_A_ROW);
                let run = (after && round) as i64;
                // (another start than the row before: the runs spread)
                let other = last.is_none_or(|(ls, _)| ls != s) as i64;
                let key = (run, least.min(1), sum + weekend * 2, other, s);
                if best.is_none_or(|b| (key.0, key.1, key.2, key.3) > (b.0, b.1, b.2, b.3)) {
                    best = Some(key);
                }
            }
            let s = best.map(|b| b.4).unwrap_or(0);
            for d in s..s + off_n {
                row[d] = true;
                off[d] += 1;
            }
            last = Some((s, s + off_n - 1));
            first.get_or_insert(s);
        }
        out.push(row);
    }
    out
}

/// Rota `id` made automatically from the duties of `week` no other rota has, in as many weeks as
/// they need - and at least the weeks it has:
///
/// * An ordinary rota (`auto_normal`): the whole and short duties of the week, an even number of
///   weeks, the drivers at work every other weekend - a weekend off is a week's Saturday and the
///   next week's Sunday -, a weekday off besides in each week, so that each week has five days
///   of work and never more than six days of work come in a row; enough weeks that the weekend's
///   duties are driven by half the drivers. A weekend with more drivers at work than duties gives
///   the others a reserve's frame, so that their weekend stays whole.
/// * A split-duty rota (`auto_split`): the split duties of Monday to Friday, its weekends off.
/// * A reserve rota: a row for each reserve driver (one at least), 5 days a week in its driver's
///   frame (early and late else).
///
/// Each day's duties go to the weeks by their times - the week that finished early yesterday
/// takes an early one today -, then are swapped between weeks where that keeps the rest and evens
/// the hours. The drivers stay on their rows (as far as there are rows). Returns how many weeks
/// it has.
pub fn auto(c: &mut Company, id: u32, week: &Week) -> Result<usize, &'static str> {
    let k = c.planning.rotas.list.iter().position(|t| t.id == id).ok_or(NO_ROTA)?;
    let t = c.planning.rotas.list[k].clone();
    let drivers: Vec<Option<u32>> = t.rows.iter().map(|r| r.driver).collect();
    let rows = match t.kind() {
        RotaKind::Reserve => auto_reserve(c, &t),
        RotaKind::Normal => auto_normal(c, k, &t, week)?,
        RotaKind::Split => auto_split(c, k, &t, week)?,
    };
    let rota = &mut c.planning.rotas.list[k];
    rota.rows = rows;
    for (r, d) in drivers.into_iter().enumerate() {
        if let Some(row) = rota.rows.get_mut(r) {
            row.driver = d;
        }
    }
    Ok(rota.rows.len())
}

fn auto_reserve(c: &Company, t: &Turnus) -> Vec<Row> {
    let reserves: Vec<&Employee> = c.staff.iter().filter(|e| staff::drives(e) && matches!(e.employment, Employment::Reserve(_))).collect();
    let n = t.rows.len().max(reserves.len()).max(1);
    let need = [n * 5 / DAYS; DAYS];
    let off = days_off(&vec![5; n], &need);
    (0..n)
        .map(|r| {
            let own = t.rows.get(r).and_then(|x| x.driver).and_then(|id| c.employee(id)).and_then(|e| e.employment.frame());
            let (from, to) = own.map(|f| (f.from, f.to)).unwrap_or(RESERVE_FRAMES[r % RESERVE_FRAMES.len()]);
            let mut row = Row::default();
            for d in 0..DAYS {
                row.days[d] = if off[r][d] { Cell::Off } else { Cell::Frame { from, to } };
            }
            row
        })
        .collect()
}

/// The duties of each day of `week` no rota but the `k`th has (None: no rota at all) that `keep`
/// keeps, each day's by their times.
fn todo_of<'a>(c: &Company, k: Option<usize>, week: &'a Week, keep: impl Fn(&ProDuty) -> bool) -> Vec<Vec<&'a ProDuty>> {
    let others = |d: usize, num: u32| c.planning.rotas.list.iter().enumerate().any(|(j, o)| Some(j) != k && o.rows.iter().any(|r| r.days[d] == Cell::Duty { n: num }));
    (0..DAYS)
        .map(|d| {
            let mut ds: Vec<&ProDuty> = week.get(d).map(|ds| ds.iter().filter(|x| keep(x) && !others(d, x.number)).collect()).unwrap_or_default();
            ds.sort_by_key(|x| (x.start, x.number));
            ds
        })
        .collect()
}

/// Whether a duty goes in an ordinary rota (not a split duty).
fn ordinary(x: &ProDuty) -> bool {
    x.kind != pd::Kind::Split
}

/// Whether a duty goes in a split-duty rota (a split duty).
fn split_duty(x: &ProDuty) -> bool {
    x.kind == pd::Kind::Split
}

/// The weeks an ordinary rota needs for `todo` (the duties of each day, Sunday first), at least
/// `least`: an even number; half of them at work each weekend, every weekend duty a week; a
/// weekday off in every week, as many weeks as leave each weekday a week at work for each of its
/// duties; the hours aimed at (`target` minutes a week) over the cycle; two at least.
fn normal_weeks(todo: &[Vec<&ProDuty>], target: i32, least: usize) -> usize {
    let weekend = todo[0].len().max(todo[DAYS - 1].len());
    let weekdays: usize = (1..DAYS - 1).map(|d| todo[d].len()).sum();
    let busiest = (1..DAYS - 1).map(|d| todo[d].len()).max().unwrap_or(0);
    let paid: i64 = todo.iter().flatten().map(|x| x.paid as i64).sum();
    let by_hours = (paid as f64 / target.max(60) as f64).round() as usize;
    let mut n = (2 * weekend).max(weekdays.div_ceil(4)).max(busiest).max(by_hours).max(least).max(2);
    n += n % 2;
    // (each week a weekday off: the weekdays' room for them must hold one a week)
    while (1..DAYS - 1).map(|d| n.saturating_sub(todo[d].len())).sum::<usize>() < n && n < MAX_WEEKS {
        n += 2;
    }
    n.min(MAX_WEEKS)
}

/// The weeks a split-duty rota needs for `todo`: as many as the busiest weekday has split duties,
/// at least `least`, one at least.
fn split_weeks(todo: &[Vec<&ProDuty>], least: usize) -> usize {
    (1..DAYS - 1).map(|d| todo[d].len()).max().unwrap_or(0).max(least).max(1).min(MAX_WEEKS)
}

/// How many weeks "Make the rota automatically" would make a new rota of `kind` with: the duties
/// of the week no rota has, the reserve drivers on no row.
pub fn suggest_weeks(c: &Company, kind: RotaKind, week: &Week) -> usize {
    match kind {
        RotaKind::Normal => {
            let todo = todo_of(c, None, week, ordinary);
            if todo.iter().all(Vec::is_empty) {
                return 0;
            }
            normal_weeks(&todo, (econ_uk::contract_week_hours(c) * 60.0).round() as i32, 0)
        }
        RotaKind::Split => {
            let todo = todo_of(c, None, week, split_duty);
            if (1..DAYS - 1).all(|d| todo[d].is_empty()) {
                return 0;
            }
            split_weeks(&todo, 0)
        }
        RotaKind::Reserve => c.staff.iter().filter(|e| staff::drives(e) && matches!(e.employment, Employment::Reserve(_)) && place_of(c, e.id).is_none()).count().max(1),
    }
}

/// The duties of the week no rota has that a rota of `kind` takes: an ordinary rota's whole and
/// short duties, a split-duty rota's split duties of Monday to Friday (a reserve rota none).
pub fn left_over(c: &Company, kind: RotaKind, week: &Week) -> usize {
    match kind {
        RotaKind::Normal => todo_of(c, None, week, ordinary).iter().map(Vec::len).sum(),
        RotaKind::Split => split_open(c, week),
        RotaKind::Reserve => 0,
    }
}

/// The split duties of Monday to Friday no rota has: they belong in a split-duty rota.
pub fn split_open(c: &Company, week: &Week) -> usize {
    let todo = todo_of(c, None, week, split_duty);
    (1..DAYS - 1).map(|d| todo[d].len()).sum()
}

/// The weekday off of each of `n` weeks (1 Monday … 5 Friday) of an ordinary rota: as many on a
/// weekday as it has weeks to spare (`n` less its duties), spread; of each two weeks - one whose
/// weekend begins at work, the next whose weekend begins off - the second's on or before the
/// first's, so that from the first's weekday off over the weekend at work to the second's are at
/// most six days of work.
fn weekdays_off(todo: &[Vec<&ProDuty>], n: usize) -> Vec<usize> {
    let mut off = [0usize; DAYS];
    let mut picks: Vec<usize> = Vec::with_capacity(n);
    for _ in 0..n {
        // (the weekday with the most room left, the later on a tie: a long weekend off)
        let d = (1..DAYS - 1).max_by_key(|&d| (n as i64 - todo[d].len() as i64 - off[d] as i64, d as i64)).unwrap_or(1);
        off[d] += 1;
        picks.push(d);
    }
    picks.sort_unstable();
    let mut out = vec![1usize; n];
    for j in 0..n / 2 {
        out[2 * j] = picks[2 * j + 1];
        out[2 * j + 1] = picks[2 * j];
    }
    out
}

/// The duties of a weekend day given to the weeks `at` at work that day, and a reserve's frame
/// (`RESERVE_FRAMES`) to each week more than there are duties - so that its weekend stays whole:
/// the weeks bound on both sides first (a Sunday: by the week before's Saturday and its own
/// Monday), the latest bound first; each takes the duty that keeps 11 hours of rest on both sides
/// best, the earliest on a tie - or a frame, when a frame is left and keeps it better.
fn give_weekend(rows: &mut [Row], minutes: &mut [i32], week: &Week, day: usize, at: &[usize], duties: &[&ProDuty]) {
    let n = rows.len();
    let lo_of = |r: usize, rows: &[Row]| -> Option<i32> {
        let (pr, pd) = if day == 0 { ((r + n - 1) % n, DAYS - 1) } else { (r, day - 1) };
        times(week, pd, rows[pr].days[pd]).map(|x| x.1 + MIN_REST - 1440)
    };
    let hi_of = |r: usize, rows: &[Row]| -> Option<i32> { if day == 0 { times(week, 1, rows[r].days[1]).map(|x| x.0 + 1440 - MIN_REST) } else { None } };
    let short = |lo: Option<i32>, hi: Option<i32>, from: i32, to: i32| lo.map_or(0, |lo| (lo - from).max(0)) + hi.map_or(0, |hi| (to - hi).max(0));
    let mut left: Vec<&ProDuty> = duties.to_vec();
    let mut frames = at.len().saturating_sub(duties.len());
    let mut order: Vec<usize> = at.to_vec();
    order.sort_by_key(|&r| {
        let (lo, hi) = (lo_of(r, rows), hi_of(r, rows));
        (std::cmp::Reverse(lo.is_some() as u8 + hi.is_some() as u8), std::cmp::Reverse(lo.unwrap_or(i32::MIN / 2)), r)
    });
    for r in order {
        let (lo, hi) = (lo_of(r, rows), hi_of(r, rows));
        let best = (0..left.len()).min_by_key(|&k| (short(lo, hi, left[k].start, left[k].end), left[k].start, left[k].number));
        let frame = (frames > 0).then(|| RESERVE_FRAMES.iter().copied().min_by_key(|f| short(lo, hi, f.0, f.1)).unwrap_or(RESERVE_FRAMES[0]));
        let frame_better = match (best, frame) {
            (Some(k), Some(f)) => short(lo, hi, f.0, f.1) < short(lo, hi, left[k].start, left[k].end),
            (None, Some(_)) => true,
            _ => false,
        };
        if let (true, Some((from, to))) = (frame_better, frame) {
            rows[r].days[day] = Cell::Frame { from, to };
            minutes[r] += to - from;
            frames -= 1;
        } else if let Some(k) = best {
            let x = left.remove(k);
            rows[r].days[day] = Cell::Duty { n: x.number };
            minutes[r] += x.paid;
        }
    }
}

/// The duties of `day` given to the weeks `at` (those at work that day): the week that finished
/// earliest the day before the earliest duty; on a Sunday each week the duty that keeps the rest
/// on both sides, the weeks bound on both first. The minutes paid of each week go up by its duty.
fn give_day(rows: &mut [Row], minutes: &mut [i32], week: &Week, day: usize, at: &[usize], given: &[&ProDuty]) {
    let n = rows.len();
    let prev_end = |r: usize, rows: &[Row]| -> Option<i32> {
        let (pr, pd) = if day == 0 { ((r + n - 1) % n, DAYS - 1) } else { (r, day - 1) };
        times(week, pd, rows[pr].days[pd]).map(|x| x.1)
    };
    let next_start = |r: usize, rows: &[Row]| -> Option<i32> { if day == 0 { times(week, 1, rows[r].days[1]).map(|x| x.0) } else { None } };
    let mut given: Vec<&ProDuty> = given.to_vec();
    given.sort_by_key(|x| (x.start, x.number));
    let mut at: Vec<usize> = at.to_vec();
    if day != 0 {
        at.sort_by_key(|&r| (prev_end(r, rows).unwrap_or(i32::MIN / 2), r));
        for (&r, x) in at.iter().zip(given.iter()) {
            rows[r].days[day] = Cell::Duty { n: x.number };
            minutes[r] += x.paid;
        }
    } else {
        at.sort_by_key(|&r| (prev_end(r, rows).is_none() as u8 + next_start(r, rows).is_none() as u8, r));
        for r in at {
            let lo = prev_end(r, rows).map(|e| e + MIN_REST - 1440);
            let hi = next_start(r, rows).map(|s| s + 1440 - MIN_REST);
            let short = |x: &ProDuty| lo.map_or(0, |lo| (lo - x.start).max(0)) + hi.map_or(0, |hi| (x.end - hi).max(0));
            let Some(k) = (0..given.len()).min_by_key(|&k| (short(given[k]), k)) else { break };
            let x = given.remove(k);
            rows[r].days[day] = Cell::Duty { n: x.number };
            minutes[r] += x.paid;
        }
    }
}

/// The weeks at work on `day` cut to its duties: more weeks than duties, those with the most hours
/// so far have the day off too; more duties than weeks, the longest are given, the rest stay "not
/// in a rota".
fn fit_day<'a>(at: &mut Vec<usize>, duties: &[&'a ProDuty], minutes: &[i32]) -> Vec<&'a ProDuty> {
    if at.len() > duties.len() {
        at.sort_by_key(|&r| (std::cmp::Reverse(minutes[r]), r));
        at.drain(..at.len() - duties.len());
    }
    let mut given: Vec<&ProDuty> = duties.to_vec();
    if given.len() > at.len() {
        given.sort_by_key(|x| std::cmp::Reverse(x.paid));
        given.truncate(at.len());
    }
    given
}

fn auto_normal(c: &Company, k: usize, t: &Turnus, week: &Week) -> Result<Vec<Row>, &'static str> {
    let todo = todo_of(c, Some(k), week, ordinary);
    if todo.iter().all(Vec::is_empty) {
        return Err(NOTHING_TO_PLACE);
    }
    let target = (target_hours(c, t) * 60.0).round() as i32;
    let n = normal_weeks(&todo, target, t.rows.len());
    // the days off: week 1, 3, 5 ... begin their weekend at work (their Saturday and the next
    // week's Sunday), week 2, 4, 6 ... off - so each week has one weekend day off -, and a weekday
    // off besides
    let x = weekdays_off(&todo, n);
    let mut rows: Vec<Row> = vec![Row::default(); n];
    let mut off = vec![[false; DAYS]; n];
    for r in 0..n {
        off[r][0] = r % 2 == 0;
        off[r][DAYS - 1] = r % 2 == 1;
        off[r][x[r]] = true;
    }
    let mut minutes = vec![0i32; n];
    // Monday to Saturday first, each by the day before; Sunday last, between the week before's
    // Saturday and its own Monday
    for d in (1..DAYS).chain([0]) {
        let mut at: Vec<usize> = (0..n).filter(|&r| !off[r][d]).collect();
        if d == 0 || d == DAYS - 1 {
            // (a weekend: every week at work keeps it - a duty, or a reserve's frame)
            let mut duties = todo[d].clone();
            if duties.len() > at.len() {
                duties.sort_by_key(|x| std::cmp::Reverse(x.paid));
                duties.truncate(at.len());
            }
            give_weekend(&mut rows, &mut minutes, week, d, &at, &duties);
        } else {
            let given = fit_day(&mut at, &todo[d], &minutes);
            give_day(&mut rows, &mut minutes, week, d, &at, &given);
        }
    }
    even_out(&mut rows, week, target);
    Ok(rows)
}

fn auto_split(c: &Company, k: usize, t: &Turnus, week: &Week) -> Result<Vec<Row>, &'static str> {
    let mut todo = todo_of(c, Some(k), week, split_duty);
    // (split duties do not run at the weekend: a split-duty rota's weekends are off)
    todo[0].clear();
    todo[DAYS - 1].clear();
    if todo.iter().all(Vec::is_empty) {
        return Err(NO_SPLIT);
    }
    let target = (target_hours(c, t) * 60.0).round() as i32;
    let n = split_weeks(&todo, t.rows.len());
    let mut rows: Vec<Row> = vec![Row::default(); n];
    let mut minutes = vec![0i32; n];
    for d in 1..DAYS - 1 {
        let mut at: Vec<usize> = (0..n).collect();
        let given = fit_day(&mut at, &todo[d], &minutes);
        give_day(&mut rows, &mut minutes, week, d, &at, &given);
    }
    even_out(&mut rows, week, target);
    Ok(rows)
}

/// Swaps between two weeks of a day - or of two or three days running, both weeks at work on the
/// same of them - where the rota costs less so (`local_cost`: the rest, the hours).
fn even_out(rows: &mut [Row], week: &Week, target: i32) {
    let n = rows.len();
    for _ in 0..6 {
        let mut better = false;
        for k in 1..=3usize {
            for d in 0..=DAYS - k {
                for a in 0..n {
                    for b in a + 1..n {
                        let block = d..d + k;
                        if block.clone().any(|x| rows[a].days[x].works() != rows[b].days[x].works()) || block.clone().all(|x| rows[a].days[x] == rows[b].days[x]) || !block.clone().any(|x| rows[a].days[x].works()) {
                            continue;
                        }
                        let before = local_cost(rows, week, target, block.clone(), a, b);
                        for x in block.clone() {
                            let (p, q) = (rows[a].days[x], rows[b].days[x]);
                            rows[a].days[x] = q;
                            rows[b].days[x] = p;
                        }
                        if local_cost(rows, week, target, block.clone(), a, b) < before {
                            better = true;
                        } else {
                            for x in block {
                                let (p, q) = (rows[a].days[x], rows[b].days[x]);
                                rows[a].days[x] = q;
                                rows[b].days[x] = p;
                            }
                        }
                    }
                }
            }
        }
        if !better {
            break;
        }
    }
}

/// The cost of the rests about days `days` of rows `a` and `b` and of the two rows' hours: each
/// rest under 11 hours much, each minute away from the hours aimed at a little.
fn local_cost(rows: &[Row], week: &Week, target: i32, days: std::ops::Range<usize>, a: usize, b: usize) -> i64 {
    let n = rows.len();
    let len = n * DAYS;
    let cell = |p: usize| rows[p / DAYS].days[p % DAYS];
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for r in [a, b] {
        for d in days.clone() {
            let p = r * DAYS + d;
            for pair in [((p + len - 1) % len, p), (p, (p + 1) % len)] {
                if !pairs.contains(&pair) {
                    pairs.push(pair);
                }
            }
        }
    }
    let mut out = 0i64;
    for (x, y) in pairs {
        if let (Some(s), Some(t)) = (times(week, x % DAYS, cell(x)), times(week, y % DAYS, cell(y))) {
            let rest = t.0 + 1440 - s.1;
            if rest < MIN_REST {
                out += 10_000 + (MIN_REST - rest) as i64;
            }
        }
    }
    for r in [a, b] {
        out += (row_minutes(week, &rows[r]) - target).abs() as i64;
    }
    out
}

// --- the day -------------------------------------------------------------------------------------

/// What the rotas decide of a day: who drives which piece of a duty, whose day they decide, and
/// the pieces of a duty whose driver is away by an exception (ill, on holiday).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RotaDay {
    pub given: Vec<(String, String, usize, u32)>,
    pub decided: Vec<u32>,
    pub absent: Vec<(String, String, usize, Problem)>,
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

impl RotaDay {
    /// The driver the rota gives piece `k` of tour `tour` of line `line`.
    pub fn driver_of(&self, line: &str, tour: &str, k: usize) -> Option<u32> {
        self.given.iter().find(|g| g.2 == k && same(&g.0, line) && same(&g.1, tour)).map(|g| g.3)
    }

    /// Why piece `k` of a tour has no driver: its rota's driver is away by an exception.
    pub fn absent_of(&self, line: &str, tour: &str, k: usize) -> Option<Problem> {
        self.absent.iter().find(|g| g.2 == k && same(&g.0, line) && same(&g.1, tour)).map(|g| g.3)
    }

    pub fn decides(&self, id: u32) -> bool {
        self.decided.contains(&id)
    }
}

/// What the rotas decide of `date` (`tours` its blocks): None when nothing (RosterFlow Pro not
/// chosen, no driver on a row of a rota begun by then).
pub fn on_day(c: &Company, date: &str, tours: &[TourOfDay]) -> Option<RotaDay> {
    if !pd::on(c) {
        return None;
    }
    let mine: Vec<(u32, Decided)> = c.staff.iter().filter_map(|e| decided(c, e.id, date).map(|d| (e.id, d))).collect();
    // (step 4: the duties the daily operations gave for the date - `pro_ops::give`)
    let calls: Vec<(u32, u32)> = super::pro_ops::calls_on(c, date).filter(|x| c.employee(x.driver).is_some()).map(|x| (x.n, x.driver)).collect();
    if mine.is_empty() && calls.is_empty() {
        return None;
    }
    let mut out = RotaDay { decided: mine.iter().map(|m| m.0).collect(), ..Default::default() };
    let wants = !calls.is_empty() || mine.iter().any(|(_, d)| matches!(d.cell, Cell::Duty { .. }) || matches!(d.planned, Cell::Duty { .. }));
    if !wants {
        return Some(out);
    }
    let duties = day_duties(c, date, tours.to_vec());
    let pieces = |n: u32| duties.iter().find(|x| x.number == n).map(|x| x.pieces.clone()).unwrap_or_default();
    // (a duty given for the day first: its driver's, whoever's row has it)
    for (n, id) in &calls {
        for (line, tour, k) in pieces(*n) {
            if !out.given.iter().any(|g| g.0 == line && g.1 == tour && g.2 == k) {
                out.given.push((line, tour, k, *id));
            }
        }
    }
    // (a duty given by an exception first: it is the swapper's, the row's driver is not on it)
    let mut ordered: Vec<&(u32, Decided)> = mine.iter().collect();
    ordered.sort_by_key(|(_, d)| d.note.is_none());
    for (id, d) in ordered {
        if let Cell::Duty { n } = d.cell {
            for (line, tour, k) in pieces(n) {
                if !out.given.iter().any(|g| g.0 == line && g.1 == tour && g.2 == k) {
                    out.given.push((line, tour, k, *id));
                }
            }
        }
    }
    for (_, d) in &mine {
        // (away by an exception: his row's duty open, saying why)
        let away = match d.note.as_deref() {
            Some("sick") => Some(Problem::Sick),
            Some("holiday") => Some(Problem::Holiday),
            _ => None,
        };
        if let (Some(p), Cell::Duty { n }, false) = (away, d.planned, d.cell.works()) {
            for (line, tour, k) in pieces(n) {
                out.absent.push((line, tour, k, p));
            }
        }
    }
    Some(out)
}

// --- the print -----------------------------------------------------------------------------------

/// A rota as a PDF file (A4 across): its rows - each day's duty and times, the hours, the driver
/// of the week shown - and the cycle's figures. `words` the page's words: the title, the days'
/// names (Sunday first), "Row", "Hours", the driver's column, "Day off", "Page".
pub fn pdf(c: &Company, k: usize, week: &Week, date: &str, words: &PrintWords) -> Vec<u8> {
    use super::rosterpdf::Pdf;
    let Some(t) = c.planning.rotas.list.get(k) else { return Pdf::new(842.0, 595.0).finish() };
    let checked = check(c, k, week);
    let (w, h) = (842.0f32, 595.0f32);
    let mut pdf = Pdf::new(w, h);
    let (left, top) = (30.0f32, 40.0f32);
    let cols: [f32; 10] = [left, left + 40.0, left + 125.0, left + 210.0, left + 295.0, left + 380.0, left + 465.0, left + 550.0, left + 635.0, left + 690.0];
    let ink = [30u8, 40, 36];
    let grey = [110u8, 120, 115];
    let head = |pdf: &mut Pdf, page: usize| {
        pdf.text(left, top, 16.0, true, &format!("{} · {}", words.title, t.name), ink, 0);
        pdf.text(left, top + 16.0, 9.0, false, &words.info, grey, 0);
        pdf.text(w - left, top, 9.0, false, &format!("{} {}", words.page, page), grey, 2);
        let y = top + 34.0;
        pdf.rect(left, y - 11.0, w - 2.0 * left, 16.0, [232, 238, 234]);
        pdf.text(cols[0] + 4.0, y, 8.5, true, &words.row, ink, 0);
        for d in 0..DAYS {
            pdf.text(cols[1 + d] + 4.0, y, 8.5, true, &words.days[d], ink, 0);
        }
        pdf.text(cols[8] + 4.0, y, 8.5, true, &words.hours, ink, 0);
        pdf.text(cols[9] + 4.0, y, 8.5, true, &words.driver, ink, 0);
    };
    let row_h = 26.0f32;
    let mut page = 1;
    head(&mut pdf, page);
    let mut y = top + 50.0;
    for (r, row) in t.rows.iter().enumerate() {
        if y + row_h > h - 40.0 {
            pdf.new_page();
            page += 1;
            head(&mut pdf, page);
            y = top + 50.0;
        }
        if r % 2 == 1 {
            pdf.rect(left, y - 4.0, w - 2.0 * left, row_h, [246, 248, 247]);
        }
        pdf.text(cols[0] + 4.0, y + 10.0, 10.0, true, &(r + 1).to_string(), ink, 0);
        for d in 0..DAYS {
            let x = cols[1 + d] + 4.0;
            match row.days[d] {
                Cell::Off => pdf.text(x, y + 10.0, 9.0, false, &words.off, grey, 0),
                Cell::Duty { n } => {
                    pdf.text(x, y + 6.0, 9.5, true, &n.to_string(), ink, 0);
                    let tm = times(week, d, row.days[d]).map(|(a, b, _)| format!("{}-{}", rf::clock(a), rf::clock(b))).unwrap_or_else(|| "?".into());
                    pdf.text(x, y + 17.0, 7.5, false, &tm, grey, 0);
                }
                Cell::Frame { from, to } => {
                    pdf.text(x, y + 6.0, 9.5, true, "R", [31, 122, 69], 0);
                    pdf.text(x, y + 17.0, 7.5, false, &format!("{}-{}", rf::clock(from), rf::clock(to)), grey, 0);
                }
            }
        }
        let hours = checked.minutes.get(r).copied().unwrap_or(0) as f64 / 60.0;
        pdf.text(cols[8] + 4.0, y + 10.0, 9.5, false, &h1(hours), ink, 0);
        let who = driver_on(t, r, date).and_then(|id| c.employee(id)).map(|e| e.name.clone()).unwrap_or_else(|| "-".into());
        pdf.text(cols[9] + 4.0, y + 10.0, 9.5, false, &who, ink, 0);
        pdf.line((left, y + row_h - 4.0), (w - left, y + row_h - 4.0), 0.3, [210, 216, 212]);
        y += row_h;
    }
    pdf.text(left, y + 14.0, 9.0, false, &words.foot, grey, 0);
    pdf.finish()
}

/// The words of a rota's print, in the page's language.
#[derive(Clone, Debug, Default)]
pub struct PrintWords {
    pub title: String,
    pub info: String,
    pub days: [String; DAYS],
    pub row: String,
    pub hours: String,
    pub driver: String,
    pub off: String,
    pub page: String,
    pub foot: String,
}

/// Every English text of the rota's core (for the translations' check).
pub fn texts() -> Vec<&'static str> {
    let mut v = vec![M_REST, M_IN_A_ROW, M_TWO_OFF, M_OVER, M_UNDER, M_CONTRACT, M_AVERAGE, M_WEEKEND, M_MISSING, M_TWICE, NO_ROTA, NO_ROW, NOT_THAT_DAY, TAKEN, NOT_A_DRIVER, NO_HOURLY, RESERVES_ONLY, NO_RESERVES, NOTHING_TO_PLACE];
    // (after the player's comments: the weekends, the split duties)
    v.extend([M_HALF_WEEKEND, M_TWO_FREE, M_TWO_WORKED, M_ODD_WEEKS, M_SPLIT_HERE, M_NOT_SPLIT, M_SPLIT_WEEKEND, WEEKEND_OFF, ONLY_SPLIT, SPLIT_ELSEWHERE, NO_SPLIT]);
    v.extend(RotaKind::LABELS);
    v.extend(NOTES.iter().map(|n| n.1));
    v
}

#[cfg(test)]
#[path = "pro_turnus_tests.rs"]
mod tests;
