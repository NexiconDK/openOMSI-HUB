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
}

impl Default for Goals {
    fn default() -> Self {
        Goals { target_from: 444, target_to: 510, longest: 600, same_line: true }
    }
}

impl Goals {
    /// The goals within reason: a target of at least an hour, ending before the longest.
    pub fn sane(mut self) -> Goals {
        self.longest = self.longest.clamp(240, 780);
        self.target_to = self.target_to.clamp(60, self.longest);
        self.target_from = self.target_from.clamp(0, self.target_to);
        self
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
        if s.work > goals.target_to {
            x += (s.work - goals.target_to) as f64 * rate * 2.0;
        } else if s.work < goals.target_from && s.work >= SHORT_DUTY {
            x += (goals.target_from - s.work) as f64 * rate * 0.25;
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
    let mut v = vec![IN_POOL, NOT_IN_POOL, OVERLAP, NO_PIECE, M_SPREAD, M_CHANGE, M_CHANGE_SAME, M_WEEKEND_SPLIT];
    v.extend(Kind::LABELS);
    v
}

#[cfg(test)]
#[path = "pro_duties_tests.rs"]
mod tests;
