//! Real-pixel 3D Clawds drawn with Sixel graphics.
//!
//! A Clawd is a small voxel model (body, stub arms, four legs, eye slits)
//! posed per frame, turned about its vertical axis, seen from a low camera
//! and shaded per face. Each toy area (the deck above an agent pane, the
//! sidebar floor) becomes one opaque Sixel image of whole cells, written to
//! the host terminal after the cell frame. Windows Terminal reports a fixed
//! 10x20 pixel cell for Sixel, whatever the font size.

use std::fmt::Write as _;
use std::sync::OnceLock;

use cmux_tui_core::Rect;

/// Sixel pixels per cell.
pub(crate) const CELL_W: u32 = 10;
pub(crate) const CELL_H: u32 = 20;

/// Claude orange and its parts.
const BODY: [f32; 3] = [217.0, 119.0, 87.0];
const LEG: [f32; 3] = [196.0, 104.0, 76.0];
const EYE: [f32; 3] = [26.0, 20.0, 16.0];
const OUTLINE: [u8; 3] = [38, 22, 18];
const SKY: [u8; 3] = [0x9C, 0xC8, 0xF0];

/// Camera elevation above the floor, degrees.
const ELEVATION: f32 = 20.0;

/// Whether toys draw as Sixel images. Windows Terminal (1.22+) supports
/// Sixel; `CMUX_TOYS_SIXEL=1` forces it on elsewhere, `=0` turns it off.
pub(crate) fn enabled() -> bool {
    if cfg!(test) {
        return false;
    }
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| match std::env::var("CMUX_TOYS_SIXEL").ok().as_deref() {
        Some("1") => true,
        Some("0") => false,
        _ => std::env::var_os("WT_SESSION").is_some(),
    })
}

/// The host terminal's default background, for areas drawn with the
/// default color. Windows Terminal's default scheme is #0C0C0C.
static DEFAULT_BG: OnceLock<[u8; 3]> = OnceLock::new();

pub(crate) fn set_default_bg(rgb: [u8; 3]) {
    let _ = DEFAULT_BG.set(rgb);
}

/// The RGB of a cell background color.
pub(crate) fn rgb(color: ratatui::style::Color) -> [u8; 3] {
    use ratatui::style::Color;
    match color {
        Color::Rgb(r, g, b) => [r, g, b],
        Color::Black => [0, 0, 0],
        _ => *DEFAULT_BG.get().unwrap_or(&[0x0C, 0x0C, 0x0C]),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Arm {
    Out,
    Up,
    Mid,
    Tucked,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Eyes {
    Open,
    Shut,
}

/// One Clawd's pose for this frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pose3 {
    /// Turn about the vertical axis, degrees; positive turns the face to
    /// the viewer's right.
    pub yaw: f32,
    /// How far each leg's foot is lifted, in voxels (walk cycle).
    pub lift: [i32; 4],
    /// Sitting or squashed: short legs, body low.
    pub low: bool,
    /// Stretched up one voxel.
    pub tall: bool,
    pub arms: (Arm, Arm),
    pub eyes: Eyes,
    /// Belly on the floor (napping).
    pub flat: bool,
}

impl Pose3 {
    pub(crate) fn stand(yaw: f32) -> Self {
        Self {
            yaw,
            lift: [0; 4],
            low: false,
            tall: false,
            arms: (Arm::Out, Arm::Out),
            eyes: Eyes::Open,
            flat: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Part {
    Body,
    Leg,
    Eye,
}

/// Model bounds: x -2..20 (arms reach out), y 0..10 (y = 0 is the face),
/// z 0..18.
const MX: i32 = 22;
const MY: i32 = 10;
const MZ: i32 = 18;
const OX: i32 = 2;
/// Width of the body, without arms.
const BODY_W: i32 = 14;

struct Model {
    vox: Vec<Option<Part>>,
}

impl Model {
    fn new() -> Self {
        Self { vox: vec![None; (MX * MY * MZ) as usize] }
    }

    fn idx(x: i32, y: i32, z: i32) -> Option<usize> {
        let x = x + OX;
        if (0..MX).contains(&x) && (0..MY).contains(&y) && (0..MZ).contains(&z) {
            Some(((z * MY + y) * MX + x) as usize)
        } else {
            None
        }
    }

    fn get(&self, x: i32, y: i32, z: i32) -> Option<Part> {
        Self::idx(x, y, z).and_then(|i| self.vox[i])
    }

    fn fill(&mut self, xs: std::ops::Range<i32>, ys: std::ops::Range<i32>, zs: std::ops::Range<i32>, part: Part) {
        for z in zs {
            for y in ys.clone() {
                for x in xs.clone() {
                    if let Some(i) = Self::idx(x, y, z) {
                        self.vox[i] = Some(part);
                    }
                }
            }
        }
    }
}

/// Body x runs 2..16; arms sit at 0..2 and 16..18.
fn build(pose: &Pose3) -> Model {
    let mut m = Model::new();
    let (leg_h, base) = if pose.flat {
        (0, 0)
    } else if pose.low {
        (1, 1)
    } else {
        (4, 4)
    };
    let top = base + 10 + i32::from(pose.tall);
    m.fill(2..16, 0..MY, base..top, Part::Body);
    for (k, x0) in [3, 6, 10, 13].into_iter().enumerate() {
        if leg_h == 0 {
            continue;
        }
        let lift = pose.lift[k].clamp(0, leg_h - 1);
        m.fill(x0..x0 + 2, 3..7, lift..base, Part::Leg);
    }
    for (side, arm) in [(0, pose.arms.0), (16, pose.arms.1)] {
        let (xs, zs) = match arm {
            Arm::Out => (side..side + 2, base + 3..base + 6),
            Arm::Tucked => (side..side + 2, base + 2..base + 4),
            Arm::Mid => (side..side + 2, base + 5..base + 9),
            Arm::Up => (side..side + 2, base + 6..top + 2),
        };
        m.fill(xs, 3..7, zs, Part::Body);
    }
    let eye_z = base + 5;
    match pose.eyes {
        Eyes::Open => {
            m.fill(5..6, 0..1, eye_z..eye_z + 3, Part::Eye);
            m.fill(12..13, 0..1, eye_z..eye_z + 3, Part::Eye);
        }
        Eyes::Shut => {
            m.fill(4..6, 0..1, eye_z + 1..eye_z + 2, Part::Eye);
            m.fill(12..14, 0..1, eye_z + 1..eye_z + 2, Part::Eye);
        }
    }
    m
}

/// An RGB image the size of a toy area, with a depth buffer and a mask of
/// toy pixels for the outline pass.
pub(crate) struct Image {
    pub w: u32,
    pub h: u32,
    bg: [u8; 3],
    px: Vec<[u8; 3]>,
    depth: Vec<f32>,
    toy: Vec<bool>,
}

impl Image {
    pub(crate) fn new(w: u32, h: u32, bg: [u8; 3]) -> Self {
        let n = (w * h) as usize;
        Self { w, h, bg, px: vec![bg; n], depth: vec![f32::MIN; n], toy: vec![false; n] }
    }

    fn put(&mut self, x: i32, y: i32, d: f32, c: [u8; 3]) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let i = (y as u32 * self.w + x as u32) as usize;
        if d > self.depth[i] {
            self.depth[i] = d;
            self.px[i] = c;
            self.toy[i] = true;
        }
    }

    fn paint(&mut self, x: i32, y: i32, c: [u8; 3]) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let i = (y as u32 * self.w + x as u32) as usize;
        if !self.toy[i] {
            self.px[i] = c;
        }
    }

    /// A soft floor shadow, centered at `cx`, under the toys.
    pub(crate) fn shadow(&mut self, cx: f32, ground: f32, half: f32) {
        let c = shade(self.bg, if luminance(self.bg) > 128.0 { 0.82 } else { 0.55 });
        let rows = (half * 0.18).max(1.5);
        let y0 = ground.round() as i32;
        for dy in -(rows as i32)..=(rows as i32) {
            let k = 1.0 - (dy as f32 / rows).powi(2);
            let span = half * k.max(0.0).sqrt();
            for x in (cx - span).round() as i32..=(cx + span).round() as i32 {
                self.paint(x, y0 + dy, c);
            }
        }
    }

    /// A 1px dark outline around every toy silhouette.
    pub(crate) fn outline(&mut self) {
        let (w, h) = (self.w as i32, self.h as i32);
        let mut edge = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                if self.toy[i] {
                    continue;
                }
                let near = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| {
                    let (nx, ny) = (x + dx, y + dy);
                    nx >= 0 && ny >= 0 && nx < w && ny < h && self.toy[(ny * w + nx) as usize]
                });
                if near {
                    edge.push(i);
                }
            }
        }
        for i in edge {
            self.px[i] = OUTLINE;
        }
    }

    /// A small "z" (or "Z") of a napping Clawd, top-left at (x, y).
    pub(crate) fn zed(&mut self, x: i32, y: i32, size: i32) {
        for k in 0..size {
            self.paint(x + k, y, SKY);
            self.paint(x + k, y + size - 1, SKY);
            self.paint(x + size - 1 - k, y + k, SKY);
        }
    }
}

fn luminance(c: [u8; 3]) -> f32 {
    0.2126 * f32::from(c[0]) + 0.7152 * f32::from(c[1]) + 0.0722 * f32::from(c[2])
}

fn shade(c: [u8; 3], k: f32) -> [u8; 3] {
    c.map(|v| (f32::from(v) * k).round().clamp(0.0, 255.0) as u8)
}

/// Draw one Clawd into `img`: `cx` is the center of its footprint and
/// `ground` the floor line, both in image pixels; `lift` raises it off the
/// floor; `scale` is pixels per voxel.
pub(crate) fn draw_clawd(img: &mut Image, pose: &Pose3, cx: f32, ground: f32, lift: f32, scale: f32) {
    let model = build(pose);
    let (sy, cy) = pose.yaw.to_radians().sin_cos();
    let (es, ec) = ELEVATION.to_radians().sin_cos();
    // From the upper left, in front of the toy.
    let mut light = [-0.45f32, -0.55, 0.70];
    let n = (light[0] * light[0] + light[1] * light[1] + light[2] * light[2]).sqrt();
    light.iter_mut().for_each(|v| *v /= n);
    // Model center (x of the body, y of the depth), turned about it.
    let (mcx, mcy) = (2.0 + BODY_W as f32 / 2.0, MY as f32 / 2.0);
    let sub = 3;
    let step = 1.0 / sub as f32;
    let dot = (scale * step).ceil() as i32 + 1;
    let normals: [(i32, i32, i32); 6] =
        [(-1, 0, 0), (1, 0, 0), (0, -1, 0), (0, 1, 0), (0, 0, -1), (0, 0, 1)];
    for z in 0..MZ {
        for y in 0..MY {
            for x in -OX..MX - OX {
                let Some(part) = model.get(x, y, z) else { continue };
                for &(nx, ny, nz) in &normals {
                    if model.get(x + nx, y + ny, z + nz).is_some() {
                        continue;
                    }
                    // The face's normal in the world, after the turn.
                    let wnx = nx as f32 * cy - ny as f32 * sy;
                    let wny = nx as f32 * sy + ny as f32 * cy;
                    if wny > 0.01 || nz < 0 {
                        continue;
                    }
                    let lit = 0.55
                        + 0.45 * (wnx * light[0] + wny * light[1] + nz as f32 * light[2]).max(0.0)
                        + if nz > 0 { 0.12 } else { 0.0 };
                    // Five steps of light keep the palette small.
                    let lit = (lit * 5.0).round() / 5.0;
                    let base = match part {
                        Part::Body => BODY,
                        Part::Leg => LEG,
                        Part::Eye => EYE,
                    };
                    let color = base.map(|v| (v * lit).round().clamp(0.0, 255.0) as u8);
                    for a in 0..sub {
                        for b in 0..sub {
                            let fa = (a as f32 + 0.5) * step;
                            let fb = (b as f32 + 0.5) * step;
                            let mut p = [x as f32 + 0.5, y as f32 + 0.5, z as f32 + 0.5];
                            let (axis, u, v) = if nx != 0 {
                                (0, 1, 2)
                            } else if ny != 0 {
                                (1, 0, 2)
                            } else {
                                (2, 0, 1)
                            };
                            let nn = [nx, ny, nz][axis] as f32;
                            p[axis] += 0.5 * nn;
                            p[u] = p[u].floor() + fa;
                            p[v] = p[v].floor() + fb;
                            let wx = p[0] - mcx;
                            let wy = p[1] - mcy;
                            let wz = p[2];
                            let rx = wx * cy - wy * sy;
                            let ry = wx * sy + wy * cy;
                            let sx = cx + rx * scale;
                            let syy = ground - lift - (wz * ec + ry * es) * scale;
                            let d = -ry * ec + wz * es;
                            let (px, py) = (sx.floor() as i32, syy.floor() as i32);
                            for ox in 0..dot {
                                for oy in 0..dot {
                                    img.put(px + ox - dot / 2, py + oy - dot / 2, d, color);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// One toy area to send as a Sixel image after the cell frame.
pub(crate) struct Job {
    pub rect: Rect,
    pub image: Image,
}

/// Encode `img` as a Sixel image placed at the cursor: 1:1 pixel aspect,
/// every pixel set, so the area is fully replaced.
pub(crate) fn encode(img: &Image) -> String {
    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut index = vec![0u16; img.px.len()];
    for (i, c) in img.px.iter().enumerate() {
        let k = match palette.iter().position(|p| p == c) {
            Some(k) => k,
            None => {
                palette.push(*c);
                palette.len() - 1
            }
        };
        index[i] = k.min(255) as u16;
    }
    let mut out = String::with_capacity(4096);
    let _ = write!(out, "\x1bP0;1;0q\"1;1;{};{}", img.w, img.h);
    for (k, c) in palette.iter().enumerate().take(256) {
        let pct = c.map(|v| (u32::from(v) * 100 + 127) / 255);
        let _ = write!(out, "#{k};2;{};{};{}", pct[0], pct[1], pct[2]);
    }
    let (w, h) = (img.w as usize, img.h as usize);
    let mut band = 0;
    while band < h {
        let rows = (h - band).min(6);
        let mut used: Vec<u16> = Vec::new();
        for r in 0..rows {
            for x in 0..w {
                let k = index[(band + r) * w + x];
                if !used.contains(&k) {
                    used.push(k);
                }
            }
        }
        for (n, &k) in used.iter().enumerate() {
            if n > 0 {
                out.push('$');
            }
            let _ = write!(out, "#{k}");
            let mut run_char = '\0';
            let mut run = 0usize;
            let flush = |out: &mut String, ch: char, run: usize| {
                if run == 0 {
                    return;
                }
                if run > 3 {
                    let _ = write!(out, "!{run}{ch}");
                } else {
                    (0..run).for_each(|_| out.push(ch));
                }
            };
            for x in 0..w {
                let mut bits = 0u8;
                for r in 0..rows {
                    if index[(band + r) * w + x] == k {
                        bits |= 1 << r;
                    }
                }
                let ch = char::from(63 + bits);
                if ch == run_char {
                    run += 1;
                } else {
                    flush(&mut out, run_char, run);
                    run_char = ch;
                    run = 1;
                }
            }
            // Trailing empty sixels need not be sent.
            if run_char != '?' {
                flush(&mut out, run_char, run);
            }
        }
        out.push('-');
        band += 6;
    }
    out.push_str("\x1b\\");
    out
}

/// The bytes that draw `jobs` on the host: each image at its area's top
/// left cell, with the cursor saved and restored around them. An image the
/// host already shows at the same place (`shown`) is not sent again;
/// `shown` is updated to this frame's images.
pub(crate) fn emit(jobs: &[Job], shown: &mut Vec<(Rect, u64)>) -> Vec<u8> {
    use std::hash::{Hash, Hasher};
    let mut out = String::new();
    let mut now = Vec::with_capacity(jobs.len());
    for job in jobs {
        let sixel = encode(&job.image);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        sixel.hash(&mut h);
        let key = (job.rect, h.finish());
        if !shown.contains(&key) {
            let _ = write!(out, "\x1b[{};{}H", job.rect.y + 1, job.rect.x + 1);
            out.push_str(&sixel);
        }
        now.push(key);
    }
    *shown = now;
    if out.is_empty() {
        return Vec::new();
    }
    format!("\x1b7{out}\x1b8").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toy3d_sixel_header_and_size() {
        let mut img = Image::new(20, 12, [15, 20, 25]);
        draw_clawd(&mut img, &Pose3::stand(30.0), 10.0, 11.0, 0.0, 0.8);
        let s = encode(&img);
        assert!(s.starts_with("\x1bP0;1;0q\"1;1;20;12"));
        assert!(s.ends_with("\x1b\\"));
        // Two bands of six rows.
        assert_eq!(s.matches('-').count(), 2);
    }

    #[test]
    fn toy3d_clawd_is_orange_with_eyes_from_the_front() {
        let mut img = Image::new(80, 60, [0, 0, 0]);
        draw_clawd(&mut img, &Pose3::stand(0.0), 40.0, 56.0, 0.0, 3.0);
        let orange = img.px.iter().filter(|c| c[0] > 150 && c[1] > 70 && c[1] < 160).count();
        let eye = img.px.iter().filter(|c| c[0] < 40 && c[1] < 40 && **c != [0, 0, 0]).count();
        assert!(orange > 600, "body pixels: {orange}");
        assert!(eye > 4, "eye pixels: {eye}");
        // Turned to the back, no eyes show.
        let mut back = Image::new(80, 60, [0, 0, 0]);
        draw_clawd(&mut back, &Pose3::stand(180.0), 40.0, 56.0, 0.0, 3.0);
        let eye = back.px.iter().filter(|c| c[0] < 40 && c[1] < 40 && **c != [0, 0, 0]).count();
        assert_eq!(eye, 0);
    }

    #[test]
    fn toy3d_emit_places_each_image_and_restores_the_cursor() {
        let jobs = vec![Job { rect: Rect { x: 4, y: 2, width: 3, height: 2 }, image: Image::new(30, 40, [1, 2, 3]) }];
        let mut shown = Vec::new();
        let bytes = String::from_utf8(emit(&jobs, &mut shown)).unwrap();
        assert!(bytes.starts_with("\x1b7\x1b[3;5H\x1bP"));
        assert!(bytes.ends_with("\x1b\\\x1b8"));
        // The same picture at the same place is not sent twice.
        assert!(emit(&jobs, &mut shown).is_empty());
    }

    /// `CMUX_TOY3D_DUMP=<file.ppm> cargo test -p cmux-tui --bin cmux-tui
    /// toy3d_dump -- --ignored` writes deck-sized and rail-sized frames.
    #[test]
    #[ignore]
    fn toy3d_dump() {
        let Ok(path) = std::env::var("CMUX_TOY3D_DUMP") else { return };
        let poses: Vec<Pose3> = {
            let mut v = Vec::new();
            for yaw in [-50.0, -20.0, 0.0, 30.0, 50.0, 90.0, 140.0, 180.0] {
                v.push(Pose3::stand(yaw));
            }
            let mut p = Pose3::stand(50.0);
            p.lift = [1, 0, 1, 0];
            p.arms = (Arm::Mid, Arm::Out);
            v.push(p);
            let mut p = Pose3::stand(15.0);
            p.arms = (Arm::Up, Arm::Up);
            p.low = true;
            v.push(p);
            let mut p = Pose3::stand(20.0);
            p.arms = (Arm::Out, Arm::Up);
            v.push(p);
            let mut p = Pose3::stand(-30.0);
            p.flat = true;
            p.eyes = Eyes::Shut;
            p.arms = (Arm::Tucked, Arm::Tucked);
            v.push(p);
            v
        };
        let bg = [0x0C, 0x0C, 0x0C];
        let mut rows = Vec::new();
        for (h, scale) in [(40u32, 2.0f32), (80, 3.0)] {
            let mut img = Image::new(poses.len() as u32 * 80, h, bg);
            for (i, p) in poses.iter().enumerate() {
                let cx = i as f32 * 80.0 + 40.0;
                img.shadow(cx, h as f32 - 5.0, 8.0 * scale);
            }
            for (i, p) in poses.iter().enumerate() {
                draw_clawd(&mut img, p, i as f32 * 80.0 + 40.0, h as f32 - 5.0, 0.0, scale);
            }
            img.outline();
            rows.push(img);
        }
        if let Ok(sixel) = std::env::var("CMUX_TOY3D_SIXEL") {
            let text: String = rows.iter().map(|r| encode(r) + "\n").collect();
            std::fs::write(sixel, text).unwrap();
        }
        let w = rows[0].w;
        let h: u32 = rows.iter().map(|r| r.h).sum();
        let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
        for r in &rows {
            for c in &r.px {
                out.extend_from_slice(c);
            }
        }
        std::fs::write(path, out).unwrap();
    }
}

