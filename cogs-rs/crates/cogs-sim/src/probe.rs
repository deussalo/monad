//! probe.js: a probe pins onto something that moves (an arm, a point on a cog, a cam's follower, a ball) and reads
//! one axis of its travel as 0..1, driving a synth's macro (a motion program) or, on any other voice, one knob set
//! at each strike.

use crate::model::*;
use crate::motion::{Motion, MotionFrom};
use crate::sim::Event;
use crate::voice::{def_at, js_num, num, probe_def, truthy};
use crate::world::World;
use serde_json::{json, Map, Value};

pub fn new_probe(id: u32) -> Probe {
    Probe {
        id, host: None, on: 0, ball: 0, r: 0.0, a: 0.0, axis: Axis::Y, to: None, macro_: 1, more: Vec::new(), param: None, lo: 0.0, hi: 1.0, on_ix: None,
        tab: Vec::new(), key: [f64::NAN; 13], tbox: [0.0; 4],
    }
}

/// A cam's or probe's further links, as saved: [{"to":"reader:3","macro":2}, ...].
pub fn links_out(more: &[MacroLink]) -> Value {
    Value::Array(more.iter().map(|l| json!({"to":target_key(l.to),"macro":l.macro_})).collect())
}
/// Read back what `links_out` wrote; parts that are gone are dropped later, by `relink`.
pub fn links_in(v: Option<&Value>) -> Vec<MacroLink> {
    let mut out = Vec::new();
    for l in v.and_then(|v| v.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
        let Some(to) = l.get("to").and_then(|t| t.as_str()).and_then(parse_key) else { continue };
        let m = to_int32(js_num(l.get("macro")));
        let l = MacroLink { to, macro_: clamp(if m == 0 { 1.0 } else { m as f64 }, 1.0, WAVE_MACROS as f64) as i64 };
        if out.len() < MACRO_LINKS - 1 && !out.contains(&l) {
            out.push(l);
        }
    }
    out
}
fn parse_key(s: &str) -> Option<Part> {
    let (k, i) = s.split_once(':')?;
    let i: u32 = i.trim().parse().ok()?;
    match k {
        "reader" => Some(Part::Reader(i)),
        "pad" => Some(Part::Pad(i)),
        "pit" => Some(Part::Pit(i)),
        _ => None,
    }
}
pub fn target_key(p: Part) -> String {
    match p {
        Part::Reader(i) => format!("reader:{i}"),
        Part::Pad(i) => format!("pad:{i}"),
        Part::Pit(i) => format!("pit:{i}"),
        Part::String(i) => format!("string:{i}"),
    }
}
pub fn probe_out(p: &Probe) -> Value {
    json!({"id":p.id,"host":p.host.map(|h| h.name()),"on":p.on,"ball":p.ball,"r":num(p.r),"a":num(p.a),"axis":p.axis.name(),
        "to":p.to.map(target_key),"macro":p.macro_,"more":links_out(&p.more),"param":p.param,"lo":num(p.lo),"hi":num(p.hi)})
}
/// probesIn: one whose host is missing is dropped, a link to a missing part comes loose.
pub fn probes_in(j: &Map<String, Value>, next: &World) -> Vec<Probe> {
    let mut out = Vec::new();
    for v in j.get("probes").and_then(|v| v.as_array()).map(|a| a.as_slice()).unwrap_or(&[]) {
        let Some(p) = v.as_object() else { continue };
        let on = js_num(p.get("on"));
        let host = match p.get("host").and_then(|h| h.as_str()) {
            Some("arm") => Host::Arm,
            Some("cog") => Host::Cog,
            Some("cam") => Host::Cam,
            Some("ball") => Host::Ball,
            _ => continue,
        };
        let found = match host {
            Host::Arm => next.arms.iter().any(|o| o.id as f64 == on),
            Host::Cog => next.cogs.iter().any(|o| o.id as f64 == on),
            Host::Cam => next.cams.iter().any(|o| o.id as f64 == on),
            Host::Ball => next.pits.iter().any(|o| o.id as f64 == on),
        };
        if !found {
            continue;
        }
        let ball = to_int32(js_num(p.get("ball")));
        if host == Host::Ball {
            let pit = next.pits.iter().find(|o| o.id as f64 == on).unwrap();
            if !pit.balls.iter().any(|b| b.id as i64 == ball as i64) {
                continue;
            }
        }
        let to = p.get("to").map(|t| match t {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        }).unwrap_or_default();
        let mut sp = to.splitn(2, ':');
        let (tk, ti) = (sp.next().unwrap_or(""), sp.next().map(|s| js_num(Some(&Value::String(s.into())))).unwrap_or(f64::NAN));
        let to = match tk {
            "reader" => next.readers.iter().find(|o| o.id as f64 == ti).map(|o| Part::Reader(o.id)),
            "pad" => next.pads.iter().find(|o| o.id as f64 == ti).map(|o| Part::Pad(o.id)),
            "pit" => next.pits.iter().find(|o| o.id as f64 == ti).map(|o| Part::Pit(o.id)),
            _ => None,
        };
        let or0 = |k: &str| {
            let x = js_num(p.get(k));
            if x.is_nan() || x == 0.0 { 0.0 } else { x }
        };
        let mut pr = new_probe(p.get("id").and_then(|v| v.as_f64()).unwrap_or(0.0) as u32);
        pr.host = Some(host);
        pr.on = on as u32;
        pr.ball = ball.max(0) as u32;
        pr.r = or0("r");
        pr.a = or0("a");
        pr.axis = match p.get("axis").and_then(|a| a.as_str()) {
            Some("x") => Axis::X,
            Some("reach") => Axis::Reach,
            _ => Axis::Y,
        };
        pr.to = to;
        let m = to_int32(js_num(p.get("macro")));
        pr.macro_ = clamp(if m == 0 { 1.0 } else { m as f64 }, 1.0, WAVE_MACROS as f64) as i64;
        pr.more = links_in(p.get("more"));
        pr.param = p.get("param").and_then(|x| x.as_str()).map(|s| s.to_string());
        pr.lo = clamp(or0("lo"), 0.0, 1.0);
        pr.hi = clamp(if matches!(p.get("hi"), None | Some(Value::Null)) { 1.0 } else { js_num(p.get("hi")) }, 0.0, 1.0);
        let _ = truthy;
        out.push(pr);
    }
    out
}

impl World {
    /// The cog a probe's point turns with.
    pub fn probe_cog(&self, pr: &Probe) -> Option<usize> {
        let o = pr.on_ix?;
        match pr.host? {
            Host::Arm => Some(self.arms.get(o)?.cog_ix),
            Host::Cog => Some(o),
            Host::Cam => Some(self.cams.get(o)?.cog_ix),
            Host::Ball => None,
        }
    }
    pub fn probe_ball(&self, pr: &Probe) -> Option<&Ball> {
        if pr.host != Some(Host::Ball) {
            return None;
        }
        self.pits.get(pr.on_ix?)?.balls.iter().find(|b| b.id == pr.ball)
    }
    pub fn probe_alive(&self, pr: &Probe) -> bool {
        let Some(o) = pr.on_ix else { return false };
        match pr.host {
            Some(Host::Arm) => self.arms.get(o).map_or(false, |a| a.id == pr.on),
            Some(Host::Cog) => self.cogs.get(o).map_or(false, |a| a.id == pr.on),
            Some(Host::Cam) => self.cams.get(o).map_or(false, |a| a.id == pr.on),
            Some(Host::Ball) => self.pits.get(o).map_or(false, |a| a.id == pr.on) && self.probe_ball(pr).is_some(),
            None => false,
        }
    }
    /// Drop probes whose host has gone; let go of a part that has gone.
    pub fn prune_probes(&mut self) {
        let mut probes = std::mem::take(&mut self.probes);
        probes.retain(|p| self.probe_alive(p));
        for p in &mut probes {
            if let Some(t) = p.to {
                if !self.part_exists(t) {
                    p.to = None;
                }
            }
        }
        self.probes = probes;
    }

    /// Where the probe is when the cog it rides has turned to th.
    pub fn probe_point_at(&self, pr: &Probe, th: f64) -> (f64, f64) {
        let o = pr.on_ix.unwrap();
        if pr.host == Some(Host::Cam) {
            let m = &self.cams[o];
            let l = self.cam_roll(m, m.at(m.u(th))) + 14.0;
            let c = &self.cogs[m.cog_ix];
            return (c.x + m.ang.cos() * l, c.y + m.ang.sin() * l);
        }
        let c = &self.cogs[self.probe_cog(pr).unwrap()];
        let (ang, l) = if pr.host == Some(Host::Arm) {
            let a = &self.arms[o];
            (th + a.a, clamp(pr.r, a.r0, a.r0 + a.len))
        } else {
            (th + pr.a, pr.r)
        };
        (c.x + ang.cos() * l, c.y + ang.sin() * l)
    }
    fn probe_key(&self, pr: &Probe) -> [f64; 13] {
        let o = pr.on_ix.unwrap();
        let mut k = [0.0; 13];
        k[0] = pr.host.map_or(-1.0, |h| h as i32 as f64);
        k[1] = pr.axis as i32 as f64;
        k[2] = pr.r;
        k[3] = pr.a;
        match pr.host {
            Some(Host::Arm) => {
                let a = &self.arms[o];
                k[4] = a.a;
                k[5] = a.r0;
                k[6] = a.len;
            }
            Some(Host::Cam) => {
                let m = &self.cams[o];
                k[7] = m.ang;
                k[8] = m.ver as f64;
                k[9] = m.phase;
                k[10] = self.cam_rb(m);
                k[11] = m.stepped() as i32 as f64;
                k[12] = m.prof.iter().fold(0.0, |h, x| h * 1.000003 + x);
            }
            _ => {}
        }
        k
    }
    /// probeTab for every probe whose geometry changed: one turn of its value, scaled to its own travel.
    pub fn refresh_probes(&mut self) {
        for i in 0..self.probes.len() {
            let pr = &self.probes[i];
            if !self.probe_alive(pr) || pr.host == Some(Host::Ball) {
                continue;
            }
            let key = self.probe_key(pr);
            if key == pr.key {
                continue;
            }
            let c = &self.cogs[self.probe_cog(pr).unwrap()];
            let (cx, cy) = (c.x, c.y);
            let mut raw = [0.0f64; PROBE_N];
            let mut b = [f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY];
            for (k, r) in raw.iter_mut().enumerate() {
                let (x, y) = self.probe_point_at(pr, TAU * k as f64 / PROBE_N as f64);
                *r = match pr.axis {
                    Axis::X => x,
                    Axis::Y => -y,
                    Axis::Reach => hypot(x - cx, y - cy),
                };
                b = [b[0].min(x), b[1].max(x), b[2].min(y), b[3].max(y)];
            }
            let lo = raw.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = raw.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let span = hi - lo;
            let pr = &mut self.probes[i];
            pr.tab.clear();
            pr.tab.extend(raw.iter().map(|x| if span > 0.01 { to_fixed((x - lo) / span, 5) } else { 0.5 }));
            pr.key = key;
            pr.tbox = [b[0] - cx, b[1] - cx, b[2] - cy, b[3] - cy];
        }
    }
    fn probe_tab_at(pr: &Probe, th: f64) -> f64 {
        let t = &pr.tab;
        if t.len() != PROBE_N {
            return 0.0;
        }
        let u = th / TAU;
        let x = (u - u.floor()) * PROBE_N as f64;
        let i = (x.floor() as usize) % PROBE_N;
        t[i] + (t[(i + 1) % PROBE_N] - t[i]) * (x - x.floor())
    }
    fn ball_value(&self, pr: &Probe, x: f64, y: f64) -> f64 {
        let p = &self.pits[pr.on_ix.unwrap()];
        let (l, b) = (p.x - p.w / 2.0, p.y + p.h / 2.0);
        clamp(match pr.axis {
            Axis::X => (x - l) / p.w,
            Axis::Y => (b - y) / p.h,
            Axis::Reach => hypot(x - p.x, y - p.y) / (p.w / 2.0),
        }, 0.0, 1.0)
    }
    /// Its value as drawn (the heard time).
    pub fn probe_now(&self, pr: &Probe) -> f64 {
        if !self.probe_alive(pr) {
            return 0.0;
        }
        if pr.host == Some(Host::Ball) {
            let b = self.probe_ball(pr).unwrap();
            let (x, y) = crate::pit::ball_at(b, self.clock.view);
            return self.ball_value(pr, x, y);
        }
        World::probe_tab_at(pr, self.render_th(self.probe_cog(pr).unwrap()))
    }
    /// Its value at sim time t (a strike being scheduled).
    pub fn probe_at(&self, pr: &Probe, t: f64) -> f64 {
        if !self.probe_alive(pr) {
            return 0.0;
        }
        if let Some(b) = self.probe_ball(pr) {
            return self.ball_value(pr, b.x, b.y);
        }
        let c = &self.cogs[self.probe_cog(pr).unwrap()];
        World::probe_tab_at(pr, c.th + c.w * (t - self.clock.sim_t))
    }
    /// Where it is as drawn.
    pub fn probe_point(&self, pr: &Probe) -> (f64, f64) {
        if let Some(b) = self.probe_ball(pr) {
            return crate::pit::ball_at(b, self.clock.view);
        }
        self.probe_point_at(pr, self.render_th(self.probe_cog(pr).unwrap()))
    }

    /// sim.js probed: every probe on the event's voice sets its knob from where it is at the strike.
    pub(crate) fn probed(&self, e: &mut Event, kind: VoiceKind, level: &mut f64, vs: &mut f64) {
        for pr in &self.probes {
            if pr.to != Some(e.part) || !self.probe_alive(pr) {
                continue;
            }
            let Some(d) = probe_def(kind, pr.param.as_deref()) else { continue };
            let x = def_at(d, pr.lo + (pr.hi - pr.lo) * self.probe_at(pr, e.t));
            match d.0 {
                "level" => *level = x,
                "velSense" => *vs = x,
                _ => {}
            }
            let n = e.n_over as usize;
            if let Some(slot) = e.over[..n].iter_mut().find(|o| o.0 == d.0) {
                slot.1 = x;
            } else if n < e.over.len() {
                e.over[n] = (d.0, x);
                e.n_over += 1;
            }
        }
    }

    /// probeProgram: what a probe on a synth part sends its (first) macro.
    pub fn probe_program<'a>(&'a self, pr: &'a Probe) -> Option<Motion<'a>> {
        self.probe_program_to(pr, pr.to?, pr.macro_)
    }
    /// What a probe sends one macro (1-based) of a synth part it is linked to.
    pub fn probe_program_to<'a>(&'a self, pr: &'a Probe, to: Part, mac: i64) -> Option<Motion<'a>> {
        let v = self.voice(to)?;
        if v.kind != VoiceKind::Wave || !self.probe_alive(pr) {
            return None;
        }
        let slot = (mac - 1).clamp(0, 7) as u8;
        if pr.host == Some(Host::Ball) {
            let y = to_fixed(self.probe_now(pr), 3);
            return Some(Motion {
                target: to, slot, from: MotionFrom::Probe(pr.id), table: &[], value: y, step: true, t0: 0.0, ph0: 0.0, rate: 0.0,
                lo: pr.lo, hi: pr.hi, glide: 0.03, sig: y.to_bits(),
            });
        }
        let ci = self.probe_cog(pr)?;
        let c = &self.cogs[ci];
        let sig = pr.key.iter().fold(0u64, |h, x| h.wrapping_mul(1099511628211).wrapping_add(x.to_bits()));
        let (t0, ph0, rate) = if self.clock.paused || c.w == 0.0 {
            (0.0, self.render_th(ci) / TAU, 0.0)
        } else {
            (self.clock.sim_t, c.th / TAU, c.w / TAU)
        };
        Some(Motion { target: to, slot, from: MotionFrom::Probe(pr.id), table: &pr.tab, value: 0.0, step: false, t0, ph0, rate, lo: pr.lo, hi: pr.hi, glide: 0.0, sig })
    }
}
