//! Omsi-Hub: the map's own picture under the navigator and the city map (openOMSI PRs #2782 and
//! #2929, the `nav_map` and `city_map` settings): the ground by what it is painted with, water,
//! woods and trees, every paved surface as it lies with its dark rim and its road markings, the
//! tracks, and the houses - flat footprints, or standing in 3D with their shadows. Read off the
//! map once (`omsi_scene::mapgen`, kept in the user's cache), built around a place on a worker
//! (a few hundred thousand triangles took the game tens of milliseconds) and kept until the
//! bus or the city map has gone far. The roads of the network are drawn only where the map's
//! paving does not cover them.

use super::*;
use omsi_mapdraw::style::Palette;
use omsi_mapdraw::{GroundLook, MapData};
use omsi_ui::Vertex;
use std::sync::Arc;

/// How the navigator's map is drawn (`nav_map`), and the city map's (`city_map`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MapStyle {
    /// Omsi-Hub's own roads, as before.
    #[default]
    Classic,
    /// The map's own picture seen from above, the houses as footprints.
    Flat,
    /// The same tilted, the houses standing in 3D with their shadows.
    ThreeD,
}

impl MapStyle {
    pub fn from_setting(v: &str) -> MapStyle {
        match omsi_launcher_lib::nav_map_style(v) {
            "flat" => MapStyle::Flat,
            "3d" => MapStyle::ThreeD,
            _ => MapStyle::Classic,
        }
    }

    pub fn new_look(self) -> bool {
        self != MapStyle::Classic
    }
}

/// The triangles of house models the small map's ground may hold (it is drawn often, and
/// built again as the bus drives).
pub(super) const PANEL_MODELS: usize = 30_000;
/// The small map's ground reaches this far round the bus (m), and is built again when the bus
/// has gone this far from where it was built.
pub(super) const PANEL_REACH: f64 = 900.0;
pub(super) const PANEL_REBUILD: f64 = 220.0;
/// The GPU buffers of the small map's ground (its four parts), and of the city map's: apart
/// from every other buffer of the two (they share one `Gpu`).
pub(super) const PANEL_BUFFERS: usize = 11;
pub(super) const CITY_BUFFERS: usize = 15;

/// A ground built around `anchor`: the ground, then the houses' shadows, their standing walls
/// and roofs, and their flat roofs (each relative to the anchor).
pub(crate) struct Built {
    pub anchor: DVec2,
    pub verts: [Vec<Vertex>; 4],
}

/// What a build reads: the network's carriageways (each with whether the map's paving covers
/// it), the map's layers, and the network for the tracks laid with objects.
#[derive(Clone)]
pub(crate) struct GroundParts {
    pub roads: Arc<Vec<(MapRoad, bool)>>,
    pub md: Arc<MapData>,
    pub net: Option<Arc<Network>>,
}

/// Everything within `reach` of `anchor` (after openOMSI's `navigator::build`). `paved`: the map's
/// paving is the roads (else every carriageway is a band, as from far); `houses`: build them
/// too, with at most so many triangles of their models.
pub(crate) fn build(parts: &GroundParts, anchor: DVec2, reach: f64, pal: &Palette, look: GroundLook, paved: bool, houses: Option<usize>) -> Built {
    let mut p = Painter::new();
    let md = parts.md.as_ref();
    md.build_land(&mut p, anchor, anchor, reach, pal, look);
    let rel = |q: DVec3| Vec3::new((q.x - anchor.x) as f32, (q.y - anchor.y) as f32, 0.0);
    // the network's carriageways where the map has no paving of its own for them (else the
    // paving is the road, and a path across a junction or a median is no road)
    let tol = if paved { 0.12 } else { (reach / 4000.0) as f32 };
    let near: Vec<(&MapRoad, Vec<Vec3>)> = parts
        .roads
        .iter()
        .filter(|(l, covered)| !(paved && *covered) && l.points.iter().any(|q| (q.truncate() - anchor).length() < reach))
        .map(|(l, _)| (l, simplify(&l.points.iter().map(|q| rel(*q)).collect::<Vec<_>>(), tol)))
        .collect();
    for (l, pts) in &near {
        p.ribbon(pts, l.width + 1.0, if l.main { 3.4 } else { 2.8 }, pal.road_edge, true);
    }
    if paved {
        md.build_paved(&mut p, anchor, anchor, reach, pal, look);
    }
    for (l, pts) in &near {
        p.ribbon(pts, l.width, if l.main { 2.2 } else { 1.6 }, pal.road, true);
    }
    if let Some(net) = parts.net.as_deref() {
        build_switches(&mut p, net, anchor, reach, pal);
    }
    md.build_lines(&mut p, anchor, anchor, reach, pal, look);
    let (mut sh, mut so, mut fl) = (Painter::new(), Painter::new(), Painter::new());
    if let Some(models) = houses {
        md.build_houses(&mut sh, &mut so, &mut fl, anchor, anchor, reach, models, pal);
    }
    Built { anchor, verts: [p.verts, sh.verts, so.verts, fl.verts] }
}

/// The tracks the map lays with objects rather than splines (points and crossings): the rail
/// lanes of `net` that come from objects - a tram's beside a street, else the railway's.
fn build_switches(p: &mut Painter, net: &Network, anchor: DVec2, radius: f64, pal: &Palette) {
    let rel = |q: DVec3| Vec3::new((q.x - anchor.x) as f32, (q.y - anchor.y) as f32, 0.0);
    for l in net.lanes.iter().filter(|l| l.kind == LaneKind::Rail && l.source != 1 && l.points.len() >= 2) {
        if (l.start().truncate() - anchor).length() > radius {
            continue;
        }
        let pts: Vec<Vec3> = l.points.iter().map(|q| rel(*q)).collect();
        let mid = l.at(l.length() * 0.5).0;
        let street = net.nearest_lane_near(mid, LaneKind::Street).is_some_and(|s| s.2 < 4.0);
        if street {
            p.ribbon(&pts, 0.9, 1.3, pal.tram, false);
        } else {
            p.ribbon(&pts, 3.6, 3.4, pal.rail, false);
        }
    }
}

/// A build running on a worker: what it was asked for and where its result comes.
pub(crate) struct Job {
    pub key: u64,
    pub lanes: usize,
    rx: std::sync::mpsc::Receiver<Built>,
}

impl Job {
    pub fn start(key: u64, lanes: usize, f: impl FnOnce() -> Built + Send + 'static) -> Job {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("navigator ground".into())
            .spawn(move || {
                let _ = tx.send(f());
            })
            .ok();
        Job { key, lanes, rx }
    }

    /// The result, once there (`Err(())`: the worker is gone).
    pub fn poll(&self) -> Result<Option<Built>, ()> {
        match self.rx.try_recv() {
            Ok(b) => Ok(Some(b)),
            Err(std::sync::mpsc::TryRecvError::Empty) => Ok(None),
            Err(_) => Err(()),
        }
    }
}

/// The map's own picture as the navigator holds it: the layers once read, the carriageways
/// with whether the paving covers each, and the ground built for the small map and the city map.
#[derive(Default)]
pub(crate) struct MapLook {
    pub nav: MapStyle,
    pub city: MapStyle,
    pub data: Option<Arc<MapData>>,
    pub layers_rx: Option<std::sync::mpsc::Receiver<MapData>>,
    /// The world, kept to read the layers when a setting asks for them later.
    pub world: Option<Arc<crate::scene::World>>,
    pub data_version: u64,
    /// The carriageways and the key (network and layers versions) they were worked out for.
    pub roads: Option<(u64, Arc<Vec<(MapRoad, bool)>>)>,
    pub roads_rx: Option<(u64, std::sync::mpsc::Receiver<Arc<Vec<(MapRoad, bool)>>>)>,
    /// The small map's ground: built (what it was built for: the look's key, the lanes seen,
    /// when) and the build under way.
    pub panel: Option<PanelGround>,
    pub panel_job: Option<Job>,
    /// The city map's ground: the same.
    pub city_ground: Option<PanelGround>,
    pub city_job: Option<Job>,
}

/// What the ground on the GPU was built for, and how many vertices each of its parts has.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PanelGround {
    pub key: u64,
    pub anchor: DVec2,
    pub lanes: usize,
    pub counts: [u32; 4],
    pub built_at: f32,
    pub reach: f64,
}

impl MapLook {
    /// The layers are wanted: one of the maps shows the map's own picture.
    pub fn wanted(&self) -> bool {
        self.nav.new_look() || self.city.new_look()
    }

    /// Read the map's layers on a worker (from the cache in a moment, the first visit of a map
    /// some seconds), when one of the maps wants them and they are not there or coming.
    pub fn start(&mut self) {
        if !self.wanted() || self.data.is_some() || self.layers_rx.is_some() {
            return;
        }
        let Some(w) = self.world.clone() else { return };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("navigator layers".into())
            .spawn(move || {
                let layers = omsi_scene::threads::background_pool().install(|| w.map_layers());
                let _ = tx.send(MapData::new(Arc::new(layers)));
            })
            .ok();
        self.layers_rx = Some(rx);
    }

    /// The layers arrived from their worker.
    pub fn poll(&mut self) {
        if let Some(rx) = self.layers_rx.as_ref() {
            match rx.try_recv() {
                Ok(md) => {
                    self.data = Some(Arc::new(md));
                    self.data_version += 1;
                    self.layers_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => self.layers_rx = None,
            }
        }
        if let Some((k, rx)) = self.roads_rx.as_ref() {
            match rx.try_recv() {
                Ok(list) => {
                    self.roads = Some((*k, list));
                    self.roads_rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                Err(_) => self.roads_rx = None,
            }
        }
    }

    /// The carriageways of `net` with whether the map's paving covers each, worked out on a
    /// worker once per version of the network and the layers (the whole map's grouping took
    /// the better part of a second); until then the last list, or none.
    pub fn road_list(&mut self, net: Arc<Network>, version: u64) -> Arc<Vec<(MapRoad, bool)>> {
        let Some(md) = self.data.clone() else { return Arc::default() };
        let key = (version << 32) ^ (net.lanes.len() as u64) ^ (self.data_version << 56);
        let fresh = self.roads.as_ref().is_some_and(|(k, _)| *k == key);
        if !fresh && self.roads_rx.as_ref().is_none_or(|(k, _)| *k != key) {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::Builder::new()
                .name("navigator roads".into())
                .spawn(move || {
                    let list: Vec<(MapRoad, bool)> = road_geometry(&net).into_iter().map(|l| {
                        let c = md.covers(&l.points);
                        (l, c)
                    }).collect();
                    let _ = tx.send(Arc::new(list));
                })
                .ok();
            self.roads_rx = Some((key, rx));
        }
        self.roads.as_ref().map(|c| c.1.clone()).unwrap_or_default()
    }
}

/// The key a ground was built for: the look (style, light) and the layers' version.
pub(crate) fn look_key(style: MapStyle, light: bool, data_version: u64) -> u64 {
    (style as u64) | ((light as u64) << 2) | (data_version << 8)
}

impl Navigator {
    /// The small map's ground round the bus (`bus`): a build that has come from its worker (to
    /// go on the GPU), and a new one started when the bus has gone far, the network or the
    /// look changed. The picture keeps the old part meanwhile.
    pub(super) fn panel_ground(&mut self, bus: DVec2) -> Option<Built> {
        let md = self.look.data.clone()?;
        let net = self.global.clone();
        let lanes_now = net.as_ref().map_or(0, |n| n.lanes.len());
        let roads = net.clone().map(|n| self.look.road_list(n, self.global_version)).unwrap_or_default();
        // (the carriageways' list counts: built once more when it has come)
        let key = look_key(self.look.nav, self.light, self.look.data_version) ^ ((self.look.roads.is_some() as u64) << 60);
        let mut built = None;
        if let Some(job) = self.look.panel_job.as_ref() {
            match job.poll() {
                Ok(Some(b)) => {
                    let counts = b.verts.each_ref().map(|v| v.len() as u32);
                    self.look.panel = Some(PanelGround { key: job.key, anchor: b.anchor, lanes: job.lanes, counts, built_at: self.time, reach: PANEL_REACH });
                    built = Some(b);
                    self.look.panel_job = None;
                }
                Ok(None) => {}
                Err(()) => self.look.panel_job = None,
            }
        }
        let rebuild = match &self.look.panel {
            None => true,
            Some(g) => g.key != key || (g.anchor - bus).length() > PANEL_REBUILD || (g.lanes != lanes_now && self.time - g.built_at > 1.5),
        };
        if rebuild && self.look.panel_job.is_none() && built.is_none() {
            let parts = GroundParts { roads, md, net };
            let pal = *omsi_mapdraw::style::palette(self.light);
            let houses = (self.look.nav == MapStyle::ThreeD || self.look.nav == MapStyle::Flat).then_some(PANEL_MODELS);
            let look = GroundLook { surfaces: true, trees: true, dashes: true };
            self.look.panel_job = Some(Job::start(key, lanes_now, move || build(&parts, bus, PANEL_REACH, &pal, look, true, houses)));
        }
        built
    }
}

impl Navigator {
    /// The city map's ground round `centre`, seen at `mpp` metres a pixel across a window of
    /// `half` metres (half its larger side): its details, paving and houses come and go with
    /// the scale as openOMSI's city map has them. A build that has come from its worker, and a
    /// new one started when the view has gone far or changed its scale much.
    pub(super) fn city_ground_step(&mut self, centre: DVec2, half: f64, mpp: f64) -> Option<Built> {
        let md = self.look.data.clone()?;
        let net = self.global.clone();
        let lanes_now = net.as_ref().map_or(0, |n| n.lanes.len());
        let roads = net.clone().map(|n| self.look.road_list(n, self.global_version)).unwrap_or_default();
        let look = GroundLook { surfaces: mpp < 0.7, trees: mpp < 0.7, dashes: mpp < 3.5 };
        let (paved, houses) = (mpp < 2.2, (mpp < 6.0).then_some(omsi_mapdraw::MODEL_TRIANGLES));
        let detail = (look.surfaces as u64) | ((look.dashes as u64) << 1) | ((paved as u64) << 2) | ((houses.is_some() as u64) << 3);
        let key = look_key(self.look.city, self.light, self.look.data_version) ^ ((self.look.roads.is_some() as u64) << 60) ^ (detail << 4);
        let mut built = None;
        if let Some(job) = self.look.city_job.as_ref() {
            match job.poll() {
                Ok(Some(b)) => {
                    let counts = b.verts.each_ref().map(|v| v.len() as u32);
                    let reach = (half * 2.0).max(300.0);
                    self.look.city_ground = Some(PanelGround { key: job.key, anchor: b.anchor, lanes: job.lanes, counts, built_at: self.time, reach });
                    built = Some(b);
                    self.look.city_job = None;
                }
                Ok(None) => {}
                Err(()) => self.look.city_job = None,
            }
        }
        // (built for twice the view round its middle: again once the view has gone a good
        // part of that, or is much larger)
        let reach = (half * 2.0).max(300.0);
        let rebuild = match &self.look.city_ground {
            None => true,
            Some(g) => g.key != key || (g.anchor - centre).length() > g.reach * 0.45 || half > g.reach * 0.8 || (g.lanes != lanes_now && self.time - g.built_at > 1.5),
        };
        if rebuild && self.look.city_job.is_none() && built.is_none() {
            let parts = GroundParts { roads, md, net };
            let pal = *omsi_mapdraw::style::palette(self.light);
            self.look.city_job = Some(Job::start(key, lanes_now, move || build(&parts, centre, reach, &pal, look, paved, houses)));
            if let Some(g) = self.look.city_ground.as_mut() {
                g.reach = reach;
            }
        }
        built
    }
}
