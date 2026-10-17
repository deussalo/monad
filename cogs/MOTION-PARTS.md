# cogs: motion parts — knobs that move over time

This is a design, written against `rig/fleet/roll-snap` at `89d24dd`.

**Status (2026-10-17): part 1, the cam, is built** (`cogs/src/motion.js`, plus the worklet's `motion` message and
`cp.mac`). It differs from the plan below in these ways, to keep the first cut small:
- **The link lives on the cam** (`cam.to`, `cam.macro`, `cam.lo`, `cam.hi`), not in a separate `W.links` list: one
  cam turns one macro of one reader. Rod objects (several per part, invert, hold, glide) can come later.
- **Linking is tap, not drag:** the cam's dial has a link sector; tap it, then tap a synth reader. On the desk, and in
  the cam's sheet on the phone, a "turns" row picks the reader. Either way the cam takes the first free macro and the
  reader's patch sheet opens with that macro armed.
- **Shape editing** is a strip in the cam's sheet (drag to draw; the shape becomes "drawn") plus shape presets on the
  dial; there is no rim-pull brush yet.
- **The shelf** has a tenth slot for the cam (key `e` on the desk); the fan-out slot waits for the second motion part.
- **Re-posting**: `postMotions()` runs every frame but posts only when a program actually changed (rate, window,
  shape version, or the phase drifting by more than 1e-4 of a turn), so pauses, crank changes, undo and kind changes
  are all caught without hooks. A new program applies at once (no pending queue).
- The worklet smooths the macro over about 3 ms.

The operator's question: *which components could we attach to knobs inside the synth, so that something like a filter cutoff moves over time, and how should they connect?*

The short answer:
- **Five motion parts.** Each is driven by the gear train like everything else, so the modulation turns with the music and you can see it turning.
- **One way to connect them, the push-rod.** You draw a rod from a motion part to a reader. The rod turns one of the reader's **macros**. The patch's existing macro routes carry that macro to cutoff, wavetable position, FX mix, or anything else.
- **The worklet computes the motion itself.** It receives a closed-form description of the motion, not a stream of values. Modulation therefore lands on the same sample clock as the notes, with no main-thread jitter. It needs a message only when the motion changes.

## What the machine gives us to build on

These facts from the code shape every choice below.

- **Angles are closed-form.** Every cog's angle is `θ(t) = θ0 + w·t`. `w` is constant per jam-free train (`solve()` in `sim.js`). `advance()` runs sim time `S.lookahead` (≥ 0.12 s, 0.2 s in the default world) ahead of what you hear. A strike at sim time `t` is posted to audio frame `(t + clock.off − clock.shift)·sampleRate` (`fire()`).
  - So anything that depends only on cog angles is known exactly, ahead of time. IDEAS.md calls this "exact".
- **Each reader owns its voice.** A reader's `voice` is its own object, and so is its wave patch. `wavePost(v)` gives each voice object a worklet patch id (a `WeakMap`) and re-posts the whole patch JSON whenever it changes.
  - Patches are therefore **per reader instance** today. The library (`patchLib`) and the presets are *templates* that get copied into a reader.
  - `loadPatch()` replaces a voice's patch in place, routes included. Changing the kind (`voiceSectors`) or undoing (`restore` → `deserialize`) builds **new voice objects, so the patch ids change**.
- **The worklet compiles a patch.** `setPatch()` turns a patch into `cp`: normalised `base`, packed `routes`, and so on. Modulation runs once per 32-sample slice (`WSLICE`) in `render()`: `src[] → routes → vals[]`.
  - The macros are sources read straight from `cp.base[MAC_P[i]]`.
  - Macros are the only sources that also reach the FX rack (`runFx`), because the rack is shared by every voice of the patch.
- **Re-posting a patch is expensive.** It costs a JSON stringify on the main thread, then a recompile in the worklet, plus `vals.set` on every live voice. It is fine per edit and wrong at 60 Hz.
- **The phone's limits.** The shelf is already 9 parts wide (`m c d a b p t r k`). At 390 px that is about 43 px a slot, under the 44 px rule. A bezel shows about 6–8 sectors. The synth sheet covers the machine, and while it is open the machine is drawn at a quarter of the frame rate.

The motion parts therefore have to be things whose output is a function of cog angle, or of strike times. Those are exactly what `advance()` already computes ahead of time.

---

## Five motion parts

Every part produces a **travel** value between 0 and 1. That value is what a rod carries (see "Connecting").

The table below rates each part on the IDEAS.md scales:
- **Cost:** S fits in under about 100 lines; M adds one new primitive.
- **Timing:** exact is closed-form in the cog angles; scheduled means it changes only at deterministic event times.

| # | Part | Signal | Period / sync | Cost | Timing |
|---|---|---|---|---|---|
| 1 | **Cam & follower** | any drawn shape | one turn of its cog | M | exact |
| 2 | **Crank & slider** | skewed sine, from geometry | one turn of its cog | S once the cam exists | exact |
| 3 | **Ratchet wheel** | stepped values, optional glide | one step per driver pin; N steps per cycle | S once the cam exists | exact |
| 4 | **Worm & phrase hand** | ramp, triangle or one-shot over many bars | 1 tooth per driver turn (1 : N) | S once the cam exists | exact |
| 5 | **Struck leaf spring** | decaying kick or wobble after each hit | arm hits | M | scheduled (exact) |

Parts 1–4 are all one primitive: **a table read at a phase that is linear in time** (see the protocol). Part 5 adds strike events.

### 1. Cam & follower

**What it is.** A brass cam plate stacked on a cog's axle, with a follower lever whose roller rides the cam's edge. The lever's lift is the output.
- The profile is the shape of the modulation, drawn as the plate's outline, so the shape you see is the shape you hear.
- One revolution of the cog is one cycle. Under tempo lock, a cog's detents are revolutions per bar, so a cam is a tempo-synced LFO with a **drawable** shape. Drawable LFO shapes are SYNTH-PARITY gap #2.

**How it looks and moves.**
- The plate turns with its cog.
- The follower is a heat-blued lever on a fixed pivot. Its roller rises and falls on the turning outline.
- A thin gauge arc at the pivot shows the lift.
- A pin flare (as on discs) marks the lowest point, so the phase reads at a glance.

**Signal.** `travel = profile(α − θ(t) − φ)`.
- `α` is the follower's world angle and `φ` is the cam's phase on its cog.
- The profile has 64 points per turn, linearly interpolated (a "steps" option uses nearest-point lookup).
- The range is 0–1, unipolar. The rod can invert it.

**Make it on a phone.**
- Drag a cam from the shelf onto a cog. It stacks on the axle the way a cog does, and the follower lands where you let go.
- At part zoom, **pull the rim** in or out to push the outline at that angle. This is the same radial gesture that resizes a cog. It acts as a soft brush: the pull moves the nearest point plus a falloff of ±3 points.
- When the cam shares an axle with a disc, the brush snaps to that disc's step divisions.
- **Drag the follower** around the plate to move the phase.
- **Bezel:**
  - Shape preset: sine, ramp up, ramp down, triangle, square, steps 4, steps 8, random.
  - Smooth: a lowpass over the profile.
  - Steps: on or off.
  - Phase: snaps to beats.
  - The link action (see "Connecting").
- A **profile sheet** gives fine editing. It is a strip like the velocity lane: one revolution unrolled left to right, with the disc's grid behind it, and you drag to paint.

**Musical use.**
- A filter that opens every bar.
- A 1/8 wobble: put the cam on a cog that turns 8× per bar.
- An accent shape on a groove.
- A wavetable sweep that repeats with the phrase.
- Put the cam on the *same axle as a disc* and its shape is drawn against that disc's notes. This is an automation lane you can see beside the notes it shapes.

### 2. Crank & slider

**What it is.** A crank pin on a cog drives a connecting rod, which pushes a slider along a straight guide, like a steam engine's piston. The slider's position is the output.

**How it looks and moves.**
- The pin goes round and the rod swings.
- A brass block slides in a steel slot, with a scale engraved along the slot.

**Signal.** `x(θ) = r·cos θ + √(L² − r²·sin² θ)`, normalised to the stroke.
- With a long rod it is a sine.
- As the rod shortens toward the crank radius it skews: it lingers at the outer end and snaps back through the inner end. That is a "sharper sine" with no drawing at all.
- Period: one turn of the cog.

**Make it on a phone.** Drop it on a cog, then:
- drag the **pin** in or out to set the crank radius (the depth knob, shown as the stroke length);
- drag the **slider block** along its slot to set the rod length (the skew);
- swing the **guide** around the cog to set the phase.

No sheet is needed. Every parameter is a drag on the part itself.

**Musical use.** It is the beginner's LFO, and the first thing to show someone:
- the pumping, breathing filter;
- the swell on a pad;
- with a short rod, a near-sidechain "suck and release" shape.

### 3. Ratchet wheel (step modulator)

**What it is.** A toothed ratchet wheel advanced one tooth at a time by a pawl. The pawl is kicked by pins on a driver cog. Each tooth carries a peg of its own height, and a pointer reads the height of the tooth under it.
- It is a step sequencer of *values*, not notes.
- The tooth count is independent of the driver. Five teeth advanced four times per bar make a 5-against-4 cutoff pattern that realigns every five bars.

**How it looks and moves.**
- The wheel clicks round one tooth at a time with a small overshoot, and a click haptic at part zoom.
- The pegs make a little skyline of heights around the rim.
- An optional **damper** vane slows each step into a glide, like a portamento on the value.

**Signal.**
- `step = floor(θ_driver · pins / 2π) mod teeth`
- `travel = height[step]`
- With the damper, a one-pole glide of `slew` seconds.

Because `θ_driver` is closed-form, the step instants are exact. They need no event bus: the step is a phase that runs at `pins/teeth` cycles per driver turn, read from a stepped table. It is the same primitive as the cam, using nearest-point lookup.

**Make it on a phone.**
- Drop it next to a cog and it meshes, so that cog becomes the driver.
- Pull the rim to change the tooth count (2–16). This is the cog's own gesture.
- Drag a peg up or down to set its height. At part zoom each peg is a 44 px radial bar.
- **Bezel:**
  - Pins per driver turn: 1, 2, 3, 4, 6 or 8.
  - Glide time.
  - Randomise.
  - The link action.

**Musical use.**
- Stepped filter patterns, as on a TB-303 or a step LFO.
- Wavetable-position sequences.
- Polymetric timbre against a straight beat.
- With glide, a "stepped but sung" movement.

### 4. Worm & phrase hand

**What it is.** A worm screw on a driver cog's axle turns a large worm wheel by **one tooth per driver turn**. That gives ratios of 1 : 16, 1 : 32 or 1 : 64 in one part. Without it you would need a chain of reducing cogs that does not fit on a phone screen.
- The wheel carries a long **phrase hand** over a dial engraved in bars.
- The hand's angle is the output.

**How it looks and moves.**
- The worm spins visibly.
- The hand creeps, slowly enough that you notice it has moved rather than see it move.
- The dial shows bar numbers and the current bar.
- The **one-shot** variant has a stop pin and a spring: the hand climbs to the pin and holds there, until you tap the part (live) or a trip arm releases it. The trip arm is IDEAS.md #6 and belongs to a later phase.

**Signal.** `travel = shape(θ_driver / (2π·N))`.
- `shape` can be a ramp (saw), a triangle, or an S-curve.
- N is the tooth count of the worm wheel, so one sweep lasts N driver turns. With a driver at 1 rev per bar and N = 16, one sweep is 16 bars.

**Make it on a phone.**
- Drop the worm on a cog's axle. The wheel and hand fold out beside it.
- Pull the wheel's rim to set N. It snaps to 4, 8, 16, 32 or 64, which are bars when the driver turns once a bar.
- **Bezel:**
  - Shape.
  - Loop or one-shot.
  - Reset: hand to zero. This is the same "to start" idea that `toStart()` applies to trains.
  - The link action.

**Musical use.**
- The long arc: a 16-bar filter rise into a drop.
- A pad that opens over a minute.
- Reverb wet that grows across a section.
- In the default world, a worm on the reese bass's driver turning macro 1 into `osc1.cut` is the demo that sells the feature.

### 5. Struck leaf spring (hit envelope)

**What it is.** A springy steel leaf (a reed) that an arm strikes in passing, exactly as it strikes a pad. The leaf deflects and then rings back to rest. Its deflection is the output.
- The arm that hits the kick pad can sweep the leaf too, so **the bass's filter ducks on every kick**. That is sidechain pumping, built mechanically.

**How it looks and moves.**
- The leaf bends on impact in proportion to the strike speed, which is the same `vn` that sets the pad's loudness.
- It then decays: either as a damped wobble, or overdamped as a single sag and return.
- A small weight on the leaf sets its period. Slide it out and the leaf rings slower.

**Signal.** After a hit at `t_k` with impact `a_k` (normalised `vn`), `travel = Σ a_k · e^{−(t−t_k)/τ} · c(t−t_k)`, clamped to 0–1.
- `c` is 1 when overdamped, or `cos(ω(t−t_k))` with the weight's ω when it wobbles.
- The hit times come from the same `hitWindow` sweep that arm → pad hits use (`armHit`), so they are exact and known `lookahead` ahead.

**Make it on a phone.**
- Drop it like a pad. Its rim sets the contact radius, as a pad's rim does.
- Drag the weight along the leaf to set the period.
- **Bezel:** damping, depth (scales `a`), and wobble (on or off). Drag one arm over both the kick pad and the leaf to sidechain.

**Musical use.**
- Sidechain pumping of cutoff or volume.
- A filter "blip" on each hit.
- Accent-driven brightness: a harder hit, from a faster cog or a longer arm, opens the filter more.

### Also considered

- **Flyball governor.** Output is the speed of its cog, so it only moves while you crank or retune. It is the right home for "live" modulation, using the fallback path below, but it does nothing in a steady machine.
- **Differential beat.** Two near-equal motors into a differential make a very slow beat-frequency hand. The worm gives the same long arcs with one motor and an exact bar count.
- **Groove ring on a disc.** This is the cam's seated form: a profile drawn on a disc, read by a cam on the same axle. It is covered by #1.

---

## Connecting a part to a synth knob

Four models are compared below. Each one has to answer the same questions:
- **phone UX;**
- **where routes live**, per patch or per reader instance;
- **how it maps into the worklet**;
- **timing;**
- **save format and undo.**

### A. A push-rod drawn from the part to a reader (*recommended*), landing on a macro

**Phone UX**
- Select the motion part. Its bezel has a **link** sector with a chain glyph.
- Drag out of that sector. A blued rod stretches from the follower or slider to your finger. This is the belt's carry band (`G.carry('belt')`, `carry.band`), reused.
- Let go over a reader. The rod snaps onto the reader's screw and claims that reader's first **free macro**: no routes, and still at its default of 0. You get a haptic tick.
- The reader's **synth sheet then opens with that macro chip already armed**. The existing hint says "macro 3 armed: drag a knob to set its depth", so one drag on cutoff finishes the connection.
- Total cost: one drag, one release, one knob drag.
- **Tap the rod** to select it. Its bezel offers:
  - macro slot, 1–8;
  - invert;
  - travel: the 0–1 window the rod maps (lo, hi);
  - hold: sample once per note at note-on;
  - glide.
- **Drag the rod** to the shelf to delete it, as with a belt.
- One part can drive several rods, and one reader can take up to 8 rods.

**Where routes live.**
- The link lives in the **world**: `from` is a part, `to` is a reader, plus a macro slot.
- The macro → knob routes live in the **patch**, where they already live. Patches stay portable:
  - Saving to the library, exporting, or loading a preset never carries a dangling part id.
  - Loading the `wobble` preset onto a linked reader means the cam is immediately turning wobble's macro 1. That macro already routes to `dist.drive`.
- Macros are the one source family that also reaches the FX rack. A rod can therefore move delay mix, distortion drive or phaser freq for free. A per-note source like `lfo1` cannot.

**Worklet.**
- A per-patch-id **macro override**, `src[MAC0+i]`. The routing code does not change.
- See the protocol below.

**Cost.**
- One extra level of indirection: part → macro → knob.
- The free-macro pick has to avoid a macro the user is already using. That is the "has no routes and sits at 0" rule.

### B. Matrix sources named after parts (`part:12`) in `patch.mods`

**Phone UX.** The synth sheet's chip row gains a chip for each motion part in the machine. Routing uses the existing arm-and-drag gestures. Nothing is drawn on the canvas.

**Where routes live.** Routes live in the patch. Every patch is a reader-instance copy, so a route is effectively per reader. However:
- **Exports and library saves carry `part:12`.** In another machine, part 12 is something else, or nothing.
- `wavePatch()` drops unknown sources, so **loading any preset silently cuts every link**.

**Worklet.** Still needs the same per-patch-id override. This model only moves the bookkeeping into the patch.

**Verdict.** It has the best routing UI, but there is nothing on the machine to see, the connection is invisible, and patches lose their portability. Its chip idea is kept inside A: a linked macro's chip shows the part's name and a live meter.

### C. Assignable slots in the reader's dial (bezel)

**Phone UX.**
- The reader's bezel gains 1–2 "mod" sectors. Tap one, then tap a motion part on the canvas, then choose a knob from a list.
- The bezel is already full: kind, six voice parameters and level.
- Choosing a knob from about 160 paths on a phone needs a picker sheet anyway.

**Where routes live.** Per reader, in the world, the same as A.

**Verdict.** It is model A without the drawing and with a worse destination picker. The bezel stays for rod settings, not for making links.

### D. Proximity: a part sitting on or touching a reader drives it

**Phone UX.** None at all. A follower lever whose tip rests on a reader drives that reader's macro 1.

**Where routes live.** Implicit, derived from geometry every frame.

**Verdict.** It is delightful once, then ambiguous:
- Which reader, when two are near?
- Which macro?
- Moving a reader to tidy the layout silently re-routes the sound.

It also contradicts the rule that every connection is an explicit object (belts, seats). Keep it only as a *drop shortcut*: dropping a motion part onto a reader creates an explicit rod.

### Timing: who computes the moving value

| Approach | Pros | Problems |
|---|---|---|
| **1. Main thread posts values per frame** (`{type:'macro', id, i, v}` from `frame()`) | Trivial. Works for anything. | Values arrive *now*, but notes play `lookahead` later (0.2 s here). The modulation would **lead the notes by up to 200 ms**, which is two 1/16s at 122 bpm. 16–66 ms steps (zipper). A stalled main thread stalls the sweep (the sheet's quarter-rate draw, phone GC, a background tab). |
| **2. Timestamped ramps** (sim evaluates at `simT`, posts `{at, v}` breakpoints, worklet interpolates) | Aligned with the notes. Works for live sources such as cranking or a governor. | A stream of messages forever. Still breaks during a main-thread stall longer than `lookahead`. Resolution is limited to the post rate. |
| **3. Closed-form programs in the worklet** (post *the motion*, not values) | Sample-accurate (per 32-frame slice, 0.7 ms) and on the same frame clock as notes. Zero messages in steady state. Survives main-thread stalls. Deterministic, so offline render and URL replay reproduce it bit-for-bit. | Only for motions that are functions of cog angle or of exact strike times. That covers all five parts. |

**Recommendation: 3 by default, with 2 as the fallback.** Use the same worklet slot for both: a program can be a phase table *or* a breakpoint queue. Live sources such as hand-cranking, or a future governor, simply re-post programs as `w` changes, which already happens on each `drive()` → `solve()`.

### Recommendation

Use **A: push-rod to a macro**, with **timing 3**. It is:
- the only model you can see on the machine;
- portable across presets and the library;
- able to reach FX knobs;
- the only option needing no routing UI beyond what the synth sheet already has.

The worklet change is small. It is a new message type and a macro override; it does not add a new source family to the matrix.

---

## Data model

The world gains two lists, `W.motions` and `W.links`. They are saved by `serialize()`, so **undo, autosave and URL share come free**. Undo snapshots `worldKey()`, and one rod drag is one step because it settles on pointer lift.

```js
// a motion part (kind-specific fields; the cam shown)
{id:31, type:'motion', kind:'cam', cog:4,     // rides cog 4's axle, like an arm rides its cog
 ang:1.2,                                      // follower's world angle (rad), fixed in the world
 phase:0,                                      // cam plate phase on its cog (rad)
 prof:[0.5, …64 numbers 0..1],                 // lift per 1/64 turn
 steps:false, smooth:0}
// the other kinds, same list:
// {kind:'crank', cog, r, L, ang}
// {kind:'ratchet', cog, pins, h:[…], glide}
// {kind:'worm', cog, n, shape, once}
// {kind:'leaf', x, y, r, tau, omega, depth}

// a rod
{id:40, from:31, to:12, macro:3,               // part 31 turns reader 12's macro 3 (1-based)
 lo:0, hi:1, inv:false, hold:false, glide:0}
```

**Rules**
- `deserialize` is lenient about links. A link whose `from` or `to` is missing, or whose reader's voice is not `wave`, is **dropped, not thrown**. A rod is a soft dependency, unlike an arm's cog.
- `removeObj`:
  - deleting a motion part or a reader removes its rods;
  - deleting a cog removes the motion parts on it, as it already removes arms.
- `copyParts` brings rods along when both ends come.
- **Changing a reader's kind away from `wave` keeps its rods.** They are drawn slack (dimmed) and do nothing until the reader is a synth again.
- A macro driven by a rod shows the part's name in the sheet ("cam 31"). This is a display override; `macroNames` is not changed, so the patch stays clean.
- The macro knob's own stored value stays put and comes back when the rod is removed.

## Message protocol (main thread → `cogs-wave`)

```js
// install or replace a program for one macro of one patch, taking effect at audio frame `at`
{type:'motion', id, slot /*0..7*/, at, prog}
prog = {table:Float32Array(64), step:false,   // the shape: cam profile, crank curve, ratchet heights, worm shape
        f0, ph0, rate,                        // phase(frame) = ph0 + rate·(frame − f0)/sampleRate  (cycles)
        lo, hi, inv, glide, hold}             // the rod's window, invert, one-pole glide seconds, per-note hold
// prog:null at `at` releases the macro back to its knob value

// part 5 (leaf) uses an impulse program and strikes, posted like notes
prog = {kind:'leaf', tau, omega, depth, lo, hi, inv, glide, hold}
{type:'kick', id, slot, at, amp}
```

**Phase anchor.** The main thread takes the anchor from the sim's own state at `simT`, so it is aligned with the strikes already posted:
- `f0 = (simT + clock.off − clock.shift)·sampleRate`. This is the frame mapping `fire()` uses.
- For the cam, `ph0 = (ang − (c.th + phase))/2π` and `rate = −c.w/2π`. The follower is fixed in the world, so the plate passes under it backwards.
- For the crank, the table is the normalised slider curve over one turn, and `ph0`/`rate` are the cog's.
- For the ratchet, `rate = c.w/2π · pins/teeth`, `step:true`, and the table holds the tooth heights resampled to 64.
- For the worm, `rate = c.w/2π / n`.
- When `once` is set, the worm's phase clamps at 1 rather than wrapping.

**When to re-post.** `at = f0`. The switch is scheduled at `simT`, *not* now, so the sound before `simT` keeps the old motion. That sound was simulated with the old motion and its notes are already queued.

The main thread re-posts on:
- `solve()`, if any linked cog's `w` changed (crank, retune, belt, mesh);
- `setPaused`: rate 0 at pause and a new `f0` at resume, because `clock.off` moves;
- `syncClock` changing `clock.shift`;
- a profile or part edit (debounced; the table is transferred);
- `deserialize`/`restore`: undo and load make new voice objects, so the patch ids change;
- `startWave()`: audio starting late;
- a slow resync every 2 s, as a cheap guard against float drift between `turn()`'s stepped `θ` and the worklet's closed form.

**Worklet side**
- Motions are kept in `this.motions: Map<id*8+slot, {prog, pending:[{at,prog}], y}>`, **separate from `cp`**.
  - `setPatch()` rebuilds `cp` on every knob edit, and a rod must survive that.
  - A patch that has not been posted yet (no note played) simply has no `cp` to apply to.
- Each slice, after notes start and before voices render, the worklet:
  1. evaluates each motion at the slice's frame;
  2. glides it;
  3. writes it into `cp.mac[slot]`.
- `cp.mac` is a new `Float64Array(8)`, initialised from `base` in `setPatch()`. `render()` reads `src[MAC0+i] = cp.mac[i]` instead of `cp.base[MAC_P[i]]`. `runFx()` reads `cp.mac` in both places it reads macros: the source and the aux.
- `hold` copies `cp.mac[slot]` into the voice at `start()`, so each note keeps the cutoff it was struck with.

## Minimal implementation plan: the cam (part 1)

1. **`sim.js` (world, save, timing)**
   - Add `W.motions` and `W.links`, plus `serialize`/`deserialize` with the lenient link check. Extend the `W.next` id scan.
   - Add `motionTravel(m, t)`: the closed-form value at sim time `t`, used for drawing, the sheet meter and tests. Add `motionProgram(m)`, which returns `{table, f0, ph0, rate, step}` at `simT`.
   - Add `postMotions()`. It walks `W.links`, calls `wavePost(reader.voice)` for the id (this also posts the patch if needed), and posts a `motion` message per link only when its program changed. It keeps a key of `id`, `slot`, `ph0`-at-a-common-frame, `rate` and table version.
   - Call `postMotions()` from `solve()`, `setPaused`, `syncClock` (when `shift` changes), `deserialize` and `startWave`, plus a 2 s timer.
2. **`wave.worklet.js`**
   - Add `cp.mac` in `setPatch()`.
   - Add the `motion` message and the `this.motions` map with its `pending` queue, applied in `process()` like `queue`.
   - Add `motionAt(prog, frame)`, the slice-loop write, and the `render()`/`runFx()` reads of `cp.mac`.
   - About 40 lines.
3. **`stage.js` (part, look, gestures)**
   - Add a `cam` entry in `PART_KEYS`, `newPart`, `placePart` (attach to the cog under the drop, like an arm; `ang` from the drop point) and `drawPartIcon`.
   - Add `drawCam`: a brass outline from `prof` turning with `renderTh(c)`, a blued follower with a roller at `motionTravel(m, clock.view)`, and a gauge arc.
   - Add hit tests, `G.cam`:
     - rim radial drag edits `prof` with a falloff brush, snapping to a coaxial disc's `div`;
     - follower drag sets `ang`;
     - double tap focuses.
   - Add a rod carry, cloned from the belt carry: from a part to a reader.
   - Add `drawRod`: a rod from the follower tip to the reader's screw, with its end nudged by the travel, dimmed when slack.
   - Add `removeObj`/`copyParts` cascades.
   - **The shelf:** replace the separate slots with one **motion** slot. Holding it fans out cam, crank, ratchet, worm and leaf; dragging up from the fan carries one. This keeps the shelf at 9 slots.
4. **`phone.js`**
   - Add `motionSectors(m)`: shape preset, smooth, steps, phase, and the link action, which starts the rod carry from the bezel.
   - Add `rodSectors(l)`: macro, invert, travel lo/hi, hold, glide.
   - Add `activeBezel` cases for both.
   - After a rod lands, `select('reader', r)`, open its synth sheet, and set `waveUi.arm = 'm' + slot`.
5. **`wave-ui.js`**
   - A rod-driven macro's chip shows the part's name and a live travel bar, computed on the main thread with `motionTravel` at `clock.view`. No round trip to the worklet is needed.
   - Its macro knob animates and is marked "driven by cam 31".
6. **`desk.js`/inspector.** Rows for the cam and the rod (the same fields), and a toolbar key for the motion parts.
7. **Tests**
   - `verify-wave.js`: an offline render of a saw through `osc1.cut` with a ramp cam at 1 rev per 0.5 s. Check that the spectral centroid of successive windows rises and wraps with the cam's phase, and that a note's onset sits at the expected profile value. That proves the modulation and the notes are on one clock.
   - `verify.js`:
     - serialize → undo → redo round-trip with a cam and a rod;
     - deleting the cog removes the cam and the rod;
     - loading a preset onto the linked reader keeps the rod;
     - pausing holds the value, and resuming continues it in phase.
8. **Docs.** Add a "Motion parts" row to DESIGN.md's model table, and update the SYNTH-PARITY drawable-LFO line.

**Order after the cam:**
1. The crank, which is only a table generator plus its drags.
2. The worm, the best demo.
3. The ratchet.
4. The leaf, which adds the `kick` path.

**Open questions for the operator**
- **Should a rod also reach drum and modal voices, and the room?** Drums are WebAudio graphs, so `setValueCurveAtTime` on their params is possible. The room is `SPACE`, which feeds the engine worklet.
  - This design keeps them out: a rod only takes a synth reader.
  - The room's wet and size would be the first extension, through the same program idea in `engine.worklet.js`.
- **Should a rod be able to land on a pad with a wave voice?** Readers only is the simplest place to start.
