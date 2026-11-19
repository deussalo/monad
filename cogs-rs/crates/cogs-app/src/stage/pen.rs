//! The palette, hardcoded to Nord (nordtheme.com), and a pen an egui painter with the stage's world→screen transform, drawing as the JS
//! canvas does (widths in screen px, lengths in world units).

use egui::{epaint::Mesh, pos2, Color32, Painter, Pos2, Shape, Stroke, Vec2};
use std::f64::consts::TAU;

pub const BG: Color32 = Color32::from_rgb(0x2e, 0x34, 0x40); // nord0
pub const FG: Color32 = Color32::from_rgb(0xd8, 0xde, 0xe9); // nord4
pub const DIM: Color32 = Color32::from_rgb(0x61, 0x6e, 0x88); // nord3, brightened
pub const FAINT: Color32 = Color32::from_rgb(0x36, 0x3c, 0x4a); // between nord0 and nord1
pub const LINE: Color32 = Color32::from_rgb(0x43, 0x4c, 0x5e); // nord2
pub const HOT: Color32 = Color32::from_rgb(0xeb, 0xcb, 0x8b); // nord13
pub const SEL: Color32 = Color32::from_rgb(0x88, 0xc0, 0xd0); // nord8
pub const BAD: Color32 = Color32::from_rgb(0xbf, 0x61, 0x6a); // nord11
pub const READ: Color32 = Color32::from_rgb(0x8f, 0xbc, 0xbb); // nord7
pub const PAD: Color32 = Color32::from_rgb(0x3b, 0x42, 0x52); // nord1
/// The darkest well: text fields, graph backgrounds (nord0, darker).
pub const DEEP: Color32 = Color32::from_rgb(0x24, 0x29, 0x33);
/// Captions and quiet labels: between nord3 and nord4.
pub const CAPTION: Color32 = Color32::from_rgb(0x8f, 0x9b, 0xb3);
/// Raised and pressed fills: nord3 and a step above it.
pub const RAISE: Color32 = Color32::from_rgb(0x4c, 0x56, 0x6a);
pub const RAISE2: Color32 = Color32::from_rgb(0x5a, 0x65, 0x7c);
/// Text on an accent fill: nord0.
pub const INK: Color32 = BG;
pub const ORANGE: Color32 = Color32::from_rgb(0xd0, 0x87, 0x70); // nord12
pub const GREEN: Color32 = Color32::from_rgb(0xa3, 0xbe, 0x8c); // nord14
pub const PURPLE: Color32 = Color32::from_rgb(0xb4, 0x8e, 0xad); // nord15
pub const BLUE: Color32 = Color32::from_rgb(0x81, 0xa1, 0xc1); // nord9
pub const DEEPBLUE: Color32 = Color32::from_rgb(0x5e, 0x81, 0xac); // nord10

/// A colour at alpha a (premultiplied, as egui keeps them).
#[inline]
pub fn alpha(c: Color32, a: f32) -> Color32 {
    c.gamma_multiply(a.clamp(0.0, 1.0))
}
pub fn hex_a(hex: u32, a: f32) -> Color32 {
    alpha(Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8), a)
}

#[derive(Clone)]
pub struct Pen<'a> {
    pub p: &'a Painter,
    /// screen = world × s + o
    pub s: f64,
    pub ox: f64,
    pub oy: f64,
    /// The heard time, for glows and ripples.
    pub now: f64,
}

impl<'a> Pen<'a> {
    pub fn new(p: &'a Painter, s: f64, ox: f64, oy: f64, now: f64) -> Self {
        Pen { p, s, ox, oy, now }
    }
    #[inline]
    pub fn at(&self, x: f64, y: f64) -> Pos2 {
        pos2((x * self.s + self.ox) as f32, (y * self.s + self.oy) as f32)
    }
    /// A world length on screen.
    #[inline]
    pub fn len(&self, l: f64) -> f32 {
        (l * self.s) as f32
    }
    /// One screen px in world units (the JS PX).
    #[inline]
    pub fn px(&self) -> f64 {
        1.0 / self.s
    }
    /// glow(t): a flash's light, fading over 120 ms.
    pub fn glow(&self, t: f64) -> f64 {
        if !t.is_finite() || self.now < t { 0.0 } else { (-(self.now - t) / 0.12).exp() }
    }
    /// hot(t, base): lit while the flash lasts.
    pub fn hot(&self, t: f64, base: Color32) -> Color32 {
        let e = self.glow(t);
        if e <= 0.03 { base } else { alpha(HOT, (0.35 + 0.65 * e) as f32) }
    }

    pub fn line(&self, x1: f64, y1: f64, x2: f64, y2: f64, w: f32, c: Color32) {
        self.p.line_segment([self.at(x1, y1), self.at(x2, y2)], Stroke::new(w, c));
    }
    pub fn dashed(&self, x1: f64, y1: f64, x2: f64, y2: f64, w: f32, c: Color32, dash: f32, gap: f32, offset: f32) {
        self.p.extend(Shape::dashed_line_with_offset(&[self.at(x1, y1), self.at(x2, y2)], Stroke::new(w, c), &[dash], &[gap], offset));
    }
    pub fn circle(&self, x: f64, y: f64, r: f64, w: f32, c: Color32) {
        self.p.circle_stroke(self.at(x, y), self.len(r.max(0.0)), Stroke::new(w, c));
    }
    pub fn fill(&self, x: f64, y: f64, r: f64, c: Color32) {
        self.p.circle_filled(self.at(x, y), self.len(r.max(0.0)), c);
    }
    pub fn dashed_circle(&self, x: f64, y: f64, r: f64, w: f32, c: Color32, dash: f32, gap: f32) {
        let pts = self.arc_pts(x, y, r, 0.0, TAU);
        self.p.extend(Shape::dashed_line(&pts, Stroke::new(w, c), dash, gap));
    }
    /// Points along an arc, fine enough on screen.
    pub fn arc_pts(&self, x: f64, y: f64, r: f64, a0: f64, a1: f64) -> Vec<Pos2> {
        let rs = (r * self.s).abs();
        let n = (((a1 - a0).abs() * rs / 3.0).ceil() as usize).clamp(4, 720);
        (0..=n).map(|i| {
            let a = a0 + (a1 - a0) * i as f64 / n as f64;
            self.at(x + a.cos() * r, y + a.sin() * r)
        }).collect()
    }
    pub fn arc(&self, x: f64, y: f64, r: f64, a0: f64, a1: f64, w: f32, c: Color32) {
        self.p.add(Shape::line(self.arc_pts(x, y, r, a0, a1), Stroke::new(w, c)));
    }
    pub fn poly(&self, pts: Vec<Pos2>, w: f32, c: Color32) {
        self.p.add(Shape::line(pts, Stroke::new(w, c)));
    }
    pub fn closed(&self, pts: Vec<Pos2>, w: f32, c: Color32) {
        self.p.add(Shape::closed_line(pts, Stroke::new(w, c)));
    }
    /// A filled ring between radii r0 and r1.
    pub fn annulus(&self, x: f64, y: f64, r0: f64, r1: f64, c: Color32) {
        let (r0, r1) = (r0.max(0.0), r1.max(0.0));
        let rs = self.len(r1);
        let n = ((rs * 0.6).ceil() as usize).clamp(24, 360);
        let mut m = Mesh::default();
        for i in 0..=n {
            let a = TAU * i as f64 / n as f64;
            m.colored_vertex(self.at(x + a.cos() * r0, y + a.sin() * r0), c);
            m.colored_vertex(self.at(x + a.cos() * r1, y + a.sin() * r1), c);
        }
        for i in 0..n as u32 {
            let k = 2 * i;
            m.add_triangle(k, k + 1, k + 2);
            m.add_triangle(k + 1, k + 3, k + 2);
        }
        self.p.add(Shape::mesh(m));
    }
    /// A radial glow: colour stops (fraction of r, colour) from the centre out.
    pub fn radial(&self, x: f64, y: f64, r: f64, stops: &[(f64, Color32)]) {
        let n = ((self.len(r) * 0.5).ceil() as usize).clamp(32, 256);
        let mut m = Mesh::default();
        let c0 = m.vertices.len() as u32;
        m.colored_vertex(self.at(x, y), stops[0].1);
        // rings at each stop past 0
        let mut ring_start = Vec::new();
        for &(f, col) in stops.iter().skip(if stops[0].0 == 0.0 { 1 } else { 0 }) {
            ring_start.push(m.vertices.len() as u32);
            for i in 0..n {
                let a = TAU * i as f64 / n as f64;
                m.colored_vertex(self.at(x + a.cos() * r * f, y + a.sin() * r * f), col);
            }
        }
        if ring_start.is_empty() {
            return;
        }
        let n32 = n as u32;
        for i in 0..n32 {
            m.add_triangle(c0, ring_start[0] + i, ring_start[0] + (i + 1) % n32);
        }
        for w in ring_start.windows(2) {
            let (a, b) = (w[0], w[1]);
            for i in 0..n32 {
                let j = (i + 1) % n32;
                m.add_triangle(a + i, b + i, b + j);
                m.add_triangle(a + i, b + j, a + j);
            }
        }
        self.p.add(Shape::mesh(m));
    }
    pub fn text(&self, x: f64, y: f64, s: &str, size: f32, c: Color32, align: egui::Align2) {
        if size < 1.0 {
            return;
        }
        self.p.text(self.at(x, y), align, s, egui::FontId::monospace(size), c);
    }
    /// A quadratic curve from a through control q to b.
    pub fn quad(&self, a: (f64, f64), q: (f64, f64), b: (f64, f64), w: f32, c: Color32) {
        let n = 16;
        let pts = (0..=n).map(|i| {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            self.at(u * u * a.0 + 2.0 * u * t * q.0 + t * t * b.0, u * u * a.1 + 2.0 * u * t * q.1 + t * t * b.1)
        }).collect();
        self.poly(pts, w, c);
    }
    pub fn offset(&self) -> Vec2 {
        Vec2::new(self.ox as f32, self.oy as f32)
    }
}
