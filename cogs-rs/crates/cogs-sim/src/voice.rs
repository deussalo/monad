//! Voices as the sim sees them: their kind, level and pitch keys, and the per-kind defaults and knob ranges sim.js
//! keeps in VOICES and VOICE_DEFS (a probe drives those knobs).

use crate::model::{Voice, VoiceKind};
use serde_json::{json, Map, Value};

/// sim.js VOICES: each kind's defaults (the wave synth's patch is cogs-dsp's).
pub fn defaults(kind: VoiceKind) -> Value {
    match kind {
        VoiceKind::Synth => json!({"note":60,"octave":0,"semi":0,"body":"bell","partials":8,"bright":0.6,"even":1,"bend":1,"hard":0.6,"attack":0,"decay":1.2,"damp":0.8}),
        VoiceKind::Kick => json!({"tune":52,"sweep":3,"pdecay":0.045,"decay":0.35}),
        VoiceKind::Snare => json!({"tune":190,"tone":0.35,"cutoff":1800,"decay":0.16}),
        VoiceKind::Hat => json!({"cutoff":7500,"decay":0.045}),
        VoiceKind::Tom => json!({"tune":110,"sweep":1.6,"pdecay":0.08,"tone":0.15,"decay":0.4}),
        VoiceKind::Clap => json!({"cutoff":1300,"spread":0.011,"decay":0.25}),
        VoiceKind::Rim => json!({"tune":1700,"decay":0.06}),
        VoiceKind::Cowbell => json!({"tune":560,"decay":0.3}),
        VoiceKind::Wood => json!({"tune":2200,"decay":0.04}),
        VoiceKind::Cymbal => json!({"cutoff":5500,"tone":0.4,"decay":1.4}),
        VoiceKind::Wave => json!({"note":60,"octave":0,"semi":0}),
    }
}

/// A numeric knob a probe can drive: [key, label, min, max].
pub type Def = (&'static str, &'static str, f64, f64);

/// VOICE_DEFS' numeric rows past the pitch keys (note, octave, semi), then COMMON_DEFS: what `probeDefs` lists.
pub fn probe_defs(kind: VoiceKind) -> &'static [Def] {
    const SYNTH: &[Def] = &[("partials", "partials", 1.0, 12.0), ("bright", "bright", 0.0, 1.0), ("even", "even partials", 0.0, 1.0),
        ("bend", "bend", 0.0, 1.0), ("hard", "hard", 0.0, 1.0), ("attack", "attack s", 0.0, 1.0), ("decay", "decay s", 0.02, 8.0),
        ("damp", "damp", 0.0, 2.0), LEVEL, VEL];
    const KICK: &[Def] = &[("tune", "tune Hz", 20.0, 200.0), ("sweep", "sweep x", 1.0, 10.0), ("pdecay", "p decay s", 0.005, 0.3),
        ("decay", "decay s", 0.02, 2.0), LEVEL, VEL];
    const SNARE: &[Def] = &[("tune", "tune Hz", 60.0, 600.0), ("tone", "tone", 0.0, 1.0), ("cutoff", "cutoff Hz", 200.0, 12000.0),
        ("decay", "decay s", 0.02, 1.0), LEVEL, VEL];
    const HAT: &[Def] = &[("cutoff", "cutoff Hz", 1000.0, 16000.0), ("decay", "decay s", 0.005, 1.0), LEVEL, VEL];
    const TOM: &[Def] = &[("tune", "tune Hz", 40.0, 400.0), ("sweep", "sweep x", 1.0, 4.0), ("pdecay", "p decay s", 0.005, 0.3),
        ("tone", "tone", 0.0, 1.0), ("decay", "decay s", 0.02, 2.0), LEVEL, VEL];
    const CLAP: &[Def] = &[("cutoff", "cutoff Hz", 300.0, 6000.0), ("spread", "spread s", 0.003, 0.04), ("decay", "decay s", 0.05, 1.0), LEVEL, VEL];
    const RIM: &[Def] = &[("tune", "tune Hz", 400.0, 4000.0), ("decay", "decay s", 0.01, 0.5), LEVEL, VEL];
    const COWBELL: &[Def] = &[("tune", "tune Hz", 200.0, 1200.0), ("decay", "decay s", 0.02, 2.0), LEVEL, VEL];
    const WOOD: &[Def] = &[("tune", "tune Hz", 500.0, 5000.0), ("decay", "decay s", 0.01, 0.5), LEVEL, VEL];
    const CYMBAL: &[Def] = &[("cutoff", "cutoff Hz", 1000.0, 12000.0), ("tone", "tone", 0.0, 1.0), ("decay", "decay s", 0.1, 4.0), LEVEL, VEL];
    const WAVE: &[Def] = &[LEVEL, VEL];
    const LEVEL: Def = ("level", "level", 0.0, 1.5);
    const VEL: Def = ("velSense", "vel sense", 0.0, 1.0);
    match kind {
        VoiceKind::Synth => SYNTH,
        VoiceKind::Kick => KICK,
        VoiceKind::Snare => SNARE,
        VoiceKind::Hat => HAT,
        VoiceKind::Tom => TOM,
        VoiceKind::Clap => CLAP,
        VoiceKind::Rim => RIM,
        VoiceKind::Cowbell => COWBELL,
        VoiceKind::Wood => WOOD,
        VoiceKind::Cymbal => CYMBAL,
        VoiceKind::Wave => WAVE,
    }
}
const PROBE_PREFER: [&str; 4] = ["cutoff", "tune", "bright", "decay"];
/// probeDef: the knob named `param`, else the first preferred one the voice has, else its first.
pub fn probe_def(kind: VoiceKind, param: Option<&str>) -> Option<Def> {
    let defs = probe_defs(kind);
    if let Some(p) = param {
        if let Some(d) = defs.iter().find(|d| d.0 == p) {
            return Some(*d);
        }
    }
    if let Some(k) = PROBE_PREFER.iter().find(|k| defs.iter().any(|e| e.0 == **k)) {
        if let Some(d) = defs.iter().find(|d| d.0 == *k) {
            return Some(*d);
        }
    }
    defs.first().copied()
}
/// defAt: a knob's value at fraction y of its range, logarithmic over a wide positive range.
pub fn def_at(d: Def, y: f64) -> f64 {
    let (_, _, a, b) = d;
    if a > 0.0 && b / a >= 20.0 { a * (b / a).powf(y) } else { a + (b - a) * y }
}

/// JS unary `+x`.
pub fn js_num(v: Option<&Value>) -> f64 {
    match v {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(b)) => *b as i32 as f64,
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Value::String(s)) => {
            let t = s.trim();
            if t.is_empty() { 0.0 } else { t.parse::<f64>().unwrap_or(f64::NAN) }
        }
        Some(Value::Array(a)) => match a.len() {
            0 => 0.0,
            1 => js_num(Some(&a[0])),
            _ => f64::NAN,
        },
        Some(Value::Object(_)) => f64::NAN,
    }
}
/// JS truthiness.
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|x| x != 0.0 && !x.is_nan()).unwrap_or(false),
        Some(Value::String(s)) => !s.is_empty(),
        _ => true,
    }
}

impl Voice {
    /// A fresh voice of a kind with the part's common keys (sim.js makeVoice). A wave voice is named by the `pluck`
    /// factory preset, as a new reader's is: cogs-dsp builds the patch from `preset` when the voice has no `osc`.
    pub fn new(kind: VoiceKind) -> Voice {
        let mut raw = Map::new();
        raw.insert("level".into(), json!(0.7));
        raw.insert("velSense".into(), json!(0.7));
        if kind == VoiceKind::Wave {
            raw.insert("preset".into(), json!("pluck"));
        }
        if let Value::Object(d) = defaults(kind) {
            raw.extend(d);
        }
        raw.insert("kind".into(), json!(kind.name()));
        Voice::from_raw(kind, raw)
    }
    /// The voice a new reader or sphere takes: the wave synth's pluck.
    pub fn reader() -> Voice {
        Voice::new(VoiceKind::Wave)
    }
    /// sim.js importedVoice: an unknown kind is an error; a drum or modal voice takes its kind's defaults under its
    /// own keys; a wave voice takes the part keys' defaults and folds a part octave/semitone into its patch's tune.
    pub fn import(v: &Value, owner: u32) -> Result<Voice, String> {
        let o = v.as_object().ok_or_else(|| format!("{owner}: voice is not an object"))?;
        let kname = o.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        let kind = VoiceKind::from_name(kname).ok_or_else(|| format!("{owner}: voice kind {kname}"))?;
        let mut raw = Map::new();
        if kind == VoiceKind::Wave {
            for (k, d) in [("kind", json!("wave")), ("note", json!(60)), ("octave", json!(0)), ("semi", json!(0)), ("level", json!(0.7)), ("velSense", json!(0.7))] {
                raw.insert(k.into(), d);
            }
            for (k, x) in o {
                raw.insert(k.clone(), x.clone());
            }
            let oct = crate::model::to_int32(js_num(raw.get("octave"))) as f64;
            let semi = crate::model::to_int32(js_num(raw.get("semi"))) as f64;
            if oct != 0.0 || semi != 0.0 {
                if let Some(Value::Object(t)) = raw.get_mut("tune") {
                    let to = js_num(t.get("oct"));
                    let ts = js_num(t.get("semi"));
                    t.insert("oct".into(), num(crate::model::clamp(to + oct, -4.0, 4.0)));
                    t.insert("semi".into(), num(crate::model::clamp(ts + semi, -12.0, 12.0)));
                    raw.insert("octave".into(), json!(0));
                    raw.insert("semi".into(), json!(0));
                }
            }
        } else {
            raw.insert("level".into(), json!(0.7));
            raw.insert("velSense".into(), json!(0.7));
            if let Value::Object(d) = defaults(kind) {
                raw.extend(d);
            }
            raw.insert("kind".into(), json!(kind.name()));
            for (k, x) in o {
                raw.insert(k.clone(), x.clone());
            }
        }
        Ok(Voice::from_raw(kind, raw))
    }
    pub fn from_raw(kind: VoiceKind, raw: Map<String, Value>) -> Voice {
        let mut v = Voice { kind, raw, level: 0.0, vel_sense: 0.0, note: 0.0, octave: 0.0, semi: 0.0, ver: 0 };
        v.recache();
        v
    }
    /// Re-read the cached keys after `raw` changed, and bump `ver`.
    pub fn recache(&mut self) {
        let f = |k: &str| js_num(self.raw.get(k));
        self.level = f("level");
        self.vel_sense = f("velSense");
        self.note = f("note");
        self.octave = f("octave");
        self.semi = f("semi");
        if let Some(k) = self.raw.get("kind").and_then(|k| k.as_str()).and_then(VoiceKind::from_name) {
            self.kind = k;
        }
        self.ver = self.ver.wrapping_add(1);
    }
    /// Set one key of the voice.
    pub fn set(&mut self, key: &str, value: Value) {
        self.raw.insert(key.into(), value);
        self.recache();
    }
    pub fn get(&self, key: &str) -> f64 {
        js_num(self.raw.get(key))
    }
    /// sim.js voiceMidi: a note as this voice sounds it (the modal and wave synths transpose by their part keys).
    pub fn midi(&self, midi: f64) -> f64 {
        let t = if matches!(self.kind, VoiceKind::Synth | VoiceKind::Wave) { 12.0 * self.octave + self.semi } else { 0.0 };
        crate::model::clamp(midi + t, 0.0, 127.0)
    }
    pub fn to_json(&self) -> Value {
        Value::Object(self.raw.clone())
    }
}

/// A number as JSON: whole numbers as integers, as JSON.stringify writes them.
pub fn num(x: f64) -> Value {
    if x.is_finite() && x == x.trunc() && x.abs() < 9.0e15 {
        Value::from(x as i64)
    } else if x.is_finite() {
        Value::from(x)
    } else {
        Value::Null
    }
}
