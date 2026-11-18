# cogs: the touch design

One idea carries the design: **you hold the machine in your hand, and the closer you look, the more of it you can touch.** The workbench shows a shelf of parts, one small balance-wheel glyph, and the machine itself. Selecting a disc or a synth reveals a notes or patch button; it opens a labelled touch editor without entering the dev layer. Pressing a control shows **option: setting** across the bottom, updates it while adjusting, and keeps it for 1.8 seconds after release. The parts shelf has no shortcut letters.

## The model

| Verb | Touch |
|---|---|
| **Create** | Drag a part up from the **shelf** (the bottom strip) into the machine. The shelf holds a cog, a disc, an arm, four pads (kick, snare, hat, bell) and a reader. The part sits under your finger, live: a cog meshes as soon as it comes within reach, and a cog dropped on an axle stacks onto it. An arm attaches to whichever cog it is dropped on, and its tip lands where you let go. A reader dropped on a disc lies along its radius. |
| **Delete** | Drag the part back onto the shelf. While you carry a part, the shelf glows as a scrap well. Everything comes from the shelf and everything goes back to it. |
| **Move** | Drag the body. A cog brings its axle stack and clicks into mesh, with a haptic tick. |
| **Grab** | Put one finger on a part and a second anywhere, and it is in your hand: the midpoint of your fingers moves it and spreading sizes it, both at once. Twisting does nothing: an angle is timing, so a grab never changes one. A second finger can land mid-drag, and lifting one leaves the other dragging. Works on cogs, discs, pads, arms, readers, strings and FX fields. A seated reader stays seated and stretches along the radius. |
| **Size a cog** | Grab it and spread or pinch. The cog gains or loses teeth one at a time, keeps its meshes and never overlaps a neighbour. With a mouse, pull the **rim** outward or inward; on a trackpad, pinch over the cog. A finger on the rim just moves the cog. |
| **Set phase** | Pause and select the plate: its **knob** is now the phase wheel. Drag it round the hub to turn the plate; its teeth hold any mesh and it catches at 0. |
| **Set speed** | Select the **motor**: a **knob** (a ringed circle) shows on its face above the hub, or just above its rim when the cog is small on screen. Drag the knob round the hub to turn the speed along the dial; let go within 6% of a detent and it settles there. |
| **Scrub** | Select **any other cog** and drag its knob round the hub, like a hand on a turntable. The train is pulled elastically to the speed of your drag (hold still and it stops under your hand), and let go it springs back to the speed its motor sets, with a little give. A train with no motor runs down to a stop. A scrub is not an edit: it is never saved or undone. |
| **Arm** | Tap anywhere along the arm to select it. Drag the blued tip to swing the arm and change its length; drag the shaft to swing it and keep its length. The quarter nearest the hub is left to the cog. |
| **Pad** | Drag to move. Drag the rim to change the contact radius. Tapping selects it, silently. |
| **Tune a patch** | Select a pad or a reader and a **bezel** blooms around it: a ring of sectors, one per parameter, each with a glyph and a brass value arc. Drag up or down from a sector to turn it. Stepped values such as notes click by in detents with a haptic tick. Edits are silent. A choice, such as the kind or the body, steps forward when tapped. |
| **Write notes** | Select a disc and tap **notes** (bottom left, beside delete, in thumb reach) for a piano-roll sheet. Tap empty grid to add a one-step note; the last note's length is never remembered. Drag a note to move it. To change a length, tap the note to select it, then drag the tab on its right end; a finger never resizes an unselected note. Hold empty grid, then drag, to box-select every note the box touches; the bar above the roll duplicates, copies, pastes (at the last tapped column) and deletes the selection. The velocity lane under the roll has a bar per note: drag on it to set the notes under your finger (only the selected ones there, if any are). Double tap deletes; two fingers pinch and scroll. The sheet's head is one row of tools in place of a title: the snap, the velocity toggle, duplicate, copy, paste, delete, then undo, redo and close; it stays visible when the sheet is tucked. Under the strip the disc's root, octave, range and scale are compact chips (tap for the native picker) and steps is a − n + stepper. The picker shows just the division (off, step/3, 1/16T, 1/4, bar…) and sets the snap, as FL Studio's: none (free), 1/6 to 1/2 of a step, a step, or a note value from 1/32 through triplets, quarters (a beat), halves and wholes, to a bar; a note value follows the disc's real speed under a bpm lock, else one revolution counts as a bar. Placing snaps the head down to the grid, dragging moves by whole snaps (an off-grid note keeps its offset), resizing puts the end on the grid, and a disc not yet touched writes notes one snap long. The velocity lane is hidden until the bar's velocity toggle shows it. A note sounds while it is placed, pressed, moved to a new pitch or resized, and stops when you lift; each new preview cuts the last. Drag the sheet's grab bar down to tuck it to a strip so the machine is visible and playable, tap or drag it up to bring it back, and drag down again from the strip to close. You can still zoom into the disc and paint directly on its face, once it is selected: tap the disc first. An unselected disc's face moves and grabs like any cog, so reaching for it never sets a pin. |
| **Set synth pitch** | Select a pad or reader and tap **patch**. Pitched pads have a named MIDI note selector; pitched pads and readers have an octave offset from −3 to +3. The offset transposes playback and auditions, not the disc score or comb. |
| **Tune a disc** | When the disc is selected and its bezel fits on screen, the bezel sets the scale, root, divisions and number of rings. Changing lane count never hides it, including at one lane; zoomed-in discs retain access through the notes button. |
| **FX field** | Drag the glowing sphere from the shelf (key `f`). It has no wall and no outline: anything that sounds inside it (a ball, a struck pad, a read pin) is also sent to its effect: **delay**, **reverb** or **distortion**. The send is full in its core and fades to nothing at the rim; **diffuse** sets how much of the radius that fade takes, and also how strongly the distance bends the effect (nearer the centre: longer, darker echoes; a bigger, longer room; harder drive). The glow is drawn with exactly that falloff, and a faint dotted ring shows where the effect's parameters are while sound is in it. Drag its centre to move it, the handle on its rim (shown when selected) to size it; tap for its dial: effect, the effect's knobs, diffuse, return and **takes dry** (how much of the dry it removes at full send; distortion starts at 0.8). Up to eight fields sound at once. |
| **Room** | Pinch out past the whole machine. It shrinks to a medallion, and the **room** bezel appears around it: the reverb's mode and color (tap to step them), wet, size, decay, air (high cut), shimmer, freeze and master. Freeze is a tap-only on/off toggle; boolean panel controls are switches, never sliders. |
| **Pause** | Tap the balance-wheel glyph. It swings while time runs. A long press opens the dev layer, which holds the old numeric panel. |

## Gestures

- **Selection:** on empty ground a slide pans and keeps the selection; hold (0.4 s) then drag to box parts, ringed live; a tap clears. With a selection, copy and paste buttons sit above the part bar (paste lands mid-stage, selected); undo and redo sit mid-row in thumb reach.
- **Seated readers:** a reader on a disc is seated: it moves with the disc and scales when it is resized. Drag its body to slide it around the disc, an end to change its span along the radius; pull it 36 px past the rim to free it. A free reader dragged onto a disc seats there.
- **Ratio snap:** a room-bezel toggle; resized cogs then take only tooth counts in a simple ratio (¼ to 4) to the cog they mesh with.
- **The listener is the camera:** a sound pans across the open stage and fades and darkens off it; zoomed out past the whole machine the mix eases (smoothstep, 80 ms glides) to quieter, low-passed and wetter, full again at the fit.

- One finger on empty space pans; a tap there deselects.
- Two fingers grab when the first lands on a grabbable part. On a group, or a selected part with no grab, they twist it. Otherwise they pinch-zoom and pan, and override the gesture in progress. A part wider than 90% of the view's short side is not grabbed, so the fingers can always zoom back out.
- A grab ignores the first 10 px of spread, so a two-finger drag does not change a part's size by accident.
- Selecting a part makes no sound. Notes do: one sounds when placed on a plate or in the roll, and as a drag takes it to a new pitch or length.
- **Thumb reach:** on the phone every control sits in the two rows over the shelf: part actions at the left, undo, reset and play at the right, MIDI at the right of the upper row. An open editor keeps its tools at the bottom of the sheet.
- A double tap on a part flies to its working zoom: a pad to its bezel, a disc to note size (anchored where you tapped), a cog to fill the view. A double tap at working zoom, or on empty space, flies back out to fit the machine.
- A mouse press on a cog or pad rim, or a field's radius handle, resizes; a finger there moves the part, and only a pinch sizes it. Speed lives on the selected cog's knob, which sits on top and wins over the rim and the face under it.
- Every target is at least 44 px on screen, whatever the zoom. A rim hit band never goes below 12 px.

## The start and 0

- **0 points right:** a plate's 0 is its mark at 3 o'clock, 90° clockwise from vertical. Step 1 of a disc lies on the mark, so with a reader seated at 3 o'clock it sounds on the first step.
- **Angle is timing, teeth are only teeth:** a plate's angle (`th`) carries its mark, dots and arms, and is only ever set by hand. Its teeth are drawn at `th + tooth`. Meshing and resizing set `tooth`, never `th`, so they change no timing, and any number of meshed plates can stand at exactly 0.
- **The start:** every plate saves a start angle (`th0`). The reset button stops the machine and puts every plate on it, stopped trains and loose cogs too, and zeroes the turn counts. Its teeth stay put, so meshes hold. Balls in a box are not part of the start.
- **Setting the global 0:** after a reset the stage is on its start until play. A plate turned there takes that angle as its new start, by its knob or the inspector. Paused mid-song, a turn is temporary and reset undoes it.
- **Zero:** the zero button (`z` on the desk) sets the selected plates and seated readers to 0 and makes that their start. Turning a paused plate by its knob, and sliding a seated reader, catch at 0 within 4° with a buzz. Paused, a selected plate shows a tick at its 0, lit when it stands on it.

## Zoom is depth

| Level | What you see | What you can touch |
|---|---|---|
| **Room** (beyond fit) | The machine as a medallion inside the room bezel | The reverb and the master level |
| **Machine** | Silhouettes, motion, impact ripples | Move parts, and carry parts to and from the shelf |
| **Part** | Teeth, spokes, hit windows of the selected arm or pad | Rims, arm tips, pad rims, reader ends, and the bezels |
| **Patch** | Ring grid and pin sockets on discs, the bezel around a pad | Notes, voices |

The level is not a mode. It is derived from on-screen size: a disc's face becomes the score exactly when its ring spacing reaches 12 px.

## Look

Every icon on a button, in both builds, is from Radix Icons (`src/icons.js`, MIT). The part drawings on the shelf and rail, the balance-wheel glyph and the bezel parameter glyphs are drawings of the machine, not interface icons, and are kept.

Watchmaking, not wireframe.
- **Ground:** dark blued enamel, `#0c1012`, with a two-scale dot lattice that fades between scales as you zoom. The lattice gives the zoom its sense of depth.
- **Cogs:** brass with a cut trapezoid tooth profile, crossed spokes on the larger wheels and a steel pivot. The light is fixed in the world, so the specular band stays still while the wheel turns beneath it. The motor carries a ruby jewel that glows while it runs. A jammed train flushes rust-red and shudders.
- **Discs:** silvered dials with engraved rings and steel pins. They are music-box cylinders laid flat. A pin flares gold when it is read.
- **Arms:** heat-blued steel watch hands with a collet at the hub.
- **Pads:** lacquered enamel buttons in brass bezels: vermilion for kick, bone for snare, verdigris for hat, amethyst for bell. A hit presses the button in, flares it and throws a ripple ring sized by impact speed.
- **Readers:** polished steel tines, held by slotted screws.
- **Motion:** bezels bloom in over 160 ms (`cubic-bezier(.16,1,.3,1)`). Camera flights ease out over 380 ms. Haptics mark mesh, tooth, detent and scrap.

## What replaces each panel control

| Panel | Now |
|---|---|
| tool buttons | the shelf |
| teeth, x, y, layer | grab, rim pull (mouse), body drag, drop onto an axle |
| motor rpm | push the teeth, or flick them |
| phase | follows from meshing; in the dev layer only |
| arm angle, len, r0 | drag the tip; r0 is in the dev layer only |
| pad x, y, radius | body drag, rim drag, grab |
| voice kind and params | the bezel |
| rings, base, scale, div | the disc bezel |
| dots, roll | the pop-up notes editor, or the disc face at patch zoom |
| space (the reverb's 22 knobs and the dry level) | the room bezel exposes 8; the rest are in the dev layer |
| master | the room bezel |
| zoom | pinch and double tap |
| audio | the first touch anywhere |
| pause | the glyph, or Space |
| del | drag to the shelf, or Delete |
| demo, clear, json, stats, tolerances | the dev layer (long-press the glyph, or `?`) |

## Affordances for the alpha features

- **Belts:** a belt sits on the shelf. Drop it on cog A and it stretches like a band from A to your finger. Let go over cog B and it loops. Tap the belt to cross it, which reverses the direction. Drag it to the shelf to remove it.
- **Tempo lock:** BPM is a sector on the room bezel. With tempo lock on, crank detents become revolutions per bar (¼, ⅓, ½, 1, 2…), and the crank gauge draws them as notches with pip counts.
- **Strings and bars:** a string part on the shelf lands as a short span. Drag an end to stretch it: the pitch plays as it changes, with detents on scale tones. The pluck point is where an arm crosses it, drawn as a bright node, so you shape the timbre by moving the arm or the string.
- **Two readers:** the **reader** (key `r`) takes pitch from each pin's ring; the **comb** (key `k`) takes pitch from where the pin crosses it, with its tine count, root note and scale in its patch sheet. They are separate parts on the shelf, not a mode. A new reader or comb plays the synth (the wave synth, kind `wave` in saves, shown as **synth**) on its pluck preset; the older modal synth is shown as **modal**, and its patch sheet has a button to switch to the synth. The demo's two melodic readers play the synth's reese bass and pluck. The modal synth's five bodies are tuned: a mode off the harmonic series sits on the nearest equal-tempered interval (the bell is a minor third, fifth, octave and major tenth over its fundamental), and partials past the measured modes continue on the harmonic series, so every body plays in tune.
- **Probability per dot:** long-press a pin and a small ring blooms around it. Drag around that ring to set its odds; the pin becomes a pie of that fraction.
- **Euclidean fill:** long-press an empty socket on a ring, then sweep around the ring. Pins spread evenly, one more for each step you sweep.
- **Flapper arms:** this needs no control. The blued hand bends on impact, in proportion to the hit, and springs back.
- **Share by URL:** a seal sits at the foot of the room bezel. Tap it to copy the link; the seal stamps itself.

## Desktop

`desk.html` drops the shelf, glyph and bezels. A toolbar sits on the right edge: an always-visible rail of labelled buttons with their keys (panel `?`, the seven parts `c d a b p t r`, select `s`, undo, redo, pause, fit, delete), and a panel that slides out of it and scrolls: transport, the inspector for the selection, world and tempo, space, stats, json. Every value is a slider with its number: wheel nudges by 1% of the range, shift-wheel by one step, shift-drag moves ten times finer, double-click resets to the default. Selecting a disc docks its roll at the bottom: time left to right, rings as rows, click or drag to paint pins. As in FL Studio, a note's right edge resizes it without selecting it first (the cursor turns ↔ and a grip tab shows); unlike FL, a new note is always one step, not the last note's length. In the roll, ctrl/cmd-drag boxes pins and ctrl/cmd-click toggles one; with a selection, a plain press elsewhere or Esc clears it, a selected pin drags the set (columns wrap, rings stop at the edge), arrows nudge a column or ring, shift+up/down an octave of scale steps, Delete removes, ctrl/cmd+C/X/V copy, cut and paste at the last clicked column (else the playhead), ctrl/cmd+A takes every pin, and the brush sliders set the selection. While the roll has focus or a selection the part keys stay off the canvas. The canvas keeps the phone's mouse gestures: the speed knob, rim resize, trackpad pinch over a cog to size it, note painting at note zoom, wheel zoom at the cursor, ctrl-drag marquee, ctrl-click toggle, group drag, Delete. Dragging a part onto the toolbar scraps it.

## Undo

Both builds keep the last 500 world states. A step is one finished action: it settles when the pointer lifts, or after 350 ms without edits, so a drag, a crank, a paint stroke, a slider drag or a burst of wheel nudges is one step and a no-op is none. Camera, pause, selection and cog phase are never undone; undo puts the parts back and each cog takes the phase it would have had, so the train keeps turning in time. Ctrl/Cmd+Z, Ctrl/Cmd+Shift+Z and Ctrl+Y, or the undo and redo buttons.

## The seam

`cogs.html` (phone) and `desk.html` (desktop) are build outputs: `node cogs/build.js`, which `make cogs` runs first, inlines `cogs/src` into each, because the artifact sandbox loads one self-contained file. Edit `src/`, never the outputs.
- `engine.worklet.js` and `sim.js`: the audio worklet, the world, solver, audio, clock, persistence, demo and share.
- `stage.js`: what both builds draw and do on the canvas: look, render, gestures, keys, the roll, the panel widgets and inspector, the frame loop.
- `phone.js` / `desk.js`: each build's chrome. Each defines the hooks the stage calls: `onResize`, `stage`, `offStage`, `chromeHit`, `drawChrome`, `reveal`, `panelShown`, `inspectorLive`, `togglePanel`, `placeRoll`, `mount`, `PROBE`.

## Wave synth

A pad or reader voice of kind `wave` is a Serum-style synth (`wave.spec.js` registry, `wave.worklet.js` voices, `wave.js` patches and library, `wave-ui.js` editor). Three wavetable oscillators whose tables are built additively, each with its own amp envelope and its own filter with a filter envelope; sub and noise on ENV1; three envelopes, four LFOs and eight macros route to any knob. In the editor, tap a source chip to arm it and drag a knob to set its depth (double tap removes), or drag a chip onto a knob. The editor opens on MAIN: the eight macros and the knobs pinned to the part's dial (or a quick set: A cutoff, resonance, attack, release, detune, volume). Tabs in one row: MAIN, A, B, C, VOICE (pitch, volume, glide, voicing, sub, noise), MOD (envelopes and LFOs), FX, MATRIX, PATCH. The head holds ◀ patch ▶, play and a Bluetooth MIDI button; there is no on-screen keyboard. The part's own settings (kind, level, vel sense, note) fold under it. While a note sounds, each routed knob shows a dot where its modulation has it, reported by the worklet 30 times a second for the open patch. FX modules that are off fold to their name; a matrix route is one card, source → knob over its depth, with ⋯ for bipolar, bypass, aux and curve. Groups are filled cards, not hairlines. The patch tab saves to a library in local storage, loads factory presets, and exports and imports JSON.

**One file.** The settings sheet (hold the glyph; the desk's panel) has **save file** and **open file**. A `.cogs.json` file holds the whole machine (parts, their patches, the settings) and the patch library. Opening one checks the machine whole before replacing it (one undo step), then merges the library: a patch already there is skipped, a different one under a taken name is kept as "name (2)". Open file also takes an old machine export or a single synth patch. `SYNTH-PARITY.md` tracks parity with Serum 2.

## MIDI keyboard

A keyboard plays the synth of whatever is selected: a reader, a pad, a box, or a plate (which means its reader; selecting one of its pins counts too). With nothing that has a synth selected it keeps playing the last one. The phone always shows a **MIDI** connect button above the part bar (it takes the keyboard's name once connected), with a record button beside it when the selection is on a plate; the desk has **keys** (`j`) and **record** (`n`) on the rail; both have a **keyboard** section in the panel with Bluetooth MIDI and MIDI in. Connect goes straight to Bluetooth on a touch device; on a computer it takes what is plugged in, then Bluetooth.

**Recording** writes onto the selected plate through its reader, or onto the plate under the selected reader; the phone's notes sheet has the button too, to watch the notes land in the roll. Record presses play if the machine is paused. Each key lands as a pin where the reader was when you heard yourself play it, so it comes back round in time. It snaps to the roll's snap, or stays as played with the snap off; its length is how long the key (or the sustain pedal) held, up to the whole plate; its velocity is how loud that key sounded. While the take runs the keyboard stays on that synth, a red ring marks the plate, a pin does not sound until you have let go of its key, and one snapped just ahead of the reader skips the crossing you have just played. A take adds to what is there, replacing a pin in the same place, and is one undo step.

The plate makes room: an empty one moves to the octave you play in, and otherwise its range grows (to three octaves, downward too), with existing pins keeping their pitch. A note past that folds by octaves, a note off the plate's scale takes the nearest one in it, and a comb or a short reader gets the nearest note it can reach; the take reports how many moved. Paused or on a plate standing still, nothing is written and the status says why.

## Ball pit (the box)

Monad's container of balls, as a part: **box** on the shelf (key `o`, type `pit` in saves). Tap inside a box to drop a
ball, drag to throw one the way the finger went; on a ball, a tap flicks it, a drag throws it, a hold takes it out.
Drag the wall to move the box with its balls; the corner of a selected box sizes it. Each ball is a note: a new one
takes the next note of the box's palette (a shuffled bag, every note before one repeats), and its size from its
pitch, low notes big. A ball sounds when it hits a wall or another ball, harder for a harder hit, on the box's voice
(the synth by default, or the modal, or a drum). Arms sweeping through the box bat the balls and cogs in it shove them
along their rims; those contacts are silent. The box's sheet has the palette as a two-octave keyboard (lit keys are
its notes, in each note's ball colour) above the voice; a note taken off moves its balls to the nearest one left.
Balls step at 240 Hz on the sim's clock, ahead of the picture by the lookahead, and are drawn along a short trail at
the heard time; a dropped ball shows at once where it was dropped. Ball positions are not undo steps: an undo or redo
keeps the balls in the box where they are, and one that comes back returns where it was.

## Reverb

The room and the synth's FX rack share one reverb, `src/verb.js` (class `Verb`), a recreation in spirit of Valhalla
VintageVerb: a predelay; an input low and high cut; an early diffuser of six allpasses whose blend sets the echo density
from sparse to dense (Early Diffusion); a swell of two long allpasses (Attack); a tapped early-reflection line
(EarlyLateMix balances it against the tail); and a ring of 4 or 6 stages (a modulated allpass, a second allpass whose gain
is Late Diffusion, a delay and a three-band decay: Bass Mult below Bass Xover, the High Shelf in dB above High Freq), read
by output taps in each stage. Twenty modes are rows of lengths, taps and a modulation recipe: concert hall, bright hall,
plate, room, chamber, random space (delay randomisation: no pitch wobble), chorus space, ambience, sanctuary, dirty hall
and plate (saturating), smooth plate, room and random, nonlin (Size is the gate's length, Attack bends it from a cut-off
decay through flat to reverse), chaotic hall, chamber and neutral (logistic-map wow and flutter), cathedral and palace.
Three colors: **1970s** runs the ring at half the sample rate inside 10 kHz with noisy modulation, **1980s** is full band
with the noisy modulation, **now** is full band and clean. A short decay shrinks a big mode's ring, so a stage never loses
more than 5 dB a pass and comes round again as a repeat. Mode and color changes fade the old tail out over a block and
rebuild in silence. Params are one flat object of numbers (mode and color as indices), so they post across the port;
`VERB_DEFS`, `VERB_MODES`, `VERB_COLORS` and `VERB_PRESETS` are read on the page too. The engine mixes the dry signal
itself and runs the reverb only while something sounds or a tail rings (`energy`, `holdBlocks`). A save from before
this reverb is mapped across (`migrateSpace`).

