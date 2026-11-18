//! The wave synth's editor (wave-ui.js): knobs in tabs, the mod sources, the matrix and the patch library, as a sheet
//! over the stage. Serum's way to modulate: tap a source chip to arm it, then drag a knob to set how far that source
//! moves it; each route shows as a blue arc inside the knob's ring. Double tap resets a knob, or, armed, removes that
//! route. Stray sideways from the knob while dragging for finer steps.
//!
//! The editor works on its own copy of the patch and only takes the world's lock to read a changed voice and to
//! write an edit back (the voice's `ver` bumps, so the sync thread recompiles the patch for the audio thread).
use crate::play::wave_patch;
use cogs_dsp::params::*;
use cogs_dsp::table::wave_spectra;
use cogs_dsp::verb::{VERB_COLORS, VERB_MODES};
use cogs_dsp::{WavePatch, WAVE_PRESET_NAMES};
use cogs_sim::{Part, VoiceKind, World};
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::f32::consts::PI;
use std::path::PathBuf;

const TABS: [&str; 9] = ["MAIN", "A", "B", "C", "VOICE", "MOD", "FX", "MATRIX", "PATCH"];
const QUICK: [&str; 6] = ["osc1.cut", "osc1.res", "osc1.a", "osc1.r", "osc1.detune", "vol"];
const DIAL_MAX: usize = 8;
const HOT: Color32 = Color32::from_rgb(0xff, 0x9a, 0x3c);
const SEL: Color32 = Color32::from_rgb(0x5a, 0xa8, 0xff);
const LINE: Color32 = Color32::from_rgb(0x44, 0x48, 0x50);
const FG: Color32 = Color32::from_rgb(0xe8, 0xe8, 0xe8);
const DIM: Color32 = Color32::from_rgb(0x90, 0x94, 0x9a);
const BG: Color32 = Color32::from_rgb(0x16, 0x17, 0x1a);
const CARD: Color32 = Color32::from_rgb(0x20, 0x22, 0x26);
const WELL: Color32 = Color32::from_rgb(0x2a, 0x2c, 0x32);
const ENVC: Color32 = Color32::from_rgb(0x6c, 0xd0, 0x8a);
const ODD: Color32 = Color32::from_rgb(0xc0, 0x8c, 0xff);
const TAB_NAMES: [&str; 9] = ["Main", "A", "B", "C", "Voice", "Mod", "FX", "Matrix", "Patch"];
/// A source's colour by kind: envelopes green, LFOs blue, macros orange, the rest violet.
fn src_col(s: u8) -> Color32 {
    let s = s as usize;
    if s < SRC_LFO0 { ENVC } else if s < SRC_MAC0 { SEL } else if s < SRC_VEL { HOT } else { ODD }
}
/// A rounded panel holding one group of controls, its title small and dim.
fn card(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    egui::Frame::new().fill(CARD).corner_radius(10.).inner_margin(10.).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if !title.is_empty() { ui.label(egui::RichText::new(title.to_uppercase()).size(10.5).color(DIM)); }
        add(ui);
    });
}
/// An envelope's shape from its knobs (each stage as wide as its knob is turned), so ADSR reads at a glance.
fn env_graph(ui: &mut Ui, a: f64, d: f64, s: f64, r: f64, col: Color32) {
    let w = ui.available_width().min(260.);
    let (rect, _) = ui.allocate_exact_size(vec2(w, 40.), Sense::hover());
    let tp = &wave_params()[O_A];
    let n = |x: f64| wave_norm(tp, x).clamp(0., 1.) as f32;
    let (wa, wd, wr) = (n(a) * 0.3 + 0.02, n(d) * 0.3 + 0.02, n(r) * 0.3 + 0.02);
    let tot = wa + wd + 0.2 + wr;
    let x = |u: f32| rect.left() + u / tot * rect.width();
    let y = |v: f32| rect.bottom() - 2. - v * (rect.height() - 4.);
    let s = s.clamp(0., 1.) as f32;
    let pts = vec![pos2(x(0.), y(0.)), pos2(x(wa), y(1.)), pos2(x(wa + wd), y(s)), pos2(x(wa + wd + 0.2), y(s)), pos2(x(tot), y(0.))];
    let p = ui.painter();
    let mut fill = pts.clone(); fill.push(pos2(x(0.), y(0.)));
    p.add(Shape::convex_polygon(vec![], Color32::TRANSPARENT, Stroke::NONE));
    for k in 0..4 { p.add(Shape::convex_polygon(vec![pts[k], pts[k + 1], pos2(pts[k + 1].x, y(0.)), pos2(pts[k].x, y(0.))], col.gamma_multiply(0.15), Stroke::NONE)); }
    p.add(Shape::line(pts, Stroke::new(2., col)));
}

pub enum EditorOut {
    /// Sound the part once (the play button, a preset step).
    Audition(u32),
    Close,
}

struct Drag { n0: f64, a0: f64, acc: f64, x0: f32, route: Option<usize>, fresh: bool }

pub struct WaveEditor {
    /// Room to leave under the sheet for the system's navigation bar, in points.
    pub safe_bottom: f32,
    /// Where the audio has the part's modulated knobs now, on its newest note: (knob, position 0..1). Set by the app.
    pub live: Vec<(u16, f32)>,
    /// The macros a cam or probe is turning, and to where.
    driven: [Option<f64>; WAVE_MACROS],
    pub part: Part,
    p: WavePatch,
    dial: Vec<String>,
    ver: u32,
    tab: usize,
    arm: Option<u8>,
    pin: bool,
    msg: String,
    json: String,
    lib: Option<PathBuf>,
    open_routes: HashSet<usize>,
    drag: Option<(usize, Drag)>,
    say: Option<(String, f64)>,
    dirty: bool,
    redraw_fx: bool,
}

fn src_label(s: u8) -> String {
    let n = WAVE_SOURCES[s as usize];
    n.to_uppercase()
}
/// A knob's name where it stands alone (the matrix, the dial): which oscillator, envelope, LFO or module it is on.
fn long_label(p: &WavePatch, i: usize) -> String {
    let d = &wave_params()[i];
    let path = d.path.as_str();
    if let Some(r) = path.strip_prefix("osc") { return format!("{} {}", &"ABC"[(r.as_bytes()[0] - b'1') as usize..][..1], d.label) }
    for g in ["env", "lfo"] { if let Some(r) = path.strip_prefix(g) { if r.as_bytes().first().map_or(false, |c| c.is_ascii_digit()) { return format!("{}{} {}", g.to_uppercase(), &r[..1], d.label) } } }
    if path.len() == 2 && path.starts_with('m') { return p.macro_names[(path.as_bytes()[1] - b'1') as usize].clone() }
    if is_fx_param(i) { return format!("{} {}", path.split('.').next().unwrap_or(""), d.label) }
    if path.starts_with("tune.") { return format!("synth {}", d.label) }
    d.label.to_string()
}
fn kfmt(x: f64) -> String {
    if x >= 1000. { if x >= 10000. { format!("{:.1}k", x / 1000.) } else { format!("{:.2}k", x / 1000.) } } else if x < 10. { format!("{x:.2}") } else { format!("{}", x.round()) }
}
/// waveText: a knob's value as the editor reads it out.
fn value_text(d: &ParamDef, x: f64) -> String {
    let p = d.path.as_str();
    if p.starts_with("eq.") || p == "comp.thresh" || p == "comp.gain" { return format!("{}{:.1} dB", if x > 0. { "+" } else { "" }, x) }
    if p == "comp.ratio" { return format!("{x:.1}:1") }
    if d.curve == Curve::Time { return if x < 1. { format!("{} ms", (x * 1000.).round()) } else { format!("{x:.2} s") } }
    if p == "reverb.decay" { return if x < 10. { format!("{x:.2} s") } else { format!("{x:.1} s") } }
    if d.curve == Curve::Log {
        let hz = [".rate", ".freq", ".cut", ".lowcut", ".highcut"].iter().any(|s| p.ends_with(s));
        return if hz { format!("{} Hz", kfmt(x)) } else { kfmt(x) };
    }
    if d.curve == Curve::Int { return format!("{}{}", if x > 0. && d.min < 0. { "+" } else { "" }, x) }
    if p.ends_with(".fine") { return format!("{}{} ct", if x > 0. { "+" } else { "" }, x.round()) }
    if d.min < 0. { return format!("{}{}", if x > 0. { "+" } else { "" }, (x * 100.).round()) }
    format!("{}%", (x * 100.).round())
}
fn arc(c: Pos2, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    let n = (((a1 - a0).abs() / 0.12).ceil() as usize).max(2);
    (0..=n).map(|k| { let a = a0 + (a1 - a0) * k as f32 / n as f32; c + vec2(a.cos(), a.sin()) * r }).collect()
}
#[derive(Clone, Copy)]
enum Ico { Prev, Next, Close, Up, Down, More, Del }
/// A button with a drawn glyph: the bundled fonts have no arrows.
fn ibtn(ui: &mut Ui, ico: Ico, size: egui::Vec2, enabled: bool, selected: bool) -> egui::Response {
    let r = ui.add_enabled(enabled, egui::Button::new("").selected(selected).min_size(size));
    let col = if enabled { FG } else { LINE };
    let c = r.rect.center();
    let s = Stroke::new(2., col);
    let p = ui.painter();
    match ico {
        Ico::Prev => { p.add(Shape::convex_polygon(vec![c + vec2(-6., 0.), c + vec2(5., -7.), c + vec2(5., 7.)], col, Stroke::NONE)); }
        Ico::Next => { p.add(Shape::convex_polygon(vec![c + vec2(6., 0.), c + vec2(-5., -7.), c + vec2(-5., 7.)], col, Stroke::NONE)); }
        Ico::Close | Ico::Del => { let d = if matches!(ico, Ico::Close) { 7. } else { 5. }; p.line_segment([c + vec2(-d, -d), c + vec2(d, d)], s); p.line_segment([c + vec2(-d, d), c + vec2(d, -d)], s); }
        Ico::Up => { p.add(Shape::line(vec![c + vec2(-6., 3.), c + vec2(0., -4.), c + vec2(6., 3.)], s)); }
        Ico::Down => { p.add(Shape::line(vec![c + vec2(-6., -3.), c + vec2(0., 4.), c + vec2(6., -3.)], s)); }
        Ico::More => { for k in [-6., 0., 6.] { p.circle_filled(c + vec2(k, 0.), 2., col); } }
    };
    r
}
/// Keep the stage's hands off fingers that land on a window or popup over it (the editor, the panel): their touches
/// are taken out of this frame's events before the stage reads them, until they lift. egui's widgets still get them.
pub fn strip_foreign(ctx: &egui::Context, foreign: &mut HashSet<u64>) {
    // where each new press lands, asked before taking the input's write lock (egui's lock is not re-entrant)
    let starts: Vec<Pos2> = ctx.input(|i| i.events.iter().filter_map(|e| match e {
        egui::Event::Touch { phase: egui::TouchPhase::Start, pos, .. } | egui::Event::PointerButton { pos, pressed: true, .. } => Some(*pos),
        _ => None,
    }).collect());
    if starts.is_empty() && foreign.is_empty() { return }
    let over: Vec<bool> = starts.iter().map(|p| ctx.layer_id_at(*p).map_or(false, |l| l.order != egui::Order::Background)).collect();
    let mut k = 0;
    ctx.input_mut(|i| i.events.retain(|e| match e {
        egui::Event::Touch { id, phase, .. } => {
            let id = id.0;
            match phase {
                egui::TouchPhase::Start => { let o = over[k]; k += 1; if o { foreign.insert(id); false } else { true } }
                egui::TouchPhase::Move => !foreign.contains(&id),
                _ => !foreign.remove(&id),
            }
        }
        egui::Event::PointerButton { pressed: true, .. } => { let o = over[k]; k += 1; if o { foreign.insert(u64::MAX); false } else { true } }
        egui::Event::PointerButton { pressed: false, .. } => !foreign.remove(&u64::MAX),
        egui::Event::PointerMoved(_) => !foreign.contains(&u64::MAX),
        _ => true,
    }));
}
fn sanitize(s: &str) -> String { s.chars().filter(|c| c.is_alphanumeric() || " -_.()".contains(*c)).collect::<String>().trim().to_string() }

impl WaveEditor {
    /// The editor for a part's voice, if it is the wave synth.
    pub fn open(w: &World, part: Part, lib: Option<PathBuf>) -> Option<Self> {
        let v = w.voice(part)?;
        if v.kind != VoiceKind::Wave { return None }
        let mut e = WaveEditor { live: Vec::new(), driven: [None; WAVE_MACROS], part, p: WavePatch::init(), dial: Vec::new(), ver: 0, tab: 0, arm: None, pin: false, msg: String::new(), json: String::new(),
            lib, open_routes: HashSet::new(), drag: None, say: None, dirty: false, redraw_fx: false, safe_bottom: 0. };
        e.load(w);
        Some(e)
    }
    fn load(&mut self, w: &World) {
        let Some(v) = w.voice(self.part) else { return };
        self.p = wave_patch(&v.raw);
        self.dial = v.raw.get("dial").and_then(|d| d.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).filter(|p| param_index(p).is_some()).map(String::from).collect()).unwrap_or_default();
        self.ver = v.ver;
        self.drag = None;
    }
    /// The patch back into the voice: the part's own keys stay, its octave and semitones are folded into the tune.
    fn write(&mut self, w: &mut World) {
        let Some(v) = w.voice_mut(self.part) else { return };
        let j = self.p.to_json();
        let mut raw = v.raw.clone();
        raw.remove("preset");
        if let Value::Object(o) = j { for (k, x) in o { raw.insert(k, x); } }
        raw.insert("octave".into(), json!(0));
        raw.insert("semi".into(), json!(0));
        if self.dial.is_empty() { raw.remove("dial"); } else { raw.insert("dial".into(), json!(self.dial)); }
        v.raw = raw;
        v.recache();
        self.ver = v.ver;
    }
    /// Draw the sheet. Locks `world` only to pick up outside changes and to write edits.
    pub fn show(&mut self, ctx: &egui::Context, world: &std::sync::Mutex<World>) -> Vec<EditorOut> {
        self.show_in(ctx, &mut |f| if let Ok(mut w) = world.lock() { f(&mut w) })
    }
    /// The tab shown, by name (MAIN, A, B, C, VOICE, MOD, FX, MATRIX, PATCH).
    pub fn set_tab(&mut self, name: &str) { if let Some(i) = TABS.iter().position(|t| t.eq_ignore_ascii_case(name)) { self.tab = i } }
    /// As `show`, with the world reached through `with` (it runs the closure under whatever lock the caller has).
    pub fn show_in(&mut self, ctx: &egui::Context, with: &mut dyn FnMut(&mut dyn FnMut(&mut World))) -> Vec<EditorOut> {
        let mut out = Vec::new();
        let mut gone = false;
        let mut driven = [None; WAVE_MACROS];
        with(&mut |w: &mut World| {
            match w.voice(self.part) {
                Some(v) if v.kind == VoiceKind::Wave => { if v.ver != self.ver && self.drag.is_none() { self.load(w) } }
                _ => gone = true,
            }
            let (part, t) = (self.part, w.clock.view);
            w.motions(&mut |m| if m.target == part { driven[m.slot as usize] = Some(m.value_at(t).clamp(0., 1.)) });
        });
        if gone { out.push(EditorOut::Close); return out }
        self.driven = driven;
        // the knobs follow the sound and the machine while the sheet is open
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
        let now = ctx.input(|i| i.time);
        let scr = ctx.content_rect();
        let (wd, ht) = ((scr.width() - 8.).min(560.), (scr.height() * 0.62).max(280.).min(scr.height() - self.safe_bottom - 12.));
        let frame = egui::Frame::new().fill(BG).corner_radius(16.).inner_margin(10.).stroke(Stroke::new(1., Color32::from_gray(48)));
        egui::Window::new("wave synth").id(egui::Id::new("wave-editor")).title_bar(false).resizable(false).collapsible(false).frame(frame)
            .anchor(Align2::CENTER_BOTTOM, [0., -4. - self.safe_bottom]).fixed_size([wd, ht]).show(ctx, |ui| {
                let v = ui.visuals_mut();
                v.selection.bg_fill = HOT; v.selection.stroke = Stroke::new(1., Color32::BLACK);
                v.widgets.inactive.weak_bg_fill = WELL; v.widgets.inactive.bg_fill = WELL;
                v.widgets.hovered.weak_bg_fill = Color32::from_gray(58); v.widgets.active.weak_bg_fill = Color32::from_gray(70);
                for w in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] { w.corner_radius = 8.into(); w.bg_stroke = Stroke::NONE; }
                v.extreme_bg_color = Color32::from_rgb(0x10, 0x11, 0x13);
                ui.spacing_mut().item_spacing = vec2(6., 8.);
                ui.spacing_mut().button_padding = vec2(10., 6.);
                self.top(ui, now, &mut out);
                self.tabs(ui);
                let knobs = !matches!(TABS[self.tab], "MATRIX" | "PATCH");
                if knobs { self.chips(ui) }
                egui::ScrollArea::vertical().id_salt(("body", self.tab)).auto_shrink([false, false]).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing = vec2(6., 8.);
                    match TABS[self.tab] {
                        "MAIN" => self.tab_main(ui),
                        "A" => self.tab_osc(ui, 0), "B" => self.tab_osc(ui, 1), "C" => self.tab_osc(ui, 2),
                        "VOICE" => self.tab_voice(ui),
                        "MOD" => self.tab_mod(ui),
                        "FX" => self.tab_fx(ui),
                        "MATRIX" => self.tab_matrix(ui),
                        _ => self.tab_patch(ui, &mut out),
                    }
                    ui.add_space(12.);
                });
            });
        if self.dirty {
            self.dirty = false;
            with(&mut |w: &mut World| self.write(w));
        }
        out
    }
    fn speak(&mut self, s: String, now: f64) { self.say = Some((s, now)) }

    fn top(&mut self, ui: &mut Ui, now: f64, out: &mut Vec<EditorOut>) {
        ui.horizontal(|ui| {
            if ibtn(ui, Ico::Prev, vec2(36., 34.), true, false).clicked() { self.browse(-1); out.push(EditorOut::Audition(self.part.id())) }
            let said = self.say.as_ref().filter(|(_, t)| now - t < 1.6).map(|(s, _)| s.clone());
            let name = said.clone().unwrap_or_else(|| if self.p.name.is_empty() { "patch".into() } else { self.p.name.clone() });
            let w = (ui.available_width() - 36. - 6. - 60. - 6. - 36. - 12.).max(60.);
            ui.add_sized([w, 34.], egui::Label::new(egui::RichText::new(name).size(16.).color(if said.is_some() { SEL } else { FG }).strong()).truncate());
            if ibtn(ui, Ico::Next, vec2(36., 34.), true, false).clicked() { self.browse(1); out.push(EditorOut::Audition(self.part.id())) }
            if ui.add(egui::Button::new(egui::RichText::new("play").color(Color32::BLACK).strong()).fill(HOT).min_size(vec2(60., 34.))).clicked() { out.push(EditorOut::Audition(self.part.id())) }
            if ibtn(ui, Ico::Close, vec2(36., 34.), true, false).clicked() { out.push(EditorOut::Close) }
        });
    }
    /// The tabs as one segmented bar that fits the width.
    fn tabs(&mut self, ui: &mut Ui) {
        let n = TABS.len() as f32;
        let gap = 3.;
        let w = ((ui.available_width() - gap * (n - 1.)) / n).floor();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.spacing_mut().button_padding = vec2(2., 4.);
            for (i, t) in TAB_NAMES.iter().enumerate() {
                let on = i == self.tab;
                let txt = egui::RichText::new(*t).size(12.5).color(if on { Color32::BLACK } else { FG });
                if ui.add_sized([w, 32.], egui::Button::new(txt).selected(on)).clicked() { self.tab = i; self.drag = None }
            }
        });
    }
    /// The mod sources: tap one to arm it, then drag a knob to set how far it moves that knob.
    fn chips(&mut self, ui: &mut Ui) {
        // slid by the finger: a bar here would only sit over the chips and take their taps
        let o = egui::ScrollArea::horizontal().id_salt("chips").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui, |ui| ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.;
            if TABS[self.tab] == "MAIN" && self.chip(ui, "DIAL", FG, self.pin, false) { self.pin = !self.pin; self.arm = None }
            for s in 0..NS as u8 {
                let used = self.p.mods.iter().any(|m| m.src == s);
                if self.chip(ui, &src_label(s), src_col(s), self.arm == Some(s), used) { self.arm = if self.arm == Some(s) { None } else { Some(s) }; self.pin = false }
            }
        }));
        // more chips to the right: fade the row's edge into the sheet
        let r = o.inner_rect;
        if o.inner.response.rect.right() > r.right() + 1. {
            for k in 0..8 {
                let x = r.right() - 24. + 3. * k as f32;
                ui.painter().rect_filled(Rect::from_min_max(pos2(x, r.top()), pos2(x + 3., r.bottom())), 0., BG.gamma_multiply((k + 1) as f32 / 8.));
            }
        }
        let hint = match (self.arm, self.pin) {
            (Some(s), _) => format!("{} armed: drag a knob to set its depth, double tap to remove", src_label(s)),
            (None, true) => "tap knobs to pin them to the part's dial".into(),
            _ if TABS[self.tab] == "MAIN" => "tap a source to modulate with it".into(),
            _ => return,
        };
        ui.label(egui::RichText::new(hint).size(11.).color(if self.arm.is_some() || self.pin { FG } else { DIM }));
    }
    fn chip(&self, ui: &mut Ui, text: &str, col: Color32, on: bool, used: bool) -> bool {
        let font = FontId::proportional(11.5);
        let tw = ui.painter().layout_no_wrap(text.to_string(), font.clone(), col).size().x;
        let (rect, r) = ui.allocate_exact_size(vec2(tw + 18., 30.), Sense::click());
        let p = ui.painter();
        if on { p.rect_filled(rect, 15., col); } else { p.rect_stroke(rect, 15., Stroke::new(1., col.gamma_multiply(0.7)), egui::StrokeKind::Inside); }
        p.text(rect.center(), Align2::CENTER_CENTER, text, font, if on { Color32::BLACK } else { col });
        if used && !on { p.circle_filled(pos2(rect.right() - 6., rect.top() + 6.), 2.5, col); }
        r.clicked()
    }

    // ---------- the knob ----------
    /// The macro this knob is, if a cam or probe is turning it: where it has it.
    fn driven_at(&self, i: usize) -> Option<f64> {
        i.checked_sub(P_MAC0).filter(|&m| m < WAVE_MACROS).and_then(|m| self.driven[m])
    }
    /// Where modulation has a knob now: the audio's word while a note sounds, else what the turning macros alone make of it.
    fn moved_to(&self, i: usize, base: f64) -> Option<f32> {
        if let Some(x) = self.live.iter().find(|x| x.0 as usize == i) { return Some(x.1) }
        let mac = |s: usize| s.checked_sub(SRC_MAC0).filter(|&m| m < WAVE_MACROS);
        let at = |m: usize| self.driven[m].unwrap_or_else(|| self.p.values[P_MAC0 + m]);
        let routes = || self.p.mods.iter().filter(|r| r.dst as usize == i && !r.off);
        if !routes().any(|r| mac(r.src as usize).map_or(false, |m| self.driven[m].is_some())) { return None }
        let mut n = base;
        for r in routes() {
            let Some(m) = mac(r.src as usize) else { continue };
            let x = mod_curve(at(m), r.curve);
            let g = match r.aux.and_then(|a| mac(a as usize)) { Some(a) if r.aux_inv => 1. - at(a), Some(a) => at(a), None => 1. };
            n += (if r.bi { x * 2. - 1. } else { x }) * r.amt * g;
        }
        Some(n.clamp(0., 1.) as f32)
    }
    fn knob(&mut self, ui: &mut Ui, i: usize, label: Option<String>) {
        let d = &wave_params()[i];
        let bare = label.as_deref() == Some("");
        let (rect, resp) = ui.allocate_exact_size(vec2(62., if bare { 66. } else { 78. }), Sense::click_and_drag());
        let now = ui.input(|x| x.time);
        let n = wave_norm(d, self.p.values[i]).clamp(0., 1.);
        let routes: Vec<usize> = self.p.mods.iter().enumerate().filter(|(_, m)| m.dst as usize == i).map(|(j, _)| j).collect();
        let armed = self.arm.and_then(|s| routes.iter().copied().find(|&j| self.p.mods[j].src == s));
        let mut changed = false;
        if self.pin && resp.clicked() {
            if let Some(k) = self.dial.iter().position(|p| *p == d.path) { self.dial.remove(k); self.speak(format!("{}: off the dial", long_label(&self.p, i)), now) }
            else if self.dial.len() < DIAL_MAX { self.dial.push(d.path.clone()); self.speak(format!("{}: on the dial", long_label(&self.p, i)), now) }
            else { self.speak(format!("the dial is full: {DIAL_MAX} knobs"), now) }
            self.dirty = true;
        } else if !self.pin {
            if resp.drag_started() {
                let (route, fresh) = match (self.arm, armed) {
                    (Some(_), Some(j)) => (Some(j), false),
                    (Some(s), None) => {
                        self.p.mods.push(cogs_dsp::patch::ModRoute { src: s, dst: i as u16, amt: 0., bi: (SRC_LFO0..SRC_LFO0 + 4).contains(&(s as usize)), off: false, aux: None, aux_inv: false, curve: 0. });
                        (Some(self.p.mods.len() - 1), true)
                    }
                    _ => (None, false),
                };
                let x0 = resp.interact_pointer_pos().map_or(rect.center().x, |p| p.x);
                self.drag = Some((i, Drag { n0: n, a0: route.map_or(0., |j| self.p.mods[j].amt), acc: 0., x0, route, fresh }));
            }
            if resp.dragged() {
                if let Some((di, dr)) = self.drag.as_mut().filter(|(di, _)| *di == i) {
                    let _ = di;
                    let px = resp.interact_pointer_pos().map_or(dr.x0, |p| p.x);
                    let fine = 1. + (px - dr.x0).abs() as f64 / 60.;
                    dr.acc += -resp.drag_delta().y as f64 / (150. * fine);
                    let txt;
                    if let Some(j) = dr.route {
                        let a = ((dr.a0 + dr.acc).clamp(-1., 1.) * 1000.).round() / 1000.;
                        self.p.mods[j].amt = a;
                        txt = format!("{} {}{}", src_label(self.p.mods[j].src), if a > 0. { "+" } else { "" }, (a * 100.).round());
                    } else {
                        self.p.values[i] = wave_denorm(d, dr.n0 + dr.acc);
                        txt = value_text(d, self.p.values[i]);
                    }
                    let f = if fine >= 1.5 { format!(" · fine ×{fine:.1}") } else { String::new() };
                    self.speak(format!("{}: {txt}{f}", long_label(&self.p, i)), now);
                    changed = true;
                }
            }
            if resp.drag_stopped() {
                if let Some((_, dr)) = self.drag.take().filter(|(di, _)| *di == i) {
                    if let Some(j) = dr.route { if self.p.mods[j].amt == 0. { self.p.mods.remove(j); } }
                    let _ = dr.fresh;
                    changed = true;
                }
            }
            if resp.double_clicked() {
                if let Some(j) = armed { self.p.mods.remove(j); } else { self.p.values[i] = d.default; }
                self.speak(format!("{}: reset", long_label(&self.p, i)), now);
                changed = true;
            }
        }
        if changed { self.dirty = true }
        // draw
        let p = ui.painter();
        let c = pos2(rect.center().x, rect.top() + 27.);
        let (a0, sw) = (PI * 0.75, PI * 1.5);
        let driven = self.driven_at(i);
        let base = driven.unwrap_or_else(|| wave_norm(d, self.p.values[i]).clamp(0., 1.));
        let n = base as f32;
        let pinned = self.dial.contains(&d.path);
        let ring = match self.arm { Some(s) => src_col(s), None if self.pin => FG, None => LINE };
        p.circle_filled(c, 17., WELL);
        if self.pin || self.arm.is_some() { p.circle_stroke(c, 26., Stroke::new(1., ring.gamma_multiply(0.35))); }
        if resp.is_pointer_button_down_on() { p.circle_filled(c, 17., Color32::from_gray(52)); }
        p.add(Shape::line(arc(c, 22., a0, a0 + sw), Stroke::new(4., LINE)));
        let z = if d.min < 0. && d.max > 0. { wave_norm(d, 0.) as f32 } else { 0. };
        p.add(Shape::line(arc(c, 22., a0 + sw * z.min(n), a0 + sw * z.max(n) + 0.001), Stroke::new(4., HOT)));
        let mut r = 16.;
        for m in self.p.mods.iter().filter(|m| m.dst as usize == i) {
            let (amt, nf) = (m.amt as f32, n);
            let (lo, hi) = if m.bi { (nf - amt.abs(), nf + amt.abs()) } else { ((nf).min(nf + amt), nf.max(nf + amt)) };
            let col = src_col(m.src);
            let col = if m.off { col.gamma_multiply(0.3) } else { col };
            p.add(Shape::line(arc(c, r, a0 + sw * lo.max(0.), a0 + sw * hi.min(1.) + 0.001), Stroke::new(3., col)));
            r -= 4.;
            if r < 6. { break }
        }
        if driven.is_none() {
            if let Some(x) = self.moved_to(i, base) {
                let a = a0 + sw * x.clamp(0., 1.);
                p.circle_filled(c + vec2(a.cos(), a.sin()) * 22., 3.5, SEL);
            }
        }
        let a = a0 + sw * n;
        p.line_segment([c + vec2(a.cos(), a.sin()) * 6., c + vec2(a.cos(), a.sin()) * 15.], Stroke::new(2.5, FG));
        let name = label.unwrap_or_else(|| d.label.to_string());
        p.text(pos2(rect.center().x, rect.top() + 58.), Align2::CENTER_CENTER, name, FontId::proportional(10.5), if pinned { HOT } else { DIM });
        let val = match armed { Some(j) => { let m = &self.p.mods[j]; format!("{} {}{}", src_label(m.src), if m.amt > 0. { "+" } else { "" }, (m.amt * 100.).round()) }
            None => value_text(d, driven.map_or(self.p.values[i], |x| wave_denorm(d, x))) };
        p.text(pos2(rect.center().x, rect.top() + if bare { 59. } else { 71. }), Align2::CENTER_CENTER, val, FontId::proportional(11.), match armed { Some(j) => src_col(self.p.mods[j].src), None => FG });
    }
    fn k(&mut self, ui: &mut Ui, path: &str) { if let Some(i) = param_index(path) { self.knob(ui, i, None) } }
    fn group(&mut self, ui: &mut Ui, title: &str, paths: &[String]) {
        card(ui, title, |ui| self.knobs(ui, paths));
    }
    /// Four knobs with the envelope they shape beside them (on top, where the row is too narrow).
    fn env_knobs(&mut self, ui: &mut Ui, paths: &[String], adsr: (f64, f64, f64, f64), col: Color32) {
        let (a, d, s, r) = adsr;
        if ui.available_width() >= paths.len() as f32 * 66. + 110. {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.;
                for p in paths { self.k(ui, p) }
                ui.add_space(6.);
                ui.vertical(|ui| { ui.add_space(14.); env_graph(ui, a, d, s, r, col) });
            });
        } else { env_graph(ui, a, d, s, r, col); self.knobs(ui, paths) }
    }
    fn knobs(&mut self, ui: &mut Ui, paths: &[String]) {
        ui.horizontal_wrapped(|ui| { ui.spacing_mut().item_spacing = vec2(4., 4.); for p in paths { self.k(ui, p) } });
    }
    fn combo(ui: &mut Ui, id: &str, label: &str, opts: &[&str], cur: &mut u8) -> bool {
        let mut ch = false;
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(label).size(11.).color(DIM));
            egui::ComboBox::from_id_salt(id).selected_text(opts.get(*cur as usize).copied().unwrap_or("?")).height(320.).show_ui(ui, |ui| {
                for (k, o) in opts.iter().enumerate() { if ui.selectable_value(cur, k as u8, *o).changed() { ch = true } }
            });
        });
        ch
    }
    fn toggle(ui: &mut Ui, label: &str, v: &mut bool) -> bool {
        if ui.add(egui::Button::new(label).selected(*v).min_size(vec2(0., 32.))).clicked() { *v = !*v; true } else { false }
    }
    /// An on/off switch with its label, for a module or an oscillator.
    fn switch(ui: &mut Ui, label: &str, v: &mut bool) -> bool {
        let font = FontId::proportional(14.);
        let tw = ui.painter().layout_no_wrap(label.to_string(), font.clone(), FG).size().x;
        let (rect, r) = ui.allocate_exact_size(vec2(44. + tw, 32.), Sense::click());
        let tr = Rect::from_min_size(pos2(rect.left(), rect.center().y - 10.), vec2(36., 20.));
        let p = ui.painter();
        p.rect_filled(tr, 10., if *v { HOT } else { WELL });
        p.circle_filled(pos2(if *v { tr.right() - 10. } else { tr.left() + 10. }, tr.center().y), 7., if *v { Color32::BLACK } else { DIM });
        p.text(pos2(tr.right() + 8., rect.center().y), Align2::LEFT_CENTER, label, font, if *v { FG } else { DIM });
        if r.clicked() { *v = !*v; true } else { false }
    }

    // ---------- tabs ----------
    fn tab_main(&mut self, ui: &mut Ui) {
        card(ui, "macros", |ui| {
            let per = (((ui.available_width() + 4.) / 76.).floor() as usize).clamp(1, WAVE_MACROS);
            let per = if per >= 8 { 8 } else if per >= 4 { 4 } else { per };
            for row in (0..WAVE_MACROS).collect::<Vec<_>>().chunks(per) { ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.;
                for &m in row {
                    ui.vertical(|ui| {
                        ui.set_width(72.);
                        ui.horizontal(|ui| { ui.add_space(5.); self.knob(ui, P_MAC0 + m, Some(String::new())) });
                        let mut s = self.p.macro_names[m].clone();
                        if ui.add(egui::TextEdit::singleline(&mut s).desired_width(68.).char_limit(16).font(FontId::proportional(11.)).align(egui::Align2::CENTER_CENTER)).changed() {
                            self.p.macro_names[m] = if s.is_empty() { format!("macro {}", m + 1) } else { s }; self.dirty = true;
                        }
                    });
                }
            }); }
        });
        let pinned = !self.dial.is_empty();
        let list: Vec<String> = if pinned { self.dial.clone() } else { QUICK.iter().map(|s| s.to_string()).collect() };
        card(ui, if pinned { "on the dial" } else { "quick" }, |ui| {
            ui.horizontal_wrapped(|ui| { ui.spacing_mut().item_spacing = vec2(4., 4.); for p in &list { if let Some(i) = param_index(p) { let l = long_label(&self.p, i); self.knob(ui, i, Some(l)) } } });
        });
    }
    fn tab_osc(&mut self, ui: &mut Ui, i: usize) {
        let t = ["A", "B", "C"][i];
        let on = self.p.osc[i].on;
        let o = |k: &str| format!("osc{}.{k}", i + 1);
        let g = |ks: &[&str]| ks.iter().map(|k| o(k)).collect::<Vec<_>>();
        card(ui, "", |ui| {
            ui.horizontal(|ui| {
                let mut on = self.p.osc[i].on;
                if Self::switch(ui, &format!("Oscillator {t}"), &mut on) { self.p.osc[i].on = on; self.dirty = true }
            });
            if !on { ui.multiply_opacity(0.45) }
            ui.horizontal(|ui| {
                let o = &mut self.p.osc[i];
                if Self::combo(ui, &format!("tab{i}"), "table", &WAVE_TABLES, &mut o.table) { self.dirty = true }
                let o = &mut self.p.osc[i];
                if Self::combo(ui, &format!("flt{i}"), "filter", &WAVE_FILTERS, &mut o.filter) { self.dirty = true }
            });
            self.preview(ui, i);
            if self.p.osc[i].table == 1 { self.harmonics(ui, i) }
            self.knobs(ui, &g(&["pos", "phase", "level", "pan"]));
        });
        if !on { ui.multiply_opacity(0.45) }
        card(ui, "warp", |ui| {
            ui.horizontal(|ui| {
                let q = &mut self.p.osc[i];
                if Self::combo(ui, &format!("w1{i}"), "warp 1", &WARP_MODES, &mut q.warp_mode) { self.dirty = true }
                let q = &mut self.p.osc[i];
                if Self::combo(ui, &format!("w2{i}"), "warp 2", &WARP_MODES, &mut q.warp2_mode) { self.dirty = true }
            });
            self.knobs(ui, &g(&["warp", "warp2"]));
        });
        card(ui, "pitch · unison", |ui| self.knobs(ui, &g(&["oct", "semi", "fine", "uni", "detune", "blend", "spread"])));
        let vals = self.p.values;
        let v = |k: usize| vals[i * OSC_N + k];
        let (a, d, s, r) = (v(O_A), v(O_D), v(O_S), v(O_R));
        card(ui, "amp envelope", |ui| self.env_knobs(ui, &g(&["a", "d", "s", "r"]), (a, d, s, r), HOT));
        card(ui, "filter", |ui| self.knobs(ui, &g(&["cut", "res", "drive", "key"])));
        let (a, d, s, r) = (v(O_FA), v(O_FD), v(O_FS), v(O_FR));
        card(ui, "filter envelope", |ui| { env_graph(ui, a, d, s, r, SEL); self.knobs(ui, &g(&["famt", "fa", "fd", "fs", "fr"])) });
    }
    /// The table's current frame, drawn from its spectrum, so the position knob shows as a shape.
    fn preview(&mut self, ui: &mut Ui, i: usize) {
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width().min(240.), 54.), Sense::hover());
        let o = &self.p.osc[i];
        let sp = wave_spectra(WAVE_TABLES[o.table as usize], &o.harm);
        let fp = self.p.values[i * OSC_N + O_POS] * (sp.len() - 1) as f64;
        let f0 = fp.floor() as usize; let f1 = (f0 + 1).min(sp.len() - 1); let ff = fp - f0 as f64;
        let mut ys = Vec::with_capacity(97); let mut peak = 1e-9f64;
        for s in 0..=96 {
            let t = s as f64 / 96.;
            let mut y = 0.;
            for k in 1..=64usize.min(sp[f0].len()) { let a = sp[f0][k - 1] * (1. - ff) + sp[f1][k - 1] * ff; if a != 0. { y += a * (std::f64::consts::TAU * k as f64 * t).sin() } }
            peak = peak.max(y.abs()); ys.push(y);
        }
        let p = ui.painter();
        p.line_segment([pos2(rect.left(), rect.center().y), pos2(rect.right(), rect.center().y)], Stroke::new(1., LINE));
        let pts: Vec<Pos2> = ys.iter().enumerate().map(|(s, y)| pos2(rect.left() + s as f32 / 96. * rect.width(), rect.center().y - (*y / peak) as f32 * (rect.height() / 2. - 4.))).collect();
        p.add(Shape::line(pts, Stroke::new(1.5, if o.on { HOT } else { DIM })));
    }
    /// The additive table's own harmonics: drag across the bars to draw levels.
    fn harmonics(&mut self, ui: &mut Ui, i: usize) {
        let w = ui.available_width().min(340.);
        let (rect, resp) = ui.allocate_exact_size(vec2(w, 76.), Sense::click_and_drag());
        let bw = w / WAVE_HARMS as f32;
        if let Some(pp) = resp.interact_pointer_pos().filter(|_| resp.is_pointer_button_down_on() || resp.clicked()) {
            let k = ((pp.x - rect.left()) / bw).floor() as i32;
            if (0..WAVE_HARMS as i32).contains(&k) {
                let v = ((1. - (pp.y - rect.top()) / rect.height()).clamp(0., 1.) * 100.).round() as f64 / 100.;
                self.p.osc[i].harm[k as usize] = v; self.dirty = true;
                let now = ui.input(|x| x.time);
                self.speak(format!("harmonic {}: {}%", k + 1, (v * 100.).round()), now);
            }
        }
        let p = ui.painter();
        for k in 0..WAVE_HARMS {
            let x = rect.left() + k as f32 * bw;
            p.rect_filled(Rect::from_min_max(pos2(x + 1., rect.top()), pos2(x + bw - 1., rect.bottom())), 0., Color32::from_gray(34));
            let h = self.p.osc[i].harm[k] as f32 * (rect.height() - 4.);
            p.rect_filled(Rect::from_min_max(pos2(x + 1., rect.bottom() - h), pos2(x + bw - 1., rect.bottom())), 0., HOT);
        }
    }
    fn tab_voice(&mut self, ui: &mut Ui) {
        card(ui, "voice", |ui| {
            ui.horizontal_wrapped(|ui| {
                if Self::combo(ui, "voicing", "voicing", &VOICE_MODES, &mut self.p.voice_mode) { self.dirty = true }
                ui.vertical(|ui| { ui.label(egui::RichText::new("glide").size(11.).color(DIM)); ui.horizontal(|ui| {
                    if Self::toggle(ui, "always", &mut self.p.glide_always) { self.dirty = true }
                    if Self::toggle(ui, "scaled", &mut self.p.glide_scale) { self.dirty = true }
                }); });
            });
            self.knobs(ui, &["tune.oct", "tune.semi", "vol", "glide", "gate"].map(String::from));
        });
        card(ui, "sub · noise (on ENV1)", |ui| self.knobs(ui, &["sub.level", "sub.oct", "noise.level", "noise.color"].map(String::from)));
    }
    fn tab_mod(&mut self, ui: &mut Ui) {
        for e in 1..=WAVE_ENVS {
            let t = if e == 1 { "ENV1 · also sub and noise".to_string() } else { format!("ENV{e}") };
            let base = P_ENV0 + 4 * (e - 1);
            let (a, d, s, r) = (self.p.values[base], self.p.values[base + 1], self.p.values[base + 2], self.p.values[base + 3]);
            card(ui, &t, |ui| self.env_knobs(ui, &["a", "d", "s", "r"].map(|k| format!("env{e}.{k}")), (a, d, s, r), ENVC));
        }
        let syncs: Vec<&str> = LFO_SYNCS.iter().map(|x| x.0).collect();
        for l in 0..WAVE_LFOS {
            card(ui, &format!("LFO{}", l + 1), |ui| {
                ui.horizontal_wrapped(|ui| {
                    if Self::combo(ui, &format!("ls{l}"), "shape", &LFO_SHAPES, &mut self.p.lfo[l].shape) { self.dirty = true }
                    if Self::combo(ui, &format!("ly{l}"), "sync", &syncs, &mut self.p.lfo[l].sync) { self.dirty = true }
                    ui.vertical(|ui| { ui.label(egui::RichText::new(" ").size(11.)); if Self::toggle(ui, "retrig", &mut self.p.lfo[l].retrig) { self.dirty = true } });
                });
                self.knobs(ui, &[format!("lfo{}.rate", l + 1), format!("lfo{}.phase", l + 1)]);
            });
        }
    }
    fn tab_fx(&mut self, ui: &mut Ui) {
        const KNOBS: [&[&str]; 9] = [&["voices", "rate", "detune", "dim", "mix"], &["drive", "mix"], &["rate", "depth", "fb", "spread", "mix"],
            &["rate", "depth", "freq", "fb", "spread", "mix"], &["rate", "depth", "mix"], &["time", "fb", "tone", "mix"],
            &["mix", "predelay", "decay", "size", "attack", "early", "late", "mod", "rate", "lowcut", "highcut"], &["thresh", "ratio", "gain"], &["low", "high"]];
        const NAMES: [&str; 9] = ["Hyper / dimension", "Distortion", "Flanger", "Phaser", "Chorus", "Delay", "Reverb", "Compressor", "EQ"];
        ui.label(egui::RichText::new("The rack runs top to bottom. Its knobs take macros from the matrix.").size(11.).color(DIM));
        let order = self.p.fx_order;
        let mut mv: Option<(usize, i32)> = None;
        for (pos, &k) in order.iter().enumerate() {
            let k = k as usize;
            card(ui, "", |ui| {
                ui.horizontal(|ui| {
                    let mut on = self.p.fx_on[k];
                    if Self::switch(ui, NAMES[k], &mut on) { self.p.fx_on[k] = on; self.dirty = true }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ibtn(ui, Ico::Down, vec2(34., 30.), pos + 1 < order.len(), false).clicked() { mv = Some((pos, 1)) }
                        if ibtn(ui, Ico::Up, vec2(34., 30.), pos > 0, false).clicked() { mv = Some((pos, -1)) }
                    });
                });
                if !self.p.fx_on[k] { return }
                ui.horizontal_wrapped(|ui| match k {
                    FX_DIST => { if Self::combo(ui, "dmode", "mode", &DIST_MODES, &mut self.p.dist_mode) { self.dirty = true } }
                    FX_REVERB => {
                        if Self::combo(ui, "vmode", "mode", &VERB_MODES, &mut self.p.verb_mode) { self.dirty = true }
                        if Self::combo(ui, "vcolor", "color", &VERB_COLORS, &mut self.p.verb_color) { self.dirty = true }
                    }
                    FX_DELAY => {
                        let s: Vec<&str> = DELAY_SYNCS.iter().map(|x| x.0).collect();
                        if Self::combo(ui, "dsync", "sync", &s, &mut self.p.delay_sync) { self.dirty = true }
                        ui.vertical(|ui| { ui.label(egui::RichText::new(" ").size(11.)); if Self::toggle(ui, "ping-pong", &mut self.p.delay_ping) { self.dirty = true } });
                    }
                    _ => {}
                });
                let paths: Vec<String> = KNOBS[k].iter().map(|q| format!("{}.{q}", WAVE_FX[k])).collect();
                self.knobs(ui, &paths);
            });
        }
        if let Some((pos, d)) = mv {
            let j = (pos as i32 + d) as usize;
            self.p.fx_order.swap(pos, j); self.dirty = true; self.redraw_fx = true;
        }
    }
    fn tab_matrix(&mut self, ui: &mut Ui) {
        let mut del = None;
        let n = self.p.mods.len();
        if n == 0 { ui.label(egui::RichText::new("No routes yet. Arm a source on another tab and drag a knob, or add one here.").size(12.).color(DIM)); }
        for j in 0..n {
            ui.push_id(("route", j), |ui| card(ui, &format!("route {}", j + 1), |ui| {
                ui.horizontal(|ui| {
                    let m = &mut self.p.mods[j];
                    let srcs: Vec<String> = (0..NS as u8).map(src_label).collect();
                    let sr: Vec<&str> = srcs.iter().map(|s| s.as_str()).collect();
                    if Self::combo(ui, "src", "source", &sr, &mut m.src) { self.dirty = true }
                    let cur = self.p.mods[j].dst as usize;
                    let mut pick = None;
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("destination").size(11.).color(DIM));
                        egui::ComboBox::from_id_salt("dst").selected_text(long_label(&self.p, cur)).height(360.).show_ui(ui, |ui| {
                            for i in 0..NP { if ui.selectable_label(i == cur, long_label(&self.p, i)).clicked() { pick = Some(i) } }
                        });
                    });
                    if let Some(i) = pick { self.p.mods[j].dst = i as u16; self.dirty = true }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                        if ibtn(ui, Ico::Del, vec2(32., 32.), true, false).clicked() { del = Some(j) }
                    });
                });
                ui.label(egui::RichText::new("amount").size(11.).color(DIM));
                let m = &mut self.p.mods[j];
                ui.horizontal(|ui| {
                    ui.spacing_mut().slider_width = (ui.available_width() - 66.).max(120.);
                    let s = egui::Slider::new(&mut m.amt, -1.0..=1.0).step_by(0.01)
                        .custom_formatter(|x, _| format!("{:+}%", (x * 100.).round()))
                        .custom_parser(|t| t.trim().trim_end_matches('%').parse::<f64>().ok().map(|v| v / 100.));
                    if ui.add(s).changed() { self.dirty = true }
                });
                let open = self.open_routes.contains(&j);
                if ui.add(egui::Button::new(if open { "fewer options" } else { "more options" }).min_size(vec2(0., 32.))).clicked() {
                    if open { self.open_routes.remove(&j); } else { self.open_routes.insert(j); }
                }
                if self.open_routes.contains(&j) {
                    ui.horizontal_wrapped(|ui| {
                        let m = &mut self.p.mods[j];
                        if Self::toggle(ui, "bipolar", &mut m.bi) { self.dirty = true }
                        if Self::toggle(ui, "bypass", &mut m.off) { self.dirty = true }
                        let mut aux = m.aux.map_or(0u8, |a| a + 1);
                        let mut names = vec!["none".to_string()]; names.extend((0..NS as u8).map(src_label));
                        let nr: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
                        if Self::combo(ui, "aux", "aux", &nr, &mut aux) { m.aux = if aux == 0 { None } else { Some(aux - 1) }; self.dirty = true }
                        if Self::toggle(ui, "aux inv", &mut m.aux_inv) { self.dirty = true }
                        if Self::curve(ui, &mut m.curve) { self.dirty = true }
                    });
                }
            }));
        }
        if let Some(j) = del { self.p.mods.remove(j); self.open_routes.clear(); self.dirty = true }
        if ui.add(egui::Button::new("+ route").min_size(vec2(90., 36.))).clicked() {
            self.p.mods.push(cogs_dsp::patch::ModRoute { src: SRC_LFO0 as u8, dst: (O_CUT) as u16, amt: 0.25, bi: true, off: false, aux: None, aux_inv: false, curve: 0. });
            self.dirty = true;
        }
    }
    /// A route's curve: drag up or down on it; double tap straightens it.
    fn curve(ui: &mut Ui, c: &mut f64) -> bool {
        let mut ch = false;
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("curve").size(11.).color(DIM));
            let (rect, resp) = ui.allocate_exact_size(vec2(56., 36.), Sense::click_and_drag());
            if resp.dragged() { *c = ((*c - resp.drag_delta().y as f64 / 80.).clamp(-1., 1.) * 100.).round() / 100.; ch = true }
            if resp.double_clicked() { *c = 0.; ch = true }
            let p = ui.painter();
            p.rect_stroke(rect, 3., Stroke::new(1., LINE), egui::StrokeKind::Inside);
            let pts: Vec<Pos2> = (0..=24).map(|k| { let u = k as f64 / 24.; let y = mod_curve(u, *c); pos2(rect.left() + u as f32 * rect.width(), rect.bottom() - 2. - y as f32 * (rect.height() - 4.)) }).collect();
            p.add(Shape::line(pts, Stroke::new(1.5, SEL)));
        });
        ch
    }
    fn tab_patch(&mut self, ui: &mut Ui, out: &mut Vec<EditorOut>) {
        card(ui, "this patch", |ui| {
            ui.horizontal(|ui| {
                let mut name = self.p.name.clone();
                if ui.add(egui::TextEdit::singleline(&mut name).desired_width((ui.available_width() - 130.).max(120.)).char_limit(40).hint_text("patch name")).changed() { self.p.name = name; self.dirty = true }
                if ui.add(egui::Button::new("save to library").min_size(vec2(0., 34.))).clicked() {
                    self.msg = match self.save_lib() { Ok(n) => format!("saved {n}"), Err(e) => format!("not saved: {e}") };
                }
            });
            if !self.msg.is_empty() { ui.label(egui::RichText::new(&self.msg).size(12.).color(SEL)); }
        });
        card(ui, "factory", |ui| {
            ui.horizontal_wrapped(|ui| {
                for n in WAVE_PRESET_NAMES {
                    if ui.add(egui::Button::new(n).min_size(vec2(0., 34.))).clicked() {
                        if let Some(p) = WavePatch::preset(n) { self.p = p; self.dirty = true; self.msg = format!("loaded {n}"); out.push(EditorOut::Audition(self.part.id())) }
                    }
                }
            });
        });
        let names = self.lib_names();
        if !names.is_empty() {
            card(ui, "library", |ui| {
                let mut del = None;
                for n in &names {
                    ui.horizontal(|ui| {
                        if ui.add(egui::Button::new(n.as_str()).min_size(vec2(160., 34.))).clicked() {
                            if let Some(p) = self.lib_load(n) { self.p = p; self.dirty = true; self.msg = format!("loaded {n}"); out.push(EditorOut::Audition(self.part.id())) }
                        }
                        if ibtn(ui, Ico::Del, vec2(34., 34.), true, false).clicked() { del = Some(n.clone()) }
                    });
                }
                if let Some(n) = del { if let Some(d) = &self.lib { let _ = std::fs::remove_file(d.join(format!("{n}.json"))); self.msg = format!("deleted {n}") } }
            });
        }
        egui::CollapsingHeader::new(egui::RichText::new("patch as text").size(12.).color(DIM)).id_salt("patch-json").show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new("copy").min_size(vec2(0., 34.))).clicked() {
                    self.json = serde_json::to_string_pretty(&self.p.to_json()).unwrap_or_default();
                    ui.ctx().copy_text(self.json.clone()); self.msg = "copied to the clipboard".into();
                }
                if ui.add(egui::Button::new("import text").min_size(vec2(0., 34.))).clicked() {
                    match serde_json::from_str::<Value>(&self.json) {
                        Ok(j) if j.is_object() => { self.p = WavePatch::from_json(&j); self.dirty = true; self.msg = format!("imported {}", self.p.name) }
                        Ok(_) => self.msg = "import failed: not a patch object".into(),
                        Err(e) => self.msg = format!("import failed: {e}"),
                    }
                }
            });
            ui.add(egui::TextEdit::multiline(&mut self.json).hint_text("paste a patch JSON here").desired_rows(5).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
        });
    }

    // ---------- the library: one JSON file per patch ----------
    fn lib_names(&self) -> Vec<String> {
        let Some(d) = &self.lib else { return Vec::new() };
        let mut v: Vec<String> = std::fs::read_dir(d).map(|it| it.filter_map(|e| e.ok()).filter_map(|e| { let p = e.path(); if p.extension()? != "json" { return None } Some(p.file_stem()?.to_string_lossy().into_owned()) }).collect()).unwrap_or_default();
        v.sort_by_key(|s| s.to_lowercase());
        v
    }
    fn lib_load(&self, n: &str) -> Option<WavePatch> { let d = self.lib.as_ref()?; WavePatch::from_str(&std::fs::read_to_string(d.join(format!("{n}.json"))).ok()?).ok() }
    fn save_lib(&self) -> Result<String, String> {
        let d = self.lib.as_ref().ok_or("no storage here")?;
        let n = sanitize(if self.p.name.is_empty() { "patch" } else { &self.p.name });
        let n = if n.is_empty() { "patch".to_string() } else { n };
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        std::fs::write(d.join(format!("{n}.json")), serde_json::to_string_pretty(&self.p.to_json()).unwrap_or_default()).map_err(|e| e.to_string())?;
        Ok(n)
    }
    /// ◀ ▶: step through the factory patches, then the library.
    fn browse(&mut self, d: i32) {
        let mut all: Vec<(bool, String)> = WAVE_PRESET_NAMES.iter().map(|n| (true, n.to_string())).collect();
        all.extend(self.lib_names().into_iter().map(|n| (false, n)));
        let len = all.len() as i32;
        let i = all.iter().position(|(_, n)| *n == self.p.name).map(|i| i as i32).unwrap_or(if d > 0 { -1 } else { 0 });
        let j = ((i + d) % len + len) % len;
        let (fac, n) = &all[j as usize];
        let p = if *fac { WavePatch::preset(n) } else { self.lib_load(n) };
        if let Some(mut p) = p { if p.name.is_empty() { p.name = n.clone() } self.p = p; self.dirty = true }
    }
}
