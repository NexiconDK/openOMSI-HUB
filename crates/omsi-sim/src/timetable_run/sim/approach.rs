//! (NexiconDK, Bus Company Online, 31 October 2026) The way a relief's bus comes up to the stop
//! a LAN player waits at: put out at the stop - its stand when he takes it over at the trip's
//! first stop, his stop when it is late - it appeared there in front of him out of nothing. It
//! is put out back along the way it comes instead, where nobody sees it appear, and drives up:
//! on time when there is the time (a layover bus a few minutes early), else late. Along its
//! route first (the stops it passes on the way it serves, late), then along the lanes that lead
//! into the route's first - the street it comes up from the depot by.

use crate::traffic::{wrap_deg, LaneKind, Network};
use glam::DVec3;

/// How far back along its way a relief's bus is put out at most (m): beyond `NEAR_HIDE` and out
/// of the picture, or round a corner, within that on nearly every map.
pub const APPROACH_MOST: f32 = 700.0;

/// The steps it looks back in (m).
const APPROACH_STEP: f32 = 12.0;

/// Where a relief's bus is put out (`relief_approach`).
#[derive(Clone, Debug, PartialEq)]
pub struct Approach {
    /// The lanes leading into the route's first that it drives first, in their order - none when
    /// it starts on the route itself.
    pub before: Vec<usize>,
    /// The route's lane it starts on (its index in the route): the first when `before` has lanes.
    pub index: usize,
    /// How far into the lane it starts on (m: `before`'s first, or the route's at `index`).
    pub s: f32,
    /// How far back from where it was to be put out (m).
    pub back: f32,
    /// Nobody sees it appear there (else the furthest back it could go).
    pub hidden: bool,
}

/// Where along the way up to `route[index]` at `s` (a route of `net`'s lanes) a relief's bus is
/// put out: back in steps along the route, then the lanes leading into its first (the one
/// running on most straight, of the route's kind and open to buses, none twice), at most
/// `APPROACH_MOST`. `place(lane, s)` says whether the bus can stand there (None: no ground there,
/// or a vehicle in the way) and whether nobody would see it appear there. The first place nobody
/// sees, else the furthest one it can stand at - None when there is no place back from there
/// (a route that begins at the end of the network).
pub fn relief_approach(net: &Network, route: &[usize], index: usize, s: f32, mut place: impl FnMut(usize, f32) -> Option<bool>) -> Option<Approach> {
    let first = *route.get(index)?;
    let kind = net.lanes.get(first)?.kind;
    // (the lane it is on, how far into it, its index in the route - None off the route: then the
    // lanes before the route's first, the nearest last)
    let (mut lane, mut at, mut ri) = (first, s.clamp(0.0, net.lanes[first].length()), Some(index));
    let mut before: Vec<usize> = Vec::new();
    let mut back = 0.0f32;
    let mut furthest: Option<Approach> = None;
    let junction = |l: usize| net.conflicts.get(l).is_some_and(|c| !c.is_empty());
    while back + APPROACH_STEP <= APPROACH_MOST {
        let mut step = APPROACH_STEP;
        // back over the start of a lane: the one before it
        while at < step {
            step -= at;
            let prev = match ri {
                Some(k) if k > 0 => {
                    ri = Some(k - 1);
                    route[k - 1]
                }
                _ => {
                    let into = net.lanes[lane].start_heading();
                    let found = net.prev.get(lane).into_iter().flatten().copied().filter(|&p| {
                        let l = &net.lanes[p];
                        l.kind == kind && (kind != LaneKind::Street || !l.no_cars || l.rule_bus) && l.length() > 0.0 && !route.contains(&p) && !before.contains(&p)
                    });
                    let Some(p) = found.min_by(|&a, &b| wrap_deg(net.lanes[a].end_heading() - into).abs().total_cmp(&wrap_deg(net.lanes[b].end_heading() - into).abs())) else {
                        return furthest;
                    };
                    ri = None;
                    before.insert(0, p);
                    p
                }
            };
            back += at;
            lane = prev;
            at = net.lanes[lane].length();
        }
        at -= step;
        back += step;
        // (not inside a junction: it would be put out across the other traffic's way)
        if junction(lane) {
            continue;
        }
        let Some(hidden) = place(lane, at) else { continue };
        let here = Approach { before: before.clone(), index: ri.unwrap_or(0), s: at, back, hidden };
        if hidden {
            return Some(here);
        }
        furthest = Some(here);
    }
    furthest
}

/// (31 October 2026) How far back along the way up to its stop a bus given back may stand (m), how
/// far on past the stop, how far off the lane and how far turned from it (degrees): `stood_on`.
const STOOD_BACK: f32 = 250.0;
const STOOD_PAST: f32 = 30.0;
const STOOD_OFF: f64 = 4.0;
const STOOD_TURNED: f32 = 45.0;
/// The most lanes `stood_on` looks at back from the route (every way into it).
const STOOD_LANES: usize = 64;

/// (31 October 2026) Where on the way up to `route[index]` at `s` - back along the route and every
/// way of lanes leading into its first, or a little on past it - a bus standing at `at`, heading
/// `heading` (degrees), stands: the AI's bus of the departure a LAN player gave his bus back for is
/// put out just there (`ScheduleSim::back_stood`), where it stood and he stands, rather than at the
/// stop's own place. The nearest place within `STOOD_OFF` of it, the way the lane runs; `back` is
/// how far back from `s` it is (negative: past it), `hidden` is not asked (false).
pub fn stood_on(net: &Network, route: &[usize], index: usize, s: f32, at: DVec3, heading: f64) -> Option<Approach> {
    let first = *route.get(index)?;
    let kind = net.lanes.get(first)?.kind;
    if !at.is_finite() || !heading.is_finite() {
        return None;
    }
    let mut best: Option<(f64, Approach)> = None;
    // (a lane: the part of it looked at, how far back its start lies from `s`, the lanes before the
    // route's first it leads through, its index in the route)
    let mut look = |lane: usize, lo: f32, hi: f32, start_back: f32, before: &[usize], ri: Option<usize>| {
        let l = &net.lanes[lane];
        let Some((x, d)) = l.nearest_point(at) else { return };
        if hi < lo || x < lo - 0.5 || x > hi + 0.5 || d > STOOD_OFF {
            return;
        }
        let x = x.clamp(lo, hi);
        if wrap_deg(l.at(x).1 - heading as f32).abs() > STOOD_TURNED {
            return;
        }
        if best.as_ref().is_none_or(|b| d < b.0) {
            best = Some((d, Approach { before: before.to_vec(), index: ri.unwrap_or(0), s: x, back: start_back - x, hidden: false }));
        }
    };
    let len = |l: usize| net.lanes[l].length();
    // its own lane, and on along the route a little
    let s = s.clamp(0.0, len(first));
    look(first, (s - STOOD_BACK).max(0.0), (s + STOOD_PAST).min(len(first)), s, &[], Some(index));
    let mut ahead = len(first) - s;
    for (k, &l) in route.iter().enumerate().skip(index + 1) {
        if ahead >= STOOD_PAST {
            break;
        }
        look(l, 0.0, (STOOD_PAST - ahead).min(len(l)), -ahead, &[], Some(k));
        ahead += len(l);
    }
    // back along the route
    let mut back = s;
    for k in (0..index).rev() {
        if back >= STOOD_BACK {
            return best.map(|b| b.1);
        }
        let l = route[k];
        back += len(l);
        look(l, (back - STOOD_BACK).max(0.0), len(l), back, &[], Some(k));
    }
    // and every way into the route's first (its lanes, how far back the end of the nearest is)
    let mut queue: std::collections::VecDeque<(Vec<usize>, f32)> = std::collections::VecDeque::new();
    let into = |l: usize, before: &[usize]| -> Vec<usize> {
        net.prev.get(l).into_iter().flatten().copied().filter(|&p| {
            let q = &net.lanes[p];
            q.kind == kind && (kind != LaneKind::Street || !q.no_cars || q.rule_bus) && q.length() > 0.0 && !route.contains(&p) && !before.contains(&p)
        }).collect()
    };
    for p in into(route[0], &[]) {
        queue.push_back((vec![p], back));
    }
    let mut looked = 0;
    while let Some((before, end_back)) = queue.pop_front() {
        looked += 1;
        if looked > STOOD_LANES || end_back >= STOOD_BACK {
            continue;
        }
        let l = before[0];
        let start_back = end_back + len(l);
        look(l, (start_back - STOOD_BACK).max(0.0), len(l), start_back, &before, None);
        for p in into(l, &before) {
            let mut way = vec![p];
            way.extend_from_slice(&before);
            queue.push_back((way, start_back));
        }
    }
    best.map(|b| b.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::LaneBuilder;

    /// Straight lanes northwards (`lengths` long, one after the other from y 0), and lanes leading
    /// into the first from `side` (start, heading, length).
    fn road(lengths: &[f64], side: &[(DVec3, f64, f64)]) -> Network {
        let mut lanes = Vec::new();
        let mut y = 0.0;
        for &l in lengths {
            lanes.push(LaneBuilder::arc(DVec3::new(0.0, y, 0.0), 0.0, l, 0.0, 0.0, LaneKind::Street, 3.0));
            y += l;
        }
        for &(at, heading, l) in side {
            lanes.push(LaneBuilder::arc(at, heading, l, 0.0, 0.0, LaneKind::Street, 3.0));
        }
        let mut net = Network { lanes, ..Default::default() };
        net.link(1.5);
        net
    }

    /// Nobody sees a bus appear south of `y`.
    fn seen_south_of(net: &Network, y: f64) -> impl FnMut(usize, f32) -> Option<bool> + '_ {
        move |l, s| Some(net.lanes[l].at(s).0.y < y)
    }

    fn y(net: &Network, a: &Approach, route: &[usize]) -> f64 {
        let lane = a.before.first().copied().unwrap_or(route[a.index]);
        net.lanes[lane].at(a.s).0.y
    }

    #[test]
    fn it_goes_back_along_its_route_and_the_street_before_it_until_nobody_sees_it() {
        // a road of three 200 m lanes; his stop on the last, 50 m into it (y 450): the bus is seen
        // within 300 m of it
        let net = road(&[200.0, 200.0, 200.0], &[]);
        // the route has the whole road: back along it
        let route = [0, 1, 2];
        let a = relief_approach(&net, &route, 2, 50.0, seen_south_of(&net, 150.0)).unwrap();
        assert!(a.hidden && a.before.is_empty() && a.index == 0, "{a:?}");
        let at = y(&net, &a, &route);
        assert!((137.5..150.0).contains(&at), "the first place nobody sees: {at}");
        assert!((a.back as f64 - (450.0 - at)).abs() < 0.5, "{a:?}");
        // the route begins at the stop (the trip's first): back along the lanes that lead into it
        let route = [2];
        let a = relief_approach(&net, &route, 0, 50.0, seen_south_of(&net, 150.0)).unwrap();
        assert_eq!(a.before, vec![0, 1]);
        assert!(a.hidden && a.index == 0);
        assert!((137.5..150.0).contains(&y(&net, &a, &route)));
    }

    #[test]
    fn where_streets_meet_it_comes_the_straight_way_and_not_through_a_junction_or_a_car() {
        // two lanes into the stop's: a side street coming in from the south-east at an angle, and
        // the road straight on
        let side = 150.0 * DVec3::new(-(330f64.to_radians().sin()), -(330f64.to_radians().cos()), 0.0);
        let net = road(&[200.0, 200.0], &[(DVec3::new(0.0, 200.0, 0.0) + side, 330.0, 150.0)]);
        assert!(net.prev[1].contains(&0) && net.prev[1].contains(&2), "{:?}", net.prev[1]);
        let route = [1];
        let a = relief_approach(&net, &route, 0, 10.0, seen_south_of(&net, 100.0)).unwrap();
        assert_eq!(a.before, vec![0], "the straight way");
        // nowhere to stand on the first 150 m back (a queue, unloaded ground): further back
        let a = relief_approach(&net, &route, 0, 10.0, |l, s| (net.lanes[l].at(s).0.y < 60.0).then_some(false)).unwrap();
        assert!(!a.hidden && y(&net, &a, &route) < 60.0);
    }

    /// (31 October 2026) The bus a player gave back: where it stands, a little before its stop,
    /// past it, back on the street before the route or a side street into it - not off the lane, nor
    /// turned the other way, nor too far.
    #[test]
    fn a_bus_given_back_is_found_where_it_stands_on_the_way_up_to_its_stop() {
        // the stop 50 m into the last of three 200 m lanes (y 450); a side street into the middle one
        let side = 150.0 * DVec3::new(-(330f64.to_radians().sin()), -(330f64.to_radians().cos()), 0.0);
        let net = road(&[200.0, 200.0, 200.0], &[(DVec3::new(0.0, 200.0, 0.0) + side, 330.0, 150.0)]);
        let route = [2];
        let at = |x: f64, y: f64, h: f64| stood_on(&net, &route, 0, 50.0, DVec3::new(x, y, 0.0), h);
        let a = at(0.3, 420.0, 2.0).unwrap();
        assert!(a.before.is_empty() && a.index == 0 && (a.s - 20.0).abs() < 0.5 && (a.back - 30.0).abs() < 0.5, "{a:?}");
        let a = at(0.0, 470.0, 0.0).unwrap();
        assert!((a.s - 70.0).abs() < 0.5 && (a.back + 20.0).abs() < 0.5, "past its stop: {a:?}");
        let a = at(0.0, 300.0, 0.0).unwrap();
        assert!(a.before == vec![1] && (a.s - 100.0).abs() < 0.5 && (a.back - 150.0).abs() < 0.5, "{a:?}");
        // on the side street, near its end (the stop 10 m into the last lane: 230 m back)
        let p = net.lanes[3].at(130.0);
        let a = stood_on(&net, &route, 0, 10.0, p.0, p.1 as f64).unwrap();
        assert!(a.before == vec![3, 1] && (a.s - 130.0).abs() < 0.5 && (a.back - 230.0).abs() < 0.5, "{a:?}");
        assert_eq!(at(0.0, 520.0, 0.0), None, "too far past it");
        assert_eq!(at(0.0, 150.0, 0.0), None, "too far back");
        assert_eq!(at(8.0, 400.0, 0.0), None, "off the road");
        assert_eq!(at(0.0, 420.0, 180.0), None, "the other way");
        assert_eq!(at(f64::NAN, 420.0, 0.0), None);
        // along the route: back along it, and on to its next lane
        let route = [0, 1, 2];
        let a = stood_on(&net, &route, 1, 190.0, DVec3::new(0.0, 205.0, 0.0), 0.0).unwrap();
        assert!(a.before.is_empty() && a.index == 1 && (a.s - 5.0).abs() < 0.5 && (a.back - 185.0).abs() < 0.5, "{a:?}");
        let a = stood_on(&net, &route, 1, 190.0, DVec3::new(0.0, 410.0, 0.0), 0.0).unwrap();
        assert!(a.index == 2 && (a.s - 10.0).abs() < 0.5 && (a.back + 20.0).abs() < 0.5, "{a:?}");
    }

    #[test]
    fn seen_everywhere_it_is_put_out_as_far_back_as_it_goes_and_never_round_in_a_circle() {
        // seen everywhere: the furthest back, at most `APPROACH_MOST`
        let net = road(&[400.0, 400.0, 400.0], &[]);
        let route = [2];
        let a = relief_approach(&net, &route, 0, 100.0, |_, _| Some(false)).unwrap();
        assert!(!a.hidden && a.back <= APPROACH_MOST && a.back > APPROACH_MOST - APPROACH_STEP, "{a:?}");
        // the network ends 300 m back: there
        let net = road(&[200.0, 200.0], &[]);
        let a = relief_approach(&net, &[1], 0, 100.0, |_, _| Some(false)).unwrap();
        assert!(!a.hidden && a.before == vec![0] && a.s < APPROACH_STEP, "{a:?}");
        // a ring road (the lanes lead into each other): once round at most
        let mut lanes = Vec::new();
        let (mut at, mut heading) = (DVec3::ZERO, 0.0);
        for _ in 0..4 {
            let l = LaneBuilder::arc(at, heading, 50.0 * std::f64::consts::FRAC_PI_2, 50.0, 0.0, LaneKind::Street, 3.0);
            (at, heading) = (l.end(), l.end_heading() as f64);
            lanes.push(l);
        }
        let mut net = Network { lanes, ..Default::default() };
        net.link(1.5);
        let a = relief_approach(&net, &[0], 0, 25.0, |_, _| Some(false)).unwrap();
        assert!(a.back < 25.0 + 3.0 * 78.6 && a.before == vec![1, 2, 3], "{a:?}");
        // none at all behind it
        assert_eq!(relief_approach(&net, &[], 0, 0.0, |_, _| Some(true)), None);
        assert_eq!(relief_approach(&road(&[200.0], &[]), &[0], 0, 5.0, |_, _| Some(true)), None);
    }
}
