//! What the audio callback does each block, whatever the platform: drain MIDI and UI commands, render.
use crate::{midi::Midi, testsynth::TestSynth};
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

/// What the UI reads back from the audio thread.
#[derive(Default)]
pub struct Meters {
    pub sample_rate: AtomicU32,
    pub block: AtomicU32,
    pub buffer: AtomicU32,
    pub burst: AtomicU32,
    /// Measured output latency, microseconds (frames queued ahead of the speaker).
    pub latency_us: AtomicU32,
    pub notes: AtomicU64,
    pub last_note: AtomicU32,
    pub xruns: AtomicU32,
}

#[derive(Clone, Copy, Debug)]
pub enum UiCmd { On(u8, u8), Off(u8), Panic }

/// The two ways in, shared with every thread that can send: the platform's MIDI threads and the UI.
pub struct Inbox {
    pub midi: Mutex<Producer<Midi>>,
    pub ui: Mutex<Producer<UiCmd>>,
    pub meters: Meters,
}

pub struct Render { midi: Consumer<Midi>, ui: Consumer<UiCmd>, synth: TestSynth, inbox: Arc<Inbox> }

pub fn channel() -> (Arc<Inbox>, impl FnOnce(f32) -> Render) {
    let (mp, mc) = RingBuffer::new(1024);
    let (up, uc) = RingBuffer::new(1024);
    let inbox = Arc::new(Inbox { midi: Mutex::new(mp), ui: Mutex::new(up), meters: Meters::default() });
    let ib = inbox.clone();
    (inbox, move |sr| Render { midi: mc, ui: uc, synth: TestSynth::new(sr), inbox: ib })
}

impl Inbox {
    pub fn midi(&self, m: Midi) { if let Ok(mut p) = self.midi.lock() { let _ = p.push(m); } }
    pub fn ui(&self, c: UiCmd) { if let Ok(mut p) = self.ui.lock() { let _ = p.push(c); } }
}

impl Render {
    pub fn block(&mut self, l: &mut [f32], r: &mut [f32]) {
        let m = &self.inbox.meters;
        while let Ok(e) = self.midi.pop() {
            match e {
                Midi::On { note, vel } => { self.synth.note_on(note, vel); m.notes.fetch_add(1, Relaxed); m.last_note.store(note as u32, Relaxed) }
                Midi::Off { note } => self.synth.note_off(note),
                Midi::Cc { cc: 120 | 123, .. } => self.synth.panic(),
                _ => {}
            }
        }
        while let Ok(c) = self.ui.pop() {
            match c { UiCmd::On(n, v) => { self.synth.note_on(n, v); m.notes.fetch_add(1, Relaxed); } UiCmd::Off(n) => self.synth.note_off(n), UiCmd::Panic => self.synth.panic() }
        }
        l.fill(0.0); r.fill(0.0);
        self.synth.render(l, r);
        m.block.store(l.len() as u32, Relaxed);
    }
}
