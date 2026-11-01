//! The moving parts of a wheelchair and a pram (`mobility`), on figures made of boxes and
//! rings.
use super::*;

/// A mesh of the triangles `tris` over `pts`, every vertex on the torso; one material.
fn mesh(pts: Vec<Vec3>, tris: Vec<[u32; 3]>) -> HumanMesh {
    let n = pts.len();
    let indices: Vec<u32> = tris.iter().flatten().copied().collect();
    let data = MeshData { normals: vec![Vec3::Z; n], uvs: vec![Vec2::ZERO; n], ranges: vec![(0, indices.len() as u32, 0)], indices, positions: pts, one_sided: true };
    HumanMesh { data, materials: vec![omsi_o3d::Material::default()], bones: Vec::new(), skin: vec![Influence { n: 1, slot: [MAIN as u8, 0, 0, 0], weight: [1.0, 0.0, 0.0, 0.0] }; n], alpha: vec![0], morphs: Vec::new() }
}

/// Add a box from `lo` to `hi`, its sides cut every 5 cm up (as a figure's mesh has vertices
/// all the way up), its faces joined.
fn add_box(pts: &mut Vec<Vec3>, tris: &mut Vec<[u32; 3]>, lo: Vec3, hi: Vec3) {
    let b = pts.len() as u32;
    let levels = (((hi.z - lo.z) / 0.05).round() as u32).max(1);
    for l in 0..=levels {
        let z = lo.z + (hi.z - lo.z) * l as f32 / levels as f32;
        for (x, y) in [(lo.x, lo.y), (hi.x, lo.y), (hi.x, hi.y), (lo.x, hi.y)] {
            pts.push(Vec3::new(x, y, z));
        }
    }
    for l in 0..levels {
        for k in 0..4u32 {
            let (a, c) = (b + 4 * l + k, b + 4 * l + (k + 1) % 4);
            tris.push([a, c, a + 4]);
            tris.push([c, c + 4, a + 4]);
        }
    }
    tris.push([b, b + 1, b + 2]);
    tris.push([b, b + 2, b + 3]);
    let t = b + 4 * levels;
    tris.push([t, t + 1, t + 2]);
    tris.push([t, t + 2, t + 3]);
}

/// Add a wheel: a ring about an axis along x at `x`, centre (`y`, `z`), outer radius `r`, 2 cm
/// thick.
fn add_wheel(pts: &mut Vec<Vec3>, tris: &mut Vec<[u32; 3]>, x: f32, y: f32, z: f32, r: f32) {
    let b = pts.len() as u32;
    let n = 24u32;
    for k in 0..n {
        let a = k as f32 / n as f32 * TAU;
        for (rr, xx) in [(r, x), (r * 0.8, x), (r, x + 0.02), (r * 0.8, x + 0.02)] {
            pts.push(Vec3::new(xx, y + rr * a.cos(), z + rr * a.sin()));
        }
    }
    for k in 0..n {
        let (i, j) = (b + 4 * k, b + 4 * ((k + 1) % n));
        tris.push([i, j, i + 1]);
        tris.push([i + 2, j + 2, i + 3]);
    }
}

/// A seated figure in a wheelchair: legs, a body (to ±0.18 m) and a head, its arms 4 cm out
/// from its body (0.22 to 0.30 m) from the shoulders down to the hands at 0.6 m, the big
/// wheels (centre 0.3 m up, radius 0.3) at its sides and castors in front (radius 0.08).
fn wheelchair() -> Vec<HumanMesh> {
    let (mut pts, mut tris) = (Vec::new(), Vec::new());
    add_box(&mut pts, &mut tris, Vec3::new(-0.15, 0.0, 0.45), Vec3::new(0.15, 0.45, 0.55));
    add_box(&mut pts, &mut tris, Vec3::new(-0.15, 0.4, 0.05), Vec3::new(0.15, 0.5, 0.45));
    add_box(&mut pts, &mut tris, Vec3::new(-0.18, -0.1, 0.5), Vec3::new(0.18, 0.1, 1.35));
    add_box(&mut pts, &mut tris, Vec3::new(-0.1, -0.1, 1.35), Vec3::new(0.1, 0.1, 1.6));
    for s in [-1.0f32, 1.0] {
        let (a, b) = if s > 0.0 { (0.22, 0.30) } else { (-0.30, -0.22) };
        add_box(&mut pts, &mut tris, Vec3::new(a, -0.05, 0.6), Vec3::new(b, 0.05, 1.3));
        add_wheel(&mut pts, &mut tris, s * 0.33, 0.0, 0.3, 0.3);
        add_wheel(&mut pts, &mut tris, s * 0.2, 0.45, 0.08, 0.08);
    }
    vec![mesh(pts, tris)]
}

fn slot(m: &HumanMesh, v: usize) -> usize {
    let inf = m.skin[v];
    let k = (0..inf.n as usize).max_by(|&a, &b| inf.weight[a].total_cmp(&inf.weight[b])).unwrap();
    inf.slot[k] as usize
}

fn type_of(meshes: Vec<HumanMesh>, mobility: Option<MobilityRig>) -> HumanType {
    let def = Human::default();
    let joints = Joints::from_links(&def.links);
    HumanType {
        omsi: crate::human_omsi::OmsiRig::new(&def),
        rig: Rig::measure(&def, &joints, &meshes),
        joints,
        def,
        model: omsi_model::Model::default(),
        model_dir: PathBuf::new(),
        meshes,
        variants: Vec::new(),
        extra_dirs: Vec::new(),
        face: None,
        mobility,
    }
}

/// The wheels of a wheelchair are found in its mesh - the big ones and the castors, each pair
/// about one axis - and turn as it rolls: a quarter of a turn on, the top of a big wheel has
/// gone forwards and down to the front.
#[test]
fn a_wheelchairs_wheels_are_found_and_turn_as_it_rolls() {
    let mut meshes = wheelchair();
    let rig = rig_wheelchair(&mut meshes).expect("its moving parts");
    let big = rig.axles[0].expect("the big wheels");
    let castor = rig.axles[1].expect("the castors");
    assert!((big.centre - Vec3::new(0.0, 0.0, 0.3)).length() < 0.01 && (big.radius - 0.3).abs() < 0.01, "{big:?}");
    assert!((castor.centre - Vec3::new(0.0, 0.45, 0.08)).length() < 0.01 && (castor.radius - 0.08).abs() < 0.01, "{castor:?}");
    assert_eq!(rig.base, MAIN);
    let m = &meshes[0];
    let top = m.data.positions.iter().position(|p| p.x > 0.32 && (p.z - 0.6).abs() < 1e-4 && p.y.abs() < 1e-4).expect("the top of the right wheel");
    assert_eq!(slot(m, top), AXLE[0]);
    let ty = type_of(meshes, Some(rig));
    let mut bones = [Affine3A::IDENTITY; SLOTS];
    mobility_bones(&ty, &mut bones, &Rolling { rolled: TAU * 0.3 / 4.0, push: 0.0, effort: 0.0 });
    let p = bones[AXLE[0]].transform_point3(ty.meshes[0].data.positions[top]);
    assert!((p - Vec3::new(p.x, 0.3, 0.3)).length() < 1e-3, "a quarter turn on: {p}");
}

/// A wheelchair figure's arms - out beyond its body, from its shoulders down - swing in a push
/// as it rolls: the hands forwards a quarter of the way into a push, back at three quarters;
/// its body and its chair stay.
#[test]
fn a_wheelchairs_arms_push_the_rims() {
    let mut meshes = wheelchair();
    let rig = rig_wheelchair(&mut meshes).expect("its moving parts");
    let arms = rig.arms.expect("its arms");
    assert!(arms[1].shoulder.x > 0.18 && arms[1].shoulder.z > 1.1 && arms[0].shoulder.x < -0.18, "{arms:?}");
    assert!(arms[1].elbow.z < arms[1].shoulder.z && arms[1].elbow.z > 0.7, "{arms:?}");
    let m = &meshes[0];
    let hand = m.data.positions.iter().position(|p| p.x > 0.29 && (p.z - 0.6).abs() < 1e-4 && p.y > 0.04).expect("the right hand");
    let body = m.data.positions.iter().position(|p| (p.x - 0.18).abs() < 1e-4 && (p.z - 0.5).abs() < 1e-4).expect("the body's side");
    assert!(matches!(slot(m, hand), s if s == FOREARM[1]), "{}", slot(m, hand));
    assert_eq!(slot(m, body), MAIN);
    let ty = type_of(meshes, Some(rig));
    let at = |push: f32, effort: f32| {
        let mut bones = [Affine3A::IDENTITY; SLOTS];
        mobility_bones(&ty, &mut bones, &Rolling { rolled: 0.0, push, effort });
        let m = &ty.meshes[0];
        let (mut pos, mut nrm) = (Vec::new(), Vec::new());
        skin(m, &bones, &mut pos, &mut nrm);
        (pos[hand], pos[body])
    };
    let (rest, body_rest) = at(0.0, 0.0);
    let (ahead, body_ahead) = at(0.25, 1.0);
    let (back, _) = at(0.75, 1.0);
    assert!(ahead.y > rest.y + 0.15, "pushing forwards: {rest} -> {ahead}");
    assert!(back.y < rest.y - 0.06 && back.y > rest.y - 0.2, "back for the next push, by the hip: {back}");
    assert!((body_ahead - body_rest).length() < 1e-5, "the body stays");
    assert!((at(0.25, 0.0).0 - rest).length() < 1e-5, "standing still: no push");
}

/// A pram of its own mesh glides on: on a frame of its own, not the hips that bob with the
/// steps; its wheels, two axles of them, turn.
#[test]
fn a_pram_glides_on_its_wheels_turning() {
    let (mut pts, mut tris) = (Vec::new(), Vec::new());
    add_box(&mut pts, &mut tris, Vec3::new(-0.2, -0.1, 0.0), Vec3::new(0.2, 0.1, 1.6));
    let figure = mesh(pts, tris);
    let (mut pts, mut tris) = (Vec::new(), Vec::new());
    add_box(&mut pts, &mut tris, Vec3::new(-0.25, 0.4, 0.4), Vec3::new(0.25, 1.2, 0.9));
    for (y, s) in [(0.55, -1.0f32), (0.55, 1.0), (1.1, -1.0), (1.1, 1.0)] {
        add_wheel(&mut pts, &mut tris, s * 0.27, y, 0.15, 0.15);
    }
    let pram = mesh(pts, tris);
    let mut meshes = vec![figure, pram];
    let rig = rig_pram(&mut meshes, Some(1)).expect("the pram");
    assert_eq!(rig.base, GLIDE);
    let ys: Vec<f32> = rig.axles.iter().flatten().map(|a| a.centre.y).collect();
    assert!(ys.len() == 2 && ys.iter().any(|y| (y - 0.55).abs() < 0.01) && ys.iter().any(|y| (y - 1.1).abs() < 0.01), "{ys:?}");
    assert!(meshes[1].skin.iter().all(|inf| inf.n == 1 && [GLIDE, AXLE[0], AXLE[1]].contains(&(inf.slot[0] as usize))));
    assert!(meshes[0].skin.iter().all(|inf| inf.slot[0] as usize == MAIN), "the figure is left alone");
    let ty = type_of(meshes, Some(rig));
    // (the hips and the body bobbing down with a step: the pram does not)
    let mut bones = [Affine3A::from_translation(Vec3::new(0.0, 0.0, -0.05)); SLOTS];
    mobility_bones(&ty, &mut bones, &Rolling { rolled: 0.3, push: 0.0, effort: 0.0 });
    assert_eq!(bones[GLIDE], Affine3A::IDENTITY);
    let basket = ty.meshes[1].data.positions[0];
    assert_eq!(bones[GLIDE].transform_point3(basket), basket);
    assert!(bones[AXLE[0]].transform_point3(Vec3::new(0.0, ys[0], 0.3)).y > ys[0] + 0.05, "the wheels turn");
}

/// A figure without a wheelchair or a pram: nothing found, nothing changed.
#[test]
fn a_figure_on_foot_has_no_moving_parts() {
    let def = Human::default();
    let mut meshes = wheelchair();
    assert!(rig_of(&def, &mut meshes, None).is_none());
    assert!(meshes[0].skin.iter().all(|inf| inf.slot[0] as usize == MAIN));
}

/// Rolling on, the push comes up to strength; stopping, it eases off and the arms come back to
/// the start of a push.
#[test]
fn the_push_eases_in_and_out() {
    let mut r = Rolling::default();
    for _ in 0..60 {
        r.advance(1.0, 1.0 / 30.0);
    }
    assert!(r.effort > 0.95 && (r.rolled - 2.0).abs() < 1e-3 && r.push > 1.5, "{r:?}");
    for _ in 0..90 {
        r.advance(0.0, 1.0 / 30.0);
    }
    assert!(r.effort < 0.02 && r.push.fract() == 0.0, "{r:?}");
}

/// (probe) Real figures (`PROBE_HUMANS`, `;`-separated .hum files): what is found, and the
/// skinned vertices at a few moments of rolling, as CSV into `PROBE_OUT`.
#[test]
#[ignore]
fn probe_figures() {
    let out = std::path::PathBuf::from(std::env::var("PROBE_OUT").unwrap());
    for (n, path) in std::env::var("PROBE_HUMANS").unwrap().split(';').enumerate() {
        let ty = HumanType::load(std::path::Path::new(path)).unwrap();
        println!("{path}: {:?}", ty.mobility);
        let anim = crate::human_omsi::OmsiAnim::default();
        let mut tris = String::new();
        for (mi, m) in ty.meshes.iter().enumerate() {
            for t in m.data.indices.chunks_exact(3) {
                tris += &format!("{mi},{},{},{}\n", t[0], t[1], t[2]);
            }
        }
        std::fs::write(out.join(format!("fig{n}_tris.csv")), tris).unwrap();
        let frames = 24;
        for k in 0..frames {
            let f = k as f32 / frames as f32;
            let mut bones = slots_from_omsi(&anim.bones(&ty.omsi));
            let r = Rolling { rolled: f * 1.1, push: f, effort: 1.0 };
            mobility_bones(&ty, &mut bones, &r);
            let mut text = String::new();
            for (mi, m) in ty.meshes.iter().enumerate() {
                let (mut pos, mut nrm) = (Vec::new(), Vec::new());
                skin(m, &bones, &mut pos, &mut nrm);
                for (i, p) in pos.iter().enumerate() {
                    text += &format!("{mi},{i},{:.4},{:.4},{:.4},{}\n", p.x, p.y, p.z, m.skin[i].slot[0]);
                }
            }
            std::fs::write(out.join(format!("fig{n}_{k}.csv")), text).unwrap();
        }
    }
}
