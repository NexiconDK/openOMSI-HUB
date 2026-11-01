//! (NexiconDK, 1 November 2026) The player's bus passing the cameras and the speed displays by
//! the road (`omsi_sim::roadside`, put up by `omsi_scene::roadside`): a speed camera flashes
//! when the bus is faster than the limit and the tolerance, and the plugins hear `speed_camera`
//! (Omsi-Hub's drive watch fines it); a red-light camera flashes when the bus runs a red light
//! (the fine is the drive watch's own, as before); a display shows the bus's speed and a face.
//! The add-ons whose models are missing are named once, when the first tiles are there.

use omsi_scene::roadside::{OFF, SEEN, UNSEEN};
use omsi_sim::roadside as rs;

/// The `speed_cameras` setting as the world keeps it.
pub(crate) fn mode(setting: &str) -> u8 {
    match omsi_launcher_lib::speed_cameras(setting) {
        "invisible" => UNSEEN,
        "off" => OFF,
        _ => SEEN,
    }
}

/// How long the card naming the missing add-ons stays (s).
const MISSING_SECS: f32 = 14.0;

impl crate::App {
    /// This frame's look at the roadside (with the game running): the setting handed to the
    /// world (new tiles follow it), the missing models told once, and what the player's bus
    /// passed.
    pub(crate) fn roadside_step(&mut self) {
        let mode = mode(&self.settings.speed_cameras);
        if let Some(w) = self.world.as_ref() {
            w.settings.set_roadside(mode);
            if mode == SEEN && !self.session.roadside_told && self.player.is_some() {
                self.session.roadside_told = true;
                let missing = w.roadside_missing();
                if !missing.is_empty() {
                    log::info!("speed cameras: not installed {missing:?} (their places are measured unseen)");
                    let text = omsi_ui::tr("Some speed cameras are not shown - these add-ons are not installed in your OMSI folder: %{addons}").replace("%{addons}", &missing.join(", "));
                    crate::ui::push_notice(&mut self.menus.notices, crate::ui::Notice { kind: crate::ui::NoticeKind::Warn, text, left: MISSING_SECS, total: MISSING_SECS });
                }
            }
        }
        if mode == OFF {
            self.session.roadside.forget();
            return;
        }
        let (Some(t), Some(p)) = (self.session.traffic.as_ref(), self.player.as_ref()) else { return };
        let v = &p.vehicle;
        let Some(passed) = self.session.roadside.step(&t.net, v.position, v.heading) else { return };
        let kmh = v.physics.velocity_kmh().abs();
        let now = self.clock.time;
        let seen = mode == SEEN;
        match passed.site.kind {
            rs::Kind::Speed => {
                if rs::over_limit(kmh, passed.site.limit).is_none() {
                    return;
                }
                if seen {
                    rs::trigger(&passed.site, now);
                }
                let round = |x: f32| omsi_plugin::InfoValue::Num((x as f64 * 10.0).round() / 10.0);
                crate::plugins::queue_event(&mut self.integrations.plugin_events, "speed_camera", vec![round(kmh), round(passed.site.limit)]);
            }
            rs::Kind::RedLight => {
                use omsi_sim::traffic::{Aspect, TrafficLightController};
                let red = t.net.lanes[passed.lane]
                    .traffic_light
                    .and_then(|(c, li)| t.light_state(c, li))
                    .is_some_and(|(state, _)| TrafficLightController::aspect(state) == Aspect::Red);
                if seen && red && kmh > 5.0 {
                    rs::trigger(&passed.site, now);
                }
            }
            rs::Kind::Display => {
                if seen {
                    rs::measure(rs::object_id(&passed.site, 0), now, kmh as f64 / 3.6);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// The setting's three values, and the fines' count is the cameras': a camera flashes
    /// exactly when the company fines.
    #[test]
    fn the_cameras_flash_when_the_company_fines() {
        use omsi_scene::roadside::{OFF, SEEN, UNSEEN};
        assert_eq!([super::mode("visible"), super::mode("invisible"), super::mode("off"), super::mode("")], [SEEN, UNSEEN, OFF, SEEN]);
        for limit in [30.0f32, 50.0, 70.0, 100.0, 120.0] {
            for k in 0..800 {
                let kmh = k as f32 * 0.25;
                assert_eq!(omsi_sim::roadside::over_limit(kmh, limit), omsi_launcher_lib::company::career::over_limit(kmh, limit), "{kmh} in {limit}");
            }
        }
    }
}
