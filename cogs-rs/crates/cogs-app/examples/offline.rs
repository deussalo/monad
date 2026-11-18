//! Runs the audio callback's work offline, as a phone would in 192-frame blocks: a world plays for N seconds and is
//! written to a WAV, with how loud it was, how many notes the machine made, and the worst block's render time.
//! cargo run -p cogs-app --release --example offline -- [world index 0..2] [seconds]
use std::io::Write;
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ix: usize = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let secs: f64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(12.);
    let (name, src) = cogs::WORLDS[ix];
    let world = cogs_sim::World::from_str(src).expect("world");
    let sr = 48000.0f32;
    let (sh, mut render) = cogs::play::start(world, sr);
    // let the sync thread hand over the patches before the machine starts
    std::thread::sleep(std::time::Duration::from_millis(400));
    sh.world.lock().unwrap().set_paused(false);
    let (n, total) = (192usize, (secs * sr as f64) as usize);
    let (mut l, mut r) = (vec![0f32; n], vec![0f32; n]);
    let mut out: Vec<i16> = Vec::with_capacity(total * 2);
    let (mut peak, mut sum, mut worst, mut nan) = (0f32, 0f64, 0f64, false);
    let mut done = 0;
    let mut times: Vec<f64> = Vec::new();
    while done < total {
        let t = std::time::Instant::now();
        render.block(&mut l, &mut r, 0.008);
        let el = t.elapsed().as_secs_f64(); worst = worst.max(el); times.push(el);
        for i in 0..n { for x in [l[i], r[i]] { if !x.is_finite() { nan = true } peak = peak.max(x.abs()); sum += (x * x) as f64; out.push((x.clamp(-1., 1.) * 32767.) as i16) } }
        done += n;
        // a keyboard note every second, to hear the MIDI path too
        if done % 48000 < n { sh.midi(cogs::midi::Midi::On { note: 60 + ((done / 48000) % 12) as u8, vel: 100 }) }
        if done % 48000 >= 24000 && done % 48000 < 24000 + n { sh.midi(cogs::midi::Midi::Off { note: 60 + ((done / 48000) % 12) as u8 }) }
        std::thread::sleep(std::time::Duration::from_micros(200));
    }
    let rms = (sum / (out.len() as f64)).sqrt();
    let path = format!("/tmp/cogs-offline-{ix}.wav");
    let mut f = std::fs::File::create(&path).unwrap();
    let bytes = out.len() * 2;
    f.write_all(b"RIFF").unwrap(); f.write_all(&(36 + bytes as u32).to_le_bytes()).unwrap(); f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap(); f.write_all(&1u16.to_le_bytes()).unwrap(); f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&48000u32.to_le_bytes()).unwrap(); f.write_all(&(48000u32 * 4).to_le_bytes()).unwrap(); f.write_all(&4u16.to_le_bytes()).unwrap(); f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap(); f.write_all(&(bytes as u32).to_le_bytes()).unwrap();
    for s in &out { f.write_all(&s.to_le_bytes()).unwrap() }
    let m = &sh.meters;
    println!("{name}: {secs}s → {path}\n  peak {:.3} ({:.1} dBFS) · rms {:.1} dBFS · NaN {nan}\n  machine notes {} · keys {}\n  worst block {:.2} ms of {:.2} ms budget",
        peak, 20. * (peak as f64).log10(), 20. * rms.log10(), m.events.load(std::sync::atomic::Ordering::Relaxed), m.notes.load(std::sync::atomic::Ordering::Relaxed),
        worst * 1000., n as f64 / 48.);
    let mut ts = times.clone(); ts.sort_by(|a, b| a.partial_cmp(b).unwrap()); let pct = |q: f64| ts[((ts.len() - 1) as f64 * q) as usize] * 1000.;
    let budget = n as f64 / 48000.; let over = times.iter().filter(|&&x| x > budget).count();
    let first_over = times.iter().position(|&x| x > budget);
    println!("  blocks: mean {:.2} ms · p50 {:.2} · p99 {:.2} · p99.9 {:.2} · over budget {} of {} (first at block {:?})", times.iter().sum::<f64>() / times.len() as f64 * 1000., pct(0.5), pct(0.99), pct(0.999), over, times.len(), first_over);
    sh.running.store(false, std::sync::atomic::Ordering::Relaxed);
}
