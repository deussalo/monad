//! phone.js: the shelf of parts (a tray wider than the phone: a sideways drag scrolls it, a drag up carries a part in),
//! the glyph (tap: play or pause; hold: the panel), the thumb row of buttons over the shelf, the readout strip, and
//! copy and paste.

use super::draw::*;
use super::gesture::{box_select, sel_of, Btn, Gesture};
use super::*;
use cogs_sim::PartRef;
use egui::{vec2, Align2, FontId, Stroke};
use serde_json::{json, Value};

const SLOT_MIN: f64 = 58.0;
const FLING_MS: f64 = 0.26;

impl Stage {
    pub(super) fn slot_w(&self) -> f64 {
        SLOT_MIN.max(self.cw / PARTS.len() as f64)
    }
    pub(super) fn shelf_max(&self) -> f64 {
        (self.slot_w() * PARTS.len() as f64 - self.cw).max(0.0)
    }
    pub(super) fn set_shelf_x(&mut self, x: f64) {
        self.shelf_x = x.clamp(0.0, self.shelf_max());
    }
    pub(super) fn slot_at(&self, x: f64) -> &'static str {
        let i = ((x + self.shelf_x) / self.slot_w()).floor().clamp(0.0, PARTS.len() as f64 - 1.0) as usize;
        PARTS[i]
    }
    pub(super) fn step_shelf(&mut self) {
        if let Some((v, t)) = self.shelf_fling {
            let dt = (self.now - t).min(0.05);
            let x = self.shelf_x + v * dt;
            self.set_shelf_x(x);
            let v = v * (-dt / FLING_MS).exp();
            self.shelf_fling = if v.abs() < 20.0 || self.shelf_x <= 0.0 || self.shelf_x >= self.shelf_max() { None } else { Some((v, self.now)) };
        }
        self.set_shelf_x(self.shelf_x);
    }
    fn glyph_at(&self) -> (f64, f64) {
        (self.cw - 32.0, self.shelf_top() - 32.0)
    }

    /// The selection's editor: a disc's notes, a voice's patch, a cam's shape.
    pub fn editor_mode(&self, w: &World) -> Option<&'static str> {
        match self.sel? {
            Sel::Cog(i) if w.cog(i).map_or(false, |c| c.track.is_some()) => Some("notes"),
            Sel::Pad(_) | Sel::Reader(_) | Sel::Pit(_) => Some("patch"),
            Sel::Cam(_) => Some("shape"),
            _ => None,
        }
    }
    /// The thumb row: every button shown now, with its rect in stage coordinates.
    pub(super) fn buttons(&self, w: &World) -> Vec<(Btn, Rect)> {
        let mut out = Vec::new();
        let busy = self.io.busy || self.carrying;
        if busy {
            return out;
        }
        let r = |x: f64, y: f64, wd: f64| Rect::from_min_size(pos2(x as f32, y as f32), vec2(wd as f32, 44.0));
        let top = self.shelf_top() - 54.0;
        let mut x = 8.0;
        if self.editor_mode(w).is_some() {
            out.push((Btn::Edit, r(x, top, 44.0)));
            x += 44.0;
        }
        if self.io.recording || self.io.can_record {
            out.push((Btn::Rec, r(x, top, 44.0)));
            x += 44.0;
        }
        if !self.zeroable(w).is_empty() {
            out.push((Btn::Zero, r(x, top, 44.0)));
        }
        let clip = self.shelf_top() - 102.0;
        let mut x = 8.0;
        if !self.group.is_empty() || matches!(self.sel, Some(s) if !matches!(s, Sel::Dot { .. })) {
            out.push((Btn::Copy, r(x, clip, 44.0)));
            x += 52.0;
        }
        if self.clip.is_some() {
            out.push((Btn::Paste, r(x, clip, 44.0)));
            x += 52.0;
        }
        if self.sel.is_some() || !self.group.is_empty() {
            out.push((Btn::Delete, r(x, clip, 44.0)));
        }
        let label = self.io.midi_label.unwrap_or("MIDI");
        let mw = (16.0 + 6.0 + 20.0 + label.len() as f64 * 7.8).clamp(44.0, 170.0);
        out.push((Btn::Midi, r(self.cw - 8.0 - mw, clip, mw)));
        out.push((Btn::Undo, r(self.cw - 106.0 - 88.0, top, 44.0)));
        out.push((Btn::Redo, r(self.cw - 106.0 - 44.0, top, 44.0)));
        out.push((Btn::Start, r(self.cw - 62.0 - 44.0, top, 44.0)));
        out
    }

    /// The chrome over the stage takes a press first: the shelf, the glyph, the buttons.
    pub(super) fn chrome_hit(&mut self, w: &World, s: (f64, f64)) -> Option<Gesture> {
        if s.1 >= self.shelf_top() {
            self.shelf_fling = None;
            return Some(Gesture::Shelf { s0: s, x0: self.shelf_x, kind: self.slot_at(s.0), mode: 0, inner: None, last: (s.0, self.now), v: 0.0 });
        }
        if dist(s, self.glyph_at()) < 26.0 {
            return Some(Gesture::Glyph);
        }
        let p = pos2(s.0 as f32, s.1 as f32);
        for (b, r) in self.buttons(w) {
            if r.contains(p) {
                return Some(Gesture::Button(b));
            }
        }
        None
    }
    pub(super) fn press(&mut self, w: &mut World, b: Btn) {
        match b {
            Btn::Edit => {
                if let Some(m) = self.editor_mode(w) {
                    self.out.push(StageOut::OpenEditor(m));
                }
            }
            Btn::Rec => self.out.push(StageOut::Record),
            Btn::Zero => self.zero_sel(w),
            Btn::Copy => {
                if self.copy_parts(w) {
                    self.buzz(8);
                    let n = self.group.len().max(1);
                    self.say(format!("copy parts: {n}"), false);
                }
            }
            Btn::Paste => {
                // a finger has no hover: paste lands in the middle of the open stage
                let h = self.hover;
                self.hover = Some((self.cw / 2.0, (self.safe_top + self.shelf_top()) / 2.0));
                self.paste_parts(w);
                self.hover = h;
                self.buzz(8);
            }
            Btn::Delete => self.remove_sel(w),
            Btn::Midi => self.out.push(StageOut::ConnectMidi),
            Btn::Undo => self.undo(w),
            Btn::Redo => self.redo(w),
            Btn::Start => self.to_start(w),
        }
    }

    // ---------- drawing the chrome ----------
    pub(super) fn draw_chrome(&mut self, p: &egui::Painter, w: &World) {
        if let Some(g) = self.carry_ghost {
            // an arm with nothing to ride yet
            let z = w.s.zoom;
            let o = self.scr(g);
            let pen = Pen::new(p, z, o.x as f64, o.y as f64, w.clock.view);
            let ghost = Pen { ..pen };
            draw_arm(&ghost, -24.0, 0.0, 0.0, 0.0, 24.0, 0.0, f64::NEG_INFINITY);
        }
        self.draw_shelf(p);
        self.draw_glyph(p, w);
        self.draw_buttons(p, w);
        self.draw_readout(p);
    }
    fn draw_shelf(&mut self, p: &egui::Painter) {
        let top = self.shelf_top();
        let r = Rect::from_min_max(self.scr((0.0, top)), self.scr((self.cw, self.ch)));
        p.rect_filled(r, 0.0, BG);
        let sw = self.slot_w();
        let clip = p.with_clip_rect(Rect::from_min_max(self.scr((0.0, top + 1.0)), self.scr((self.cw, top + SHELF_H as f64))));
        for (i, kind) in PARTS.iter().enumerate() {
            let x = sw * (i as f64 + 0.5) - self.shelf_x;
            if x < -sw || x > self.cw + sw {
                continue;
            }
            let o = self.scr((x, top + SHELF_H as f64 / 2.0));
            let s = icon_scale(kind);
            let pen = Pen::new(&clip, s, o.x as f64, o.y as f64, f64::NEG_INFINITY);
            draw_part_icon(&pen, kind);
            if self.armed == Some(*kind) {
                let rr = Rect::from_min_max(self.scr((sw * i as f64 - self.shelf_x + 2.0, top + 2.0)), self.scr((sw * (i + 1) as f64 - self.shelf_x - 2.0, top + SHELF_H as f64 - 2.0)));
                p.rect_stroke(rr, 0.0, Stroke::new(1.0, HOT), egui::StrokeKind::Inside);
            }
        }
        if self.scrap {
            // the tray dims, and pulses red: letting go here deletes
            p.rect_filled(r, 0.0, alpha(BG, 0.65));
            let a = 0.2 + 0.08 * (self.now * 1000.0 / 90.0).sin();
            p.rect_filled(r, 0.0, alpha(BAD, a as f32));
        }
        p.line_segment([self.scr((0.0, top)), self.scr((self.cw, top))], Stroke::new(1.0, FAINT));
        // more parts past either edge: the tray fades out there
        for (on, x0, x1) in [(self.shelf_x > 1.0, 0.0, 28.0), (self.shelf_x < self.shelf_max() - 1.0, self.cw, self.cw - 28.0)] {
            if !on {
                continue;
            }
            let mut m = egui::epaint::Mesh::default();
            let (a, b) = (self.scr((x0, top + 1.0)), self.scr((x1, top + SHELF_H as f64)));
            m.colored_vertex(pos2(a.x, a.y), BG);
            m.colored_vertex(pos2(b.x, a.y), Color32::TRANSPARENT);
            m.colored_vertex(pos2(b.x, b.y), Color32::TRANSPARENT);
            m.colored_vertex(pos2(a.x, b.y), BG);
            m.add_triangle(0, 1, 2);
            m.add_triangle(0, 2, 3);
            p.add(egui::Shape::mesh(m));
        }
    }
    fn draw_glyph(&self, p: &egui::Painter, w: &World) {
        let (x, y) = self.glyph_at();
        let c = self.scr((x, y));
        let bpm = if w.s.bpm != 0.0 { w.s.bpm } else { w.cogs.iter().map(|c| w.motor_rpm(c)).find(|r| *r != 0.0).unwrap_or(0.0).abs() };
        let a = (w.clock.view * std::f64::consts::PI * bpm / 60.0) as f32;
        let k = if w.clock.paused { 0.45 } else { 1.0 };
        p.circle_stroke(c, 13.0, Stroke::new(1.0, alpha(DIM, k)));
        let d = vec2(a.cos(), a.sin()) * 13.0;
        p.line_segment([c - d, c + d], Stroke::new(1.0, alpha(FG, k)));
        p.circle_filled(c - d, 2.0, alpha(FG, k));
        p.circle_filled(c + d, 2.0, alpha(FG, k));
    }
    fn draw_buttons(&mut self, p: &egui::Painter, w: &World) {
        for (b, r) in self.buttons(w) {
            let r = r.translate(self.rect.min.to_vec2());
            let icon = |me: &mut Stage, name: &str, flip: bool, tint: Color32| {
                let ir = Rect::from_center_size(r.center(), vec2(22.0, 22.0));
                me.icons.paint(p, name, flip, ir, tint);
            };
            match b {
                Btn::Edit => icon(self, if self.editor_mode(w) == Some("notes") { "Piano" } else { "MixerVertical" }, false, FG),
                Btn::Rec => {
                    if self.io.recording {
                        p.rect_filled(r, 8.0, PAD);
                    }
                    icon(self, "Record", false, HOT)
                }
                Btn::Zero => icon(self, "Zero", false, FG),
                Btn::Copy => icon(self, "Copy", false, FG),
                Btn::Paste => icon(self, "Clipboard", false, FG),
                Btn::Delete => icon(self, "Trash", false, FG),
                Btn::Undo => icon(self, "Reset", false, alpha(FG, if self.edits.can_undo() { 1.0 } else { 0.3 })),
                Btn::Redo => icon(self, "Reset", true, alpha(FG, if self.edits.can_redo() { 1.0 } else { 0.3 })),
                Btn::Start => icon(self, "TrackPrevious", false, if w.clock.at_start { HOT } else { FG }),
                Btn::Midi => {
                    let on = self.io.midi_connected;
                    let (bg, ink) = if on { (SEL, BG) } else { (PAD, SEL) };
                    p.rect_filled(r, 8.0, bg);
                    // the Bluetooth rune, stroked
                    let o = r.left_center() + vec2(10.0, -8.0);
                    let k = 16.0 / 15.0;
                    let pts = [(4.5, 4.5), (10.5, 10.0), (7.5, 12.5), (7.5, 2.5), (10.5, 5.0), (4.5, 10.5)];
                    let pts: Vec<Pos2> = pts.iter().map(|&(x, y)| o + vec2(x * k, y * k)).collect();
                    p.add(egui::Shape::line(pts, Stroke::new(1.2, ink)));
                    p.text(r.left_center() + vec2(32.0, 0.0), Align2::LEFT_CENTER, self.io.midi_label.unwrap_or("MIDI"), FontId::monospace(13.0), ink);
                }
            }
        }
    }
    fn draw_readout(&mut self, p: &egui::Painter) {
        let Some((text, until)) = &self.readout else { return };
        if self.now > *until {
            self.readout = None;
            return;
        }
        let top = self.ch - INTERACTION_H as f64 - self.safe_bot;
        let r = Rect::from_min_max(self.scr((0.0, top)), self.scr((self.cw, self.ch)));
        p.rect_filled(r, 0.0, BG);
        p.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, LINE));
        p.text(self.scr((self.cw / 2.0, top + INTERACTION_H as f64 / 2.0)), Align2::CENTER_CENTER, text, FontId::monospace(14.0), HOT);
    }

    // ---------- the readout ----------
    pub(super) fn say_gesture(&mut self, w: &World, g: &Gesture, held: bool) {
        let t = self.readout_of(w, g).unwrap_or_else(|| format!("view zoom: {}%", (w.s.zoom * 100.0).round()));
        self.say(t, held);
    }
    pub(super) fn readout_of(&self, w: &World, g: &Gesture) -> Option<String> {
        let kind_label = |k: cogs_sim::VoiceKind| match k {
            cogs_sim::VoiceKind::Synth => "modal".to_string(),
            cogs_sim::VoiceKind::Wave => "synth".to_string(),
            k => k.name().to_string(),
        };
        let on_off = |b: bool| if b { "on" } else { "off" };
        Some(match g {
            Gesture::Carry { kind, .. } => format!("place part: {kind}"),
            Gesture::Shelf { mode, kind, .. } => {
                if *mode == 2 {
                    let i = PARTS.iter().position(|k| *k == self.slot_at(self.cw / 2.0)).unwrap_or(0);
                    format!("parts: {} of {}", i + 1, PARTS.len())
                } else {
                    format!("place part: {kind}")
                }
            }
            Gesture::Glyph => format!("playback: {}", if w.clock.paused { "paused" } else { "playing" }),
            Gesture::Arm { id, .. } | Gesture::ArmBody(id) => {
                let a = w.arms.iter().find(|a| a.id == *id)?;
                format!("arm length: {} px · angle: {}°", a.len.round(), (cogs_sim::wrap(a.a) * 180.0 / std::f64::consts::PI).round())
            }
            Gesture::Pad { id, rim, .. } => {
                let o = w.pads.iter().find(|a| a.id == *id)?;
                if *rim {
                    format!("pad radius: {} px", fmt3(o.r))
                } else {
                    let v = &o.voice;
                    let note = if matches!(v.kind, cogs_sim::VoiceKind::Synth | cogs_sim::VoiceKind::Wave) {
                        format!(" · {}", note_name(v.midi(v.note) as i64))
                    } else {
                        String::new()
                    };
                    format!("pad voice: {}{note}", kind_label(v.kind))
                }
            }
            Gesture::Span { reader, id, end, .. } => {
                if *reader {
                    let r = w.readers.iter().find(|a| a.id == *id)?;
                    let len = (r.x2 - r.x1).hypot(r.y2 - r.y1).round();
                    let what = if r.tines != 0.0 { "comb" } else { "reader" };
                    if *end != 0 { format!("{what} length: {len} px") } else { format!("{what} voice: {}", kind_label(r.voice.kind)) }
                } else {
                    let s = w.strings.iter().find(|a| a.id == *id)?;
                    format!("{} note: {} · {} px", s.kind, note_name(s.midi().round() as i64), s.len().round())
                }
            }
            Gesture::Belt(id) => format!("belt crossed: {}", on_off(w.belts.iter().find(|b| b.id == *id)?.cross)),
            Gesture::Cog { id, .. } => {
                let c = w.cog(*id)?;
                format!("{} position: {}, {}", if c.track.is_some() { "disc" } else if c.motor.is_some() { "motor" } else { "cog" }, c.x.round(), c.y.round())
            }
            Gesture::Rim { id, .. } => format!("teeth: {}", w.cog(*id)?.n),
            Gesture::Knob { id, mode, .. } => {
                let ci = w.cog_ix(*id)?;
                match mode {
                    0 => format!("phase: {:.1}°", cogs_sim::wrap(w.render_th(ci)) * 180.0 / std::f64::consts::PI),
                    1 => format!("speed rpm: {:.1}", w.rpm(ci)),
                    _ => format!("scrub rpm: {:.1}", self.spin.as_ref().map(|s| s.rpm).unwrap_or(0.0)),
                }
            }
            Gesture::Notes { id, .. } => {
                let ci = w.cog_ix(*id)?;
                let at = self.gesture.as_ref().map(|a| a.last).unwrap_or((0.0, 0.0));
                let q = self.to_world(w, at);
                let t = w.cogs[ci].track.as_ref()?;
                match w.cell_at(ci, q.0, q.1) {
                    Some((ring, _)) => format!("disc note: {}", note_name(t.ring_note(ring as i64) as i64)),
                    None => format!("disc notes: {} pins", t.dots.len()),
                }
            }
            Gesture::Pan { boxing, .. } => {
                if *boxing {
                    format!("select parts: {}", self.group.len())
                } else {
                    format!("view: {}% · {}, {}", (w.s.zoom * 100.0).round(), w.view.x.round(), w.view.y.round())
                }
            }
            Gesture::Group { .. } => format!("move parts: {}", self.group.len()),
            Gesture::Marquee { .. } => format!("select parts: {}", self.group.len()),
            Gesture::Cam(id) => {
                let m = w.cams.iter().find(|m| m.id == *id)?;
                format!("cam {} lift: {}%", m.id, (w.cam_lift(m) * 100.0).round())
            }
            Gesture::Probe(id) => {
                let pr = w.probes.iter().find(|m| m.id == *id)?;
                let host = match pr.host {
                    Some(cogs_sim::Host::Ball) => format!("ball {} in sphere {}", pr.ball, pr.on),
                    Some(h) => format!("{} {}", h.name(), pr.on),
                    None => "nothing".into(),
                };
                format!("probe {} on {host}: {}%", pr.id, (w.probe_now(pr) * 100.0).round())
            }
            Gesture::Field { id, edge, .. } => {
                let f = w.fields.iter().find(|m| m.id == *id)?;
                if *edge { format!("fx {} radius {}", f.id, f.r.round()) } else { format!("fx {}: {}", f.id, if f.mode == "dist" { "distortion" } else { &f.mode }) }
            }
            Gesture::PitSize(id) => format!("sphere: {} across", w.pits.iter().find(|p| p.id == *id)?.w.round()),
            Gesture::PitIn { id, ball, .. } => {
                let p = w.pits.iter().find(|p| p.id == *id)?;
                match ball.and_then(|b| p.balls.iter().find(|x| x.id == b)) {
                    Some(b) => format!("ball {}", note_name(b.note as i64)),
                    None => format!("{} ball{}", p.balls.len(), if p.balls.len() == 1 { "" } else { "s" }),
                }
            }
            _ => return None,
        })
    }
    pub(super) fn grip_readout(&self, w: &World, g: &super::gesture::Grip) -> String {
        use super::gesture::Grip;
        match g {
            Grip::Cog { id, .. } => format!("teeth: {}", w.cog(*id).map(|c| c.n).unwrap_or(0.0)),
            Grip::Field { id, .. } => format!("fx {id} radius {}", w.fields.iter().find(|f| f.id == *id).map(|f| f.r.round()).unwrap_or(0.0)),
            Grip::Pad { id, .. } => format!("pad radius: {} px", fmt3(w.pads.iter().find(|f| f.id == *id).map(|f| f.r).unwrap_or(0.0))),
            Grip::Arm { id, .. } => format!("arm length: {}", w.arms.iter().find(|f| f.id == *id).map(|f| f.len.round()).unwrap_or(0.0)),
            Grip::SpanSeated { id, .. } | Grip::SpanFree { reader: true, id, .. } => {
                format!("reader length: {}", w.readers.iter().find(|f| f.id == *id).map(|r| (r.x2 - r.x1).hypot(r.y2 - r.y1).round()).unwrap_or(0.0))
            }
            Grip::SpanFree { id, .. } => format!("string length: {}", w.strings.iter().find(|f| f.id == *id).map(|r| r.len().round()).unwrap_or(0.0)),
        }
    }

    // ---------- copy and paste ----------
    /// copyParts: a copied cog takes its arms and cams; a belt comes when both its cogs do.
    pub(super) fn copy_parts(&mut self, w: &World) -> bool {
        let items: Vec<Sel> = if !self.group.is_empty() { self.group.clone() } else { self.sel.into_iter().filter(|s| !matches!(s, Sel::Dot { .. })).collect() };
        if items.is_empty() {
            return false;
        }
        let ids: Vec<u64> = items.iter().filter_map(|s| s.part()).map(|p| sel_id(sel_of(p)) as u64).collect();
        let j = w.serialize();
        let has = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_u64()).map_or(false, |i| ids.contains(&i));
        let list = |k: &str, f: &dyn Fn(&Value) -> bool| Value::Array(j[k].as_array().map(|a| a.iter().filter(|v| f(v)).cloned().collect()).unwrap_or_default());
        self.clip = Some(json!({
            "cogs": list("cogs", &|v| has(v, "id")), "arms": list("arms", &|v| has(v, "id") || has(v, "cog")), "pads": list("pads", &|v| has(v, "id")),
            "readers": list("readers", &|v| has(v, "id")), "strings": list("strings", &|v| has(v, "id")),
            "belts": list("belts", &|v| has(v, "a") && has(v, "b")), "cams": list("cams", &|v| has(v, "id") || has(v, "cog")),
            "pits": list("pits", &|v| has(v, "id")), "fields": list("fields", &|v| has(v, "id")),
        }));
        true
    }
    /// pasteParts: lands centred under the pointer (or a step down-right), slid clear of other cogs, selected.
    pub(super) fn paste_parts(&mut self, w: &mut World) -> bool {
        let Some(mut c) = self.clip.clone() else { return false };
        let mut j = w.serialize();
        let mut map: Vec<(u64, u64)> = Vec::new();
        let next = w.next as u64;
        let fresh = |old: u64, map: &mut Vec<(u64, u64)>| {
            let n = next + map.len() as u64;
            map.push((old, n));
            n
        };
        let get = |v: &Value, k: &str| v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
        let mut pts = Vec::new();
        for k in ["cogs", "pads", "pits", "fields"] {
            for o in c[k].as_array().unwrap() {
                pts.push((get(o, "x"), get(o, "y")));
            }
        }
        for k in ["readers", "strings"] {
            for o in c[k].as_array().unwrap() {
                pts.push(((get(o, "x1") + get(o, "x2")) / 2.0, (get(o, "y1") + get(o, "y2")) / 2.0));
            }
        }
        let step = 30.0 / w.s.zoom;
        let (mut dx, mut dy) = (step, step);
        if let (false, Some(h)) = (pts.is_empty(), self.hover) {
            let at = self.to_world(w, h);
            let (x0, x1) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| (a.0.min(p.0), a.1.max(p.0)));
            let (y0, y1) = pts.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |a, p| (a.0.min(p.1), a.1.max(p.1)));
            dx = at.0 - (x0 + x1) / 2.0;
            dy = at.1 - (y0 + y1) / 2.0;
        }
        let bump = |o: &mut Value, k: &str, d: f64| {
            let v = o[k].as_f64().unwrap_or(0.0) + d;
            o[k] = json!(v);
        };
        for k in ["cogs", "pads", "fields", "pits"] {
            for o in c[k].as_array_mut().unwrap() {
                let id = fresh(o["id"].as_u64().unwrap_or(0), &mut map);
                o["id"] = json!(id);
                bump(o, "x", dx);
                bump(o, "y", dy);
                if k == "pits" {
                    for b in o["balls"].as_array_mut().into_iter().flatten() {
                        bump(b, "bx", dx);
                        bump(b, "by", dy);
                    }
                }
            }
        }
        for k in ["readers", "strings"] {
            for o in c[k].as_array_mut().unwrap() {
                let id = fresh(o["id"].as_u64().unwrap_or(0), &mut map);
                o["id"] = json!(id);
                for (a, d) in [("x1", dx), ("y1", dy), ("x2", dx), ("y2", dy)] {
                    bump(o, a, d);
                }
            }
        }
        let look = |map: &Vec<(u64, u64)>, id: Option<u64>| id.and_then(|i| map.iter().find(|m| m.0 == i).map(|m| m.1));
        for o in c["readers"].as_array_mut().unwrap() {
            o["on"] = json!(look(&map, o["on"].as_u64()));
        }
        let live: Vec<u64> = w.cogs.iter().map(|c| c.id as u64).collect();
        for k in ["arms", "cams"] {
            let arr = c[k].as_array_mut().unwrap();
            arr.retain(|a| a["cog"].as_u64().map_or(false, |i| look(&map, Some(i)).is_some() || live.contains(&i)));
            for a in arr.iter_mut() {
                match look(&map, a["cog"].as_u64()) {
                    Some(n) => a["cog"] = json!(n),
                    None => bump(a, if k == "arms" { "a" } else { "ang" }, 0.4),
                }
                if k == "cams" {
                    if let Some(n) = look(&map, a["to"].as_u64()) {
                        a["to"] = json!(n);
                    }
                }
            }
        }
        for k in ["arms", "cams"] {
            for a in c[k].as_array_mut().unwrap() {
                let id = fresh(a["id"].as_u64().unwrap_or(0), &mut map);
                a["id"] = json!(id);
            }
        }
        for b in c["belts"].as_array_mut().unwrap() {
            b["a"] = json!(look(&map, b["a"].as_u64()));
            b["b"] = json!(look(&map, b["b"].as_u64()));
            let id = fresh(b["id"].as_u64().unwrap_or(0), &mut map);
            b["id"] = json!(id);
        }
        for (k, v) in c.as_object().unwrap() {
            if let (Some(dst), Some(src)) = (j[k.as_str()].as_array_mut(), v.as_array()) {
                dst.extend(src.iter().cloned());
            }
        }
        if w.reload(&j).is_err() {
            return false;
        }
        let made: Vec<u32> = map.iter().map(|m| m.1 as u32).collect();
        let pasted = box_select(w, (-1e12, -1e12), (1e12, 1e12)).into_iter().filter(|s| made.contains(&sel_id(*s))).collect::<Vec<_>>();
        let mut pasted = pasted;
        for (i, a) in w.arms.iter().enumerate() {
            let _ = i;
            if made.contains(&a.id) && !pasted.contains(&Sel::Arm(a.id)) {
                pasted.push(Sel::Arm(a.id));
            }
        }
        // a paste that would land on cogs slides to the nearest clear spot, everything it brought with it
        let pc: Vec<(usize, f64, f64)> = pasted.iter().filter_map(|s| if let Sel::Cog(i) = s { w.cog_ix(*i).map(|ci| (ci, w.cogs[ci].x, w.cogs[ci].y)) } else { None }).collect();
        let (ex, ey) = w.clear_shift(&pc, 0.0, 0.0, None);
        if ex != 0.0 || ey != 0.0 {
            for s in &pasted {
                match *s {
                    Sel::Pad(i) => {
                        if let Some(o) = w.pads.iter_mut().find(|o| o.id == i) {
                            o.x += ex;
                            o.y += ey;
                        }
                    }
                    Sel::Field(i) => {
                        if let Some(o) = w.fields.iter_mut().find(|o| o.id == i) {
                            o.x += ex;
                            o.y += ey;
                        }
                    }
                    Sel::Pit(i) => {
                        if let Some(o) = w.pits.iter_mut().find(|o| o.id == i) {
                            o.shift(ex, ey);
                        }
                    }
                    Sel::Reader(i) => {
                        if let Some(o) = w.readers.iter_mut().find(|o| o.id == i && o.on.is_none()) {
                            o.x1 += ex;
                            o.y1 += ey;
                            o.x2 += ex;
                            o.y2 += ey;
                        }
                    }
                    Sel::String(i) => {
                        if let Some(o) = w.strings.iter_mut().find(|o| o.id == i) {
                            o.x1 += ex;
                            o.y1 += ey;
                            o.x2 += ex;
                            o.y2 += ey;
                        }
                    }
                    _ => {}
                }
            }
            w.solve();
        }
        if pasted.len() == 1 {
            self.select(Some(pasted[0]));
        } else {
            self.sel = None;
            self.group = pasted;
        }
        self.changed(w, true);
        let _ = PartRef::Cog(0);
        true
    }
}

pub(super) fn sel_id(s: Sel) -> u32 {
    match s {
        Sel::Cog(i) | Sel::Arm(i) | Sel::Pad(i) | Sel::Reader(i) | Sel::String(i) | Sel::Belt(i) | Sel::Cam(i) | Sel::Pit(i) | Sel::Probe(i) | Sel::Field(i) => i,
        Sel::Dot { cog, .. } => cog,
    }
}
fn fmt3(x: f64) -> String {
    let s = format!("{:.3}", x);
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}
