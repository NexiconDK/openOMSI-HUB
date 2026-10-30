//! (NexiconDK, 30 October 2026) Waving after a bus that left one at the stop: an arm thrown up
//! high on the bus's side, the hand open towards it and swinging from side to side over the
//! head - "wait, you forgot me!" - as `people::passed_by` has somebody do it. The arm is put
//! by two-bone IK from where the pose had it, `amount` of the way (eased), so it rises and
//! falls back smoothly; the gaze is `HailLook`'s.
use super::*;

/// How fast the hand swings (Hz), the person's own tempo within a little.
fn tempo(person: u32) -> f32 {
    1.55 + (person % 7) as f32 * 0.05
}

/// The side whose arm waves at a bus `toward` (model frame): the one the bus is on.
pub fn wave_side(bones: &[Affine3A; SLOTS], toward: Vec3) -> usize {
    let fwd = bones[MAIN].transform_vector3(Vec3::Y);
    let right = Vec3::new(fwd.x, fwd.y, 0.0).normalize_or(Vec3::Y).cross(Vec3::Z);
    usize::from(toward.dot(right) >= 0.0)
}

/// The waving arm `side` (0 left, 1 right) of a pose, towards a bus at `toward` (model frame,
/// a point), `amount` (0 the pose as it was, 1 waving), `t` seconds into the wave.
pub fn wave_after_arm(rig: &Rig, bones: &mut [Affine3A; SLOTS], side: usize, toward: Vec3, amount: f32, t: f32, person: u32) {
    let amount = if amount.is_finite() { amount.clamp(0.0, 1.0) } else { 0.0 };
    if amount <= 1e-4 || !toward.is_finite() || !t.is_finite() || rig.upper_arm <= 0.0 || rig.forearm <= 0.0 {
        return;
    }
    let amount = amount * amount * (3.0 - 2.0 * amount);
    let side = side.min(1);
    let sh = bones[MAIN].transform_point3(rig.shoulder[side]);
    // (the way to the bus along the ground; straight ahead when it is right over us)
    let fwd = bones[MAIN].transform_vector3(Vec3::Y);
    let fwd = Vec3::new(fwd.x, fwd.y, 0.0).normalize_or(Vec3::Y);
    let to = Vec3::new(toward.x - sh.x, toward.y - sh.y, 0.0).normalize_or(fwd);
    let outward = fwd.cross(Vec3::Z) * SIDE[side];
    // up high, a little towards the bus and out to the side; swung about the way to the bus,
    // so that the hand goes from side to side as the driver sees it
    let up = (Vec3::Z + to * 0.3 + outward * 0.28).normalize();
    let swing = 0.42 * (TAU * tempo(person) * t).sin();
    let dir = Quat::from_axis_angle(to, swing * SIDE[side]) * up;
    let reach = (rig.upper_arm + rig.forearm) * 0.93;
    let old_el = bones[FORE[side]].transform_point3(rig.elbow[side]);
    let old_wr = bones[HAND[side]].transform_point3(rig.wrist[side]);
    let old_upper = bones[UPPER[side]].to_scale_rotation_translation().1;
    let old_fore = bones[FORE[side]].to_scale_rotation_translation().1;
    let target = old_wr.lerp(sh + dir * reach, amount);
    // (the elbow out to the side and a little back, coming from where it was)
    let was = (old_el - (sh + old_wr) * 0.5).normalize_or(outward);
    let want = (outward - to * 0.25 - Vec3::Z * 0.2).normalize();
    let pole = was.lerp(want, amount).normalize_or(want);
    let (el, wr, _) = two_bone(sh, rig.upper_arm, rig.forearm, target, pole);
    let upper_axis = (el - sh).normalize_or(Vec3::Z);
    let fore_axis = (wr - el).normalize_or(Vec3::Z);
    let upper_rest = rig.elbow[side] - rig.shoulder[side];
    let fore_rest = rig.wrist[side] - rig.elbow[side];
    // (from the pose's own roll of each bone, to the palm open towards the bus)
    let upper_native = Quat::from_rotation_arc((old_upper * upper_rest).normalize_or(upper_axis), upper_axis) * old_upper;
    let fore_native = Quat::from_rotation_arc((old_fore * fore_rest).normalize_or(fore_axis), fore_axis) * old_fore;
    let upper_want = Quat::from_mat3a(&bone_rot(upper_rest, -Vec3::Z, upper_axis, to));
    let fore_want = Quat::from_mat3a(&bone_rot(fore_rest, -Vec3::Z, fore_axis, to));
    let upper = Mat3A::from_quat(upper_native.slerp(upper_want, amount));
    let fore = Quat::from_mat3a(&Mat3A::from_quat(fore_native.slerp(fore_want, amount)));
    // the hand straight on from the forearm, its palm to the bus
    let old_hand = bones[HAND[side]].to_scale_rotation_translation().1;
    let natural = fore * old_fore.conjugate() * old_hand;
    let open = Quat::from_mat3a(&bone_rot(fore_rest, -Vec3::Z, fore_axis, to));
    let hand = natural.slerp(open, amount);
    let hand = fore * limit_quat(fore.conjugate() * hand, 40f32.to_radians());
    bones[UPPER[side]] = joint_xf(rig.shoulder[side], sh, upper);
    bones[FORE[side]] = joint_xf(rig.elbow[side], el, Mat3A::from_quat(fore));
    bones[HAND[side]] = joint_xf(rig.wrist[side], wr, Mat3A::from_quat(hand));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rig(height: f32) -> Rig {
        let mut def = Human {
            height: 1.77,
            seat_height: 0.82,
            links: vec![0.09, 0., 0.92, 0.09, -0.03, 0.53, 0.02, 1.17, 0.18, -0.05, 1.43, 0.44, -0.04, 1.41, -0.02, 1.55, 0.69, -0.03, 1.43, 0.9, -0.03, 1.43],
            walk_param: [1.4, 80., 1., 1., 0.],
            ..Default::default()
        };
        let scale = height / def.height;
        for v in &mut def.links {
            *v *= scale;
        }
        def.height = height;
        Rig::measure(&def, &Joints::from_links(&def.links), &[])
    }

    /// A person standing (the arms hanging), facing `heading` (radians about z).
    fn standing(r: &Rig) -> [Affine3A; SLOTS] {
        let mut p = Pose::new(7);
        (p.idle_from, p.idle_to) = (Idle::Hang, Idle::Hang);
        for _ in 0..120 {
            p.advance(r, &PoseInput::default(), 1.0 / 60.0);
        }
        p.bones(r).bones
    }

    #[test]
    fn the_arm_goes_up_over_the_head_on_the_buses_side_and_swings() {
        for height in [1.2, 1.77, 2.0] {
            let r = rig(height);
            let base = standing(&r);
            for bus in [Vec3::new(20.0, 5.0, 1.0), Vec3::new(-20.0, 5.0, 1.0), Vec3::new(4.0, -25.0, 1.0)] {
                let side = wave_side(&base, bus);
                assert_eq!(side, usize::from(bus.x > 0.0), "the arm on the bus's side ({bus:?})");
                let mut xs = Vec::new();
                for k in 0..40 {
                    let t = k as f32 * 0.05;
                    let mut b = base;
                    wave_after_arm(&r, &mut b, side, bus, 1.0, t, 3);
                    assert!(b.iter().all(|x| x.is_finite()));
                    let sh = b[UPPER[side]].transform_point3(r.shoulder[side]);
                    let el = b[FORE[side]].transform_point3(r.elbow[side]);
                    let wr = b[HAND[side]].transform_point3(r.wrist[side]);
                    // (the bones keep their lengths and stay joined)
                    assert!((sh.distance(el) - r.upper_arm).abs() < 1e-3);
                    assert!((el.distance(wr) - r.forearm).abs() < 1e-3);
                    assert!(b[UPPER[side]].transform_point3(r.elbow[side]).distance(el) < 1e-3);
                    // (high over the shoulder, above the head's top at full height)
                    assert!(wr.z > sh.z + 0.6 * (r.upper_arm + r.forearm), "wrist {wr:?} shoulder {sh:?}");
                    // (the other arm and the rest of the body as they were)
                    for slot in 0..SLOTS {
                        if ![UPPER[side], FORE[side], HAND[side]].contains(&slot) {
                            assert!(b[slot].abs_diff_eq(base[slot], 1e-6));
                        }
                    }
                    let to = Vec3::new(bus.x, bus.y, 0.0).normalize();
                    xs.push(wr.dot(to.cross(Vec3::Z)));
                }
                let (lo, hi) = xs.iter().fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(*x), b.max(*x)));
                assert!(hi - lo > 0.4 * (r.upper_arm + r.forearm), "the hand swings from side to side as the bus sees it: {lo}..{hi}");
            }
        }
    }

    #[test]
    fn it_rises_and_falls_smoothly_and_nothing_breaks_it() {
        let r = rig(1.77);
        let base = standing(&r);
        let bus = Vec3::new(18.0, 8.0, 1.0);
        let mut last: Option<Vec3> = None;
        for k in 0..=50 {
            let mut b = base;
            wave_after_arm(&r, &mut b, 1, bus, k as f32 / 50.0, 0.0, 1);
            let wr = b[HAND[1]].transform_point3(r.wrist[1]);
            if let Some(l) = last {
                assert!(l.distance(wr) < 0.1, "a jump of {} at {k}", l.distance(wr));
            }
            last = Some(wr);
        }
        let mut b = base;
        wave_after_arm(&r, &mut b, 1, bus, 0.0, 1.0, 1);
        assert!(b.iter().zip(base.iter()).all(|(x, y)| x.abs_diff_eq(*y, 1e-6)), "nothing at 0");
        for (amount, t, at) in [(f32::NAN, 0.0, bus), (1.0, f32::NAN, bus), (1.0, 0.0, Vec3::splat(f32::NAN))] {
            let mut b = base;
            wave_after_arm(&r, &mut b, 1, at, amount, t, 1);
            assert!(b.iter().zip(base.iter()).all(|(x, y)| x.abs_diff_eq(*y, 1e-6)));
        }
    }
}
