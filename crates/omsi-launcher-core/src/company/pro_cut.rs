//! RosterFlow Pro: cutting an open duty for its date (NexiconDK, 4 November 2026, "Klip vagten").
//!
//! When nobody can or will take the whole of an open duty, the planner cuts it at its relief
//! points so that as much of it as possible is driven - as a Danish bus company's planner does
//! on the day. `propose` chooses the cut: at most `MAX_PARTS` parts, the one that covers most,
//! then the fewest parts, then the cheapest. Each part gets one suggestion (`suggest`): a driver
//! whose own duty ends at or just before it at the same or a nearby stop and who goes on with a
//! little overtime, one whose own duty begins just after it and who begins earlier, one who
//! answered "too early" / "too late" with a time that fits it, a reserve within his frame, an
//! hourly-paid driver, or a free colleague. The working-time rules hold for each (rest, overlap,
//! the longest day: `plan::can_take`), and the time to change from step 2 (`pro_duties::Change`).
//!
//! A cut lives only for its date (`Split`, kept in `Ops::splits`): the blocks are cut there for
//! that date only (`tour_cuts`, applied when the date's tours are laid out), so each part is
//! whole pieces of the date's plan; the Vagter tab and the other days do not change. A part is
//! given to a driver for that date (`pro_ops::Call` with its pieces). `uncut` joins it again.

use super::model::{Cents, Company, Employment};
use super::network::{self, TourOfDay};
use super::plan::{self, Cannot, DayPlan, Who};
use super::pro_ops::{self as po, OpenDuty, Price, Reply};
use super::pro_turnus::{self as pt, ProDuty};
use super::staff::{self, Block};
use serde::{Deserialize, Serialize};

/// The most parts a duty is cut into.
pub const MAX_PARTS: usize = 4;
/// The longest a driver waits between his own duty and a part he goes on with (or begins
/// earlier for), in minutes.
pub const NEAR_MINUTES: i32 = 45;

/// A piece of a block by the trip it begins with: the same piece however the blocks are cut.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PieceKey {
    pub line: String,
    pub tour: String,
    pub dep: i32,
    pub name: String,
}

fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

impl PieceKey {
    pub fn of(t: &TourOfDay, at: usize) -> PieceKey {
        PieceKey { line: t.line.clone(), tour: t.tour.clone(), dep: t.trips[at].dep, name: t.trips[at].name.clone() }
    }

    fn is_tour(&self, t: &TourOfDay) -> bool {
        same(&self.line, &t.line) && same(&self.tour, &t.tour)
    }

    /// The trip of `t` it begins with.
    pub fn trip_in(&self, t: &TourOfDay) -> Option<usize> {
        if !self.is_tour(t) {
            return None;
        }
        t.trips.iter().position(|x| x.dep == self.dep && x.name == self.name)
    }

    /// The piece (line, block, piece of it) of `tours` it is.
    pub fn resolve(&self, tours: &[TourOfDay]) -> Option<(String, String, usize)> {
        tours.iter().find_map(|t| {
            let at = self.trip_in(t)?;
            let k = network::duties_of(t).iter().position(|r| r.start == at)?;
            Some((t.line.clone(), t.tour.clone(), k))
        })
    }

    /// The piece (tour, piece) of the day's plan `dp` it is.
    pub fn in_plan(&self, dp: &DayPlan) -> Option<(usize, usize)> {
        dp.tours.iter().enumerate().find_map(|(ti, t)| {
            let at = self.trip_in(&t.tour)?;
            t.duties.iter().position(|d| d.start == at).map(|k| (ti, k))
        })
    }
}

/// A part of a cut duty: its pieces (after the cut), when it begins and ends.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Part {
    pub pieces: Vec<PieceKey>,
    pub from: i32,
    pub to: i32,
}

/// Duty `n` of `date` cut into parts for that date: the parts, and the trips the blocks are cut
/// before for it (`inner`: inside a piece; a cut between two pieces needs none).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Split {
    pub date: String,
    pub n: u32,
    pub parts: Vec<Part>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inner: Vec<PieceKey>,
}

/// "A", "B", … for part `i`.
pub fn letter(i: usize) -> char {
    (b'A' + (i as u8 % 26)) as char
}

/// The splits of `date`, RosterFlow Pro chosen.
pub fn splits_on<'a>(c: &'a Company, date: &'a str) -> impl Iterator<Item = &'a Split> + 'a {
    let on = super::pro_duties::on(c);
    c.planning.ops.splits.iter().filter(move |s| on && s.date == date)
}

/// The split of duty `n` of `date`.
pub fn split_of<'a>(c: &'a Company, date: &str, n: u32) -> Option<&'a Split> {
    let on = super::pro_duties::on(c);
    c.planning.ops.splits.iter().find(|s| on && s.date == date && s.n == n)
}

/// The blocks of `date` cut where its splits cut them (`network::finish_tours`).
pub fn tour_cuts(c: &Company, date: &str, tours: &mut [TourOfDay]) {
    for s in splits_on(c, date) {
        for key in &s.inner {
            let Some(t) = tours.iter_mut().find(|t| key.is_tour(t)) else { continue };
            let Some(at) = key.trip_in(t) else { continue };
            if at == 0 || at >= t.trips.len() {
                continue;
            }
            let mut starts: Vec<usize> = network::duties_of(t).iter().map(|r| r.start).filter(|k| *k > 0).collect();
            starts.push(at);
            starts.sort_unstable();
            starts.dedup();
            t.cuts = Some(starts);
        }
    }
}

/// The blocks of `date` (`tours`, cut by `tour_cuts`) as they are uncut: the trips its splits cut
/// before joined again.
pub fn uncut_tours(c: &Company, date: &str, tours: &[TourOfDay]) -> Vec<TourOfDay> {
    let mut out = tours.to_vec();
    for s in splits_on(c, date) {
        for key in &s.inner {
            let Some(t) = out.iter_mut().find(|t| key.is_tour(t)) else { continue };
            let Some(at) = key.trip_in(t) else { continue };
            if let Some(cuts) = t.cuts.as_mut() {
                cuts.retain(|k| *k != at);
            }
        }
    }
    out
}

/// The pieces of part `p` in the day's plan `dp` (cut as its split says).
pub fn part_pieces(dp: &DayPlan, p: &Part) -> Vec<(usize, usize)> {
    p.pieces.iter().filter_map(|k| k.in_plan(dp)).collect()
}

/// The label of piece `k` of tour `ti` when it is a part of a split of the plan's date
/// ("1002B").
pub fn label_of(c: &Company, dp: &DayPlan, ti: usize, k: usize) -> Option<String> {
    let t = &dp.tours[ti];
    let first = t.duties.get(k)?.start;
    let key = PieceKey::of(&t.tour, first);
    splits_on(c, dp.date.as_str()).find_map(|s| s.parts.iter().position(|p| p.pieces.contains(&key)).map(|i| format!("{}{}", s.n, letter(i))))
}

// --- a stretch of trips, as a part is before the cut --------------------------------------------

/// Trips `a..b` of tour `ti` of a day's plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seg {
    pub ti: usize,
    pub a: usize,
    pub b: usize,
}

impl Seg {
    pub fn from(&self, dp: &DayPlan) -> i32 {
        dp.tours[self.ti].tour.trips[self.a].dep
    }

    pub fn to(&self, dp: &DayPlan) -> i32 {
        dp.tours[self.ti].tour.trips[self.a..self.b].iter().map(|x| x.arr).max().unwrap_or(0)
    }

    pub fn block(&self, dp: &DayPlan) -> Block {
        let t = &dp.tours[self.ti].tour;
        Block { key: format!("cut/{}/{}/{}", t.line, t.tour, self.a), from: self.from(dp), to: self.to(dp), from_stop: t.trips[self.a].from.clone(), to_stop: t.trips[self.b - 1].to.clone() }
    }

    fn counts(&self, dp: &DayPlan) -> bool {
        dp.tours[self.ti].tour.trips[self.a..self.b].iter().any(|x| x.counts())
    }

    /// The trips of it that carry passengers.
    pub fn trips(&self, dp: &DayPlan) -> usize {
        dp.tours[self.ti].tour.trips[self.a..self.b].iter().filter(|x| x.counts()).count()
    }
}

/// Pieces `pieces` of a day's plan as stretches of trips.
pub fn segs_of(dp: &DayPlan, pieces: &[(usize, usize)]) -> Vec<Seg> {
    let mut v: Vec<Seg> = pieces.iter().map(|&(ti, k)| Seg { ti, a: dp.tours[ti].duties[k].start, b: dp.tours[ti].duties[k].end }).collect();
    v.sort_by_key(|s| s.from(dp));
    v
}

// --- who could take a part ---------------------------------------------------------------------

/// How a driver comes to a part.
#[derive(Clone, Debug, PartialEq)]
pub enum How {
    /// He goes on after his own duty `own`, which ends at `at` at `stop`.
    Continues { own: u32, at: i32, stop: String },
    /// He begins earlier than his own duty `own`, which begins at `at` at `stop`.
    Earlier { own: u32, at: i32, stop: String },
    /// He said he can from (`late` false) or until (`late` true) `time`.
    Said { late: bool, time: i32 },
    /// A reserve within his frame.
    Frame,
    Hourly,
    /// A free colleague.
    Colleague,
}

impl How {
    /// How good a way it is (0 best: going on or beginning earlier).
    pub fn rank(&self) -> u8 {
        match self {
            How::Continues { .. } | How::Earlier { .. } => 0,
            How::Said { .. } => 1,
            How::Frame => 2,
            How::Hourly => 3,
            How::Colleague => 4,
        }
    }
}

/// A driver suggested for a part.
#[derive(Clone, Debug, PartialEq)]
pub struct Sugg {
    pub driver: u32,
    pub tier: u8,
    pub how: How,
    pub price: Price,
    pub cost: Cents,
}

/// Two stops near each other: named alike, or the same first word ("Bytorvet" and "Bytorvet
/// Nord") - the map has no places to measure.
pub fn nearby(a: &str, b: &str) -> bool {
    let first = |s: &str| s.split_whitespace().next().map(str::to_lowercase).unwrap_or_default();
    same(a, b) || (!first(a).is_empty() && first(a) == first(b))
}

/// The number of the duty (`duties`, the date's) piece `k` of tour `ti` belongs to (0: none).
pub fn number_at(dp: &DayPlan, duties: &[ProDuty], ti: usize, k: usize) -> u32 {
    let t = &dp.tours[ti].tour;
    duties.iter().find(|d| d.pieces.iter().any(|(l, tt, kk)| *kk == k && same(l, &t.line) && same(tt, &t.tour))).map_or(0, |d| d.number)
}

/// Whether driver `e` can drive the stretches `segs` beside his other work of the day.
fn fit_segs(c: &Company, dp: &DayPlan, e: &super::model::Employee, segs: &[Seg]) -> Result<(), Cannot> {
    match pt::decided(c, e.id, &dp.date).and_then(|d| d.note).as_deref() {
        Some("sick") => return Err(Cannot::Ill { until: dp.date.clone() }),
        Some("holiday") => return Err(Cannot::Holiday { until: dp.date.clone() }),
        _ => {}
    }
    let mut work: Vec<Block> = dp.duties_of(Who::Staff(e.id)).into_iter().map(|(ti, k)| plan::duty_block(dp, ti, k)).collect();
    for s in segs {
        let b = s.block(dp);
        plan::can_take(e, &dp.date, dp.today, &dp.tours[s.ti].bus_of(c), &work, &b)?;
        work.push(b);
    }
    Ok(())
}

/// The best driver for the stretches `segs` of the plan `dp` (`duties` the date's, `week(id)` the
/// minutes of his week, `said` the answers given for the duty, `skip` drivers not to suggest),
/// or None when nobody can.
pub fn suggest(c: &Company, dp: &DayPlan, segs: &[Seg], duties: &[ProDuty], week: &dyn Fn(u32) -> f64, said: &[(u32, Reply)], skip: &[u32]) -> Option<Sugg> {
    let (first, last) = (segs.first()?, segs.last()?);
    let (from, to) = (first.from(dp), last.to(dp));
    let t0 = &dp.tours[first.ti].tour;
    let t1 = &dp.tours[last.ti].tour;
    let (from_stop, to_stop) = (t0.trips[first.a].from.clone(), t1.trips[last.b - 1].to.clone());
    let p = &c.planning.flow.prefs;
    let need = |a: &str, b: &str| if same(a, b) { p.change.same_stop } else { p.change.other_stop };
    let span = Block { key: "pro-cut".into(), from, to, ..Default::default() };
    let mut best: Option<(u8, Cents, u32, Sugg)> = None;
    for e in c.staff.iter().filter(|e| staff::drives(e) && !skip.contains(&e.id)) {
        // (what he answered for the duty: no is no; too early or too late only when it fits)
        let mine_said: Vec<&Reply> = said.iter().filter(|(id, _)| *id == e.id).map(|x| &x.1).collect();
        let mut said_how = None;
        let mut refused = false;
        for r in &mine_said {
            match r {
                Reply::Early { from: f, .. } if *f <= from => said_how = Some(How::Said { late: false, time: *f }),
                Reply::Late { until, .. } if *until >= to => said_how = Some(How::Said { late: true, time: *until }),
                Reply::Early { .. } | Reply::Late { .. } | Reply::No { .. } | Reply::Silent => refused = true,
                _ => {}
            }
        }
        if refused && said_how.is_none() {
            continue;
        }
        if fit_segs(c, dp, e, segs).is_err() {
            continue;
        }
        let tier = plan::fill_tier(c, e, &dp.date, &span);
        let mine = dp.duties_of(Who::Staff(e.id));
        let blocks: Vec<((usize, usize), Block)> = mine.iter().map(|&(ti, k)| ((ti, k), plan::duty_block(dp, ti, k))).collect();
        let before = blocks.iter().filter(|(_, b)| b.to <= from).max_by_key(|(_, b)| b.to);
        let after = blocks.iter().filter(|(_, b)| b.from >= to).min_by_key(|(_, b)| b.from);
        let mut how = None;
        let mut minutes = (to - from) + p.sign_on + p.sign_off;
        if let Some(((ti, k), b)) = before {
            let gap = from - b.to;
            if nearby(&b.to_stop, &from_stop) && gap >= need(&b.to_stop, &from_stop) && gap <= NEAR_MINUTES {
                how = Some(How::Continues { own: number_at(dp, duties, *ti, *k), at: b.to, stop: b.to_stop.trim().to_string() });
                minutes = to - b.to;
            }
        }
        if how.is_none() {
            if let Some(((ti, k), b)) = after {
                let gap = b.from - to;
                if nearby(&to_stop, &b.from_stop) && gap >= need(&to_stop, &b.from_stop) && gap <= NEAR_MINUTES {
                    how = Some(How::Earlier { own: number_at(dp, duties, *ti, *k), at: b.from, stop: b.from_stop.trim().to_string() });
                    minutes = b.from - from;
                }
            }
        }
        let how = how.or(said_how).unwrap_or(if tier == plan::TIER_RESERVE {
            How::Frame
        } else if e.employment == Employment::Hourly {
            How::Hourly
        } else {
            How::Colleague
        });
        let (price, cost) = po::price_of(c, e, tier, minutes, week(e.id), !mine.is_empty());
        let key = (how.rank(), cost, e.id);
        if best.as_ref().is_none_or(|b| key < (b.0, b.1, b.2)) {
            best = Some((key.0, key.1, key.2, Sugg { driver: e.id, tier, how, price, cost }));
        }
    }
    best.map(|b| b.3)
}

// --- the cut ------------------------------------------------------------------------------------

/// A part as proposed: its stretches, when it begins and ends, and its driver if one can.
#[derive(Clone, Debug, PartialEq)]
pub struct PartPlan {
    pub segs: Vec<Seg>,
    pub from: i32,
    pub to: i32,
    pub sugg: Option<Sugg>,
}

/// A cut as proposed: its parts.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub parts: Vec<PartPlan>,
}

impl Proposal {
    /// The minutes of it a driver is found for.
    pub fn covered(&self) -> i32 {
        self.parts.iter().filter(|p| p.sugg.is_some()).map(|p| p.to - p.from).sum()
    }

    pub fn cost(&self) -> Cents {
        self.parts.iter().filter_map(|p| p.sugg.as_ref()).map(|s| s.cost).sum()
    }
}

/// Where a duty may be cut: (piece of it, trip) - inside a piece at a relief point
/// (`studio::can_cut`), or `usize::MAX` for "after this piece" (between two pieces).
fn cut_points(c: &Company, dp: &DayPlan, units: &[Seg]) -> Vec<(usize, usize)> {
    let tours: Vec<TourOfDay> = dp.tours.iter().map(|t| t.tour.clone()).collect();
    let mut v = Vec::new();
    for (u, s) in units.iter().enumerate() {
        for at in s.a + 1..s.b {
            if super::studio::can_cut(c, &tours, s.ti, at).is_ok() {
                v.push((u, at));
            }
        }
        if u + 1 < units.len() {
            v.push((u, usize::MAX));
        }
    }
    // (at most 12 places, spread over the duty: the cuts between pieces always)
    if v.len() > 12 {
        let between: Vec<(usize, usize)> = v.iter().copied().filter(|x| x.1 == usize::MAX).collect();
        let inner: Vec<(usize, usize)> = v.iter().copied().filter(|x| x.1 != usize::MAX).collect();
        let room = 12usize.saturating_sub(between.len()).max(1);
        let step = inner.len() as f64 / room as f64;
        let mut keep: Vec<(usize, usize)> = (0..room).filter_map(|i| inner.get((i as f64 * step) as usize).copied()).collect();
        keep.extend(between);
        keep.sort();
        keep.dedup();
        v = keep;
    }
    v
}

/// Whether open duty `d` of the plan `dp` can be cut at all: it has a relief point inside, or more
/// than one piece.
pub fn can_cut_duty(c: &Company, dp: &DayPlan, d: &OpenDuty) -> bool {
    !cut_points(c, dp, &segs_of(dp, &d.pieces)).is_empty()
}

/// The parts of the duty `units` cut at `chosen` (points of `cut_points`).
fn parts_at(units: &[Seg], chosen: &[(usize, usize)]) -> Vec<Vec<Seg>> {
    let mut out: Vec<Vec<Seg>> = vec![Vec::new()];
    for (u, s) in units.iter().enumerate() {
        let mut ats: Vec<usize> = chosen.iter().filter(|x| x.0 == u && x.1 != usize::MAX).map(|x| x.1).collect();
        ats.sort_unstable();
        let mut a = s.a;
        for at in ats {
            out.last_mut().unwrap().push(Seg { ti: s.ti, a, b: at });
            out.push(Vec::new());
            a = at;
        }
        out.last_mut().unwrap().push(Seg { ti: s.ti, a, b: s.b });
        if chosen.contains(&(u, usize::MAX)) {
            out.push(Vec::new());
        }
    }
    out.retain(|p| !p.is_empty());
    out
}

/// The cut for open duty `d` of the plan `dp`: at most `MAX_PARTS` parts, the one that covers
/// most, then the fewest parts, then the cheapest; each part with its driver if one can
/// (`suggest`, one driver a part).
pub fn propose(c: &Company, dp: &DayPlan, d: &OpenDuty, duties: &[ProDuty], week: &dyn Fn(u32) -> f64, said: &[(u32, Reply)]) -> Proposal {
    let units = segs_of(dp, &d.pieces);
    let points = cut_points(c, dp, &units);
    let mut best: Option<Proposal> = None;
    let mut choose = |chosen: &[(usize, usize)]| {
        let parts = parts_at(&units, chosen);
        if parts.len() > MAX_PARTS || parts.iter().any(|p| !p.iter().any(|s| s.counts(dp))) {
            return;
        }
        let mut used: Vec<u32> = Vec::new();
        let plans: Vec<PartPlan> = parts
            .into_iter()
            .map(|segs| {
                let sugg = suggest(c, dp, &segs, duties, week, said, &used);
                if let Some(s) = &sugg {
                    used.push(s.driver);
                }
                let (from, to) = (segs[0].from(dp), segs.last().unwrap().to(dp));
                PartPlan { segs, from, to, sugg }
            })
            .collect();
        let p = Proposal { parts: plans };
        let better = match &best {
            None => true,
            Some(b) => (p.covered(), std::cmp::Reverse(p.parts.len()), std::cmp::Reverse(p.cost())) > (b.covered(), std::cmp::Reverse(b.parts.len()), std::cmp::Reverse(b.cost())),
        };
        if better {
            best = Some(p);
        }
    };
    let n = points.len();
    choose(&[]);
    for i in 0..n {
        choose(&[points[i]]);
        for j in i + 1..n {
            choose(&[points[i], points[j]]);
            for k in j + 1..n {
                choose(&[points[i], points[j], points[k]]);
            }
        }
    }
    best.unwrap_or(Proposal { parts: Vec::new() })
}

/// Refusals (English: keys of the translations).
pub const NOT_CUT: &str = "That duty cannot be cut: it has no relief point where a driver can change.";
pub const NO_SUCH_PART: &str = "That part of the duty is gone.";

/// Duty `n` of `date` (open duty `d` of the plan `dp`) cut as `p` proposes, for that date.
pub fn cut(c: &mut Company, dp: &DayPlan, d: &OpenDuty, p: &Proposal) -> Result<(), &'static str> {
    if p.parts.len() < 2 {
        return Err(NOT_CUT);
    }
    let starts: Vec<(usize, usize)> = d.pieces.iter().map(|&(ti, k)| (ti, dp.tours[ti].duties[k].start)).collect();
    let mut inner = Vec::new();
    let parts: Vec<Part> = p
        .parts
        .iter()
        .map(|pp| {
            let pieces = pp
                .segs
                .iter()
                .map(|s| {
                    let key = PieceKey::of(&dp.tours[s.ti].tour, s.a);
                    if !starts.contains(&(s.ti, s.a)) {
                        inner.push(key.clone());
                    }
                    key
                })
                .collect();
            Part { pieces, from: pp.from, to: pp.to }
        })
        .collect();
    let date = dp.date.clone();
    c.planning.ops.splits.retain(|s| !(s.date == date && s.n == d.n));
    c.planning.ops.splits.push(Split { date, n: d.n, parts, inner });
    Ok(())
}

/// Duty `n` of `date` joined again: its split, the parts given and the questions for them gone.
/// Whether anything changed.
pub fn uncut(c: &mut Company, date: &str, n: u32) -> bool {
    let before = c.planning.ops.splits.len();
    c.planning.ops.splits.retain(|s| !(s.date == date && s.n == n));
    let game = super::clock::now(c);
    c.planning.ops.calls.retain(|x| !(x.date == date && x.n == n && x.part.is_some()));
    for a in c.planning.ops.asks.iter_mut().filter(|a| a.date == date && a.n == n && a.part.is_some()) {
        a.shut(game);
    }
    before != c.planning.ops.splits.len()
}

/// The pieces of `date`'s duties a split covers: they are its parts, not duties of their own.
pub fn covered(c: &Company, dp: &DayPlan) -> Vec<(usize, usize)> {
    splits_on(c, &dp.date).flat_map(|s| s.parts.iter().flat_map(|p| part_pieces(dp, p))).collect()
}

/// Every English text of this module.
pub fn texts() -> Vec<&'static str> {
    vec![NOT_CUT, NO_SUCH_PART]
}

#[cfg(test)]
#[path = "pro_cut_tests.rs"]
mod tests;
