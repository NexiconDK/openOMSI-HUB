//! (NexiconDK, 1 November 2026) The speed cameras, red-light cameras and speed displays the
//! game puts up by the road, staged with the tile they stand in. Where they stand and which
//! model is `omsi_sim::roadside`'s; the models are the player's own add-ons in his OMSI folder
//! (never shipped with the game: their licences let a map carry them unchanged, not a game),
//! those he has. The display's pole is the game's own, kept in memory.

use super::*;
use omsi_sim::roadside as rs;

/// The `speed_cameras` setting as the world keeps it (`host::SceneSettings::roadside`): no
/// cameras, cameras measuring unseen, or standing by the road.
pub const OFF: u8 = 0;
pub const UNSEEN: u8 = 1;
pub const SEEN: u8 = 2;

/// How near another road's lane may come to a model's foot, beyond that lane's half width
/// (m): a camera is not put into a bus bay beside its road, or onto the next street.
const CLEAR: f64 = 0.8;

impl World {
    /// Which of the models (`omsi_sim::roadside::MODELS`) the player has installed: the
    /// object, its parts (the display's pole is the game's own) and what it needs besides (the
    /// display's font).
    pub fn roadside_installed(&self) -> Vec<bool> {
        rs::MODELS
            .iter()
            .map(|m| {
                std::iter::once(m.file)
                    .chain(m.parts.iter().copied().filter(|p| *p != rs::POLE))
                    .chain(m.needs.iter().copied())
                    .all(|f| omsi_cfg::vfs::is_file(&omsi_cfg::resolve_path(&self.root, f)))
            })
            .collect()
    }

    /// The add-ons the player lacks models of (each once, in the order of the models).
    pub fn roadside_missing(&self) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        for (m, have) in rs::MODELS.iter().zip(self.roadside_installed()) {
            if !have && !out.contains(&m.package) {
                out.push(m.package);
            }
        }
        out
    }

    /// The display's pole: its files in memory, where the OMSI folder would hold them.
    fn roadside_pole(&self) {
        if omsi_cfg::vfs::is_file(&omsi_cfg::resolve_path(&self.root, rs::POLE)) {
            return;
        }
        for (rel, text) in rs::pole_files() {
            omsi_cfg::vfs::put_memory_file(omsi_cfg::resolve_path(&self.root, rel), Arc::from(text.into_bytes()));
        }
    }

    /// The cameras and displays of the tile being staged, when the player wants them seen: at
    /// each place of its roads and its crossings' lights the model of it among those
    /// installed, beside the outermost lane of the road's direction (of the light's approach),
    /// turned to the traffic coming - unless another road's lane passes there.
    pub(super) fn stage_roadside(&self, out: &mut StagedTile, index: &MapIndex) {
        if self.settings.roadside() != SEEN {
            return;
        }
        let installed = self.roadside_installed();
        if !installed.iter().any(|b| *b) {
            return;
        }
        let left = self.global.left_hand_traffic;
        let (tx, ty) = (out.tx, out.ty);
        // the crossings' lanes as the tile's placing will lay them out (which program number
        // does not matter here, only which lanes a light holds)
        let mut crossing_lanes: Vec<Lane> = Vec::new();
        for o in out.objects.iter().filter(|o| o.map_object && !o.ot.sco.paths.is_empty()) {
            let Placement::Pose(p) = &o.place else { continue };
            let lit = traffic_light_program_enabled(&o.ot.sco, index.traffic_light_parents.contains(&o.id));
            crossing_lanes.extend(object_lanes(&o.ot.sco, p.pos, [p.heading(), 0.0, 0.0], lit.then_some(0), (tx, ty), o.id, &o.rules));
        }
        let roads = out.lanes.lock();
        let all: Vec<&Lane> = roads.iter().chain(crossing_lanes.iter()).collect();
        // each place by the outermost lane of its group - of every street lane of it, one closed
        // to cars (a bus lane) or too short for the place too: by the lane a camera has, it
        // stood in a bus lane beside it
        let mut sites: HashMap<rs::Group, rs::Site> = HashMap::new();
        for l in all.iter() {
            if let (Some(site), Some(g)) = (rs::site(l), rs::group(l)) {
                sites.entry(g).or_insert(site);
            }
        }
        let mut places: HashMap<rs::Group, (rs::Site, usize, f64)> = HashMap::new();
        for (i, l) in all.iter().enumerate() {
            if l.kind != LaneKind::Street || l.invisible {
                continue;
            }
            let Some(g) = rs::group(l) else { continue };
            let Some(&site) = sites.get(&g) else { continue };
            let outside = rs::outside(l, site.along.min(l.length()), left);
            let e = places.entry(g).or_insert((site, i, outside));
            if outside > e.2 {
                *e = (site, i, outside);
            }
        }
        let mut places: Vec<(rs::Group, (rs::Site, usize, f64))> = places.into_iter().collect();
        places.sort_by_key(|(g, _)| (g.tile, g.id, g.side));
        let mut placed: Vec<StagedObject> = Vec::new();
        for (_, (site, i, _)) in places {
            let Some(model) = rs::model_of(&site, &installed) else { continue };
            let (foot, heading) = rs::pose(all[i], &site, model, left);
            let ground = foot - DVec3::Z * model.height as f64;
            if !clear_of_other_roads(&all, i, ground) {
                continue;
            }
            let Some(ot) = self.object_type(model.file) else { continue };
            let rot = object_rotation([heading, 0.0, 0.0]);
            let id = rs::object_id(&site, 0);
            // (the display's strings: what it shows, the limit its faces judge by, faces on)
            let extra = match site.kind {
                rs::Kind::Display => vec![String::new(), format!("{:.0}", site.limit), "1".to_string()],
                _ => Vec::new(),
            };
            placed.push(standing(ot, id, foot, rot, extra));
            for (k, part) in model.parts.iter().enumerate() {
                if *part == rs::POLE {
                    self.roadside_pole();
                }
                let Some(ot) = self.object_type(part) else { continue };
                let at = if *part == rs::POLE { rs::pole_foot(foot, heading) } else { foot };
                placed.push(standing(ot, rs::object_id(&site, k + 1), at, rot, Vec::new()));
            }
            rs::place(id, rs::lens(model, foot, heading), model.flash);
        }
        drop(roads);
        out.objects.extend(placed);
    }
}

/// A model standing by the road: an object of no map (its id says what it is, see
/// `omsi_sim::roadside::object_id`).
fn standing(ot: Arc<ObjectType>, id: i64, pos: DVec3, rot: Mat4, extra: Vec<String>) -> StagedObject {
    StagedObject {
        ot,
        id,
        place: Placement::Pose(Pose { pos, rot }),
        rules: Vec::new(),
        extra,
        lamp_parent: None,
        parked: false,
        map_object: false,
        instance: 0,
        key: id,
    }
}

/// No street lane but its own (`own`, an index into `lanes`) passes within its half width and
/// [`CLEAR`] of `foot` - another road's, the other direction's, a bus lane or a turning lane
/// beside it - on the level (a road on a bridge over it, or in a tunnel under it, does not
/// count).
fn clear_of_other_roads(lanes: &[&Lane], own: usize, foot: DVec3) -> bool {
    lanes.iter().enumerate().filter(|(k, l)| *k != own && l.kind == LaneKind::Street).map(|(_, l)| l).all(|l| {
        let keep = l.width.max(2.5) as f64 * 0.5 + CLEAR;
        l.points.windows(2).all(|w| {
            let (a, b) = (w[0], w[1]);
            let ab = (b - a).truncate();
            let t = ((foot - a).truncate().dot(ab) / ab.length_squared().max(1e-9)).clamp(0.0, 1.0);
            let q = a + (b - a) * t;
            (q.truncate() - foot.truncate()).length() > keep || (q.z - foot.z).abs() > 4.0
        })
    })
}
