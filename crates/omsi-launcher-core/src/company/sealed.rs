//! (NexiconDK, 31 October 2026) Bus Company Online's tenders as sealed bids, as a public
//! authority lets them: every company of the map and the map's operators that take part hand in
//! one bid each - a company may change its own until the deadline, its last counts - nobody sees
//! another's, and at the close they are opened together and scored 70/30 (`auction::score`)
//! against the lowest valid one. The best total wins.
//!
//! Before, the map's tenders were open auctions: every bid shown at once, each new one had to
//! lead, and the operators answered down to a floor of the authority's cost price - reckoned on
//! the map's guessed country, a UK operator's costs in "International" - so the most aggressive
//! of them won at some 354 kr a timetable hour, below what a company of the server could run a
//! line for. Now:
//! * an operator bids its own cost - an aggressive one runs lean, a big one has its scale, a small
//!   or cautious one the authority's reckoning - with a margin by its character; an aggressive one
//!   now and then gambles below its own cost (`OPERATORS`);
//! * a bid under `LOW_SHARE` of the authority's cost price is abnormally low and is rejected, an
//!   operator's or a company's (the operators know the rule and stay above it);
//! * the cap is the cost price by `CAP_SHARE` (at most the server's own cap a timetable hour);
//! * the cost price is reckoned in the country and at the prices of the map's companies
//!   (`shared_tenders::template`);
//! * an operator that won below its own cost may give the line up before its term ends
//!   (`gives_up`): the line goes out to tender again at once (`shared_tenders::OperatorHold`).

use super::auction::{self, Character};
use super::concessions::{Standing, Tender};
use super::model::Cents;
use super::rng::Rng;

/// Bids under this share of the authority's cost price are abnormally low: rejected.
pub const LOW_SHARE: f64 = 0.85;
/// The cap: the authority's cost price by this.
pub const CAP_SHARE: f64 = 1.5;

/// An operator's own cost against the authority's reckoning of the work, and its margin over it
/// (from, to; an aggressive one may go below its own cost), by its character.
pub fn operator_terms(ch: Character) -> (f64, f64, f64) {
    match ch {
        Character::Aggressive => (0.96, -0.06, 0.05),
        Character::Big => (0.93, 0.02, 0.10),
        Character::Small => (1.00, 0.05, 0.14),
        Character::Cautious => (1.00, 0.10, 0.20),
    }
}

/// Whole hundreds of cents, up.
fn round_up(x: f64) -> Cents {
    (x / auction::ROUND as f64).ceil() as Cents * auction::ROUND
}

/// The least a bid of tender `t` may ask a year: under it, abnormally low.
pub fn low_line(t: &Tender) -> Cents {
    round_up(t.cost.max(0) as f64 * LOW_SHARE)
}

/// The cap of a tender of the cost price `cost` a year: `CAP_SHARE` of it, at most `most` (0:
/// none).
pub fn cap_of(cost: Cents, most: Cents) -> Cents {
    let cap = round_up(cost.max(0) as f64 * CAP_SHARE);
    if most > 0 {
        cap.min(most)
    } else {
        cap
    }
}

/// An operator's sealed bid: who, its quality, its ask a year and its own cost of the work.
#[derive(Clone, Debug, PartialEq)]
pub struct OperatorBid {
    pub rival: u32,
    pub name: String,
    pub quality: f64,
    pub amount: Cents,
    pub own_cost: Cents,
}

/// The sealed bids of the operators taking part in a tender of the map's market: its own cost by
/// its margin, drawn per tender (the same wherever it is reckoned), never under the abnormally low
/// line; one whose ask comes above the cap does not bid.
pub fn operator_bids(t: &Tender) -> Vec<OperatorBid> {
    let Some(m) = &t.market else { return Vec::new() };
    let low = low_line(t);
    let mut out = Vec::new();
    for id in &t.bidders {
        let Some(rv) = m.rivals.iter().find(|r| r.id == *id) else { continue };
        let mut rng = Rng::of(&[&m.key, "sealed", &rv.name], m.id as i64);
        let (share, lo, hi) = operator_terms(rv.character);
        let own = t.cost.max(0) as f64 * share;
        let amount = round_up((own * (1.0 + rng.range(lo, hi))).max(low as f64 * 1.005));
        if amount > t.reference {
            continue;
        }
        out.push(OperatorBid { rival: rv.id, name: rv.name.clone(), quality: rv.quality, amount, own_cost: round_up(own) });
    }
    out
}

/// A bid opened at the close: whose (an index of the caller's), its quality and its ask a year.
#[derive(Clone, Debug, PartialEq)]
pub struct Opened {
    pub who: usize,
    pub name: String,
    pub quality: f64,
    pub amount: Cents,
}

/// The bids of tender `t` opened and scored: every valid one 70/30 against the lowest valid ask,
/// the best first (on equal totals the better price, then the better quality); those under the
/// abnormally low line or above the cap rejected, last, with no points. The winner: the first,
/// if it is valid.
pub fn score(t: &Tender, bids: &[Opened]) -> (Option<usize>, Vec<(usize, Standing)>) {
    let low = low_line(t);
    let valid = |b: &Opened| b.amount >= low && t.counts(b.amount);
    let lowest = bids.iter().filter(|b| valid(b)).map(|b| b.amount).min();
    let mut rows: Vec<(usize, Standing)> = bids
        .iter()
        .map(|b| {
            let ok = valid(b);
            let (price, quality) = match lowest {
                Some(l) if ok => (auction::price_points(b.amount, l), auction::quality_points(b.quality)),
                _ => (0.0, 0.0),
            };
            (b.who, Standing { name: b.name.clone(), amount: b.amount, price, quality, total: price + quality, rejected: !ok })
        })
        .collect();
    rows.sort_by(|a, b| {
        a.1.rejected
            .cmp(&b.1.rejected)
            .then(b.1.total.total_cmp(&a.1.total))
            .then(b.1.price.total_cmp(&a.1.price))
            .then(b.1.quality.total_cmp(&a.1.quality))
    });
    let winner = rows.first().filter(|r| !r.1.rejected).map(|r| r.0);
    (winner, rows)
}

/// The day an operator that won at `amount` with its own cost `own_cost` gives the line up,
/// counted from its term's first day (`days` long) - None: it runs the term. The further below
/// its own cost, the likelier (ten times the shortfall's share, at most nine in ten), on a day
/// of the middle of its term. `seed`: the draw's (the line and the term).
pub fn gives_up(amount: Cents, own_cost: Cents, days: i64, seed: &[&str]) -> Option<i64> {
    if own_cost <= 0 || amount >= own_cost || days < 4 {
        return None;
    }
    let short = (own_cost - amount) as f64 / own_cost as f64;
    let mut rng = Rng::of(seed, 0);
    rng.chance((short * 10.0).min(0.9)).then(|| rng.int(days / 4, days * 3 / 4))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::company::concessions::{Rival, Shared};

    fn tender(cost: Cents) -> Tender {
        let c = crate::company::found(&crate::company::Founding { name: "T".into(), date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "");
        let l = crate::company::concessions::tests::line("Linie5", "5", 3, true);
        let mut t = crate::company::concessions::make_tender(&c, &l, 7, 0, false, "k", &[], (0, 0));
        t.cost = cost;
        t.reference = cap_of(cost, 0);
        let rivals: Vec<Rival> = Character::ALL.iter().enumerate().map(|(k, ch)| Rival { id: k as u32 + 1, name: format!("Op {k}"), character: *ch, quality: 90.0 }).collect();
        t.bidders = rivals.iter().map(|r| r.id).collect();
        t.market = Some(Shared { key: "grundorf".into(), id: 7, rivals, ..Default::default() });
        t
    }

    /// Opened together: the lowest valid ask gets the 70 price points; one under 85 % of the cost
    /// price or above the cap is rejected, last, with none; the best valid total wins.
    #[test]
    fn the_bids_are_opened_and_scored_and_the_abnormally_low_rejected() {
        let t = tender(100_000_00);
        let bid = |who: usize, amount: Cents, quality: f64| Opened { who, name: format!("B{who}"), quality, amount };
        let (w, rows) = score(&t, &[bid(0, 80_000_00, 100.0), bid(1, 95_000_00, 90.0), bid(2, 110_000_00, 100.0), bid(3, 160_000_00, 100.0)]);
        assert_eq!(w, Some(1), "{rows:?}");
        assert!((rows[0].1.price - 70.0).abs() < 1e-9 && rows[0].1.total > rows[1].1.total);
        let rejected: Vec<usize> = rows.iter().filter(|r| r.1.rejected).map(|r| r.0).collect();
        assert_eq!(rejected, vec![0, 3], "too low, above the cap");
        assert!(rows.iter().rev().take(2).all(|r| r.1.rejected && r.1.total == 0.0));
        // nothing valid: nobody wins
        assert_eq!(score(&t, &[bid(0, 10_000_00, 100.0)]).0, None);
        assert_eq!(low_line(&t), 85_000_00);
    }

    /// The operators bid their own cost by their margin: never under the abnormally low line,
    /// never above the cap; the aggressive one now and then below its own cost.
    #[test]
    fn the_operators_bid_their_own_cost_by_their_margin() {
        let mut below = 0;
        for k in 0..200u32 {
            let mut t = tender(100_000_00);
            t.market.as_mut().unwrap().id = k;
            let ops = operator_bids(&t);
            assert_eq!(ops.len(), 4);
            for o in &ops {
                assert!(o.amount >= low_line(&t) && o.amount <= t.reference, "{o:?}");
            }
            let aggressive = ops.iter().find(|o| o.name == "Op 0").unwrap();
            below += (aggressive.amount < aggressive.own_cost) as u32;
            let cautious = ops.iter().find(|o| o.name == "Op 1").unwrap();
            assert!(cautious.amount >= 110_000_00, "{cautious:?}");
        }
        assert!((40..=160).contains(&below), "{below}");
    }

    /// The further below its own cost an operator won, the likelier it gives the line up, in the
    /// middle of its term; at or above it, never.
    #[test]
    fn an_operator_below_its_cost_may_give_up() {
        assert_eq!(gives_up(100, 100, 14, &["a"]), None);
        let n = |short: Cents| (0..400).filter(|k| gives_up(100_00 - short, 100_00, 14, &["l", &k.to_string()]).is_some()).count();
        let (small, big) = (n(1_00), n(6_00));
        assert!(small < big && big > 200, "{small} {big}");
        for k in 0..100 {
            if let Some(d) = gives_up(90_00, 100_00, 14, &["d", &k.to_string()]) {
                assert!((3..=10).contains(&d), "{d}");
            }
        }
    }
}
