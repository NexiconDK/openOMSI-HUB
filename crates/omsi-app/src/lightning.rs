//! Thunderstorms in the game (NexiconDK): the weather's storm over the map (omsi-weather
//! `lightning`), its flashes lighting the street and the clouds, the strikes' channels drawn,
//! and their thunder heard when the sound gets to the player (`thunder`).
//!
//! Everything is drawn from a seed every player has (the room's in multiplayer, else the
//! map's name, and the weather's) and the game clock: every game in a room sees the same
//! storm, the same flash at the same moment. The storm's cells drift with the weather's wind
//! and never jump; a flash is in one of them.

use glam::{DVec2, DVec3};
use omsi_weather::lightning::{self, Piece, Storm, Strike};
use omsi_weather::thunder::{self, Thunder};
use std::sync::mpsc;
use std::sync::Arc;

/// A flash still lit or still to be heard.
struct Live {
    strike: Strike,
    /// Its channel (none for a flash in the cloud).
    channel: Vec<Piece>,
}

/// The game's thunderstorm: its storm, its flashes and their thunder.
#[derive(Default)]
pub(crate) struct Lightning {
    storm: Option<Storm>,
    /// The game time (s since year 0 of the clock) the flashes were taken up to.
    taken_to: Option<f64>,
    live: Vec<Live>,
    /// The light of the flashes this frame, for the picture (`omsi_render::Lighting`).
    pub(crate) flash: Option<omsi_render::Flash>,
    /// Thunder being made (on a helper thread) and made, waiting for the sound's moment
    /// (game time).
    made: Option<(mpsc::Sender<(f64, Thunder)>, mpsc::Receiver<(f64, Thunder)>)>,
    waiting: Vec<(f64, Thunder)>,
    playing: Vec<omsi_audio::mixer::VoiceId>,
}

/// What a frame of the storm takes from the game.
pub(crate) struct Moment<'a> {
    /// The setting (Options: thunderstorms).
    pub(crate) on: bool,
    pub(crate) weather: Option<&'a omsi_content::weather::Weather>,
    pub(crate) world: Option<&'a crate::scene::World>,
    /// The room's seed in multiplayer (`lan::population_seed`).
    pub(crate) room: Option<u64>,
    /// Where every player is (a number every game knows them by), for the rare strikes
    /// close to one.
    pub(crate) players: Vec<(u32, DVec2)>,
    pub(crate) clock: &'a omsi_sim::SimClock,
    pub(crate) paused: bool,
    /// Where the camera is: what the player sees and hears from.
    pub(crate) eye: DVec3,
    /// In a vehicle, and how open it is (its `Snd_OutsideVol`): the thunder through the
    /// bodywork.
    pub(crate) inside: bool,
    pub(crate) open: f32,
    pub(crate) volume: f32,
}

/// The game clock as one count of seconds: every game in a room has the same.
pub(crate) fn game_time(c: &omsi_sim::SimClock) -> f64 {
    (c.year as f64 * 366.0 + c.day_of_year as f64) * 86_400.0 + c.time
}

/// The wind the storm drifts with: the weather's, by sixteen points of the compass and in
/// steps of 2 m/s - a report's wind that wanders by a few degrees or a knot leaves the
/// storm where it is (its cells' tracks depend on it).
pub(crate) fn storm_wind(w: &omsi_content::weather::Weather) -> DVec2 {
    let dir = ((w.wind.0 as f64 / 22.5).round() * 22.5).to_radians();
    let speed = (w.wind.1 as f64 / 2.0).round() * 2.0;
    DVec2::new(dir.sin(), dir.cos()) * speed
}

/// The map's centre (the middle of its tiles), where the storm's cells pass, and how far its
/// tiles reach from it (m): every player has the same.
fn map_extent(w: &crate::scene::World) -> (DVec2, f64) {
    let ts = omsi_map::tile_size();
    let tiles = &w.global.tiles;
    if tiles.is_empty() {
        return (DVec2::ZERO, 5000.0);
    }
    let middle = |t: &omsi_map::global::MapTileRef| DVec2::new((t.x as f64 + 0.5) * ts, (t.y as f64 + 0.5) * ts);
    let centre = tiles.iter().fold(DVec2::ZERO, |a, t| a + middle(t)) / tiles.len() as f64;
    let half = tiles.iter().map(|t| (middle(t) - centre).length()).fold(0.0, f64::max) + ts * 0.5;
    (centre, half)
}

impl Lightning {
    /// Whether a storm is going on (the ambience's own thunder keeps quiet then).
    pub(crate) fn active(&self) -> bool {
        self.storm.is_some()
    }

    /// A frame of the storm: its new flashes, their light, the channels into `bolts` and the
    /// thunder that has reached the player.
    pub(crate) fn frame(&mut self, m: Moment, bolts: &mut Vec<omsi_render::Bolt>, audio: Option<&omsi_audio::AudioEngine>) {
        bolts.clear();
        self.flash = None;
        let level = m.weather.map(lightning::thunder_level).unwrap_or(0.0);
        let world = match (m.on && level > 0.0, m.world, m.weather) {
            (true, Some(w), Some(_)) => w,
            _ => {
                self.storm = None;
                self.taken_to = None;
                self.live.clear();
                self.waiting.clear();
                self.muffle(&m, audio);
                return;
            }
        };
        let w = m.weather.expect("weather");
        let map = if world.global.name.is_empty() { world.map_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() } else { world.global.name.clone() };
        let seed = lightning::storm_seed(m.room, &map, &w.name);
        let cloud_base = if w.clouds.1 > 100.0 { w.clouds.1 as f64 } else { 1200.0 };
        let (centre, half) = map_extent(world);
        let storm = Storm::new(seed, centre, half, storm_wind(w), level, cloud_base);
        if self.storm.as_ref() != Some(&storm) {
            if self.storm.as_ref().is_some_and(|s| s.seed != storm.seed) {
                // (another weather: another storm)
                self.live.clear();
                self.waiting.clear();
            }
            self.storm = Some(storm);
        }
        let storm = self.storm.as_ref().expect("storm");
        let t = game_time(m.clock);
        let eye_ground = world.ground_height(m.eye.x, m.eye.y).unwrap_or(m.eye.z - 1.7);
        // the flashes since the last frame (none while the game stands still)
        if !m.paused {
            let from = self.taken_to.filter(|&f| f <= t && t - f < 10.0).unwrap_or(t - 0.001);
            for s in storm.strikes(from, t, &m.players) {
                if (s.at - m.eye.truncate()).length() > lightning::REACH {
                    continue;
                }
                let ground = world.ground_height(s.at.x, s.at.y).unwrap_or(eye_ground);
                let channel = s.channel(ground);
                // its thunder, made on a helper thread (some tens of milliseconds)
                if (s.light_point(ground) - m.eye).length() < thunder::HEARD + s.cloud_base {
                    let tx = self.made.get_or_insert_with(mpsc::channel).0.clone();
                    let (s2, ch2, ear) = (s.clone(), channel.clone(), m.eye);
                    let _ = std::thread::Builder::new().name("thunder".into()).spawn(move || {
                        if let Some(th) = thunder::thunder(&s2, &ch2, ground, ear) {
                            let _ = tx.send((s2.t + th.delay, th));
                        }
                    });
                }
                self.live.push(Live { strike: s, channel });
            }
            self.taken_to = Some(t);
        }
        // (a clock set back or on: what was to come is let go)
        self.live.retain(|l| !l.strike.over(t) && l.strike.t < t + 5.0);
        if !m.paused {
            let strikes: Vec<Strike> = self.live.iter().map(|l| l.strike.clone()).collect();
            if let Some(g) = lightning::glow(&strikes, t, m.eye, eye_ground) {
                self.flash = Some(omsi_render::Flash { dir: g.dir, ground: g.ground, sky: g.sky, spread: g.spread });
            }
            for l in &self.live {
                let (all, first) = (l.strike.light(t, false), l.strike.light(t, true));
                if all <= 0.002 {
                    continue;
                }
                bolts.extend(l.channel.iter().map(|p| omsi_render::Bolt {
                    a: p.a,
                    b: p.b,
                    glow: p.glow * if p.main { all } else { first },
                    width: if p.main { 1.0 } else { 0.6 },
                }));
            }
        }
        // the thunder whose moment has come (a clock set back or far on lets it go)
        if let Some((_, rx)) = self.made.as_ref() {
            self.waiting.extend(rx.try_iter());
        }
        self.waiting.retain(|(at, _)| *at < t + 90.0 && *at > t - 20.0);
        let (now, later): (Vec<_>, Vec<_>) = std::mem::take(&mut self.waiting).into_iter().partition(|(at, _)| *at <= t);
        self.waiting = later;
        if let Some(a) = audio {
            for (at, th) in now {
                // (from where the sound is by now: a strike close by is heard a moment after
                // it, and its thunder may have been made a little after that)
                let skip = ((t - at).max(0.0) * thunder::RATE as f64) as usize;
                if skip + thunder::RATE as usize / 2 >= th.samples.len() {
                    continue;
                }
                let clip = Arc::new(omsi_audio::mixer::Clip { sample_rate: thunder::RATE, channels: 1, samples: th.samples[skip..].to_vec() });
                let id = a.play(clip, self.params(&m, th.from));
                self.playing.push(id);
            }
        }
        self.muffle(&m, audio);
    }

    /// How a thunder from `from` is played: at its own level (the clip holds its distance),
    /// placed where it comes from, through the bodywork when inside.
    fn params(&self, m: &Moment, from: DVec3) -> omsi_audio::mixer::VoiceParams {
        let (through, lowpass) = through_bodywork(m.inside, m.open);
        omsi_audio::mixer::VoiceParams {
            gain: m.volume * through,
            pitch: 1.0,
            looping: false,
            position: Some(from.as_vec3()),
            doppler: false,
            range: 1.0e6,
            lowpass_hz: lowpass,
            important: true,
        }
    }

    /// The thunder heard as the player now is: inside or out, doors open or shut.
    fn muffle(&mut self, m: &Moment, audio: Option<&omsi_audio::AudioEngine>) {
        let Some(a) = audio else {
            self.playing.clear();
            return;
        };
        self.playing.retain(|id| a.is_playing(*id));
        let (through, lowpass) = through_bodywork(m.inside, m.open);
        for id in &self.playing {
            if let Some((mut p, _)) = a.voice_state(*id) {
                let gain = m.volume * through;
                if (p.gain - gain).abs() > 1e-3 || (p.lowpass_hz - lowpass).abs() > 1.0 {
                    p.gain = gain;
                    p.lowpass_hz = lowpass;
                    a.set_params(*id, p);
                }
            }
        }
    }
}

/// How much of the thunder outside comes through a vehicle's bodywork, and its low-pass:
/// quieter and dull unless the doors stand open - but less so than the ambience's sounds
/// (`soundscape`): thunder is mostly the deep notes a bus's sides let through, and a strike
/// near shakes it.
fn through_bodywork(inside: bool, open: f32) -> (f32, f32) {
    let open = if inside { open.clamp(0.0, 1.0) } else { 1.0 };
    (0.45 + 0.55 * open, if open < 0.95 { 900.0 + 6000.0 * open * open } else { 0.0 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_storm_keeps_its_way_through_a_reports_small_changes_of_wind() {
        let mut w = omsi_content::weather::Weather { wind: (243.0, 6.6), ..Default::default() };
        let a = storm_wind(&w);
        w.wind = (247.0, 6.2);
        assert_eq!(a, storm_wind(&w));
        // (towards the south-west and a little west: the wind's heading, as the rain takes it)
        assert!(a.x < 0.0 && a.y < 0.0 && (a.length() - 6.0).abs() < 1e-9);
        w.wind = (290.0, 6.2);
        assert_ne!(a, storm_wind(&w));
        // the clock as one count: a day on, a day's seconds on
        let c = omsi_sim::SimClock { year: 2026, day_of_year: 200, time: 3600.0, ..Default::default() };
        let d = omsi_sim::SimClock { day_of_year: 201, ..c.clone() };
        assert_eq!(game_time(&d) - game_time(&c), 86_400.0);
        assert_eq!(through_bodywork(false, 0.0), (1.0, 0.0));
        assert!((0.3..0.6).contains(&through_bodywork(true, 0.0).0));
    }
}
