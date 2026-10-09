# cogs: the touch design

One idea carries the design: **you hold the machine in your hand, and the closer you look, the more of it you can touch.** The screen has no forms and no words. It shows a workbench with a shelf of parts, one small balance-wheel glyph, and the machine itself.

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
| **Tune a patch** | Select a pad or a reader and a **bezel** blooms around it: a ring of sectors, one per parameter, each with a glyph and a brass value arc. Drag around a sector to turn it. A ring gives endless travel, so fine control needs no extra mode. Stepped values such as notes click by in detents with a haptic tick, and every change plays the pad, so you tune by ear. A choice, such as the kind or the body, steps forward when tapped. |
| **Write notes** | Zoom into a disc until its rings are finger-sized. The face then becomes the score: tap a pin socket to set or clear it, or drag to paint along. Further out, the same drag moves the disc. |
| **Tune a disc** | When the disc is selected and fits on screen, a bezel around its rim sets the scale, the root, the divisions and the number of rings. |
| **Room** | Pinch out past the whole machine. It shrinks to a medallion, and the **room** bezel appears around it: wet, size, decay, air, shimmer, freeze and master. |
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
| dots, roll | the disc face at patch zoom; the roll stays in the dev layer |
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
- **Comb reader:** a reader variant with tines. At patch zoom you tune each tine by dragging its tip; its length is its pitch, as on a real comb.
- **Probability per dot:** long-press a pin and a small ring blooms around it. Drag around that ring to set its odds; the pin becomes a pie of that fraction.
- **Euclidean fill:** long-press an empty socket on a ring, then sweep around the ring. Pins spread evenly, one more for each step you sweep.
- **Flapper arms:** this needs no control. The blued hand bends on impact, in proportion to the hit, and springs back.
- **Share by URL:** a seal sits at the foot of the room bezel. Tap it to copy the link; the seal stamps itself.

## The seam

`cogs.html` is edited in place. A second copy of a verified sim would drift.
- **Sim and engine** (the lead's): the worklet, then everything from `/* ---------- world` to `/* ---------- clock`, plus the roll, persistence and demo.
- **UI** (mine): `/* ---------- look`, `/* ---------- render`, `/* ---------- touch`, `/* ---------- dev layer` and `/* ---------- loop`.

The UI touches the sim only through `W`, `S`, `SPACE`, `solve`, `snap`, `stackOf`, `radius`, `ringR`, `renderTh`, `hitWindow`, `changed`, `strike`, plus the `flash` and `hits` fields that the sim already writes. The one change on the sim side splits `fire()` into `fire()` (sim event) and `strike()` (sound), so the bezel can audition a pad without logging a sim event.
