# cogs on the web

The same Rust app as on Android, in any browser with WebAssembly, WebGL and AudioWorklet.

    sh web/build.sh            # from cogs-rs/; needs wasm-bindgen-cli 0.2.130 (cargo install wasm-bindgen-cli --version 0.2.130)

It writes `web/dist/`: `index.html` with `cogs.js` + `cogs_bg.wasm` (the app), `worklet.js` + `worklet.wasm` (the
sound) and `cogs-web.js` (the glue). Serve the folder over http, or open `web/dist/cogs.html`, the same thing in one
self-contained page (it works from a file too).

## How it is put together

Static hosts don't send the COOP/COEP headers SharedArrayBuffer needs, so the UI and the sound can't share memory.
The machine therefore lives twice (crates/cogs-app/src/link.rs has the details):

- **The worklet** (`crates/cogs-worklet`, a bare wasm module with C exports: the worklet scope has no TextDecoder, so
  no wasm-bindgen there) owns the World and the Engine. Each 128-frame quantum is play.rs's `Render::block`, as on
  Android; play.rs's `Sync::step` runs every 8 quanta, in the worklet itself, because there are no threads.
- **The UI** (`crates/cogs-app/src/web.rs`, eframe's web runner) keeps its own World: the stage edits it and draws it.
  It advances that World to the heard time itself, throwing its sounds away, so gears, pins and flashes come from the
  same deterministic machine. When the world changes (an edit, a drag (checked every 150 ms), play or pause, a world
  loaded) it sends the whole save with its clock. The worklet lays it over its own World (`World::reload`: cogs keep
  their turns, live balls are kept, voices keep their version where their patch is unchanged, so only edited patches
  compile again), puts its clock back to the UI's time and steps forward silently to where it was. Nothing already
  played sounds again and nothing is skipped (`cargo run -p cogs-app --release --example web_link` checks this).
- **Back from the worklet**, every 8 quanta: its sim time and the context time that sim time will reach the speaker.
  The heard time is that less what `getOutputTimestamp` says is playing now. Also note counts, the peak, and the balls,
  which are chaotic, so only the worklet's world knows where they are. The UI lays them over its own.
- **MIDI**: Web MIDI and Web Bluetooth (BLE-MIDI packets parsed as cogs/src/midi.js does) in `cogs-web.js`, straight
  to the worklet: `{"type":"midi","d":[status,d1,d2]}`.

The sound starts on the first tap or key (the browsers' autoplay rule).
