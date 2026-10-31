//! A player's bus that leaves people at their stop (`passed_by`).
use super::*;

/// A bus of two places with a door (the cabin files written for the test).
fn cabin(name: &str) -> Arc<Cabin> {
    let dir = std::env::temp_dir().join(format!("omsi-passed-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("test.bus"), "[boundingbox]\n2.5\n12\n3\n0\n0\n1.5\n\n[passengercabin]\ncabin.cfg\n\n[paths]\npaths.cfg\n").unwrap();
    std::fs::write(dir.join("paths.cfg"), "[pathpnt]\n1.1\n4\n0.4\n\n[pathpnt]\n0\n4\n0.5\n\n[pathlink]\n0\n1\n").unwrap();
    std::fs::write(dir.join("cabin.cfg"), "[entry]\n0\n\n[exit]\n0\n\n[passpos]\n-0.5\n0\n0.9\n0.45\n0\n\n[passpos]\n0.5\n0\n0.9\n0.45\n0\n").unwrap();
    let def = omsi_vehicle::Vehicle::load(&dir.join("test.bus")).unwrap();
    let c = Arc::new(Cabin::load_train(&[(&def, Vec3::ZERO, f32::INFINITY)]).expect("cabin"));
    std::fs::remove_dir_all(&dir).ok();
    c
}

fn human(name: &str) -> Arc<HumanType> {
    let dir = std::env::temp_dir().join(format!("omsi-passed-hum-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("person.hum"), "[model]\nmodel.cfg\n").unwrap();
    std::fs::write(dir.join("model.cfg"), "").unwrap();
    let ty = Arc::new(HumanType::load(&dir.join("person.hum")).unwrap());
    std::fs::remove_dir_all(&dir).ok();
    ty
}

/// The stop of the test: at the origin, its buses going north (+y), two line records - to
/// Rathaus and to Bahnhof.
fn stop() -> PaxStop {
    PaxStop {
        name: "Markt".into(),
        alias: String::new(),
        pos: DVec3::ZERO,
        heading: 0.0,
        gather: DVec3::ZERO,
        spots: Vec::new(),
        taken: Vec::new(),
        enter_max: 0.0,
        enter_min: 0.0,
        length: 30.0,
        lane: None,
        was_near: false,
        near: true,
        clock_ms: 0.0,
        want: 0,
        factor: 1.0,
        buses: Vec::new(),
        dests: Vec::new(),
        lines: vec![("Rathaus".into(), ["Rathaus".to_string()].into_iter().collect()), ("Bahnhof".into(), ["Bahnhof".to_string()].into_iter().collect())],
    }
}

/// Somebody waiting at stop `at` (7: the test's) for line record `line`.
fn waiting(ty: &Arc<HumanType>, id: u32, at: i64, line: usize, pos: DVec3) -> Person {
    let mut p = Pax::new(1.1, 0.5);
    p.task = Task::WaitingForBus;
    p.stop = Some(at);
    p.line = Some(line);
    p.dest = Some(if line == 0 { "Rathaus" } else { "Bahnhof" }.into());
    Person {
        id,
        ty: ty.clone(),
        variant: 0,
        position: pos,
        heading: 270.0,
        lheading: 0.0,
        place: Place::Ground,
        vel: DVec2::ZERO,
        pace: 1.1,
        activity: Activity::Stand,
        anim: OmsiAnim::default(),
        state: State::Pax(Box::new(p)),
        t_state: 0.0,
        interior: 0.0,
        tilt: Mat4::IDENTITY,
        age: 40.0,
        stuck: 0.0,
        ghost: 0.0,
        car_wait: 0.0,
        detour: 0.0,
        detour_side: 0.0,
        why: "",
        puppet: None,
        remote: false,
    }
}

fn bus(cabin: &Arc<Cabin>, id: BusId, y: f64, speed: f64, heading: f64, terminus: Option<&str>) -> BusNow {
    BusNow {
        id,
        cabin: cabin.clone(),
        pos: DVec3::new(if heading == 0.0 { -1.75 } else { -5.25 }, y, 0.0),
        rot: Mat4::IDENTITY,
        heading,
        speed,
        entry_open: vec![false],
        exit_open: vec![false],
        walk_open: None,
        interior: 0.0,
        air: CabinAir::default(),
        half: DVec2::new(1.25, 6.0),
        centre: DVec2::ZERO,
        accel: DVec2::ZERO,
        trailers: Vec::new(),
        terminus: terminus.map(str::to_string),
        takes: if matches!(id, BusId::Ai(k) if remote_bus_player(k).is_some()) { Takes::Nobody } else { Takes::Terminus },
        next_stop: None,
        places_off: Vec::new(),
        served: None,
    }
}

/// The people of the test: two waiting at the Markt for Rathaus, one there for Bahnhof, one
/// for Rathaus at another stop.
fn sim(name: &str) -> PeopleSim {
    let ty = human(name);
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.stops.insert(7, stop());
    let mut far = stop();
    far.pos = DVec3::new(0.0, 400.0, 0.0);
    h.stops.insert(8, far);
    h.people = vec![
        waiting(&ty, 1, 7, 0, DVec3::new(2.3, 0.6, 0.0)),
        waiting(&ty, 2, 7, 0, DVec3::new(3.1, -1.6, 0.0)),
        waiting(&ty, 3, 7, 1, DVec3::new(2.8, 1.5, 0.0)),
        waiting(&ty, 4, 8, 0, DVec3::new(2.3, 400.0, 0.0)),
    ];
    h
}

/// The bus driven from y `from` to `to` at `speed` (m/s), standing `stand` seconds at y -4
/// on the way: the people's clock and `passed_tick` four times a second.
fn drive(h: &mut PeopleSim, mk: &dyn Fn(f64, f64) -> BusNow, from: f64, to: f64, speed: f64, stand: f64) {
    let mut y = from;
    let mut stood = 0.0;
    while y < to {
        let standing = stand > 0.0 && stood < stand && (y - -4.0).abs() < 1.5;
        let v = if standing { 0.0 } else { speed };
        if standing {
            stood += 0.25;
        }
        let b = mk(y, v);
        h.time += 0.25;
        h.last_buses = Arc::new(vec![b.clone()]);
        h.passed_tick(&[b]);
        y += v * 0.25;
    }
}

#[test]
fn a_players_bus_driving_past_leaves_those_who_wanted_it_and_they_wave_after_it() {
    let c = cabin("past");
    let mut h = sim("past");
    let mk = |y, v| bus(&c, BusId::Player, y, v, 0.0, Some("Rathaus"));
    drive(&mut h, &mk, -80.0, 20.0, 10.0, 0.0);
    let left = h.take_left_behind();
    assert_eq!(left, vec![LeftBehind { bus: BusId::Player, stop: 7, count: 2, full: false, stood: false }], "the two for Rathaus, not the one for Bahnhof nor at another stop");
    assert!(h.take_left_behind().is_empty(), "told once");
    let mut waving: Vec<u32> = h.waving.iter().map(|w| w.person).collect();
    waving.sort();
    assert_eq!(waving, [1, 2]);
    let (at, age) = h.wave_of(1).unwrap();
    assert!(at.y > 12.0 && age < 1.0, "after the bus, where it is: {at:?} {age}");
    assert!(h.wave_of(3).is_none());
    // (a wave ends by itself)
    for _ in 0..20 {
        h.time += 0.25;
        h.passed_tick(&[]);
    }
    assert!(h.wave_of(1).is_none() && h.waving.is_empty());
}

#[test]
fn a_bus_that_stands_and_leaves_them_leaves_them_too_and_a_full_one_is_excused() {
    let c = cabin("stood");
    let mut h = sim("stood");
    let mk = |y, v| bus(&c, BusId::Player, y, v, 0.0, Some("Rathaus"));
    drive(&mut h, &mk, -80.0, 60.0, 8.0, 10.0);
    assert_eq!(h.take_left_behind(), vec![LeftBehind { bus: BusId::Player, stop: 7, count: 2, full: false, stood: true }]);
    // every place taken: they could not get on
    let mut h = sim("full");
    h.seats.insert(BusId::Player, vec![true, true]);
    drive(&mut h, &mk, -80.0, 60.0, 8.0, 10.0);
    let left = h.take_left_behind();
    assert_eq!(left.len(), 1);
    assert!(left[0].full && left[0].count == 2);
    // nobody waiting for it any more (they got on): nobody left
    let mut h = sim("boarded");
    for p in h.people.iter_mut() {
        if let State::Pax(x) = &mut p.state {
            x.task = Task::WalkingToBusstop;
        }
    }
    drive(&mut h, &mk, -80.0, 60.0, 8.0, 10.0);
    assert!(h.take_left_behind().is_empty());
}

#[test]
fn only_the_players_buses_on_their_way_count() {
    let c = cabin("ai");
    // a timetable bus driving past: its timetable decides where it stops
    let mut h = sim("ai");
    drive(&mut h, &|y, v| bus(&c, BusId::Ai(17), y, v, 0.0, Some("Rathaus")), -80.0, 60.0, 10.0, 0.0);
    assert!(h.take_left_behind().is_empty() && h.waving.is_empty());
    // a player's bus showing another terminus
    let mut h = sim("other");
    drive(&mut h, &|y, v| bus(&c, BusId::Player, y, v, 0.0, Some("Zoo")), -80.0, 60.0, 10.0, 0.0);
    assert!(h.take_left_behind().is_empty());
    // the other way, on the other side of the road
    let mut h = sim("back");
    let mut y = 60.0;
    while y > -80.0 {
        let b = bus(&c, BusId::Player, y, 10.0, 180.0, Some("Rathaus"));
        h.time += 0.25;
        h.passed_tick(&[b]);
        y -= 2.5;
    }
    assert!(h.take_left_behind().is_empty());
    // a LAN client tells nothing itself: the host does
    let mut h = sim("client");
    h.mirror = true;
    drive(&mut h, &|y, v| bus(&c, BusId::Player, y, v, 0.0, Some("Rathaus")), -80.0, 60.0, 10.0, 0.0);
    assert!(h.take_left_behind().is_empty());
}

#[test]
fn another_lan_players_bus_counts_on_the_host_by_what_it_shows() {
    let c = cabin("remote");
    let id = BusId::Ai(remote_bus_id(4));
    // (what it shows not known yet: nobody knows it is theirs)
    let mut h = sim("remote");
    drive(&mut h, &|y, v| bus(&c, id, y, v, 0.0, None), -80.0, 60.0, 10.0, 0.0);
    assert!(h.take_left_behind().is_empty());
    let mut h = sim("remote2");
    h.set_remote_termini([(4, "Rathaus".to_string())].into_iter());
    drive(&mut h, &|y, v| bus(&c, id, y, v, 0.0, None), -80.0, 60.0, 10.0, 0.0);
    assert_eq!(h.take_left_behind(), vec![LeftBehind { bus: id, stop: 7, count: 2, full: false, stood: false }]);
}

#[test]
fn the_host_says_who_signals_and_who_waves_and_a_client_draws_it() {
    let c = cabin("gest");
    let mut h = sim("gest");
    // coming, 40 m off: those for Rathaus signal to it
    let b = bus(&c, BusId::Player, -40.0, 10.0, 0.0, Some("Rathaus"));
    h.last_buses = Arc::new(vec![b]);
    let mut g = h.lan_gestures();
    g.sort_by_key(|x| x.0);
    assert_eq!(g.iter().map(|x| (x.0, x.1)).collect::<Vec<_>>(), [(1, Gesture::Hail), (2, Gesture::Hail)]);
    // full: a glance
    h.seats.insert(BusId::Player, vec![true, true]);
    assert!(h.lan_gestures().iter().all(|x| x.1 == Gesture::Full));
    h.seats.clear();
    // gone past them: they wave
    drive(&mut h, &|y, v| bus(&c, BusId::Player, y, v, 0.0, Some("Rathaus")), -40.0, 40.0, 10.0, 0.0);
    let mut g = h.lan_gestures();
    g.sort_by_key(|x| x.0);
    assert_eq!(g.iter().map(|x| (x.0, x.1)).collect::<Vec<_>>(), [(1, Gesture::Wave), (2, Gesture::Wave)]);
    // a client: one of the host's people, the bus of ours nearest where the host had it
    let mut cl = PeopleSim::new(Path::new("/nonexistent"), 200);
    cl.mirror = true;
    let mine = bus(&c, BusId::Player, 30.0, 10.0, 0.0, Some("Rathaus"));
    cl.last_buses = Arc::new(vec![mine]);
    cl.set_mirror_gesture(1, Some((Gesture::Wave, DVec3::new(-1.0, 28.0, 0.0))));
    let (at, _) = cl.wave_of(1).unwrap();
    assert_eq!(at, DVec3::new(-1.75, 30.0, 0.0), "where our bus is");
    cl.set_mirror_gesture(2, Some((Gesture::Hail, DVec3::new(-1.75, 31.0, 0.0))));
    assert_eq!(cl.mirror_hail(2).map(|x| (x.0, x.3)), Some((BusId::Player, false)));
    cl.set_mirror_gesture(2, Some((Gesture::Full, DVec3::new(500.0, 500.0, 0.0))));
    let (_, _, at, full) = cl.mirror_hail(2).unwrap();
    assert!(full && at == DVec3::new(500.0, 500.0, 0.0), "no bus of ours there: the place itself");
    cl.set_mirror_gesture(2, None);
    assert!(cl.mirror_hail(2).is_none());
    cl.time += WAVE_SECS + 0.1;
    assert!(cl.wave_of(1).is_none(), "a wave ends by itself");
}

/// (openOMSI PR #3442 by jay23606) A passenger of a timetable bus who gets off at the next stop
/// asks for it on their own bus: the stop request is that bus's (once, however many ask), not the
/// player's stop button.
#[test]
fn a_timetable_bus_passenger_asks_for_its_own_stop() {
    let c = cabin("ai-stop");
    let ty = human("ai-stop");
    let world = crate::world_data::WorldData::default();
    let buses = vec![bus(&c, BusId::Ai(5), 0.0, 0.0, 0.0, Some("Rathaus"))];
    let bus_ix: HashMap<BusId, usize> = [(BusId::Ai(5), 0)].into_iter().collect();
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.people = vec![waiting(&ty, 1, 7, 0, DVec3::ZERO), waiting(&ty, 2, 7, 0, DVec3::ZERO)];
    for i in 0..2 {
        let p = h.pax_mut(i).unwrap();
        p.task = Task::SittingInBus;
        p.bus = Some(BusId::Ai(5));
        p.inside = Some(BusId::Ai(5));
        h.set_task(i, Task::InBusToExit, &buses, &bus_ix, &world);
    }
    assert!(!h.stop_request, "the player's stop button is not this bus's request");
    assert_eq!(h.take_ai_stop_requests(), vec![5]);
    assert!(h.take_ai_stop_requests().is_empty());
}

/// (NexiconDK, 31 October 2026) The people in a bus and where each rides to: what a LAN player's
/// bus given back to the AI hands on to the AI's in its place (`relief back`, `seed_riders_in`).
#[test]
fn the_people_in_a_bus_say_where_they_ride_to() {
    let mut h = sim("dests");
    for (i, dest) in [(0, Some("Rathaus")), (1, None), (2, Some("Bahnhof"))] {
        h.people[i].place = Place::Bus(BusId::Player, Vec3::ZERO);
        let p = h.pax_mut(i).unwrap();
        p.inside = Some(BusId::Player);
        p.dest = dest.map(str::to_string);
    }
    h.people[3].place = Place::Bus(BusId::Ai(9), Vec3::ZERO);
    let mut d = h.rider_dests(BusId::Player);
    d.sort();
    assert_eq!(d, vec![None, Some("Bahnhof".to_string()), Some("Rathaus".to_string())]);
    assert_eq!(h.rider_dests(BusId::Ai(9)).len(), 1);
    assert!(h.rider_dests(BusId::Ai(10)).is_empty());
}
