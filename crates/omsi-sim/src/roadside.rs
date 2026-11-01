//! (NexiconDK, 1 November 2026) What stands by the road for the player's driving: speed
//! cameras, red-light cameras at the traffic lights and speed displays in town.
//!
//! *Where* is decided here, from the road's own identity in the map - the tile, the spline or
//! crossing object and the direction ([`site`]) - so that a camera stands in the same place on
//! every drive and for every player of a session. (The drive watch had chosen them by the
//! lane's number in the traffic network, which goes by the order the tiles were loaded in: the
//! cameras stood elsewhere after another start.)
//!
//! * A **speed camera** stands on a share of the roads (more on fast ones, some where the limit
//!   is low), 20 to 60 m into its piece of road, beside the outermost lane of its direction,
//!   turned to the traffic coming. It measures every lane of that direction: the player's bus
//!   past it faster than the limit and the tolerance is fined (the drive watch) and the camera
//!   flashes.
//! * A **red-light camera** stands at a share of the traffic lights, at the stop line beside
//!   the outermost lane of the light's approach: it flashes when the player's bus crosses the
//!   line on red (the drive watch fines every red light run, as before).
//! * A **speed display** stands on a share of the roads in town (limit 50 or less) and shows
//!   the bus's speed as it comes, then a smiling or a sad face. It fines nobody.
//!
//! The models are add-ons the player installs in his OMSI folder ([`MODELS`]): those he has are
//! mixed by the place's own number; without any of a kind the place is still measured, unseen.
//! Their own scripts flash them and fill the display, driven through their `TrafficLightPhase`
//! ([`phase_of`]): the display's script measures the time between two phases five metres
//! apart, as it does between the induction loops it was made for.

use crate::traffic::{Lane, LaneKind};
use glam::DVec3;
use std::collections::HashMap;
use std::sync::Mutex;

/// What stands at a place.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Speed,
    RedLight,
    Display,
}

impl Kind {
    fn salt(self) -> u8 {
        match self {
            Kind::Speed => 0x5a,
            Kind::RedLight => 0x3c,
            Kind::Display => 0x71,
        }
    }

    fn code(self) -> i64 {
        match self {
            Kind::Speed => 0,
            Kind::RedLight => 1,
            Kind::Display => 2,
        }
    }
}

/// How a model flashes: its own script and lamp (driven by its `TrafficLightPhase`), or a
/// light the game puts at its lens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flash {
    Script,
    Light,
    None,
}

/// A model of the roadside.
#[derive(Clone, Copy, Debug)]
pub struct Model {
    pub kind: Kind,
    /// The object, under the OMSI folder.
    pub file: &'static str,
    /// The add-on it comes with (the player is told which he lacks).
    pub package: &'static str,
    /// Its front - the lens, the display - faces the object's own forward (`false`) or
    /// backward (`true`, +Z or -Z of its mesh): what turns it to the traffic coming.
    pub faces_back: bool,
    /// The lens as it stands: its height and how far before the object's origin (m).
    pub lens: (f32, f32),
    pub flash: Flash,
    /// Parts placed with it (the display's bracket), at the same place.
    pub parts: &'static [&'static str],
    /// Other files it needs (the display's font).
    pub needs: &'static [&'static str],
    /// Placed this high over the road (the display hangs on its pole).
    pub height: f32,
}

/// Every model the game places, by kind; those installed are used.
pub const MODELS: &[Model] = &[
    Model { kind: Kind::Speed, file: concat!("Sceneryobjects\\Reutlingen_SH\\", "Blitzer_Typ1.sco"), package: "Blitzer_SH (Reutlingen_SH)", faces_back: true, lens: (2.63, 0.2), flash: Flash::Script, parts: &[], needs: &[], height: 0.0 },
    Model { kind: Kind::Speed, file: concat!("Sceneryobjects\\Herr-Z\\Zubehoer\\Blitzer\\", "Blitzer_Saeule.sco"), package: "Herr-Z Blitzer v1.1", faces_back: false, lens: (1.9, 0.24), flash: Flash::Light, parts: &[], needs: &[], height: 0.0 },
    Model { kind: Kind::Speed, file: concat!("Sceneryobjects\\Herr-Z\\Zubehoer\\Blitzer\\", "Blitzer_Star.sco"), package: "Herr-Z Blitzer v1.1", faces_back: false, lens: (2.03, 0.23), flash: Flash::Light, parts: &[], needs: &[], height: 0.0 },
    Model { kind: Kind::RedLight, file: concat!("Sceneryobjects\\Herr-Z\\Zubehoer\\Blitzer\\", "Blitzer_Ampel.sco"), package: "Herr-Z Blitzer v1.1", faces_back: false, lens: (2.0, 0.18), flash: Flash::Light, parts: &[], needs: &[], height: 0.0 },
    Model { kind: Kind::RedLight, file: concat!("Sceneryobjects\\Reutlingen_SH\\", "Ampelblitzer_Typ1.sco"), package: "Ampelblitzer_SH (Reutlingen_SH)", faces_back: true, lens: (2.63, 0.2), flash: Flash::Script, parts: &[], needs: &[], height: 0.0 },
    Model {
        kind: Kind::Display,
        file: concat!("Sceneryobjects\\Reutlingen_SH\\", "Geschwindigkeitsanzeigeanlage_Typ1.sco"),
        package: "Geschwindigkeitsanzeigeanlage_SH (Reutlingen_SH)",
        faces_back: true,
        lens: (0.0, 0.0),
        flash: Flash::None,
        parts: &[concat!("Sceneryobjects\\Reutlingen_SH\\", "GAA_Halterung_Solarpanel.sco"), POLE],
        needs: &["Fonts\\GAA_T1_SH.oft"],
        height: DISPLAY_HEIGHT,
    },
];

/// The pole the display hangs on: the game's own (`pole_files`), in its content folder.
pub const POLE: &str = "Sceneryobjects\\Omsi-Hub\\Roadside\\GAA_Mast.sco";
/// The display's centre over the road (m), and where the pole stands behind it (its own
/// forward, m).
pub const DISPLAY_HEIGHT: f32 = 2.4;
const POLE_BEHIND: f32 = 0.46;

/// How far beside the edge of its lane a model stands (m).
const KERB: f32 = 1.4;

/// A place of the roadside: what, where along its lane, the lane's limit, and the place's own
/// number (the model is chosen by it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Site {
    pub kind: Kind,
    /// Metres into the lane: where it measures (and stands).
    pub along: f32,
    pub limit: f32,
    pub number: u64,
}

/// FNV-1a over the place's identity and a byte for its kind, mixed through (MurmurHash3's
/// finaliser).
fn hash(tile: (i32, i32), id: i64, side: u8, salt: u8) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut put = |b: u8| {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    for b in tile.0.to_le_bytes().into_iter().chain(tile.1.to_le_bytes()).chain(id.to_le_bytes()) {
        put(b);
    }
    put(side);
    put(salt);
    // (FNV alone carries a last byte hardly into the high bits: a display's number came out
    // next to its road's camera's, and the two shared their roads - mixed through)
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^ (h >> 33)
}

/// Two numbers of 0..1 from a place's hash.
fn unit(h: u64) -> (f64, f64) {
    let u = (h >> 11) as f64 / 9_007_199_254_740_992.0;
    let v = (h.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 11) as f64 / 9_007_199_254_740_992.0;
    (u, v)
}

/// The share of the roads with a speed camera, by the limit (km/h).
fn camera_share(limit: f32) -> f64 {
    if limit >= 70.0 {
        0.12
    } else if limit >= 60.0 {
        0.08
    } else if limit <= 30.0 {
        0.06
    } else {
        0.025
    }
}

/// The share of the roads in town with a speed display, and of the lights with a camera.
const DISPLAY_SHARE: f64 = 0.03;
const RED_LIGHT_SHARE: f64 = 0.2;
/// The display measures this far before it (m): the two "loops", five metres apart.
pub const DISPLAY_MEASURE: f32 = 30.0;
pub const LOOPS: f32 = 5.0;

/// A road of the map a camera or a display may stand by: a street lane of a spline (not an
/// editor-only one, not closed to cars).
fn on_road(l: &Lane) -> bool {
    l.kind == LaneKind::Street && l.source == 1 && l.key.is_some() && !l.invisible && !l.no_cars
}

/// The place on lane `l`, if one is there. Every lane of a road's direction has the same (its
/// spline, its direction); a crossing's lanes of one light the same red-light camera.
pub fn site(l: &Lane) -> Option<Site> {
    let key = l.key?;
    let limit = l.speed_limit_kmh;
    if on_road(l) && limit > 0.0 {
        let side = l.reversed as u8;
        let h = hash(key.tile, key.id, side, Kind::Speed.salt());
        let (u, v) = unit(h);
        let along = (20.0 + 40.0 * v) as f32;
        if u < camera_share(limit) && along + 5.0 <= l.length() {
            return Some(Site { kind: Kind::Speed, along, limit, number: h });
        }
        if limit <= 50.0 {
            let h = hash(key.tile, key.id, side, Kind::Display.salt());
            let (u, v) = unit(h);
            let along = DISPLAY_MEASURE + 10.0 + (40.0 * v) as f32;
            if u < DISPLAY_SHARE && along + 5.0 <= l.length() {
                return Some(Site { kind: Kind::Display, along, limit, number: h });
            }
        }
        return None;
    }
    // a crossing's lane that a light holds: the light's camera, at its stop line
    if l.kind == LaneKind::Street && l.source == 2 && !l.invisible {
        let (_, light) = l.traffic_light?;
        let h = hash(key.tile, key.id, light as u8, Kind::RedLight.salt());
        if unit(h).0 < RED_LIGHT_SHARE {
            return Some(Site { kind: Kind::RedLight, along: 0.5, limit, number: h });
        }
    }
    None
}

/// The lanes of one direction of a road (or of one light's approach), whose outermost one the
/// camera stands by.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Group {
    pub tile: (i32, i32),
    pub id: i64,
    pub side: u32,
}

pub fn group(l: &Lane) -> Option<Group> {
    let key = l.key?;
    let side = if l.source == 2 { l.traffic_light.map(|t| t.1 as u32)? } else { l.reversed as u32 };
    Some(Group { tile: key.tile, id: key.id, side })
}

/// How far to the outside of the road (right, or left where traffic keeps left) lane `l`
/// lies at its place `along` (m): the outermost lane of a group is the one the model stands by.
pub fn outside(l: &Lane, along: f32, left_hand: bool) -> f64 {
    let (p, h) = l.at(along);
    let hr = (h as f64).to_radians();
    let right = DVec3::new(hr.cos(), -hr.sin(), 0.0);
    let d = p.dot(right);
    if left_hand {
        -d
    } else {
        d
    }
}

/// Where a model of `site` stands by lane `l` (its outermost lane): its foot and its heading
/// (degrees clockwise from north, as the map's objects), turned to the traffic coming.
pub fn pose(l: &Lane, site: &Site, model: &Model, left_hand: bool) -> (DVec3, f64) {
    let (p, h) = l.at(site.along.min(l.length()));
    let hr = (h as f64).to_radians();
    let right = DVec3::new(hr.cos(), -hr.sin(), 0.0);
    let out = if left_hand { -1.0 } else { 1.0 };
    let beside = (l.width.max(2.5) * 0.5 + KERB) as f64 * out;
    // (a red-light camera a step before its stop line)
    let back = if site.kind == Kind::RedLight { -1.5 } else { 0.0 };
    let fwd = DVec3::new(hr.sin(), hr.cos(), 0.0);
    let foot = p + right * beside + fwd * back + DVec3::Z * model.height as f64;
    // (the front faces the traffic coming: back along the lane)
    let heading = if model.faces_back { h as f64 } else { h as f64 + 180.0 };
    (foot, heading.rem_euclid(360.0))
}

/// Where the lens of a model standing at `foot` with `heading` is (for the flash's light).
pub fn lens(model: &Model, foot: DVec3, heading: f64) -> DVec3 {
    let hr = heading.to_radians();
    let fwd = DVec3::new(hr.sin(), hr.cos(), 0.0);
    // (its front: the object's backward when it faces back)
    let front = if model.faces_back { -fwd } else { fwd };
    foot + DVec3::Z * model.lens.0 as f64 + front * model.lens.1 as f64
}

/// The display's pole: behind it, from the ground.
pub fn pole_foot(foot: DVec3, heading: f64) -> DVec3 {
    let hr = heading.to_radians();
    let fwd = DVec3::new(hr.sin(), hr.cos(), 0.0);
    // (the display faces back: its pole stands on its forward side, under it)
    foot + fwd * POLE_BEHIND as f64 - DVec3::Z * DISPLAY_HEIGHT as f64
}

/// The model of `site` among those installed (`installed[i]` for [`MODELS`]`[i]`), chosen by
/// the place's number; None when none of its kind is installed.
pub fn model_of(site: &Site, installed: &[bool]) -> Option<&'static Model> {
    let have: Vec<&'static Model> = MODELS.iter().enumerate().filter(|(i, m)| m.kind == site.kind && installed.get(*i).copied().unwrap_or(false)).map(|(_, m)| m).collect();
    if have.is_empty() {
        return None;
    }
    Some(have[((site.number >> 32) as usize) % have.len()])
}

// --- the objects' ids, and what their scripts are given -------------------------------------

/// The ids the placed objects take: below every id of a map (negative), the kind in the low
/// bits, a part (the display's bracket and pole) above them.
const ID_BASE: i64 = -(1 << 60);

pub fn object_id(site: &Site, part: usize) -> i64 {
    ID_BASE - (((site.number & 0x0000_ffff_ffff_ffff) as i64) << 4) - ((part as i64) << 2) - site.kind.code()
}

/// The kind of a placed object's id, when it is one of these.
pub fn kind_of_id(id: i64) -> Option<Kind> {
    if !(ID_BASE - (1 << 53)..=ID_BASE).contains(&id) {
        return None;
    }
    match (ID_BASE - id) & 3 {
        0 => Some(Kind::Speed),
        1 => Some(Kind::RedLight),
        2 => Some(Kind::Display),
        _ => None,
    }
}

/// A placed object's script, in the middle of what it shows.
#[derive(Clone, Copy, Debug)]
enum Show {
    /// A flash, since this time of the game's day (s).
    Flash(f64),
    /// A measurement: the first loop at this time, the second this many seconds later.
    Measure(f64, f64),
}

/// The scripts' shows, by object id; and the flashes the game lights at a lens (where, since
/// when).
static SHOWS: Mutex<Option<HashMap<i64, Show>>> = Mutex::new(None);
static LIGHTS: Mutex<Vec<(DVec3, std::time::Instant)>> = Mutex::new(Vec::new());

/// Seconds since `t0` on the game's clock of the day (round midnight).
fn since(now: f64, t0: f64) -> f64 {
    let d = now - t0;
    if d < -43_200.0 {
        d + 86_400.0
    } else {
        d
    }
}

/// The object `id` flashes now (`now`: the game's time of day, s).
pub fn flash(id: i64, now: f64) {
    SHOWS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(id, Show::Flash(now));
}

/// The display `id` measures a vehicle at `speed` (m/s) now.
pub fn measure(id: i64, now: f64, speed: f64) {
    let gap = (LOOPS as f64 / speed.max(0.5)).min(10.0);
    SHOWS.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(id, Show::Measure(now, gap));
}

/// A flash of light at a lens (a model without a lamp of its own).
pub fn flash_light(at: DVec3) {
    LIGHTS.lock().unwrap_or_else(|e| e.into_inner()).push((at, std::time::Instant::now()));
}

/// The flashes of light lit now: where, and how strong (0..1).
pub fn lights_now() -> Vec<(DVec3, f32)> {
    let mut l = LIGHTS.lock().unwrap_or_else(|e| e.into_inner());
    l.retain(|(_, t)| t.elapsed().as_secs_f32() < 1.0);
    l.iter().filter_map(|(p, t)| flash_on(t.elapsed().as_secs_f64()).then_some((*p, 1.0))).collect()
}

/// A camera's flash: two short ones, 0.4 s apart (as the stock induction loop drives them).
fn flash_on(t: f64) -> bool {
    (0.0..0.1).contains(&t) || (0.4..0.5).contains(&t)
}

/// What a placed object's script reads as its `TrafficLightPhase` and `TrafficLightApproach`
/// now (`now`: the game's time of day), when `id` is one of these. Idle a camera reads 12
/// (dark: its lamp off); flashing 0 (the speed camera's script lights its lamp on 0) or, for a
/// red-light camera, 8 with a vehicle coming (its script's trigger). A display reads 0 idle,
/// 1 at its first loop and 2 at its second.
pub fn phase_of(id: i64, now: f64) -> Option<(f32, f32)> {
    let kind = kind_of_id(id)?;
    let mut g = SHOWS.lock().unwrap_or_else(|e| e.into_inner());
    let shows = g.get_or_insert_with(HashMap::new);
    let show = shows.get(&id).copied();
    let idle = match kind {
        Kind::Display => (0.0, 0.0),
        _ => (12.0, 0.0),
    };
    let Some(show) = show else { return Some(idle) };
    let (value, over) = match (kind, show) {
        (Kind::Speed, Show::Flash(t0)) => {
            let t = since(now, t0);
            (if flash_on(t) { (0.0, 0.0) } else { idle }, t > 1.0)
        }
        (Kind::RedLight, Show::Flash(t0)) => {
            let t = since(now, t0);
            (if t < 0.2 { (8.0, 1.0) } else { idle }, t > 1.5)
        }
        (Kind::Display, Show::Measure(t0, gap)) => {
            let t = since(now, t0);
            (if t < gap { (1.0, 0.0) } else if t < gap + 1.0 { (2.0, 0.0) } else { idle }, t > gap + 1.5)
        }
        _ => (idle, true),
    };
    if over {
        shows.remove(&id);
    }
    Some(value)
}

/// The placed objects' shows forgotten (another session).
pub fn reset() {
    *SHOWS.lock().unwrap_or_else(|e| e.into_inner()) = None;
    LIGHTS.lock().unwrap_or_else(|e| e.into_inner()).clear();
    PLACED.lock().unwrap_or_else(|e| e.into_inner()).take();
}

// --- what stands, and what it does when the player's bus passes -----------------------------

/// The models standing (put there as their tiles were staged): by the id of the place's model,
/// its lens and how it flashes.
static PLACED: Mutex<Option<HashMap<i64, (DVec3, Flash)>>> = Mutex::new(None);

/// The model of place `id` (`object_id(site, 0)`) stands, its lens at `lens`.
pub fn place(id: i64, lens: DVec3, flash: Flash) {
    PLACED.lock().unwrap_or_else(|e| e.into_inner()).get_or_insert_with(HashMap::new).insert(id, (lens, flash));
}

/// The camera of `site` flashes now (`now`: the game's time of day), if a model stands there:
/// its own lamp through its script, or a light at its lens. True when one does.
pub fn trigger(site: &Site, now: f64) -> bool {
    let id = object_id(site, 0);
    let Some((lens, how)) = PLACED.lock().unwrap_or_else(|e| e.into_inner()).as_ref().and_then(|p| p.get(&id).copied()) else {
        return false;
    };
    flash(id, now);
    if how == Flash::Light {
        flash_light(lens);
    }
    true
}

/// The flash's colour: the red of the stock camera's own lamp (`Blitzer_Typ1.sco`'s
/// `[light_enh_2]`, 228 7 50).
pub const FLASH_COLOUR: [f32; 3] = [0.894, 0.027, 0.196];

/// How far over the limit (km/h) a camera counts `kmh`: after the tolerance (3 km/h up to
/// 100, 3 % above), in whole km/h; None within it. As the company's fines count it
/// (`career::over_limit`), so the camera flashes exactly when a fine follows.
pub fn over_limit(kmh: f32, limit: f32) -> Option<f32> {
    if !(kmh.is_finite() && limit > 0.0) {
        return None;
    }
    let tolerance = if kmh <= 100.0 { 3.0 } else { (kmh * 0.03).ceil() };
    let counted = (kmh - tolerance).floor();
    (counted > limit).then(|| counted - limit)
}

/// A place the player's bus has just passed: the camera (`along` of its lane), the point a
/// display measures at (`DISPLAY_MEASURE` before it), or a red-light camera's stop line - with
/// the lane it was passed on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Passed {
    pub site: Site,
    pub lane: usize,
}

/// Where the player's bus drove last: the lane, its direction of the road and how far along.
#[derive(Clone, Copy, Debug, Default)]
pub struct Watch {
    last: Option<(usize, Option<Group>, f32)>,
    /// The crossing whose red-light camera the bus passed, until it is on a road again (its
    /// lanes overlap where they begin, and the bus seen on one and then another is one pass).
    crossing: Option<((i32, i32), i64)>,
}

impl Watch {
    /// The bus stands at `pos` turned to `heading` now: the place it passed since the last
    /// look, if one. A place is passed when the bus goes over its point on a lane of its road's
    /// direction (any of them: changing lanes past a camera is no escape); a red-light camera
    /// when the bus comes onto a lane of its light from another road. Driving the other way, or
    /// first coming onto a road beyond the point, passes nothing.
    pub fn step(&mut self, net: &crate::traffic::Network, pos: DVec3, heading: f64) -> Option<Passed> {
        let Some((lane, s, _)) = net.lane_along(pos, heading, LaneKind::Street, 4.0, 45.0) else {
            self.last = None;
            return None;
        };
        let l = &net.lanes[lane];
        let g = group(l);
        if l.source != 2 {
            self.crossing = None;
        }
        let last = self.last.replace((lane, g, s));
        let (_, last_group, last_s) = last?;
        let site = site(l)?;
        let same = g.is_some() && g == last_group;
        let passed = match site.kind {
            Kind::Speed => same && last_s < site.along && site.along <= s,
            Kind::Display => {
                let at = site.along - DISPLAY_MEASURE;
                same && last_s < at && at <= s
            }
            // (onto the light's lanes from the road before it, near their start: its stop line)
            Kind::RedLight => {
                let crossing = g.map(|g| (g.tile, g.id));
                let first = !same && s < 8.0 && self.crossing != crossing;
                if first {
                    self.crossing = crossing;
                }
                first
            }
        };
        passed.then_some(Passed { site, lane })
    }

    /// Forget where the bus was (it was moved, another session).
    pub fn forget(&mut self) {
        *self = Watch::default();
    }
}

// --- the display's pole ---------------------------------------------------------------------

/// The pole the display hangs on, as files of the game's own (an `.sco` and its mesh, a plain
/// galvanised tube): (path under the OMSI folder, text). The scene keeps them in memory
/// (`omsi_cfg::vfs::put_memory_file`): nothing is written into the player's folders.
pub fn pole_files() -> Vec<(&'static str, String)> {
    let sco = "[friendlyname]\r\nOmsi-Hub GAA Mast\r\n\r\n[groups]\r\n1\r\nOmsi-Hub\r\n\r\n[mesh]\r\nGAA_Mast.obj\r\n";
    let (r, h, n) = (0.04f64, (DISPLAY_HEIGHT + 0.75) as f64, 12usize);
    let mut obj = String::from("# Omsi-Hub: the pole of a speed display (crates/omsi-sim/src/roadside.rs)\nmtllib GAA_Mast.mtl\nusemtl mast\n");
    // a ring at the foot and one at the top (y up), their normals, the cap's centre
    for k in 0..n {
        let a = std::f64::consts::TAU * k as f64 / n as f64;
        obj.push_str(&format!("v {:.4} 0 {:.4}\nv {:.4} {h:.4} {:.4}\nvn {:.4} 0 {:.4}\n", r * a.cos(), r * a.sin(), r * a.cos(), r * a.sin(), a.cos(), a.sin()));
    }
    obj.push_str(&format!("v 0 {h:.4} 0\nvn 0 1 0\n"));
    for k in 0..n {
        let (a, b) = (2 * k + 1, 2 * ((k + 1) % n) + 1);
        let (na, nb) = (k + 1, (k + 1) % n + 1);
        // (each face both ways round, its normal outward: whichever way the reader winds them,
        // the pole is seen from outside and lit as one)
        let tris = [[(a, na), (b, nb), (b + 1, nb)], [(a, na), (b + 1, nb), (a + 1, na)], [(a + 1, n + 1), (b + 1, n + 1), (2 * n + 1, n + 1)]];
        for t in tris {
            for order in [[0, 1, 2], [0, 2, 1]] {
                obj.push_str(&format!("f {}//{} {}//{} {}//{}\n", t[order[0]].0, t[order[0]].1, t[order[1]].0, t[order[1]].1, t[order[2]].0, t[order[2]].1));
            }
        }
    }
    let mtl = "newmtl mast\nKd 0.62 0.64 0.66\nd 1\n".to_string();
    // (a scenery object's meshes lie in its `model` folder)
    vec![
        ("Sceneryobjects\\Omsi-Hub\\Roadside\\GAA_Mast.sco", sco.to_string()),
        ("Sceneryobjects\\Omsi-Hub\\Roadside\\model\\GAA_Mast.obj", obj),
        ("Sceneryobjects\\Omsi-Hub\\Roadside\\model\\GAA_Mast.mtl", mtl),
    ]
}

#[cfg(test)]
mod tests;
