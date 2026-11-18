//! field.js: FX fields, their geometry and the sends a sound at a point takes.

use crate::model::*;
use crate::sim::{Event, Send};
use crate::voice::{js_num, num};
use crate::world::World;
use serde_json::{Map, Value};
use std::collections::VecDeque;

pub const FIELD_MODES: [&str; 3] = ["delay", "reverb", "dist"];
pub const FIELD_SYNC_OPTS: [&str; 8] = ["free", "1/16", "1/8", "1/8.", "1/4", "1/4.", "1/2", "1/2."];
pub const FIELD_DIST_KINDS: [&str; 4] = ["tube", "fold", "fuzz", "crush"];

pub fn new_field(id: u32, x: f64, y: f64) -> Field {
    Field {
        id, x, y, r: 150.0, mode: "delay".into(), diffuse: 0.5, level: 0.8, insert: 0.0, sync: "1/8.".into(), time: 0.375, fb: 0.45, tone: 0.5,
        ping: 1.0, size: 0.6, decay: 2.5, predelay: 10.0, verb_mode: 0.0, verb_color: 2.0, kind: "tube".into(), drive: 0.5, slot: -1,
        flashes: VecDeque::with_capacity(17),
    }
}

/// fieldIn: a saved field, its knobs checked.
pub fn field_in(j: &Map<String, Value>) -> Field {
    let n = |k: &str| {
        let x = js_num(j.get(k));
        if x.is_nan() { 0.0 } else { x }
    };
    let mut f = new_field(j.get("id").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32, n("x"), n("y"));
    for (k, v) in j {
        if v.is_null() {
            continue;
        }
        let x = js_num(Some(v));
        let num_slot = match k.as_str() {
            "r" => Some(&mut f.r),
            "diffuse" => Some(&mut f.diffuse),
            "level" => Some(&mut f.level),
            "insert" => Some(&mut f.insert),
            "time" => Some(&mut f.time),
            "fb" => Some(&mut f.fb),
            "tone" => Some(&mut f.tone),
            "ping" => Some(&mut f.ping),
            "size" => Some(&mut f.size),
            "decay" => Some(&mut f.decay),
            "predelay" => Some(&mut f.predelay),
            "verbMode" => Some(&mut f.verb_mode),
            "verbColor" => Some(&mut f.verb_color),
            "drive" => Some(&mut f.drive),
            _ => None,
        };
        if let Some(slot) = num_slot {
            if x.is_finite() {
                *slot = x;
            }
            continue;
        }
        if let Some(s) = v.as_str() {
            match k.as_str() {
                "mode" => f.mode = s.into(),
                "sync" => f.sync = s.into(),
                "kind" => f.kind = s.into(),
                _ => {}
            }
        }
    }
    if !FIELD_MODES.contains(&f.mode.as_str()) {
        f.mode = "delay".into();
    }
    if !FIELD_SYNC_OPTS.contains(&f.sync.as_str()) {
        f.sync = "1/8.".into();
    }
    if !FIELD_DIST_KINDS.contains(&f.kind.as_str()) {
        f.kind = "tube".into();
    }
    f.r = clamp(f.r, 30.0, 800.0);
    f.diffuse = clamp(f.diffuse, 0.0, 1.0);
    f.insert = clamp(f.insert, 0.0, 1.0);
    f
}
pub fn field_out(f: &Field) -> Value {
    serde_json::json!({"id":f.id,"x":num(f.x),"y":num(f.y),"r":num(f.r),"mode":f.mode,"diffuse":num(f.diffuse),"level":num(f.level),
        "insert":num(f.insert),"sync":f.sync,"time":num(f.time),"fb":num(f.fb),"tone":num(f.tone),"ping":num(f.ping),"size":num(f.size),
        "decay":num(f.decay),"predelay":num(f.predelay),"verbMode":num(f.verb_mode),"verbColor":num(f.verb_color),"kind":f.kind,"drive":num(f.drive)})
}

impl Field {
    /// The send at a point: 1 inside the core, falling smoothly to 0 at the rim.
    pub fn send(&self, x: f64, y: f64) -> f64 {
        let r = hypot(x - self.x, y - self.y) / self.r;
        if r >= 1.0 {
            return 0.0;
        }
        let core = 1.0 - clamp(self.diffuse, 0.0, 1.0);
        if r <= core {
            return 1.0;
        }
        let u = (r - core) / (1.0 - core);
        1.0 - u * u * (3.0 - 2.0 * u)
    }
    /// How far in a point is: 0 at the rim, 1 at the centre.
    pub fn q(&self, x: f64, y: f64) -> f64 {
        clamp(1.0 - hypot(x - self.x, y - self.y) / self.r, 0.0, 1.0)
    }
    /// Set its effect, as setFieldMode does (distortion takes the dry, by default).
    pub fn set_mode(&mut self, m: &str) {
        if !FIELD_MODES.contains(&m) {
            return;
        }
        self.mode = m.into();
        if m == "dist" && self.insert == 0.0 {
            self.insert = 0.8;
        } else if m != "dist" && self.insert == 0.8 {
            self.insert = 0.0;
        }
        self.tone = if m == "dist" { 0.6 } else { 0.5 };
    }
}

impl World {
    /// fieldSlots: each field to its slot, the lowest free one, kept while it lives.
    pub fn field_slots(&mut self) {
        let mut used = [false; FIELD_SLOTS_N];
        for f in &mut self.fields {
            if f.slot >= 0 && (f.slot as usize) < FIELD_SLOTS_N && !used[f.slot as usize] {
                used[f.slot as usize] = true;
            } else {
                f.slot = -1;
            }
        }
        for f in &mut self.fields {
            if f.slot < 0 {
                if let Some(k) = (0..FIELD_SLOTS_N).find(|&k| !used[k]) {
                    f.slot = k as i32;
                    used[k] = true;
                }
            }
        }
    }
    /// fieldSendsAt + fieldHit: the sends at the event's place, and a ripple in each field it sounds in.
    pub(crate) fn field_sends(&mut self, e: &mut Event) {
        e.n_sends = 0;
        e.dry = 1.0;
        for f in &mut self.fields {
            if f.slot < 0 {
                continue;
            }
            let a = f.send(e.x, e.y);
            if a <= 0.001 {
                continue;
            }
            if (e.n_sends as usize) < FIELD_SLOTS_N {
                e.sends[e.n_sends as usize] = Send { slot: f.slot as u8, amount: to_fixed(a, 4) as f32, q: to_fixed(f.q(e.x, e.y), 4) as f32 };
                e.n_sends += 1;
            }
            e.dry *= 1.0 - clamp(f.insert, 0.0, 1.0) * a;
            if f.flashes.len() >= 16 {
                f.flashes.pop_front();
            }
            f.flashes.push_back(FieldFlash { x: e.x, y: e.y, t: e.t, a });
        }
    }
    /// The sends a sound at (x, y) would take: (slot, amount) pairs into `out`, and the dry level left.
    pub fn field_sends_at(&self, x: f64, y: f64, out: &mut Vec<Send>) -> f64 {
        out.clear();
        let mut dry = 1.0;
        for f in &self.fields {
            if f.slot < 0 {
                continue;
            }
            let a = f.send(x, y);
            if a <= 0.001 {
                continue;
            }
            out.push(Send { slot: f.slot as u8, amount: to_fixed(a, 4) as f32, q: to_fixed(f.q(x, y), 4) as f32 });
            dry *= 1.0 - clamp(f.insert, 0.0, 1.0) * a;
        }
        dry
    }
}
