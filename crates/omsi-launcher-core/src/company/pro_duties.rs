//! RosterFlow Pro, step 2 (NexiconDK, 4 November 2026): the duties as the "Vagter" tab builds
//! them, HASTUS's way. The day's blocks are cut into pieces at the relief points (`studio::cut`);
//! a piece without a duty waits in the pool (`Sheet::pool`) until the planner puts it in a duty
//! - a duty of its own, or one with pieces of other blocks and lines (a line change). A duty has
//! a type (`Kind`: whole, split, short), a number that stays its own while the day is edited
//! (`DutyNumber`: 1001 on for a weekday, 2001 on for a Saturday, 3001 on for a Sunday), and the
//! marks of the rules it breaks (`marks`): what the working-time rules say of it, a split duty
//! spread over 12 h 30, and too little time to change from one piece to the next (`Change`: by
//! default 5 minutes at the same stop, 10 between two stops).
//!
//! It is RosterFlow's own day underneath: the pieces are the studio's, a duty of several pieces
//! a joined duty (`studio::JoinedDuty`), the pool and the numbers kept in RosterFlow's sheet, so
//! they are in the draft, Undo and Save like every other edit. A piece in the pool still runs:
//! the day's roster plans it as a short duty of its own (numbered from 1501, 2501, 3501), so
//! nothing of the timetable is dropped for a duty not built. A day nobody built anything on runs
//! the duties RosterFlow makes by itself (`studio::duties`), numbered in their rows - the
//! fall-back. "Make duties automatically" is RosterFlow's Optimise of the duties alone
//! (`auto`).

use super::autopilot;
use super::model::{Company, Planner};
use super::network::{self, PlannedTrip, TourOfDay};
use super::rosterflow::{self as rf, Board, Shaped};
use super::studio::{self as st, JoinedDuty, PieceRef, TripRef};
use crate::LineInfo;
use serde::{Deserialize, Serialize};

/// A gap between two pieces this long or longer makes a split duty (unpaid).
pub const SPLIT_GAP: i32 = 90;
/// A duty working less than this is a short one (paid at least `staff::MIN_PAID`).
pub const SHORT_DUTY: i32 = 240;
/// The longest a split duty may be spread, sign-on to sign-off (12 h 30).
pub const SPLIT_SPREAD: i32 = 750;
/// Where the numbers of the pieces in the pool begin, above a day type's base.
pub const POOL_FROM: u32 = 500;

/// A piece of work no duty has yet: the Vagter tab's pool, on a weekday's plan (its slot).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Pooled {
    pub weekday: u8,
    pub piece: PieceRef,
}

/// A duty's number on a weekday's plan, kept by the duty's key (its first piece).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DutyNumber {
    pub weekday: u8,
    pub key: PieceRef,
    pub number: u32,
}

/// (NexiconDK, 7 November 2026, round K) The planner's own name of a duty on a weekday's plan
/// ("M131-1"), shown instead of its number - kept by a piece of the duty, so the name follows the
/// duty however its pieces and its number change (`name_of`, `set_name`).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct DutyName {
    pub weekday: u8,
    pub piece: PieceRef,
    pub name: String,
}

/// The longest a duty's own name may be.
pub const NAME_MAX: usize = 12;

/// The least time a driver needs to change from one piece of his duty to the next: at the same
/// stop, and between two stops (walking; a relief point's own travel time when it is longer).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct Change {
    pub same_stop: i32,
    pub other_stop: i32,
}

impl Default for Change {
    fn default() -> Self {
        Change { same_stop: 5, other_stop: 10 }
    }
}

/// (Round J, NexiconDK 7 November 2026) The planner's goals for "Make duties automatically": a
/// duty's work within `target_from`..`target_to` (7:24 to 8:30), never longer than `longest`
/// (10:00), and one line all the duty where it can (`same_line`). The shortest piece is
/// RosterFlow's own (`Prefs::min_piece`); split duties stay off at the weekend.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct Goals {
    pub target_from: i32,
    pub target_to: i32,
    pub longest: i32,
    pub same_line: bool,
    /// (Round O, NexiconDK 7 November 2026) A night duty's (night-bus work: `rf::night_trip`)
    /// target length (5:00 to 9:30), and the shortest piece of night work a duty is made of
    /// (3:00): a night-bus driver drives one long night duty, not scraps of one or two hours.
    pub night_from: i32,
    pub night_to: i32,
    pub night_least: i32,
}

impl Default for Goals {
    fn default() -> Self {
        Goals { target_from: 444, target_to: 510, longest: 600, same_line: true, night_from: 300, night_to: 570, night_least: 180 }
    }
}

impl Goals {
    /// The goals within reason: a target of at least an hour, ending before the longest.
    pub fn sane(mut self) -> Goals {
        self.longest = self.longest.clamp(240, 780);
        self.target_to = self.target_to.clamp(60, self.longest);
        self.target_from = self.target_from.clamp(0, self.target_to);
        self.night_to = self.night_to.clamp(60, self.longest);
        self.night_from = self.night_from.clamp(0, self.night_to);
        self.night_least = self.night_least.clamp(0, self.night_to);
        self
    }

    /// The target of a duty: a night duty's (`night`), else a day duty's.
    pub fn target(&self, night: bool) -> (i32, i32) {
        if night {
            (self.night_from, self.night_to)
        } else {
            (self.target_from, self.target_to)
        }
    }
}

/// The type of a duty.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// One stretch, or pieces with short breaks between ("Hel").
    Whole,
    /// Two stretches with a long unpaid gap between ("Delt").
    Split,
    /// Under four hours of work, paid four ("Kort").
    Short,
}

impl Kind {
    pub const LABELS: [&'static str; 3] = ["Whole", "Split duty", "Short"];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Whole => Self::LABELS[0],
            Kind::Split => Self::LABELS[1],
            Kind::Short => Self::LABELS[2],
        }
    }

    /// "whole", "split" or "short" (the page's class).
    pub fn key(self) -> &'static str {
        match self {
            Kind::Whole => "whole",
            Kind::Split => "split",
            Kind::Short => "short",
        }
    }
}

/// The gaps between a duty's pieces as worked, in order.
pub fn gaps(s: &Shaped) -> Vec<i32> {
    let mut spans = s.spans.clone();
    spans.sort_by_key(|x| x.from);
    spans.windows(2).map(|w| w[1].from - w[0].to).collect()
}

/// (Round P) The parts of duty `s` a split divides it into: its pieces between gaps of
/// `SPLIT_GAP` or more, each as (from, to, minutes worked - its unpaid breaks not). A duty not
/// split is one part.
pub fn parts(s: &Shaped) -> Vec<(i32, i32, i32)> {
    let mut spans = s.spans.clone();
    spans.sort_by_key(|x| x.from);
    let mut out: Vec<(i32, i32)> = Vec::new();
    for sp in &spans {
        match out.last_mut() {
            Some(l) if sp.from - l.1 < SPLIT_GAP => l.1 = l.1.max(sp.to),
            _ => out.push((sp.from, sp.to)),
        }
    }
    out.into_iter().map(|(a, z)| (a, z, (z - a) - s.splits.iter().filter(|g| g.0 >= a && g.1 <= z).map(|g| g.1 - g.0).sum::<i32>())).collect()
}

/// (Round P) The meal break inside part `part` of duty `s` (`parts`): a break of its own long
/// enough for the meal rule - never the gap between two parts.
pub fn part_meal(s: &Shaped, part: (i32, i32)) -> Option<rf::DutyBreak> {
    s.plan.breaks.iter().find(|b| b.meal && b.kind != rf::BreakKind::Split && b.from >= part.0 && b.to <= part.1).copied()
}

/// The type of duty `s`: split when two of its pieces have `SPLIT_GAP` or more between them,
/// else short when it works less than `SHORT_DUTY`, else whole.
pub fn kind_of(s: &Shaped) -> Kind {
    if gaps(s).iter().any(|g| *g >= SPLIT_GAP) {
        Kind::Split
    } else if s.work < SHORT_DUTY {
        Kind::Short
    } else {
        Kind::Whole
    }
}

// --- the numbers -------------------------------------------------------------------------------

/// Whether the company plans with RosterFlow Pro.
pub fn on(c: &Company) -> bool {
    autopilot::planner(c) == Planner::Pro
}

/// The day type of a plan's slot: 0 Monday to Friday, 1 Saturday, 2 Sunday (a plan of single
/// dates by its first date's weekday).
pub fn day_type(c: &Company, slot: u8) -> u8 {
    let wd = if slot < 7 { slot } else { super::plandays::dates_of(c, slot).first().map(|d| super::plan::weekday_of(d)).unwrap_or(0) };
    match wd {
        5 => 1,
        6 => 2,
        _ => 0,
    }
}

/// The number before a day type's first duty: 1000, 2000 or 3000.
pub fn base(c: &Company, slot: u8) -> u32 {
    1000 * (day_type(c, slot) as u32 + 1)
}

fn sheet(c: &Company) -> &rf::Sheet {
    &c.planning.flow.sheet
}

/// Whether piece `p` is in the pool on weekday `wd`.
pub fn pooled(c: &Company, wd: u8, p: &PieceRef) -> bool {
    sheet(c).pool.iter().any(|x| x.weekday == wd && x.piece.is(p))
}

/// The piece of a duty's span, as the studio knows it.
fn span_piece(tours: &[TourOfDay], sp: &rf::Span) -> Option<PieceRef> {
    st::piece_ref(tours, sp.tour, sp.duty)
}

/// Whether a duty of the board is a piece in the pool (a duty of one piece, that piece pooled).
pub fn is_pool_duty(c: &Company, wd: u8, tours: &[TourOfDay], s: &Shaped) -> bool {
    s.spare.is_none() && s.spans.len() == 1 && span_piece(tours, &s.spans[0]).is_some_and(|p| pooled(c, wd, &p))
}

/// Number the day's duties (`shaped` in their rows; with RosterFlow Pro only): each its kept
/// number (`DutyNumber`), the others the lowest numbers free from the day type's base on, in
/// their rows; the pieces in the pool from `POOL_FROM` above the base. A day nothing was kept
/// for is numbered in its rows, as RosterFlow numbers it.
pub fn number_rows(c: &Company, wd: u8, tours: &[TourOfDay], shaped: &mut [Shaped]) {
    if !on(c) {
        return;
    }
    let base = base(c, wd);
    // (a number of another day type's range - a copy of another day's - is not kept)
    let kept: Vec<&DutyNumber> = sheet(c).numbers.iter().filter(|n| n.weekday == wd && n.number > base && n.number <= base + POOL_FROM).collect();
    let pool: Vec<bool> = shaped.iter().map(|s| is_pool_duty(c, wd, tours, s)).collect();
    let mut taken: Vec<u32> = Vec::new();
    let mut out: Vec<Option<u32>> = vec![None; shaped.len()];
    for (row, s) in shaped.iter().enumerate() {
        if s.spare.is_some() || pool[row] {
            continue;
        }
        let Some(key) = &s.key else { continue };
        if let Some(n) = kept.iter().find(|n| n.key.is(key)).map(|n| n.number).filter(|n| !taken.contains(n)) {
            out[row] = Some(n);
            taken.push(n);
        }
    }
    let mut next = base + 1;
    let mut next_pool = base + POOL_FROM + 1;
    for row in 0..shaped.len() {
        if shaped[row].spare.is_some() || out[row].is_some() {
            continue;
        }
        if pool[row] {
            out[row] = Some(next_pool);
            next_pool += 1;
            continue;
        }
        while taken.contains(&next) {
            next += 1;
        }
        out[row] = Some(next);
        taken.push(next);
    }
    for (s, n) in shaped.iter_mut().zip(out) {
        if s.spare.is_none() {
            s.number = n;
        }
    }
    // (round K: the planner's own names)
    for (row, s) in shaped.iter_mut().enumerate() {
        if s.spare.is_none() && !pool[row] {
            s.name = name_of(c, wd, tours, s);
        }
    }
}

// --- the duties' own names (round K) -------------------------------------------------------------

pub const NAME_LONG: &str = "A duty's name is at most 12 characters.";
pub const NAME_TAKEN: &str = "Another duty of the day has this name already.";

/// The planner's own name of duty `s` of weekday `wd` (`tours` its blocks): the name kept by one
/// of its pieces.
pub fn name_of(c: &Company, wd: u8, tours: &[TourOfDay], s: &Shaped) -> Option<String> {
    let pieces = pieces_of(tours, s);
    sheet(c).names.iter().find(|n| n.weekday == wd && pieces.iter().any(|p| p.is(&n.piece))).map(|n| n.name.clone())
}

/// The pieces of the duty piece `p` (block, piece) is in, as they are now: its joined duty's, or
/// itself.
fn duty_pieces(c: &Company, wd: u8, tours: &[TourOfDay], p: (usize, usize)) -> Result<Vec<PieceRef>, &'static str> {
    let me = piece(tours, p)?;
    Ok(c.planning.joined.iter().find(|j| j.weekday == wd && j.pieces.iter().any(|x| x.is(&me))).map(|j| j.pieces.clone()).unwrap_or_else(|| vec![me]))
}

/// Give the duty piece `p` is in the name `name` (trimmed; empty: its number again). A name is at
/// most `NAME_MAX` characters and no other duty of the weekday has it (whatever its case).
pub fn set_name(c: &mut Company, wd: u8, tours: &[TourOfDay], p: (usize, usize), name: &str) -> Result<(), &'static str> {
    let name = name.trim();
    if name.chars().count() > NAME_MAX {
        return Err(NAME_LONG);
    }
    let pieces = duty_pieces(c, wd, tours, p)?;
    let mine = |n: &DutyName| n.weekday == wd && pieces.iter().any(|x| x.is(&n.piece));
    if !name.is_empty() && sheet(c).names.iter().any(|n| !mine(n) && n.weekday == wd && st::piece_at(tours, &n.piece).is_some() && n.name.trim().eq_ignore_ascii_case(name)) {
        return Err(NAME_TAKEN);
    }
    c.planning.flow.sheet.names.retain(|n| !mine(n));
    if !name.is_empty() {
        // (kept by the duty's first piece in the day)
        let first = pieces.iter().filter_map(|x| Some((piece_time(tours, st::piece_at(tours, x)?)?.0, x))).min_by_key(|x| x.0).map(|x| x.1.clone()).unwrap_or_else(|| pieces[0].clone());
        c.planning.flow.sheet.names.push(DutyName { weekday: wd, piece: first, name: name.to_string() });
    }
    Ok(())
}

/// The name a duty of `date` is shown by, by its number (`number` its kept number, as the rota,
/// the day's operations and the messages know it): its own name, else the number.
pub fn label_of(c: &Company, lines: &[LineInfo], date: &str, number: u32) -> String {
    if on(c) && !sheet(c).names.is_empty() {
        let b = rf::board(c, lines, date);
        if let Some((row, s)) = b.shaped.iter().enumerate().find(|(row, s)| s.spare.is_none() && number_of(s, *row) == number) {
            return s.label(row);
        }
    }
    number.to_string()
}

/// The name duty `n` of `date` is told by in a message (no timetable at hand): the name kept by
/// the piece its number is kept by, else the number.
pub fn shown_number(c: &Company, date: &str, n: u32) -> String {
    let wd = super::plandays::slot_of(c, date);
    let sh = sheet(c);
    sh.numbers.iter().find(|x| x.weekday == wd && x.number == n).and_then(|x| sh.names.iter().find(|m| m.weekday == wd && m.piece.is(&x.key))).map(|m| m.name.clone()).unwrap_or_else(|| n.to_string())
}

// --- the other weekdays (round M) -------------------------------------------------------------------

/// Whether weekday slot `wd` has a plan of its own: blocks laid out, duties joined, pieces in the
/// Vagter tab's pool, duties numbered or named. None: it runs the timetable's tours and the duties
/// RosterFlow makes by itself (the game's plan).
pub fn planned(c: &Company, wd: u8) -> bool {
    let sh = sheet(c);
    c.planning.layouts.iter().any(|l| l.weekday == wd)
        || c.planning.joined.iter().any(|j| j.weekday == wd)
        || sh.pool.iter().any(|x| x.weekday == wd)
        || sh.numbers.iter().any(|x| x.weekday == wd)
        || sh.names.iter().any(|x| x.weekday == wd)
}

/// The other weekdays of weekday slot `wd`'s day type that have no plan of their own while `wd`
/// has one (Monday to Friday only: a Saturday and a Sunday are a day type each) - the days
/// "Use for … too" offers.
pub fn unplanned_peers(c: &Company, wd: u8) -> Vec<u8> {
    if wd >= 5 || !planned(c, wd) {
        return Vec::new();
    }
    (0..5u8).filter(|&d| d != wd && !planned(c, d)).collect()
}

/// How a weekday got the plan of another (`use_for`).
#[derive(Clone, Debug, PartialEq)]
pub enum Used {
    /// Its timetable is the same: the plan copied as it is (`plandays::copy`).
    Copied,
    /// Its timetable differs (Friday's night tours, say): its duties made automatically on its
    /// own timetable (`auto`), as many as it has.
    Made(usize),
}

/// "Use for Tuesday - Friday too" (round M): the plan of weekday slot `from` (`src` a date it runs
/// and its timetable) onto each of `days` (a weekday slot, a date of it and that date's
/// timetable). A day of the same timetable - every trip matched, none left over - gets the plan
/// copied as it is, its duties' names and pool with it; another gets its duties made
/// automatically on its own timetable. Each numbered in the order its duties sign on.
pub fn use_for(c: &mut Company, from: u8, src: (&str, &[LineInfo]), days: &[(u8, String, Vec<LineInfo>)]) -> Vec<(u8, Used)> {
    use super::plandays::{self as pdays, Day, What};
    let mut out = Vec::new();
    for (wd, date, lines) in days {
        let mut probe = c.clone();
        let made = pdays::copy(&mut probe, from, Day { date: src.0, lines: src.1 }, *wd, Day { date, lines }, What::ALL);
        let exact = !made.same_plan && made.missing.is_empty() && made.gone == 0 && made.matched == made.total;
        if exact {
            // (the Vagter tab's own of the duties: their names, the pieces in the pool)
            let sh = &mut probe.planning.flow.sheet;
            let names: Vec<DutyName> = sh.names.iter().filter(|n| n.weekday == from).map(|n| DutyName { weekday: *wd, ..n.clone() }).collect();
            sh.names.retain(|n| n.weekday != *wd);
            sh.names.extend(names);
            let pool: Vec<Pooled> = sh.pool.iter().filter(|x| x.weekday == from).map(|x| Pooled { weekday: *wd, piece: x.piece.clone() }).collect();
            sh.pool.retain(|x| x.weekday != *wd);
            sh.pool.extend(pool);
            *c = probe;
            number_by_sign_on(c, lines, date);
            out.push((*wd, Used::Copied));
        } else {
            let n = auto(c, lines, date);
            number_by_sign_on(c, lines, date);
            out.push((*wd, Used::Made(n)));
        }
    }
    out
}

// --- a duty built from departures (round K: the "Afgange" tab) ----------------------------------

pub const NO_DEPARTURE: &str = "Choose the departures of the duty first.";
pub const BUILD_OVERLAP: &str = "Two of the departures chosen are at the same time: one driver cannot drive both.";

/// The longest a bus stands between a run without passengers and the duty's trips for the run to
/// go with them (`run_of`), in minutes.
pub const RUN_GAP: i32 = 30;

/// The trips of block `i` a duty built of departures `js` (trips of it) takes: from the first to
/// the last chosen, and the runs without passengers just before and after them that the bus goes
/// on with (at most `RUN_GAP` between) - its pull-out and pull-in, its run empty to the first trip.
pub fn run_of(t: &TourOfDay, js: &[usize]) -> Option<(usize, usize)> {
    let (mut a, mut b) = (*js.iter().min()?, *js.iter().max()?);
    if b >= t.trips.len() {
        return None;
    }
    // (only the runs the bus goes on with: one after a long stand - a pull-in, then hours at the
    // depot - is not the duty's)
    while a > 0 && !t.trips[a - 1].counts() && t.trips[a].dep - t.trips[a - 1].arr <= RUN_GAP {
        a -= 1;
    }
    while b + 1 < t.trips.len() && !t.trips[b + 1].counts() && t.trips[b + 1].dep - t.trips[b].arr <= RUN_GAP {
        b += 1;
    }
    Some((a, b))
}

/// The runs of a duty built of the departures `trips` (block, trip) of `tours`: each block's
/// trips from `run_of`, in order of time; refused when two of them are at the same time.
pub fn runs_of(tours: &[TourOfDay], trips: &[(usize, usize)]) -> Result<Vec<(usize, usize, usize)>, &'static str> {
    let mut blocks: Vec<usize> = trips.iter().map(|x| x.0).collect();
    blocks.sort_unstable();
    blocks.dedup();
    if blocks.is_empty() {
        return Err(NO_DEPARTURE);
    }
    let mut runs: Vec<(usize, usize, usize)> = Vec::new();
    for i in blocks {
        let t = tours.get(i).ok_or(st::NOT_FOUND)?;
        let js: Vec<usize> = trips.iter().filter(|x| x.0 == i).map(|x| x.1).collect();
        let (a, b) = run_of(t, &js).ok_or(st::NOT_FOUND)?;
        runs.push((i, a, b));
    }
    let time = |r: &(usize, usize, usize)| -> (i32, i32) {
        let t = &tours[r.0];
        (t.trips[r.1].dep, t.trips[r.1..=r.2].iter().map(|x| x.arr).max().unwrap_or(0))
    };
    runs.sort_by_key(|r| time(r).0);
    for w in runs.windows(2) {
        if time(&w[1]).0 < time(&w[0]).1 {
            return Err(BUILD_OVERLAP);
        }
    }
    Ok(runs)
}

/// Build a duty of the departures `trips` (block, trip) of weekday `wd` (`tours` its blocks as they
/// are): each block cut before and after its trips (`run_of`) at whatever trip - a relief point or
/// not -, those pieces out of the duties and the pool they were in (a duty they were part of
/// keeps the rest; what a cut leaves of a piece after them goes to the pool), and the pieces one
/// duty. Named `name` when it is not empty. Returns the duty's pieces.
pub fn build_duty(c: &mut Company, wd: u8, tours: &[TourOfDay], trips: &[(usize, usize)], name: &str) -> Result<Vec<PieceRef>, &'static str> {
    let runs = runs_of(tours, trips)?;
    if name.trim().chars().count() > NAME_MAX {
        return Err(NAME_LONG);
    }
    let mut now = tours.to_vec();
    let mut mine: Vec<PieceRef> = Vec::new();
    let mut left: Vec<PieceRef> = Vec::new();
    for &(i, a, b) in &runs {
        if rf::block_locked(c, wd, &now[i].line, &now[i].tour) {
            return Err(st::LOCKED);
        }
        let was = st::starts_of(&now[i]);
        let mut at: Vec<usize> = was.iter().copied().filter(|&x| x <= a || x > b + 1).collect();
        if a > 0 {
            at.push(a);
        }
        if b + 1 < now[i].trips.len() {
            at.push(b + 1);
        }
        at.sort_unstable();
        at.dedup();
        if at != was {
            st::set_cuts_free(c, wd, &now, i, &at)?;
            now[i].cuts = Some(at.clone());
        }
        let pieces = network::duties_of(&now[i]);
        let k = pieces.iter().position(|r| r.start == a).ok_or(st::NOT_FOUND)?;
        mine.push(st::piece_ref(&now, i, k).ok_or(st::NOT_FOUND)?);
        // (the piece a new cut after the run begins: to the pool)
        if b + 1 < now[i].trips.len() && !was.contains(&(b + 1)) {
            if let Some(k2) = pieces.iter().position(|r| r.start == b + 1) {
                left.extend(st::piece_ref(&now, i, k2));
            }
        }
    }
    // (the pieces out of the duties and the pool they were in)
    for j in c.planning.joined.iter_mut().filter(|j| j.weekday == wd) {
        j.pieces.retain(|p| !mine.iter().any(|m| m.is(p)));
    }
    c.planning.joined.retain(|j| j.weekday != wd || j.pieces.len() > 1);
    for p in &mine {
        take_from_pool(c, wd, p);
    }
    for p in left {
        // (a piece left over that is in a duty still stays in it)
        if !c.planning.joined.iter().any(|j| j.weekday == wd && j.pieces.iter().any(|x| x.is(&p))) {
            add_pool(c, wd, p);
        }
    }
    if mine.len() > 1 {
        c.planning.joined.push(JoinedDuty { weekday: wd, pieces: mine.clone() });
    }
    // (its name: kept by its first piece; a name another duty has is refused)
    let name = name.trim();
    if !name.is_empty() {
        let first = st::piece_at(&now, &mine[0]).ok_or(st::NOT_FOUND)?;
        set_name(c, wd, &now, first, name)?;
    }
    Ok(mine)
}


/// The number duty `row` of the board shows.
pub fn number_of(s: &Shaped, row: usize) -> u32 {
    s.number.unwrap_or(1001 + row as u32)
}

/// The pieces of a duty of the board.
fn pieces_of(tours: &[TourOfDay], s: &Shaped) -> Vec<PieceRef> {
    s.spans.iter().filter_map(|sp| span_piece(tours, sp)).collect()
}

/// After an edit of weekday `wd` (the board `before` it, `after` it, of the company as it is
/// now): every duty keeps the number it had - the duty with the same first piece first, then the
/// duty its first piece was in, then a duty it shares a piece with - and a new one takes the
/// lowest number free. The pieces in the pool have none kept. Written to the sheet.
pub fn renumber(c: &mut Company, wd: u8, before: &Board, after: &Board) {
    if !on(c) {
        return;
    }
    let base = base(c, wd);
    let was: Vec<(Option<PieceRef>, Vec<PieceRef>, u32)> = before
        .shaped
        .iter()
        .enumerate()
        .filter(|(_, s)| s.spare.is_none() && !is_pool_duty(c, wd, &before.tours, s))
        .map(|(row, s)| (s.key.clone(), pieces_of(&before.tours, s), number_of(s, row)))
        .filter(|w| w.2 > base && w.2 <= base + POOL_FROM)
        .collect();
    let now: Vec<(usize, PieceRef, Vec<PieceRef>)> = after
        .shaped
        .iter()
        .enumerate()
        .filter(|(_, s)| s.spare.is_none() && !is_pool_duty(c, wd, &after.tours, s))
        .filter_map(|(row, s)| Some((row, s.key.clone()?, pieces_of(&after.tours, s))))
        .collect();
    let mut taken: Vec<u32> = Vec::new();
    let mut got: Vec<Option<u32>> = vec![None; now.len()];
    // (by the same key; by the duty the first piece was in; by any piece shared)
    let tests: [&dyn Fn(&(Option<PieceRef>, Vec<PieceRef>, u32), &PieceRef, &[PieceRef]) -> bool; 3] = [
        &|w, key, _| w.0.as_ref().is_some_and(|k| k.is(key)),
        &|w, key, _| w.1.iter().any(|p| p.is(key)),
        &|w, _, ps| w.1.iter().any(|p| ps.iter().any(|q| q.is(p))),
    ];
    for test in tests {
        for (k, (_, key, ps)) in now.iter().enumerate() {
            if got[k].is_some() {
                continue;
            }
            if let Some(w) = was.iter().find(|w| !taken.contains(&w.2) && test(w, key, ps)) {
                got[k] = Some(w.2);
                taken.push(w.2);
            }
        }
    }
    let mut next = base + 1;
    for g in got.iter_mut().filter(|g| g.is_none()) {
        while taken.contains(&next) {
            next += 1;
        }
        *g = Some(next);
        taken.push(next);
    }
    let sh = &mut c.planning.flow.sheet;
    sh.numbers.retain(|n| n.weekday != wd);
    for ((_, key, _), n) in now.iter().zip(got) {
        sh.numbers.push(DutyNumber { weekday: wd, key: key.clone(), number: n.unwrap_or(0) });
    }
    sh.numbers.sort_by_key(|n| (n.weekday, n.number));
}

// --- the pool ----------------------------------------------------------------------------------

pub const IN_POOL: &str = "This piece has no duty already.";
pub const NOT_IN_POOL: &str = "This piece is in a duty already.";
pub const OVERLAP: &str = "The driver would be on two buses at once: the pieces overlap.";
pub const NO_PIECE: &str = "Choose a piece first.";

/// The piece (block, piece) of `tours` as the studio knows it.
fn piece(tours: &[TourOfDay], p: (usize, usize)) -> Result<PieceRef, &'static str> {
    st::piece_ref(tours, p.0, p.1).ok_or(st::NOT_FOUND)
}

fn guard(c: &Company, wd: u8, tours: &[TourOfDay], p: (usize, usize)) -> Result<(), &'static str> {
    let t = tours.get(p.0).ok_or(st::NOT_FOUND)?;
    if rf::block_locked(c, wd, &t.line, &t.tour) {
        return Err(st::LOCKED);
    }
    if rf::piece_locked(c, wd, tours, p.0, p.1) {
        return Err(st::DUTY_LOCKED);
    }
    Ok(())
}

fn add_pool(c: &mut Company, wd: u8, p: PieceRef) {
    if !pooled(c, wd, &p) {
        c.planning.flow.sheet.pool.push(Pooled { weekday: wd, piece: p });
    }
}

fn take_from_pool(c: &mut Company, wd: u8, p: &PieceRef) {
    c.planning.flow.sheet.pool.retain(|x| !(x.weekday == wd && x.piece.is(p)));
}

/// Take piece `p` (block, piece) out of its duty into the pool (the other pieces of the duty stay
/// together).
pub fn to_pool(c: &mut Company, wd: u8, tours: &[TourOfDay], p: (usize, usize)) -> Result<(), &'static str> {
    guard(c, wd, tours, p)?;
    let me = piece(tours, p)?;
    if pooled(c, wd, &me) {
        return Err(IN_POOL);
    }
    // (in a duty of several pieces: out of it; a duty of its own: it is the piece)
    let _ = st::detach(c, wd, tours, p);
    add_pool(c, wd, me);
    Ok(())
}

/// Every piece of the duty piece `p` is in, into the pool.
pub fn dissolve(c: &mut Company, wd: u8, tours: &[TourOfDay], p: (usize, usize)) -> Result<(), &'static str> {
    guard(c, wd, tours, p)?;
    let me = piece(tours, p)?;
    let group = c.planning.joined.iter().position(|j| j.weekday == wd && j.pieces.iter().any(|x| x.is(&me)));
    let all = match group {
        Some(g) => c.planning.joined.remove(g).pieces,
        None => vec![me],
    };
    for x in all {
        add_pool(c, wd, x);
    }
    Ok(())
}

/// Piece `p` out of the pool: a duty of its own.
pub fn new_duty(c: &mut Company, wd: u8, tours: &[TourOfDay], p: (usize, usize)) -> Result<(), &'static str> {
    let me = piece(tours, p)?;
    if !pooled(c, wd, &me) {
        return Err(NOT_IN_POOL);
    }
    take_from_pool(c, wd, &me);
    Ok(())
}

/// The minutes piece (block, piece) is worked: its first trip's departure to its last arrival.
fn piece_time(tours: &[TourOfDay], p: (usize, usize)) -> Option<(i32, i32)> {
    let t = tours.get(p.0)?;
    let r = network::duties_of(t).get(p.1)?.clone();
    let trips = t.trips.get(r)?;
    Some((trips.first()?.dep, trips.iter().map(|x| x.arr).max()?))
}

/// Put piece `p` (block, piece; from the pool or another duty) into the duty piece `into` is in.
/// Next to each other on one bus the cut between them goes; else the duty takes it as a piece of
/// its own - another bus, another line. Only pieces at the same time are refused: what else the
/// rules say of the duty is marked on it, not refused.
pub fn add_to_duty(c: &mut Company, wd: u8, tours: &[TourOfDay], p: (usize, usize), into: (usize, usize)) -> Result<(), &'static str> {
    if p == into {
        return Err(st::SAME_DUTY);
    }
    guard(c, wd, tours, p)?;
    guard(c, wd, tours, into)?;
    let (me, target) = (piece(tours, p)?, piece(tours, into)?);
    let group_of = |c: &Company, x: &PieceRef| c.planning.joined.iter().position(|j| j.weekday == wd && j.pieces.iter().any(|y| y.is(x)));
    if group_of(c, &me).is_some() && group_of(c, &me) == group_of(c, &target) {
        return Err(st::SAME_DUTY);
    }
    let mut probe = c.clone();
    take_from_pool(&mut probe, wd, &me);
    let _ = st::detach(&mut probe, wd, tours, p);
    // (the duty the target is in, as it is now)
    let group = probe.planning.joined.iter().position(|j| j.weekday == wd && j.pieces.iter().any(|x| x.is(&target)));
    let mut pieces = group.map(|g| probe.planning.joined[g].pieces.clone()).unwrap_or_else(|| vec![target.clone()]);
    if pieces.iter().any(|x| x.is(&me)) {
        return Err(st::SAME_DUTY);
    }
    let (from, to) = piece_time(tours, p).ok_or(st::NOT_FOUND)?;
    for x in &pieces {
        let Some(at) = st::piece_at(tours, x) else { continue };
        let Some((a, b)) = piece_time(tours, at) else { continue };
        // (next to each other on one bus: one piece again)
        if at.0 == p.0 && at.1.abs_diff(p.1) == 1 {
            continue;
        }
        if from < b && a < to {
            return Err(OVERLAP);
        }
    }
    // next to a piece of the duty on the same bus: the cut between them goes
    let next_to = pieces.iter().filter_map(|x| st::piece_at(tours, x)).find(|at| at.0 == p.0 && at.1.abs_diff(p.1) == 1);
    if let Some(at) = next_to {
        if let Some(g) = group {
            probe.planning.joined.remove(g);
        }
        let later = network::duties_of(&tours[p.0]).get(at.1.max(p.1)).ok_or(st::NOT_FOUND)?.start;
        // (the joined duty again, its piece the one of the two that begins first)
        let first = if at.1 < p.1 { pieces.iter().find(|x| x.is(&piece(tours, at).unwrap_or_default())).cloned() } else { Some(me.clone()) };
        pieces.retain(|x| !st::piece_at(tours, x).is_some_and(|y| y == at));
        st::uncut(&mut probe, wd, tours, p.0, later)?;
        if let Some(f) = first {
            pieces.push(f);
        }
        if pieces.len() > 1 {
            probe.planning.joined.push(JoinedDuty { weekday: wd, pieces });
        }
        *c = probe;
        return Ok(());
    }
    if let Some(g) = group {
        probe.planning.joined.remove(g);
    }
    pieces.push(me);
    probe.planning.joined.push(JoinedDuty { weekday: wd, pieces });
    *c = probe;
    Ok(())
}

/// Cut block `i` before its trip `at` (`studio::cut`): the piece that begins there goes to the
/// pool.
pub fn cut_to_pool(c: &mut Company, wd: u8, tours: &[TourOfDay], i: usize, at: usize) -> Result<(), &'static str> {
    let t = tours.get(i).ok_or(st::NOT_FOUND)?;
    let first = t.trips.get(at).ok_or(st::NOT_FOUND)?;
    let p = PieceRef { line: t.line.clone(), tour: t.tour.clone(), first: TripRef::of(first) };
    st::cut(c, wd, tours, i, at)?;
    add_pool(c, wd, p);
    Ok(())
}

/// The pool of weekday `wd` emptied (every piece a duty of its own).
pub fn clear_pool(c: &mut Company, wd: u8) {
    c.planning.flow.sheet.pool.retain(|x| x.weekday != wd);
}

/// The pool of weekday `wd` as the day is now (`tours`): what is no longer a piece of a block,
/// or is in a duty of several pieces, is not in it.
pub fn tidy_pool(c: &mut Company, wd: u8, tours: &[TourOfDay]) {
    let joined: Vec<PieceRef> = c.planning.joined.iter().filter(|j| j.weekday == wd).flat_map(|j| j.pieces.iter().cloned()).collect();
    c.planning.flow.sheet.pool.retain(|x| x.weekday != wd || (st::piece_at(tours, &x.piece).is_some() && !joined.iter().any(|j| j.is(&x.piece))));
    // (round K: a name whose piece is gone goes with it)
    c.planning.flow.sheet.names.retain(|n| n.weekday != wd || st::piece_at(tours, &n.piece).is_some());
}

/// "Make duties automatically": RosterFlow's Optimise of the duties alone (the blocks kept), the
/// pool emptied - on a Saturday or a Sunday without split duties. Returns how many duties the
/// day has.
///
/// (Round J) It aims at the planner's goals (`Goals`): it makes the day several ways - the cost
/// alone as before, and the goals weighed lightly to strongly, with and without one line all the
/// duty - and keeps the one that scores best (`score`): the fewest rules broken, no duty longer
/// than the longest, the duties near the target, the cheapest. The duties are numbered in the
/// order they sign on (`number_by_sign_on`).
pub fn auto(c: &mut Company, lines: &[LineInfo], date: &str) -> usize {
    let wd = super::plandays::slot_of(c, date);
    // (split duties do not run at the weekend: a Saturday's and a Sunday's made without, NexiconDK
    // 4 November 2026)
    let splits = day_type(c, wd) == 0;
    let goals = c.planning.flow.prefs.goals.sane();
    let mut aims: Vec<Option<rf::Aim>> = vec![None];
    for (over, under) in [(0.5, 0.1), (1.5, 0.25), (4.0, 0.5), (10.0, 1.0)] {
        aims.push(Some(rf::Aim { goals, over, under, line_change: 20_00.0 }));
        if goals.same_line {
            aims.push(Some(rf::Aim { goals: Goals { same_line: false, ..goals }, over, under, line_change: 0.0 }));
        }
    }
    let mut best: Option<(f64, Company)> = None;
    for aim in aims {
        let (layouts, joined) = rf::optimise_aimed(c, lines, date, false, true, splits, aim);
        let mut view = c.clone();
        rf::apply(&mut view, wd, layouts, joined);
        clear_pool(&mut view, wd);
        let b = rf::board(&view, lines, date);
        let x = score(&view, &b);
        if best.as_ref().is_none_or(|b| x < b.0) {
            best = Some((x, view));
        }
    }
    if let Some((_, view)) = best {
        *c = view;
    }
    number_by_sign_on(c, lines, date)
}

/// What a day of duties scores for "Make duties automatically" (less is better): what Optimise
/// weighs it (`rf::weighed`: the cost, the soft rules' penalties, a hard rule a bus a day), RosterFlow
/// Pro's red marks as dear as a hard rule, a duty working longer than the goals' longest as dear,
/// and each minute of a duty past the target twice a minute's wage, short of it a quarter (a duty
/// under four hours is paid four: its cost says so already).
pub fn score(c: &Company, b: &Board) -> f64 {
    let goals = c.planning.flow.prefs.goals.sane();
    let pro = on(c);
    let (hour, ..) = rf::rates(c);
    let rate = hour / 60.0;
    let mut x = rf::weighed(b) as f64;
    for (row, s) in b.shaped.iter().enumerate() {
        if s.spare.is_some() {
            continue;
        }
        x += 500_00.0 * marks(c, b, row).iter().filter(|m| m.hard).count() as f64;
        if s.work > goals.longest {
            x += 500_00.0;
        }
        // (round O: a night duty at the night's target; a scrap of night work dear)
        let night = pro && rf::spans_night(&b.tours, &s.spans);
        let (from, to) = goals.target(night);
        if night && s.work < goals.night_least {
            x += rf::NIGHT_SCRAP;
        }
        if s.work > to {
            x += (s.work - to) as f64 * rate * 2.0;
        } else if s.work < from && (s.work >= SHORT_DUTY || night) {
            x += (from - s.work) as f64 * rate * 0.25;
        }
    }
    x
}

/// The day's duties numbered in the order they sign on, from the day type's base (1001, 2001,
/// 3001): the numbers kept for them (`DutyNumber`) written anew; the pieces in the pool keep
/// theirs from `POOL_FROM`. Returns how many duties the day has.
pub fn number_by_sign_on(c: &mut Company, lines: &[LineInfo], date: &str) -> usize {
    let wd = super::plandays::slot_of(c, date);
    let b = rf::board(c, lines, date);
    let base = base(c, wd);
    let mut rows: Vec<(i32, PieceRef)> = b.shaped.iter().filter(|s| s.spare.is_none() && !is_pool_duty(c, wd, &b.tours, s)).filter_map(|s| Some((s.start, s.key.clone()?))).collect();
    rows.sort_by_key(|r| r.0);
    let n = b.shaped.iter().filter(|s| s.spare.is_none()).count();
    if !on(c) {
        return n;
    }
    let sh = &mut c.planning.flow.sheet;
    sh.numbers.retain(|x| x.weekday != wd);
    for (k, (_, key)) in rows.into_iter().enumerate() {
        sh.numbers.push(DutyNumber { weekday: wd, key, number: base + 1 + k as u32 });
    }
    n
}

// --- the marks ---------------------------------------------------------------------------------

pub const M_SPREAD: &str = "A split duty spread over %{time}, at most %{max}";
pub const M_CHANGE: &str = "%{have} to change from %{from} to %{to}, %{need} needed";
pub const M_CHANGE_SAME: &str = "%{have} to change buses at %{stop}, %{need} needed";
pub const M_WEEKEND_SPLIT: &str = "Split duties do not run at the weekend";
/// (Round O) A split duty with night-bus work (the user, 7 November 2026: never).
pub const M_NIGHT_SPLIT: &str = "Split duties may not have night-bus work";
/// (Round P) A part of a split duty long enough to need a meal break with none of its own (the
/// gap between the parts is no meal break: the driver is off duty then).
pub const M_PART_MEAL: &str = "Part %{n} of the split duty works %{time} with no meal break, one is needed after %{after}";
/// (Round K) A piece begun or ended at a stop that is no relief point (built in "Afgange").
pub const M_NOT_RELIEF_ON: &str = "Takes over the bus at %{stop} at %{time}, which is no relief point";
pub const M_NOT_RELIEF_OFF: &str = "Hands over the bus at %{stop} at %{time}, which is no relief point";

/// A mark of a duty: what it breaks, red (`hard`) or yellow.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub hard: bool,
    pub text: &'static str,
    pub args: Vec<(&'static str, String)>,
}

/// The trips of a span's piece.
fn span_trips<'a>(tours: &'a [TourOfDay], sp: &rf::Span) -> &'a [PlannedTrip] {
    let Some(t) = tours.get(sp.tour) else { return &[] };
    let r = network::duties_of(t).get(sp.duty).cloned().unwrap_or(0..0);
    t.trips.get(r).unwrap_or(&[])
}

/// What RosterFlow Pro marks on duty `row` of the board besides RosterFlow's own issues: a split
/// duty spread over `SPLIT_SPREAD`, a split duty at the weekend (yellow), and too little time to
/// change between two of its pieces.
pub fn marks(c: &Company, board: &Board, row: usize) -> Vec<Mark> {
    let Some(s) = board.shaped.get(row) else { return Vec::new() };
    let mut out = Vec::new();
    if kind_of(s) == Kind::Split && s.spread() > SPLIT_SPREAD {
        out.push(Mark { hard: true, text: M_SPREAD, args: vec![("time", rf::hm(s.spread())), ("max", rf::hm(SPLIT_SPREAD))] });
    }
    // (a split duty on a Saturday or a Sunday: they run Monday to Friday, in a split-duty rota)
    if kind_of(s) == Kind::Split && day_type(c, board.weekday) != 0 {
        out.push(Mark { hard: false, text: M_WEEKEND_SPLIT, args: Vec::new() });
    }
    // (round O: a split duty with night-bus work - never, red; made by hand it is marked, not
    // refused, as the other rules)
    if (kind_of(s) == Kind::Split || !s.splits.is_empty()) && rf::spans_night(&board.tours, &s.spans) {
        out.push(Mark { hard: true, text: M_NIGHT_SPLIT, args: Vec::new() });
    }
    // (round P: each part of a split duty its own meal break when it is long enough to need one)
    let prefs = &c.planning.flow.prefs;
    // (RosterFlow's own meal issue already counts it: not twice)
    let rf_meal = s.issues.iter().any(|&i| board.issues.get(i).is_some_and(|x| x.text == rf::I_MEAL || x.text == rf::I_MEAL_LATE));
    if kind_of(s) == Kind::Split && prefs.meal.on && !rf_meal {
        for (n, (a, z, worked)) in parts(s).into_iter().enumerate() {
            if worked > prefs.meal_after && part_meal(s, (a, z)).is_none() {
                out.push(Mark { hard: false, text: M_PART_MEAL, args: vec![("n", (n + 1).to_string()), ("time", rf::hm(worked)), ("after", rf::hm(prefs.meal_after))] });
            }
        }
    }
    // (round K: a piece begun or ended where no relief point is - yellow, the planner's choice)
    for sp in &s.spans {
        let Some(t) = board.tours.get(sp.tour) else { continue };
        let Some(r) = network::duties_of(t).get(sp.duty).cloned() else { continue };
        if r.start > 0 && !rf::relief_ok(c, &board.tours, &t.trips[r.start - 1], &t.trips[r.start]) {
            out.push(Mark { hard: false, text: M_NOT_RELIEF_ON, args: vec![("stop", t.trips[r.start].from.trim().to_string()), ("time", rf::clock(t.trips[r.start].dep))] });
        }
        if r.end < t.trips.len() && r.end > 0 && !rf::relief_ok(c, &board.tours, &t.trips[r.end - 1], &t.trips[r.end]) {
            out.push(Mark { hard: false, text: M_NOT_RELIEF_OFF, args: vec![("stop", t.trips[r.end - 1].end().trim().to_string()), ("time", rf::clock(t.trips[r.end - 1].arr))] });
        }
    }
    let change = c.planning.flow.prefs.change;
    let mut spans = s.spans.clone();
    spans.sort_by_key(|x| x.from);
    for w in spans.windows(2) {
        let (a, b) = (span_trips(&board.tours, &w[0]), span_trips(&board.tours, &w[1]));
        let (Some(last), Some(first)) = (a.last(), b.first()) else { continue };
        // (the same bus going on: no change)
        if w[0].tour == w[1].tour && w[1].duty == w[0].duty + 1 {
            continue;
        }
        let have = first.dep - last.arr;
        let (from, to) = (last.end().trim().to_string(), first.from.trim().to_string());
        if from.eq_ignore_ascii_case(&to) {
            if have < change.same_stop {
                out.push(Mark { hard: true, text: M_CHANGE_SAME, args: vec![("have", format!("{have} min")), ("stop", from), ("need", format!("{} min", change.same_stop))] });
            }
        } else {
            let need = change.other_stop.max(rf::travel_between(c, &board.tours, &from, &to, None).1);
            if have < need {
                out.push(Mark { hard: true, text: M_CHANGE, args: vec![("have", format!("{have} min")), ("from", from), ("to", to), ("need", format!("{need} min"))] });
            }
        }
    }
    out
}

/// Every English text of RosterFlow Pro's duties (for the translations' check).
pub fn texts() -> Vec<&'static str> {
    let mut v = vec![IN_POOL, NOT_IN_POOL, OVERLAP, NO_PIECE, M_SPREAD, M_CHANGE, M_CHANGE_SAME, M_WEEKEND_SPLIT, M_NIGHT_SPLIT, M_PART_MEAL, M_NOT_RELIEF_ON, M_NOT_RELIEF_OFF, NAME_LONG, NAME_TAKEN, NO_DEPARTURE, BUILD_OVERLAP];
    v.extend(Kind::LABELS);
    v
}

#[cfg(test)]
#[path = "pro_duties_tests.rs"]
mod tests;
