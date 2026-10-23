use super::*;

fn renderer() -> Renderer {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    pollster::block_on(Renderer::new_with(
        &instance,
        None,
        Some(wgpu::TextureFormat::Rgba8UnormSrgb),
        RenderOptions { msaa: 1, ssao: false, shadow_size: 1024, fxaa: false, render_scale: 1.0, ..Default::default() },
    ))
    .expect("test renderer")
}

/// Broken cloud on a summer's day in the enhanced picture, the clouds drifted `drift` m east.
fn day(drift: f32) -> Lighting {
    Lighting {
        shadows: false,
        enhanced: true,
        sun_dir: Vec3::new(-0.5, 0.3, 0.8).normalize(),
        sun_intensity: 1.0,
        fog_density: 0.0001,
        cloud_density: 0.5,
        cloud_offset: [drift / 2500.0, 0.0],
        ..Default::default()
    }
}

/// A window's frame (the sky cube a slice at a time, as in the game), read back.
fn frame(renderer: &mut Renderer, scene: &mut Scene, tex: &wgpu::Texture, camera: &Camera, lighting: &Lighting) -> Vec<u8> {
    let view = tex.create_view(&Default::default());
    renderer.render(scene, &view, tex.width(), tex.height(), camera, lighting);
    renderer.read_texture(tex, wgpu::TextureAspect::All).expect("read back")
}

/// The enhanced clouds move on with the wind in every frame, though the sky cube they are
/// kept in is redrawn only a slice of a face a frame: it holds them in their own frame,
/// which the sky shows drifted on. (In the ground's frame each part of the sky took the
/// clouds' new place only when its slice was redrawn - every 24th frame, a fifth of it
/// blended in - and in the frames between, the clouds stood still: they hopped.)
#[test]
#[ignore = "requires a graphics adapter; run with --ignored on a GPU host"]
fn the_clouds_glide_with_the_wind_in_every_frame() {
    let mut renderer = renderer();
    let mut scene = renderer.new_scene();
    // (the sky's textures: without them no sky is drawn but the clear colour)
    let mut tex = |rgb: [u8; 3]| renderer.add_texture(&mut scene, &omsi_texture::Image { width: 4, height: 4, rgba: [rgb[0], rgb[1], rgb[2], 255].repeat(16), has_alpha: false }, false);
    let textures = [tex([120, 150, 200]), tex([200, 140, 100]), tex([10, 12, 20])];
    let field = tex([128, 128, 128]);
    renderer.set_sky_textures_clouds(&mut scene, textures, Some(field));
    // looking up into the clouds, 1.4 km over the street
    let camera = Camera { position: DVec3::new(0.0, 0.0, 2.0), yaw: 0.0, pitch: 60.0, roll: 0.0, fov_deg: 60.0, near: 0.1, far: 1000.0 };
    let (w, h) = (160u32, 120u32);
    // the cube drawn whole for a start
    renderer.render_to_image(&mut scene, w, h, &camera, &day(0.0)).unwrap();
    let tex = renderer.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("window"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: renderer.format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    // (a few frames for the exposure to settle as a window's does)
    let mut last = Vec::new();
    for _ in 0..4 {
        last = frame(&mut renderer, &mut scene, &tex, &camera, &day(0.0));
    }
    // 8 m of drift a frame (a breeze of 8 m/s at one frame a second: a few pixels here)
    let mut moved = Vec::new();
    for k in 1..=12 {
        let now = frame(&mut renderer, &mut scene, &tex, &camera, &day(k as f32 * 8.0));
        let changed = now.chunks(4).zip(last.chunks(4)).filter(|(a, b)| (0..3).map(|c| (a[c] as i32 - b[c] as i32).abs()).sum::<i32>() >= 6).count();
        moved.push(changed as f32 / (w * h) as f32);
        if let Some(dir) = std::env::var_os("OMSI_CLOUD_PICTURES") {
            let mut ppm = format!("P6\n{w} {h}\n255\n").into_bytes();
            for px in now.chunks(4) {
                ppm.extend_from_slice(&px[..3]);
            }
            std::fs::write(std::path::Path::new(&dir).join(format!("clouds-{k:02}.ppm")), ppm).unwrap();
        }
        last = now;
    }
    eprintln!("share of the picture changed frame to frame: {moved:.2?}");
    assert!(moved.iter().all(|m| *m > 0.2), "the clouds move on in every frame: {moved:.2?}");
}
