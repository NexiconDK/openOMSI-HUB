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
    // (everybody signals, waves, glances: what each does is the test's)
    h.gestures = GestureShares { signal: 1.0, wave: 1.0, full: 1.0, phone: 0.0 };
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

/// (31 October 2026) Most set off to where the bus will stand as soon as it is within 60 m and
/// rolling (`Task::ToBus`): it leaves them too when it goes on (not those going to another bus),
/// but they do not signal on the way - those who stay at their place do
/// (`stays_till_it_stands`).
#[test]
fn those_stepping_up_to_the_bus_are_left_by_it_and_those_staying_signal() {
    let c = cabin("stepping");
    let mut h = sim("stepping");
    for p in h.people.iter_mut() {
        if let State::Pax(x) = &mut p.state {
            x.task = Task::ToBus;
            x.bus = Some(if p.id == 2 { BusId::Ai(9) } else { BusId::Player });
        }
    }
    let coming = bus(&c, BusId::Player, -40.0, 10.0, 0.0, Some("Rathaus"));
    assert!(h.visual_hail_bus(0, std::slice::from_ref(&coming)).is_none(), "on the move: no signal");
    let mk = |y, v| bus(&c, BusId::Player, y, v, 0.0, Some("Rathaus"));
    drive(&mut h, &mk, -80.0, 20.0, 10.0, 0.0);
    assert_eq!(h.take_left_behind(), vec![LeftBehind { bus: BusId::Player, stop: 7, count: 1, full: false, stood: false }]);
    assert!(h.wave_of(1).is_some() && h.wave_of(2).is_none());
    // still at their place: they signal
    let h = sim("staying");
    assert!(h.visual_hail_bus(0, &[coming]).is_some_and(|(id, moving, _)| id == BusId::Player && moving));
}

/// (31 October 2026) Those who signal or wave to a coming bus stay at their place till it stands,
/// by the shares of them (`greetings`); the others step up to it, as in Omsi - all of them with
/// nothing shared. A group does all the same.
#[test]
fn some_stay_at_their_place_till_the_bus_stands() {
    let ty = human("stay");
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.people = (0..400).map(|id| waiting(&ty, id, 7, 0, DVec3::ZERO)).collect();
    h.gestures = GestureShares::NONE;
    assert!((0..400).all(|i| !h.stays_till_it_stands(i)), "nothing shared: as in Omsi");
    h.gestures.signal = 0.3;
    let n = (0..400).filter(|i| h.stays_till_it_stands(*i)).count();
    assert!((90..=150).contains(&n), "about three in ten: {n}");
    assert!((0..400).filter_map(|i| h.greets(i)).all(|g| g == Greet::Signal));
    h.gestures.wave = 0.3;
    let n = (0..400).filter(|i| h.stays_till_it_stands(*i)).count();
    assert!((170..=240).contains(&n), "about half, signalling or waving: {n}");
    let waving = (0..400).filter(|i| h.greets(*i) == Some(Greet::Wave)).count();
    assert!((60..=140).contains(&waving), "{waving}");
    h.gestures.signal = 1.0;
    assert!((0..400).all(|i| h.stays_till_it_stands(i)), "every time");
    h.gestures.signal = 0.3;
    for (i, p) in h.people.iter_mut().enumerate() {
        if let State::Pax(x) = &mut p.state {
            x.group = Some(if i < 200 { 1 } else { 2 });
        }
    }
    let (a, b) = (h.greets(0), h.greets(200));
    assert!((0..200).all(|i| h.greets(i) == a) && (200..400).all(|i| h.greets(i) == b));
}

/// (31 October 2026) On their way to the stop, those who wave wave to the bus coming to it - and
/// somebody late, running for it, till they are at it. A full bus: nothing.
#[test]
fn those_on_their_way_to_the_stop_wave_to_the_bus() {
    let c = cabin("onway");
    let mut h = sim("onway");
    if let State::Pax(x) = &mut h.people[0].state {
        x.task = Task::WalkingToBusstop;
    }
    h.people[0].position = DVec3::new(2.3, 30.0, 0.0);
    let coming = [bus(&c, BusId::Player, -40.0, 10.0, 0.0, Some("Rathaus"))];
    let mut none_full = |_: &BusNow| false;
    assert_eq!(h.gesture_of(0, &coming, &mut none_full).map(|g| (g.0, g.1)), Some((Gesture::WaveTo, BusId::Player)));
    assert!(h.gesture_of(0, &coming, &mut |_| true).is_none(), "full");
    h.gestures.wave = 0.0;
    assert!(h.gesture_of(0, &coming, &mut none_full).is_none(), "nobody waves");
    // late, running for a player's bus standing at the stop
    h.gestures.wave = 1.0;
    if let State::Pax(x) = &mut h.people[0].state {
        x.task = Task::ToBus;
        x.bus = Some(BusId::Player);
        x.late = Some(super::runners::Late { phase: super::runners::LatePhase::Hurry, walk: 1.2, run: 3.2, hurried: 0.0, stood: true });
    }
    let standing = [bus(&c, BusId::Player, -4.0, 0.0, 0.0, Some("Rathaus"))];
    assert_eq!(h.gesture_of(0, &standing, &mut none_full).map(|g| g.0), Some(Gesture::WaveTo));
    h.people[0].position = DVec3::new(2.3, -2.0, 0.0);
    assert!(h.gesture_of(0, &standing, &mut none_full).is_none(), "at the bus");
}

/// (31 October 2026) Those a bus leaves behind are counted all the same; only those who wave
/// wave after it.
#[test]
fn only_those_who_wave_wave_after_the_bus_but_all_are_left() {
    let c = cabin("nowave");
    let mut h = sim("nowave");
    h.gestures.wave = 0.0;
    drive(&mut h, &|y, v| bus(&c, BusId::Player, y, v, 0.0, Some("Rathaus")), -80.0, 20.0, 10.0, 0.0);
    assert_eq!(h.take_left_behind().first().map(|l| l.count), Some(2));
    assert!(h.waving.is_empty());
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
    // a LAN client: not for the host's people drawn here - the host tells of those
    let mut h = sim("client");
    h.mirror = true;
    for p in h.people.iter_mut() {
        p.remote = true;
    }
    drive(&mut h, &|y, v| bus(&c, BusId::Player, y, v, 0.0, Some("Rathaus")), -80.0, 60.0, 10.0, 0.0);
    assert!(h.take_left_behind().is_empty());
    // (31 October 2026) but for those it took from the host as its bus came, its own now - the
    // host knows nothing of them any more; and only for its own bus
    let mut h = sim("client-own");
    h.mirror = true;
    drive(&mut h, &|y, v| bus(&c, BusId::Player, y, v, 0.0, Some("Rathaus")), -80.0, 20.0, 10.0, 0.0);
    assert_eq!(h.take_left_behind(), vec![LeftBehind { bus: BusId::Player, stop: 7, count: 2, full: false, stood: false }]);
    assert!(h.wave_of(1).is_some());
    let mut h = sim("client-other");
    h.mirror = true;
    h.set_remote_termini([(4, "Rathaus".to_string())].into_iter());
    drive(&mut h, &|y, v| bus(&c, BusId::Ai(remote_bus_id(4)), y, v, 0.0, None), -80.0, 60.0, 10.0, 0.0);
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
    // coming, 40 m off: those for Rathaus signal to it - or wave to it
    let b = bus(&c, BusId::Player, -40.0, 10.0, 0.0, Some("Rathaus"));
    h.last_buses = Arc::new(vec![b]);
    h.gestures.wave = 0.0;
    let mut g = h.lan_gestures();
    g.sort_by_key(|x| x.0);
    assert_eq!(g.iter().map(|x| (x.0, x.1)).collect::<Vec<_>>(), [(1, Gesture::Hail), (2, Gesture::Hail)]);
    h.gestures = GestureShares { signal: 0.0, wave: 1.0, full: 1.0, phone: 0.0 };
    assert!(h.lan_gestures().iter().all(|x| x.1 == Gesture::WaveTo));
    h.gestures.signal = 1.0;
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
    assert_eq!(cl.mirror_gesture_now(2).map(|x| (x.0, x.1)), Some((Gesture::Hail, Some(BusId::Player))));
    assert_eq!(Gesture::of_word("waveto"), Some(Gesture::WaveTo));
    cl.set_mirror_gesture(2, Some((Gesture::WaveTo, DVec3::new(-1.75, 31.0, 0.0))));
    assert_eq!(cl.mirror_gesture_now(2).map(|x| x.0), Some(Gesture::WaveTo));
    cl.set_mirror_gesture(2, Some((Gesture::Full, DVec3::new(500.0, 500.0, 0.0))));
    let (g, bus, at) = cl.mirror_gesture_now(2).unwrap();
    assert!(g == Gesture::Full && bus.is_none() && at == DVec3::new(500.0, 500.0, 0.0), "no bus of ours there: the place itself");
    // (what this game shares nobody with is not drawn)
    cl.gestures.full = 0.0;
    assert!(cl.mirror_gesture_now(2).is_none());
    cl.set_mirror_gesture(2, None);
    assert!(cl.mirror_gesture_now(2).is_none());
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

/// (31 October 2026) Another LAN player's bus on the host as it really comes (its vehicle, as
/// `set_remote_buses` makes it a bus the people know, what it shows from his game): those
/// waiting for it signal to it as it comes, and wave after it as it drives past - the words the
/// clients draw them by.
#[test]
fn a_lan_players_real_bus_is_signalled_to_and_waved_after_on_the_host() {
    let dir = std::env::temp_dir().join(format!("omsi-passed-real-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("test.bus"), "[model]\nmodel.cfg\n\n[boundingbox]\n2.5\n12\n3\n0\n0\n1.5\n\n[passengercabin]\ncabin.cfg\n\n[paths]\npaths.cfg\n").unwrap();
    std::fs::write(dir.join("model.cfg"), "").unwrap();
    std::fs::write(dir.join("paths.cfg"), "[pathpnt]\n1.1\n4\n0.4\n\n[pathpnt]\n0\n4\n0.5\n\n[pathlink]\n0\n1\n").unwrap();
    std::fs::write(dir.join("cabin.cfg"), "[entry]\n0\n\n[exit]\n0\n\n[passpos]\n-0.5\n0\n0.9\n0.45\n0\n\n[passpos]\n0.5\n0\n0.9\n0.45\n0\n").unwrap();
    let ty = Arc::new(crate::VehicleType::load_ai(&dir, &dir.join("test.bus")).unwrap());
    let mut v = crate::VehicleInstance::new(ty, crate::VehicleHost::new(crate::SimClock::default()));
    let world = crate::world_data::WorldData::default();
    let mut h = sim("real");
    h.set_remote_termini([(4, "Rathaus".to_string())].into_iter());
    let step = |h: &mut PeopleSim, v: &mut crate::VehicleInstance, y: f64, speed: f32| {
        v.position = DVec3::new(-1.75, y, 0.0);
        v.heading = 0.0;
        v.physics.speed = speed;
        h.set_remote_buses([(4u32, &*v)].into_iter());
        let buses = h.gather_buses(&world, None, None);
        assert_eq!(buses.len(), 1, "the player's bus is one the people know");
        h.last_buses = Arc::new(buses);
        h.time += 0.25;
        let buses = h.last_buses.clone();
        h.passed_tick(&buses);
        h.lan_gestures()
    };
    // coming, 40 m before the stop: those for Rathaus signal to it
    let g = step(&mut h, &mut v, -40.0, 10.0);
    // (signalling or waving to it, each by their lot)
    let mut hail: Vec<u32> = g.iter().filter(|x| matches!(x.1, Gesture::Hail | Gesture::WaveTo)).map(|x| x.0).collect();
    hail.sort();
    assert_eq!(hail, vec![1, 2], "{g:?}");
    // on past the stop without stopping: they wave after it
    let mut y = -30.0;
    let mut waved = Vec::new();
    while y < 60.0 {
        y += 2.5;
        waved = step(&mut h, &mut v, y, 10.0);
        if waved.iter().any(|x| x.1 == Gesture::Wave) {
            break;
        }
    }
    let mut wave: Vec<u32> = waved.iter().filter(|x| x.1 == Gesture::Wave).map(|x| x.0).collect();
    wave.sort();
    assert_eq!(wave, vec![1, 2], "{waved:?}");
    std::fs::remove_dir_all(&dir).ok();
}

/// (NexiconDK, 31 October 2026) One of the LAN host's people in their place in a timetable bus,
/// as a player's game draws them: on a seat they sit (its height), as the host has them - they
/// stood beside every seat; walking in the bus, or out of it, as they are.
#[test]
fn the_hosts_riders_sit_on_their_seats_in_a_players_game() {
    let c = cabin("mirror-seat");
    let ty = human("mirror-seat");
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.last_buses = Arc::new(vec![bus(&c, BusId::Ai(5), 0.0, 0.0, 0.0, Some("Rathaus"))]);
    let mut p = waiting(&ty, 40, 7, 0, DVec3::ZERO);
    p.remote = true;
    p.state = State::Idle;
    h.people = vec![p];
    let pose = |seat: Option<usize>| MirrorPose { pos: DVec3::ZERO, heading: 0.0, vel: DVec2::ZERO, activity: Activity::Stand, aboard: Some((5, Vec3::new(-0.5, 0.0, 0.45), 0.0, seat)), waiting: None };
    h.mirror_set(40, &pose(Some(0)));
    let f = h.mirror_facts(&h.people[0]);
    assert_eq!(f.activity, Activity::Sit);
    assert!(f.seat.is_some() && f.seated_level.is_some_and(|l| (l - (ty.def.seat_height - 0.45) as f64).abs() < 1e-4), "{f:?}");
    assert_eq!(h.people[0].activity, Activity::Sit, "the original animations sit too");
    // on the way to it, and off the bus: as they are
    h.mirror_set(40, &pose(None));
    assert_eq!(h.mirror_facts(&h.people[0]).activity, Activity::Stand);
    h.mirror_set(40, &pose(Some(0)));
    h.mirror_set(40, &MirrorPose { aboard: None, ..pose(None) });
    assert!(h.mirror_facts(&h.people[0]).seat.is_none() && h.mirror_seat.is_empty());
    // a place its cabin here has not: as they are
    h.mirror_set(40, &pose(Some(9)));
    assert!(h.mirror_facts(&h.people[0]).seat.is_none());
}

/// (NexiconDK, 31 October 2026) Somebody coming to wait at a stop a player sees walks up along its
/// pavement: the points along it, over the footpaths joined to it the straight way on, either
/// way - and which stops a player sees (a LAN player near one, the player's eye).
#[test]
fn people_coming_to_wait_where_a_player_sees_the_stop_walk_up_along_its_pavement() {
    use super::super::populate::{along_pavement, WALK_UP_SEEN};
    use crate::traffic::LaneBuilder;
    // a pavement of three 30 m paths northwards, and one turning off east at the end of the first
    let mut lanes: Vec<_> = (0..3).map(|k| LaneBuilder::arc(DVec3::new(5.0, 30.0 * k as f64, 0.0), 0.0, 30.0, 0.0, 0.0, LaneKind::Sidewalk, 2.0)).collect();
    lanes.push(LaneBuilder::arc(DVec3::new(5.0, 30.0, 0.0), 30.0, 30.0, 0.0, 0.0, LaneKind::Sidewalk, 2.0));
    let mut net = Network { lanes, ..Default::default() };
    net.link(1.5);
    // from the middle path, 10 m in (y 40): 40 m on - over its end, the straight way - and back
    let on = along_pavement(&net, 1, 10.0, 40.0, true).unwrap();
    assert!((on - DVec3::new(5.0, 80.0, 0.0)).length() < 0.5, "{on:?}");
    let back = along_pavement(&net, 1, 10.0, 35.0, false).unwrap();
    assert!((back - DVec3::new(5.0, 5.0, 0.0)).length() < 0.5, "{back:?}");
    assert_eq!(along_pavement(&net, 1, 10.0, 45.0, false), None, "the pavement ends sooner");
    assert_eq!(along_pavement(&net, 1, 10.0, 200.0, true), None);
    // who sees a stop: a LAN player near it, not one far off
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.center = DVec3::new(10_000.0, 0.0, 0.0);
    let stop = DVec3::new(0.0, 40.0, 0.0);
    assert!(!h.stop_seen(stop));
    h.lan_centers = vec![DVec3::new(0.0, 100.0, 0.0)];
    assert!(h.stop_seen(stop));
    h.lan_centers = vec![DVec3::new(0.0, 100.0 + WALK_UP_SEEN, 0.0)];
    assert!(!h.stop_seen(stop));
}

/// (31 October 2026) A stop whose heading runs against the way its buses go - one on the other
/// side of the road the map lays (`side` 1): those waiting there signal to a bus coming as to any
/// other, and wave after one that drives past; not to one going the other way.
#[test]
fn a_stop_laid_the_other_way_round_is_signalled_and_waved_at_as_any() {
    let c = cabin("reversed");
    let mut h = sim("reversed");
    for s in h.stops.values_mut() {
        s.heading = 180.0;
    }
    let b = |y: f64, v: f64| bus(&c, BusId::Player, y, v, 0.0, Some("Rathaus"));
    h.last_buses = Arc::new(vec![b(-40.0, 10.0)]);
    let mut hail: Vec<u32> = h.lan_gestures().into_iter().filter(|g| matches!(g.1, Gesture::Hail | Gesture::WaveTo)).map(|g| g.0).collect();
    hail.sort();
    assert_eq!(hail, vec![1, 2], "coming: they signal");
    drive(&mut h, &|y, v| b(y, v), -80.0, 60.0, 10.0, 0.0);
    assert_eq!(h.take_left_behind(), vec![LeftBehind { bus: BusId::Player, stop: 7, count: 2, full: false, stood: false }]);
    // the other way along the street (going south, past the stop): nobody
    let mut h = sim("reversed2");
    h.last_buses = Arc::new(vec![bus(&c, BusId::Player, 40.0, 10.0, 180.0, Some("Rathaus"))]);
    assert!(h.lan_gestures().is_empty(), "{:?}", h.lan_gestures());
}
