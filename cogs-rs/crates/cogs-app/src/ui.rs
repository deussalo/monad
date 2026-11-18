//! The first screen: connect a keyboard, see what the audio is doing, and a row of keys to compare touch with MIDI.
use crate::audio::{Inbox, UiCmd};
use eframe::egui;
use std::sync::{atomic::Ordering::Relaxed, Arc, Mutex};

pub struct App {
    pub inbox: Arc<Inbox>,
    pub status: Arc<Mutex<String>>,
    pub connect: Box<dyn Fn()>,
    held: Option<u8>,
}

impl App {
    pub fn new(inbox: Arc<Inbox>, status: Arc<Mutex<String>>, connect: Box<dyn Fn()>) -> Self { App { inbox, status, connect, held: None } }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        ctx.set_pixels_per_point(ctx.pixels_per_point().max(2.0));
        let m = &self.inbox.meters;
        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_space(40.0);
            ui.heading("cogs · Rust");
            ui.label(self.status.lock().map(|s| s.clone()).unwrap_or_default());
            if ui.add_sized([220.0, 56.0], egui::Button::new("Connect keyboard")).clicked() { (self.connect)() }
            ui.separator();
            let sr = m.sample_rate.load(Relaxed).max(1) as f32;
            ui.label(format!("audio: {} Hz · block {} · burst {} · buffer {} ({:.1} ms)", sr as u32, m.block.load(Relaxed), m.burst.load(Relaxed),
                m.buffer.load(Relaxed), m.buffer.load(Relaxed) as f32 / sr * 1000.0));
            ui.label(format!("measured output latency: {:.1} ms · glitches: {}", m.latency_us.load(Relaxed) as f32 / 1000.0, m.xruns.load(Relaxed)));
            ui.label(format!("notes played: {} · last {}", m.notes.load(Relaxed), m.last_note.load(Relaxed)));
            ui.add_space(24.0);
            ui.label("touch keys (to compare with the keyboard):");
            // One octave of big keys: press plays, release stops.
            let keys = [60u8, 62, 64, 65, 67, 69, 71, 72];
            let w = (ui.available_width() / keys.len() as f32 - 4.0).max(30.0);
            let mut down = None;
            ui.horizontal(|ui| {
                for k in keys {
                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, 160.0), egui::Sense::click_and_drag());
                    let on = resp.is_pointer_button_down_on();
                    ui.painter().rect_filled(rect, 6.0, if on { egui::Color32::from_rgb(230, 180, 60) } else { egui::Color32::from_gray(200) });
                    if on { down = Some(k) }
                }
            });
            if down != self.held {
                if let Some(k) = self.held { self.inbox.ui(UiCmd::Off(k)) }
                if let Some(k) = down { self.inbox.ui(UiCmd::On(k, 100)) }
                self.held = down;
            }
        });
        ctx.request_repaint();
    }
}
