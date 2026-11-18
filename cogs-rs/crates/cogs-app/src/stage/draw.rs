//! Every part's picture, from stage.js draw*(), motion.js drawCam, pit.js drawPit/drawBall, probe.js drawProbe and
//! field.js drawField. Lengths are world units; widths are screen px, as the canvas's PX.

use super::pen::*;
use cogs_sim::{pit::ball_r, Cog, Field, Host, Part, Str, Track, VoiceKind, World, SCALES, TAU};
use egui::{Align2, Color32};
use std::f64::consts::PI;

pub const NOTE_NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
pub fn note_name(m: i64) -> String {
    format!("{}{}", NOTE_NAMES[m.rem_euclid(12) as usize], m.div_euclid(12) - 1)
}
pub fn wrap_pi(a: f64) -> f64 {
    cogs_sim::wrap(a + PI) - PI
}
/// The gap between a disc's rings, in world units.
pub fn ring_gap(c: &Cog) -> f64 {
    let n = c.track.as_ref().map(|t| t.rings()).unwrap_or(1);
    if n > 1 { c.radius() * 0.65 / (n - 1) as f64 } else { c.radius() * 0.3 }
}
/// A disc is a score once its rings are 12 px apart on screen.
pub fn note_mode(c: &Cog, zoom: f64) -> bool {
    c.track.is_some() && ring_gap(c) * zoom >= 12.0
}
pub fn roll_dir(c: &Cog) -> f64 {
    if c.w < 0.0 { -1.0 } else { 1.0 }
}

/// What the drawing needs to know of the selection.
#[derive(Default, Clone, Copy)]
pub struct Marks {
    /// A selected pin: (cog id, ring, angle).
    pub dot: Option<(u32, f64, f64)>,
    /// The zoom from which a disc face is a score to edit, and drawn as one (0: by ring spacing alone).
    pub score_zoom: f64,
}

pub fn draw_cog(pen: &Pen, c: &Cog, th: f64, marks: &Marks) {
    let px = pen.px();
    let r = c.radius();
    let (mut x, mut y) = (c.x, c.y);
    if c.jam {
        let k = pen.now;
        x += (k * 71.0).sin() * 1.3 * px;
        y += (k * 53.0).cos() * 1.3 * px;
    }
    let ink = if c.jam { BAD } else { DIM };
    if c.layer != 0.0 { pen.dashed_circle(x, y, r, 1.0, ink, 4.0, 3.0) } else { pen.circle(x, y, r, 1.0, ink) }
    let n = c.n.max(0.0) as usize;
    for k in 0..n {
        let a = th + c.tooth + k as f64 * TAU / c.n;
        let (ca, sa) = (a.cos(), a.sin());
        pen.line(x + ca * (r - 3.0), y + sa * (r - 3.0), x + ca * (r + 3.0), y + sa * (r + 3.0), 1.0, ink);
    }
    if let Some(t) = &c.track {
        draw_disc_face(pen, c, t, th, x, y, marks);
    }
    pen.line(x, y, x + th.cos() * (r - 5.0), y + th.sin() * (r - 5.0), 1.0, FG);
    if c.motor.is_some() {
        pen.circle(x, y, (5.0 * px).max(r * 0.3), 2.0, if c.jam { BAD } else if c.w != 0.0 { HOT } else { DIM });
    }
    let hub = if c.jam { BAD } else { FG };
    if c.motor.is_some() { pen.fill(x, y, 3.0 * px, hub) } else { pen.circle(x, y, 3.0 * px, 1.0, hub) }
}

fn draw_disc_face(pen: &Pen, c: &Cog, t: &Track, th: f64, x: f64, y: f64, marks: &Marks) {
    let px = pen.px();
    let note = note_mode(c, pen.s) && pen.s >= marks.score_zoom;
    let gap = ring_gap(c);
    let rings = t.rings();
    let (r_in, r_out) = (c.ring_r(0.0), c.ring_r((rings - 1) as f64));
    if note {
        for i in 0..rings {
            if (t.ring_note(i as i64) as i64).rem_euclid(12) == (t.base() as i64).rem_euclid(12) {
                let rr = c.ring_r(i as f64);
                pen.annulus(x, y, rr - gap / 2.0, rr + gap / 2.0, PAD);
            }
        }
    }
    for i in 0..rings {
        pen.circle(x, y, c.ring_r(i as f64), 1.0, FAINT);
    }
    let div = t.div.max(1.0) as usize;
    for k in 0..div {
        let a = th + k as f64 * TAU / t.div;
        pen.line(x + a.cos() * r_in, y + a.sin() * r_in, x + a.cos() * r_out, y + a.sin() * r_out, 1.0, if note && k % 4 == 0 { DIM } else { FAINT });
    }
    let base = if note { (gap * 0.32).clamp(1.5, 4.5) } else { 1.5 * px };
    for d in &t.dots {
        let rr = c.ring_r(d.ring);
        let a = th + d.a;
        let (dx, dy) = (x + a.cos() * rr, y + a.sin() * rr);
        let pr = base * if note { 0.6 + 0.4 * d.v } else { 1.0 + d.v } + 2.0 * pen.glow(d.flash) * px;
        let col = alpha(pen.hot(d.flash, FG), (0.3 + 0.7 * d.p) as f32);
        pen.fill(dx, dy, pr, col);
        if d.len > 1.0 {
            let tail = a - roll_dir(c) * (d.len - 1.0) * TAU / t.div;
            pen.arc(x, y, rr, a.min(tail), a.max(tail), pen.len(pr), col);
        }
        if marks.dot.map_or(false, |(id, ring, da)| id == c.id && ring == d.ring && (wrap_pi(da - d.a)).abs() < 1e-6) {
            pen.circle(dx, dy, pr + 3.0 * px, 1.0, SEL);
        }
    }
}

/// An arm's flex after a strike: a damped wobble.
pub fn arm_bend(bend_t: f64, bend_a: f64, now: f64) -> f64 {
    let age = now - bend_t;
    if age >= 0.0 && age < 2.0 { bend_a * (-age / 0.25).exp() * (age * TAU * 9.0).cos() } else { 0.0 }
}
pub fn draw_arm(pen: &Pen, cx: f64, cy: f64, ang: f64, r0: f64, len: f64, bend: f64, flash: f64) {
    let (bx, by) = (cx + ang.cos() * r0, cy + ang.sin() * r0);
    let (mx, my) = (bx + ang.cos() * len / 2.0, by + ang.sin() * len / 2.0);
    let (tx, ty) = (bx + (ang + bend).cos() * len, by + (ang + bend).sin() * len);
    pen.quad((bx, by), (mx, my), (tx, ty), 3.0, pen.hot(flash, FG));
}

pub fn emblem(pen: &Pen, kind: VoiceKind, x: f64, y: f64, r: f64, col: Color32, w: f32) {
    match kind {
        VoiceKind::Kick => pen.circle(x, y, r * 0.45, w, col),
        VoiceKind::Snare => {
            pen.line(x - r * 0.5, y - r * 0.15, x + r * 0.5, y - r * 0.15, w, col);
            pen.line(x - r * 0.5, y + r * 0.15, x + r * 0.5, y + r * 0.15, w, col);
        }
        VoiceKind::Hat => {
            pen.line(x - r * 0.55, y, x + r * 0.55, y, w, col);
            pen.circle(x, y, r * 0.12, w, col);
        }
        VoiceKind::Synth => {
            let a = (x - r * 0.35, y + r * 0.3);
            pen.quad(a, (x - r * 0.35, y - r * 0.5), (x, y - r * 0.5), w, col);
            pen.quad((x, y - r * 0.5), (x + r * 0.35, y - r * 0.5), (x + r * 0.35, y + r * 0.3), w, col);
            pen.line(x + r * 0.35, y + r * 0.3, a.0, a.1, w, col);
        }
        k => {
            let ch = &k.name()[..1];
            pen.text(x, y + r * 0.05, ch, pen.len(r), col, Align2::CENTER_CENTER);
        }
    }
}

pub fn draw_pad(pen: &Pen, x: f64, y: f64, r: f64, kind: VoiceKind, flash: f64, last_v: f64, v_ref: f64) {
    let px = pen.px();
    if flash.is_finite() && pen.now >= flash {
        let dt = pen.now - flash;
        if dt < 0.6 {
            let k = dt / 0.6;
            let amp = (last_v / v_ref).clamp(0.25, 1.0);
            pen.circle(x, y, r + 3.0 * px + k * (14.0 + 26.0 * amp) * px, 1.0, alpha(HOT, ((1.0 - k) * 0.6 * amp) as f32));
        }
    }
    pen.fill(x, y, r, pen.hot(flash, PAD));
    pen.circle(x, y, r, 1.0, FG);
    if r / px > 10.0 {
        emblem(pen, kind, x, y, r, DIM, 1.0);
    }
}

pub fn draw_reader(pen: &Pen, x1: f64, y1: f64, x2: f64, y2: f64, tines: f64, flash: f64) {
    let px = pen.px();
    let (ex, ey) = (x2 - x1, y2 - y1);
    let l = {
        let l = ex.hypot(ey);
        if l == 0.0 { 1.0 } else { l }
    };
    pen.line(x1, y1, x2, y2, 2.0, pen.hot(flash, READ));
    let n = tines.max(0.0) as usize;
    for i in 1..n {
        let u = i as f64 / tines;
        pen.line(x1 + ex * u - ey / l * 4.0 * px, y1 + ey * u + ex / l * 4.0 * px, x1 + ex * u + ey / l * 4.0 * px, y1 + ey * u - ex / l * 4.0 * px, 1.0, READ);
    }
    pen.fill(x1, y1, 2.5 * px, READ);
    pen.fill(x2, y2, 2.5 * px, READ);
}

pub fn draw_string(pen: &Pen, s: &Str) {
    let px = pen.px();
    let (ex, ey) = (s.x2 - s.x1, s.y2 - s.y1);
    let l = {
        let l = ex.hypot(ey);
        if l == 0.0 { 1.0 } else { l }
    };
    let age = pen.now - if s.flash.is_finite() { s.flash } else { -9.0 };
    let amp = if age >= 0.0 && age < 2.0 { (s.last_v / 60.0).min(8.0) * (-age / 0.35).exp() * (age * TAU * 11.0).sin() } else { 0.0 };
    let u = if s.hits > 0 { s.last_u } else { 0.5 };
    let (mx, my) = (s.x1 + ex * u - ey / l * amp, s.y1 + ey * u + ex / l * amp);
    let w = if s.kind == "bar" { 4.0 } else { 1.5 };
    pen.poly(vec![pen.at(s.x1, s.y1), pen.at(mx, my), pen.at(s.x2, s.y2)], w, pen.hot(s.flash, FG));
    pen.fill(s.x1, s.y1, 2.5 * px, DIM);
    pen.fill(s.x2, s.y2, 2.5 * px, DIM);
}

pub fn draw_belt(pen: &Pen, lines: &[[f64; 4]], offset: f64) {
    // the canvas's lineDashOffset is in world units here; the dash is 6 px on, 5 off
    let period = 11.0f32;
    let off = (-(offset * pen.s) as f32).rem_euclid(period);
    for l in lines {
        pen.dashed(l[0], l[1], l[2], l[3], 2.0, DIM, 6.0, 5.0, off);
    }
}

/// Paused, a selected plate shows where its 0 is: a tick outside the rim, lit when the plate stands on it.
pub fn zero_tick(pen: &Pen, c: &Cog, rth: f64) {
    let px = pen.px();
    let r = c.radius();
    let on = wrap_pi(rth).abs() < 1e-3;
    pen.line(c.x + r + 5.0 * px, c.y, c.x + r + 15.0 * px, c.y, if on { 2.0 } else { 1.0 }, if on { HOT } else { SEL });
}
pub fn halo(pen: &Pen, x: f64, y: f64, r: f64) {
    pen.circle(x, y, r + 3.0 * pen.px(), 1.5, SEL);
}

// ---------- the cam ----------
pub fn cam_rb(c: &Cog) -> f64 {
    (c.radius() * 0.78).max(8.0)
}
pub fn draw_cam(pen: &Pen, m: &cogs_sim::Cam, c: &Cog, th: f64) {
    let px = pen.px();
    let rb = cam_rb(c);
    let roll = |y: f64| rb * (0.45 + 0.55 * y);
    let n = cogs_sim::CAM_N;
    let pts: Vec<_> = (0..n).map(|i| {
        let u = i as f64 / n as f64;
        let w = th + TAU * (u + m.phase);
        let r = roll(m.at(u % 1.0));
        pen.at(c.x + w.cos() * r, c.y + w.sin() * r)
    }).collect();
    pen.closed(pts, 1.5, alpha(HOT, 0.85));
    let mut lo = 0;
    for i in 1..n {
        if m.prof[i] < m.prof[lo] {
            lo = i;
        }
    }
    let w = th + TAU * (lo as f64 / n as f64 + m.phase);
    let r = roll(m.prof[lo]);
    pen.fill(c.x + w.cos() * r, c.y + w.sin() * r, 1.8 * px, HOT);
    let y = m.at(m.u(th));
    let r = roll(y);
    let (ca, sa) = (m.ang.cos(), m.ang.sin());
    pen.line(c.x + ca * r, c.y + sa * r, c.x + ca * (r + 14.0), c.y + sa * (r + 14.0), 2.0, FG);
    pen.circle(c.x + ca * r, c.y + sa * r, 2.2 * px, 1.0, FG);
    pen.fill(c.x + ca * (r + 14.0), c.y + sa * (r + 14.0), 2.6 * px, FG);
    let (g0, g1) = (roll(0.0) + 14.0, roll(1.0) + 14.0);
    for s in [-1.0, 1.0] {
        pen.line(c.x + ca * g0 + s * sa * 4.0 * px, c.y + sa * g0 - s * ca * 4.0 * px, c.x + ca * g1 + s * sa * 4.0 * px, c.y + sa * g1 - s * ca * 4.0 * px, 1.0, DIM);
    }
}
pub fn cam_tip(m: &cogs_sim::Cam, c: &Cog, th: f64) -> (f64, f64) {
    let rb = cam_rb(c);
    let r = rb * (0.45 + 0.55 * m.at(m.u(th))) + 14.0;
    (c.x + m.ang.cos() * r, c.y + m.ang.sin() * r)
}

// ---------- the ball pit ----------
pub fn draw_pit(pen: &Pen, x: f64, y: f64, w: f64, flash: f64) {
    let r = w / 2.0;
    pen.fill(x, y, r, alpha(PAD, 0.35));
    pen.circle(x, y, r, 2.0, pen.hot(flash, DIM));
    pen.arc(x, y, r * 0.86, PI * 1.08, PI * 1.42, 1.5, alpha(FG, 0.18));
    pen.arc(x, y, r * 0.9, PI * 0.1, PI * 0.55, 1.5, alpha(FG, 0.08));
}
pub fn draw_ball(pen: &Pen, x: f64, y: f64, r: f64, note: i32, flash: f64, big: bool) {
    let px = pen.px();
    let g = pen.glow(flash);
    if flash.is_finite() && pen.now >= flash && pen.now - flash < 0.4 {
        let k = (pen.now - flash) / 0.4;
        pen.circle(x, y, r + 2.0 * px + k * 10.0 * px, 1.0, alpha(HOT, ((1.0 - k) * 0.5) as f32));
    }
    pen.fill(x, y, r, PAD);
    pen.circle(x, y, r, 1.0, if g > 0.03 { HOT } else { FG });
    let a = (note.rem_euclid(12)) as f64 / 12.0 * TAU - PI / 2.0;
    let (ca, sa) = (a.cos(), a.sin());
    pen.line(x + ca * r * 0.55, y + sa * r * 0.55, x + ca * r, y + sa * r, 1.5, if g > 0.03 { HOT } else { DIM });
    if big && r * pen.s >= 14.0 {
        pen.text(x, y, NOTE_NAMES[note.rem_euclid(12) as usize], (11.0f64).min(r * pen.s * 0.5) as f32, DIM, Align2::CENTER_CENTER);
    }
}
pub fn draw_pit_sel(pen: &Pen, x: f64, y: f64, w: f64) {
    let px = pen.px();
    let r = w / 2.0;
    pen.circle(x, y, r + 4.0 * px, 1.5, SEL);
}

// ---------- the FX field ----------
pub fn field_color(mode: &str) -> u32 {
    match mode {
        "reverb" => 0xa98be0,
        "dist" => 0xe66a4a,
        _ => 0x5aa9e6,
    }
}
pub fn field_glyph(pen: &Pen, mode: &str, x: f64, y: f64, s: f64, w: f32, col: Color32) {
    match mode {
        "delay" => {
            for i in 0..3 {
                let r = s * (0.35 + i as f64 * 0.35);
                pen.arc(x - s * 0.6, y, r, -0.9, 0.9, w, col);
            }
        }
        "reverb" => {
            pen.circle(x, y, s * 0.3, w, col);
            pen.circle(x, y, s * 0.75, w, col);
        }
        _ => {
            let mut pts = vec![pen.at(x - s, y)];
            for i in 0..4 {
                pts.push(pen.at(x - s + (i as f64 + 0.5) * s / 2.0, y + if i % 2 == 1 { s * 0.7 } else { -s * 0.7 }));
            }
            pts.push(pen.at(x + s, y));
            pen.poly(pts, w, col);
        }
    }
}
fn smooth(u: f64) -> f64 {
    u * u * (3.0 - 2.0 * u)
}
pub fn draw_field(pen: &Pen, f: &Field) {
    let px = pen.px();
    let col = field_color(&f.mode);
    let core = 1.0 - f.diffuse.clamp(0.0, 1.0);
    let silent = f.slot < 0;
    let a = if silent { 0.05 } else { 0.16 };
    let mut stops = vec![(0.0, hex_a(col, (a * 1.25) as f32))];
    if core > 0.02 {
        stops.push((core * 0.98, hex_a(col, a as f32)));
    }
    for i in 1..=4 {
        let u = i as f64 / 5.0;
        let r = core + (1.0 - core) * u;
        stops.push((r.min(0.999), hex_a(col, (a * (1.0 - smooth(u))) as f32)));
    }
    stops.push((1.0, hex_a(col, 0.0)));
    stops.sort_by(|p, q| p.0.partial_cmp(&q.0).unwrap());
    pen.radial(f.x, f.y, f.r, &stops);
    for h in &f.flashes {
        if pen.now < h.t {
            continue;
        }
        let k = (pen.now - h.t) / 0.5;
        if k > 1.0 {
            continue;
        }
        pen.circle(h.x, h.y, (4.0 + k * 26.0) * px, 1.5, hex_a(col, ((1.0 - k) * 0.6 * (h.a * 1.5).min(1.0)) as f32));
    }
    let ink = hex_a(col, if silent { 0.4 } else { 0.9 });
    pen.circle(f.x, f.y, 9.0 * px, 1.2, ink);
    field_glyph(pen, &f.mode, f.x, f.y, 6.0 * px, 1.2, ink);
}
pub fn draw_field_sel(pen: &Pen, f: &Field) {
    let px = pen.px();
    let col = field_color(&f.mode);
    pen.dashed_circle(f.x, f.y, f.r, 1.0, hex_a(col, 0.6), 4.0, 5.0);
    let core = 1.0 - f.diffuse.clamp(0.0, 1.0);
    if core > 0.02 && core < 0.98 {
        pen.dashed_circle(f.x, f.y, f.r * core, 1.0, hex_a(col, 0.3), 4.0, 5.0);
    }
    halo(pen, f.x, f.y, 9.0 * px);
    let (hx, hy) = field_handle(f);
    pen.fill(hx, hy, 4.0 * px, hex_a(col, 1.0));
    pen.circle(hx, hy, 4.0 * px, 1.0, FG);
}
pub fn field_handle(f: &Field) -> (f64, f64) {
    (f.x + f.r * std::f64::consts::FRAC_1_SQRT_2, f.y + f.r * std::f64::consts::FRAC_1_SQRT_2)
}

// ---------- the probe ----------
pub fn draw_probe(pen: &Pen, w: &World, pr: &cogs_sim::Probe) {
    if !w.probe_alive(pr) {
        return;
    }
    let px = pen.px();
    let q = w.probe_point(pr);
    let y = w.probe_now(pr);
    let g = 8.0 * px;
    if pr.axis == cogs_sim::Axis::Reach {
        let c = if pr.host == Some(Host::Ball) {
            let p = &w.pits[pr.on_ix.unwrap()];
            (p.x, p.y)
        } else {
            let c = &w.cogs[w.probe_cog(pr).unwrap()];
            (c.x, c.y)
        };
        pen.dashed(c.0, c.1, q.0, q.1, 1.0, alpha(SEL, 0.5), 2.0, 3.0, 0.0);
    } else {
        let (x0, x1, y0, y1) = if pr.host == Some(Host::Ball) {
            let p = &w.pits[pr.on_ix.unwrap()];
            (p.x - p.w / 2.0, p.x + p.w / 2.0, p.y - p.h / 2.0, p.y + p.h / 2.0)
        } else {
            let c = &w.cogs[w.probe_cog(pr).unwrap()];
            let b = pr.tbox;
            (c.x + b[0], c.x + b[1], c.y + b[2], c.y + b[3])
        };
        let ink = alpha(SEL, 0.55);
        if pr.axis == cogs_sim::Axis::Y {
            let (rx, ty) = (x1 + g, y1 + (y0 - y1) * y);
            pen.line(rx, y0, rx, y1, 1.0, ink);
            pen.line(rx - 3.0 * px, ty, rx + 3.0 * px, ty, 1.0, ink);
            pen.dashed(q.0, q.1, rx, ty, 1.0, ink, 2.0, 3.0, 0.0);
        } else {
            let (ry, tx) = (y1 + g, x0 + (x1 - x0) * y);
            pen.line(x0, ry, x1, ry, 1.0, ink);
            pen.line(tx, ry - 3.0 * px, tx, ry + 3.0 * px, 1.0, ink);
            pen.dashed(q.0, q.1, tx, ry, 1.0, ink, 2.0, 3.0, 0.0);
        }
    }
    pen.circle(q.0, q.1, 3.2 * px, 1.0, SEL);
    pen.line(q.0 - 5.0 * px, q.1, q.0 + 5.0 * px, q.1, 1.0, SEL);
    pen.line(q.0, q.1 - 5.0 * px, q.0, q.1 + 5.0 * px, 1.0, SEL);
    if let Some(t) = pr.to {
        if let Some(b) = part_point(w, t) {
            pen.dashed(q.0, q.1, b.0, b.1, 1.0, alpha(SEL, 0.6), 2.0, 3.0, 0.0);
            pen.fill(b.0, b.1, 2.0 * px, SEL);
        }
    }
}
/// A voiced part's place: a reader's middle, a pad's or sphere's centre.
pub fn part_point(w: &World, p: Part) -> Option<(f64, f64)> {
    match p {
        Part::Reader(i) => w.readers.iter().find(|r| r.id == i).map(|r| ((r.x1 + r.x2) / 2.0, (r.y1 + r.y2) / 2.0)),
        Part::Pad(i) => w.pads.iter().find(|r| r.id == i).map(|r| (r.x, r.y)),
        Part::Pit(i) => w.pits.iter().find(|r| r.id == i).map(|r| (r.x, r.y)),
        Part::String(i) => w.strings.iter().find(|r| r.id == i).map(|r| ((r.x1 + r.x2) / 2.0, (r.y1 + r.y2) / 2.0)),
    }
}

/// hitWindow arcs: where each arm meets each pad, shown when asked for or for a selected arm or pad.
pub fn draw_windows(pen: &Pen, w: &World, show: impl Fn(u32, u32) -> bool) {
    for arm in &w.arms {
        let c = &w.cogs[arm.cog_ix];
        for pad in &w.pads {
            if !show(arm.id, pad.id) {
                continue;
            }
            let Some((beta, half, s)) = cogs_sim::sim::hit_window(c, arm, pad) else { continue };
            pen.arc(c.x, c.y, s, beta - half, beta + half, 1.0, FAINT);
            if c.w != 0.0 {
                let e = if c.w > 0.0 { beta - half } else { beta + half };
                pen.line(c.x, c.y, c.x + e.cos() * s, c.y + e.sin() * s, 1.0, FAINT);
            }
        }
    }
}

/// The machine, in stage.js draw()'s order.
pub fn draw_world(pen: &Pen, w: &World, marks: &Marks, windows: impl Fn(u32, u32) -> bool) {
    for f in &w.fields {
        draw_field(pen, f);
    }
    for p in &w.pits {
        draw_pit(pen, p.x, p.y, p.w, p.flash);
    }
    let mut order: Vec<usize> = (0..w.cogs.len()).collect();
    order.sort_by(|a, b| w.cogs[*a].layer.partial_cmp(&w.cogs[*b].layer).unwrap_or(std::cmp::Ordering::Equal));
    for i in order {
        draw_cog(pen, &w.cogs[i], w.render_th(i), marks);
    }
    for (i, b) in w.belts.iter().enumerate() {
        if let Some(l) = w.belt_lines(i) {
            let a = &w.cogs[b.a_ix];
            draw_belt(pen, &l, w.render_th(b.a_ix) * (a.radius() + 3.0));
        }
    }
    draw_windows(pen, w, windows);
    for s in &w.strings {
        draw_string(pen, s);
    }
    for r in &w.readers {
        draw_reader(pen, r.x1, r.y1, r.x2, r.y2, r.tines, r.flash);
    }
    for p in &w.pads {
        draw_pad(pen, p.x, p.y, p.r, p.voice.kind, p.flash, p.last_v, w.s.v_ref);
    }
    for a in &w.arms {
        let c = &w.cogs[a.cog_ix];
        draw_arm(pen, c.x, c.y, w.render_th(a.cog_ix) + a.a, a.r0, a.len, arm_bend(a.bend_t, a.bend_a, pen.now), a.flash);
    }
    for m in &w.cams {
        draw_cam(pen, m, &w.cogs[m.cog_ix], w.render_th(m.cog_ix));
    }
    for m in &w.cams {
        let Some(ri) = m.to_ix else { continue };
        let rd = &w.readers[ri];
        let a = cam_tip(m, &w.cogs[m.cog_ix], w.render_th(m.cog_ix));
        let b = ((rd.x1 + rd.x2) / 2.0, (rd.y1 + rd.y2) / 2.0);
        let live = rd.voice.kind == VoiceKind::Wave;
        pen.dashed(a.0, a.1, b.0, b.1, 1.0, alpha(SEL, if live { 0.7 } else { 0.3 }), 2.0, 3.0, 0.0);
        pen.fill(b.0, b.1, 2.0 * pen.px(), SEL);
    }
    let big = w.s.zoom >= 1.2;
    for p in &w.pits {
        for b in &p.balls {
            let q = cogs_sim::pit::ball_at(b, pen.now);
            draw_ball(pen, q.0, q.1, b.r, b.note, b.flash, big);
        }
    }
    for pr in &w.probes {
        draw_probe(pen, w, pr);
    }
}

// ---------- the shelf's pictures ----------
pub const PARTS: [&str; 13] = ["motor", "cog", "disc", "arm", "belt", "pad", "string", "reader", "comb", "cam", "sphere", "probe", "field"];
pub fn icon_scale(kind: &str) -> f64 {
    match kind {
        "disc" => 0.235,
        "cog" | "motor" => 0.82,
        "pad" => 1.3,
        _ => 0.8,
    }
}
pub fn model_cog(kind: &str) -> Cog {
    let mut c = Cog {
        id: 0, x: 0.0, y: 0.0, n: 12.0, motor: None, layer: 0.0, th: 0.13, th0: 0.0, tooth: 0.0, track: None, w: 0.0, k: None, jam: false,
        turns: 0.0, driver: None, scrub: None,
    };
    match kind {
        "motor" => {
            c.motor = Some(cogs_sim::Motor { rpm: 30.0, revs: 0.0 });
            c.w = 1.0;
        }
        "disc" => {
            c.n = 48.0;
            c.th = 0.3;
            let mut t = Track { root: 9.0, octave: 3.0, range: 1.0, scale: cogs_sim::scale_ix("minpent").unwrap(), div: 8.0, ..Default::default() };
            for (ring, a) in [(0.0, 0.0), (2.0, 1.57), (4.0, 3.93), (3.0, 5.5), (1.0, 2.36)] {
                t.dots.push(cogs_sim::Dot::new(ring, a));
            }
            c.track = Some(t);
        }
        _ => {}
    }
    let _ = SCALES;
    c
}
/// A part as its picture, about 50 px across at scale 1, centred on the pen's origin.
pub fn draw_part_icon(pen: &Pen, kind: &str) {
    let px = pen.px();
    let m = Marks::default();
    match kind {
        "motor" | "cog" | "disc" => {
            let c = model_cog(kind);
            draw_cog(pen, &c, c.th, &m);
        }
        "arm" => draw_arm(pen, -14.0, 14.0, -PI / 4.0, 0.0, 40.0, 0.0, f64::NEG_INFINITY),
        "cam" => {
            let mut c = model_cog("cog");
            c.y = 4.0;
            c.th = 0.0;
            let mut cam = cogs_sim::motion::new_cam(0);
            cam.ang = -PI / 2.0;
            cam.set_shape("ramp up");
            draw_cog(pen, &c, 0.0, &m);
            draw_cam(pen, &cam, &c, 0.0);
        }
        "probe" => {
            pen.circle(-8.0, 6.0, 12.0, 1.0, DIM);
            let q = (-8.0 + (-1.0f64).cos() * 10.0, 6.0 + (-1.0f64).sin() * 10.0);
            pen.circle(q.0, q.1, 3.0, 1.0, SEL);
            pen.line(q.0 - 5.0, q.1, q.0 + 5.0, q.1, 1.0, SEL);
            pen.line(q.0, q.1 - 5.0, q.0, q.1 + 5.0, 1.0, SEL);
            pen.line(16.0, -12.0, 16.0, 18.0, 1.0, SEL);
            pen.line(13.0, q.1, 19.0, q.1, 1.0, SEL);
            pen.dashed(q.0, q.1, 16.0, q.1, 1.0, SEL, 2.0, 3.0, 0.0);
        }
        "field" => {
            pen.radial(0.0, 0.0, 22.0, &[(0.0, hex_a(0x5aa9e6, 0.45)), (0.5, hex_a(0x5aa9e6, 0.25)), (1.0, hex_a(0x5aa9e6, 0.0))]);
            let w = (1.2 * pen.s) as f32;
            pen.circle(0.0, 0.0, 7.0, w, hex_a(0x5aa9e6, 1.0));
            field_glyph(pen, "delay", 0.0, 0.0, 5.0, w, hex_a(0x5aa9e6, 1.0));
        }
        "pad" => draw_pad(pen, 0.0, 0.0, 11.0, VoiceKind::Kick, f64::NEG_INFINITY, 0.0, 500.0),
        "belt" => {
            pen.circle(-12.0, 0.0, 9.0, 1.0, DIM);
            pen.circle(14.0, 0.0, 6.0, 1.0, DIM);
            let w = (1.5 * pen.s) as f32;
            let d = (3.0 * pen.s) as f32;
            pen.dashed(-12.0, -9.0, 14.0, -6.0, w, DIM, d, d, 0.0);
            pen.dashed(-12.0, 9.0, 14.0, 6.0, w, DIM, d, d, 0.0);
        }
        "string" => {
            let s = Str {
                id: 0, x1: -20.0, y1: 14.0, x2: 20.0, y2: -14.0, kind: "string".into(), f200: 220.0, quant: true, decay: 1.0, damp: 0.5, hard: 0.5, level: 1.0,
                vel_sense: 0.5, hits: 0, last_v: 0.0, last_u: 0.5, flash: f64::NEG_INFINITY,
            };
            draw_string(pen, &s);
        }
        "sphere" => {
            draw_pit(pen, 0.0, 0.0, 40.0, f64::NEG_INFINITY);
            pen.dashed(-8.0, 8.0, 6.0, -17.0, 1.0, DIM, 2.0, 2.0, 0.0);
            pen.dashed(6.0, -17.0, 13.0, -3.0, 1.0, DIM, 2.0, 2.0, 0.0);
            let a = -PI / 2.0 + 0.33;
            pen.arc(0.0, 0.0, 20.0, a - 0.3, a + 0.3, 1.5, HOT);
            for (x, y, m) in [(-7.0, 8.0, 48), (11.0, -2.0, 60), (4.0, 11.0, 72)] {
                draw_ball(pen, x, y, ball_r(m) * 0.42, m, f64::NEG_INFINITY, false);
            }
        }
        _ => draw_reader(pen, -17.0, 17.0, 17.0, -17.0, if kind == "comb" { 6.0 } else { 0.0 }, f64::NEG_INFINITY),
    }
    let _ = px;
}
