//! People in a wheelchair or with a pram (`mobility`).
use super::*;
use omsi_content::human::Mobility;

/// A bus of two doors - the front one at y 4, the middle one at y 0 - two seats and two
/// standing places, one by the middle door; `floor`: its floor (a low floor 0.35, a high one
/// 0.9, reached from a step at the door), `marked`: a `{mobility}` line on the rear standing
/// place.
fn cabin(name: &str, floor: f32, marked: bool) -> Arc<Cabin> {
    let dir = std::env::temp_dir().join(format!("omsi-mobility-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let step = floor.min(0.35);
    std::fs::write(dir.join("test.bus"), "[boundingbox]\n2.5\n12\n3\n0\n0\n1.5\n\n[passengercabin]\ncabin.cfg\n\n[paths]\npaths.cfg\n").unwrap();
    let pts = [(1.1, 4.0, step), (0.0, 4.0, floor), (1.1, 0.0, step), (0.0, 0.0, floor), (0.0, -3.0, floor)];
    let mut paths = String::new();
    for (x, y, z) in pts {
        paths += &format!("[pathpnt]\n{x}\n{y}\n{z}\n\n");
    }
    for (a, b) in [(0, 1), (1, 3), (2, 3), (3, 4)] {
        paths += &format!("[pathlink]\n{a}\n{b}\n\n");
    }
    std::fs::write(dir.join("paths.cfg"), paths).unwrap();
    let seat_z = floor + 0.45;
    let cfg = format!(
        "[entry]\n0\n\n[entry]\n2\n\n[exit]\n2\n\n[exit]\n0\n\n\
         [passpos]\n-0.5\n3\n{seat_z}\n0.45\n0\n\n[passpos]\n-0.5\n-3\n{seat_z}\n0.45\n0\n\n\
         [passpos]\n0.6\n-0.5\n{floor}\n0\n90\n\n[passpos]\n0\n-3\n{floor}\n0\n180\n{}\n",
        if marked { "{mobility}\n" } else { "" }
    );
    std::fs::write(dir.join("cabin.cfg"), cfg).unwrap();
    let def = omsi_vehicle::Vehicle::load(&dir.join("test.bus")).unwrap();
    let c = Arc::new(Cabin::load_train(&[(&def, Vec3::ZERO, f32::INFINITY)]).expect("cabin"));
    std::fs::remove_dir_all(&dir).ok();
    c
}

fn human(name: &str, mobility: &str) -> Arc<HumanType> {
    let dir = std::env::temp_dir().join(format!("omsi-mobility-hum-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let extra = if mobility.is_empty() { String::new() } else { format!("[openomsi_mobility]\n{mobility}\n") };
    std::fs::write(dir.join("person.hum"), format!("[model]\nmodel.cfg\n{extra}")).unwrap();
    std::fs::write(dir.join("model.cfg"), "").unwrap();
    let ty = Arc::new(HumanType::load(&dir.join("person.hum")).unwrap());
    std::fs::remove_dir_all(&dir).ok();
    ty
}

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
        lines: vec![("Rathaus".into(), ["Rathaus".to_string()].into_iter().collect())],
    }
}

/// Somebody at the stop (7) for Rathaus, `task` their task.
fn person(ty: &Arc<HumanType>, id: u32, task: Task, pos: DVec3) -> Person {
    let mut p = Pax::new(1.1, 0.5);
    p.task = task;
    p.stop = Some(7);
    p.line = Some(0);
    p.dest = Some("Rathaus".into());
    p.pos = pos;
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

fn bus(cabin: &Arc<Cabin>, y: f64, speed: f64) -> BusNow {
    BusNow {
        id: BusId::Player,
        cabin: cabin.clone(),
        pos: DVec3::new(-1.75, y, 0.0),
        rot: Mat4::IDENTITY,
        heading: 0.0,
        speed,
        entry_open: vec![false, false],
        exit_open: vec![false, false],
        walk_open: None,
        interior: 0.0,
        air: CabinAir::default(),
        half: DVec2::new(1.25, 6.0),
        centre: DVec2::ZERO,
        accel: DVec2::ZERO,
        trailers: Vec::new(),
        terminus: Some("Rathaus".into()),
        takes: Takes::Terminus,
        next_stop: None,
        places_off: Vec::new(),
        served: None,
    }
}

/// The place for a wheelchair or a pram: the standing place by the middle door, or the one
/// marked `{mobility}`; a wheelchair rolls in only where no step leads to it.
#[test]
fn a_bus_keeps_a_place_for_a_wheelchair_or_a_pram_by_its_middle_door() {
    let low = cabin("low", 0.35, false);
    assert_eq!(low.seats.iter().map(|s| s.mobility).collect::<Vec<_>>(), [false, false, true, false], "the standing place by the middle door");
    let marked = cabin("marked", 0.35, true);
    assert_eq!(marked.seats.iter().map(|s| s.mobility).collect::<Vec<_>>(), [false, false, false, true], "the one marked, alone");
    assert_eq!(marked.seats[3].rot, 180.0);
    // the doors to it: the middle one, in and out; on a high floor none without steps
    assert_eq!(low.mobility_door(&low.entries, 2, true), Some(1));
    assert_eq!(low.mobility_door(&low.exits, 2, true), Some(0));
    let high = cabin("high", 0.9, false);
    assert!(high.seats[2].mobility);
    assert_eq!(high.mobility_door(&high.entries, 2, true), None, "steps up from the door");
    assert_eq!(high.mobility_door(&high.entries, 2, false), Some(1), "a pram is carried up");
}

/// A bus whose doors have two wings, each its own `[entry]` (the Solaris Urbino's) - the front
/// door's at y 5.2 and 4.6, the middle door's at 0.9 and 0.3, on the right - with standing
/// places `(x, y)`.
fn winged_cabin(name: &str, places: &[(f32, f32)]) -> Cabin {
    let dir = std::env::temp_dir().join(format!("omsi-mobility-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("test.bus"), "[boundingbox]\n2.5\n12\n3\n0\n0\n1.5\n\n[passengercabin]\ncabin.cfg\n\n[paths]\npaths.cfg\n").unwrap();
    // (inside the doors the aisle)
    let pts = [(1.04, 5.2, 0.46), (1.04, 4.6, 0.46), (0.0, 4.9, 0.46), (1.04, 0.9, 0.46), (1.04, 0.3, 0.46), (0.0, 0.6, 0.46), (0.0, -3.0, 0.46)];
    let mut paths = String::new();
    for (x, y, z) in pts {
        paths += &format!("[pathpnt]\n{x}\n{y}\n{z}\n\n");
    }
    for (a, b) in [(0, 2), (1, 2), (2, 5), (3, 5), (4, 5), (5, 6)] {
        paths += &format!("[pathlink]\n{a}\n{b}\n\n");
    }
    std::fs::write(dir.join("paths.cfg"), paths).unwrap();
    let mut cfg = String::from("[entry]\n0\n\n[entry]\n1\n\n[entry]\n3\n\n[entry]\n4\n\n[exit]\n3\n\n[exit]\n4\n\n");
    for (x, y) in places {
        cfg += &format!("[passpos]\n{x}\n{y}\n0.46\n0\n30\n\n");
    }
    std::fs::write(dir.join("cabin.cfg"), cfg).unwrap();
    let def = omsi_vehicle::Vehicle::load(&dir.join("test.bus")).unwrap();
    let c = Cabin::load_train(&[(&def, Vec3::ZERO, f32::INFINITY)]).expect("cabin");
    std::fs::remove_dir_all(&dir).ok();
    c
}

/// The front door's other wing is no second door: the places are by the middle door, two of
/// them a wheelchair's width apart (a wheelchair and a pram), and a wheelchair rolls in at a
/// low floor's door point 0.46 m over the street.
#[test]
fn a_door_of_two_wings_is_one_door() {
    // (by the front door, two by the middle door, one at the back)
    let c = winged_cabin("wings", &[(0.1, 3.9), (0.5, 0.55), (-0.4, 0.5), (0.0, -3.0)]);
    assert_eq!(c.seats.iter().map(|s| s.mobility).collect::<Vec<_>>(), [false, true, true, false], "the two by the middle door, not the one by the driver");
    for k in [1, 2] {
        let door = c.mobility_door(&c.entries, k, true).expect("a wheelchair rolls in");
        assert!(c.entries[door].inside.y < 1.0, "by the middle door: {:?}", c.entries[door].inside);
        assert_eq!(c.seats[k].rot, 30.0, "facing as the place does");
    }
}

/// Standing places along the wall across from the middle door - its multi-purpose area: the
/// front one (by the backrest, in front of the door's middle) and the next a wheelchair's length
/// behind it, facing backwards; none of those in the aisle by the door, nor one too near the
/// first.
#[test]
fn the_places_are_along_the_wall_across_from_the_door() {
    let c = winged_cabin("wall", &[(0.3, 0.6), (-0.9, 1.3), (-0.9, 0.6), (-0.9, -0.5), (0.0, -3.0), (-0.9, 3.5)]);
    assert_eq!(c.seats.iter().map(|s| s.mobility).collect::<Vec<_>>(), [false, true, false, true, false, false]);
    assert_eq!((c.seats[1].rot, c.seats[3].rot), (180.0, 180.0), "backs to the board");
    assert_eq!((c.seats[0].rot, c.seats[2].rot), (30.0, 30.0));
}

/// One in a wheelchair takes the bus's place for them alone, by its middle door, with no
/// ticket to buy; a second one finds it taken and stays (at a full bus, not left behind); the
/// others leave it free while another place is.
#[test]
fn a_wheelchair_boards_only_where_its_place_is_free() {
    let c = cabin("board", 0.35, false);
    let chair = human("chair", "wheelchair");
    let walker = human("walker", "");
    let world = crate::world_data::WorldData::default();
    let buses = vec![bus(&c, -2.0, 0.0)];
    let bus_ix: HashMap<BusId, usize> = [(BusId::Player, 0)].into_iter().collect();
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    let mut s = stop();
    s.buses = vec![(BusId::Player, true)];
    h.stops.insert(7, s);
    h.people = vec![person(&chair, 1, Task::ToBus, DVec3::new(2.0, 0.0, 0.0)), person(&chair, 2, Task::ToBus, DVec3::new(2.0, 1.0, 0.0))];
    for i in 0..2 {
        h.pax_mut(i).unwrap().bus = Some(BusId::Player);
    }
    let (at_stops, mut taken, mut remove) = (HashMap::new(), false, Vec::new());
    h.pax_task(0, 0.1, &world, &buses, &bus_ix, &at_stops, None, &mut taken, &mut remove);
    let p = h.pax(0).unwrap();
    assert_eq!((p.task, p.seat, p.door, p.ticket, p.no_room), (Task::WalkingToBus, Some(2), Some(1), TICKET_NONE, None));
    h.pax_task(1, 0.1, &world, &buses, &bus_ix, &at_stops, None, &mut taken, &mut remove);
    let p = h.pax(1).unwrap();
    assert_eq!((p.task, p.seat, p.no_room), (Task::ToBus, None, Some(BusId::Player)), "its place taken: stays");
    // the others: anywhere but there while another place is free, there once none is
    h.people.push(person(&walker, 3, Task::ToBus, DVec3::ZERO));
    h.seats.insert(BusId::Player, vec![false; 4]);
    assert_eq!(h.places_off_for(2, BusId::Player, &c, &[]), [false, false, true, false]);
    assert_eq!(h.places_off_for(0, BusId::Player, &c, &[]), [true, true, false, true]);
    h.seats.insert(BusId::Player, vec![true, true, false, true]);
    assert_eq!(h.places_off_for(2, BusId::Player, &c, &[]), [false; 4], "every other place taken");
    // a high floor: no wheelchair at all, a pram by the middle door
    let high = cabin("board-high", 0.9, false);
    h.seats.clear();
    assert!(h.places_off_for(0, BusId::Player, &high, &[]).iter().all(|o| *o));
    h.people.push(person(&human("pram", "pram"), 4, Task::ToBus, DVec3::ZERO));
    assert_eq!(h.mobility(3), Some(Mobility::Pram));
    assert_eq!(h.places_off_for(3, BusId::Player, &high, &[]), [true, true, false, true]);
}

/// Somebody in a wheelchair the bus had no room for is not left behind by its driver: counted
/// as at a full bus; the others who wanted it are left behind as ever.
#[test]
fn a_wheelchair_without_room_is_not_left_behind_by_the_driver() {
    let c = cabin("left", 0.35, false);
    let chair = human("left-chair", "wheelchair");
    let walker = human("left-walker", "");
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.gestures = GestureShares { signal: 1.0, wave: 1.0, full: 1.0, phone: 0.0 };
    h.stops.insert(7, stop());
    h.people = vec![person(&chair, 1, Task::WaitingForBus, DVec3::new(2.3, 0.6, 0.0)), person(&walker, 2, Task::WaitingForBus, DVec3::new(3.1, -1.6, 0.0))];
    h.pax_mut(0).unwrap().no_room = Some(BusId::Player);
    let mut y = -80.0;
    while y < 20.0 {
        let b = bus(&c, y, 10.0);
        h.time += 0.25;
        h.last_buses = Arc::new(vec![b.clone()]);
        h.passed_tick(&[b]);
        y += 2.5;
    }
    let left = h.take_left_behind();
    assert_eq!(left.iter().map(|l| (l.count, l.full)).collect::<Vec<_>>(), [(1, true), (1, false)], "{left:?}");
    assert!(!h.waves(0) && h.greets(0).is_none() && !h.glances(0), "no gestures from a wheelchair");
}

/// A figure in a wheelchair holds still as it rolls, one with a pram keeps the upper body on
/// the handle: no twist, no turn of the hips.
#[test]
fn a_wheelchair_rolls_still_and_a_pram_is_pushed_steadily() {
    let chair = human("anim-chair", "wheelchair");
    let pram = human("anim-pram", "pram");
    let walker = human("anim-walker", "");
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.people = vec![person(&chair, 1, Task::Nothing, DVec3::ZERO), person(&pram, 2, Task::Nothing, DVec3::ZERO), person(&walker, 3, Task::Nothing, DVec3::ZERO)];
    for p in h.people.iter_mut() {
        p.state = State::Standing;
        p.vel = DVec2::new(0.0, 1.3);
    }
    let world = crate::world_data::WorldData::default();
    for _ in 0..40 {
        h.animate(0.03, &world, &[], &HashMap::new());
    }
    let [a, b, w] = [&h.people[0].anim.angles, &h.people[1].anim.angles, &h.people[2].anim.angles];
    assert!(a[0].abs() < 1e-3 && a[29] == 0.0 && a[10] == 0.0, "the wheelchair's figure does not walk: {a:?}");
    assert!(b[29] == 0.0 && b[10] == 0.0 && b[0].abs() > 1.0, "the legs walk, the upper body holds still: {b:?}");
    assert!(w[29] != 0.0, "somebody on foot twists as ever: {w:?}");
}


/// (NexiconDK) The player's bus with a ramp at its middle door (`ramp`): somebody in a
/// wheelchair waits at that door until the ramp is out - getting in and getting out - and the
/// driver is told; somebody on foot, another door, a timetable bus: no waiting.
#[test]
fn a_wheelchair_waits_for_the_ramp() {
    let c = cabin("ramp", 0.35, false);
    let chair = human("ramp-chair", "wheelchair");
    let walker = human("ramp-walker", "");
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.people = vec![person(&chair, 1, Task::WalkingToBus, DVec3::ZERO), person(&walker, 2, Task::WalkingToBus, DVec3::ZERO)];
    h.player_ramps = vec![super::ramp::Ramp { var: "pandus_2_mov".into(), entry: Some(1), exit: Some(0), door: 2 }];
    let b = bus(&c, -2.0, 0.0);
    h.see_ramps(&[0.0], 0.1);
    assert!(h.waits_for_ramp(0, &b, 1, true), "in: the ramp is in");
    assert_eq!(h.ramp_wanted, Some((true, 2)));
    assert!(h.waits_for_ramp(0, &b, 0, false), "out alike");
    assert!(!h.waits_for_ramp(0, &b, 0, true), "the front door has none");
    assert!(!h.waits_for_ramp(1, &b, 1, true), "somebody on foot");
    let mut ai = b.clone();
    ai.id = BusId::Ai(3);
    assert!(!h.waits_for_ramp(0, &ai, 1, true), "a timetable bus: its driver sees to it");
    // the driver folds it out: once it stands at the end, in they roll
    for k in 1..=10 {
        h.see_ramps(&[18.0 * k as f32], 0.1);
    }
    assert!(h.waits_for_ramp(0, &b, 1, true), "still turning");
    for _ in 0..5 {
        h.see_ramps(&[180.0], 0.1);
    }
    assert!(!h.waits_for_ramp(0, &b, 1, true), "out: in they roll");
    // told now and then while they wait, of one getting off only with the bus standing
    h.ramp_wanted = Some((false, 2));
    assert_eq!(h.take_ramp_hint(false), None);
    assert_eq!(h.take_ramp_hint(true), Some((false, 2)));
    assert_eq!(h.take_ramp_hint(true), None, "not every frame");
}

/// A driver who leaves without folding the ramp out has left the passenger in the wheelchair
/// behind - not excused as at a full bus.
#[test]
fn leaving_without_the_ramp_leaves_the_wheelchair_behind() {
    let c = cabin("ramp-left", 0.35, false);
    let chair = human("ramp-left-chair", "wheelchair");
    let mut h = PeopleSim::new(Path::new("/nonexistent"), 200);
    h.stops.insert(7, stop());
    h.people = vec![person(&chair, 1, Task::WalkingToBusstop, DVec3::new(2.3, 0.6, 0.0))];
    h.pax_mut(0).unwrap().ramp_wait = Some(BusId::Player);
    let mut y = -10.0;
    while y < 20.0 {
        let b = bus(&c, y, 10.0);
        h.time += 0.25;
        h.last_buses = Arc::new(vec![b.clone()]);
        h.passed_tick(&[b]);
        y += 2.5;
    }
    let left = h.take_left_behind();
    assert_eq!(left.iter().map(|l| (l.count, l.full)).collect::<Vec<_>>(), [(1, false)], "{left:?}");
    assert_eq!(h.pax(0).unwrap().ramp_wait, None, "counted once");
}
