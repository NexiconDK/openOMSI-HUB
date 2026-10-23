//! Bus Company Online (NexiconDK, phase 1 of docs/omsihub/BUS_COMPANY_ONLINE.md): a bus company
//! that lives on a server and runs there for good, with the real clock - its days, its plan and
//! its books going on whether anybody is there or not, and its own buses driving its tours in
//! the server's world.
//!
//! `Host` is the company as the server keeps it. Opened, it catches up with the real time it was
//! away (`clock::catch_up`: the days between closed, the dispatcher deciding), and from then on
//! each `tick` moves its time on to where the real clock has it (`clock::advance`, quick: nobody
//! is asked), tells what the server's world reported of its buses as it comes (`store::peek_live`,
//! `clock::tell_live`; the day's close books it), closes its days at their midnights, writes the
//! day's plan for the world (`plan::save_live_plan`: the tours with their buses, paints and
//! numbers - one more `plan_gen` each time) and saves itself now and then and after each close,
//! so that a server started again goes on where it was.
//!
//! The server's game clock follows the company's (`now`): its date and minute are the world's.
//!
//! What the launcher's pages do besides on their tick (a line's change on its day, the dealer's
//! deliveries, the control room's messages) is not run here yet: the phases after this one.

use super::clock::{self, Away, World};
use super::day::DayReport;
use super::model::Company;
use super::{network, plan, store};
use crate::LineInfo;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// How often the company is saved while nothing closes (real seconds): a server stopped hard
/// loses at most this much of its day (the day's close saves at once).
pub const SAVE_EVERY: i64 = 60;

/// The company on a server.
pub struct Host {
    /// The server's company folder (`store::dir` in it): the company, its live file, its plan.
    pub data: PathBuf,
    /// The map's folder as the server has it (its timetables read from there; None: through the
    /// launcher's OMSI folder, as on a player's computer).
    pub map_dir: Option<PathBuf>,
    pub company: Company,
    /// The timetables read so far (`store::Disk::with_read`).
    read: Vec<(String, String, Vec<LineInfo>)>,
    /// The date the world's plan was written for.
    planned: Option<String>,
    /// One more each time the world's plan is written: the world takes it in anew.
    pub plan_gen: u64,
    /// The live events of the day told already: (the day, how many).
    seen: (String, usize),
    /// When it was saved last (real seconds), and whether it changed since.
    saved_at: i64,
    dirty: bool,
}

/// What a step came to.
#[derive(Default)]
pub struct Stepped {
    /// The days closed on the way.
    pub closed: Vec<DayReport>,
    /// Live events told (the world's reports of the company's buses).
    pub told: usize,
    /// The world's plan was written (a new day, or the first).
    pub planned: bool,
    /// The company was saved.
    pub saved: bool,
}

impl Host {
    /// The company `id` of the server's folder `data`, caught up with the real clock (`real`,
    /// Unix seconds) and saved, its day's plan written. A company that did not run with the real
    /// clock does from now on (a server's always does), from where it is.
    /// `map_dir`: the map's folder as the server has it.
    pub fn open(data: &Path, id: &str, real: i64, map_dir: Option<&Path>) -> Result<(Host, Option<Away>)> {
        let c = store::load(data, id).with_context(|| format!("the company {id} in {}", data.display()))?;
        let mut h = Host::new(data, c, real);
        h.map_dir = map_dir.map(Path::to_path_buf);
        let mut disk = store::Disk::with_read(data, std::mem::take(&mut h.read)).on_map(map_dir);
        let r = h.open_with(real, &mut disk);
        h.done_with(disk);
        r.map(|away| (h, away))
    }

    /// `open` with the world given (the timetable and the night: a test's own).
    pub fn open_in(data: &Path, id: &str, real: i64, w: &mut dyn World) -> Result<(Host, Option<Away>)> {
        let c = store::load(data, id)?;
        let mut h = Host::new(data, c, real);
        let away = h.open_with(real, w)?;
        Ok((h, away))
    }

    fn new(data: &Path, mut company: Company, real: i64) -> Host {
        if !company.clock.real.on {
            clock::set_realtime(&mut company, true);
        }
        let seen = (company.date.clone(), 0);
        Host { data: data.to_path_buf(), map_dir: None, company, read: Vec::new(), planned: None, plan_gen: 0, seen, saved_at: real, dirty: false }
    }

    fn open_with(&mut self, real: i64, w: &mut dyn World) -> Result<Option<Away>> {
        let away = clock::catch_up(&mut self.company, real, w).map_err(anyhow::Error::msg)?;
        self.seen = (self.company.date.clone(), 0);
        self.write_plan(w)?;
        self.save(real)?;
        Ok(away)
    }

    /// The company's time moved on to the real clock's (`real`, Unix seconds), with the world's
    /// reports told, its days closed and planned, and saved as it is due.
    pub fn tick(&mut self, real: i64) -> Result<Stepped> {
        let (data, map_dir) = (self.data.clone(), self.map_dir.clone());
        let mut disk = store::Disk::with_read(&data, std::mem::take(&mut self.read)).on_map(map_dir.as_deref());
        let r = self.tick_in(real, &mut disk);
        self.done_with(disk);
        r
    }

    /// `tick` with the world given.
    pub fn tick_in(&mut self, real: i64, w: &mut dyn World) -> Result<Stepped> {
        let mut s = Stepped::default();
        let before = self.company.date.clone();
        let c = &mut self.company;
        // never in step yet: in step from now (a company that did not run with the real clock)
        if c.clock.real.synced.is_none() {
            clock::sync(c, real);
            self.dirty = true;
        }
        // what the world reported of the company's buses since: told as it comes
        if self.seen.0 != c.date {
            self.seen = (c.date.clone(), 0);
        }
        let live = store::peek_live(&self.data, &c.id, self.seen.1);
        if !live.is_empty() {
            self.seen.1 += live.len();
            s.told = clock::tell_live(c, &live);
            self.dirty = true;
        }
        // its time on to the real clock's: a long way as an absence is caught up, else step by
        // step (the dispatcher deciding: nobody is asked on a server)
        let now = clock::now(c);
        if let Some(to) = clock::course(c, real).filter(|to| *to > now) {
            if to - now >= clock::AWAY_TOLD {
                if let Some(away) = clock::catch_up(c, real, w).map_err(anyhow::Error::msg)? {
                    s.closed = away.reports;
                }
            } else {
                s.closed = clock::advance(c, to, w, true).map_err(anyhow::Error::msg)?.reports;
            }
            self.dirty = true;
        }
        let new_day = self.company.date != before;
        if new_day {
            self.seen = (self.company.date.clone(), 0);
        }
        if new_day || self.planned.as_deref() != Some(self.company.date.as_str()) {
            self.write_plan(w)?;
            s.planned = true;
        }
        if new_day || (self.dirty && real - self.saved_at >= SAVE_EVERY) {
            self.save(real)?;
            s.saved = true;
        }
        Ok(s)
    }

    /// The day's plan for the world: the company's tours of its date with their buses (or
    /// dropped), as the launcher writes it for a drive (`plan::live_plan`).
    fn write_plan(&mut self, w: &mut dyn World) -> Result<()> {
        // (the day entered first: its roster filled or repaired, as the clock does at its start)
        clock::enter_day(&mut self.company, w).map_err(anyhow::Error::msg)?;
        let c = &self.company;
        let date = c.date.clone();
        let lines = w.lines(c, &date).map_err(anyhow::Error::msg)?;
        let tours = network::tours_of_day(c, &lines, &date);
        let day = plan::day_plan(c, &date, tours, &[], &[], false);
        plan::save_live_plan(&self.data, &plan::live_plan(c, &day))?;
        self.planned = Some(date);
        self.plan_gen += 1;
        Ok(())
    }

    /// Saved now.
    pub fn save(&mut self, real: i64) -> Result<()> {
        store::save(&self.data, &self.company)?;
        self.saved_at = real;
        self.dirty = false;
        Ok(())
    }

    /// The disk's timetables kept, and the live file a close took in let go (saved with it).
    fn done_with(&mut self, disk: store::Disk) {
        let (read, took) = disk.into_parts();
        self.read = read;
        if let Some(date) = took {
            // (a close saves the company: `tick_in`; saved again here should the save above
            // have been skipped by an error)
            if self.dirty {
                let _ = store::save(&self.data, &self.company);
                self.dirty = false;
            }
            store::forget_taken(&self.data, &self.company.id, &date);
        }
    }

    /// The company's date and minute of the day: the server's world runs on them.
    pub fn now(&self) -> (String, u32) {
        (self.company.date.clone(), self.company.clock.minute)
    }
}

#[cfg(test)]
mod tests {
    use super::super::concessions::tests::line;
    use super::super::market::{self, MarketBus, Payment};
    use super::super::model::{GameMode, Licence};
    use super::super::staff::{applicants, hire};
    use super::super::{day, depot, found, Founding};
    use super::*;
    use crate::LineInfo;

    /// The timetable of every day the same; the night as the store runs it.
    struct Fake(Vec<LineInfo>);

    impl World for Fake {
        fn lines(&mut self, _: &Company, _: &str) -> std::result::Result<Vec<LineInfo>, String> {
            Ok(self.0.clone())
        }

        fn close_day(&mut self, c: &mut Company, lines: &[LineInfo]) -> std::result::Result<DayReport, String> {
            network::refresh_lines(c, lines);
            let date = c.date.clone();
            let tours = network::tours_of_day(c, lines, &date);
            let r = day::close_day(c, tours, &[]);
            Ok(depot::after_close(c, r, lines))
        }
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("openomsi-online-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    /// A company of a line in service with its buses and drivers, in the easy mode (its own
    /// routine fills the roster every morning: `autopilot`), saved in `data`.
    fn company(data: &Path) -> (String, Fake) {
        let lines = vec![line("Linie5", "5", 3, true), line("Linie7", "7", 2, true)];
        let mut c = found(&Founding { name: "Online".into(), game_mode: GameMode::Casual, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.cash += 5_000_000_00;
        network::add_line(&mut c, &lines[0], None).unwrap();
        network::start_service(&mut c, "Linie5", 0).unwrap();
        c.planning.auto = true;
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        for _ in 0..4 {
            market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
        }
        let id = c.id.clone();
        while c.staff.len() < 8 {
            for a in applicants(&c) {
                if c.staff.len() < 8 && a.licence == Licence::D {
                    hire(&mut c, &a).unwrap();
                }
            }
            c.id.push('x');
            c.taken = Default::default();
        }
        c.id = id.clone();
        for e in c.staff.iter_mut() {
            e.sick_until = None;
            e.holiday_until = None;
        }
        // (the company's morning: in step with the real clock from when it is opened)
        c.clock.minute = 5 * 60;
        store::save(data, &c).unwrap();
        (id, Fake(lines))
    }

    /// Real time: Unix seconds `hours` after the moment the tests start from (to the minute).
    fn at(hours: f64) -> i64 {
        1_800_000_000 + (hours * 60.0).round() as i64 * 60
    }

    /// Opened, the company goes in step with the real clock and writes its day's plan for the
    /// world; its time then runs as the real time does, a minute a minute, and the plan is
    /// written once a day.
    #[test]
    fn a_server_company_runs_with_the_real_clock() {
        let data = temp("clock");
        let (id, mut w) = company(&data);
        let (mut h, away) = Host::open_in(&data, &id, at(0.0), &mut w).unwrap();
        assert!(away.is_none(), "never in step before: nothing caught up");
        assert_eq!(h.now(), ("2024-03-04".to_string(), 5 * 60));
        assert_eq!(h.plan_gen, 1);
        let p = plan::live_plans(&data).into_iter().find(|p| p.company == id).expect("the world's plan");
        assert_eq!(p.date, "2024-03-04");
        assert!(p.tours.iter().any(|t| t.vehicle.is_some()), "its buses on its tours: {:?}", p.tours);
        // ten minutes on: ten minutes of the company's day, the plan as it was
        let s = h.tick_in(at(10.0 / 60.0), &mut w).unwrap();
        assert_eq!(h.now().1, 5 * 60 + 10);
        assert!(!s.planned && s.closed.is_empty() && h.plan_gen == 1);
        // (an hour: as an absence - caught up the same)
        h.tick_in(at(1.0 + 10.0 / 60.0), &mut w).unwrap();
        assert_eq!(h.now().1, 6 * 60 + 10);
        let _ = std::fs::remove_dir_all(&data);
    }

    /// At midnight the day is closed and booked, the next day planned for the world and the
    /// company saved; a server away for two days catches them up, each closed.
    #[test]
    fn its_days_are_closed_planned_and_saved() {
        let data = temp("days");
        let (id, mut w) = company(&data);
        let (mut h, _) = Host::open_in(&data, &id, at(0.0), &mut w).unwrap();
        let history = h.company.history.len();
        // to 00:05 of the next day
        let s = h.tick_in(at(19.0 + 5.0 / 60.0), &mut w).unwrap();
        assert_eq!(s.closed.len(), 1, "the day closed");
        assert!(s.planned && s.saved);
        assert_eq!(h.now(), ("2024-03-05".to_string(), 5));
        assert_eq!(h.company.history.len(), history + 1);
        assert!(plan::live_plans(&data).iter().any(|p| p.company == id && p.date == "2024-03-05"), "the new day's plan");
        assert_eq!(store::load(&data, &id).unwrap().date, "2024-03-05", "saved");
        // two days on: both closed
        let s = h.tick_in(at(19.0 + 48.0 + 5.0 / 60.0), &mut w).unwrap();
        assert_eq!(s.closed.len(), 2);
        assert_eq!(h.now(), ("2024-03-07".to_string(), 5));
        let _ = std::fs::remove_dir_all(&data);
    }

    /// A server stopped and started again goes on where the company was saved: the time it was
    /// away caught up, nothing closed twice.
    #[test]
    fn a_server_started_again_goes_on_where_it_was() {
        let data = temp("restart");
        let (id, mut w) = company(&data);
        let (mut h, _) = Host::open_in(&data, &id, at(0.0), &mut w).unwrap();
        h.tick_in(at(20.0), &mut w).unwrap();
        assert_eq!(h.now(), ("2024-03-05".to_string(), 60));
        // (the minutes after the last save are the server's; saved on the way down)
        h.save(at(20.0)).unwrap();
        let days = h.company.history.len();
        drop(h);
        // five hours later
        let (h, away) = Host::open_in(&data, &id, at(25.0), &mut w).unwrap();
        let away = away.expect("the five hours caught up");
        assert_eq!((away.caught.from, away.caught.to), (clock::moment("2024-03-05", 60), clock::moment("2024-03-05", 6 * 60)));
        assert_eq!(h.now(), ("2024-03-05".to_string(), 6 * 60));
        assert_eq!(h.company.history.len(), days, "no day closed again");
        let _ = std::fs::remove_dir_all(&data);
    }

    /// What the world reports of the company's buses is told once, as it comes, and booked at
    /// the day's close.
    #[test]
    fn the_worlds_reports_are_told_once() {
        let data = temp("live");
        let (id, mut w) = company(&data);
        let (mut h, _) = Host::open_in(&data, &id, at(0.0), &mut w).unwrap();
        let trip = day::LiveEvent::Trip { line: "5".into(), tour: "1".into(), vehicle: Some(1), passengers: 31, km: 10.0, delay: 45.0, completed: true, dep: Some(6 * 60) };
        store::append_live(&data, &id, &trip).unwrap();
        store::append_live(&data, &id, &trip).unwrap();
        assert_eq!(h.tick_in(at(0.1), &mut w).unwrap().told, 2);
        assert_eq!(h.tick_in(at(0.2), &mut w).unwrap().told, 0, "told once");
        store::append_live(&data, &id, &trip).unwrap();
        assert_eq!(h.tick_in(at(0.3), &mut w).unwrap().told, 1);
        let _ = std::fs::remove_dir_all(&data);
    }

    /// A company on a real map, run on its server for a day and a half: how long its steps take
    /// (`OMSI_COMPANY_MAP_DIR=<a map's folder> cargo test -p omsi-launcher-core --lib
    /// online::tests::on_a_real_map -- --ignored --nocapture`).
    #[test]
    #[ignore = "needs a map's folder: OMSI_COMPANY_MAP_DIR"]
    fn on_a_real_map() {
        let Some(dir) = std::env::var_os("OMSI_COMPANY_MAP_DIR").map(PathBuf::from) else { return };
        let data = temp("real");
        let date = "2024-03-04";
        let t = std::time::Instant::now();
        let all = crate::lines_in_map_dir(&dir, date).unwrap();
        eprintln!("the map's lines read in {:.2} s: {} lines, {} tours today", t.elapsed().as_secs_f32(), all.len(), all.iter().map(|l| l.tours.iter().filter(|t| t.today).count()).sum::<usize>());
        let mut c = found(&Founding { name: "Server".into(), game_mode: GameMode::Casual, date: date.into(), map: "maps/X/global.cfg".into(), ..Default::default() }, "Server");
        c.cash += 50_000_000_00;
        // the three lines with the most tours today
        let mut lines: Vec<&LineInfo> = all.iter().filter(|l| l.user_allowed).collect();
        lines.sort_by_key(|l| std::cmp::Reverse(l.tours.iter().filter(|t| t.today).count()));
        let mut tours = 0;
        for l in lines.iter().take(3) {
            if network::add_line(&mut c, l, None).is_ok() && network::start_service(&mut c, &l.name, 0).is_ok() {
                tours += l.tours.iter().filter(|t| t.today).count();
            }
        }
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        // (as many as the depot has room for)
        for _ in 0..tours + 2 {
            if market::buy_new(&mut c, &bus, Payment::Cash, "").is_err() {
                break;
            }
        }
        let id = c.id.clone();
        while c.staff.len() < tours * 2 + 2 {
            for a in applicants(&c) {
                if c.staff.len() < tours * 2 + 2 && a.licence == Licence::D {
                    hire(&mut c, &a).unwrap();
                }
            }
            c.id.push('x');
            c.taken = Default::default();
        }
        c.id = id.clone();
        c.clock.minute = 5 * 60;
        store::save(&data, &c).unwrap();
        let t = std::time::Instant::now();
        let (mut h, _) = Host::open(&data, &id, at(0.0), Some(&dir)).unwrap();
        eprintln!("opened in {:.2} s: {} lines in service, {} tours today, {} buses, {} staff", t.elapsed().as_secs_f32(), h.company.lines.len(), tours, h.company.fleet.len(), h.company.staff.len());
        let p = plan::live_plans(&data).into_iter().find(|p| p.company == id).unwrap();
        eprintln!("the world's plan: {} tours, {} with a bus, {} dropped", p.tours.len(), p.tours.iter().filter(|t| t.vehicle.is_some()).count(), p.tours.iter().filter(|t| t.dropped).count());
        // a step a minute through the morning
        let t = std::time::Instant::now();
        for m in 1..=60 {
            h.tick(at(m as f64 / 60.0)).unwrap();
        }
        eprintln!("an hour, a step a minute: {:.1} ms a step", t.elapsed().as_secs_f64() * 1000.0 / 60.0);
        let t = std::time::Instant::now();
        let s = h.tick(at(19.0 + 5.0 / 60.0)).unwrap();
        eprintln!("on to 00:05 of the next day: {:.2} s, {} day closed, result {:?}", t.elapsed().as_secs_f32(), s.closed.len(), s.closed.first().map(|r| r.result));
        assert_eq!(h.now(), ("2024-03-05".to_string(), 5));
        let p = plan::live_plans(&data).into_iter().find(|p| p.company == id && p.date == "2024-03-05").expect("the next day's plan");
        assert!(p.tours.iter().any(|t| t.vehicle.is_some()));
        let size = std::fs::metadata(store::path_of(&data, &id)).map(|m| m.len()).unwrap_or(0);
        eprintln!("the company's file: {} KB", size / 1024);
        let _ = std::fs::remove_dir_all(&data);
    }
}
