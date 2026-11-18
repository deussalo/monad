//! How the ring's depth follows glitches: printed for the cases that matter. Depth in frames, one tick each 50 ms.
//!   cargo run -p cogs-app --release --example ladder
use cogs::out::{Ladder, Move};
fn run(name: &str, glitches: &[f64], secs: f64) {
    let (mut v, step, base) = (480u32, 240u32, 480u32);
    let mut l = Ladder::new(base, 0.);
    let (mut t, mut log, mut next) = (0., Vec::new(), glitches.iter().copied().peekable());
    while t < secs {
        let g = next.peek().is_some_and(|x| *x <= t); if g { next.next(); }
        match l.tick(t, g, v, step, false) {
            Move::Up => { v += step; l.grew(t, v); log.push(format!("{t:.1}s up {v}")) }
            Move::Down => { v -= step; log.push(format!("{t:.1}s down {v}")) }
            Move::Stay => {}
        }
        t += 0.05;
    }
    println!("{name}: ends at {v}, floor {}; {}", l.floor, log.join(", "));
}
fn main() {
    run("a glitch while starting (ignored)", &[0.5, 1.0], 40.);
    run("one early glitch (Capstan)", &[3.0], 40.);
    run("two early glitches", &[3.0, 4.0], 40.);
    run("a glitch right after stepping down (the depth was needed)", &[3.0, 15.0], 60.);
}
