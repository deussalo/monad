//! The wave synth (wave.worklet.js): three wavetable oscillators with their own amp and filter envelopes and filters,
//! sub and noise on ENV1, four envelopes, four LFOs, eight macros, a mod matrix, poly/mono/legato voicing with glide,
//! and a per-patch FX rack. Modulation runs per 32-sample slice: sources are sampled at the slice start and every
//! routed parameter is recomputed from its knob position plus the routes, as Serum does.

use crate::params::*;
use crate::patch::WavePatch;
use crate::table::{builtin, build_table, wave_spectra, WaveTable, WMIPS, WN};
use crate::verb::{Verb, VerbParams, V_ATTACK, V_COLOR, V_DECAY, V_EARLY, V_HIGHCUT, V_LATE, V_LOWCUT, V_MODDEPTH, V_MODE, V_MODRATE, V_PREDELAY, V_SIZE, V_WET};
use crate::{Rng, Sends, BLOCK, FIELD_SLOTS};
#[allow(unused_imports)]
use std::f64::consts::FRAC_PI_2;
use std::f64::consts::{PI, SQRT_2, TAU};
use std::sync::Arc;

pub const WSLICE: usize = 32;
pub const WVOICES: usize = 24;
pub const WPOLY: usize = 16;
pub const WSTREAMS: f64 = 64.;
/// A cam's table has 64 points, a probe's 128.
pub const MOTION_POINTS: usize = 128;

#[derive(Clone, Copy, Debug)]
struct Route { s: usize, d: usize, amt: f64, bi: bool, ax: Option<usize>, ai: bool, cv: f64 }

/// A patch compiled for the voices: knob positions normalised, routes packed, a table for each oscillator.
/// Build it off the audio thread (a harmonics table is an FFT per mip) and send it in `Cmd::Patch`.
pub struct CompiledPatch {
    base: [f64; NP],
    val: [f64; NP],
    routes: Vec<Route>,
    modded: Vec<u16>,
    tabs: [Arc<WaveTable>; WAVE_OSCS],
    harm: [[f64; WAVE_HARMS]; WAVE_OSCS],
    on: [bool; WAVE_OSCS],
    warp_mode: [u8; WAVE_OSCS],
    warp_mode2: [u8; WAVE_OSCS],
    ftype: [u8; WAVE_OSCS],
    lfo_shape: [u8; WAVE_LFOS],
    lfo_retrig: [bool; WAVE_LFOS],
    lfo_sync: [f64; WAVE_LFOS],
    mode: u8,
    always: bool,
    glide_scale: bool,
    fx_on: [bool; 9],
    fx_order: [u8; 9],
    dist_mode: u8,
    verb_mode: u8,
    verb_color: u8,
    delay_sync: f64,
    ping: bool,
}
impl std::fmt::Debug for CompiledPatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.debug_struct("CompiledPatch").field("routes", &self.routes.len()).finish() }
}
impl CompiledPatch {
    pub fn new(p: &WavePatch) -> Self { Self::compile(p, None) }
    /// As `new`, keeping `old`'s harmonics tables where the harmonics did not change (an edit of another knob is cheap).
    pub fn compile(p: &WavePatch, old: Option<&CompiledPatch>) -> Self {
        let params = wave_params();
        let mut base = [0.; NP];
        let mut val = [0.; NP];
        for i in 0..NP {
            let d = &params[i];
            let x = if p.values[i].is_finite() { p.values[i] } else { d.default };
            base[i] = wave_norm(d, x).clamp(0., 1.);
            if base[i].is_nan() { base[i] = 0.; }
            val[i] = wave_denorm(d, base[i]);
        }
        let mut routes = Vec::new();
        let mut modded: Vec<u16> = Vec::new();
        for m in &p.mods {
            if m.amt == 0. || m.off || m.dst as usize >= NP { continue; }
            routes.push(Route { s: m.src as usize, d: m.dst as usize, amt: m.amt, bi: m.bi, ax: m.aux.map(|a| a as usize), ai: m.aux_inv, cv: m.curve });
            if !modded.contains(&m.dst) { modded.push(m.dst); }
        }
        let tabs = std::array::from_fn(|i| {
            let o = &p.osc[i];
            if o.table == 1 {
                match old { Some(c) if c.harm_is_built(i) && c.harm[i] == o.harm => c.tabs[i].clone(),
                    _ => Arc::new(build_table(&wave_spectra("harmonics", &o.harm))) }
            } else { builtin()[o.table as usize].clone() }
        });
        CompiledPatch {
            base, val, routes, modded, tabs,
            harm: std::array::from_fn(|i| if p.osc[i].table == 1 { p.osc[i].harm } else { [f64::NAN; WAVE_HARMS] }),
            on: std::array::from_fn(|i| p.osc[i].on), warp_mode: std::array::from_fn(|i| p.osc[i].warp_mode),
            warp_mode2: std::array::from_fn(|i| p.osc[i].warp2_mode), ftype: std::array::from_fn(|i| p.osc[i].filter),
            lfo_shape: std::array::from_fn(|i| p.lfo[i].shape), lfo_retrig: std::array::from_fn(|i| p.lfo[i].retrig),
            lfo_sync: std::array::from_fn(|i| LFO_SYNCS[p.lfo[i].sync as usize].1),
            mode: p.voice_mode, always: p.glide_always, glide_scale: p.glide_scale, fx_on: p.fx_on, fx_order: p.fx_order,
            dist_mode: p.dist_mode, verb_mode: p.verb_mode, verb_color: p.verb_color, delay_sync: DELAY_SYNCS[p.delay_sync as usize].1, ping: p.delay_ping,
        }
    }
    fn harm_is_built(&self, i: usize) -> bool { !self.harm[i][0].is_nan() }
    /// The modulated knobs (routes' destinations), for the editor's moving dots.
    pub fn modded(&self) -> &[u16] { &self.modded }
    /// how long the rack rings on after its last note: the delay's echoes, the reverb's tail, or a short while
    fn fx_tail(&self, fxv: &[f64; NP]) -> f64 {
        let d = if self.fx_on[FX_DELAY] { 2. + 12. * self.val[DELAY_FB] } else { 0. };
        let r = if self.fx_on[FX_REVERB] { 1. + self.val[REVERB_PREDELAY] + 2.4 * self.val[REVERB_DECAY].max(fxv[REVERB_DECAY]) } else { 0. };
        d.max(r).max(if self.fx_on.iter().any(|x| *x) { 0.3 } else { 0. })
    }
}

/// A motion (a cam or a probe on a macro): a table read at a phase that moves linearly with the frame, mapped into
/// lo..hi, linear or stepped; posted once per change, evaluated every slice on the notes' own clock.
#[derive(Clone, Copy, Debug)]
pub struct MotionProg {
    pub table: [f32; MOTION_POINTS],
    /// points used of `table` (64 for a cam, 128 for a probe, 1 for a ball's value)
    pub len: usize,
    pub step: bool,
    /// the frame where the phase is ph0
    pub f0: f64,
    pub ph0: f64,
    /// turns per second
    pub rate: f64,
    pub lo: f64,
    pub hi: f64,
    /// one-pole glide time in seconds; 0 takes the default (about 3 ms)
    pub glide: f64,
}
#[derive(Clone, Copy)]
struct Motion { p: MotionProg, g: f64, y: f64 }
fn motion_at(p: &MotionProg, frame: f64, fs: f64) -> f64 {
    let n = p.len.clamp(1, MOTION_POINTS);
    let ph = p.ph0 + p.rate * (frame - p.f0) / fs;
    let u = (ph - ph.floor()) * n as f64;
    let i = (u.floor() as usize) % n;
    let y = if p.step { p.table[i] as f64 } else { p.table[i] as f64 + (p.table[(i + 1) % n] as f64 - p.table[i] as f64) * (u - u.floor()) };
    p.lo + (p.hi - p.lo) * y
}

#[derive(Clone, Copy, Default)]
struct Env { st: u8, lv: f64 }
#[derive(Clone, Copy, Default)]
struct OscState { ph: [f64; 8], env: Env, fenv: Env, ec: [f64; 3], fc: [f64; 3], z: [f64; 8], mph: f64 }
#[derive(Clone, Copy, Default)]
struct LfoState { ph: f64, held: f64, prev: f64 }

struct Voice {
    on: bool, slot: usize, ver: u64, vals: [f64; NP], src: [f64; NS], osc: [OscState; WAVE_OSCS], envs: [Env; WAVE_ENVS], ecs: [[f64; 3]; WAVE_ENVS],
    lfo: [LfoState; WAVE_LFOS], sub_ph: f64, nz: f64, dl: f64, dr: f64, kill: f64, age: u64, released: bool, key: u32,
    midi: f64, from: f64, glide_t: f64, t0: f64, off_at: f64, amp: f64, pan: f64, dark: f64, vel: f64, rand: f64, sends: Sends,
}
impl Voice {
    fn new() -> Self {
        Voice { on: false, slot: 0, ver: 0, vals: [0.; NP], src: [0.; NS], osc: [OscState::default(); 3], envs: [Env::default(); 4], ecs: [[0.; 3]; 4],
            lfo: [LfoState { ph: 0., held: 0.5, prev: 0.5 }; 4], sub_ph: 0., nz: 0., dl: 0., dr: 0., kill: 0., age: 0, released: false, key: 0,
            midi: 60., from: 60., glide_t: 0., t0: 0., off_at: 0., amp: 0., pan: 0., dark: 0., vel: 0., rand: 0., sends: Sends::default() }
    }
}

/// A stereo delay line for a rack's delay, from the engine's pool.
pub struct DelayLine { l: Vec<f32>, r: Vec<f32> }
impl DelayLine { pub fn new(fs: f64) -> Self { let n = (fs * 2.1).ceil() as usize; DelayLine { l: vec![0.; n], r: vec![0.; n] } } }

struct FxState {
    held: [f64; 2], hc: u32, cb: [Vec<f32>; 2], cw: i64, cph: f64,
    db: Option<Box<DelayLine>>, dw: usize, dlp: [f64; 2],
    eq: [[f64; 4]; 4], eq_key: (i64, i64), eqc: [[f64; 5]; 2], env: f64,
    hb: [Vec<f32>; 2], hw: i64, hph: [f64; 7], fbuf: [Vec<f32>; 2], fw: i64, fph: f64,
    ap: [f64; 12], pph: f64, pz: [f64; 2], pa: [f64; 2],
    verb: Option<Box<Verb>>, verb_fresh: bool, vp: VerbParams, vl: [f32; BLOCK], vr: [f32; BLOCK],
}
impl FxState {
    fn new() -> Self {
        FxState { held: [0.; 2], hc: 0, cb: [vec![0.; 4096], vec![0.; 4096]], cw: 0, cph: 0., db: None, dw: 0, dlp: [0.; 2],
            eq: [[0.; 4]; 4], eq_key: (i64::MIN, i64::MIN), eqc: [[0.; 5]; 2], env: 0., hb: [vec![0.; 4096], vec![0.; 4096]], hw: 0, hph: [0.; 7],
            fbuf: [vec![0.; 1024], vec![0.; 1024]], fw: 0, fph: 0., ap: [0.; 12], pph: 0., pz: [0.; 2], pa: [0.; 2],
            verb: None, verb_fresh: false, vp: VerbParams::default(), vl: [0.; BLOCK], vr: [0.; BLOCK] }
    }
    fn reset(&mut self) {
        let db = self.db.take(); let verb = self.verb.take();
        let (mut cb, mut hb, mut fb) = (std::mem::take(&mut self.cb), std::mem::take(&mut self.hb), std::mem::take(&mut self.fbuf));
        for b in cb.iter_mut().chain(hb.iter_mut()).chain(fb.iter_mut()) { b.fill(0.); }
        *self = FxState { cb, hb, fbuf: fb, ..FxState::new_light() };
        self.db = db; self.verb = verb;
    }
    fn new_light() -> Self {
        FxState { held: [0.; 2], hc: 0, cb: [Vec::new(), Vec::new()], cw: 0, cph: 0., db: None, dw: 0, dlp: [0.; 2],
            eq: [[0.; 4]; 4], eq_key: (i64::MIN, i64::MIN), eqc: [[0.; 5]; 2], env: 0., hb: [Vec::new(), Vec::new()], hw: 0, hph: [0.; 7],
            fbuf: [Vec::new(), Vec::new()], fw: 0, fph: 0., ap: [0.; 12], pph: 0., pz: [0.; 2], pa: [0.; 2],
            verb: None, verb_fresh: false, vp: VerbParams::default(), vl: [0.; BLOCK], vr: [0.; BLOCK] }
    }
}

struct PartSlot {
    used: bool, id: u32, cp: Option<Arc<CompiledPatch>>, ver: u64, mac: [f64; WAVE_MACROS], motions: [Option<Motion>; WAVE_MACROS],
    last: Option<f64>, bus_l: [f32; BLOCK], bus_r: [f32; BLOCK], fx: FxState, fxv: [f64; NP], quiet: f64, live: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct NoteMsg { pub at: f64, pub part: u32, pub key: u32, pub midi: f64, pub amp: f64, pub vel: f64, pub pan: f64, pub hold: f64, pub dark: f64, pub sends: Sends }

pub(crate) struct Aux { pub l: [[f32; BLOCK]; FIELD_SLOTS], pub r: [[f32; BLOCK]; FIELD_SLOTS], pub used: [bool; FIELD_SLOTS] }
impl Aux {
    pub fn new() -> Self { Aux { l: [[0.; BLOCK]; FIELD_SLOTS], r: [[0.; BLOCK]; FIELD_SLOTS], used: [false; FIELD_SLOTS] } }
    pub fn clear(&mut self) { self.used = [false; FIELD_SLOTS]; }
    #[inline]
    pub fn take(&mut self, k: usize) -> (&mut [f32; BLOCK], &mut [f32; BLOCK]) {
        if !self.used[k] { self.used[k] = true; self.l[k].fill(0.); self.r[k].fill(0.); }
        (&mut self.l[k], &mut self.r[k])
    }
}

struct Scratch { ol: [f64; WSLICE], or: [f64; WSLICE], mix_l: [f64; WSLICE], mix_r: [f64; WSLICE], bws: [f64; 8] }

pub struct WaveSynth {
    fs: f64,
    pub bpm: f64,
    voices: Vec<Voice>,
    slots: Vec<PartSlot>,
    sc: Scratch,
    queue: Vec<NoteMsg>,
    offs: Vec<(f64, u32)>,
    delays: Vec<Box<DelayLine>>,
    verbs: Vec<Box<Verb>>,
    ver: u64,
    streams: f64,
    uni_scale: f64,
    frame: u64,
    motion_glide: f64,
    pub watch: Option<u32>,
    w_age: i64,
    w_n: [f64; NP],
    w_slot: Option<usize>,
    pub dropped: u32,
    rng: Rng,
}

/* envelopes: linear attack, exponential decay and release, each reaching ~1% in its time */
#[inline]
fn env_coefs(c: &mut [f64; 3], a: f64, d: f64, r: f64, fs: f64) {
    c[0] = 1. / (a.max(0.0005) * fs); c[1] = (-4.6 / (d.max(0.001) * fs)).exp(); c[2] = (-4.6 / (r.max(0.001) * fs)).exp();
}
/// An envelope stepped a whole slice at once, in closed form.
#[inline]
fn env_skip(e: &mut Env, c: &[f64; 3], s: f64, n: usize) -> f64 {
    match e.st {
        1 => { e.lv += c[0] * n as f64; if e.lv >= 1. { e.lv = 1.; e.st = 2; } }
        2 => { e.lv = s + (e.lv - s) * c[1].powi(n as i32); if (e.lv - s).abs() < 1e-5 { e.lv = s; e.st = if s > 0. { 3 } else { 0 }; } }
        3 => e.lv = s,
        4 => { e.lv *= c[2].powi(n as i32); if e.lv < 1e-5 { e.lv = 0.; e.st = 0; } }
        _ => {}
    }
    e.lv
}
/// Headroom a phase warp needs above the note, for picking a mip level that stays under Nyquist.
#[inline]
fn warp_room(m: u8, w: f64) -> f64 { if (1..=6).contains(&m) { 1. + w.abs() * if m == 2 { 7. } else { 3. } } else { 1. } }
/// Phase warps: where in the table to read for phase q; -1 is silence (pwm's closed part). m is the modulator sine.
#[inline]
fn warp_q(mode: u8, q: f64, w: f64, m: f64) -> f64 {
    if w == 0. { return q; }
    let a = w.abs();
    match mode {
        1 => if w > 0. { q.powf(1. + 3. * w) } else { 1. - (1. - q).powf(1. - 3. * w) },
        2 => (q * (1. + a * 7.)) % 1.,
        3 => { let d = 1. - 0.95 * a; if q < d { q / d } else { -1. } }
        4 => { let d = 0.5 + 0.45 * w; if q < d { q * 0.5 / d } else { 0.5 + (q - d) * 0.5 / (1. - d) } }
        5 => q + a * ((if q < 0.5 { q } else { 1. - q }) - q),
        6 => { let r = (q + a * 1.5 * m) % 1.; if r < 0. { r + 1. } else { r } }
        _ => q,
    }
}
/// Amplitude warps on the value read.
#[inline]
fn warp_y(mode: u8, y: f64, q: f64, w: f64, m: f64) -> f64 {
    let a = w.abs();
    if a == 0. { return y; }
    match mode {
        7 => if q > 1. - a * 0.5 { -y } else { y },
        8 => { let l = 2f64.powf(1. + (1. - a) * 7.); crate::params::js_round(y * l) / l }
        9 => y * (1. - a * (0.5 - 0.5 * m)),
        10 => y * (1. - a + a * m),
        _ => y,
    }
}
#[inline]
fn lfo_at(shape: u8, ph: f64, l: &LfoState) -> f64 {
    match shape {
        1 => if ph < 0.5 { ph * 2. } else { 2. - ph * 2. },
        2 => ph,
        3 => 1. - ph,
        4 => if ph < 0.5 { 1. } else { 0. },
        5 => l.held,
        6 => { let u = ph * ph * (3. - 2. * ph); l.prev + (l.held - l.prev) * u }
        _ => 0.5 - 0.5 * (TAU * ph).cos(),
    }
}
#[inline]
fn hz(m: f64) -> f64 { 440. * 2f64.powf((m - 69.) / 12.) }

/// RBJ shelving biquad coefficients
fn shelf(f: f64, db: f64, fs: f64, high: bool) -> [f64; 5] {
    let a = 10f64.powf(db / 40.); let w = TAU * f / fs; let (cs, sn) = (w.cos(), w.sin()); let al = sn / 2. * SQRT_2; let sq = 2. * a.sqrt() * al;
    let (b0, b1, b2, a0, a1, a2);
    if high {
        b0 = a * ((a + 1.) + (a - 1.) * cs + sq); b1 = -2. * a * ((a - 1.) + (a + 1.) * cs); b2 = a * ((a + 1.) + (a - 1.) * cs - sq);
        a0 = (a + 1.) - (a - 1.) * cs + sq; a1 = 2. * ((a - 1.) - (a + 1.) * cs); a2 = (a + 1.) - (a - 1.) * cs - sq;
    } else {
        b0 = a * ((a + 1.) - (a - 1.) * cs + sq); b1 = 2. * a * ((a - 1.) - (a + 1.) * cs); b2 = a * ((a + 1.) - (a - 1.) * cs - sq);
        a0 = (a + 1.) + (a - 1.) * cs + sq; a1 = -2. * ((a - 1.) + (a + 1.) * cs); a2 = (a + 1.) + (a - 1.) * cs - sq;
    }
    [b0 / a0, b1 / a0, b2 / a0, a1 / a0, a2 / a0]
}
#[inline]
fn biquad(c: &[f64; 5], z: &mut [f64; 4], x: f64) -> f64 {
    let y = c[0] * x + c[1] * z[0] + c[2] * z[1] - c[3] * z[2] - c[4] * z[3];
    z[1] = z[0]; z[0] = x; z[3] = z[2]; z[2] = y; y
}

impl WaveSynth {
    pub fn new(fs: f64, parts: usize, delays: usize, verbs: usize) -> Self {
        builtin();
        WaveSynth {
            fs, bpm: 120., voices: (0..WVOICES).map(|_| Voice::new()).collect(),
            slots: (0..parts).map(|_| PartSlot { used: false, id: 0, cp: None, ver: 0, mac: [0.; 8], motions: [None; 8], last: None,
                bus_l: [0.; BLOCK], bus_r: [0.; BLOCK], fx: FxState::new(), fxv: [0.; NP], quiet: 1e9, live: false }).collect(),
            sc: Scratch { ol: [0.; WSLICE], or: [0.; WSLICE], mix_l: [0.; WSLICE], mix_r: [0.; WSLICE], bws: [0.; 8] },
            queue: Vec::with_capacity(512), offs: Vec::with_capacity(512),
            delays: (0..delays).map(|_| Box::new(DelayLine::new(fs))).collect(),
            verbs: (0..verbs).map(|_| Box::new(Verb::new(fs, &VerbParams::default()))).collect(),
            ver: 0, streams: 0., uni_scale: 1., frame: 0, motion_glide: 1. - (-(WSLICE as f64) / (0.003 * fs)).exp(),
            watch: None, w_age: -1, w_n: [0.; NP], w_slot: None, dropped: 0, rng: Rng::new(0x5eed_0001),
        }
    }
    fn slot_of(&self, part: u32) -> Option<usize> { self.slots.iter().position(|s| s.used && s.id == part) }
    fn slot_for(&mut self, part: u32) -> Option<usize> {
        if let Some(i) = self.slot_of(part) { return Some(i); }
        let i = self.slots.iter().position(|s| !s.used)?;
        let s = &mut self.slots[i];
        s.used = true; s.id = part; s.cp = None; s.motions = [None; 8]; s.last = None; s.quiet = 1e9; s.live = false;
        s.fx.reset();
        Some(i)
    }
    /// A patch arrives whole, compiled. Returns the replaced patch, for the caller to drop off the audio thread if it wants.
    pub fn set_patch(&mut self, part: u32, cp: Arc<CompiledPatch>) -> Option<Arc<CompiledPatch>> {
        let Some(i) = self.slot_for(part) else { self.dropped += 1; return Some(cp) };
        self.ver += 1;
        let s = &mut self.slots[i];
        if cp.fx_on[FX_DELAY] && s.fx.db.is_none() { if let Some(mut d) = self.delays.pop() { d.l.fill(0.); d.r.fill(0.); s.fx.db = Some(d); s.fx.dw = 0; } }
        if cp.fx_on[FX_REVERB] && s.fx.verb.is_none() { if let Some(v) = self.verbs.pop() { s.fx.verb = Some(v); s.fx.verb_fresh = true; } }
        // the macros as they play: the knobs' values, or where a motion part has turned them
        for k in 0..WAVE_MACROS { s.mac[k] = match s.motions[k] { Some(m) => m.y, None => cp.base[P_MAC0 + k] }; }
        s.ver = self.ver;
        s.cp.replace(cp)
    }
    /// A part is gone: its voices stop, its pooled delay and reverb go back.
    pub fn remove_part(&mut self, part: u32) -> Option<Arc<CompiledPatch>> {
        let i = self.slot_of(part)?;
        for v in &mut self.voices { if v.on && v.slot == i { v.on = false; } }
        let s = &mut self.slots[i];
        s.used = false;
        if let Some(d) = s.fx.db.take() { self.delays.push(d); }
        if let Some(v) = s.fx.verb.take() { self.verbs.push(v); }
        s.cp.take()
    }
    pub fn motion(&mut self, part: u32, slot: usize, prog: Option<MotionProg>) {
        if slot >= WAVE_MACROS { return; }
        let Some(i) = self.slot_for(part) else { return };
        let fs = self.fs; let frame = self.frame as f64; let mg = self.motion_glide;
        let s = &mut self.slots[i];
        match prog {
            None => { s.motions[slot] = None; if let Some(cp) = &s.cp { s.mac[slot] = cp.base[P_MAC0 + slot]; } }
            Some(p) => {
                let g = if p.glide > 0. { 1. - (-(WSLICE as f64) / (p.glide * fs)).exp() } else { mg };
                let y = match s.motions[slot] { Some(o) => o.y, None => motion_at(&p, frame, fs) };
                s.motions[slot] = Some(Motion { p, g, y });
            }
        }
    }
    pub fn note(&mut self, m: NoteMsg) { if self.queue.len() < self.queue.capacity() { self.queue.push(m) } else { self.dropped += 1 } }
    pub fn off(&mut self, at: f64, key: u32) { if self.offs.len() < self.offs.capacity() { self.offs.push((at, key)) } else { self.dropped += 1 } }
    pub fn panic(&mut self) { for v in &mut self.voices { v.on = false; } self.queue.clear(); self.offs.clear(); }
    pub fn voices(&self) -> usize { self.voices.iter().filter(|v| v.on).count() }
    /// Where the watched part's newest note had each routed knob (normalised) in the last block: (knob, position)
    /// pairs into out; returns how many.
    pub fn watched_into(&self, out: &mut [(u16, f64)]) -> usize {
        let (Some(si), true) = (self.w_slot, self.w_age >= 0) else { return 0 };
        let Some(cp) = &self.slots[si].cp else { return 0 };
        let mut n = 0;
        for &d in &cp.modded { if n < out.len() { out[n] = (d, self.w_n[d as usize]); n += 1; } }
        n
    }

    fn start(&mut self, m: &NoteMsg) {
        let Some(si) = self.slot_of(m.part) else { return };
        let Some(cp) = self.slots[si].cp.clone() else { return };
        let fs = self.fs;
        let (mut live, mut free, mut oldest, mut mine, mut held) = (0, None::<usize>, None::<usize>, None::<usize>, false);
        for (i, v) in self.voices.iter().enumerate() {
            if !v.on { if free.is_none() { free = Some(i); } continue; }
            if v.kill != 0. { continue; }
            live += 1;
            if oldest.map_or(true, |o| v.age < self.voices[o].age) { oldest = Some(i); }
            if v.slot == si { if mine.map_or(true, |o| v.age > self.voices[o].age) { mine = Some(i); } if !v.released { held = true; } }
        }
        let gl = cp.val[P_GLIDE];
        // mono and legato: one voice per patch, gliding to the new note; mono restarts the envelopes from where they
        // are, legato only when the last note had already let go
        if cp.mode != 0 { if let Some(mi) = mine {
            for (i, v) in self.voices.iter_mut().enumerate() { if i != mi && v.on && v.slot == si { v.kill = 1.; } }
            let frame = self.frame;
            let from = if gl > 0. && (cp.always || held) { pitch(&self.voices[mi], m.at, fs) } else { m.midi };
            let gt = glide_time(&cp, from, m.midi);
            let v = &mut self.voices[mi];
            v.from = from; v.midi = m.midi; v.t0 = m.at; v.sends = m.sends; v.key = m.key; v.age = frame; v.amp = m.amp;
            v.vel = m.vel.clamp(0., 1.); v.glide_t = gt; v.off_at = m.at + m.hold.max(cp.val[P_GATE]) * fs;
            self.slots[si].last = Some(m.midi);
            if cp.mode == 1 || !held { for o in &mut v.osc { o.env.st = 1; o.fenv.st = 1; } for e in &mut v.envs { e.st = 1; } }
            v.released = false;
            return;
        } }
        if live >= WPOLY { if let Some(o) = oldest { self.voices[o].kill = 1.; } }
        let Some(vi) = free.or(oldest) else { return };
        let last = self.slots[si].last;
        let from = if gl > 0. && (cp.always || held) && last.is_some() { last.unwrap() } else { m.midi };
        self.slots[si].last = Some(m.midi);
        let mut r = [0.; 32];
        for x in &mut r { *x = self.rng.next(); }
        let frame = self.frame;
        let v = &mut self.voices[vi];
        v.on = true; v.kill = 0.; v.slot = si; v.ver = u64::MAX; v.age = frame; v.key = m.key; v.midi = m.midi; v.from = from;
        v.glide_t = glide_time(&cp, from, m.midi);
        v.amp = m.amp; v.pan = m.pan; v.dark = m.dark; v.sends = m.sends; v.vel = m.vel.clamp(0., 1.); v.rand = r[0];
        v.t0 = m.at; v.off_at = m.at + m.hold.max(cp.val[P_GATE]) * fs; v.released = false; v.dl = 0.; v.dr = 0.;
        v.vals = cp.val;
        for i in 0..WAVE_OSCS {
            let o = &mut v.osc[i];
            o.env = Env { st: 1, lv: 0. }; o.fenv = Env { st: 1, lv: 0. }; o.z = [0.; 8];
            let ph = cp.val[i * OSC_N + O_PHASE];
            for j in 0..8 { o.ph[j] = ph + if j > 0 { r[1 + i * 7 + j - 1] } else { 0. }; }
        }
        for e in &mut v.envs { *e = Env { st: 1, lv: 0. }; }
        for i in 0..WAVE_LFOS { v.lfo[i] = LfoState { ph: 0., held: r[22 + i * 2], prev: r[23 + i * 2] }; }
        v.sub_ph = 0.;
    }

    /// Render one block (BLOCK frames) of every patch, after its FX rack, into out (added); field sends into aux.
    pub(crate) fn process(&mut self, frame: u64, n: usize, out_l: &mut [f32], out_r: &mut [f32], aux: &mut Aux) {
        let fs = self.fs;
        self.frame = frame;
        // last block's demand sets this block's unison budget
        let per = self.streams / (n as f64 / WSLICE as f64).max(1.);
        self.uni_scale = if per > WSTREAMS { WSTREAMS / per } else { 1. };
        self.streams = 0.;
        self.w_age = -1;
        self.w_slot = self.watch.and_then(|p| self.slot_of(p));
        let mut at = 0;
        while at < n {
            let sl = WSLICE.min(n - at);
            let t = (frame + (at + sl) as u64) as f64;
            // motions
            let fnow = (frame + at as u64) as f64;
            for s in self.slots.iter_mut() {
                if !s.used { continue; }
                for k in 0..WAVE_MACROS { if let Some(mo) = &mut s.motions[k] { let x = motion_at(&mo.p, fnow, fs); mo.y += (x - mo.y) * mo.g; if s.cp.is_some() { s.mac[k] = mo.y; } } }
            }
            let mut i = 0;
            while i < self.queue.len() { if self.queue[i].at < t { let m = self.queue.remove(i); self.start(&m); } else { i += 1; } }
            let mut i = 0;
            while i < self.offs.len() {
                if self.offs[i].0 < t {
                    let key = self.offs.remove(i).1;
                    for v in &mut self.voices { if v.on && v.key == key && !v.released { release(v); } }
                } else { i += 1; }
            }
            for vi in 0..self.voices.len() {
                if !self.voices[vi].on { continue; }
                let si = self.voices[vi].slot;
                let s = &mut self.slots[si];
                if !s.live { s.live = true; s.bus_l.fill(0.); s.bus_r.fill(0.); }
                self.sc.mix_l[..sl].fill(0.); self.sc.mix_r[..sl].fill(0.);
                self.render(vi, sl, at, aux);
            }
            at += sl;
        }
        // each patch's voices are summed on its bus and go through its FX rack; a rack with a tail keeps running
        let bpm = self.bpm;
        for s in self.slots.iter_mut() {
            let Some(cp) = &s.cp else { continue };
            if s.live { s.quiet = 0.; } else {
                if s.quiet > cp.fx_tail(&s.fxv) * fs { continue; }
                s.quiet += n as f64; s.bus_l.fill(0.); s.bus_r.fill(0.);
            }
            run_fx(cp, &mut s.fxv, &s.mac, &mut s.fx, &mut s.bus_l[..n], &mut s.bus_r[..n], bpm, fs);
            for k in 0..n { out_l[k] += s.bus_l[k]; out_r[k] += s.bus_r[k]; }
            s.live = false;
        }
    }

    /// One voice for one slice: sources, routed values, then the oscillators through their filters.
    fn render(&mut self, vi: usize, n: usize, at: usize, aux: &mut Aux) {
        let fs = self.fs;
        let now = (self.frame + at as u64) as f64;
        let bpm = self.bpm;
        let v = &mut self.voices[vi];
        let s = &mut self.slots[v.slot];
        let Some(cp) = s.cp.as_deref() else { v.on = false; return };
        if !v.released && now >= v.off_at { release(v); }
        if v.ver != s.ver { v.vals = cp.val; v.ver = s.ver; }
        let lfo_rate = |vals: &[f64; NP], i: usize| { let b = cp.lfo_sync[i]; if b != 0. { bpm / 60. / b } else { vals[P_LFO0 + 2 * i] } };
        for i in 0..WAVE_ENVS { v.src[SRC_ENV0 + i] = v.envs[i].lv; }
        for i in 0..WAVE_LFOS {
            let l = v.lfo[i];
            let mut ph = l.ph;
            if !cp.lfo_retrig[i] { ph = now / fs * lfo_rate(&v.vals, i); }
            ph = (ph + v.vals[P_LFO0 + 2 * i + 1]) % 1.;
            v.src[SRC_LFO0 + i] = lfo_at(cp.lfo_shape[i], ph, &l);
        }
        for i in 0..WAVE_MACROS { v.src[SRC_MAC0 + i] = s.mac[i]; }
        v.src[SRC_VEL] = v.vel; v.src[SRC_NOTE] = ((v.midi - 24.) / 84.).clamp(0., 1.); v.src[SRC_RAND] = v.rand;
        if !cp.routes.is_empty() {
            for &d in &cp.modded { v.vals[d as usize] = cp.base[d as usize]; }
            for r in &cp.routes {
                let x = mod_curve(v.src[r.s], r.cv);
                let aux_g = match r.ax { None => 1., Some(a) => if r.ai { 1. - v.src[a] } else { v.src[a] } };
                v.vals[r.d] += (if r.bi { x * 2. - 1. } else { x }) * r.amt * aux_g;
            }
            if self.w_slot == Some(v.slot) && v.age as i64 >= self.w_age {
                self.w_age = v.age as i64;
                for &d in &cp.modded { self.w_n[d as usize] = v.vals[d as usize]; }
            }
            let params = wave_params();
            for &d in &cp.modded { v.vals[d as usize] = wave_denorm(&params[d as usize], v.vals[d as usize]); }
        }
        for i in 0..WAVE_LFOS {
            let nph = v.lfo[i].ph + lfo_rate(&v.vals, i) * n as f64 / fs;
            if nph >= 1. { v.lfo[i].prev = v.lfo[i].held; v.lfo[i].held = self.rng.next(); }
            v.lfo[i].ph = nph % 1.;
        }
        // the patch transposes itself: every note into it, from any part, sounds that far up or down
        let midi = pitch(v, now, fs) + 12. * v.vals[P_TOCT] + v.vals[P_TSEMI];
        let mut alive = false;
        for i in 0..WAVE_OSCS {
            if cp.on[i] && osc(v, cp, &mut self.sc, &mut self.streams, self.uni_scale, i, n, midi, fs) { alive = true; }
        }
        // sub and noise ride ENV1, sample by sample; the other envelopes only modulate, so they step a slice at a time
        let (sub_l, nz_l) = (v.vals[P_SUB], v.vals[P_NOISE]);
        for i in 0..WAVE_ENVS { let e = P_ENV0 + 4 * i; let (a, d, r) = (v.vals[e], v.vals[e + 1], v.vals[e + 3]); env_coefs(&mut v.ecs[i], a, d, r, fs); }
        for i in 1..WAVE_ENVS { let su = v.vals[P_ENV0 + 4 * i + 2]; let c = v.ecs[i]; env_skip(&mut v.envs[i], &c, su, n); }
        let s1 = v.vals[P_ENV0 + 2];
        let (mut sp, mut nz) = (v.sub_ph, v.nz);
        let c0 = v.ecs[0];
        if sub_l > 0. || nz_l > 0. {
            let sinc = hz(midi + 12. * v.vals[P_SUBOCT]) / fs;
            let ca = 1. - (-TAU * (0.45 * fs).min(200. * 2f64.powf(v.vals[P_COLOR] * 7.)) / fs).exp();
            let q0 = v.envs[0].lv; let q1 = env_skip(&mut v.envs[0], &c0, s1, n); let dq = (q1 - q0) / n as f64;
            let rng = &mut self.rng;
            for k in 0..n {
                let e = q0 + dq * (k + 1) as f64;
                let mut y = 0.;
                if sub_l > 0. { y += (TAU * sp).sin() * sub_l; sp += sinc; if sp >= 1. { sp -= 1.; } }
                if nz_l > 0. { nz += (rng.next() * 2. - 1. - nz) * ca; y += nz * nz_l * 1.6; }
                self.sc.mix_l[k] += y * e; self.sc.mix_r[k] += y * e;
            }
        } else { env_skip(&mut v.envs[0], &c0, s1, n); }
        v.sub_ph = sp; v.nz = nz;
        if (sub_l > 0. || nz_l > 0.) && v.envs[0].st != 0 { alive = true; }
        // the voice's level, the room's distance darkening and its place in the stereo field
        let amp = v.amp * v.vals[P_VOL] * 0.25;
        let pa = (v.pan.clamp(-1., 1.) + 1.) * PI / 4.;
        let (pl, pr) = (pa.cos() * SQRT_2 * amp, pa.sin() * SQRT_2 * amp);
        let dc = if v.dark > 0.02 { 1. - (-TAU * 16000. * (-v.dark * 3.2).exp() / fs).exp() } else { 1. };
        let (mut dl, mut dr, mut kill) = (v.dl, v.dr, v.kill);
        let kstep = 1. / (0.005 * fs);
        let has_fx = v.sends.any();
        let dg = v.sends.dry as f64;
        let (ml, mr) = (&mut self.sc.mix_l, &mut self.sc.mix_r);
        for k in 0..n {
            let (mut xl, mut xr) = (ml[k], mr[k]);
            if dc < 1. { dl += (xl - dl) * dc; dr += (xr - dr) * dc; xl = dl; xr = dr; }
            let mut g = 1.;
            if kill != 0. { kill = (kill - kstep).max(0.); g = kill; }
            let (yl, yr) = (xl * pl * g, xr * pr * g);
            ml[k] = yl; mr[k] = yr;
            s.bus_l[at + k] += (yl * dg) as f32; s.bus_r[at + k] += (yr * dg) as f32;
        }
        // in a field: the dry part to the patch's bus, and the whole of it to each field's slot by its send
        if has_fx {
            for f in 0..FIELD_SLOTS {
                let a = v.sends.amt[f] as f64;
                if a == 0. { continue; }
                let (al, ar) = aux.take(f);
                for k in 0..n { al[at + k] += (ml[k] * a) as f32; ar[at + k] += (mr[k] * a) as f32; }
            }
        }
        v.dl = dl; v.dr = dr;
        if v.kill != 0. { v.kill = kill; if kill <= 0. { v.on = false; } }
        if !alive { v.on = false; }
    }
}

fn release(v: &mut Voice) {
    if v.released { return; }
    v.released = true;
    for o in &mut v.osc { if o.env.st != 0 { o.env.st = 4; } if o.fenv.st != 0 { o.fenv.st = 4; } }
    for e in &mut v.envs { if e.st != 0 { e.st = 4; } }
}
/// the voice's pitch now, partway along its glide
fn pitch(v: &Voice, at: f64, fs: f64) -> f64 {
    let tt = (at - v.t0) / fs;
    if v.glide_t > 0. { v.midi + (v.from - v.midi) * (-tt / (v.glide_t / 4.6)).exp() } else { v.midi }
}
/// glide time; scaled, it is per octave of the interval, so short steps glide faster
fn glide_time(cp: &CompiledPatch, a: f64, b: f64) -> f64 { let g = cp.val[P_GLIDE]; if cp.glide_scale { g * (a - b).abs() / 12. } else { g } }

/// One oscillator of one voice for one slice, into the voice's mix; false once its envelope has finished.
#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn osc(v: &mut Voice, cp: &CompiledPatch, sc: &mut Scratch, streams: &mut f64, uni_scale: f64, i: usize, n: usize, midi: f64, fs: f64) -> bool {
    let b = i * OSC_N;
    let vals = &v.vals;
    let o = &mut v.osc[i];
    let level = vals[b + O_LEVEL];
    env_coefs(&mut o.ec, vals[b + O_A], vals[b + O_D], vals[b + O_R], fs);
    env_coefs(&mut o.fc, vals[b + O_FA], vals[b + O_FD], vals[b + O_FR], fs);
    if o.env.st == 0 { return false; }
    let sus = vals[b + O_S];
    // the filter envelope only moves the cutoff, which is set once per slice: it steps a slice at a time
    let fenv = o.fenv.lv;
    let fc = o.fc;
    env_skip(&mut o.fenv, &fc, vals[b + O_FS], n);
    let ec = o.ec;
    // a silent oscillator (level at 0, as B and C start) costs only its envelope
    if level < 1e-4 { env_skip(&mut o.env, &ec, sus, n); return true; }
    let f = hz(midi + 12. * vals[b + O_OCT] + vals[b + O_SEMI] + vals[b + O_FINE] / 100.);
    // unison under a budget: past WSTREAMS table reads per sample across all voices, every oscillator's unison thins
    let want = crate::params::js_round(vals[b + O_UNI]).clamp(1., 8.) as usize;
    *streams += want as f64;
    let nu = if uni_scale < 1. { (crate::params::js_round(want as f64 * uni_scale) as usize).max(1) } else { want };
    let span = vals[b + O_DETUNE] * vals[b + O_DETUNE] * 1.2;
    let tab = &cp.tabs[i];
    let (wm, wm2, warp, w2) = (cp.warp_mode[i], cp.warp_mode2[i], vals[b + O_WARP], vals[b + O_WARP2]);
    let fmax = f * 2f64.powf(span / 24.) * warp_room(wm, warp) * warp_room(wm2, w2);
    let lim = 0.45 * fs / fmax.max(1.);
    let mut mip = 0;
    while mip < WMIPS - 1 && (1023 >> mip) as f64 > lim { mip += 1; }
    let fp = vals[b + O_POS] * (tab.frames - 1) as f64;
    let f0 = fp.floor() as usize; let ff = fp - f0 as f64; let f1 = (f0 + 1).min(tab.frames - 1);
    let (ta, tb) = (tab.frame(mip, f0), tab.frame(mip, f1));
    let norm = 1. / (nu as f64).sqrt();
    let (pan0, spread, blend) = (vals[b + O_PAN], vals[b + O_SPREAD], vals[b + O_BLEND]);
    // the filter: cutoff from the knob, the key and the filter envelope (±6 octaves), set once per slice; a low-pass
    // left wide open (above 18 kHz, little resonance) is skipped
    let mut ft = cp.ftype[i];
    let cut = (vals[b + O_CUT] * 2f64.powf(vals[b + O_KEY] * (midi - 60.) / 12. + vals[b + O_FAMT] * 6. * fenv)).max(20.).min(fs * 0.45);
    if (ft == 1 || ft == 2) && cut >= 18000. && vals[b + O_RES] < 0.2 { ft = 0; }
    let g = (PI * cut / fs).tan(); let k = 2. - 1.96 * vals[b + O_RES];
    let a1 = 1. / (1. + g * (g + k)); let a2 = g * a1; let a3 = g * a2;
    let drive = 1. + vals[b + O_DRIVE] * 8.; let dn = 1. / drive.sqrt();
    // one voice of unison is mono: read, drive and filter one channel, pan at the end
    let mono = nu == 1;
    let plain = (wm == 0 || wm == 1 && warp == 0.) && (wm2 == 0 || wm2 == 1 && w2 == 0.);
    let (ol, or) = (&mut sc.ol, &mut sc.or);
    ol[..n].fill(0.); if !mono { or[..n].fill(0.); }
    let bws = &mut sc.bws;
    let mut sw = 0.;
    for j in 0..nu {
        bws[j] = if nu > 1 { (1. + (blend - 0.5) * 2. * ((j as f64 / (nu - 1) as f64 - 0.5).abs() * 4. - 1.)).max(0.) } else { 1. };
        sw += bws[j] * bws[j];
    }
    let bn = if sw > 1e-9 { (nu as f64 / sw).sqrt() } else { 1. };
    if sw <= 1e-9 { bws.fill(1.); }
    let src = if i > 0 { 0 } else { OSC_N };
    let needs_m = |m: u8| m >= 6 && m != 7 && m != 8;
    let minc = if needs_m(wm) || needs_m(wm2) { hz(midi + 12. * vals[src + O_OCT] + vals[src + O_SEMI] + vals[src + O_FINE] / 100.) / fs } else { 0. };
    let mut mp = o.mph; let mp0 = mp;
    let (mut mgl, mut mgr) = (0., 0.);
    let wnf = WN as f64;
    for j in 0..nu {
        mp = mp0;
        let off = if nu > 1 { (j as f64 / (nu - 1) as f64 - 0.5) * span } else { 0. };
        let inc = f * 2f64.powf(off / 12.) / fs;
        let p = (pan0 + if nu > 1 { spread * (j as f64 / (nu - 1) as f64 * 2. - 1.) } else { 0. }).clamp(-1., 1.);
        let ang = (p + 1.) * PI / 4.;
        let bw = bws[j] * bn;
        let gl_ = ang.cos() * norm * level * SQRT_2 * bw; let gr_ = ang.sin() * norm * level * SQRT_2 * bw;
        let mut ph = o.ph[j];
        if mono { mgl = gl_; mgr = gr_; }
        let wl = if mono { 1. } else { gl_ };
        if plain {
            for s in 0..n {
                let x = ph * wnf; let xi = x as usize; let xf = x - xi as f64;
                let a = ta[xi] as f64 + (ta[xi + 1] - ta[xi]) as f64 * xf;
                let y = a + (tb[xi] as f64 + (tb[xi + 1] - tb[xi]) as f64 * xf - a) * ff;
                ol[s] += y * wl; if !mono { or[s] += y * gr_; }
                ph += inc; if ph >= 1. { ph -= 1.; }
            }
        } else {
            for s in 0..n {
                let m = if minc != 0. { (TAU * mp).sin() } else { 0. };
                let mut q = warp_q(wm, ph, warp, m);
                let mut y = 0.;
                if q >= 0. && wm2 != 0 { q = warp_q(wm2, q, w2, m); }
                if q >= 0. {
                    let x = q * wnf; let xi = (x as usize).min(WN - 1); let xf = x - xi as f64;
                    let a = ta[xi] as f64 + (ta[xi + 1] - ta[xi]) as f64 * xf; let bb = tb[xi] as f64 + (tb[xi + 1] - tb[xi]) as f64 * xf;
                    y = a + (bb - a) * ff;
                    if wm >= 7 { y = warp_y(wm, y, q, warp, m); }
                    if wm2 >= 7 { y = warp_y(wm2, y, q, w2, m); }
                }
                ol[s] += y * wl; if !mono { or[s] += y * gr_; }
                ph += inc; if ph >= 1. { ph -= 1.; }
                mp += minc; if mp >= 1. { mp -= 1.; }
            }
        }
        o.ph[j] = ph;
    }
    o.mph = mp;
    // amp envelope, drive and filter per sample, in place, then into the voice's mix; the amp envelope is stepped per
    // slice in closed form and ramped linearly across it
    let chans = if mono { 1 } else { 2 };
    let e0 = o.env.lv; let e1 = env_skip(&mut o.env, &ec, sus, n); let de = (e1 - e0) / n as f64;
    let z = &mut o.z;
    for s in 0..n {
        let e = e0 + de * (s + 1) as f64;
        for c in 0..chans {
            let mut x = if c == 1 { or[s] } else { ol[s] };
            if drive > 1. { x = (x * drive).tanh() * dn; }
            if ft != 0 {
                let zb = c * 4;
                let mut v3 = x - z[zb + 1];
                let v1 = a1 * z[zb] + a2 * v3; let v2 = z[zb + 1] + a2 * z[zb] + a3 * v3;
                z[zb] = 2. * v1 - z[zb]; z[zb + 1] = 2. * v2 - z[zb + 1];
                let (lp, bp, hp) = (v2, v1, x - k * v1 - v2);
                if ft == 2 || ft == 4 {
                    let x2 = if ft == 2 { lp } else { hp };
                    v3 = x2 - z[zb + 3];
                    let u1 = a1 * z[zb + 2] + a2 * v3; let u2 = z[zb + 3] + a2 * z[zb + 2] + a3 * v3;
                    z[zb + 2] = 2. * u1 - z[zb + 2]; z[zb + 3] = 2. * u2 - z[zb + 3];
                    x = if ft == 2 { u2 } else { x2 - k * u1 - u2 };
                } else {
                    x = match ft { 1 => lp, 3 => hp, 5 => bp * k, 6 => lp + hp, _ => lp - hp };
                }
            }
            if c == 1 { or[s] = x * e; } else { ol[s] = x * e; }
        }
    }
    if mono { for s in 0..n { sc.mix_l[s] += sc.ol[s] * mgl; sc.mix_r[s] += sc.ol[s] * mgr; } }
    else { for s in 0..n { sc.mix_l[s] += sc.ol[s]; sc.mix_r[s] += sc.or[s]; } }
    true
}

/* the FX rack: run in the patch's order on its summed voices */
#[allow(clippy::too_many_arguments)]
fn run_fx(cp: &CompiledPatch, v: &mut [f64; NP], mac: &[f64; WAVE_MACROS], st: &mut FxState, l: &mut [f32], r: &mut [f32], bpm: f64, fs: f64) {
    // FX knobs take the macros' routes (a rack is shared by every voice, so per-note sources do not reach it)
    for i in P_FX0..NP { v[i] = cp.base[i]; }
    for rt in &cp.routes {
        let r = rt;
        if r.s >= SRC_MAC0 && r.s < SRC_MAC0 + WAVE_MACROS && is_fx_param(r.d) {
            let x = mod_curve(mac[r.s - SRC_MAC0], r.cv);
            let a = match r.ax { Some(ax) if ax >= SRC_MAC0 && ax < SRC_MAC0 + WAVE_MACROS => mac[ax - SRC_MAC0], _ => 1. };
            v[r.d] += (if r.bi { x * 2. - 1. } else { x }) * r.amt * if r.ax.is_none() { 1. } else if r.ai { 1. - a } else { a };
        }
    }
    let params = wave_params();
    for i in P_FX0..NP { v[i] = wave_denorm(&params[i], v[i]); }
    for &k in &cp.fx_order {
        let k = k as usize;
        if !cp.fx_on[k] { continue; }
        match k {
            FX_HYPER => fx_hyper(v, st, l, r, fs),
            FX_DIST => fx_dist(cp, v, st, l, r),
            FX_FLANGER => fx_flanger(v, st, l, r, fs),
            FX_PHASER => fx_phaser(v, st, l, r, fs),
            FX_CHORUS => fx_chorus(v, st, l, r, fs),
            FX_DELAY => fx_delay(cp, v, st, l, r, bpm, fs),
            FX_REVERB => fx_reverb(cp, v, st, l, r),
            FX_COMP => fx_comp(v, st, l, r, fs),
            _ => fx_eq(v, st, l, r, fs),
        }
    }
}
type Buf = [f32];
/// Reverb: the room's own (verb.rs), one per patch. Mix keeps the dry full to half way and the wet full from half way.
fn fx_reverb(cp: &CompiledPatch, v: &[f64; NP], st: &mut FxState, l: &mut Buf, r: &mut Buf) {
    let Some(verb) = st.verb.as_mut() else { return };
    let p = &mut st.vp;
    p.0[V_MODE] = cp.verb_mode as f64; p.0[V_COLOR] = cp.verb_color as f64; p.0[V_WET] = 1.; p.0[V_PREDELAY] = v[REVERB_PREDELAY] * 1000.;
    p.0[V_DECAY] = v[REVERB_DECAY]; p.0[V_SIZE] = v[REVERB_SIZE]; p.0[V_ATTACK] = v[REVERB_ATTACK]; p.0[V_EARLY] = v[REVERB_EARLY];
    p.0[V_LATE] = v[REVERB_LATE]; p.0[V_MODDEPTH] = v[REVERB_MOD]; p.0[V_MODRATE] = v[REVERB_RATE]; p.0[V_LOWCUT] = v[REVERB_LOWCUT]; p.0[V_HIGHCUT] = v[REVERB_HIGHCUT];
    if st.verb_fresh { st.verb_fresh = false; verb.clear(p); } else { verb.set(p); }
    let n = l.len();
    st.vl[..n].fill(0.); st.vr[..n].fill(0.);
    verb.process(l, r, &mut st.vl[..n], &mut st.vr[..n], n);
    let mix = v[REVERB_MIX]; let dg = (2. * (1. - mix)).min(1.) as f32; let wg = (2. * mix).min(1.) as f32;
    for s in 0..l.len() { l[s] = l[s] * dg + st.vl[s] * wg; r[s] = r[s] * dg + st.vr[s] * wg; }
}
fn fx_dist(cp: &CompiledPatch, v: &[f64; NP], st: &mut FxState, l: &mut Buf, r: &mut Buf) {
    let dr = v[DIST_DRIVE]; let mix = v[DIST_MIX]; let g = 1. + dr * 30.; let inv = 1. / g.sqrt(); let mode = cp.dist_mode;
    let q = 2f64.powf(16. - dr * 14.); let hold = 1 + (dr * 16.).floor() as u32;
    for s in 0..l.len() {
        for c in 0..2 {
            let x = if c == 1 { r[s] } else { l[s] } as f64;
            let y = match mode {
                0 => (x * g).tanh() * inv,
                1 => (x * g).clamp(-1., 1.) * inv,
                2 => (x * g).sin() * inv,
                _ => { if c == 0 { st.hc += 1; if st.hc >= hold { st.hc = 0; } } if st.hc == 0 { st.held[c] = crate::params::js_round(x * q) / q; } st.held[c] }
            };
            let o = (x + (y - x) * mix) as f32;
            if c == 1 { r[s] = o } else { l[s] = o }
        }
    }
}
fn fx_chorus(v: &[f64; NP], st: &mut FxState, l: &mut Buf, r: &mut Buf, fs: f64) {
    let rate = v[CHORUS_RATE]; let dep = v[CHORUS_DEPTH] * 0.006 * fs; let mix = v[CHORUS_MIX]; let base = 0.012 * fs; const M: i64 = 4095;
    for s in 0..l.len() {
        st.cph = (st.cph + rate / fs) % 1.;
        for c in 0..2 {
            let x = if c == 1 { r[s] } else { l[s] } as f64;
            let b = &mut st.cb[c];
            b[(st.cw & M) as usize] = x as f32;
            let d = base + dep * (0.5 + 0.5 * (TAU * (st.cph + c as f64 * 0.25)).sin());
            let p = st.cw as f64 - d; let i = p.floor() as i64; let f = p - i as f64;
            let (b0, b1) = (b[(i & M) as usize] as f64, b[((i + 1) & M) as usize] as f64);
            let w = b0 + (b1 - b0) * f;
            let o = (x * (1. - mix * 0.5) + w * mix) as f32;
            if c == 1 { r[s] = o } else { l[s] = o }
        }
        st.cw += 1;
    }
}
fn fx_delay(cp: &CompiledPatch, v: &[f64; NP], st: &mut FxState, l: &mut Buf, r: &mut Buf, bpm: f64, fs: f64) {
    let Some(db) = st.db.as_mut() else { return };
    let dlen = db.l.len();
    let t = ((if cp.delay_sync != 0. { 60. / bpm * cp.delay_sync } else { v[DELAY_TIME] }) * fs).max(1.).min((dlen - 2) as f64);
    let fb = v[DELAY_FB]; let mix = v[DELAY_MIX];
    let tc = 1. - (-TAU * (0.45 * fs).min(400. * 2f64.powf(v[DELAY_TONE] * 5.5)) / fs).exp();
    let ping = cp.ping;
    for s in 0..l.len() {
        let mut p = st.dw as f64 - t; if p < 0. { p += dlen as f64; }
        let i = p.floor() as usize; let f = p - i as f64; let j = (i + 1) % dlen;
        let dl = db.l[i] as f64 + (db.l[j] - db.l[i]) as f64 * f; let dr = db.r[i] as f64 + (db.r[j] - db.r[i]) as f64 * f;
        st.dlp[0] += (dl - st.dlp[0]) * tc; st.dlp[1] += (dr - st.dlp[1]) * tc;
        let (xl, xr) = (l[s] as f64, r[s] as f64);
        if ping { db.l[st.dw] = ((xl + xr) * 0.5 + fb * st.dlp[1]) as f32; db.r[st.dw] = (fb * st.dlp[0]) as f32; }
        else { db.l[st.dw] = (xl + fb * st.dlp[0]) as f32; db.r[st.dw] = (xr + fb * st.dlp[1]) as f32; }
        l[s] = (xl + dl * mix) as f32; r[s] = (xr + dr * mix) as f32;
        st.dw = (st.dw + 1) % dlen;
    }
}
fn fx_eq(v: &[f64; NP], st: &mut FxState, l: &mut Buf, r: &mut Buf, fs: f64) {
    let (lo, hi) = (v[EQ_LOW], v[EQ_HIGH]);
    let key = ((lo * 100.).round() as i64, (hi * 100.).round() as i64);
    if key != st.eq_key { st.eq_key = key; st.eqc = [shelf(200., lo, fs, false), shelf(4000., hi, fs, true)]; }
    let [z0, z1, z2, z3] = &mut st.eq;
    for s in 0..l.len() {
        l[s] = biquad(&st.eqc[1], z2, biquad(&st.eqc[0], z0, l[s] as f64)) as f32;
        r[s] = biquad(&st.eqc[1], z3, biquad(&st.eqc[0], z1, r[s] as f64)) as f32;
    }
}
fn fx_comp(v: &[f64; NP], st: &mut FxState, l: &mut Buf, r: &mut Buf, fs: f64) {
    let th = v[COMP_THRESH]; let ra = v[COMP_RATIO]; let mk = 10f64.powf(v[COMP_GAIN] / 20.);
    let at = (-1. / (0.005 * fs)).exp(); let re = (-1. / (0.12 * fs)).exp();
    let mut e = st.env;
    for s in 0..l.len() {
        let x = (l[s].abs().max(r[s].abs())) as f64;
        e = if x > e { x + (e - x) * at } else { x + (e - x) * re };
        let db = 20. * (e + 1e-9).log10(); let over = db - th;
        let g = (if over > 0. { 10f64.powf(-over * (1. - 1. / ra) / 20.) } else { 1. }) * mk;
        l[s] = (l[s] as f64 * g) as f32; r[s] = (r[s] as f64 * g) as f32;
    }
    st.env = e;
}
/// Hyper: up to 7 micro-delay voices drifting sharp and flat; Dimension: cross-fed short delays for width
fn fx_hyper(v: &[f64; NP], st: &mut FxState, l: &mut Buf, r: &mut Buf, fs: f64) {
    let nv = (crate::params::js_round(v[HYPER_VOICES]) as usize).max(1).min(7);
    let rate = v[HYPER_RATE]; let dep = v[HYPER_DETUNE] * 0.004 * fs; let mix = v[HYPER_MIX]; let dim = v[HYPER_DIM]; const M: i64 = 4095;
    let g = 1. / (nv as f64).sqrt();
    for s in 0..l.len() {
        let (xl, xr) = (l[s] as f64, r[s] as f64);
        st.hb[0][(st.hw & M) as usize] = xl as f32; st.hb[1][(st.hw & M) as usize] = xr as f32;
        let (mut wl, mut wr) = (0., 0.);
        for j in 0..nv {
            st.hph[j] = (st.hph[j] + rate * (1. + j as f64 * 0.13) / fs) % 1.;
            let d = (0.006 + 0.0025 * j as f64) * fs + dep * (0.5 + 0.5 * (TAU * (st.hph[j] + j as f64 / nv as f64)).sin());
            let c = j & 1; let p = st.hw as f64 - d; let i = p.floor() as i64; let f = p - i as f64; let bb = &st.hb[c];
            let (b0, b1) = (bb[(i & M) as usize] as f64, bb[((i + 1) & M) as usize] as f64);
            let w = b0 + (b1 - b0) * f;
            if c == 1 { wr += w } else { wl += w }
        }
        // wet voices added over the dry, not crossfaded: a short-delay crossfade would comb-cancel the dry
        let mut ol = xl * (1. - mix * 0.3) + wl * g * mix; let mut or = xr * (1. - mix * 0.3) + wr * g * mix;
        if dim > 0. {
            let il = (st.hw as f64 - (0.011 + 0.012 * dim) * fs).floor() as i64; let ir = (st.hw as f64 - (0.017 + 0.015 * dim) * fs).floor() as i64;
            ol += dim * 0.45 * st.hb[1][(il & M) as usize] as f64; or += dim * 0.45 * st.hb[0][(ir & M) as usize] as f64;
        }
        l[s] = ol as f32; r[s] = or as f32; st.hw += 1;
    }
}
/// Flanger: a 0.3-6 ms delay swept by an LFO, fed back
fn fx_flanger(v: &[f64; NP], st: &mut FxState, l: &mut Buf, r: &mut Buf, fs: f64) {
    let rate = v[FLANGER_RATE]; let dep = v[FLANGER_DEPTH]; let fb = v[FLANGER_FB]; let mix = v[FLANGER_MIX]; let sp = v[FLANGER_SPREAD]; const M: i64 = 1023;
    for s in 0..l.len() {
        st.fph = (st.fph + rate / fs) % 1.;
        for c in 0..2 {
            let x = if c == 1 { r[s] } else { l[s] } as f64;
            let d = (0.0003 + 0.0057 * dep * (0.5 + 0.5 * (TAU * (st.fph + c as f64 * sp * 0.5)).sin())) * fs;
            let p = st.fw as f64 - d; let i = p.floor() as i64; let f = p - i as f64; let bb = &mut st.fbuf[c];
            let (b0, b1) = (bb[(i & M) as usize] as f64, bb[((i + 1) & M) as usize] as f64);
            let w = b0 + (b1 - b0) * f;
            bb[(st.fw & M) as usize] = (x + fb * w) as f32;
            let o = (x * (1. - mix * 0.5) + w * mix) as f32;
            if c == 1 { r[s] = o } else { l[s] = o }
        }
        st.fw += 1;
    }
}
/// Phaser: six allpass stages per side swept around a centre frequency, with feedback and a stereo phase offset
fn fx_phaser(v: &[f64; NP], st: &mut FxState, l: &mut Buf, r: &mut Buf, fs: f64) {
    let rate = v[PHASER_RATE]; let dep = v[PHASER_DEPTH]; let f0 = v[PHASER_FREQ]; let fb = v[PHASER_FB]; let mix = v[PHASER_MIX]; let sp = v[PHASER_SPREAD];
    for s in 0..l.len() {
        st.pph = (st.pph + rate / fs) % 1.;
        if s & 7 == 0 {
            for c in 0..2 {
                let fc = (0.45 * fs).min(f0 * 2f64.powf(dep * 3. * (TAU * (st.pph + c as f64 * sp * 0.5)).sin()));
                let t = (PI * fc / fs).tan(); st.pa[c] = (t - 1.) / (t + 1.);
            }
        }
        for c in 0..2 {
            let x = if c == 1 { r[s] } else { l[s] } as f64;
            let a = st.pa[c];
            let mut y = x + fb * st.pz[c];
            for k in 0..6 { let z = c * 6 + k; let o = a * y + st.ap[z]; st.ap[z] = y - a * o; y = o; }
            st.pz[c] = y;
            let o = (x * (1. - mix * 0.5) + y * mix) as f32;
            if c == 1 { r[s] = o } else { l[s] = o }
        }
    }
}
