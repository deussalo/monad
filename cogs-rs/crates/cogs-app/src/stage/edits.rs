//! Undo: sim.js `edits`. One step per finished action; camera, pause, selection and the plates' phases are never
//! undone. A step is the save minus those keys; restoring it keeps the clock running: each cog takes the phase it would
//! now have had it kept the speed it had then, and balls keep moving.

use cogs_sim::{wrap, World};
use serde_json::Value;

pub const EDITS_MAX: usize = 500;
pub const EDITS_QUIET: f64 = 0.35;
const UNDONE_ELSEWHERE: [&str; 7] = ["view", "zoom", "th", "bx", "by", "bvx", "bvy"];

#[derive(Clone)]
pub struct Snap {
    key: Value,
    t: f64,
    phase: Vec<(u32, f64, f64)>,
    balls: Vec<((u32, u32), [f64; 4])>,
}

fn strip(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for k in UNDONE_ELSEWHERE {
                m.remove(k);
            }
            for x in m.values_mut() {
                strip(x);
            }
        }
        Value::Array(a) => {
            for x in a {
                strip(x);
            }
        }
        _ => {}
    }
}
pub fn world_key(w: &World) -> Value {
    let mut v = w.serialize();
    strip(&mut v);
    v
}
fn balls(w: &World) -> Vec<((u32, u32), [f64; 4])> {
    w.pits.iter().flat_map(|p| p.balls.iter().map(move |b| ((p.id, b.id), [b.x, b.y, b.vx, b.vy]))).collect()
}
fn snap(w: &World) -> Snap {
    Snap { key: world_key(w), t: w.clock.sim_t, phase: w.cogs.iter().map(|c| (c.id, c.th, c.w)).collect(), balls: balls(w) }
}

#[derive(Default)]
pub struct Edits {
    past: Vec<Snap>,
    future: Vec<Snap>,
    cur: Option<Snap>,
    /// When something last changed and is not yet a step.
    pub pending: Option<f64>,
    /// What the last undo or redo changed, in words.
    pub last_change: String,
}
impl Edits {
    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
    pub fn mark(&mut self, now: f64) {
        self.pending = Some(now);
    }
    /// Settle the pending change into a step; true when the world differs from the last one.
    pub fn commit(&mut self, w: &World) -> bool {
        self.pending = None;
        let s = snap(w);
        if let Some(c) = &self.cur {
            if c.key == s.key {
                return false;
            }
        }
        if let Some(c) = self.cur.take() {
            self.past.push(c);
            if self.past.len() > EDITS_MAX {
                self.past.remove(0);
            }
            self.future.clear();
        }
        self.cur = Some(s);
        true
    }
    pub fn reset(&mut self, w: &World) {
        self.pending = None;
        self.past.clear();
        self.future.clear();
        self.cur = Some(snap(w));
    }
    pub fn undo(&mut self, w: &mut World) -> bool {
        self.step(w, true)
    }
    pub fn redo(&mut self, w: &mut World) -> bool {
        self.step(w, false)
    }
    fn step(&mut self, w: &mut World, back: bool) -> bool {
        if self.pending.is_some() {
            self.commit(w);
        }
        let from = if back { &mut self.past } else { &mut self.future };
        let Some(s) = from.pop() else { return false };
        if let Some(c) = self.cur.take() {
            if back { self.future.push(c) } else { self.past.push(c) }
        }
        if let Some(c) = (if back { self.future.last() } else { self.past.last() }) {
            self.last_change = what_changed(&c.key, &s.key);
        }
        restore(w, &s);
        self.cur = Some(s);
        true
    }
}

/// The parts two steps differ in, in words: "reader 9's sound", "pad 12 and 2 more", "the room".
fn what_changed(a: &Value, b: &Value) -> String {
    const LISTS: [(&str, &str); 10] = [("cogs", "plate"), ("arms", "arm"), ("pads", "pad"), ("readers", "reader"), ("belts", "belt"),
        ("strings", "string"), ("cams", "cam"), ("pits", "sphere"), ("probes", "probe"), ("fields", "fx")];
    let mut said: Vec<String> = Vec::new();
    let empty = Vec::new();
    for (k, name) in LISTS {
        let (x, y) = (a[k].as_array().unwrap_or(&empty), b[k].as_array().unwrap_or(&empty));
        let find = |l: &'_ [Value], id: &Value| l.iter().position(|o| o["id"] == *id);
        for o in x {
            match find(y, &o["id"]) {
                None => said.push(format!("{name} {}", o["id"])),
                Some(i) if y[i] != *o => {
                    let only_voice = {
                        let (mut p, mut q) = (o.clone(), y[i].clone());
                        p["voice"] = Value::Null;
                        q["voice"] = Value::Null;
                        p == q
                    };
                    said.push(if only_voice { format!("{name} {}'s sound", o["id"]) } else { format!("{name} {}", o["id"]) });
                }
                _ => {}
            }
        }
        for o in y {
            if find(x, &o["id"]).is_none() {
                said.push(format!("{name} {}", o["id"]));
            }
        }
    }
    for (k, name) in [("SPACE", "the room"), ("climax", "the line amp"), ("S", "the settings")] {
        if a[k] != b[k] {
            said.push(name.into());
        }
    }
    match said.len() {
        0 => "nothing visible".into(),
        1 => said.remove(0),
        n => format!("{} and {} more", said[0], n - 1),
    }
}

fn restore(w: &mut World, s: &Snap) {
    let mut j = s.key.clone();
    let live = balls(w);
    if let Some(o) = j.as_object_mut() {
        if let Some(Value::Object(sj)) = o.get_mut("S") {
            sj.insert("zoom".into(), Value::from(w.s.zoom));
        }
        o.insert("view".into(), serde_json::json!({"x": w.view.x, "y": w.view.y}));
        if let Some(Value::Array(cogs)) = o.get_mut("cogs") {
            for c in cogs {
                let Some(c) = c.as_object_mut() else { continue };
                let id = c.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let Some(&(_, th, sw)) = s.phase.iter().find(|p| p.0 == id) else { continue };
                let th = wrap(th + sw * (w.clock.sim_t - s.t));
                let th0 = c.get("th0").and_then(|v| v.as_f64()).unwrap_or(0.0);
                if w.clock.at_start {
                    let tooth = c.get("tooth").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    c.insert("tooth".into(), Value::from(wrap(tooth + th - th0)));
                    c.insert("th".into(), Value::from(th0));
                } else {
                    c.insert("th".into(), Value::from(th));
                }
            }
        }
        if let Some(Value::Array(pits)) = o.get_mut("pits") {
            for p in pits {
                let pid = p.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                if let Some(Value::Array(bs)) = p.get_mut("balls") {
                    for b in bs {
                        let bid = b.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        let st = live.iter().find(|x| x.0 == (pid, bid)).or_else(|| s.balls.iter().find(|x| x.0 == (pid, bid)));
                        if let (Some((_, q)), Some(b)) = (st, b.as_object_mut()) {
                            for (k, v) in ["bx", "by", "bvx", "bvy"].iter().zip(q) {
                                b.insert((*k).into(), Value::from(*v));
                            }
                        }
                    }
                }
            }
        }
    }
    let at_start = w.clock.at_start;
    let before = w.clone();
    if w.reload(&j).is_ok() {
        w.clock.at_start = at_start;
        // the restored voices start their versions past the old ones', so the engine takes every patch again
        crate::play::bump_versions(&before, w);
    }
}
