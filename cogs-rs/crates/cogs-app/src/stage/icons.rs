//! The buttons' icons: SVG path data (icon_data.rs) parsed, flattened and filled even-odd into a white mask, loaded as
//! an egui texture and drawn tinted.

use egui::{Color32, ColorImage, Context, Rect, TextureHandle, TextureOptions};
use std::collections::HashMap;

/// Parse an SVG path's d into closed polylines (in the path's own units).
pub fn parse_path(d: &str) -> Vec<Vec<(f64, f64)>> {
    let mut toks: Vec<Tok> = Vec::new();
    let b = d.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i] as char;
        if c.is_ascii_alphabetic() && c != 'e' && c != 'E' {
            toks.push(Tok::Cmd(c));
            i += 1;
        } else if c == '-' || c == '.' || c.is_ascii_digit() {
            let st = i;
            let mut seen_dot = false;
            if c == '-' {
                i += 1;
            }
            while i < b.len() {
                let ch = b[i] as char;
                if ch.is_ascii_digit() {
                    i += 1;
                } else if ch == '.' && !seen_dot {
                    seen_dot = true;
                    i += 1;
                } else if (ch == 'e' || ch == 'E') && i + 1 < b.len() {
                    i += 1;
                    if b[i] == b'-' || b[i] == b'+' {
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            toks.push(Tok::Num(d[st..i].parse().unwrap_or(0.0)));
        } else {
            i += 1;
        }
    }
    let mut out: Vec<Vec<(f64, f64)>> = Vec::new();
    let mut cur: Vec<(f64, f64)> = Vec::new();
    let (mut x, mut y, mut sx, mut sy) = (0.0, 0.0, 0.0, 0.0);
    let (mut lcx, mut lcy) = (0.0, 0.0);
    let mut cmd = 'M';
    let mut k = 0;
    let num = |k: &mut usize| -> f64 {
        while *k < toks.len() {
            if let Tok::Num(v) = toks[*k] {
                *k += 1;
                return v;
            }
            break;
        }
        0.0
    };
    let has_num = |k: usize| k < toks.len() && matches!(toks[k], Tok::Num(_));
    while k < toks.len() {
        if let Tok::Cmd(c) = toks[k] {
            cmd = c;
            k += 1;
            if c == 'Z' || c == 'z' {
                cur.push((sx, sy));
                if cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
                x = sx;
                y = sy;
                continue;
            }
        } else if !has_num(k) {
            k += 1;
            continue;
        }
        let rel = cmd.is_ascii_lowercase();
        let (bx, by) = if rel { (x, y) } else { (0.0, 0.0) };
        match cmd.to_ascii_uppercase() {
            'M' => {
                if cur.len() > 1 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
                x = bx + num(&mut k);
                y = by + num(&mut k);
                sx = x;
                sy = y;
                cur.push((x, y));
                cmd = if rel { 'l' } else { 'L' };
            }
            'L' => {
                x = bx + num(&mut k);
                y = by + num(&mut k);
                cur.push((x, y));
            }
            'H' => {
                x = (if rel { x } else { 0.0 }) + num(&mut k);
                cur.push((x, y));
            }
            'V' => {
                y = (if rel { y } else { 0.0 }) + num(&mut k);
                cur.push((x, y));
            }
            'C' | 'S' => {
                let (x1, y1) = if cmd.to_ascii_uppercase() == 'C' {
                    (bx + num(&mut k), by + num(&mut k))
                } else {
                    (2.0 * x - lcx, 2.0 * y - lcy)
                };
                let (x2, y2) = (bx + num(&mut k), by + num(&mut k));
                let (x3, y3) = (bx + num(&mut k), by + num(&mut k));
                for s in 1..=12 {
                    let t = s as f64 / 12.0;
                    let u = 1.0 - t;
                    cur.push((
                        u * u * u * x + 3.0 * u * u * t * x1 + 3.0 * u * t * t * x2 + t * t * t * x3,
                        u * u * u * y + 3.0 * u * u * t * y1 + 3.0 * u * t * t * y2 + t * t * t * y3,
                    ));
                }
                lcx = x2;
                lcy = y2;
                x = x3;
                y = y3;
                continue;
            }
            'Q' => {
                let (x1, y1) = (bx + num(&mut k), by + num(&mut k));
                let (x2, y2) = (bx + num(&mut k), by + num(&mut k));
                for s in 1..=10 {
                    let t = s as f64 / 10.0;
                    let u = 1.0 - t;
                    cur.push((u * u * x + 2.0 * u * t * x1 + t * t * x2, u * u * y + 2.0 * u * t * y1 + t * t * y2));
                }
                x = x2;
                y = y2;
            }
            'A' => {
                let (rx, ry, rot, large, sweep) = (num(&mut k), num(&mut k), num(&mut k), num(&mut k), num(&mut k));
                let (x2, y2) = (bx + num(&mut k), by + num(&mut k));
                arc_to(&mut cur, (x, y), (x2, y2), rx, ry, rot, large != 0.0, sweep != 0.0);
                x = x2;
                y = y2;
            }
            _ => {
                k += 1;
            }
        }
        lcx = x;
        lcy = y;
    }
    if cur.len() > 1 {
        out.push(cur);
    }
    out
}
#[derive(Clone, Copy)]
enum Tok {
    Cmd(char),
    Num(f64),
}
fn arc_to(out: &mut Vec<(f64, f64)>, p0: (f64, f64), p1: (f64, f64), rx: f64, ry: f64, rot: f64, large: bool, sweep: bool) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 {
        out.push(p1);
        return;
    }
    let phi = rot.to_radians();
    let (c, s) = (phi.cos(), phi.sin());
    let (dx, dy) = ((p0.0 - p1.0) / 2.0, (p0.1 - p1.1) / 2.0);
    let (x1, y1) = (c * dx + s * dy, -s * dx + c * dy);
    let l = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry);
    if l > 1.0 {
        rx *= l.sqrt();
        ry *= l.sqrt();
    }
    let num = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut f = (num / den).sqrt();
    if large == sweep {
        f = -f;
    }
    let (cx1, cy1) = (f * rx * y1 / ry, -f * ry * x1 / rx);
    let (cx, cy) = (c * cx1 - s * cy1 + (p0.0 + p1.0) / 2.0, s * cx1 + c * cy1 + (p0.1 + p1.1) / 2.0);
    let ang = |ux: f64, uy: f64, vx: f64, vy: f64| (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    let t1 = ang(1.0, 0.0, (x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut dt = ang((x1 - cx1) / rx, (y1 - cy1) / ry, (-x1 - cx1) / rx, (-y1 - cy1) / ry);
    if !sweep && dt > 0.0 {
        dt -= std::f64::consts::TAU;
    } else if sweep && dt < 0.0 {
        dt += std::f64::consts::TAU;
    }
    let n = ((dt.abs() / 0.2).ceil() as usize).max(2);
    for i in 1..=n {
        let t = t1 + dt * i as f64 / n as f64;
        let (ex, ey) = (rx * t.cos(), ry * t.sin());
        out.push((c * ex - s * ey + cx, s * ex + c * ey + cy));
    }
}

/// Fill polylines even-odd into an n×n alpha mask (4×4 supersampled), from a `view`-unit square.
pub fn raster(polys: &[Vec<(f64, f64)>], view: f64, n: usize, flip: bool) -> Vec<u8> {
    let ss = 4;
    let m = n * ss;
    let k = m as f64 / view;
    let mut acc = vec![0u16; n * n];
    let mut xs: Vec<f64> = Vec::new();
    for row in 0..m {
        let y = (row as f64 + 0.5) / k;
        xs.clear();
        for p in polys {
            for i in 0..p.len() {
                let (a, b) = (p[i], p[(i + 1) % p.len()]);
                if (a.1 <= y) != (b.1 <= y) {
                    xs.push(a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0));
                }
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for pair in xs.chunks(2) {
            if pair.len() < 2 {
                break;
            }
            let c0 = ((pair[0] * k - 0.5).ceil().max(0.0)) as usize;
            let c1 = ((pair[1] * k - 0.5).floor().min(m as f64 - 1.0)) as isize;
            for col in c0 as isize..=c1 {
                let mut cx = col as usize / ss;
                if flip {
                    cx = n - 1 - cx;
                }
                acc[(row / ss) * n + cx] += 1;
            }
        }
    }
    acc.iter().map(|&v| ((v as u32 * 255) / (ss * ss) as u32).min(255) as u8).collect()
}

/// Icons as textures, made on first use.
#[derive(Default)]
pub struct Icons {
    tex: HashMap<(String, bool), TextureHandle>,
}
impl Icons {
    pub fn get(&mut self, ctx: &Context, name: &str, flip: bool) -> Option<egui::TextureId> {
        if let Some(t) = self.tex.get(&(name.to_string(), flip)) {
            return Some(t.id());
        }
        let paths = super::icon_data::ICONS.iter().find(|(n, _)| *n == name)?.1;
        let polys: Vec<_> = paths.iter().flat_map(|d| parse_path(d)).collect();
        let n = 72;
        let a = raster(&polys, 15.0, n, flip);
        let img = ColorImage::from_rgba_premultiplied([n, n], &a.iter().flat_map(|&v| [v, v, v, v]).collect::<Vec<_>>());
        let t = ctx.load_texture(format!("icon-{name}-{flip}"), img, TextureOptions::LINEAR);
        let id = t.id();
        self.tex.insert((name.to_string(), flip), t);
        Some(id)
    }
    pub fn paint(&mut self, painter: &egui::Painter, name: &str, flip: bool, rect: Rect, tint: Color32) {
        if let Some(id) = self.get(painter.ctx(), name, flip) {
            painter.image(id, rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), tint);
        }
    }
}
