//! Stepping the machine: sim.js advance(), the hits it finds (an arm striking a pad, a pin crossing a reader, an arm
//! plucking a string, balls in a box), and fire() up to where it hands to audio, reported as `Event`s.

use crate::model::*;
use crate::world::World;

/// Who made a sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// `arm{arm}>pad{pad}`
    Arm { arm: u32, pad: u32 },
    /// `cog{cog}.ring{ring}>reader{reader}`
    Dot { cog: u32, ring: u32, reader: u32 },
    /// `arm{arm}>string{string}`
    Pluck { arm: u32, string: u32 },
    /// `ball{pit}.{ball}`
    Ball { pit: u32, ball: u32 },
}
impl Source {
    /// The JS log's source tag.
    pub fn tag(&self) -> String {
        match *self {
            Source::Arm { arm, pad } => format!("arm{arm}>pad{pad}"),
            Source::Dot { cog, ring, reader } => format!("cog{cog}.ring{ring}>reader{reader}"),
            Source::Pluck { arm, string } => format!("arm{arm}>string{string}"),
            Source::Ball { pit, ball } => format!("ball{pit}.{ball}"),
        }
    }
}

/// One FX field a sound is sent to: the field's slot, the send (0..1, rounded as the JS posts it) and how far in it
/// is (0 at the rim, 1 at the centre: what bends the field's parameters).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Send {
    pub slot: u8,
    pub amount: f32,
    pub q: f32,
}

/// A sound the machine made, at `t` seconds of sim time: what fire() computes before the listener. The listener
/// (pan, the gain and darkening with distance from the camera) is the app's: it has `x`, `y`.
#[derive(Clone, Copy, Debug)]
pub struct Event {
    pub t: f64,
    pub src: Source,
    /// The part whose voice plays it. `Part::String` plays the string's own modal strike (stage pluckSound): its
    /// partials from `World::strings`, struck at `u` along it.
    pub part: Part,
    pub kind: Option<VoiceKind>,
    /// The note as handed to playVoice: before the voice's own octave and semitones (`Voice::midi` applies them).
    /// Fractional for an unquantised string.
    pub midi: f64,
    /// The engine's key for this note: part id × 128 + note, as the JS keys strikes, dampers and note-offs.
    pub key: u32,
    /// gain × level × velocity response, with probes on the voice applied.
    pub amp: f64,
    /// vx: the strike speed over S.vRef, 0..1 (what sets contact time and the wave synth's velocity).
    pub vel: f64,
    /// The raw strike speed in px/s.
    pub vn: f64,
    /// Seconds to hold (a pin longer than one step), 0 for a strike.
    pub hold: f64,
    /// Where along a string it was plucked, 0..1.
    pub u: f64,
    /// World position of the sound.
    pub x: f64,
    pub y: f64,
    /// The field sends at (x, y): `sends[..n_sends]`, and the dry level left.
    pub sends: [Send; FIELD_SLOTS_N],
    pub n_sends: u8,
    pub dry: f64,
    /// Knobs probes set on this strike (drums and the modal synth): `over[..n_over]`, key and value.
    pub over: [(&'static str, f64); 4],
    pub n_over: u8,
}
impl Event {
    pub(crate) fn new(t: f64, src: Source, part: Part, kind: Option<VoiceKind>, midi: f64, key: u32, vn: f64, x: f64, y: f64) -> Event {
        Event {
            t, src, part, kind, midi, key, amp: 0.0, vel: 0.0, vn, hold: 0.0, u: 0.0, x, y,
            sends: [Send::default(); FIELD_SLOTS_N], n_sends: 0, dry: 1.0, over: [("", 0.0); 4], n_over: 0,
        }
    }
    pub fn sends(&self) -> &[Send] {
        &self.sends[..self.n_sends as usize]
    }
    pub fn overrides(&self) -> &[(&'static str, f64)] {
        &self.over[..self.n_over as usize]
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Pair {
    c: usize,
    arm: usize,
    pad: usize,
    s: f64,
    edge: f64,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Reach {
    pub p: usize,
    pub arms: (usize, usize),
    pub cogs: (usize, usize),
}
/// advance()'s per-call lists, kept between calls so stepping allocates nothing once warm.
#[derive(Clone, Debug, Default)]
pub(crate) struct Scratch {
    pairs: Vec<Pair>,
    plucks: Vec<(usize, usize)>,
    reads: Vec<(usize, usize, usize)>,
    pub reach: Vec<Reach>,
    pub reach_arms: Vec<usize>,
    pub reach_cogs: Vec<usize>,
}

/// sim.js chance: a hash of the seed and the numbers, 0..1.
pub fn chance(seed: f64, n: &[f64]) -> f64 {
    let mut h: u32 = (to_int32(seed) as u32) ^ 0x9e3779b9;
    for &x in n {
        h = (h ^ (to_int32(js_round(x * 4096.0)) as u32)).wrapping_mul(0x85ebca6b);
        h ^= h >> 13;
        h = h.wrapping_mul(0xc2b2ae35);
        h ^= h >> 16;
    }
    h as f64 / 4294967296.0
}

/// An arm's hit window on a pad: the angle of the pad's centre, the half width, and the reach where they touch.
pub fn hit_window(c: &Cog, arm: &Arm, pad: &Pad) -> Option<(f64, f64, f64)> {
    let (dx, dy) = (pad.x - c.x, pad.y - c.y);
    let d = hypot(dx, dy);
    let rho = pad.r;
    if d <= rho {
        return None;
    }
    let s = clamp((d * d - rho * rho).sqrt(), js_max(arm.r0, 1e-6), arm.r0 + arm.len);
    let cosd = (s * s + d * d - rho * rho) / (2.0 * s * d);
    if cosd >= 1.0 || cosd <= -1.0 {
        return None;
    }
    Some((dy.atan2(dx), cosd.acos(), s))
}

pub struct Cross {
    pub f: f64,
    pub u: f64,
    pub x: f64,
    pub y: f64,
    pub vn: f64,
}
/// Where the circle of radius r about cog c, from angle a0 over dt, crosses a segment.
pub fn circle_cross(c: &Cog, r: f64, a0: f64, dt: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> Option<Cross> {
    let a1 = a0 + c.w * dt;
    let (px0, py0, px1, py1) = (c.x + a0.cos() * r, c.y + a0.sin() * r, c.x + a1.cos() * r, c.y + a1.sin() * r);
    let (ex, ey) = (x2 - x1, y2 - y1);
    let s0 = ex * (py0 - y1) - ey * (px0 - x1);
    let s1 = ex * (py1 - y1) - ey * (px1 - x1);
    // Half open: a dot exactly on the line crosses as it leaves; one that ended the last step there never twice.
    let on0 = s0.abs() < 1e-9;
    let on1 = s1.abs() < 1e-9;
    if on1 || !(on0 || s0 < 0.0 && s1 > 0.0 || s0 > 0.0 && s1 < 0.0) {
        return None;
    }
    let f = if on0 { 0.0 } else { s0 / (s0 - s1) };
    let (qx, qy) = (px0 + (px1 - px0) * f, py0 + (py1 - py0) * f);
    let l2 = ex * ex + ey * ey;
    let u = ((qx - x1) * ex + (qy - y1) * ey) / l2;
    if u < 0.0 || u > 1.0 {
        return None;
    }
    let am = a0 + c.w * dt * f;
    Some(Cross { f, u, x: qx, y: qy, vn: (c.w * r * (am.sin() * ey + am.cos() * ex)).abs() / l2.sqrt() })
}
fn seg_dist(px: f64, py: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    let (ex, ey) = (x2 - x1, y2 - y1);
    let l = ex * ex + ey * ey;
    let u = clamp(((px - x1) * ex + (py - y1) * ey) / if l == 0.0 { 1.0 } else { l }, 0.0, 1.0);
    hypot(px - x1 - ex * u, py - y1 - ey * u)
}
pub fn seg_dist_pt(px: f64, py: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> f64 {
    seg_dist(px, py, x1, y1, x2, y2)
}
/// A circle of radius r about c can only cross a segment that comes within r of c.
fn within(c: &Cog, r: f64, x1: f64, y1: f64, x2: f64, y2: f64) -> bool {
    seg_dist(c.x, c.y, x1, y1, x2, y2) <= r + 1e-9
}
fn turn(c: &mut Cog, dt: f64) {
    let th = c.th + c.w * dt;
    let k = (th / TAU).floor();
    c.th = th - k * TAU;
    c.turns += k;
}

impl World {
    /// sim.js advance: step the machine to `t_end` (sim seconds), reporting every sound as an Event, in step order.
    /// Called from the audio callback: allocation-free once its scratch lists have grown.
    pub fn advance(&mut self, t_end: f64, out: &mut impl FnMut(Event)) {
        if t_end - self.clock.sim_t > 0.5 {
            let gap = t_end - self.s.lookahead - self.clock.sim_t;
            for c in &mut self.cogs {
                turn(c, gap);
            }
            self.clock.sim_t += gap;
            self.stats.skipped += gap;
        }
        let mut sc = std::mem::take(&mut self.scratch);
        sc.pairs.clear();
        sc.plucks.clear();
        sc.reads.clear();
        for (ai, arm) in self.arms.iter().enumerate() {
            let c = &self.cogs[arm.cog_ix];
            if c.w == 0.0 {
                continue;
            }
            for (pi, pad) in self.pads.iter().enumerate() {
                if let Some((beta, half, s)) = hit_window(c, arm, pad) {
                    sc.pairs.push(Pair { c: arm.cog_ix, arm: ai, pad: pi, s, edge: if c.w > 0.0 { beta - half } else { beta + half } });
                }
            }
        }
        for (ai, arm) in self.arms.iter().enumerate() {
            let c = &self.cogs[arm.cog_ix];
            if c.w == 0.0 {
                continue;
            }
            for (si, s) in self.strings.iter().enumerate() {
                if within(c, arm.r0 + arm.len, s.x1, s.y1, s.x2, s.y2) {
                    sc.plucks.push((ai, si));
                }
            }
        }
        for (ci, c) in self.cogs.iter().enumerate() {
            let Some(t) = &c.track else { continue };
            if c.w == 0.0 {
                continue;
            }
            for (di, dot) in t.dots.iter().enumerate() {
                let rr = c.ring_r(dot.ring);
                for (ri, rd) in self.readers.iter().enumerate() {
                    if within(c, rr, rd.x1, rd.y1, rd.x2, rd.y2) {
                        sc.reads.push((ci, di, ri));
                    }
                }
            }
        }
        let mut max_w: f64 = 0.0;
        for c in &self.cogs {
            max_w = js_max(max_w, c.w.abs());
        }
        // a box with balls in it steps the world finely: they bounce at 240 Hz
        self.pit_reach(&mut sc);
        self.refresh_probes();
        self.stats.steps_tick = 0;
        while self.clock.sim_t < t_end {
            let mut dt = t_end - self.clock.sim_t;
            if max_w != 0.0 {
                dt = js_min(dt, self.s.max_step / max_w);
            }
            if !sc.reach.is_empty() {
                dt = js_min(dt, crate::pit::PIT_DT);
            }
            for i in 0..sc.pairs.len() {
                let p = sc.pairs[i];
                let c = &self.cogs[p.c];
                let psi = c.th + self.arms[p.arm].a;
                let span = c.w.abs() * dt;
                let d0 = if c.w > 0.0 { wrap(p.edge - psi) } else { wrap(psi - p.edge) };
                if d0 > 0.0 && d0 <= span {
                    let t = self.clock.sim_t + dt * d0 / span;
                    self.arm_hit(p, t, out);
                }
            }
            for i in 0..sc.reads.len() {
                let (c, d, r) = sc.reads[i];
                self.dot_cross(c, d, r, dt, out);
            }
            for i in 0..sc.plucks.len() {
                let (a, s) = sc.plucks[i];
                self.pluck(a, s, dt, out);
            }
            for i in 0..sc.reach.len() {
                let q = sc.reach[i];
                let t = self.clock.sim_t;
                self.pit_step(&sc, q, dt, t, out);
            }
            for c in &mut self.cogs {
                turn(c, dt);
            }
            self.clock.sim_t += dt;
            self.stats.steps += 1;
            self.stats.steps_tick += 1;
        }
        self.scratch = sc;
    }

    fn flex(&mut self, arm: usize, t: f64, vn: f64) {
        let w = self.cogs[self.arms[arm].cog_ix].w;
        let v_ref = self.s.v_ref;
        let a = &mut self.arms[arm];
        a.bend_t = t;
        a.bend_a = js_min(0.6, vn / v_ref * 0.5) * a.flex * if w > 0.0 { -1.0 } else { 1.0 };
    }

    fn arm_hit(&mut self, p: Pair, t: f64, out: &mut impl FnMut(Event)) {
        let (c, arm, pad) = (&self.cogs[p.c], &self.arms[p.arm], &self.pads[p.pad]);
        let (px, py) = (c.x + p.edge.cos() * p.s, c.y + p.edge.sin() * p.s);
        let nl = {
            let l = hypot(px - pad.x, py - pad.y);
            if l == 0.0 { 1.0 } else { l }
        };
        let (nx, ny) = ((px - pad.x) / nl, (py - pad.y) / nl);
        let vn = (c.w * p.s * (-p.edge.sin() * nx + p.edge.cos() * ny)).abs();
        let (aid, pid, x, y) = (arm.id, pad.id, pad.x, pad.y);
        {
            let pad = &mut self.pads[p.pad];
            pad.hits += 1;
            pad.last_v = vn;
            pad.flash = t;
        }
        self.arms[p.arm].flash = t;
        self.flex(p.arm, t, vn);
        let v = &self.pads[p.pad].voice;
        let note = to_int32(v.note) as f64;
        let e = Event::new(t, Source::Arm { arm: aid, pad: pid }, Part::Pad(pid), Some(v.kind), note, (pid as i64 * 128 + note as i64) as u32, vn, x, y);
        self.fire(e, 1.0, out);
    }

    fn dot_cross(&mut self, ci: usize, di: usize, ri: usize, dt: f64, out: &mut impl FnMut(Event)) {
        let c = &self.cogs[ci];
        let t = c.track.as_ref().unwrap();
        let dot = &t.dots[di];
        let rd = &self.readers[ri];
        let Some(hit) = circle_cross(c, c.ring_r(dot.ring), c.th + dot.a, dt, rd.x1, rd.y1, rd.x2, rd.y2) else { return };
        let tt = self.clock.sim_t + dt * hit.f;
        let turn = c.turns + ((c.th + c.w * dt * hit.f) / TAU).floor();
        if dot.skip > tt {
            return;
        }
        if dot.p < 1.0 && chance(self.s.seed, &[c.id as f64, dot.ring, dot.a, turn]) >= dot.p {
            return;
        }
        let midi = if rd.tines != 0.0 {
            let sc = crate::model::scale_ix(&rd.comb_scale).unwrap_or(1);
            scale_note(rd.comb_base, sc, js_min(rd.tines - 1.0, (hit.u * rd.tines).floor()) as i64)
        } else {
            t.ring_note(dot.ring as i64)
        };
        let hold = if dot.len > 1.0 { dot.len * TAU / t.div / c.w.abs() } else { 0.0 };
        let (gain, cid, ring, rid, kind) = (dot.v, c.id, dot.ring, rd.id, rd.voice.kind);
        let last_midi = rd.voice.midi(midi);
        {
            let rd = &mut self.readers[ri];
            rd.hits += 1;
            rd.last_v = hit.vn;
            rd.last_midi = last_midi;
            rd.flash = tt;
        }
        self.cogs[ci].track.as_mut().unwrap().dots[di].flash = tt;
        let mut e = Event::new(tt, Source::Dot { cog: cid, ring: ring as u32, reader: rid }, Part::Reader(rid), Some(kind), midi,
            (rid as i64 * 128 + midi as i64) as u32, hit.vn, hit.x, hit.y);
        e.hold = hold;
        self.fire(e, gain, out);
    }

    fn pluck(&mut self, ai: usize, si: usize, dt: f64, out: &mut impl FnMut(Event)) {
        let arm = &self.arms[ai];
        let c = &self.cogs[arm.cog_ix];
        let s = &self.strings[si];
        let Some(hit) = circle_cross(c, arm.r0 + arm.len, c.th + arm.a, dt, s.x1, s.y1, s.x2, s.y2) else { return };
        let t = self.clock.sim_t + dt * hit.f;
        let midi = s.midi();
        let (aid, sid) = (arm.id, s.id);
        {
            let s = &mut self.strings[si];
            s.hits += 1;
            s.last_v = hit.vn;
            s.last_u = hit.u;
            s.flash = t;
        }
        self.flex(ai, t, hit.vn * 0.5);
        let mut e = Event::new(t, Source::Pluck { arm: aid, string: sid }, Part::String(sid), None, midi,
            (sid as i64 * 128 + js_round(midi) as i64) as u32, hit.vn, hit.x, hit.y);
        e.u = hit.u;
        self.fire(e, 1.0, out);
    }

    /// sim.js fire: the velocity response, the probes on the voice (sim.js probed), and the field sends at the sound.
    pub(crate) fn fire(&mut self, mut e: Event, gain: f64, out: &mut impl FnMut(Event)) {
        self.stats.events += 1;
        let (mut level, mut vs) = match e.part {
            Part::String(id) => match self.strings.iter().find(|s| s.id == id) {
                Some(s) => (s.level, s.vel_sense),
                None => return,
            },
            p => match self.voice(p) {
                Some(v) => (v.level, v.vel_sense),
                None => return,
            },
        };
        if let Some(kind) = e.kind {
            if kind != VoiceKind::Wave && !self.probes.is_empty() {
                self.probed(&mut e, kind, &mut level, &mut vs);
            }
        }
        let vx = clamp(e.vn / self.s.v_ref, 0.0, 1.0);
        let amp = gain * level * (1.0 - vs + vs * vx.powf(self.s.vel_curve));
        if !(amp > 0.0) {
            return;
        }
        e.amp = amp;
        e.vel = vx;
        self.field_sends(&mut e);
        out(e);
    }
}
