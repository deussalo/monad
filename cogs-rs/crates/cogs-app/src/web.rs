//! The web build's screen: the same stage as on Android, in a canvas (eframe's web runner). Its sound runs in an
//! AudioWorklet (cogs-worklet, driven by web/cogs-web.js); this side keeps its own copy of the machine for drawing and
//! editing, and keeps the worklet's in step (see link.rs). The page's glue is `window.cogs`.
use crate::link;
use crate::stage::{Sel, Stage, StageIo, StageOut};
use cogs_sim::World;
use eframe::egui;
use serde_json::Value;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[wasm_bindgen]
extern "C" {
    /// Post a message to the worklet (dropped until the sound has started).
    #[wasm_bindgen(js_namespace = cogs, js_name = send)]
    fn js_send(s: &str);
    /// The worklet's next state message, if one came.
    #[wasm_bindgen(js_namespace = cogs, js_name = poll)]
    fn js_poll() -> Option<String>;
    /// The sim time coming out of the speaker now, or < 0 while there is no sound.
    #[wasm_bindgen(js_namespace = cogs, js_name = heard)]
    fn js_heard() -> f64;
    /// The sound runs and the worklet is listening.
    #[wasm_bindgen(js_namespace = cogs, js_name = ready)]
    fn js_ready() -> bool;
    #[wasm_bindgen(js_namespace = cogs, js_name = connectMidi)]
    fn js_connect_midi();
    /// One line about the sound and the keyboards, for the panel.
    #[wasm_bindgen(js_namespace = cogs, js_name = status)]
    fn js_status() -> String;
    #[wasm_bindgen(js_namespace = cogs, js_name = buzz)]
    fn js_buzz(ms: u32);
}

/// Start the app in the canvas with this id.
#[wasm_bindgen]
pub fn start(canvas_id: String) {
    wasm_bindgen_futures::spawn_local(async move {
        let Some(canvas) = web_sys::window().and_then(|w| w.document()).and_then(|d| d.get_element_by_id(&canvas_id))
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok()) else { return };
        let r = eframe::WebRunner::new().start(canvas, eframe::WebOptions::default(), Box::new(|_cc| Ok(Box::new(WebApp::new())))).await;
        if let Err(e) = r { web_sys::console::error_1(&e) }
    });
}

pub struct WebApp {
    world: World,
    stage: Stage,
    panel: bool,
    world_ix: usize,
    fitted: bool,
    note: Option<(String, f64)>,
    /// the world as last sent to the worklet, and when it was last checked
    sent: Value,
    checked: f64,
    ready: bool,
    wall: Option<f64>,
    state: Value,
    balls: Option<Value>,
    part: u32,
    listen: String,
    listen_at: f64,
}

impl WebApp {
    pub fn new() -> Self {
        let world = World::from_str(crate::WORLDS[0].1).unwrap_or_else(|_| World::demo());
        WebApp { world, stage: Stage::new(), panel: false, world_ix: 0, fitted: false, note: None, sent: Value::Null, checked: f64::NEG_INFINITY,
            ready: false, wall: None, state: Value::Null, balls: None, part: u32::MAX, listen: String::new(), listen_at: f64::NEG_INFINITY }
    }
    fn keyboard_part(w: &World, sel: Option<Sel>) -> Option<u32> {
        match sel? {
            Sel::Reader(i) | Sel::Pad(i) | Sel::Pit(i) => Some(i),
            Sel::Cog(c) | Sel::Dot { cog: c, .. } => w.readers.iter().find(|r| r.on == Some(c)).map(|r| r.id),
            _ => None,
        }
    }
    fn load(&mut self, i: usize) {
        let Ok(mut nw) = World::from_str(crate::WORLDS[i].1) else { return };
        nw.set_paused(self.world.clock.paused);
        nw.clock.sim_t = self.world.clock.sim_t;
        nw.clock.view = self.world.clock.view;
        self.world = nw;
        self.stage.loaded(&self.world);
        self.world_ix = i; self.fitted = false; self.part = u32::MAX; self.sent = Value::Null;
    }
    /// The machine's own clock: to the heard time while there is sound, else to the wall clock.
    fn advance(&mut self, now: f64) {
        let dt = self.wall.map_or(0., |w| (now - w).clamp(0., 0.25));
        self.wall = Some(now);
        while let Some(s) = js_poll() {
            if let Ok(v) = serde_json::from_str::<Value>(&s) {
                if v.get("balls").is_some() { self.balls = Some(v.clone()) }
                self.state = v;
            }
        }
        let ready = js_ready();
        if ready && !self.ready { self.sent = Value::Null; self.checked = f64::NEG_INFINITY; }
        self.ready = ready;
        if self.world.clock.paused { return }
        let h = js_heard();
        let target = if ready && h >= 0. { h } else { self.world.clock.sim_t + dt };
        if target > self.world.clock.sim_t { self.world.advance(target, &mut |_| {}); }
        if let Some(b) = &self.balls { link::apply_balls(&mut self.world, b) }
    }
    /// Keep the worklet's machine in step with this one, and tell it the camera and what the keyboard plays.
    fn sync(&mut self, now: f64, force: bool, size: egui::Vec2) {
        if !self.ready { return }
        if force || now - self.checked > 0.15 {
            self.checked = now;
            let sig = link::world_sig(&self.world);
            if sig != self.sent { js_send(&link::world_message(&self.world)); self.sent = sig; }
        }
        let part = Self::keyboard_part(&self.world, self.stage.sel).unwrap_or(0);
        if part != self.part { self.part = part; js_send(&format!(r#"{{"type":"part","id":{part}}}"#)); }
        if now - self.listen_at > 0.1 {
            self.listen_at = now;
            let (_, _, _, fit) = self.stage.fit_target(&self.world);
            let k = ((fit / self.world.s.zoom).ln() / 4f64.ln()).clamp(0., 1.);
            let e = k * k * (3. - 2. * k);
            let l = format!(r#"{{"type":"listen","vx":{:.1},"vy":{:.1},"zoom":{:.4},"w":{:.0},"h":{:.0},"e":{:.3}}}"#, self.world.view.x, self.world.view.y, self.world.s.zoom, size.x, size.y, e);
            if l != self.listen { js_send(&l); self.listen = l; }
        }
    }
}

impl Default for WebApp { fn default() -> Self { Self::new() } }

impl eframe::App for WebApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        self.advance(now);
        let status = js_status();
        let connected = status.contains("keyboard:");
        let io = StageIo { heard: self.world.clock.sim_t, safe_top: 8.0, safe_bottom: 8.0, busy: self.panel, recording: false, can_record: false,
            midi_label: Some(if connected { "keyboard" } else { "MIDI" }), midi_connected: connected };
        let mut outs = Vec::new();
        let mut size = egui::Vec2::ZERO;
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            size = ui.max_rect().size();
            if !self.fitted { self.stage.fit_now(&mut self.world, size, &io); self.fitted = true }
            outs = self.stage.show(ui, &mut self.world, &io);
        });
        let mut force = false;
        for o in outs {
            match o {
                StageOut::ConnectMidi => js_connect_midi(),
                StageOut::TogglePanel => self.panel = !self.panel,
                StageOut::Buzz(ms) => js_buzz(ms),
                StageOut::Changed | StageOut::Paused { .. } => force = true,
                StageOut::Audition { part, midi, key } => js_send(&format!(r#"{{"type":"audition","part":{},"midi":{},"key":{}}}"#, part.map(|p| p.id()).unwrap_or(0), midi, key)),
                StageOut::OpenEditor(kind) => self.note = Some((format!("The {kind} editor is being ported next."), now)),
                StageOut::Record => self.note = Some(("Recording onto plates comes with the notes editor.".into(), now)),
            }
        }
        self.sync(now, force, size);
        if !self.ready {
            egui::Area::new(egui::Id::unique("tap")).anchor(egui::Align2::CENTER_TOP, [0.0, 12.0]).show(&ctx, |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| ui.label("tap anywhere for sound"));
            });
        }
        if let Some((t, at)) = &self.note {
            if now - at > 2.5 { self.note = None } else {
                egui::Area::new(egui::Id::unique("note")).anchor(egui::Align2::CENTER_TOP, [0.0, 40.0]).show(&ctx, |ui| { egui::Frame::popup(ui.style()).show(ui, |ui| ui.label(t.as_str())); });
            }
        }
        if self.panel { self.panel_ui(&ctx, &status) }
    }
}

impl WebApp {
    fn panel_ui(&mut self, ctx: &egui::Context, status: &str) {
        let mut open = true;
        let st = self.state.clone();
        egui::Window::new("cogs").open(&mut open).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_BOTTOM, [0.0, -12.0]).show(ctx, |ui| {
            ui.label(status);
            if ui.add_sized([200.0, 44.0], egui::Button::new("Connect keyboard")).clicked() { js_connect_midi() }
            ui.separator();
            let mut load = None;
            ui.horizontal_wrapped(|ui| {
                for (i, (name, _)) in crate::WORLDS.iter().enumerate() {
                    if ui.add_sized([100.0, 40.0], egui::Button::new(*name).selected(i == self.world_ix)).clicked() { load = Some(i) }
                }
            });
            if let Some(i) = load { self.load(i) }
            ui.separator();
            let n = |k: &str| st.get(k).and_then(|v| v.as_f64()).unwrap_or(0.);
            ui.label(format!("keys {} · machine notes {} · voices {} · peak {:.2}", n("notes"), n("events"), n("voices"), n("peak")));
        });
        if !open { self.panel = false }
    }
}
