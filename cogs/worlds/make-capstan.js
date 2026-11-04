// Builds cogs/worlds/capstan.json: "Capstan", deep dub house in F minor at 119 bpm. A 192-bar turn of one song wheel.
// node cogs/worlds/make-capstan.js && node cogs/worlds/make-capstan-html.js
//
// Dub house is built from a tape echo, a chord and a bass. This machine builds those out of gears instead of effects:
// - The reel is the tape loop. Its pins are the one recorded chord figure and each reader is a playback head, set
//   round the rim at 3, 7 and 13 sixteenths. So the echoes are geometry, and each head reads its own lanes through its
//   own patch: the echo changes instrument and voicing on its way round.
// - The reel slips. It turns at 255/512 of a bar (48:64 then 85:128) where a true gear would be 256/512, so the whole
//   figure drifts one sixteenth later every 16 bars against a dead straight kick: a Reich phase, locked to the grid
//   again at each 16-bar turn. It has 512 bars to come home; the song wheel only hears 192 of them.
// - The bass plate meshes a mirror plate, which turns the other way: its pins are the bass line in retrograde, two
//   octaves up.
// - Hammers on 15- and 21-tooth pinions knock every 5 and 7 sixteenths. 7- and 11-bar wheels move the filters.
// - A cam a beat ducks the pad: a sidechain with no sidechain, still pumping when the kick has gone.
// - A sphere of balls, batted by an arm on a belt, rains off the grid; one ball's height is the pad's vowel.
// - A 1:192 clock train turns the song wheel, whose cams are every fader. The arrangement is the shape of those cams.
const fs = require('fs'), path = require('path');
const TAU = Math.PI * 2, TOOTH = 12, BPM = 119, STEP = 1 / 16;   // a step is a sixteenth of a bar
const R = n => n * TOOTH / TAU, wrap = a => ((a % TAU) + TAU) % TAU;

const W = {
  S: { bpm: BPM, beats: 4, seed: 11, meshTol: 4, coaxTol: 6, snapTol: 16, vRef: 500, velCurve: .72, master: .74, lookahead: .2, maxStep: .05, zoom: 1, windows: 0, ids: 0, ratios: 0 },
  SPACE: { dry: 1, mode: 0, color: 0, wet: .17, predelay: 24, decay: 3.4, size: .66, attack: .2, early: .6, late: .7, modRate: .25, modDepth: .4, highCut: 6500, lowCut: 260, bassMult: .6, bassXover: 200, highShelf: -8, highFreq: 3500, earlyLate: .6, width: 1.35, shimmer: 0, shimmerPitch: 12, freeze: 0 },
  climax: { on: 1, drive: 2.5, sat: 38, classA: 1, excite: 1.5, air: 1, airFreq: 10000, low: 1, lowFreq: 60, high: -.5, highFreq: 3000, mix: 1, out: 0 },
  view: { x: 0, y: 0 }, cogs: [], arms: [], pads: [], readers: [], belts: [], strings: [], cams: [], pits: [], probes: [], fields: [] };
let nid = 1; const id = () => nid++;
const num = x => +x.toFixed(6);

/* ---------------- gearing ---------------- */
// every plate starts at 0; k is its turns a bar, its teeth are put in mesh by a tooth offset, as interlock() does
const cogs = [];
function cog(n, layer, x, y, o = {}) { const c = { id: id(), n, layer, x, y, k: 0, tooth: 0, motor: null, track: null, ...o }; cogs.push(c); return c; }
function meshAt(a, n, layer, ang) {   // a new cog in mesh with a, at world angle ang from it
  const d = R(a.n) + R(n), c = cog(n, layer, a.x + Math.cos(ang) * d, a.y + Math.sin(ang) * d);
  const onA = (ang - a.tooth) * a.n / TAU;
  c.tooth = wrap(ang + Math.PI - (.5 - onA) * TAU / n); c.k = -a.k * a.n / n; return c;
}
const axle = (a, n, layer) => { const c = cog(n, layer, a.x, a.y); c.k = a.k; return c; };

const M = cog(48, 0, 0, 0, { k: 1, motor: { rpm: BPM / 4, revs: 1 } });       // once a bar
const M2 = axle(M, 24, 2);
// the reel: 48:64, 85:128 is 255/512
const A = meshAt(M, 64, 0, -Math.PI / 2), A2 = axle(A, 85, 1);
const REEL = meshAt(A2, 128, 1, -Math.PI / 2);
// the drums, two bars a turn
const DRUMS = meshAt(M, 96, 0, Math.PI / 2), D1 = axle(DRUMS, 16, 1);
// the prime wheels, 7 and 11 bars a turn
const P7 = meshAt(D1, 56, 1, Math.PI), P11 = meshAt(D1, 88, 1, Math.PI * .2);
// the hammers and the pump
const H5 = meshAt(M, 15, 0, Math.PI), H7 = meshAt(M, 21, 0, 0), PUMP = meshAt(M, 12, 0, .65);
// the bass plate and its mirror, four bars a turn
const BASS = meshAt(M2, 96, 2, -.25), MIRROR = meshAt(BASS, 96, 2, 0);
// the clock train down to the song wheel: 12:48, 12:48, 16:48 off the bass plate is 1:192
const S0 = axle(BASS, 12, 3), S1 = meshAt(S0, 48, 3, Math.PI / 2), S1b = axle(S1, 12, 4), S2 = meshAt(S1b, 48, 4, Math.PI / 3), S2b = axle(S2, 16, 5), SONG = meshAt(S2b, 48, 5, 1);
// the rain wheel, on a long belt from the drums' pinion: three bars a turn
const RW = cog(24, 1, -405, 215); RW.k = D1.k * D1.n / RW.n * -1;
W.belts.push({ id: id(), a: D1.id, b: RW.id, cross: true });

/* ---------------- discs, pins, readers ---------------- */
const SC = { minor: [0, 2, 3, 5, 7, 8, 10], minpent: [0, 3, 5, 7, 10] };
const nRings = t => SC[t.scale].length * t.range;
const ringFrac = (t, i) => nRings(t) > 1 ? .25 + .65 * i / (nRings(t) - 1) : .6;
function disc(c, root, octave, range, scale) { c.track = { root, octave, range, scale, div: Math.round(16 / Math.abs(c.k)), dots: [] }; }
// a pin on disc c that reaches world angle phi at bar t
const pin = (c, phi, ring, t, len = 1, v = 1, p = 1) => c.track.dots.push({ ring, a: +wrap(phi - TAU * c.k * t).toFixed(5), p, v, len });
function reader(c, phi, f0, f1, voice) {
  const r = R(c.n), ux = Math.cos(phi), uy = Math.sin(phi), o = { id: id(), on: c.id, x1: num(c.x + ux * f0 * r), y1: num(c.y + uy * f0 * r), x2: num(c.x + ux * f1 * r), y2: num(c.y + uy * f1 * r), voice, tines: 0, combBase: 60, combScale: 'major' };
  W.readers.push(o); return o;
}
const half = t => .65 / (nRings(t) - 1) / 2;
const span = (c, phi, i0, i1, voice) => { const t = c.track, h = half(t) * .92; return reader(c, phi, ringFrac(t, i0) - h, ringFrac(t, i1) + h, voice); };
const at = (c, phi, f) => ({ x: c.x + Math.cos(phi) * f * R(c.n), y: c.y + Math.sin(phi) * f * R(c.n) });

/* ---------------- voices ---------------- */
const KNOBS = { kick: ['tune', 'sweep', 'pdecay', 'decay'], clap: ['cutoff', 'spread', 'decay'], hat: ['cutoff', 'decay'], rim: ['tune', 'decay'], wood: ['tune', 'decay'], tom: ['tune', 'sweep', 'pdecay', 'tone', 'decay'] };
const drum = (kind, level, velSense, knobs) => ({ level, velSense, ...Object.fromEntries(KNOBS[kind].map(k => [k, knobs[k]])), kind });
const osc = o => ({ on: true, table: 'basic', filter: 'lp24', warpMode: 'bend', warp2Mode: 'off', ...o });
const mod = (src, dst, amt, extra = {}) => ({ src, dst, amt, bi: false, off: false, aux: '', auxInv: false, curve: 0, ...extra });
const GATE = amt => mod('m8', 'vol', amt);   // the song wheel's cam on macro 8 is the part's fader
function wave(name, p) {
  return { kind: 'wave', name, note: 60, octave: 0, semi: 0, level: .7, velSense: .3,
    osc: [0, 1, 2].map(i => p.osc[i] ? osc(p.osc[i]) : { on: false }), sub: p.sub || { level: 0, oct: -1 }, noise: p.noise || { level: 0, color: .6 }, tune: p.tune || {},
    env: p.env || [], lfo: p.lfo || [], macro: [], macroNames: p.names || ['', '', '', '', '', '', '', 'song'], mods: p.mods || [], voicing: p.voicing || { mode: 'poly', always: true }, fx: p.fx || {},
    vol: 0, glide: p.glide || 0, gate: p.gate ?? .2, ...(p.top || {}) };
}
const NAMES = (a, b, c) => [a || '', b || '', c || '', '', '', '', '', 'song'];
// head A: the warm chord. Two saws through a ladder that closes fast; a 7-bar wheel (m1) moves the cutoff, as deep as the song wheel (m2) allows
const headA = wave('head A: warm', { names: NAMES('7 bars', 'open'),
  osc: [{ pos: .667, uni: 3, detune: .16, spread: .7, level: .62, cut: 520, res: .3, drive: .2, famt: .42, fd: .17, fs: .02, a: .002, d: .28, s: 0, r: .22 },
    { table: 'pulse', pos: .1, semi: 12, level: .13, cut: 700, res: .15, famt: .25, fd: .1, fs: 0, a: .002, d: .16, s: 0, r: .15 }],
  mods: [GATE(.86), mod('m1', 'osc1.cut', .16, { aux: 'm2' }), mod('m2', 'osc1.cut', .2), mod('m2', 'osc2.cut', .2)],
  fx: { chorus: { on: true, rate: .22, depth: .55, mix: .3 } }, gate: .18, top: { level: 1.2 } });
// head B, 3 sixteenths on: the lower voices only, as a stopped organ pipe, dark
const headB = wave('head B: pipe', {
  osc: [{ table: 'organ', pos: .3, level: .7, filter: 'lp12', cut: 620, res: .1, a: .004, d: .2, s: 0, r: .16 }],
  mods: [GATE(.8)], gate: .12, top: { level: 1 } });
// head C, 7 sixteenths on: the middle voices a fifth up, a closed vowel moved by the 11-bar wheel
const headC = wave('head C: vowel', { tune: { semi: 7 }, names: NAMES('11 bars'),
  osc: [{ table: 'vocal', pos: .25, uni: 2, detune: .1, spread: .9, level: .72, filter: 'bp', cut: 900, res: .32, a: .008, d: .3, s: 0, r: .3 }],
  mods: [GATE(.82), mod('m1', 'osc1.pos', .6), mod('m1', 'osc1.cut', .1)], gate: .16, top: { level: .8 } });
// head D, 13 sixteenths on: the top note alone, an octave up, a struck glass
const headD = wave('head D: glass', {
  tune: { oct: 1 }, osc: [{ table: 'harmonics', harm: [1, 0, .5, 0, 0, .35, 0, .2, 0, 0, .25], filter: 'lp12', cut: 3200, a: .001, d: .42, s: 0, r: .5 }],
  mods: [GATE(.8)], gate: .08, top: { level: 1.1 } });
// the bass: a round sine under a little filtered saw, off the kick; mono, a hair of glide
const bass = wave('bass', { voicing: { mode: 'mono', always: true }, glide: .03, gate: .3,
  osc: [{ pos: 0, level: .95, filter: 'off', drive: .22, a: .005, d: .5, s: .55, r: .1 }, { pos: .667, level: .2, cut: 320, res: .2, famt: .22, fd: .12, fs: 0, a: .002, d: .16, s: .05, r: .08 }],
  mods: [GATE(.9)], top: { level: 1.15, velSense: .15 } });
// the pad: a slow vowel and detuned saws. m1 opens it over the song, a ball's height (m2) is its vowel, the beat cam (m3) ducks it
const pad = wave('pad: the fog', { tune: { oct: 1 }, gate: 1, names: NAMES('song open', 'ball', 'pump'),
  osc: [{ table: 'vocal', pos: .3, uni: 4, detune: .22, spread: 1, level: .5, filter: 'lp12', cut: 720, res: .12, a: .55, d: 1, s: .9, r: 1.3 },
    { pos: .667, uni: 4, detune: .3, spread: 1, level: .3, filter: 'lp24', cut: 480, res: .15, a: .9, d: 1, s: .85, r: 1.6 }],
  mods: [GATE(.72), mod('m1', 'osc1.cut', .14), mod('m1', 'osc2.cut', .22), mod('m2', 'osc1.pos', .5), mod('m3', 'vol', -.34)],
  fx: { chorus: { on: true, rate: .14, depth: .7, mix: .45 } }, top: { level: .85, velSense: 0 } });
// the mirror: the bass line backwards, a soft round pluck
const mirror = wave('mirror', { names: NAMES(),
  osc: [{ table: 'basic', pos: .33, level: .7, filter: 'lp12', cut: 1500, res: .2, famt: .2, fd: .08, fs: 0, a: .001, d: .22, s: 0, r: .2 }],
  mods: [GATE(.75)], gate: .09, top: { level: 1.1 } });
// the rain: a soft tuned bar
const rain = wave('rain', {
  osc: [{ table: 'harmonics', harm: [1, 0, .25, 0, 0, .12, 0, 0, .06], filter: 'lp12', cut: 2600, a: .001, d: .7, s: 0, r: .7 }],
  mods: [GATE(.8)], gate: .08, top: { level: 1.2, velSense: .25 } });

/* ---------------- the reel: the chord on tape, and four heads ---------------- */
disc(REEL, 5, 3, 2, 'minor');   // F3 to Eb5: F G Ab Bb C Db Eb, twice
// a head at delay d sixteenths sits that far round: a pin takes 1/32 of a turn a step, near enough
const headAngle = d => -2.85 + TAU * REEL.k * d * STEP;
const PH = { A: headAngle(0), B: headAngle(3), C: headAngle(7), D: headAngle(13) };
const rA = span(REEL, PH.A, 0, 13, headA), rB = span(REEL, PH.B, 0, 6, headB), rC = span(REEL, PH.C, 4, 10, headC), rD = span(REEL, PH.D, 11, 13, headD);
const Fm9 = [2, 4, 6, 8, 11], Fm11 = [3, 6, 7, 9, 11], Db69 = [2, 5, 7, 9, 11, 13], Absix = [2, 4, 6, 7, 11];
for (const [s, chord, v, p] of [[3, Fm9, .95, 1], [11, Absix, .68, 1], [14, Fm11, .82, 1], [19, Fm9, .9, 1], [27, Db69, .74, .75], [30, Fm11, .5, .45]])
  for (const ring of chord) pin(REEL, PH.A, ring, s * STEP, 1, v, p);

/* ---------------- the drums: two bars, straight ---------------- */
disc(DRUMS, 5, 3, 1, 'minpent');   // five lanes; their pitch is not heard
const PD = Math.PI / 2, lane = { kick: 0, clap: 1, hat: 2, shaker: 3, ghost: 4 }, SWING = .11;
const phis = { kick: PD, clap: PD + .42, hat: PD - .55, shaker: PD - 1, ghost: PD + .95 };
const dv = {
  kick: drum('kick', 1.05, .1, { tune: 44, sweep: 3.6, pdecay: .034, decay: .36 }),
  clap: drum('clap', 1.4, .3, { cutoff: 1500, spread: .014, decay: .24 }),
  hat: drum('hat', .36, .3, { cutoff: 7800, decay: .1 }),
  shaker: drum('hat', .42, .3, { cutoff: 10500, decay: .028 }),
  ghost: drum('rim', .34, .4, { tune: 1100, decay: .035 }) };
const rd = Object.fromEntries(Object.keys(lane).map(k => [k, span(DRUMS, phis[k], lane[k], lane[k], dv[k])]));
const hits = (name, steps, v = 1, p = 1) => steps.forEach((s, i) => pin(DRUMS, phis[name], lane[name], s * STEP, 1, Array.isArray(v) ? v[i] : v, Array.isArray(p) ? p[i] : p));
hits('kick', [0, 4, 8, 12, 16, 20, 24, 28]);
hits('clap', [4 + SWING, 12 + SWING, 20 + SWING, 28 + SWING], [.9, .85, .9, .95]);
hits('hat', [2, 6, 10, 14, 18, 22, 26, 30], [.9, .72, .85, .7, .9, .72, .85, .8]);
hits('shaker', [1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27, 29, 31].map(s => s + SWING), [.55, .35, .5, .3, .55, .35, .5, .4, .55, .35, .5, .3, .55, .35, .5, .45], [1, .85, 1, .7, 1, .85, 1, .9, 1, .85, 1, .7, 1, .85, 1, .9]);
hits('ghost', [7, 15, 25, 31], [.6, .45, .55, .35], [.8, .4, .7, .35]);

/* ---------------- the bass plate, the pad on its outer lanes ---------------- */
disc(BASS, 5, 1, 3, 'minor');   // F1 upward: rings 0-6 the bass (F1 G1 Ab1 Bb1 C2 Db2 Eb2), rings 7-20 the pad, an octave up
const rBass = span(BASS, Math.PI, 0, 6, bass), rPad = span(BASS, -Math.PI / 2, 7, 20, pad);
const F = 0, Ab = 2, C = 4, Db = 5, Eb = 6;
const LINE = [[3, F, 2, .95], [6, F, 1, .7], [10, F, 3, .9], [19, F, 2, .95], [23, Ab, 1, .7], [26, F, 2, .85], [30, Eb, 1, .65],
  [35, Db, 3, .95], [38, Db, 1, .7], [42, Db, 3, .9], [51, Eb, 2, .95], [54, Eb, 1, .7], [58, F, 2, .85], [62, C, 1, .7]];
for (const [s, ring, len, v] of LINE) pin(BASS, Math.PI, ring, s * STEP, len, v);
const padChords = [[0, 32, [9, 11, 13, 15]], [32, 16, [7, 9, 11, 12]], [48, 16, [8, 10, 12, 14]]];   // Fm9, Dbmaj7, Eb9
for (const [s, len, rings] of padChords) for (const ring of rings) pin(BASS, -Math.PI / 2, ring, s * STEP, len, .9);

/* ---------------- the mirror plate: the line backwards ---------------- */
disc(MIRROR, 5, 4, 1, 'minor');   // F4 to Eb5, the same seven degrees
const rMirror = span(MIRROR, 0, 0, 6, mirror);
for (const [s, ring, , v] of LINE) pin(MIRROR, 0, ring, (64 - s) * STEP, 1, v * .85);

/* ---------------- the hammers ---------------- */
function hammer(c, p, bar, len) {   // an arm on cog c reaching pad p at bar t
  const dx = p.x - c.x, dy = p.y - c.y, D = Math.hypot(dx, dy), s = Math.min(Math.sqrt(D * D - p.r * p.r), len), cosd = (s * s + D * D - p.r * p.r) / (2 * s * D);
  const beta = Math.atan2(dy, dx), half_ = Math.acos(cosd), edge = c.k > 0 ? beta - half_ : beta + half_;
  W.arms.push({ id: id(), cog: c.id, a: +wrap(edge - TAU * c.k * bar).toFixed(5), r0: 0, len, flex: .5 });
}
const padAt = (x, y, r, voice) => { const p = { id: id(), x: num(x), y: num(y), r, voice }; W.pads.push(p); return p; };
const pad5 = padAt(H5.x - 58, H5.y - 8, 10, drum('rim', .28, .2, { tune: 1500, decay: .05 }));
const pad7 = padAt(H7.x + 66, H7.y - 8, 12, drum('tom', .2, .2, { tune: 150, sweep: 1.5, pdecay: .03, tone: .12, decay: .16 }));
hammer(H5, pad5, 2 * STEP, 52);
hammer(H7, pad7, 3 * STEP, 62);

/* ---------------- the dust: pins sown at random on the 11-bar wheel ---------------- */
disc(P11, 5, 3, 1, 'minpent');
{ let s = 41; const rnd = () => (s = (s * 1664525 + 1013904223) >>> 0) / 4294967296;
  for (let i = 0; i < 110; i++) P11.track.dots.push({ ring: i % 3 === 0 ? 0 : 1, a: +(rnd() * TAU).toFixed(5), p: .16 + rnd() * .2, v: .4 + rnd() * .6, len: 1 }); }
const rTick = span(P11, 0, 1, 1, drum('hat', .16, 0, { cutoff: 6000, decay: .007 })), rPop = span(P11, .9, 0, 0, drum('wood', .05, 0, { tune: 900, decay: .012 }));

/* ---------------- the rain ---------------- */
const PIT = { id: id(), x: -520, y: 215, w: 270, h: 270, voice: rain, notes: [65, 68, 70, 72, 75, 77], balls: [] };
[65, 68, 72, 75, 77].forEach((m, i) => { const a = i * 1.26, r0 = 28 + i * 13; PIT.balls.push({ id: i + 1, note: m, bx: num(PIT.x - 12 + Math.cos(a) * r0), by: num(PIT.y + Math.sin(a) * r0), bvx: num(Math.cos(a + 1.3) * 40), bvy: num(Math.sin(a + 1.3) * 40) }); });
W.pits.push(PIT);
W.arms.push({ id: id(), cog: RW.id, a: 0, r0: 12, len: 118, flex: .6 });

/* ---------------- the cams ---------------- */
// a cam on cog c whose lift at bar t is fn(t), wrapped on the cog's own period
function cam(c, ang, fn, to = null, macro = 8) {
  const P = 1 / Math.abs(c.k), m = { id: id(), cog: c.id, ang: num(ang), phase: +wrap(ang).toFixed(6) / TAU, shape: 'drawn', prof: [], to: to ? to.id : null, macro, lo: 0, hi: 1 };
  m.prof = Array.from({ length: 64 }, (_, i) => { let t = (-(i / 64) / c.k) % P; if (t < 0) t += P; return +Math.max(0, Math.min(1, fn(t))).toFixed(3); });
  W.cams.push(m); return m;
}
// bar, value pairs, straight lines between; the song wheel's lift is read every 3 bars, so every breakpoint is a multiple of 3
const ramp = (...ks) => t => {
  for (let i = 1; i < ks.length; i++) if (t < ks[i][0]) { const [a, b] = [ks[i - 1], ks[i]]; return a[1] + (b[1] - a[1]) * (t - a[0]) / (b[0] - a[0]); }
  return ks[ks.length - 1][1];
};
// spread round the song wheel so the followers read as a dial
const FOLLOWERS = 14; let nf = 0; const fa = () => TAU * (nf++) / FOLLOWERS;
const song = (fn, to, macro) => cam(SONG, fa(), fn, to, macro);
//                    in              the groove                    breakdown            back                 out
song(ramp([0, 0], [6, 1], [186, 1], [192, 0]), rA);                                                    // head A, the whole way
song(ramp([0, .05], [24, .25], [72, .55], [96, .8], [108, 1], [120, .6], [156, 1], [168, .7], [192, .05]), rA, 2);   // and how far it opens
song(ramp([0, 0], [12, 0], [18, .9], [96, 1], [120, 1], [180, .8], [189, 0]), rB);                     // head B
song(ramp([0, 0], [48, 0], [72, .85], [96, 1], [120, 1], [168, .7], [180, 0]), rC);                    // head C
song(ramp([0, 0], [72, 0], [84, .8], [96, 1], [120, .8], [165, .7], [174, .9], [186, 0]), rD);         // head D
song(ramp([0, 0], [30, 0], [33, 1], [93, 1], [96, 0], [117, 0], [120, 1], [168, 1], [171, 0]), rBass); // the bass
song(ramp([0, 0], [72, 0], [84, .9], [93, 1], [96, .3], [129, .3], [135, 1], [165, 1], [171, 0]), rMirror);   // the mirror
const padOpen = ramp([0, .05], [48, .3], [96, .75], [108, 1], [120, .5], [156, .85], [192, .05]);
song(ramp([0, .55], [24, .6], [48, .55], [96, .85], [108, 1], [120, .7], [168, .6], [192, .55]), rPad);   // the pad
song(padOpen, rPad, 1);
// the drums' faders: a probe on the cam's follower sets a drum's level at each hit, so these only need the cam
const camKick = song(ramp([0, 0], [21, 0], [24, 1], [93, 1], [96, 0], [117, 0], [120, 1], [183, 1], [186, 0]));
const camClap = song(ramp([0, 0], [45, 0], [48, 1], [93, 1], [96, 0], [120, 0], [123, 1], [168, 1], [171, 0]));
const camTop = song(ramp([0, 0], [24, 0], [27, .55], [48, 1], [93, 1], [96, .3], [117, .3], [120, 1], [168, 1], [186, 0]));
const camHam = song(ramp([0, 0], [9, 0], [15, .7], [72, .8], [84, 1], [96, .8], [120, .8], [156, 1], [186, .4], [192, 0]));
const camRain = song(ramp([0, .8], [24, .55], [48, .25], [84, .25], [96, 1], [120, 1], [126, .3], [165, .3], [177, .9], [192, .8]));
if (nf !== FOLLOWERS) throw new Error(`song cams: ${nf}, spread for ${FOLLOWERS}`);
function probe(host, onId, to, o) { W.probes.push({ id: id(), host, on: onId, ball: 0, r: 0, a: 0, axis: 'reach', to, macro: 1, param: null, lo: 0, hi: 1, ...o }); }
const levelBy = (m, part) => probe('cam', m.id, (W.pads.includes(part) ? 'pad:' : 'reader:') + part.id, { param: 'level', hi: +(part.voice.level / 1.5).toFixed(4) });
levelBy(camKick, rd.kick); levelBy(camClap, rd.clap); levelBy(camTop, rd.hat); levelBy(camTop, rd.shaker); levelBy(camClap, rd.ghost);
levelBy(camHam, pad5); levelBy(camClap, pad7);
probe('cam', camRain.id, 'pit:' + PIT.id, { macro: 8 });
// the 7- and 11-bar wheels on the filters, never in step: 77 bars before they meet
const swell = t => .5 - .5 * Math.cos(TAU * t * Math.abs(P7.k));
cam(P7, -Math.PI * .75, t => Math.pow(swell(t), 1.6), rA, 1);
cam(P11, Math.PI * .75, t => { const u = t * Math.abs(P11.k) % 1; return u < .5 ? u * 2 : 2 - u * 2; }, rC, 1);
// the pump, once a beat: down on the kick, back by the offbeat, phased so it bites on the beat
cam(PUMP, -Math.PI / 2, t => { const u = (t * 4) % 1; return u < .04 ? u / .04 : Math.exp(-(u - .04) * 7.5); }, rPad, 3);
// a ball's height is the pad's vowel
probe('ball', PIT.id, 'reader:' + rPad.id, { ball: 3, axis: 'y', macro: 2 });

/* ---------------- the dub: fields of delay and room ---------------- */
const FD = { r: 150, mode: 'delay', diffuse: .5, level: .8, insert: 0, sync: '1/8.', time: .375, fb: .45, tone: .5, ping: 1, size: .6, decay: 2.5, predelay: 10, verbMode: 0, verbColor: 0, kind: 'tube', drive: .5 };
const field = (p, o) => W.fields.push({ id: id(), x: num(p.x), y: num(p.y), ...FD, ...o });
field(at(REEL, PH.B, .46), { r: 115, diffuse: .9, level: .72, sync: '1/8.', fb: .56, tone: .3 });                       // head B's lanes cross the core: each lane bends the tape
field(at(REEL, PH.C, .6), { r: 190, mode: 'reverb', diffuse: .8, level: .85, size: .86, decay: 6.5, predelay: 34, insert: .45 });  // head C is mostly room
field(at(REEL, PH.D, .85), { r: 95, diffuse: .8, level: .8, sync: '1/4.', fb: .62, tone: .26, insert: .5 });            // head D is mostly echo
field(at(REEL, PH.A, .6), { r: 105, diffuse: .85, level: .46, sync: '1/8.', fb: .42, tone: .34 });                      // head A throws a little
field(at(DRUMS, phis.clap, ringFrac(DRUMS.track, lane.clap)), { r: 42, mode: 'reverb', diffuse: .5, level: .9, size: .55, decay: 1.7, predelay: 12 });   // the clap's room, and nothing else's
field(pad5, { r: 58, diffuse: .35, level: .62, sync: '1/4.', fb: .46, tone: .4 });                                      // the rim hammer answers itself
field({ x: PIT.x, y: PIT.y }, { r: 200, diffuse: .95, level: .7, sync: '1/4.', fb: .55, tone: .35 });                   // where a ball is bends the rain's echoes
field(at(MIRROR, 0, .55), { r: 130, diffuse: .7, level: .62, sync: '1/8.', fb: .5, tone: .4 });                          // the mirror's echoes
if (W.fields.length > 8) throw new Error('more fields than slots');

/* ---------------- emit ---------------- */
W.cogs = cogs.map(c => ({ id: c.id, x: num(c.x), y: num(c.y), n: c.n, motor: c.motor, layer: c.layer, th: 0, th0: 0, tooth: num(wrap(c.tooth)), track: c.track }));
const out = path.join(__dirname, 'capstan.json');
const line = (k, v) => `"${k}":${JSON.stringify(v)}`, list = (k, a) => `"${k}":[${a.length ? '\n  ' + a.map(o => JSON.stringify(o)).join(',\n  ') + '\n' : ''}]`;
fs.writeFileSync(out, '{' + ['S', 'SPACE', 'climax', 'view'].map(k => line(k, W[k])).join(',\n') + ',\n' +
  ['cogs', 'arms', 'pads', 'readers', 'belts', 'strings', 'cams', 'pits', 'probes', 'fields'].map(k => list(k, W[k])).join(',\n') + '}\n');
console.log('wrote', out, fs.statSync(out).size, 'bytes');
console.log('turns a bar', Object.fromEntries(Object.entries({ M, REEL, DRUMS, BASS, MIRROR, P7, P11, H5, H7, PUMP, SONG, RW }).map(([k, c]) => [k, +c.k.toFixed(6)])));
console.log('parts', JSON.stringify({ headA: rA.id, headB: rB.id, headC: rC.id, headD: rD.id, bass: rBass.id, pad: rPad.id, mirror: rMirror.id, kick: rd.kick.id, clap: rd.clap.id, hat: rd.hat.id, shaker: rd.shaker.id, ghost: rd.ghost.id, tick: rTick.id, pop: rPop.id, rim: pad5.id, tom: pad7.id }));
