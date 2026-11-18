//! phone.js bezels: a ring of sectors around the selected part, each one parameter; drag up or down from a sector to
//! turn it, tap a choice to step it, tap an action to do it. With nothing selected, a tap on empty ground brings up the
//! room's bezel: the reverb, the master, tempo, the line amp, and the seal (share).

use super::draw::*;
use super::*;
use cogs_sim::{Part, VoiceKind, SCALES};
use egui::{vec2, Align2, FontId, Stroke};
use serde_json::{json, Value};

pub const BEZEL_W: f64 = 19.0;
const DRAG_PX: f64 = 240.0;
const OPTION_PX: f64 = 32.0;
const BEZEL_PARAMS: usize = 6;

pub const VERB_MODES: [&str; 20] = ["concert hall", "bright hall", "plate", "room", "chamber", "random space", "chorus space", "ambience",
    "sanctuary", "dirty hall", "dirty plate", "smooth plate", "smooth room", "smooth random", "nonlin", "chaotic hall", "chaotic chamber",
    "chaotic neutral", "cathedral", "palace"];
pub const VERB_COLORS: [&str; 3] = ["1970s", "1980s", "now"];

/// What a sector reads and writes.
#[derive(Clone, Debug, PartialEq)]
pub enum Key {
    Voice(Part, &'static str),
    VoiceKind(Part),
    Reader(u32, &'static str),
    Str(u32, &'static str),
    Track(u32, &'static str),
    Cam(u32, &'static str),
    Probe(u32, &'static str),
    Field(u32, &'static str),
    Dot(u32, f64, f64, &'static str),
    Space(&'static str),
    S(&'static str),
    Climax(&'static str),
    /// A wave patch knob pinned to the part's dial, by its index in cogs_dsp's params; read and set normalised.
    Dial(Part, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Val {
    N(f64),
    S(String),
}
impl Val {
    pub fn n(&self) -> f64 {
        match self {
            Val::N(x) => *x,
            Val::S(_) => 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fmt {
    Plain,
    Note,
    NoteClass,
    OnOff,
    Macro,
    Oct,
    Kind,
    Verb,
    Color,
    Axis,
    FxMode,
    /// A patch knob's value, from its normalised position.
    Dial(usize),
}

#[derive(Clone, Debug)]
pub enum Act {
    Link(Sel),
    Share,
    /// Open the part's synth patch editor.
    Patch,
}

#[derive(Clone, Debug)]
pub struct Sector {
    pub icon: &'static str,
    pub label: String,
    pub key: Option<Key>,
    pub min: f64,
    pub max: f64,
    pub step: f64,
    pub log: bool,
    pub options: Option<Vec<Val>>,
    pub toggle: bool,
    pub action: Option<Act>,
    pub fmt: Fmt,
}
impl Sector {
    fn num(icon: &'static str, label: &str, key: Key, min: f64, max: f64, step: f64) -> Sector {
        let fmt = match &key {
            Key::Voice(_, "note") | Key::Reader(_, "combBase") => Fmt::Note,
            _ => Fmt::Plain,
        };
        Sector { icon, label: label.into(), key: Some(key), min, max, step, log: min > 0.0 && max / min >= 20.0, options: None, toggle: false, action: None, fmt }
    }
    fn step(icon: &'static str, label: &str, key: Key, min: f64, max: f64, fmt: Fmt) -> Sector {
        Sector { icon, label: label.into(), key: Some(key), min, max, step: 1.0, log: false, options: None, toggle: false, action: None, fmt }
    }
    fn choice(icon: &'static str, label: &str, key: Key, options: Vec<Val>, fmt: Fmt) -> Sector {
        Sector { icon, label: label.into(), key: Some(key), min: 0.0, max: 0.0, step: 0.0, log: false, options: Some(options), toggle: fmt == Fmt::OnOff, action: None, fmt }
    }
    fn action(icon: &'static str, label: String, a: Act) -> Sector {
        Sector { icon, label, key: None, min: 0.0, max: 0.0, step: 0.0, log: false, options: None, toggle: false, action: Some(a), fmt: Fmt::Plain }
    }
    pub fn steps(&self) -> f64 {
        if self.options.is_none() && self.action.is_none() && self.step >= 1.0 && (self.max - self.min) / self.step <= 120.0 { (self.max - self.min) / self.step } else { 0.0 }
    }
    pub fn norm(&self, v: f64) -> f64 {
        if self.log { (v / self.min).ln() / (self.max / self.min).ln() } else { (v - self.min) / (self.max - self.min) }
    }
    pub fn denorm(&self, n: f64) -> f64 {
        let n = n.clamp(0.0, 1.0);
        let x = if self.log { self.min * (self.max / self.min).powf(n) } else { self.min + (self.max - self.min) * n };
        (((x / self.step).round() * self.step * 1e6).round() / 1e6).clamp(self.min, self.max)
    }
    pub fn format(&self, v: &Val) -> String {
        let n = v.n();
        match (self.fmt, v) {
            (_, Val::S(s)) => match self.fmt {
                Fmt::Kind => kind_label(s),
                Fmt::FxMode if s == "dist" => "distortion".into(),
                Fmt::Axis => match s.as_str() {
                    "y" => "up–down".into(),
                    "x" => "left–right".into(),
                    _ => "reach".into(),
                },
                _ => s.clone(),
            },
            (Fmt::Note, _) => note_name(n as i64),
            (Fmt::NoteClass, _) => NOTE_NAMES[(n as i64).rem_euclid(12) as usize].into(),
            (Fmt::OnOff, _) => if n != 0.0 { "on" } else { "off" }.into(),
            (Fmt::Macro, _) => format!("M{n}"),
            (Fmt::Oct, _) => format!("{n} oct"),
            (Fmt::Verb, _) => VERB_MODES.get(n as usize).copied().unwrap_or("").into(),
            (Fmt::Color, _) => VERB_COLORS.get(n as usize).copied().unwrap_or("").into(),
            (Fmt::Dial(i), _) => {
                let p = &cogs_dsp::params::wave_params()[i];
                let x = cogs_dsp::params::wave_denorm(p, n);
                if x.abs() >= 1000.0 { format!("{:.1}k", x / 1000.0) } else { setting_text((x * 100.0).round() / 100.0) }
            }
            _ => setting_text(n),
        }
    }
}
pub fn setting_text(x: f64) -> String {
    let s = format!("{:.3}", x);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}
pub fn kind_label(k: &str) -> String {
    match k {
        "synth" => "modal".into(),
        "wave" => "synth".into(),
        k => k.into(),
    }
}

/// A bezel: where, how big, and its sectors.
pub struct Bezel {
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
    pub sectors: Vec<Sector>,
    pub owner: Option<Sel>,
    /// Half the ring's width.
    pub bw: f64,
    /// Sectors say what they are in words, with their values.
    pub labels: bool,
    /// The middle says whose bezel it is.
    pub hub: Option<(String, String)>,
    /// A compact bezel beside its part: the point on the part its leader runs to.
    pub leader: Option<(f64, f64)>,
}

// ---------- reading and writing ----------
fn n(v: Option<&Value>) -> f64 {
    cogs_sim::voice::js_num(v)
}
pub fn get(w: &World, k: &Key) -> Val {
    let num = |x: f64| Val::N(x);
    match k {
        Key::Voice(p, key) => match w.voice(*p) {
            Some(v) => match v.raw.get(*key) {
                Some(Value::String(s)) => Val::S(s.clone()),
                x => num(n(x)),
            },
            None => num(0.0),
        },
        Key::VoiceKind(p) => Val::S(w.voice(*p).map(|v| v.kind.name()).unwrap_or("").into()),
        Key::Reader(id, key) => match w.readers.iter().find(|r| r.id == *id) {
            Some(r) => match *key {
                "tines" => num(r.tines),
                "combBase" => num(r.comb_base),
                _ => Val::S(r.comb_scale.clone()),
            },
            None => num(0.0),
        },
        Key::Str(id, key) => match w.strings.iter().find(|r| r.id == *id) {
            Some(s) => match *key {
                "kind" => Val::S(s.kind.clone()),
                "f200" => num(s.f200),
                "quant" => num(s.quant as i32 as f64),
                "decay" => num(s.decay),
                "damp" => num(s.damp),
                "hard" => num(s.hard),
                "level" => num(s.level),
                _ => num(s.vel_sense),
            },
            None => num(0.0),
        },
        Key::Track(id, key) => match w.cog(*id).and_then(|c| c.track.as_ref()) {
            Some(t) => match *key {
                "scale" => Val::S(SCALES[t.scale].0.into()),
                "root" => num(t.root),
                "octave" => num(t.octave),
                "range" => num(t.range),
                _ => num(t.div),
            },
            None => num(0.0),
        },
        Key::Cam(id, key) => match w.cams.iter().find(|m| m.id == *id) {
            Some(m) => match *key {
                "shape" => Val::S(m.shape.clone()),
                "phase" => num(m.phase),
                "lo" => num(m.lo),
                "hi" => num(m.hi),
                _ => num(m.macro_ as f64),
            },
            None => num(0.0),
        },
        Key::Probe(id, key) => match w.probes.iter().find(|m| m.id == *id) {
            Some(p) => match *key {
                "axis" => Val::S(p.axis.name().into()),
                "lo" => num(p.lo),
                "hi" => num(p.hi),
                "macro" => num(p.macro_ as f64),
                _ => {
                    let kind = p.to.and_then(|t| w.voice(t)).map(|v| v.kind).unwrap_or(VoiceKind::Kick);
                    Val::S(cogs_sim::voice::probe_def(kind, p.param.as_deref()).map(|d| d.0).unwrap_or("").into())
                }
            },
            None => num(0.0),
        },
        Key::Field(id, key) => match w.fields.iter().find(|f| f.id == *id) {
            Some(f) => match *key {
                "mode" => Val::S(f.mode.clone()),
                "sync" => Val::S(f.sync.clone()),
                "kind" => Val::S(f.kind.clone()),
                k => num(field_num(f, k)),
            },
            None => num(0.0),
        },
        Key::Dot(cog, ring, a, key) => match w.cog(*cog).and_then(|c| c.track.as_ref()).and_then(|t| t.dots.iter().find(|d| d.ring == *ring && wrap_pi(d.a - a).abs() < 1e-6)) {
            Some(d) => num(match *key {
                "p" => d.p,
                "v" => d.v,
                _ => d.len,
            }),
            None => num(0.0),
        },
        Key::Dial(p, i) => match w.voice(*p) {
            Some(v) => {
                let patch = crate::play::wave_patch(&v.raw);
                num(cogs_dsp::params::wave_norm(&cogs_dsp::params::wave_params()[*i], patch.values[*i]))
            }
            None => num(0.0),
        },
        Key::Space(key) => num(n(w.space.get(*key))),
        Key::Climax(key) => num(n(w.climax.get(*key))),
        Key::S(key) => num(match *key {
            "master" => w.s.master,
            "bpm" => w.s.bpm,
            "beats" => w.s.beats,
            _ => w.s.ratios,
        }),
    }
}
fn field_num(f: &cogs_sim::Field, k: &str) -> f64 {
    match k {
        "time" => f.time,
        "fb" => f.fb,
        "tone" => f.tone,
        "ping" => f.ping,
        "size" => f.size,
        "decay" => f.decay,
        "predelay" => f.predelay,
        "verbMode" => f.verb_mode,
        "verbColor" => f.verb_color,
        "drive" => f.drive,
        "diffuse" => f.diffuse,
        "level" => f.level,
        "insert" => f.insert,
        _ => f.r,
    }
}
fn jnum(x: f64) -> Value {
    cogs_sim::voice::num(x)
}
/// Write a value; true when the change is structural (the solver must run).
pub fn set(w: &mut World, k: &Key, v: Val) -> bool {
    let x = v.n();
    match k {
        Key::Voice(p, key) => {
            if let Some(vo) = w.voice_mut(*p) {
                vo.set(key, match v {
                    Val::S(s) => json!(s),
                    Val::N(x) => jnum(x),
                });
            }
        }
        Key::VoiceKind(p) => {
            let Val::S(kind) = v else { return false };
            let Some(old) = w.voice(*p) else { return false };
            let Some(k) = VoiceKind::from_name(&kind) else { return false };
            let mut nv = cogs_sim::Voice::new(k);
            nv.set("level", jnum(old.level));
            nv.set("velSense", jnum(old.vel_sense));
            if old.raw.contains_key("note") && (k == VoiceKind::Synth || k == VoiceKind::Wave) {
                nv.set("note", jnum(old.note));
            }
            let ver = old.ver;
            let slot = w.voice_mut(*p).unwrap();
            *slot = nv;
            slot.ver = ver.wrapping_add(7);
        }
        Key::Reader(id, key) => {
            if let Some(r) = w.readers.iter_mut().find(|r| r.id == *id) {
                match (*key, v) {
                    ("tines", _) => r.tines = x,
                    ("combBase", _) => r.comb_base = x,
                    (_, Val::S(s)) => r.comb_scale = s,
                    _ => {}
                }
            }
        }
        Key::Str(id, key) => {
            if let Some(s) = w.strings.iter_mut().find(|r| r.id == *id) {
                match (*key, v) {
                    ("kind", Val::S(k)) => s.kind = k,
                    ("f200", _) => s.f200 = x,
                    ("quant", _) => s.quant = x != 0.0,
                    ("decay", _) => s.decay = x,
                    ("damp", _) => s.damp = x,
                    ("hard", _) => s.hard = x,
                    ("level", _) => s.level = x,
                    ("velSense", _) => s.vel_sense = x,
                    _ => {}
                }
            }
        }
        Key::Track(id, key) => {
            if let Some(t) = w.cog_mut(*id).and_then(|c| c.track.as_mut()) {
                match (*key, v) {
                    ("scale", Val::S(s)) => {
                        if let Some(i) = cogs_sim::scale_ix(&s) {
                            t.scale = i;
                        }
                    }
                    ("root", _) => t.root = x,
                    ("octave", _) => t.octave = x,
                    ("range", _) => t.range = x,
                    ("div", _) => t.div = x,
                    _ => {}
                }
                t.trim_dots();
            }
        }
        Key::Cam(id, key) => {
            if let Some(m) = w.cams.iter_mut().find(|m| m.id == *id) {
                match (*key, v) {
                    ("shape", Val::S(s)) => m.set_shape(&s),
                    ("phase", _) => m.phase = x,
                    ("lo", _) => m.lo = x,
                    ("hi", _) => m.hi = x,
                    ("macro", _) => m.macro_ = x as i64,
                    _ => {}
                }
            }
            return true;
        }
        Key::Probe(id, key) => {
            if let Some(p) = w.probes.iter_mut().find(|m| m.id == *id) {
                match (*key, v) {
                    ("axis", Val::S(s)) => {
                        p.axis = match s.as_str() {
                            "x" => cogs_sim::Axis::X,
                            "reach" => cogs_sim::Axis::Reach,
                            _ => cogs_sim::Axis::Y,
                        };
                        p.key = [f64::NAN; 13];
                    }
                    ("lo", _) => p.lo = x,
                    ("hi", _) => p.hi = x,
                    ("macro", _) => p.macro_ = x as i64,
                    ("param", Val::S(s)) => p.param = Some(s),
                    _ => {}
                }
            }
            return true;
        }
        Key::Field(id, key) => {
            if let Some(f) = w.fields.iter_mut().find(|f| f.id == *id) {
                match (*key, v) {
                    ("mode", Val::S(s)) => f.set_mode(&s),
                    ("sync", Val::S(s)) => f.sync = s,
                    ("kind", Val::S(s)) => f.kind = s,
                    ("time", _) => f.time = x,
                    ("fb", _) => f.fb = x,
                    ("tone", _) => f.tone = x,
                    ("ping", _) => f.ping = x,
                    ("size", _) => f.size = x,
                    ("decay", _) => f.decay = x,
                    ("predelay", _) => f.predelay = x,
                    ("verbMode", _) => f.verb_mode = x,
                    ("verbColor", _) => f.verb_color = x,
                    ("drive", _) => f.drive = x,
                    ("diffuse", _) => f.diffuse = x,
                    ("level", _) => f.level = x,
                    ("insert", _) => f.insert = x,
                    ("r", _) => f.r = x,
                    _ => {}
                }
            }
        }
        Key::Dot(cog, ring, a, key) => {
            if let Some(d) = w.cog_mut(*cog).and_then(|c| c.track.as_mut()).and_then(|t| t.dots.iter_mut().find(|d| d.ring == *ring && wrap_pi(d.a - a).abs() < 1e-6)) {
                match *key {
                    "p" => d.p = x,
                    "v" => d.v = x,
                    _ => d.len = x,
                }
            }
        }
        Key::Dial(p, i) => {
            if let Some(v) = w.voice_mut(*p) {
                // as the synth editor writes a patch back: the patch's keys over the voice's, its pitch folded in
                let mut patch = crate::play::wave_patch(&v.raw);
                patch.values[*i] = cogs_dsp::params::wave_denorm(&cogs_dsp::params::wave_params()[*i], x);
                let mut raw = v.raw.clone();
                raw.remove("preset");
                if let Value::Object(o) = patch.to_json() {
                    for (k, val) in o {
                        if k != "dial" {
                            raw.insert(k, val);
                        }
                    }
                }
                raw.insert("octave".into(), json!(0));
                raw.insert("semi".into(), json!(0));
                v.raw = raw;
                v.recache();
            }
        }
        Key::Space(key) => {
            w.space.insert((*key).into(), jnum(x));
        }
        Key::Climax(key) => {
            w.climax.insert((*key).into(), jnum(x));
        }
        Key::S(key) => {
            match *key {
                "master" => w.s.master = x,
                "bpm" => w.s.bpm = x,
                "beats" => w.s.beats = x,
                _ => w.s.ratios = x,
            }
            return *key != "master";
        }
    }
    false
}

fn nums(v: &[f64]) -> Vec<Val> {
    v.iter().map(|x| Val::N(*x)).collect()
}
fn strs(v: &[&str]) -> Vec<Val> {
    v.iter().map(|x| Val::S((*x).into())).collect()
}
const KINDS: [&str; 11] = ["synth", "kick", "snare", "hat", "tom", "clap", "rim", "cowbell", "wood", "cymbal", "wave"];
/// VOICE_DEFS, all rows: [key, label, min, max, step] (a choice row has no range).
fn voice_defs(kind: VoiceKind) -> Vec<(&'static str, &'static str, f64, f64, f64)> {
    use VoiceKind::*;
    match kind {
        Synth => vec![("note", "note", 12.0, 108.0, 1.0), ("octave", "octave", -3.0, 3.0, 1.0), ("semi", "semitones", -12.0, 12.0, 1.0), ("body", "body", 0.0, 0.0, 0.0),
            ("partials", "partials", 1.0, 12.0, 1.0), ("bright", "bright", 0.0, 1.0, 0.01), ("even", "even partials", 0.0, 1.0, 0.01), ("bend", "bend", 0.0, 1.0, 0.01),
            ("hard", "hard", 0.0, 1.0, 0.01), ("attack", "attack s", 0.0, 1.0, 0.005), ("decay", "decay s", 0.02, 8.0, 0.01), ("damp", "damp", 0.0, 2.0, 0.01)],
        Kick => vec![("tune", "tune Hz", 20.0, 200.0, 1.0), ("sweep", "sweep x", 1.0, 10.0, 0.1), ("pdecay", "p decay s", 0.005, 0.3, 0.001), ("decay", "decay s", 0.02, 2.0, 0.01)],
        Snare => vec![("tune", "tune Hz", 60.0, 600.0, 1.0), ("tone", "tone", 0.0, 1.0, 0.01), ("cutoff", "cutoff Hz", 200.0, 12000.0, 10.0), ("decay", "decay s", 0.02, 1.0, 0.005)],
        Hat => vec![("cutoff", "cutoff Hz", 1000.0, 16000.0, 10.0), ("decay", "decay s", 0.005, 1.0, 0.005)],
        Tom => vec![("tune", "tune Hz", 40.0, 400.0, 1.0), ("sweep", "sweep x", 1.0, 4.0, 0.1), ("pdecay", "p decay s", 0.005, 0.3, 0.001), ("tone", "tone", 0.0, 1.0, 0.01), ("decay", "decay s", 0.02, 2.0, 0.01)],
        Clap => vec![("cutoff", "cutoff Hz", 300.0, 6000.0, 10.0), ("spread", "spread s", 0.003, 0.04, 0.001), ("decay", "decay s", 0.05, 1.0, 0.005)],
        Rim => vec![("tune", "tune Hz", 400.0, 4000.0, 10.0), ("decay", "decay s", 0.01, 0.5, 0.005)],
        Cowbell => vec![("tune", "tune Hz", 200.0, 1200.0, 1.0), ("decay", "decay s", 0.02, 2.0, 0.01)],
        Wood => vec![("tune", "tune Hz", 500.0, 5000.0, 10.0), ("decay", "decay s", 0.01, 0.5, 0.005)],
        Cymbal => vec![("cutoff", "cutoff Hz", 1000.0, 12000.0, 10.0), ("tone", "tone", 0.0, 1.0, 0.01), ("decay", "decay s", 0.1, 4.0, 0.01)],
        Wave => vec![("note", "note", 12.0, 108.0, 1.0)],
    }
}
pub fn voice_defs_pub(kind: VoiceKind) -> Vec<(&'static str, &'static str, f64, f64, f64)> {
    voice_defs(kind)
}
fn voice_icon(k: &str) -> &'static str {
    match k {
        "note" | "tune" => "fork",
        "octave" => "octave",
        "semi" => "semi",
        "body" => "bell",
        "partials" => "partials",
        "bright" => "bright",
        "even" => "even",
        "bend" => "bend",
        "hard" => "hard",
        "attack" => "attack",
        "pdecay" => "pdecay",
        "damp" => "damp",
        "sweep" => "sweep",
        "tone" => "tone",
        "cutoff" => "cutoff",
        "spread" => "spread",
        "level" => "level",
        "velSense" => "vel",
        _ => "decay",
    }
}
fn voice_sectors(w: &World, p: Part, with_note: bool) -> Vec<Sector> {
    let Some(v) = w.voice(p) else { return vec![] };
    let mut out = vec![Sector::choice("kind", "voice", Key::VoiceKind(p), strs(&KINDS), Fmt::Kind)];
    if v.kind == VoiceKind::Wave {
        if with_note {
            out.push(Sector::num("fork", "note", Key::Voice(p, "note"), 12.0, 108.0, 1.0));
        }
        out.push(Sector::num("level", "volume", Key::Voice(p, "level"), 0.0, 1.5, 0.01));
        out.extend(dial_sectors(v, p));
        return out;
    }
    for (k, l, a, b, s) in voice_defs(v.kind).into_iter().filter(|d| with_note || d.0 != "note").take(BEZEL_PARAMS) {
        if k == "body" {
            out.push(Sector::choice("bell", l, Key::Voice(p, "body"), strs(&["harmonic", "bell", "plate", "bar", "bowl"]), Fmt::Plain));
        } else {
            out.push(Sector::num(voice_icon(k), l, Key::Voice(p, k), a, b, s));
        }
    }
    out.push(Sector::num("level", "level", Key::Voice(p, "level"), 0.0, 1.5, 0.01));
    out
}

impl Stage {
    /// activeBezel: the bezel for what is selected now, or the room's.
    pub(super) fn active_bezel(&mut self, w: &World) -> Option<Bezel> {
        if self.carrying {
            return None;
        }
        let ring = |me: &Stage, at: (f64, f64), size: f64, sectors: Vec<Sector>, owner: Sel| {
            let c = me.to_screen(w, at.0, at.1);
            Bezel { cx: c.0, cy: c.1, r: (size + 46.0).clamp(92.0, 132.0), sectors, owner: Some(owner), bw: BEZEL_W, labels: false, hub: None, leader: None }
        };
        let z = w.s.zoom;
        let b = match self.sel {
            None => {
                if !self.room {
                    None
                } else {
                    let (t, bt) = (self.safe_top, self.shelf_top());
                    Some(Bezel { cx: self.cw / 2.0, cy: (t + bt) / 2.0, r: self.cw.min(bt - t) / 2.0 - 44.0, sectors: room_sectors(), owner: None, bw: BEZEL_W, labels: false, hub: None, leader: None })
                }
            }
            Some(s @ Sel::Pad(i)) => w.pads.iter().find(|o| o.id == i).map(|o| ring(self, (o.x, o.y), o.r * z, voice_sectors(w, Part::Pad(i), true), s)),
            Some(s @ Sel::Reader(i)) => w.readers.iter().find(|o| o.id == i).map(|r| {
                let p = Part::Reader(i);
                let wave = r.voice.kind == VoiceKind::Wave;
                let mut sec = Vec::new();
                if wave {
                    sec.push(Sector::action("patch", "edit patch".into(), Act::Patch));
                    sec.push(Sector::num("level", "volume", Key::Voice(p, "level"), 0.0, 1.5, 0.01));
                    sec.push(Sector::num("vel", "vel sense", Key::Voice(p, "velSense"), 0.0, 1.0, 0.01));
                    sec.extend(dial_sectors(&r.voice, p));
                    sec.push(Sector::choice("kind", "voice", Key::VoiceKind(p), strs(&KINDS), Fmt::Kind));
                } else {
                    sec = voice_sectors(w, p, false);
                }
                if r.tines != 0.0 {
                    sec.push(Sector::num("comb", "tines", Key::Reader(i, "tines"), 1.0, 36.0, 1.0));
                    sec.push(Sector::num("fork", "comb note", Key::Reader(i, "combBase"), 12.0, 108.0, 1.0));
                    sec.push(Sector::choice("scale", "comb scale", Key::Reader(i, "combScale"), strs(&SCALES.map(|s| s.0)), Fmt::Plain));
                }
                let name = r.voice.raw.get("name").and_then(|n| n.as_str()).map(|s| s.to_string()).unwrap_or_else(|| kind_label(r.voice.kind.name()));
                let what = if r.tines != 0.0 { "comb" } else { "reader" };
                self.beside(w, ((r.x1, r.y1), (r.x2, r.y2)), sec, s, (format!("{what} {i}"), name))
            }),
            Some(s @ Sel::String(i)) => w.strings.iter().find(|o| o.id == i).map(|o| {
                let k = |key| Key::Str(i, key);
                let sec = vec![
                    Sector::choice("string", "instrument", k("kind"), strs(&["string", "bar"]), Fmt::Plain),
                    Sector::num("fork", "tuning Hz", k("f200"), 20.0, 2000.0, 1.0),
                    Sector::choice("quant", "quantize", k("quant"), nums(&[0.0, 1.0]), Fmt::OnOff),
                    Sector::num("decay", "decay s", k("decay"), 0.0, 8.0, 0.01),
                    Sector::num("damp", "damping", k("damp"), 0.0, 2.0, 0.01),
                    Sector::num("hard", "hardness", k("hard"), 0.0, 1.0, 0.01),
                    Sector::num("level", "level", k("level"), 0.0, 1.5, 0.01),
                    Sector::num("vel", "velocity sense", k("velSense"), 0.0, 1.0, 0.01),
                ];
                self.beside(w, ((o.x1, o.y1), (o.x2, o.y2)), sec, s, (format!("{} {i}", o.kind), note_name(o.midi().round() as i64)))
            }),
            Some(s @ Sel::Cam(i)) => w.cams.iter().find(|o| o.id == i).map(|m| {
                let k = |key| Key::Cam(i, key);
                let mut shapes = cogs_sim::motion::CAM_SHAPE_NAMES.to_vec();
                shapes.push("drawn");
                let link = match m.to {
                    Some(r) => format!("linked to reader {r}: tap to relink"),
                    None => "link to a synth reader".into(),
                };
                let sec = vec![
                    Sector::choice("sweep", "shape", k("shape"), strs(&shapes), Fmt::Plain),
                    Sector::num("odds", "phase", k("phase"), 0.0, 1.0, 0.01),
                    Sector::num("level", "low", k("lo"), 0.0, 1.0, 0.01),
                    Sector::num("level", "high", k("hi"), 0.0, 1.0, 0.01),
                    Sector::step("knob", "macro", k("macro"), 1.0, 8.0, Fmt::Macro),
                    Sector::action("link", link, Act::Link(s)),
                ];
                ring(self, cam_tip(m, &w.cogs[m.cog_ix], w.render_th(m.cog_ix)), 0.0, sec, s)
            }),
            Some(s @ Sel::Probe(i)) => w.probes.iter().find(|o| o.id == i).filter(|pr| w.probe_alive(pr)).map(|pr| {
                let k = |key| Key::Probe(i, key);
                let mut sec = vec![
                    Sector::choice("sweep", "axis", k("axis"), strs(&["y", "x", "reach"]), Fmt::Axis),
                    Sector::num("level", "low", k("lo"), 0.0, 1.0, 0.01),
                    Sector::num("level", "high", k("hi"), 0.0, 1.0, 0.01),
                ];
                let to_kind = pr.to.and_then(|t| w.voice(t)).map(|v| v.kind);
                match to_kind {
                    Some(VoiceKind::Wave) => sec.push(Sector::step("knob", "macro", k("macro"), 1.0, 8.0, Fmt::Macro)),
                    Some(kind) => {
                        let defs: Vec<&str> = cogs_sim::voice::probe_defs(kind).iter().map(|d| d.0).collect();
                        if !defs.is_empty() {
                            sec.push(Sector::choice("knob", "drives", k("param"), strs(&defs), Fmt::Plain));
                        }
                    }
                    None => {}
                }
                let link = match pr.to {
                    Some(t) => format!("drives {} {}: tap to relink", match t { Part::Reader(_) => "reader", Part::Pad(_) => "pad", Part::Pit(_) => "sphere", Part::String(_) => "string" }, t.id()),
                    None => "link to a reader, pad or sphere".into(),
                };
                sec.push(Sector::action("link", link, Act::Link(s)));
                ring(self, w.probe_point(pr), 0.0, sec, s)
            }),
            Some(s @ Sel::Field(i)) => w.fields.iter().find(|o| o.id == i).map(|f| {
                let k = |key| Key::Field(i, key);
                let mut sec = vec![Sector::choice("fxmode", "effect", k("mode"), strs(&["delay", "reverb", "dist"]), Fmt::FxMode)];
                match f.mode.as_str() {
                    "reverb" => {
                        sec.push(Sector::choice("size", "algorithm", k("verbMode"), nums(&(0..20).map(|x| x as f64).collect::<Vec<_>>()), Fmt::Verb));
                        sec.push(Sector::choice("air", "color", k("verbColor"), nums(&[0.0, 1.0, 2.0]), Fmt::Color));
                        sec.push(Sector::num("size", "size", k("size"), 0.0, 1.0, 0.01));
                        sec.push(Sector::num("tail", "decay s", k("decay"), 0.2, 20.0, 0.01));
                        sec.push(Sector::num("attack", "predelay ms", k("predelay"), 0.0, 200.0, 1.0));
                    }
                    "dist" => {
                        sec.push(Sector::choice("bend", "kind", k("kind"), strs(&cogs_sim::field::FIELD_DIST_KINDS), Fmt::Plain));
                        sec.push(Sector::num("hard", "drive", k("drive"), 0.0, 1.0, 0.01));
                        sec.push(Sector::num("tone", "tone", k("tone"), 0.0, 1.0, 0.01));
                    }
                    _ => {
                        sec.push(Sector::choice("tempo", "time", k("sync"), strs(&cogs_sim::field::FIELD_SYNC_OPTS), Fmt::Plain));
                        sec.push(Sector::num("tempo", "free time s", k("time"), 0.02, 2.5, 0.005));
                        sec.push(Sector::num("tail", "feedback", k("fb"), 0.0, 0.95, 0.01));
                        sec.push(Sector::num("tone", "tone", k("tone"), 0.0, 1.0, 0.01));
                        sec.push(Sector::choice("spread", "ping-pong", k("ping"), nums(&[0.0, 1.0]), Fmt::OnOff));
                    }
                }
                sec.push(Sector::num("spread", "diffuse", k("diffuse"), 0.0, 1.0, 0.01));
                sec.push(Sector::num("level", "return", k("level"), 0.0, 1.5, 0.01));
                sec.push(Sector::num("wet", "takes dry", k("insert"), 0.0, 1.0, 0.01));
                ring(self, (f.x, f.y), 0.0, sec, s)
            }),
            Some(s @ Sel::Dot { cog, ring: rg, a }) => w.cog(cog).and_then(|c| c.track.as_ref()).map(|t| {
                let k = |key| Key::Dot(cog, rg, a, key);
                let sec = vec![
                    Sector::num("odds", "probability", k("p"), 0.0, 1.0, 0.01),
                    Sector::num("level", "velocity", k("v"), 0.0, 1.0, 0.01),
                    Sector::num("div", "length steps", k("len"), 1.0, t.div, 1.0),
                ];
                ring(self, self.dot_at, 0.0, sec, s)
            }),
            Some(s @ Sel::Cog(i)) => w.cog(i).filter(|c| c.track.is_some() && !note_mode(c, z)).map(|c| {
                let k = |key| Key::Track(i, key);
                let sec = vec![
                    Sector::choice("scale", "scale", k("scale"), strs(&SCALES.map(|s| s.0)), Fmt::Plain),
                    Sector::step("fork", "root", k("root"), 0.0, 11.0, Fmt::NoteClass),
                    Sector::step("octave", "octave", k("octave"), 1.0, 7.0, Fmt::Plain),
                    Sector::step("range", "range", k("range"), 1.0, 3.0, Fmt::Oct),
                    Sector::num("div", "steps", k("div"), 1.0, 96.0, 1.0),
                ];
                let (t, bt) = (self.safe_top, self.shelf_top());
                let r = (c.radius() * z + 40.0).max(92.0).min(self.cw.min(bt - t) / 2.0 - BEZEL_W - 8.0);
                let sc = self.to_screen(w, c.x, c.y);
                Bezel { cx: sc.0, cy: sc.1, r, sectors: sec, owner: Some(s), bw: BEZEL_W, labels: false, hub: None, leader: None }
            }),
            _ => None,
        };
        let owner = b.as_ref().map(|b| b.owner);
        if owner != self.bezel_owner {
            self.bezel_owner = owner;
            self.bezel_at = self.now;
        }
        b
    }
    /// A compact bezel beside a long part (a reader, a string): the spot near it, on screen, that covers the fewest
    /// other parts, joined to it by a leader.
    fn beside(&self, w: &World, seg: ((f64, f64), (f64, f64)), sectors: Vec<Sector>, owner: Sel, hub: (String, String)) -> Bezel {
        const R: f64 = 58.0;
        const BW: f64 = 26.0;
        let a = self.to_screen(w, seg.0 .0, seg.0 .1);
        let b = self.to_screen(w, seg.1 .0, seg.1 .1);
        let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        let len = dist(a, b).max(1.0);
        let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
        let out = R + BW + 10.0;
        let mut cand = Vec::new();
        for t in [-1.0, 1.0] {
            // past either end, and to either side of the middle and of each end
            cand.push((mid.0 + ux * t * (len / 2.0 + out), mid.1 + uy * t * (len / 2.0 + out)));
            cand.push((mid.0 - uy * t * out, mid.1 + ux * t * out));
            for e in [a, b] {
                cand.push((e.0 - uy * t * out, e.1 + ux * t * out));
            }
        }
        for k in 0..12 {
            let ang = k as f64 * TAU / 12.0;
            cand.push((mid.0 + ang.cos() * (out + 30.0), mid.1 + ang.sin() * (out + 30.0)));
        }
        let (t, bt) = (self.safe_top, self.shelf_top() - 110.0);
        let rr = R + BW;
        let z = w.s.zoom;
        let cost = |c: (f64, f64)| {
            let mut k = 0.0;
            // off the open stage
            let m = rr + 6.0;
            k += 8.0 * ((m - c.0).max(0.0) + (c.0 - (self.cw - m)).max(0.0) + (t + m - c.1).max(0.0) + (c.1 - (bt - m)).max(0.0));
            for cg in &w.cogs {
                let s = self.to_screen(w, cg.x, cg.y);
                k += (rr + cg.radius() * z - dist(c, s)).max(0.0).min(2.0 * rr);
            }
            for p in &w.pads {
                let s = self.to_screen(w, p.x, p.y);
                k += 2.0 * (rr + p.r * z - dist(c, s)).max(0.0);
            }
            for p in &w.pits {
                let s = self.to_screen(w, p.x, p.y);
                k += (rr + p.w / 2.0 * z - dist(c, s)).max(0.0).min(2.0 * rr);
            }
            // its own part, under the ring
            k += 3.0 * (rr + 8.0 - super::seg_dist(c, a.0, a.1, b.0, b.1)).max(0.0);
            k + 0.05 * dist(c, mid)
        };
        let best = cand.into_iter().min_by(|p, q| cost(*p).partial_cmp(&cost(*q)).unwrap()).unwrap_or(mid);
        Bezel { cx: best.0, cy: best.1, r: R, sectors, owner: Some(owner), bw: BW, labels: true, hub: Some(hub), leader: Some(mid) }
    }
    pub(super) fn bezel_hit(b: &Bezel, s: (f64, f64)) -> Option<usize> {
        let d = dist(s, (b.cx, b.cy));
        if (d - b.r).abs() > b.bw + 6.0 || b.sectors.is_empty() {
            return None;
        }
        let a = cogs_sim::wrap((s.1 - b.cy).atan2(s.0 - b.cx) + std::f64::consts::FRAC_PI_2);
        Some(((a / (TAU / b.sectors.len() as f64)) as usize).min(b.sectors.len() - 1))
    }
    /// Keep a new bezel on screen: slide the camera, never zoom.
    pub(super) fn reveal(&mut self, w: &World) {
        let Some(b) = self.active_bezel(w) else { return };
        if b.leader.is_some() {
            return;
        }
        let m = b.r + b.bw + 8.0;
        let dx = if b.cx < m { b.cx - m } else if b.cx > self.cw - m { b.cx - (self.cw - m) } else { 0.0 };
        let (t, bt) = (self.safe_top, self.shelf_top());
        let dy = if b.cy < t + m { b.cy - t - m } else if b.cy > bt - m { b.cy - (bt - m) } else { 0.0 };
        if dx != 0.0 || dy != 0.0 {
            let z = w.s.zoom;
            self.fly_to(w, w.view.x + dx / z, w.view.y + dy / z, z);
        }
    }

    /// The bezel gesture: drag up or down from a sector to turn it; tap a choice to step it, an action to do it.
    pub(super) fn bezel_move(&mut self, w: &mut World, i: usize, v0: &Val, s0: (f64, f64), s: (f64, f64)) {
        let Some(b) = self.active_bezel(w) else { return };
        let Some(p) = b.sectors.get(i).cloned() else { return };
        let Some(key) = p.key.clone() else { return };
        if p.toggle {
            return;
        }
        let dy = s0.1 - s.1;
        let was = get(w, &key);
        let steps = p.steps();
        let nv = if let Some(opts) = &p.options {
            let k = opts.len() as i64;
            let at = opts.iter().position(|o| o == v0).unwrap_or(0) as i64;
            opts[(at + (dy / OPTION_PX).round() as i64).rem_euclid(k) as usize].clone()
        } else if steps > 0.0 {
            let per = (DRAG_PX / steps).max(6.0);
            Val::N((v0.n() + (dy / per).round() * p.step).clamp(p.min, p.max))
        } else {
            Val::N(p.denorm(p.norm(v0.n()) + dy / DRAG_PX))
        };
        if nv != was {
            if set(w, &key, nv) {
                w.solve();
            }
            if p.options.is_some() || steps > 0.0 {
                self.buzz(3);
            }
        }
        self.say(format!("{}: {}", p.label, p.format(&get(w, &key))), true);
    }
    pub(super) fn bezel_tap(&mut self, w: &mut World, i: usize) {
        let Some(b) = self.active_bezel(w) else { return };
        let Some(p) = b.sectors.get(i).cloned() else { return };
        if let Some(a) = &p.action {
            match a {
                Act::Link(o) => {
                    self.linking = Some(*o);
                    self.buzz(6);
                    let say = if matches!(o, Sel::Probe(_)) { "tap a reader, pad or sphere to drive it" } else { "tap a synth reader to link it" };
                    self.say(format!("{} {}: {say}", o.name(), super::shelf::sel_id(*o)), true);
                }
                Act::Share => self.share(w),
                Act::Patch => {
                    self.buzz(10);
                    self.out.push(StageOut::OpenEditor("patch"));
                }
            }
        } else if let (Some(opts), Some(key)) = (&p.options, &p.key) {
            let cur = get(w, key);
            let at = opts.iter().position(|o| *o == cur).unwrap_or(0);
            let nv = opts[(at + 1) % opts.len()].clone();
            if set(w, key, nv) {
                w.solve();
            }
            self.buzz(4);
            self.say(format!("{}: {}", p.label, p.format(&get(w, key))), false);
        }
        self.changed(w, true);
    }
    /// The seal: the machine as a share link (deflate-raw in stored blocks, base64url), handed to the app to copy.
    fn share(&mut self, w: &World) {
        let json = w.to_string();
        let url = format!("https://cogs.techmate.help/#w={}", b64url(&deflate_stored(json.as_bytes())));
        self.seal_at = self.now;
        self.out.push(StageOut::Share(url));
        self.buzz(20);
    }

    pub(super) fn draw_bezel(&mut self, p: &egui::Painter, w: &World) {
        let Some(b) = self.active_bezel(w) else { return };
        let n = b.sectors.len();
        if n == 0 {
            return;
        }
        let span = TAU / n as f64;
        let k = ((self.now - self.bezel_at) / 0.16).clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - k).powi(4);
        let r = b.r * (0.7 + 0.3 * e);
        let ea = e as f32;
        let o = self.rect.min;
        let at = |a: f64, rr: f64| pos2(o.x + (b.cx + a.cos() * rr) as f32, o.y + (b.cy + a.sin() * rr) as f32);
        if let Some(l) = b.leader {
            // the leader, from the ring's edge to the part
            let (dx, dy) = (l.0 - b.cx, l.1 - b.cy);
            let dd = (dx * dx + dy * dy).sqrt().max(1.0);
            let from = (b.cx + dx / dd * (r + b.bw), b.cy + dy / dd * (r + b.bw));
            if dd > r + b.bw {
                p.extend(egui::Shape::dashed_line(&[self.scr(from), self.scr(l)], Stroke::new(1.0, alpha(SEL, 0.7 * ea)), 3.0, 3.0));
                p.circle_filled(self.scr(l), 2.5, alpha(SEL, ea));
            }
            // a disc behind the ring, so it reads over the machine
            p.circle_filled(self.scr((b.cx, b.cy)), (r + b.bw + 1.0) as f32, alpha(BG, 0.92 * ea));
            if let Some((who, what)) = &b.hub {
                p.text(self.scr((b.cx, b.cy - 7.0)), Align2::CENTER_CENTER, who, FontId::proportional(11.0), alpha(DIM, ea));
                let what: String = if what.chars().count() > 14 { what.chars().take(13).collect::<String>() + "…" } else { what.clone() };
                p.text(self.scr((b.cx, b.cy + 8.0)), Align2::CENTER_CENTER, what, FontId::proportional(12.0), alpha(FG, ea));
            }
        }
        let active_i = match &self.gesture {
            Some(Active { g: super::gesture::Gesture::Bezel { i, .. }, .. }) => Some(*i),
            _ => None,
        };
        for (i, sec) in b.sectors.iter().enumerate() {
            let a0 = -std::f64::consts::FRAC_PI_2 + i as f64 * span + 0.025;
            let a1 = a0 + span - 0.05;
            let am = (a0 + a1) / 2.0;
            let active = active_i == Some(i);
            // the wedge
            let m = ((a1 - a0) * (r + b.bw) / 4.0).ceil().max(4.0) as usize;
            let mut mesh = egui::epaint::Mesh::default();
            let patch = matches!(sec.action, Some(Act::Patch));
            let fill = alpha(if active || patch { PAD } else { BG }, ea);
            let mut outline = Vec::new();
            for j in 0..=m {
                let a = a0 + (a1 - a0) * j as f64 / m as f64;
                mesh.colored_vertex(at(a, r - b.bw), fill);
                mesh.colored_vertex(at(a, r + b.bw), fill);
                outline.push(at(a, r + b.bw));
            }
            for j in 0..m as u32 {
                let q = 2 * j;
                mesh.add_triangle(q, q + 1, q + 2);
                mesh.add_triangle(q + 1, q + 3, q + 2);
            }
            p.add(egui::Shape::mesh(mesh));
            for j in (0..=m).rev() {
                outline.push(at(a0 + (a1 - a0) * j as f64 / m as f64, r - b.bw));
            }
            p.add(egui::Shape::closed_line(outline, Stroke::new(if patch { 1.5 } else { 1.0 }, alpha(if patch { HOT } else { FAINT }, ea))));
            let rv = r + b.bw - 5.0;
            let val = sec.key.as_ref().map(|k| get(w, k));
            let mut ink = if sec.toggle { if val.as_ref().map_or(0.0, |v| v.n()) != 0.0 { HOT } else { DIM } } else if active { HOT } else { FG };
            if sec.action.is_some() && matches!(sec.action, Some(Act::Share)) && self.now - self.seal_at < 0.9 {
                ink = HOT;
            }
            if let (Some(opts), Some(v)) = (&sec.options, &val) {
                let mm = opts.len();
                let cur = opts.iter().position(|o| o == v);
                for j in 0..mm {
                    let a = a0 + 0.12 + (a1 - a0 - 0.24) * if mm == 1 { 0.5 } else { j as f64 / (mm - 1) as f64 };
                    let on = cur == Some(j);
                    p.circle_filled(at(a, rv), if on { 2.6 } else { 1.5 }, alpha(if on { HOT } else { DIM }, ea));
                }
            } else if let (None, Some(v)) = (&sec.action, &val) {
                let nv = sec.norm(v.n()).clamp(0.0, 1.0);
                let ae = a0 + 0.08 + (a1 - a0 - 0.16) * nv;
                let arc = |from: f64, to: f64, wd: f32, c: Color32| {
                    let mm = (((to - from).abs() * rv / 3.0).ceil() as usize).max(2);
                    let pts: Vec<Pos2> = (0..=mm).map(|j| at(from + (to - from) * j as f64 / mm as f64, rv)).collect();
                    p.add(egui::Shape::line(pts, Stroke::new(wd, c)));
                };
                arc(a0 + 0.08, a1 - 0.08, 1.0, alpha(DIM, ea));
                arc(a0 + 0.08, ae.max(a0 + 0.081), 2.0, alpha(FG, ea));
                p.circle_filled(at(ae, rv), 2.6, alpha(HOT, ea));
            }
            if b.labels {
                // words: the label, and the value under it (an action: its glyph and its name)
                let c = at(am, r - 2.0);
                if patch {
                    self.icons.paint(p, "MixerVertical", false, Rect::from_center_size(c - vec2(0.0, 7.0), vec2(16.0, 16.0)), alpha(HOT, ea));
                    p.text(c + vec2(0.0, 10.0), Align2::CENTER_CENTER, "patch", FontId::proportional(11.0), alpha(HOT, ea));
                    continue;
                }
                let label: String = sec.label.chars().take(13).collect();
                let size = if label.chars().count() > 9 { 9.0 } else { 10.0 };
                p.text(c - vec2(0.0, 7.0), Align2::CENTER_CENTER, label, FontId::proportional(size), alpha(DIM, ea));
                if let Some(v) = &val {
                    let text: String = sec.format(v).chars().take(9).collect();
                    p.text(c + vec2(0.0, 7.0), Align2::CENTER_CENTER, text, FontId::proportional(12.5), alpha(if active { HOT } else { FG }, ea));
                } else {
                    p.text(c + vec2(0.0, 7.0), Align2::CENTER_CENTER, "tap", FontId::proportional(12.0), alpha(FG, ea));
                }
                continue;
            }
            // the icon
            let c = at(am, r - 4.0);
            let ink = alpha(ink, ea);
            let pen = Pen::new(p, 1.0, c.x as f64, c.y as f64, w.clock.view);
            match sec.icon {
                "kind" => {
                    pen.circle(0.0, 0.0, 8.0, 1.2, ink);
                    if let Some(Val::S(k)) = &val {
                        if let Some(k) = VoiceKind::from_name(k) {
                            emblem(&pen, k, 0.0, 0.0, 8.0, ink, 1.2);
                        }
                    }
                }
                "fxmode" => {
                    pen.circle(0.0, 0.0, 8.0, 1.2, ink);
                    if let Some(Val::S(m)) = &val {
                        field_glyph(&pen, m, 0.0, 0.0, 5.0, 1.2, ink);
                    }
                }
                name => {
                    for line in icon_lines(name) {
                        let pts: Vec<Pos2> = line.iter().map(|&(x, y)| c + vec2((x * 0.75 - 9.0) as f32, (y * 0.75 - 9.0) as f32)).collect();
                        p.add(egui::Shape::line(pts, Stroke::new(1.8 * 0.75, ink)));
                    }
                }
            }
        }
    }
}

/// The patch knobs a synth voice pins to its dial (its raw `dial`: paths like "osc1.cut"), as the synth editor sets them.
fn dial_sectors(v: &cogs_sim::Voice, p: Part) -> Vec<Sector> {
    let Some(Value::Array(d)) = v.raw.get("dial") else { return vec![] };
    let params = cogs_dsp::params::wave_params();
    d.iter().filter_map(|x| x.as_str()).filter_map(|path| cogs_dsp::params::param_index(path).map(|i| (path, i))).take(8).map(|(path, i)| {
        let def = &params[i];
        let int = def.curve == cogs_dsp::params::Curve::Int;
        // the oscillator or section first, so "osc1 cut" reads apart from "osc2 cut"
        let section = path.split('.').next().unwrap_or("");
        let label = if path.contains('.') && section != def.label { format!("{section} {}", def.label) } else { def.label.to_string() };
        let mut s = Sector::num("knob", &label, Key::Dial(p, i), 0.0, 1.0, 0.001);
        s.fmt = Fmt::Dial(i);
        s.log = false;
        if int {
            // an integer knob turns a step at a time
            s.step = 1.0 / (def.max - def.min).max(1.0);
        }
        s
    }).collect()
}

/// The room's bezel: its reverb, the master, tempo, the line amp, and the seal.
fn room_sectors() -> Vec<Sector> {
    let sp = |k| Key::Space(k);
    let cl = |k| Key::Climax(k);
    vec![
        Sector::choice("verb", "reverb mode", sp("mode"), nums(&(0..20).map(|x| x as f64).collect::<Vec<_>>()), Fmt::Verb),
        Sector::choice("era", "reverb color", sp("color"), nums(&[0.0, 1.0, 2.0]), Fmt::Color),
        Sector::num("wet", "wet", sp("wet"), 0.0, 1.5, 0.001),
        Sector::num("size", "size", sp("size"), 0.0, 1.0, 0.01),
        Sector::num("tail", "decay s", sp("decay"), 0.2, 60.0, 0.01),
        Sector::num("air", "high cut Hz", sp("highCut"), 1000.0, 20000.0, 10.0),
        Sector::num("shimmer", "shimmer", sp("shimmer"), 0.0, 1.0, 0.01),
        Sector::choice("freeze", "reverb freeze", sp("freeze"), nums(&[0.0, 1.0]), Fmt::OnOff),
        Sector::num("level", "master", Key::S("master"), 0.0, 1.5, 0.01),
        Sector::choice("quant", "ratio snap", Key::S("ratios"), nums(&[0.0, 1.0]), Fmt::OnOff),
        Sector::num("tempo", "tempo bpm", Key::S("bpm"), 0.0, 300.0, 1.0),
        Sector::num("beats", "beats/bar", Key::S("beats"), 1.0, 16.0, 1.0),
        Sector::choice("hard", "line amp", cl("on"), nums(&[0.0, 1.0]), Fmt::OnOff),
        Sector::num("level", "drive dB", cl("drive"), -6.0, 12.0, 0.1),
        Sector::num("damp", "saturation %", cl("sat"), 0.0, 150.0, 1.0),
        Sector::num("shimmer", "excite", cl("excite"), 0.0, 10.0, 0.1),
        Sector::num("air", "air", cl("air"), 0.0, 10.0, 0.1),
        Sector::action("seal", "share".into(), Act::Share),
    ]
}

/// phone.js ICON: the bezel's glyphs, 24-unit stroked paths.
fn icon_path(k: &str) -> &'static str {
    match k {
        "fork" => "M9 3v8a3 3 0 0 0 6 0V3M12 14v7M9.5 21h5",
        "bell" => "M6 17c0-7 2.5-11 6-11s6 4 6 11zM4 17h16M11 20h2",
        "bend" => "M3 18C10 18 12 6 21 6",
        "bright" => "M12 8a4 4 0 1 1-.01 0M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M19 5l-2 2M7 17l-2 2",
        "hard" => "M12 3l7 9-7 9-7-9z",
        "decay" => "M3 5h3l13 14h2",
        "pdecay" => "M3 5h1l5 14h12",
        "damp" => "M2 12c1.5-7 3-7 4.5 0s3 5 4.5 0 3-3 4.5 0 3 2 4.5 0",
        "level" => "M3 19L21 5v14z",
        "vel" => "M3 12h11M10 8l4 4-4 4M19 4v16",
        "sweep" => "M3 4c1 11 7 15 18 15",
        "tone" => "M2 12c2.5-8 5-8 7.5 0s5 8 7.5 0 3-4 5-4",
        "cutoff" => "M2 8h11c4 0 5 9 9 11",
        "scale" => "M3 20h4v-4h4v-4h4v-4h4V4",
        "div" => "M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0M12 3v18M3 12h18M5.6 5.6l12.8 12.8M18.4 5.6L5.6 18.4",
        "rings" => "M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0M7 12a5 5 0 1 0 10 0a5 5 0 1 0-10 0M11 12a1 1 0 1 0 2 0a1 1 0 1 0-2 0",
        "wet" => "M12 3c4 6 6 9 6 12a6 6 0 0 1-12 0c0-3 2-6 6-12z",
        "size" => "M3 20V10l9-6 9 6v10zM3 20h18",
        "tail" => "M3 20h18M4 20V6M8 20v-9M12 20v-6M16 20v-4M20 20v-2",
        "air" => "M3 9h12a3 3 0 1 0-3-3M3 15h15a3 3 0 1 1-3 3",
        "verb" => "M3 18a9 9 0 0 1 18 0M7 18a5 5 0 0 1 10 0M11 18a1 1 0 0 1 2 0",
        "era" => "M12 3a9 9 0 1 0 .01 0M12 7v5l3 3",
        "shimmer" => "M12 2l2 7 7 3-7 3-2 7-2-7-7-3 7-3z",
        "freeze" => "M12 2v20M3.3 7l17.4 10M3.3 17L20.7 7",
        "tempo" => "M8 21l3-18h2l3 18zM12 16l6-10",
        "beats" => "M5 12h.01M10 12h.01M15 12h.01M20 12h.01",
        "comb" => "M3 5h18M5 5v14M9 5v12M13 5v10M17 5v8M21 5v6",
        "odds" => "M12 3a9 9 0 1 0 .01 0M12 3v9h9",
        "string" => "M3 19L21 5M3 19h.01M21 5h.01",
        "quant" => "M4 20V14M9 20V9M14 20V12M19 20V4",
        "seal" => "M12 3a9 9 0 1 0 .01 0M12 7l1.5 3.2 3.5.4-2.6 2.4.7 3.5-3.1-1.8-3.1 1.8.7-3.5L7 10.6l3.5-.4z",
        "semi" => "M4 14h16M4 10h16M9 5v14M15 5v14",
        "partials" => "M3 20V8M7 20V5M11 20V9M15 20V11M19 20V13M23 20v-5",
        "even" => "M3 20V8M11 20V12M19 20V15",
        "attack" => "M3 20L10 5h11",
        "range" => "M12 3v18M8 7l4-4 4 4M8 17l4 4 4-4",
        "octave" => "M4 18V6M4 12h16M20 18V6",
        "knob" => "M12 4a8 8 0 1 0 .01 0M12 12L16 6",
        "link" => "M10 14a4 4 0 0 1 0-6l3-3a4 4 0 0 1 6 6l-1.5 1.5M14 10a4 4 0 0 1 0 6l-3 3a4 4 0 0 1-6-6l1.5-1.5",
        "spread" => "M4 20V9M8 20V6M12 20V9M16 20V12",
        _ => "M12 4a8 8 0 1 0 .01 0",
    }
}
fn icon_lines(k: &str) -> Vec<Vec<(f64, f64)>> {
    super::icons::parse_path(icon_path(k))
}

/// Deflate-raw with stored (uncompressed) blocks: what DecompressionStream('deflate-raw') reads.
fn deflate_stored(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 65535 * 5 + 5);
    let chunks: Vec<&[u8]> = if data.is_empty() { vec![&[][..]] } else { data.chunks(65535).collect() };
    for (i, c) in chunks.iter().enumerate() {
        out.push(if i + 1 == chunks.len() { 1 } else { 0 });
        let n = c.len() as u16;
        out.extend_from_slice(&n.to_le_bytes());
        out.extend_from_slice(&(!n).to_le_bytes());
        out.extend_from_slice(c);
    }
    out
}
fn b64url(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut s = String::with_capacity(b.len() * 4 / 3 + 4);
    for c in b.chunks(3) {
        let v = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        let k = c.len() + 1;
        for j in 0..k {
            s.push(T[((v >> (18 - 6 * j)) & 63) as usize] as char);
        }
    }
    s
}
