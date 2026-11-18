//! The world, and its persistence: `serialize` / `deserialize` / `migrateSpace` / `climaxIn` from sim.js, with the
//! cam, pit, probe and field parts of each from their own files.

use crate::model::*;
use crate::voice::{js_num, num, truthy};
use serde_json::{json, Map, Value};
use std::collections::VecDeque;

/// The machine. Parts refer to each other by id; after changing the part lists directly, call `solve()` (which
/// relinks) before `advance()`.
#[derive(Clone, Debug, Default)]
pub struct World {
    pub s: Settings,
    /// The room's reverb settings (cogs-dsp's): migrated and clamped on load.
    pub space: Map<String, Value>,
    /// The master line amp's settings (cogs-dsp's): clamped on load.
    pub climax: Map<String, Value>,
    pub view: View,
    pub cogs: Vec<Cog>,
    pub arms: Vec<Arm>,
    pub pads: Vec<Pad>,
    pub readers: Vec<Reader>,
    pub belts: Vec<Belt>,
    pub strings: Vec<Str>,
    pub cams: Vec<Cam>,
    pub pits: Vec<Pit>,
    pub probes: Vec<Probe>,
    pub fields: Vec<Field>,
    pub next: u32,
    pub clock: Clock,
    pub stats: Stats,
    pub(crate) scratch: crate::sim::Scratch,
    pub(crate) rng: u64,
}

// ---------- the room ----------
/// SPACE_DEFS: dry, then verb.js VERB_DEFS: [key, min, max].
pub const SPACE_DEFS: [(&str, f64, f64); 23] = [
    ("dry", 0.0, 1.0), ("mode", 0.0, 19.0), ("color", 0.0, 2.0), ("wet", 0.0, 1.5), ("predelay", 0.0, 500.0), ("decay", 0.2, 60.0),
    ("size", 0.0, 1.0), ("attack", 0.0, 1.0), ("early", 0.0, 1.0), ("late", 0.0, 1.0), ("modRate", 0.05, 10.0), ("modDepth", 0.0, 1.0),
    ("highCut", 1000.0, 20000.0), ("lowCut", 10.0, 1000.0), ("bassMult", 0.25, 4.0), ("bassXover", 50.0, 2000.0), ("highShelf", -24.0, 0.0),
    ("highFreq", 500.0, 16000.0), ("earlyLate", 0.0, 1.0), ("width", 0.0, 1.5), ("shimmer", 0.0, 1.0), ("shimmerPitch", -12.0, 24.0),
    ("freeze", 0.0, 1.0),
];
/// sim.js SPACE as it starts: VERB_DEFAULT under the room's own.
pub fn space_default() -> Map<String, Value> {
    let v = json!({"dry":1,"mode":0,"color":1,"wet":0.26,"predelay":8,"decay":2,"size":0.55,"attack":0.15,"early":0.75,"late":0.7,
        "modRate":0.6,"modDepth":0.5,"highCut":9000,"lowCut":80,"bassMult":1.2,"bassXover":400,"highShelf":-4,"highFreq":5000,
        "earlyLate":0.6,"width":1,"shimmer":0,"shimmerPitch":12,"freeze":0});
    match v {
        Value::Object(m) => m,
        _ => unreachable!(),
    }
}
/// migrateSpace: a save from before this reverb maps its old keys across; every key is clamped to its range.
pub fn migrate_space(j: Option<&Value>) -> Map<String, Value> {
    let mut out = Map::new();
    let Some(Value::Object(j)) = j else { return out };
    let mut o = j.clone();
    let old = ["highMult", "lowMult", "density", "diffusion", "duck", "lowCross", "highCross"];
    if !j.contains_key("mode") && old.iter().any(|k| j.contains_key(*k)) {
        let n = |k: &str| {
            let x = js_num(j.get(k));
            if x.is_finite() { Some(x) } else { None }
        };
        o = space_default();
        for k in ["dry", "wet", "predelay", "decay", "modRate", "shimmer", "shimmerPitch", "freeze", "width", "lowCut", "highCut"] {
            if let Some(x) = n(k) {
                o.insert(k.into(), num(x));
            }
        }
        if let Some(x) = n("size") {
            o.insert("size".into(), Value::from((js_max(0.2, x) / 0.2).ln() / 15f64.ln()));
        }
        if let Some(x) = n("highMult") {
            o.insert("highShelf".into(), Value::from(20.0 * js_max(1e-3, x).log10()));
        }
        if let Some(x) = n("lowMult") {
            o.insert("bassMult".into(), num(x));
        }
        if let Some(x) = n("lowCross") {
            o.insert("bassXover".into(), num(x));
        }
        if let Some(x) = n("highCross") {
            o.insert("highFreq".into(), num(x));
        }
        if let Some(x) = n("diffusion") {
            o.insert("early".into(), num(x));
        }
        if let Some(x) = n("density") {
            o.insert("late".into(), num(x));
        }
        if let Some(x) = n("modDepth") {
            o.insert("modDepth".into(), Value::from(x / 1.5));
        }
    }
    for (k, lo, hi) in SPACE_DEFS {
        let x = js_num(o.get(k));
        if x.is_finite() {
            out.insert(k.into(), num(clamp(x, lo, hi)));
        }
    }
    out
}
/// climax.js CLIMAX_DEFS: [key, min, max].
const CLIMAX_DEFS: [(&str, f64, f64); 13] = [
    ("on", 0.0, 1.0), ("drive", -6.0, 12.0), ("sat", 0.0, 150.0), ("classA", 0.0, 1.0), ("excite", 0.0, 10.0), ("air", 0.0, 10.0),
    ("airFreq", 5000.0, 16000.0), ("low", 0.0, 10.0), ("lowFreq", 40.0, 500.0), ("high", 0.0, 10.0), ("highFreq", 150.0, 5000.0),
    ("mix", 0.0, 1.0), ("out", -24.0, 6.0),
];
/// climaxIn: the saved line amp clamped to its ranges; a world from before it takes the defaults.
pub fn climax_in(j: Option<&Value>) -> Map<String, Value> {
    let mut out = match json!({"on":1,"drive":2,"sat":30,"classA":1,"excite":3,"air":2,"airFreq":10000,"low":0,"lowFreq":80,"high":0,"highFreq":3000,"mix":1,"out":0}) {
        Value::Object(m) => m,
        _ => unreachable!(),
    };
    let Some(Value::Object(j)) = j else { return out };
    for (k, lo, hi) in CLIMAX_DEFS {
        let x = js_num(j.get(k));
        if x.is_finite() {
            out.insert(k.into(), num(js_min(hi, js_max(lo, x))));
        }
    }
    out
}

// ---------- helpers on JSON objects ----------
fn obj<'a>(v: &'a Value, who: &str) -> Result<&'a Map<String, Value>, String> {
    v.as_object().ok_or_else(|| format!("{who}: not an object"))
}
fn arr<'a>(j: &'a Map<String, Value>, k: &str) -> &'a [Value] {
    j.get(k).and_then(|v| v.as_array()).map(|a| a.as_slice()).unwrap_or(&[])
}
/// A field read as JS reads `o.k` in arithmetic: missing is NaN (here: the default, since every save has them).
fn f(o: &Map<String, Value>, k: &str, d: f64) -> f64 {
    match o.get(k) {
        None => d,
        v => js_num(v),
    }
}
fn id(o: &Map<String, Value>) -> u32 {
    let x = js_num(o.get("id"));
    if x.is_finite() && x >= 0.0 { x as u32 } else { 0 }
}
fn s(o: &Map<String, Value>, k: &str, d: &str) -> String {
    o.get(k).and_then(|v| v.as_str()).unwrap_or(d).to_string()
}

impl World {
    pub fn new() -> World {
        World { space: space_default(), climax: climax_in(None), next: 1, rng: 0x2545F4914F6CDD1D, ..Default::default() }
    }

    /// The demo machine (sim.js demo(), without the camera fit, which is the app's).
    pub fn demo() -> World {
        let j: Value = serde_json::from_str(include_str!("../../../../cogs/src/default-world.json")).expect("default world");
        World::from_json(&j).expect("default world loads")
    }

    pub fn from_json(j: &Value) -> Result<World, String> {
        let mut w = World::new();
        w.deserialize(j)?;
        Ok(w)
    }
    pub fn from_str(s: &str) -> Result<World, String> {
        let j: Value = serde_json::from_str(s).map_err(|e| e.to_string())?;
        World::from_json(&j)
    }
    pub fn to_string(&self) -> String {
        serde_json::to_string(&self.serialize()).unwrap_or_default()
    }

    /// sim.js reload: replace the world from j, every surviving cog keeping its revolution count. The clock runs on.
    pub fn reload(&mut self, j: &Value) -> Result<(), String> {
        let turns: Vec<(u32, f64)> = self.cogs.iter().map(|c| (c.id, c.turns)).collect();
        self.deserialize(j)?;
        for c in &mut self.cogs {
            c.turns = turns.iter().find(|t| t.0 == c.id).map(|t| t.1).unwrap_or(0.0);
        }
        Ok(())
    }

    /// sim.js deserialize: every part is built and checked before any of the live world is replaced.
    pub fn deserialize(&mut self, jv: &Value) -> Result<(), String> {
        let j = obj(jv, "world")?;
        // cogs
        let mut cogs = Vec::new();
        for cv in arr(j, "cogs") {
            let c = obj(cv, "cog")?;
            let cid = id(c);
            let motor = if truthy(c.get("motor")) {
                let m = c.get("motor").and_then(|m| m.as_object());
                let g = |k| {
                    let x = m.map(|m| js_num(m.get(k))).unwrap_or(f64::NAN);
                    if x.is_nan() || x == 0.0 { 0.0 } else { x }
                };
                Some(Motor { rpm: g("rpm"), revs: g("revs") })
            } else {
                None
            };
            let th = f(c, "th", 0.0);
            let th0 = match c.get("th0") {
                None | Some(Value::Null) => th,
                v => js_num(v),
            };
            let tooth = {
                let x = f(c, "tooth", 0.0);
                if x.is_nan() { 0.0 } else { x }
            };
            let track = match c.get("track") {
                Some(Value::Object(t)) => {
                    let sc = t.get("scale").and_then(|v| v.as_str()).and_then(scale_ix);
                    let range = t.get("range").and_then(|v| v.as_f64());
                    let octave = t.get("octave").and_then(|v| v.as_f64());
                    let root = js_num(t.get("root"));
                    let ok = sc.is_some()
                        && range.map_or(false, |r| [1.0, 2.0, 3.0].contains(&r))
                        && octave.map_or(false, |o| (1..=7).any(|k| k as f64 == o))
                        && root >= 0.0
                        && root < 12.0;
                    if !ok {
                        return Err(format!("cog {cid}: track scale {}, range {}, octave {}, root {}",
                            t.get("scale").unwrap_or(&Value::Null), t.get("range").unwrap_or(&Value::Null),
                            t.get("octave").unwrap_or(&Value::Null), t.get("root").unwrap_or(&Value::Null)));
                    }
                    let mut dots = Vec::new();
                    for dv in arr(t, "dots") {
                        let d = obj(dv, "dot")?;
                        let mut dot = Dot::new(f(d, "ring", 0.0), f(d, "a", 0.0));
                        dot.p = f(d, "p", 1.0);
                        dot.v = f(d, "v", 1.0);
                        dot.len = f(d, "len", 1.0);
                        dots.push(dot);
                    }
                    let mut extra = Map::new();
                    for (k, v) in t {
                        if !["root", "octave", "range", "scale", "div", "dots"].contains(&k.as_str()) {
                            extra.insert(k.clone(), v.clone());
                        }
                    }
                    Some(Track { root, octave: octave.unwrap(), range: range.unwrap(), scale: sc.unwrap(), div: f(t, "div", 16.0), dots, extra })
                }
                _ => None,
            };
            cogs.push(Cog {
                id: cid, x: f(c, "x", 0.0), y: f(c, "y", 0.0), n: f(c, "n", 12.0), motor, layer: f(c, "layer", 0.0), th, th0, tooth, track,
                w: 0.0, k: None, jam: false, turns: 0.0, driver: None, scrub: None,
            });
        }
        if cogs.iter().any(|c| c.th != c.th0) {
            self.clock.at_start = false;
        }
        let has_cog = |cid: u32| cogs.iter().any(|c| c.id == cid);
        let cog_ref = |v: Option<&Value>, who: String| -> Result<u32, String> {
            let x = js_num(v);
            if x.is_finite() && x >= 0.0 && has_cog(x as u32) && x == x.trunc() { Ok(x as u32) } else { Err(format!("{who}: no cog {}", v.unwrap_or(&Value::Null))) }
        };
        let mut arms = Vec::new();
        for av in arr(j, "arms") {
            let a = obj(av, "arm")?;
            let aid = id(a);
            arms.push(Arm {
                id: aid, cog: cog_ref(a.get("cog"), format!("arm {aid}"))?, a: f(a, "a", 0.0), r0: f(a, "r0", 0.0), len: f(a, "len", 4.0),
                flex: f(a, "flex", 0.5), cog_ix: 0, flash: f64::NEG_INFINITY, bend_t: f64::NEG_INFINITY, bend_a: 0.0,
            });
        }
        let mut belts = Vec::new();
        for bv in arr(j, "belts") {
            let b = obj(bv, "belt")?;
            let bid = id(b);
            belts.push(Belt {
                id: bid, a: cog_ref(b.get("a"), format!("belt {bid}"))?, b: cog_ref(b.get("b"), format!("belt {bid}"))?,
                cross: truthy(b.get("cross")), a_ix: 0, b_ix: 0,
            });
        }
        let mut strings = Vec::new();
        for sv in arr(j, "strings") {
            let o = obj(sv, "string")?;
            strings.push(Str {
                id: id(o), x1: f(o, "x1", 0.0), y1: f(o, "y1", 0.0), x2: f(o, "x2", 0.0), y2: f(o, "y2", 0.0), kind: s(o, "kind", "string"),
                f200: f(o, "f200", 220.0), quant: truthy(o.get("quant")), decay: f(o, "decay", 2.5), damp: f(o, "damp", 0.5), hard: f(o, "hard", 0.5),
                level: f(o, "level", 0.7), vel_sense: f(o, "velSense", 0.7), hits: 0, last_v: 0.0, last_u: 0.0, flash: f64::NEG_INFINITY,
            });
        }
        let mut pads = Vec::new();
        for pv in arr(j, "pads") {
            let p = obj(pv, "pad")?;
            let pid = id(p);
            pads.push(Pad {
                id: pid, x: f(p, "x", 0.0), y: f(p, "y", 0.0), r: f(p, "r", 12.0), voice: Voice::import(p.get("voice").unwrap_or(&Value::Null), pid)?,
                hits: 0, last_v: 0.0, flash: f64::NEG_INFINITY,
            });
        }
        let mut readers = Vec::new();
        for rv in arr(j, "readers") {
            let r = obj(rv, "reader")?;
            let rid = id(r);
            readers.push(Reader {
                id: rid, on: None, x1: f(r, "x1", 0.0), y1: f(r, "y1", 0.0), x2: f(r, "x2", 0.0), y2: f(r, "y2", 0.0),
                voice: Voice::import(r.get("voice").unwrap_or(&Value::Null), rid)?, tines: f(r, "tines", 0.0), comb_base: f(r, "combBase", 60.0),
                comb_scale: s(r, "combScale", "major"), seat: None, on_ix: None, hits: 0, last_v: 0.0, last_midi: 0.0, flash: f64::NEG_INFINITY,
            });
        }
        // cams: one on a missing cog is dropped, a link to a missing reader comes loose
        let mut cams = Vec::new();
        for mv in arr(j, "cams") {
            let Some(m) = mv.as_object() else { continue };
            let cogv = js_num(m.get("cog"));
            if !(cogv.is_finite() && cogv >= 0.0 && has_cog(cogv as u32)) {
                continue;
            }
            let mut o = crate::motion::new_cam(id(m));
            o.cog = cogv as u32;
            o.ang = f(m, "ang", 0.0);
            o.phase = f(m, "phase", 0.0);
            o.shape = s(m, "shape", "sine");
            o.lo = f(m, "lo", 0.0);
            o.hi = f(m, "hi", 1.0);
            o.prof = match m.get("prof") {
                Some(Value::Array(p)) if p.len() == CAM_N => p.iter().map(|x| {
                    let v = js_num(Some(x));
                    clamp(if v.is_nan() { 0.0 } else { v }, 0.0, 1.0)
                }).collect(),
                _ => crate::motion::cam_shape(if crate::motion::shape_known(&o.shape) { &o.shape } else { "sine" }),
            };
            o.macro_ = clamp(to_int32(js_num(m.get("macro"))) as f64, 1.0, WAVE_MACROS as f64) as i64;
            let to = js_num(m.get("to"));
            o.to = readers.iter().find(|r| r.id as f64 == to).map(|r| r.id);
            cams.push(o);
        }
        // A save from before seats: a reader lying wholly on a disc is seated on it.
        let readers_j = arr(j, "readers");
        for (i, r) in readers.iter_mut().enumerate() {
            let rj = readers_j[i].as_object().unwrap();
            let c: Option<&Cog> = match rj.get("on") {
                Some(Value::Null) => None,
                Some(v) => {
                    let cid = cog_ref(Some(v), format!("reader {}", r.id))?;
                    cogs.iter().find(|c| c.id == cid)
                }
                None => {
                    let mut best: Option<&Cog> = None;
                    for c in &cogs {
                        if c.track.is_some() && hypot(r.x1 - c.x, r.y1 - c.y) <= c.radius() && hypot(r.x2 - c.x, r.y2 - c.y) <= c.radius() {
                            if best.map_or(true, |b| c.layer > b.layer) {
                                best = Some(c);
                            }
                        }
                    }
                    best
                }
            };
            if let Some(c) = c {
                if c.track.is_some() {
                    crate::solve::seat_reader(r, c);
                }
            }
        }
        // ball pits
        let mut pits = Vec::new();
        for pv in arr(j, "pits") {
            let p = obj(pv, "pit")?;
            pits.push(crate::pit::pit_in(p)?);
        }
        let mut next = World {
            cogs, arms, belts, strings, pads, readers, cams, pits, ..World::new()
        };
        next.probes = crate::probe::probes_in(j, &next);
        next.fields = arr(j, "fields").iter().filter_map(|v| v.as_object()).map(crate::field::field_in).collect();

        // swap in
        self.cogs = std::mem::take(&mut next.cogs);
        self.arms = std::mem::take(&mut next.arms);
        self.belts = std::mem::take(&mut next.belts);
        self.strings = std::mem::take(&mut next.strings);
        self.pads = std::mem::take(&mut next.pads);
        self.readers = std::mem::take(&mut next.readers);
        self.cams = std::mem::take(&mut next.cams);
        self.pits = std::mem::take(&mut next.pits);
        self.probes = std::mem::take(&mut next.probes);
        self.fields = std::mem::take(&mut next.fields);
        self.next = 1 + self.max_id();
        // Object.assign(S, j.S)
        if let Some(Value::Object(sj)) = j.get("S") {
            self.assign_settings(sj);
        }
        for (k, v) in migrate_space(j.get("SPACE")) {
            self.space.insert(k, v);
        }
        self.climax = climax_in(j.get("climax"));
        if let Some(Value::Object(v)) = j.get("view") {
            for (k, x) in v {
                match k.as_str() {
                    "x" => self.view.x = js_num(Some(x)),
                    "y" => self.view.y = js_num(Some(x)),
                    _ => {
                        self.view.extra.insert(k.clone(), x.clone());
                    }
                }
            }
        }
        // Older saves schedule 60 ms ahead, too close for a phone's main thread.
        self.s.lookahead = js_max(self.s.lookahead, LOOKAHEAD_MIN);
        self.solve();
        Ok(())
    }

    fn assign_settings(&mut self, sj: &Map<String, Value>) {
        let s = &mut self.s;
        for (k, v) in sj {
            let x = js_num(Some(v));
            let slot = match k.as_str() {
                "bpm" => &mut s.bpm,
                "beats" => &mut s.beats,
                "seed" => &mut s.seed,
                "meshTol" => &mut s.mesh_tol,
                "coaxTol" => &mut s.coax_tol,
                "snapTol" => &mut s.snap_tol,
                "vRef" => &mut s.v_ref,
                "velCurve" => &mut s.vel_curve,
                "master" => &mut s.master,
                "lookahead" => &mut s.lookahead,
                "maxStep" => &mut s.max_step,
                "zoom" => &mut s.zoom,
                "windows" => &mut s.windows,
                "ids" => &mut s.ids,
                "ratios" => &mut s.ratios,
                _ => {
                    s.extra.insert(k.clone(), v.clone());
                    continue;
                }
            };
            *slot = x;
        }
    }

    /// The largest id of any part (balls aside).
    pub fn max_id(&self) -> u32 {
        let mut m = 0;
        for i in self.cogs.iter().map(|o| o.id)
            .chain(self.arms.iter().map(|o| o.id))
            .chain(self.belts.iter().map(|o| o.id))
            .chain(self.strings.iter().map(|o| o.id))
            .chain(self.pads.iter().map(|o| o.id))
            .chain(self.readers.iter().map(|o| o.id))
            .chain(self.cams.iter().map(|o| o.id))
            .chain(self.pits.iter().map(|o| o.id))
            .chain(self.probes.iter().map(|o| o.id))
            .chain(self.fields.iter().map(|o| o.id))
        {
            m = m.max(i);
        }
        m
    }

    /// sim.js serialize: the `.cogs.json` save.
    pub fn serialize(&self) -> Value {
        let s = &self.s;
        let mut sj = Map::new();
        for (k, x) in [("bpm", s.bpm), ("beats", s.beats), ("seed", s.seed), ("meshTol", s.mesh_tol), ("coaxTol", s.coax_tol),
            ("snapTol", s.snap_tol), ("vRef", s.v_ref), ("velCurve", s.vel_curve), ("master", s.master), ("lookahead", s.lookahead),
            ("maxStep", s.max_step), ("zoom", s.zoom), ("windows", s.windows), ("ids", s.ids), ("ratios", s.ratios)]
        {
            sj.insert(k.into(), num(x));
        }
        for (k, v) in &s.extra {
            sj.insert(k.clone(), v.clone());
        }
        let mut view = Map::new();
        view.insert("x".into(), num(self.view.x));
        view.insert("y".into(), num(self.view.y));
        for (k, v) in &self.view.extra {
            view.insert(k.clone(), v.clone());
        }
        let cogs: Vec<Value> = self.cogs.iter().map(|c| {
            let track = match &c.track {
                None => Value::Null,
                Some(t) => {
                    let mut o = t.extra.clone();
                    o.insert("root".into(), num(t.root));
                    o.insert("octave".into(), num(t.octave));
                    o.insert("range".into(), num(t.range));
                    o.insert("scale".into(), json!(SCALES[t.scale].0));
                    o.insert("div".into(), num(t.div));
                    o.insert("dots".into(), Value::Array(t.dots.iter().map(|d| json!({"ring":num(d.ring),"a":num(d.a),"p":num(d.p),"v":num(d.v),"len":num(d.len)})).collect()));
                    Value::Object(o)
                }
            };
            json!({"id":c.id,"x":num(c.x),"y":num(c.y),"n":num(c.n),
                "motor":c.motor.map(|m| json!({"rpm":num(m.rpm),"revs":num(m.revs)})).unwrap_or(Value::Null),
                "layer":num(c.layer),"th":num(wrap(c.th)),"th0":num(wrap(c.th0)),"tooth":num(wrap(c.tooth)),"track":track})
        }).collect();
        let arms: Vec<Value> = self.arms.iter().map(|a| json!({"id":a.id,"cog":a.cog,"a":num(a.a),"r0":num(a.r0),"len":num(a.len),"flex":num(a.flex)})).collect();
        let pads: Vec<Value> = self.pads.iter().map(|p| json!({"id":p.id,"x":num(p.x),"y":num(p.y),"r":num(p.r),"voice":p.voice.to_json()})).collect();
        let readers: Vec<Value> = self.readers.iter().map(|r| json!({"id":r.id,"on":r.on,"x1":num(r.x1),"y1":num(r.y1),"x2":num(r.x2),"y2":num(r.y2),
            "voice":r.voice.to_json(),"tines":num(r.tines),"combBase":num(r.comb_base),"combScale":r.comb_scale})).collect();
        let belts: Vec<Value> = self.belts.iter().map(|b| json!({"id":b.id,"a":b.a,"b":b.b,"cross":b.cross})).collect();
        let strings: Vec<Value> = self.strings.iter().map(|s| json!({"id":s.id,"x1":num(s.x1),"y1":num(s.y1),"x2":num(s.x2),"y2":num(s.y2),"kind":s.kind,
            "f200":num(s.f200),"quant":s.quant,"decay":num(s.decay),"damp":num(s.damp),"hard":num(s.hard),"level":num(s.level),"velSense":num(s.vel_sense)})).collect();
        let cams: Vec<Value> = self.cams.iter().map(|m| json!({"id":m.id,"cog":m.cog,"ang":num(m.ang),"phase":num(m.phase),"shape":m.shape,
            "prof":m.prof.iter().map(|x| num(*x)).collect::<Vec<_>>(),"to":m.to,"macro":m.macro_,"lo":num(m.lo),"hi":num(m.hi)})).collect();
        let pits: Vec<Value> = self.pits.iter().map(crate::pit::pit_out).collect();
        let probes: Vec<Value> = self.probes.iter().filter(|p| self.probe_alive(p)).map(|p| crate::probe::probe_out(p)).collect();
        let fields: Vec<Value> = self.fields.iter().map(crate::field::field_out).collect();
        json!({"S":sj,"SPACE":self.space,"climax":self.climax,"view":view,"cogs":cogs,"arms":arms,"pads":pads,"readers":readers,
            "belts":belts,"strings":strings,"cams":cams,"pits":pits,"probes":probes,"fields":fields})
    }

    // ---------- lookups ----------
    pub fn cog_ix(&self, id: u32) -> Option<usize> {
        self.cogs.iter().position(|c| c.id == id)
    }
    pub fn cog(&self, id: u32) -> Option<&Cog> {
        self.cogs.iter().find(|c| c.id == id)
    }
    pub fn cog_mut(&mut self, id: u32) -> Option<&mut Cog> {
        self.cogs.iter_mut().find(|c| c.id == id)
    }
    pub fn reader_ix(&self, id: u32) -> Option<usize> {
        self.readers.iter().position(|c| c.id == id)
    }
    /// The voice of a voiced part.
    pub fn voice(&self, p: Part) -> Option<&Voice> {
        match p {
            Part::Pad(i) => self.pads.iter().find(|o| o.id == i).map(|o| &o.voice),
            Part::Reader(i) => self.readers.iter().find(|o| o.id == i).map(|o| &o.voice),
            Part::Pit(i) => self.pits.iter().find(|o| o.id == i).map(|o| &o.voice),
            Part::String(_) => None,
        }
    }
    pub fn voice_mut(&mut self, p: Part) -> Option<&mut Voice> {
        match p {
            Part::Pad(i) => self.pads.iter_mut().find(|o| o.id == i).map(|o| &mut o.voice),
            Part::Reader(i) => self.readers.iter_mut().find(|o| o.id == i).map(|o| &mut o.voice),
            Part::Pit(i) => self.pits.iter_mut().find(|o| o.id == i).map(|o| &mut o.voice),
            Part::String(_) => None,
        }
    }
    pub fn part_exists(&self, p: Part) -> bool {
        match p {
            Part::String(i) => self.strings.iter().any(|o| o.id == i),
            _ => self.voice(p).is_some(),
        }
    }

    /// Resolve every id reference to an index. Arms, belts and cams on a missing cog are dropped, cam and probe
    /// links to missing parts come loose (sim.js removeObj). A probe whose host is gone stays, dead, until
    /// `prune_probes` (as in the JS, it is not saved).
    pub fn relink(&mut self) {
        let cogs = &self.cogs;
        let ix = |id: u32| cogs.iter().position(|c| c.id == id);
        self.arms.retain(|a| ix(a.cog).is_some());
        for a in &mut self.arms {
            a.cog_ix = ix(a.cog).unwrap();
        }
        self.belts.retain(|b| ix(b.a).is_some() && ix(b.b).is_some());
        for b in &mut self.belts {
            b.a_ix = ix(b.a).unwrap();
            b.b_ix = ix(b.b).unwrap();
        }
        self.cams.retain(|m| ix(m.cog).is_some());
        let readers = &self.readers;
        for m in &mut self.cams {
            m.cog_ix = ix(m.cog).unwrap();
            m.to_ix = m.to.and_then(|t| readers.iter().position(|r| r.id == t));
            if m.to_ix.is_none() {
                m.to = None;
            }
        }
        for r in &mut self.readers {
            r.on_ix = r.on.and_then(ix);
        }
        for p in &mut self.probes {
            p.on_ix = match p.host {
                Some(Host::Arm) => self.arms.iter().position(|o| o.id == p.on),
                Some(Host::Cog) => ix(p.on),
                Some(Host::Cam) => self.cams.iter().position(|o| o.id == p.on),
                Some(Host::Ball) => self.pits.iter().position(|o| o.id == p.on),
                None => None,
            };
        }
        let mut probes = std::mem::take(&mut self.probes);
        for p in &mut probes {
            if let Some(t) = p.to {
                if !matches!(t, Part::String(_)) && !self.part_exists(t) {
                    p.to = None;
                }
            }
        }
        self.probes = probes;
    }
}

impl serde::Serialize for World {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        World::serialize(self).serialize(ser)
    }
}
impl<'de> serde::Deserialize<'de> for World {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(de)?;
        World::from_json(&v).map_err(serde::de::Error::custom)
    }
}

pub(crate) fn new_trail() -> VecDeque<[f64; 3]> {
    VecDeque::with_capacity(256)
}
