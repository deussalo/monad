# cogs: feature ideas

This is ideation against `cogs.html` as of `f8264d2`. Nothing here is built yet.

## How costs and timing are rated

Today every cog's angle is `θ(t) = θ0 + w·t`, where `w` is constant per jam-free component and comes from `solve()`. `advance()` sweeps sim time ahead of audio by `S.lookahead` (60 ms) in steps of at most `maxStep` radians. Hits are found by sub-step interpolation, so every strike lands on an exact sample. Each idea is rated against that architecture.

**Cost**
- **S**: fits `solve()`, `advance()` or a voice as they are, in under about 100 lines.
- **M**: adds one new primitive, which is a new solver edge kind, a new event kind or a new worklet voice.
- **L**: changes the time model, either with piecewise or time-varying `w`, or with analytic free bodies.
- **XL**: needs a real rigid-body integrator with contacts.

**Timing**
- **exact**: closed-form in the cog angles and swept like arms and dots are today. Deterministic and sample-accurate.
- **scheduled**: `w` or a topology changes only at sim-time instants that a deterministic event produces. Still exact, but `advance()` has to split its step at each change point.
- **live**: the result depends on a human gesture. It sounds at least `lookahead` late because strikes already posted to the worklet cannot be recalled. It replays identically only if the gesture is recorded into an input log.
- **integrated**: fixed-step dynamics in sim time. It reproduces on the same JS engine, and impacts are still sub-step interpolated. It is chaotic, though: a 0.1 px edit can change every downstream event, and the >0.5 s jump in `advance()` cannot simply skip ahead.

Determinism is the product's superpower, not just a constraint. An exact or scheduled machine can be rendered offline faster than real time, scrubbed, shared by URL and replayed bit-for-bit. Ideas that keep it are cheaper twice over.

---

## Curated top 15, by delight per unit cost

| # | Idea | Cost | Timing | Why it's near the top |
|---|------|------|--------|-----------------------|
| 1 | **Strings and bars tuned by length** (pitch from geometry, timbre from pluck point) | S | exact | It extends the core thesis from loudness to pitch. You tune by dragging an endpoint, and `harmonic` mode already models an ideal string. |
| 2 | **Belts and chains** | S | exact | They free the layout from mesh adjacency. A chain is just a solver edge with ratio `+rA/rB`, and it is the most-requested mechanism in any gear toy. |
| 3 | **Flapper arms** (bend on impact, spring back) | S→M | exact | The owner asked for it. Visual-only is S; a damped flapper that rattles across a ring of pegs (the roulette clicker) is M. |
| 4 | **Tempo lock and phasing motors** | S | exact | A global BPM with rpm in "revs per bar" makes grooves sit on a grid. A ±0.1 % detune on a second motor gives Steve Reich *Piano Phase* for free. |
| 5 | **Share by URL plus offline render to WAV** | S+M | exact | Deflate plus base64url in the hash gives one link per machine. Determinism means `OfflineAudioContext` renders a perfect take. |
| 6 | **Trip pads, clutches and step cogs** (Rube Goldberg logic without physics) | M | scheduled | A hit can release a latch, engage a clutch or advance a ratchet by one tooth. That gives chain reactions, counters, mute and solo, and song sections, all deterministic. |
| 7 | **Music-box comb** | S | exact | It is a reader whose tines each carry their own pitch, so the pitch follows the position along the bar rather than the ring. It is instantly legible and very pretty. |
| 8 | **Seeded probability and peg size per dot** | S | exact | It adds variation while every replay stays identical. Peg size sets pluck depth and therefore velocity, so it stays geometric. |
| 9 | **Audio-rate gears** (rhythm melts into pitch) | S | exact | At high rpm a single arm becomes a buzz, then a tone, which echoes Stockhausen's *…how time passes…*. Tooth-pass clicks make gears "sing". |
| 10 | **Euclidean and polyrhythm presets** (fill a ring, 3:2, 4:3, 5:4, clave by ratio) | S | exact | A good-sounding machine in one click, and it teaches the ratio idea. |
| 11 | **Cams with followers** | M | exact | A profile on a cog drives a lever that both triggers and modulates continuously (brightness, damping), so a groove gets an accent shape. |
| 12 | **Pendulum and escapement clock source** | M | exact | Period comes from length (`2π√(L/g)`). Ligeti's *Poème symphonique* and pendulum waves make polyrhythm from lengths alone. |
| 13 | **Oval (non-circular) gears for swing** | M | exact | Swing becomes a *shape*: a pair of elliptical gears speeds and slows the output twice per turn. |
| 14 | **Hand crank, touch-to-brake and time scrub** | M | live; scrub is exact | Kinesthetic performance. Scrubbing is uniquely cheap here because `θ(t)` is closed-form. |
| 15 | **Marble run with lift** (analytic ballistics) | L | exact | It is the Wintergatan Marble Machine. Ramps and bars are segments, so flight and roll times are quadratics: no integrator, still sample-accurate. |

### Suggested alpha order

1. **Geometry is the instrument** (all S): strings and bars tuned by length with pluck-point timbre (#1), comb (#7), belts (#2), visual flapper (#3a), tempo lock and phasing (#4), presets (#10).
2. **Keep and share**: URL share, then offline WAV render (#5). Prove the render matches real time by comparing event logs.
3. **Infrastructure P1 and P2 below**: piecewise-`w` advance and an event→action bus.
4. **Chain reactions without physics** (#6): trip pads, clutches, step cogs and section cogs. This is the Rube Goldberg identity in deterministic form.
5. **Mechanisms**: cams (#11), Geneva, escapement and pendulum (#12), oval gears (#13), worm.
6. **Performance** (#14): crank, brake and scrub, with an input log so performances replay.
7. **Free bodies, analytic**: marbles (#15), dominoes, then the flapper rattle. Real dynamics (XL) only if something analytic can't express it.

### Infrastructure that unlocks many ideas

- **P1. Piecewise `w`.** Each component carries a list of `(t, w)` breakpoints. `advance()` splits a step at each breakpoint and integrates `θ` across it. Needed by clutches, shifts, brakes, cranks, spring motors and Geneva-like followers (or those use an angle map instead; see P3).
- **P2. Event→action bus.** A strike can emit `{t, source, v}` to listeners such as a latch, clutch, step cog or spawner. Every listener acts at the exact sim time `t` and is deterministic.
- **P3. Angle maps.** An edge type whose output angle is `f(θin)` rather than `k·θin` (Geneva, oval gears, cams, cranks). The sweep then needs `f` to be monotone, or split at its extrema.
- **P4. Retractable strikes.** Strikes are keyed by sim time, so a live edit can send `cancel after t` to the worklet. That removes the lookahead latency penalty from live gestures and from edits made while running.
- **P5. Seeded PRNG.** `hash(dotId, revolution)` gives probability, humanise and variation, and replays identically.
- **P6. Input log.** Gestures are timestamped in sim time and stored with the machine. Live performances then become exact replays and can be rendered offline.

---

## All ideas, grouped

Each entry gives what it is, then why it is interesting, then cost and timing.

### Mechanisms

- **Belt or chain**: links two cogs anywhere with a drawn loop.
  - Why: same-direction rotation, long reach, and a visible moving chain. Chain links passing a reader could even trigger like dots.
  - Cost: S, a solver edge with `+rA/rB`; a crossed belt uses the negative ratio. Timing: exact.
- **Worm gear**: one turn of the worm advances the wheel one tooth.
  - Why: a 1:40 reduction in a small space gives the slow "section" and "phrase" cogs that structure needs.
  - Cost: S, an edge with ratio `1/n`. Timing: exact.
- **Rack and pinion, slider-crank, linkage**: a cog drives a sliding bar or a crank-rocker arm.
  - Why: reciprocating motion makes back-and-forth strikes, where outbound and return give two velocities and therefore an accent pattern.
  - Cost: M (P3), closed-form kinematics. A four-bar linkage is closed-form too. Timing: exact.
- **Cam and follower**: an editable radius profile on a cog, with a follower lever riding it.
  - Why: the follower crossing a threshold triggers, and its height is a continuous control signal (filter, level, a pad's position).
  - Cost: M (P3). Timing: exact.
- **Geneva drive**: continuous input becomes quarter-turn steps with dwells.
  - Why: rests and stepped sequencing. A disc on a Geneva output plays one column, then waits.
  - Cost: M, a piecewise angle map. Timing: exact.
- **Ratchet and pawl**: a one-way wheel with a clicking pawl.
  - Why: each tooth pass is a click whose rate is `n·rpm/60`. It gives hat rolls at low speed and a pitched zipper at high speed, and only one direction sounds.
  - Cost: S as a sound source; M as a one-way coupling (P1). Timing: exact.
- **Escapement**: a pendulum or balance releases the train one tooth per swing.
  - Why: the clock's tick-tock is a rhythm that a physical length sets, not an rpm.
  - Cost: M. Small-angle motion is analytic; the train then becomes stepped (P1 or P3). Timing: exact.
- **Clutch**: a coaxial joint that engages or disengages.
  - Why: mute and solo by mechanism. A disengaged cog coasts or stops (choose one; stopping is exact).
  - Cost: S in `solve()`, where an engaged clutch is a coax edge. Toggling at run time needs P1. Timing: scheduled, or live if clicked.
- **Gearbox and shifter**: one input and several output ratios, with a selected gear.
  - Why: changes tempo or meter between song sections; shift to ×3/2 for a triplet section.
  - Cost: M (P1). Shifts quantise to the next revolution of a master cog to stay on grid. Timing: scheduled.
- **Differential**: output speed is `(wA + wB)/2`.
  - Why: tempo arithmetic. Two near-equal motors make a very slow beat-frequency output, a built-in LFO or phrase clock.
  - Cost: M. `solve()` turns from ratio propagation into a small linear system per component. Timing: exact.
- **Planetary set**: sun, planets and ring, with any one held.
  - Why: three ratios from one assembly and hypnotic motion. Arms on the planets trace epicycles and so hit pads at varying radii and speeds.
  - Cost: M–L. It is the differential solver plus the planet carrier pose. Timing: exact.
- **Oval and eccentric gears**: non-circular pitch curves.
  - Why: swing and groove from shape. Eccentric mounting gives push-pull feel, and a heart-shaped gear gives a limp.
  - Cost: M (P3); the mesh condition for non-circular pairs is the hard part, so ship pairs designed together. Timing: exact.
- **Spring motor and falling weight**: a power source that runs down.
  - Why: the machine slowly ritardandos and stops, which is a natural ending. You wind it by hand-cranking.
  - Cost: M (P1). `w(t)` has a closed-form exponential or linear decay. Timing: exact once wound; winding is live.
- **Flywheel and inertia**: a heavy cog smooths speed changes.
  - Why: after a brake or crank the tempo glides back and does not snap.
  - Cost: L, a first-order ODE per component that is closed-form if linear. Timing: scheduled.
- **Governor (centrifugal)**: balls fly out with speed and cap the rpm.
  - Why: visual delight and a natural limiter under feedback.
  - Cost: M–L, needs P1. Timing: integrated, or exact if modelled algebraically.

### Arms, pads and contact

- **Flapper arm**: the arm bends on impact and springs back. This is the owner's request.
  - Why: you can see that a hit happened and how hard. A rubber flapper across a ring of pegs is the roulette-wheel click-down.
  - Cost: (a) S, visual only, an analytic damped oscillation started at the impact with amplitude ∝ `vn`. (b) M, the flapper rattling over repeated pegs: tip deflection shifts the next contact time, which is still closed-form per peg. Timing: (a) exact; (b) exact if each re-strike is solved analytically.
- **Pad radius sets pitch**: a bigger pad is a lower drum or bar.
  - Why: one less number to type, so you tune by size, and geometry stays the instrument.
  - Cost: S, a per-voice option. Timing: exact.
- **Strike position sets timbre**: the contact point on the pad (centre vs rim) weights the modes.
  - Why: centre hits on a drum are boomy and rim hits are bright. The same arm sweeping across a pad gets a timbre arc.
  - Cost: S, because `armHit` already knows the contact point. Timing: exact.
- **Pad on a spring**: the pad recoils and re-hits a stop.
  - Why: flams, drags and buzz rolls from one hit.
  - Cost: M, analytic damped motion with exact re-contact times. Timing: exact.
- **Mallet mass**: a heavier arm tip hits louder and darker, with a longer contact time.
  - Why: a second geometric timbre control.
  - Cost: S, mapping mass to contact duration in `modal()`. Timing: exact.
- **Brush and scrape arm**: the arm *drags* across a reader or pad, not strikes.
  - Why: a continuous friction sound whose loudness follows surface speed.
  - Cost: M, a sustained voice whose parameters are scheduled. Timing: exact.

### Sound sources

- **Plucked string between two points**: a segment with pitch `∝ 1/length` and partial `k` weighted by `sin(kπu)` at pluck point `u`.
  - Why: this is literal string physics, already supported by the `harmonic` body, so you tune by stretching. Plucking near the bridge is twangy and plucking in the middle is round.
  - Cost: S. It is a reader with derived pitch, and `dotCross` already computes `u`. Timing: exact.
- **Tuned bar or marimba key**: a bar whose pitch comes from its length (`∝ 1/L²`), using the `bar` body.
  - Why: a vibraphone you lay out by hand, and the target for marbles.
  - Cost: S. Timing: exact.
- **Music-box comb**: N tines along a bar, each tuned. A disc's pegs pluck whichever tine their radius meets.
  - Why: instantly readable, and the disc becomes a music-box cylinder.
  - Cost: S. Timing: exact.
- **Tooth-pass voice**: a gear meshing or a pawl emits a click per tooth.
  - Why: gears "sing" when spun up, and rhythm and pitch are one axis.
  - Cost: S–M. Event density gets high, so above a threshold it switches to an oscillator at `n·rpm/60`. Timing: exact.
- **Hurdy-gurdy wheel**: a rosined cog rubbing strings, giving a sustained bowed tone whose amplitude follows rim speed.
  - Why: a drone layer under the percussion, and the drone's swell follows the machine.
  - Cost: M–L, a continuous bowed-string voice in the worklet. Timing: exact.
- **Resonator tubes and bottles**: a pad that excites a tuned tube.
  - Why: a soft, breathy colour.
  - Cost: M, a waveguide voice. Timing: exact.
- **Sympathetic coupling**: struck pads excite nearby pads tuned near a harmonic, scaled by distance and tuning match.
  - Why: the machine "rings" as a whole. Placing pads close together becomes a sonic decision.
  - Cost: M. The extra quiet strikes are scheduled at the same time as the source strike. Timing: exact.
- **Audio-rate gears**: raise the rpm cap and let arms hit at audio rate.
  - Why: rhythm continuously becomes pitch, which is a "wow" moment.
  - Cost: S. It needs an event cap per pad (switch to a tone voice); `advance()` step count scales with `maxW`. Timing: exact.
- **Listener (microphone) object**: a draggable point that sets pan, distance attenuation and distance delay.
  - Why: walking the mic around the machine is a mix gesture, and machines feel spatial.
  - Cost: S–M. Today pan derives from `view.x`, so this replaces it. Timing: exact.

### Free bodies and chain reactions

- **Marbles on tracks**: polyline ramps with gravity along them. Time per straight segment is a quadratic, and impacts at the end are exact.
  - Why: the Rube Goldberg and marble-machine fantasy, kept deterministic.
  - Cost: L. Analytic only on straight segments; curves become short chords. Timing: exact.
- **Ballistic flight and bounce**: a ball leaves a ramp or is flicked by an arm, follows a parabola and bounces off bars (segments) with restitution.
  - Why: launch speed comes from the arm tip speed, so the machine "throws" notes, and bounce trains give natural accelerando.
  - Cost: L. Parabola–segment intersection is a quadratic. Avoid circles as targets, because parabola–circle is a quartic. Timing: exact.
- **Lift, hopper and spawner**: a gear-driven conveyor carries marbles back to the top.
  - Why: it closes the loop, so a one-shot chain reaction becomes a *periodic* piece. This is the essential pairing for marbles.
  - Cost: M, kinematic, with spawn times derived from conveyor phase. Timing: exact.
- **Dominoes**: a row topples with a delay set by spacing, and each fall is a click.
  - Why: a visible fuse and an accelerating or decelerating run.
  - Cost: M, a closed-form delay per gap. Resetting by sweeper arm gives a loop. Timing: exact.
- **Trip pad, latch and relay**: a hit on a pad *does something*: it releases a weight, engages a clutch or spawns a ball.
  - Why: it is the core Rube Goldberg grammar of causal chains, kept fully discrete.
  - Cost: M (P2). Timing: scheduled.
- **Step cog (hit-driven ratchet)**: a cog that advances one tooth per hit and is not motor-driven.
  - Why: counters and clock dividers ("every 4th kick advances the bassline disc one step"), so sequencing is built from mechanism.
  - Cost: M (P2 + P1). Timing: scheduled.
- **Free tokens you drop on the machine**: pegs placed on a spinning disc ride along and become dots, and loose weights placed on a lever tip it.
  - Why: this answers the owner's "free objects you move around". Pegs ride kinematically, and tokens on levers are discrete states.
  - Cost: S for pegs; M for levers. Timing: exact, or live while dragging.
- **Rigid-body sandbox**: true physics for balls, flappers and stacks.
  - Why: maximum Rube Goldberg chaos.
  - Cost: XL. Timing: integrated. Determinism is fragile, and offline render must re-run the identical integrator. Do it last, if ever.

### Modulation and groove

- **Tempo lock**: global BPM, with motors set in revs per bar.
  - Why: machines stay musically on grid when edited.
  - Cost: S. Timing: exact.
- **Phasing**: a fine detune on a motor.
  - Why: Reich-style phase music from two identical loops.
  - Cost: S, finer rpm steps and a "detune %" control. Timing: exact.
- **Swing from geometry**: oval gears (see Mechanisms), or a "swing" ring that offsets every 2nd dot slot.
  - Why: groove you can see.
  - Cost: S for slot offset; M for oval gears. Timing: exact.
- **Speed linked to anything**: a cam, a differential or a slow cog's angle modulates a motor's rpm.
  - Why: tempo drift, accelerando and breathing.
  - Cost: L (P1), with a closed-form `w(θ)` only for simple links. Timing: exact if analytic, otherwise integrated.
- **Probability dots**: each dot has a chance to sound, seeded by `hash(dot, revolution)`.
  - Why: variation without losing replay.
  - Cost: S (P5). Timing: exact.
- **Peg size and velocity per dot**: a longer peg plucks deeper and louder.
  - Why: accents stay a physical property.
  - Cost: S. Timing: exact.
- **Humanise**: seeded micro-offsets per dot (a bent peg).
  - Why: life.
  - Cost: S. Timing: exact.
- **Polyrhythm presets**: 3:2, 4:3, 5:4, 7:4 and son clave as gear pairs or arm counts.
  - Why: teaching plus instant joy.
  - Cost: S. Timing: exact.
- **Euclidean fill**: put k dots evenly into n slots on a ring, with rotation.
  - Why: fast, musical pattern entry.
  - Cost: S. Timing: exact.

### Composition and structure

- **Multiple discs on one train**: already possible. Add a "disc library" to swap a disc like a record or cylinder.
  - Why: song parts as objects.
  - Cost: S. Timing: exact.
- **Section cog**: a very slow cog (via worm) whose dots trigger clutches and shifts.
  - Why: the machine plays its own arrangement: intro, groove, break.
  - Cost: M (P1, P2). Timing: scheduled.
- **Mute and solo by clutch**: clicking a clutch is mute, and a "solo" disengages every other train.
  - Why: the mix is mechanical and visible.
  - Cost: S–M. Timing: scheduled or live.
- **Score view**: an unwrapped timeline of every event source for N bars.
  - Why: you see what the machine will play before it does. This is possible only because the timing is exact.
  - Cost: M, by running `advance()` on a clone without audio. Timing: exact.
- **Loop length readout**: the LCM of all revolution periods, "the machine repeats every 37.5 s".
  - Why: an honest answer to "what is this piece", and it also gives the export length.
  - Cost: S, using rational rpm. Timing: exact.

### Expression and performance

- **Hand crank**: drag around any cog to drive its train, which overrides the motor.
  - Why: the most kinesthetic thing possible; you *play* the machine.
  - Cost: M (P1, P6, ideally P4). Timing: live.
- **Touch-to-brake**: hold a cog to slow it, with motor slip.
  - Why: tape-stop and drag effects, and tension and release.
  - Cost: M (P1). Timing: live.
- **Time scrub**: pause, then drag a time slider. Cogs pose at any `t`, and audio optionally plays crossings in the scrub direction.
  - Why: audible scrubbing, like scratching a music box.
  - Cost: M. Pose is closed-form, and events come from sweeping between scrub positions. Timing: exact.
- **Live pad nudging**: drag a pad in and out of an arm's path while it runs. It already works; a "performance lock" mode that freezes structure would make it safe.
  - Why: contact radius equals velocity, so a drag is a dynamics fader.
  - Cost: S. Timing: live.
- **MIDI in and out**: emit timestamped notes, map knobs to rpm, and follow a MIDI clock.
  - Why: cogs as a sequencer inside a real setup.
  - Cost: M. Timing: exact out (with a timestamped send); live in.

### Feedback: sound pushes mechanics

- **Impulse feedback**: every strike delivers a torque kick to a flywheel train.
  - Why: busy patterns accelerate themselves into a self-oscillating groove, and a governor caps it.
  - Cost: L. Strikes are known in sim time, so this is P1 with event-driven breakpoints, deterministic and *not* driven by audio analysis. Timing: scheduled.
- **Sail and vane**: loudness, computed from the strike schedule, pushes a vane cog.
  - Why: the room "blows" the machine.
  - Cost: L. Timing: scheduled.
- **Resonance trigger**: a pad whose sympathetic ring passes a threshold trips a latch (P2).
  - Why: chords that cause events, so harmony becomes logic.
  - Cost: M. Timing: scheduled.
- **Mic input**: a real-world sound brakes or kicks the machine.
  - Why: clap to wind it up.
  - Cost: M. Timing: live.

### Sharing and play

- **Share by URL**: the deflated `serialize()` goes in the URL hash.
  - Why: frictionless sharing of tiny machines.
  - Cost: S (`CompressionStream`). Timing: exact.
- **Offline render to WAV and loop export**: render exactly one LCM loop.
  - Why: a seamless loop file for a DAW.
  - Cost: M (`OfflineAudioContext` with the same worklet). Timing: exact.
- **Realtime recording**: `MediaRecorder` on master, including live gestures.
  - Why: catches performances.
  - Cost: S. Timing: live.
- **Video or GIF of the machine**: canvas `captureStream` plus audio.
  - Why: social sharing, where the machine is the visual.
  - Cost: S–M. Timing: live.
- **Presets gallery**: Wintergatan, *Piano Phase*, Ligeti metronomes, music box, clock shop, Geneva step sequencer.
  - Why: an onboarding showcase.
  - Cost: S each. Timing: exact.
- **Challenge mode**: hear a target rhythm, then build it from a limited parts bin (only these cogs, no motor tweaks).
  - Why: ratio puzzles with an ear-training payoff. It is checkable exactly against the target event list.
  - Cost: M. Timing: exact.
- **Remix lineage**: a shared URL carries its parent hash.
  - Why: a remix chain.
  - Cost: S. Timing: exact.

### Delight and polish

- Tooth-meshing sparkle and per-hit particle bursts sized by `vn`.
- A velocity heat colour on arms and pads, so you see where the loud hits are.
- Motor "hum" and a bearing creak at very low rpm.
- An idle "wind down" when the tab loses focus, not a cut; return with "wind up".
- The jam shows as a shudder animation plus a grinding sound rather than a red cog.
- Undo and redo, which is cheap because `serialize()` exists.

---

## Deliberately ranked low

- **True rigid-body physics (XL)**: it fights the exact-timing thesis. Analytic marbles, dominoes and flappers deliver about 80 % of the Rube Goldberg feel and keep offline render, scrub and share-by-URL exact.
- **Audio-analysis-driven feedback**: it is non-deterministic and latency-bound. Feed back from the strike schedule instead.
- **Free-form curved tracks for ballistics**: parabola-against-curve intersections are numerical. Keep tracks as polylines.
