//! (NexiconDK, 1 November 2026) A chrono scenario's lines come and go with its dates while the
//! timetable runs on: a dedicated server started the day before a match day gets the match's
//! shuttle line at midnight, the line the scenario takes off stops, and the day after both are
//! as before. Only the year in the file counts.

use super::*;

struct ChronoWorld {
    dir: PathBuf,
    ai: omsi_map::AiLists,
    positions: crate::world_data::ObjectPlaces,
}

impl TimetableWorld for ChronoWorld {
    fn root(&self) -> &Path {
        &self.dir
    }
    fn map_dir(&self) -> &Path {
        &self.dir
    }
    fn chrono_dirs(&self) -> Vec<PathBuf> {
        omsi_map::active_chrono_dirs(&self.dir, 20261031)
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
        20261031
    }
    fn object_positions(&self) -> &crate::world_data::ObjectPlaces {
        &self.positions
    }
    fn stop_side(&self, _: i64) -> f32 {
        0.0
    }
}

/// A line of one tour (every day) with the trip `T` at `minute`.
fn line(minute: u32) -> String {
    format!("[newtour]\r\n1\r\nBus\r\n1023\r\n\r\n[addtrip]\r\nT\r\n0\r\n{minute}.000\r\n\r\n")
}

/// Lines 5 and 6 every day; the match of 1 November 2026 brings "Sonderverkehr" and takes
/// line 6 off; those of 2 November 2025 and 3 November 2024 (the years in their files) never
/// run.
fn chrono_map() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("omsihub-chrono-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let tt = dir.join("TTData");
    std::fs::create_dir_all(&tt).unwrap();
    std::fs::write(tt.join("T.ttp"), "[trip]\r\n\r\nStadion\r\n5\r\n\r\n[profile]\r\nStandard\r\n20\r\n").unwrap();
    std::fs::write(tt.join("5.ttl"), line(600)).unwrap();
    std::fs::write(tt.join("6.ttl"), line(610)).unwrap();
    for (folder, date) in [("1_FCA/01.11.26 - 14.00 Uhr", "20261101"), ("1_FCA/02.11.25 - 14.00 Uhr", "20251102"), ("1_FCA/03.11.24 - 14.00 Uhr", "20241103")] {
        let c = dir.join("Chrono").join(folder);
        std::fs::create_dir_all(c.join("TTData")).unwrap();
        std::fs::write(c.join("Chrono.cfg"), format!("[name]\r\nFCA {date}\r\n\r\n[startdate]\r\n{date}\r\n\r\n[enddate]\r\n{date}\r\n\r\n[deactivate_lines]\r\n1\r\n6\r\n")).unwrap();
        std::fs::write(c.join("TTData").join("Sonderverkehr.ttl"), line(720)).unwrap();
    }
    dir
}

fn running(s: &ScheduleSim) -> Vec<String> {
    let mut v: Vec<String> = (0..s.departures.len()).filter(|&i| s.runs(i)).map(|i| s.departures[i].line.clone()).collect();
    v.sort();
    v
}

fn lines(s: &ScheduleSim) -> Vec<String> {
    let mut v: Vec<String> = s.data.lines.iter().map(|l| l.name.clone()).collect();
    v.sort();
    v
}

#[test]
fn a_match_days_lines_come_and_go_at_midnight() {
    let dir = chrono_map();
    let world = ChronoWorld { dir: dir.clone(), ai: Default::default(), positions: Default::default() };
    let mut clock = crate::SimClock::default();
    clock.set_date(2026, 10, 31);
    let mut s = ScheduleSim::new(&dir, &world, &clock);
    assert_eq!(lines(&s), ["5", "6"], "the day before");
    assert_eq!(running(&s), ["5", "6"]);
    assert!(s.deactivated.is_empty());
    // midnight: the match day
    clock.next_day();
    s.set_day(&clock);
    assert_eq!(lines(&s), ["5", "Sonderverkehr"], "the match day: its line, line 6 off");
    assert_eq!(running(&s), ["5", "Sonderverkehr"]);
    assert_eq!(s.deactivated.iter().map(|(l, _)| l.as_str()).collect::<Vec<_>>(), ["6"]);
    // the day after: as before
    clock.next_day();
    s.set_day(&clock);
    assert_eq!(lines(&s), ["5", "6"], "the day after");
    assert_eq!(running(&s), ["5", "6"]);
    // 2 November 2025's match is taken in (not a year gone) but never ran; 2024's is left out
    assert_eq!(s.chrono.versions.iter().filter(|v| v.folder > 0).count(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}
