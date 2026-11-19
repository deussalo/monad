//! motion.js: the cam, and the motion programs cams and probes send a synth's macros. A program is a table read at a
//! phase moving linearly with time (wave.worklet.js motionAt); `Motion::value_at` evaluates it.

use crate::model::*;
use crate::world::World;

pub const CAM_SHAPE_NAMES: [&str; 8] = ["sine", "ramp up", "ramp down", "triangle", "square", "steps 4", "steps 8", "random"];
const CAM_STEPPED: [&str; 4] = ["square", "steps 4", "steps 8", "random"];
pub fn shape_known(name: &str) -> bool {
    CAM_SHAPE_NAMES.contains(&name)
}
fn shape_at(name: &str, u: f64) -> f64 {
    match name {
        "sine" => 0.5 - 0.5 * (TAU * u).cos(),
        "ramp up" => u,
        "ramp down" => 1.0 - u,
        "triangle" => 1.0 - (2.0 * u - 1.0).abs(),
        "square" => if u < 0.5 { 1.0 } else { 0.0 },
        "steps 4" => (u * 4.0).floor() / 3.0,
        "steps 8" => (u * 8.0).floor() / 7.0,
        "random" => {
            let k = (u * 8.0).floor();
            let x = (k * 12.9898 + 78.233).sin() * 43758.5453;
            x - x.floor()
        }
        _ => 0.5 - 0.5 * (TAU * u).cos(),
    }
}
/// A named shape's profile, CAM_N points rounded to 3 places.
pub fn cam_shape(name: &str) -> Vec<f64> {
    (0..CAM_N).map(|i| to_fixed(shape_at(name, i as f64 / CAM_N as f64), 3)).collect()
}
pub fn new_cam(id: u32) -> Cam {
    Cam {
        id, cog: 0, ang: 0.0, phase: 0.0, shape: "sine".into(), prof: cam_shape("sine"), to: None, macro_: 1, more: Vec::new(), lo: 0.0, hi: 1.0, ver: 0,
        cog_ix: 0, to_ix: None,
    }
}
impl Cam {
    pub fn stepped(&self) -> bool {
        self.shape != "drawn" && CAM_STEPPED.contains(&self.shape.as_str())
    }
    /// The fraction of a turn under the follower when the cog stands at th.
    pub fn u(&self, th: f64) -> f64 {
        let u = (self.ang - th) / TAU - self.phase;
        u - u.floor()
    }
    /// The profile at fraction u of a turn.
    pub fn at(&self, u: f64) -> f64 {
        let t = &self.prof;
        let x = u * CAM_N as f64;
        let i = (x.floor() as i64).rem_euclid(CAM_N as i64) as usize;
        if self.stepped() { t[i] } else { t[i] + (t[(i + 1) % CAM_N] - t[i]) * (x - x.floor()) }
    }
    /// Set a named shape (or "drawn", keeping the profile).
    pub fn set_shape(&mut self, name: &str) {
        if name != "drawn" {
            self.prof = cam_shape(name);
        }
        self.shape = name.into();
        self.ver += 1;
    }
    /// Draw: set profile points i0..=i1 to y; the shape becomes "drawn".
    pub fn draw(&mut self, i0: usize, i1: usize, y: f64) {
        let y = to_fixed(clamp(y, 0.0, 1.0), 3);
        for k in i0.min(i1)..=i0.max(i1).min(CAM_N - 1) {
            self.prof[k] = y;
        }
        self.shape = "drawn".into();
        self.ver += 1;
    }
}

/// Which part turns a macro.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MotionFrom {
    Cam(u32),
    Probe(u32),
}

/// A motion program for one macro of a synth part: value(t) = lo + (hi − lo)·table(ph0 + rate·(t − t0)), linear or
/// stepped, wrapping. `t0` is sim time: the app maps it to a frame as it maps events. `glide` is the smoothing the
/// worklet applies, in seconds (0: its default 3 ms). An empty table means the constant `value` (a ball's probe,
/// streamed: send it again when it changes).
#[derive(Clone, Copy, Debug)]
pub struct Motion<'a> {
    pub target: Part,
    /// 0-based macro slot.
    pub slot: u8,
    pub from: MotionFrom,
    pub table: &'a [f64],
    pub value: f64,
    pub step: bool,
    pub t0: f64,
    pub ph0: f64,
    pub rate: f64,
    pub lo: f64,
    pub hi: f64,
    pub glide: f64,
    /// Changes when the table does (a cam's `ver`, a probe's geometry).
    pub sig: u64,
}
impl Motion<'_> {
    /// Its value at sim time t (wave.worklet.js motionAt).
    pub fn value_at(&self, t: f64) -> f64 {
        let n = self.table.len();
        let y = if n == 0 {
            self.value
        } else {
            let ph = self.ph0 + self.rate * (t - self.t0);
            let u = (ph - ph.floor()) * n as f64;
            let i = (u.floor() as usize) % n;
            if self.step { self.table[i] } else { self.table[i] + (self.table[(i + 1) % n] - self.table[i]) * (u - u.floor()) }
        };
        self.lo + (self.hi - self.lo) * y
    }
    /// Whether this program continues `old` without a jump (postMotions' test for not posting again).
    pub fn same_as(&self, old: &Motion) -> bool {
        if old.from != self.from || old.sig != self.sig || old.rate != self.rate || old.lo != self.lo || old.hi != self.hi || old.step != self.step {
            return false;
        }
        if self.table.is_empty() {
            return old.value == self.value;
        }
        let at = old.ph0 + old.rate * (self.t0 - old.t0);
        let d = at - self.ph0;
        (d - js_round(d)).abs() < 1e-4
    }
}

impl World {
    pub fn cam_rb(&self, m: &Cam) -> f64 {
        js_max(8.0, self.cogs[m.cog_ix].radius() * 0.78)
    }
    pub fn cam_roll(&self, m: &Cam, y: f64) -> f64 {
        self.cam_rb(m) * (0.45 + 0.55 * y)
    }
    /// The follower's lift as drawn, 0..1.
    pub fn cam_lift(&self, m: &Cam) -> f64 {
        m.at(m.u(self.render_th(m.cog_ix)))
    }
    /// The follower's tip as drawn.
    pub fn cam_tip(&self, m: &Cam) -> (f64, f64) {
        let c = &self.cogs[m.cog_ix];
        let r = self.cam_roll(m, self.cam_lift(m)) + 14.0;
        (c.x + m.ang.cos() * r, c.y + m.ang.sin() * r)
    }

    /// Every macro a cam or probe turns on a synth (wave) part, as programs (motion.js postMotions: the first for a
    /// part's slot wins, cams before probes).
    pub fn motions<'a>(&'a self, out: &mut impl FnMut(Motion<'a>)) {
        let mut seen: Vec<(Part, u8)> = Vec::new();
        let mut take = |m: &Motion| {
            if seen.iter().any(|s| s.0 == m.target && s.1 == m.slot) {
                return false;
            }
            seen.push((m.target, m.slot));
            true
        };
        for m in &self.cams {
            let first = m.to_ix.map(|ri| (Part::Reader(self.readers[ri].id), m.macro_));
            if first.is_none() && m.more.is_empty() {
                continue;
            }
            let c = &self.cogs[m.cog_ix];
            let (t0, ph0, rate) = if self.clock.paused || c.w == 0.0 {
                (0.0, m.u(self.render_th(m.cog_ix)), 0.0)
            } else {
                (self.clock.sim_t, m.u(c.th), -c.w / TAU)
            };
            for (target, mac) in first.into_iter().chain(m.more.iter().map(|l| (l.to, l.macro_))) {
                if self.voice(target).map(|v| v.kind) != Some(VoiceKind::Wave) {
                    continue;
                }
                let mo = Motion {
                    target, slot: (mac - 1).clamp(0, 7) as u8, from: MotionFrom::Cam(m.id), table: &m.prof, value: 0.0,
                    step: m.stepped(), t0, ph0, rate, lo: m.lo, hi: m.hi, glide: 0.0, sig: m.ver as u64,
                };
                if take(&mo) {
                    out(mo);
                }
            }
        }
        for pr in &self.probes {
            for (target, mac) in pr.to.map(|t| (t, pr.macro_)).into_iter().chain(pr.more.iter().map(|l| (l.to, l.macro_))) {
                if let Some(mo) = self.probe_program_to(pr, target, mac) {
                    if take(&mo) {
                        out(mo);
                    }
                }
            }
        }
    }
    /// The value a part's macro slot (0-based) is turned to at sim time t, if a cam or probe turns it.
    pub fn macro_value(&self, target: Part, slot: u8, t: f64) -> Option<f64> {
        let mut v = None;
        self.motions(&mut |m| {
            if v.is_none() && m.target == target && m.slot == slot {
                v = Some(m.value_at(t));
            }
        });
        v
    }
}
