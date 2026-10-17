# Wave synth vs Serum 2: parity report

The heartbeat loop updates this file every half hour: it researches Serum 2, ranks the gaps, builds the top ones and
logs what landed. Sources: Xfer's [Serum 2 What's New](https://static.xferrecords.com/Serum%202%20What's%20New.pdf),
[RouteNote overview](https://create.routenote.com/blog/serum-2-everything-you-need-to-know/), [Dubspot](https://blog.dubspot.com/xfer-records-releases-serum-2).

## Where it stands

| Area | Serum 2 | Wave synth now | Gap |
|---|---|---|---|
| Oscillators | 3 main (wavetable, sample, multisample, granular, spectral) + sub + noise | 3 wavetable oscs built additively (basic, harmonics, sweep, pulse, vocal, organ) + sub + noise | sample/granular/spectral modes, warp modes (only bend), wavetable import/editor |
| Per-osc | unison up to 16, detune, blend, stack, warp ×2 | unison 8, detune, stereo, bend, phase, pan, per-osc amp ADSR, per-osc filter + filter ADSR | unison blend/stack, second warp, random phase knob |
| Filters | 2 filters, series/parallel, many types, drive | per-osc SVF: lp12/24, hp12/24, bp, notch, peak; drive, key track | global filters with routing, comb/formant/ladder types |
| Envelopes | 4 (ENV1 = amp), BPM sync, curves | 3 global mod envs (ENV1 also amp for sub/noise) + per-osc amp and filter envs | a 4th, curve shaping, BPM sync, legato modes |
| LFOs | up to 10, path/2D/chaos/S&H, BPM sync, drawable | 4: sine, tri, saw up/down, square, S&H, smooth; retrig or free; rate and phase | BPM sync, drawable shapes, envelope/one-shot mode, more LFOs |
| Macros | 8, assignable, as destinations | 8 named macros, any knob can take one | macro curve per route |
| Mod matrix | any knob; bypass, reorder, curves, aux source | any knob, ±bipolar, bypass, delete; drag-and-drop and arm-and-drag | aux source (mod × mod), route curves, modulating route depth |
| Voicing | poly/mono/legato, glide, voices | 16-voice poly, glide, gate length | mono/legato, glide curve, voice count knob |
| FX | 13 effects on buses | none in the synth (the machine's room reverb follows) | distortion, chorus, delay, EQ, compressor… |
| Extras | arpeggiator, clip sequencer, preset browser, undo | patch library (save/load/delete), JSON import/export, factory presets, undo | arpeggiator, preset tags |

## Next, ranked

1. FX rack on the synth output: distortion, chorus, delay (BPM-synced), then EQ and compressor.
2. BPM sync for LFOs and envelopes from the machine's bpm lock.
3. Aux sources and route curves in the matrix; a 4th envelope.
4. Mono/legato modes, unison blend.
5. More oscillator modes: spectral (live additive with partial modulation), and more warp modes (sync, FM from B, PWM).
6. Drawable LFO shapes.

## Log

- 2026-10-17 03:30Z: first build. Wave voice kind; worklet with additive tables and mips, per-osc amp/filter envelopes and
  SVF filters, sub, noise, 3 envs, 4 LFOs, 8 macros, mod matrix; Serum-style knob editor with source chips (arm or drag),
  matrix tab, patch library, JSON export/import, 7 factory presets; `cogs/verify-wave.js`.
