# cogs: the touch design

One idea carries the design: **you hold the machine in your hand, and the closer you look, the more of it you can touch.** The workbench shows a shelf of parts, one small balance-wheel glyph, and the machine itself. Selecting a disc or a synth reveals a notes or patch button; it opens a labelled touch editor without entering the dev layer. Pressing a control shows **option: setting** across the bottom, updates it while adjusting, and keeps it for 1.8 seconds after release. The parts shelf has no shortcut letters.

## The model

| Verb | Touch |
|---|---|
| **Create** | Drag a part up from the **shelf** (the bottom strip) into the machine. The shelf holds a cog, a disc, an arm, four pads (kick, snare, hat, bell) and a reader. The part sits under your finger, live: a cog meshes as soon as it comes within reach, and a cog dropped on an axle stacks onto it. An arm attaches to whichever cog it is dropped on, and its tip lands where you let go. A reader dropped on a disc lies along its radius. |
| **Delete** | Drag the part back onto the shelf. While you carry a part, the shelf glows as a scrap well. Everything comes from the shelf and everything goes back to it. |
| **Move** | Drag the body. A cog brings its axle stack and clicks into mesh, with a haptic tick. |
| **Size a cog** | Pull the **rim** outward or push it inward. The cog gains or loses teeth one at a time. |
| **Set speed** | **Push the teeth** along the rim, the way you would turn a real wheel. The whole train turns with your finger while you crank. A flick leaves it spinning at the flick's speed, snapped to a detent. Hold still and let go to stop it. The cog you last pushed becomes the train's motor, so two motors can never fight. |
| **Arm** | Drag the blued tip to swing the arm and change its length. |
| **Pad** | Drag to move. Drag the rim to change the contact radius. Tap it to hear it. |
| **Tune a patch** | Select a pad or a reader and a **bezel** blooms around it: a ring of sectors, one per parameter, each with a glyph and a brass value arc. Drag up or down from a sector to turn it. Stepped values such as notes click by in detents with a haptic tick. Edits are silent: tap the pad to hear it. A choice, such as the kind or the body, steps forward when tapped. |
| **Write notes** | Select a disc and tap **notes** (bottom left, beside delete, in thumb reach) for a piano-roll sheet. Tap empty grid to add a one-step note; the last note's length is never remembered. Drag a note to move it. To change a length, tap the note to select it, then drag the tab on its right end; a finger never resizes an unselected note. Hold empty grid, then drag, to box-select every note the box touches; the bar above the roll duplicates, copies, pastes (at the last tapped column) and deletes the selection. The velocity lane under the roll has a bar per note: drag on it to set the notes under your finger (only the selected ones there, if any are). Double tap deletes; two fingers pinch and scroll. A note sounds while it is placed, pressed, moved to a new pitch or resized, and stops when you lift; each new preview cuts the last. Drag the sheet's grab bar down to tuck it to a strip so the machine is visible and playable, tap or drag it up to bring it back, and drag down again from the strip to close. You can still zoom into the disc and paint directly on its face. |
| **Set synth pitch** | Select a pad or reader and tap **patch**. Pitched pads have a named MIDI note selector; pitched pads and readers have an octave offset from −3 to +3. The offset transposes playback and auditions, not the disc score or comb. |
| **Tune a disc** | When the disc is selected and its bezel fits on screen, the bezel sets the scale, root, divisions and number of rings. Changing lane count never hides it, including at one lane; zoomed-in discs retain access through the notes button. |
| **Room** | Pinch out past the whole machine. It shrinks to a medallion, and the **room** bezel appears around it: wet, size, decay, air, shimmer, freeze and master. Freeze is a tap-only on/off toggle; boolean panel controls are switches, never sliders. |
| **Pause** | Tap the balance-wheel glyph. It swings while time runs. A long press opens the dev layer, which holds the old numeric panel. |

## Gestures

- One finger on empty space pans; a tap there deselects.
- Two fingers pinch-zoom and pan anywhere, and override any gesture in progress.
- A double tap on a part flies to its working zoom: a pad to its bezel, a disc to note size (anchored where you tapped), a cog to fill the view. A double tap at working zoom, or on empty space, flies back out to fit the machine.
- A press on a rim is classified by the first 8 px of motion: tangential motion cranks, radial motion resizes.
- Every target is at least 44 px on screen, whatever the zoom. A rim hit band never goes below 12 px.

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
| teeth, x, y, layer | rim pull, body drag, drop onto an axle |
| motor rpm | push the teeth, or flick them |
| phase | follows from meshing; in the dev layer only |
| arm angle, len, r0 | drag the tip; r0 is in the dev layer only |
| pad x, y, radius | body drag, rim drag |
| voice kind and params | the bezel |
| rings, base, scale, div | the disc bezel |
| dots, roll | the pop-up notes editor, or the disc face at patch zoom |
| space (20 params) | the room bezel exposes 7; the rest are in the dev layer |
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
- **Reader modes:** **disc notes** takes pitch from each pin's ring; **comb** takes pitch from where the pin crosses the reader. The bezel and patch sheet expose both modes, all four voices and, for modal synthesis, all five bodies. Comb mode reveals tine count, named root note and scale.
- **Probability per dot:** long-press a pin and a small ring blooms around it. Drag around that ring to set its odds; the pin becomes a pie of that fraction.
- **Euclidean fill:** long-press an empty socket on a ring, then sweep around the ring. Pins spread evenly, one more for each step you sweep.
- **Flapper arms:** this needs no control. The blued hand bends on impact, in proportion to the hit, and springs back.
- **Share by URL:** a seal sits at the foot of the room bezel. Tap it to copy the link; the seal stamps itself.

## Desktop

`desk.html` drops the shelf, glyph and bezels. A toolbar sits on the right edge: an always-visible rail of labelled buttons with their keys (panel `?`, the seven parts `c d a b p t r`, select `s`, undo, redo, pause, fit, delete), and a panel that slides out of it and scrolls: transport, the inspector for the selection, world and tempo, space, stats, json. Every value is a slider with its number: wheel nudges by 1% of the range, shift-wheel by one step, shift-drag moves ten times finer, double-click resets to the default. Selecting a disc docks its roll at the bottom: time left to right, rings as rows, click or drag to paint pins. As in FL Studio, a note's right edge resizes it without selecting it first (the cursor turns ↔ and a grip tab shows); unlike FL, a new note is always one step, not the last note's length. In the roll, ctrl/cmd-drag boxes pins and ctrl/cmd-click toggles one; with a selection, a plain press elsewhere or Esc clears it, a selected pin drags the set (columns wrap, rings stop at the edge), arrows nudge a column or ring, shift+up/down an octave of scale steps, Delete removes, ctrl/cmd+C/X/V copy, cut and paste at the last clicked column (else the playhead), ctrl/cmd+A takes every pin, and the brush sliders set the selection. While the roll has focus or a selection the part keys stay off the canvas. The canvas keeps the phone's mouse gestures: rim crank and resize, note painting at note zoom, wheel zoom at the cursor, ctrl-drag marquee, ctrl-click toggle, group drag, Delete. Dragging a part onto the toolbar scraps it.

## Undo

Both builds keep the last 500 world states. A step is one finished action: it settles when the pointer lifts, or after 350 ms without edits, so a drag, a crank, a paint stroke, a slider drag or a burst of wheel nudges is one step and a no-op is none. Camera, pause, selection and cog phase are never undone; undo puts the parts back and each cog takes the phase it would have had, so the train keeps turning in time. Ctrl/Cmd+Z, Ctrl/Cmd+Shift+Z and Ctrl+Y, or the undo and redo buttons.

## The seam

`cogs.html` (phone) and `desk.html` (desktop) are build outputs: `node cogs/build.js`, which `make cogs` runs first, inlines `cogs/src` into each, because the artifact sandbox loads one self-contained file. Edit `src/`, never the outputs.
- `engine.worklet.js` and `sim.js`: the audio worklet, the world, solver, audio, clock, persistence, demo and share.
- `stage.js`: what both builds draw and do on the canvas: look, render, gestures, keys, the roll, the panel widgets and inspector, the frame loop.
- `phone.js` / `desk.js`: each build's chrome. Each defines the hooks the stage calls: `onResize`, `stage`, `offStage`, `chromeHit`, `drawChrome`, `reveal`, `panelShown`, `inspectorLive`, `togglePanel`, `placeRoll`, `mount`, `PROBE`.
