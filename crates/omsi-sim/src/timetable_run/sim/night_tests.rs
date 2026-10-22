//! Night tours in the lists (NexiconDK, from openOMSI_DK): a tour is offered on its own days,
//! and the day before's night tour only after midnight while it is still under way. A
//! Friday tour (784) with trips at 24:10 and 25:10 is not offered on Thursday evening, nor a
//! Saturday tour (800) on Friday evening; Thursday's (776) is gone once its night is over.

use super::*;

struct NightWorld {
    dir: PathBuf,
    ai: omsi_map::AiLists,
    positions: crate::world_data::ObjectPlaces,
}

impl TimetableWorld for NightWorld {
    fn root(&self) -> &Path {
        &self.dir
    }
    fn map_dir(&self) -> &Path {
        &self.dir
    }
    fn chrono_dirs(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    fn ailists(&self) -> &omsi_map::AiLists {
        &self.ai
    }
    fn raw_tiles(&self) -> Vec<(i32, i32)> {
        Vec::new()
    }
    fn has_tile(&self, _: (i32, i32)) -> bool {
        false
    }
    fn date(&self) -> i32 {
        20261009
    }
    fn object_positions(&self) -> &crate::world_data::ObjectPlaces {
        &self.positions
    }
    fn stop_side(&self, _: i64) -> f32 {
        0.0
    }
}

/// Tours of line TEST, the trip `T` (40 min): Friday's at 22:00, 23:00, 24:10, 25:10, and
/// Thursday's and Saturday's five and ten minutes later.
fn night_map() -> PathBuf {
    // (a folder of each test's own: the tests run side by side and each removes its folder)
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("omsihub-night-{}-{n}", std::process::id()));
    let tt = dir.join("TTData");
    std::fs::create_dir_all(&tt).unwrap();
    std::fs::write(tt.join("T.ttp"), "[trip]\r\n\r\nNochem\r\nTEST\r\n\r\n[profile]\r\nStandard\r\n40\r\n").unwrap();
    let mut ttl = String::new();
    for (name, mask, off) in [("TEST-FRE", 784, 0), ("TEST-TOR", 776, 5), ("TEST-LOR", 800, 10)] {
        ttl += &format!("[newtour]\r\n{name}\r\nBus\r\n{mask}\r\n\r\n");
        for m in [1320, 1380, 1450, 1510] {
            ttl += &format!("[addtrip]\r\nT\r\n0\r\n{}.000\r\n\r\n", m + off);
        }
    }
    std::fs::write(tt.join("TEST.ttl"), ttl).unwrap();
    dir
}

/// The timetable on a Friday (9 October 2026) at `h:m`.
fn friday_at(h: f64, m: f64) -> (ScheduleSim, PathBuf) {
    let dir = night_map();
    let world = NightWorld { dir: dir.clone(), ai: Default::default(), positions: Default::default() };
    let mut clock = crate::SimClock::default();
    clock.set_date(2026, 10, 9);
    assert_eq!(clock.weekday(), 4, "9 October 2026 is a Friday");
    clock.time = ((h * 60.0 + m) * 60.0) as _;
    let mut s = ScheduleSim::new(&dir, &world, &clock);
    s.begin_tick(clock.time as f64);
    (s, dir)
}

fn tour<'a>(s: &'a ScheduleSim, name: &str) -> &'a omsi_timetable::Tour {
    s.data.lines.iter().flat_map(|l| &l.tours).find(|t| t.number == name).unwrap_or_else(|| panic!("no tour {name}"))
}

#[test]
fn the_lists_offer_a_night_tour_on_its_own_day_not_the_evening_before() {
    let (mut s, dir) = friday_at(22.0, 45.0);
    // Friday evening: Friday's tour only - Thursday's night is over, Saturday's not begun
    assert!(s.tour_available(tour(&s, "TEST-FRE")));
    assert!(!s.tour_available(tour(&s, "TEST-TOR")));
    assert!(!s.tour_available(tour(&s, "TEST-LOR")), "Saturday's night tour is offered on Friday evening");
    // Saturday 0:20: Friday's tour is under way (its 0:10 and 1:10), Saturday's own is today's
    s.begin_tick(DAY + 20.0 * 60.0);
    assert!(s.tour_available(tour(&s, "TEST-FRE")));
    assert!(!s.tour_available(tour(&s, "TEST-TOR")));
    assert!(s.tour_available(tour(&s, "TEST-LOR")));
    // the lists after midnight: Friday's night tour is yesterday's, Saturday's is tonight's
    assert!(s.tour_runs_yesterday(tour(&s, "TEST-FRE")) && !s.tour_runs_today(tour(&s, "TEST-FRE")));
    assert!(s.tour_runs_today(tour(&s, "TEST-LOR")) && !s.tour_runs_yesterday(tour(&s, "TEST-LOR")));
    // Saturday 3:00: Friday's night is over
    s.begin_tick(DAY + 3.0 * 3600.0);
    assert!(!s.tour_available(tour(&s, "TEST-FRE")));
    assert!(s.tour_available(tour(&s, "TEST-LOR")));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn thursdays_night_tour_is_not_offered_on_friday_afternoon() {
    let (s, dir) = friday_at(16.0, 0.0);
    assert!(!s.tour_available(tour(&s, "TEST-TOR")), "Thursday's tour is offered on Friday afternoon");
    assert!(s.tour_available(tour(&s, "TEST-FRE")));
    std::fs::remove_dir_all(dir).ok();
}

/// The trip a night tour starts with at 2:18: the 2:50 one, not the evening's first.
#[test]
fn a_night_tours_trip_now_is_the_one_after_midnight() {
    let h = |h: f64, m: f64| (h * 60.0 + m) * 60.0;
    // 19:29, 21:00, 25:10 (1:10), 26:50 (2:50)
    let order = [(0, h(19.0, 29.0), h(20.0, 10.0)), (1, h(21.0, 0.0), h(21.0, 40.0)), (2, h(25.0, 10.0), h(25.0, 50.0)), (3, h(26.0, 50.0), h(27.0, 30.0))];
    assert_eq!(super::tours::trip_at(&order, h(2.0, 18.0), true), 3);
    assert_eq!(super::tours::trip_at(&order, h(1.0, 20.0), true), 2);
    assert_eq!(super::tours::trip_at(&order, h(20.0, 30.0), true), 1);
    // the afternoon before it starts: its first trip
    assert_eq!(super::tours::trip_at(&order, h(15.0, 0.0), true), 0);
    // a tour that did not run the night before: at 2:18 its first trip, this evening
    assert_eq!(super::tours::trip_at(&order, h(2.0, 18.0), false), 0);
}
