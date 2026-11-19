//! The world as sim.js holds it in `W`, `S`, `SPACE`, `CLIMAX` and `view`: every part, with its saved fields and the
//! runtime state the JS keeps on the same objects (speeds, flashes, hit counts). References between parts are by id,
//! as in the save; `World::relink` resolves them to indices (the `*_ix` fields) after any structural change.

use serde_json::{Map, Value};
use std::collections::VecDeque;

pub const TAU: f64 = std::f64::consts::PI * 2.0;
pub const TOOTH: f64 = 12.0;
pub const CAM_N: usize = 64;
pub const PROBE_N: usize = 128;
pub const FIELD_SLOTS_N: usize = 8;
pub const WAVE_MACROS: i64 = 8;
pub const LOOKAHEAD_MIN: f64 = 0.12;

/// `((a % TAU) + TAU) % TAU`, as JS computes it.
#[inline]
pub fn wrap(a: f64) -> f64 {
    ((a % TAU) + TAU) % TAU
}
/// `Math.min(b, Math.max(a, x))`.
#[inline]
pub fn clamp(x: f64, a: f64, b: f64) -> f64 {
    js_min(b, js_max(a, x))
}
#[inline]
pub fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else if a > b { a } else { b }
}
#[inline]
pub fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else if a < b { a } else { b }
}
/// `Math.hypot(x, y)` as V8 computes it (scaled, Kahan-summed), so distances match the JS to the last bit.
#[inline]
pub fn hypot(x: f64, y: f64) -> f64 {
    let (ax, ay) = (x.abs(), y.abs());
    if ax == f64::INFINITY || ay == f64::INFINITY {
        return f64::INFINITY;
    }
    if ax.is_nan() || ay.is_nan() {
        return f64::NAN;
    }
    let max = if ax > ay { ax } else { ay };
    if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut comp = 0.0;
    for v in [ax, ay] {
        let n = v / max;
        let summand = n * n - comp;
        let pre = sum + summand;
        comp = (pre - sum) - summand;
        sum = pre;
    }
    sum.sqrt() * max
}
#[inline]
pub fn hz(m: f64) -> f64 {
    440.0 * 2f64.powf((m - 69.0) / 12.0)
}
/// A cog's pitch radius from its tooth count.
#[inline]
pub fn radius_n(n: f64) -> f64 {
    n * TOOTH / TAU
}
/// JS `Math.round`: halves go up.
#[inline]
pub fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}
/// JS `ToInt32`, as `x|0` and the bit operators apply it.
pub fn to_int32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    let t = x.trunc();
    let m = t.rem_euclid(4294967296.0);
    (m as u64 as u32) as i32
}
/// `+x.toFixed(d)` for the values the JS rounds that way (non-negative, far from a tie in practice).
#[inline]
pub fn to_fixed(x: f64, d: i32) -> f64 {
    let k = 10f64.powi(d);
    let r = (x * k).round() / k;
    if r == 0.0 { 0.0 } else { r }
}

pub const SCALES: [(&str, &[i32]); 7] = [
    ("chromatic", &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]),
    ("major", &[0, 2, 4, 5, 7, 9, 11]),
    ("minor", &[0, 2, 3, 5, 7, 8, 10]),
    ("dorian", &[0, 2, 3, 5, 7, 9, 10]),
    ("pentatonic", &[0, 2, 4, 7, 9]),
    ("minpent", &[0, 3, 5, 7, 10]),
    ("whole", &[0, 2, 4, 6, 8, 10]),
];
pub fn scale_ix(name: &str) -> Option<usize> {
    SCALES.iter().position(|(n, _)| *n == name)
}
/// A note on a scale: degree i above base, wrapping into octaves.
pub fn scale_note(base: f64, scale: usize, i: i64) -> f64 {
    let sc = SCALES[scale].1;
    let l = sc.len() as i64;
    base + sc[i.rem_euclid(l) as usize] as f64 + 12.0 * (i as f64 / l as f64).floor()
}

/// sim.js `S`: the engineering settings. Keys a save carries beyond these are kept in `extra`.
#[derive(Clone, Debug)]
pub struct Settings {
    pub bpm: f64,
    pub beats: f64,
    pub seed: f64,
    pub mesh_tol: f64,
    pub coax_tol: f64,
    pub snap_tol: f64,
    pub v_ref: f64,
    pub vel_curve: f64,
    pub master: f64,
    pub lookahead: f64,
    pub max_step: f64,
    pub zoom: f64,
    pub windows: f64,
    pub ids: f64,
    pub ratios: f64,
    pub extra: Map<String, Value>,
}
impl Default for Settings {
    fn default() -> Self {
        Settings {
            bpm: 0.0, beats: 4.0, seed: 1.0, mesh_tol: 4.0, coax_tol: 6.0, snap_tol: 16.0, v_ref: 500.0, vel_curve: 0.72,
            master: 0.7, lookahead: 0.2, max_step: 0.05, zoom: 1.0, windows: 0.0, ids: 0.0, ratios: 0.0, extra: Map::new(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motor {
    pub rpm: f64,
    pub revs: f64,
}

/// A pin on a disc: on ring `ring` at angle `a` from the disc's mark, played with probability `p`, gain `v`, held for
/// `len` steps.
#[derive(Clone, Debug)]
pub struct Dot {
    pub ring: f64,
    pub a: f64,
    pub p: f64,
    pub v: f64,
    pub len: f64,
    /// Not sounded before this sim time (being recorded, or just played by hand). Never saved.
    pub skip: f64,
    /// Sim time it last sounded.
    pub flash: f64,
}
impl Dot {
    pub fn new(ring: f64, a: f64) -> Dot {
        Dot { ring, a, p: 1.0, v: 1.0, len: 1.0, skip: f64::NEG_INFINITY, flash: f64::NEG_INFINITY }
    }
}

/// A disc's score: `range` octaves of `scale` from `root` in `octave`, `div` steps a turn.
#[derive(Clone, Debug)]
pub struct Track {
    pub root: f64,
    pub octave: f64,
    pub range: f64,
    pub scale: usize,
    pub div: f64,
    pub dots: Vec<Dot>,
    /// Keys the save's track carries beyond these (kept, as the JS spread keeps them).
    pub extra: Map<String, Value>,
}
impl Default for Track {
    fn default() -> Self {
        Track { root: 0.0, octave: 3.0, range: 1.0, scale: 0, div: 16.0, dots: Vec::new(), extra: Map::new() }
    }
}
impl Track {
    pub fn rings(&self) -> usize {
        (self.range as usize) * SCALES[self.scale].1.len()
    }
    pub fn base(&self) -> f64 {
        12.0 * (self.octave + 1.0) + self.root
    }
    pub fn ring_note(&self, i: i64) -> f64 {
        scale_note(self.base(), self.scale, i)
    }
    /// Drop dots past the last ring (after a change of scale or range).
    pub fn trim_dots(&mut self) {
        let n = self.rings() as f64;
        self.dots.retain(|d| d.ring < n);
    }
}

#[derive(Clone, Debug)]
pub struct Cog {
    pub id: u32,
    pub x: f64,
    pub y: f64,
    pub n: f64,
    pub motor: Option<Motor>,
    pub layer: f64,
    /// Its angle: its timing. Set only by hand.
    pub th: f64,
    /// Its start angle.
    pub th0: f64,
    /// Its teeth's offset from `th`.
    pub tooth: f64,
    pub track: Option<Track>,
    // runtime
    pub w: f64,
    pub k: Option<f64>,
    pub jam: bool,
    pub turns: f64,
    /// Index of its train's motor cog.
    pub driver: Option<usize>,
    /// Held by the speed knob at this rpm (never saved).
    pub scrub: Option<f64>,
}
impl Cog {
    pub fn radius(&self) -> f64 {
        radius_n(self.n)
    }
    /// Radius of ring i of its track.
    pub fn ring_r(&self, i: f64) -> f64 {
        let r = self.radius();
        let n = self.track.as_ref().map(|t| t.rings()).unwrap_or(1);
        if n > 1 { r * (0.25 + 0.65 * i / (n - 1) as f64) } else { r * 0.6 }
    }
}

#[derive(Clone, Debug)]
pub struct Arm {
    pub id: u32,
    pub cog: u32,
    pub a: f64,
    pub r0: f64,
    pub len: f64,
    pub flex: f64,
    pub cog_ix: usize,
    pub flash: f64,
    pub bend_t: f64,
    pub bend_a: f64,
}

/// What a part's voice is. cogs-dsp owns what the rest of the voice JSON means.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VoiceKind {
    /// The modal synth (saved as `synth`, shown as "modal").
    Synth,
    /// The wave synth (saved as `wave`, shown as "synth").
    Wave,
    Kick,
    Snare,
    Hat,
    Tom,
    Clap,
    Rim,
    Cowbell,
    Wood,
    Cymbal,
}
impl VoiceKind {
    pub const ALL: [VoiceKind; 11] = [
        VoiceKind::Synth, VoiceKind::Kick, VoiceKind::Snare, VoiceKind::Hat, VoiceKind::Tom, VoiceKind::Clap,
        VoiceKind::Rim, VoiceKind::Cowbell, VoiceKind::Wood, VoiceKind::Cymbal, VoiceKind::Wave,
    ];
    pub fn name(self) -> &'static str {
        match self {
            VoiceKind::Synth => "synth",
            VoiceKind::Wave => "wave",
            VoiceKind::Kick => "kick",
            VoiceKind::Snare => "snare",
            VoiceKind::Hat => "hat",
            VoiceKind::Tom => "tom",
            VoiceKind::Clap => "clap",
            VoiceKind::Rim => "rim",
            VoiceKind::Cowbell => "cowbell",
            VoiceKind::Wood => "wood",
            VoiceKind::Cymbal => "cymbal",
        }
    }
    pub fn from_name(s: &str) -> Option<VoiceKind> {
        VoiceKind::ALL.iter().copied().find(|k| k.name() == s)
    }
    pub fn is_drum(self) -> bool {
        !matches!(self, VoiceKind::Synth | VoiceKind::Wave)
    }
}

/// A part's voice: the save's `voice` object kept whole in `raw` (cogs-dsp reads it), with the fields the sim itself
/// uses cached. Change it through `Voice::set` (or replace `raw` and call `Voice::recache`), which bumps `ver` so the
/// app knows to send the engine the new patch.
#[derive(Clone, Debug)]
pub struct Voice {
    pub kind: VoiceKind,
    pub raw: Map<String, Value>,
    pub level: f64,
    pub vel_sense: f64,
    pub note: f64,
    pub octave: f64,
    pub semi: f64,
    pub ver: u32,
}

#[derive(Clone, Debug)]
pub struct Pad {
    pub id: u32,
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub voice: Voice,
    pub hits: u32,
    pub last_v: f64,
    pub flash: f64,
}

/// Where a seated reader lies on its disc: a radius's world angle, and its ends as fractions of the disc's radius.
#[derive(Clone, Copy, Debug)]
pub struct Seat {
    pub ang: f64,
    pub f0: f64,
    pub f1: f64,
}

#[derive(Clone, Debug)]
pub struct Reader {
    pub id: u32,
    pub on: Option<u32>,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    pub voice: Voice,
    /// A comb: this many tines across its length, each a note of `comb_scale` from `comb_base`. 0: a plain reader.
    pub tines: f64,
    pub comb_base: f64,
    pub comb_scale: String,
    pub seat: Option<Seat>,
    pub on_ix: Option<usize>,
    pub hits: u32,
    pub last_v: f64,
    pub last_midi: f64,
    pub flash: f64,
}

#[derive(Clone, Debug)]
pub struct Belt {
    pub id: u32,
    pub a: u32,
    pub b: u32,
    pub cross: bool,
    pub a_ix: usize,
    pub b_ix: usize,
}

/// A string or bar, plucked by arms.
#[derive(Clone, Debug)]
pub struct Str {
    pub id: u32,
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    /// `string` or `bar`.
    pub kind: String,
    pub f200: f64,
    pub quant: bool,
    pub decay: f64,
    pub damp: f64,
    pub hard: f64,
    pub level: f64,
    pub vel_sense: f64,
    pub hits: u32,
    pub last_v: f64,
    pub last_u: f64,
    pub flash: f64,
}
impl Str {
    pub fn len(&self) -> f64 {
        hypot(self.x2 - self.x1, self.y2 - self.y1)
    }
    /// Its pitch, from its length.
    pub fn midi(&self) -> f64 {
        let l = self.len().max(1.0);
        let f0 = if self.kind == "bar" { self.f200 * (200.0 / l).powi(2) } else { self.f200 * 200.0 / l };
        let m = 69.0 + 12.0 * (f0 / 440.0).log2();
        if self.quant { js_round(m) } else { m }
    }
}

#[derive(Clone, Debug)]
pub struct Cam {
    pub id: u32,
    pub cog: u32,
    pub ang: f64,
    pub phase: f64,
    pub shape: String,
    pub prof: Vec<f64>,
    /// The reader it turns a macro of.
    pub to: Option<u32>,
    /// 1-based macro slot.
    pub macro_: i64,
    /// The macros it turns beside its first (`to`, `macro_`): up to MACRO_LINKS in all.
    pub more: Vec<MacroLink>,
    pub lo: f64,
    pub hi: f64,
    /// Bumped when its profile changes.
    pub ver: u32,
    pub cog_ix: usize,
    pub to_ix: Option<usize>,
}

#[derive(Clone, Debug)]
pub struct Ball {
    pub id: u32,
    pub note: i32,
    pub r: f64,
    pub x: f64,
    pub y: f64,
    pub vx: f64,
    pub vy: f64,
    pub at: f64,
    pub flash: f64,
    /// [t, x, y]: where it was, for drawing it at the heard time.
    pub trail: VecDeque<[f64; 3]>,
}

/// A ball pit: a sphere (w = h is its diameter) of balls, each a note.
#[derive(Clone, Debug)]
pub struct Pit {
    pub id: u32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub voice: Voice,
    pub notes: Vec<i32>,
    pub balls: Vec<Ball>,
    pub nb: u32,
    pub bag: Vec<i32>,
    pub hits: u32,
    pub last_v: f64,
    pub flash: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    Arm,
    Cog,
    Cam,
    Ball,
}
impl Host {
    pub fn name(self) -> &'static str {
        match self {
            Host::Arm => "arm",
            Host::Cog => "cog",
            Host::Cam => "cam",
            Host::Ball => "ball",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Y,
    X,
    Reach,
}
impl Axis {
    pub fn name(self) -> &'static str {
        match self {
            Axis::Y => "y",
            Axis::X => "x",
            Axis::Reach => "reach",
        }
    }
}
/// How many macros one cam or probe may turn, its first link included.
pub const MACRO_LINKS: usize = 64;
/// One more macro a cam or probe turns: a synth part and its 1-based macro slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MacroLink {
    pub to: Part,
    pub macro_: i64,
}

/// A voiced part: what a probe drives, and which voice an event plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Part {
    Pad(u32),
    Reader(u32),
    Pit(u32),
    String(u32),
}
impl Part {
    pub fn id(self) -> u32 {
        match self {
            Part::Pad(i) | Part::Reader(i) | Part::Pit(i) | Part::String(i) => i,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Probe {
    pub id: u32,
    pub host: Option<Host>,
    pub on: u32,
    pub ball: u32,
    pub r: f64,
    pub a: f64,
    pub axis: Axis,
    pub to: Option<Part>,
    pub macro_: i64,
    /// The synth macros it turns beside its first (`to`, `macro_`): up to MACRO_LINKS in all.
    pub more: Vec<MacroLink>,
    pub param: Option<String>,
    pub lo: f64,
    pub hi: f64,
    pub on_ix: Option<usize>,
    /// One turn of its value, cached on `key`.
    pub tab: Vec<f64>,
    pub key: [f64; 13],
    /// The travel's box relative to the axle: x0, x1, y0, y1.
    pub tbox: [f64; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct FieldFlash {
    pub x: f64,
    pub y: f64,
    pub t: f64,
    pub a: f64,
}

/// An FX field: a borderless sphere of effect.
#[derive(Clone, Debug)]
pub struct Field {
    pub id: u32,
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub mode: String,
    pub diffuse: f64,
    pub level: f64,
    pub insert: f64,
    pub sync: String,
    pub time: f64,
    pub fb: f64,
    pub tone: f64,
    pub ping: f64,
    pub size: f64,
    pub decay: f64,
    pub predelay: f64,
    pub verb_mode: f64,
    pub verb_color: f64,
    pub kind: String,
    pub drive: f64,
    /// Its slot in the fields engine, or -1: silent.
    pub slot: i32,
    pub flashes: VecDeque<FieldFlash>,
}

#[derive(Clone, Debug, Default)]
pub struct View {
    pub x: f64,
    pub y: f64,
    pub extra: Map<String, Value>,
}

/// sim.js's `simT` and the parts of `clock` the machine itself reads.
#[derive(Clone, Debug, Default)]
pub struct Clock {
    /// How far the machine has been stepped.
    pub sim_t: f64,
    /// The heard time, what is drawn (`clock.view`): the app sets it.
    pub view: f64,
    pub paused: bool,
    /// Paused on the start by `to_start`, with nothing played since: plates turned now set their start.
    pub at_start: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub events: u64,
    pub steps: u64,
    pub steps_tick: u64,
    pub skipped: f64,
}
