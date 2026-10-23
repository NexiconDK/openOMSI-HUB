//! Thunder (NexiconDK): the sound of a flash, made from its channel as heard from one place.
//!
//! Every part of a channel sends out its own pressure pulse (an N-wave, the shock of the
//! heated air) when the stroke heats it. They reach the ear one after the other, each after
//! its own distance at the speed of sound: the nearest part first - the crack of a strike
//! close by - and the rest over the seconds after as the channel climbs and turns away, which
//! is the rumble; parts square to the line of sight are heard loudest (a clap). On the way
//! the air takes the treble, the more the farther (a time-varying low-pass: what comes later
//! came from farther), so far thunder is a low roll. Under it the air's own deep rumble
//! follows the arrivals, and the land throws echoes back for seconds after.
//!
//! The clip starts with the first sound to arrive (`Thunder::delay` after the flash): played
//! then, every player hears it when the sound of that flash really gets to them.

use crate::lightning::{Piece, Strike, SOUND};
use glam::DVec3;

/// The sample rate the thunder is made at (Hz): it holds little above 8 kHz.
pub const RATE: u32 = 24_000;
/// Farther than this (m) a flash is not heard.
pub const HEARD: f64 = 18_000.0;
/// The level of a strike's crack at 1 km (its peak, before the limiter).
const AT_KM: f32 = 0.32;

/// A flash's thunder at one place.
#[derive(Clone, Debug)]
pub struct Thunder {
    /// After the flash, when the clip starts (s): the first sound's way.
    pub delay: f64,
    /// Where the first sound comes from (the nearest part of the channel).
    pub from: DVec3,
    /// That distance (m).
    pub distance: f64,
    /// Mono samples at `RATE`.
    pub samples: Vec<i16>,
}

struct Draw(u64);

impl Draw {
    fn u(&mut self) -> f32 {
        // (xorshift64*: noise for a clip, quick)
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32 / (1u64 << 24) as f32
    }
}

/// The parts a cloud flash is heard from: a crooked line in the cloud, some kilometres long
/// (it has no channel to the ground).
fn cloud_channel(s: &Strike, ground: f64) -> Vec<Piece> {
    let mut d = Draw(s.id | 1);
    let mid = s.light_point(ground);
    let a = d.u() as f64 * std::f64::consts::TAU;
    let half = 1000.0 + 2500.0 * d.u() as f64;
    let dir = DVec3::new(a.sin(), a.cos(), (d.u() as f64 - 0.5) * 0.3);
    let n = 160;
    let mut out = Vec::with_capacity(n);
    let mut p = mid - dir * half;
    let step = dir * (2.0 * half / n as f64);
    for _ in 0..n {
        let jitter = DVec3::new(d.u() as f64 - 0.5, d.u() as f64 - 0.5, d.u() as f64 - 0.5) * (step.length() * 0.9);
        let q = p + step + jitter;
        out.push(Piece { a: p, b: q, glow: 0.45, main: true });
        p = q;
    }
    out
}

/// The thunder of strike `s` (its channel `channel`, empty for a cloud flash) heard at
/// `ear`, with the ground at `ground` m under the flash; None when it is too far to hear.
pub fn thunder(s: &Strike, channel: &[Piece], ground: f64, ear: DVec3) -> Option<Thunder> {
    let cloud;
    let parts: &[Piece] = if channel.is_empty() {
        cloud = cloud_channel(s, ground);
        &cloud
    } else {
        channel
    };
    // the parts as short elements: their middle, length, direction and strength
    let mut elems: Vec<(DVec3, f64, DVec3, f32)> = Vec::new();
    for p in parts {
        let len = (p.b - p.a).length();
        let n = (len / 12.0).ceil().max(1.0) as usize;
        let dir = (p.b - p.a) / len.max(1e-6);
        for k in 0..n {
            let m = p.a + (p.b - p.a) * ((k as f64 + 0.5) / n as f64);
            elems.push((m, len / n as f64, dir, p.glow));
        }
    }
    let (near, r_min) = elems.iter().map(|e| (e.0, (e.0 - ear).length())).min_by(|a, b| a.1.total_cmp(&b.1))?;
    if r_min > HEARD {
        return None;
    }
    let r_max = elems.iter().map(|e| (e.0 - ear).length()).fold(r_min, f64::max).min(r_min + 9000.0);
    let rate = RATE as f64;
    // (the arrivals' spread, the echoes after it and the roll dying away)
    let tail = 4.5 + 2.0 * (r_min / 6000.0).min(1.0);
    let len = (((r_max - r_min) / SOUND + tail) * rate) as usize + 1;
    let mut clap = vec![0.0f32; len];
    let mut energy = vec![0.0f32; len / 240 + 2];
    let mut d = Draw(s.id ^ 0x7468_756e_6465_72);
    let e = s.energy;
    for (m, l, dir, glow) in &elems {
        let to = *m - ear;
        let r = to.length().max(30.0);
        let i0 = (((r - r_min) / SOUND) * rate) as usize;
        if i0 >= len {
            continue;
        }
        // (a clap square to the line of sight, little along it)
        let sin = (1.0 - (dir.dot(to / r)).powi(2)).max(0.0).sqrt() as f32;
        let a = AT_KM * 1000.0 / r as f32 * glow * e * (l / 12.0).sqrt() as f32 * (0.25 + 0.75 * sin) * (0.6 + 0.8 * d.u());
        // the N-wave: up at once, down through zero to as far below, back at once; longer
        // for a stronger stroke and drawn out by the way (the shock wave slows)
        let tau = (0.0007 + 0.0022 * d.u()) * e.sqrt() + (r as f32 * 1.2e-6);
        let n = ((2.0 * tau * RATE as f32) as usize).max(2);
        for k in 0..n.min(len - i0) {
            clap[i0 + k] += a * (1.0 - 2.0 * k as f32 / n as f32);
        }
        // and the tearing crackle of its kinks: a short burst of noise at the front (the
        // air's low-pass below leaves it only to the strike close by)
        let m = ((0.0015 + 0.0025 * d.u()) * RATE as f32) as usize;
        for k in 0..m.min(len - i0) {
            clap[i0 + k] += a * 0.9 * (d.u() - 0.5) * 2.0 * (-(k as f32) / (m as f32 * 0.35)).exp();
        }
        energy[i0 / 240] += a * a * n as f32;
    }
    // the air takes the treble on the way: a low-pass closing as later sound came farther
    let mut lp1 = 0.0f32;
    let mut lp2 = 0.0f32;
    for (i, x) in clap.iter_mut().enumerate() {
        let r = r_min + i as f64 / rate * SOUND;
        let fc = (16_000.0 * (300.0 / r.max(300.0))).clamp(120.0, 16_000.0);
        let k = (1.0 - (-std::f64::consts::TAU * fc / rate).exp()) as f32;
        lp1 += k * (*x - lp1);
        lp2 += k * (lp1 - lp2);
        *x = lp2;
    }
    // how loud the claps are at their loudest twentieth of a second (the roll's measure)
    let win = (RATE / 20) as usize;
    let mut loudest = 1e-9f32;
    for c in clap.chunks(win) {
        let ms = c.iter().map(|x| x * x).sum::<f32>() / c.len() as f32;
        loudest = loudest.max(ms.sqrt());
    }
    // the roll under it: noise between some 35 and 220 Hz that follows the arrivals
    // (smoothed over a fifth of a second) and dies away after the last, relatively louder
    // the farther (the treble gone, the roll is what is left)
    let smooth = {
        let mut out = vec![0.0f32; energy.len()];
        let mut acc = 0.0f32;
        for (i, v) in energy.iter().enumerate() {
            acc = acc * 0.82 + v.sqrt();
            out[i] = acc;
        }
        out
    };
    let peak_env = smooth.iter().cloned().fold(1e-9, f32::max);
    let k_lo = 1.0 - (-std::f32::consts::TAU * 220.0 / RATE as f32).exp();
    let k_hi = 1.0 - (-std::f32::consts::TAU * 35.0 / RATE as f32).exp();
    let (mut a1, mut a2, mut hp) = (0.0f32, 0.0f32, 0.0f32);
    let mut roll: Vec<f32> = (0..len)
        .map(|_| {
            a1 += k_lo * ((d.u() - 0.5) - a1);
            a2 += k_lo * (a1 - a2);
            hp += k_hi * (a2 - hp);
            a2 - hp
        })
        .collect();
    let roll_rms = (roll.iter().map(|x| x * x).sum::<f32>() / len as f32).sqrt().max(1e-9);
    let roll_k = 0.25 + 0.7 * (r_min as f32 / 5000.0).min(1.0);
    let arrivals = ((r_max - r_min) / SOUND) as f32;
    let mut out = vec![0.0f32; len];
    for i in 0..len {
        let j = (i / 240).min(smooth.len() - 1);
        let after = (i as f32 / RATE as f32 - arrivals).max(0.0);
        let env = (smooth[j] / peak_env).max((-after / 1.4).exp() * 0.3 * (i > win) as u8 as f32);
        roll[i] *= loudest * roll_k * env / roll_rms;
        out[i] = clap[i] + roll[i];
    }
    // the echoes off the land: a few dozen late copies, each softer and duller
    let dry = out.clone();
    let mut echo = vec![0.0f32; len];
    for _ in 0..28 {
        let delay = (0.15 + (-(d.u().max(1e-4)).ln()) * 0.9) as f64;
        let g = 0.3 * (-delay as f32 / 1.6).exp() * if d.u() < 0.5 { -1.0 } else { 1.0 };
        let off = (delay * rate) as usize;
        if off >= len {
            continue;
        }
        for i in off..len {
            echo[i] += dry[i - off] * g;
        }
    }
    let mut elp = 0.0f32;
    for i in 0..len {
        elp += 0.08 * (echo[i] - elp);
        out[i] += elp;
    }
    // as loud as a strike's thunder is at that distance (its loudest moment), then faded out
    // over the last second, a soft limiter for the strike next door
    let peak = out.iter().fold(1e-9f32, |m, x| m.max(x.abs()));
    let target = (AT_KM * e * (1000.0 / r_min.max(50.0) as f32).powf(0.9)).min(1.7);
    let scale = target / peak;
    let fade = (RATE as usize).min(len);
    let samples = out
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let mut v = *x * scale;
            if i + fade > len {
                v *= (len - i) as f32 / fade as f32;
            }
            v = if v.abs() > 0.6 { v.signum() * (0.6 + 0.4 * ((v.abs() - 0.6) / 0.4).tanh()) } else { v };
            (v.clamp(-1.0, 1.0) * 32767.0) as i16
        })
        .collect();
    Some(Thunder { delay: r_min / SOUND, from: near, distance: r_min, samples })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lightning::Storm;
    use glam::DVec2;

    fn strike(ground: bool) -> Strike {
        let s = Storm::new(5, DVec2::ZERO, 5000.0, DVec2::new(5.0, 0.0), 1.0, 1200.0);
        let mut t = 0.0;
        loop {
            if let Some(f) = s.strikes(t, t + 1.0, &[]).into_iter().find(|f| f.ground == ground) {
                return f;
            }
            t += 1.0;
        }
    }

    fn peak(x: &[i16]) -> f32 {
        x.iter().map(|v| v.unsigned_abs()).max().unwrap_or(0) as f32 / 32767.0
    }

    /// Where its loud part is (s into the clip): the first sample over half its peak.
    fn onset(x: &[i16]) -> f32 {
        let p = peak(x) * 32767.0 * 0.5;
        x.iter().position(|v| v.unsigned_abs() as f32 >= p).unwrap_or(0) as f32 / RATE as f32
    }

    /// The share of its energy above `hz` (a sharp crack has much, a far roll little).
    fn treble(x: &[i16], hz: f32) -> f32 {
        let k = 1.0 - (-std::f32::consts::TAU * hz / RATE as f32).exp();
        let (mut lp, mut lo, mut all) = (0.0f32, 0.0f32, 0.0f32);
        for v in x {
            let v = *v as f32;
            lp += k * (v - lp);
            lo += lp * lp;
            all += v * v;
        }
        1.0 - lo / all.max(1.0)
    }

    #[test]
    fn near_thunder_comes_sooner_louder_and_sharper_than_far() {
        let s = strike(true);
        let ch = s.channel(0.0);
        let at = |m: f64| thunder(&s, &ch, 0.0, (s.at + DVec2::new(m, 0.0)).extend(1.5)).unwrap();
        let (near, mid, far) = (at(150.0), at(3000.0), at(12_000.0));
        // the first sound after its way at the speed of sound
        assert!((near.delay - near.distance / SOUND).abs() < 1e-9);
        // (3 km off where it strikes, its nearest branch up to a kilometre nearer)
        assert!(near.distance < 200.0 && (2000.0..3000.1).contains(&mid.distance) && (mid.delay - mid.distance / SOUND).abs() < 1e-9 && far.delay > 30.0);
        assert!(peak(&near.samples) > 0.6, "a strike next door is loud: {}", peak(&near.samples));
        assert!(peak(&near.samples) > 2.0 * peak(&mid.samples) && peak(&mid.samples) > 2.0 * peak(&far.samples));
        assert!(peak(&far.samples) > 0.003, "far thunder is still heard");
        // close: the crack at once; far: a roll that is longer and has lost its treble
        assert!(onset(&near.samples) < 0.05, "crack at {}", onset(&near.samples));
        assert!(treble(&near.samples, 800.0) > 2.5 * treble(&far.samples, 800.0), "{} {}", treble(&near.samples, 800.0), treble(&far.samples, 800.0));
        assert!(far.samples.len() > near.samples.len() / 2);
        // nothing beyond earshot
        assert!(thunder(&s, &ch, 0.0, (s.at + DVec2::new(30_000.0, 0.0)).extend(0.0)).is_none());
        // every player makes the same thunder of it at the same place
        assert_eq!(at(3000.0).samples, mid.samples);
    }

    /// The thunder of one strike from near to far, as numbers and as WAV files in
    /// `OMSI_THUNDER_WAV=<folder>` (`cargo test -p omsi-weather thunder_samples -- --ignored
    /// --nocapture`).
    #[test]
    #[ignore]
    fn thunder_samples() {
        let s = strike(true);
        let ch = s.channel(0.0);
        let dir = std::env::var("OMSI_THUNDER_WAV").ok();
        for m in [80.0, 150.0, 400.0, 1000.0, 3000.0, 6000.0, 12_000.0] {
            let t = thunder(&s, &ch, 0.0, (s.at + DVec2::new(m, 0.0)).extend(1.5)).unwrap();
            eprintln!("{m:>6} m: delay {:5.1} s, {:4.1} s long, peak {:.3}, onset {:.2} s, treble>800Hz {:.3}", t.delay, t.samples.len() as f32 / RATE as f32, peak(&t.samples), onset(&t.samples), treble(&t.samples, 800.0));
            if let Some(d) = &dir {
                let mut w = Vec::new();
                let n = t.samples.len() as u32 * 2;
                w.extend_from_slice(b"RIFF");
                w.extend_from_slice(&(36 + n).to_le_bytes());
                w.extend_from_slice(b"WAVEfmt ");
                w.extend_from_slice(&16u32.to_le_bytes());
                w.extend_from_slice(&1u16.to_le_bytes());
                w.extend_from_slice(&1u16.to_le_bytes());
                w.extend_from_slice(&RATE.to_le_bytes());
                w.extend_from_slice(&(RATE * 2).to_le_bytes());
                w.extend_from_slice(&2u16.to_le_bytes());
                w.extend_from_slice(&16u16.to_le_bytes());
                w.extend_from_slice(b"data");
                w.extend_from_slice(&n.to_le_bytes());
                for v in &t.samples {
                    w.extend_from_slice(&v.to_le_bytes());
                }
                std::fs::write(format!("{d}/thunder_{m:.0}m.wav"), w).unwrap();
            }
        }
        let c = strike(false);
        let t = thunder(&c, &[], 0.0, (c.at + DVec2::new(2000.0, 0.0)).extend(0.0)).unwrap();
        eprintln!("cloud flash 2 km: delay {:5.1} s, {:4.1} s long, peak {:.3}, treble {:.3}", t.delay, t.samples.len() as f32 / RATE as f32, peak(&t.samples), treble(&t.samples, 800.0));
    }

    #[test]
    fn a_cloud_flash_rumbles_without_a_crack() {
        let s = strike(false);
        let t = thunder(&s, &[], 0.0, (s.at + DVec2::new(500.0, 0.0)).extend(0.0)).unwrap();
        // (its sound comes down from the cloud, over a kilometre up)
        assert!(t.distance > 1000.0, "{}", t.distance);
        assert!(peak(&t.samples) > 0.01 && t.samples.len() > RATE as usize * 4);
    }
}
