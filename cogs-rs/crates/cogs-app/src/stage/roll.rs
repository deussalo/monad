//! The notes roll: a disc unwrapped into a strip, time left to right, rings as rows (stage.js rollSection, drawRoll,
//! phone placeRoll). Touch edits as FL Mobile's does: tap empty grid for a note, drag a note to move it (it sounds),
//! drag the tab on a selected note's end to resize, double tap a note to delete it, hold empty grid to box notes, slide
//! empty grid to scroll, two fingers to zoom and scroll. A velocity lane can sit under it.

use super::draw::*;
use super::pen::*;
use super::StageOut;
use cogs_sim::{wrap, Cog, Dot, Part, World, TAU};
use egui::{pos2, vec2, Align2, Color32, FontId, Painter, Rect, Stroke};
use std::collections::{BTreeMap, HashMap};

pub const LANE_H: f64 = 56.0;
/// A row's height at zoom 1: thumb-sized, and never stretched to fill the box.
const ROW0: f64 = 38.0;
/// The keys column at the left: wide enough for "C#4" and a thumb.
pub const LABEL: f64 = 34.0;
/// A step's least width at zoom 1. Fewer steps than fit spread out, up to `MAX_ASPECT` rows wide; more scroll sideways.
const MIN_CELL: f64 = 34.0;
const MAX_ASPECT: f64 = 2.5;
/// The line under the grid that says what a note's look means.
pub const LEGEND_H: f64 = 20.0;
/// Pinch bounds, per axis: rows stay finger-sized, a step never far narrower or wider than its row.
const ZY: (f64, f64) = (0.75, 1.6);
const ROW_TO_CELL: (f64, f64) = (0.6, 4.0);
const MIN_LEN: f64 = 1.0 / 16.0;

/// Snap, as FL Studio's menu: [name, value, unit (s: steps, n: note value, b: bar), short label].
pub const SNAPS: [(&str, f64, char, &str); 17] = [
    ("none", 0.0, 's', "off"), ("1/6 step", 1.0 / 6.0, 's', "step/6"), ("1/4 step", 0.25, 's', "step/4"), ("1/3 step", 1.0 / 3.0, 's', "step/3"),
    ("1/2 step", 0.5, 's', "step/2"), ("step", 1.0, 's', "step"), ("1/32", 1.0 / 32.0, 'n', "1/32"), ("1/16 triplet", 1.0 / 24.0, 'n', "1/16T"),
    ("1/16", 1.0 / 16.0, 'n', "1/16"), ("1/8 triplet", 1.0 / 12.0, 'n', "1/8T"), ("1/8", 0.125, 'n', "1/8"), ("1/4 triplet", 1.0 / 6.0, 'n', "1/4T"),
    ("1/4 (beat)", 0.25, 'n', "1/4"), ("1/2 triplet", 1.0 / 3.0, 'n', "1/2T"), ("1/2", 0.5, 'n', "1/2"), ("whole", 1.0, 'n', "1/1"), ("bar", 1.0, 'b', "bar"),
];

fn tidy(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}
fn snap_down(x: f64, g: f64) -> f64 {
    if g != 0.0 { tidy((x / g + 1e-6).floor() * g) } else { x }
}
fn same_cell(a: &Dot, b: &Dot) -> bool {
    a.ring == b.ring && wrap_pi(a.a - b.a).abs() < 1e-6
}

enum Press {
    Move { from: Vec<(usize, f64, f64)>, x0: f64, ring: f64, moved: bool, heard: f64, fresh: bool },
    Stretch { lead: usize, x0: f64, set: Vec<(usize, f64)>, moved: bool },
    Marquee { hit: Option<usize> },
}
struct Hold {
    at: (f64, f64),
    s: (f64, f64),
    t: f64,
}
struct Two {
    d: f64,
    z: f64,
    zy: f64,
    sx: f64,
    sy: f64,
    col: f64,
    row: f64,
    mid0: (f64, f64),
}

/// The geometry of one frame: the strip's box on screen, its cell sizes, and the disc's steps and rings.
#[derive(Clone, Copy)]
pub struct Geo {
    pub rect: Rect,
    pub row: f64,
    pub cw: f64,
    pub div: f64,
    pub rings: usize,
    pub lane: bool,
}
impl Geo {
    pub fn w(&self) -> f64 {
        LABEL + self.cw * self.div
    }
    pub fn h(&self) -> f64 {
        self.row * self.rings as f64
    }
    /// The grid's height in the box (the lane, when shown, takes the box's bottom).
    pub fn grid_h(&self) -> f64 {
        self.rect.height() as f64 - if self.lane { LANE_H } else { 0.0 } - LEGEND_H
    }
    /// The box's height when nothing caps it: every row, the lane if shown, the legend.
    pub fn content_h(&self) -> f64 {
        self.h() + if self.lane { LANE_H } else { 0.0 } + LEGEND_H
    }
}

/// The keys the roll's previews sound under: clear of a part's own (id × 128 + note) and of the synth editor's.
pub const PREVIEW_KEYS: u32 = 0x7000_0000;

pub struct Roll {
    pub cog: u32,
    /// Zoom across (time) and down (rows), each within its bounds.
    pub zx: f64,
    pub zy: f64,
    pub sx: f64,
    pub sy: f64,
    pub sel: Vec<usize>,
    pub snap: usize,
    pub vel: bool,
    pub clip: Vec<(f64, f64, f64, f64, f64)>,
    pub status: Option<String>,
    anchor: Option<f64>,
    pen_len: HashMap<u32, f64>,
    press: Option<Press>,
    marquee: Option<(f64, f64, f64, f64)>,
    grabbed: Option<usize>,
    touches: BTreeMap<u64, (f64, f64)>,
    two: Option<Two>,
    pan: Option<((f64, f64), f64, f64)>,
    hold: Option<Hold>,
    last_tap: Option<(usize, f64)>,
    second: Option<usize>,
    lane_on: bool,
    preview: Option<(u32, f64)>,
    /// Counts previews, so each sounds under a key of its own (see `play`).
    preview_n: u32,
    /// A finger on the piano keys.
    keys: Option<u64>,
    /// The Euclid popover's pattern, ghosted on its ring: (ring, pulses, rotation).
    pub ghost: Option<(f64, i64, i64)>,
}

impl Roll {
    pub fn new(cog: u32) -> Roll {
        Roll {
            cog, zx: 1.0, zy: 1.0, sx: 0.0, sy: 0.0, sel: Vec::new(), snap: 5, vel: false, clip: Vec::new(), status: None, anchor: None,
            pen_len: HashMap::new(), press: None, marquee: None, grabbed: None, touches: BTreeMap::new(), two: None, pan: None, hold: None,
            last_tap: None, second: None, lane_on: false, preview: None, preview_n: 0, keys: None, ghost: None,
        }
    }
    /// A new disc: the same roll settings, a fresh selection.
    pub fn retarget(&mut self, cog: u32) {
        if cog != self.cog {
            self.cog = cog;
            self.sel.clear();
            self.anchor = None;
            self.sx = 0.0;
            self.sy = 0.0;
        }
    }
    /// The strip's geometry in a box rect_w wide and at most max_h tall. Rows keep their own height; the box is as tall
    /// as they are (to max_h, then it scrolls). Steps spread to the width when few, else keep a least width and scroll.
    pub fn geo(&self, w: &World, rect_w: f64, max_h: f64) -> Option<Geo> {
        let c = w.cog(self.cog)?;
        let t = c.track.as_ref()?;
        let (row, cw) = self.cells(rect_w, t.div.max(1.0), self.zx, self.zy);
        let div = t.div.max(1.0);
        let rings = t.rings();
        let mut g = Geo { rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(rect_w as f32, 0.0)), row, cw, div, rings, lane: self.vel };
        let h = g.content_h().min(max_h.max(LEGEND_H + row.min(g.content_h())));
        g.rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(rect_w as f32, h as f32));
        Some(g)
    }
    /// A row's height and a step's width at zooms zx, zy.
    fn cells(&self, rect_w: f64, div: f64, zx: f64, zy: f64) -> (f64, f64) {
        let row = (ROW0 * zy).round();
        let fit = ((rect_w - LABEL) / div).floor();
        let base = fit.min(ROW0 * MAX_ASPECT).max(MIN_CELL);
        let cw = (base * zx).round().clamp(row * ROW_TO_CELL.0, row * ROW_TO_CELL.1).max(4.0);
        (row, cw)
    }
    /// zx's bounds for this width and these steps: a step from ROW_TO_CELL.0 to ROW_TO_CELL.1 rows wide.
    fn zx_bounds(&self, rect_w: f64, div: f64, zy: f64) -> (f64, f64) {
        let row = (ROW0 * zy).round();
        let base = ((rect_w - LABEL) / div).floor().min(ROW0 * MAX_ASPECT).max(MIN_CELL);
        // and no narrower than needed to see the whole disc, where that keeps the aspect
        let least = ((rect_w - LABEL) / div / base).min(1.0);
        ((row * ROW_TO_CELL.0 / base).max(least.min(1.0)).min(1.0), row * ROW_TO_CELL.1 / base)
    }
    fn clamp_scroll(&mut self, g: &Geo) {
        let bw = g.rect.width() as f64;
        self.sx = self.sx.clamp(0.0, (g.w() - bw).max(0.0));
        self.sy = self.sy.clamp(0.0, (g.h() - g.grid_h()).max(0.0));
    }

    // ---------- the disc's grid ----------
    fn snap_of(&self, w: &World, c: &Cog) -> f64 {
        let (_, v, k, _) = SNAPS[self.snap];
        let t = c.track.as_ref().unwrap();
        let rpm = (c.w * 60.0 / TAU).abs();
        let spb = if w.s.bpm != 0.0 && rpm > 1e-6 { t.div * rpm / w.s.bpm } else { t.div / w.s.beats };
        match k {
            's' => v,
            'n' => v * 4.0 * spb,
            _ => w.s.beats * spb,
        }
    }
    fn roll_u(c: &Cog, a: f64) -> f64 {
        wrap(-roll_dir(c) * a) / TAU
    }
    fn col_angle(c: &Cog, col: f64) -> f64 {
        wrap(-roll_dir(c) * col * TAU / c.track.as_ref().unwrap().div)
    }
    fn pos(c: &Cog, d: &Dot) -> f64 {
        let n = c.track.as_ref().unwrap().div;
        let x = tidy(Roll::roll_u(c, d.a) * n);
        if x >= n { x - n } else { x }
    }
    fn end(c: &Cog, d: &Dot) -> f64 {
        let n = c.track.as_ref().unwrap().div;
        let e = Roll::roll_u(c, d.a) * n + d.len;
        if e > n { e - n } else { e }
    }
    /// The pointer in grid units: columns from the first step, rows from the top ring. s is in box coordinates.
    fn at(&self, g: &Geo, s: (f64, f64)) -> (f64, f64) {
        ((s.0 + self.sx - LABEL) / g.cw, (s.1 + self.sy) / g.row)
    }
    /// The cell under a point: its column, x (snapped down), ring and angle.
    fn cell(&self, w: &World, c: &Cog, at: (f64, f64)) -> Option<(f64, f64, f64)> {
        let t = c.track.as_ref()?;
        let (col, row) = (at.0.floor(), at.1.floor());
        if col < 0.0 || col >= t.div || row < 0.0 || row >= t.rings() as f64 {
            return None;
        }
        let x = snap_down(at.0, self.snap_of(w, c)).min(t.div);
        Some((x, (t.rings() - 1) as f64 - row, Roll::col_angle(c, x)))
    }
    /// The note under a point as drawn: its own span, wrapping; where notes overlap, the shortest on top.
    fn hit(c: &Cog, at: (f64, f64)) -> Option<usize> {
        let t = c.track.as_ref()?;
        let n = t.div;
        let ring = (t.rings() - 1) as f64 - at.1.floor();
        let mut best: Option<usize> = None;
        for (i, d) in t.dots.iter().enumerate() {
            if d.ring != ring {
                continue;
            }
            let off = ((at.0 - Roll::roll_u(c, d.a) * n) % n + n) % n;
            if off < d.len && best.map_or(true, |b| d.len <= t.dots[b].len) {
                best = Some(i);
            }
        }
        best
    }
    /// The length tab nearest the point on a selected note, within reach of a finger.
    fn grip(&self, c: &Cog, g: &Geo, at: (f64, f64)) -> Option<usize> {
        let t = c.track.as_ref()?;
        let r = t.rings() as f64;
        let mut best = None;
        let mut bd = f64::INFINITY;
        for &i in &self.sel {
            let Some(d) = t.dots.get(i) else { continue };
            if d.ring >= r {
                continue;
            }
            let dx = (at.0 - Roll::end(c, d)) * g.cw;
            let dy = (at.1 - (r - 1.0 - d.ring + 0.5)).abs() * g.row;
            let inner = (d.len * g.cw * 0.15).min(5.0);
            if dx < -inner || dx > 30.0 || dy > g.row {
                continue;
            }
            if dx > 12.0 {
                if let Some(o) = Roll::hit(c, at) {
                    if o != i {
                        continue;
                    }
                }
            }
            let score = dx.abs() + dy;
            if score < bd {
                bd = score;
                best = Some(i);
            }
        }
        best
    }
    fn play(&mut self, w: &World, out: &mut Vec<StageOut>, ring: f64, now: f64) {
        let Some(c) = w.cog(self.cog) else { return };
        let t = c.track.as_ref().unwrap();
        let reader = w.readers.iter().find(|r| super::seg_dist((c.x, c.y), r.x1, r.y1, r.x2, r.y2) < c.radius()).map(|r| Part::Reader(r.id));
        // a key of its own each time: the app lets go of the last preview after sending this one, and under one key
        // that release cut the new pitch off as it began, so a drag across rows was all but silent
        self.preview_n = self.preview_n.wrapping_add(1);
        let key = PREVIEW_KEYS | (self.preview_n & 0x00ff_ffff);
        if let Some((last, _)) = self.preview {
            out.push(StageOut::AuditionOff { key: last, after: 0.0 });
        }
        out.push(StageOut::Audition { part: reader, midi: t.ring_note(ring as i64), key });
        self.preview = Some((key, now));
    }
    fn preview_off(&mut self, out: &mut Vec<StageOut>, now: f64) {
        if let Some((key, at)) = self.preview.take() {
            out.push(StageOut::AuditionOff { key, after: (at + 0.18 - now).max(0.0) });
        }
    }

    // ---------- the selection ----------
    fn move_sel(c: &mut Cog, from: &[(usize, f64, f64)], dc: f64, dr: f64) {
        let dir = roll_dir(c);
        let t = c.track.as_mut().unwrap();
        let lo = from.iter().map(|f| f.2).fold(f64::INFINITY, f64::min);
        let hi = from.iter().map(|f| f.2).fold(f64::NEG_INFINITY, f64::max);
        let dr = dr.clamp(-lo, (t.rings() - 1) as f64 - hi);
        let div = t.div;
        for &(i, a, ring) in from {
            if let Some(d) = t.dots.get_mut(i) {
                d.a = wrap(a - dir * dc * TAU / div);
                d.ring = ring + dr;
            }
        }
    }
    /// Remove the dots for which keep is false, keeping the selection's indices right.
    fn retain(&mut self, t: &mut cogs_sim::Track, keep: impl Fn(usize, &Dot) -> bool) {
        let mut map = vec![usize::MAX; t.dots.len()];
        let mut n = 0;
        let mut out = Vec::with_capacity(t.dots.len());
        for (i, d) in t.dots.drain(..).enumerate() {
            if keep(i, &d) {
                map[i] = n;
                n += 1;
                out.push(d);
            }
        }
        t.dots = out;
        self.sel = self.sel.iter().filter_map(|&i| map.get(i).copied().filter(|&j| j != usize::MAX)).collect();
        if let Some(g) = self.grabbed {
            self.grabbed = map.get(g).copied().filter(|&j| j != usize::MAX);
        }
    }
    /// A selected pin landing on another pin replaces it.
    fn settle(&mut self, t: &mut cogs_sim::Track) {
        let sel: Vec<Dot> = self.sel.iter().filter_map(|&i| t.dots.get(i).cloned()).collect();
        let ids = self.sel.clone();
        self.retain(t, |i, d| ids.contains(&i) || !sel.iter().any(|s| same_cell(s, d)));
    }
    pub fn has_sel(&self) -> bool {
        !self.sel.is_empty()
    }
    pub fn delete_sel(&mut self, w: &mut World) -> bool {
        let Some(t) = w.cog_mut(self.cog).and_then(|c| c.track.as_mut()) else { return false };
        if self.sel.is_empty() {
            return false;
        }
        let ids = self.sel.clone();
        self.retain(t, |i, _| !ids.contains(&i));
        self.sel.clear();
        true
    }
    pub fn copy_sel(&mut self, w: &World) -> bool {
        let Some(c) = w.cog(self.cog) else { return false };
        let t = c.track.as_ref().unwrap();
        let sel: Vec<&Dot> = self.sel.iter().filter_map(|&i| t.dots.get(i)).collect();
        if sel.is_empty() {
            return false;
        }
        let cols: Vec<f64> = sel.iter().map(|d| Roll::pos(c, d)).collect();
        let min = cols.iter().copied().fold(f64::INFINITY, f64::min);
        self.clip = sel.iter().zip(cols).map(|(d, x)| (tidy(x - min), d.ring, d.p, d.v, d.len)).collect();
        true
    }
    pub fn paste(&mut self, w: &mut World) -> bool {
        if self.clip.is_empty() {
            return false;
        }
        let Some(ci) = w.cog_ix(self.cog) else { return false };
        let snap = self.snap_of(w, &w.cogs[ci]);
        let head = w.cogs[ci].track.as_ref().map(|t| t.div).and_then(|div| self.playhead_u(w, ci).map(|u| snap_down(u * div, if snap != 0.0 { snap } else { 1.0 })));
        let at = self.anchor.or(head).unwrap_or(0.0);
        let c = w.cogs[ci].clone();
        let t = w.cogs[ci].track.as_mut().unwrap();
        self.sel.clear();
        for &(dc, ring, p, v, len) in &self.clip.clone() {
            let mut d = Dot::new(ring.min((t.rings() - 1) as f64), Roll::col_angle(&c, (at + dc) % t.div));
            d.p = p;
            d.v = v;
            d.len = len;
            t.dots.push(d);
            self.sel.push(t.dots.len() - 1);
        }
        self.settle(t);
        true
    }
    /// Duplicate, as FL's ctrl+B: a copy of the selection right after it, selected in its place.
    pub fn duplicate(&mut self, w: &mut World) -> bool {
        let Some(ci) = w.cog_ix(self.cog) else { return false };
        let c = w.cogs[ci].clone();
        let t = w.cogs[ci].track.as_mut().unwrap();
        let sel: Vec<Dot> = self.sel.iter().filter_map(|&i| t.dots.get(i).cloned()).collect();
        if sel.is_empty() {
            return false;
        }
        let ends = sel.iter().map(|d| Roll::pos(&c, d) + d.len).fold(f64::NEG_INFINITY, f64::max);
        let starts = sel.iter().map(|d| Roll::pos(&c, d)).fold(f64::INFINITY, f64::min);
        let span = tidy(ends - starts);
        self.sel.clear();
        for d in sel {
            let mut e = d.clone();
            e.a = wrap(d.a - roll_dir(&c) * span * TAU / t.div);
            t.dots.push(e);
            self.sel.push(t.dots.len() - 1);
        }
        self.settle(t);
        true
    }
    /// Where the disc's readers cross the middle ring, as fractions of the strip.
    fn playheads(w: &World, ci: usize) -> Vec<(usize, f64)> {
        let c = &w.cogs[ci];
        let t = c.track.as_ref().unwrap();
        let r = c.ring_r(((t.rings() - 1) / 2) as f64);
        let th = w.render_th(ci);
        let mut out = Vec::new();
        for (ri, rd) in w.readers.iter().enumerate() {
            let (dx, dy, fx, fy) = (rd.x2 - rd.x1, rd.y2 - rd.y1, rd.x1 - c.x, rd.y1 - c.y);
            let (a, b) = (dx * dx + dy * dy, 2.0 * (fx * dx + fy * dy));
            let disc = b * b - 4.0 * a * (fx * fx + fy * fy - r * r);
            if a == 0.0 || disc < 0.0 {
                continue;
            }
            for s in [(-b - disc.sqrt()) / (2.0 * a), (-b + disc.sqrt()) / (2.0 * a)] {
                if (0.0..=1.0).contains(&s) {
                    out.push((ri, Roll::roll_u(c, (fy + dy * s).atan2(fx + dx * s) - th)));
                }
            }
        }
        out
    }
    fn playhead_u(&self, w: &World, ci: usize) -> Option<f64> {
        Roll::playheads(w, ci).first().map(|p| p.1)
    }
    /// Shift every pin a step earlier (k = -1) or later.
    pub fn shift(&mut self, w: &mut World, k: f64) {
        let Some(c) = w.cog_mut(self.cog) else { return };
        let dir = roll_dir(c);
        let t = c.track.as_mut().unwrap();
        let s = dir * TAU / t.div;
        for d in &mut t.dots {
            d.a = wrap(d.a - k * s);
        }
    }
    /// Euclid fill: k pins spread evenly over the steps of one ring, rotated.
    pub fn euclid(&mut self, w: &mut World, ring: f64, k: i64, rot: i64, p: f64, v: f64) {
        let Some(c) = w.cog_mut(self.cog) else { return };
        let dir = roll_dir(c);
        let t = c.track.as_mut().unwrap();
        let ring = ring.min((t.rings() - 1) as f64);
        let n = t.div as i64;
        self.retain(t, |_, d| d.ring != ring);
        for j in 0..n {
            if ((j + rot) * k).rem_euclid(n.max(1)) < k {
                let mut d = Dot::new(ring, wrap(-dir * j as f64 * TAU / n as f64));
                d.p = p;
                d.v = v;
                t.dots.push(d);
            }
        }
    }

    // ---------- touch ----------
    /// A finger down in the box (s in box coordinates). Returns true when it changed the world.
    pub fn touch_down(&mut self, w: &mut World, out: &mut Vec<StageOut>, g: &Geo, id: u64, s: (f64, f64), now: f64) {
        // the piano keys at the left: audition the row
        if s.0 < LABEL && s.1 < g.grid_h() {
            if let Some(c) = w.cog(self.cog) {
                let ring = g.rings as f64 - 1.0 - ((s.1 + self.sy) / g.row).floor();
                if ring >= 0.0 && ring < g.rings as f64 {
                    let _ = c;
                    self.play(w, out, ring, now);
                    self.keys = Some(id);
                }
            }
            return;
        }
        self.touches.insert(id, s);
        if self.touches.len() == 2 {
            self.start_two(w, out, g, now);
            return;
        }
        if self.touches.len() > 2 || self.two.is_some() {
            return;
        }
        if g.lane && s.1 >= g.grid_h() {
            self.lane_on = false;
            self.set_vel(w, g, s);
            return;
        }
        let Some(c) = w.cog(self.cog).cloned() else { return };
        let at = self.at(g, s);
        let cell = self.cell(w, &c, at);
        let hit = cell.and(Roll::hit(&c, at));
        self.second = match (hit, self.last_tap) {
            (Some(h), Some((d, t))) if d == h && now - t < 0.4 => Some(h),
            _ => None,
        };
        self.last_tap = None;
        if cell.is_some() && hit.is_none() && self.grip(&c, g, at).is_none() {
            self.hold = Some(Hold { at, s, t: now });
            return;
        }
        self.down(w, out, g, at, now);
    }
    fn down(&mut self, w: &mut World, out: &mut Vec<StageOut>, g: &Geo, at: (f64, f64), now: f64) {
        let Some(ci) = w.cog_ix(self.cog) else { return };
        let c = w.cogs[ci].clone();
        let cell = self.cell(w, &c, at);
        let hit = cell.and(Roll::hit(&c, at));
        if let Some(cl) = cell {
            self.anchor = Some(cl.0);
        }
        if let Some(g0) = self.grip(&c, g, at) {
            let t = c.track.as_ref().unwrap();
            let set: Vec<usize> = if self.sel.contains(&g0) { self.sel.clone() } else { vec![g0] };
            self.press = Some(Press::Stretch { lead: g0, x0: at.0, set: set.iter().map(|&i| (i, t.dots[i].len)).collect(), moved: false });
            self.grabbed = Some(g0);
            out.push(StageOut::Buzz(4));
            return;
        }
        if let Some(h) = hit {
            if !self.sel.contains(&h) {
                self.sel = vec![h];
            }
            let t = c.track.as_ref().unwrap();
            let ring = t.dots[h].ring;
            self.play(w, out, ring, now);
            self.pen_len.insert(c.id, t.dots[h].len);
            let from: Vec<(usize, f64, f64)> = self.sel.iter().map(|&i| (i, t.dots[i].a, t.dots[i].ring)).collect();
            self.press = Some(Press::Move { from, x0: at.0, ring: cell.unwrap().1, moved: false, heard: ring, fresh: false });
            return;
        }
        if !self.sel.is_empty() {
            self.sel.clear();
            self.press = None;
            return;
        }
        let Some((_, ring, a)) = cell else {
            self.press = None;
            return;
        };
        let snap = self.snap_of(w, &c);
        let div = c.track.as_ref().unwrap().div;
        let mut d = Dot::new(ring, a);
        d.len = self.pen_len.get(&c.id).copied().unwrap_or(if snap != 0.0 { snap } else { 1.0 }).min(div);
        let t = w.cogs[ci].track.as_mut().unwrap();
        t.dots.push(d);
        let i = t.dots.len() - 1;
        self.play(w, out, ring, now);
        self.sel = vec![i];
        self.press = Some(Press::Move { from: vec![(i, a, ring)], x0: at.0, ring, moved: false, heard: ring, fresh: true });
    }
    pub fn touch_move(&mut self, w: &mut World, out: &mut Vec<StageOut>, g: &Geo, id: u64, s: (f64, f64), now: f64) {
        if self.keys == Some(id) {
            return;
        }
        let Some(f) = self.touches.get_mut(&id) else { return };
        *f = s;
        if self.two.is_some() {
            if self.touches.len() >= 2 {
                self.move_two(w, g);
            }
            return;
        }
        if g.lane && self.press.is_none() && self.hold.is_none() && self.pan.is_none() && s.1 >= g.grid_h() - 1.0 && self.lane_drag() {
            self.set_vel(w, g, s);
            return;
        }
        if let Some(h) = &self.hold {
            if super::dist(s, h.s) < 8.0 {
                return;
            }
            self.pan = Some((h.s, self.sx, self.sy));
            self.hold = None;
        }
        if let Some((p0, sx, sy)) = self.pan {
            self.sx = sx - (s.0 - p0.0);
            self.sy = sy - (s.1 - p0.1);
            self.clamp_scroll(g);
            return;
        }
        self.mv(w, out, g, self.at(g, s), now);
    }
    fn lane_drag(&self) -> bool {
        self.lane_on || self.status.as_deref().map_or(false, |s| s.starts_with("velocity"))
    }
    fn mv(&mut self, w: &mut World, out: &mut Vec<StageOut>, g: &Geo, at: (f64, f64), now: f64) {
        let Some(ci) = w.cog_ix(self.cog) else { return };
        if let Some(m) = &mut self.marquee {
            m.2 = at.0;
            m.3 = at.1;
            let (x0, x1, y0, y1) = (m.0.min(m.2), m.0.max(m.2), m.1.min(m.3), m.1.max(m.3));
            let c = &w.cogs[ci];
            let n = c.track.as_ref().unwrap().dots.iter().filter(|d| Roll::in_box(c, d, x0, x1, y0, y1)).count();
            self.status = Some(format!("select notes: {n}"));
            return;
        }
        let c = w.cogs[ci].clone();
        let snap = self.snap_of(w, &c);
        let rings = g.rings as f64;
        let mut play_ring = None;
        match &mut self.press {
            Some(Press::Move { from, x0, ring, moved, heard, .. }) => {
                let dc = if snap != 0.0 {
                    tidy(((at.0 / snap + 1e-6).floor() - (*x0 / snap + 1e-6).floor()) * snap)
                } else if *moved || (at.0 - *x0).abs() * g.cw >= 4.0 {
                    at.0 - *x0
                } else {
                    0.0
                };
                let dr = rings - 1.0 - at.1.floor() - *ring;
                if dc != 0.0 || dr != 0.0 || *moved {
                    *moved = true;
                    Roll::move_sel(&mut w.cogs[ci], from, dc, dr);
                }
                let lead = w.cogs[ci].track.as_ref().unwrap().dots.get(from[0].0).map(|d| d.ring).unwrap_or(*heard);
                if lead != *heard {
                    *heard = lead;
                    play_ring = Some(lead);
                }
            }
            Some(Press::Stretch { lead, x0, set, moved }) => {
                let t = w.cogs[ci].track.as_ref().unwrap();
                let Some(ld) = t.dots.get(*lead) else { return };
                let head = Roll::pos(&c, ld);
                let len0 = set.iter().find(|s| s.0 == *lead).map(|s| s.1).unwrap_or(ld.len);
                let end = head + len0 + at.0 - *x0;
                let was = ld.len;
                let want = if snap != 0.0 {
                    tidy((end / snap).round() * snap).max(tidy(((head / snap + 1e-6).floor() + 1.0) * snap))
                } else {
                    end.max(head + MIN_LEN)
                };
                let delta = tidy(want - head - len0);
                let div = t.div;
                let t = w.cogs[ci].track.as_mut().unwrap();
                for &(i, l0) in set.iter() {
                    if let Some(d) = t.dots.get_mut(i) {
                        d.len = tidy(l0 + delta).clamp(l0.min(if snap != 0.0 { snap } else { MIN_LEN }), div);
                    }
                }
                let now_len = t.dots[*lead].len;
                if now_len != was {
                    *moved = true;
                    out.push(StageOut::Buzz(3));
                    play_ring = Some(t.dots[*lead].ring);
                    self.pen_len.insert(c.id, now_len);
                }
                self.status = Some(format!("note length: {} step{}", setting2(now_len), if now_len == 1.0 { "" } else { "s" }));
            }
            _ => {}
        }
        if let Some(r) = play_ring {
            self.play(w, out, r, now);
        }
    }
    fn in_box(c: &Cog, d: &Dot, x0: f64, x1: f64, y0: f64, y1: f64) -> bool {
        let t = c.track.as_ref().unwrap();
        let col = Roll::roll_u(c, d.a) * t.div;
        let row = (t.rings() - 1) as f64 - d.ring;
        d.ring < t.rings() as f64 && col < x1 && col + d.len > x0 && row < y1 && row + 1.0 > y0
    }
    /// A finger up. Returns true when the world changed (an undo step).
    pub fn touch_up(&mut self, w: &mut World, out: &mut Vec<StageOut>, g: &Geo, id: u64, cancel: bool, now: f64) -> bool {
        if self.keys == Some(id) {
            self.keys = None;
            self.preview_off(out, now);
            return false;
        }
        if self.touches.remove(&id).is_none() {
            return false;
        }
        if self.two.is_some() {
            if self.touches.is_empty() {
                self.two = None;
            }
            return false;
        }
        if self.lane_on {
            self.lane_on = false;
            self.status = None;
            return true;
        }
        if self.pan.take().is_some() {
            return false;
        }
        if let Some(h) = self.hold.take() {
            if cancel {
                return false;
            }
            self.down(w, out, g, h.at, now);
        }
        let second = self.second.take();
        let (changed, tapped) = self.up(w, out, g, now);
        if let (Some(d2), Some(t)) = (second, tapped) {
            if d2 == t {
                if let Some(tr) = w.cog_mut(self.cog).and_then(|c| c.track.as_mut()) {
                    self.retain(tr, |i, _| i != d2);
                }
                self.sel.retain(|&i| i != d2);
                out.push(StageOut::Buzz(12));
                self.last_tap = None;
                return true;
            }
        }
        self.last_tap = tapped.map(|d| (d, now));
        changed
    }
    /// Returns (changed, the note a still press tapped).
    fn up(&mut self, w: &mut World, out: &mut Vec<StageOut>, g: &Geo, now: f64) -> (bool, Option<usize>) {
        let p = self.press.take();
        let m = self.marquee.take();
        self.grabbed = None;
        self.status = None;
        self.preview_off(out, now);
        let Some(ci) = w.cog_ix(self.cog) else { return (false, None) };
        if let Some((mx0, my0, mx1, my1)) = m {
            let (x0, x1, y0, y1) = (mx0.min(mx1), mx0.max(mx1), my0.min(my1), my0.max(my1));
            if ((x1 - x0) * g.cw).hypot((y1 - y0) * g.row) < 4.0 {
                if let Some(Press::Marquee { hit: Some(h) }) = p {
                    if let Some(k) = self.sel.iter().position(|&i| i == h) {
                        self.sel.remove(k);
                    } else {
                        self.sel.push(h);
                    }
                }
                return (false, None);
            }
            self.sel.clear();
            let c = &w.cogs[ci];
            for (i, d) in c.track.as_ref().unwrap().dots.iter().enumerate() {
                if Roll::in_box(c, d, x0, x1, y0, y1) {
                    self.sel.push(i);
                }
            }
            return (false, None);
        }
        match p {
            Some(Press::Move { from, moved, fresh, .. }) => {
                let lead = from.first().map(|f| f.0);
                if moved || fresh {
                    let t = w.cogs[ci].track.as_mut().unwrap();
                    self.settle(t);
                    if fresh {
                        self.sel.clear();
                    }
                    return (true, None);
                }
                (false, lead)
            }
            Some(Press::Stretch { lead, moved, .. }) => (moved, if moved { None } else { Some(lead) }),
            _ => (false, None),
        }
    }
    /// Holds and the like: called each frame.
    pub fn tick(&mut self, now: f64, out: &mut Vec<StageOut>) {
        if let Some(h) = &self.hold {
            if now - h.t >= 0.38 {
                let at = h.at;
                self.hold = None;
                self.sel.clear();
                self.press = Some(Press::Marquee { hit: None });
                self.marquee = Some((at.0, at.1, at.0, at.1));
                self.status = Some("select notes: 0".into());
                out.push(StageOut::Buzz(10));
            }
        }
    }
    fn start_two(&mut self, w: &mut World, out: &mut Vec<StageOut>, g: &Geo, now: f64) {
        self.hold = None;
        self.pan = None;
        // a second finger undoes what the first began
        if let Some(p) = self.press.take() {
            if let Some(ci) = w.cog_ix(self.cog) {
                let t = w.cogs[ci].track.as_mut().unwrap();
                match p {
                    Press::Move { from, fresh: true, .. } => {
                        let ids: Vec<usize> = from.iter().map(|f| f.0).collect();
                        self.retain(t, |i, _| !ids.contains(&i));
                    }
                    Press::Move { from, .. } => {
                        for (i, a, ring) in from {
                            if let Some(d) = t.dots.get_mut(i) {
                                d.a = a;
                                d.ring = ring;
                            }
                        }
                    }
                    Press::Stretch { set, .. } => {
                        for (i, len) in set {
                            if let Some(d) = t.dots.get_mut(i) {
                                d.len = len;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        self.marquee = None;
        self.grabbed = None;
        self.status = None;
        self.preview_off(out, now);
        let (mid, d) = self.pair();
        let (sx, sy) = self.spans();
        self.two = Some(Two { d, z: self.zx, zy: self.zy, sx, sy, col: (self.sx + mid.0 - LABEL) / g.cw, row: (self.sy + mid.1) / g.row, mid0: mid });
    }
    /// The two fingers' spread across and down.
    fn spans(&self) -> (f64, f64) {
        let mut it = self.touches.values();
        let (a, b) = (*it.next().unwrap(), *it.next().unwrap());
        ((a.0 - b.0).abs(), (a.1 - b.1).abs())
    }
    fn pair(&self) -> ((f64, f64), f64) {
        let mut it = self.touches.values();
        let (a, b) = (*it.next().unwrap(), *it.next().unwrap());
        (((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), super::dist(a, b).max(1.0))
    }
    fn move_two(&mut self, w: &World, g: &Geo) {
        let (mid, _) = self.pair();
        let (sx, sy) = self.spans();
        let Some(t) = &self.two else { return };
        let (col, row) = (t.col, t.row);
        // each axis zooms by the fingers' spread along it; a spread mostly along one leaves the other be
        let along = |s: f64, s0: f64, other: f64, other0: f64| if s0 > 40.0 && s0 >= 0.5 * other0 && s > 0.0 && (s / s0 - 1.0).abs() > 0.02 || (other0 < 40.0 && s0 > 20.0) { s / s0 } else { let _ = other; 1.0 };
        let (kx, ky) = (along(sx, t.sx, sy, t.sy), along(sy, t.sy, sx, t.sx));
        self.zy = (t.zy * ky).clamp(ZY.0, ZY.1);
        let (lo, hi) = self.zx_bounds(g.rect.width() as f64, g.div, self.zy);
        self.zx = (t.z * kx).clamp(lo, hi);
        let Some(g2) = self.geo(w, g.rect.width() as f64, g.rect.height() as f64) else { return };
        self.sx = LABEL + col * g2.cw - mid.0;
        self.sy = row * g2.row - mid.1;
        let mut g3 = *g;
        g3.cw = g2.cw;
        g3.row = g2.row;
        self.clamp_scroll(&g3);
    }
    /// The velocity lane: the bars under the finger take its height (with a selection, only selected ones there).
    fn set_vel(&mut self, w: &mut World, g: &Geo, s: (f64, f64)) {
        let Some(c) = w.cog(self.cog).cloned() else { return };
        let col = ((s.0 + self.sx - LABEL) / g.cw).floor();
        let y = s.1 - g.grid_h();
        let v = ((1.0 - (y - 4.0) / (LANE_H - 8.0)) * 100.0).round().clamp(5.0, 100.0) / 100.0;
        self.status = Some(format!("velocity: {}%", (v * 100.0).round()));
        let t = c.track.as_ref().unwrap();
        let mut at: Vec<usize> = (0..t.dots.len()).filter(|&i| {
            let d = &t.dots[i];
            d.ring < t.rings() as f64 && (Roll::pos(&c, d) + 1e-6).floor() == col
        }).collect();
        if at.iter().any(|i| self.sel.contains(i)) {
            at.retain(|i| self.sel.contains(i));
        }
        if at.is_empty() {
            return;
        }
        let t = w.cog_mut(self.cog).unwrap().track.as_mut().unwrap();
        for i in at {
            t.dots[i].v = v;
        }
        self.lane_on = true;
    }

    // ---------- drawing ----------
    pub fn draw(&self, base: &Painter, w: &World, g: &Geo, now: f64) {
        let Some(ci) = w.cog_ix(self.cog) else { return };
        let c = &w.cogs[ci];
        let Some(t) = c.track.as_ref() else { return };
        let r = g.rect;
        let gh = g.grid_h();
        let grid = Rect::from_min_size(r.min, vec2(r.width(), gh as f32));
        let p = base.with_clip_rect(grid);
        p.rect_filled(Rect::from_min_size(r.min, r.size()), 0.0, BG);
        let gw = g.cw * g.div;
        let rh = g.row;
        let ox = r.min.x as f64 + LABEL - self.sx;
        let oy = r.min.y as f64 - self.sy;
        let rings = t.rings();
        let xy = |x: f64, y: f64| pos2(x as f32, y as f32);
        for row in 0..rings {
            let note = t.ring_note((rings - 1 - row) as i64) as i64;
            if note.rem_euclid(12) == (t.base() as i64).rem_euclid(12) {
                p.rect_filled(Rect::from_min_max(xy(ox, oy + row as f64 * rh), xy(ox + gw, oy + (row + 1) as f64 * rh)), 0.0, PAD);
            }
        }
        for k in 0..=(g.div as usize) {
            let x = (ox + k as f64 * g.cw).floor() + 0.5;
            p.line_segment([xy(x, oy), xy(x, oy + g.h())], Stroke::new(1.0, if k % 4 != 0 { FAINT } else { DIM }));
        }
        // snap lines: whole snaps mark their multiples, a snap off the step grid dashes them
        let sg = self.snap_of(w, c);
        let whole = tidy(sg) == tidy(sg).trunc();
        if sg != 0.0 && sg != 1.0 && sg * g.cw >= 4.0 {
            let mut k = 1.0;
            while k * sg < g.div - 1e-6 {
                let pp = tidy(k * sg);
                k += 1.0;
                if !whole && pp == pp.trunc() {
                    continue;
                }
                let x = (ox + pp * g.cw).round() + 0.5;
                if whole {
                    p.line_segment([xy(x, oy), xy(x, oy + g.h())], Stroke::new(1.0, DIM));
                } else {
                    p.extend(egui::Shape::dashed_line(&[xy(x, oy), xy(x, oy + g.h())], Stroke::new(1.0, FAINT), 2.0, 3.0));
                }
            }
        }
        for rr in 0..=rings {
            let y = oy + rr as f64 * rh + 0.5;
            p.line_segment([xy(ox, y), xy(ox + gw, y)], Stroke::new(1.0, FAINT));
        }
        // notes, longest first so a shorter one over a longer is on top
        let mut order: Vec<usize> = (0..t.dots.len()).collect();
        order.sort_by(|a, b| t.dots[*b].len.partial_cmp(&t.dots[*a].len).unwrap_or(std::cmp::Ordering::Equal));
        let pen = Pen::new(&p, 1.0, 0.0, 0.0, w.clock.view);
        for i in order {
            let d = &t.dots[i];
            if d.ring >= rings as f64 {
                continue;
            }
            // every note the same shape, inset from its cell so neighbours read apart: brightness is its velocity, a
            // dashed edge and a percentage its chance, gold the moment it sounds
            let inset = (rh * 0.12).clamp(3.0, 6.0);
            let top = oy + (rings as f64 - 1.0 - d.ring) * rh + inset;
            let hgt = rh - 2.0 * inset;
            let px = ox + Roll::roll_u(c, d.a) * gw + 1.5;
            let on = self.sel.contains(&i);
            let wd = (d.len * g.cw - 3.0).max(2.0);
            let base_col = if on { SEL } else { lerp_col(super::pen::RAISE, FG, d.v as f32) };
            let col = pen.hot(d.flash, base_col);
            let fill = if d.p < 1.0 { alpha(col, (0.25 + 0.5 * d.p) as f32) } else { col };
            let right = ox + gw;
            let parts = [(px, (px + wd).min(right)), (ox, ox + px + wd - right)];
            for (k, &(x0, x1)) in parts.iter().enumerate() {
                if k == 1 && px + wd <= right {
                    break;
                }
                let rr = Rect::from_min_max(xy(x0, top), xy(x1, top + hgt));
                p.rect_filled(rr, 3.0, fill);
                if d.p < 1.0 {
                    let pts = [rr.left_top(), rr.right_top(), rr.right_bottom(), rr.left_bottom(), rr.left_top()];
                    p.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.2, col), 3.0, 2.5));
                }
            }
            if d.p < 1.0 && wd >= 30.0 && hgt >= 16.0 {
                p.text(xy(px + wd.min(right - px) / 2.0, top + hgt / 2.0), Align2::CENTER_CENTER, format!("{}%", (d.p * 100.0).round()), FontId::proportional(11.0), FG);
            }
            if on {
                let y0 = oy + (rings as f64 - 1.0 - d.ring) * rh + 1.5;
                p.rect_stroke(Rect::from_min_max(xy(px - 1.0, y0), xy(px + wd.min(right - px) + 1.0, y0 + rh - 3.0)), 3.0, Stroke::new(1.0, SEL), egui::StrokeKind::Inside);
                // the length grip: a tab on its right end
                let ex = ox + Roll::end(c, d) * g.cw;
                let tw = (g.cw * 0.35).round().clamp(4.0, 12.0);
                let th = (rh * 0.72).round().max(4.0);
                let ty = oy + (rings as f64 - 1.0 - d.ring) * rh + (rh - th) / 2.0;
                let x0 = (ex - 2.0).round();
                let held = self.grabbed == Some(i);
                p.rect_filled(Rect::from_min_size(xy(x0, ty), vec2(tw as f32, th as f32)), (tw / 2.0).min(3.0) as f32, if held { HOT } else { SEL });
                if tw >= 8.0 && th >= 12.0 {
                    for k in [-1.0, 1.0] {
                        p.rect_filled(Rect::from_min_size(xy(x0 + tw / 2.0 + k * 1.5 - 0.5, ty + th * 0.3), vec2(1.0, (th * 0.4) as f32)), 0.0, BG);
                    }
                }
            }
        }
        if let Some((ring, k, rot)) = self.ghost {
            let n = g.div as i64;
            let y = oy + (rings as f64 - 1.0 - ring) * rh;
            for j in 0..n {
                if k > 0 && ((j + rot) * k).rem_euclid(n.max(1)) < k {
                    let x = ox + j as f64 * g.cw;
                    p.rect_stroke(Rect::from_min_max(xy(x + 2.0, y + 2.0), xy(x + g.cw - 2.0, y + rh - 2.0)), 3.0, Stroke::new(1.5, alpha(HOT, 0.8)), egui::StrokeKind::Inside);
                }
            }
        }
        for (ri, u) in Roll::playheads(w, ci) {
            let x = ox + u * gw;
            p.line_segment([xy(x, oy), xy(x, oy + g.h())], Stroke::new(2.0, pen.hot(w.readers[ri].flash, READ)));
        }
        if let Some((x0, y0, x1, y1)) = self.marquee {
            let rr = Rect::from_two_pos(xy(ox + x0 * g.cw + 0.5, oy + y0 * rh + 0.5), xy(ox + x1 * g.cw + 0.5, oy + y1 * rh + 0.5));
            let pts = [rr.left_top(), rr.right_top(), rr.right_bottom(), rr.left_bottom(), rr.left_top()];
            p.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.0, SEL), 5.0, 4.0));
        }
        // the keys, fixed at the left
        let keys = Rect::from_min_size(r.min, vec2(LABEL as f32, gh as f32));
        let kp = p.with_clip_rect(keys);
        kp.rect_filled(keys, 0.0, BG);
        for row in 0..rings {
            let note = t.ring_note((rings - 1 - row) as i64) as i64;
            let y = oy + row as f64 * rh;
            let black = [1, 3, 6, 8, 10].contains(&note.rem_euclid(12));
            kp.rect_filled(Rect::from_min_size(xy(r.min.x as f64, y), vec2(LABEL as f32, rh as f32)), 0.0, if black { BG } else { PAD });
            kp.text(xy(r.min.x as f64 + 5.0, y + rh / 2.0), Align2::LEFT_CENTER, note_name(note), FontId::proportional(12.0), if black { super::pen::CAPTION } else { FG });
            kp.rect_filled(Rect::from_min_size(xy(r.min.x as f64, y + rh - 1.0), vec2(LABEL as f32, 1.0)), 0.0, FAINT);
        }
        let _ = now;
        // the whole disc at a glance when the strip is wider than the box: what is in view, and the readers
        if g.w() > r.width() as f64 {
            let x0 = r.min.x as f64 + LABEL;
            let bw = r.width() as f64 - LABEL;
            let band = Rect::from_min_size(xy(x0, r.min.y as f64), vec2(bw as f32, 5.0));
            p.rect_filled(band, 0.0, alpha(BG, 0.9));
            let (u0, u1) = (self.sx / gw, ((self.sx + bw) / gw).min(1.0));
            p.rect_filled(Rect::from_min_max(xy(x0 + u0 * bw, r.min.y as f64 + 1.0), xy(x0 + u1 * bw, r.min.y as f64 + 4.0)), 1.0, DIM);
            for (_, u) in Roll::playheads(w, ci) {
                let x = x0 + u * bw;
                p.line_segment([xy(x, r.min.y as f64), xy(x, r.min.y as f64 + 5.0)], Stroke::new(2.0, READ));
            }
        }
        if g.lane {
            self.draw_lane(base, w, c, g);
        }
        // what a note's look means, under the grid
        let ly = r.max.y as f64 - LEGEND_H / 2.0;
        let lx = r.min.x as f64 + 8.0;
        let font = FontId::proportional(11.0);
        let cap = super::pen::CAPTION;
        let mut x = lx;
        for (sw, word) in [(0u8, "velocity: brightness"), (1, "chance: dashed"), (2, "sounding: gold")] {
            let s = Rect::from_center_size(xy(x + 6.0, ly), vec2(12.0, 9.0));
            match sw {
                0 => {
                    base.rect_filled(Rect::from_min_max(s.min, pos2(s.center().x, s.max.y)), 2.0, super::pen::DIM);
                    base.rect_filled(Rect::from_min_max(pos2(s.center().x, s.min.y), s.max), 2.0, FG);
                }
                1 => {
                    base.rect_filled(s, 2.0, alpha(FG, 0.4));
                    let pts = [s.left_top(), s.right_top(), s.right_bottom(), s.left_bottom(), s.left_top()];
                    base.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.0, FG), 2.0, 2.0));
                }
                _ => {
                    base.rect_filled(s, 2.0, HOT);
                }
            }
            let gal = base.layout_no_wrap(word.to_string(), font.clone(), cap);
            let tw = gal.size().x as f64;
            base.galley(xy(x + 16.0, ly - gal.size().y as f64 / 2.0), gal, cap);
            x += 16.0 + tw + 14.0;
        }
    }
    fn draw_lane(&self, p: &Painter, _w: &World, c: &Cog, g: &Geo) {
        let t = c.track.as_ref().unwrap();
        let r = g.rect;
        let top = r.min.y as f64 + g.grid_h();
        let lane = Rect::from_min_max(pos2(r.min.x, top as f32), pos2(r.max.x, (top + LANE_H) as f32));
        let p = p.with_clip_rect(lane);
        p.rect_filled(lane, 0.0, BG);
        p.line_segment([lane.left_top(), lane.right_top()], Stroke::new(1.0, LINE));
        let ox = r.min.x as f64 + LABEL - self.sx;
        let xy = |x: f64, y: f64| pos2(x as f32, y as f32);
        for k in 0..=(g.div as usize) {
            let x = (ox + k as f64 * g.cw).floor() + 0.5;
            p.line_segment([xy(x, top), xy(x, top + LANE_H)], Stroke::new(1.0, if k % 4 != 0 { FAINT } else { DIM }));
        }
        let bw = (g.cw * 0.3).round().clamp(3.0, 10.0);
        let some = !self.sel.is_empty();
        for (i, d) in t.dots.iter().enumerate() {
            if d.ring >= t.rings() as f64 {
                continue;
            }
            let on = self.sel.contains(&i);
            let h = (d.v * (LANE_H - 8.0)).max(2.0);
            let bx = ox + Roll::pos(c, d) * g.cw + (g.cw.min(d.len * g.cw) - bw) / 2.0;
            let col = alpha(if on { SEL } else { FG }, if some && !on { 0.4 } else { 1.0 });
            p.rect_filled(Rect::from_min_size(xy(bx, top + LANE_H - 4.0 - h), vec2(bw as f32, h as f32)), 0.0, col);
            p.rect_filled(Rect::from_min_size(xy(bx - 2.0, top + LANE_H - 4.0 - h), vec2((bw + 4.0) as f32, 2.0)), 0.0, col);
        }
        let tag = Rect::from_min_size(lane.min, vec2(LABEL as f32, LANE_H as f32));
        p.rect_filled(tag, 0.0, BG);
        p.text(tag.center(), Align2::CENTER_CENTER, "vel", FontId::proportional(11.0), DIM);
    }
}

fn setting2(x: f64) -> String {
    let s = format!("{:.2}", x);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn lerp_col(a: Color32, b: Color32, k: f32) -> Color32 {
    let k = k.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}
