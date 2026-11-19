//! Touch and mouse: stage.js begin(), pointerGesture() and the G gestures, the pointer handlers (taps, double taps,
//! holds, a second finger's grab or pinch), the wheel and the keyboard.

use super::draw::*;
use super::*;
use cogs_sim::{solve::place_seated, CogKind, Host, PartRef, Seat, TOOTH};
use egui::{Event, Key, PointerButton, TouchPhase};

const MOUSE: u64 = u64::MAX;

/// A gesture in progress: what a press began, with what it needs to carry on.
pub(crate) enum Gesture {
    Pan { v0: (f64, f64), a: (f64, f64), boxing: bool },
    Marquee { a: (f64, f64) },
    Group { p0: (f64, f64), start: Vec<(Sel, [f64; 4])>, cogs: Vec<(u32, f64, f64)>, last: Option<(f64, f64)> },
    Carry { kind: &'static str, made: Option<PartRef>, belt_a: Option<u32> },
    Shelf { s0: (f64, f64), x0: f64, kind: &'static str, mode: u8, inner: Option<Box<Gesture>>, last: (f64, f64), v: f64 },
    Glyph,
    Button(Btn),
    /// The middle of a compact bezel: a tap puts it away.
    Hub,
    /// A sector of the bezel: what it held when pressed.
    Bezel { i: usize, v0: super::bezel::Val, s0: (f64, f64), owner: Option<Sel> },
    /// A press that has done its work (a link made or cancelled).
    Nothing,
    Cam(u32),
    Probe(u32),
    Field { id: u32, off: (f64, f64), edge: bool },
    Arm { id: u32, shaft: Option<(f64, f64)> },
    /// The finger left on an arm after a two-finger grab: it only carries the arm to the shelf.
    ArmBody(u32),
    Pad { id: u32, off: (f64, f64), rim: bool },
    Span { reader: bool, id: u32, p0: (f64, f64), k: [f64; 4], end: u8, note: i64, turn: f64 },
    Belt(u32),
    Cog { id: u32, p0: (f64, f64), start: Vec<(u32, f64, f64)>, last: Option<(f64, f64)> },
    Rim { id: u32, grow: Option<cogs_sim::Resizer> },
    Knob { id: u32, mode: u8, at: f64, acc: f64, th0: f64, dial: f64 },
    Notes { id: u32, cell0: Option<(f64, f64)>, paint: Option<bool>, mode: u8, sweep: (f64, f64, i64) },
    PitBox { id: u32, at: (f64, f64) },
    PitIn { id: u32, w0: (f64, f64), ball: Option<u32>, held: bool, last: Option<(f64, f64)> },
}

/// The thumb row's buttons.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Btn {
    Edit,
    /// A sphere's notes.
    Balls,
    Rec,
    Zero,
    Copy,
    Paste,
    Delete,
    Midi,
    Undo,
    Redo,
    Start,
    /// Bring the whole machine back into view.
    Fit,
    /// The room's ring.
    Room,
}

/// Two fingers on a part: the midpoint moves it, the spread sizes it. Never turns it.
pub(crate) enum Grip {
    /// `anchor`: the cog the resize keeps its mesh with, and snaps its ratio against.
    Cog { id: u32, start: Vec<(u32, f64, f64)>, last: Option<(f64, f64)>, b: (f64, f64), n0: f64, raw: f64, anchor: Option<u32> },
    Field { id: u32, x: f64, y: f64, r: f64 },
    Pad { id: u32, x: f64, y: f64, r: f64 },
    Pit { id: u32, x: f64, y: f64, d: f64 },
    Arm { id: u32, l: f64 },
    SpanSeated { id: u32, mid: f64, half: f64 },
    SpanFree { reader: bool, id: u32, mx: f64, my: f64, hx: f64, hy: f64, l: f64, note: i64 },
}

impl Gesture {
    /// The part it acts on, for double taps and the group.
    pub(crate) fn owner(&self) -> Option<Sel> {
        Some(match *self {
            Gesture::Cam(i) => Sel::Cam(i),
            Gesture::Probe(i) => Sel::Probe(i),
            Gesture::Field { id, .. } => Sel::Field(id),
            Gesture::Arm { id, .. } | Gesture::ArmBody(id) => Sel::Arm(id),
            Gesture::Pad { id, .. } => Sel::Pad(id),
            Gesture::Span { reader: true, id, .. } => Sel::Reader(id),
            Gesture::Span { reader: false, id, .. } => Sel::String(id),
            Gesture::Belt(i) => Sel::Belt(i),
            Gesture::Cog { id, .. } | Gesture::Rim { id, .. } | Gesture::Knob { id, .. } | Gesture::Notes { id, .. } => Sel::Cog(id),
            Gesture::PitBox { id, .. } | Gesture::PitIn { id, .. } => Sel::Pit(id),
            Gesture::Bezel { owner, .. } => return owner,
            _ => return None,
        })
    }
}

fn idx<T>(v: &[T], f: impl Fn(&T) -> bool) -> Option<usize> {
    v.iter().position(f)
}

impl Stage {
    // ---------- input plumbing ----------
    pub(super) fn input(&mut self, ui: &egui::Ui, w: &mut World) {
        let (events, mods) = ui.input(|i| (i.events.clone(), i.modifiers));
        self.shift = mods.shift;
        let o = self.rect.min;
        let local = |p: Pos2| ((p.x - o.x) as f64, (p.y - o.y) as f64);
        for e in events {
            match e {
                Event::Touch { id, phase, pos, .. } => {
                    let id = id.0;
                    let s = local(pos);
                    // a browser may have told of this finger as a mouse first: that press will never be let go
                    if !self.seen_touch && self.mouse_down {
                        self.mouse_down = false;
                        if !self.route_sheet(w, MOUSE, s, TouchPhase::Cancel) {
                            self.up(w, MOUSE, s, true);
                        }
                    }
                    self.seen_touch = true;
                    if self.route_sheet(w, id, s, phase) {
                        continue;
                    }
                    match phase {
                        TouchPhase::Start => self.down(w, id, s, true, mods),
                        TouchPhase::Move => self.moved(w, id, s),
                        TouchPhase::End => self.up(w, id, s, false),
                        TouchPhase::Cancel => self.up(w, id, s, true),
                    }
                }
                Event::PointerMoved(p) if !self.seen_touch => {
                    let s = local(p);
                    self.hover = Some(s);
                    if self.mouse_down && self.route_sheet(w, MOUSE, s, TouchPhase::Move) {
                        continue;
                    }
                    if self.mouse_down {
                        self.moved(w, MOUSE, s);
                    }
                }
                Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers } if !self.seen_touch => {
                    let s = local(pos);
                    let phase = if pressed { TouchPhase::Start } else { TouchPhase::End };
                    if (pressed && self.rect.contains(pos) || !pressed) && self.route_sheet(w, MOUSE, s, phase) {
                        self.mouse_down = pressed;
                        continue;
                    }
                    if pressed && self.rect.contains(pos) {
                        self.mouse_down = true;
                        self.down(w, MOUSE, s, false, modifiers);
                    } else if !pressed && self.mouse_down {
                        self.mouse_down = false;
                        self.up(w, MOUSE, s, false);
                    }
                }
                Event::PointerGone if !self.seen_touch => self.hover = None,
                Event::MouseWheel { unit, delta, modifiers, .. } => {
                    let k = match unit {
                        egui::MouseWheelUnit::Point => 1.0,
                        egui::MouseWheelUnit::Line => 33.0,
                        egui::MouseWheelUnit::Page => self.ch,
                    };
                    let s = self.hover.unwrap_or((self.cw / 2.0, self.ch / 2.0));
                    self.wheel(w, s, -delta.x as f64 * k, -delta.y as f64 * k, modifiers.ctrl || modifiers.command);
                }
                Event::Key { key, pressed: true, modifiers, .. } => self.key(w, key, modifiers),
                _ => {}
            }
        }
        if let Some(t) = &self.trackpad {
            if self.now - t.4 > 0.25 {
                self.end_trackpad(w);
            }
        }
    }

    /// Fingers that land on an open sheet are the sheet's until they lift. True: the event was the sheet's.
    fn route_sheet(&mut self, w: &mut World, id: u64, s: (f64, f64), phase: TouchPhase) -> bool {
        match phase {
            TouchPhase::Start => {
                let Some(o) = self.sheet_hit(w, s) else { return false };
                self.sheet_fingers.insert(id, o);
                self.sheet_down(w, id, s, o);
                true
            }
            TouchPhase::Move => match self.sheet_fingers.get(&id).copied() {
                Some(o) => {
                    self.sheet_move(w, id, s, o);
                    true
                }
                None => false,
            },
            TouchPhase::End | TouchPhase::Cancel => match self.sheet_fingers.remove(&id) {
                Some(o) => {
                    self.sheet_up(w, id, phase == TouchPhase::Cancel, o);
                    true
                }
                None => false,
            },
        }
    }
    fn down(&mut self, w: &mut World, id: u64, s: (f64, f64), touch: bool, mods: egui::Modifiers) {
        self.fingers.insert(id, Finger { pos: s, touch });
        self.down_t = self.now;
        self.fly = None;
        if self.fingers.len() >= 2 {
            if self.grip.is_some() {
                return;
            }
            if self.fingers.len() == 2 && touch && self.start_grip(w, id) {
                return;
            }
            self.drop_gesture(w);
            self.repinch(w);
            return;
        }
        if self.pinch.is_none() {
            // a press says nothing by itself (a bezel sector without words excepted): hints report changes
            self.readout = None;
            self.readout_held = false;
            let g = self.pointer_gesture(w, s, touch, mods);
            let hint0 = self.readout_of(w, &g);
            self.gesture = Some(Active { id, g, s0: s, last: s, t0: self.now, moved: false, hint0, hinted: false });
        }
    }
    fn moved(&mut self, w: &mut World, id: u64, s: (f64, f64)) {
        let Some(f) = self.fingers.get_mut(&id) else { return };
        f.pos = s;
        if let Some(g) = &self.grip {
            if id == g.a || id == g.b {
                self.move_grip(w);
            }
            return;
        }
        if let Some(p) = &self.pinch {
            if self.fingers.len() >= 2 {
                let (mid, d) = self.pinch_now();
                let (zoom, anchor, d0) = (p.zoom, p.anchor, p.d);
                self.zoom_at(w, mid, zoom * d / d0, anchor);
            }
            return;
        }
        let Some(a) = &mut self.gesture else { return };
        if a.id != id {
            return;
        }
        a.last = s;
        if !a.moved && dist(s, a.s0) < 8.0 {
            return;
        }
        a.moved = true;
        let mut g = std::mem::replace(&mut a.g, Gesture::Glyph);
        self.g_move(w, &mut g, s);
        // the readout follows the gesture, from when its value first differs from the press. These have no value to
        // compare: moving at all is the news.
        let text = self.readout_of(w, &g);
        let always = matches!(g, Gesture::Pan { .. } | Gesture::Carry { .. } | Gesture::Shelf { .. } | Gesture::Group { .. } | Gesture::Marquee { .. });
        let mut speak = false;
        if let Some(a) = &mut self.gesture {
            a.g = g;
            a.hinted |= always || text != a.hint0;
            speak = a.hinted;
        }
        if let (true, Some(t)) = (speak, text) {
            self.say(t, true);
        }
    }
    fn up(&mut self, w: &mut World, id: u64, s: (f64, f64), cancel: bool) {
        if self.fingers.remove(&id).is_none() {
            return;
        }
        if self.readout_held {
            if let Some(r) = &mut self.readout {
                r.1 = self.now + 1.8;
            }
            self.readout_held = false;
        }
        if let Some(g) = self.grip.take() {
            if id != g.a && id != g.b {
                self.grip = Some(g);
                return;
            }
            let other = if id == g.a { g.b } else { g.a };
            if let Some(f) = self.fingers.get(&other).copied() {
                let body = self.grip_body(w, &g.hand, f.pos);
                self.gesture = Some(Active { id: other, g: body, s0: f.pos, last: f.pos, t0: self.now, moved: true, hint0: None, hinted: false });
            }
            self.changed(w, true);
            return;
        }
        if self.pinch.is_some() {
            if self.fingers.len() >= 2 {
                self.repinch(w);
                return;
            }
            // the camera is not an edit: nothing here to undo
            self.pinch = None;
            if let Some((&fid, f)) = self.fingers.iter().next() {
                let g = Gesture::Pan { v0: (w.view.x, w.view.y), a: self.to_world(w, f.pos), boxing: false };
                self.gesture = Some(Active { id: fid, g, s0: f.pos, last: f.pos, t0: self.now, moved: true, hint0: None, hinted: false });
            }
            return;
        }
        let Some(a) = self.gesture.take() else { return };
        if a.id != id {
            self.gesture = Some(a);
            return;
        }
        let mut g = a.g;
        if cancel {
            self.g_cancel(w, &mut g);
            return;
        }
        if a.moved || self.now - a.t0 > 0.45 {
            self.g_up(w, &mut g, s);
            return;
        }
        let o = g.owner();
        let dbl = self.last_tap.map_or(false, |l| self.now - l.t < 0.4 && dist(s, l.s) < 30.0 && l.o == o);
        if dbl && self.g_dbl(w, &mut g, s) {
            self.last_tap = None;
        } else {
            self.last_tap = Some(LastTap { t: self.now, s, o });
            self.g_tap(w, &mut g, s);
        }
    }
    fn drop_gesture(&mut self, w: &mut World) {
        if let Some(a) = self.gesture.take() {
            let mut g = a.g;
            if !self.g_cancel(w, &mut g) && a.moved {
                self.g_up(w, &mut g, a.last);
            }
        }
        self.scrap = false;
        self.crank = None;
        self.carrying = false;
        self.carry_ghost = None;
        self.band = None;
        self.knob = None;
        if let Some(s) = &mut self.spin {
            s.held = false;
        }
    }
    fn pinch_now(&self) -> ((f64, f64), f64) {
        let mut it = self.fingers.values();
        let (a, b) = (it.next().unwrap().pos, it.next().unwrap().pos);
        (((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), dist(a, b).max(1.0))
    }
    fn repinch(&mut self, w: &World) {
        let (mid, d) = self.pinch_now();
        self.pinch = Some(Pinch { d, zoom: w.s.zoom, anchor: self.to_world(w, mid) });
    }
    fn grip_now(&self, g: &GripState) -> ((f64, f64), f64) {
        let (a, b) = (self.fingers[&g.a].pos, self.fingers[&g.b].pos);
        (((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0), dist(a, b).max(1.0))
    }
    /// A second finger takes the part under the first, wherever it lands. False: nothing to take.
    fn start_grip(&mut self, w: &mut World, id: u64) -> bool {
        if self.pinch.is_some() {
            return false;
        }
        let Some(a) = &self.gesture else { return false };
        let Some(hand) = self.grip_of(w, &a.g) else { return false };
        let first = a.id;
        self.gesture = None;
        self.pit_band = None;
        let hint0 = self.grip_readout(w, &hand);
        let mut g = GripState { a: first, b: id, hand, mid0: (0.0, 0.0), d0: 1.0, hint0, hinted: false };
        let (mid, d) = self.grip_now(&g);
        g.mid0 = mid;
        g.d0 = d;
        self.grip = Some(g);
        true
    }
    fn move_grip(&mut self, w: &mut World) {
        let Some(mut g) = self.grip.take() else { return };
        let (mid, d) = self.grip_now(&g);
        self.scrap = self.off_stage(mid);
        let spread = d - g.d0;
        let k = 1.0 + spread.signum() * (spread.abs() - SPREAD_DEAD).max(0.0) / g.d0;
        let m = ((mid.0 - g.mid0.0) / w.s.zoom, (mid.1 - g.mid0.1) / w.s.zoom);
        self.grip_hold(w, &mut g.hand, m, k);
        let text = self.grip_readout(w, &g.hand);
        g.hinted |= text != g.hint0;
        if g.hinted {
            self.say(text, true);
        }
        self.grip = Some(g);
    }

    // ---------- what a press begins ----------
    fn pointer_gesture(&mut self, w: &mut World, s: (f64, f64), touch: bool, mods: egui::Modifiers) -> Gesture {
        if (mods.ctrl || mods.command) && !self.off_stage(s) {
            let a = self.to_world(w, s);
            self.marquee = Some((a, a));
            return Gesture::Marquee { a };
        }
        if let Some(m) = self.linking.take() {
            if !self.off_stage(s) {
                self.link_gesture(w, m, s);
                return Gesture::Nothing;
            }
        }
        if let Some(kind) = self.armed {
            if !self.off_stage(s) {
                let mut g = Gesture::Carry { kind, made: None, belt_a: None };
                self.carrying = true;
                self.g_move(w, &mut g, s);
                return g;
            }
        }
        let g = self.begin(w, s, touch);
        let o = g.owner();
        if self.group.len() > 1 && o.map_or(false, |o| self.group.contains(&o)) {
            return self.group_gesture(w, s);
        }
        // a slide on empty ground pans and keeps the selection; only a tap there clears it
        if !self.group.is_empty() {
            if let Some(o) = o {
                let disc = matches!(o, Sel::Cog(i) if w.cog(i).map_or(false, |c| c.track.is_some()));
                if !disc {
                    self.group.clear();
                }
            }
        }
        g
    }

    fn begin(&mut self, w: &mut World, s: (f64, f64), touch: bool) -> Gesture {
        if let Some(g) = self.chrome_hit(w, s) {
            return g;
        }
        let p = self.to_world(w, s);
        let z = w.s.zoom;
        let d = |q: (f64, f64)| dist(q, p) * z;
        // the nearest small part under the finger wins
        let mut best: Option<(f64, Gesture)> = None;
        let mut near = |dd: f64, reach: f64, g: Gesture| {
            if dd <= reach && best.as_ref().map_or(true, |b| dd < b.0) {
                best = Some((dd, g));
            }
        };
        for (i, a) in w.arms.iter().enumerate() {
            near(d(w.arm_tip(i)) - 3.3 * z, 20.0, Gesture::Arm { id: a.id, shaft: None });
            let c = &w.cogs[a.cog_ix];
            let ang = w.render_th(a.cog_ix) + a.a;
            let (r0, r1) = (a.r0 + a.len * 0.25, a.r0 + a.len);
            let (ux, uy) = (ang.cos(), ang.sin());
            let sd = seg_dist(p, c.x + ux * r0, c.y + uy * r0, c.x + ux * r1, c.y + uy * r1) * z + 10.0;
            near(sd, 22.0, Gesture::Arm { id: a.id, shaft: Some((a.a, (p.1 - c.y).atan2(p.0 - c.x))) });
        }
        for m in &w.cams {
            near(d(cam_tip(m, &w.cogs[m.cog_ix], w.render_th(m.cog_ix))) - 3.0 * z, 20.0, Gesture::Cam(m.id));
        }
        for pr in &w.probes {
            if w.probe_alive(pr) {
                near(d(w.probe_point(pr)) - 6.0 * z, 20.0, Gesture::Probe(pr.id));
            }
        }
        for o in &w.pads {
            let dd = d((o.x, o.y));
            let r = o.r * z;
            near(dd - r, (22.0 - r).max(8.0), Gesture::Pad { id: o.id, off: (o.x - p.0, o.y - p.1), rim: !touch && r >= 18.0 && dd >= r - 8.0 });
        }
        let span = |reader: bool, id: u32, x1: f64, y1: f64, x2: f64, y2: f64, end: u8| Gesture::Span {
            reader, id, p0: p, k: [x1, y1, x2, y2], end, note: 0, turn: 0.0,
        };
        for o in &w.readers {
            // a reader lying on a plate takes only a press close to it: the rest of the face is the plate's
            let on_plate = o.on_ix.map_or(false, |ci| dist(p, (w.cogs[ci].x, w.cogs[ci].y)) <= w.cogs[ci].radius());
            let (end, body) = if on_plate { (16.0, 20.0) } else { (22.0, 22.0) };
            near(d((o.x1, o.y1)), end, span(true, o.id, o.x1, o.y1, o.x2, o.y2, 1));
            near(d((o.x2, o.y2)), end, span(true, o.id, o.x1, o.y1, o.x2, o.y2, 2));
            near(seg_dist(p, o.x1, o.y1, o.x2, o.y2) * z + 8.0, body, span(true, o.id, o.x1, o.y1, o.x2, o.y2, 0));
        }
        for o in &w.strings {
            near(d((o.x1, o.y1)), 22.0, span(false, o.id, o.x1, o.y1, o.x2, o.y2, 1));
            near(d((o.x2, o.y2)), 22.0, span(false, o.id, o.x1, o.y1, o.x2, o.y2, 2));
            near(seg_dist(p, o.x1, o.y1, o.x2, o.y2) * z + 8.0, 22.0, span(false, o.id, o.x1, o.y1, o.x2, o.y2, 0));
        }
        for (i, o) in w.belts.iter().enumerate() {
            for l in w.belt_lines(i).unwrap_or([[f64::NAN; 4]; 2]) {
                if l[0].is_finite() {
                    near(seg_dist(p, l[0], l[1], l[2], l[3]) * z + 10.0, 22.0, Gesture::Belt(o.id));
                }
            }
        }
        let mut boxed = None;
        for o in &w.pits {
            let r = o.w / 2.0;
            let dd = dist(p, (o.x, o.y));
            let edge = (dd - r).abs() * z;
            if edge <= 12.0 {
                near(edge + 6.0, 18.0, Gesture::PitBox { id: o.id, at: p });
            }
            if dd <= r {
                boxed = Some(o.id);
            }
        }
        for f in &w.fields {
            near(d((f.x, f.y)) - 4.0, 18.0, Gesture::Field { id: f.id, off: (f.x - p.0, f.y - p.1), edge: false });
            if self.is_sel(Sel::Field(f.id)) && !touch {
                near(d(field_handle(f)), 20.0, Gesture::Field { id: f.id, off: (0.0, 0.0), edge: true });
            }
        }
        let c = w.pick_cog(p.0, p.1, false, 14.0 / z);
        let (cd, rs) = c.map_or((0.0, 0.0), |c| (d((w.cogs[c].x, w.cogs[c].y)), w.cogs[c].radius() * z));
        if let Some(ci) = c {
            if rs >= 30.0 && !touch {
                near((cd - rs).abs(), 14.0, Gesture::Rim { id: w.cogs[ci].id, grow: None });
            }
        }
        if let Some(kc) = self.knob_cog(w) {
            let (k, _) = self.knob_at(w, kc, -std::f64::consts::FRAC_PI_2);
            near(dist(s, k) - KNOB_R, (22.0 - KNOB_R).max(8.0), Gesture::Knob { id: kc, mode: 0, at: 0.0, acc: 0.0, th0: 0.0, dial: 0.0 });
        }
        // the hub is the plate's: what merely passes over it (an arm, a reader, a belt) does not take the press
        if c.is_some() && cd <= 24f64.max(rs * 0.22).min(rs * 0.7) && matches!(best, Some((_, Gesture::Arm { .. } | Gesture::Span { .. } | Gesture::Belt(_)))) {
            best = None;
        }
        if let Some((_, g)) = best {
            return self.started(w, g, s);
        }
        if let Some(ci) = c {
            let cg = &w.cogs[ci];
            if self.pins_live(w, cg) && self.is_sel(Sel::Cog(cg.id)) && cd > 22f64.max(rs * 0.2) && w.cell_at(ci, p.0, p.1).is_some() {
                let cell0 = w.cell_at(ci, p.0, p.1);
                return Gesture::Notes { id: cg.id, cell0, paint: None, mode: 0, sweep: (0.0, 0.0, -1) };
            }
            return self.cog_gesture(w, cg.id, p);
        }
        if let Some(id) = boxed {
            let ball = w.pits.iter().find(|q| q.id == id).and_then(|q| q.ball_under(p.0, p.1, w.clock.view, z)).map(|i| w.pits.iter().find(|q| q.id == id).unwrap().balls[i].id);
            return Gesture::PitIn { id, w0: p, ball, held: false, last: None };
        }
        Gesture::Pan { v0: (w.view.x, w.view.y), a: p, boxing: false }
    }
    /// Gestures that do something the moment they begin.
    fn started(&mut self, w: &mut World, g: Gesture, s: (f64, f64)) -> Gesture {
        match g {
            Gesture::Knob { id, .. } => self.knob_gesture(w, id, s),
            Gesture::Span { reader: true, id, p0, k, end, .. } => {
                let ri = idx(&w.readers, |r| r.id == id).unwrap();
                let r = &w.readers[ri];
                let turn = match (r.on_ix, r.seat) {
                    (Some(ci), Some(seat)) => seat.ang - (p0.1 - w.cogs[ci].y).atan2(p0.0 - w.cogs[ci].x),
                    _ => 0.0,
                };
                Gesture::Span { reader: true, id, p0, k, end, note: 0, turn }
            }
            Gesture::Span { reader: false, id, p0, k, end, .. } => {
                let note = w.strings.iter().find(|r| r.id == id).map_or(0, |s| s.midi().round() as i64);
                Gesture::Span { reader: false, id, p0, k, end, note, turn: 0.0 }
            }
            g => g,
        }
    }
    fn cog_gesture(&mut self, w: &World, id: u32, p0: (f64, f64)) -> Gesture {
        let c = w.cog(id).unwrap();
        let stack: Vec<usize> = w.stack_at(c.x, c.y).collect();
        let stuck = stack.iter().any(|&o| w.clashes(o, &stack));
        Gesture::Cog { id, p0, start: stack.iter().map(|&o| (w.cogs[o].id, w.cogs[o].x, w.cogs[o].y)).collect(), last: if stuck { None } else { Some((0.0, 0.0)) } }
    }
    fn knob_gesture(&mut self, w: &mut World, id: u32, s: (f64, f64)) -> Gesture {
        let ci = w.cog_ix(id).unwrap();
        let k = { let c = &w.cogs[ci]; self.to_screen(w, c.x, c.y) };
        let at = (s.1 - k.1).atan2(s.0 - k.0);
        self.knob = Some((id, at));
        self.buzz(6);
        if w.clock.paused {
            self.caught = false;
            return Gesture::Knob { id, mode: 0, at, acc: 0.0, th0: w.render_th(ci), dial: 0.0 };
        }
        if w.cogs[ci].motor.is_some() {
            self.crank = Some(id);
            return Gesture::Knob { id, mode: 1, at, acc: 0.0, th0: 0.0, dial: gauge_angle(w.rpm(ci)) };
        }
        let was = self.spin.take().filter(|p| p.c == id);
        let rpm = was.as_ref().map(|p| p.rpm).unwrap_or(if w.cogs[ci].jam { 0.0 } else { w.rpm(ci) });
        let v = was.as_ref().map(|p| p.v).unwrap_or(0.0);
        self.spin = Some(Spin { c: id, held: true, t: self.now, rpm, v, samples: vec![(self.now, 0.0)] });
        Gesture::Knob { id, mode: 2, at, acc: 0.0, th0: 0.0, dial: 0.0 }
    }
    fn group_gesture(&mut self, w: &mut World, s: (f64, f64)) -> Gesture {
        // seated readers in the group come free unless their disc moves too
        for g in self.group.clone() {
            if let Sel::Reader(id) = g {
                let ri = idx(&w.readers, |r| r.id == id).unwrap();
                if let Some(on) = w.readers[ri].on {
                    if !self.group.contains(&Sel::Cog(on)) {
                        w.readers[ri].on = None;
                        w.readers[ri].seat = None;
                    }
                }
            }
        }
        let p0 = self.to_world(w, s);
        let mut start = Vec::new();
        for g in &self.group {
            let k = match *g {
                Sel::Cog(i) => w.cog(i).map(|o| [o.x, o.y, 0.0, 0.0]),
                Sel::Pad(i) => w.pads.iter().find(|o| o.id == i).map(|o| [o.x, o.y, 0.0, 0.0]),
                Sel::Field(i) => w.fields.iter().find(|o| o.id == i).map(|o| [o.x, o.y, 0.0, 0.0]),
                Sel::Pit(i) => w.pits.iter().find(|o| o.id == i).map(|o| [o.x, o.y, 0.0, 0.0]),
                Sel::Reader(i) => w.readers.iter().find(|o| o.id == i).map(|o| [o.x1, o.y1, o.x2, o.y2]),
                Sel::String(i) => w.strings.iter().find(|o| o.id == i).map(|o| [o.x1, o.y1, o.x2, o.y2]),
                _ => None,
            };
            if let Some(k) = k {
                start.push((*g, k));
            }
        }
        let cogs: Vec<(u32, f64, f64)> = self.group.iter().filter_map(|g| if let Sel::Cog(i) = g { w.cog(*i).map(|c| (*i, c.x, c.y)) } else { None }).collect();
        let ix: Vec<usize> = cogs.iter().filter_map(|c| w.cog_ix(c.0)).collect();
        let stuck = ix.iter().any(|&o| w.clashes(o, &ix));
        Gesture::Group { p0, start, cogs, last: if stuck { None } else { Some((0.0, 0.0)) } }
    }

    // ---------- each gesture's move ----------
    pub(super) fn g_move(&mut self, w: &mut World, g: &mut Gesture, s: (f64, f64)) {
        let q = self.to_world(w, s);
        match g {
            Gesture::Pan { v0, a, boxing } => {
                if *boxing {
                    self.marquee = Some((*a, q));
                    self.group = box_select(w, *a, q);
                    return;
                }
                let s0 = self.gesture_s0();
                w.view.x = v0.0 - (s.0 - s0.0) / w.s.zoom;
                w.view.y = v0.1 - (s.1 - s0.1) / w.s.zoom;
            }
            Gesture::Marquee { a } => self.marquee = Some((*a, q)),
            Gesture::Group { p0, start, cogs, last } => {
                self.scrap = self.off_stage(s);
                let (dx, dy) = if self.scrap {
                    (q.0 - p0.0, q.1 - p0.1)
                } else {
                    let ms: Vec<(usize, f64, f64)> = cogs.iter().filter_map(|c| w.cog_ix(c.0).map(|i| (i, c.1, c.2))).collect();
                    let r = w.clear_shift(&ms, q.0 - p0.0, q.1 - p0.1, *last);
                    *last = Some(r);
                    r
                };
                for (o, k) in start.iter() {
                    match *o {
                        Sel::Cog(i) => {
                            if let Some(c) = w.cog_mut(i) {
                                c.x = k[0] + dx;
                                c.y = k[1] + dy;
                            }
                        }
                        Sel::Pad(i) => {
                            if let Some(c) = w.pads.iter_mut().find(|o| o.id == i) {
                                c.x = k[0] + dx;
                                c.y = k[1] + dy;
                            }
                        }
                        Sel::Field(i) => {
                            if let Some(c) = w.fields.iter_mut().find(|o| o.id == i) {
                                c.x = k[0] + dx;
                                c.y = k[1] + dy;
                            }
                        }
                        Sel::Pit(i) => {
                            if let Some(c) = w.pits.iter_mut().find(|o| o.id == i) {
                                let (ex, ey) = (k[0] + dx - c.x, k[1] + dy - c.y);
                                c.shift(ex, ey);
                            }
                        }
                        Sel::Reader(i) => {
                            if let Some(c) = w.readers.iter_mut().find(|o| o.id == i) {
                                c.x1 = k[0] + dx;
                                c.y1 = k[1] + dy;
                                c.x2 = k[2] + dx;
                                c.y2 = k[3] + dy;
                            }
                        }
                        Sel::String(i) => {
                            if let Some(c) = w.strings.iter_mut().find(|o| o.id == i) {
                                c.x1 = k[0] + dx;
                                c.y1 = k[1] + dy;
                                c.x2 = k[2] + dx;
                                c.y2 = k[3] + dy;
                            }
                        }
                        _ => {}
                    }
                }
                w.solve();
            }
            Gesture::Carry { kind, made, belt_a } => self.carry_move(w, kind, made, belt_a, s),
            Gesture::Shelf { s0, x0, kind, mode, inner, last, v } => {
                if *mode == 0 {
                    let carry = s.1 < self.shelf_top() || 2.0 * (s.1 - s0.1).abs() >= (s.0 - s0.0).abs() || self.shelf_max() == 0.0;
                    *mode = if carry { 1 } else { 2 };
                    if carry {
                        *inner = Some(Box::new(Gesture::Carry { kind, made: None, belt_a: None }));
                        self.carrying = true;
                    }
                } else if *mode == 2 && s.1 < self.shelf_top() - 8.0 {
                    *mode = 1;
                    *kind = self.slot_at(s.0.clamp(0.0, self.cw - 1.0));
                    *inner = Some(Box::new(Gesture::Carry { kind, made: None, belt_a: None }));
                    self.carrying = true;
                    self.buzz(6);
                }
                if *mode == 1 {
                    if let Some(i) = inner {
                        self.g_move(w, i, s);
                    }
                    return;
                }
                self.set_shelf_x(*x0 - (s.0 - s0.0));
                if self.now > last.1 {
                    *v = (last.0 - s.0) / (self.now - last.1);
                    *last = (s.0, self.now);
                }
            }
            Gesture::Cam(id) => {
                self.scrap = self.off_stage(s);
                if let Some(m) = w.cams.iter_mut().find(|m| m.id == *id) {
                    let c = &w.cogs[m.cog_ix];
                    m.ang = (q.1 - c.y).atan2(q.0 - c.x);
                }
            }
            Gesture::Probe(id) => {
                self.scrap = self.off_stage(s);
                if !self.scrap {
                    if let Some(i) = idx(&w.probes, |p| p.id == *id) {
                        w.probe_attach(i, q.0, q.1, w.s.zoom, true);
                    }
                }
            }
            Gesture::Field { id, off, edge } => {
                if let Some(f) = w.fields.iter_mut().find(|f| f.id == *id) {
                    if *edge {
                        f.r = (q.0 - f.x).hypot(q.1 - f.y).clamp(30.0, 800.0);
                    } else {
                        f.x = q.0 + off.0;
                        f.y = q.1 + off.1;
                    }
                }
                if !*edge {
                    self.scrap = self.off_stage(s);
                }
            }
            Gesture::Arm { id, shaft } => {
                self.scrap = self.off_stage(s);
                let Some(ai) = idx(&w.arms, |a| a.id == *id) else { return };
                let ci = w.arms[ai].cog_ix;
                let (cx, cy) = (w.cogs[ci].x, w.cogs[ci].y);
                let rth = w.render_th(ci);
                let a = &mut w.arms[ai];
                match shaft {
                    Some((a0, g0)) => a.a = cogs_sim::wrap(*a0 + (q.1 - cy).atan2(q.0 - cx) - *g0),
                    None => {
                        a.a = cogs_sim::wrap((q.1 - cy).atan2(q.0 - cx) - rth);
                        a.len = ((q.0 - cx).hypot(q.1 - cy) - a.r0).max(4.0);
                    }
                }
            }
            Gesture::Pad { id, off, rim } => {
                if !*rim {
                    self.scrap = self.off_stage(s);
                }
                if let Some(o) = w.pads.iter_mut().find(|o| o.id == *id) {
                    if *rim {
                        o.r = (q.0 - o.x).hypot(q.1 - o.y).clamp(3.0, 200.0);
                    } else {
                        o.x = q.0 + off.0;
                        o.y = q.1 + off.1;
                    }
                }
            }
            Gesture::Span { reader, id, p0, k, end, note, turn } => {
                let (dx, dy) = (q.0 - p0.0, q.1 - p0.1);
                if *end == 0 {
                    self.scrap = self.off_stage(s);
                }
                if *reader {
                    let Some(ri) = idx(&w.readers, |r| r.id == *id) else { return };
                    if let (Some(ci), Some(_)) = (w.readers[ri].on_ix, w.readers[ri].seat) {
                        let c = w.cogs[ci].clone();
                        let rr = c.radius();
                        let d = (q.0 - c.x).hypot(q.1 - c.y);
                        if d <= rr + UNSEAT_PX / w.s.zoom && !self.scrap {
                            let ang = (q.1 - c.y).atan2(q.0 - c.x) + *turn;
                            let caught = if *end == 0 { self.catch_zero(ang) } else { 0.0 };
                            let r = &mut w.readers[ri];
                            let seat = r.seat.as_mut().unwrap();
                            match *end {
                                1 => seat.f0 = (d / rr).clamp(0.0, 1.0),
                                2 => seat.f1 = (d / rr).clamp(0.0, 1.0),
                                _ => seat.ang = caught,
                            }
                            place_seated(r, &c);
                            return;
                        }
                        let r = &mut w.readers[ri];
                        r.on = None;
                        r.on_ix = None;
                        self.buzz(10);
                        *k = [r.x1 - dx, r.y1 - dy, r.x2 - dx, r.y2 - dy];
                    }
                    let r = &mut w.readers[ri];
                    if *end != 2 {
                        r.x1 = k[0] + dx;
                        r.y1 = k[1] + dy;
                    }
                    if *end != 1 {
                        r.x2 = k[2] + dx;
                        r.y2 = k[3] + dy;
                    }
                    if *end == 0 && !self.scrap {
                        if let Some(di) = w.pick_cog(q.0, q.1, true, 0.0) {
                            let d = w.cogs[di].clone();
                            let rings = d.track.as_ref().unwrap().rings();
                            let keep = w.readers[ri].seat.unwrap_or(Seat { ang: 0.0, f0: (d.ring_r(0.0) - 6.0) / d.radius(), f1: (d.ring_r((rings - 1) as f64) + 6.0) / d.radius() });
                            let ang = self.catch_zero((q.1 - d.y).atan2(q.0 - d.x));
                            let r = &mut w.readers[ri];
                            r.on = Some(d.id);
                            r.on_ix = Some(di);
                            r.seat = Some(Seat { ang, f0: keep.f0, f1: keep.f1 });
                            *turn = 0.0;
                            place_seated(r, &d);
                            self.buzz(8);
                        }
                    }
                } else {
                    let Some(si) = idx(&w.strings, |r| r.id == *id) else { return };
                    let r = &mut w.strings[si];
                    if *end != 2 {
                        r.x1 = k[0] + dx;
                        r.y1 = k[1] + dy;
                    }
                    if *end != 1 {
                        r.x2 = k[2] + dx;
                        r.y2 = k[3] + dy;
                    }
                    if *end != 0 {
                        let m = r.midi().round() as i64;
                        if m != *note {
                            *note = m;
                            self.buzz(3);
                        }
                    }
                }
            }
            Gesture::Belt(_) | Gesture::ArmBody(_) => self.scrap = self.off_stage(s),
            Gesture::Cog { p0, start, last, id } => {
                self.scrap = self.off_stage(s);
                let (dx, dy) = (q.0 - p0.0, q.1 - p0.1);
                let ms: Vec<(usize, f64, f64)> = start.iter().filter_map(|c| w.cog_ix(c.0).map(|i| (i, c.1, c.2))).collect();
                if self.scrap {
                    for &(o, x, y) in &ms {
                        w.cogs[o].x = x + dx;
                        w.cogs[o].y = y + dy;
                    }
                    w.solve();
                    return;
                }
                let was = *last;
                let r = w.clear_shift(&ms, dx, dy, *last);
                *last = Some(r);
                let blocked = was.is_some() && (r.0 != dx || r.1 != dy);
                let stack: Vec<usize> = ms.iter().map(|m| m.0).collect();
                let Some(ci) = w.cog_ix(*id) else { return };
                let snapped = w.snap_clear(ci, &stack) || blocked;
                if snapped && !self.snapped {
                    self.buzz(8);
                }
                self.snapped = snapped;
                w.solve();
            }
            Gesture::Rim { id, grow } => {
                let Some(ci) = w.cog_ix(*id) else { return };
                if grow.is_none() {
                    *grow = Some(w.resizer(ci));
                }
                let g = grow.as_ref().unwrap();
                let n = (g.radius_through(w, q.0, q.1) * TAU / TOOTH).round();
                w.set_teeth(g, n);
            }
            Gesture::Knob { id, mode, at, acc, th0, dial } => {
                let Some(ci) = w.cog_ix(*id) else { return };
                let k = { let c = &w.cogs[ci]; self.to_screen(w, c.x, c.y) };
                let a = (s.1 - k.1).atan2(s.0 - k.0);
                self.knob = Some((*id, a));
                match *mode {
                    0 => {
                        *acc += wrap_pi(a - *at);
                        *at = a;
                        let ph = self.catch_zero(cogs_sim::wrap(*th0 + *acc));
                        w.set_phase(ci, ph);
                    }
                    1 => {
                        *dial = (*dial + wrap_pi(a - *at)).clamp(-GAUGE_SWEEP, GAUGE_SWEEP);
                        *at = a;
                        let rpm = gauge_rpm(*dial);
                        w.retimed(|w| {
                            w.drive(ci, rpm);
                        });
                    }
                    _ => {
                        *acc += wrap_pi(a - *at);
                        *at = a;
                        if let Some(p) = &mut self.spin {
                            p.samples.push((self.now, *acc));
                            while p.samples.len() > 2 && p.samples[1].0 < self.now - 0.25 {
                                p.samples.remove(0);
                            }
                        }
                    }
                }
            }
            Gesture::Notes { id, cell0, paint, mode, sweep } => {
                let Some(ci) = w.cog_ix(*id) else { return };
                if *mode == 2 {
                    // the Euclid sweep: k pins spread evenly round the ring
                    let Some(c0) = *cell0 else { return };
                    let c = &w.cogs[ci];
                    let a = (q.1 - c.y).atan2(q.0 - c.x) - w.render_th(ci);
                    sweep.1 += wrap_pi(a - sweep.0);
                    sweep.0 = a;
                    let t = c.track.as_ref().unwrap();
                    let n = t.div;
                    let k = ((sweep.1.abs() / (TAU / n)).round()).clamp(0.0, n) as i64;
                    let dir = if sweep.1 < 0.0 { -1.0 } else { 1.0 };
                    if k == sweep.2 {
                        return;
                    }
                    sweep.2 = k;
                    let t = w.cogs[ci].track.as_mut().unwrap();
                    t.dots.retain(|d| d.ring != c0.0);
                    let nn = n as i64;
                    for j in 0..nn {
                        if k != 0 && (j * k) % nn < k {
                            t.dots.push(cogs_sim::Dot::new(c0.0, cogs_sim::wrap(c0.1 + dir * j as f64 * TAU / n)));
                        }
                    }
                    self.buzz(3);
                    return;
                }
                if *mode != 0 {
                    return;
                }
                if paint.is_none() {
                    self.put_pin(w, ci, *cell0, paint);
                }
                let cell = w.cell_at(ci, q.0, q.1);
                self.put_pin(w, ci, cell, paint);
            }
            Gesture::PitBox { id, at } => {
                self.scrap = self.off_stage(s);
                if let Some(p) = w.pits.iter_mut().find(|p| p.id == *id) {
                    p.shift(q.0 - at.0, q.1 - at.1);
                }
                *at = q;
            }
            Gesture::PitIn { id, w0, ball, held, last } => {
                if *held {
                    return;
                }
                *last = Some(s);
                let a = ball
                    .and_then(|b| w.pits.iter().find(|p| p.id == *id).and_then(|p| p.balls.iter().find(|x| x.id == b)))
                    .map(|b| cogs_sim::pit::ball_at(b, w.clock.view))
                    .unwrap_or(*w0);
                self.pit_band = Some((a, q));
            }
            Gesture::Bezel { i, v0, s0, .. } => {
                let (i, v0, s0) = (*i, v0.clone(), *s0);
                self.bezel_move(w, i, &v0, s0, s);
            }
            Gesture::Glyph | Gesture::Button(_) | Gesture::Nothing | Gesture::Hub => {}
        }
    }
    fn gesture_s0(&self) -> (f64, f64) {
        self.gesture.as_ref().map(|a| a.s0).unwrap_or((0.0, 0.0))
    }
    fn put_pin(&mut self, w: &mut World, ci: usize, cell: Option<(f64, f64)>, paint: &mut Option<bool>) {
        let Some((ring, a)) = cell else { return };
        let t = w.cogs[ci].track.as_ref().unwrap();
        let on = t.dots.iter().any(|o| o.ring == ring && wrap_pi(o.a - a).abs() < 1e-6);
        if paint.is_none() {
            *paint = Some(!on);
        }
        let p = paint.unwrap();
        if on != p {
            w.set_dot(ci, ring, a, p);
            if p {
                self.buzz(3);
                let note = w.cogs[ci].track.as_ref().unwrap().ring_note(ring as i64);
                let key = w.cogs[ci].id * 128;
                self.audition_disc(w, ci, note, key);
            }
        }
    }
    pub(super) fn audition_disc(&mut self, w: &World, ci: usize, midi: f64, key: u32) {
        let c = &w.cogs[ci];
        let r = w.readers.iter().find(|r| seg_dist((c.x, c.y), r.x1, r.y1, r.x2, r.y2) < c.radius());
        self.out.push(StageOut::Audition { part: r.map(|r| Part::Reader(r.id)), midi, key });
    }

    // ---------- lift, tap, double tap, cancel ----------
    pub(super) fn g_up(&mut self, w: &mut World, g: &mut Gesture, s: (f64, f64)) {
        match g {
            Gesture::Pan { boxing, .. } => {
                if *boxing {
                    self.marquee = None;
                }
            }
            Gesture::Marquee { a } => {
                let b = self.to_world(w, s);
                self.marquee = None;
                self.sel = None;
                self.group = box_select(w, *a, b);
            }
            Gesture::Group { .. } => {
                if self.scrap {
                    self.scrap = false;
                    self.remove_sel(w);
                } else {
                    self.changed(w, true);
                }
            }
            Gesture::Carry { kind, made, belt_a } => self.carry_up(w, kind, made, belt_a, s),
            Gesture::Shelf { mode, inner, last, v, .. } => {
                if *mode == 1 {
                    if let Some(i) = inner {
                        self.g_up(w, i, s);
                    }
                    return;
                }
                if *mode == 2 && self.now - last.1 < 0.08 && v.abs() > 150.0 {
                    self.shelf_fling = Some((*v, self.now));
                }
            }
            Gesture::Cam(id) => self.end_scrap(w, PartRef::Cam(*id)),
            Gesture::Probe(id) => self.end_scrap(w, PartRef::Probe(*id)),
            Gesture::Field { id, edge, .. } => {
                if *edge { self.changed(w, false) } else { self.end_scrap(w, PartRef::Field(*id)) }
            }
            Gesture::Arm { id, .. } | Gesture::ArmBody(id) => self.end_scrap(w, PartRef::Arm(*id)),
            Gesture::Pad { id, rim, .. } => {
                if *rim { self.changed(w, false) } else { self.end_scrap(w, PartRef::Pad(*id)) }
            }
            Gesture::Span { reader, id, end, .. } => {
                if *end != 0 {
                    self.changed(w, false);
                } else {
                    self.end_scrap(w, if *reader { PartRef::Reader(*id) } else { PartRef::String(*id) });
                }
            }
            Gesture::Belt(id) => self.end_scrap(w, PartRef::Belt(*id)),
            Gesture::Cog { start, .. } => {
                self.snapped = false;
                if self.scrap {
                    self.scrap = false;
                    for (id, _, _) in start.iter() {
                        w.remove(PartRef::Cog(*id));
                    }
                    self.buzz(18);
                    self.validate(w);
                    self.changed(w, true);
                } else {
                    self.changed(w, true);
                }
            }
            Gesture::Rim { id, grow } => {
                let Some(g) = grow else { return };
                if let Some(ci) = w.cog_ix(*id) {
                    if g.anchor.is_none() && w.stack_at(w.cogs[ci].x, w.cogs[ci].y).count() == 1 {
                        w.snap(ci);
                    }
                }
                self.changed(w, true);
            }
            Gesture::Knob { id, mode, .. } => {
                self.knob = None;
                match *mode {
                    0 => self.changed(w, false),
                    1 => {
                        self.crank = None;
                        if let Some(ci) = w.cog_ix(*id) {
                            let rpm = detent(w, w.rpm(ci));
                            w.retimed(|w| {
                                w.drive(ci, rpm);
                            });
                        }
                        self.buzz(12);
                        self.changed(w, true);
                    }
                    _ => {
                        if let Some(p) = &mut self.spin {
                            p.held = false;
                        }
                    }
                }
            }
            Gesture::Notes { .. } => self.changed(w, false),
            Gesture::PitBox { id, .. } => self.end_scrap(w, PartRef::Pit(*id)),
            Gesture::PitIn { id, w0, ball, held, last } => {
                self.pit_band = None;
                if *held {
                    return;
                }
                let (vx, vy) = self.fling(w, *w0, last.unwrap_or(s));
                self.throw(w, *id, *w0, *ball, vx, vy);
            }
            Gesture::Bezel { .. } => self.changed(w, true),
            Gesture::Glyph | Gesture::Button(_) | Gesture::Nothing | Gesture::Hub => {}
        }
    }
    pub(super) fn g_tap(&mut self, w: &mut World, g: &mut Gesture, s: (f64, f64)) {
        match g {
            // a tap on empty ground only lets go of the selection (and of the room ring)
            Gesture::Pan { .. } => {
                self.room = false;
                self.select(None);
            }
            Gesture::Hub => self.select(None),
            Gesture::Marquee { .. } => {
                self.marquee = None;
                let mut g2 = self.begin(w, s, false);
                let o = g2.owner();
                self.g_cancel(w, &mut g2);
                let Some(o) = o else { return };
                let mut cur: Vec<Sel> = if !self.group.is_empty() { self.group.clone() } else { self.sel.into_iter().collect() };
                if cur.contains(&o) {
                    cur.retain(|x| *x != o);
                } else {
                    cur.push(o);
                }
                self.sel = None;
                self.group = cur;
            }
            Gesture::Carry { .. } => {
                if self.armed.is_some() {
                    let mut g = std::mem::replace(g, Gesture::Glyph);
                    self.g_up(w, &mut g, s);
                } else {
                    self.carrying = false;
                    self.band = None;
                }
            }
            Gesture::Glyph => {
                let p = !w.clock.paused;
                let from_start = w.set_paused(p);
                self.out.push(StageOut::Paused { paused: p, from_start });
                self.say(format!("playback: {}", if p { "paused" } else { "playing" }), false);
            }
            Gesture::Button(b) => self.press(w, *b),
            Gesture::Bezel { i, .. } => {
                let i = *i;
                self.bezel_tap(w, i);
            }
            Gesture::Nothing => {}
            Gesture::Belt(id) => {
                if let Some(b) = w.belts.iter_mut().find(|b| b.id == *id) {
                    b.cross = !b.cross;
                    let on = b.cross;
                    self.say(format!("belt crossed: {}", if on { "on" } else { "off" }), false);
                }
                self.buzz(8);
                self.changed(w, true);
            }
            Gesture::Knob { mode, .. } => {
                self.knob = None;
                self.crank = None;
                if *mode == 2 {
                    if let Some(p) = &mut self.spin {
                        p.held = false;
                    }
                }
            }
            Gesture::Notes { id, cell0, paint, .. } => {
                let Some(ci) = w.cog_ix(*id) else { return };
                let n0 = w.cogs[ci].track.as_ref().map_or(0, |t| t.dots.len());
                self.put_pin(w, ci, *cell0, paint);
                if let (Some(t), Some((ring, _))) = (w.cogs[ci].track.as_ref(), *cell0) {
                    if t.dots.len() != n0 {
                        let what = if t.dots.len() > n0 { "pin added" } else { "pin removed" };
                        self.say(format!("{what}: {}", note_name(t.ring_note(ring as i64) as i64)), false);
                    }
                }
                if !self.is_sel(Sel::Cog(*id)) {
                    self.select(Some(Sel::Cog(*id)));
                }
                self.changed(w, false);
            }
            Gesture::PitIn { id, w0, ball, held, .. } => {
                if *held {
                    return;
                }
                let a = self.rand() * TAU;
                if let Some(b) = ball {
                    let p = w.pits.iter_mut().find(|p| p.id == *id).unwrap();
                    if let Some(bb) = p.balls.iter_mut().find(|x| x.id == *b) {
                        bb.vx += a.cos() * 220.0;
                        bb.vy += a.sin() * 220.0;
                        let (note, pid) = (bb.note, p.id);
                        self.out.push(StageOut::Audition { part: Some(Part::Pit(pid)), midi: note as f64, key: pid * 128 + note as u32 });
                    }
                } else {
                    self.throw(w, *id, *w0, None, a.cos() * 160.0, a.sin() * 160.0);
                }
                if !self.is_sel(Sel::Pit(*id)) {
                    self.select(Some(Sel::Pit(*id)));
                }
                self.changed(w, false);
                self.buzz(6);
            }
            Gesture::PitBox { id, .. } => self.choose(w, Sel::Pit(*id)),
            Gesture::Shelf { .. } => {}
            g => {
                if let Some(o) = g.owner() {
                    self.choose(w, o);
                }
            }
        }
    }
    /// A double tap; false when the gesture has none (it is then a tap).
    pub(super) fn g_dbl(&mut self, w: &mut World, g: &mut Gesture, s: (f64, f64)) -> bool {
        match g {
            Gesture::Pan { .. } => {
                self.readout = None;
                self.fly_fit(w);
                true
            }
            Gesture::Notes { id, cell0, .. } => {
                if let (Some(ci), Some((ring, a))) = (w.cog_ix(*id), *cell0) {
                    let on = w.cogs[ci].track.as_ref().unwrap().dots.iter().any(|o| o.ring == ring && wrap_pi(o.a - a).abs() < 1e-6);
                    w.set_dot(ci, ring, a, !on);
                }
                self.focus(w, Sel::Cog(*id), s);
                true
            }
            Gesture::Cam(id) => {
                if let Some(m) = w.cams.iter().find(|m| m.id == *id) {
                    self.focus(w, Sel::Cog(m.cog), s);
                }
                true
            }
            Gesture::Probe(id) => {
                if let Some(pr) = w.probes.iter().find(|p| p.id == *id) {
                    if w.probe_alive(pr) {
                        let t = if pr.host == Some(Host::Ball) { Sel::Pit(pr.on) } else { Sel::Cog(w.cogs[w.probe_cog(pr).unwrap()].id) };
                        self.focus(w, t, s);
                    }
                }
                true
            }
            Gesture::Arm { id, .. } => {
                self.focus(w, Sel::Arm(*id), s);
                true
            }
            Gesture::Field { id, .. } => {
                self.focus(w, Sel::Field(*id), s);
                true
            }
            Gesture::Pad { id, .. } => {
                self.focus(w, Sel::Pad(*id), s);
                true
            }
            Gesture::Span { reader, id, .. } => {
                self.focus(w, if *reader { Sel::Reader(*id) } else { Sel::String(*id) }, s);
                true
            }
            Gesture::Cog { id, .. } | Gesture::Rim { id, .. } => {
                self.focus(w, Sel::Cog(*id), s);
                true
            }
            Gesture::PitBox { id, .. } => {
                self.focus(w, Sel::Pit(*id), s);
                true
            }
            _ => false,
        }
    }
    /// Cancel; false when the gesture has no cancel of its own (it then lifts as usual).
    pub(super) fn g_cancel(&mut self, w: &mut World, g: &mut Gesture) -> bool {
        match g {
            Gesture::Pan { .. } | Gesture::Marquee { .. } => {
                self.marquee = None;
                true
            }
            Gesture::Carry { made, .. } => {
                self.carrying = false;
                self.carry_ghost = None;
                self.band = None;
                if let Some(m) = made.take() {
                    w.remove(m);
                }
                true
            }
            Gesture::Shelf { inner, .. } => {
                if let Some(i) = inner {
                    self.g_cancel(w, i);
                }
                true
            }
            Gesture::Knob { mode, .. } => {
                self.knob = None;
                self.crank = None;
                if *mode == 2 {
                    if let Some(p) = &mut self.spin {
                        p.held = false;
                    }
                }
                true
            }
            Gesture::Notes { .. } | Gesture::Glyph | Gesture::Button(_) | Gesture::Nothing | Gesture::Hub => true,
            Gesture::Bezel { .. } => {
                self.changed(w, true);
                true
            }
            Gesture::PitIn { .. } => {
                self.pit_band = None;
                true
            }
            _ => false,
        }
    }

    /// Holds: a press held still turns into something else.
    pub(super) fn step_holds(&mut self, w: &mut World) {
        let Some(a) = &self.gesture else { return };
        if a.moved {
            return;
        }
        let age = self.now - a.t0;
        let s0 = a.s0;
        let mut g = match &self.gesture {
            Some(a) => match &a.g {
                Gesture::Pan { boxing: false, .. } if age >= 0.38 => 1,
                Gesture::Notes { paint: None, mode: 0, .. } if age >= 0.45 => 2,
                Gesture::Glyph if age >= 0.6 => 3,
                Gesture::PitIn { ball: Some(_), held: false, .. } if age >= 0.45 => 4,
                _ => 0,
            },
            None => 0,
        };
        match g {
            1 => {
                if let Some(Active { g: Gesture::Pan { boxing, a, .. }, .. }) = &mut self.gesture {
                    *boxing = true;
                    let a = *a;
                    self.marquee = Some((a, a));
                    self.sel = None;
                    self.group.clear();
                    self.buzz(10);
                }
            }
            2 => {
                let q = self.to_world(w, s0);
                let Some(Active { g: Gesture::Notes { id, cell0, .. }, .. }) = &self.gesture else { return };
                let (id, cell0) = (*id, *cell0);
                let Some((ring, a)) = cell0 else { return };
                let Some(ci) = w.cog_ix(id) else { return };
                let has = w.cogs[ci].track.as_ref().unwrap().dots.iter().any(|o| o.ring == ring && wrap_pi(o.a - a).abs() < 1e-6);
                let c = &w.cogs[ci];
                let start = ((q.1 - c.y).atan2(q.0 - c.x) - w.render_th(ci), 0.0, -1);
                if let Some(Active { g: Gesture::Notes { mode, sweep, .. }, .. }) = &mut self.gesture {
                    if has {
                        *mode = 1;
                    } else {
                        *mode = 2;
                        *sweep = start;
                    }
                }
                if has {
                    self.dot_at = q;
                    self.sel = Some(Sel::Dot { cog: id, ring, a });
                    self.group.clear();
                }
                self.buzz(12);
            }
            3 => {
                self.gesture = None;
                self.toggle_panel();
                self.buzz(15);
            }
            4 => {
                if let Some(Active { g: Gesture::PitIn { id, ball, held, .. }, .. }) = &mut self.gesture {
                    *held = true;
                    let (id, b) = (*id, ball.unwrap());
                    if let Some(p) = w.pits.iter_mut().find(|p| p.id == id) {
                        p.balls.retain(|x| x.id != b);
                    }
                    self.pit_band = None;
                    self.buzz(18);
                    self.changed(w, false);
                }
            }
            _ => {}
        }
        g = 0;
        let _ = g;
    }

    // ---------- the pit ----------
    fn fling(&self, w: &World, w0: (f64, f64), s: (f64, f64)) -> (f64, f64) {
        let q = self.to_world(w, s);
        let dt = (self.now - self.gesture_t0()).max(0.06);
        let k = (1.0 / dt).clamp(2.0, 8.0);
        let (mut vx, mut vy) = ((q.0 - w0.0) * k, (q.1 - w0.1) * k);
        let sp = vx.hypot(vy);
        if sp > 900.0 {
            vx *= 900.0 / sp;
            vy *= 900.0 / sp;
        }
        (vx, vy)
    }
    fn gesture_t0(&self) -> f64 {
        self.down_t
    }
    fn throw(&mut self, w: &mut World, id: u32, w0: (f64, f64), ball: Option<u32>, vx: f64, vy: f64) {
        let Some(pi) = idx(&w.pits, |p| p.id == id) else { return };
        if let Some(b) = ball {
            if let Some(bb) = w.pits[pi].balls.iter_mut().find(|x| x.id == b) {
                bb.vx = vx;
                bb.vy = vy;
            }
        } else {
            let bid = w.drop_ball(pi, w0.0, w0.1, vx, vy, None);
            let note = w.pits[pi].balls.iter().find(|b| b.id == bid).map(|b| b.note).unwrap_or(60);
            self.out.push(StageOut::Audition { part: Some(Part::Pit(id)), midi: note as f64, key: id * 128 + note as u32 });
        }
        if !self.is_sel(Sel::Pit(id)) {
            self.select(Some(Sel::Pit(id)));
        }
        self.changed(w, false);
        self.buzz(6);
    }
    fn rand(&mut self) -> f64 {
        let x = (self.now * 12.9898 + 78.233).sin() * 43758.5453;
        x - x.floor()
    }

    // ---------- carrying a part in ----------
    fn carry_move(&mut self, w: &mut World, kind: &mut &'static str, made: &mut Option<PartRef>, belt_a: &mut Option<u32>, s: (f64, f64)) {
        let p = self.to_world(w, s);
        if *kind == "belt" {
            let c = if self.off_stage(s) { None } else { w.pick_cog(p.0, p.1, false, 0.0) };
            if belt_a.is_none() {
                if let Some(c) = c {
                    *belt_a = Some(w.cogs[c].id);
                    self.buzz(6);
                }
            }
            self.band = belt_a.and_then(|a| w.cog(a)).map(|c| ((c.x, c.y), p));
            return;
        }
        if self.off_stage(s) {
            if let Some(m) = made.take() {
                w.remove(m);
            }
            self.carry_ghost = None;
            return;
        }
        let present = made.map_or(false, |m| exists(w, sel_of(m)));
        if !present {
            *made = None;
        }
        let z = w.s.zoom;
        match (*kind, *made) {
            ("motor" | "cog" | "disc", None) => {
                let k = match *kind {
                    "motor" => CogKind::Motor,
                    "disc" => CogKind::Disc,
                    _ => CogKind::Cog,
                };
                *made = Some(PartRef::Cog(w.add_cog(k, p.0, p.1, z)));
                self.buzz(6);
            }
            ("motor" | "cog" | "disc", Some(PartRef::Cog(id))) => {
                if let Some(ci) = w.cog_ix(id) {
                    w.place_cog(ci, p.0, p.1, z);
                }
            }
            ("pad", None) => {
                *made = Some(PartRef::Pad(w.add_pad(p.0, p.1)));
                self.buzz(6);
            }
            ("pad", Some(PartRef::Pad(id))) => {
                if let Some(o) = w.pads.iter_mut().find(|o| o.id == id) {
                    o.x = p.0;
                    o.y = p.1;
                }
            }
            ("field", None) => {
                *made = Some(PartRef::Field(w.add_field(p.0, p.1)));
                self.buzz(6);
            }
            ("field", Some(PartRef::Field(id))) => {
                if let Some(o) = w.fields.iter_mut().find(|o| o.id == id) {
                    o.x = p.0;
                    o.y = p.1;
                }
            }
            ("sphere", None) => {
                *made = Some(PartRef::Pit(w.add_pit(p.0, p.1)));
                self.buzz(6);
            }
            ("sphere", Some(PartRef::Pit(id))) => {
                if let Some(o) = w.pits.iter_mut().find(|o| o.id == id) {
                    let (dx, dy) = (p.0 - o.x, p.1 - o.y);
                    o.shift(dx, dy);
                }
            }
            ("string", None) => {
                *made = Some(PartRef::String(w.add_string(p.0, p.1)));
                self.buzz(6);
            }
            ("string", Some(PartRef::String(id))) => {
                if let Some(o) = w.strings.iter_mut().find(|o| o.id == id) {
                    o.x1 = p.0 - 60.0;
                    o.y1 = p.1;
                    o.x2 = p.0 + 60.0;
                    o.y2 = p.1;
                }
            }
            ("reader" | "comb", None) => {
                *made = Some(PartRef::Reader(w.add_reader(p.0, p.1, *kind == "comb")));
                self.buzz(6);
            }
            ("reader" | "comb", Some(PartRef::Reader(id))) => {
                if let Some(ri) = w.reader_ix(id) {
                    place_reader(w, ri, p);
                }
            }
            ("arm", Some(PartRef::Arm(id))) => match w.pick_cog(p.0, p.1, false, 0.0) {
                Some(ci) => {
                    let (cx, cy, cid) = (w.cogs[ci].x, w.cogs[ci].y, w.cogs[ci].id);
                    let rth = w.render_th(ci);
                    if let Some(a) = w.arms.iter_mut().find(|a| a.id == id) {
                        a.cog = cid;
                        a.cog_ix = ci;
                        a.a = cogs_sim::wrap((p.1 - cy).atan2(p.0 - cx) - rth);
                        a.len = (p.0 - cx).hypot(p.1 - cy).max(4.0);
                    }
                    w.solve();
                }
                None => {
                    w.remove(PartRef::Arm(id));
                    *made = None;
                }
            },
            ("cam", Some(PartRef::Cam(id))) => match w.pick_cog(p.0, p.1, false, 0.0) {
                Some(ci) => {
                    let (cx, cy, cid) = (w.cogs[ci].x, w.cogs[ci].y, w.cogs[ci].id);
                    if let Some(m) = w.cams.iter_mut().find(|m| m.id == id) {
                        m.cog = cid;
                        m.ang = (p.1 - cy).atan2(p.0 - cx);
                    }
                    w.solve();
                }
                None => {
                    w.remove(PartRef::Cam(id));
                    *made = None;
                }
            },
            ("probe", Some(PartRef::Probe(id))) => {
                if let Some(i) = idx(&w.probes, |q| q.id == id) {
                    if !w.probe_attach(i, p.0, p.1, z, false) {
                        w.remove(PartRef::Probe(id));
                        *made = None;
                    }
                }
            }
            ("arm" | "cam" | "probe", None) => {
                let id = match *kind {
                    "arm" => w.add_arm(p.0, p.1).map(PartRef::Arm),
                    "cam" => w.add_cam(p.0, p.1).map(PartRef::Cam),
                    _ => w.add_probe(p.0, p.1, z).map(PartRef::Probe),
                };
                if id.is_some() {
                    self.buzz(6);
                }
                *made = id;
            }
            _ => {}
        }
        self.carry_ghost = if made.is_none() { Some(s) } else { None };
    }
    fn carry_up(&mut self, w: &mut World, kind: &mut &'static str, made: &mut Option<PartRef>, belt_a: &mut Option<u32>, s: (f64, f64)) {
        self.carrying = false;
        self.carry_ghost = None;
        self.band = None;
        self.armed = None;
        if *kind == "belt" {
            let p = self.to_world(w, s);
            let b = if self.off_stage(s) { None } else { w.pick_cog(p.0, p.1, false, 0.0).map(|c| w.cogs[c].id) };
            if let (Some(a), Some(b)) = (*belt_a, b) {
                if w.add_belt(a, b, false).is_some() {
                    self.buzz(10);
                    self.changed(w, true);
                }
            }
            return;
        }
        let Some(m) = *made else { return };
        if !exists(w, sel_of(m)) {
            return;
        }
        if self.off_stage(s) {
            w.remove(m);
            return;
        }
        self.select(Some(sel_of(m)));
        self.changed(w, true);
        self.buzz(10);
    }

    // ---------- removing ----------
    fn end_scrap(&mut self, w: &mut World, p: PartRef) {
        let gone = self.scrap;
        self.scrap = false;
        if gone {
            self.remove_obj(w, p);
        } else {
            self.changed(w, true);
        }
    }
    pub(super) fn remove_obj(&mut self, w: &mut World, p: PartRef) {
        w.remove(p);
        self.validate(w);
        self.buzz(18);
        self.changed(w, true);
    }
    pub(super) fn remove_sel(&mut self, w: &mut World) {
        let items: Vec<Sel> = if !self.group.is_empty() { std::mem::take(&mut self.group) } else { self.sel.into_iter().collect() };
        for s in items {
            match s {
                Sel::Dot { cog, ring, a } => {
                    if let Some(ci) = w.cog_ix(cog) {
                        w.set_dot(ci, ring, a, false);
                        self.sel = None;
                        self.changed(w, false);
                    }
                }
                s => {
                    if let Some(p) = s.part() {
                        if exists(w, s) {
                            self.remove_obj(w, p);
                        }
                    }
                }
            }
        }
    }
    pub(super) fn choose(&mut self, w: &World, o: Sel) {
        self.select(Some(o));
        self.reveal(w);
    }
    /// The tap after a cam's or probe's link sector: a reader (a cam), or a reader, pad or sphere (a probe), takes the
    /// link; anywhere else cancels it.
    fn link_gesture(&mut self, w: &mut World, m: Sel, s: (f64, f64)) {
        let p = self.to_world(w, s);
        let z = w.s.zoom;
        let mut best: Option<(f64, Part)> = None;
        let mut take = |d: f64, reach: f64, o: Part| {
            if d <= reach && best.map_or(true, |b| d < b.0) {
                best = Some((d, o));
            }
        };
        for r in &w.readers {
            take(seg_dist(p, r.x1, r.y1, r.x2, r.y2) * z, 24.0, Part::Reader(r.id));
        }
        if let Sel::Probe(_) = m {
            for o in &w.pads {
                take((dist(p, (o.x, o.y)) - o.r).max(0.0) * z, 16.0, Part::Pad(o.id));
            }
            for o in &w.pits {
                if dist(p, (o.x, o.y)) <= o.w / 2.0 {
                    take(20.0, 24.0, Part::Pit(o.id));
                }
            }
        }
        // a synth takes one more macro link (a cam or probe turns up to 64); a probe on any other voice drives a knob
        let mut add = |w: &mut World, from: cogs_sim::MotionFrom, o: Part| match w.add_macro_link(from, o) {
            Ok(slot) => {
                let n = match from {
                    cogs_sim::MotionFrom::Cam(id) => w.cams.iter().find(|m| m.id == id).map_or(0, |m| m.to.is_some() as usize + m.more.len()),
                    cogs_sim::MotionFrom::Probe(id) => w.probes.iter().find(|p| p.id == id).map_or(0, |p| p.to.is_some() as usize + p.more.len()),
                };
                self.say(format!("link: macro {slot}{}", if n > 1 { format!(" · {n} macros") } else { String::new() }), false);
                true
            }
            Err(why) => { self.say(format!("link: {why}"), false); false }
        };
        let ok = match (m, best) {
            (Sel::Cam(c), Some((_, o @ Part::Reader(_)))) => add(w, cogs_sim::MotionFrom::Cam(c), o),
            (Sel::Probe(pr), Some((_, o))) if w.voice(o).map(|v| v.kind) == Some(cogs_sim::VoiceKind::Wave) => add(w, cogs_sim::MotionFrom::Probe(pr), o),
            (Sel::Probe(pr), Some((_, o))) => w.link_probe(pr, o),
            _ => false,
        };
        if ok {
            self.buzz(12);
            self.changed(w, true);
            if let (Sel::Probe(_), Some((_, o))) = (m, best) {
                if let Some(v) = w.voice(o) {
                    if v.kind == cogs_sim::VoiceKind::Wave {
                        self.select(Some(super::gesture::sel_of_part(o)));
                        self.out.push(StageOut::OpenEditor("patch"));
                    }
                }
            } else if let Sel::Cam(_) = m {
                if let Some((_, o)) = best {
                    self.select(Some(super::gesture::sel_of_part(o)));
                    self.out.push(StageOut::OpenEditor("patch"));
                }
            }
        } else if best.is_none() {
            self.say("link: cancelled".into(), false);
        }
    }

    // ---------- grips ----------
    fn grip_of(&self, w: &World, g: &Gesture) -> Option<Grip> {
        let fills = |r: f64| 2.0 * r * w.s.zoom > 0.9 * self.cw.min(self.ch);
        match g {
            Gesture::Cog { id, .. } => {
                let c = w.cog(*id)?;
                if fills(c.radius()) {
                    return None;
                }
                let stack: Vec<usize> = w.stack_at(c.x, c.y).collect();
                let stuck = stack.iter().any(|&o| w.clashes(o, &stack));
                let ci = w.cog_ix(*id)?;
                Some(Grip::Cog {
                    anchor: w.resizer(ci).anchor.map(|a| w.cogs[a].id),
                    id: *id, start: stack.iter().map(|&o| (w.cogs[o].id, w.cogs[o].x, w.cogs[o].y)).collect(),
                    last: if stuck { None } else { Some((0.0, 0.0)) }, b: (0.0, 0.0), n0: c.n, raw: c.n,
                })
            }
            Gesture::Field { id, .. } => {
                let f = w.fields.iter().find(|f| f.id == *id)?;
                if fills(f.r) { None } else { Some(Grip::Field { id: *id, x: f.x, y: f.y, r: f.r }) }
            }
            Gesture::Pad { id, .. } => {
                let f = w.pads.iter().find(|f| f.id == *id)?;
                if fills(f.r) { None } else { Some(Grip::Pad { id: *id, x: f.x, y: f.y, r: f.r }) }
            }
            Gesture::PitBox { id, .. } | Gesture::PitIn { id, held: false, .. } => {
                let p = w.pits.iter().find(|p| p.id == *id)?;
                if fills(p.w / 2.0) { None } else { Some(Grip::Pit { id: *id, x: p.x, y: p.y, d: p.w }) }
            }
            Gesture::Arm { id, .. } => {
                let a = w.arms.iter().find(|f| f.id == *id)?;
                Some(Grip::Arm { id: *id, l: a.r0 + a.len })
            }
            Gesture::Span { reader, id, .. } => {
                if *reader {
                    let r = w.readers.iter().find(|f| f.id == *id)?;
                    if let (Some(_), Some(q)) = (r.on_ix, r.seat) {
                        return Some(Grip::SpanSeated { id: *id, mid: (q.f0 + q.f1) / 2.0, half: (q.f1 - q.f0) / 2.0 });
                    }
                    let (hx, hy) = ((r.x2 - r.x1) / 2.0, (r.y2 - r.y1) / 2.0);
                    Some(Grip::SpanFree { reader: true, id: *id, mx: (r.x1 + r.x2) / 2.0, my: (r.y1 + r.y2) / 2.0, hx, hy, l: hx.hypot(hy).max(1e-9), note: 0 })
                } else {
                    let r = w.strings.iter().find(|f| f.id == *id)?;
                    let (hx, hy) = ((r.x2 - r.x1) / 2.0, (r.y2 - r.y1) / 2.0);
                    Some(Grip::SpanFree { reader: false, id: *id, mx: (r.x1 + r.x2) / 2.0, my: (r.y1 + r.y2) / 2.0, hx, hy, l: hx.hypot(hy).max(1e-9), note: r.midi().round() as i64 })
                }
            }
            _ => None,
        }
    }
    fn grip_hold(&mut self, w: &mut World, g: &mut Grip, m: (f64, f64), k: f64) {
        match g {
            Grip::Cog { id, start, last, b, n0, raw, anchor } => {
                let (dx, dy) = (m.0 + b.0, m.1 + b.1);
                let ms: Vec<(usize, f64, f64)> = start.iter().filter_map(|c| w.cog_ix(c.0).map(|i| (i, c.1, c.2))).collect();
                if self.scrap {
                    for &(o, x, y) in &ms {
                        w.cogs[o].x = x + dx;
                        w.cogs[o].y = y + dy;
                    }
                    w.solve();
                    return;
                }
                let was = *last;
                let r = w.clear_shift(&ms, dx, dy, *last);
                *last = Some(r);
                let blocked = was.is_some() && (r.0 != dx || r.1 != dy);
                let stack: Vec<usize> = ms.iter().map(|m| m.0).collect();
                let Some(ci) = w.cog_ix(*id) else { return };
                let snapped = w.snap_clear(ci, &stack) || blocked;
                if snapped && !self.snapped {
                    self.buzz(8);
                }
                self.snapped = snapped;
                w.solve();
                // a resize keeps the meshes from wherever the cog stands now, and may shift it: the fingers keep that shift
                let rn = (*n0 * k).round();
                if rn != *raw {
                    *raw = rn;
                    let (px, py) = (w.cogs[ci].x, w.cogs[ci].y);
                    let grow = w.resizer(ci);
                    *anchor = grow.anchor.map(|a| w.cogs[a].id);
                    w.set_teeth(&grow, rn);
                    let (ex, ey) = (w.cogs[ci].x - px, w.cogs[ci].y - py);
                    b.0 += ex;
                    b.1 += ey;
                    if let Some(l) = last {
                        l.0 += ex;
                        l.1 += ey;
                    }
                }
            }
            Grip::Field { id, x, y, r } => {
                if let Some(f) = w.fields.iter_mut().find(|f| f.id == *id) {
                    f.x = *x + m.0;
                    f.y = *y + m.1;
                    f.r = (*r * k).clamp(30.0, 800.0);
                }
            }
            Grip::Pad { id, x, y, r } => {
                if let Some(f) = w.pads.iter_mut().find(|f| f.id == *id) {
                    f.x = *x + m.0;
                    f.y = *y + m.1;
                    f.r = (*r * k).clamp(3.0, 200.0);
                }
            }
            Grip::Pit { id, x, y, d } => {
                if let Some(p) = w.pits.iter_mut().find(|p| p.id == *id) {
                    p.reframe(*x + m.0, *y + m.1, *d * k);
                }
            }
            Grip::Arm { id, l } => {
                if let Some(a) = w.arms.iter_mut().find(|f| f.id == *id) {
                    a.len = (*l * k - a.r0).max(4.0);
                }
            }
            Grip::SpanSeated { id, mid, half } => {
                let Some(ri) = idx(&w.readers, |r| r.id == *id) else { return };
                let Some(ci) = w.readers[ri].on_ix else { return };
                let c = w.cogs[ci].clone();
                let r = &mut w.readers[ri];
                if let Some(s) = &mut r.seat {
                    s.f0 = (*mid - *half * k).clamp(0.0, 1.0);
                    s.f1 = (*mid + *half * k).clamp(0.0, 1.0);
                }
                place_seated(r, &c);
            }
            Grip::SpanFree { reader, id, mx, my, hx, hy, l, note } => {
                let f = k.max(2.0 / *l);
                let (ex, ey) = (*hx * f, *hy * f);
                let seg = [*mx + m.0 - ex, *my + m.1 - ey, *mx + m.0 + ex, *my + m.1 + ey];
                if *reader {
                    if let Some(r) = w.readers.iter_mut().find(|r| r.id == *id) {
                        (r.x1, r.y1, r.x2, r.y2) = (seg[0], seg[1], seg[2], seg[3]);
                    }
                } else if let Some(r) = w.strings.iter_mut().find(|r| r.id == *id) {
                    (r.x1, r.y1, r.x2, r.y2) = (seg[0], seg[1], seg[2], seg[3]);
                    let n = r.midi().round() as i64;
                    if n != *note {
                        *note = n;
                        self.buzz(3);
                    }
                }
            }
        }
    }
    /// What the finger left behind carries on with when the other lifts.
    fn grip_body(&mut self, w: &World, g: &Grip, s: (f64, f64)) -> Gesture {
        let p = self.to_world(w, s);
        match *g {
            Grip::Cog { id, .. } => self.cog_gesture(w, id, p),
            Grip::Field { id, .. } => {
                let f = w.fields.iter().find(|f| f.id == id).unwrap();
                Gesture::Field { id, off: (f.x - p.0, f.y - p.1), edge: false }
            }
            Grip::Pad { id, .. } => {
                let f = w.pads.iter().find(|f| f.id == id).unwrap();
                Gesture::Pad { id, off: (f.x - p.0, f.y - p.1), rim: false }
            }
            Grip::Pit { id, .. } => Gesture::PitBox { id, at: p },
            Grip::Arm { id, .. } => Gesture::ArmBody(id),
            Grip::SpanSeated { id, .. } | Grip::SpanFree { reader: true, id, .. } => {
                let r = w.readers.iter().find(|f| f.id == id).unwrap();
                let turn = match (r.on_ix, r.seat) {
                    (Some(ci), Some(seat)) => seat.ang - (p.1 - w.cogs[ci].y).atan2(p.0 - w.cogs[ci].x),
                    _ => 0.0,
                };
                Gesture::Span { reader: true, id, p0: p, k: [r.x1, r.y1, r.x2, r.y2], end: 0, note: 0, turn }
            }
            Grip::SpanFree { id, .. } => {
                let r = w.strings.iter().find(|f| f.id == id).unwrap();
                Gesture::Span { reader: false, id, p0: p, k: [r.x1, r.y1, r.x2, r.y2], end: 0, note: r.midi().round() as i64, turn: 0.0 }
            }
        }
    }

    // ---------- the wheel and the keyboard ----------
    fn wheel(&mut self, w: &mut World, s: (f64, f64), dx: f64, dy: f64, ctrl: bool) {
        if s.1 >= self.shelf_top() {
            self.set_shelf_x(self.shelf_x + if dx.abs() > dy.abs() { dx } else { dy });
            return;
        }
        self.fly = None;
        if ctrl {
            // a trackpad pinch: it takes the cog under the cursor where it begins, until it rests
            if self.trackpad.is_none() {
                let p = self.to_world(w, s);
                let c = w.pick_cog(p.0, p.1, false, 0.0).filter(|&c| 2.0 * w.cogs[c].radius() * w.s.zoom <= 0.9 * self.cw.min(self.ch));
                let n0 = c.map(|c| w.cogs[c].n).unwrap_or(0.0);
                self.trackpad = Some((c.map(|c| w.cogs[c].id), 1.0, n0, n0, self.now));
            }
            let t = self.trackpad.as_mut().unwrap();
            t.4 = self.now;
            if let Some(id) = t.0 {
                t.1 *= (-dy * 0.01).exp();
                let r = (t.2 * t.1).round();
                if r != t.3 {
                    t.3 = r;
                    if let Some(ci) = w.cog_ix(id) {
                        let g = w.resizer(ci);
                        let anchor = g.anchor.map(|a| w.cogs[a].id);
                        if w.set_teeth(&g, r) {
                            let text = self.resize_text(w, id, anchor);
                            self.say(text, false);
                        }
                    }
                }
                return;
            }
        }
        if ctrl {
            // no plate under the cursor: a sphere there takes the pinch
            let (p, most) = (self.to_world(w, s), 0.9 * self.cw.min(self.ch) / w.s.zoom);
            if let Some(o) = w.pits.iter_mut().find(|o| dist(p, (o.x, o.y)) <= o.w / 2.0 && o.w <= most) {
                let (x, y, d) = (o.x, o.y, o.w * (-dy * 0.01).exp());
                o.reframe(x, y, d);
                let across = o.w.round();
                self.say(format!("sphere: {across} across"), false);
                self.changed(w, false);
                return;
            }
        }
        let anchor = self.to_world(w, s);
        let z = w.s.zoom * (-dy * if ctrl { 0.01 } else { 0.002 }).exp();
        self.zoom_at(w, s, z, anchor);
    }
    fn end_trackpad(&mut self, w: &mut World) {
        let Some((c, ..)) = self.trackpad.take() else { return };
        if let Some(ci) = c.and_then(|c| w.cog_ix(c)) {
            let lone = w.stack_at(w.cogs[ci].x, w.cogs[ci].y).count() == 1;
            let meshed = (0..w.cogs.len()).any(|o| o != ci && w.contact(ci, o) == Some(cogs_sim::Contact::Mesh));
            if lone && !meshed {
                w.snap_clear(ci, &[ci]);
            }
            self.changed(w, true);
        }
    }
    fn key(&mut self, w: &mut World, key: Key, m: egui::Modifiers) {
        let cmd = m.ctrl || m.command;
        if cmd && !m.alt {
            match key {
                Key::Z if !m.shift => self.undo(w),
                Key::Z | Key::Y => self.redo(w),
                Key::C => {
                    self.copy_parts(w);
                }
                Key::X => {
                    if self.copy_parts(w) {
                        self.remove_sel(w);
                    }
                }
                Key::V => {
                    self.paste_parts(w);
                }
                _ => {}
            }
            return;
        }
        if m.ctrl || m.command || m.alt {
            return;
        }
        let part = match key {
            Key::M => Some("motor"),
            Key::C => Some("cog"),
            Key::D => Some("disc"),
            Key::A => Some("arm"),
            Key::B => Some("belt"),
            Key::P => Some("pad"),
            Key::T => Some("string"),
            Key::R => Some("reader"),
            Key::K => Some("comb"),
            Key::E => Some("cam"),
            Key::O => Some("sphere"),
            Key::Y => Some("probe"),
            Key::F => Some("field"),
            _ => None,
        };
        if let Some(p) = part {
            self.armed = Some(p);
            return;
        }
        match key {
            Key::S | Key::Escape => self.armed = None,
            Key::H => self.to_start(w),
            Key::Z => self.zero_sel(w),
            Key::Num0 => self.fly_fit(w),
            Key::Delete | Key::Backspace => self.remove_sel(w),
            Key::Space => {
                let p = !w.clock.paused;
                let from_start = w.set_paused(p);
                self.out.push(StageOut::Paused { paused: p, from_start });
            }
            _ => {}
        }
    }
    pub(super) fn to_start(&mut self, w: &mut World) {
        self.spin = None;
        w.to_start();
        self.out.push(StageOut::Paused { paused: true, from_start: false });
        self.changed(w, false);
        self.buzz(10);
    }
    /// What zero acts on: the selected plates, and seated readers.
    pub(super) fn zeroable(&self, w: &World) -> Vec<Sel> {
        let items: Vec<Sel> = if !self.group.is_empty() { self.group.clone() } else { self.sel.into_iter().collect() };
        items.into_iter().filter(|s| match s {
            Sel::Cog(_) => true,
            Sel::Reader(i) => w.readers.iter().any(|r| r.id == *i && r.on.is_some()),
            _ => false,
        }).collect()
    }
    pub(super) fn zero_sel(&mut self, w: &mut World) {
        let parts = self.zeroable(w);
        if parts.is_empty() {
            return;
        }
        for s in parts {
            match s {
                Sel::Cog(i) => {
                    if let Some(ci) = w.cog_ix(i) {
                        w.zero_plate(ci);
                    }
                }
                Sel::Reader(i) => {
                    if let Some(ri) = w.reader_ix(i) {
                        w.zero_reader(ri);
                    }
                }
                _ => {}
            }
        }
        self.buzz(10);
        self.changed(w, false);
    }
}


/// stage.js placePart for a reader: on a disc it lies along the radius there, seated, spanning the rings; elsewhere
/// it lies level, free.
fn place_reader(w: &mut World, ri: usize, p: (f64, f64)) {
    match w.pick_cog(p.0, p.1, true, 0.0) {
        Some(di) => {
            let d = w.cogs[di].clone();
            let a = (p.1 - d.y).atan2(p.0 - d.x);
            let rings = d.track.as_ref().unwrap().rings();
            let (r0, r1) = (d.ring_r(0.0) - 6.0, d.ring_r((rings - 1) as f64) + 6.0);
            let r = &mut w.readers[ri];
            (r.x1, r.y1, r.x2, r.y2) = (d.x + a.cos() * r0, d.y + a.sin() * r0, d.x + a.cos() * r1, d.y + a.sin() * r1);
            cogs_sim::solve::seat_reader(r, &d);
        }
        None => {
            let r = &mut w.readers[ri];
            (r.x1, r.y1, r.x2, r.y2) = (p.0 - 30.0, p.1, p.0 + 30.0, p.1);
            r.on = None;
            r.seat = None;
        }
    }
    w.solve();
}

pub(super) fn sel_of_part(p: Part) -> Sel {
    match p {
        Part::Pad(i) => Sel::Pad(i),
        Part::Reader(i) => Sel::Reader(i),
        Part::Pit(i) => Sel::Pit(i),
        Part::String(i) => Sel::String(i),
    }
}

pub(super) fn sel_of(p: PartRef) -> Sel {
    match p {
        PartRef::Cog(i) => Sel::Cog(i),
        PartRef::Arm(i) => Sel::Arm(i),
        PartRef::Pad(i) => Sel::Pad(i),
        PartRef::Reader(i) => Sel::Reader(i),
        PartRef::Belt(i) => Sel::Belt(i),
        PartRef::String(i) => Sel::String(i),
        PartRef::Cam(i) => Sel::Cam(i),
        PartRef::Pit(i) => Sel::Pit(i),
        PartRef::Probe(i) => Sel::Probe(i),
        PartRef::Field(i) => Sel::Field(i),
    }
}

/// boxSelect: the parts whose centres (or middles, tips, points) lie in the box.
pub(super) fn box_select(w: &World, a: (f64, f64), b: (f64, f64)) -> Vec<Sel> {
    let (x0, x1, y0, y1) = (a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1));
    let inside = |q: (f64, f64)| q.0 >= x0 && q.0 <= x1 && q.1 >= y0 && q.1 <= y1;
    let mut g = Vec::new();
    for o in &w.cogs {
        if inside((o.x, o.y)) {
            g.push(Sel::Cog(o.id));
        }
    }
    for o in &w.pads {
        if inside((o.x, o.y)) {
            g.push(Sel::Pad(o.id));
        }
    }
    for o in &w.pits {
        if inside((o.x, o.y)) {
            g.push(Sel::Pit(o.id));
        }
    }
    for o in &w.fields {
        if inside((o.x, o.y)) {
            g.push(Sel::Field(o.id));
        }
    }
    for o in &w.readers {
        if inside(((o.x1 + o.x2) / 2.0, (o.y1 + o.y2) / 2.0)) {
            g.push(Sel::Reader(o.id));
        }
    }
    for o in &w.strings {
        if inside(((o.x1 + o.x2) / 2.0, (o.y1 + o.y2) / 2.0)) {
            g.push(Sel::String(o.id));
        }
    }
    for (i, o) in w.arms.iter().enumerate() {
        if inside(w.arm_tip(i)) {
            g.push(Sel::Arm(o.id));
        }
    }
    for m in &w.cams {
        if inside(cam_tip(m, &w.cogs[m.cog_ix], w.render_th(m.cog_ix))) {
            g.push(Sel::Cam(m.id));
        }
    }
    for o in &w.probes {
        if w.probe_alive(o) && inside(w.probe_point(o)) {
            g.push(Sel::Probe(o.id));
        }
    }
    for o in &w.belts {
        if g.contains(&Sel::Cog(o.a)) && g.contains(&Sel::Cog(o.b)) {
            g.push(Sel::Belt(o.id));
        }
    }
    g
}
