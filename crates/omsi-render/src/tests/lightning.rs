use super::*;

fn lightning_renderer() -> Renderer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    ))
    .expect("test renderer")
}

/// A grey floor round the camera and a wall to the left of the view: the street a flash
/// lights, and something a channel behind it is hidden by.
fn street(renderer: &Renderer, scene: &mut Scene) {
    let grey = renderer.add_material_extra(scene, None, AlphaMode::Opaque, [0.5, 0.5, 0.5, 1.0], false, None, None, None, None, [0.0; 3], MaterialExtra::default());
    let floor = renderer.add_mesh(scene, &MeshData {
        positions: vec![Vec3::new(-60.0, -10.0, 0.0), Vec3::new(60.0, -10.0, 0.0), Vec3::new(60.0, 200.0, 0.0), Vec3::new(-60.0, 200.0, 0.0)],
        normals: vec![Vec3::Z; 4],
        uvs: vec![glam::Vec2::ZERO; 4],
        ranges: vec![(0, 6, 0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        one_sided: false,
    });
    renderer.add_instance(scene, floor, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    // (from 5 to 40 degrees left of the view, 40 m ahead, 60 m high)
    let wall = renderer.add_mesh(scene, &MeshData {
        positions: vec![Vec3::new(-33.0, 40.0, 0.0), Vec3::new(-3.5, 40.0, 0.0), Vec3::new(-3.5, 40.0, 60.0), Vec3::new(-33.0, 40.0, 60.0)],
        normals: vec![-Vec3::Y; 4],
        uvs: vec![glam::Vec2::ZERO; 4],
        ranges: vec![(0, 6, 0)],
        indices: vec![0, 1, 2, 0, 2, 3],
        one_sided: false,
    });
    renderer.add_instance(scene, wall, DVec3::ZERO, Mat4::IDENTITY, vec![grey]);
    // a stormy night's sky (the night picture of the three)
    let mut sky = |rgb: [u8; 3]| renderer.add_texture(scene, &omsi_texture::Image { width: 4, height: 4, rgba: [rgb[0], rgb[1], rgb[2], 255].repeat(16), has_alpha: false }, false);
    let textures = [sky([120, 140, 170]), sky([60, 60, 80]), sky([14, 16, 22])];
    renderer.set_sky_textures(scene, textures);
}

/// A crooked channel from the cloud base to the ground at (x, y).
fn channel(x: f64, y: f64) -> Vec<Bolt> {
    let n = 40;
    let p = |k: usize| {
        let f = k as f64 / n as f64;
        DVec3::new(x + (f * 37.0).sin() * 12.0, y + (f * 23.0).cos() * 8.0, 900.0 * (1.0 - f))
    };
    (0..n).map(|k| Bolt { a: p(k), b: p(k + 1), glow: 1.0, width: 1.0 }).collect()
}

fn night(classic: bool, enhanced: bool) -> Lighting {
    Lighting {
        shadows: false,
        classic,
        enhanced,
        night: 1.0,
        sun_dir: Vec3::new(0.3, 0.2, -0.5).normalize(),
        sun_intensity: 0.0,
        sun_color: Vec3::ZERO,
        ambient: Vec3::splat(0.04),
        secondary: Vec3::ZERO,
        fog_color: Vec3::splat(0.05),
        fog_density: 0.00015,
        sky_weights: [0.0, 0.0, 1.0],
        overcast: 1.0,
        cloud_density: 1.0,
        rain: 0.6,
        ..Default::default()
    }
}

const MODES: [(bool, bool); 3] = [(true, false), (false, false), (false, true)];

fn luma(rgba: &[u8], w: usize, x: usize, y: usize) -> f32 {
    let o = (y * w + x) * 4;
    0.2126 * rgba[o] as f32 + 0.7152 * rgba[o + 1] as f32 + 0.0722 * rgba[o + 2] as f32
}

/// A strike's channel is drawn over the sky kilometres off - beyond the far plane - as a
/// line a pixel or two wide, and hidden by what stands in front of it, in every graphics
/// mode; nothing is drawn without one.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_lightning_channel_is_drawn_over_the_sky_and_hidden_by_a_wall() {
    let mut renderer = lightning_renderer();
    let mut scene = renderer.new_scene();
    street(&renderer, &mut scene);
    let camera = Camera { position: DVec3::new(0.0, 0.0, 1.7), yaw: 0.0, pitch: 12.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 1000.0 };
    let (w, h) = (192usize, 128usize);
    for (classic, enhanced) in MODES {
        let lighting = night(classic, enhanced);
        let mode = format!("classic {classic}, enhanced {enhanced}");
        scene.bolts.clear();
        let dark = renderer.render_to_image(&mut scene, w as u32, h as u32, &camera, &lighting).unwrap();
        // one 2.5 km ahead; and alone, one as far behind the wall (with the one ahead, the
        // enhanced picture's glow round it reaches the wall)
        scene.bolts = channel(-1300.0, 2500.0);
        let hidden = renderer.render_to_image(&mut scene, w as u32, h as u32, &camera, &lighting).unwrap();
        scene.bolts = channel(0.0, 2500.0);
        let lit = renderer.render_to_image(&mut scene, w as u32, h as u32, &camera, &lighting).unwrap();
        // the rows over the horizon (12 degrees up: the horizon is at row 64 + 21)
        let most = |x0: usize, x1: usize, y0: usize, y1: usize| (y0..y1).map(|y| (x0..x1).map(|x| luma(&lit, w, x, y) - luma(&dark, w, x, y)).fold(0.0f32, f32::max)).fold(0.0f32, f32::max);
        let ahead = most(88, 104, 20, 70);
        // a few pixels wide: 8 pixels to its side the glow round the core is faint
        let beside = most(80, 81, 20, 70);
        // hidden by the wall (27 degrees left, behind the wall's 5 to 40): nothing of it
        let behind = (0..h).map(|y| (0..w).map(|x| (luma(&hidden, w, x, y) - luma(&dark, w, x, y)).abs()).fold(0.0f32, f32::max)).fold(0.0f32, f32::max);
        eprintln!("{mode}: ahead {ahead}, beside {beside}, behind the wall {behind}");
        let peak = (20..70).map(|y| (88..104).map(|x| luma(&lit, w, x, y)).fold(0.0f32, f32::max)).fold(0.0f32, f32::max);
        assert!(ahead > 25.0 && peak > 235.0, "the channel ahead is drawn white ({mode}): {peak}, {ahead} over the sky");
        assert!(beside < ahead * 0.5, "the channel's glow stays narrow ({mode}): {beside} beside, {ahead} on it");
        assert!(behind < 3.0, "the channel behind the wall shows ({mode}): {behind}");
    }
}

/// A flash lights the night street (the floor at the foot of the picture) and the clouds
/// round where it is, more than the far side of the sky, in every graphics mode.
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn a_flash_lights_the_street_and_the_clouds() {
    let mut renderer = lightning_renderer();
    let mut scene = renderer.new_scene();
    street(&renderer, &mut scene);
    let camera = Camera { position: DVec3::new(0.0, 0.0, 1.7), yaw: 0.0, pitch: 5.0, roll: 0.0, fov_deg: 70.0, near: 0.1, far: 1000.0 };
    let (w, h) = (128usize, 96usize);
    for (classic, enhanced) in MODES {
        let mode = format!("classic {classic}, enhanced {enhanced}");
        let mut lighting = night(classic, enhanced);
        let dark = renderer.render_to_image(&mut scene, w as u32, h as u32, &camera, &lighting).unwrap();
        // a strike 1.5 km ahead and to the right (for the enhanced picture's eye, taken to
        // a moonless night without a lamp, one some 8 km off: 1.5 km off took it all white)
        let (ground, sky) = if enhanced { (0.0002, 0.0004) } else { (0.004, 0.012) };
        lighting.lightning = Some(Flash { dir: Vec3::new(0.6, 0.75, 0.3).normalize(), ground, sky, spread: 0.6 });
        let lit = renderer.render_to_image(&mut scene, w as u32, h as u32, &camera, &lighting).unwrap();
        let floor = |img: &[u8]| (88..94).map(|y| luma(img, w, 64, y)).sum::<f32>() / 6.0;
        let sky = |img: &[u8], x: usize| (4..14).map(|y| luma(img, w, x, y)).sum::<f32>() / 10.0;
        eprintln!("{mode}: floor {} -> {}, sky towards {} -> {}, across {} -> {}", floor(&dark), floor(&lit), sky(&dark, 120), sky(&lit, 120), sky(&dark, 8), sky(&lit, 8));
        assert!(floor(&lit) > floor(&dark) + 20.0, "the street lit ({mode}): {} -> {}", floor(&dark), floor(&lit));
        let towards = sky(&lit, 120) - sky(&dark, 120);
        let away = sky(&lit, 8) - sky(&dark, 8);
        assert!(towards > 15.0 && towards > away * 1.3, "the clouds glow towards the flash ({mode}): {towards} there, {away} across the sky");
    }
}

/// Pictures of a strike at night in each graphics mode, for looking at:
/// `OMSI_LIGHTNING_PICTURES=<folder> cargo test -p omsi-render lightning_pictures -- --ignored`
/// writes `lightning-<mode>-<dark|flash>.ppm` there.
#[test]
#[ignore = "requires a graphics adapter; writes pictures with OMSI_LIGHTNING_PICTURES"]
fn lightning_pictures() {
    let Some(dir) = std::env::var_os("OMSI_LIGHTNING_PICTURES") else { return };
    let dir = std::path::PathBuf::from(dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut renderer = lightning_renderer();
    let mut scene = renderer.new_scene();
    street(&renderer, &mut scene);
    let camera = Camera { position: DVec3::new(6.0, -4.0, 1.7), yaw: 0.0, pitch: 10.0, roll: 0.0, fov_deg: 65.0, near: 0.1, far: 1000.0 };
    let (w, h) = (640u32, 360u32);
    for (classic, enhanced) in MODES {
        let name = if enhanced { "enhanced" } else if classic { "classic" } else { "vanilla-plus" };
        let mut lighting = night(classic, enhanced);
        for flash in [false, true] {
            scene.bolts.clear();
            lighting.lightning = None;
            if flash {
                scene.bolts = channel(250.0, 1800.0);
                // and a branch
                let from = scene.bolts[12].a;
                let to = from + DVec3::new(160.0, -60.0, -240.0);
                scene.bolts.extend((0..12).map(|k| {
                    let p = |k: usize| from.lerp(to, k as f64 / 12.0) + DVec3::new((k as f64 * 2.1).sin() * 9.0, 0.0, (k as f64 * 1.7).cos() * 6.0);
                    Bolt { a: p(k), b: p(k + 1), glow: 0.6, width: 0.6 }
                }));
                lighting.lightning = Some(Flash { dir: DVec3::new(250.0, 1800.0, 700.0).normalize().as_vec3(), ground: 0.006, sky: 0.015, spread: 0.45 });
            }
            let rgba = renderer.render_to_image(&mut scene, w, h, &camera, &lighting).unwrap();
            let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
            for px in rgba.chunks(4) {
                ppm.extend_from_slice(&px[..3]);
            }
            std::fs::write(dir.join(format!("lightning-{name}-{}.ppm", if flash { "flash" } else { "dark" })), ppm).unwrap();
        }
    }
}
