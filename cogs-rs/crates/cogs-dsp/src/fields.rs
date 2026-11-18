//! The fields (field.worklet.js): an effect send for every FX field on the stage. Each slot runs its field's effect
//! (delay, reverb or distortion) on what the voices sent it; the slots are summed to one stereo return. Where in the
//! field the latest sounds were struck (q: 0 at the rim, 1 at the centre) bends the effect's parameters, by as much as
//! the field is diffuse.

use crate::verb::{Verb, VerbParams, V_COLOR, V_DECAY, V_MODE, V_PREDELAY, V_SIZE, V_WET};
use crate::{Aux, FIELD_SLOTS};
use serde_json::Value;
use std::f64::consts::TAU;

pub const FIELD_MODES: [&str; 3] = ["delay", "reverb", "dist"];
pub const FIELD_SYNCS: [(&str, f64); 7] = [("1/16", 0.25), ("1/8", 0.5), ("1/8.", 0.75), ("1/4", 1.), ("1/4.", 1.5), ("1/2", 2.), ("1/2.", 3.)];
pub const FIELD_DIST_KINDS: [&str; 4] = ["tube", "fold", "fuzz", "crush"];

/// A field's effect, as fieldParams() posts it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FieldParams {
    /// 0 delay, 1 reverb, 2 dist
    pub mode: u8,
    pub diffuse: f64,
    pub level: f64,
    /// tempo-locked delay time in beats, or None for the free time
    pub sync: Option<f64>,
    pub time: f64,
    pub fb: f64,
    pub tone: f64,
    pub ping: bool,
    pub size: f64,
    pub decay: f64,
    pub predelay: f64,
    /// index into FIELD_DIST_KINDS
    pub kind: u8,
    pub drive: f64,
    pub verb_mode: u8,
    pub verb_color: u8,
}
impl Default for FieldParams {
    /// FIELD_DEFAULT
    fn default() -> Self {
        FieldParams { mode: 0, diffuse: 0.5, level: 0.8, sync: Some(0.75), time: 0.375, fb: 0.45, tone: 0.5, ping: true, size: 0.6, decay: 2.5, predelay: 10.,
            kind: 0, drive: 0.5, verb_mode: 0, verb_color: 2 }
    }
}
impl FieldParams {
    /// From a saved field (its keys as the save has them: mode, sync 'free' or a division, verbMode, ...).
    pub fn from_json(j: &Value) -> Self {
        let mut p = Self::default();
        let num = |k: &str| j.get(k).map(crate::patch::js_num).filter(|x| x.is_finite());
        if let Some(m) = j.get("mode").and_then(|v| v.as_str()).and_then(|m| FIELD_MODES.iter().position(|x| *x == m)) { p.mode = m as u8; }
        if let Some(s) = j.get("sync") { p.sync = s.as_str().and_then(|s| FIELD_SYNCS.iter().find(|x| x.0 == s)).map(|x| x.1); }
        if let Some(x) = num("diffuse") { p.diffuse = x; } if let Some(x) = num("level") { p.level = x; } if let Some(x) = num("time") { p.time = x; }
        if let Some(x) = num("fb") { p.fb = x; } if let Some(x) = num("tone") { p.tone = x; } if let Some(v) = j.get("ping") { p.ping = crate::patch::js_truthy(v); }
        if let Some(x) = num("size") { p.size = x; } if let Some(x) = num("decay") { p.decay = x; } if let Some(x) = num("predelay") { p.predelay = x; }
        if let Some(x) = num("drive") { p.drive = x; }
        if let Some(k) = j.get("kind").and_then(|v| v.as_str()).and_then(|m| FIELD_DIST_KINDS.iter().position(|x| *x == m)) { p.kind = k as u8; }
        if let Some(x) = num("verbMode") { p.verb_mode = x.clamp(0., 19.) as u8; } if let Some(x) = num("verbColor") { p.verb_color = x.clamp(0., 2.) as u8; }
        p
    }
}

struct Slot {
    p: Option<FieldParams>, mode: i32, q: f64, tq: f64, pub k: f64, quiet: f64,
    dl: Vec<f32>, dr: Vec<f32>, dw: usize, d_t: f64, dlp: [f64; 2], dhp: [f64; 2],
    verb: Option<Box<Verb>>, verb_fresh: bool, dz: [f64; 4], crush: [f64; 2], crush_n: u64, tl: Vec<f32>, tr: Vec<f32>,
}

pub struct Fields { fs: f64, slots: Vec<Slot>, verbs: Vec<Box<Verb>>, hits: Vec<(u64, usize, f64, f64)>, zero: Vec<f32>, pub bpm: f64 }
impl Fields {
    pub fn new(fs: f64, verbs: usize, max_block: usize) -> Self {
        let n = (fs * 3.2).ceil() as usize;
        Fields {
            fs,
            slots: (0..FIELD_SLOTS).map(|_| Slot { p: None, mode: -1, q: 0.5, tq: 0.5, k: 0., quiet: 1e9, dl: vec![0.; n], dr: vec![0.; n], dw: 0, d_t: -1.,
                dlp: [0.; 2], dhp: [0.; 2], verb: None, verb_fresh: false, dz: [0.; 4], crush: [0.; 2], crush_n: 0, tl: vec![0.; max_block], tr: vec![0.; max_block] }).collect(),
            verbs: (0..verbs).map(|_| Box::new(Verb::new(fs, &VerbParams::default()))).collect(),
            hits: Vec::with_capacity(512), zero: vec![0.; max_block], bpm: 120.,
        }
    }
    fn verb_params(p: &FieldParams, k: f64) -> [(usize, f64); 6] {
        [(V_MODE, p.verb_mode as f64), (V_COLOR, p.verb_color as f64), (V_WET, 1.), (V_SIZE, (p.size + 0.35 * k).clamp(0., 1.)),
            (V_DECAY, (p.decay * 2f64.powf(1.3 * k)).max(0.2)), (V_PREDELAY, (p.predelay * (1. - 0.6 * k)).max(0.))]
    }
    /// A field's effect for a slot, or None: the slot is free.
    pub fn set(&mut self, slot: usize, p: Option<FieldParams>) {
        let Some(s) = self.slots.get_mut(slot) else { return };
        match p {
            None => { s.p = None; if let Some(v) = s.verb.take() { self.verbs.push(v); } }
            Some(p) => {
                let m = p.mode.min(2) as i32;
                if m != s.mode {
                    s.mode = m; s.dl.fill(0.); s.dr.fill(0.); s.d_t = -1.; s.dz = [0.; 4]; s.crush = [0.; 2]; s.crush_n = 0;
                    if s.verb.is_some() { s.verb_fresh = true; }
                }
                s.p = Some(p);
            }
        }
    }
    /// A sound struck in a field at frame `at`: q (0 rim .. 1 centre) and how much was sent.
    pub fn hit(&mut self, at: u64, slot: usize, q: f64, amt: f64) { if self.hits.len() < self.hits.capacity() { self.hits.push((at, slot, q, amt)); } }
    /// Where each slot's parameters are now (0 at the rim, 1 at the centre), for the stage's ring.
    pub fn focus(&self, slot: usize) -> Option<f64> { self.slots.get(slot).filter(|s| s.p.is_some()).map(|s| s.q) }

    pub(crate) fn process(&mut self, frame: u64, n: usize, aux: &Aux, out_l: &mut [f32], out_r: &mut [f32]) {
        let end = frame + n as u64;
        let mut i = 0;
        while i < self.hits.len() {
            let (at, k, q, a) = self.hits[i];
            if at < end { if let Some(s) = self.slots.get_mut(k) { s.tq += a.clamp(0.35, 1.) * (q - s.tq); } self.hits.swap_remove(i); } else { i += 1; }
        }
        let fs = self.fs; let bpm = self.bpm;
        for k in 0..FIELD_SLOTS {
            let s = &mut self.slots[k];
            let Some(p) = s.p else { continue };
            let (inl, inr): (&[f32], &[f32]) = if aux.used[k] { (&aux.l[k][..n], &aux.r[k][..n]) } else { (&self.zero[..n], &self.zero[..n]) };
            let mut in_e = 0.; let mut j = 0; while j < n { in_e += (inl[j].abs() + inr[j].abs()) as f64; j += 4; }
            if in_e > 1e-7 { s.quiet = 0.; } else { s.quiet += n as f64; }
            let sync_t = p.sync.map_or(0., |b| 60. / bpm * b);
            let tail = match s.mode {
                0 => (if sync_t > 0. { sync_t } else if p.time != 0. { p.time } else { 0.4 }) * (4. + 30. * p.fb) * fs,
                1 => (if p.decay != 0. { p.decay } else { 2.5 }) * 2.5 * fs + fs,
                _ => 0.05 * fs,
            };
            if s.quiet > tail { continue; }
            let gq = 1. - (-(n as f64) / (fs * 0.06)).exp(); s.q += (s.tq - s.q) * gq;
            let d = p.diffuse.clamp(0., 1.); let kk = d * (2. * s.q - 1.); s.k = kk; let lv = p.level;
            match s.mode {
                0 => delay(s, &p, kk, inl, inr, out_l, out_r, n, bpm, lv, fs),
                1 => {
                    if s.verb.is_none() { s.verb = self.verbs.pop(); s.verb_fresh = true; }
                    let Some(v) = s.verb.as_mut() else { continue };
                    let vp = Self::verb_params(&p, kk);
                    if s.verb_fresh { s.verb_fresh = false; let mut q = VerbParams::default(); for (i, x) in vp { q.0[i] = x; } v.clear(&q); }
                    else { for (i, x) in vp { v.set_one(i, x); } }
                    s.tl[..n].fill(0.); s.tr[..n].fill(0.);
                    v.process(inl, inr, &mut s.tl[..n], &mut s.tr[..n], n);
                    for j in 0..n { out_l[j] += s.tl[j] * lv as f32; out_r[j] += s.tr[j] * lv as f32; }
                }
                _ => dist(s, &p, kk, inl, inr, out_l, out_r, n, lv, fs),
            }
        }
    }
}
/// A tape-ish delay: its length glides, so where the strikes fall bends the echoes' pitch as they move.
#[allow(clippy::too_many_arguments)]
fn delay(s: &mut Slot, p: &FieldParams, k: f64, inl: &[f32], inr: &[f32], l: &mut [f32], r: &mut [f32], n: usize, bpm: f64, lv: f64, fs: f64) {
    let nn = s.dl.len();
    let t0 = match p.sync { Some(b) if b > 0. => 60. / bpm * b, _ => (if p.time != 0. { p.time } else { 0.375 }).max(0.02) };
    let t = (t0 * 2f64.powf(0.5 * k) * fs).max(8.).min((nn - 4) as f64);
    if s.d_t < 0. { s.d_t = t; }
    let fb = (p.fb + 0.25 * k).clamp(0., 0.95); let tone = (p.tone - 0.25 * k).clamp(0., 1.);
    let lc = 1. - (-TAU * (0.45 * fs).min(500. * 2f64.powf(tone * 5.)) / fs).exp(); let hc = 1. - (-TAU * 90. / fs).exp(); let g_t = 1. - (-1. / (fs * 0.12)).exp();
    let (mut w, mut d_t) = (s.dw, s.d_t); let [mut l1, mut l2] = s.dlp; let [mut h1, mut h2] = s.dhp;
    for j in 0..n {
        d_t += (t - d_t) * g_t;
        let pos = w as f64 - d_t; let i = pos.floor() as i64; let f = pos - i as f64; let a = i.rem_euclid(nn as i64) as usize; let b = if a + 1 == nn { 0 } else { a + 1 };
        let yl = s.dl[a] as f64 + (s.dl[b] - s.dl[a]) as f64 * f; let yr = s.dr[a] as f64 + (s.dr[b] - s.dr[a]) as f64 * f;
        l1 += (yl - l1) * lc; l2 += (yr - l2) * lc; h1 += (l1 - h1) * hc; h2 += (l2 - h2) * hc;
        let el = ((l1 - h1) * fb).tanh(); let er = ((l2 - h2) * fb).tanh();
        if p.ping { let m = (inl[j] + inr[j]) as f64 * 0.5; s.dl[w] = (m + er) as f32; s.dr[w] = el as f32; }
        else { s.dl[w] = (inl[j] as f64 + el) as f32; s.dr[w] = (inr[j] as f64 + er) as f32; }
        l[j] += (yl * lv) as f32; r[j] += (yr * lv) as f32; w = if w + 1 == nn { 0 } else { w + 1 };
    }
    s.dw = w; s.d_t = d_t; s.dlp = [l1, l2]; s.dhp = [h1, h2];
}
/// Drive, shaped by its kind, then a tone low-pass and a DC block; nearer the centre drives harder and darker.
#[allow(clippy::too_many_arguments)]
fn dist(s: &mut Slot, p: &FieldParams, k: f64, inl: &[f32], inr: &[f32], l: &mut [f32], r: &mut [f32], n: usize, lv: f64, fs: f64) {
    let kind = p.kind; let dr = (p.drive + 0.35 * k).clamp(0., 1.); let g = 1. + dr * dr * 40.; let inv = 1. / g.powf(if kind == 2 { 0.75 } else { 0.5 });
    let tone = (p.tone - 0.2 * k).clamp(0., 1.);
    let lc = 1. - (-TAU * (0.45 * fs).min(700. * 2f64.powf(tone * 4.8)) / fs).exp(); let hc = 1. - (-TAU * 25. / fs).exp();
    let hold = 1 + (dr * 20.).floor() as u64; let bits = 2f64.powf(15. - dr * 12.);
    for j in 0..n {
        for ch in 0..2 {
            let x = (if ch == 1 { inr[j] } else { inl[j] }) as f64 * g;
            let mut y = match kind {
                0 => (x + 0.18).tanh() - 0.178,
                1 => (x * 0.6).sin(),
                2 => (x * 1.5).clamp(-1., 1.),
                _ => { if s.crush_n % hold == 0 { s.crush[ch] = crate::params::js_round((x * 0.3).tanh() * bits) / bits; } s.crush[ch] }
            };
            y *= if kind == 3 { 0.6 } else { inv };
            s.dz[ch] += (y - s.dz[ch]) * lc; s.dz[2 + ch] += (s.dz[ch] - s.dz[2 + ch]) * hc; let o = s.dz[ch] - s.dz[2 + ch];
            if ch == 1 { r[j] += (o * lv) as f32 } else { l[j] += (o * lv) as f32 }
        }
        s.crush_n += 1;
    }
}
