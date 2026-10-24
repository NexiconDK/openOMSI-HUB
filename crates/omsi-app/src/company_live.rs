//! The player's bus company in the game (the launcher's company, `omsi_launcher_lib::company`):
//! time runs mixed - while the player drives, the company's tours on the map are run by the
//! AI with the company's own buses, and what they drive goes back to the company's books
//! (`company::day::record_live` through `store::append_live`); the days not played are
//! settled with "Close the day".
//!
//! The launcher writes the company's day for the game (`company::plan::LivePlan`, each tour
//! with its bus, paint, fleet number and plate, or dropped). The timetable takes it only in a
//! drive from the company's planning (`--company`, Luc: in the company's mode only its own
//! buses; in a duty or tour of one's own the map's as ever), when the map and the date are
//! the company's: those tours get that bus instead of one of the map's depot
//! (`Schedule::choose`), a dropped tour does not run, the map's other timetable buses stay in
//! their depots (`keeps_off`), and each trip a company bus drives to its end is reported
//! once.
//!
//! On a dedicated server (Bus Company Online, `crate::company_server`) every company lives in the
//! server's folder and the world is everybody's (`ServerCompanies`): each line belongs to at most
//! one company, whose tours on it run with its buses as its plan has them, while its lines' other
//! tours do not run (what its planners left open); the map's lines no company has run as the map
//! has them. The plans are taken in again whenever a company's host writes one anew
//! (`ScheduleSim::reload_company`), not only at midnight.

use omsi_launcher_lib::company::plan::{self, LivePlan, LiveTour};
use omsi_launcher_lib::company::store;
use omsi_sim::VehicleType;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Default)]
pub struct CompanyLive {
    map_dir: PathBuf,
    data: PathBuf,
    /// The company the game was started for (`--company`): only its tours run. None: a drive
    /// of the player's own, the map's timetable as ever.
    company: Option<String>,
    plan: Option<LivePlan>,
    /// A dedicated server's company (`for_server`): the map's other lines run.
    server: bool,
    /// The company's lines as its last plan had them (a server's: kept off the map's buses
    /// while the new day's plan is not written yet).
    lines: Vec<String>,
    /// The day the plan is for (yyyymmdd).
    day: i32,
    /// The plan is the company's own day: what its buses drive is reported (a later day's,
    /// for a duty driven ahead, only puts them on the road).
    reports: bool,
    /// The company's bus types, loaded the first time a tour wants one (None: did not load).
    types: HashMap<String, Option<Arc<VehicleType>>>,
    /// Departures already reported.
    reported: HashSet<usize>,
    /// The company's depot file as each bus folder has it (the timetable's `depot_file`).
    pub(crate) hofs: hashbrown::HashMap<(PathBuf, String), Option<Arc<omsi_vehicle::Hof>>>,
}

/// openOMSI's own data folder, `~/.openomsi` (the launcher's `data_dir`; not made here).
pub fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from).unwrap_or_default();
    home.join(".openomsi")
}

/// `20240304` as `2024-03-04`.
fn date_of(code: i32) -> String {
    format!("{:04}-{:02}-{:02}", code / 10000, (code / 100) % 100, code % 100)
}

impl CompanyLive {
    /// A drive for company `id` (`--company`): its plan of this map and date (from `data`, the
    /// launcher's data folder) - its own day's, else a duty driven ahead's.
    pub fn for_company(map_dir: &Path, date_code: i32, data: PathBuf, id: &str) -> CompanyLive {
        let mut c = CompanyLive { map_dir: map_dir.to_path_buf(), data, company: Some(id.trim().to_string()), ..Default::default() };
        c.set_day(date_code);
        c
    }

    /// A dedicated server's company `id` (Bus Company Online): its plan of this map and date,
    /// from the server's company folder `data`; the map's lines no company has run as ever.
    pub fn for_server(map_dir: &Path, date_code: i32, data: PathBuf, id: &str) -> CompanyLive {
        let mut c = CompanyLive { map_dir: map_dir.to_path_buf(), data, company: Some(id.trim().to_string()), server: true, ..Default::default() };
        c.set_day(date_code);
        c
    }

    /// The day moved on (midnight): the plan of the new day, if there is one. The same day
    /// again: its plan taken in anew (a server's host wrote it again, `plan_gen`).
    pub fn set_day(&mut self, date_code: i32) {
        self.plan = None;
        self.reports = false;
        // (the same day's plan taken in again: its trips told already stay told)
        if date_code != self.day {
            self.reported.clear();
        }
        self.day = date_code;
        let Some(id) = self.company.clone() else { return };
        let date = date_of(date_code);
        let fits = |p: &LivePlan| p.company == id && p.is_for(&self.map_dir, &date);
        if let Some(p) = plan::live_plans(&self.data).into_iter().find(|p| fits(p)) {
            self.plan = Some(p);
            self.reports = true;
        } else {
            self.plan = plan::drive_plan(&self.data, &id).filter(|p| fits(p));
        }
        if let Some(p) = self.plan.as_ref().filter(|p| !p.lines.is_empty()) {
            self.lines = p.lines.clone();
        } else if self.server && self.lines.is_empty() {
            // (a server started before the host wrote the day's plan: the lines of its last)
            let here = self.map_dir.file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
            let folder = |p: &str| p.replace('\\', "/").trim_end_matches("/global.cfg").rsplit('/').next().unwrap_or("").to_lowercase();
            if let Some(p) = plan::live_plans(&self.data).into_iter().find(|p| p.company == id && folder(&p.map) == here) {
                self.lines = p.lines;
            }
        }
        match (&self.plan, self.server) {
            (Some(p), true) => {
                let buses = p.tours.iter().filter(|t| t.vehicle.is_some()).count();
                let dropped = p.tours.iter().filter(|t| t.dropped).count();
                log::info!("company {id} ({date}): {buses} of its tours with its buses, {dropped} not run; its {} lines' other tours stay in their depots, the map's other lines run as ever", self.lines.len());
            }
            (Some(p), false) => {
                let buses = p.tours.iter().filter(|t| t.vehicle.is_some()).count();
                let dropped = p.tours.iter().filter(|t| t.dropped).count();
                log::info!("company {id}: only its own buses run today ({date}): {buses} of its tours with its buses, {dropped} dropped; the map's other timetable buses stay in their depots");
            }
            (None, true) => log::info!("company {id}: no plan of it for {date} yet - its lines stay in their depots until it is written"),
            (None, false) => log::info!("company {id}: no plan of it for {date} on this map - none of the map's timetable buses run"),
        }
    }


    pub fn active(&self) -> bool {
        self.plan.is_some()
    }

    /// A drive from the company's planning: only its buses run, plan for the day or not (not
    /// on a server: the map's other lines run there).
    pub fn own_only(&self) -> bool {
        self.company.is_some() && !self.server
    }

    /// Whether the timetable keeps a tour off the road: one the company dropped today, and in
    /// a drive for the company every tour of the map's own buses (`map_bus`) that is not the
    /// company's. Trains and planes run as ever.
    /// On a server: a tour of one of the company's lines it does not run today.
    pub fn keeps_off(&self, line: &str, tour: &str, map_bus: bool) -> bool {
        match self.tour(line, tour) {
            Some(t) => t.dropped,
            None if self.server => map_bus && self.lines.iter().any(|l| l.trim().eq_ignore_ascii_case(line.trim())),
            None => self.own_only() && map_bus,
        }
    }

    /// The company's tour, if it is one.
    pub fn tour(&self, line: &str, tour: &str) -> Option<&LiveTour> {
        self.plan.as_ref()?.tour(line, tour)
    }

    /// The depot file the company's buses carry.
    pub fn depot(&self) -> Option<&str> {
        self.plan.as_ref().map(|p| p.depot.as_str()).filter(|d| !d.trim().is_empty())
    }

    /// The bus type of a company bus file (loaded once).
    pub fn vehicle_type(&mut self, root: &Path, file: &str) -> Option<Arc<VehicleType>> {
        let key = file.trim().to_ascii_lowercase();
        if key.is_empty() {
            return None;
        }
        self.types
            .entry(key)
            .or_insert_with(|| {
                let path = omsi_cfg::resolve_path(root, file.trim());
                match VehicleType::load_ai(root, &path) {
                    Ok(t) => Some(Arc::new(t)),
                    Err(e) => {
                        log::warn!("company bus {file}: {e}");
                        None
                    }
                }
            })
            .clone()
    }

    /// A company trip its bus drove to the end (departure `index`, of `line`/`tour`, left at
    /// minute `dep`): reported to the company once.
    pub fn report(&mut self, index: usize, line: &str, tour: &str, dep: i32, km: f64, stops: usize, delay: f64) {
        let Some(p) = self.plan.as_ref().filter(|_| self.reports) else { return };
        if self.reported.contains(&index) {
            return;
        }
        let Some(ev) = p.trip_event(line, tour, dep, km, stops, delay) else { return };
        self.reported.insert(index);
        if let Err(e) = store::append_live(&self.data, &p.company, &ev) {
            log::warn!("company {}: a trip of line {line} tour {tour} not reported: {e:#}", p.company);
        }
    }
}

/// Every company of a dedicated server's folder on its map (Bus Company Online), as the timetable
/// asks them (`TimetableCompany`, in `schedule`): each tour goes to the company whose line it is -
/// the first by id that has the line, should two claim it (a line belongs to one company; the
/// founding refuses a line taken, this is the world's guard).
pub struct ServerCompanies {
    map_dir: PathBuf,
    data: PathBuf,
    day: i32,
    pub(crate) companies: Vec<CompanyLive>,
    /// Each line (lower case) to the company that has it (its index in `companies`).
    owner: HashMap<String, usize>,
}

impl ServerCompanies {
    /// The companies in the server's folder `data` that have a plan of the map in `map_dir`, for
    /// the day `date_code`.
    pub fn load(map_dir: &Path, date_code: i32, data: PathBuf) -> ServerCompanies {
        let mut s = ServerCompanies { map_dir: map_dir.to_path_buf(), data, day: date_code, companies: Vec::new(), owner: HashMap::new() };
        s.set_day(date_code);
        s
    }

    /// The day moved on, or a plan was written anew (the same day): every company's plan taken
    /// in again, a company new on the server taken on, and the lines given to their companies.
    pub fn set_day(&mut self, date_code: i32) {
        self.day = date_code;
        let here = self.map_dir.file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
        let folder = |p: &str| p.replace('\\', "/").trim_end_matches("/global.cfg").rsplit('/').next().unwrap_or("").to_lowercase();
        let mut ids: Vec<String> = plan::live_plans(&self.data).into_iter().filter(|p| !here.is_empty() && folder(&p.map) == here).map(|p| p.company).collect();
        ids.sort();
        ids.dedup();
        for id in ids {
            if !self.companies.iter().any(|c| c.company.as_deref() == Some(id.as_str())) {
                self.companies.push(CompanyLive::for_server(&self.map_dir, date_code, self.data.clone(), &id));
            }
        }
        self.companies.sort_by(|a, b| a.company.cmp(&b.company));
        for c in self.companies.iter_mut() {
            c.set_day(date_code);
        }
        self.owner.clear();
        for (k, c) in self.companies.iter().enumerate() {
            for line in &c.lines {
                let key = line.trim().to_lowercase();
                match self.owner.get(&key) {
                    Some(&first) if first != k => log::warn!("line {line}: both {} and {} have it on this server - it runs for {}", self.companies[first].company.as_deref().unwrap_or(""), c.company.as_deref().unwrap_or(""), self.companies[first].company.as_deref().unwrap_or("")),
                    Some(_) => {}
                    None => {
                        self.owner.insert(key, k);
                    }
                }
            }
        }
        log::info!("server: {} companies on the map, {} of its lines theirs", self.companies.len(), self.owner.len());
    }

    /// The company whose line `line` is.
    pub fn owner_of(&self, line: &str) -> Option<usize> {
        self.owner.get(&line.trim().to_lowercase()).copied()
    }

    /// The company whose line `line` is.
    pub fn company_of(&mut self, line: &str) -> Option<&mut CompanyLive> {
        let k = self.owner_of(line)?;
        self.companies.get_mut(k)
    }

    pub fn active(&self) -> bool {
        self.companies.iter().any(CompanyLive::active)
    }

    /// A tour of a company's line it does not run today (dropped, or nobody planned it); the
    /// map's other lines run.
    pub fn keeps_off(&self, line: &str, tour: &str, map_bus: bool) -> bool {
        self.owner_of(line).is_some_and(|k| self.companies[k].keeps_off(line, tour, map_bus))
    }

    /// The company's tour, if it is one of a company's.
    pub fn tour(&self, line: &str, tour: &str) -> Option<&LiveTour> {
        self.companies.get(self.owner_of(line)?)?.tour(line, tour)
    }

    /// A company trip its bus drove to the end: told to the company whose line it is, once.
    pub fn report(&mut self, index: usize, line: &str, tour: &str, dep: i32, km: f64, stops: usize, delay: f64) {
        if let Some(c) = self.company_of(line) {
            c.report(index, line, tour, dep, km, stops, delay);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::day::LiveEvent;

    #[test]
    fn the_company_day_is_taken_for_its_map_and_date_and_trips_are_reported_once() {
        let data = std::env::temp_dir().join(format!("openomsi-company-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let mut c = omsi_launcher_lib::company::found(&omsi_launcher_lib::company::Founding { name: "Live".into(), date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
        c.depot = "Grundorf.hof".into();
        store::save(&data, &c).unwrap();
        let lp = LivePlan {
            company: c.id.clone(),
            map: c.map.clone(),
            date: c.date.clone(),
            depot: c.depot.clone(),
            game_mode: c.game_mode,
            tours: vec![
                LiveTour { line: "Linie5".into(), number: "5".into(), tour: "1".into(), vehicle: Some(3), bus: "Vehicles/X/x.bus".into(), ..Default::default() },
                LiveTour { line: "Linie5".into(), number: "5".into(), tour: "2".into(), dropped: true, ..Default::default() },
            ],
            lines: Vec::new(),
        };
        plan::save_live_plan(&data, &lp).unwrap();
        let grundorf = Path::new("C:/OMSI 2/maps/Grundorf");
        // a drive of the player's own on the company's map and day: the map as ever, nothing
        // of the company, nothing reported
        let mut own = CompanyLive::default();
        own.set_day(20240304);
        assert!(!own.active() && !own.keeps_off("Linie9", "1", true));
        own.report(17, "Linie5", "1", 6 * 60, 12.5, 14, 75.0);
        // another map, another day: no plan - and in the company's drive none of the map's buses
        assert!(!CompanyLive::for_company(Path::new("C:/OMSI 2/maps/Ahlheim"), 20240304, data.clone(), &c.id).active());
        let later = CompanyLive::for_company(grundorf, 20240305, data.clone(), &c.id);
        assert!(!later.active() && later.keeps_off("Linie9", "1", true));
        // another company's drive on that map and day: not this company's plan
        let other = CompanyLive::for_company(grundorf, 20240304, data.clone(), "someone-else");
        assert!(!other.active() && other.keeps_off("Linie5", "1", true));
        let mut live = CompanyLive::for_company(grundorf, 20240304, data.clone(), &c.id);
        assert!(live.active());
        assert_eq!(live.depot(), Some("Grundorf.hof"));
        // its tour runs, its dropped tour not, the map's bus not, a train as ever
        assert!(!live.keeps_off("Linie5", "1", true) && live.keeps_off("linie5", "2", true));
        assert!(live.keeps_off("Linie9", "1", true) && !live.keeps_off("Linie9", "1", false));
        assert_eq!(live.tour("Linie5", "1").and_then(|t| t.vehicle), Some(3));
        live.report(17, "Linie5", "1", 6 * 60, 12.5, 14, 75.0);
        live.report(17, "Linie5", "1", 6 * 60, 12.5, 14, 75.0);
        live.report(18, "Linie9", "1", 7 * 60, 12.5, 14, 75.0);
        let base = c.clone();
        store::take_live(&data, &mut c);
        assert_eq!(c.live.len(), 1);
        assert!(matches!(&c.live[0], LiveEvent::Trip { vehicle: Some(3), dep: Some(360), delay, .. } if *delay == 75.0));
        // a duty driven ahead: that day's plan puts the buses on the road, nothing is reported
        plan::save_drive_plan(&data, &LivePlan { date: "2024-03-05".into(), ..lp.clone() }).unwrap();
        let mut ahead = CompanyLive::for_company(grundorf, 20240305, data.clone(), &c.id);
        assert!(ahead.active() && !ahead.keeps_off("Linie5", "1", true));
        ahead.report(17, "Linie5", "1", 6 * 60, 12.5, 14, 75.0);
        // (the day's trips read again into the company as it was: still the one)
        let mut again = base.clone();
        store::take_live(&data, &mut again);
        assert_eq!(again.live.len(), 1);
        assert_eq!(plan::live_plans(&data).len(), 1);
        assert!(plan::drive_plan(&data, "../x").is_none());
        let _ = std::fs::remove_dir_all(&data);
    }

    /// On a server (Bus Company Online) the company's tours run with its buses, its lines'
    /// other tours (and those it dropped) stay in their depots, and the map's other lines run
    /// as ever; the plan written anew for the same day is taken in without telling a trip
    /// twice, and a new day without its plan yet keeps the company's lines off still.
    #[test]
    fn on_a_server_the_maps_other_lines_run() {
        let data = std::env::temp_dir().join(format!("openomsi-company-server-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let c = omsi_launcher_lib::company::found(&omsi_launcher_lib::company::Founding { name: "Server".into(), date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
        store::save(&data, &c).unwrap();
        let mut lp = LivePlan {
            company: c.id.clone(),
            map: c.map.clone(),
            date: c.date.clone(),
            tours: vec![LiveTour { line: "Linie5".into(), number: "5".into(), tour: "1".into(), vehicle: Some(3), bus: "Vehicles/X/x.bus".into(), ..Default::default() }],
            lines: vec!["Linie5".into()],
            ..Default::default()
        };
        plan::save_live_plan(&data, &lp).unwrap();
        let grundorf = Path::new("C:/OMSI 2/maps/Grundorf");
        let mut live = CompanyLive::for_server(grundorf, 20240304, data.clone(), &c.id);
        assert!(live.active() && !live.own_only());
        assert!(!live.keeps_off("Linie5", "1", true), "its tour with its bus");
        assert!(live.keeps_off("Linie5", "2", true), "its line's tour nobody planned");
        assert!(!live.keeps_off("Linie9", "1", true), "the map's other line");
        live.report(17, "Linie5", "1", 6 * 60, 12.5, 14, 0.0);
        // a planner gave tour 2 a bus: the plan written anew, taken in for the same day
        lp.tours.push(LiveTour { line: "Linie5".into(), number: "5".into(), tour: "2".into(), vehicle: Some(4), bus: "Vehicles/X/x.bus".into(), ..Default::default() });
        plan::save_live_plan(&data, &lp).unwrap();
        live.set_day(20240304);
        assert!(!live.keeps_off("Linie5", "2", true));
        live.report(17, "Linie5", "1", 6 * 60, 12.5, 14, 0.0);
        assert_eq!(store::peek_live(&data, &c.id, 0).len(), 1, "a trip told once");
        // midnight before the host wrote the new day's plan: its lines stay off, the rest runs
        live.set_day(20240305);
        assert!(!live.active());
        assert!(live.keeps_off("Linie5", "1", true) && !live.keeps_off("Linie9", "1", true));
        let _ = std::fs::remove_dir_all(&data);
    }

    /// Several companies on a server: each line runs for the company that has it, in its buses,
    /// its trips told to it; a line two claim goes to the first; the map's other lines run; a
    /// company new on the server is taken on when the plans are taken in again.
    #[test]
    fn every_company_of_the_server_runs_its_own_lines() {
        let data = std::env::temp_dir().join(format!("openomsi-server-companies-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let plan_of = |id: &str, lines: &[&str], tours: Vec<LiveTour>| LivePlan {
            company: id.into(),
            map: "maps/Grundorf/global.cfg".into(),
            date: "2024-03-04".into(),
            tours,
            lines: lines.iter().map(|l| l.to_string()).collect(),
            ..Default::default()
        };
        let tour = |line: &str, tour: &str, vehicle: u32| LiveTour { line: line.into(), number: line.trim_start_matches("Linie").into(), tour: tour.into(), vehicle: Some(vehicle), bus: "Vehicles/X/x.bus".into(), ..Default::default() };
        // (a plan counts beside its company only)
        let company = |id: &str| {
            let mut c = omsi_launcher_lib::company::found(&omsi_launcher_lib::company::Founding { name: id.into(), date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "owner");
            c.id = id.into();
            store::save(&data, &c).unwrap();
        };
        for id in ["alpha", "beta", "gamma", "delta"] {
            company(id);
        }
        plan::save_live_plan(&data, &plan_of("alpha", &["Linie5", "Linie7"], vec![tour("Linie5", "1", 1)])).unwrap();
        // (beta claims Linie7 too: alpha has it)
        plan::save_live_plan(&data, &plan_of("beta", &["Linie6", "Linie7"], vec![tour("Linie6", "1", 2), tour("Linie7", "1", 3)])).unwrap();
        // a company of another map: not in this world
        plan::save_live_plan(&data, &LivePlan { map: "maps/Ahlheim/global.cfg".into(), ..plan_of("gamma", &["Linie8"], vec![tour("Linie8", "1", 4)]) }).unwrap();
        let grundorf = Path::new("C:/OMSI 2/maps/Grundorf");
        let mut s = ServerCompanies::load(grundorf, 20240304, data.clone());
        assert_eq!(s.companies.len(), 2);
        assert!(s.active());
        assert!(!s.keeps_off("Linie5", "1", true) && s.keeps_off("Linie5", "2", true), "alpha's tour runs, its line's other tour not");
        assert!(!s.keeps_off("Linie6", "1", true) && s.keeps_off("Linie6", "2", true), "beta's");
        assert!(s.keeps_off("Linie7", "1", true), "Linie7 is alpha's: beta's plan of it does not count");
        assert!(!s.keeps_off("Linie8", "1", true) && !s.keeps_off("Linie9", "1", true), "the map's other lines run");
        assert_eq!(s.tour("Linie6", "1").and_then(|t| t.vehicle), Some(2));
        s.report(10, "Linie5", "1", 6 * 60, 10.0, 12, 0.0);
        s.report(11, "Linie6", "1", 7 * 60, 10.0, 12, 0.0);
        s.report(12, "Linie9", "1", 7 * 60, 10.0, 12, 0.0);
        assert_eq!((store::peek_live(&data, "alpha", 0).len(), store::peek_live(&data, "beta", 0).len()), (1, 1), "each trip to its company");
        // a new company founded on the server: taken on with the plans
        plan::save_live_plan(&data, &plan_of("delta", &["Linie9"], vec![tour("Linie9", "1", 5)])).unwrap();
        s.set_day(20240304);
        assert_eq!(s.companies.len(), 3);
        assert!(!s.keeps_off("Linie9", "1", true) && s.keeps_off("Linie9", "2", true));
        s.report(11, "Linie6", "1", 7 * 60, 10.0, 12, 0.0);
        assert_eq!(store::peek_live(&data, "beta", 0).len(), 1, "told once");
        let _ = std::fs::remove_dir_all(&data);
    }
}
