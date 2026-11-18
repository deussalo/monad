//! The wave synth's parameter registry (wave.spec.js): one list of knobs for the patch editor, the mod matrix and the
//! voices. Every value here can be a mod destination. Plain data, so the UI can build its editor from it.

use std::sync::OnceLock;

pub const WAVE_OSCS: usize = 3;
pub const WAVE_ENVS: usize = 4;
pub const WAVE_LFOS: usize = 4;
pub const WAVE_MACROS: usize = 8;
pub const WAVE_HARMS: usize = 32;
pub const WAVE_TABLES: [&str; 6] = ["basic", "harmonics", "sweep", "pulse", "vocal", "organ"];
pub const WAVE_FILTERS: [&str; 8] = ["off", "lp12", "lp24", "hp12", "hp24", "bp", "notch", "peak"];
pub const LFO_SHAPES: [&str; 7] = ["sine", "tri", "saw up", "saw down", "square", "s&h", "smooth"];
/// Tempo divisions in beats: LFOs lock to the machine's bpm (120 without a bpm lock).
pub const LFO_SYNCS: [(&str, f64); 11] = [("off", 0.0), ("1/32", 1.0 / 8.0), ("1/16", 0.25), ("1/8T", 1.0 / 3.0), ("1/8", 0.5),
    ("1/4T", 2.0 / 3.0), ("1/4", 1.0), ("1/2", 2.0), ("1 bar", 4.0), ("2 bars", 8.0), ("4 bars", 16.0)];
pub const DELAY_SYNCS: [(&str, f64); 9] = [("off", 0.0), ("1/16", 0.25), ("1/8T", 1.0 / 3.0), ("1/8", 0.5), ("1/8.", 0.75),
    ("1/4T", 2.0 / 3.0), ("1/4", 1.0), ("1/4.", 1.5), ("1/2", 2.0)];
/// The FX rack's modules, in their default order (as Serum's).
pub const WAVE_FX: [&str; 9] = ["hyper", "dist", "flanger", "phaser", "chorus", "delay", "reverb", "comp", "eq"];
pub const DIST_MODES: [&str; 4] = ["tube", "hard", "fold", "crush"];
pub const VOICE_MODES: [&str; 3] = ["poly", "mono", "legato"];
/// Warp modes: 1-6 bend where the table is read, 7-10 reshape what it reads. FM/AM/RM take a sine at another
/// oscillator's pitch: A takes B's, B and C take A's.
pub const WARP_MODES: [&str; 11] = ["off", "bend", "sync", "pwm", "asym", "mirror", "fm", "flip", "quantize", "am", "rm"];

pub const FX_HYPER: usize = 0;
pub const FX_DIST: usize = 1;
pub const FX_FLANGER: usize = 2;
pub const FX_PHASER: usize = 3;
pub const FX_CHORUS: usize = 4;
pub const FX_DELAY: usize = 5;
pub const FX_REVERB: usize = 6;
pub const FX_COMP: usize = 7;
pub const FX_EQ: usize = 8;

pub fn index_in(list: &[&str], name: &str) -> Option<usize> { list.iter().position(|x| *x == name) }
pub fn sync_beats(list: &[(&str, f64)], name: &str) -> Option<f64> { list.iter().find(|x| x.0 == name).map(|x| x.1) }

/// A knob's response: lin, log (a ratio range), time (cubic, fine at the short end), int.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Curve { Lin, Log, Time, Int }

#[derive(Clone, Debug)]
pub struct ParamDef { pub path: String, pub label: &'static str, pub min: f64, pub max: f64, pub default: f64, pub curve: Curve }

pub const OSC_KEYS: [&str; 26] = ["pos", "oct", "semi", "fine", "level", "pan", "uni", "detune", "blend", "spread", "phase", "warp", "warp2",
    "a", "d", "s", "r", "cut", "res", "drive", "key", "famt", "fa", "fd", "fs", "fr"];
pub const OSC_N: usize = 26;
// offsets of an oscillator's knobs inside its block (osc i's block starts at i*OSC_N)
pub const O_POS: usize = 0; pub const O_OCT: usize = 1; pub const O_SEMI: usize = 2; pub const O_FINE: usize = 3; pub const O_LEVEL: usize = 4;
pub const O_PAN: usize = 5; pub const O_UNI: usize = 6; pub const O_DETUNE: usize = 7; pub const O_BLEND: usize = 8; pub const O_SPREAD: usize = 9;
pub const O_PHASE: usize = 10; pub const O_WARP: usize = 11; pub const O_WARP2: usize = 12; pub const O_A: usize = 13; pub const O_D: usize = 14;
pub const O_S: usize = 15; pub const O_R: usize = 16; pub const O_CUT: usize = 17; pub const O_RES: usize = 18; pub const O_DRIVE: usize = 19;
pub const O_KEY: usize = 20; pub const O_FAMT: usize = 21; pub const O_FA: usize = 22; pub const O_FD: usize = 23; pub const O_FS: usize = 24; pub const O_FR: usize = 25;
pub const P_SUB: usize = 78; pub const P_SUBOCT: usize = 79; pub const P_NOISE: usize = 80; pub const P_COLOR: usize = 81;
pub const P_ENV0: usize = 82; // env i: P_ENV0 + 4i + (a, d, s, r)
pub const P_LFO0: usize = 98; // lfo i: P_LFO0 + 2i + (rate, phase)
pub const P_MAC0: usize = 106; // macro i: P_MAC0 + i
pub const P_TOCT: usize = 114; pub const P_TSEMI: usize = 115; pub const P_VOL: usize = 116; pub const P_GLIDE: usize = 117; pub const P_GATE: usize = 118;
pub const P_FX0: usize = 119;
pub const HYPER_VOICES: usize = 119; pub const HYPER_RATE: usize = 120; pub const HYPER_DETUNE: usize = 121; pub const HYPER_MIX: usize = 122; pub const HYPER_DIM: usize = 123;
pub const FLANGER_RATE: usize = 124; pub const FLANGER_DEPTH: usize = 125; pub const FLANGER_FB: usize = 126; pub const FLANGER_SPREAD: usize = 127; pub const FLANGER_MIX: usize = 128;
pub const PHASER_RATE: usize = 129; pub const PHASER_DEPTH: usize = 130; pub const PHASER_FREQ: usize = 131; pub const PHASER_FB: usize = 132; pub const PHASER_SPREAD: usize = 133; pub const PHASER_MIX: usize = 134;
pub const DIST_DRIVE: usize = 135; pub const DIST_MIX: usize = 136;
pub const CHORUS_RATE: usize = 137; pub const CHORUS_DEPTH: usize = 138; pub const CHORUS_MIX: usize = 139;
pub const DELAY_TIME: usize = 140; pub const DELAY_FB: usize = 141; pub const DELAY_TONE: usize = 142; pub const DELAY_MIX: usize = 143;
pub const REVERB_MIX: usize = 144; pub const REVERB_PREDELAY: usize = 145; pub const REVERB_DECAY: usize = 146; pub const REVERB_SIZE: usize = 147;
pub const REVERB_ATTACK: usize = 148; pub const REVERB_EARLY: usize = 149; pub const REVERB_LATE: usize = 150; pub const REVERB_MOD: usize = 151;
pub const REVERB_RATE: usize = 152; pub const REVERB_LOWCUT: usize = 153; pub const REVERB_HIGHCUT: usize = 154;
pub const EQ_LOW: usize = 155; pub const EQ_HIGH: usize = 156;
pub const COMP_THRESH: usize = 157; pub const COMP_RATIO: usize = 158; pub const COMP_GAIN: usize = 159;
/// How many knobs a patch has.
pub const NP: usize = 160;

/// Mod sources: env1-4, lfo1-4, m1-8, vel, note, rand.
pub const NS: usize = 19;
pub const SRC_ENV0: usize = 0; pub const SRC_LFO0: usize = 4; pub const SRC_MAC0: usize = 8;
pub const SRC_VEL: usize = 16; pub const SRC_NOTE: usize = 17; pub const SRC_RAND: usize = 18;
pub const WAVE_SOURCES: [&str; NS] = ["env1", "env2", "env3", "env4", "lfo1", "lfo2", "lfo3", "lfo4",
    "m1", "m2", "m3", "m4", "m5", "m6", "m7", "m8", "vel", "note", "rand"];

fn build() -> Vec<ParamDef> {
    use Curve::*;
    let mut p = Vec::with_capacity(NP);
    let mut push = |path: String, label: &'static str, min: f64, max: f64, default: f64, curve: Curve| p.push(ParamDef { path, label, min, max, default, curve });
    for i in 1..=WAVE_OSCS {
        let o = |k: &str| format!("osc{i}.{k}");
        push(o("pos"), "position", 0., 1., 0., Lin); push(o("oct"), "octave", -4., 4., 0., Int); push(o("semi"), "semi", -12., 12., 0., Int);
        push(o("fine"), "fine", -100., 100., 0., Lin); push(o("level"), "level", 0., 1., if i == 1 { 0.75 } else { 0. }, Lin);
        push(o("pan"), "pan", -1., 1., 0., Lin); push(o("uni"), "unison", 1., 8., 1., Int); push(o("detune"), "detune", 0., 1., 0.25, Lin);
        push(o("blend"), "blend", 0., 1., 0.5, Lin); push(o("spread"), "stereo", 0., 1., 0.8, Lin); push(o("phase"), "phase", 0., 1., 0., Lin);
        push(o("warp"), "warp", -1., 1., 0., Lin); push(o("warp2"), "warp 2", -1., 1., 0., Lin);
        push(o("a"), "attack", 0., 8., 0.003, Time); push(o("d"), "decay", 0., 8., 0.8, Time); push(o("s"), "sustain", 0., 1., 0.7, Lin); push(o("r"), "release", 0., 8., 0.3, Time);
        push(o("cut"), "cutoff", 20., 20000., 20000., Log); push(o("res"), "resonance", 0., 1., 0.1, Lin); push(o("drive"), "drive", 0., 1., 0., Lin);
        push(o("key"), "key track", 0., 1., 0., Lin); push(o("famt"), "env amt", -1., 1., 0., Lin); push(o("fa"), "f attack", 0., 8., 0.003, Time);
        push(o("fd"), "f decay", 0., 8., 0.5, Time); push(o("fs"), "f sustain", 0., 1., 0., Lin); push(o("fr"), "f release", 0., 8., 0.3, Time);
    }
    push("sub.level".into(), "sub", 0., 1., 0., Lin); push("sub.oct".into(), "sub oct", -3., 0., -1., Int);
    push("noise.level".into(), "noise", 0., 1., 0., Lin); push("noise.color".into(), "color", 0., 1., 0.6, Lin);
    for i in 1..=WAVE_ENVS {
        push(format!("env{i}.a"), "attack", 0., 8., 0.003, Time); push(format!("env{i}.d"), "decay", 0., 8., 0.6, Time);
        push(format!("env{i}.s"), "sustain", 0., 1., if i == 1 { 0.8 } else { 0. }, Lin); push(format!("env{i}.r"), "release", 0., 8., 0.3, Time);
    }
    for i in 1..=WAVE_LFOS { push(format!("lfo{i}.rate"), "rate", 0.02, 40., 2., Log); push(format!("lfo{i}.phase"), "phase", 0., 1., 0., Lin); }
    const MACRO_LABELS: [&str; 8] = ["macro 1", "macro 2", "macro 3", "macro 4", "macro 5", "macro 6", "macro 7", "macro 8"];
    for i in 1..=WAVE_MACROS { push(format!("m{i}"), MACRO_LABELS[i - 1], 0., 1., 0., Lin); }
    push("tune.oct".into(), "octave", -4., 4., 0., Int); push("tune.semi".into(), "semi", -12., 12., 0., Int); push("vol".into(), "volume", 0., 1., 0.8, Lin);
    push("glide".into(), "glide", 0., 2., 0., Time); push("gate".into(), "gate", 0.02, 8., 0.35, Time);
    let fx: [(&str, &'static str, f64, f64, f64, Curve); 41] = [
        ("hyper.voices", "voices", 1., 7., 4., Int), ("hyper.rate", "rate", 0.05, 5., 0.5, Log), ("hyper.detune", "detune", 0., 1., 0.4, Lin),
        ("hyper.mix", "mix", 0., 1., 0.5, Lin), ("hyper.dim", "dimension", 0., 1., 0.3, Lin),
        ("flanger.rate", "rate", 0.05, 10., 0.3, Log), ("flanger.depth", "depth", 0., 1., 0.7, Lin), ("flanger.fb", "feedback", 0., 0.95, 0.5, Lin),
        ("flanger.spread", "stereo", 0., 1., 0.25, Lin), ("flanger.mix", "mix", 0., 1., 0.5, Lin),
        ("phaser.rate", "rate", 0.05, 10., 0.4, Log), ("phaser.depth", "depth", 0., 1., 0.7, Lin), ("phaser.freq", "freq", 100., 8000., 800., Log),
        ("phaser.fb", "feedback", 0., 0.9, 0.4, Lin), ("phaser.spread", "stereo", 0., 1., 0.25, Lin), ("phaser.mix", "mix", 0., 1., 0.5, Lin),
        ("dist.drive", "drive", 0., 1., 0.3, Lin), ("dist.mix", "mix", 0., 1., 1., Lin),
        ("chorus.rate", "rate", 0.05, 5., 0.6, Log), ("chorus.depth", "depth", 0., 1., 0.5, Lin), ("chorus.mix", "mix", 0., 1., 0.5, Lin),
        ("delay.time", "time", 0.01, 2., 0.3, Time), ("delay.fb", "feedback", 0., 0.95, 0.4, Lin), ("delay.tone", "tone", 0., 1., 0.6, Lin), ("delay.mix", "mix", 0., 1., 0.25, Lin),
        ("reverb.mix", "mix", 0., 1., 0.3, Lin), ("reverb.predelay", "predelay", 0., 0.25, 0.01, Time), ("reverb.decay", "decay", 0.2, 20., 2., Log),
        ("reverb.size", "size", 0., 1., 0.55, Lin), ("reverb.attack", "attack", 0., 1., 0.15, Lin), ("reverb.early", "early diff", 0., 1., 0.75, Lin),
        ("reverb.late", "late diff", 0., 1., 0.7, Lin), ("reverb.mod", "mod depth", 0., 1., 0.5, Lin), ("reverb.rate", "mod rate", 0.05, 10., 0.6, Log),
        ("reverb.lowcut", "low cut", 10., 1000., 80., Log), ("reverb.highcut", "high cut", 1000., 20000., 9000., Log),
        ("eq.low", "low dB", -15., 15., 0., Lin), ("eq.high", "high dB", -15., 15., 0., Lin),
        ("comp.thresh", "threshold", -40., 0., -12., Lin), ("comp.ratio", "ratio", 1., 20., 4., Log), ("comp.gain", "makeup", 0., 18., 3., Lin),
    ];
    for (a, b, c, d, e, f) in fx { push(a.into(), b, c, d, e, f); }
    p
}

/// The whole registry, in order: index i is the knob every `[f64; NP]` in this crate holds at i.
pub fn wave_params() -> &'static [ParamDef] {
    static P: OnceLock<Vec<ParamDef>> = OnceLock::new();
    P.get_or_init(|| { let p = build(); assert_eq!(p.len(), NP); p })
}
/// A knob's index by its path (`osc1.cut`, `env2.a`, `m3`, `delay.fb`).
pub fn param_index(path: &str) -> Option<usize> { wave_params().iter().position(|p| p.path == path) }
/// Whether a knob belongs to the FX rack (a rack is shared by a patch's voices, so only macros reach it).
pub fn is_fx_param(i: usize) -> bool { i >= P_FX0 && i < NP }

/// Normalised position (0..1) of a value on its knob.
pub fn wave_norm(p: &ParamDef, x: f64) -> f64 {
    let (a, b) = (p.min, p.max);
    if p.curve == Curve::Log { return (x / a).ln() / (b / a).ln(); }
    let n = (x - a) / (b - a);
    if p.curve == Curve::Time { n.max(0.).cbrt() } else { n }
}
/// The value at a normalised knob position.
pub fn wave_denorm(p: &ParamDef, n: f64) -> f64 {
    let (a, b) = (p.min, p.max);
    let n = n.clamp(0., 1.);
    match p.curve {
        Curve::Log => a * (b / a).powf(n),
        Curve::Time => a + (b - a) * n * n * n,
        Curve::Int => js_round(a + (b - a) * n),
        Curve::Lin => a + (b - a) * n,
    }
}
/// A route's source through its curve (-1 bends toward the bottom, +1 toward the top).
#[inline]
pub fn mod_curve(x: f64, c: f64) -> f64 {
    if c == 0. { x } else if c > 0. { 1. - (1. - x).powf(1. + 3. * c) } else { x.powf(1. - 3. * c) }
}
/// JavaScript's Math.round: halves round up.
#[inline]
pub fn js_round(x: f64) -> f64 { (x + 0.5).floor() }
