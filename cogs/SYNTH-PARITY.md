# Wave synth vs Serum 2: parity report

The heartbeat loop updates this file every half hour: it researches Serum 2, ranks the gaps, builds the top ones and
logs what landed. Sources: Xfer's [Serum 2 What's New](https://static.xferrecords.com/Serum%202%20What's%20New.pdf),
[RouteNote overview](https://create.routenote.com/blog/serum-2-everything-you-need-to-know/), [Dubspot](https://blog.dubspot.com/xfer-records-releases-serum-2).

## Where it stands

| Area | Serum 2 | Wave synth now | Gap |
|---|---|---|---|
| Oscillators | 3 main (wavetable, sample, multisample, granular, spectral) + sub + noise | 3 wavetable oscs built additively (basic, harmonics, sweep, pulse, vocal, organ) + sub + noise | sample/granular/spectral modes, warp modes (only bend), wavetable import/editor |
| Per-osc | unison up to 16, detune, blend, stack, dual warp | unison 8, detune, blend, stereo, phase, pan; two warp slots (bend, sync, pwm, asym, mirror, fm, flip, quantize, am, rm; FM/AM/RM from another osc); per-osc amp ADSR, filter + filter ADSR | unison stack modes, random phase knob, warp modulator choice |
| Filters | 2 filters, series/parallel, many types, drive | per-osc SVF: lp12/24, hp12/24, bp, notch, peak; drive, key track | global filters with routing, comb/formant/ladder types |
| Envelopes | 4 (ENV1 = amp), BPM sync, curves | 4 global mod envs (ENV1 also amp for sub/noise) + per-osc amp and filter envs | segment curve shaping, BPM sync |
| LFOs | up to 10, path/2D/chaos/S&H, BPM sync, drawable | 4: sine, tri, saw up/down, square, S&H, smooth; retrig or free; rate or tempo sync (1/32 to 4 bars), phase | drawable shapes, envelope/one-shot mode, chaos, more LFOs |
| Macros | 8, assignable, as destinations | 8 named macros, any knob can take one | macro curve per route |
| Mod matrix | any knob; bypass, reorder, curves, aux source | any knob, ±bipolar, bypass, delete, aux source (or inverted), drawn curve per route; drag-and-drop and arm-and-drag | reorder, modulating a route's depth, mod-wheel/aftertouch sources |
| Voicing | poly/mono/legato, glide, always, scaled glide | poly, mono, legato; glide with always and scaled; 16 voices; gate | voice count knob, glide curve |
| FX | 13 effects on buses, splitters, reorderable | per-patch rack in any order (↑ ↓): hyper/dimension (1-7 voices), distortion (tube, hard, fold, crush), flanger, phaser (6 stages), chorus, delay (tempo sync, ping-pong, tone), compressor, EQ shelves; macros drive it | reverb per patch, filter FX, multiband split/second bus, tempo sync for flanger/phaser |
| Extras | arpeggiator, clip sequencer, preset browser, undo | patch library (save/load/delete), JSON import/export, factory presets, undo | arpeggiator, preset tags |

## Next, ranked

1. Oscillator modes: spectral (live additive with partial modulation); a choosable warp modulator source.
2. Drawable LFO shapes; envelope BPM sync and segment curves.
3. Live mod display on knobs (moving dot), mod-wheel/aftertouch-like sources (e.g. reader hit speed).

## Log

- 2026-10-17 03:30Z: first build. Wave voice kind; worklet with additive tables and mips, per-osc amp/filter envelopes and
  SVF filters, sub, noise, 3 envs, 4 LFOs, 8 macros, mod matrix; Serum-style knob editor with source chips (arm or drag),
  matrix tab, patch library, JSON export/import, 7 factory presets; `cogs/verify-wave.js`.
- 2026-10-17 04:00Z (beat 1): FX rack per patch (distortion 4 modes, chorus, tempo-synced ping-pong delay with tone, EQ shelves,
  compressor), macro routes into FX knobs, LFO tempo sync. Also from the operator: the reader and the comb are now two parts
  on the shelf; new readers play the wave synth (pluck); the modal synth's bodies are tuned to equal temperament.
- 2026-10-17 04:30Z (beat 2): matrix aux source (with invert) and a drawn curve per route; a 4th envelope; voicing modes
  poly/mono/legato with glide always and scaled glide; unison blend (power-kept).
- 2026-10-17 05:00Z (beat 3): FX rack made modular and reorderable; hyper/dimension, flanger and phaser added (8 modules).
- 2026-10-17 06:00Z (beat 4): two warp slots per oscillator with 10 modes (phase: bend, sync, pwm, asym, mirror, fm; amplitude: flip, quantize, am, rm).

## Performance and UX (operator's focus from beat 4 on)

CPU of the wave worklet, ms per second of audio on this box (node, min of 3; 8 held notes; 100 ms = 10% of a core):

| patch | before | after beat 5 |
|---|---|---|
| init | 52.5 | 26.2 |
| super saw | 145 | 112 |
| pluck | 28.7 | 18 |
| pad | 121 | 90 |
| reese bass | 119 | 91 |
| worst (3 osc × 8 unison, 16 notes) | 502 | 121 |

- 2026-10-17 06:15Z (beat 5): oscillator render split into its own method (V8 optimises it), mono path for unison 1,
  wide-open low-pass skipped, silent oscillators skipped, envelopes that only modulate stepped per slice in closed form,
  amp envelopes ramped per slice, warp-free fast loop, a 64-stream unison budget across voices; env4 now actually runs.
  UX: tabs in a 5×2 grid (no sideways scroll), ◀ patch ▶ browsing in the header (plays it), a sticky two-octave
  keyboard with octave shift, multi-touch and slide.
- 2026-10-17 06:40Z (beat 6): the keyboard is transposed by the voice's octave/semitones (it is MIDI into the instrument).
  Phone, patch sheet open: the machine behind the scrim is drawn at a quarter rate; 390×844 at 2×, CPU throttled 4×:
  frame median 33.4 → 16.7 ms, idle 0.6% → 37%. Sheet header compacted (level and vel sense side by side, no pitch
  readout, hint only while armed, choice rows one line): the oscillator knobs are now on screen when it opens.
- 2026-10-17 07:15Z (beat 6, operator steer): the synth transposes itself: tune.oct / tune.semi knobs in the patch
  (SUB tab, voice group), applied in the worklet to every note; a part's old octave/semitones fold into them on load.
  The part's dial for a synth is now just voice + volume (+ note on a pad) plus up to 12 knobs pinned from the editor
  with the ◎ DIAL chip (tap knobs to pin/unpin; saved in the patch as `dial`). Disc root/octave/range are stepped
  sliders on the dial, and octave/range get − + steppers on the notes sheet, like steps.
- 2026-10-17 07:20Z (beat 7): the machine's canvas draws at most at 2× device pixels. A 3× phone (390×844, CPU
  throttled 4×, default world playing): frame median 50 → 33 ms, p90 67 → 50 ms (44% of the pixels to raster).
