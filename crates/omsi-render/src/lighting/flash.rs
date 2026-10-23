//! Lightning in the picture: a flash's light (`Lighting::lightning`) on the street, in the
//! air and in the clouds - in the classic picture's terms or the enhanced picture's - and
//! the strikes' channels (`Scene::bolts`, drawn by bolt.wgsl).

use crate::*;

/// A part of a lightning channel, as the app hands it over every frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Bolt {
    /// Its ends (world).
    pub a: DVec3,
    pub b: DVec3,
    /// How bright it glows now (about 1 at a stroke of the main channel; 0 is dark).
    pub glow: f32,
    /// How wide (1 the main channel, less a branch).
    pub width: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct GpuBolt {
    /// xyz the ends (render-origin relative), a.w the glow, b.w the width.
    pub(crate) a: [f32; 4],
    pub(crate) b: [f32; 4],
}

/// The colour of a flash's light: a little bluer than daylight (the channel's nitrogen
/// and oxygen lines), its luminance about the light's.
pub(crate) const FLASH_RGB: Vec3 = Vec3::new(0.86, 0.92, 1.08);

/// How much a flash lifts the classic picture's light: its light on the ground (`ground`)
/// and the clouds' glow (`sky`), each taken as the eye sees a light - by its square root
/// (Stevens' brightness law): a near strike's some 300 lux turns a night street to day, a
/// far one's lux or two still flicker over it - and by night more than by day, when it is
/// lost in the daylight.
pub(crate) fn classic_flash(f: &Flash, night: f32) -> (f32, f32) {
    let by_night = 0.3 + 0.7 * night.clamp(0.0, 1.0);
    let ground = (f.ground.max(0.0) / 0.03).sqrt().min(1.6) * 0.55;
    let sky = (f.sky.max(0.0) / 0.02).sqrt().min(1.6) * 0.6;
    (ground * by_night, sky * by_night)
}

/// The flash as the eye takes it in the enhanced picture, drawn pre-exposed by `pre`: as
/// bright as it is up to about white, and over that more and more compressed (the eye's
/// response to a sudden light, `R = I / (I + s)`): a strike a kilometre or two off, some
/// hundred lux on a night street the eye has taken to the lamps' ten, or to a moonless
/// field's thousandth, lights it brightly with every surface still seen - taken as it is,
/// the picture went white from edge to edge. By day it is as it is.
pub(crate) fn flash_seen(f: &Flash, pre: f32) -> Flash {
    let white = 0.8 / std::f32::consts::PI * f.ground.max(0.0) * pre;
    let sky = f.sky.max(0.0) * pre;
    Flash { ground: f.ground.max(0.0) * (1.5 / (1.5 + white)), sky: f.sky.max(0.0) * (1.2 / (1.2 + sky)), ..*f }
}

/// The enhanced picture's light from the sky and the ground (`sh`, order-2 spherical
/// harmonics of the irradiance) with the flash's added: most of it from where it is, the
/// rest from all round - the cloud base and the rain it lights.
pub(crate) fn add_flash_light(sh: &mut [[f32; 4]; 9], f: &Flash) {
    let e = f.ground.max(0.0);
    if !(e > 0.0) {
        return;
    }
    let dir = f.dir.normalize_or_zero();
    let (towards, round) = (e * 0.65, e * 0.35);
    // (a light from one direction, irradiance E on a surface facing it: the coefficients
    // A_l E y_lm(dir), A = pi, 2pi/3, pi/4 by order - the cosine lobe's)
    let y = atmosphere::sh_basis(dir);
    let order_a = [
        std::f32::consts::PI,
        2.0 * std::f32::consts::PI / 3.0,
        2.0 * std::f32::consts::PI / 3.0,
        2.0 * std::f32::consts::PI / 3.0,
        std::f32::consts::PI / 4.0,
        std::f32::consts::PI / 4.0,
        std::f32::consts::PI / 4.0,
        std::f32::consts::PI / 4.0,
        std::f32::consts::PI / 4.0,
    ];
    for k in 0..9 {
        let mut c = order_a[k] * towards * y[k];
        if k == 0 {
            // (the same irradiance whichever way a surface faces)
            c += round / 0.282_095;
        }
        for (i, ch) in FLASH_RGB.to_array().iter().enumerate() {
            sh[k][i] += c * ch;
        }
    }
}

/// The enhanced picture's fog radiance with the flash's light in it: the light all round
/// it, scattered as the fog scatters its other light.
pub(crate) fn flash_fog(f: Option<&Flash>) -> Vec3 {
    f.map(|f| FLASH_RGB * f.ground.max(0.0) * 0.9 / std::f32::consts::PI).unwrap_or(Vec3::ZERO)
}

impl Renderer {
    /// Upload this frame's lightning channels (none between flashes).
    pub(crate) fn prepare_bolts(&self, scene: &mut Scene) {
        let ro = scene.render_origin;
        let data: Vec<GpuBolt> = scene
            .bolts
            .iter()
            .filter(|b| b.glow > 0.002 && b.a.is_finite() && b.b.is_finite())
            .map(|b| {
                let a = (b.a - ro).as_vec3();
                let e = (b.b - ro).as_vec3();
                GpuBolt { a: a.extend(b.glow).to_array(), b: e.extend(b.width.clamp(0.05, 2.0)).to_array() }
            })
            .collect();
        scene.bolt_count = data.len() as u32;
        if data.is_empty() {
            return;
        }
        let bytes: &[u8] = bytemuck::cast_slice(&data);
        match &scene.bolt_buf {
            Some(b) if b.size() as usize >= bytes.len() => self.queue.write_buffer(b, 0, bytes),
            _ => {
                let b = self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("lightning"),
                    size: (bytes.len() * 2) as u64,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                self.queue.write_buffer(&b, 0, bytes);
                scene.bolt_buf = Some(b);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The flash's light on a surface facing it is about its irradiance, on one facing away
    /// the share from all round; a classic picture's lift is more by night and less far off.
    #[test]
    fn a_flash_lights_what_faces_it() {
        let f = Flash { dir: Vec3::new(0.0, 0.6, 0.8), ground: 0.03, sky: 0.02, spread: 0.5 };
        let mut sh = [[0.0f32; 4]; 9];
        add_flash_light(&mut sh, &f);
        let coeffs: [Vec3; 9] = std::array::from_fn(|k| Vec3::new(sh[k][0], sh[k][1], sh[k][2]));
        let facing = atmosphere::sh_irradiance(&coeffs, f.dir.normalize());
        let away = atmosphere::sh_irradiance(&coeffs, -f.dir.normalize());
        let lum = |v: Vec3| v.dot(Vec3::new(0.2126, 0.7152, 0.0722));
        assert!((lum(facing) / 0.03 - 0.95).abs() < 0.2, "{facing:?}");
        assert!(lum(away) > 0.003 && lum(away) < 0.015, "{away:?}");
        let (near_night, _) = classic_flash(&f, 1.0);
        let (near_day, _) = classic_flash(&f, 0.0);
        let (far_night, _) = classic_flash(&Flash { ground: 0.0006, ..f }, 1.0);
        assert!(near_night > 0.5 && near_day < near_night * 0.4, "{near_night} {near_day}");
        assert!(far_night > 0.05 && far_night < near_night * 0.25, "{far_night}");
        assert_eq!(flash_fog(None), Vec3::ZERO);
        // by day as it is; by night a near one up to about white, a far one as it is
        let day = flash_seen(&f, 0.5);
        assert!((day.ground / f.ground) > 0.99 && (day.sky / f.sky) > 0.98);
        let night = flash_seen(&f, 900.0);
        let white = 0.8 / std::f32::consts::PI * night.ground * 900.0;
        assert!(white > 1.0 && white < 1.5, "{white}");
        let far = flash_seen(&Flash { ground: 0.0002, sky: 0.0004, ..f }, 900.0);
        assert!(far.ground > 0.0002 * 0.9);
    }
}
