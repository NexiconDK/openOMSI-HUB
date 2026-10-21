//! Pictures of the pause menu with the game's own lists open in the sheet beside the panel
//! (Omsi-Hub's menu, `shell::hub`), drawn as the game draws them, for a look by eye: they need
//! a graphics adapter and are left out unless asked for -
//! `cargo test -p omsi-app --lib shell::preview -- --ignored`, written beside
//! `OMSI_UI_PREVIEW` (its folder) or into `target/`.

use crate::game_lists::ListKind;
use omsi_ui::{Draw, Gpu};

/// The menu of `app` as it is now, in a window of `w` x `h`, written to `name` over a stand-in
/// for the street.
fn picture(app: &mut crate::App, name: &str, (w, h): (u32, u32)) {
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("device");
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let mut gpu = Gpu::new(&device, format, 1, app.shell.ui.atlas.size);
    // (the wordmark on the card first: the menu draws it where the game has it)
    let key = omsihub_game::pause_menu::wordmark_key(1.0);
    let (lw, lh, px) = omsihub_game::pause_menu::wordmark_pixels(key).expect("wordmark");
    app.shell.logo_tex = Some(gpu.add_image(&device, &queue, lw, lh, &px));
    // (twice: the eases settle)
    for _ in 0..2 {
        app.draw_pause(w as f32, h as f32, 1.0, 1.0);
    }
    render(&mut app.shell, &mut gpu, &device, &queue, name, (w, h));
}

/// What `sh` drew last, written to `name` over a stand-in for the street.
fn render(sh: &mut super::Shell, gpu: &mut Gpu, device: &wgpu::Device, queue: &wgpu::Queue, name: &str, (w, h): (u32, u32)) {
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let d = sh.drawn.as_ref().expect("drawn");
    sh.ui.atlas.mark_all_dirty();
    gpu.upload(device, queue, 0, &d.verts);
    gpu.upload_atlas(queue, &mut sh.ui.atlas);
    let draws: Vec<Draw> = d.ranges.iter().enumerate().map(|(k, (range, tex))| Draw { buffer: 0, range: range.clone(), layer: k, texture: *tex }).collect();
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = target.create_view(&Default::default());
    let mut enc = device.create_command_encoder(&Default::default());
    gpu.render(device, queue, &mut enc, &view, (w, h), Some(wgpu::Color::TRANSPARENT), &d.layers, &draws);
    // read back, laid over sky, houses and the street
    let stride = (w * 4).div_ceil(256) * 256;
    let buf = device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (stride * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
    enc.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo { buffer: &buf, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: None } },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([enc.finish()]);
    buf.slice(..).map_async(wgpu::MapMode::Read, |_| {});
    device.poll(wgpu::PollType::wait_indefinitely()).ok();
    let data = buf.slice(..).get_mapped_range();
    let img = image::RgbaImage::from_fn(w, h, |x, y| {
        let t = y as f32 / h as f32;
        let bg: [f32; 3] = if t < 0.42 { [96.0 + 60.0 * t, 112.0 + 50.0 * t, 128.0] } else if t < 0.62 { [178.0, 140.0, 64.0] } else { [74.0, 72.0, 68.0] };
        let i = (y * stride + x * 4) as usize;
        let a = data[i + 3] as f32 / 255.0;
        let px = |c: usize| (data[i + c] as f32 + bg[c] * (1.0 - a)).min(255.0) as u8;
        image::Rgba([px(0), px(1), px(2), 255])
    });
    let dir = omsi_cfg::flags::OMSI_UI_PREVIEW
        .live_var()
        .ok()
        .and_then(|p| std::path::PathBuf::from(p).parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    std::fs::create_dir_all(&dir).ok();
    img.save(dir.join(name)).unwrap();
    println!("wrote {}", dir.join(name).display());
}

#[test]
#[ignore]
fn preview() {
    let args = <crate::cli::Args as clap::Parser>::parse_from(["openomsi"]);
    let mut app = crate::assemble_app(args, Default::default());
    app.open_game_menu();
    let camera = crate::game_lists::tab_of(&app, ListKind::Options(0), "Camera");
    for (name, kind) in [
        ("pause-options.png", ListKind::Options(0)),
        ("pause-camera.png", ListKind::Options(camera)),
        ("pause-controls.png", ListKind::Controls),
        ("pause-vehicle.png", ListKind::Vehicle(0)),
        ("pause-world.png", ListKind::World(0)),
        ("pause-lines.png", ListKind::Lines),
    ] {
        app.open_list(kind);
        picture(&mut app, name, (1920, 1080));
        app.close_list();
    }
}

/// Omsi-Hub: a tour's page (after openOMSI's roadbook, #2936) with made-up tours, trips, IBIS
/// routes and a route map, drawn as the menu draws it.
#[test]
#[ignore]
fn tour_page() {
    use crate::ui::{IbisBox, MenuKind, Preview, RouteMap, TripEnds, TripRow};
    use glam::Vec2;
    let ends = |a: &str, d: &str, b: &str, r: &str| TripEnds { from: a.into(), dep: d.into(), to: b.into(), arr: r.into() };
    let trips = vec![
        TripRow { line: "131".into(), ends: ends("Zeche Hindenburg", "20:08", "Kursker Allee", "20:52"), gap: "6 min break".into() },
        TripRow { line: "131".into(), ends: ends("Kursker Allee", "20:58", "Zeche Hindenburg", "21:42"), gap: "26 min break".into() },
        TripRow { line: "131".into(), ends: ends("Zeche Hindenburg", "22:08", "Kursker Allee", "22:52"), gap: "8 min break".into() },
        TripRow { line: "N31".into(), ends: ends("Kursker Allee", "23:00", "Betriebshof", "23:20"), gap: String::new() },
    ];
    let names = ["Zeche Hindenburg", "Amelatzestr.", "Hindenburgstr.", "Rathaus Sternenberg", "Sternenberger Markt", "Trautmannsfeld", "Viermännerhöfe", "Lidersbaum S", "Kursker Allee"];
    let rows: Vec<(String, String)> = names.iter().enumerate().map(|(i, n)| (n.to_string(), format!("22:{:02}", 8 + i * 5))).collect();
    let way: Vec<Vec2> = (0..=60).map(|k| { let t = k as f32 / 60.0; Vec2::new(t * 3000.0, (t * 7.0).sin() * 500.0 + t * 800.0) }).collect();
    let stops = names.iter().enumerate().map(|(i, n)| (way[i * 60 / (names.len() - 1)], n.to_string(), format!("22:{:02}", 8 + i * 5), i)).collect();
    let mut roads = Vec::new();
    for k in 0..30 {
        let x = -400.0 + k as f32 * 130.0;
        roads.push(vec![Vec2::new(x, -900.0), Vec2::new(x + 80.0, 1900.0)]);
        roads.push(vec![Vec2::new(-500.0, -900.0 + k as f32 * 100.0), Vec2::new(3500.0, -800.0 + k as f32 * 100.0)]);
    }
    let map = RouteMap { roads, route: vec![way.clone()], stops, min: Vec2::new(0.0, -500.0), max: Vec2::new(3000.0, 1300.0) };
    let ibis = IbisBox {
        rows: vec![
            ("Zeche Hindenburg - Kursker Allee".into(), "131".into(), "02".into(), true),
            ("Kursker Allee - Zeche Hindenburg".into(), "131".into(), "01".into(), false),
            ("Zeche Hindenburg - Lidersbaum S".into(), "131".into(), "03".into(), false),
        ],
        note: "Depot file Ahlheim 5".into(),
    };
    let p = Preview {
        title: "Tour 131H12".into(),
        meta: "Line 131  |  Trip 3/4  |  Choose the stop to start from".into(),
        rows,
        chosen: Some(0),
        button: Some("Start trip".into()),
        time: Some("22:08".into()),
        ends: Some(ends("Zeche Hindenburg", "22:08", "Kursker Allee", "22:52")),
        trips,
        trip: 2,
        ibis: Some(ibis),
        map: Some(std::sync::Arc::new(map)),
    };
    let items: Vec<(&str, &str)> = vec![("tour 131\u{1}131H09", "Tour 131H09"), ("tour 131\u{1}131H10", "Tour 131H10"), ("tour 131\u{1}131H11", "Tour 131H11"), ("tour 131\u{1}131H12", "Tour 131H12"), ("back", "Back")];
    let list = super::pause::ListView { kind: MenuKind::Tours, head: Some(("Line and tour".into(), "Line 131".into())), tabs: None, items, sel: 3, preview: Some(&p), pane_first: None, dropdown: None, key: "tours131".into() };
    let v = super::pause::PauseView { paused: true, rail: &[], rail_sel: None, rail_open: None, list: None, kbd: false, keys: false, summary: Default::default() };
    let instance = wgpu::Instance::default();
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).expect("adapter");
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).expect("device");
    let mut sh = super::Shell::new();
    let mut gpu = Gpu::new(&device, wgpu::TextureFormat::Rgba8UnormSrgb, 1, sh.ui.atlas.size);
    for (name, w, h) in [("tour-page.png", 1600u32, 900u32), ("tour-page-narrow.png", 900, 700)] {
        for _ in 0..2 {
            sh.begin(w as f32, h as f32, 1.0, 1.0);
            sh.ui.p().rounded(omsi_ui::Rect::new(0.0, 0.0, w as f32, h as f32), 0.0, omsi_ui::theme::BG);
            super::pause::timetable_lists(&mut sh, &v, &list, omsi_ui::Rect::new(24.0, 24.0, w as f32 - 48.0, h as f32 - 48.0));
            sh.finish();
        }
        render(&mut sh, &mut gpu, &device, &queue, name, (w, h));
    }
}
