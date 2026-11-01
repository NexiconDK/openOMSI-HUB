//! (NexiconDK, 1 November 2026) The moving parts of a wheelchair and a pram
//! (`omsi_content::human::Mobility`), which their models have no bones for: a figure and its
//! chair or pram are one rigid mesh, or the figure and a pram mesh of its own.
//!
//! The wheels are found in the mesh itself: a wheel is a piece of it (its triangles joined) that
//! is round seen from the side, thin across and touches the ground; the pieces of one wheel and
//! those about the same axis (the other side's wheel, a hub, a hand rim) turn together. A
//! wheelchair's arms are the figure's vertices out at its sides from its shoulders down - out
//! beyond its body, where the gap between arm and body lies at each height - and they swing in
//! a push on the hand rims as it rolls. A pram, and its wheels, go on a frame of their own
//! (`GLIDE`): it glides on with the person rather than bobbing with her hips as she walks.
//!
//! The extra bone slots are filled by [`mobility_bones`]; without it they stay identity, the
//! rest pose.

use super::*;
use omsi_content::human::Mobility;

/// The slots of the moving parts, after the engine's bones and the feet ([`SLOTS`]): the pram's
/// frame, two axles, and a wheelchair's arms - the whole arm about the shoulder, the forearm
/// about the elbow besides.
pub const GLIDE: usize = 17;
pub const AXLE: [usize; 2] = [18, 19];
pub const ARM: [usize; 2] = [20, 21];
pub const FOREARM: [usize; 2] = [22, 23];

/// How far a wheelchair rolls on one push of the rims (m), how far the arm swings about the
/// shoulder forwards and back of where it rests (degrees: back less, the hand by the hip, not
/// behind the backrest), and how far the forearm bends up at the elbow on the way back (the
/// hand off the rim).
const STROKE: f32 = 1.1;
/// A seated figure's arms lie further out from its middle than this (m) - its thighs and
/// hips within - and not this far forwards of it (its knees and feet are).
const ARM_INNER: f32 = 0.17;
const ARM_AHEAD: f32 = 0.3;
const SHOULDER_AHEAD: f32 = 18.0;
const SHOULDER_BACK: f32 = 10.0;
const ELBOW_LIFT: f32 = 14.0;

/// An axle: where it is (the mesh frame: x right, y forwards, z up; it runs along x) and the
/// radius of its largest wheel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axle {
    pub centre: Vec3,
    pub radius: f32,
}

/// The joints a wheelchair's arm turns about (the mesh frame).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArmJoints {
    pub shoulder: Vec3,
    pub elbow: Vec3,
}

/// What of a wheelchair or a pram moves, as found in the model.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MobilityRig {
    pub axles: [Option<Axle>; 2],
    /// The slot the wheels go round on: the pram's frame, or the figure's torso.
    pub base: usize,
    /// A wheelchair's arms (left, right).
    pub arms: Option<[ArmJoints; 2]>,
}

/// The pieces of a mesh: per vertex the piece it belongs to (its triangles joined, and
/// vertices at one place - a seam of the texture - together).
fn pieces(data: &MeshData) -> Vec<usize> {
    let n = data.positions.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], i: usize) -> usize {
        let mut r = i;
        while p[r] != r {
            r = p[r];
        }
        let mut j = i;
        while p[j] != r {
            let k = p[j];
            p[j] = r;
            j = k;
        }
        r
    }
    let join = |p: &mut Vec<usize>, a: usize, b: usize| {
        let (ra, rb) = (find(p, a), find(p, b));
        if ra != rb {
            p[ra] = rb;
        }
    };
    for t in data.indices.chunks_exact(3) {
        let (a, b, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
        if a < n && b < n && c < n {
            join(&mut parent, a, b);
            join(&mut parent, b, c);
        }
    }
    let mut at: std::collections::HashMap<(i32, i32, i32), usize> = std::collections::HashMap::new();
    for (i, v) in data.positions.iter().enumerate() {
        let k = ((v.x * 2000.0).round() as i32, (v.y * 2000.0).round() as i32, (v.z * 2000.0).round() as i32);
        match at.get(&k) {
            Some(&j) => join(&mut parent, i, j),
            None => {
                at.insert(k, i);
            }
        }
    }
    (0..n).map(|i| find(&mut parent, i)).collect()
}

/// Per vertex the material slot of the triangles it is a corner of.
fn materials(data: &MeshData) -> Vec<u32> {
    let mut out = vec![u32::MAX; data.positions.len()];
    for &(first, count, slot) in &data.ranges {
        for &i in data.indices.iter().skip(first as usize).take(count as usize) {
            if let Some(m) = out.get_mut(i as usize) {
                *m = slot;
            }
        }
    }
    out
}

/// A piece's extent (min, max) and its vertices.
struct Piece {
    lo: Vec3,
    hi: Vec3,
    verts: Vec<usize>,
}

fn gather(data: &MeshData, of: &[usize]) -> Vec<Piece> {
    let mut by: std::collections::HashMap<usize, Piece> = std::collections::HashMap::new();
    for (i, &p) in of.iter().enumerate() {
        let v = data.positions[i];
        let e = by.entry(p).or_insert(Piece { lo: v, hi: v, verts: Vec::new() });
        e.lo = e.lo.min(v);
        e.hi = e.hi.max(v);
        e.verts.push(i);
    }
    by.into_values().collect()
}

/// The wheels among the pieces of `meshes` (those the filter allows): round seen from the
/// side, thin across, touching the ground (`ground`, the model's lowest point) - and the
/// pieces about the same axis inside them (hubs, spokes, hand rims). Per axle (largest
/// first): its centre and radius, and the (mesh, vertex) list of what turns about it.
fn find_axles(meshes: &[HumanMesh], ground: f32, allow: &dyn Fn(usize, u32) -> bool) -> Vec<(Axle, Vec<(usize, usize)>)> {
    // (per mesh its pieces, each with its material)
    let mut all: Vec<(usize, Piece, u32)> = Vec::new();
    for (k, m) in meshes.iter().enumerate() {
        let mats = materials(&m.data);
        for p in gather(&m.data, &pieces(&m.data)) {
            let mat = mats.get(p.verts[0]).copied().unwrap_or(u32::MAX);
            if allow(k, mat) {
                all.push((k, p, mat));
            }
        }
    }
    let round = |p: &Piece| {
        let (sy, sz, sx) = (p.hi.y - p.lo.y, p.hi.z - p.lo.z, p.hi.x - p.lo.x);
        p.verts.len() >= 12 && sy >= 0.06 && sz >= 0.06 && (0.75..=1.33).contains(&(sy / sz)) && sx <= 0.5 * sy.max(sz)
    };
    let mut axles: Vec<(Axle, Vec<(usize, usize)>)> = Vec::new();
    for (k, p, _) in all.iter().filter(|(_, p, _)| round(p) && p.lo.z <= ground + 0.06) {
        let centre = Vec3::new(0.0, (p.lo.y + p.hi.y) * 0.5, (p.lo.z + p.hi.z) * 0.5);
        let radius = (p.hi.y - p.lo.y).max(p.hi.z - p.lo.z) * 0.5;
        let verts = p.verts.iter().map(|&v| (*k, v));
        match axles.iter_mut().find(|(a, _)| (a.centre - centre).length() < 0.05) {
            Some((a, list)) => {
                a.radius = a.radius.max(radius);
                list.extend(verts);
            }
            None => axles.push((Axle { centre, radius }, verts.collect())),
        }
    }
    // (inside a wheel, about its axis: a hub, spokes, a hand rim that does not reach the ground)
    for (k, p, _) in &all {
        let c = Vec2::new((p.lo.y + p.hi.y) * 0.5, (p.lo.z + p.hi.z) * 0.5);
        let r = (p.hi.y - p.lo.y).max(p.hi.z - p.lo.z) * 0.5;
        if let Some((_, list)) = axles.iter_mut().find(|(a, list)| (Vec2::new(a.centre.y, a.centre.z) - c).length() < 0.04 && r <= a.radius * 1.05 && !list.contains(&(*k, p.verts[0]))) {
            list.extend(p.verts.iter().map(|&v| (*k, v)));
        }
    }
    axles.sort_by(|a, b| b.0.radius.total_cmp(&a.0.radius));
    axles
}

/// Give the vertices of `list` to `slot` alone.
fn assign(meshes: &mut [HumanMesh], list: &[(usize, usize)], slot: usize) {
    for &(k, v) in list {
        if let Some(inf) = meshes.get_mut(k).and_then(|m| m.skin.get_mut(v)) {
            *inf = Influence { n: 1, slot: [slot as u8, 0, 0, 0], weight: [1.0, 0.0, 0.0, 0.0] };
        }
    }
}

/// The figure's lowest point (the ground it stands on).
fn ground(meshes: &[HumanMesh]) -> f32 {
    meshes.iter().flat_map(|m| m.data.positions.iter().map(|p| p.z)).fold(f32::MAX, f32::min)
}

/// A wheelchair figure: its wheels on their axles (turning with the torso), and its arms.
pub(super) fn rig_wheelchair(meshes: &mut [HumanMesh]) -> Option<MobilityRig> {
    let g = ground(meshes);
    let axles = find_axles(meshes, g, &|_, _| true);
    let mut rig = MobilityRig { base: MAIN, ..Default::default() };
    // (the chair: every vertex of its wheels' materials - its frame, its push handles - no arm)
    let per: Vec<Vec<u32>> = meshes.iter().map(|m| materials(&m.data)).collect();
    let mut chair: Vec<(usize, u32)> = axles.iter().flat_map(|a| a.1.iter()).map(|&(k, v)| (k, per[k][v])).collect();
    chair.sort_unstable();
    chair.dedup();
    for (k, (a, list)) in axles.iter().take(2).enumerate() {
        rig.axles[k] = Some(*a);
        assign(meshes, list, AXLE[k]);
    }
    let wheels: std::collections::HashSet<(usize, usize)> = axles.iter().flat_map(|a| a.1.iter().copied()).collect();
    // (a mesh of one material, chair and figure alike: no telling them apart by it - its
    // wheels at least)
    let apart: Vec<bool> = per.iter().enumerate().map(|(k, list)| list.iter().any(|m| !chair.contains(&(k, *m)))).collect();
    rig.arms = arms(meshes, &|k, v| wheels.contains(&(k, v)) || (apart[k] && chair.contains(&(k, per[k][v]))));
    (rig.axles[0].is_some() || rig.arms.is_some()).then_some(rig)
}

/// A wheelchair figure's arms (not its chair, `chair`): per side the vertices out beyond its body from the shoulder
/// down (the gap between arm and body measured at each height), on the arm slots - the upper
/// arm fading in from the shoulder, the forearm from the elbow. Their joints.
fn arms(meshes: &mut [HumanMesh], chair: &dyn Fn(usize, usize) -> bool) -> Option<[ArmJoints; 2]> {
    let body: Vec<(usize, usize, Vec3)> = meshes
        .iter()
        .enumerate()
        .flat_map(|(k, m)| m.data.positions.iter().enumerate().map(move |(v, p)| (k, v, *p)))
        .filter(|(k, v, _)| !chair(*k, *v))
        .collect();
    let top = body.iter().map(|b| b.2.z).fold(f32::MIN, f32::max);
    let bottom = body.iter().map(|b| b.2.z).fold(f32::MAX, f32::min);
    let h = top - bottom;
    if h < 0.8 {
        return None;
    }
    // per side and band of height, where the arm begins: the widest gap in what lies out there
    // between a third of the shoulders' width and the outside
    let band = 0.05f32;
    let (lo_z, hi_z) = (bottom + 0.30 * h, bottom + 0.78 * h);
    let bands = ((hi_z - lo_z) / band).ceil() as usize;
    let mut out = [ArmJoints { shoulder: Vec3::ZERO, elbow: Vec3::ZERO }; 2];
    let mut cut: [Vec<Option<f32>>; 2] = [vec![None; bands], vec![None; bands]];
    for side in 0..2 {
        for (b, c) in cut[side].iter_mut().enumerate() {
            let z0 = lo_z + b as f32 * band;
            let mut xs: Vec<f32> = body.iter().filter(|e| e.2.z >= z0 && e.2.z < z0 + band && e.2.x * SIDE[side] > ARM_INNER && e.2.y < ARM_AHEAD).map(|e| e.2.x.abs()).collect();
            xs.sort_by(f32::total_cmp);
            let outer = xs.last().copied().unwrap_or(0.0);
            // (the widest gap of 1 cm or more; an arm lies 3 cm out at least)
            let mut best: Option<(f32, f32)> = None;
            for w in xs.windows(2) {
                let gap = w[1] - w[0];
                if gap >= 0.01 && outer - w[1] >= 0.03 && best.is_none_or(|b| gap > b.0) {
                    best = Some((gap, (w[0] + w[1]) * 0.5));
                }
            }
            *c = best.map(|b| b.1);
        }
        // (where none is, the next band's - the arm against the body)
        let known: Vec<(usize, f32)> = cut[side].iter().enumerate().filter_map(|(b, c)| c.map(|c| (b, c))).collect();
        if known.len() < 3 {
            return None;
        }
        for b in 0..bands {
            if cut[side][b].is_none() {
                cut[side][b] = known.iter().min_by_key(|(k, _)| k.abs_diff(b)).map(|k| k.1);
            }
        }
    }
    let cut_at = |side: usize, z: f32| -> f32 {
        let b = (((z - lo_z) / band).floor().max(0.0) as usize).min(bands - 1);
        cut[side][b].unwrap_or(0.2)
    };
    // the joints: the shoulder where the arm meets the body at the top, the elbow halfway down
    // to the lowest point of the hand
    for side in 0..2 {
        let arm: Vec<Vec3> = body.iter().filter(|e| e.2.z >= lo_z && e.2.z < hi_z && e.2.y < ARM_AHEAD && e.2.x * SIDE[side] > cut_at(side, e.2.z)).map(|e| e.2).collect();
        if arm.len() < 30 {
            return None;
        }
        let hand_z = arm.iter().map(|p| p.z).fold(f32::MAX, f32::min);
        let mean_y = arm.iter().map(|p| p.y).sum::<f32>() / arm.len() as f32;
        let mean_x = arm.iter().map(|p| p.x.abs()).sum::<f32>() / arm.len() as f32;
        let shoulder = Vec3::new(SIDE[side] * (cut_at(side, hi_z - band) + 0.02), mean_y, hi_z);
        let elbow_z = hand_z + (shoulder.z - hand_z) * 0.52;
        let at_elbow: Vec<Vec3> = arm.iter().copied().filter(|p| (p.z - elbow_z).abs() < 0.05).collect();
        let ey = if at_elbow.is_empty() { mean_y } else { at_elbow.iter().map(|p| p.y).sum::<f32>() / at_elbow.len() as f32 };
        out[side] = ArmJoints { shoulder, elbow: Vec3::new(SIDE[side] * mean_x, ey, elbow_z) };
    }
    // the weights: out beyond the cut (softly, over 1.5 cm), the upper arm fading in below the
    // shoulder, the forearm below the elbow
    let smooth = |a: f32, b: f32, x: f32| {
        let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    for (k, v, p) in body {
        let side = usize::from(p.x > 0.0);
        if p.z >= hi_z || p.z < lo_z - 0.1 || p.y >= ARM_AHEAD {
            continue;
        }
        let out_w = smooth(0.0, 0.015, p.x.abs() - cut_at(side, p.z));
        let arm_w = out_w * smooth(out[side].shoulder.z, out[side].shoulder.z - 0.10 * h, p.z);
        if arm_w < 0.02 {
            continue;
        }
        let fore = smooth(out[side].elbow.z + 0.04, out[side].elbow.z - 0.04, p.z);
        let Some(inf) = meshes.get_mut(k).and_then(|m| m.skin.get_mut(v)) else { continue };
        // (what was the torso's, the head's: the rest beside the arm)
        let rest = 1.0 - arm_w;
        let mut list: Vec<(u8, f32)> = (0..inf.n as usize).map(|i| (inf.slot[i], inf.weight[i] * rest)).collect();
        list.push((ARM[side] as u8, arm_w * (1.0 - fore)));
        list.push((FOREARM[side] as u8, arm_w * fore));
        list.retain(|e| e.1 > 1e-4);
        list.sort_by(|a, b| b.1.total_cmp(&a.1));
        list.truncate(4);
        let total: f32 = list.iter().map(|e| e.1).sum();
        let mut new = Influence { n: list.len() as u8, ..Default::default() };
        for (i, e) in list.iter().enumerate() {
            new.slot[i] = e.0;
            new.weight[i] = e.1 / total.max(1e-6);
        }
        *inf = new;
    }
    Some(out)
}

/// A pram: its own mesh (`pram`, an index of `meshes`), or - a pram that is part of the
/// figure - the pieces of the material its wheels are of, on the glide frame; its wheels on
/// their axles.
pub(super) fn rig_pram(meshes: &mut [HumanMesh], pram: Option<usize>) -> Option<MobilityRig> {
    let g = ground(meshes);
    let axles = match pram {
        Some(p) => find_axles(meshes, g, &|k, _| k == p),
        None => find_axles(meshes, g, &|_, _| true),
    };
    // (what glides: the pram's mesh, else every vertex of its wheels' materials)
    let mut glide: Vec<(usize, usize)> = Vec::new();
    match pram {
        Some(p) => glide.extend((0..meshes.get(p).map_or(0, |m| m.data.positions.len())).map(|v| (p, v))),
        None => {
            let per: Vec<Vec<u32>> = meshes.iter().map(|m| materials(&m.data)).collect();
            let mut mats: Vec<(usize, u32)> = axles.iter().flat_map(|a| a.1.iter()).map(|&(k, v)| (k, per[k][v])).collect();
            mats.sort_unstable();
            mats.dedup();
            for (k, list) in per.iter().enumerate() {
                glide.extend(list.iter().enumerate().filter(|(_, m)| mats.contains(&(k, **m))).map(|(v, _)| (k, v)));
            }
        }
    }
    if glide.is_empty() {
        return None;
    }
    assign(meshes, &glide, GLIDE);
    let mut rig = MobilityRig { base: GLIDE, ..Default::default() };
    for (k, (a, list)) in axles.iter().take(2).enumerate() {
        rig.axles[k] = Some(*a);
        assign(meshes, list, AXLE[k]);
    }
    Some(rig)
}

/// How a wheelchair or a pram moves now: how far it has rolled (m), where its push is (the
/// strokes made, a fraction into the next) and how much the arms push (0 standing still, 1
/// rolling).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Rolling {
    pub rolled: f32,
    pub push: f32,
    pub effort: f32,
}

impl Rolling {
    /// On by `speed` (m/s) for `dt` s: the wheels on, the push on with them, the arms easing
    /// to the push or to rest.
    pub fn advance(&mut self, speed: f32, dt: f32) {
        let d = speed.max(0.0) * dt;
        self.rolled = (self.rolled + d) % 1000.0;
        self.push = (self.push + d / STROKE) % 1000.0;
        let want = if speed > 0.15 { 1.0 } else { 0.0 };
        self.effort += (want - self.effort) * (dt * 4.0).min(1.0);
        // (at rest, back to the start of a push: the hands on the rims)
        if want == 0.0 && self.effort < 0.02 {
            self.push = self.push.floor();
        }
    }
}

/// The extra slots of a figure in a wheelchair or with a pram: the wheels turned as far as
/// it has rolled (on the torso, or on the pram's glide frame - the model's rest frame, not
/// bobbing with the steps), a wheelchair's arms in their push.
pub fn mobility_bones(ty: &HumanType, bones: &mut [Affine3A; SLOTS], r: &Rolling) {
    let Some(m) = ty.mobility.as_ref() else { return };
    let base = if m.base == GLIDE { Affine3A::IDENTITY } else { bones[m.base] };
    bones[GLIDE] = Affine3A::IDENTITY;
    let about = |p: Vec3, angle: f32| Affine3A::from_translation(p) * Affine3A::from_rotation_x(angle) * Affine3A::from_translation(-p);
    for k in 0..2 {
        bones[AXLE[k]] = match m.axles[k] {
            // (rolling forwards the top of a wheel goes forwards: about x, the other way)
            Some(a) => base * about(a.centre, -r.rolled / a.radius.max(0.02)),
            None => base,
        };
    }
    if let Some(arms) = m.arms {
        let phase = r.push.fract() * TAU;
        let swing = (SHOULDER_AHEAD - SHOULDER_BACK) * 0.5 + (SHOULDER_AHEAD + SHOULDER_BACK) * 0.5 * phase.sin();
        let shoulder = (r.effort * swing).to_radians();
        // (forwards on the rim while the shoulder swings forwards - the push - and back with
        // the forearm bent up off it)
        let elbow = (ELBOW_LIFT * r.effort * (-phase.cos()).max(0.0)).to_radians();
        for side in 0..2 {
            let upper = base * about(arms[side].shoulder, shoulder);
            bones[ARM[side]] = upper;
            bones[FOREARM[side]] = upper * about(arms[side].elbow, elbow);
        }
    }
}

/// Which of the moving parts a type has (`HumanType::load`).
pub(super) fn rig_of(def: &Human, meshes: &mut [HumanMesh], pram_mesh: Option<usize>) -> Option<MobilityRig> {
    match def.mobility? {
        Mobility::Wheelchair => rig_wheelchair(meshes),
        Mobility::Pram => rig_pram(meshes, pram_mesh),
    }
}

#[cfg(test)]
#[path = "mobility_tests.rs"]
mod tests;
