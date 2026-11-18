//! The modal synth (engine.worklet.js, and `synth()` in sim.js): a strike excites up to 12 resonant modes, each a
//! decaying complex rotation, through a raised-cosine contact pulse. 64 voices; a key restruck takes its own voice,
//! a full engine steals the quietest voice through a short fade.

use crate::{Aux, Sends, FIELD_SLOTS};
use serde_json::Value;
use std::collections::VecDeque;
use std::f64::consts::{PI, TAU};

pub const MAXM: usize = 12;
const LN1000: f64 = 6.907755278982137;
pub const PARTIALS_MAX: usize = 12;

/// One strike: partial frequencies (Hz), gains and decay times (s, to -60 dB), at a frame.
#[derive(Clone, Copy, Debug)]
pub struct Strike {
    pub at: u64,
    pub key: u32,
    pub amp: f64,
    /// contact time, s: the length of the excitation pulse
    pub contact: f64,
    /// attack, s
    pub attack: f64,
    pub pan: f64,
    pub n: usize,
    pub f: [f64; MAXM],
    pub g: [f64; MAXM],
    pub t: [f64; MAXM],
    pub sends: Sends,
}

pub const BODIES: [&str; 5] = ["harmonic", "bell", "plate", "bar", "bowl"];
const BODY_RAW: [[f64; 5]; 5] = [[1., 2., 3., 4., 5.], [1., 1.183, 1.506, 2.0, 2.514], [1., 1.594, 2.136, 2.653, 3.156], [1., 2.756, 5.404, 8.933, 13.34], [1., 2.32, 4.25, 6.63, 9.38]];
/// A body's measured modes, tuned: a mode off the harmonic series moves to the nearest equal-tempered interval.
fn tune_mode(r: f64) -> f64 { if (r - r.round()).abs() < 0.02 { (r + 0.5).floor() } else { 2f64.powf(((12. * r.log2()) + 0.5).floor() / 12.) } }
/// Partials past a body's measured modes continue on the harmonic series, from the first whole multiple above its last mode.
fn body_ratio(f: &[f64; 5], k: usize) -> f64 { if k < 5 { f[k] } else { (f[4] + 1e-9).ceil() + k as f64 - 5. } }

/// The modal voice's knobs (VOICES.synth), as a save carries them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModalVoice { pub note: f64, pub octave: f64, pub semi: f64, pub body: u8, pub partials: usize, pub bright: f64, pub even: f64, pub bend: f64,
    pub hard: f64, pub attack: f64, pub decay: f64, pub damp: f64 }
impl Default for ModalVoice {
    fn default() -> Self { ModalVoice { note: 60., octave: 0., semi: 0., body: 1, partials: 8, bright: 0.6, even: 1., bend: 1., hard: 0.6, attack: 0., decay: 1.2, damp: 0.8 } }
}
/// [key, label, min, max, step] (VOICE_DEFS.synth; body is a choice of BODIES)
pub const MODAL_DEFS: [(&str, &str, f64, f64, f64); 11] = [("note", "note", 12., 108., 1.), ("octave", "octave", -3., 3., 1.), ("semi", "semitones", -12., 12., 1.),
    ("partials", "partials", 1., 12., 1.), ("bright", "bright", 0., 1., 0.01), ("even", "even partials", 0., 1., 0.01), ("bend", "bend", 0., 1., 0.01),
    ("hard", "hard", 0., 1., 0.01), ("attack", "attack s", 0., 1., 0.005), ("decay", "decay s", 0.02, 8., 0.01), ("damp", "damp", 0., 2., 0.01)];
impl ModalVoice {
    pub fn from_json(j: &Value) -> Self {
        let mut v = Self::default();
        let num = |k: &str| j.get(k).map(crate::patch::js_num).filter(|x| x.is_finite());
        for (k, _, lo, hi, _) in MODAL_DEFS {
            if let Some(x) = num(k) { let x = x.clamp(lo, hi); match k {
                "note" => v.note = x, "octave" => v.octave = x, "semi" => v.semi = x, "partials" => v.partials = x as usize, "bright" => v.bright = x, "even" => v.even = x,
                "bend" => v.bend = x, "hard" => v.hard = x, "attack" => v.attack = x, "decay" => v.decay = x, _ => v.damp = x } }
        }
        if let Some(b) = j.get("body").and_then(|b| b.as_str()).and_then(|b| BODIES.iter().position(|x| *x == b)) { v.body = b as u8; }
        v
    }
    /// synth(): additive, `partials` ringing modes at the body's ratios, bent toward harmonic, each decaying by its
    /// ratio^-damp. `midi` before the voice's own octave and semitones; x is the hit's velocity 0..1.
    #[allow(clippy::too_many_arguments)]
    pub fn strike(&self, at: u64, key: u32, midi: f64, amp: f64, pan: f64, x: f64, hold: f64, dark: f64, sends: Sends) -> Strike {
        let raw = BODY_RAW[self.body as usize % 5];
        let body: [f64; 5] = std::array::from_fn(|i| tune_mode(raw[i]));
        let f0 = 440. * 2f64.powf(((midi + 12. * self.octave + self.semi).clamp(0., 127.) - 69.) / 12.);
        let mut s = Strike { at, key, amp: 0., contact: 0., attack: self.attack, pan, n: 0, f: [0.; MAXM], g: [0.; MAXM], t: [0.; MAXM], sends };
        let mut e = 0.;
        for k in 0..self.partials.min(PARTIALS_MAX) {
            let r = k as f64 + 1. + (body_ratio(&body, k) - k as f64 - 1.) * self.bend;
            let gk = self.bright.powi(k as i32) * (if k % 2 == 1 { self.even } else { 1. }) * (-dark * k as f64).exp();
            s.f[k] = f0 * r; s.g[k] = gk; s.t[k] = (hold * 1.5).max(self.decay * r.powf(-self.damp)); e += gk * gk; s.n = k + 1;
        }
        s.amp = amp * (1.3 / e.sqrt()).min(1.) * 0.3;
        s.contact = ((0.0016 - 0.0014 * self.hard) * x.max(0.02).powf(-0.35)).clamp(0.00012, 0.004);
        s
    }
}

pub struct Modal {
    fs: f64, nv: usize,
    on: Vec<bool>, key: Vec<u32>, nm: Vec<usize>, zr: Vec<f64>, zi: Vec<f64>, kr: Vec<f64>, ki: Vec<f64>, xg: Vec<f64>,
    c_n: Vec<usize>, c_len: Vec<usize>, env: Vec<f64>, env_step: Vec<f64>, fade: Vec<f64>, fade_step: Vec<f64>, pend: Vec<Option<Strike>>, dl: Vec<usize>,
    pl: Vec<f64>, pr: Vec<f64>, pl_t: Vec<f64>, pr_t: Vec<f64>, e: Vec<f64>, snd: Vec<Sends>,
    exc: Vec<f64>, vb: Vec<f64>, vl: Vec<f32>, vr: Vec<f32>,
    queue: VecDeque<Strike>, damps: Vec<(u64, u32)>, wait_from: Option<u64>, max_wait: u64, max_starts: usize,
    pub steals: u32, pub drops: u32, pub strikes: u32, frame: u64, n: usize,
}
impl Modal {
    pub fn new(fs: f64, voices: usize, max_block: usize) -> Self {
        let v = voices; let vm = v * MAXM;
        Modal { fs, nv: v, on: vec![false; v], key: vec![0; v], nm: vec![0; v], zr: vec![0.; vm], zi: vec![0.; vm], kr: vec![0.; vm], ki: vec![0.; vm], xg: vec![0.; vm],
            c_n: vec![0; v], c_len: vec![0; v], env: vec![0.; v], env_step: vec![0.; v], fade: vec![0.; v], fade_step: vec![0.; v], pend: vec![None; v], dl: vec![0; v],
            pl: vec![0.; v], pr: vec![0.; v], pl_t: vec![0.; v], pr_t: vec![0.; v], e: vec![0.; v], snd: vec![Sends::default(); v],
            exc: vec![0.; max_block], vb: vec![0.; max_block], vl: vec![0.; max_block], vr: vec![0.; max_block],
            queue: VecDeque::with_capacity(1024), damps: Vec::with_capacity(256), wait_from: None, max_wait: 8 * 128, max_starts: 4 * v,
            steals: 0, drops: 0, strikes: 0, frame: 0, n: 128 }
    }
    pub fn strike(&mut self, s: Strike) { if self.queue.len() < self.queue.capacity() { self.queue.push_back(s) } else { self.drops += 1 } }
    /// A held note rings for its length, then a damper takes it off over 60 ms.
    pub fn damp(&mut self, at: u64, key: u32) { if self.damps.len() < self.damps.capacity() { self.damps.push((at, key)) } }
    pub fn panic(&mut self) { self.queue.clear(); self.damps.clear(); for v in 0..self.nv { self.on[v] = false; self.pend[v] = None; } }
    pub fn active(&self) -> usize { self.on.iter().filter(|x| **x).count() }
    fn offset(&self, s: &Strike) -> usize { (s.at as i64 - self.frame as i64).clamp(0, self.n as i64 - 1) as usize }
    fn load(&mut self, v: usize, d: &Strike, off: usize) {
        let b = v * MAXM; let fs = self.fs; let mut k = 0;
        for i in 0..d.n.min(MAXM) {
            if d.f[i] >= fs * 0.45 || d.f[i] <= 0. { continue; }
            let r = (-LN1000 / (d.t[i].max(0.01) * fs)).exp(); let w = TAU * d.f[i] / fs;
            self.kr[b + k] = r * w.cos(); self.ki[b + k] = r * w.sin(); self.xg[b + k] = d.g[i] * d.amp; k += 1;
        }
        self.nm[v] = k;
        let len = ((if d.contact != 0. { d.contact } else { 0.002 }) * fs).round().max(8.) as usize; self.c_len[v] = len; self.c_n[v] = len;
        let p = d.pan.clamp(-1., 1.); let a = (p + 1.) * PI / 4.; self.pl_t[v] = a.cos(); self.pr_t[v] = a.sin(); self.dl[v] = off;
        self.snd[v] = d.sends;
    }
    /// false: every voice is already fading out, so the strike has to wait.
    fn start(&mut self, d: &mut Strike) -> bool {
        let off = self.offset(d);
        for v in 0..self.nv { if self.on[v] && self.key[v] == d.key && self.fade[v] == 0. { self.strikes += 1; self.load(v, d, off); return true; } }
        let free = (0..self.nv).find(|&v| !self.on[v]);
        let poly = (8. / (8f64).max(self.active() as f64 + 1.)).sqrt();
        match free {
            None => {
                let mut best = None; let mut be = f64::INFINITY;
                for v in 0..self.nv { if self.fade[v] == 0. && self.e[v] < be { be = self.e[v]; best = Some(v); } }
                let Some(b) = best else { return false };
                self.strikes += 1; d.amp *= poly; self.steals += 1; self.fade[b] = 1.; self.fade_step[b] = 1. / 96.; self.pend[b] = Some(*d); true
            }
            Some(f) => { self.strikes += 1; d.amp *= poly; self.begin(f, d, off); true }
        }
    }
    fn begin(&mut self, v: usize, d: &Strike, off: usize) {
        let b = v * MAXM; for k in 0..MAXM { self.zr[b + k] = 0.; self.zi[b + k] = 0.; }
        self.on[v] = true; self.key[v] = d.key; self.fade[v] = 0.; self.pend[v] = None; self.load(v, d, off); self.pl[v] = self.pl_t[v]; self.pr[v] = self.pr_t[v];
        let a = d.attack; if a > 0.002 { self.env[v] = 0.; self.env_step[v] = 1. / (a * self.fs); } else { self.env[v] = 1.; self.env_step[v] = 0.; }
    }
    fn render_voice(&mut self, v: usize, n: usize, out_l: &mut [f32], out_r: &mut [f32]) {
        let b = v * MAXM; let nm = self.nm[v];
        let mut c_n = self.c_n[v]; let c_len = self.c_len[v]; let norm = 2. / c_len as f64;
        let exciting = c_n > 0;
        let mut delay = self.dl[v];
        if exciting { for s in 0..n {
            if delay > 0 { self.exc[s] = 0.; delay -= 1; }
            else if c_n > 0 { let ph = (c_len - c_n) as f64 / c_len as f64 + 0.5 / c_len as f64; self.exc[s] = (1. - (TAU * ph).cos()) * 0.5 * norm; c_n -= 1; }
            else { self.exc[s] = 0.; }
        } }
        self.vb[..n].fill(0.); self.c_n[v] = c_n; self.dl[v] = delay;
        let mut e = 0.;
        // partials ring independently; one that has decayed below -110 dB stops costing
        for j in 0..nm {
            let p = b + j;
            if !(exciting || self.zr[p] * self.zr[p] + self.zi[p] * self.zi[p] > 1e-11) { continue; }
            let (mut r, mut i) = (self.zr[p], self.zi[p]); let (a, c, g) = (self.kr[p], self.ki[p], self.xg[p]);
            if exciting { for s in 0..n { let nr = a * r - c * i + self.exc[s] * g; i = a * i + c * r; r = nr; self.vb[s] += r; } }
            else { for s in 0..n { let nr = a * r - c * i; i = a * i + c * r; r = nr; self.vb[s] += r; } }
            self.zr[p] = r; self.zi[p] = i; e += r * r + i * i;
        }
        let (mut env, mut es, mut fl, mut pl, mut pr) = (self.env[v], self.env_step[v], self.fade[v], self.pl[v], self.pr[v]);
        let fstep = self.fade_step[v]; let dl = (self.pl_t[v] - pl) / n as f64; let dr = (self.pr_t[v] - pr) / n as f64;
        for s in 0..n {
            if es > 0. { env += es; if env >= 1. { env = 1.; es = 0.; } }
            let mut g = env;
            if fl > 0. { fl = (fl - fstep).max(1e-9); g *= if fl <= 1e-9 { 0. } else { fl }; }
            pl += dl; pr += dr; let x = self.vb[s] * g; out_l[s] += (x * pl) as f32; out_r[s] += (x * pr) as f32;
        }
        self.pl[v] = pl; self.pr[v] = pr; self.env[v] = env; self.env_step[v] = es; self.e[v] = e;
        if fl > 0. {
            self.fade[v] = fl;
            if fl <= 1e-9 {
                self.on[v] = false; self.fade[v] = 0.;
                if let Some(d) = self.pend[v].take() { let off = self.offset(&d); self.begin(v, &d, off); self.render_voice(v, n, out_l, out_r); }
            }
        } else if !exciting && c_n == 0 && e < 1e-11 { self.on[v] = false; }
    }
    /// One block: strikes due, damps, then every voice into dry (added), and into the field slots by its sends.
    pub(crate) fn process(&mut self, frame: u64, n: usize, dry_l: &mut [f32], dry_r: &mut [f32], aux: &mut Aux) -> usize {
        self.frame = frame; self.n = n;
        let end = frame + n as u64;
        // due strikes in the order they came (one sent ahead for a later block waits in place)
        let mut starts = 0; let mut i = 0;
        while starts < self.max_starts && i < self.queue.len() {
            let mut d = self.queue[i];
            if d.at >= end { i += 1; continue; }
            starts += 1;
            if !self.start(&mut d) {
                let wf = *self.wait_from.get_or_insert(frame);
                if frame - wf < self.max_wait { break; }
                self.strikes += 1; self.drops += 1;
            }
            self.wait_from = None; self.queue.remove(i);
        }
        if !self.damps.is_empty() {
            let mut i = 0;
            while i < self.damps.len() {
                let (at, key) = self.damps[i];
                if at >= end { i += 1; continue; }
                for v in 0..self.nv { if self.on[v] && self.key[v] == key && self.fade[v] == 0. { self.fade[v] = 1.; self.fade_step[v] = 1. / (self.fs * 0.06); self.pend[v] = None; } }
                self.damps.swap_remove(i);
            }
        }
        let mut voices = 0;
        for v in 0..self.nv {
            if !self.on[v] { continue; }
            voices += 1;
            let sd = self.snd[v];
            // a voice in a field renders on its own, then goes to the dry mix at its dry level and to each field it is in
            if !sd.any() && sd.dry == 1. { self.render_voice(v, n, dry_l, dry_r); continue; }
            let (mut vl, mut vr) = (std::mem::take(&mut self.vl), std::mem::take(&mut self.vr));
            vl[..n].fill(0.); vr[..n].fill(0.);
            self.render_voice(v, n, &mut vl, &mut vr);
            for s in 0..n { dry_l[s] += vl[s] * sd.dry; dry_r[s] += vr[s] * sd.dry; }
            for k in 0..FIELD_SLOTS { let a = sd.amt[k]; if a == 0. { continue; } let (al, ar) = aux.take(k); for s in 0..n { al[s] += vl[s] * a; ar[s] += vr[s] * a; } }
            self.vl = vl; self.vr = vr;
        }
        voices
    }
}
