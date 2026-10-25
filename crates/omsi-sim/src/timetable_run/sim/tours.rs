//! Tours: their stops for the menus, the tours LAN players drive, and the player's duty taken
//! from one.

use super::*;

pub struct TourPlan {
    pub line: String,
    pub number: String,
    pub ai_group: String,
    pub extra: String,
    pub available: bool,
    pub trips: Vec<TourPlanTrip>,
}

pub struct TourPlanTrip {
    /// Position in the source tour, including missing trip references.
    pub source_index: usize,
    pub track: String,
    pub reverse: bool,
    pub profile_index: usize,
    pub profile: Option<omsi_timetable::TripProfile>,
    pub trip: PlannedTrip,
}

impl ScheduleSim {
    /// A complete read-only tour snapshot. Times retain the service day's 24:xx values.
    /// Unlike `player_duty`, this neither reserves the AI vehicle nor changes selection.
    pub fn tour_plan(
        &self,
        world: &dyn TimetableWorld,
        line: &str,
        tour: &str,
    ) -> Result<TourPlan, String> {
        let l = self
            .data
            .lines
            .iter()
            .find(|l| l.name.trim().eq_ignore_ascii_case(line.trim()))
            .ok_or("unknown line")?;
        let t = l
            .tours
            .iter()
            .find(|t| t.number.trim().eq_ignore_ascii_case(tour.trim()))
            .ok_or("unknown tour")?;
        Ok(TourPlan {
            line: l.name.clone(),
            number: t.number.clone(),
            ai_group: t.ai_group.clone(),
            extra: t.extra.clone(),
            available: self.tour_available(t),
            trips: self.plan_tour_entries(world, t),
        })
    }

    fn plan_tour_entries(
        &self,
        world: &dyn TimetableWorld,
        t: &omsi_timetable::Tour,
    ) -> Vec<TourPlanTrip> {
        let mut out = Vec::new();
        for (source_index, tt) in t.trips.iter().enumerate() {
            let Some(ti) = self
                .data
                .trips
                .iter()
                .position(|x| x.name.eq_ignore_ascii_case(&tt.trip))
            else {
                continue;
            };
            let trip = &self.data.trips[ti];
            let planned = self.planned_trip(world, ti, tt.profile, tt.departure as f64 * 60.0);
            let profile_index = usize::try_from(tt.profile)
                .unwrap_or(0)
                .min(self.times[ti].len() - 1);
            out.push(TourPlanTrip {
                source_index,
                track: trip.display_name.clone(),
                reverse: trip.train_reverse,
                profile_index,
                profile: trip.profiles.get(profile_index).cloned(),
                trip: planned,
            });
        }
        out
    }
    /// The stops of a tour in the order it drives them, over all its trips: (trip number in
    /// the duty, stop number in the trip, name, departure there in seconds). Passing stations
    /// are left out.
    pub fn tour_stops(&self, line: &str, tour: &str) -> Vec<(usize, usize, String, f64)> {
        let Some(l) = self.data.lines.iter().find(|l| l.name.eq_ignore_ascii_case(line)) else { return Vec::new() };
        let Some(t) = l.tours.iter().find(|t| t.number.eq_ignore_ascii_case(tour)) else { return Vec::new() };
        let mut out = Vec::new();
        let mut k = 0;
        for tt in &t.trips {
            let Some(ti) = self.data.trips.iter().position(|x| x.name.eq_ignore_ascii_case(&tt.trip)) else { continue };
            let trip = &self.data.trips[ti];
            let departure = tt.departure as f64 * 60.0;
            let times = &self.times[ti][usize::try_from(tt.profile).unwrap_or(0).min(self.times[ti].len() - 1)];
            for (i, id) in trip_stations(trip).iter().enumerate() {
                if !times.stops.get(i).copied().unwrap_or(true) {
                    continue;
                }
                let name = self
                    .data
                    .bus_stops
                    .iter()
                    .find(|b| b.object_id == *id)
                    .map(|b| b.name.clone())
                    .filter(|n| !n.trim().is_empty())
                    .or_else(|| trip.stations.is_empty().then(|| trip.stations_legacy.get(i).and_then(|r| r.get(2)).map(|n| n.trim().to_string())).flatten())
                    .unwrap_or_else(|| format!("{}", i + 1));
                out.push((k, i, name, departure + times.stations[i].1));
            }
            k += 1;
        }
        out
    }

    /// How many trips a tour has that the timetable knows (the trips `tour_stops` numbers).
    pub fn tour_trip_count(&self, line: &str, tour: &str) -> usize {
        let mut n = 0;
        let mut last = None;
        for s in self.tour_stops(line, tour) {
            if last != Some(s.0) {
                n += 1;
                last = Some(s.0);
            }
        }
        n
    }

    /// The position (in order of departure, as `tour_trip_stops` counts) of the trip of a
    /// tour that is under way or next to leave at `now` (seconds of the day).
    pub fn tour_trip_now(&self, line: &str, tour: &str, now: f64) -> usize {
        let all = self.tour_stops(line, tour);
        let mut order: Vec<(usize, f64, f64)> = Vec::new();
        for s in &all {
            match order.iter_mut().find(|o| o.0 == s.0) {
                Some(o) => o.2 = o.2.max(s.3),
                None => order.push((s.0, s.3, s.3)),
            }
        }
        order.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
        // (the day before's tour, under way after midnight, or this day's)
        let yesterday = self.data.lines.iter().find(|l| l.name == line).and_then(|l| l.tours.iter().find(|t| t.number == tour)).is_some_and(|t| self.tour_runs_yesterday(t));
        trip_at(&order, now, yesterday)
    }

    /// All the stops of the trip in position `pos` of the tour's trips in order of the time
    /// they leave. The trip number in the entries is the one `tour_stops` gives it.
    pub fn tour_trip_stops(&self, line: &str, tour: &str, pos: usize) -> Vec<(usize, usize, String, f64)> {
        let all = self.tour_stops(line, tour);
        let mut order: Vec<(usize, f64)> = Vec::new();
        for s in &all {
            if !order.iter().any(|o| o.0 == s.0) {
                order.push((s.0, s.3));
            }
        }
        order.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0)));
        let Some(&(k, _)) = order.get(pos) else { return Vec::new() };
        all.into_iter().filter(|s| s.0 == k).collect()
    }

    /// The stops of the trip of a tour that is under way or next to start at `now` (seconds
    /// of the day): the first trip with a stop still to come (a minute of grace), and only
    /// that trip's stops from there on. A tour with nothing left today lists its first trip
    /// whole. Same entries and numbering as `tour_stops`.
    pub fn tour_stops_from(&self, line: &str, tour: &str, now: f64) -> Vec<(usize, usize, String, f64)> {
        let all = self.tour_stops(line, tour);
        let limit = now - 60.0;
        let Some(k) = all.iter().find(|s| s.3 >= limit).map(|s| s.0).or_else(|| all.first().map(|s| s.0)) else { return Vec::new() };
        let trip: Vec<_> = all.into_iter().filter(|s| s.0 == k).collect();
        let rest: Vec<_> = trip.iter().filter(|s| s.3 >= limit).cloned().collect();
        if rest.is_empty() { trip } else { rest }
    }

    /// The player drives this tour (line and tour as `player_duty` names them): OMSI leaves
    /// it to the player, so no AI bus runs it as well - one of line 76 tour 1 appeared
    /// 5 m beside the player's own bus at the Bauernhof, where the passengers queued.
    /// Returns how many of today's departures that takes from the AI.
    pub fn reserve_tour(&mut self, line: &str, tour: &str) -> usize {
        self.player_tour = Some((line.to_string(), tour.to_string()));
        self.player_trips.clear();
        let n = self.leave_to_player();
        log::info!("timetable: {n} departures of line {line} tour {tour} left to the player");
        n
    }

    /// The player drives these trips (line, tour, departure in s of the day) of a duty made
    /// of several tours: the AI runs the rest of each tour - its bus drives off at the
    /// terminus where the player takes the tour over, and a bus of its own comes for the
    /// trips after.
    pub fn reserve_trips(&mut self, trips: Vec<(String, String, f64)>) -> usize {
        self.player_tour = None;
        self.player_departure = None;
        self.player_trips = trips;
        let n = self.leave_to_player();
        log::info!("timetable: {n} departures of the player's duty left to the player");
        n
    }

    /// Takes the departures the player drives from the AI (their buses already on the road
    /// go at the next tick). Returns how many.
    fn leave_to_player(&mut self) -> usize {
        let mine: Vec<bool> = (0..self.departures.len())
            .map(|i| self.is_player_tour(i))
            .collect();
        let mut n = 0;
        for (d, m) in self.departures.iter_mut().zip(&mine) {
            if *m {
                n += 1;
                d.spawned = true;
            }
        }
        self.pending.retain(|i| !mine[*i]);
        self.waiting.retain(|i| !mine[*i]);
        self.retry_at.retain(|i, _| !mine[*i]);
        self.purge_player_tour = true;
        n
    }

    /// LAN play (host): what the other players drive now (`LanClaim`: a tour, or the trips of
    /// a duty on it). What is taken leaves the timetable like the player's own (its bus on the
    /// road goes; the AI's bus of the tour before drives off at the terminus where the player
    /// takes it over, and one of its own comes for the trips after); what is given up (the
    /// player left, or took another duty) runs again from its next departure - unless the
    /// company of its line drops what a player left (`TimetableCompany::drops_left`: Bus
    /// Company Online, its owner's choice), when nobody drives it.
    pub fn set_lan_tours(&mut self, tours: HashSet<LanClaim>) {
        if tours == self.lan_tours {
            return;
        }
        let added: Vec<LanClaim> = tours.difference(&self.lan_tours).cloned().collect();
        let removed: Vec<LanClaim> = self.lan_tours.difference(&tours).cloned().collect();
        self.lan_tours = tours;
        for t in &removed {
            let later: Vec<usize> = (0..self.departures.len())
                .filter(|&i| t.has(&self.departures[i].line, &self.departures[i].tour, self.departures[i].time) && self.departures[i].time > self.last_tod && !self.is_player_tour(i))
                .collect();
            if later.is_empty() {
                continue;
            }
            if self.company.as_deref().is_some_and(|c| c.drops_left(&t.line)) {
                log::info!("timetable: {} is no LAN player's any more: its company drops its {} later departures", t.text(), later.len());
                continue;
            }
            for &i in &later {
                self.departures[i].spawned = false;
            }
            log::info!("timetable: {} is no LAN player's any more: its {} later departures run again", t.text(), later.len());
        }
        if !added.is_empty() {
            let mine: Vec<bool> = (0..self.departures.len()).map(|i| self.is_player_tour(i)).collect();
            for (d, m) in self.departures.iter_mut().zip(&mine) {
                if *m {
                    d.spawned = true;
                }
            }
            self.pending.retain(|i| !mine[*i]);
            self.waiting.retain(|i| !mine[*i]);
            self.retry_at.retain(|i, _| !mine[*i]);
            self.purge_player_tour = true;
            for t in &added {
                log::info!("timetable: {} left to a LAN player", t.text());
            }
        }
    }

    /// Does a player drive departure `i` (we, or another LAN player)?
    pub(super) fn is_player_tour(&self, i: usize) -> bool {
        if let Some(d) = self.departures.get(i) {
            if self.lan_tours.iter().any(|t| t.has(&d.line, &d.tour, d.time)) {
                return true;
            }
            if self.player_trips.iter().any(|(line, tour, time)| d.line.eq_ignore_ascii_case(line) && d.tour.eq_ignore_ascii_case(tour) && (d.time - time).abs() < 30.0) {
                return true;
            }
        }
        match (&self.player_tour, self.departures.get(i)) {
            (Some((line, tour)), Some(d)) => {
                d.line.eq_ignore_ascii_case(line)
                    && d.tour.eq_ignore_ascii_case(tour)
                    && self.player_departure.map(|t| {
                        // (yesterday's night trips a day earlier: 00:30 is the trip of 24:30)
                        let dt = (d.time - t).rem_euclid(DAY);
                        dt < 30.0 || dt > DAY - 30.0
                    }).unwrap_or(true)
            }
            _ => false,
        }
    }

    /// Assign the player a tour of a line (names as in the .ttl / `[newtour]`), starting
    /// with the trip that fits the time of day `now`: the one under way, else the next to
    /// leave - as OMSI does when a time is picked for a tour. (The duty used to start with
    /// the tour's first trip whatever the time: Spandau's "Mo-Fr 3" of line 5 at 15:05 began
    /// with the 14:44 depot run, and the IBIS was typed for it.) The AI no longer drives the
    /// tour: its trips are the player's ([`ScheduleSim::reserve_tour`]).
    /// When the line or its tour cannot be driven, the error says why in words a driver can
    /// act on (a line the date's chrono takes off names the chrono and the day it starts);
    /// the callers used to drop that silently and the game started without a duty.
    ///
    /// `trip` is the trip to start with as the player picked it (OMSI's timetable dialog
    /// has line, tour and trip): its departure time "HH:MM" - the first trip leaving then or
    /// later - or its number in the tour (1 = first). The duty goes on with the tour's next
    /// trips from there, as OMSI's does.
    pub fn player_duty(
        &mut self,
        world: &dyn TimetableWorld,
        line: &str,
        tour: &str,
        now: f64,
        trip: Option<&str>,
        whole_tour: bool,
    ) -> Result<PlayerDuty, String> {
        let date = |code: i32| {
            format!(
                "{:04}-{:02}-{:02}",
                code / 10000,
                code / 100 % 100,
                code % 100
            )
        };
        let Some(l) = self
            .data
            .lines
            .iter()
            .find(|l| l.name.eq_ignore_ascii_case(line))
        else {
            let running = self.data.lines.len();
            if let Some((name, dir)) = self
                .deactivated
                .iter()
                .find(|(l, _)| l.trim().eq_ignore_ascii_case(line.trim()))
            {
                let from = Some(omsi_cfg::resolve_path(dir, "Chrono.cfg"))
                    .and_then(|p| omsi_cfg::CfgFile::read(&p).ok())
                    .map(|f| omsi_map::ailists::parse_chrono_cfg(&f).start_date)
                    .filter(|d| *d > 0);
                return Err(format!(
                    "line {name} does not run on {}: the timetable change '{}'{} takes it off. {running} other lines run that day: pick one of them, or an earlier date",
                    date(world.date()),
                    dir.file_name().unwrap_or_default().to_string_lossy(),
                    from.map(|d| format!(" of {}", date(d))).unwrap_or_default()
                ));
            }
            return Err(format!("line {line} is not in the timetable of this map on {}: {running} lines run that day", date(world.date())));
        };
        let t = match l.tours.iter().find(|t| t.number.eq_ignore_ascii_case(tour)) {
            Some(t) => t,
            None => {
                let Some(first) = l.tours.first() else {
                    return Err(format!("line {} has no tours", l.name));
                };
                if !tour.is_empty() {
                    log::warn!(
                        "line {} has no tour '{tour}': driving its tour {}",
                        l.name,
                        first.number
                    );
                }
                first
            }
        };
        let mut trips = self.planned_trips(world, t);
        if trips.is_empty() {
            return Err(format!(
                "line {} tour {} has no trip the timetable knows ({} listed)",
                l.name,
                t.number,
                t.trips.len()
            ));
        }
        // Yesterday's night tour, still under way after midnight (its trips past 24:00 are
        // tonight's): its times a day earlier, so the duty goes on with the trip running
        // now. After a restart at 0:45 the night lines begun the day before could not be
        // taken (#1576). A tour that runs every night is yesterday's too while today's has not
        // begun: at 03:25 the 22:00 tour's trip of 27:30 is the one at 03:30, not tomorrow's
        // (#2166).
        if (!self.tour_runs_on(t, self.day_bits) || trips[0].departure > now) && self.tour_runs_on(t, self.yesterday_bits()) && trips.iter().any(|p| p.departure >= DAY) && trips.last().is_some_and(|p| p.end - DAY > now) {
            for p in &mut trips {
                p.departure -= DAY;
                p.end -= DAY;
                for s in &mut p.stops {
                    s.arr -= DAY;
                    s.dep -= DAY;
                }
            }
            log::info!("player duty: line {} tour {} is yesterday's night tour, under way past midnight", l.name, t.number);
        }
        let (line_name, tour_name) = (l.name.clone(), t.number.clone());
        let trip_index = match trip.map(str::trim).filter(|t| !t.is_empty()) {
            Some(pick) => chosen_trip(&trips, pick).ok_or_else(|| {
                format!(
                    "line {line_name} tour {tour_name} has no trip '{pick}': its {} trips leave {}",
                    trips.len(),
                    trips.iter().map(|t| hhmm(t.departure)).collect::<Vec<_>>().join(", ")
                )
            })?,
            None => starting_trip(&trips, now),
        };
        let hm = hhmm;
        // A picked trip is the duty, one way to its terminus, as a trip chosen in OMSI is;
        // the rest of the tour only with `--whole-tour`.
        let (trips, trip_index, first_trip) = if trip.is_some() && !whole_tour {
            (vec![trips[trip_index].clone()], 0, trip_index)
        } else {
            (trips, trip_index, 0)
        };
        // the AI leaves the player what the player drives: the tour, or just the one trip
        self.player_departure = (trips.len() == 1 && trip.is_some() && !whole_tour).then(|| trips[0].departure);
        let reserved = self.reserve_tour(&line_name, &tour_name);
        let current = &trips[trip_index];
        log::info!(
            "player duty: line {line_name} tour {tour_name}: {} trips from {} ({} of them not driven by the AI); at {} trip {} {} {} (to {}), then {}",
            trips.len(),
            hm(trips[0].departure),
            reserved,
            hm(now),
            trip_index + 1,
            current.name,
            // (a trip after midnight picked in the evening leaves tonight)
            if current.departure < now && now - current.departure < DAY / 2.0 { format!("under way since {}", hm(current.departure)) } else { format!("leaves at {}", hm(current.departure)) },
            current.terminus,
            trips.get(trip_index + 1).map(|t| format!("{} at {}", t.name, hm(t.departure))).unwrap_or_else(|| "the end of the duty".into())
        );
        let unnumbered_public = self.unnumbered_public(&trips);
        let mut duty = PlayerDuty::new(
            line_name,
            tour_name,
            trips,
            trip_index,
            first_trip,
            trip.map(|t| !t.trim().is_empty()).unwrap_or(false),
        );
        duty.unnumbered_public = unnumbered_public;
        Ok(duty)
    }

    /// The trips of `trips` without a line number that one of the map's depot files runs as
    /// public trips ([`ibis::serves_unnumbered`]).
    fn unnumbered_public(&self, trips: &[PlannedTrip]) -> Vec<String> {
        let mut hofs: Vec<&Arc<omsi_vehicle::Hof>> = Vec::new();
        for h in self.depots.values().flatten().filter_map(|v| v.2.as_ref()) {
            if !hofs.iter().any(|x| Arc::ptr_eq(x, h)) {
                hofs.push(h);
            }
        }
        let mut out: Vec<String> = Vec::new();
        for t in trips.iter().filter(|t| t.line.trim().is_empty()) {
            let stops: Vec<&str> = t.stops.iter().map(|s| s.name.as_str()).collect();
            if !out.contains(&t.name) && hofs.iter().any(|h| ibis::serves_unnumbered(h, &t.terminus, &stops)) {
                log::info!("player duty: trip {} has no line number, but the depot file runs it as a public trip (line 000)", t.name);
                out.push(t.name.clone());
            }
        }
        out
    }

    /// The trips of a tour as the player drives them, with their stops and times; a trip the
    /// timetable does not know is left out (so a trip's place here is its place in OMSI's
    /// dialog, which leaves them out too).
    fn planned_trips(&self, world: &dyn TimetableWorld, t: &omsi_timetable::Tour) -> Vec<PlannedTrip> {
        let mut trips = Vec::new();
        for tt in &t.trips {
            let Some(ti) = self
                .data
                .trips
                .iter()
                .position(|x| x.name.eq_ignore_ascii_case(&tt.trip))
            else {
                continue;
            };
            trips.push(self.planned_trip(world, ti, tt.profile, tt.departure as f64 * 60.0));
        }
        trips
    }

    /// Trip `ti` of the timetable as the player drives it, leaving at `departure` (seconds of
    /// the day) with the times of its profile `profile`: its stops, their places and times.
    /// A stop is the object of the tile its station names (`trip_station_tiles`), where the
    /// map repeats its ids across tiles.
    fn planned_trip(&self, world: &dyn TimetableWorld, ti: usize, profile: i32, departure: f64) -> PlannedTrip {
        let trip = &self.data.trips[ti];
        let times = &self.times[ti][usize::try_from(profile)
            .unwrap_or(0)
            .min(self.times[ti].len() - 1)];
        let tiles = trip_station_tiles(trip, &self.data.bus_stops);
        let stops = trip_stations(trip)
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let name = self.station_name(trip, i, *id);
                let position = world.object_of_tile(tiles.get(i).copied().flatten(), *id).map(|p| p.0);
                let (arr, dep) = times.stations[i];
                PlannedStop {
                    object_id: *id,
                    name,
                    arr: departure + arr,
                    dep: departure + dep,
                    position,
                    dir: StopDir::default(),
                    stops: times.stops[i],
                }
            })
            .collect();
        let mut planned = PlannedTrip {
            name: trip.name.clone(),
            line: trip.line.clone(),
            terminus: trip.terminus.clone(),
            departure,
            end: departure + times.duration,
            stops,
        };
        planned.set_dirs();
        planned
    }

    /// A free drive along one route of a line (`--free-line`, the launcher's free drive with
    /// a line the player chose): the timetable's trip `trip` (its file's name, the route as
    /// the map names it) as a duty of that one trip, leaving at `now` - the player chose
    /// where to drive, not when - so the navigator draws its way and the IBIS can be typed
    /// for it. Nothing is booked: the AI keeps every tour (the timetable's own bus on that
    /// route still drives), and the duty is `free`, so nobody keeps its time. `line` is the
    /// timetable's line it was chosen under.
    pub fn free_line_duty(&self, world: &dyn TimetableWorld, line: &str, trip: &str, now: f64) -> Result<PlayerDuty, String> {
        let Some(ti) = self.data.trips.iter().position(|x| x.name.trim().eq_ignore_ascii_case(trip.trim())) else {
            return Err(format!("the timetable of this map has no route '{}'", trip.trim()));
        };
        if self.times.get(ti).is_none_or(|t| t.is_empty()) {
            return Err(format!("route '{}' has no times", trip.trim()));
        }
        let planned = self.planned_trip(world, ti, 0, now);
        if planned.stops.is_empty() {
            return Err(format!("route '{}' has no stops", trip.trim()));
        }
        let line = if line.trim().is_empty() { planned.line.trim().to_string() } else { line.trim().to_string() };
        log::info!(
            "free drive along line {line}: route {} from {} to {}, {} stops, nothing booked",
            planned.name,
            planned.stops[0].name.trim(),
            planned.terminus.trim(),
            planned.stops.len()
        );
        let mut d = PlayerDuty::new(line, String::new(), vec![planned], 0, 0, true);
        d.free = true;
        Ok(d)
    }

    /// A duty of trips from several tours, as the launcher puts one together (omsi-app's
    /// `--duty-leg`, parsed there: see `omsi_launcher_lib::compose`): each leg is (line, tour,
    /// first, trips), so many trips of the tour from its trip `first` (1 = the first) on.
    /// Where one leg ends the next leaves, a few minutes later: the bus stays the player's
    /// and takes the next tour over there, as a driver whose shift goes on with another
    /// line. The AI keeps the rest of those tours.
    pub fn player_plan(&mut self, world: &dyn TimetableWorld, legs: &[(String, String, usize, usize)]) -> Result<PlayerDuty, String> {
        let mut trips: Vec<PlannedTrip> = Vec::new();
        let mut tours: Vec<(String, String, usize)> = Vec::new();
        for (line, tour, first, count) in legs {
            let (first, count) = (*first, *count);
            let Some(l) = self.data.lines.iter().find(|l| l.name.eq_ignore_ascii_case(line)) else {
                return Err(format!("line {line} is not in the timetable of this map on this day"));
            };
            let Some(t) = l.tours.iter().find(|t| t.number.eq_ignore_ascii_case(tour)) else {
                return Err(format!("line {} has no tour {tour}", l.name));
            };
            let all = self.planned_trips(world, t);
            if first == 0 || first > all.len() {
                return Err(format!("line {} tour {} has no trip {first}: it has {}", l.name, t.number, all.len()));
            }
            for (k, p) in all.into_iter().enumerate().skip(first - 1).take(count.max(1)) {
                tours.push((l.name.clone(), t.number.clone(), k));
                trips.push(p);
            }
        }
        let Some(first) = trips.first() else {
            return Err("the duty has no trips".into());
        };
        log::info!(
            "player duty of {} trips in {} parts: from {} {} at {} to {} at {}",
            trips.len(),
            legs.len(),
            tours[0].0,
            first.name,
            hhmm(first.departure),
            trips[trips.len() - 1].terminus,
            hhmm(trips[trips.len() - 1].end)
        );
        self.reserve_trips(trips.iter().zip(&tours).map(|(p, (l, t, _))| (l.clone(), t.clone(), p.departure)).collect());
        // (the duty is the player's from its first trip, however late the bus is for it)
        let mut d = PlayerDuty::new(tours[0].0.clone(), tours[0].1.clone(), trips, 0, 0, true);
        d.tours = tours;
        Ok(d)
    }
}

/// What a LAN player drives, as his game tells the host (`omsi_net::Pose::tour`): a tour of a
/// line (lower case) and of it the departures of his duty's trips still to come, the first and
/// the last (minutes of the day; None: the whole tour - an older game's `<line>/<tour>`). The
/// host's timetable leaves those to him and the rest of the tour to the AI: a duty that is a part
/// of a tour has the AI before it and after it (NexiconDK, Bus Company Online phase 3 step 3; the
/// whole tour used to leave the AI while he drove a part of it).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LanClaim {
    pub line: String,
    pub tour: String,
    pub window: Option<(i32, i32)>,
}

/// The longest text of claims a game sends (the INFO field of `omsi_net`): whole claims, the one
/// of the trip under way first.
pub const LAN_CLAIMS_MAX: usize = 64;

impl LanClaim {
    /// Departure `time` (s of the day) of `line`/`tour` is his (a minute either side; the
    /// service day's 24:xx as well as 0:xx).
    pub fn has(&self, line: &str, tour: &str, time: f64) -> bool {
        if !line.trim().eq_ignore_ascii_case(&self.line) || !tour.trim().eq_ignore_ascii_case(&self.tour) {
            return false;
        }
        let Some((a, b)) = self.window else { return true };
        let (from, to) = (a as f64 * 60.0 - 30.0, b as f64 * 60.0 + 30.0);
        [time, time + DAY, time - DAY].iter().any(|t| *t >= from && *t <= to)
    }

    /// As the game sends it: `<line>/<tour>`, `@<first>~<last>` with a window.
    pub fn code(&self) -> String {
        match self.window {
            Some((a, b)) => format!("{}/{}@{a}~{b}", self.line, self.tour),
            None => format!("{}/{}", self.line, self.tour),
        }
    }

    /// As the logs say it.
    pub fn text(&self) -> String {
        let hm = |m: i32| format!("{:02}:{:02}", m.rem_euclid(1440) / 60, m.rem_euclid(60));
        match self.window {
            Some((a, b)) => format!("line {} tour {} ({} - {})", self.line, self.tour, hm(a), hm(b)),
            None => format!("line {} tour {}", self.line, self.tour),
        }
    }
}

/// The claims of a game's text (`lan_claims_text`): `;` between them; one that does not read as
/// a claim is left out.
pub fn lan_claims(text: &str) -> Vec<LanClaim> {
    text.split(';')
        .filter_map(|part| {
            let (head, window) = match part.rsplit_once('@') {
                Some((h, w)) => {
                    let (a, b) = w.split_once('~')?;
                    (h, Some((a.trim().parse::<i32>().ok()?, b.trim().parse::<i32>().ok()?)))
                }
                None => (part, None),
            };
            let (line, tour) = head.split_once('/')?;
            let (line, tour) = (line.trim().to_lowercase(), tour.trim().to_lowercase());
            (!line.is_empty() && !tour.is_empty()).then(|| LanClaim { line, tour, window: window.map(|(a, b): (i32, i32)| (a.min(b), a.max(b))) })
        })
        .collect()
}

/// The text a game sends of `claims`: as many whole ones as fit in `LAN_CLAIMS_MAX` characters.
pub fn lan_claims_text(claims: &[LanClaim]) -> String {
    let mut out = String::new();
    for c in claims {
        let code = c.code();
        let len = out.chars().count() + code.chars().count() + usize::from(!out.is_empty());
        if len > LAN_CLAIMS_MAX {
            break;
        }
        if !out.is_empty() {
            out.push(';');
        }
        out.push_str(&code);
    }
    out
}

impl PlayerDuty {
    /// What the player drives, for the LAN host (`LanClaim`): of each tour of his duty, in the
    /// order he drives them, the trips still to come from the one under way; nothing once its
    /// last trip is driven, nor on a free drive.
    pub fn lan_claims(&self) -> Vec<LanClaim> {
        if self.free || self.trips.is_empty() || (self.done && self.trip_index + 1 >= self.trips.len()) {
            return Vec::new();
        }
        let mut out: Vec<LanClaim> = Vec::new();
        for (i, trip) in self.trips.iter().enumerate().skip(self.trip_index) {
            let (line, tour) = match self.tours.get(i) {
                Some((l, t, _)) => (l.trim(), t.trim()),
                None => (self.line.trim(), self.tour.trim()),
            };
            if line.is_empty() || tour.is_empty() || !trip.departure.is_finite() {
                continue;
            }
            let dep = (trip.departure / 60.0).round() as i32;
            match out.last_mut().filter(|c| c.line.eq_ignore_ascii_case(line) && c.tour.eq_ignore_ascii_case(tour)) {
                Some(c) => {
                    if let Some(w) = c.window.as_mut() {
                        *w = (w.0.min(dep), w.1.max(dep));
                    }
                }
                None => out.push(LanClaim { line: line.to_lowercase(), tour: tour.to_lowercase(), window: Some((dep, dep)) }),
            }
        }
        out
    }
}

/// Of a tour's trips in order (`(trip, departure, last arrival)`), the one under way or next
/// to leave at `now` (seconds of the day). After midnight a night tour of the day before
/// counts its trips past 24:00: 2:18 is its 26:18, under way since the evening (the tour
/// showed the evening's first trip, hours over). `yesterday`: the tour ran the day before
/// (else its night trips are the coming night's). (NexiconDK, as openOMSI_DK.)
pub(super) fn trip_at(order: &[(usize, f64, f64)], now: f64, yesterday: bool) -> usize {
    let start = order.first().map_or(0.0, |o| o.1);
    let end = order.iter().map(|o| o.2).fold(f64::MIN, f64::max);
    let now = if yesterday && now < start && now + DAY >= start && now + DAY <= end + 60.0 { now + DAY } else { now };
    order.iter().position(|o| o.2 >= now - 60.0).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct World {
        path: PathBuf,
        ai: omsi_map::AiLists,
        positions: crate::world_data::ObjectPlaces,
    }
    impl TimetableWorld for World {
        fn root(&self) -> &Path {
            &self.path
        }
        fn map_dir(&self) -> &Path {
            &self.path
        }
        fn chrono_dirs(&self) -> Vec<PathBuf> {
            vec![]
        }
        fn ailists(&self) -> &omsi_map::AiLists {
            &self.ai
        }
        fn raw_tiles(&self) -> Vec<(i32, i32)> {
            vec![]
        }
        fn has_tile(&self, _: (i32, i32)) -> bool {
            false
        }
        fn date(&self) -> i32 {
            20221010
        }
        fn object_positions(&self) -> &crate::world_data::ObjectPlaces {
            &self.positions
        }
        fn stop_side(&self, _: i64) -> f32 {
            0.0
        }
    }
    #[test]
    fn full_tour_snapshot_preserves_profiles_passing_stops_and_service_day() {
        let w = World {
            path: std::env::temp_dir().join(format!("omsi_tour_plan_{}", std::process::id())),
            ai: Default::default(),
            positions: Default::default(),
        };
        let mut s = ScheduleSim::new(&w.path, &w, &crate::SimClock::default());
        s.data.trips.push(omsi_timetable::Trip {
            name: "night".into(),
            line: "N1".into(),
            terminus: "Airport".into(),
            stations: vec![10, 20],
            display_name: "track".into(),
            train_reverse: true,
            profiles: vec![omsi_timetable::TripProfile {
                name: "night profile".into(),
                factor: 1.2,
                ..Default::default()
            }],
            ..Default::default()
        });
        s.times = vec![vec![TripTimes {
            stations: vec![(0.0, 30.0), (120.0, 150.0)],
            stops: vec![true, false],
            duration: 150.0,
            ..Default::default()
        }]];
        s.data.bus_stops.push(omsi_timetable::BusStopEntry {
            object_id: 10,
            name: "Start".into(),
            ..Default::default()
        });
        let tt = |name: &str, departure| omsi_timetable::TourTrip {
            trip: name.into(),
            departure,
            profile: 0,
        };
        s.data.lines.push(omsi_timetable::Line {
            name: "N1".into(),
            tours: vec![omsi_timetable::Tour {
                number: "1".into(),
                ai_group: "Depot".into(),
                extra: "1023".into(),
                trips: vec![
                    tt("night", 1439.0),
                    tt("missing", 1440.0),
                    tt("night", 1441.0),
                ],
            }],
            ..Default::default()
        });
        let plan = s.tour_plan(&w, "n1", "1").unwrap();
        assert_eq!(plan.trips.len(), 2);
        assert_eq!(plan.trips[1].source_index, 2);
        assert_eq!(plan.trips[1].trip.departure, 86460.0);
        assert_eq!(plan.trips[1].trip.stops[1].arr, 86580.0);
        assert!(!plan.trips[1].trip.stops[1].stops);
        assert_eq!(plan.trips[0].trip.stops[0].name, "Start");
        assert_eq!(
            plan.trips[0].profile.as_ref().unwrap().name,
            "night profile"
        );
        assert!(plan.trips[0].reverse);
        assert!(s.player_tour.is_none() && s.player_departure.is_none() && !s.purge_player_tour);
        assert!(s.tour_plan(&w, "N1", "unknown").is_err());
        // Selecting only the second known trip must not truncate a later read of its tour.
        let duty = s
            .player_duty(&w, "N1", "1", 86400.0, Some("2"), false)
            .unwrap();
        assert_eq!(duty.trips.len(), 1);
        assert_eq!(duty.first_trip, 1);
        assert_eq!(s.tour_plan(&w, "N1", "1").unwrap().trips.len(), 2);
    }

    /// A company that drops what a LAN player leaves of his duty (Bus Company Online).
    struct Drops;
    impl TimetableCompany for Drops {
        fn active(&self) -> bool {
            true
        }
        fn own_only(&self) -> bool {
            false
        }
        fn set_day(&mut self, _: i32) {}
        fn keeps_off(&self, _: &str, _: &str, _: bool) -> bool {
            false
        }
        fn runs_tour(&self, _: &str, _: &str) -> bool {
            true
        }
        fn report(&mut self, _: usize, _: &str, _: &str, _: i32, _: f64, _: usize, _: f64) {}
        fn bus(&mut self, _: &Path, _: &str, _: &str) -> Option<CompanyBus> {
            None
        }
        fn drops_left(&self, line: &str) -> bool {
            line.eq_ignore_ascii_case("linie5")
        }
        fn keeps_off_departure(&self, _: &str, tour: &str, dep: i32) -> bool {
            tour == "3" && dep == 12 * 60
        }
    }

    /// LAN play (NexiconDK, Bus Company Online phase 3 step 3): a player's duty that is a part
    /// of a tour takes only its trips from the AI - those before it and after it stay the AI's
    /// (the whole tour used to leave the AI); given up halfway, its later trips run again, or
    /// stay off when the company drops what a player left; and what the game sends of it.
    #[test]
    fn a_lan_players_duty_takes_only_its_trips_from_the_ai() {
        let w = World { path: std::env::temp_dir().join(format!("omsi_lan_claims_{}", std::process::id())), ai: Default::default(), positions: Default::default() };
        let mut s = ScheduleSim::new(&w.path, &w, &crate::SimClock::default());
        s.data.trips.push(omsi_timetable::Trip { name: "5a".into(), line: "Linie5".into(), stations: vec![1, 2], ..Default::default() });
        let d = |h: f64| Departure { time: h * 3600.0, trip: 0, profile: 0, line: "Linie5".into(), ai_group: "depot".into(), tour: "3".into(), mask: -1, spawned: false, yesterday: false };
        let fresh = |s: &mut ScheduleSim| {
            s.departures = vec![d(8.0), d(9.0), d(10.0), d(11.0), d(12.0)];
            s.lan_tours.clear();
        };
        fresh(&mut s);
        let claims = lan_claims("Linie5/3@540~600");
        assert_eq!(claims, vec![LanClaim { line: "linie5".into(), tour: "3".into(), window: Some((540, 600)) }]);
        s.last_tod = 8.5 * 3600.0;
        s.set_lan_tours(claims.iter().cloned().collect());
        assert_eq!((0..4).map(|i| s.is_player_tour(i)).collect::<Vec<_>>(), [false, true, true, false], "his 9:00 and 10:00 only");
        assert!(s.departures[1].spawned && s.departures[2].spawned && !s.departures[3].spawned, "the 11:00 stays the AI's");
        // (he left at 9:30: the AI drives his 10:00 - the 9:00 under way it cannot)
        s.last_tod = 9.5 * 3600.0;
        s.set_lan_tours(HashSet::new());
        assert!(!s.departures[2].spawned && s.departures[1].spawned);
        // (the window moving on as he drives gives nothing back to the AI)
        fresh(&mut s);
        s.last_tod = 8.5 * 3600.0;
        s.set_lan_tours(lan_claims("linie5/3@540~600").into_iter().collect());
        s.last_tod = 9.9 * 3600.0;
        s.set_lan_tours(lan_claims("linie5/3@600~600").into_iter().collect());
        assert!(s.departures[2].spawned && s.is_player_tour(2) && !s.is_player_tour(1));
        // an older game's claim: the whole tour
        fresh(&mut s);
        s.set_lan_tours(lan_claims("Linie5/3").into_iter().collect());
        assert!((0..5).all(|i| s.is_player_tour(i)));
        // a company that drops what a player left: his 10:00 stays off, and so does the
        // departure its plan drops
        fresh(&mut s);
        s.set_company(Box::new(Drops));
        assert!(s.kept_off(4) && !s.kept_off(3), "the 12:00 its plan drops");
        s.last_tod = 8.5 * 3600.0;
        s.set_lan_tours(lan_claims("Linie5/3@540~600").into_iter().collect());
        s.last_tod = 9.5 * 3600.0;
        s.set_lan_tours(HashSet::new());
        assert!(s.departures[2].spawned, "nobody drives what he left");
        // what the game sends: whole claims that fit, the trip under way first; nothing once
        // the duty is over
        let trip = |dep: f64| crate::timetable_run::tests::planned(dep, &[(0.0, dep, dep), (900.0, dep + 900.0, dep + 900.0)]);
        let mut duty = PlayerDuty::new("Linie5".into(), "3".into(), vec![trip(32_400.0), trip(36_000.0), trip(39_600.0)], 1, 0, true);
        duty.tours = vec![("Linie5".into(), "3".into(), 3), ("Linie5".into(), "3".into(), 4), ("Linie7".into(), "Mo-Fr 2".into(), 0)];
        let mine = duty.lan_claims();
        assert_eq!(lan_claims_text(&mine), "linie5/3@600~600;linie7/mo-fr 2@660~660");
        assert_eq!(lan_claims(&lan_claims_text(&mine)), mine);
        let long: Vec<LanClaim> = (0..9).map(|k| LanClaim { line: format!("line{k}"), tour: "10".into(), window: Some((600, 700)) }).collect();
        let text = lan_claims_text(&long);
        assert!(text.chars().count() <= LAN_CLAIMS_MAX && lan_claims(&text).len() == 3 && lan_claims(&text)[0] == long[0], "{text}");
        assert!(lan_claims("x@1~;/3;5/@1~2;ok/1@-30~20").iter().map(LanClaim::code).eq(["ok/1@-30~20".to_string()]), "what does not read as a claim is left out");
        duty.trip_index = 2;
        duty.done = true;
        assert!(duty.lan_claims().is_empty(), "his last trip driven");
        assert!(PlayerDuty { free: true, ..PlayerDuty::new("5".into(), "1".into(), vec![trip(0.0)], 0, 0, false) }.lan_claims().is_empty(), "a free drive claims nothing");
    }
}
