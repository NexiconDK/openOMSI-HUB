//! (NexiconDK, 31 October 2026) An AI bus's driver greets the player's bus he meets: coming the
//! other way along the same road, both driving, some 110 to 35 m apart, he flashes his main beam
//! twice - or, three times in ten, gives a short toot (`ev_AI_Horn`, as `horn` has it sound).
//! How often he does is the setting `ai_greet` (`TrafficSim::greet_share`: every time,
//! sometimes, rarely, never); once a meeting, greeted or not, and the same bus not again for two
//! minutes.
//!
//! Presentation only, it decides nothing: no claim, no stop, no speed depends on it. Each game
//! looks at its own player's bus - a LAN client at the host's buses as it draws them - so the
//! greeting is the player's own; the others neither see nor hear it.

use super::*;

/// How far apart the two buses are when he greets (m): near enough to be seen, before they pass.
pub const GREET_FROM: f64 = 110.0;
pub const GREET_TO: f64 = 35.0;
/// Seconds before the same bus's driver greets again.
pub const GREET_AGAIN: f32 = 120.0;
/// The main beam flashed: on, off, on (s from the greeting).
const FLASH: [(f32, f32); 2] = [(0.0, 0.3), (0.5, 0.8)];
const FLASH_SECS: f32 = 0.8;
/// The share of the greetings that are a toot rather than a flash.
const TOOT_SHARE: f32 = 0.3;

impl TrafficSim {
    /// Once a tick, the mirror's too: who meets the player's bus (`player`, as `tick` has it)
    /// and greets it.
    pub(crate) fn greet_tick(&mut self, dt: f32, player: Option<PlayerBox>) {
        self.greet_clock += dt;
        let now = self.greet_clock;
        self.flashes.retain(|(_, since)| now - *since < FLASH_SECS);
        self.greeted.retain(|_, at| now - *at < GREET_AGAIN);
        if self.greet_share <= 0.0 {
            return;
        }
        let Some((pc, ph, _, _, pv)) = player else { return };
        if pv.abs() < 3.0 {
            return;
        }
        let h = ph.to_radians();
        let pf = DVec2::new(h.sin(), h.cos());
        for i in 0..self.cars.len() {
            let c = &self.cars[i];
            let bus = c.is_bus() || c.vehicle.ty.def.path.extension().is_some_and(|e| e.eq_ignore_ascii_case("bus"));
            if !bus || c.is_rail() || c.state.speed < 3.0 || self.greeted.contains_key(&c.id) {
                continue;
            }
            let d = (c.vehicle.position - pc).truncate();
            if !(GREET_TO..=GREET_FROM).contains(&d.length()) {
                continue;
            }
            let hc = c.vehicle.heading.to_radians();
            let cf = DVec2::new(hc.sin(), hc.cos());
            // coming the other way along the same road, each ahead of the other
            if pf.dot(cf) > -0.85 || pf.dot(d) <= 0.0 || cf.dot(-d) <= 0.0 || (pf.x * d.y - pf.y * d.x).abs() > 9.0 {
                continue;
            }
            let (id, seed) = (c.id, c.seed);
            self.greeted.insert(id, now);
            let n = self.greet_n;
            self.greet_n += 1;
            if horn::draw(seed, 900 + n) >= self.greet_share {
                continue;
            }
            if horn::draw(seed, 1900 + n) < TOOT_SHARE {
                let _ = self.cars[i].vehicle.trigger("ev_AI_Horn");
                self.cars[i].horn.last = self.time;
                self.honked.push((id, true));
                if self.honked.len() > 64 {
                    self.honked.remove(0);
                }
            } else {
                self.flashes.push((id, now));
            }
            if omsi_cfg::flags::OMSI_DEBUG_TRAFFIC.is_set() {
                log::info!("t={:.1}: bus {id} greets the player's bus {:.0} m off", self.time, d.length());
            }
        }
    }

    /// Whether car `id`'s driver flashes his main beam now (`AiFrame::flash`).
    pub fn flashing(&self, id: u64) -> bool {
        self.flashes.iter().any(|(c, since)| {
            let t = self.greet_clock - since;
            *c == id && FLASH.iter().any(|(a, b)| t >= *a && t < *b)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::scenario_tests::{add_car, street, traffic, Fixture};
    use super::*;

    /// A bus on its way north at `y`, 10 m/s; the player's bus going south `gap` m ahead of it
    /// (`across` m to the side, heading `heading`).
    fn meeting(f: &Fixture, seed: u64) -> (TrafficSim, u64) {
        let mut net = Network { lanes: vec![street(DVec3::ZERO, 0.0, 600.0)], ..Default::default() };
        net.link(1.5);
        let mut t = traffic(f, net);
        let id = add_car(&mut t, f, 0, 300.0, seed, Some(10.0));
        let c = t.cars.iter_mut().find(|c| c.id == id).unwrap();
        c.vehicle.position = DVec3::new(0.0, 300.0, 0.0);
        c.vehicle.heading = 0.0;
        c.state.speed = 10.0;
        (t, id)
    }

    fn player(gap: f64, across: f64, heading: f64) -> Option<PlayerBox> {
        Some((DVec3::new(across, 300.0 + gap, 0.0), heading, 6.0, 1.25, 10.0))
    }

    #[test]
    fn a_bus_coming_the_other_way_greets_the_players_once() {
        let f = Fixture::new();
        // (a driver who flashes rather than toots)
        let seed = (0..100u64).find(|s| horn::draw(*s, 1900) >= TOOT_SHARE).unwrap();
        let (mut t, id) = meeting(&f, seed);
        t.greet_share = 1.0;
        t.greet_tick(0.1, player(80.0, -3.5, 180.0));
        assert!(t.greeted.contains_key(&id) && t.flashes.len() == 1 && t.honked.is_empty());
        // the main beam: on, off, on, off
        let at = |t: &mut TrafficSim, dt: f32| {
            t.greet_tick(dt, None);
            t.flashing(id)
        };
        assert!(t.flashing(id));
        assert!(!at(&mut t, 0.35));
        assert!(at(&mut t, 0.2));
        assert!(!at(&mut t, 0.3));
        // once a meeting
        t.greet_tick(0.1, player(60.0, -3.5, 180.0));
        assert!(t.flashes.is_empty() && t.greet_n == 1);
        // a toot from one who toots
        let seed = (0..100u64).find(|s| horn::draw(*s, 1900) < TOOT_SHARE).unwrap();
        let (mut t, id) = meeting(&f, seed);
        t.greet_share = 1.0;
        t.greet_tick(0.1, player(80.0, -3.5, 180.0));
        assert_eq!(t.honked, vec![(id, true)]);
    }

    #[test]
    fn only_a_bus_meeting_the_players_on_the_same_road_greets_it() {
        let f = Fixture::new();
        for (gap, across, heading, speed, why) in [
            (80.0, -3.5, 0.0, 10.0, "the same way"),
            (200.0, -3.5, 180.0, 10.0, "too far"),
            (20.0, -3.5, 180.0, 10.0, "passing already"),
            (-80.0, -3.5, 180.0, 10.0, "behind it, going away"),
            (80.0, -25.0, 180.0, 10.0, "another road"),
            (80.0, -3.5, 180.0, 0.0, "the player standing"),
        ] {
            let (mut t, _) = meeting(&f, 1);
            t.greet_share = 1.0;
            let p = player(gap, across, heading).map(|(c, h, l, w, _)| (c, h, l, w, speed));
            t.greet_tick(0.1, p);
            assert!(t.greeted.is_empty(), "{why}");
        }
        let (mut t, _) = meeting(&f, 1);
        t.greet_share = 0.0;
        t.greet_tick(0.1, player(80.0, -3.5, 180.0));
        assert!(t.greeted.is_empty() && t.flashes.is_empty(), "never");
    }
}
