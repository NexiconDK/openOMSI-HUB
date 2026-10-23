// Lightning: the channels of the strikes to the ground (omsi-weather lightning.rs), each
// part of a channel a ribbon facing the camera - a core of half a metre, or a pixel's
// angle far off, and a fainter glow round it six times as wide - drawn over the scene
// with the larger of the two kept (`MAX_BLEND`): the parts overlap at their kinks and
// where a far channel's glow is wider than its parts are long, and added up, the kinks
// would stand out as beads along it. Seen through what lies between: the classic
// picture's fog, the enhanced picture's air (`air_of`).
struct Camera {
    view_proj: mat4x4<f32>,
    cam_pos: vec4<f32>,
    // (kept in lockstep with `CameraUniform` in lib.rs, as sky.wgsl's)
    world_origin: vec4<f32>,
    sun_dir: vec4<f32>,
    ambient: vec4<f32>,
    fog: vec4<f32>,
    sun_color: vec4<f32>,
    sky_color: vec4<f32>,
    light_grid: vec4<f32>,
    sky: vec4<f32>,
    cam_right: vec4<f32>,
    cam_up: vec4<f32>,
    clouds: vec4<f32>,
    light_view_proj: mat4x4<f32>,
    light_view_proj_far: mat4x4<f32>,
    shadow: vec4<f32>,
    post: vec4<f32>,
    inside_a: vec4<f32>,
    inside_b: vec4<f32>,
    inside_c: vec4<f32>,
    flags: vec4<f32>,
    light_view_proj_close: mat4x4<f32>,
    wind: vec4<f32>,
    lamp_view_proj: array<mat4x4<f32>, 4>,
    lamp_shadow: vec4<f32>,
    tree_wind: vec4<f32>,
    inside2_a: vec4<f32>,
    inside2_b: vec4<f32>,
    inside2_c: vec4<f32>,
    cookies: array<vec4<f32>, 8>,
};
@group(0) @binding(0) var<uniform> camera: Camera;

struct BoltIn {
    // xyz the ends (render-origin relative), a.w how bright the part glows now, b.w how
    // wide (1 the main channel)
    @location(0) a: vec4<f32>,
    @location(1) b: vec4<f32>,
};

struct BoltOut {
    @builtin(position) clip: vec4<f32>,
    // across the ribbon (-1 .. 1), the core's share of its half width
    @location(0) across: f32,
    @location(1) core: f32,
    @location(2) glow: f32,
    @location(3) pos: vec3<f32>,
};

// The colour of the channel's glow round its white core.
const BOLT_TINT = vec3<f32>(0.72, 0.78, 1.0);

@vertex
fn vs_bolt(@builtin(vertex_index) vid: u32, inst: BoltIn) -> BoltOut {
    let ends = array<f32, 6>(0.0, 1.0, 1.0, 0.0, 1.0, 0.0);
    let sides = array<f32, 6>(-1.0, -1.0, 1.0, -1.0, 1.0, 1.0);
    let e = ends[vid % 6u];
    let s = sides[vid % 6u];
    let along = inst.b.xyz - inst.a.xyz;
    let p = inst.a.xyz + along * e;
    let to_cam = camera.cam_pos.xyz - p;
    let dist = max(length(to_cam), 1.0);
    var side = cross(along, to_cam);
    side = select(vec3<f32>(1.0, 0.0, 0.0), normalize(side), length(side) > 1e-6);
    // (lengthened by its glow's width at both ends: the parts of a crooked channel meet)
    let core = max(0.5 * inst.b.w, dist * 0.0011 * (0.6 + 0.4 * inst.b.w));
    let half = core * 6.0;
    let reach = normalize(along) * min(core, length(along) * 0.5) * (e * 2.0 - 1.0);
    var out: BoltOut;
    out.clip = camera.view_proj * vec4<f32>(p + reach + side * (s * half), 1.0);
    // (a channel kilometres off lies beyond the far plane: kept at its far end - the depth
    // is reversed - behind everything drawn)
    out.clip.z = max(out.clip.z, 1e-7 * out.clip.w);
    out.across = s;
    out.core = core / half;
    out.glow = inst.a.w;
    out.pos = p;
    return out;
}

// The light across the ribbon: the core (x) and the glow round it (y).
fn bolt_profile(in: BoltOut) -> vec2<f32> {
    let x = abs(in.across);
    let c = x / max(in.core, 1e-3);
    return vec2<f32>(exp(-c * c * 1.6), 0.15 * exp(-x * x * 5.0) * (1.0 - x));
}

// the classic picture: as bright as the picture goes, through the fog as the lamps are
@fragment
fn fs_bolt(in: BoltOut) -> @location(0) vec4<f32> {
    let dist = length(camera.cam_pos.xyz - in.pos);
    let t = exp(-dist * max(camera.fog.w, 0.0) * 0.5);
    let p = bolt_profile(in) * (in.glow * t * 4.0);
    return vec4<f32>(min(vec3<f32>(p.x) + BOLT_TINT * p.y, vec3<f32>(1.0)), 1.0);
}

// the enhanced picture: a radiance far over the scene's - white in any light - through the
// air (`air_of`; what the air scatters towards the eye is the sky's and the fog's, there
// already); the core only a little over white, as the sun's disc (sky_enhanced.wgsl): the
// picture's glow spreads from it, and from tens of thousands it took the whole picture
// white
const BOLT_RADIANCE: f32 = 60.0;

@fragment
fn fs_bolt_enhanced(in: BoltOut) -> @location(0) vec4<f32> {
    let to = in.pos - camera.cam_pos.xyz;
    let dist = length(to);
    let h0 = camera.cam_pos.z - enh.fog.z;
    let h1 = in.pos.z - enh.fog.z;
    let tau = layer_depth(enh.fog.x, enh.fog.y, h0, h1, dist) + layer_depth(enh.fog.w, 1.0 / 1500.0, h0, h1, dist);
    let p = bolt_profile(in) * (in.glow * exp(-tau) * BOLT_RADIANCE * enh.exposure.x);
    return vec4<f32>(vec3<f32>(min(p.x, 24.0)) + BOLT_TINT * min(p.y, 1.5), 1.0);
}
