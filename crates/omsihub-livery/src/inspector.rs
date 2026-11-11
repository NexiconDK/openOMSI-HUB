//! (NexiconDK, ticket inspection round U, 11 November 2026) The ticket inspectors' clothes in the
//! game: a passenger figure's own texture repainted into an inspector's - look A (a navy uniform
//! and a neon-yellow vest with reflective bands, BILLETKONTROL on the back), B (the figure's own
//! clothes and an ID card on a lanyard) or C (the company's uniform: a dark jacket with a band and
//! cuffs in the company's colour, its name and BILLETKONTROL on the back, the cap in its colours).
//!
//! How: every triangle of the figure's mesh is rasterised into texture space (`Body`), so that each
//! texel knows the point of the body it covers (x, y, z in the figure's frame - OMSI's: y up, +z
//! the figure's front, +x its left) and the body part (by its vertices' strongest bone). The
//! colours are chosen per part and height keeping the original's folds (`shade`: the texel's
//! brightness against its part's median, half of it blurred); texts and badges are decals placed
//! on the BODY (metres on its front or back), so they read right whatever the UV layout.
//!
//! `dress_figure` makes a figure's clothes as the game takes them: a DDS beside its own textures
//! and a `.cti` item `~Inspector_<look>_<company>` - in openOMSI's content folder's copy of the
//! figure's `[CTC]` folder, never in the OMSI 2 installation (`export::inside`), and made again
//! only when what they show changed (`Dress::key`, the `.cti`'s first line).

use super::shapes::{text_raster, DEFAULT_FONT};
use glam::{Vec2, Vec3};
use resvg::tiny_skia::{self, FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};
use std::path::{Path, PathBuf};

/// The body parts (by the bones of aXYZ's figures, which most of OMSI's people share).
pub const NONE: u8 = 0;
pub const TORSO: u8 = 1;
pub const ARM: u8 = 2;
pub const HAND: u8 = 3;
pub const LEG: u8 = 4;
pub const HEAD: u8 = 5;

/// The part a bone moves.
pub fn part_of(bone: &str) -> u8 {
    match bone.trim() {
        b if b.eq_ignore_ascii_case("main") || b.eq_ignore_ascii_case("hip") => TORSO,
        b if ["oa_l", "oa_r", "ua_l", "ua_r"].iter().any(|x| b.eq_ignore_ascii_case(x)) => ARM,
        b if b.eq_ignore_ascii_case("hand_l") || b.eq_ignore_ascii_case("hand_r") => HAND,
        b if ["os_l", "os_r", "us_l", "us_r"].iter().any(|x| b.eq_ignore_ascii_case(x)) => LEG,
        b if b.eq_ignore_ascii_case("head") => HEAD,
        _ => NONE,
    }
}

/// The triangles of a mesh that show one texture: the vertices' positions, normals, UVs and
/// parts, and the triangles.
pub struct MeshPart<'a> {
    pub pos: &'a [Vec3],
    pub nor: &'a [Vec3],
    pub uv: &'a [Vec2],
    pub parts: &'a [u8],
    pub tris: Vec<[u32; 3]>,
}

/// The texture's texels as points of the body.
pub struct Body {
    pub w: u32,
    pub h: u32,
    p: Vec<Vec3>,
    /// The normal's z (+: facing the figure's front).
    n: Vec<f32>,
    part: Vec<u8>,
    cover: Vec<bool>,
}

impl Body {
    pub fn of(meshes: &[MeshPart], w: u32, h: u32) -> Body {
        let n = (w * h) as usize;
        let mut b = Body { w, h, p: vec![Vec3::ZERO; n], n: vec![0.0; n], part: vec![NONE; n], cover: vec![false; n] };
        let (wf, hf) = (w as f32, h as f32);
        for m in meshes {
            let px = |i: u32| m.uv.get(i as usize).map_or(Vec2::ZERO, |uv| Vec2::new(uv.x * wf - 0.5, (1.0 - uv.y.abs()) * hf - 0.5));
            for t in &m.tris {
                if t.iter().any(|&i| i as usize >= m.pos.len()) {
                    continue;
                }
                let (a, bb, c) = (px(t[0]), px(t[1]), px(t[2]));
                let lo = a.min(bb).min(c).floor();
                let hi = a.max(bb).max(c).ceil();
                // (a triangle stretched over much of the texture is no island's: an inner face or
                // a lining borrowing a corner of it - it would paint the wrong body point there)
                if hi.x - lo.x > wf * 0.3 || hi.y - lo.y > hf * 0.3 {
                    continue;
                }
                let (x0, y0) = (lo.x.max(0.0) as i64, lo.y.max(0.0) as i64);
                let (x1, y1) = ((hi.x as i64).min(w as i64 - 1), (hi.y as i64).min(h as i64 - 1));
                if x1 < x0 || y1 < y0 {
                    continue;
                }
                let den = (bb.y - c.y) * (a.x - c.x) + (c.x - bb.x) * (a.y - c.y);
                if den.abs() < 1e-9 {
                    continue;
                }
                let ps = [m.pos[t[0] as usize], m.pos[t[1] as usize], m.pos[t[2] as usize]];
                let nz = [m.nor.get(t[0] as usize).map_or(0.0, |v| v.z), m.nor.get(t[1] as usize).map_or(0.0, |v| v.z), m.nor.get(t[2] as usize).map_or(0.0, |v| v.z)];
                // (the part most of its vertices are of; the lower one where they differ)
                let vp = [m.parts.get(t[0] as usize).copied().unwrap_or(NONE), m.parts.get(t[1] as usize).copied().unwrap_or(NONE), m.parts.get(t[2] as usize).copied().unwrap_or(NONE)];
                let part = (0..=HEAD).max_by_key(|p| (vp.iter().filter(|x| *x == p).count(), std::cmp::Reverse(*p))).unwrap_or(NONE);
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        let (gx, gy) = (x as f32, y as f32);
                        let l1 = ((bb.y - c.y) * (gx - c.x) + (c.x - bb.x) * (gy - c.y)) / den;
                        let l2 = ((c.y - a.y) * (gx - c.x) + (a.x - c.x) * (gy - c.y)) / den;
                        let l3 = 1.0 - l1 - l2;
                        if l1 < -0.02 || l2 < -0.02 || l3 < -0.02 {
                            continue;
                        }
                        let i = (y as u32 * w + x as u32) as usize;
                        b.p[i] = ps[0] * l1 + ps[1] * l2 + ps[2] * l3;
                        b.n[i] = nz[0] * l1 + nz[1] * l2 + nz[2] * l3;
                        b.part[i] = part;
                        b.cover[i] = true;
                    }
                }
            }
        }
        // (the islands' edges a texel or two wider, so that no seam shows the old colour)
        for _ in 0..3 {
            for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                let was = b.cover.clone();
                for y in 0..h as i64 {
                    for x in 0..w as i64 {
                        let i = (y * w as i64 + x) as usize;
                        if was[i] {
                            continue;
                        }
                        let (sx, sy) = (x - dx, y - dy);
                        if sx < 0 || sy < 0 || sx >= w as i64 || sy >= h as i64 {
                            continue;
                        }
                        let s = (sy * w as i64 + sx) as usize;
                        if was[s] {
                            b.p[i] = b.p[s];
                            b.n[i] = b.n[s];
                            b.part[i] = b.part[s];
                            b.cover[i] = true;
                        }
                    }
                }
            }
        }
        b
    }

    /// Whether any texel shows part `part`.
    pub fn has(&self, part: u8) -> bool {
        self.cover.iter().zip(&self.part).any(|(c, p)| *c && *p == part)
    }

    /// The highest point of part `part` (None: no texel shows it).
    pub fn top_of(&self, part: u8) -> Option<f32> {
        self.cover.iter().zip(&self.part).zip(&self.p).filter(|((c, p), _)| **c && **p == part).map(|(_, v)| v.y).reduce(f32::max)
    }

    fn mask(&self, f: impl Fn(usize) -> bool) -> Vec<bool> {
        (0..self.p.len()).map(|i| self.cover[i] && f(i)).collect()
    }
}

/// An RGBA picture in floats (0..255).
pub struct Pic {
    pub w: u32,
    pub h: u32,
    pub px: Vec<[f32; 4]>,
}

impl Pic {
    fn new(w: u32, h: u32) -> Pic {
        Pic { w: w.max(1), h: h.max(1), px: vec![[0.0; 4]; (w.max(1) * h.max(1)) as usize] }
    }

    fn of_pixmap(p: &Pixmap) -> Pic {
        let px = p
            .pixels()
            .iter()
            .map(|c| {
                let c = c.demultiply();
                [c.red() as f32, c.green() as f32, c.blue() as f32, c.alpha() as f32]
            })
            .collect();
        Pic { w: p.width(), h: p.height(), px }
    }

    /// `top` over this one at (x, y), source-over.
    fn over(&mut self, top: &Pic, ox: i64, oy: i64) {
        for y in 0..top.h as i64 {
            for x in 0..top.w as i64 {
                let (tx, ty) = (ox + x, oy + y);
                if tx < 0 || ty < 0 || tx >= self.w as i64 || ty >= self.h as i64 {
                    continue;
                }
                let s = top.px[(y * top.w as i64 + x) as usize];
                let d = &mut self.px[(ty * self.w as i64 + tx) as usize];
                let a = s[3] / 255.0;
                let da = d[3] / 255.0;
                let oa = a + da * (1.0 - a);
                for k in 0..3 {
                    d[k] = if oa > 0.0 { (s[k] * a + d[k] * da * (1.0 - a)) / oa } else { 0.0 };
                }
                d[3] = oa * 255.0;
            }
        }
    }
}

/// `lines` (text, relative size) centred in a w x h metre picture, `scale` pixels a metre.
pub fn text_picture(lines: &[(&str, f32)], w: f32, h: f32, colour: [u8; 4], scale: f32) -> Pic {
    let (pw, ph) = ((w * scale) as u32, (h * scale) as u32);
    let mut pic = Pic::new(pw, ph);
    let total: f32 = lines.iter().map(|l| l.1).sum::<f32>().max(1e-6);
    let mut y = 0.0;
    for (text, s) in lines {
        let lh = ph as f32 * s / total;
        // (the letters' height: about four fifths of the line, narrower where it is too wide)
        let mut em = (lh * 0.82).max(4.0);
        let mut r = text_raster(text, DEFAULT_FONT, 0.0, em, 0.0).0;
        if r.w as f32 > pw as f32 * 0.96 {
            em = (em * pw as f32 * 0.96 / r.w as f32).max(3.0);
            r = text_raster(text, DEFAULT_FONT, 0.0, em, 0.0).0;
        }
        let mut t = Pic::new(r.w, r.h);
        for (i, a) in r.data.iter().enumerate().take(t.px.len()) {
            t.px[i] = [colour[0] as f32, colour[1] as f32, colour[2] as f32, *a as f32 * colour[3] as f32 / 255.0];
        }
        let ox = ((pw as f32 - r.w as f32) / 2.0).round() as i64;
        let oy = (y + (lh - r.h as f32) / 2.0).round() as i64;
        pic.over(&t, ox, oy);
        y += lh;
    }
    pic
}

fn paint_of(c: [u8; 3]) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color_rgba8(c[0], c[1], c[2], 255);
    p.anti_alias = true;
    p
}

/// An ID card on its lanyard (w x h metres): the cord from both sides of the neck, the card, a
/// photo, lines of text.
pub fn id_card(w: f32, h: f32, accent: [u8; 3]) -> Pic {
    let (pw, ph) = ((w * 2000.0) as u32, (h * 2000.0) as u32);
    let Some(mut pm) = Pixmap::new(pw.max(1), ph.max(1)) else { return Pic::new(1, 1) };
    let (fw, fh) = (pw as f32, ph as f32);
    let (cw, ch) = (fw * 0.42, fh * 0.36);
    let (cx0, cy0) = ((fw - cw) / 2.0, fh - ch);
    let cord = Stroke { width: (fw * 0.045).max(3.0), line_cap: tiny_skia::LineCap::Round, ..Default::default() };
    for sx in [0.04f32, 0.96] {
        let mut pb = PathBuilder::new();
        pb.move_to(fw * sx, 0.0);
        pb.line_to(fw / 2.0 + (sx - 0.5) * cw * 0.5, cy0 + 4.0);
        if let Some(path) = pb.finish() {
            pm.stroke_path(&path, &paint_of(accent), &cord, Transform::identity(), None);
        }
    }
    let rect = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_ltrb(x0, y0, x1, y1);
    let r = cw * 0.08;
    let mut pb = PathBuilder::new();
    pb.move_to(cx0 + r, cy0);
    pb.line_to(cx0 + cw - r, cy0);
    pb.quad_to(cx0 + cw, cy0, cx0 + cw, cy0 + r);
    pb.line_to(cx0 + cw, cy0 + ch - r);
    pb.quad_to(cx0 + cw, cy0 + ch, cx0 + cw - r, cy0 + ch);
    pb.line_to(cx0 + r, cy0 + ch);
    pb.quad_to(cx0, cy0 + ch, cx0, cy0 + ch - r);
    pb.line_to(cx0, cy0 + r);
    pb.quad_to(cx0, cy0, cx0 + r, cy0);
    pb.close();
    if let Some(path) = pb.finish() {
        pm.fill_path(&path, &paint_of([246, 246, 246]), FillRule::Winding, Transform::identity(), None);
        pm.stroke_path(&path, &paint_of([150, 150, 150]), &Stroke { width: 2.0, ..Default::default() }, Transform::identity(), None);
    }
    if let Some(x) = rect(cx0, cy0, cx0 + cw, cy0 + ch * 0.22) {
        pm.fill_rect(x, &paint_of(accent), Transform::identity(), None);
    }
    if let Some(x) = rect(cx0 + cw * 0.1, cy0 + ch * 0.32, cx0 + cw * 0.42, cy0 + ch * 0.86) {
        pm.fill_rect(x, &paint_of([150, 160, 175]), Transform::identity(), None);
    }
    for k in 0..3 {
        let yy = cy0 + ch * (0.38 + 0.16 * k as f32);
        if let Some(x) = rect(cx0 + cw * 0.5, yy, cx0 + cw * (0.9 - 0.12 * k as f32), yy + ch * 0.07) {
            pm.fill_rect(x, &paint_of([90, 90, 90]), Transform::identity(), None);
        }
    }
    Pic::of_pixmap(&pm)
}

/// The texels of bare skin (legs under a skirt, a neck, a wrist): never repainted.
fn skin(c: [f32; 3]) -> bool {
    let [r, g, b] = c;
    r > 95.0 && g > 45.0 && b > 25.0 && r > g && g > b && r - b > 35.0 && r - g > 12.0
}

fn lum(c: [f32; 3]) -> f32 {
    0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2]
}

/// A box blur of `v` (w x h), three times: about a Gaussian of `radius`'s size.
fn blur(v: &[f32], w: u32, h: u32, radius: i64) -> Vec<f32> {
    let (w, h) = (w as i64, h as i64);
    let mut a = v.to_vec();
    let mut b = vec![0.0; a.len()];
    for _ in 0..3 {
        for y in 0..h {
            for x in 0..w {
                let (lo, hi) = ((x - radius).max(0), (x + radius).min(w - 1));
                let s: f32 = (lo..=hi).map(|k| a[(y * w + k) as usize]).sum();
                b[(y * w + x) as usize] = s / (hi - lo + 1) as f32;
            }
        }
        for y in 0..h {
            for x in 0..w {
                let (lo, hi) = ((y - radius).max(0), (y + radius).min(h - 1));
                let s: f32 = (lo..=hi).map(|k| b[(k * w + x) as usize]).sum();
                a[(y * w + x) as usize] = s / (hi - lo + 1) as f32;
            }
        }
    }
    a
}

/// A texture being repainted (RGB in floats; its alpha kept as it was).
pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub rgb: Vec<[f32; 3]>,
}

impl Canvas {
    pub fn of_rgba(rgba: &[u8], w: u32, h: u32) -> Canvas {
        Canvas { w, h, rgb: rgba.chunks_exact(4).map(|c| [c[0] as f32, c[1] as f32, c[2] as f32]).collect() }
    }

    /// Back into `rgba` (its alpha left).
    pub fn into_rgba(&self, rgba: &mut [u8]) {
        for (c, o) in self.rgb.iter().zip(rgba.chunks_exact_mut(4)) {
            for k in 0..3 {
                o[k] = c[k].round().clamp(0.0, 255.0) as u8;
            }
        }
    }

    /// Colour `mask` in `colour`, keeping its folds: the brightness against the part's median,
    /// `keep` of it from a blurred copy (so stripes and prints of the original fade out). Bare
    /// skin stays as it is.
    pub fn shade(&mut self, mask: &[bool], colour: [f32; 3], keep: f32, lo: f32, hi: f32) {
        self.shade_as(mask, colour, keep, lo, hi, true);
    }

    /// The jacket: as `shade`, but a cloth of the skin's colours - a beige jumper, mostly
    /// "skin" - is cloth all over (a skirt's bare legs are not in it).
    fn shade_jacket(&mut self, mask: &[bool], colour: [f32; 3]) {
        let (n, bare) = mask.iter().zip(&self.rgb).filter(|(m, _)| **m).fold((0usize, 0usize), |(n, b), (_, c)| (n + 1, b + skin(*c) as usize));
        self.shade_as(mask, colour, 0.55, 0.5, 1.4, bare * 10 <= n * 4);
    }

    fn shade_as(&mut self, mask: &[bool], colour: [f32; 3], keep: f32, lo: f32, hi: f32, keep_skin: bool) {
        let sel: Vec<bool> = mask.iter().zip(&self.rgb).map(|(m, c)| *m && !(keep_skin && skin(*c))).collect();
        let mut ls: Vec<f32> = sel.iter().zip(&self.rgb).filter(|(m, _)| **m).map(|(_, c)| lum(*c)).collect();
        if ls.is_empty() {
            return;
        }
        let mid = ls.len() / 2;
        let m = ls.select_nth_unstable_by(mid, f32::total_cmp).1.to_owned().max(8.0);
        let l: Vec<f32> = self.rgb.iter().map(|c| lum(*c).clamp(0.0, 255.0).round()).collect();
        let bl = blur(&l, self.w, self.h, 4);
        for i in 0..self.rgb.len() {
            if !sel[i] {
                continue;
            }
            let s = (keep * bl[i] / m + (1.0 - keep) * l[i] / m).clamp(lo, hi);
            // (gentler: the new cloth's own colour carries)
            let s = 0.75 + 0.25 * s;
            self.rgb[i] = [(colour[0] * s).clamp(0.0, 255.0), (colour[1] * s).clamp(0.0, 255.0), (colour[2] * s).clamp(0.0, 255.0)];
        }
    }

    fn shade_default(&mut self, mask: &[bool], colour: [f32; 3]) {
        self.shade(mask, colour, 0.55, 0.5, 1.4);
    }

    /// Paint `pic` on the body's front (`front`) or back: the rectangle centred on `x0` (metres,
    /// the figure's left +) with its top at `y0`, `w` wide and `h` high, read by someone facing
    /// that side.
    #[allow(clippy::too_many_arguments)]
    pub fn decal(&mut self, body: &Body, mask: &[bool], front: bool, x0: f32, y0: f32, w: f32, h: f32, pic: &Pic) {
        for i in 0..self.rgb.len() {
            if !mask[i] || (front && body.n[i] <= 0.2) || (!front && body.n[i] >= -0.2) {
                continue;
            }
            let p = body.p[i];
            if (p.x - x0).abs() > w / 2.0 || p.y > y0 || p.y < y0 - h {
                continue;
            }
            // (seen from the front the figure's left is the viewer's right; from behind, its
            // left is the left)
            let t = if front { (x0 + w / 2.0 - p.x) / w } else { (p.x - (x0 - w / 2.0)) / w };
            let u = (y0 - p.y) / h;
            let px = ((t * (pic.w - 1) as f32) as i64).clamp(0, pic.w as i64 - 1);
            let py = ((u * (pic.h - 1) as f32) as i64).clamp(0, pic.h as i64 - 1);
            let s = pic.px[(py * pic.w as i64 + px) as usize];
            let a = s[3] / 255.0;
            for k in 0..3 {
                self.rgb[i][k] = self.rgb[i][k] * (1.0 - a) + s[k] * a;
            }
        }
    }
}

/// What an inspector's clothes show.
#[derive(Clone, Debug, PartialEq)]
pub struct Dress {
    /// 'A', 'B' or 'C' (see the module's words).
    pub look: char,
    /// The company's colour and name (upper case on the back), the word on the vest.
    pub colour: [u8; 3],
    pub company: String,
    pub text: String,
}

/// The generator's version: clothes made by an older one are made again.
pub const VERSION: u32 = 1;

impl Dress {
    /// What the clothes' files were made of, as the `.cti`'s first line keeps it.
    pub fn key(&self, figure: &str) -> String {
        format!("; Omsi-Hub inspector clothes v{VERSION} {} #{:02x}{:02x}{:02x} {} | {} | {}", self.look, self.colour[0], self.colour[1], self.colour[2], self.company, self.text, figure)
    }
}

fn rgb(h: u32) -> [f32; 3] {
    [((h >> 16) & 255) as f32, ((h >> 8) & 255) as f32, (h & 255) as f32]
}

/// Repaint a body texture into the inspector's (`hem`: where the jacket ends, metres up).
pub fn paint_body(cv: &mut Canvas, body: &Body, d: &Dress, hem: f32) {
    let accent = [d.colour[0] as f32, d.colour[1] as f32, d.colour[2] as f32];
    let part = |i: usize, p: u8| body.part[i] == p;
    let torso = body.mask(|i| part(i, TORSO));
    let arm = body.mask(|i| part(i, ARM));
    let jacket: Vec<bool> = (0..torso.len()).map(|i| (torso[i] && body.p[i].y > hem) || arm[i]).collect();
    let trousers = body.mask(|i| part(i, LEG) || (part(i, TORSO) && body.p[i].y <= hem));
    // (the measure of everything placed: the shoulders' height - the arms' top, out in the rest
    // pose - so that a short figure's vest, stripe and text sit where a tall one's do)
    let top = body.top_of(ARM).or_else(|| body.top_of(TORSO).map(|t| t - 0.09)).unwrap_or(1.45) + 0.017;
    let upper = body.mask(|i| part(i, TORSO) && body.p[i].y > hem);
    match d.look {
        'A' => {
            cv.shade_jacket(&jacket, rgb(0x1e2a45));
            cv.shade_default(&trousers, rgb(0x1b2236));
            let low = hem.max(top - 0.50);
            // (in front the lapels are their own flaps: the vest stops below them; behind it
            // reaches the collar)
            let vest = body.mask(|i| {
                let p = body.p[i];
                let behind = body.n[i] < -0.2 && p.z < -0.03;
                let vest_top = if behind { top - 0.05 } else { top - 0.20 };
                part(i, TORSO) && p.y > low && p.y < vest_top && p.x.abs() < 0.15
            });
            cv.shade(&vest, rgb(0xd4ee2c), 0.7, 0.7, 1.25);
            let span = top - low;
            for k in [0.22f32, 0.38] {
                let band: Vec<bool> = (0..vest.len()).map(|i| vest[i] && (body.p[i].y - (low + span * k)).abs() < 0.018).collect();
                cv.shade(&band, rgb(0xcfd6df), 0.8, 0.85, 1.15);
            }
            // (the vest open down the front)
            let open: Vec<bool> = (0..vest.len()).map(|i| vest[i] && body.n[i] > 0.2 && body.p[i].x.abs() < 0.016).collect();
            cv.shade_default(&open, rgb(0x1e2a45));
            let navy = [24, 32, 56, 255];
            cv.decal(body, &vest, false, 0.0, top - 0.07, 0.27, 0.13, &text_picture(&[("BILLET-", 1.0), ("KONTROL", 1.0)], 0.27, 0.13, navy, 900.0));
            cv.decal(body, &vest, true, 0.08, top - 0.24, 0.11, 0.035, &text_picture(&[("KONTROL", 1.0)], 0.11, 0.035, navy, 900.0));
        }
        'B' => {
            let top = body.top_of(TORSO).unwrap_or(1.45);
            cv.decal(body, &torso, true, 0.0, top - 0.03, 0.16, 0.30, &id_card(0.16, 0.30, d.colour));
        }
        _ => {
            cv.shade_jacket(&jacket, rgb(0x2b303b));
            cv.shade_default(&trousers, rgb(0x23262e));
            let band_y = top - 0.355;
            let band = body.mask(|i| part(i, TORSO) && (body.p[i].y - band_y).abs() < 0.02 && body.p[i].y > hem);
            cv.shade(&band, accent, 0.8, 0.8, 1.2);
            // (the cuffs in the company's colour: the sleeves' outer end, arms out in the rest pose)
            let hands: Vec<f32> = (0..body.p.len()).filter(|&i| body.cover[i] && part(i, HAND)).map(|i| body.p[i].x.abs()).collect();
            let wrist = hands.iter().copied().reduce(f32::min).or_else(|| (0..body.p.len()).filter(|&i| arm[i]).map(|i| body.p[i].x.abs()).reduce(f32::max)).unwrap_or(1.0);
            let cuffs: Vec<bool> = (0..arm.len()).map(|i| arm[i] && body.p[i].x.abs() > wrist - 0.045 && body.p[i].x.abs() < wrist - 0.01).collect();
            cv.shade(&cuffs, accent, 0.8, 0.8, 1.2);
            let white = [255, 255, 255, 255];
            let name = d.company.to_uppercase();
            cv.decal(body, &upper, false, 0.0, top - 0.07, 0.27, 0.15, &text_picture(&[(&name, 1.5), (&d.text, 1.0)], 0.27, 0.15, white, 900.0));
            // (the company's name in its colour: painted over the white again)
            cv.decal(body, &upper, false, 0.0, top - 0.07, 0.27, 0.15 * 1.5 / 2.5, &text_picture(&[(&name, 1.0)], 0.27, 0.15 * 1.5 / 2.5, [d.colour[0], d.colour[1], d.colour[2], 255], 900.0));
            let mut patch = Pic::new(220, 90);
            for p in patch.px.iter_mut() {
                *p = [accent[0], accent[1], accent[2], 255.0];
            }
            patch.over(&text_picture(&[("KONTROL", 1.0)], 0.11, 0.045, white, 2000.0), 0, 0);
            cv.decal(body, &upper, true, 0.085, top - 0.19, 0.10, 0.04, &patch);
        }
    }
}

/// Repaint a head texture's cap (A: navy, C: the company's dark one with a band in its colour;
/// B wears none): from `cap_from` metres up.
pub fn paint_cap(cv: &mut Canvas, body: &Body, d: &Dress, cap_from: f32) {
    let cap = body.mask(|i| body.p[i].y > cap_from);
    match d.look {
        'A' => cv.shade_default(&cap, rgb(0x1e2a45)),
        'C' => {
            cv.shade_default(&cap, rgb(0x2b303b));
            let band: Vec<bool> = (0..cap.len()).map(|i| cap[i] && body.p[i].y < cap_from + 0.045).collect();
            cv.shade(&band, [d.colour[0] as f32, d.colour[1] as f32, d.colour[2] as f32], 0.8, 0.8, 1.2);
        }
        _ => {}
    }
}

/// Where the jacket ends on a figure `height` metres tall (aXYZ's man01 0.86, the women's
/// coats longer: 0.93).
pub fn hem_of(name: &str, height: f32) -> f32 {
    let n = name.to_ascii_lowercase();
    if n.contains("woman") || n.contains("women") || n.contains("frau") {
        if n.contains("axyz") && (1.6..1.8).contains(&height) {
            0.93
        } else {
            0.547 * height
        }
    } else if n.contains("axyz") && n.contains("man01") && (1.7..1.9).contains(&height) {
        0.86
    } else {
        0.483 * height
    }
}

/// What `dress_figure` wrote.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Dressed {
    /// The `.cti` item's name (`~Inspector_<look>_<slug>`), the `.cti` and the textures written.
    pub item: String,
    pub files: Vec<PathBuf>,
    /// Nothing was written: the clothes were made already of the same.
    pub cached: bool,
}

/// The content folder's copy of `dir` (a folder of the installation): None outside both.
pub fn in_content(dir: &Path, root: &Path, content: &Path) -> Option<PathBuf> {
    if dir.starts_with(content) {
        Some(dir.to_path_buf())
    } else {
        dir.strip_prefix(root).ok().map(|rel| content.join(rel))
    }
}

/// Make the inspector's clothes `d` of the figure `hum` (a `.hum` file): the item
/// `~Inspector_<look>_<slug>` in the content folder's copy of its `[CTC]` folder. Refused: a
/// figure without a `[CTC]` folder, its body unknown (not aXYZ's bones), or outside the folders.
pub fn dress_figure(hum: &Path, root: &Path, content: &Path, d: &Dress, slug: &str) -> Result<Dressed, String> {
    let item = format!("~Inspector_{}_{}", d.look, slug);
    let t = omsi_sim::human::HumanType::load(hum).map_err(|e| format!("{e:#}"))?;
    let hum_dir = hum.parent().unwrap_or(Path::new(""));
    let ctc = t.model.ctc.first().ok_or("the figure has no [CTC] folder")?;
    let ctc_dir = omsi_cfg::resolve_path(hum_dir, &ctc.path);
    let out_dir = in_content(&ctc_dir, root, content).ok_or("the figure lies outside the game's folders")?;
    if !super::export::inside(&out_dir, content) {
        return Err("the figure's clothes would not go into the content folder".into());
    }
    let cti = out_dir.join(format!("{item}.cti"));
    let key = d.key(&hum.to_string_lossy());
    if std::fs::read_to_string(&cti).ok().and_then(|s| s.lines().next().map(str::to_string)).as_deref() == Some(key.as_str()) {
        return Ok(Dressed { item, files: vec![cti], cached: true });
    }
    // the lod-0 meshes as their files have them (y up), with each vertex's part
    struct Raw {
        pos: Vec<Vec3>,
        nor: Vec<Vec3>,
        uv: Vec<Vec2>,
        parts: Vec<u8>,
        tris: Vec<([u32; 3], String)>,
    }
    let mut raws: Vec<Raw> = Vec::new();
    let meshes = if t.model.lods.is_empty() { &t.model.meshes[..] } else { t.model.lod_meshes(0) };
    for md in meshes {
        let p = omsi_cfg::resolve_path(&t.model_dir, &md.file);
        let Ok(m) = omsi_o3d::load_mesh(&p) else { continue };
        let mut parts = vec![NONE; m.vertices.len()];
        let mut best = vec![0.0f32; m.vertices.len()];
        for b in &m.bones {
            let part = part_of(&b.name);
            for w in &b.weights {
                if let Some(x) = best.get_mut(w.vertex as usize) {
                    if w.weight > *x {
                        *x = w.weight;
                        parts[w.vertex as usize] = part;
                    }
                }
            }
        }
        let tris = m.triangles.iter().map(|tr| (tr.indices, m.materials.get(tr.material as usize).map(|x| x.texture.trim().to_string()).unwrap_or_default())).collect();
        raws.push(Raw { pos: m.vertices.iter().map(|v| v.position).collect(), nor: m.vertices.iter().map(|v| v.normal).collect(), uv: m.vertices.iter().map(|v| v.uv).collect(), parts, tris });
    }
    let height = raws.iter().flat_map(|r| r.pos.iter().map(|p| p.y)).reduce(f32::max).unwrap_or(1.75);
    // (a child is no inspector)
    if height < 1.5 {
        return Err("the figure is a child's".into());
    }
    let hem = hem_of(&hum.to_string_lossy(), height);
    let dirs = t.texture_dirs(root);
    let dir_refs: Vec<&Path> = dirs.iter().map(PathBuf::as_path).collect();
    let sub = out_dir.join("~Inspector");
    let mut items: Vec<(String, String)> = Vec::new();
    let mut files = Vec::new();
    let mut body_done = false;
    for (slot, default) in &t.model.ctc_textures {
        let parts: Vec<MeshPart> = raws
            .iter()
            .map(|r| MeshPart { pos: &r.pos, nor: &r.nor, uv: &r.uv, parts: &r.parts, tris: r.tris.iter().filter(|(_, tex)| tex.eq_ignore_ascii_case(default.trim())).map(|(t, _)| *t).collect() })
            .filter(|m| !m.tris.is_empty())
            .collect();
        if parts.is_empty() {
            continue;
        }
        let Some(file) = omsi_texture::find_texture(default.trim(), &dir_refs) else { continue };
        let Ok(img) = omsi_texture::decode_file(&file) else { continue };
        let (w, h) = (img.width, img.height);
        if w < 16 || h < 16 || w > 4096 || h > 4096 {
            continue;
        }
        let body = Body::of(&parts, w, h);
        let mut cv = Canvas::of_rgba(&img.rgba, w, h);
        if body.has(TORSO) {
            paint_body(&mut cv, &body, d, hem);
            body_done = true;
        } else if body.has(HEAD) && d.look != 'B' {
            let Some(top) = body.top_of(HEAD) else { continue };
            paint_cap(&mut cv, &body, d, top - 0.10);
        } else {
            continue;
        }
        let mut rgba = img.rgba.clone();
        cv.into_rgba(&mut rgba);
        let alpha = rgba.chunks_exact(4).any(|c| c[3] < 255);
        let dds = omsi_texture::dds::encode(&rgba, w, h, if alpha { omsi_texture::bc::Bc::Bc3 } else { omsi_texture::bc::Bc::Bc1 { punch: false } });
        let stem = Path::new(default.trim()).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "skin".into());
        let name = format!("{stem}_{}_{slug}.dds", d.look);
        std::fs::create_dir_all(&sub).map_err(|e| format!("{}: {e}", sub.display()))?;
        let path = sub.join(&name);
        std::fs::write(&path, &dds).map_err(|e| format!("{}: {e}", path.display()))?;
        files.push(path);
        items.push((slot.clone(), format!("~Inspector\\{name}")));
    }
    if !body_done {
        for f in &files {
            let _ = std::fs::remove_file(f);
        }
        return Err("the figure's body is not one the clothes know".into());
    }
    let mut text = format!("{key}\r\n\r\n");
    for (slot, file) in &items {
        text.push_str(&format!("[item]\r\n{item}\r\n{slot}\r\n{file}\r\n\r\n"));
    }
    std::fs::write(&cti, text).map_err(|e| format!("{}: {e}", cti.display()))?;
    files.insert(0, cti);
    Ok(Dressed { item, files, cached: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tiny figure: a box-shaped torso (front and back faces) and two arm quads, mapped onto a
    /// 64 x 64 texture - front left, back right, arms below.
    fn tiny() -> (Vec<Vec3>, Vec<Vec3>, Vec<Vec2>, Vec<u8>, Vec<[u32; 3]>) {
        let mut pos = Vec::new();
        let mut nor = Vec::new();
        let mut uv = Vec::new();
        let mut parts = Vec::new();
        let mut tris = Vec::new();
        // (each quad in 4 x 4 pieces: a mesh's triangles are small against its texture)
        let mut quad = |p: [Vec3; 4], n: Vec3, u: [Vec2; 4], part: u8| {
            const K: u32 = 4;
            let b = pos.len() as u32;
            for j in 0..=K {
                for i in 0..=K {
                    let (s, t) = (i as f32 / K as f32, j as f32 / K as f32);
                    pos.push(p[0].lerp(p[1], s).lerp(p[3].lerp(p[2], s), t));
                    uv.push(u[0].lerp(u[1], s).lerp(u[3].lerp(u[2], s), t));
                    nor.push(n);
                    parts.push(part);
                }
            }
            for j in 0..K {
                for i in 0..K {
                    let v = b + j * (K + 1) + i;
                    tris.push([v, v + 1, v + K + 2]);
                    tris.push([v, v + K + 2, v + K + 1]);
                }
            }
        };
        let v = Vec3::new;
        let t = Vec2::new;
        // (the torso from the hips at 0.9 m to the shoulders at 1.45 m, 0.36 m wide; uv v from the top)
        quad([v(-0.18, 0.9, 0.1), v(0.18, 0.9, 0.1), v(0.18, 1.45, 0.1), v(-0.18, 1.45, 0.1)], Vec3::Z, [t(0.45, 0.7), t(0.05, 0.7), t(0.05, 0.1), t(0.45, 0.1)], TORSO);
        quad([v(-0.18, 0.9, -0.1), v(0.18, 0.9, -0.1), v(0.18, 1.45, -0.1), v(-0.18, 1.45, -0.1)], -Vec3::Z, [t(0.55, 0.7), t(0.95, 0.7), t(0.95, 0.1), t(0.55, 0.1)], TORSO);
        // (an arm out to each side at shoulder height)
        quad([v(0.18, 1.40, 0.0), v(0.6, 1.40, 0.0), v(0.6, 1.45, 0.0), v(0.18, 1.45, 0.0)], Vec3::Y, [t(0.05, 0.95), t(0.45, 0.95), t(0.45, 0.8), t(0.05, 0.8)], ARM);
        quad([v(-0.6, 1.40, 0.0), v(-0.18, 1.40, 0.0), v(-0.18, 1.45, 0.0), v(-0.6, 1.45, 0.0)], Vec3::Y, [t(0.55, 0.95), t(0.95, 0.95), t(0.95, 0.8), t(0.55, 0.8)], ARM);
        (pos, nor, uv, parts, tris)
    }

    fn texel(rgba: &[u8], w: u32, u: f32, v: f32) -> [u8; 3] {
        // (as `Body` maps them: v from the texture's bottom)
        let (x, y) = ((u * w as f32) as usize, ((1.0 - v) * w as f32) as usize);
        let i = (y * w as usize + x) * 4;
        [rgba[i], rgba[i + 1], rgba[i + 2]]
    }

    #[test]
    fn a_tiny_figure_gets_the_vest_and_the_company_s_uniform() {
        let (pos, nor, uv, parts, tris) = tiny();
        let body = Body::of(&[MeshPart { pos: &pos, nor: &nor, uv: &uv, parts: &parts, tris }], 64, 64);
        assert!(body.has(TORSO) && body.has(ARM) && !body.has(HEAD));
        assert!((body.top_of(ARM).unwrap() - 1.45).abs() < 0.02);
        // (a grey shirt: the texture's every texel)
        let grey = vec![120u8, 120, 120, 255].repeat(64 * 64);
        let paint = |look: char| {
            let mut cv = Canvas::of_rgba(&grey, 64, 64);
            let d = Dress { look, colour: [0xF5, 0x82, 0x20], company: "Umove".into(), text: "BILLETKONTROL".into() };
            paint_body(&mut cv, &body, &d, 0.95);
            let mut out = grey.clone();
            cv.into_rgba(&mut out);
            out
        };
        let a = paint('A');
        // the vest's yellow on the chest, front and back; navy below the hem and on the arms
        let vest = texel(&a, 64, 0.18, 0.45);
        assert!(vest[0] > 150 && vest[1] > 170 && vest[2] < 90, "{vest:?}");
        let hip = texel(&a, 64, 0.25, 0.67);
        assert!(hip[2] > hip[0] && hip[0] < 60, "{hip:?}");
        let sleeve = texel(&a, 64, 0.25, 0.88);
        assert!(sleeve[2] > sleeve[0] && sleeve[0] < 60, "{sleeve:?}");
        // (the alpha kept, the corners not on the body as they were)
        assert_eq!(texel(&a, 64, 0.5, 0.02), [120, 120, 120]);
        let c = paint('C');
        let jacket = texel(&c, 64, 0.25, 0.2);
        assert!(jacket[0] < 80 && jacket[1] < 80, "{jacket:?}");
        // (somewhere the company's colour: its band or its name)
        assert!(c.chunks_exact(4).any(|p| p[0] > 180 && p[1] > 90 && p[1] < 170 && p[2] < 90));
        // B: the figure's own clothes, a card on the chest
        let b = paint('B');
        assert_eq!(texel(&b, 64, 0.25, 0.67), [120, 120, 120]);
        assert!(b.chunks_exact(4).any(|p| p[0] > 230 && p[1] > 230 && p[2] > 230), "the card's white");
        // (the same draws the same)
        assert_eq!(paint('A'), a);
    }

    #[test]
    fn the_clothes_are_kept_by_what_they_show() {
        let d = Dress { look: 'A', colour: [1, 2, 3], company: "Umove".into(), text: "BILLETKONTROL".into() };
        assert_eq!(d.key("x"), d.clone().key("x"));
        assert_ne!(d.key("x"), Dress { colour: [1, 2, 4], ..d.clone() }.key("x"));
        assert_ne!(d.key("x"), Dress { look: 'C', ..d.clone() }.key("x"));
        assert_ne!(d.key("x"), d.key("y"));
        assert!(d.key("x").starts_with(';'), "a comment line before the [item]s");
        assert_eq!((hem_of("Humans/aXYZ/man01.hum", 1.8), hem_of("Humans/aXYZ/woman01.hum", 1.7)), (0.86, 0.93));
        assert!((hem_of("Humans/Other/man04.hum", 2.0) - 0.966).abs() < 1e-3);
        assert_eq!(part_of("OA_L"), ARM);
        assert_eq!(part_of("Head"), HEAD);
        assert_eq!(part_of("whatever"), NONE);
        let (root, content) = (Path::new("/omsi"), Path::new("/omsi/openomsi/content"));
        assert_eq!(in_content(Path::new("/omsi/Humans/aXYZ/texture/man01"), root, content), Some(content.join("Humans/aXYZ/texture/man01")));
        assert_eq!(in_content(Path::new("/elsewhere/x"), root, content), None);
    }

    /// (by hand) Dress real figures: `INSPECTOR_ROOT` an installation's folder with `Humans`,
    /// `INSPECTOR_CONTENT` where the clothes go, `INSPECTOR_HUMS` the `.hum` files (`;`).
    #[test]
    #[ignore]
    fn dress_real_figures() {
        let (Ok(root), Ok(content), Ok(hums)) = (std::env::var("INSPECTOR_ROOT"), std::env::var("INSPECTOR_CONTENT"), std::env::var("INSPECTOR_HUMS")) else { return };
        for look in ['A', 'B', 'C'] {
            let d = Dress { look, colour: [0xF5, 0x82, 0x20], company: "Umove".into(), text: "BILLETKONTROL".into() };
            for h in hums.split(';') {
                let r = dress_figure(Path::new(h), Path::new(&root), Path::new(&content), &d, "umove");
                println!("{look} {h}: {r:?}");
                assert!(r.is_ok());
                // (made again only when it changed)
                assert!(dress_figure(Path::new(h), Path::new(&root), Path::new(&content), &d, "umove").unwrap().cached);
            }
        }
    }

    #[test]
    fn a_text_and_a_card_are_drawn() {
        let t = text_picture(&[("KONTROL", 1.0)], 0.11, 0.035, [24, 32, 56, 255], 900.0);
        assert_eq!((t.w, t.h), (99, 31));
        assert!(t.px.iter().any(|p| p[3] > 200.0) && t.px.iter().any(|p| p[3] == 0.0));
        let card = id_card(0.16, 0.30, [0xF5, 0x82, 0x20]);
        assert!(card.px.iter().any(|p| p[3] > 250.0 && p[0] > 240.0 && p[1] > 240.0));
    }
}
