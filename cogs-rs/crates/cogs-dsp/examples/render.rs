//! Renders a few seconds of each part of the engine to WAVs in /tmp/cogs-dsp, and prints each one's duration, peak,
//! RMS and whether it has NaNs or silence, for a quick check against the HTML build.
//!   cargo run -p cogs-dsp --release --example render [raw]
//! `raw` skips the master chain (to compare with the JS worklets alone).

use cogs_dsp::*;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Counts allocations and frees made while the audio thread's calls run: there must be none.
struct Counting;
static WATCH: AtomicBool = AtomicBool::new(false);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: std::alloc::Layout) -> *mut u8 { if WATCH.load(Ordering::Relaxed) { ALLOCS.fetch_add(1, Ordering::Relaxed); } std::alloc::System.alloc(l) }
    unsafe fn dealloc(&self, p: *mut u8, l: std::alloc::Layout) { if WATCH.load(Ordering::Relaxed) { ALLOCS.fetch_add(1, Ordering::Relaxed); } std::alloc::System.dealloc(p, l) }
}
#[global_allocator]
static A: Counting = Counting;

const FS: f32 = 48000.;

fn wav(path: &str, l: &[f32], r: &[f32]) {
    let n = l.len();
    let mut b = Vec::with_capacity(44 + n * 4);
    let w32 = |b: &mut Vec<u8>, x: u32| b.extend_from_slice(&x.to_le_bytes());
    let w16 = |b: &mut Vec<u8>, x: u16| b.extend_from_slice(&x.to_le_bytes());
    b.extend_from_slice(b"RIFF"); w32(&mut b, 36 + n as u32 * 4); b.extend_from_slice(b"WAVEfmt ");
    w32(&mut b, 16); w16(&mut b, 1); w16(&mut b, 2); w32(&mut b, FS as u32); w32(&mut b, FS as u32 * 4); w16(&mut b, 4); w16(&mut b, 16);
    b.extend_from_slice(b"data"); w32(&mut b, n as u32 * 4);
    for i in 0..n { for x in [l[i], r[i]] { w16(&mut b, ((x.clamp(-1., 1.) * 32767.) as i16) as u16); } }
    std::fs::write(path, b).unwrap();
}

struct Scene { name: &'static str, secs: f32, cmds: Vec<(f32, Cmd)> }

fn note(part: u32, key: u32, midi: f32, amp: f32, hold: f32) -> Cmd {
    Cmd::Note { at: 0, part, key, midi, amp, vel: amp, pan: 0., hold, dark: 0., sends: Sends::default() }
}
fn at(cmd: Cmd, t: f32) -> (f32, Cmd) { (t, cmd) }
fn patch(part: u32, name: &str) -> Cmd { Cmd::Patch { part, patch: Arc::new(CompiledPatch::new(&WavePatch::preset(name).unwrap())) } }
fn stamp(cmd: Cmd, frame: u64) -> Cmd {
    match cmd {
        Cmd::Note { part, key, midi, amp, vel, pan, hold, dark, sends, .. } => Cmd::Note { at: frame, part, key, midi, amp, vel, pan, hold, dark, sends },
        Cmd::Off { key, .. } => Cmd::Off { at: frame, key },
        Cmd::Modal { key, voice, midi, amp, pan, x, hold, dark, sends, .. } => Cmd::Modal { at: frame, key, voice, midi, amp, pan, x, hold, dark, sends },
        Cmd::Drum { voice, amp, pan, dark, sends, .. } => Cmd::Drum { at: frame, voice, amp, pan, dark, sends },
        c => c,
    }
}

fn run(s: &Scene, raw: bool, dry_room: bool) {
    let mut e = Engine::new(FS);
    e.set_raw(raw);
    if dry_room { let mut sp = SpaceParams::default(); sp.verb.0[cogs_dsp::verb::V_WET] = 0.; e.command(Cmd::Space(sp)); }
    let n = (s.secs * FS) as usize;
    let (mut l, mut r) = (vec![0f32; n], vec![0f32; n]);
    let mut cmds = s.cmds.clone();
    cmds.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut ci = 0;
    let block = 192; // an Android-sized buffer, not a multiple of 128
    let mut done = 0;
    let t0 = std::time::Instant::now();
    while done < n {
        let m = block.min(n - done);
        while ci < cmds.len() && ((cmds[ci].0 * FS) as usize) < done + m {
            let c = stamp(cmds[ci].1.clone(), (cmds[ci].0 * FS) as u64);
            WATCH.store(true, Ordering::Relaxed); e.command(c); WATCH.store(false, Ordering::Relaxed);
            ci += 1;
        }
        WATCH.store(true, Ordering::Relaxed);
        e.process(&mut l[done..done + m], &mut r[done..done + m]);
        WATCH.store(false, Ordering::Relaxed);
        done += m;
    }
    let el = t0.elapsed().as_secs_f64();
    let allocs = ALLOCS.swap(0, Ordering::Relaxed);
    if allocs > 0 { println!("{}: {allocs} allocations on the audio thread!", s.name); }
    let nan = l.iter().chain(r.iter()).any(|x| !x.is_finite());
    let peak = l.iter().chain(r.iter()).fold(0f32, |a, x| a.max(x.abs()));
    let rms = (l.iter().chain(r.iter()).map(|x| (*x as f64).powi(2)).sum::<f64>() / (2 * n) as f64).sqrt();
    let path = format!("/tmp/cogs-dsp/{}{}.wav", s.name, if raw { "-raw" } else { "" });
    wav(&path, &l, &r);
    println!("{:<16} {:>5.2}s  peak {:>6.3} ({:>6.1} dBFS)  rms {:>7.4} ({:>6.1} dB)  {}  cpu {:>5.1}%  {}", s.name, s.secs, peak, 20. * peak.log10(), rms,
        20. * rms.log10(), if nan { "NaN!" } else if peak < 1e-5 { "SILENT!" } else { "ok" }, 100. * el / s.secs as f64, path);
}

fn main() {
    std::fs::create_dir_all("/tmp/cogs-dsp").unwrap();
    let raw = std::env::args().any(|a| a == "raw");
    let mut scenes = Vec::new();
    // wave synth presets: an arpeggio of plucks, a held chord
    let arp = |part| (0..8).map(move |i| at(note(part, i, [60., 64., 67., 72.][i as usize % 4], 0.8, 0.2), i as f32 * 0.25)).collect::<Vec<_>>();
    for name in WAVE_PRESET_NAMES {
        let mut cmds = vec![at(patch(1, name), 0.)];
        if ["pad", "super saw"].contains(&name) {
            for (i, m) in [48., 55., 60., 64.].iter().enumerate() { cmds.push(at(note(1, i as u32, *m, 0.8, 2.), 0.)); }
        } else { cmds.extend(arp(1)); }
        scenes.push(Scene { name: Box::leak(format!("wave-{}", name.replace(' ', "-")).into_boxed_str()), secs: 4., cmds });
    }
    // the worst case: 3 oscillators × 8 unison, 16 notes, a rack with chorus, delay and reverb
    {
        let mut p = WavePatch::preset("super saw").unwrap();
        for i in 0..3 { p.osc[i].on = true; p.set(&format!("osc{}.uni", i + 1), 8.); p.set(&format!("osc{}.level", i + 1), 0.4); }
        p.fx_on[cogs_dsp::params::FX_REVERB] = true;
        let mut cmds = vec![at(Cmd::Patch { part: 1, patch: Arc::new(CompiledPatch::new(&p)) }, 0.)];
        for i in 0..16 { cmds.push(at(note(1, i, 40. + i as f32 * 3., 0.5, 3.), 0.)); }
        scenes.push(Scene { name: "stress", secs: 4., cmds });
    }
    // a modal strike: a bell chord
    let bell = ModalVoice::default();
    scenes.push(Scene { name: "modal-bell", secs: 4., cmds: [60., 64., 67.].iter().enumerate().map(|(i, m)| at(Cmd::Modal { at: 0, key: 100 + i as u32, voice: bell, midi: *m, amp: 0.8,
        pan: i as f32 * 0.5 - 0.5, x: 0.7, hold: 0., dark: 0., sends: Sends::default() }, i as f32 * 0.3)).collect() });
    // every drum, one after another
    let kinds = [DrumKind::Kick, DrumKind::Snare, DrumKind::Hat, DrumKind::Tom, DrumKind::Clap, DrumKind::Rim, DrumKind::Cowbell, DrumKind::Wood, DrumKind::Cymbal];
    for k in kinds {
        scenes.push(Scene { name: Box::leak(format!("drum-{}", k.name()).into_boxed_str()), secs: 1.5,
            cmds: vec![at(Cmd::Drum { at: 0, voice: DrumVoice::new(k), amp: 0.8, pan: 0., dark: 0., sends: Sends::default() }, 0.05)] });
    }
    // everything: a beat, plucks, a pad and bells through the room, a delay field and the line amp
    let mut mix = vec![at(patch(1, "pluck"), 0.), at(patch(2, "pad"), 0.), at(Cmd::Tempo { bpm: 120. }, 0.),
        at(Cmd::Field { slot: 0, params: Some(FieldParams::default()) }, 0.)];
    for b in 0..16 {
        let t = b as f32 * 0.25;
        let d = |k| at(Cmd::Drum { at: 0, voice: DrumVoice::new(k), amp: 0.7, pan: 0., dark: 0., sends: Sends::default() }, t);
        if b % 4 == 0 { mix.push(d(DrumKind::Kick)); }
        if b % 4 == 2 { mix.push(d(DrumKind::Snare)); }
        mix.push(d(DrumKind::Hat));
        let mut sends = Sends::default(); sends.amt[0] = 0.6;
        mix.push(at(Cmd::Note { at: 0, part: 1, key: 10 + b, midi: [60., 63., 67., 70.][b as usize % 4], amp: 0.6, vel: 0.6, pan: 0.3, hold: 0.15, dark: 0., sends }, t));
    }
    for (i, m) in [48., 55., 63.].iter().enumerate() { mix.push(at(note(2, 50 + i as u32, *m, 0.5, 3.5), 0.)); }
    mix.push(at(Cmd::Modal { at: 0, key: 99, voice: bell, midi: 79., amp: 0.5, pan: -0.4, x: 0.8, hold: 0., dark: 0., sends: Sends::default() }, 2.));
    // a patch edited while it plays, another part removed, a cam on the pluck's macro
    mix.push(at(patch(1, "wobble"), 3.)); mix.push(at(Cmd::RemovePart { part: 2 }, 5.));
    let mut table = [0f32; cogs_dsp::wave::MOTION_POINTS]; for (i, x) in table.iter_mut().enumerate().take(64) { *x = (i as f32 / 64.).sin(); }
    mix.push(at(Cmd::Motion { part: 1, slot: 0, prog: Some(MotionProg { table, len: 64, step: false, f0: 0., ph0: 0., rate: 0.5, lo: 0., hi: 1., glide: 0. }) }, 1.));
    scenes.push(Scene { name: "mix", secs: 6., cmds: mix });
    // the far listener: quieter, wetter, low-passed
    let mut far = scenes.last().unwrap().cmds.clone(); far.push(at(Cmd::Listen { e: 1. }, 0.));
    scenes.push(Scene { name: "mix-far", secs: 6., cmds: far });
    // raw: the wave synth dry (as its worklet's output), the modal engine with its room (as the engine worklet's)
    for s in &scenes { run(s, raw, raw && !s.name.starts_with("modal")); }
}
