//! (Omsi-Hub, 16 November 2026: openOMSI's GPS screen in the cockpit, `[navigatortexture]` -
//! docs/NAVIGATORTEXTURE.md - brought over to this navigator. NexiconDK, from a friend's tablet:
//! "GPS-kortet fra spillets navigator: Omsi-Hub har det ikke.") The small map on a screen of the
//! player's bus, drawn by the same `paint_panel` as the panel in the window's corner, told what
//! differs by a `ScreenLook`. It is drawn whether or not the panel shows - hiding the panel
//! (Shift+N) leaves the screen on - and is black while the variable that powers it reads 0.

use super::*;
use crate::nav_panel::{self, Layout};

/// The map and the bars round it fit this many layout points high (openOMSI's).
const SCREEN_POINTS_HIGH: f32 = 300.0;
/// The route behind the bus (`showHistory`): its own vertex buffer, clear of the panel's
/// (0-2, 6), the city map's (3-7) and the ground's (`ground::PANEL_BUFFERS` on).
pub(super) const HISTORY_BUFFER: usize = 24;

/// A mod's GPS screen on the player's bus: its texture (a render target the scene made for the
/// script texture slot), its size, whether it is on, and how it is drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Screen {
    pub texture: TextureId,
    pub size: (u32, u32),
    pub on: bool,
    pub look: ScreenLook,
}

/// How a screen's drawing differs from the panel's (the `[navigatortexture]`'s options).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScreenLook {
    /// A zoom on top of the navigator's own: 2 shows half the way, 0.5 twice it.
    pub map_scale: f64,
    /// North at the top, the bus's arrow turning.
    pub fixed_north: bool,
    /// The map alone: no bars, cards, schedule or vehicles' tags.
    pub no_nav_ui: bool,
    /// The bus in the middle of the map, not low in it looking ahead.
    pub nav_center: bool,
    /// The trip's route behind the bus too, and all its stops served greyed.
    pub show_history: bool,
}

impl Default for ScreenLook {
    fn default() -> Self {
        ScreenLook { map_scale: 1.0, fixed_north: false, no_nav_ui: false, nav_center: false, show_history: false }
    }
}

impl Screen {
    /// The model's first `[navigatortexture]`, its texture among a vehicle's script textures
    /// (`textures`), on as `var` reads its power variable.
    pub fn find(model: &omsi_model::Model, textures: &[Option<TextureId>], var: impl Fn(&str) -> Option<f32>) -> Option<Screen> {
        let def = model.navigator_textures.first()?;
        let texture = textures.get(def.script_index).copied().flatten()?;
        Some(Screen {
            texture,
            size: (def.width.max(1) as u32, def.height.max(1) as u32),
            on: def.is_on(var),
            look: ScreenLook { map_scale: def.map_scale, fixed_north: def.fixed_north, no_nav_ui: def.no_nav_ui, nav_center: def.nav_center, show_history: def.show_history },
        })
    }

    pub fn of_vehicle(vehicle: &omsi_sim::VehicleInstance, render: &omsi_scene::VehicleRender) -> Option<Screen> {
        Self::find(&vehicle.ty.model, &render.script_textures, |name| vehicle.var(name))
    }
}

/// The scale (pixels a point) a screen `size` pixels is laid out at: held by its shorter way
/// against the design's (`SCREEN_POINTS_HIGH` high, `nav_panel::WIDTH` wide).
pub(super) fn screen_scale(size: (u32, u32)) -> f32 {
    (size.1 as f32 / SCREEN_POINTS_HIGH).min(size.0 as f32 / nav_panel::WIDTH).max(0.05)
}

/// The parts of a screen `size` pixels at scale `s` with `stops` stops ahead: the map alone
/// (`no_nav_ui`); else the header, the map, the next-stop bar and as many of the next stops
/// under it as fit while the map keeps half the screen (and the rows they take).
pub(super) fn screen_layout(size: (u32, u32), s: f32, stops: usize, no_nav_ui: bool) -> (Layout, usize) {
    let (w, h) = (size.0 as f32, size.1 as f32);
    if no_nav_ui {
        let none = Rect::new(0.0, h, w, 0.0);
        return (Layout { s, head: Rect::new(0.0, 0.0, w, 0.0), map: Rect::new(0.0, 0.0, w, h), next: none, board: None, handle: None, beside: false }, 0);
    }
    let bars = (nav_panel::HEAD * s).round() + (nav_panel::NEXT * s).round();
    let room = (h * 0.5 - bars).max(0.0);
    let (rows, bh) = schedule_fitting(stops, s, room);
    (nav_panel::layout(w, h, s, (rows > 0).then_some(bh), false, false), rows)
}

#[derive(Default)]
pub(super) struct ScreenState {
    /// As last drawn or blacked out: a change is drawn at once.
    shown: Option<Screen>,
    frames_since_drawn: u32,
    drawn_at: f32,
    /// Held only while a screen is on, so a bus without one costs no video memory.
    attachments: Option<omsi_ui::Attachments>,
}

impl Navigator {
    /// The player's bus's GPS screen this frame (None: it has none).
    pub fn set_screen(&mut self, screen: Option<Screen>) {
        self.screen = screen;
    }

    /// Whether the screen asks for the navigator's frame though it shows nothing itself.
    pub(super) fn screen_wants_frame(&self) -> bool {
        self.screen.is_some_and(|s| s.on) || self.screen != self.screen_state.shown
    }

    /// Draw the screen when due (the panel's pace), black it out when it went off.
    pub(super) fn frame_screen(&mut self, renderer: &Renderer, scene: &Scene, f: &NavFrame) {
        let Some(screen) = self.screen.filter(|s| s.on) else {
            if let Some(off) = self.screen.filter(|_| self.screen != self.screen_state.shown) {
                renderer.clear_texture(scene, off.texture, wgpu::Color::BLACK);
            }
            self.screen_state = ScreenState { shown: self.screen, ..ScreenState::default() };
            return;
        };
        let changed = self.screen_state.shown != Some(screen);
        let state = &mut self.screen_state;
        state.frames_since_drawn = state.frames_since_drawn.saturating_add(1);
        let due = self.time - state.drawn_at >= NAV_REDRAW_S && state.frames_since_drawn >= NAV_REDRAW_FRAMES;
        // (the city map covers the cockpit: the screen keeps its last picture)
        if !changed && (self.city.open || !due) {
            return;
        }
        // (a GPU that could not make the texture as large as declared made it smaller)
        let (Some(view), Some((w, h, _))) = (renderer.texture_view(scene, screen.texture), renderer.texture_levels(scene, screen.texture)) else { return };
        state.frames_since_drawn = 0;
        state.drawn_at = self.time;
        let mut attachments = state.attachments.take().unwrap_or_default();
        let size = (w.max(1), h.max(1));
        let s = screen_scale(size);
        let (lay, rows) = screen_layout(size, s, f.stops.len(), screen.look.no_nav_ui);
        if self.gpu.is_none() {
            self.gpu = Some(Gpu::with_depth(&renderer.device, renderer.format(), map_samples(renderer.format()), self.atlas.size));
        }
        self.painting = Some(screen.look);
        self.draw(renderer, &view, size, &lay, f, rows, Some(&mut attachments));
        self.painting = None;
        self.screen_state.attachments = Some(attachments);
        self.screen_state.shown = Some(screen);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(text: &str) -> omsi_model::Model {
        omsi_model::Model::parse(&omsi_cfg::CfgFile::from_str("model.cfg", text))
    }

    #[test]
    fn a_vehicle_screen_is_its_first_navigator_texture() {
        let m = model("[scripttexture]\n64\n32\n[navigatortexture]\n800\n480\npower\n[navigatortexture]\n10\n10\n");
        let textures = [Some(7), Some(9), Some(11)];
        let on = Screen::find(&m, &textures, |_| Some(1.0)).unwrap();
        assert_eq!(on, Screen { texture: 9, size: (800, 480), on: true, look: ScreenLook::default() });
        assert!(!Screen::find(&m, &textures, |_| Some(0.0)).unwrap().on);
        assert!(Screen::find(&m, &[Some(7), None], |_| None).is_none(), "its texture is not there");
        assert!(Screen::find(&model("[scripttexture]\n64\n32\n"), &textures, |_| None).is_none());
    }

    #[test]
    fn screen_options_are_taken_from_the_model() {
        let m = model("[navigatortexture]\n800\n480\n\n0.5\n1\n1\n1\n1\n");
        let s = Screen::find(&m, &[Some(9)], |_| None).unwrap();
        assert_eq!(s.look, ScreenLook { map_scale: 0.5, fixed_north: true, no_nav_ui: true, nav_center: true, show_history: true });
    }

    #[test]
    fn a_screen_is_laid_out_for_its_shorter_way() {
        assert!((screen_scale((800, 240)) - 0.8).abs() < 1e-6, "held by the height");
        assert!((screen_scale((360, 1000)) - 1.0).abs() < 1e-6, "held by the width");
    }

    #[test]
    fn the_map_keeps_half_the_screen_or_all_of_it() {
        let s = screen_scale((800, 480));
        let (lay, rows) = screen_layout((800, 480), s, 6, false);
        assert!(lay.map.h >= 240.0 - 1.0, "{lay:?}");
        assert!(rows <= 5 && lay.handle.is_none());
        assert_eq!(lay.board.is_some(), rows > 0);
        let (bare, rows) = screen_layout((800, 480), s, 6, true);
        assert_eq!((bare.map, rows, bare.board), (Rect::new(0.0, 0.0, 800.0, 480.0), 0, None));
        // a low screen: the stops under the map as fit, the map keeping half of it
        let (low, rows) = screen_layout((800, 160), screen_scale((800, 160)), 6, false);
        assert!(low.map.h >= 80.0 - 1.0 && rows <= 5, "{low:?} {rows}");
        // no stops ahead: none under it
        assert_eq!(screen_layout((800, 480), s, 0, false).1, 0);
    }
}
