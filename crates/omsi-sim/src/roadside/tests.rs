use super::*;
use crate::traffic::{LaneBuilder, LaneKey, Network};

/// The tests that hold the shows, the lights and the models standing (statics) one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

/// A straight road lane of spline `id` on tile (3, -2), 120 m north from x, its limit `limit`.
fn lane(id: i64, x: f64, reversed: bool, limit: f32) -> Lane {
    let pts: Vec<DVec3> = (0..=40).map(|k| DVec3::new(x, k as f64 * 3.0, 10.0)).collect();
    let pts = if reversed { pts.into_iter().rev().collect() } else { pts };
    let mut l = LaneBuilder::polyline(pts, LaneKind::Street, 3.5);
    l.key = Some(LaneKey { tile: (3, -2), id, path: 0 });
    l.source = 1;
    l.reversed = reversed;
    l.speed_limit_kmh = limit;
    l
}

/// The places of a road come out the same on every drive, for each lane of one direction, and
/// as often as the shares say.
#[test]
fn a_roads_places_are_its_own() {
    let mut speed = 0;
    let mut display = 0;
    let n = 20_000;
    for id in 0..n {
        let a = site(&lane(id, 0.0, false, 50.0));
        // (another lane of the same direction: a second path of the spline, beside it)
        let mut b = lane(id, 3.5, false, 50.0);
        b.key = Some(LaneKey { tile: (3, -2), id, path: 1 });
        assert_eq!(a, site(&b), "{id}");
        assert_eq!(a, site(&lane(id, 0.0, false, 50.0)), "the same again");
        match a.map(|s| s.kind) {
            Some(Kind::Speed) => speed += 1,
            Some(Kind::Display) => display += 1,
            _ => {}
        }
        if let Some(s) = a {
            assert!(s.along >= 20.0 && s.along + 5.0 <= 120.0, "{s:?}");
        }
    }
    let share = |k: usize| k as f64 / n as f64;
    assert!((share(speed) - 0.025).abs() < 0.006, "{}", share(speed));
    assert!((share(display) - 0.03 * 0.975).abs() < 0.007, "{}", share(display));
    // fast roads have more cameras and no displays; a road closed to cars, an editor-only one
    // and a crossing's lane without a light none
    let fast = (0..n).filter(|id| site(&lane(*id, 0.0, false, 80.0)).is_some_and(|s| s.kind == Kind::Speed)).count();
    assert!((share(fast) - 0.12).abs() < 0.01, "{}", share(fast));
    assert!((0..n).all(|id| site(&lane(id, 0.0, false, 80.0)).is_none_or(|s| s.kind == Kind::Speed)));
    let closed: Vec<Lane> = (0..2000).map(|id| {
        let mut l = lane(id, 0.0, false, 80.0);
        l.no_cars = true;
        l
    }).collect();
    assert!(closed.iter().all(|l| site(l).is_none()));
    let crossing: Vec<Lane> = (0..2000).map(|id| {
        let mut l = lane(id, 0.0, false, 50.0);
        l.source = 2;
        l
    }).collect();
    assert!(crossing.iter().all(|l| site(l).is_none()));
    // the other direction of a road is a place of its own
    let both = (0..n).filter(|id| site(&lane(*id, 0.0, false, 80.0)).is_some() && site(&lane(*id, 0.0, true, 80.0)).is_some()).count();
    assert!(share(both) < 0.03, "{}", share(both));
}

/// A traffic light's approach: about one in five has a camera, at its stop line, the same for
/// every lane the light holds.
#[test]
fn a_lights_camera_stands_at_its_stop_line() {
    let mut cams = 0;
    for id in 0..5000 {
        let mut l = lane(id, 0.0, false, 50.0);
        l.source = 2;
        l.traffic_light = Some((7, 2));
        let mut m = lane(id, 3.5, false, 50.0);
        m.source = 2;
        m.key = Some(LaneKey { tile: (3, -2), id, path: 3 });
        m.traffic_light = Some((7, 2));
        let (a, b) = (site(&l), site(&m));
        assert_eq!(a, b);
        assert_eq!(group(&l), group(&m));
        if let Some(s) = a {
            assert_eq!((s.kind, s.along), (Kind::RedLight, 0.5));
            cams += 1;
        }
    }
    assert!((cams as f64 / 5000.0 - 0.2).abs() < 0.03, "{cams}");
}

/// A model stands beside the outer edge of its lane (the right; the left where traffic keeps
/// left), turned to the traffic coming; its lens is on that side of it.
#[test]
fn a_model_stands_by_the_road_and_faces_the_traffic() {
    let l = lane(1, 0.0, false, 50.0);
    let s = Site { kind: Kind::Speed, along: 30.0, limit: 50.0, number: 0 };
    let sh = &MODELS[0];
    let star = &MODELS[2];
    let (foot, heading) = pose(&l, &s, sh, false);
    // (the lane runs north: the right is east)
    assert!((foot.x - (1.75 + 1.4)).abs() < 0.01 && (foot.y - 30.0).abs() < 0.01 && (foot.z - 10.0).abs() < 0.01, "{foot:?}");
    // its lens faces south, towards the traffic coming up the lane
    let lens_at = lens(sh, foot, heading);
    assert!(lens_at.y < foot.y && (lens_at.z - foot.z - 2.63).abs() < 0.01, "{lens_at:?} {heading}");
    let (foot2, heading2) = pose(&l, &s, star, false);
    assert_eq!(foot2, foot);
    assert!((heading2 - (heading + 180.0).rem_euclid(360.0)).abs() < 1e-9);
    assert!(lens(star, foot2, heading2).y < foot2.y, "a model facing its own forward is turned round");
    let (left, _) = pose(&l, &s, sh, true);
    assert!((left.x + 3.15).abs() < 0.01, "{left:?}");
    // the outermost lane of a direction: furthest right (left where traffic keeps left)
    let inner = lane(1, -3.5, false, 50.0);
    assert!(outside(&l, 30.0, false) > outside(&inner, 30.0, false));
    assert!(outside(&l, 30.0, true) < outside(&inner, 30.0, true));
    // a display hangs at its height, its pole behind it on the ground
    let d = &MODELS[5];
    let ds = Site { kind: Kind::Display, ..s };
    let (dfoot, dh) = pose(&l, &ds, d, false);
    assert!((dfoot.z - 10.0 - DISPLAY_HEIGHT as f64).abs() < 1e-6);
    let pole = pole_foot(dfoot, dh);
    assert!((pole.z - 10.0).abs() < 1e-6 && pole.y > dfoot.y, "the pole on the display's far side: {pole:?}");
}

/// The model of a place among those installed, mixed by the place's number; none installed:
/// none.
#[test]
fn the_models_are_mixed_among_those_installed() {
    let all = vec![true; MODELS.len()];
    let mut seen = std::collections::HashSet::new();
    for k in 0..200u64 {
        let s = Site { kind: Kind::Speed, along: 30.0, limit: 50.0, number: k.wrapping_mul(0x9e37_79b9_7f4a_7c15) };
        let m = model_of(&s, &all).unwrap();
        assert_eq!(m.kind, Kind::Speed);
        seen.insert(m.file);
    }
    assert_eq!(seen.len(), 3, "every speed camera model comes up");
    let only_star: Vec<bool> = MODELS.iter().map(|m| m.file.ends_with("Blitzer_Star.sco")).collect();
    for k in 0..50u64 {
        let s = Site { kind: Kind::Speed, along: 30.0, limit: 50.0, number: k << 33 };
        assert!(model_of(&s, &only_star).unwrap().file.ends_with("Blitzer_Star.sco"));
        assert!(model_of(&Site { kind: Kind::Display, ..s }, &only_star).is_none());
    }
    assert!(model_of(&Site { kind: Kind::RedLight, along: 0.5, limit: 50.0, number: 1 }, &[]).is_none());
}

/// The placed objects' ids lie below every map's, and say their kind; their scripts read 12
/// (dark) idle, the camera's flash two short zeros, the red-light camera's trigger 8 with a
/// vehicle coming, the display's two loops one and two, five metres apart at the speed.
#[test]
fn the_objects_scripts_are_driven_through_their_light_phase() {
    let _one = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let s = Site { kind: Kind::Speed, along: 30.0, limit: 50.0, number: 0xdead_beef_1234_5678 };
    let id = object_id(&s, 0);
    assert!(id < -(1 << 59));
    assert_eq!(kind_of_id(id), Some(Kind::Speed));
    assert_eq!(kind_of_id(object_id(&Site { kind: Kind::Display, ..s }, 2)), Some(Kind::Display));
    assert_eq!(kind_of_id(object_id(&Site { kind: Kind::RedLight, ..s }, 0)), Some(Kind::RedLight));
    assert_ne!(object_id(&s, 0), object_id(&s, 1));
    for map_id in [0, 5_802_525, -1, i64::MIN, i64::MAX] {
        assert_eq!(kind_of_id(map_id), None);
        assert_eq!(phase_of(map_id, 100.0), None);
    }
    assert_eq!(phase_of(id, 100.0), Some((12.0, 0.0)));
    flash(id, 100.0);
    assert_eq!(phase_of(id, 100.05), Some((0.0, 0.0)));
    assert_eq!(phase_of(id, 100.2), Some((12.0, 0.0)));
    assert_eq!(phase_of(id, 100.45), Some((0.0, 0.0)));
    assert_eq!(phase_of(id, 102.0), Some((12.0, 0.0)), "over");
    assert_eq!(phase_of(id, 100.45), Some((12.0, 0.0)), "forgotten once over");
    // round midnight
    flash(id, 86_399.95);
    assert_eq!(phase_of(id, 0.02), Some((0.0, 0.0)));
    let red = object_id(&Site { kind: Kind::RedLight, ..s }, 0);
    flash(red, 50.0);
    assert_eq!(phase_of(red, 50.1), Some((8.0, 1.0)));
    assert_eq!(phase_of(red, 50.5), Some((12.0, 0.0)));
    let display = object_id(&Site { kind: Kind::Display, ..s }, 0);
    assert_eq!(phase_of(display, 10.0), Some((0.0, 0.0)));
    measure(display, 10.0, 10.0);
    assert_eq!(phase_of(display, 10.2), Some((1.0, 0.0)));
    assert_eq!(phase_of(display, 10.6), Some((2.0, 0.0)), "the second loop 0.5 s later at 10 m/s");
    assert_eq!(phase_of(display, 12.0), Some((0.0, 0.0)));
    reset();
}

/// The display's pole: an object of the game's own, a closed tube.
#[test]
fn the_displays_pole_is_a_tube() {
    let files = pole_files();
    assert_eq!(files.len(), 3);
    let obj = &files[1].1;
    assert_eq!(obj.lines().filter(|l| l.starts_with("v ")).count(), 25);
    assert_eq!(obj.lines().filter(|l| l.starts_with("f ")).count(), 12 * 3 * 2);
    assert!(files[0].1.contains("GAA_Mast.obj") && files[0].0.ends_with("GAA_Mast.sco") && POLE == files[0].0);
    // every face names vertices and normals there are
    for l in obj.lines().filter(|l| l.starts_with("f ")) {
        for c in l.split_whitespace().skip(1) {
            let (v, n) = c.split_once("//").unwrap();
            assert!((1..=25).contains(&v.parse::<usize>().unwrap()) && (1..=13).contains(&n.parse::<usize>().unwrap()), "{l}");
        }
    }
}

/// One light may hold the ways in from two sides (straight on from both): each its own
/// group, stop line and camera.
#[test]
fn a_light_of_two_ways_in_has_a_camera_for_each() {
    let way = |heading_north: bool, path: u16| {
        let pts: Vec<DVec3> = (0..=10).map(|k| DVec3::new(0.0, if heading_north { k as f64 * 3.0 } else { 30.0 - k as f64 * 3.0 }, 10.0)).collect();
        let mut l = LaneBuilder::polyline(pts, LaneKind::Street, 3.0);
        l.key = Some(LaneKey { tile: (3, -2), id: 9, path });
        l.source = 2;
        l.traffic_light = Some((0, 1));
        l
    };
    let (north, south, beside) = (way(true, 0), way(false, 1), way(true, 2));
    assert_ne!(group(&north), group(&south));
    assert_eq!(group(&north), group(&beside), "two lanes of one way in");
}

/// A camera counts as the company's fines do: the tolerance off, whole km/h over the limit.
#[test]
fn a_camera_counts_as_the_fines_do() {
    assert_eq!(over_limit(56.0, 50.0), Some(3.0));
    assert_eq!(over_limit(53.9, 50.0), None, "within the tolerance");
    assert_eq!(over_limit(54.0, 50.0), Some(1.0));
    assert_eq!(over_limit(110.0, 100.0), Some(6.0), "3 % above 100 km/h: 4 km/h off");
    assert_eq!(over_limit(f32::NAN, 50.0), None);
    assert_eq!(over_limit(80.0, 0.0), None, "no limit, no fine");
}

/// A camera standing flashes: its own lamp through its script, the others a light at the lens;
/// one that does not stand (unseen, or not installed) does not.
#[test]
fn a_camera_standing_flashes() {
    let _one = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    reset();
    let s = Site { kind: Kind::Speed, along: 30.0, limit: 50.0, number: 77 };
    assert!(!trigger(&s, 10.0));
    assert!(lights_now().is_empty());
    let lens_at = DVec3::new(1.0, 2.0, 3.0);
    place(object_id(&s, 0), lens_at, Flash::Light);
    assert!(trigger(&s, 10.0));
    assert_eq!(lights_now(), vec![(lens_at, 1.0)]);
    assert_eq!(phase_of(object_id(&s, 0), 10.05), Some((0.0, 0.0)));
    let own = Site { number: 78, ..s };
    place(object_id(&own, 0), lens_at, Flash::Script);
    reset();
    assert!(!trigger(&own, 10.0), "forgotten with the session");
    place(object_id(&own, 0), lens_at, Flash::Script);
    assert!(trigger(&own, 10.0));
    assert!(lights_now().is_empty(), "its own lamp, no light of the game's");
    reset();
}

/// The bus driving a road with a camera passes it once, on whichever lane of its direction,
/// changing lanes on the way or not; starting beyond it, or driving the other way, it passes
/// nothing. A display's point is 30 m before it.
#[test]
fn the_bus_passes_the_camera_of_its_road() {
    let id = (0..100_000)
        .find(|id| site(&lane(*id, 0.0, false, 80.0)).is_some_and(|s| s.kind == Kind::Speed) && site(&lane(*id, -3.5, true, 80.0)).is_none())
        .unwrap();
    let a = lane(id, 0.0, false, 80.0);
    let mut b = lane(id, 3.5, false, 80.0);
    b.key = Some(LaneKey { tile: (3, -2), id, path: 1 });
    let back = lane(id, -3.5, true, 80.0);
    let at = site(&a).unwrap().along as f64;
    let mut net = Network::default();
    net.extend(vec![a, b, back], 1.5);
    let drive = |w: &mut Watch, path: &[(f64, f64)], heading: f64| -> Vec<Passed> { path.iter().filter_map(|(x, y)| w.step(&net, DVec3::new(*x, *y, 10.0), heading)).collect() };
    let north: Vec<(f64, f64)> = (0..55).map(|k| (0.0, k as f64 * 2.0)).collect();
    let mut w = Watch::default();
    let p = drive(&mut w, &north, 0.0);
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!((p[0].site.kind, p[0].lane), (Kind::Speed, 0));
    // over to the other lane just before it
    w.forget();
    let across: Vec<(f64, f64)> = (0..55).map(|k| k as f64 * 2.0).map(|y| (if y < at - 1.0 { 0.0 } else { 3.5 }, y)).collect();
    let p = drive(&mut w, &across, 0.0);
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].lane, 1);
    // coming onto the road past it
    w.forget();
    let late: Vec<(f64, f64)> = north.iter().copied().filter(|p| p.1 > at + 0.5).collect();
    assert!(drive(&mut w, &late, 0.0).is_empty());
    // the other way
    w.forget();
    let south: Vec<(f64, f64)> = (0..55).map(|k| (-3.5, 118.0 - k as f64 * 2.0)).collect();
    assert!(drive(&mut w, &south, 180.0).is_empty());
    // its bus lane, closed to cars: measured by the road's camera all the same
    let mut bus_lane = lane(id, 7.0, false, 80.0);
    bus_lane.key = Some(LaneKey { tile: (3, -2), id, path: 2 });
    bus_lane.no_cars = true;
    assert!(site(&bus_lane).is_none());
    let mut net2 = Network::default();
    net2.extend(vec![lane(id, 0.0, false, 80.0), bus_lane], 1.5);
    let mut w = Watch::default();
    let in_bus_lane: Vec<(f64, f64)> = (0..55).map(|k| (7.0, k as f64 * 2.0)).collect();
    let p: Vec<Passed> = in_bus_lane.iter().filter_map(|(x, y)| w.step(&net2, DVec3::new(*x, *y, 10.0), 0.0)).collect();
    assert_eq!(p.len(), 1, "{p:?}");
    assert_eq!((p[0].site.kind, p[0].lane), (Kind::Speed, 1));
    // a road in town with a display: passed where it measures
    let id = (0..100_000).find(|id| site(&lane(*id, 0.0, false, 50.0)).is_some_and(|s| s.kind == Kind::Display)).unwrap();
    let d = lane(id, 0.0, false, 50.0);
    let at = site(&d).unwrap().along as f64;
    let mut net = Network::default();
    net.extend(vec![d], 1.5);
    let mut w = Watch::default();
    let p: Vec<(f64, Passed)> = north.iter().filter_map(|(x, y)| w.step(&net, DVec3::new(*x, *y, 10.0), 0.0).map(|p| (*y, p))).collect();
    assert_eq!(p.len(), 1);
    assert_eq!(p[0].1.site.kind, Kind::Display);
    assert!(p[0].0 >= at - DISPLAY_MEASURE as f64 && p[0].0 < at - DISPLAY_MEASURE as f64 + 2.0, "{} {at}", p[0].0);
}

/// Onto a light's lanes from the road before it: its camera is passed at the stop line, once
/// for the crossing even where two of its lights' lanes begin together.
#[test]
fn the_bus_passes_a_lights_camera_at_its_stop_line() {
    let crossing = |id: i64, x: f64, light: usize, path: u16| {
        let pts: Vec<DVec3> = (0..=10).map(|k| DVec3::new(x, 120.0 + k as f64 * 3.0, 10.0)).collect();
        let mut l = LaneBuilder::polyline(pts, LaneKind::Street, 3.0);
        l.key = Some(LaneKey { tile: (3, -2), id, path });
        l.source = 2;
        l.traffic_light = Some((0, light));
        l
    };
    let id = (0..100_000)
        .find(|id| site(&crossing(*id, 0.0, 2, 0)).is_some() && site(&crossing(*id, 0.4, 3, 1)).is_some())
        .unwrap();
    let road = lane(5, 0.0, false, 50.0);
    let mut net = Network::default();
    net.extend(vec![road, crossing(id, 0.0, 2, 0), crossing(id, 0.4, 3, 1)], 1.5);
    let mut w = Watch::default();
    // (on the crossing the bus is seen on one light's lane, then on the other's)
    let path: Vec<(f64, f64)> = (0..30).map(|k| (if k % 2 == 0 { -0.1 } else { 0.5 }, 100.0 + k as f64)).collect();
    let p: Vec<Passed> = path.iter().filter_map(|(x, y)| w.step(&net, DVec3::new(*x, *y, 10.0), 0.0)).collect();
    assert_eq!(p.len(), 1, "{p:?}");
    assert!(p[0].site.kind == Kind::RedLight && [1, 2].contains(&p[0].lane), "{p:?}");
}
