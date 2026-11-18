//! pit.js: the ball pit, a sphere of balls, each a note, that sound on its wall and on each other. Arms and cogs in
//! it knock them about. Balls run at 240 Hz on the sim's clock and keep a short trail, drawn at the heard time.

use crate::model::*;
use crate::sim::{Event, Reach, Scratch, Source};
use crate::model::Voice;
use crate::voice::{js_num, num};
use crate::world::{new_trail, World};
use serde_json::{json, Map, Value};

pub const PIT_WALL: f64 = 0.92;
pub const PIT_BALL: f64 = 0.9;
pub const PIT_ARM: f64 = 0.85;
pub const PIT_DRAG: f64 = 0.12;
pub const PIT_MIN_V: f64 = 18.0;
pub const PIT_COOL: f64 = 0.045;
pub const PIT_MAX_V: f64 = 1200.0;
pub const PIT_DT: f64 = 1.0 / 240.0;
pub const PIT_TRAIL: f64 = 1.0 / 120.0;
pub const PIT_MAX: usize = 48;
pub const PIT_ARM_R: f64 = 2.0;
pub const PIT_NOTES: [i32; 6] = [60, 62, 64, 67, 69, 72];

/// A ball's size from its pitch: low notes big.
pub fn ball_r(m: i32) -> f64 {
    clamp(11.0 * 2f64.powf((60 - m) as f64 / 24.0), 6.0, 22.0)
}
pub fn make_ball(note: i32, x: f64, y: f64, vx: f64, vy: f64, id: u32) -> Ball {
    Ball { id, note, r: ball_r(note), x, y, vx, vy, at: -9.0, flash: f64::NEG_INFINITY, trail: new_trail() }
}
pub fn new_pit(id: u32, x: f64, y: f64) -> Pit {
    Pit { id, x, y, w: 220.0, h: 220.0, voice: Voice::reader(), notes: PIT_NOTES.to_vec(), balls: Vec::new(), nb: 1, bag: Vec::new(), hits: 0, last_v: 0.0, flash: f64::NEG_INFINITY }
}

pub fn pit_in(p: &Map<String, Value>) -> Result<Pit, String> {
    let n0 = |k: &str| {
        let x = js_num(p.get(k));
        if x.is_nan() { 0.0 } else { x }
    };
    let id = p.get("id").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
    let mut notes: Vec<i32> = Vec::new();
    for n in p.get("notes").and_then(|v| v.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
        let m = clamp(to_int32(js_num(Some(n))) as f64, 0.0, 127.0) as i32;
        if !notes.contains(&m) {
            notes.push(m);
        }
    }
    notes.sort();
    let mut o = new_pit(id, n0("x"), n0("y"));
    o.voice = Voice::import(p.get("voice").unwrap_or(&Value::Null), id)?;
    if !notes.is_empty() {
        o.notes = notes;
    }
    // a sphere; a box saved before it was one becomes the sphere of its mean side
    let (w, h) = (js_num(p.get("w")), js_num(p.get("h")));
    let tw = crate::voice::truthy(p.get("w")) && crate::voice::truthy(p.get("h"));
    let d = if tw { (w + h) / 2.0 } else if w.is_nan() || w == 0.0 { 220.0 } else { w };
    o.w = js_max(60.0, d);
    o.h = o.w;
    for bv in p.get("balls").and_then(|v| v.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
        let Some(b) = bv.as_object() else { continue };
        let n = clamp(to_int32(js_num(b.get("note"))) as f64, 0.0, 127.0) as i32;
        let pos = |k: &str, d: f64| match b.get(k) {
            None | Some(Value::Null) => d,
            v => js_num(v),
        };
        let vel = |k: &str| {
            let x = js_num(b.get(k));
            if x.is_nan() { 0.0 } else { x }
        };
        o.balls.push(make_ball(n, pos("bx", o.x), pos("by", o.y), vel("bvx"), vel("bvy"), to_int32(js_num(b.get("id"))).max(0) as u32));
    }
    o.nb = 1 + o.balls.iter().map(|b| b.id).max().unwrap_or(0);
    Ok(o)
}
pub fn pit_out(p: &Pit) -> Value {
    json!({"id":p.id,"x":num(p.x),"y":num(p.y),"w":num(p.w),"h":num(p.h),"voice":p.voice.to_json(),"notes":p.notes,
        "balls":p.balls.iter().map(|b| json!({"id":b.id,"note":b.note,"bx":num(b.x),"by":num(b.y),"bvx":num(b.vx),"bvy":num(b.vy)})).collect::<Vec<_>>()})
}

/// Where a ball is at time t: between the two trail points either side of it.
pub fn ball_at(b: &Ball, t: f64) -> (f64, f64) {
    let tr = &b.trail;
    if tr.is_empty() {
        return (b.x, b.y);
    }
    if t <= tr[0][0] {
        return (tr[0][1], tr[0][2]);
    }
    for i in (1..tr.len()).rev() {
        let (a, c) = (tr[i - 1], tr[i]);
        if t >= a[0] {
            if t >= c[0] {
                return (c[1], c[2]);
            }
            let d = c[0] - a[0];
            let f = (t - a[0]) / if d == 0.0 { 1.0 } else { d };
            return (a[1] + (c[1] - a[1]) * f, a[2] + (c[2] - a[2]) * f);
        }
    }
    (tr[0][1], tr[0][2])
}

impl Pit {
    pub fn radius(&self) -> f64 {
        self.w / 2.0
    }
    /// The palette changed: a ball whose note is gone takes the nearest that is left.
    pub fn retune(&mut self) {
        self.bag.clear();
        if self.notes.is_empty() {
            return;
        }
        for b in &mut self.balls {
            if !self.notes.contains(&b.note) {
                b.note = self.notes.iter().copied().fold(self.notes[0], |a, n| if (n - b.note).abs() < (a - b.note).abs() { n } else { a });
                b.r = ball_r(b.note);
            }
        }
    }
    /// Move the box and everything in it, trails too.
    pub fn shift(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
        for b in &mut self.balls {
            b.x += dx;
            b.y += dy;
            for s in &mut b.trail {
                s[1] += dx;
                s[2] += dy;
            }
        }
    }
    /// The ball under world point (x, y) as drawn at time t, topmost first.
    pub fn ball_under(&self, x: f64, y: f64, t: f64, zoom: f64) -> Option<usize> {
        (0..self.balls.len()).rev().find(|&i| {
            let b = &self.balls[i];
            let q = ball_at(b, t);
            hypot(q.0 - x, q.1 - y) <= b.r + 4.0 / zoom
        })
    }
}

impl World {
    pub(crate) fn rand(&mut self) -> f64 {
        // xorshift64*: the JS uses Math.random here (shuffling the note bag, a tap's direction)
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        (x.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
    /// pitNote: the next note from the box's shuffled bag, so every note comes round before one repeats.
    pub fn pit_note(&mut self, pi: usize) -> i32 {
        if self.pits[pi].bag.is_empty() {
            let mut bag = self.pits[pi].notes.clone();
            for i in (1..bag.len()).rev() {
                let j = (self.rand() * (i + 1) as f64).floor() as usize;
                bag.swap(i, j);
            }
            self.pits[pi].bag = bag;
        }
        self.pits[pi].bag.pop().unwrap_or(60)
    }
    /// dropBall: a ball dropped or thrown now, at (x, y) as drawn; the sim, already ahead, takes it where it will be
    /// by then. Returns its id.
    pub fn drop_ball(&mut self, pi: usize, x: f64, y: f64, vx: f64, vy: f64, note: Option<i32>) -> u32 {
        let note = match note {
            Some(n) => n,
            None => self.pit_note(pi),
        };
        let ahead = js_max(0.0, self.clock.sim_t - self.clock.view);
        let view = self.clock.view;
        let p = &mut self.pits[pi];
        let lim = js_max(0.0, p.radius() - ball_r(note));
        let cl = |qx: f64, qy: f64| {
            let (dx, dy) = (qx - p.x, qy - p.y);
            let d = hypot(dx, dy);
            if d <= lim { (qx, qy) } else { (p.x + dx / d * lim, p.y + dy / d * lim) }
        };
        let (x, y) = cl(x, y);
        let w = cl(x + vx * ahead, y + vy * ahead);
        let id = p.nb;
        p.nb += 1;
        let mut b = make_ball(note, w.0, w.1, vx, vy, id);
        b.trail.push_back([view, x, y]);
        if p.balls.len() >= PIT_MAX {
            p.balls.remove(0);
        }
        p.balls.push(b);
        id
    }

    /// pitReach: for each box with balls, the arms whose sweep crosses it and the cogs that overlap it.
    pub(crate) fn pit_reach(&self, sc: &mut Scratch) {
        sc.reach.clear();
        sc.reach_arms.clear();
        sc.reach_cogs.clear();
        for (pi, p) in self.pits.iter().enumerate() {
            if p.balls.is_empty() {
                continue;
            }
            let (l, r, t, b) = (p.x - p.w / 2.0, p.x + p.w / 2.0, p.y - p.h / 2.0, p.y + p.h / 2.0);
            let near = |x: f64, y: f64, rr: f64| x + rr > l && x - rr < r && y + rr > t && y - rr < b;
            let a0 = sc.reach_arms.len();
            for (ai, a) in self.arms.iter().enumerate() {
                let c = &self.cogs[a.cog_ix];
                if near(c.x, c.y, a.r0 + a.len + PIT_ARM_R + 22.0) {
                    sc.reach_arms.push(ai);
                }
            }
            let c0 = sc.reach_cogs.len();
            for (ci, c) in self.cogs.iter().enumerate() {
                if near(c.x, c.y, c.radius() + 25.0) {
                    sc.reach_cogs.push(ci);
                }
            }
            sc.reach.push(Reach { p: pi, arms: (a0, sc.reach_arms.len()), cogs: (c0, sc.reach_cogs.len()) });
        }
    }

    fn ball_hit(&mut self, pi: usize, bi: usize, vn: f64, t: f64, x: f64, y: f64, out: &mut impl FnMut(Event)) {
        let p = &mut self.pits[pi];
        let b = &mut p.balls[bi];
        if t - b.at < PIT_COOL {
            return;
        }
        b.at = t;
        b.flash = t;
        let (note, bid) = (b.note, b.id);
        p.hits += 1;
        p.last_v = vn;
        p.flash = t;
        let (pid, kind) = (p.id, p.voice.kind);
        let e = Event::new(t, Source::Ball { pit: pid, ball: bid }, Part::Pit(pid), Some(kind), note as f64, pid * 128 + note.max(0) as u32, vn, x, y);
        self.fire(e, 1.0, out);
    }

    /// pitStep: one step of a box's balls: drag, each other, arms and cogs, then the wall.
    pub(crate) fn pit_step(&mut self, sc: &Scratch, q: Reach, dt: f64, t: f64, out: &mut impl FnMut(Event)) {
        let pi = q.p;
        let dk = (-PIT_DRAG * dt).exp();
        {
            let p = &mut self.pits[pi];
            for b in &mut p.balls {
                b.vx *= dk;
                b.vy *= dk;
                let sp = hypot(b.vx, b.vy);
                if sp > PIT_MAX_V {
                    b.vx *= PIT_MAX_V / sp;
                    b.vy *= PIT_MAX_V / sp;
                }
                b.x += b.vx * dt;
                b.y += b.vy * dt;
            }
        }
        // balls on each other: pushed apart by mass (the square of the radius), bounced, and both sound
        let n = self.pits[pi].balls.len();
        for i in 0..n {
            for j in i + 1..n {
                let bs = &mut self.pits[pi].balls;
                let (a, b) = {
                    let (l, r) = bs.split_at_mut(j);
                    (&mut l[i], &mut r[0])
                };
                let (mut dx, dy) = (b.x - a.x, b.y - a.y);
                let min = a.r + b.r;
                let d2 = dx * dx + dy * dy;
                if d2 >= min * min {
                    continue;
                }
                let mut d = d2.sqrt();
                if d < 1e-6 {
                    dx = 1e-3;
                    d = 1e-3;
                }
                let (nx, ny) = (dx / d, dy / d);
                let (ma, mb) = (a.r * a.r, b.r * b.r);
                let ov = min - d;
                a.x -= nx * ov * mb / (ma + mb);
                a.y -= ny * ov * mb / (ma + mb);
                b.x += nx * ov * ma / (ma + mb);
                b.y += ny * ov * ma / (ma + mb);
                let closing = (b.vx - a.vx) * nx + (b.vy - a.vy) * ny;
                if closing < 0.0 {
                    let jj = -(1.0 + PIT_BALL) * closing / (1.0 / ma + 1.0 / mb);
                    a.vx -= jj * nx / ma;
                    a.vy -= jj * ny / ma;
                    b.vx += jj * nx / mb;
                    b.vy += jj * ny / mb;
                    if -closing > PIT_MIN_V {
                        let (hx, hy) = (a.x + nx * a.r, a.y + ny * a.r);
                        self.ball_hit(pi, i, -closing, t, hx, hy, out);
                        self.ball_hit(pi, j, -closing, t, hx, hy, out);
                    }
                }
            }
        }
        // arms sweep through and bat them, cogs shove them along their rims: neither sounds
        for bi in 0..n {
            for k in q.arms.0..q.arms.1 {
                let ai = sc.reach_arms[k];
                let a = &self.arms[ai];
                let c = &self.cogs[a.cog_ix];
                let ang = c.th + a.a;
                let (ux, uy) = (ang.cos(), ang.sin());
                let (x0, y0) = (c.x + ux * a.r0, c.y + uy * a.r0);
                let (cw, cx, cy, alen) = (c.w, c.x, c.y, a.len);
                let b = &mut self.pits[pi].balls[bi];
                let u = clamp((b.x - x0) * ux + (b.y - y0) * uy, 0.0, alen);
                let (qx, qy) = (x0 + ux * u, y0 + uy * u);
                let (mut dx, mut dy) = (b.x - qx, b.y - qy);
                let min = b.r + PIT_ARM_R;
                let d2 = dx * dx + dy * dy;
                if d2 >= min * min {
                    continue;
                }
                let mut d = d2.sqrt();
                if d < 1e-6 {
                    dx = -uy;
                    dy = ux;
                    d = 1.0;
                }
                let (nx, ny) = (dx / d, dy / d);
                b.x = qx + nx * min;
                b.y = qy + ny * min;
                let (wx, wy) = (-cw * (qy - cy), cw * (qx - cx));
                let vn = (b.vx - wx) * nx + (b.vy - wy) * ny;
                if vn < 0.0 {
                    b.vx -= (1.0 + PIT_ARM) * vn * nx;
                    b.vy -= (1.0 + PIT_ARM) * vn * ny;
                    self.arms[ai].flash = t;
                }
            }
            for k in q.cogs.0..q.cogs.1 {
                let c = &self.cogs[sc.reach_cogs[k]];
                let (cx, cy, cw, rad) = (c.x, c.y, c.w, c.radius());
                let b = &mut self.pits[pi].balls[bi];
                let (mut dx, dy) = (b.x - cx, b.y - cy);
                let min = rad + b.r;
                let d2 = dx * dx + dy * dy;
                if d2 >= min * min {
                    continue;
                }
                let mut d = d2.sqrt();
                if d < 1e-6 {
                    dx = 1.0;
                    d = 1.0;
                }
                let (nx, ny) = (dx / d, dy / d);
                b.x = cx + nx * min;
                b.y = cy + ny * min;
                let (rx, ry) = (-cw * ny * rad, cw * nx * rad);
                let vn = (b.vx - rx) * nx + (b.vy - ry) * ny;
                if vn < 0.0 {
                    b.vx -= (1.0 + PIT_ARM) * vn * nx;
                    b.vy -= (1.0 + PIT_ARM) * vn * ny;
                }
                // the rim drags the ball a little along with it
                let (tx, ty) = (-ny, nx);
                let vt = (b.vx - rx) * tx + (b.vy - ry) * ty;
                b.vx -= 0.25 * vt * tx;
                b.vy -= 0.25 * vt * ty;
            }
        }
        // the wall last, so nothing leaves the sphere
        let (px, py, rw) = (self.pits[pi].x, self.pits[pi].y, self.pits[pi].radius());
        let view = self.clock.view;
        for bi in 0..n {
            let b = &mut self.pits[pi].balls[bi];
            let (mut dx, dy) = (b.x - px, b.y - py);
            let lim = rw - b.r;
            let mut d = hypot(dx, dy);
            let mut hit = None;
            if lim <= 0.0 {
                b.x = px;
                b.y = py;
            } else if d > lim {
                if d < 1e-9 {
                    dx = 1.0;
                    d = 1.0;
                }
                let (nx, ny) = (dx / d, dy / d);
                let vn = b.vx * nx + b.vy * ny;
                b.x = px + nx * lim;
                b.y = py + ny * lim;
                if vn > 0.0 {
                    b.vx -= (1.0 + PIT_WALL) * vn * nx;
                    b.vy -= (1.0 + PIT_WALL) * vn * ny;
                    if vn > PIT_MIN_V {
                        hit = Some((vn, px + nx * rw, py + ny * rw));
                    }
                }
            }
            if let Some((vn, hx, hy)) = hit {
                self.ball_hit(pi, bi, vn, t, hx, hy, out);
            }
            let b = &mut self.pits[pi].balls[bi];
            let tr = &mut b.trail;
            let push = match tr.back() {
                None => true,
                Some(last) => t + dt - last[0] >= PIT_TRAIL,
            };
            if push {
                tr.push_back([t + dt, b.x, b.y]);
                if tr.len() > 4 && tr[1][0] < view - 0.05 {
                    tr.pop_front();
                }
                while tr.len() > 240 {
                    tr.pop_front();
                }
            }
        }
    }
}
