//! (NexiconDK, 1 November 2026) The duty menu's "Nearby" tab: the tours of the timetable a
//! trip of which starts near the bus - within the distance chosen, leaving within the next
//! two hours (as the lines list has them), the nearest first - to take one on where it starts
//! rather than look through every line for one.
//!
//! A tour is listed once, at the start of its trip that suits best: one the bus can reach in
//! time before one it cannot, then the nearest, then the soonest. The distance is in a
//! straight line from the bus (the stops are where the navigator's reading of the whole map
//! has them); the tour chosen shows its way there on the roads.

use super::*;
use glam::{DVec3, Vec2};

/// How fast a bus gets through town to a stop, on average (m/s), and how much further the
/// roads are than the straight line: for "leaves before you can be there".
const CITY_SPEED: f64 = 25.0 / 3.6;
const DETOUR: f64 = 1.35;
/// ... and the time to pull out and draw up at the stop (s).
const PULL_OUT: f64 = 30.0;
/// The distance chosen when none was (`Settings::nearby_radius`: an index into `radii`).
pub(crate) const DEFAULT_RADIUS: usize = 1;

const METRES_PER_MILE: f64 = 1609.344;

/// The distances to choose from (m): 1, 3, 5 and 10 km, or with miles ½, 2, 3 and 6.
pub(crate) fn radii() -> [f64; 4] {
    radii_in(crate::units::Units::now())
}

fn radii_in(units: crate::units::Units) -> [f64; 4] {
    match units {
        crate::units::Units::Metric => [1000.0, 3000.0, 5000.0, 10_000.0],
        _ => [0.5, 2.0, 3.0, 6.0].map(|m| m * METRES_PER_MILE),
    }
}

/// A distance of `radii` as its button says it: "3 km", "0.5 mi".
pub(crate) fn radius_text(m: f64) -> String {
    radius_text_in(crate::units::Units::now(), m)
}

fn radius_text_in(units: crate::units::Units, m: f64) -> String {
    match units {
        crate::units::Units::Metric => format!("{:.0} km", m / 1000.0),
        _ if m < METRES_PER_MILE * 0.99 => format!("{:.1} mi", m / METRES_PER_MILE),
        _ => format!("{:.0} mi", m / METRES_PER_MILE),
    }
}

/// About how many minutes the bus takes to drive `road` m through town (one at least).
pub(crate) fn drive_minutes(road: f64) -> u32 {
    ((road / CITY_SPEED + PULL_OUT) / 60.0).ceil().max(1.0) as u32
}

/// The distance chosen, as an index into `radii`.
pub(crate) fn radius_of(app: &App) -> usize {
    (app.settings.nearby_radius as usize).min(3)
}

/// A trip of a tour the lines list shows, leaving within the next two hours: where it starts.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Start {
    pub line: String,
    pub tour: String,
    /// Its place among the tour's trips in the order they leave (`Schedule::tour_trip_stops`),
    /// and how many the tour has.
    pub trip: usize,
    pub trips: usize,
    /// The line its bus shows.
    pub sign: String,
    pub stop: String,
    /// Its last stop.
    pub terminus: String,
    /// The stop's object (the navigator's `places` key).
    pub station: i64,
    /// When it leaves its first stop (s of the timetable's day), and how long from now that is.
    pub dep: f64,
    pub until: f64,
}

/// A tour near the bus: the start of its trip that suits best, where that is, how far from the
/// bus in a straight line (m), which way (degrees clockwise from straight ahead), and whether
/// it leaves sooner than the bus can be there.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Near {
    pub start: Start,
    pub at: DVec3,
    pub dist: f64,
    pub angle: f64,
    pub tight: bool,
}

/// The tours near the bus as the tab shows them, made when it opens (`App::open_list`): those
/// within the distance chosen, how many more start within the next one out, and why there are
/// none to look through (no timetable, no bus, the map not read yet).
#[derive(Debug, Clone, Default)]
pub(crate) struct NearList {
    pub rows: Vec<Near>,
    pub more: usize,
    pub radius: usize,
    pub why: Option<String>,
}

/// Whether a trip leaving in `until` s, `dist` m away in a straight line, leaves before the bus
/// can be there.
pub(crate) fn tight(dist: f64, until: f64) -> bool {
    until < dist * DETOUR / CITY_SPEED + PULL_OUT
}

/// Where `at` lies from a bus at `bus` heading `heading` (degrees clockwise from north): degrees
/// clockwise from straight ahead, -180 to 180.
pub(crate) fn angle_from(bus: DVec3, heading: f64, at: DVec3) -> f64 {
    let d = at - bus;
    let bearing = d.x.atan2(d.y).to_degrees();
    (bearing - heading + 180.0).rem_euclid(360.0) - 180.0
}

/// The trips of the tours listed now (`tour_listed`) that leave within the next two hours (a
/// minute gone at most), with where each starts.
pub(crate) fn starts(sch: &crate::schedule::Schedule, now: f64) -> Vec<Start> {
    let mut out = Vec::new();
    for l in sch.data.lines.iter().filter(|l| l.user_allowed) {
        for t in &l.tours {
            if !tour_listed(sch, &l.name, t, now) {
                continue;
            }
            let all = sch.tour_stops(&l.name, &t.number);
            // (the trips in the order they leave, as `tour_trip_stops` counts them)
            let mut order: Vec<(usize, f64)> = Vec::new();
            for s in &all {
                if !order.iter().any(|o| o.0 == s.0) {
                    order.push((s.0, s.3));
                }
            }
            order.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
            for (pos, &(k, _)) in order.iter().enumerate() {
                let calls: Vec<&(usize, usize, String, f64)> = all.iter().filter(|s| s.0 == k).collect();
                let (Some(first), Some(last)) = (calls.first(), calls.last()) else { continue };
                // (a night tour's times go on past 24:00: in the early hours the day before's)
                let today = sch.tour_runs_today(t).then(|| first.3 - now);
                let yesterday = sch.tour_runs_yesterday(t).then(|| first.3 - 86400.0 - now);
                let Some(until) = [today, yesterday].into_iter().flatten().filter(|u| (-60.0..=LIST_AHEAD).contains(u)).min_by(|a, b| a.abs().total_cmp(&b.abs())) else { continue };
                let Some(name) = tour_trip_name(sch, t, k) else { continue };
                let Some(tt) = sch.data.trips.iter().find(|x| x.name.eq_ignore_ascii_case(&name)) else { continue };
                let Some(&station) = omsi_sim::timetable_run::trip_stations(tt).get(first.1) else { continue };
                let sign = Some(tt.line.trim().to_string()).filter(|s| !s.is_empty()).unwrap_or_else(|| line_sign(Some(sch), l));
                out.push(Start { line: l.name.clone(), tour: t.number.clone(), trip: pos, trips: order.len(), sign, stop: first.2.trim().to_string(), terminus: last.2.trim().to_string(), station, dep: first.3, until });
            }
        }
    }
    out
}

/// The tours a trip of which starts within `radius` m of the bus (`starts`, the stops where
/// `places` has them), each at the start that suits best - one the bus can reach in time,
/// then the nearest, then the soonest - nearest first.
pub(crate) fn nearest(starts: &[Start], places: &hashbrown::HashMap<i64, DVec3>, bus: DVec3, heading: f64, radius: f64) -> Vec<Near> {
    let key = |n: &Near| (n.tight, n.dist, n.start.until);
    let better = |a: &Near, b: &Near| {
        let (ka, kb) = (key(a), key(b));
        ka.0.cmp(&kb.0).then(ka.1.total_cmp(&kb.1)).then(ka.2.total_cmp(&kb.2)).is_lt()
    };
    let mut out: Vec<Near> = Vec::new();
    for s in starts {
        let Some(&at) = places.get(&s.station) else { continue };
        let dist = (at - bus).truncate().length();
        if dist > radius {
            continue;
        }
        let n = Near { start: s.clone(), at, dist, angle: angle_from(bus, heading, at), tight: tight(dist, s.until) };
        match out.iter_mut().find(|o| o.start.line == s.line && o.start.tour == s.tour) {
            Some(o) if better(&n, o) => *o = n,
            Some(_) => {}
            None => out.push(n),
        }
    }
    out.sort_by(|a, b| a.dist.total_cmp(&b.dist).then(a.start.until.total_cmp(&b.start.until)).then(a.start.line.cmp(&b.start.line)).then(a.start.tour.cmp(&b.start.tour)));
    out
}

/// The bus (or the player on foot without one): where and which way (degrees clockwise from
/// north).
pub(crate) fn bus_pose(app: &App) -> Option<(DVec3, f64)> {
    match (app.player.as_ref(), app.session.on_foot.as_ref()) {
        (Some(p), _) => Some((p.vehicle.position, p.vehicle.heading)),
        (None, Some(f)) => Some((f.pos, f.heading)),
        (None, None) => None,
    }
}

/// The tours near the bus now, at the distance chosen.
pub(crate) fn near_list(app: &App) -> NearList {
    let tr = |t: &str| omsi_ui::tr(t).into_owned();
    let radius = radius_of(app);
    let why = |w: &str| NearList { radius, why: Some(tr(w)), ..Default::default() };
    let Some(sch) = crate::duty_book::timetable(app) else { return why("No timetable on this map") };
    let Some((bus, heading)) = bus_pose(app) else { return why("Put a bus on the map to see the tours that start near it") };
    let Some(places) = app.menus.navigator.as_ref().and_then(|n| n.places()).filter(|p| !p.is_empty()) else { return why("The distances come once the navigator has read the map") };
    let all = starts(sch, app.clock.time);
    let r = radii();
    let rows = nearest(&all, places, bus, heading, r[radius]);
    let more = r.get(radius + 1).map(|&next| nearest(&all, places, bus, heading, next).len().saturating_sub(rows.len())).unwrap_or(0);
    NearList { rows, more, radius, why: None }
}

/// "in 12 min", "now".
fn until_text(until: f64) -> String {
    if until < 60.0 {
        omsi_ui::tr("now").into_owned()
    } else {
        omsi_ui::tr("in %{min} min").replace("%{min}", &format!("{:.0}", (until / 60.0).floor()))
    }
}

/// "21:00" of a time of the timetable's day (s).
fn clock(s: f64) -> String {
    format!("{:02}:{:02}", (s / 3600.0) as i64 % 24, ((s % 3600.0) / 60.0) as i64)
}

/// The lines of the tab: a tour each (`near {line}\u{1}{tour}\u{1}{trip}`), or why there is
/// none.
pub(crate) fn items(app: &App) -> Vec<(String, String)> {
    let made;
    let list = match app.menus.near.as_deref() {
        Some(l) => l,
        None => {
            made = near_list(app);
            &made
        }
    };
    let dist = |m: f64| crate::units::Units::now().distance(m);
    let mut out: Vec<(String, String)> = list
        .rows
        .iter()
        .map(|n| {
            let s = &n.start;
            (format!("{}  {}  {}  {}", s.sign, s.stop, clock(s.dep), dist(n.dist)), format!("near {}\u{1}{}\u{1}{}", s.line, s.tour, s.trip))
        })
        .collect();
    if out.is_empty() {
        out.push((empty_text(list), "noop".into()));
    }
    out
}

/// Why the tab lists nothing.
fn empty_text(list: &NearList) -> String {
    list.why.clone().unwrap_or_else(|| omsi_ui::tr("No tour starts within %{dist} of your bus in the next two hours.").replace("%{dist}", &radius_text(radii()[list.radius])))
}

/// The tab as the pause menu draws it (`ui::NearView`).
pub(crate) fn view(app: &App) -> crate::ui::NearView {
    let tr = |t: &str| omsi_ui::tr(t).into_owned();
    let made;
    let list = match app.menus.near.as_deref() {
        Some(l) => l,
        None => {
            made = near_list(app);
            &made
        }
    };
    let r = radii();
    let dist = |m: f64| crate::units::Units::now().distance(m);
    let rows = list
        .rows
        .iter()
        .map(|n| {
            let s = &n.start;
            crate::ui::NearRow {
                sign: s.sign.clone(),
                stop: s.stop.clone(),
                detail: format!("{} {}  ›  {}", tr("Tour"), s.tour.trim(), s.terminus),
                time: clock(s.dep),
                until: until_text(s.until),
                dist: dist(n.dist),
                angle: n.angle as f32,
                tight: n.tight,
            }
        })
        .collect();
    let more = match (list.more, r.get(list.radius + 1)) {
        (0, _) | (_, None) => String::new(),
        (1, Some(&next)) => tr("1 more tour starts within %{dist}").replace("%{dist}", &radius_text(next)),
        (n, Some(&next)) => tr("%{count} more tours start within %{dist}").replace("%{count}", &n.to_string()).replace("%{dist}", &radius_text(next)),
    };
    crate::ui::NearView {
        radii: r.iter().map(|m| radius_text(*m)).collect(),
        radius: list.radius,
        title: tr("Tours starting near your bus"),
        hint: tr("Where each tour's next trips start, nearest first, leaving within two hours"),
        more,
        note: tr("Distances in a straight line from your bus"),
        empty: (list.rows.is_empty()).then(|| empty_text(list)),
        rows,
    }
}

/// The tour on line `k` of the tab: (line, tour, trip).
pub(crate) fn near_at(app: &App, k: usize) -> Option<(String, String, usize)> {
    let action = app.menus.admin_list.as_ref()?.get(k)?.1.strip_prefix("near ")?;
    let mut p = action.split('\u{1}');
    let (line, tour, trip) = (p.next()?, p.next()?, p.next()?.parse().ok()?);
    Some((line.to_string(), tour.to_string(), trip))
}

/// The tour on line `k` of the tab as the list has it.
fn near_row(app: &App, k: usize) -> Option<&Near> {
    let (line, tour, trip) = near_at(app, k)?;
    app.menus.near.as_deref()?.rows.iter().find(|n| n.start.line == line && n.start.tour == tour && n.start.trip == trip)
}

/// The tour chosen in the tab beside the list: its trip's ends, the tour's trips, how far it
/// starts from the bus, and the map of the way there (made once a tour, `near_map`).
pub(crate) fn preview(app: &mut App) -> Option<crate::ui::Preview> {
    let tr = |t: &str| omsi_ui::tr(t).into_owned();
    let k = app.menus.chooser?;
    let n = near_row(app, k)?.clone();
    let s = &n.start;
    let key = format!("{}\u{1}{}\u{1}{}", s.line, s.tour, s.trip);
    let map = match app.menus.near_map.as_ref().filter(|m| m.0 == key) {
        Some(m) => Some(m.1.clone()),
        None => {
            let made = (|| {
                let nav = app.menus.navigator.as_ref()?;
                let (net, places) = (nav.map_net()?, nav.places()?);
                let sch = crate::duty_book::timetable(app)?;
                let (bus, heading) = bus_pose(app)?;
                Some(std::sync::Arc::new(near_map(sch, net, places, &n, bus, heading)))
            })();
            if let Some(m) = made.as_ref() {
                app.menus.near_map = Some((key, m.clone()));
            }
            made
        }
    };
    let sch = crate::duty_book::timetable(app)?;
    let line = sch.data.lines.iter().find(|l| l.name == s.line)?;
    let tour = line.tours.iter().find(|t| t.number == s.tour)?;
    let stops = sch.tour_trip_stops(&s.line, &s.tour, s.trip);
    let dist = crate::units::Units::now().distance(n.dist);
    Some(crate::ui::Preview {
        title: format!("{} {}", tr("Tour"), s.tour.trim()),
        meta: format!("{} {}  |  {} {}/{}  |  {}", tr("Line"), s.sign, tr("Trip"), s.trip + 1, s.trips.max(1), tr("%{dist} from your bus").replace("%{dist}", &dist)),
        rows: stops.iter().map(|c| (c.2.trim().to_string(), clock(c.3))).collect(),
        ends: trip_ends_of(&stops),
        trips: tour_trip_rows(sch, &s.line, tour, &s.sign),
        trip: s.trip,
        button: Some(tr("Start trip")),
        map,
        ..Default::default()
    })
}

/// The way from the bus to the start of tour `n`'s trip on a map: the streets round both, the
/// way on the roads (the bus's lane on to the trip's first, the shortest), its length, the
/// stop and the bus.
pub(crate) fn near_map(sch: &crate::schedule::Schedule, net: &omsi_sim::traffic::Network, places: &hashbrown::HashMap<i64, DVec3>, n: &Near, bus: DVec3, heading: f64) -> crate::ui::RouteMap {
    use omsi_sim::traffic::LaneKind;
    let v2 = |p: DVec3| Vec2::new(p.x as f32, p.y as f32);
    let at = places.get(&n.start.station).copied().unwrap_or(n.at);
    // the bus's lane, and the lane the trip leaves the stop on (its way's first)
    let from = net.lane_along(bus, heading, LaneKind::Street, 8.0, 60.0).or_else(|| net.nearest_lane(bus, LaneKind::Street).filter(|l| l.2 < 30.0));
    let trip = sch.data.lines.iter().find(|l| l.name == n.start.line).and_then(|l| l.tours.iter().find(|t| t.number == n.start.tour)).and_then(|t| {
        // (`tour_stops`'s number of the trip in place `trip`)
        let k = sch.tour_trip_stops(&n.start.line, &n.start.tour, n.start.trip).first()?.0;
        tour_trip_name(sch, t, k)
    });
    let to = trip
        .and_then(|t| sch.trip_route_in(net, &t).first().copied())
        .map(|l| (l, net.lanes[l].nearest_point(at).map(|p| p.0).unwrap_or(0.0)))
        .or_else(|| net.nearest_lane(at, LaneKind::Street).filter(|l| l.2 < 40.0).map(|l| (l.0, l.1)));
    let mut route: Vec<Vec<Vec2>> = Vec::new();
    let mut road = None;
    if let (Some((a, sa, _)), Some((b, sb))) = (from, to) {
        if let Some(path) = net.shortest_path(a, b) {
            let lanes: Vec<&omsi_sim::traffic::Lane> = path.iter().filter_map(|&l| net.lanes.get(l)).collect();
            let len: f32 = lanes.iter().map(|l| l.length()).sum::<f32>() - sa - lanes.last().map(|l| l.length() - sb).unwrap_or(0.0);
            road = Some(len.max(0.0) as f64);
            for (i, l) in lanes.iter().enumerate() {
                // (the bus's lane from the bus on, the last up to the stop)
                let (s0, s1) = (if i == 0 { sa } else { 0.0 }, if i + 1 == lanes.len() { sb } else { l.length() });
                let pts: Vec<Vec2> = l.points.iter().zip(l.dist.iter()).filter(|(_, d)| **d >= s0 && **d <= s1).map(|(p, _)| v2(*p)).collect();
                let mut pts = pts;
                if i == 0 {
                    pts.insert(0, v2(l.at(sa).0));
                }
                if i + 1 == lanes.len() {
                    pts.push(v2(l.at(sb).0));
                }
                if pts.len() >= 2 {
                    route.push(pts);
                }
            }
        }
    }
    let (b, s) = (v2(bus), v2(at));
    let mut min = b.min(s);
    let mut max = b.max(s);
    for p in route.iter().flatten() {
        min = min.min(*p);
        max = max.max(*p);
    }
    // (the streets within a margin round both, each way of a two-way street once)
    let pad = ((max - min).max_element() * 0.15).max(200.0);
    let (lo, hi) = (min - Vec2::splat(pad), max + Vec2::splat(pad));
    let roads: Vec<Vec<Vec2>> = net
        .lanes
        .iter()
        .filter(|l| l.kind == LaneKind::Street && !l.reversed && l.points.len() >= 2)
        .filter(|l| l.points.iter().any(|p| (p.x as f32) > lo.x && (p.x as f32) < hi.x && (p.y as f32) > lo.y && (p.y as f32) < hi.y))
        .take(6000)
        .map(|l| l.points.iter().map(|p| v2(*p)).collect())
        .collect();
    let time = clock(n.start.dep);
    crate::ui::RouteMap { roads, route, stops: vec![(s, n.start.stop.clone(), time, 0)], min, max, bus: Some((b, heading as f32)), road }
}

/// The tours near the bus, kept for the tab's lines, its drawing and its clicks, and for the
/// count on its title (`App::open_list`, the lines and the tab).
pub(crate) fn remember(app: &mut App) {
    app.menus.near = Some(std::sync::Arc::new(near_list(app)));
    app.menus.near_map = None;
}

/// What the tab's clicks beside the list are (`shell::Action::Pane`): the trip started, the
/// stop shown on the city map, the tour's page opened, a distance chosen.
pub(crate) const PANE_START: usize = usize::MAX;
pub(crate) const PANE_MAP: usize = usize::MAX - 3;
pub(crate) const PANE_TOUR: usize = usize::MAX - 4;
pub(crate) const fn pane_radius(r: usize) -> usize {
    usize::MAX - 20 - r
}

/// A click beside the tab's list (or its keys): see `PANE_START` and the rest.
pub(crate) fn pane_click(app: &mut App, i: usize) {
    if (pane_radius(3)..=pane_radius(0)).contains(&i) {
        step_radius(app, usize::MAX - 20 - i);
        let k = app.menus.chooser.unwrap_or(0);
        let chosen = near_at(app, k);
        app.open_list(ListKind::Nearby);
        // (the same tour stays chosen where it is still listed)
        if let Some(c) = chosen {
            let action = format!("near {}\u{1}{}\u{1}{}", c.0, c.1, c.2);
            if let Some(k) = app.menus.admin_list.as_ref().and_then(|l| l.iter().position(|x| x.1 == action)) {
                app.menus.chooser = Some(k);
            }
        }
        return;
    }
    let Some(k) = app.menus.chooser else { return };
    let Some((line, tour, trip)) = near_at(app, k) else { return };
    match i {
        PANE_START => {
            start_duty_at(app, &line, &tour, trip, 0);
            app.menus.chooser = None;
            app.menus.admin_list = None;
            app.menus.list_kind = None;
            app.close_game_menu();
        }
        PANE_MAP => {
            let Some(at) = near_row(app, k).map(|n| n.at) else { return };
            app.close_list();
            app.close_game_menu();
            if let Some(n) = app.menus.navigator.as_mut() {
                n.show_place(at);
            }
        }
        PANE_TOUR => {
            app.menus.near_back = true;
            app.menus.menu_top = None;
            app.open_list(ListKind::Tours(line, Some((tour, 0, trip))));
        }
        _ => {}
    }
}

/// Distance `to` (an index into `radii`) chosen, kept for the next time.
pub(crate) fn step_radius(app: &mut App, to: usize) {
    let to = to.min(3);
    if to == radius_of(app) {
        return;
    }
    app.settings.nearby_radius = to as u8;
    remember_setting("nearby_radius", &to.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(tour: &str, trip: usize, station: i64, until: f64) -> Start {
        Start { line: "131".into(), tour: tour.into(), trip, trips: 4, sign: "131".into(), stop: format!("Stop {station}"), terminus: "End".into(), station, dep: 72000.0 + until, until }
    }

    /// Each tour once, at the start of its trip the bus can reach in time before a nearer one
    /// it cannot, the nearer of two it can, the sooner of two at one stop; none beyond the
    /// distance or of a stop the map has not; the nearest first.
    #[test]
    fn a_tour_is_listed_once_at_its_best_start_nearest_first() {
        let places: hashbrown::HashMap<i64, DVec3> = [(1, DVec3::new(0.0, 300.0, 0.0)), (2, DVec3::new(0.0, 2500.0, 0.0)), (3, DVec3::new(-800.0, 0.0, 0.0)), (4, DVec3::new(0.0, 9000.0, 0.0))].into_iter().collect();
        let starts = vec![
            // A: leaves the near stop in 20 s (too soon to get there), again an hour later
            start("A", 0, 1, 20.0),
            start("A", 2, 1, 3600.0),
            // B: the near stop too soon (in 10 s), the far one in good time
            start("B", 0, 1, 10.0),
            start("B", 1, 2, 1800.0),
            // C: to the left, 800 m
            start("C", 1, 3, 900.0),
            // D: too far, and a stop the map does not have
            start("D", 0, 4, 900.0),
            start("E", 0, 99, 900.0),
        ];
        let near = nearest(&starts, &places, DVec3::ZERO, 0.0, 3000.0);
        let got: Vec<(&str, usize)> = near.iter().map(|n| (n.start.tour.as_str(), n.start.trip)).collect();
        assert_eq!(got, [("A", 2), ("C", 1), ("B", 1)]);
        assert!(!near[0].tight && (near[0].dist - 300.0).abs() < 1e-6);
        assert!((near[1].angle + 90.0).abs() < 1e-6, "to the left: {}", near[1].angle);
        assert_eq!(nearest(&starts, &places, DVec3::ZERO, 0.0, 10_000.0).len(), 4);
        assert!(nearest(&starts, &places, DVec3::ZERO, 0.0, 200.0).is_empty());
    }

    /// Too soon to be there: a trip 2 km off leaving in four minutes; in good time in ten.
    #[test]
    fn a_trip_leaving_before_the_bus_can_be_there_is_tight() {
        assert!(tight(2000.0, 240.0));
        assert!(!tight(2000.0, 600.0));
        assert!(tight(0.0, 10.0) && !tight(0.0, 60.0));
    }

    /// Which way from the bus: ahead, right, behind, left - whichever way it faces.
    #[test]
    fn the_way_to_a_stop_is_from_the_buses_heading() {
        let a = |h: f64, x: f64, y: f64| angle_from(DVec3::ZERO, h, DVec3::new(x, y, 0.0));
        assert!(a(0.0, 0.0, 10.0).abs() < 1e-9);
        assert!((a(0.0, 10.0, 0.0) - 90.0).abs() < 1e-9);
        assert!((a(90.0, 10.0, 0.0)).abs() < 1e-9);
        assert!((a(90.0, 0.0, 10.0) + 90.0).abs() < 1e-9);
        assert!((a(0.0, 0.0, -10.0).abs() - 180.0).abs() < 1e-9);
    }

    /// The distances in kilometres, or with miles in miles.
    #[test]
    fn the_distances_to_choose_from_follow_the_units() {
        use crate::units::Units;
        for (units, texts) in [(Units::Metric, ["1 km", "3 km", "5 km", "10 km"]), (Units::British, ["0.5 mi", "2 mi", "3 mi", "6 mi"]), (Units::American, ["0.5 mi", "2 mi", "3 mi", "6 mi"])] {
            assert_eq!(radii_in(units).map(|m| radius_text_in(units, m)), texts);
        }
    }
}
