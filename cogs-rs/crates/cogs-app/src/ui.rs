//! A first screen while the stage is ported: pick a world and play it, choose what the keyboard plays, and see what
//! the audio is doing. Touch keys play the same part, to compare touch with the keyboard.
use crate::play::Shared;
use cogs_sim::{VoiceKind, World};
use eframe::egui;
use std::sync::{atomic::Ordering::Relaxed, Arc, Mutex};

pub struct App {
    pub sh: Arc<Shared>,
    pub status: Arc<Mutex<String>>,
    pub connect: Box<dyn Fn()>,
    held: Option<u8>,
    world_ix: usize,
}

impl App {
    pub fn new(sh: Arc<Shared>, status: Arc<Mutex<String>>, connect: Box<dyn Fn()>) -> Self { App { sh, status, connect, held: None, world_ix: 0 } }
}

fn kind_name(k: VoiceKind) -> &'static str { match k { VoiceKind::Wave => "synth", VoiceKind::Synth => "modal", _ => "drum" } }

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.set_pixels_per_point(ctx.pixels_per_point().max(2.0));
        let m = &self.sh.meters;
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(36.0);
            ui.heading("cogs · Rust");
            ui.label(self.status.lock().map(|s| s.clone()).unwrap_or_default());
            if ui.add_sized([220.0, 52.0], egui::Button::new("Connect keyboard")).clicked() { (self.connect)() }
            ui.separator();
            // the world
            ui.horizontal_wrapped(|ui| {
                for (i, (name, src)) in crate::WORLDS.iter().enumerate() {
                    if ui.add_sized([110.0, 44.0], egui::Button::new(*name).selected(i == self.world_ix)).clicked() {
                        if let Ok(nw) = World::from_str(src) { if let Ok(mut w) = self.sh.world.lock() { let paused = w.clock.paused; *w = nw; w.set_paused(paused); } self.world_ix = i; self.sh.midi_part.store(0, Relaxed) }
                    }
                }
            });
            let paused = self.sh.world.lock().map(|w| w.clock.paused).unwrap_or(true);
            ui.horizontal(|ui| {
                if ui.add_sized([110.0, 48.0], egui::Button::new(if paused { "▶ play" } else { "⏸ pause" })).clicked() {
                    if let Ok(mut w) = self.sh.world.lock() { w.set_paused(!paused); }
                }
                if ui.add_sized([110.0, 48.0], egui::Button::new("⏮ start")).clicked() { if let Ok(mut w) = self.sh.world.lock() { w.to_start(); } }
            });
            ui.separator();
            // what the keyboard plays
            ui.label("the keyboard plays:");
            let parts: Vec<(u32, String)> = self.sh.world.lock().map(|w| {
                let mut v: Vec<(u32, String)> = Vec::new();
                for r in &w.readers { v.push((r.id, format!("reader {} · {}", r.id, kind_name(r.voice.kind)))) }
                for p in &w.pads { v.push((p.id, format!("pad {} · {}", p.id, kind_name(p.voice.kind)))) }
                for p in &w.pits { v.push((p.id, format!("box {} · {}", p.id, kind_name(p.voice.kind)))) }
                v
            }).unwrap_or_default();
            let cur = self.sh.midi_part.load(Relaxed);
            ui.horizontal_wrapped(|ui| {
                for (id, label) in parts {
                    if ui.add_sized([150.0, 40.0], egui::Button::new(label).selected(cur == id)).clicked() { self.sh.midi_part.store(id, Relaxed) }
                }
            });
            ui.separator();
            let sr = m.sample_rate.load(Relaxed).max(1) as f32;
            ui.label(format!("audio: {} Hz · block {} · burst {} · buffer {} ({:.1} ms)", sr as u32, m.block.load(Relaxed), m.burst.load(Relaxed),
                m.buffer.load(Relaxed), m.buffer.load(Relaxed) as f32 / sr * 1000.0));
            ui.label(format!("measured output latency: {:.1} ms · glitches: {} · audio busy {}%", m.latency_us.load(Relaxed) as f32 / 1000.0, m.xruns.load(Relaxed), m.busy.load(Relaxed)));
            ui.label(format!("keys played: {} · last {} · machine notes: {}", m.notes.load(Relaxed), m.last_note.load(Relaxed), m.events.load(Relaxed)));
            ui.add_space(16.0);
            ui.label("touch keys (to compare with the keyboard):");
            let keys = [60u8, 62, 64, 65, 67, 69, 71, 72];
            let w = (ui.available_width() / keys.len() as f32 - 4.0).max(30.0);
            let mut down = None;
            ui.horizontal(|ui| {
                for k in keys {
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 150.0), egui::Sense::click_and_drag());
                    let on = resp.is_pointer_button_down_on();
                    ui.painter().rect_filled(rect, 6.0, if on { egui::Color32::from_rgb(230, 180, 60) } else { egui::Color32::from_gray(200) });
                    if on { down = Some(k) }
                }
            });
            if down != self.held {
                if let Some(k) = self.held { self.sh.midi(crate::midi::Midi::Off { note: k }) }
                if let Some(k) = down { self.sh.midi(crate::midi::Midi::On { note: k, vel: 100 }) }
                self.held = down;
            }
            });
        });
        ctx.request_repaint();
    }
}
