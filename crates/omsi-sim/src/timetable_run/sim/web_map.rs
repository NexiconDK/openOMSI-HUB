//! Omsi-Hub: what a dedicated server's live map (`GET /kort`) draws of the map and its
//! timetable - the map's ways, stops and tours, each trip's route - and the timetable buses
//! on the road now. Coordinates are world metres (x east, y north), as `GET /players` gives.

use super::*;
use glam::DVec2;
use std::fmt::Write as _;

/// The timetable as the live map needs it, taken off the running timetable (so that it can
/// be put together with the map's tiles on another thread).
#[derive(Debug, Clone, Default)]
pub struct WebTimetable {
    /// The trips the tours drive, each once.
    pub trips: Vec<WebTrip>,
    /// The tours: `<line>/<tour>` (as `GET /players` names a player's) and their stops.
    pub tours: Vec<(String, Vec<WebCall>)>,
    /// The lines and the trips their tours drive.
    pub lines: Vec<(String, Vec<usize>)>,
}

#[derive(Debug, Clone, Default)]
pub struct WebTrip {
    pub name: String,
    pub terminus: String,
    /// The splines it drives, in order: tile and spline id.
    pub pieces: Vec<(i32, i32, i64)>,
    /// Its stations (map object ids), in order.
    pub stations: Vec<i64>,
}

/// One stop of a tour: which trip (of `WebTimetable::trips`), which stop of it, its name,
/// its station and its departure (seconds of the day).
#[derive(Debug, Clone, Default)]
pub struct WebCall {
    pub trip: usize,
    pub name: String,
    pub station: i64,
    pub dep: f64,
}

/// The live map's files: `kort.json` and each trip's route (`rute/<n>`, in the order of
/// `WebTimetable::trips`).
#[derive(Debug, Clone, Default)]
pub struct WebMap {
    pub map: Vec<u8>,
    pub routes: Vec<Vec<u8>>,
}

impl ScheduleSim {
    /// The tours of every day (a server runs on past midnight) and their trips, as the live
    /// map draws them. The map's machinery (its `Z_` lines: trains, signals) is left out.
    pub fn web_timetable(&self) -> WebTimetable {
        let mut out = WebTimetable::default();
        let mut trip_ix: HashMap<usize, usize> = HashMap::new();
        for line in &self.data.lines {
            if line.name.trim().to_ascii_uppercase().starts_with("Z_") {
                continue;
            }
            let mut line_trips: Vec<usize> = Vec::new();
            for tour in &line.tours {
                let calls = self.tour_stops(&line.name, &tour.number);
                if calls.is_empty() {
                    continue;
                }
                // the trips as tour_stops numbers them: those of the tour the timetable has
                let found: Vec<usize> = tour
                    .trips
                    .iter()
                    .filter_map(|tt| self.data.trips.iter().position(|x| x.name.eq_ignore_ascii_case(&tt.trip)))
                    .collect();
                let mut web_calls = Vec::with_capacity(calls.len());
                for (k, i, name, dep) in calls {
                    let Some(&ti) = found.get(k) else { continue };
                    let n = *trip_ix.entry(ti).or_insert_with(|| {
                        let t = &self.data.trips[ti];
                        let stations = trip_stations(t);
                        let mut pieces: Vec<(i32, i32, i64)> = Vec::new();
                        for s in self.steps_of(&t.name, &stations).0 {
                            if let Some(key) = s.key {
                                let p = (key.tile.0, key.tile.1, key.id);
                                if pieces.last() != Some(&p) {
                                    pieces.push(p);
                                }
                            }
                        }
                        out.trips.push(WebTrip { name: t.name.trim().to_string(), terminus: t.terminus.trim().to_string(), pieces, stations });
                        out.trips.len() - 1
                    });
                    if !line_trips.contains(&n) {
                        line_trips.push(n);
                    }
                    let station = out.trips[n].stations.get(i).copied().unwrap_or(-1);
                    web_calls.push(WebCall { trip: n, name: name.trim().to_string(), station, dep });
                }
                out.tours.push((format!("{}/{}", line.name.trim(), tour.number.trim()), web_calls));
            }
            if !line_trips.is_empty() {
                out.lines.push((line.name.trim().to_string(), line_trips));
            }
        }
        out
    }

    /// `GET /kort/busser`: the timetable buses on the road now, a JSON array. `aboard`: how
    /// many people sit in each (`PeopleSim::ai_aboard`).
    pub fn web_buses_json(&self, traffic: &TrafficSim, aboard: &HashMap<u64, usize>) -> String {
        let mut o = String::from("[");
        for car in &traffic.cars {
            let Some(&i) = self.car_departure.get(&car.id) else { continue };
            let Some(b) = car.bus.as_ref() else { continue };
            let d = &self.departures[i];
            let trip = &self.data.trips[d.trip];
            let next = b.stops.front().and_then(|s| self.data.bus_stops.iter().find(|x| x.object_id == s.id)).map(|x| x.name.trim().to_string()).unwrap_or_default();
            let pos = car.vehicle.position;
            if o.len() > 1 {
                o.push(',');
            }
            let _ = write!(
                o,
                "{{\"id\":{},\"line\":{},\"tour\":{},\"trip\":{},\"destination\":{},\"x\":{:.1},\"y\":{:.1},\"heading\":{:.0},\"speed_kmh\":{:.0},\"delay\":{:.0},\"passengers\":{},\"at_stop\":{},\"layover\":{},\"next\":{},\"number\":{},\"bus\":{}}}",
                car.id,
                js(&self.display_line(i)),
                js(&format!("{}/{}", d.line.trim(), d.tour.trim())),
                js(trip.name.trim()),
                js(b.terminus.trim()),
                pos.x,
                pos.y,
                car.vehicle.heading.rem_euclid(360.0),
                car.state.speed.max(0.0) * 3.6,
                if b.delay.is_finite() { b.delay } else { 0.0 },
                aboard.get(&car.id).copied().unwrap_or(0),
                car.at_stop(),
                b.layover,
                js(&next),
                js(car.vehicle.str_var("number").trim()),
                js(&car.vehicle.ty.def.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()),
            );
        }
        o.push(']');
        o
    }

    /// (NexiconDK) Bus Company Online's control room: the companies' timetable buses on the
    /// road now, by their timetable line and tour (as a company's plan names them), in world
    /// metres. `aboard`: how many people sit in each (`PeopleSim::ai_aboard`).
    pub fn live_buses(&self, traffic: &TrafficSim, aboard: &HashMap<u64, usize>) -> Vec<LiveBus> {
        let mut out = Vec::new();
        for car in &traffic.cars {
            let Some(&i) = self.car_departure.get(&car.id) else { continue };
            let Some(b) = car.bus.as_ref() else { continue };
            let d = &self.departures[i];
            // (a company's only: the map's own lines are nobody's control room's)
            let Some(company) = self.line_owner(&d.line) else { continue };
            let trip = &self.data.trips[d.trip];
            let next = b.stops.front().and_then(|s| self.data.bus_stops.iter().find(|x| x.object_id == s.id)).map(|x| x.name.trim().to_string()).unwrap_or_default();
            let pos = car.vehicle.position;
            out.push(LiveBus {
                company,
                line: d.line.trim().to_string(),
                tour: d.tour.trim().to_string(),
                trip: trip.name.trim().to_string(),
                dep: (d.time / 60.0).round() as i32 + if d.yesterday { 1440 } else { 0 },
                x: pos.x,
                y: pos.y,
                heading: car.vehicle.heading.rem_euclid(360.0),
                speed_kmh: car.state.speed.max(0.0) as f64 * 3.6,
                delay: if b.delay.is_finite() { b.delay } else { 0.0 },
                passengers: aboard.get(&car.id).copied().unwrap_or(0),
                at_stop: car.at_stop(),
                layover: b.layover,
                next,
                terminus: b.terminus.trim().to_string(),
                number: car.vehicle.str_var("number").trim().to_string(),
            });
        }
        out
    }
}

/// (NexiconDK) A timetable bus on the road now (`ScheduleSim::live_buses`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LiveBus {
    /// The company whose line it is (`TimetableCompany::owner`).
    pub company: String,
    pub line: String,
    pub tour: String,
    pub trip: String,
    /// The trip's departure, minutes of the day (a night trip after midnight past 1440).
    pub dep: i32,
    pub x: f64,
    pub y: f64,
    pub heading: f64,
    pub speed_kmh: f64,
    /// Seconds late (negative: early).
    pub delay: f64,
    pub passengers: usize,
    pub at_stop: bool,
    pub layover: bool,
    pub next: String,
    pub terminus: String,
    /// Its fleet number.
    pub number: String,
}

/// A JSON string.
fn js(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 32 => {}
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// A way of the map: a road, a path (pavements, bike paths, stairs) or a rail, by the
/// spline's file name; None for what is no way (markings, kerbs, walls, plants, wires).
fn way_kind(file: &str) -> Option<&'static str> {
    let n = file.to_lowercase().replace('\\', "/");
    let n = n.rsplit('/').next().unwrap_or("").to_string();
    let has = |words: &[&str]| words.iter().any(|w| n.contains(w));
    // a word that starts at the beginning or after a character that is not a letter
    let starts_word = |w: &str, seps: &dyn Fn(char) -> bool| {
        n.match_indices(w).any(|(i, _)| i == 0 || n[..i].chars().last().is_some_and(|c| seps(c)))
    };
    let not_letter = |c: char| !c.is_ascii_lowercase();
    let sep = |c: char| c == '_' || c == '-' || c == ' ';
    let digits = n.chars().take_while(|c| c.is_ascii_digit()).count();
    if has(&["grün", "gruen", "grun", "zebra", "pfeil", "schad", "invis_human", "invis_sidewalk", "markier", "haltelinie", "wait_line", "_n_line", "normal_line"])
        || (digits > 0 && n[digits..].starts_with("cm"))
    {
        return None;
    }
    if has(&["rail", "gleis", "schiene", "tram", "strab", "weiche"]) {
        return Some("skinne");
    }
    let tree = n.match_indices("tree").any(|(i, _)| i == 0 || !n[..i].ends_with('s'));
    if tree
        || has(&[
            "mauer", "wall", "zaun", "fence", "hecke", "busch", "buesche", "terrain", "bord", "kant", "damm", "fahrdraht", "geländer", "gelander", "baum", "wasser", "water", "ts-hbf", "rbk_", "rohr", "kabel", "quertrag", "unterstand", "gras", "hexa", "dorfteich", "randstein", "decke",
        ])
    {
        return None;
    }
    let road_start = ["ngw4_", "2ngw4_", "str", "o2_str", "avgstr", "rq_"].iter().any(|p| n.starts_with(p));
    if road_start || n.contains("spur") {
        return Some("vej");
    }
    let ngw_digit = n.match_indices("ngw").any(|(i, _)| (i == 0 || not_letter(n[..i].chars().last().unwrap())) && n[i + 3..].chars().next().is_some_and(|c| c.is_ascii_digit()));
    if n.starts_with("gw")
        || ngw_digit
        || ["gw", "gehweg", "ped", "sidewalk", "fuß", "fus", "bike", "rad", "bussteig", "bürgersteig", "burgersteig", "treppe", "grw"].iter().any(|w| starts_word(w, &not_letter))
    {
        return Some("sti");
    }
    if starts_word("str", &sep)
        || starts_word("o2_str", &sep)
        || has(&["street", "rq_", "fahrbahn", "asph", "kopfstein", "pflaster", "betonplatten", "einfahrt", "zufahrt", "schotter", "brücke", "brucke", "piste", "spur", "kreuz", "autobahn", "road"])
    {
        return Some("vej");
    }
    None
}

/// A way's width (m): the last "<n> m" of its file name, else the kind's usual one.
fn way_width(file: &str, kind: &str) -> f64 {
    let n = file.to_lowercase().replace('\\', "/");
    let n = n.rsplit('/').next().unwrap_or("");
    let b = n.as_bytes();
    let mut found = None;
    let mut i = 0;
    while i < b.len() {
        if b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_digit()) {
            let start = i;
            while i < b.len() && (b[i].is_ascii_digit() || ((b[i] == b'.' || b[i] == b',') && i + 1 < b.len() && b[i + 1].is_ascii_digit())) {
                i += 1;
            }
            let mut j = i;
            while j < b.len() && b[j] == b' ' {
                j += 1;
            }
            if j < b.len() && b[j] == b'm' && !(j + 1 < b.len() && b[j + 1].is_ascii_lowercase()) {
                found = n[start..i].replace(',', ".").parse::<f64>().ok();
            }
        } else {
            i += 1;
        }
    }
    found
        .unwrap_or(match kind {
            "vej" => 7.0,
            "sti" => 2.5,
            _ => 3.0,
        })
        .clamp(1.5, 20.0)
}

/// A spline's centre line in tile metres: straight, or an arc (radius > 0 turns right).
fn spline_points(x: f64, y: f64, heading: f64, length: f64, radius: f64) -> Vec<DVec2> {
    let a = heading.to_radians();
    if radius.abs() < 1e-6 || length <= 0.0 {
        return vec![DVec2::new(x, y), DVec2::new(x + length.max(0.0) * a.sin(), y + length.max(0.0) * a.cos())];
    }
    let (cx, cy) = (x + radius * a.cos(), y - radius * a.sin());
    let theta = length / radius;
    let n = ((theta.abs() / 4f64.to_radians()) as usize + 1).max((length / 12.0) as usize + 1).max(2);
    (0..=n)
        .map(|i| {
            let h = a + theta * i as f64 / n as f64;
            DVec2::new(cx - radius * h.cos(), cy + radius * h.sin())
        })
        .collect()
}

/// Which points of a line to keep so that none left out lies further than `tol` from the
/// line through the kept ones (Douglas-Peucker).
fn simplify(pts: &[DVec2], tol: f64) -> Vec<DVec2> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0usize, n - 1)];
    while let Some((a, b)) = stack.pop() {
        let (pa, pb) = (pts[a], pts[b]);
        let ab = pb - pa;
        let len = ab.length();
        let mut worst = (0.0f64, 0usize);
        for (k, p) in pts.iter().enumerate().take(b).skip(a + 1) {
            let d = if len < 1e-9 { (*p - pa).length() } else { (*p - pa).perp_dot(ab).abs() / len };
            if d > worst.0 {
                worst = (d, k);
            }
        }
        if worst.0 > tol {
            keep[worst.1] = true;
            stack.push((a, worst.1));
            stack.push((worst.1, b));
        }
    }
    pts.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

/// A line in decimetres: the first point, then each from the one before.
fn encode(o: &mut String, pts: &[DVec2]) {
    let mut last: Option<(i64, i64)> = None;
    for p in pts {
        let q = ((p.x * 10.0).round() as i64, (p.y * 10.0).round() as i64);
        match last {
            None => {
                let _ = write!(o, "{},{}", q.0, q.1);
            }
            Some(l) => {
                let _ = write!(o, ",{},{}", q.0 - l.0, q.1 - l.1);
            }
        }
        last = Some(q);
    }
}

/// The live map of the map in `map_dir` (named `map_name`, as `server.cfg` names it) with
/// the timetable `tt`: its tiles read once, here.
pub fn build_web_map(map_dir: &Path, map_name: &str, tt: &WebTimetable) -> WebMap {
    use rayon::prelude::*;
    const TS: f64 = 300.0;
    let t0 = std::time::Instant::now();
    let Ok(g) = omsi_map::global::GlobalCfg::load(&map_dir.join("global.cfg")) else {
        log::warn!("live map: no global.cfg in {}", map_dir.display());
        return WebMap::default();
    };
    // each tile: its splines (centre line, kind, width) and its objects' places
    type TileOut = (Vec<((i32, i32, i64), Vec<DVec2>, Option<&'static str>, f64)>, Vec<(i64, DVec2)>);
    let read: Vec<TileOut> = g
        .tiles
        .par_iter()
        .map(|t| {
            let Ok(tile) = omsi_map::tile::Tile::load(&map_dir.join(&t.file)) else { return (Vec::new(), Vec::new()) };
            let o = DVec2::new(t.x as f64 * TS, t.y as f64 * TS);
            let splines = tile
                .splines
                .iter()
                .filter(|s| !s.deleted)
                .map(|s| {
                    let pts: Vec<DVec2> = spline_points(s.pos[0], s.pos[1], s.heading, s.length, s.radius).into_iter().map(|p| p + o).collect();
                    let kind = way_kind(&s.file);
                    ((t.x, t.y, s.id), pts, kind, kind.map(|k| way_width(&s.file, k)).unwrap_or(0.0))
                })
                .collect();
            let objects = tile.objects.iter().chain(tile.attach_objects.iter()).map(|ob| (ob.id, o + DVec2::new(ob.pos[0], ob.pos[1]))).collect();
            (splines, objects)
        })
        .collect();
    let mut lines: HashMap<(i32, i32, i64), Vec<DVec2>> = HashMap::new();
    let mut places: HashMap<i64, DVec2> = HashMap::new();
    let mut ways: [(&str, String); 3] = [("vej", String::new()), ("sti", String::new()), ("skinne", String::new())];
    for (splines, objects) in read {
        for (key, pts, kind, width) in splines {
            if let Some(k) = kind {
                if let Some(w) = ways.iter_mut().find(|w| w.0 == k) {
                    if !w.1.is_empty() {
                        w.1.push(',');
                    }
                    let _ = write!(w.1, "[{},", (width * 10.0).round() as i64);
                    encode(&mut w.1, &pts);
                    w.1.push(']');
                }
            }
            lines.insert(key, pts);
        }
        places.extend(objects);
    }
    let mut o = String::new();
    let _ = write!(o, "{{\"map\":{},\"name\":{},\"tile\":{TS},\"tiles\":[", js(map_name), js(if g.friendly_name.trim().is_empty() { g.name.trim() } else { g.friendly_name.trim() }));
    let tiles: Vec<String> = g.tiles.iter().map(|t| format!("[{},{}]", t.x, t.y)).collect();
    o.push_str(&tiles.join(","));
    o.push_str("],\"stops\":[");
    // the entry points: one marker per stop (those of a name within 250 m of each other
    // together), a depot (`#`) square, the lines in the name's brackets apart
    let mut stops: Vec<(String, Vec<DVec2>)> = Vec::new();
    for e in &g.entry_points {
        let Some(&(tx, ty)) = g.raw_tiles.get(usize::try_from(e.group).unwrap_or(usize::MAX)) else { continue };
        let p = DVec2::new(tx as f64 * TS + e.pos[0], ty as f64 * TS + e.pos[2]);
        let name = e.name.trim().to_string();
        match stops.iter_mut().find(|s| s.0 == name && s.1[0].distance(p) < 250.0) {
            Some(s) => s.1.push(p),
            None => stops.push((name, vec![p])),
        }
    }
    let stops: Vec<String> = stops
        .iter()
        .map(|(name, pts)| {
            let c = pts.iter().copied().sum::<DVec2>() / pts.len() as f64;
            let depot = name.starts_with('#');
            let bare = name.trim_start_matches(['#', ' ']);
            let (label, lines) = match (bare.rfind('['), bare.ends_with(']')) {
                (Some(i), true) => (bare[..i].trim(), bare[i + 1..bare.len() - 1].trim()),
                _ => (bare, ""),
            };
            format!("{{\"n\":{},\"l\":{},\"d\":{depot},\"x\":{:.1},\"y\":{:.1}}}", js(label), js(lines), c.x, c.y)
        })
        .collect();
    o.push_str(&stops.join(","));
    o.push_str("],\"veje\":{");
    let ways: Vec<String> = ways.iter().map(|(k, v)| format!("{}:[{}]", js(k), v)).collect();
    o.push_str(&ways.join(","));
    // the tours, compact: the stops they call at (name, place), the termini, and per tour its
    // trips: [terminus, first departure, stop, stop, seconds since the one before, ...]; which
    // trip each is (`tt`), and the lines with their trips
    let mut stop_ix: HashMap<(String, i64), usize> = HashMap::new();
    let mut stop_list: Vec<String> = Vec::new();
    let mut terms: Vec<String> = Vec::new();
    let mut term_ix: HashMap<String, usize> = HashMap::new();
    let mut tours: Vec<String> = Vec::new();
    let mut tour_trips: Vec<String> = Vec::new();
    let mut stop_of = |name: &str, station: i64| -> usize {
        *stop_ix.entry((name.to_string(), station)).or_insert_with(|| {
            match places.get(&station) {
                Some(p) => stop_list.push(format!("[{},{:.1},{:.1}]", js(name), p.x, p.y)),
                None => stop_list.push(format!("[{},null,null]", js(name))),
            }
            stop_list.len() - 1
        })
    };
    for (key, calls) in &tt.tours {
        let mut entries: Vec<String> = Vec::new();
        let mut which: Vec<String> = Vec::new();
        let mut k = 0;
        while k < calls.len() {
            let trip = calls[k].trip;
            let mut end = k;
            while end < calls.len() && calls[end].trip == trip {
                end += 1;
            }
            let term = tt.trips[trip].terminus.clone();
            let ti = *term_ix.entry(term.clone()).or_insert_with(|| {
                terms.push(js(&term));
                terms.len() - 1
            });
            let mut e = format!("[{ti},{},{}", calls[k].dep.round() as i64, stop_of(&calls[k].name, calls[k].station));
            for w in calls[k..end].windows(2) {
                let _ = write!(e, ",{},{}", stop_of(&w[1].name, w[1].station), (w[1].dep - w[0].dep).round() as i64);
            }
            e.push(']');
            entries.push(e);
            which.push(trip.to_string());
            k = end;
        }
        tours.push(format!("{}:[{}]", js(key), entries.join(",")));
        tour_trips.push(format!("{}:[{}]", js(key), which.join(",")));
    }
    let trip_names: Vec<String> = tt.trips.iter().map(|t| js(&t.name)).collect();
    let line_list: Vec<String> = tt
        .lines
        .iter()
        .map(|(name, trips)| format!("[{},[{}]]", js(name), trips.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(",")))
        .collect();
    let _ = write!(
        o,
        "}},\"ture\":{{\"stops\":[{}],\"terms\":[{}],\"tours\":{{{}}},\"tt\":{{{}}},\"trips\":[{}],\"lines\":[{}]}}}}",
        stop_list.join(","),
        terms.join(","),
        tours.join(","),
        tour_trips.join(","),
        trip_names.join(","),
        line_list.join(",")
    );
    // each trip's route: its splines end to end (each turned to carry on from the one
    // before), thinned to its bends; and its stops' places
    let routes: Vec<Vec<u8>> = tt
        .trips
        .par_iter()
        .map(|t| {
            let mut pts: Vec<DVec2> = Vec::new();
            for (n, key) in t.pieces.iter().enumerate() {
                let Some(line) = lines.get(key) else { continue };
                let mut seg = line.clone();
                // (the first spline: the way that leads on to the next)
                let flip = match pts.last() {
                    Some(last) => seg.last().unwrap().distance(*last) < seg[0].distance(*last),
                    None => t.pieces.get(n + 1).and_then(|k| lines.get(k)).is_some_and(|next| {
                        let near = |p: DVec2| next[0].distance(p).min(next.last().unwrap().distance(p));
                        near(seg[0]) < near(*seg.last().unwrap())
                    }),
                };
                if flip {
                    seg.reverse();
                }
                if pts.last().is_some_and(|l| l.distance(seg[0]) < 0.5) {
                    seg.remove(0);
                }
                pts.extend(seg);
            }
            let pts = simplify(&pts, 1.0);
            let mut r = String::from("{\"p\":[");
            encode(&mut r, &pts);
            r.push_str("],\"s\":[");
            let s: Vec<String> = t.stations.iter().filter_map(|id| places.get(id)).map(|p| format!("[{:.1},{:.1}]", p.x, p.y)).collect();
            r.push_str(&s.join(","));
            r.push_str("]}");
            r.into_bytes()
        })
        .collect();
    log::info!(
        "live map: {} tiles, {} tours, {} trips, {:.1} MB, in {:.1} s",
        g.tiles.len(),
        tt.tours.len(),
        tt.trips.len(),
        (o.len() + routes.iter().map(|r| r.len()).sum::<usize>()) as f64 / 1e6,
        t0.elapsed().as_secs_f64()
    );
    WebMap { map: o.into_bytes(), routes }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ways_by_their_files() {
        assert_eq!(way_kind("Splines\\Straßen\\str_2spur_7m.sli"), Some("vej"));
        assert_eq!(way_kind("Splines\\Gehweg_3m.sli"), Some("sti"));
        assert_eq!(way_kind("Splines\\Gleis_Beton.sli"), Some("skinne"));
        assert_eq!(way_kind("Splines\\street_4m.sli"), Some("vej"));
        assert_eq!(way_kind("Splines\\street_tree_row.sli"), None);
        assert_eq!(way_kind("Splines\\Baumreihe.sli"), None);
        assert_eq!(way_kind("Splines\\Zebra_4m.sli"), None);
        assert_eq!(way_kind("Splines\\10cm_line.sli"), None);
        assert_eq!(way_width("str_2spur_7,5m.sli", "vej"), 7.5);
        assert_eq!(way_width("str_mitte.sli", "vej"), 7.0);
        assert_eq!(way_width("gw_40m.sli", "sti"), 20.0);
    }

    #[test]
    fn a_curve_ends_where_it_turns_to() {
        // a quarter circle of radius 10 to the right, heading north: it ends 10 m east,
        // 10 m north
        let p = spline_points(0.0, 0.0, 0.0, std::f64::consts::FRAC_PI_2 * 10.0, 10.0);
        let e = *p.last().unwrap();
        assert!((e.x - 10.0).abs() < 1e-6 && (e.y - 10.0).abs() < 1e-6, "{e:?}");
        let mut o = String::new();
        encode(&mut o, &simplify(&[DVec2::new(0.0, 0.0), DVec2::new(5.0, 0.01), DVec2::new(10.0, 0.0)], 0.5));
        assert_eq!(o, "0,0,100,0");
    }
}
