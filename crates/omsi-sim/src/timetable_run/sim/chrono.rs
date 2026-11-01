//! (NexiconDK, 1 November 2026) The chrono scenarios' lines day by day. A map's `Chrono`
//! folders bring lines of their own on their dates (a match day's shuttles, a fair's), replace
//! one of the map's or take one off (`[deactivate_lines]`). The timetable took the lines of the
//! folders in force on the day it was made and kept them: a dedicated server that runs for
//! weeks, or a drive past midnight, never got a scenario that came into force later, nor lost
//! one that ended. Now every line a scenario of the session may come to run is a version of
//! its own (`omsi_timetable::LineVersion`), its departures kept with the rest, and which run is
//! decided at each new day by the folders in force then (`set_day`) - only the year in the
//! file counts, as in the original.

use omsi_map::ailists::ChronoCfg;
use omsi_timetable::{lines_in_force, lines_of, LineVersion, TimetableData};
use std::path::{Path, PathBuf};

/// The chrono folders a session takes in, their lines' versions and which of them run.
#[derive(Default)]
pub(super) struct ChronoLines {
    /// The folders, in the game's order, with their `Chrono.cfg` (dates, lines taken off).
    folders: Vec<(PathBuf, ChronoCfg)>,
    pub(super) versions: Vec<LineVersion>,
    /// The lines a folder takes off: (line, folder index - 1 the first chrono folder).
    off: Vec<(String, usize)>,
    /// Per version: whether it runs today, and yesterday (its night trips after midnight).
    pub(super) today: Vec<bool>,
    pub(super) yesterday: Vec<bool>,
}

/// How far back an ended scenario is still taken in (date code: a year): the clock may be set
/// back a little, never to a scenario long over.
const ENDED_KEPT: i32 = 10000;

impl ChronoLines {
    /// The map's timetable with every chrono folder that is in force on `date` or may come to
    /// be (one that ended more than a year before is left out), and `start`: the folders the
    /// world has in force (one not under the map's `Chrono` folder - a test's - always is).
    /// The lines of the data are those running on `date` (`yesterday`: the day before).
    pub(super) fn load(map_dir: &Path, start: &[PathBuf], date: i32, yesterday: i32) -> (TimetableData, ChronoLines) {
        let mut folders: Vec<(PathBuf, ChronoCfg)> = omsi_map::all_chrono(map_dir)
            .into_iter()
            .filter(|(d, c)| start.contains(d) || c.end_date == 0 || c.end_date >= date - ENDED_KEPT)
            .collect();
        for d in start {
            if !folders.iter().any(|(x, _)| x == d) {
                folders.push((d.clone(), ChronoCfg { start_date: 1, ..Default::default() }));
            }
        }
        let mut c = ChronoLines { folders, ..Default::default() };
        let on = c.on(date);
        let given: Vec<(PathBuf, bool)> = c.folders.iter().zip(&on[1..]).map(|((d, _), o)| (d.clone(), *o)).collect();
        let (mut data, versions) = TimetableData::load_versions(map_dir, &given);
        c.off = c.folders.iter().enumerate().flat_map(|(k, (_, cfg))| cfg.deactivate_lines.iter().map(move |l| (l.clone(), k + 1))).collect();
        // a version a later folder in force for good (open-ended) replaces or takes off never
        // runs again: left out (Berlin-Spandau's scenarios each bring the whole timetable anew)
        let forever: Vec<bool> = std::iter::once(true).chain(c.folders.iter().zip(&on[1..]).map(|((_, cfg), o)| *o && cfg.end_date == 0)).collect();
        let shadowed = |v: &LineVersion| {
            let name = v.line.name.trim();
            versions.iter().any(|w| w.folder > v.folder && forever[w.folder] && w.line.name.trim().eq_ignore_ascii_case(name))
                || c.off.iter().any(|(l, j)| *j > v.folder && forever[*j] && l.trim().eq_ignore_ascii_case(name))
        };
        let keep: Vec<bool> = versions.iter().map(|v| !shadowed(v)).collect();
        c.versions = versions.into_iter().zip(keep).filter(|(_, k)| *k).map(|(v, _)| v).collect();
        c.set_day(date, yesterday);
        data.lines = lines_of(&c.versions, &c.today);
        let other = c.versions.iter().filter(|v| v.folder > 0).count();
        if other > 0 {
            log::info!("chrono: {} lines of {} chrono folders taken in, {} running today", other, c.folders.len(), c.versions.iter().zip(&c.today).filter(|(v, t)| v.folder > 0 && **t).count());
        }
        (data, c)
    }

    /// Per folder (0 the map's own, k the k-th chrono folder): in force on `date`.
    fn on(&self, date: i32) -> Vec<bool> {
        std::iter::once(true).chain(self.folders.iter().map(|(_, c)| c.active_on(date))).collect()
    }

    /// A new day: which versions run on it and the day before. True when today's changed.
    pub(super) fn set_day(&mut self, date: i32, yesterday: i32) -> bool {
        let today = lines_in_force(&self.versions, &self.off, &self.on(date));
        self.yesterday = lines_in_force(&self.versions, &self.off, &self.on(yesterday));
        let changed = today != self.today;
        if changed && !self.today.is_empty() {
            for (k, v) in self.versions.iter().enumerate().filter(|(k, v)| v.folder > 0 && today[*k] != self.today[*k]) {
                let folder = self.folders[v.folder - 1].0.file_name().unwrap_or_default().to_string_lossy().into_owned();
                log::info!("chrono: line {} of '{folder}' {} on {date}", v.line.name, if today[k] { "runs" } else { "no longer runs" });
            }
        }
        self.today = today;
        changed
    }

    /// The lines running today.
    pub(super) fn lines(&self) -> Vec<omsi_timetable::Line> {
        lines_of(&self.versions, &self.today)
    }

    /// Whether version `v`'s departures run: today's, or yesterday's for a night trip of the
    /// day before. A departure of no version (a test's) runs.
    pub(super) fn runs(&self, v: usize, yesterday: bool) -> bool {
        (if yesterday { &self.yesterday } else { &self.today }).get(v).copied().unwrap_or(true)
    }

    /// The lines the folders in force on `date` take off, each with its folder: why a duty
    /// on such a line cannot be driven.
    pub(super) fn deactivated(&self, date: i32) -> Vec<(String, PathBuf)> {
        let on = self.on(date);
        self.off.iter().filter(|(_, j)| on[*j]).map(|(l, j)| (l.clone(), self.folders[*j - 1].0.clone())).collect()
    }
}
