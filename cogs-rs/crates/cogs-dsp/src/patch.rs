//! Wave synth patches (wave.js): the typed patch, `wavePatch()`'s validation of anything loaded, and the factory presets.
//! A loaded patch (a save, an import, a preset) is laid over a fresh init, checked value by value.

use crate::params::*;
use crate::verb::{VERB_COLORS, VERB_MODES};
use serde_json::{json, Map, Value};

#[derive(Clone, Debug, PartialEq)]
pub struct OscSetup {
    pub on: bool,
    /// index into WAVE_TABLES
    pub table: u8,
    /// index into WAVE_FILTERS
    pub filter: u8,
    /// indices into WARP_MODES
    pub warp_mode: u8,
    pub warp2_mode: u8,
    /// the harmonics table's amplitudes (harmonic k+1 at index k)
    pub harm: [f64; WAVE_HARMS],
}
#[derive(Clone, Debug, PartialEq)]
pub struct LfoSetup {
    /// index into LFO_SHAPES
    pub shape: u8,
    pub retrig: bool,
    /// index into LFO_SYNCS (0: off)
    pub sync: u8,
}
/// A mod-matrix route: source through its curve, unipolar or bipolar, times amt, times the aux source (or its inverse).
#[derive(Clone, Debug, PartialEq)]
pub struct ModRoute {
    /// index into WAVE_SOURCES
    pub src: u8,
    /// index into wave_params()
    pub dst: u16,
    pub amt: f64,
    pub bi: bool,
    pub off: bool,
    pub aux: Option<u8>,
    pub aux_inv: bool,
    pub curve: f64,
}

/// Everything a `wave` voice carries in a save, typed. `values` holds every knob of `wave_params()`, by index.
#[derive(Clone, Debug, PartialEq)]
pub struct WavePatch {
    pub name: String,
    pub values: [f64; NP],
    pub osc: [OscSetup; WAVE_OSCS],
    pub lfo: [LfoSetup; WAVE_LFOS],
    /// index into VOICE_MODES
    pub voice_mode: u8,
    /// One envelope for the whole voice, in place of the oscillators' own and ENV1 on the sub and noise.
    pub amp_on: bool,
    pub glide_always: bool,
    pub glide_scale: bool,
    /// the rack's order: indices into WAVE_FX, each once
    pub fx_order: [u8; 9],
    pub fx_on: [bool; 9],
    /// index into DIST_MODES
    pub dist_mode: u8,
    /// index into DELAY_SYNCS
    pub delay_sync: u8,
    pub delay_ping: bool,
    /// indices into VERB_MODES and VERB_COLORS
    pub verb_mode: u8,
    pub verb_color: u8,
    pub macro_names: [String; WAVE_MACROS],
    pub mods: Vec<ModRoute>,
}

/* JavaScript's conversions, so a save reads as the HTML build reads it */
/// `+x`
pub(crate) fn js_num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::Bool(b) => *b as i32 as f64,
        Value::Null => 0.,
        Value::String(s) => { let t = s.trim(); if t.is_empty() { 0. } else { t.parse().unwrap_or(f64::NAN) } }
        Value::Array(a) => match a.len() { 0 => 0., 1 => js_num(&a[0]), _ => f64::NAN },
        Value::Object(_) => f64::NAN,
    }
}
/// `!!x`
pub(crate) fn js_truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => { let x = n.as_f64().unwrap_or(0.); x != 0. && !x.is_nan() }
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}
fn get<'a>(v: &'a Value, k: &str) -> Option<&'a Value> { v.as_object().and_then(|o| o.get(k)) }
fn name_in(list: &[&str], v: Option<&Value>) -> Option<u8> { v.and_then(|v| v.as_str()).and_then(|s| index_in(list, s)).map(|i| i as u8) }
fn sync_in(list: &[(&str, f64)], v: Option<&Value>) -> Option<u8> {
    let key = match v? { Value::String(s) => s.clone(), Value::Number(n) => n.to_string(), _ => return None };
    list.iter().position(|x| x.0 == key).map(|i| i as u8)
}
fn chars(s: &str, n: usize) -> String { s.encode_utf16().take(n).collect::<Vec<u16>>().pipe(|u| String::from_utf16_lossy(&u)) }
trait Pipe: Sized { fn pipe<R>(self, f: impl FnOnce(Self) -> R) -> R { f(self) } }
impl<T> Pipe for T {}

/// Where a knob lives in the save's JSON: osc1.cut is osc[0].cut, env2.a is env[1].a, m3 is macro[2].
enum Home<'a> { List(&'a str, usize, &'a str), Macro(usize), Group(&'a str, &'a str), Fx(&'a str, &'a str), Top(&'a str) }
fn home(path: &str) -> Home<'_> {
    for g in ["osc", "env", "lfo"] {
        if let Some(r) = path.strip_prefix(g) {
            let b = r.as_bytes();
            if b.len() > 2 && b[0].is_ascii_digit() && b[1] == b'.' { return Home::List(g, (b[0] - b'1') as usize, &r[2..]); }
        }
    }
    if let Some(r) = path.strip_prefix('m') { if r.len() == 1 && r.as_bytes()[0].is_ascii_digit() { return Home::Macro((r.as_bytes()[0] - b'1') as usize); } }
    if let Some((g, k)) = path.split_once('.') {
        if ["sub", "noise", "tune", "amp"].contains(&g) { return Home::Group(g, k); }
        if WAVE_FX.contains(&g) { return Home::Fx(g, k); }
    }
    Home::Top(path)
}
fn json_get<'a>(j: &'a Value, path: &str) -> Option<&'a Value> {
    match home(path) {
        Home::List(g, i, k) => get(j, g)?.as_array()?.get(i).and_then(|o| get(o, k)),
        Home::Macro(i) => get(j, "macro")?.as_array()?.get(i),
        Home::Group(g, k) => get(get(j, g)?, k),
        Home::Fx(g, k) => get(get(get(j, "fx")?, g)?, k),
        Home::Top(k) => get(j, k),
    }
}
fn obj(v: &mut Value) -> &mut Map<String, Value> { if !v.is_object() { *v = Value::Object(Map::new()) } v.as_object_mut().unwrap() }
fn json_set(j: &mut Value, path: &str, x: Value) {
    match home(path) {
        Home::List(g, i, k) => { obj(&mut obj(j).entry(g).or_insert(json!([])).as_array_mut().unwrap()[i]).insert(k.into(), x); }
        Home::Macro(i) => { obj(j).entry("macro").or_insert(json!([])).as_array_mut().unwrap()[i] = x; }
        Home::Group(g, k) => { obj(obj(j).entry(g).or_insert(json!({}))).insert(k.into(), x); }
        Home::Fx(g, k) => { obj(obj(obj(j).entry("fx").or_insert(json!({}))).entry(g).or_insert(json!({}))).insert(k.into(), x); }
        Home::Top(k) => { obj(j).insert(k.into(), x); }
    }
}

impl Default for WavePatch { fn default() -> Self { Self::init() } }

impl WavePatch {
    /// waveInit(): a saw on A through an open low-pass, every knob at its default.
    pub fn init() -> Self {
        let mut values = [0.; NP];
        for (i, p) in wave_params().iter().enumerate() { values[i] = p.default; }
        values[O_POS] = 2. / 3.; // a saw, as a synth's init patch starts
        let osc = |on| OscSetup { on, table: 0, filter: 2, warp_mode: 1, warp2_mode: 0, harm: std::array::from_fn(|k| if k == 0 { 1. } else { 0. }) };
        WavePatch {
            name: "init".into(), values, osc: [osc(true), osc(false), osc(false)],
            lfo: std::array::from_fn(|_| LfoSetup { shape: 0, retrig: true, sync: 0 }),
            voice_mode: 0, glide_always: true, glide_scale: false,
            amp_on: false, fx_order: std::array::from_fn(|i| i as u8), fx_on: [false; 9], dist_mode: 0,
            delay_sync: index_in(&DELAY_SYNCS.map(|x| x.0), "1/8.").unwrap() as u8, delay_ping: true, verb_mode: 0, verb_color: 1,
            macro_names: std::array::from_fn(|i| format!("macro {}", i + 1)), mods: Vec::new(),
        }
    }
    /// wavePatch(j): anything loaded, laid over a fresh init and checked value by value. Never fails.
    pub fn from_json(j: &Value) -> Self {
        let mut p = Self::init();
        if !j.is_object() { return p; }
        if let Some(n) = get(j, "name").and_then(|v| v.as_str()) { p.name = chars(n, 40); }
        for (i, d) in wave_params().iter().enumerate() {
            if let Some(v) = json_get(j, &d.path) { let x = js_num(v); if x.is_finite() { p.values[i] = x.clamp(d.min, d.max); } }
        }
        if let Some(list) = get(j, "osc").and_then(|v| v.as_array()) {
            for (i, o) in list.iter().take(WAVE_OSCS).enumerate() {
                if !js_truthy(o) { continue; }
                let q = &mut p.osc[i];
                q.on = get(o, "on").map_or(false, js_truthy);
                if let Some(t) = name_in(&WAVE_TABLES, get(o, "table")) { q.table = t; }
                if let Some(t) = name_in(&WAVE_FILTERS, get(o, "filter")) { q.filter = t; }
                if let Some(t) = name_in(&WARP_MODES, get(o, "warpMode")) { q.warp_mode = t; }
                if let Some(t) = name_in(&WARP_MODES, get(o, "warp2Mode")) { q.warp2_mode = t; }
                if let Some(h) = get(o, "harm").and_then(|v| v.as_array()) {
                    q.harm = std::array::from_fn(|k| { let x = h.get(k).map_or(0., js_num); if x.is_nan() { 0. } else { x.clamp(0., 1.) } });
                }
            }
        }
        if let Some(list) = get(j, "lfo").and_then(|v| v.as_array()) {
            for (i, l) in list.iter().take(WAVE_LFOS).enumerate() {
                if !js_truthy(l) { continue; }
                if let Some(s) = name_in(&LFO_SHAPES, get(l, "shape")) { p.lfo[i].shape = s; }
                p.lfo[i].retrig = get(l, "retrig") != Some(&Value::Bool(false));
                if let Some(s) = sync_in(&LFO_SYNCS, get(l, "sync")) { p.lfo[i].sync = s; }
            }
        }
        if let Some(vc) = get(j, "voicing") {
            if let Some(m) = name_in(&VOICE_MODES, get(vc, "mode")) { p.voice_mode = m; }
            if let Some(a) = get(vc, "always") { p.glide_always = js_truthy(a); }
            p.glide_scale = get(vc, "scale").map_or(false, js_truthy);
        }
        p.amp_on = get(j, "amp").and_then(|a| get(a, "on")).map_or(false, js_truthy);
        p.fx_order = fx_order(get(j, "fxOrder"));
        if let Some(fx) = get(j, "fx") {
            for (k, name) in WAVE_FX.iter().enumerate() {
                let Some(q) = get(fx, name) else { continue };
                if !js_truthy(q) { continue; }
                p.fx_on[k] = get(q, "on").map_or(false, js_truthy);
                if k == FX_DIST { if let Some(m) = name_in(&DIST_MODES, get(q, "mode")) { p.dist_mode = m; } }
                if k == FX_DELAY {
                    if let Some(s) = sync_in(&DELAY_SYNCS, get(q, "sync")) { p.delay_sync = s; }
                    p.delay_ping = get(q, "ping") != Some(&Value::Bool(false));
                }
                if k == FX_REVERB {
                    if let Some(m) = name_in(&VERB_MODES, get(q, "mode")) { p.verb_mode = m; }
                    if let Some(c) = name_in(&VERB_COLORS, get(q, "color")) { p.verb_color = c; }
                }
            }
        }
        if let Some(n) = get(j, "macroNames").and_then(|v| v.as_array()) {
            for (i, s) in n.iter().take(WAVE_MACROS).enumerate() { if let Some(s) = s.as_str() { p.macro_names[i] = chars(s, 16); } }
        }
        if let Some(mods) = get(j, "mods").and_then(|v| v.as_array()) {
            let num = |m: &Value, k: &str| { let x = get(m, k).map_or(f64::NAN, js_num); if x.is_nan() || x == 0. { 0. } else { x.clamp(-1., 1.) } };
            for m in mods {
                if !js_truthy(m) { continue; }
                let (Some(src), Some(dst)) = (name_in(&WAVE_SOURCES, get(m, "src")), get(m, "dst").and_then(|v| v.as_str()).and_then(param_index)) else { continue };
                p.mods.push(ModRoute { src, dst: dst as u16, amt: num(m, "amt"), bi: get(m, "bi").map_or(false, js_truthy), off: get(m, "off").map_or(false, js_truthy),
                    aux: name_in(&WAVE_SOURCES, get(m, "aux")), aux_inv: get(m, "auxInv").map_or(false, js_truthy), curve: num(m, "curve") });
            }
        }
        p
    }
    pub fn from_str(s: &str) -> Result<Self, serde_json::Error> { Ok(Self::from_json(&serde_json::from_str(s)?)) }

    /// The patch as the HTML build saves it (patchOf(v): no part-level keys).
    pub fn to_json(&self) -> Value {
        let mut j = json!({"name": self.name, "osc": [], "sub": {}, "noise": {}, "tune": {}, "amp": {}, "env": [{}, {}, {}, {}], "lfo": [], "macro": [0, 0, 0, 0, 0, 0, 0, 0],
            "macroNames": self.macro_names, "mods": [], "fx": {}});
        for o in &self.osc {
            j["osc"].as_array_mut().unwrap().push(json!({"on": o.on, "table": WAVE_TABLES[o.table as usize], "filter": WAVE_FILTERS[o.filter as usize],
                "warpMode": WARP_MODES[o.warp_mode as usize], "warp2Mode": WARP_MODES[o.warp2_mode as usize], "harm": o.harm.to_vec()}));
        }
        for l in &self.lfo {
            j["lfo"].as_array_mut().unwrap().push(json!({"shape": LFO_SHAPES[l.shape as usize], "retrig": l.retrig, "sync": LFO_SYNCS[l.sync as usize].0}));
        }
        j["voicing"] = json!({"mode": VOICE_MODES[self.voice_mode as usize], "always": self.glide_always, "scale": self.glide_scale});
        j["amp"]["on"] = json!(self.amp_on);
        j["fxOrder"] = json!(self.fx_order.iter().map(|&k| WAVE_FX[k as usize]).collect::<Vec<_>>());
        for (k, name) in WAVE_FX.iter().enumerate() {
            let mut q = json!({"on": self.fx_on[k]});
            if k == FX_DIST { q["mode"] = json!(DIST_MODES[self.dist_mode as usize]); }
            if k == FX_DELAY { q["sync"] = json!(DELAY_SYNCS[self.delay_sync as usize].0); q["ping"] = json!(self.delay_ping); }
            if k == FX_REVERB { q["mode"] = json!(VERB_MODES[self.verb_mode as usize]); q["color"] = json!(VERB_COLORS[self.verb_color as usize]); }
            j["fx"][*name] = q;
        }
        for (i, d) in wave_params().iter().enumerate() { json_set(&mut j, &d.path, json!(self.values[i])); }
        for m in &self.mods {
            j["mods"].as_array_mut().unwrap().push(json!({"src": WAVE_SOURCES[m.src as usize], "dst": wave_params()[m.dst as usize].path, "amt": m.amt,
                "bi": m.bi, "off": m.off, "aux": m.aux.map_or("", |a| WAVE_SOURCES[a as usize]), "auxInv": m.aux_inv, "curve": m.curve}));
        }
        j
    }

    pub fn get(&self, path: &str) -> Option<f64> { param_index(path).map(|i| self.values[i]) }
    /// Set a knob by path, clamped to its range. Unknown paths are ignored.
    pub fn set(&mut self, path: &str, x: f64) { if let Some(i) = param_index(path) { let d = &wave_params()[i]; self.values[i] = x.clamp(d.min, d.max); } }
    fn osc_set(&mut self, i: usize, kv: &[(&str, f64)]) { for (k, x) in kv { self.set(&format!("osc{}.{k}", i + 1), *x); } }
    fn route(src: &str, dst: &str, amt: f64, bi: bool) -> ModRoute {
        ModRoute { src: index_in(&WAVE_SOURCES, src).unwrap() as u8, dst: param_index(dst).unwrap() as u16, amt, bi, off: false, aux: None, aux_inv: false, curve: 0. }
    }
    fn table(name: &str) -> u8 { index_in(&WAVE_TABLES, name).unwrap() as u8 }

    /// A factory preset by name (WAVE_PRESET_NAMES).
    pub fn preset(name: &str) -> Option<Self> {
        let mut p = Self::init();
        match name {
            "init" => {}
            "super saw" => {
                p.osc_set(0, &[("pos", 0.667), ("uni", 7.), ("detune", 0.45), ("spread", 1.), ("cut", 6000.), ("res", 0.15), ("famt", 0.25), ("fd", 0.9), ("fs", 0.4)]);
                p.osc[1].on = true; p.osc_set(1, &[("pos", 0.667), ("oct", -1.), ("level", 0.45), ("uni", 3.), ("detune", 0.3), ("cut", 4000.)]);
                p.set("sub.level", 0.3); p.fx_on[FX_CHORUS] = true; p.fx_on[FX_DELAY] = true; p.set("delay.mix", 0.18);
            }
            "pluck" => {
                p.osc_set(0, &[("pos", 0.667), ("d", 0.35), ("s", 0.), ("r", 0.25), ("cut", 400.), ("res", 0.3), ("famt", 0.55), ("fd", 0.25), ("fs", 0.)]);
                p.osc[1].on = true; p.osc_set(1, &[("pos", 1.), ("semi", 12.), ("level", 0.25), ("d", 0.2), ("s", 0.), ("r", 0.2), ("cut", 900.), ("famt", 0.5), ("fd", 0.15), ("fs", 0.)]);
                p.mods = vec![Self::route("vel", "osc1.cut", 0.3, false)];
            }
            "pad" => {
                p.set("gate", 1.2); p.osc[0].table = Self::table("vocal"); p.osc[0].filter = 1;
                p.osc_set(0, &[("pos", 0.2), ("uni", 5.), ("detune", 0.35), ("a", 0.9), ("d", 1.5), ("s", 0.8), ("r", 2.2), ("cut", 2500.)]);
                p.osc[1].on = true; p.osc[1].table = Self::table("sweep");
                p.osc_set(1, &[("pos", 0.6), ("oct", 1.), ("level", 0.3), ("uni", 3.), ("a", 1.6), ("r", 2.5), ("cut", 3500.)]);
                p.lfo[0].shape = 1; p.set("lfo1.rate", 0.15); p.set("lfo2.rate", 0.4);
                p.mods = vec![Self::route("lfo1", "osc1.pos", 0.6, false), Self::route("lfo2", "osc2.cut", 0.25, true)];
                p.fx_on[FX_CHORUS] = true; p.fx_on[FX_DELAY] = true; p.delay_sync = index_in(&DELAY_SYNCS.map(|x| x.0), "1/4.").unwrap() as u8; p.set("delay.fb", 0.55);
            }
            "reese bass" => {
                p.osc_set(0, &[("pos", 0.667), ("oct", -1.), ("uni", 2.), ("detune", 0.3), ("spread", 0.3), ("cut", 900.), ("res", 0.2), ("drive", 0.4)]);
                p.osc[1].on = true; p.osc_set(1, &[("pos", 0.667), ("oct", -1.), ("fine", 14.), ("level", 0.6), ("cut", 900.)]);
                p.set("sub.level", 0.5); p.set("sub.oct", -1.); p.set("lfo1.rate", 0.3);
                p.mods = vec![Self::route("lfo1", "osc1.cut", 0.25, true)];
            }
            "wobble" => {
                p.osc_set(0, &[("pos", 0.9), ("oct", -1.), ("cut", 250.), ("res", 0.55), ("drive", 0.5)]); p.set("sub.level", 0.5); p.set("gate", 0.6);
                p.set("lfo1.rate", 3.); p.lfo[0].shape = 1; p.macro_names[0] = "wobble".into(); p.set("m1", 0.7);
                p.lfo[0].sync = LFO_SYNCS.iter().position(|x| x.0 == "1/8").unwrap() as u8; p.fx_on[FX_DIST] = true; p.set("dist.drive", 0.45); p.fx_on[FX_COMP] = true;
                p.mods = vec![Self::route("lfo1", "osc1.cut", 0.55, false), Self::route("m1", "dist.drive", 0.5, false)];
            }
            "additive bell" => {
                p.osc[0].table = Self::table("harmonics");
                let h = [1., 0., 0.6, 0., 0., 0.4, 0., 0.25, 0., 0., 0.3, 0., 0., 0., 0.2, 0., 0., 0., 0., 0.15, 0., 0., 0., 0., 0., 0., 0., 0., 0., 0., 0.1, 0.];
                p.osc[0].harm = h; p.osc_set(0, &[("d", 2.5), ("s", 0.), ("r", 2.)]); p.osc[0].filter = 0;
                p.osc[1].on = true; p.osc[1].table = Self::table("organ"); p.osc_set(1, &[("pos", 0.5), ("oct", 2.), ("level", 0.2), ("d", 0.6), ("s", 0.), ("r", 0.6)]); p.osc[1].filter = 0;
                p.set("gate", 0.1);
            }
            "formant lead" => {
                p.osc[0].table = Self::table("vocal"); p.osc_set(0, &[("uni", 3.), ("detune", 0.2), ("cut", 5000.)]); p.set("glide", 0.08); p.set("gate", 0.4);
                p.set("lfo1.rate", 5.); p.mods = vec![Self::route("env2", "osc1.pos", 1., false), Self::route("lfo1", "osc1.fine", 0.04, true)];
                p.set("env2.a", 0.01); p.set("env2.d", 0.8); p.set("env2.s", 0.2);
            }
            _ => return None,
        }
        if name != "init" { p.name = name.into(); }
        Some(p)
    }
}
pub const WAVE_PRESET_NAMES: [&str; 8] = ["init", "super saw", "pluck", "pad", "reese bass", "wobble", "additive bell", "formant lead"];

/// A rack order: the save's own list (known modules, each once), any module it leaves out keeping its place at the end.
fn fx_order(o: Option<&Value>) -> [u8; 9] {
    let mut out = Vec::with_capacity(9);
    if let Some(a) = o.and_then(|v| v.as_array()) {
        for v in a { if let Some(k) = v.as_str().and_then(|s| index_in(&WAVE_FX, s)) { if !out.contains(&(k as u8)) { out.push(k as u8); } } }
    }
    for k in 0..9u8 { if !out.contains(&k) { out.push(k); } }
    std::array::from_fn(|i| out[i])
}

impl serde::Serialize for WavePatch {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> { self.to_json().serialize(s) }
}
impl<'de> serde::Deserialize<'de> for WavePatch {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> { Ok(Self::from_json(&Value::deserialize(d)?)) }
}
