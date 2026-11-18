//! The reverb (verb.js): a VintageVerb-style algorithmic reverb, for the room, for each synth's FX rack and for reverb
//! fields. After Sean Costello's Valhalla VintageVerb, in spirit: a predelay, an input low and high cut, an early
//! diffuser of allpasses whose blend sets the echo density, a swell of two long allpasses for the Attack, a tapped
//! early-reflection line, and a late ring of 4 or 6 stages (modulated allpass, allpass, delay, three-band decay) read by
//! Dattorro-style output taps. Each mode is a row of lengths, taps and a modulation recipe; each color is an era:
//! 1970s runs the tank at half the sample rate, band-limited, with noisy modulation; 1980s is full band with the noisy
//! modulation; now is full band and clean. All memory is allocated in `Verb::new`.

use crate::Rng;
use std::f64::consts::{PI, SQRT_2};

pub const VERB_MODES: [&str; 20] = ["concert hall", "bright hall", "plate", "room", "chamber", "random space", "chorus space", "ambience", "sanctuary",
    "dirty hall", "dirty plate", "smooth plate", "smooth room", "smooth random", "nonlin", "chaotic hall", "chaotic chamber", "chaotic neutral", "cathedral", "palace"];
pub const VERB_COLORS: [&str; 3] = ["1970s", "1980s", "now"];
/// [key, label, min, max, step]. Mode and color are indices into VERB_MODES and VERB_COLORS.
pub const VERB_DEFS: [(&str, &str, f64, f64, f64); 22] = [("mode", "mode", 0., 19., 1.), ("color", "color", 0., 2., 1.), ("wet", "wet", 0., 1.5, 0.001),
    ("predelay", "predelay ms", 0., 500., 1.), ("decay", "decay s", 0.2, 60., 0.01), ("size", "size", 0., 1., 0.01), ("attack", "attack", 0., 1., 0.01),
    ("early", "early diffusion", 0., 1., 0.01), ("late", "late diffusion", 0., 1., 0.01), ("modRate", "mod rate Hz", 0.05, 10., 0.01), ("modDepth", "mod depth", 0., 1., 0.01),
    ("highCut", "high cut Hz", 1000., 20000., 10.), ("lowCut", "low cut Hz", 10., 1000., 1.), ("bassMult", "bass mult", 0.25, 4., 0.01), ("bassXover", "bass xover Hz", 50., 2000., 1.),
    ("highShelf", "high shelf dB", -24., 0., 0.1), ("highFreq", "high freq Hz", 500., 16000., 10.), ("earlyLate", "early/late", 0., 1., 0.01), ("width", "width", 0., 1.5, 0.01),
    ("shimmer", "shimmer", 0., 1., 0.01), ("shimmerPitch", "shimmer st", -12., 24., 1.), ("freeze", "freeze", 0., 1., 1.)];
pub const VERB_N: usize = 22;
pub const V_MODE: usize = 0; pub const V_COLOR: usize = 1; pub const V_WET: usize = 2; pub const V_PREDELAY: usize = 3; pub const V_DECAY: usize = 4;
pub const V_SIZE: usize = 5; pub const V_ATTACK: usize = 6; pub const V_EARLY: usize = 7; pub const V_LATE: usize = 8; pub const V_MODRATE: usize = 9;
pub const V_MODDEPTH: usize = 10; pub const V_HIGHCUT: usize = 11; pub const V_LOWCUT: usize = 12; pub const V_BASSMULT: usize = 13; pub const V_BASSXOVER: usize = 14;
pub const V_HIGHSHELF: usize = 15; pub const V_HIGHFREQ: usize = 16; pub const V_EARLYLATE: usize = 17; pub const V_WIDTH: usize = 18; pub const V_SHIMMER: usize = 19;
pub const V_SHIMMERPITCH: usize = 20; pub const V_FREEZE: usize = 21;
pub const VERB_DEFAULT: [f64; VERB_N] = [0., 1., 0.3, 4., 2.2, 0.6, 0.2, 0.75, 0.7, 0.6, 0.5, 9000., 80., 1.2, 400., -4., 5000., 0.6, 1., 0., 12., 0.];

/// A reverb's knobs, by VERB_DEFS index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VerbParams(pub [f64; VERB_N]);
impl Default for VerbParams { fn default() -> Self { VerbParams(VERB_DEFAULT) } }
impl VerbParams {
    pub fn index(key: &str) -> Option<usize> { VERB_DEFS.iter().position(|d| d.0 == key) }
    pub fn get(&self, key: &str) -> Option<f64> { Self::index(key).map(|i| self.0[i]) }
    /// Set by key, clamped to its range (as a save is read). Unknown keys are ignored.
    pub fn set(&mut self, key: &str, x: f64) { if let Some(i) = Self::index(key) { if x.is_finite() { self.0[i] = x.clamp(VERB_DEFS[i].2, VERB_DEFS[i].3); } } }
    /// From a JSON object (a save's `space`, a preset): known keys over the defaults.
    pub fn from_json(j: &serde_json::Value) -> Self {
        let mut p = Self::default();
        if let Some(o) = j.as_object() { for (k, v) in o { p.set(k, crate::patch::js_num(v)); } }
        p
    }
}
/// Musical starting points: each sets a mode and color and the knobs that make it sit.
pub fn verb_preset(name: &str) -> Option<VerbParams> {
    // mode,color,predelay,decay,size,attack,early,late,modRate,modDepth,highCut,lowCut,bassMult,bassXover,highShelf,highFreq,earlyLate
    const P: [(&str, [f64; 17]); 15] = [
        ("big hall", [0., 1., 18., 3.4, 0.8, 0.35, 0.8, 0.75, 0.5, 0.55, 8000., 90., 1.1, 350., -6., 4500., 0.7]),
        ("bright hall", [1., 2., 12., 2.6, 0.7, 0.25, 0.8, 0.7, 0.8, 0.6, 14000., 120., 1., 400., -2., 7000., 0.65]),
        ("vocal plate", [2., 1., 24., 2., 0.55, 0.1, 0.85, 0.7, 0.7, 0.45, 11000., 180., 0.9, 500., -3., 6000., 0.8]),
        ("drum room", [3., 2., 0., 0.7, 0.35, 0., 0.9, 0.6, 0.4, 0.3, 10000., 100., 0.9, 300., -6., 4000., 0.45]),
        ("tight chamber", [4., 1., 6., 1.2, 0.4, 0.1, 0.9, 0.75, 0.6, 0.4, 9000., 90., 1., 400., -4., 5000., 0.55]),
        ("deep space", [5., 2., 30., 7., 0.9, 0.6, 0.8, 0.8, 0.3, 0.7, 7000., 70., 1.3, 300., -8., 3500., 0.85]),
        ("chorus space", [6., 2., 20., 5., 0.85, 0.5, 0.75, 0.75, 0.9, 0.75, 9000., 80., 1.2, 350., -6., 4500., 0.8]),
        ("ambience", [7., 2., 0., 0.5, 0.5, 0., 0.9, 0.6, 0.5, 0.3, 12000., 120., 0.8, 400., -3., 6000., 0.35]),
        ("sanctuary", [8., 2., 40., 9., 0.9, 0.7, 0.75, 0.85, 0.35, 0.6, 6500., 60., 1.4, 300., -9., 3000., 0.9]),
        ("smooth plate", [11., 2., 10., 2.4, 0.6, 0.15, 0.85, 0.8, 0.6, 0.5, 12000., 150., 1., 450., -3., 6000., 0.75]),
        ("smooth room", [12., 2., 4., 1., 0.45, 0.05, 0.9, 0.75, 0.5, 0.4, 10000., 100., 1., 400., -4., 5000., 0.5]),
        ("gated", [14., 1., 0., 1., 0.35, 0.5, 1., 0.7, 0.5, 0.2, 9000., 120., 1., 400., -3., 5000., 1.]),
        ("reverse", [14., 2., 0., 1., 0.6, 1., 1., 0.7, 0.5, 0.3, 9000., 120., 1., 400., -3., 5000., 1.]),
        ("tape hall", [15., 0., 20., 3., 0.75, 0.3, 0.8, 0.7, 0.7, 0.6, 8000., 90., 1.2, 350., -6., 4000., 0.7]),
        ("cathedral", [18., 2., 35., 12., 1., 0.6, 0.8, 0.85, 0.3, 0.5, 6000., 60., 1.5, 250., -10., 3000., 0.9]),
    ];
    const K: [usize; 17] = [V_MODE, V_COLOR, V_PREDELAY, V_DECAY, V_SIZE, V_ATTACK, V_EARLY, V_LATE, V_MODRATE, V_MODDEPTH, V_HIGHCUT, V_LOWCUT,
        V_BASSMULT, V_BASSXOVER, V_HIGHSHELF, V_HIGHFREQ, V_EARLYLATE];
    let (_, v) = P.iter().find(|p| p.0 == name)?;
    let mut p = VerbParams::default();
    for (i, k) in K.iter().enumerate() { p.0[*k] = v[i]; }
    Some(p)
}
pub const VERB_PRESET_NAMES: [&str; 15] = ["big hall", "bright hall", "vocal plate", "drum room", "tight chamber", "deep space", "chorus space", "ambience",
    "sanctuary", "smooth plate", "smooth room", "gated", "reverse", "tape hall", "cathedral"];

/// A mode. Lengths are ms at size 1 (size 0 is smin of that); d: ring delays, a1: the modulated allpasses, a2: the
/// second allpasses, ind: the early diffuser, er: [taps per side, span ms, level], md: weights of [sine chorus, random
/// walk on the allpasses, random delay, chaos], depth/dd: chorus and delay-randomisation depth, hi: the treble decay
/// against the decay knob, sat: saturation in the loop, att: built-in attack, diff: the ceiling of late diffusion.
#[derive(Clone, Copy)]
struct Algo { k: usize, d: [f64; 6], a1: [f64; 6], a2: [f64; 6], ind: [f64; 4], er: [f64; 3], md: [f64; 4], depth: f64, dd: f64, hi: f64, sat: f64,
    smin: f64, att: f64, diff: f64, gain: f64, nl: bool }
fn algos() -> &'static [Algo; 20] {
    static A: std::sync::OnceLock<[Algo; 20]> = std::sync::OnceLock::new();
    A.get_or_init(|| {
        let hall = Algo { k: 4, d: [137., 113., 127., 101., 0., 0.], a1: [29.3, 23.1, 26.7, 19.9, 0., 0.], a2: [47.3, 39.1, 53.7, 31.9, 0., 0.], ind: [4.77, 3.59, 12.73, 9.31],
            er: [12., 70., 0.5], md: [1., 0., 0., 0.], depth: 1., dd: 0., hi: 0.75, sat: 0., smin: 0.3, att: 0., diff: 0.7, gain: 1., nl: false };
        let plate = Algo { d: [149.6, 125., 141.7, 106.3, 0., 0.], a1: [22.6, 30.5, 24.9, 28.1, 0., 0.], a2: [60.5, 89.2, 53.3, 71.9, 0., 0.], er: [12., 30., 0.6],
            depth: 0.8, hi: 1., smin: 0.35, ..hall };
        let room = Algo { d: [53., 61., 47., 67., 0., 0.], a1: [11.3, 8.9, 13.1, 7.7, 0., 0.], a2: [17.3, 23.9, 13.7, 26.7, 0., 0.], ind: [3.1, 2.3, 5.7, 4.1], er: [14., 55., 1.],
            md: [0., 1., 0., 0.], depth: 0.6, hi: 0.6, smin: 0.25, diff: 0.6, ..hall };
        let chamber = Algo { d: [73., 89., 67., 81., 0., 0.], a1: [17.3, 13.9, 19.1, 11.3, 0., 0.], a2: [27.1, 33.9, 23.3, 29.7, 0., 0.], ind: [4.1, 3.3, 7.9, 6.1], er: [12., 35., 0.6],
            md: [1., 0.3, 0., 0.], depth: 0.8, hi: 0.85, ..hall };
        let space = Algo { k: 6, d: [131., 157., 113., 149., 121., 163.], a1: [29.1, 23.3, 37.7, 19.7, 31.3, 26.9], a2: [43.1, 59.3, 37.9, 51.7, 41.9, 62.3], ind: [5.3, 4.1, 14.3, 10.9],
            er: [16., 80., 0.6], md: [0., 0., 1., 0.], depth: 0.5, dd: 1., att: 0.25, ..hall };
        let scale = |o: Algo, f: f64, fa: f64| Algo { d: o.d.map(|x| x * f), a1: o.a1.map(|x| x * fa), ..o };
        let ambience = Algo { k: 4, d: [23., 29., 19., 31., 0., 0.], a1: [7.1, 5.3, 6.7, 4.3, 0., 0.], a2: [7.9, 11.3, 6.7, 9.1, 0., 0.], ind: [1.3, 1.9, 2.9, 3.7], er: [16., 28., 1.2],
            md: [1., 0., 0., 0.], depth: 0.4, dd: 0., hi: 0.8, sat: 0., smin: 0.3, att: 0., diff: 0.65, gain: 1., nl: false };
        let trim = [1., 1., 1.06, 0.8, 0.88, 1.5, 1.48, 0.62, 1.57, 1.03, 1.09, 1.05, 0.81, 1.45, 1.68, 1.01, 0.88, 1.06, 1.66, 1.67];
        let mut a = [hall,
            Algo { hi: 1., depth: 1.6, ..hall },
            plate, room, chamber, space,
            Algo { md: [1., 0., 0., 0.], depth: 1.2, dd: 0., ..space },
            ambience,
            Algo { md: [1., 0.3, 0.5, 0.], depth: 1.3, dd: 0.5, hi: 0.55, att: 0.45, smin: 0.35, er: [14., 100., 0.45], ..scale(space, 1.25, 1.) },
            Algo { md: [0., 1., 0., 0.], depth: 1.2, sat: 0.6, diff: 0.5, hi: 0.7, ..hall },
            Algo { md: [0., 0.4, 0., 0.], depth: 0.3, sat: 0.5, diff: 0.35, ..plate },
            Algo { md: [1., 0., 0.6, 0.], depth: 0.8, dd: 0.6, diff: 0.75, ind: [5.9, 4.3, 15.1, 11.3], ..plate },
            Algo { md: [1., 0., 0.6, 0.], dd: 0.6, diff: 0.7, er: [14., 55., 0.6], ..room },
            Algo { md: [0.5, 0., 1., 0.], diff: 0.75, ..space },
            Algo { nl: true, ind: [2.3, 3.7, 5.9, 8.9], er: [32., 500., 1.], smin: 0.12, ..room },
            Algo { md: [0., 0., 0., 1.], depth: 1., dd: 0.5, sat: 0.3, ..hall },
            Algo { md: [0., 0., 0., 1.], depth: 1., dd: 0.4, sat: 0.2, ..chamber },
            Algo { md: [0., 0., 0., 1.], depth: 0.8, dd: 0.6, hi: 1., diff: 0.75, ..plate },
            Algo { md: [1., 0., 0.4, 0.], depth: 1., dd: 0.4, hi: 0.6, att: 0.4, smin: 0.4, er: [14., 120., 0.45], ..scale(space, 1.6, 1.3) },
            Algo { md: [1., 0., 0.3, 0.], depth: 1.4, dd: 0.3, hi: 0.9, att: 0.35, smin: 0.4, er: [14., 140., 0.45], ..scale(space, 2., 1.) },
        ];
        for (i, x) in a.iter_mut().enumerate() { x.gain = trim[i]; }
        a
    })
}
const SWELL: [f64; 2] = [19.3, 31.7];
const IND0: [f64; 2] = [1.13, 1.71];
fn p2(n: usize) -> usize { let mut p = 1; while p < n { p <<= 1; } p }
fn js_round(x: f64) -> f64 { (x + 0.5).floor() }
/// A seeded generator, so a mode's taps are the same every time.
struct VRng(u64);
impl VRng { fn next(&mut self) -> f64 { self.0 = (self.0.wrapping_mul(1664525).wrapping_add(1013904223)) & 0xffff_ffff; self.0 as f64 / 4294967296. } }

const S: usize = 6;
pub struct Verb {
    fs: f64,
    off: Vec<i64>, mask: Vec<i64>,
    l_pre: [usize; 2], l_ind: [usize; 12], l_sw: [usize; 4], l_er: [usize; 2], l_a1: [usize; 6], l_a2: [usize; 6], l_d: [usize; 6],
    mem: Vec<f32>,
    sh_size: usize, sh_buf: Vec<f32>,
    tin_l: Vec<f64>, tin_r: Vec<f64>, tout_l: Vec<f64>, tout_r: Vec<f64>,
    d_len: [f64; S], a1_len: [f64; S], a2_len: [f64; S], gl: [f64; S], gm: [f64; S], gh: [f64; S], lo: [f64; S], hp: [f64; S], o: [f64; S],
    mc: [f64; S], ms: [f64; S], rc: [f64; S], rs: [f64; S], r1: [f64; S], r2: [f64; S], tg: [f64; S], cnt: [f64; S], chaos: [f64; S],
    inj_l: [f64; S], inj_r: [f64; S], m_a: [f64; S], inj_k: [i64; S],
    e_o: [i64; 128], e_m: [i64; 128], e_d: [i64; 128], e_ga: [f64; 128], e_u: [f64; 128],
    t_o: [i64; 64], t_m: [i64; 64], t_d: [i64; 64], t_ga: [f64; 64], t_f: [f64; 64], t_line: [u8; 64], t_stage: [usize; 64],
    ind_d: [i64; 12], sw_d: [f64; 4], g_in: [f64; 6],
    pub target: [f64; VERB_N], pub cur: [f64; VERB_N],
    /// mean power of the last block's output, for idling
    pub energy: f64,
    /// blocks of silence the tail outlives its input by
    pub hold_blocks: usize,
    /// a mode or color change waits for the next block
    pub pending: bool,
    moving: bool,
    glide: f64, size_glide: f64, pre_glide: f64, glide_n: usize,
    w: i64, pw: i64, ph: u8, hold: [f64; 2], sh_w: i64, sh_p: f64, sh_lp: f64, sh_out: f64, dn: f64,
    fz: [f64; 6], bq: [[f64; 4]; 4], bqc: [f64; 5], in_gain: f64, fade: f64,
    algo: Algo, kk: usize, decim: usize, fs_t: f64, nl: bool, n_e: usize, n_t: usize, tap_norm: f64,
    blend: f64, att: f64, g_sw: f64, e_g: f64, l_g: f64, k_s: f64, k_r: f64, k_d: f64, chaotic: bool, d_a: f64, d_d: f64, rate: f64, ra: f64,
    g_l1: f64, g_l2: f64, a_lo: f64, a_hi: f64, sat: f64, a_lc: f64, a_hc: f64, pre_len: f64, shim: f64, sh_ratio: f64, sh_win: f64, sh_a: f64,
    rng: Rng,
}

impl Verb {
    pub fn new(fs: f64, params: &VerbParams) -> Self {
        let mut off = Vec::new(); let mut mask = Vec::new(); let mut total = 0usize;
        let mut alloc = |ms: f64| { let l = p2((ms * fs / 1000.).ceil() as usize + 8); off.push(total as i64); mask.push(l as i64 - 1); total += l; off.len() - 1 };
        let al = algos();
        let most = |f: &dyn Fn(&Algo) -> f64| al.iter().map(f).fold(f64::MIN, f64::max);
        let l_pre = [alloc(501.), alloc(501.)];
        let mut l_ind = [0; 12];
        for c in 0..2 { for j in 0..6 { l_ind[c * 6 + j] = alloc(if j < 2 { IND0[j] * 1.12 } else { most(&|a| a.ind[j - 2]) * 1.12 }); } }
        let mut l_sw = [0; 4];
        for c in 0..2 { for j in 0..2 { l_sw[c * 2 + j] = alloc(SWELL[j] * 1.12); } }
        let er = most(&|a| a.er[1]) * 1.02 + 2.;
        let l_er = [alloc(er), alloc(er)];
        let (mut l_a1, mut l_a2, mut l_d) = ([0; 6], [0; 6], [0; 6]);
        for i in 0..6 { l_a1[i] = alloc(most(&|a| a.a1[i]) + 3.5); l_a2[i] = alloc(most(&|a| a.a2[i]) + 1.); l_d[i] = alloc(most(&|a| a.d[i]) + 9.); }
        let sh_size = p2((fs * 0.2) as usize);
        const B: f64 = 128.;
        let mut v = Verb {
            fs, off, mask, l_pre, l_ind, l_sw, l_er, l_a1, l_a2, l_d, mem: vec![0.; total], sh_size, sh_buf: vec![0.; sh_size],
            tin_l: vec![0.; 128], tin_r: vec![0.; 128], tout_l: vec![0.; 128], tout_r: vec![0.; 128],
            d_len: [0.; S], a1_len: [0.; S], a2_len: [0.; S], gl: [0.; S], gm: [0.; S], gh: [0.; S], lo: [0.; S], hp: [0.; S], o: [0.; S],
            mc: [0.; S], ms: [0.; S], rc: [0.; S], rs: [0.; S], r1: [0.; S], r2: [0.; S], tg: [0.; S], cnt: [0.; S], chaos: [0.; S],
            inj_l: [0.; S], inj_r: [0.; S], m_a: [0.; S], inj_k: [0; S],
            e_o: [0; 128], e_m: [0; 128], e_d: [0; 128], e_ga: [0.; 128], e_u: [0.; 128],
            t_o: [0; 64], t_m: [0; 64], t_d: [0; 64], t_ga: [0.; 64], t_f: [0.; 64], t_line: [0; 64], t_stage: [0; 64],
            ind_d: [0; 12], sw_d: [0.; 4], g_in: [0.; 6], target: VERB_DEFAULT, cur: VERB_DEFAULT, energy: 0., hold_blocks: 8, pending: false, moving: false,
            glide: 1. - (-B / (fs * 0.05)).exp(), size_glide: 1. - (-B / (fs * 0.35)).exp(), pre_glide: 1. - (-B / (fs * 0.12)).exp(), glide_n: 128,
            w: 0x100000, pw: 0, ph: 0, hold: [0.; 2], sh_w: 0, sh_p: 0., sh_lp: 0., sh_out: 0., dn: 1e-18, fz: [0.; 6], bq: [[0.; 4]; 4], bqc: [0.; 5],
            in_gain: 1., fade: 1., algo: al[0], kk: 4, decim: 1, fs_t: fs, nl: false, n_e: 0, n_t: 0, tap_norm: 0.,
            blend: 0., att: 0., g_sw: 0., e_g: 0., l_g: 0., k_s: 0., k_r: 0., k_d: 0., chaotic: false, d_a: 0., d_d: 0., rate: 1., ra: 0.,
            g_l1: 0., g_l2: 0., a_lo: 0., a_hi: 0., sat: 0., a_lc: 0., a_hc: 0., pre_len: 0., shim: 0., sh_ratio: 1., sh_win: 1., sh_a: 0.,
            rng: Rng::new(0x9e3779b97f4a7c15 ^ fs as u64),
        };
        v.clear(params);
        v
    }
    /// Silence everything and take params at once, without a glide.
    pub fn clear(&mut self, params: &VerbParams) {
        self.mem.fill(0.); self.sh_buf.fill(0.);
        self.target = params.0;
        for (i, d) in VERB_DEFS.iter().enumerate() { if !self.target[i].is_finite() { self.target[i] = VERB_DEFAULT[i]; } let _ = d; }
        self.target[V_MODE] = js_round(self.target[V_MODE].clamp(0., 19.));
        self.target[V_COLOR] = js_round(self.target[V_COLOR].clamp(0., 2.));
        self.cur = self.target;
        self.w = 0x100000; self.pw = 0; self.ph = 0; self.hold = [0.; 2]; self.sh_w = 0; self.sh_p = 0.; self.sh_lp = 0.; self.sh_out = 0.; self.dn = 1e-18; self.energy = 0.;
        self.fz = [0.; 6]; self.bq = [[0.; 4]; 4];
        self.in_gain = if self.cur[V_FREEZE] >= 0.5 { 0. } else { 1. }; self.fade = 1.; self.pending = false;
        self.lo = [0.; S]; self.hp = [0.; S]; self.o = [0.; S]; self.r1 = [0.; S]; self.r2 = [0.; S]; self.tg = [0.; S]; self.m_a = [0.; S];
        for i in 0..S { let ph = i as f64 * 2.399; self.mc[i] = ph.cos(); self.ms[i] = ph.sin(); self.chaos[i] = 0.13 + 0.11 * i as f64; self.cnt[i] = 0.; }
        self.build(); self.retune(true);
    }
    /// New params; continuous ones glide, mode and color crossfade through silence.
    pub fn set(&mut self, p: &VerbParams) { for i in 0..VERB_N { self.set_one(i, p.0[i]); } }
    pub fn set_one(&mut self, k: usize, x: f64) {
        if !x.is_finite() || k >= VERB_N { return; }
        if k == V_MODE || k == V_COLOR {
            let r = js_round(x.clamp(0., if k == V_MODE { 19. } else { 2. }));
            if r != self.target[k] { self.target[k] = r; self.pending = true; }
        } else if self.target[k] != x { self.target[k] = x; self.moving = true; }
    }
    /// The structure of the current mode and color: lengths, taps and how the input enters the ring.
    fn build(&mut self) {
        let c = self.cur;
        let a = algos()[c[V_MODE] as usize];
        self.algo = a;
        let k = if a.nl { 0 } else { a.k };
        self.kk = k;
        let mut rnd = VRng(7919 * (c[V_MODE] as u64 + 1));
        self.decim = if c[V_COLOR] == 0. { 2 } else { 1 };
        self.fs_t = self.fs / self.decim as f64; self.nl = a.nl;
        // the 1970s band: 2-pole lowpasses before the decimation and after the hold
        let fc = 9500f64.min(self.fs * 0.2); let w = 2. * PI * fc / self.fs; let al = w.sin() / (2. * std::f64::consts::FRAC_1_SQRT_2); let cs = w.cos(); let a0 = 1. + al;
        self.bqc = [(1. - cs) / 2. / a0, (1. - cs) / a0, (1. - cs) / 2. / a0, -2. * cs / a0, (1. - al) / a0];
        // early taps: per side, times bunched later as a room's reflections thicken, gains falling, a third crossing over
        let n = a.er[0] as usize; let e = 2 * n; self.n_e = e;
        for j in 0..e {
            let side = if j < n { 0 } else { 1 }; let kk = j % n;
            let u = ((kk as f64 + 0.5 + 0.8 * (rnd.next() - 0.5)) / n as f64).powf(if a.nl { 1. } else { 0.75 });
            let src = if rnd.next() < (if a.nl { 0.5 } else { 0.3 }) { 1 - side } else { side };
            let sgn = if rnd.next() < 0.5 { -1. } else { 1. };
            self.e_u[j] = u.clamp(0.002, 1.); self.e_o[j] = self.off[self.l_er[src]]; self.e_m[j] = self.mask[self.l_er[src]]; self.e_ga[j] = sgn;
        }
        // late taps: two in each stage's delay (one near its head, so the ring speaks at once) and one in its second
        // allpass, per side, with alternating signs
        let tq = if k > 4 { 2 } else { 3 };
        let mut t = 0;
        for side in 0..2 { for i in 0..k { for q in (3 - tq)..3 {
            let sgn = if (i + q + side) & 1 == 1 { -1. } else { 1. };
            let line = if q < 2 { self.l_d[i] } else { self.l_a2[i] };
            self.t_line[t] = if q < 2 { 0 } else { 1 }; self.t_o[t] = self.off[line]; self.t_m[t] = self.mask[line];
            self.t_f[t] = if q == 0 { 0.02 + 0.16 * rnd.next() } else if q == 1 { if tq < 3 { 0.02 + 0.4 * rnd.next() } else { 0.3 + 0.65 * rnd.next() } } else { 0.05 + 0.45 * rnd.next() };
            self.t_stage[t] = i; self.t_ga[t] = sgn; t += 1;
        } } }
        self.n_t = t;
        self.tap_norm = if k > 0 { 1.6 / (t as f64 / 2.).sqrt() } else { 0. };
    }
    fn retune(&mut self, snap: bool) {
        if snap || self.moving {
            let mut moved = false;
            for k in 2..VERB_N {
                let t = self.target[k];
                if snap || k == V_FREEZE || k == V_SHIMMERPITCH { self.cur[k] = t; } else {
                    let g = if k == V_SIZE { self.size_glide } else if k == V_PREDELAY { self.pre_glide } else { self.glide };
                    self.cur[k] += (t - self.cur[k]) * g;
                    if (t - self.cur[k]).abs() <= 1e-6 * (1. + t.abs()) { self.cur[k] = t; }
                }
                if self.cur[k] != t { moved = true; }
            }
            self.moving = moved;
        } else { return; }
        let a = self.algo; let k = self.kk; let fs = self.fs; let fs_t = self.fs_t; let ms = fs_t / 1000.; let c = self.cur; let tau = 2. * PI;
        let sc = a.smin + (1. - a.smin) * c[V_SIZE].clamp(0., 1.);
        let frozen = c[V_FREEZE] >= 0.5; let tt = c[V_DECAY].max(0.05); let tl = tt * c[V_BASSMULT].max(0.1);
        let th = (tt * 1.2).min(tt * a.hi * 10f64.powf(c[V_HIGHSHELF].min(0.) / 20.));
        // a short decay shrinks a big space: a stage that lost more than 5 dB a pass would come round again as a repeat
        let mut ring = 0.; let mut stage = 0f64;
        for i in 0..k { stage = stage.max(a.d[i] + a.a1[i] + a.a2[i]); }
        let sc_r = if frozen { sc } else { sc.min(tt * 1000. / 12. / stage) };
        let rd = |x: f64| if frozen { js_round(x) } else { x };
        for i in 0..k {
            let d = rd((a.d[i] * sc_r * ms).max(4.)); let a1 = rd((a.a1[i] * sc_r * ms).max(2.)); let a2 = rd((a.a2[i] * sc_r * ms).max(2.));
            self.d_len[i] = d; self.a1_len[i] = a1; self.a2_len[i] = a2; ring += d + a1 + a2;
            let sec = (d + a1 + a2) / fs_t;
            self.gm[i] = if frozen { 1. } else { 10f64.powf(-3. * sec / tt) }; self.gl[i] = if frozen { 1. } else { 10f64.powf(-3. * sec / tl) };
            self.gh[i] = if frozen { 1. } else { 10f64.powf(-3. * sec / th) };
        }
        for j in 0..self.n_t { let i = self.t_stage[j]; let l = if self.t_line[j] == 1 { self.a2_len[i] } else { self.d_len[i] }; self.t_d[j] = (js_round(self.t_f[j] * l) as i64).max(1); }
        // early: the diffuser (lengths fixed, so a size change never sweeps the onset), the swell and the reflection taps
        for side in 0..2 { for j in 0..6 {
            self.ind_d[side * 6 + j] = (js_round((if j < 2 { IND0[j] } else { a.ind[j - 2] }) * (if side == 1 { 1.11 } else { 1. }) * ms) as i64).max(1);
        } }
        let ge = 0.5 + 0.25 * c[V_EARLY].clamp(0., 1.);
        self.g_in = [0.6, 0.6, ge, ge, ge * 0.83, ge * 0.83];
        self.blend = (c[V_EARLY].max(0.) * 1.6).min(1.);
        let att = (c[V_ATTACK] + a.att * (1. - c[V_ATTACK])).clamp(0., 1.); self.att = att; self.g_sw = 0.7 * att;
        for side in 0..2 { for j in 0..2 { self.sw_d[side * 2 + j] = (SWELL[j] * (if side == 1 { 1.11 } else { 1. }) * sc * ms).max(1.); } }
        let span = a.er[1] * (if a.nl { 0.12 + 0.88 * c[V_SIZE].clamp(0., 1.) } else { sc }) * ms;
        let mut er_max = 0i64;
        let nl = self.nl; let shape = c[V_ATTACK] * 2. - 1.;
        // nonlin: attack bends the gate from a cut-off decay (0) through flat (.5) to a reverse swell (1)
        let env = |u: f64| if nl { (if shape < 0. { shape * 4. * u } else { -shape * 4. * (1. - u) }).exp() } else { (-2.3 * u).exp() };
        // the taps' energy is held level, so a gate of any shape and length sounds as loud
        let mut sum = 0.; for j in 0..self.n_e { sum += env(self.e_u[j]).powi(2); }
        let norm = if nl { 0.73 / (sum / 2.).sqrt() } else { 1. / (self.n_e as f64 / 2.).sqrt() };
        for j in 0..self.n_e {
            let d = (js_round(self.e_u[j] * span) as i64).max(1); self.e_d[j] = d; er_max = er_max.max(d);
            self.e_ga[j] = self.e_ga[j].signum() * env(self.e_u[j]) * norm;
        }
        // inject: the first stage takes the left, the middle stage the right; a fast attack also feeds the other stages
        for i in 0..k {
            let h = k >> 1; let spread = (1. - att) * 0.7; let s = if i & 2 != 0 { -1. } else { 1. }; let odd = i & 1 == 1;
            self.inj_l[i] = if i == 0 || i == h { 0. } else { spread * s * if odd { 0.35 } else { 0.9 } };
            self.inj_r[i] = if i == 0 || i == h { 0. } else { spread * s * (if odd { 0.9 } else { 0.35 }) * if i > h { -1. } else { 1. } };
            self.inj_k[i] = js_round(self.d_len[i] * (0.2 + 0.6 * ((i as f64 * 0.618034) % 1.))) as i64 - js_round(self.d_len[i]) as i64;
        }
        // early/late balance: each full at the middle
        let el = c[V_EARLYLATE].clamp(0., 1.);
        let cg = a.gain * if self.decim > 1 { 1.27 } else { 1. };
        self.e_g = ((2. * (1. - el)).min(1.) * a.er[2] * (1. - 0.75 * att) * (if nl { 0. } else { 1. }) + if nl { 1. } else { 0. }) * cg;
        self.l_g = if nl { 0. } else { (2. * el).min(1.) * self.tap_norm * cg };
        // modulation: [sine, random walk, random delay, chaos]; the old colors add the noisy walk
        let m = a.md; let dep = c[V_MODDEPTH].clamp(0., 1.); let noisy = if c[V_COLOR] < 2. { 0.5 } else { 0. };
        self.k_s = m[0]; self.k_r = m[1] + m[3] + noisy; self.k_d = (m[2] + m[3] * 0.5) * a.dd; self.chaotic = m[3] > 0.;
        self.d_a = if frozen { 0. } else { dep * a.depth * 0.0009 * fs_t }; self.d_d = if frozen { 0. } else { dep * 0.0025 * fs_t };
        for i in 0..k { self.d_a = self.d_a.min(self.a1_len[i] - 1.5); }
        let rate = c[V_MODRATE].max(0.01); self.rate = rate;
        for i in 0..k { let w = tau * rate * (0.8 + 0.4 * i as f64 / (k.max(2) - 1) as f64) / fs_t; self.rc[i] = w.cos(); self.rs[i] = w.sin(); }
        self.ra = 1. - (-tau * rate * (if self.chaotic { 3. } else { 1.2 }) / fs_t).exp();
        self.g_l1 = c[V_LATE].clamp(0., 1.) * a.diff; self.g_l2 = self.g_l1 * 0.72;
        self.a_lo = 1. - (-tau * c[V_BASSXOVER].min(fs_t * 0.45) / fs_t).exp(); self.a_hi = 1. - (-tau * c[V_HIGHFREQ].min(fs_t * 0.45) / fs_t).exp();
        self.sat = a.sat;
        self.a_lc = 1. - (-tau * c[V_LOWCUT].min(fs * 0.45) / fs).exp(); self.a_hc = 1. - (-tau * c[V_HIGHCUT].min(fs * 0.45) / fs).exp();
        self.pre_len = c[V_PREDELAY].clamp(0., 500.) * fs / 1000.;
        self.shim = if frozen { 0. } else { (c[V_SHIMMER].max(0.) * 0.6).min(0.6) }; self.sh_ratio = 2f64.powf(c[V_SHIMMERPITCH] / 12.); self.sh_win = js_round(fs_t * 0.07);
        self.sh_a = 1. - (-tau * 6000f64.min(fs_t * 0.4) / fs_t).exp();
        // the tail outlives its input by the predelay, the early taps, the swell and one pass of the ring
        self.hold_blocks = ((self.pre_len + self.decim as f64 * (er_max as f64 + ring + self.sw_d[2] + self.sw_d[3] + 40.)) / 128.).ceil() as usize + 2;
    }
    /// in: the dry block. Adds only the wet signal, scaled by wet, into out. n ≤ 128.
    pub fn process(&mut self, in_l: &[f32], in_r: &[f32], out_l: &mut [f32], out_r: &mut [f32], n: usize) {
        let switching = self.pending;
        // the glides are per block of 128; a shorter block glides that much less
        if n != self.glide_n { let b = n as f64; let fs = self.fs; self.glide_n = n;
            self.glide = 1. - (-b / (fs * 0.05)).exp(); self.size_glide = 1. - (-b / (fs * 0.35)).exp(); self.pre_glide = 1. - (-b / (fs * 0.12)).exp(); }
        self.retune(false);
        let c = self.cur; let decim = self.decim;
        // predelay, then the input's low cut and 2-pole high cut, at the full rate
        let (p_l, p_r, p_m) = (self.off[self.l_pre[0]], self.off[self.l_pre[1]], self.mask[self.l_pre[0]]);
        let pre = self.pre_len; let (a_lc, a_hc) = (self.a_lc, self.a_hc);
        let ph0 = self.ph; let mut pw = self.pw; let [mut lc_l, mut lc_r, mut h1_l, mut h1_r, mut h2_l, mut h2_r] = self.fz;
        let mut m = 0; let mut ph = self.ph;
        let ig = self.in_gain; let ig_t = if c[V_FREEZE] >= 0.5 { 0. } else { 1. }; let ig_s = (ig_t - ig) / n as f64; let mut g = ig;
        let [b0, b1, b2, a1, a2] = self.bqc;
        let mem = &mut self.mem;
        for s in 0..n {
            mem[(p_l + (pw & p_m)) as usize] = in_l[s]; mem[(p_r + (pw & p_m)) as usize] = in_r[s];
            let (mut xl, mut xr);
            if pre < 1. { xl = in_l[s] as f64; xr = in_r[s] as f64; } else {
                let pos = pw as f64 - pre; let i = pos.floor() as i64; let fr = pos - i as f64; let (i0, i1) = (i & p_m, (i + 1) & p_m);
                let (l0, l1, r0, r1) = (mem[(p_l + i0) as usize] as f64, mem[(p_l + i1) as usize] as f64, mem[(p_r + i0) as usize] as f64, mem[(p_r + i1) as usize] as f64);
                xl = l0 + (l1 - l0) * fr; xr = r0 + (r1 - r0) * fr;
            }
            pw = (pw + 1) & 0x3fffffff; g += ig_s;
            lc_l += a_lc * (xl - lc_l); lc_r += a_lc * (xr - lc_r); xl = (xl - lc_l) * g; xr = (xr - lc_r) * g;
            h1_l += a_hc * (xl - h1_l); h1_r += a_hc * (xr - h1_r); h2_l += a_hc * (h1_l - h2_l); h2_r += a_hc * (h1_r - h2_r); xl = h2_l; xr = h2_r;
            if decim == 1 { self.tin_l[s] = xl; self.tin_r[s] = xr; m += 1; } else {
                let zl = &mut self.bq[0];
                let yl = b0 * xl + b1 * zl[0] + b2 * zl[1] - a1 * zl[2] - a2 * zl[3]; zl[1] = zl[0]; zl[0] = xl; zl[3] = zl[2]; zl[2] = yl;
                let zr = &mut self.bq[1];
                let yr = b0 * xr + b1 * zr[0] + b2 * zr[1] - a1 * zr[2] - a2 * zr[3]; zr[1] = zr[0]; zr[0] = xr; zr[3] = zr[2]; zr[2] = yr;
                ph ^= 1; if ph == 1 { self.tin_l[m] = yl; self.tin_r[m] = yr; m += 1; }
            }
        }
        self.in_gain = ig_t; self.pw = pw; self.fz = [lc_l, lc_r, h1_l, h1_r, h2_l, h2_r];
        self.tank(m);
        // back to the full rate (the 1970s hold and lowpass), width, the switch fade, wet
        let wet = c[V_WET]; let wd = c[V_WIDTH];
        let mut e = 0.; let mut fade = self.fade; let f_step = if switching { -1. / n as f64 } else if fade < 1. { 1. / n as f64 } else { 0. };
        let mut t = 0; let (mut hl, mut hr) = (self.hold[0], self.hold[1]); ph = ph0;
        for s in 0..n {
            let (mut l, mut r);
            if decim == 1 { l = self.tout_l[s]; r = self.tout_r[s]; } else {
                ph ^= 1;
                if ph == 1 { let (nl, nr) = (self.tout_l[t], self.tout_r[t]); t += 1; l = 0.5 * (hl + nl); r = 0.5 * (hr + nr); hl = nl; hr = nr; } else { l = hl; r = hr; }
                let yl = &mut self.bq[2];
                let ol = b0 * l + b1 * yl[0] + b2 * yl[1] - a1 * yl[2] - a2 * yl[3]; yl[1] = yl[0]; yl[0] = l; yl[3] = yl[2]; yl[2] = ol;
                let yr = &mut self.bq[3];
                let or = b0 * r + b1 * yr[0] + b2 * yr[1] - a1 * yr[2] - a2 * yr[3]; yr[1] = yr[0]; yr[0] = r; yr[3] = yr[2]; yr[2] = or;
                l = ol; r = or;
            }
            let mid = (l + r) * 0.5; let sd = (l - r) * 0.5 * wd; l = mid + sd; r = mid - sd;
            e += l * l + r * r;
            fade = (fade + f_step).clamp(0., 1.); let k = wet * fade;
            out_l[s] += (l * k) as f32; out_r[s] += (r * k) as f32;
        }
        self.hold = [hl, hr]; self.ph = ph; self.fade = fade;
        self.energy = e / n as f64;
        // a new mode or color: the old tail has faded out over this block; rebuild in silence and fade in on the next
        if switching {
            self.pending = false; self.cur[V_MODE] = self.target[V_MODE]; self.cur[V_COLOR] = self.target[V_COLOR];
            let keep = self.target;
            self.mem.fill(0.); self.sh_buf.fill(0.);
            self.lo = [0.; S]; self.hp = [0.; S]; self.o = [0.; S]; self.r1 = [0.; S]; self.r2 = [0.; S]; self.m_a = [0.; S];
            self.bq = [[0.; 4]; 4]; self.hold = [0.; 2]; self.sh_lp = 0.; self.sh_out = 0.;
            self.build(); self.target = keep; self.retune(true); self.fade = 0.;
        }
    }
    #[inline(never)]
    fn tank(&mut self, m: usize) {
        let k = self.kk;
        let (e_ol, e_or, e_mk) = (self.off[self.l_er[0]], self.off[self.l_er[1]], self.mask[self.l_er[0]]);
        let n_e = self.n_e; let n_e2 = n_e >> 1; let (e_g, l_g) = (self.e_g, self.l_g);
        let n_t = self.n_t; let n_t2 = n_t >> 1; let hk = k >> 1;
        let i_o: [i64; 12] = std::array::from_fn(|j| self.off[self.l_ind[j]]); let i_m: [i64; 12] = std::array::from_fn(|j| self.mask[self.l_ind[j]]);
        let s_o: [i64; 4] = std::array::from_fn(|j| self.off[self.l_sw[j]]); let s_m: [i64; 4] = std::array::from_fn(|j| self.mask[self.l_sw[j]]);
        let a1_o: [i64; 6] = std::array::from_fn(|j| self.off[self.l_a1[j]]); let a1_m: [i64; 6] = std::array::from_fn(|j| self.mask[self.l_a1[j]]);
        let a2_o: [i64; 6] = std::array::from_fn(|j| self.off[self.l_a2[j]]); let a2_m: [i64; 6] = std::array::from_fn(|j| self.mask[self.l_a2[j]]);
        let d_o: [i64; 6] = std::array::from_fn(|j| self.off[self.l_d[j]]); let d_m: [i64; 6] = std::array::from_fn(|j| self.mask[self.l_d[j]]);
        let kn = 1. / (self.k_s + self.k_r).max(1.); let (k_s, k_r, k_d) = (self.k_s * kn, self.k_r * kn, self.k_d);
        let (d_a, d_d, ra, chaotic) = (self.d_a, self.d_d, self.ra, self.chaotic);
        let period = self.fs_t / (self.rate * if chaotic { 3. } else { 2. });
        let (g1, g2, a_lo, a_hi, sat, shim) = (-self.g_l1, self.g_l2, self.a_lo, self.a_hi, self.sat, self.shim);
        let (blend, att, g_sw) = (self.blend, self.att, self.g_sw);
        let mut w = self.w; let mut dn = self.dn;
        for t in 0..m {
            let (mut xl, mut xr) = (self.tin_l[t], self.tin_r[t]);
            let mem = &mut self.mem;
            // early diffusion: six allpasses a side, blended in from the raw input (sparse) to all of it (dense)
            let (mut yl, mut yr) = (xl, xr);
            for j in 0..6 {
                let g = self.g_in[j];
                let (kk, oo) = (i_m[j], i_o[j]); let d = mem[(oo + ((w - self.ind_d[j]) & kk)) as usize] as f64; let v = yl - g * d; mem[(oo + (w & kk)) as usize] = v as f32; yl = d + g * v;
                let (kk, oo) = (i_m[j + 6], i_o[j + 6]); let d = mem[(oo + ((w - self.ind_d[j + 6]) & kk)) as usize] as f64; let v = yr - g * d; mem[(oo + (w & kk)) as usize] = v as f32; yr = d + g * v;
            }
            xl += (yl - xl) * blend; xr += (yr - xr) * blend;
            mem[(e_ol + (w & e_mk)) as usize] = xl as f32; mem[(e_or + (w & e_mk)) as usize] = xr as f32;
            let (mut el, mut er) = (0., 0.);
            for j in 0..n_e2 { el += mem[(self.e_o[j] + ((w - self.e_d[j]) & e_mk)) as usize] as f64 * self.e_ga[j]; }
            for j in n_e2..n_e { er += mem[(self.e_o[j] + ((w - self.e_d[j]) & e_mk)) as usize] as f64 * self.e_ga[j]; }
            if k == 0 { self.tout_l[t] = el * e_g; self.tout_r[t] = er * e_g; w = if w + 1 >= 0x40000000 { 0x100000 } else { w + 1 }; continue; }
            // the swell: two long allpasses a side, blended in by the attack
            let (mut sl, mut sr) = (xl, xr);
            for j in 0..2 {
                let rd = |mem: &Vec<f32>, oo: i64, kk: i64, pos: f64| { let i = pos as i64; let fr = pos - i as f64; let a = mem[(oo + (i & kk)) as usize] as f64; a + (mem[(oo + ((i + 1) & kk)) as usize] as f64 - a) * fr };
                let d = rd(mem, s_o[j], s_m[j], w as f64 - self.sw_d[j]); let v = sl - g_sw * d; mem[(s_o[j] + (w & s_m[j])) as usize] = v as f32; sl = d + g_sw * v;
                let d = rd(mem, s_o[j + 2], s_m[j + 2], w as f64 - self.sw_d[j + 2]); let v = sr - g_sw * d; mem[(s_o[j + 2] + (w & s_m[j + 2])) as usize] = v as f32; sr = d + g_sw * v;
            }
            sl = xl + (sl - xl) * att; sr = xr + (sr - xr) * att;
            if shim > 0. { sl += self.sh_out * shim; sr += self.sh_out * shim; }
            // the ring: read every stage's delay (randomised for the random modes), its three-band decay and saturation
            for i in 0..k {
                let (cc, ss) = (self.mc[i], self.ms[i]); self.mc[i] = cc * self.rc[i] - ss * self.rs[i]; self.ms[i] = ss * self.rc[i] + cc * self.rs[i];
                self.cnt[i] -= 1.;
                if self.cnt[i] <= 0. {
                    self.cnt[i] = period * (0.75 + 0.5 * self.rng.next());
                    if chaotic { let mut x = self.chaos[i]; x = 3.97 * x * (1. - x); if x <= 0. || x >= 1. { x = 0.37; } self.chaos[i] = x; self.tg[i] = 2. * x - 1.; }
                    else { self.tg[i] = self.rng.next() * 2. - 1.; }
                }
                let q1 = self.r1[i] + ra * (self.tg[i] - self.r1[i]); let q2 = self.r2[i] + ra * (q1 - self.r2[i]); self.r1[i] = q1; self.r2[i] = q2;
                self.m_a[i] = d_a * (1. + k_s * self.ms[i] + k_r * q2);
                let mem = &self.mem;
                let (kk, oo) = (d_m[i], d_o[i]); let pos = w as f64 - self.d_len[i] - d_d * k_d * (1. + q2); let ii = pos as i64; let fr = pos - ii as f64;
                let a = mem[(oo + (ii & kk)) as usize] as f64; let r = a + (mem[(oo + ((ii + 1) & kk)) as usize] as f64 - a) * fr;
                let l = self.lo[i] + a_lo * (r - self.lo[i]); let h = self.hp[i] + a_hi * (r - self.hp[i]); self.lo[i] = l; self.hp[i] = h;
                let mut y = self.gl[i] * l + self.gm[i] * (h - l) + self.gh[i] * (r - h);
                if sat > 0. { y /= 1. + sat * y.abs(); }
                self.o[i] = y;
            }
            let mem = &mut self.mem;
            // and write each stage from the one before: the modulated allpass, the second allpass, the delay
            for i in 0..k {
                let mut x = self.o[if i == 0 { k - 1 } else { i - 1 }] + if i == 0 { sl } else if i == hk { sr } else { 0. };
                let (kk, oo) = (a1_m[i], a1_o[i]); let pos = w as f64 - self.a1_len[i] - self.m_a[i]; let ii = pos as i64; let fr = pos - ii as f64;
                let a = mem[(oo + (ii & kk)) as usize] as f64; let d = a + (mem[(oo + ((ii + 1) & kk)) as usize] as f64 - a) * fr; let v = x - g1 * d;
                mem[(oo + (w & kk)) as usize] = v as f32; x = d + g1 * v;
                let (kk, oo) = (a2_m[i], a2_o[i]); let pos = w as f64 - self.a2_len[i]; let ii = pos as i64; let fr = pos - ii as f64;
                let a = mem[(oo + (ii & kk)) as usize] as f64; let d = a + (mem[(oo + ((ii + 1) & kk)) as usize] as f64 - a) * fr; let v = x - g2 * d;
                mem[(oo + (w & kk)) as usize] = v as f32; x = d + g2 * v;
                mem[(d_o[i] + (w & d_m[i])) as usize] = (x + dn) as f32;
            }
            for i in 1..k { if i != hk { let kk = d_m[i]; let idx = (d_o[i] + ((w + self.inj_k[i]) & kk)) as usize; mem[idx] += (sl * self.inj_l[i] + sr * self.inj_r[i]) as f32; } }
            dn = -dn;
            let (mut ll, mut lr) = (0., 0.);
            for j in 0..n_t2 { ll += mem[(self.t_o[j] + ((w - self.t_d[j]) & self.t_m[j])) as usize] as f64 * self.t_ga[j]; }
            for j in n_t2..n_t { lr += mem[(self.t_o[j] + ((w - self.t_d[j]) & self.t_m[j])) as usize] as f64 * self.t_ga[j]; }
            ll *= l_g; lr *= l_g;
            if shim > 0. { self.shift((ll + lr) * 0.5); }
            self.tout_l[t] = el * e_g + ll; self.tout_r[t] = er * e_g + lr;
            w = if w + 1 >= 0x40000000 { 0x100000 } else { w + 1 };
        }
        self.w = w; self.dn = dn;
        for i in 0..k { let n = self.mc[i].hypot(self.ms[i]); self.mc[i] /= n; self.ms[i] /= n; }
    }
    /// shimmer: two crossfaded grains read at the pitch ratio, lowpassed and soft-limited, fed back into the ring
    fn shift(&mut self, x: f64) {
        let b = &mut self.sh_buf; let mm = self.sh_size as i64 - 1; let ww = self.sh_win; let w = self.sh_w;
        b[(w & mm) as usize] = x as f32;
        let mut p = self.sh_p + (1. - self.sh_ratio) / ww; p -= p.floor(); self.sh_p = p;
        let mut q = p + 0.5; if q >= 1. { q -= 1.; }
        let rd = |pos: f64| { let i = pos.floor() as i64; let f = pos - i as f64; b[(i & mm) as usize] as f64 * (1. - f) + b[((i + 1) & mm) as usize] as f64 * f };
        let ya = rd(w as f64 - p * ww - 1.); let yb = rd(w as f64 - q * ww - 1.);
        let (sa, sb) = ((PI * p).sin(), (PI * q).sin()); let y = ya * sa * sa + yb * sb * sb;
        self.sh_w = (self.sh_w + 1) & 0x3fffffff; self.sh_lp += self.sh_a * (y - self.sh_lp); self.sh_out = self.sh_lp / (1. + self.sh_lp.abs());
    }
}
#[allow(dead_code)]
const _SQ: f64 = SQRT_2;
