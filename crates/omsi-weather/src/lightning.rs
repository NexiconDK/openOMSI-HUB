//! Thunderstorms (NexiconDK): storm cells with their own place on the map that drift with the
//! wind, the flashes in them, the channels of the strikes to the ground and the light of their
//! strokes.
//!
//! Everything here follows from a seed, the game's clock and where the players are, so that
//! every player of a session sees the same flashes at the same moment and in the same place:
//! nothing is drawn by chance per frame. Time is cut into one-second slots; whether a slot
//! holds a flash, where and how strong is a pure function of the seed and the slot's number.
//!
//! A storm is a row of cells. Cell `j` grows, matures and dies over an hour or so; its path is
//! a straight line along the wind that passes the map's centre at some distance to the side,
//! halfway through its life - so a storm comes up from one side, passes (sometimes right over
//! the players, mostly beside them) and moves off to the other, and the next one follows on
//! its own track. Most flashes stay in the cloud and only light it; about one in three is a
//! strike to the ground with a visible channel. When a cell is over a player, one flash in
//! about twenty-five strikes close to them (`CLOSE`).

use glam::{DVec2, DVec3, Vec3};
use omsi_content::weather::Weather;

/// The speed of sound (m/s): a flash's thunder comes this much later per metre.
pub const SOUND: f64 = 343.0;

/// The share of the flashes of a cell over a player that strike close to them (60..300 m).
pub const CLOSE: f64 = 0.04;

/// A new cell starts every this many seconds (each one's start drawn within its slot).
const CELL_EVERY: f64 = 25.0 * 60.0;
/// A cell lives this long (s), drawn per cell.
const CELL_LIFE: (f64, f64) = (55.0 * 60.0, 95.0 * 60.0);
/// The slots flashes are drawn in (s).
const SLOT: f64 = 1.0;
/// Flashes farther than this from every player are left out (m): no glow reaches that far.
pub const REACH: f64 = 40_000.0;

/// How strong a thunderstorm `w` has (0: none, up to 1). A METAR says so itself
/// (`Weather::thunder`); an `.owt` file by its name ("Gewitter", "thunder" ...) or by towering
/// clouds over a warm downpour, as the ambience took a thundery sky (`soundscape`).
pub fn thunder_level(w: &Weather) -> f32 {
    if w.thunder > 0.0 {
        return w.thunder.clamp(0.0, 1.0);
    }
    let (kind, rate) = crate::conditions::precip_of(w);
    let rain = if kind == 1 { rate } else { 0.0 };
    let text = format!("{} {} {}", w.name, w.clouds.0, w.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()).to_lowercase();
    let named = ["gewitter", "thunder", "onweer", "orage", "burza", "гроза", "torden", "åska", "temporale", "tormenta"].iter().any(|n| text.contains(n));
    if named {
        return (0.55 + 0.45 * rain).clamp(0.3, 1.0);
    }
    let c = w.clouds.0.to_ascii_lowercase();
    if (c.contains("cumulus 3") || c.contains("cumulonimbus")) && rain > 0.45 && w.temp.0 > 8.0 {
        return (0.4 + 0.6 * (rain - 0.45) / 0.55).clamp(0.3, 1.0);
    }
    0.0
}

/// The seed a storm is drawn from: the session's room seed in multiplayer (every player has
/// it), else the map's name; and the weather (a new weather, a new storm).
pub fn storm_seed(room: Option<u64>, map: &str, weather: &str) -> u64 {
    let base = room.unwrap_or_else(|| fnv(map));
    mix(base ^ fnv(weather).rotate_left(17) ^ 0x7468_756e_6465_7221)
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}

/// SplitMix64: a well-mixed number from `z`.
fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Numbers in 0..1 from a seed, one after the other.
struct Draw(u64);

impl Draw {
    fn new(seed: u64) -> Draw {
        Draw(mix(seed))
    }
    fn u(&mut self) -> f64 {
        self.0 = mix(self.0);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
    fn range(&mut self, a: f64, b: f64) -> f64 {
        a + (b - a) * self.u()
    }
    /// Roughly normal, mean 0 and spread 1 (the sum of three).
    fn normal(&mut self) -> f64 {
        (self.u() + self.u() + self.u() - 1.5) * 2.0
    }
}

/// A storm over a map: what every player shares.
#[derive(Clone, Debug, PartialEq)]
pub struct Storm {
    pub seed: u64,
    /// Where the cells pass (the map's centre, m).
    pub centre: DVec2,
    /// How the cells move (m/s): with the weather's wind, faster than it at the ground.
    pub drift: DVec2,
    /// How strong (`thunder_level`): how often it flashes; a weak one (a METAR's `VCTS`, in
    /// the vicinity) passes farther away.
    pub level: f32,
    /// The clouds' base over the ground (m).
    pub cloud_base: f64,
}

/// A cell at a moment: where it is, how big and how often it flashes (per second).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub index: i64,
    pub at: DVec2,
    pub radius: f64,
    pub rate: f64,
}

/// One flash.
#[derive(Clone, Debug, PartialEq)]
pub struct Strike {
    /// The same for every player (drawn from the seed and the slot).
    pub id: u64,
    /// When it flashes (game time, s).
    pub t: f64,
    /// Where it strikes (or the cloud flash is over).
    pub at: DVec2,
    /// To the ground, with a channel (else in the cloud: its glow only).
    pub ground: bool,
    /// One of the rare ones close to a player.
    pub close: bool,
    /// How strong (about 0.5 .. 1.5).
    pub energy: f32,
    /// Its strokes: when after `t` (s) and how strong.
    pub strokes: Vec<(f32, f32)>,
    /// The clouds' base over the ground (m).
    pub cloud_base: f64,
}

impl Storm {
    /// A storm drawn from `seed` over the map round `centre`, carried by `wind` (m/s, the
    /// direction the air moves, as the snowfall and the clouds take it).
    pub fn new(seed: u64, centre: DVec2, wind: DVec2, level: f32, cloud_base: f64) -> Storm {
        let speed = (wind.length() * 1.4).clamp(4.0, 18.0);
        let dir = if wind.length() > 0.5 {
            wind.normalize()
        } else {
            // (no wind at the ground: the storm drifts all the same, its way drawn)
            let a = Draw::new(seed ^ 0x6472_6966_74).u() * std::f64::consts::TAU;
            DVec2::new(a.sin(), a.cos())
        };
        Storm { seed, centre, drift: dir * speed, level: level.clamp(0.0, 1.0), cloud_base: cloud_base.clamp(500.0, 3000.0) }
    }

    /// Cell `j`: when it starts, how long it lives, where it is halfway through its life,
    /// its radius and how often it flashes when mature (per second). Some slots hold none.
    fn cell(&self, j: i64) -> Option<(f64, f64, DVec2, f64, f64)> {
        let mut d = Draw::new(self.seed ^ (j as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x63656c6c);
        if d.u() > 0.75 {
            return None;
        }
        let birth = j as f64 * CELL_EVERY + d.range(0.0, 0.6) * CELL_EVERY;
        let life = d.range(CELL_LIFE.0, CELL_LIFE.1);
        let along = self.drift.normalize_or_zero();
        let side = DVec2::new(-along.y, along.x);
        // (beside the map's centre: within 12 km either side; a storm in the vicinity
        // only passes 9 to 20 km off)
        let off = if self.level <= 0.35 {
            d.range(9_000.0, 20_000.0) * if d.u() < 0.5 { -1.0 } else { 1.0 }
        } else {
            d.range(-12_000.0, 12_000.0)
        };
        let mid = self.centre + side * off + along * d.range(-3_000.0, 3_000.0);
        let radius = d.range(2_500.0, 4_500.0);
        // (a mature cell flashes one to four times a minute: a storm has its quiet spells)
        let per_minute = d.range(1.0, 3.5) * (0.4 + 0.8 * self.level as f64);
        Some((birth, life, mid, radius, per_minute / 60.0))
    }

    /// The cells alive at `t` (game time, s).
    pub fn cells_at(&self, t: f64) -> Vec<Cell> {
        let mut out = Vec::new();
        let first = ((t - CELL_LIFE.1) / CELL_EVERY).floor() as i64 - 1;
        let last = (t / CELL_EVERY).floor() as i64;
        for j in first..=last {
            let Some((birth, life, mid, radius, peak)) = self.cell(j) else { continue };
            if t < birth || t >= birth + life {
                continue;
            }
            let age = (t - birth) / life;
            let at = mid + self.drift * (t - (birth + life * 0.5));
            // (it grows, matures and dies away)
            let rate = peak * (std::f64::consts::PI * age).sin().powi(2);
            out.push(Cell { index: j, at, radius, rate });
        }
        out
    }

    /// The flashes of the slots that start after `t0` up to `t1` (game time, s): at most the
    /// last three seconds' (a jump of the clock skips the rest). `players`: where the
    /// players are (by a number every game knows them by), for the close strikes - only
    /// a player under a cell has them.
    pub fn strikes(&self, t0: f64, t1: f64, players: &[(u32, DVec2)]) -> Vec<Strike> {
        let mut out = Vec::new();
        if !(t1 > t0) || self.level <= 0.0 {
            return out;
        }
        let first = ((t0 / SLOT).floor() as i64 + 1).max((t1 / SLOT).floor() as i64 - 2);
        let last = (t1 / SLOT).floor() as i64;
        let mut ps: Vec<(u32, DVec2)> = players.to_vec();
        ps.sort_by_key(|p| p.0);
        for k in first..=last {
            if let Some(s) = self.slot(k, &ps) {
                out.push(s);
            }
        }
        out
    }

    /// Slot `k`'s flash, if it has one (it flashes within the second after `k`).
    fn slot(&self, k: i64, players: &[(u32, DVec2)]) -> Option<Strike> {
        let tk = k as f64 * SLOT;
        let cells = self.cells_at(tk);
        let total: f64 = cells.iter().map(|c| c.rate).sum();
        let id = mix(self.seed ^ (k as u64).wrapping_mul(0xD6E8_FEB8_6659_FD93));
        let mut d = Draw::new(id);
        if total <= 0.0 || d.u() >= (total * SLOT).min(0.9) {
            return None;
        }
        // the cell, by how often each flashes
        let mut pick = d.u() * total;
        let cell = *cells.iter().find(|c| {
            pick -= c.rate;
            pick <= 0.0
        }).unwrap_or(&cells[cells.len() - 1]);
        // where in it: mostly in its core
        let r = cell.radius * d.u().powf(0.75);
        let a = d.u() * std::f64::consts::TAU;
        let mut at = cell.at + DVec2::new(a.sin(), a.cos()) * r;
        let t = tk + d.range(0.0, 0.98) * SLOT;
        let mut ground = d.u() < 0.3;
        let mut energy = (d.range(0.5, 1.2) * (0.8 + 0.4 * self.level as f64)) as f32;
        // one of the rare ones close to a player under the cell
        let under: Vec<&(u32, DVec2)> = players.iter().filter(|p| (p.1 - cell.at).length() < cell.radius * 1.2).collect();
        let close = !under.is_empty() && d.u() < CLOSE;
        if close {
            let p = under[((d.u() * under.len() as f64) as usize).min(under.len() - 1)];
            let a = d.u() * std::f64::consts::TAU;
            at = p.1 + DVec2::new(a.sin(), a.cos()) * d.range(60.0, 300.0);
            ground = true;
            energy = energy.max(1.0);
        }
        // the strokes: a strike to the ground has one to five within a few tenths of a second,
        // the first the strongest; a cloud flash flickers on a little longer and weaker
        let mut strokes = vec![(0.0f32, 1.0f32)];
        if ground {
            let n = match d.u() {
                u if u < 0.2 => 1,
                u if u < 0.45 => 2,
                u if u < 0.7 => 3,
                u if u < 0.9 => 4,
                _ => 5,
            };
            let mut at_t = 0.0f32;
            for _ in 1..n {
                at_t += d.range(0.03, 0.12) as f32;
                strokes.push((at_t, d.range(0.4, 0.85) as f32));
            }
        } else {
            let n = 2 + (d.u() * 3.0) as usize;
            let mut at_t = 0.0f32;
            strokes[0].1 = d.range(0.5, 0.9) as f32;
            for _ in 1..n {
                at_t += d.range(0.05, 0.18) as f32;
                strokes.push((at_t, d.range(0.3, 0.8) as f32));
            }
        }
        Some(Strike { id, t, at, ground, close, energy, strokes, cloud_base: self.cloud_base })
    }
}

/// How long a stroke's light takes to fade (s): the return stroke is over in an instant,
/// but the channel glows on and the eye keeps it.
const GLOW: f32 = 0.045;
const CLOUD_GLOW: f32 = 0.09;

impl Strike {
    /// The light of the flash at game time `t` (0 before and after it; about its energy at a
    /// stroke). `first_only`: the first stroke's alone (a channel's branches: the leader's
    /// side channels light with the first return stroke, the later ones follow the main one).
    pub fn light(&self, t: f64, first_only: bool) -> f32 {
        let dt = (t - self.t) as f32;
        if dt < 0.0 {
            return 0.0;
        }
        let tau = if self.ground { GLOW } else { CLOUD_GLOW };
        let mut sum = 0.0;
        for (k, (at, s)) in self.strokes.iter().enumerate() {
            if first_only && k > 0 {
                break;
            }
            let x = dt - at;
            if x >= 0.0 {
                sum += s * (-x / tau).exp();
            }
        }
        sum * self.energy
    }

    /// Over (its light gone): it can be forgotten.
    pub fn over(&self, t: f64) -> bool {
        let last = self.strokes.last().map(|s| s.0).unwrap_or(0.0) as f64;
        t > self.t + last + 0.6
    }

    /// Where its light comes from: a strike's channel in the lower cloud, a cloud flash
    /// higher in it (over the ground at `ground` m).
    pub fn light_point(&self, ground: f64) -> DVec3 {
        let h = if self.ground { self.cloud_base * 0.8 } else { self.cloud_base + 1200.0 };
        self.at.extend(ground + h)
    }

    /// Its channel (a strike to the ground's; none for a cloud flash) down to the ground at
    /// height `ground`: the main channel from the cloud base, zigzagging, and its branches,
    /// each part with how bright it glows. The same for every player (drawn from its id).
    pub fn channel(&self, ground: f64) -> Vec<Piece> {
        if !self.ground {
            return Vec::new();
        }
        let mut d = Draw::new(self.id ^ 0x6368_616e);
        let bottom = self.at.extend(ground);
        let lean = d.range(100.0, 900.0);
        let a = d.u() * std::f64::consts::TAU;
        let top = (self.at + DVec2::new(a.sin(), a.cos()) * lean).extend(ground + self.cloud_base + d.range(0.0, 300.0));
        let main = zigzag(&mut d, top, bottom, 8, 0.2);
        let mut out: Vec<Piece> = main.windows(2).map(|w| Piece { a: w[0], b: w[1], glow: 1.0, main: true }).collect();
        // the branches: from the upper two thirds, down and out, and theirs
        let n = 3 + (d.u() * 7.0) as usize;
        for _ in 0..n {
            let i = ((d.range(0.04, 0.7) * main.len() as f64) as usize).min(main.len() - 2);
            let from = main[i];
            let h = from.z - ground;
            let a = d.u() * std::f64::consts::TAU;
            let out_m = d.range(120.0, 650.0);
            let down = d.range(0.25, 0.75) * h;
            let to = from + DVec3::new(a.sin() * out_m, a.cos() * out_m, -down);
            let glow = (d.range(0.4, 0.7) * (1.0 - 0.3 * i as f64 / main.len() as f64)) as f32;
            let branch = zigzag(&mut d, from, to, 5, 0.22);
            out.extend(branch.windows(2).map(|w| Piece { a: w[0], b: w[1], glow, main: false }));
            for _ in 0..(d.u() * 2.5) as usize {
                let j = ((d.range(0.1, 0.8) * branch.len() as f64) as usize).min(branch.len() - 1);
                let s0 = branch[j];
                let a = d.u() * std::f64::consts::TAU;
                let l = d.range(50.0, 260.0);
                let s1 = s0 + DVec3::new(a.sin() * l, a.cos() * l, -d.range(0.3, 1.0) * l);
                let sub = zigzag(&mut d, s0, s1, 4, 0.25);
                out.extend(sub.windows(2).map(|w| Piece { a: w[0], b: w[1], glow: glow * 0.55, main: false }));
            }
        }
        out
    }
}

/// A part of a channel: from `a` to `b` (world), how bright, and whether it is the main
/// channel (which every stroke lights; a branch only the first).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub a: DVec3,
    pub b: DVec3,
    pub glow: f32,
    pub main: bool,
}

/// A line from `a` to `b` made crooked by midpoint displacement: `depth` halvings, each
/// midpoint moved aside by `rough` of its segment's length (the zigzag of a lightning
/// channel: the same kinks on every scale).
fn zigzag(d: &mut Draw, a: DVec3, b: DVec3, depth: u32, rough: f64) -> Vec<DVec3> {
    let mut pts = vec![a, b];
    for _ in 0..depth {
        let mut next = Vec::with_capacity(pts.len() * 2);
        for w in pts.windows(2) {
            let (p, q) = (w[0], w[1]);
            let len = (q - p).length();
            let along = (q - p) / len.max(1e-6);
            // (aside: any direction across the segment)
            let mut side = DVec3::new(d.normal(), d.normal(), d.normal() * 0.5);
            side -= along * side.dot(along);
            let m = (p + q) * 0.5 + side.normalize_or_zero() * len * rough * d.normal().abs().min(2.0);
            next.push(p);
            next.push(m);
        }
        next.push(*pts.last().unwrap());
        pts = next;
    }
    pts
}

/// The flash's light at the place of the camera, as the renderer takes it
/// (`omsi_render::Flash`): towards where it shines from, how much it lights the ground
/// (1 = 10 000 lux, as the enhanced picture's units), how bright the clouds glow there and
/// how wide that glow spreads (radians).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Glow {
    pub dir: Vec3,
    pub ground: f32,
    pub sky: f32,
    pub spread: f32,
}

/// The light of the flashes `strikes` at game time `t` seen from `eye` (the ground under
/// the strikes at `ground` m), summed: towards the strongest.
pub fn glow(strikes: &[Strike], t: f64, eye: DVec3, ground: f64) -> Option<Glow> {
    let mut best: Option<(f32, Glow)> = None;
    let (mut g_sum, mut s_sum) = (0.0f32, 0.0f32);
    for s in strikes {
        let l = s.light(t, false);
        if l <= 0.001 {
            continue;
        }
        let p = s.light_point(ground);
        let d = (p.truncate() - eye.truncate()).length();
        // the ground's light: a strike's channel lights the street near it as day (some 300
        // lux under it), the clouds spread a cloud flash's farther and fainter
        let (g0, d0) = if s.ground { (0.03, 700.0) } else { (0.012, 1500.0) };
        let g = l * g0 / (1.0 + (d / d0).powi(2)) as f32;
        let sky = l * 0.02 / (1.0 + (d / 4000.0).powi(2)) as f32;
        g_sum += g;
        s_sum += sky;
        let to = (p - eye).as_vec3().normalize_or_zero();
        let spread = (3000.0 / d.max(1.0)).atan() as f32;
        if best.as_ref().is_none_or(|b| sky > b.0) {
            best = Some((sky, Glow { dir: to, ground: 0.0, sky: 0.0, spread: spread.clamp(0.15, 1.4) }));
        }
    }
    best.map(|(_, g)| Glow { ground: g_sum, sky: s_sum, ..g })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn storm() -> Storm {
        Storm::new(42, DVec2::new(1500.0, 2500.0), DVec2::new(6.0, 0.0), 0.8, 1200.0)
    }

    /// A day of the storm's flashes, slot by slot as the frames take them.
    fn flashes(s: &Storm, players: &[(u32, DVec2)], from: f64, to: f64, frame: f64) -> Vec<Strike> {
        let mut out = Vec::new();
        let mut t = from;
        while t < to {
            out.extend(s.strikes(t, t + frame, players));
            t += frame;
        }
        out
    }

    #[test]
    fn every_game_sees_the_same_flashes_whatever_its_frame_rate() {
        let s = storm();
        let p = [(1, DVec2::new(1500.0, 2500.0))];
        let a = flashes(&s, &p, 0.0, 3.0 * 3600.0, 1.0 / 60.0);
        let b = flashes(&s, &p, 0.0, 3.0 * 3600.0, 1.0 / 23.0);
        assert!(a.len() > 50, "{} flashes in three hours", a.len());
        assert_eq!(a, b);
        // and another seed, another storm
        let other = Storm { seed: 43, ..storm() };
        assert_ne!(a, flashes(&other, &p, 0.0, 3.0 * 3600.0, 1.0 / 60.0));
        // the room's seed decides in multiplayer, the map's name else; the weather always
        assert_eq!(storm_seed(Some(7), "Ahlheim", "metar:EKCH"), storm_seed(Some(7), "Grundorf", "metar:EKCH"));
        assert_ne!(storm_seed(None, "Ahlheim", "x"), storm_seed(None, "Grundorf", "x"));
        assert_ne!(storm_seed(Some(7), "Ahlheim", "a"), storm_seed(Some(7), "Ahlheim", "b"));
    }

    #[test]
    fn a_cell_moves_steadily_with_the_wind_and_does_not_jump() {
        let s = storm();
        let mut seen = 0;
        for t in (0..(4 * 3600)).step_by(10) {
            let t = t as f64;
            for c in s.cells_at(t) {
                if let Some(c2) = s.cells_at(t + 10.0).into_iter().find(|x| x.index == c.index) {
                    let moved = c2.at - c.at;
                    assert!((moved - s.drift * 10.0).length() < 1e-6, "cell {} jumped by {moved:?}", c.index);
                    seen += 1;
                }
            }
        }
        assert!(seen > 500);
        // (the wind east: the storm goes east, at 1.4 times the wind)
        assert!((s.drift - DVec2::new(8.4, 0.0)).length() < 1e-9);
        // the flashes are in the cells
        for f in flashes(&s, &[], 0.0, 4.0 * 3600.0, 0.5) {
            let cells = s.cells_at(f.t.floor());
            assert!(cells.iter().any(|c| (c.at - f.at).length() <= c.radius + 1.0), "a flash outside every cell");
        }
    }

    #[test]
    fn strikes_close_to_a_player_are_rare_and_only_under_a_cell() {
        let s = Storm::new(9, DVec2::ZERO, DVec2::new(0.0, 5.0), 1.0, 1200.0);
        let mut under = 0usize;
        let mut close = 0usize;
        // a player wherever a cell is (the worst case: always under one)
        let mut t = 0.0;
        while t < 40.0 * 3600.0 {
            let cells = s.cells_at(t);
            if let Some(c) = cells.iter().max_by(|a, b| a.rate.total_cmp(&b.rate)) {
                let p = [(5u32, c.at)];
                for f in s.strikes(t, t + 1.0, &p) {
                    if (f.at - c.at).length() < c.radius * 2.0 || f.close {
                        under += 1;
                    }
                    if f.close {
                        close += 1;
                        let d = (f.at - c.at).length();
                        assert!((59.0..=301.0).contains(&d) && f.ground, "close at {d} m");
                    }
                }
            }
            t += 1.0;
        }
        let share = close as f64 / under as f64;
        assert!(under > 1000 && (0.02..0.07).contains(&share), "{close} close of {under}: {share:.3}");
        // nobody under a cell: none close
        let far = [(5u32, DVec2::new(500_000.0, 0.0))];
        assert!(flashes(&s, &far, 0.0, 10.0 * 3600.0, 1.0).iter().all(|f| !f.close));
    }

    #[test]
    fn a_strike_has_strokes_a_channel_to_the_ground_and_fades() {
        let s = storm();
        let all = flashes(&s, &[], 0.0, 6.0 * 3600.0, 1.0);
        let strike = all.iter().find(|f| f.ground && f.strokes.len() >= 3).expect("a strike with three strokes");
        let ch = strike.channel(50.0);
        let main: Vec<_> = ch.iter().filter(|p| p.main).collect();
        assert_eq!(main.len(), 256);
        // from the cloud base down to the ground, where it strikes
        assert!((main.last().unwrap().b - strike.at.extend(50.0)).length() < 1e-6);
        assert!(main[0].a.z >= 50.0 + strike.cloud_base);
        assert!(ch.iter().any(|p| !p.main), "branches");
        // the same channel for every player
        assert_eq!(ch, strike.channel(50.0));
        // light: none before, a flicker of strokes, gone after
        assert_eq!(strike.light(strike.t - 0.01, false), 0.0);
        assert!(strike.light(strike.t + 0.001, false) > 0.5 * strike.energy);
        let second = strike.t + strike.strokes[1].0 as f64 + 0.001;
        assert!(strike.light(second, false) > strike.light(second - 0.002, false), "the second stroke lights it again");
        assert!(strike.light(second, true) < strike.light(second, false), "the branches only with the first");
        assert!(strike.over(strike.t + 3.0) && !strike.over(strike.t));
        assert!(strike.light(strike.t + 1.5, false) < 0.01);
        // a cloud flash has no channel
        let cloud = all.iter().find(|f| !f.ground).unwrap();
        assert!(cloud.channel(0.0).is_empty());
    }

    #[test]
    fn the_glow_comes_from_the_flash_and_fades_with_its_distance() {
        let s = storm();
        let f = flashes(&s, &[], 0.0, 6.0 * 3600.0, 1.0).into_iter().find(|f| f.ground).unwrap();
        let t = f.t + 0.002;
        let at = |d: f64| glow(std::slice::from_ref(&f), t, (f.at + DVec2::new(d, 0.0)).extend(0.0), 0.0).unwrap();
        let (near, mid, far) = (at(300.0), at(3000.0), at(15_000.0));
        assert!(near.ground > mid.ground * 5.0 && mid.ground > far.ground * 5.0, "{near:?} {mid:?} {far:?}");
        assert!(near.sky > far.sky && near.spread > far.spread);
        // towards the flash (west of the eye, up)
        assert!(mid.dir.x < 0.0 && mid.dir.z > 0.0);
        assert!(glow(std::slice::from_ref(&f), f.t + 2.0, DVec3::ZERO, 0.0).is_none());
    }

    /// How a storm feels at the map's centre over a day: flashes an hour, and how far
    /// (`cargo test -p omsi-weather storm_statistics -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn storm_statistics() {
        for level in [0.3f32, 0.7, 1.0] {
            let s = Storm::new(11, DVec2::ZERO, DVec2::new(4.0, 3.0), level, 1200.0);
            let p = [(1u32, DVec2::ZERO)];
            let all = flashes(&s, &p, 0.0, 24.0 * 3600.0, 1.0);
            let near = |m: f64| all.iter().filter(|f| f.at.length() < m).count();
            let ground = all.iter().filter(|f| f.ground).count();
            let close = all.iter().filter(|f| f.close).count();
            eprintln!(
                "level {level}: {:.0} flashes an hour ({ground} to the ground, {close} close); within 1 km {}, 3 km {}, 8 km {}, 16 km {}, 40 km {}",
                all.len() as f64 / 24.0, near(1000.0), near(3000.0), near(8000.0), near(16000.0), near(40000.0)
            );
        }
    }

    #[test]
    fn a_thunderstorm_is_read_from_the_weather() {
        let mut w = Weather { name: "Sommergewitter".into(), precip: vec![1.0, 200.0], temp: (22.0, 10.0), ..Default::default() };
        assert!(thunder_level(&w) > 0.8);
        w.name = "Regen".into();
        assert_eq!(thunder_level(&w), 0.0);
        w.clouds.0 = "Cumulus 3".into();
        assert!(thunder_level(&w) > 0.5);
        w.temp.0 = 3.0;
        assert_eq!(thunder_level(&w), 0.0, "no thunder in a cold rain");
        let m = omsi_content::weather::from_metar("EKCH", "EKCH 221520Z 24014KT 3000 +TSRA BKN025CB 22/18 Q1006");
        assert_eq!(thunder_level(&m), 1.0);
    }
}
