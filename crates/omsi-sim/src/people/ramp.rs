//! (NexiconDK, 1 November 2026) A bus's ramp for wheelchairs: the flap a driver folds out at a
//! door (the Solaris Urbino's at its second door: the mesh `Urbino_Wnetrze_Rampa.o3d`, clicked
//! as `pandus_2`, turned by `pandus_2_mov`). In the player's bus somebody in a wheelchair rolls
//! in or out at a door with a ramp only once it is out - and a driver who leaves without
//! folding it out has left them behind (`passed_by`).
//!
//! A ramp is found in the bus's model: a mesh with an animation whose name, mouse event or
//! variable says ramp (`pandus`, `rampe`, `ramp`), at the door nearest it. Whether it is out is
//! read from its variable as the script leaves it, without knowing the script: it has turned
//! (or slid) from where it rested to the far end of its travel and stands still there.

use super::*;

/// Words a ramp's mesh, mouse event or variable has in it (Polish, German, English).
const NAMES: [&str; 3] = ["pandus", "rampe", "ramp"];
/// How near a door a ramp lies (m, along and across the bus).
const NEAR_DOOR: f32 = 2.0;
/// How often the driver is told of somebody waiting for the ramp (s).
const TELL_EVERY: f64 = 12.0;
/// How long the ramp's variable stands still at the end of its travel before it is out (s),
/// and how far from that end it may stand (a share of the travel).
const SETTLED: f32 = 0.4;
const AT_END: f32 = 0.9;

/// A ramp of the player's bus: the variable that moves it, the entries and the exits (of the
/// cabin's) of the door it is at - every wing of it -, and that door's number from the front
/// (for the driver's hint).
#[derive(Debug, Clone, PartialEq)]
pub struct Ramp {
    pub var: String,
    pub entries: Vec<usize>,
    pub exits: Vec<usize>,
    pub door: usize,
}

/// Whether a name says ramp.
fn ramp_name(s: &str) -> bool {
    let s = s.to_ascii_lowercase();
    NAMES.iter().any(|n| s.contains(n))
}

/// The ramps of a bus of type `ty` with cabin `cabin`: each animated mesh of its model that
/// says ramp, at the door nearest the middle of the mesh.
pub fn ramps_of(ty: &crate::vehicle::VehicleType, cabin: &Cabin) -> Vec<Ramp> {
    let mut out: Vec<Ramp> = Vec::new();
    for m in &ty.model.meshes {
        let Some(anim) = m.animations.iter().find(|a| a.kind.is_some() && !a.variable.trim().is_empty()) else { continue };
        let named = ramp_name(&m.file) || m.mouse_event.as_deref().is_some_and(ramp_name) || ramp_name(&anim.variable);
        if !named || out.iter().any(|r| r.var.eq_ignore_ascii_case(anim.variable.trim())) {
            continue;
        }
        // where it lies: the middle of its mesh (Direct3D's axes in the file: y up, z forwards)
        let path = omsi_cfg::resolve_path(&ty.model_dir, &m.file);
        let Ok(mesh) = omsi_o3d::load_mesh(&path) else { continue };
        let (lo, hi) = mesh.vertices.iter().fold((Vec3::MAX, Vec3::MIN), |(a, b), v| (a.min(v.position), b.max(v.position)));
        if lo.x > hi.x {
            continue;
        }
        let mid = (lo + hi) * 0.5;
        let at = Vec3::new(mid.x, mid.z, mid.y);
        let nearest = |doors: &[Door]| (0..doors.len()).filter(|&d| (doors[d].inside.truncate() - at.truncate()).length() < NEAR_DOOR).min_by(|&a, &b| (doors[a].inside - at).length().total_cmp(&(doors[b].inside - at).length()));
        let (entry, exit) = (nearest(&cabin.entries), nearest(&cabin.exits));
        let Some(door) = entry.map(|e| &cabin.entries[e]).or_else(|| exit.map(|x| &cabin.exits[x])).map(|d| door_number(cabin, d)) else { continue };
        // (every wing of that door: the Citaro G's ramp lies by one wing of its door 2)
        let wings = |doors: &[Door]| (0..doors.len()).filter(|&k| door_number(cabin, &doors[k]) == door).collect::<Vec<usize>>();
        let (entries, exits) = (wings(&cabin.entries), wings(&cabin.exits));
        log::info!("{}: a ramp at door {door} ({}, moved by {})", ty.def.path.display(), m.file, anim.variable.trim());
        out.push(Ramp { var: anim.variable.trim().to_string(), entries, exits, door });
    }
    out
}

/// The number from the front of door `d` among all of the cabin's - its ways in and its ways out,
/// the wings of one door one door (the Citaro G's way out by its ramp is its door 2).
fn door_number(cabin: &Cabin, d: &Door) -> usize {
    let same = |a: &Door, b: &Door| (a.inside.y - b.inside.y).abs() < super::cabin::DOOR_WINGS && a.side == b.side && a.group == b.group;
    let mut sorted: Vec<&Door> = cabin.entries.iter().chain(cabin.exits.iter()).collect();
    sorted.sort_by(|a, b| b.inside.y.total_cmp(&a.inside.y).then(a.group.cmp(&b.group)));
    let mut fronts: Vec<&Door> = Vec::new();
    for x in sorted {
        if !fronts.iter().any(|f| same(f, x)) {
            fronts.push(x);
        }
    }
    fronts.iter().position(|f| same(f, d)).map_or(1, |n| n + 1)
}

/// What is seen of a ramp's variable: the furthest it has gone from where it rested, where it
/// was, and how long it has stood there.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RampSeen {
    rest: Option<f32>,
    far: f32,
    last: f32,
    still: f32,
}

impl RampSeen {
    /// The variable at `v` after `dt` s.
    pub fn see(&mut self, v: f32, dt: f32) {
        let v = if v.is_finite() { v } else { 0.0 };
        let rest = *self.rest.get_or_insert(v);
        self.far = self.far.max((v - rest).abs());
        let travel = self.far.max(1e-6);
        if (v - self.last).abs() <= travel * 1e-3 {
            self.still += dt;
        } else {
            self.still = 0.0;
        }
        self.last = v;
    }

    /// It is out: gone (almost) as far as it goes from where it rested, and standing there.
    pub fn out(&self) -> bool {
        let Some(rest) = self.rest else { return false };
        self.far > 1e-4 && (self.last - rest).abs() >= self.far * AT_END && self.still >= SETTLED
    }
}

impl PeopleSim {
    /// The player's bus's ramps' variables now (`CabinView::ramps`).
    pub fn ramp_values(&self, v: &VehicleInstance) -> Vec<f32> {
        self.player_ramps.iter().map(|r| v.var(&r.var).unwrap_or(0.0)).collect()
    }

    /// The ramps of the player's bus as its view says this frame, `dt` s on.
    pub(super) fn see_ramps(&mut self, values: &[f32], dt: f32) {
        self.ramp_seen.resize(self.player_ramps.len(), RampSeen::default());
        for (seen, v) in self.ramp_seen.iter_mut().zip(values) {
            seen.see(*v, dt);
        }
    }

    /// What to tell the driver of somebody waiting for the ramp this frame: getting in (else
    /// out) at door number n - now and then while they wait, and of one getting out only with
    /// the bus standing (`standing`).
    pub fn take_ramp_hint(&mut self, standing: bool) -> Option<(bool, usize)> {
        let w = self.ramp_wanted.filter(|w| w.0 || standing)?;
        if self.time - self.ramp_told < TELL_EVERY {
            return None;
        }
        self.ramp_told = self.time;
        Some(w)
    }

    /// Whether person `i` - in a wheelchair, at door `door` of the player's bus `bn`, getting
    /// in (`entering`, an entry) or out (an exit) - waits for its ramp: there is one there and
    /// it is not out. The driver is told (`ramp_wanted`).
    pub(super) fn waits_for_ramp(&mut self, i: usize, bn: &BusNow, door: usize, entering: bool) -> bool {
        if bn.id != BusId::Player || self.mobility(i) != Some(omsi_content::human::Mobility::Wheelchair) {
            return false;
        }
        let Some(k) = self.player_ramps.iter().position(|r| if entering { r.entries.contains(&door) } else { r.exits.contains(&door) }) else { return false };
        if self.ramp_seen.get(k).is_some_and(|s| s.out()) {
            return false;
        }
        self.ramp_wanted = Some((entering, self.player_ramps[k].door));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A ramp folds out (0 to 180 degrees): it is out once it has stood at the end a moment -
    /// not while it turns, nor when it is folded back in. One that rests at 180 and folds out to
    /// 0 alike.
    #[test]
    fn a_ramp_is_out_at_the_end_of_its_travel_standing_still() {
        let mut r = RampSeen::default();
        for _ in 0..10 {
            r.see(0.0, 0.1);
        }
        assert!(!r.out(), "folded in");
        for k in 1..=12 {
            r.see(15.0 * k as f32, 0.1);
            assert!(!r.out(), "turning: {}", 15 * k);
        }
        for _ in 0..3 {
            r.see(180.0, 0.1);
        }
        assert!(!r.out(), "just come to the end");
        for _ in 0..3 {
            r.see(180.0, 0.1);
        }
        assert!(r.out(), "out");
        for k in (0..12).rev() {
            r.see(15.0 * k as f32, 0.1);
        }
        for _ in 0..10 {
            r.see(0.0, 0.1);
        }
        assert!(!r.out(), "folded in again");
        let mut back = RampSeen::default();
        back.see(180.0, 0.1);
        for k in (0..12).rev() {
            back.see(15.0 * k as f32, 0.1);
        }
        for _ in 0..6 {
            back.see(0.0, 0.1);
        }
        assert!(back.out(), "out the other way");
    }

    #[test]
    fn a_ramp_is_known_by_its_name() {
        for n in ["pandus_2_mov", "Urbino_Wnetrze_Rampa.o3d", "Rampe_hinten", "door_ramp"] {
            assert!(ramp_name(n), "{n}");
        }
        for n in ["door_2_mov", "Klappe", "wiper"] {
            assert!(!ramp_name(n), "{n}");
        }
    }
}
