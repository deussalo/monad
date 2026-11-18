# cogs in Rust

The HTML build (`../cogs/src`, built to `cogs.html` / `desk.html`) is the reference: same machine, same sound, same
saves. This is a port, not a redesign: when in doubt, do what the JS does, and keep its comments' intent.

## Crates

| crate | from | owns |
|---|---|---|
| `cogs-dsp` | `wave.spec.js`, `wave.worklet.js`, `wave.js` (patches, presets), `engine.worklet.js` (modal strikes), `verb.js`, `climax.js`, `field.worklet.js`, the drum graph in `sim.js` (`drum()`), master chain in `initAudio()` | `Engine`: everything that makes samples. Real-time safe `process()`: no allocation, no locks, no I/O. |
| `cogs-sim` | `sim.js` (world, solver, advance, persistence, demo), `motion.js` (cams), `pit.js` (box and balls), `probe.js`, `field.js` (field geometry and sends) | `World`: the machine, serde to the exact `.cogs.json` format, `solve()`, `advance()` reporting sounds as events. No audio, no UI. |
| `cogs-app` | `stage.js`, `phone.js`, `desk.js`, `wave-ui.js`, `midi.js` | egui UI, platform audio and MIDI, wiring sim events into engine commands. |

## The seam between them

- **Time.** The engine counts frames (`Engine::frame()`). The audio callback runs the sim: each block it advances the
  world to the block's end and turns each event into a `Cmd` at its exact frame. There is no 200 ms lookahead: sound is
  scheduled inside the block it falls in. The UI reads world state from the same lock it edits under (`try_lock` on the
  audio side; a busy lock just means the sim catches up next block).
- **Events.** `cogs_sim::Event` carries what `fire()` computes before the listener: time, which part, the voice, midi,
  amp, velocity, hold, plus the world position of the sound (for the listener's pan/distance and FX-field sends, which
  the app applies, since they depend on the camera).
- **Voices.** A part's voice is the save's `voice` JSON (`kind`: `wave`, `synth` (modal), or a drum kind). The app
  sends a wave patch to the engine once per change (`Cmd::Patch`) and notes by part id.
