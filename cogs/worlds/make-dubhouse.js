// Builds cogs/src/default-world.json, the world a fresh page opens: "Minimal & Dub House", 120 bpm in A minor.
// node cogs/worlds/make-dubhouse.js && node cogs/build.js
// A motor turning once a bar drives three 48-tooth discs at half speed, so each disc is two bars to a turn in
// sixteenths (div 32): the drums below (four on the floor, claps on two and four, open hats on the offbeat, a
// shaker and a sparse rim), a round dub bass to the right, and a minor-seventh chord stab to the left, thrown into
// a dotted-eighth delay and a long room. A wood knock on the motor's arm, once a bar, echoes in its own delay.
// The file is written in the order the page saves a world (serialize in sim.js), so a fresh page opens it exactly.
const fs = require('fs'), path = require('path');
const TAU = Math.PI * 2, TOOTH = 12;
const R = n => n * TOOTH / TAU;
const wrap = a => ((a % TAU) + TAU) % TAU;
const BPM = 120, BAR = 1 / 16;   // a step is a sixteenth of a bar

const W = {
  S: { bpm: BPM, beats: 4, seed: 3, meshTol: 4, coaxTol: 6, snapTol: 16, vRef: 500, velCurve: .72, master: .72, lookahead: .2, maxStep: .05, zoom: 1, windows: 0, ids: 0, ratios: 0 },
  // a darker, longer room than a dry house mix: the dub is in the tails
  SPACE: { dry: 1, mode: 0, color: 1, wet: .26, predelay: 28, decay: 2.8, size: .62, attack: .15, early: .6, late: .7, modRate: .3, modDepth: .3333, highCut: 9000, lowCut: 220, bassMult: .7, bassXover: 200, highShelf: -6.0206, highFreq: 4000, earlyLate: .65, width: 1.3, shimmer: 0, shimmerPitch: 12, freeze: 0 },
  // the line amp warm rather than hot: a little drive and saturation, the low end held, a touch of air
  climax: { on: 1, drive: 2.5, sat: 35, classA: 1, excite: 2, air: 1.5, airFreq: 10000, low: 1, lowFreq: 70, high: 0, highFreq: 3000, mix: 1, out: 0 },
  view: { x: 0, y: 0 }, cogs: [], arms: [], pads: [], readers: [], belts: [], strings: [], cams: [], pits: [], probes: [], fields: [] };
// ids laid out as the earlier default world's were (its checks name the bass disc 8 and its reader 9)
let nid = 1; const id = () => nid++, from = n => { nid = n; };

/* ---------------- gearing ---------------- */
const cogs = [];
function cog(o) { const c = { id: id(), x: 0, y: 0, n: 24, motor: null, layer: 0, th: 0, track: null, k: 0, ...o }; cogs.push(c); return c; }
// b meshed onto a at world angle ang, its teeth phased into a's
function meshAt(a, b, ang) {
  const d = R(a.n) + R(b.n); b.x = a.x + Math.cos(ang) * d; b.y = a.y + Math.sin(ang) * d;
  const tooth = (ang - a.th) * a.n / TAU;
  b.th = wrap(ang + Math.PI - (.5 - tooth) * TAU / b.n); b.k = -a.k * a.n / b.n;
}
from(1); const M = cog({ n: 24, motor: { rpm: BPM / 4, revs: 1 }, k: 1 });            // once a bar
from(2); const DRUMS = cog({ n: 48 }); meshAt(M, DRUMS, Math.PI / 2);
from(8); const BASS = cog({ n: 48 }); meshAt(M, BASS, 0);
from(10); const CHORDS = cog({ n: 48 }); meshAt(M, CHORDS, Math.PI);

/* ---------------- discs, pins and readers ---------------- */
const SC = { minor: [0, 2, 3, 5, 7, 8, 10], minpent: [0, 3, 5, 7, 10] };
const nRings = t => SC[t.scale].length * t.range;
const ringFrac = (t, i) => nRings(t) > 1 ? .25 + .65 * i / (nRings(t) - 1) : .6;
function disc(c, root, octave, range, scale) { c.track = { root, octave, range, scale, div: Math.round(16 / Math.abs(c.k)), dots: [] }; }
// a pin on disc c that reaches world angle phi at bar t
const pin = (c, phi, ring, t, len = 1, v = 1, p = 1) => c.track.dots.push({ ring, a: +wrap(phi - c.th - TAU * c.k * t).toFixed(5), p, v, len });
function reader(c, phi, f0, f1, voice) {
  const r = R(c.n), ux = Math.cos(phi), uy = Math.sin(phi);
  W.readers.push({ id: id(), on: c.id, x1: c.x + ux * f0 * r, y1: c.y + uy * f0 * r, x2: c.x + ux * f1 * r, y2: c.y + uy * f1 * r, voice, tines: 0, combBase: 60, combScale: 'major' });
}
const half = t => .65 / (nRings(t) - 1) / 2;
const laneReader = (c, phi, ring, voice) => { const f = ringFrac(c.track, ring), h = half(c.track) * .9; reader(c, phi, f - h, f + h, voice); };
const fullReader = (c, phi, voice) => { const t = c.track, h = half(t); reader(c, phi, ringFrac(t, 0) - h, ringFrac(t, nRings(t) - 1) + h, voice); };
const at = (c, phi, f) => ({ x: c.x + Math.cos(phi) * f * R(c.n), y: c.y + Math.sin(phi) * f * R(c.n) });

/* ---------------- voices ---------------- */
// a drum in the order the page builds one (makeVoice in sim.js): level, vel sense, the kind's knobs, kind
const KNOBS = { kick: ['tune', 'sweep', 'pdecay', 'decay'], clap: ['cutoff', 'spread', 'decay'], hat: ['cutoff', 'decay'], rim: ['tune', 'decay'], wood: ['tune', 'decay'] };
const drum = (kind, level, velSense, knobs) => ({ level, velSense, ...Object.fromEntries(KNOBS[kind].map(k => [k, knobs[k]])), kind });
const osc = o => ({ on: true, table: 'basic', filter: 'lp24', warpMode: 'bend', warp2Mode: 'off', ...o });
function wave(name, p) {
  return { kind: 'wave', name, note: 60, octave: 0, semi: 0, level: .7, velSense: .6,
    osc: [0, 1, 2].map(i => p.osc[i] ? osc(p.osc[i]) : { on: false }), sub: p.sub || { level: 0, oct: -1 }, noise: { level: 0, color: .6 }, tune: {},
    env: [], lfo: p.lfo || [], macro: [], macroNames: [], mods: p.mods || [], voicing: p.voicing || { mode: 'poly', always: true }, fx: p.fx || {},
    vol: p.vol ?? .8, glide: p.glide || 0, gate: p.gate ?? .35, ...(p.top || {}) };
}
// round and short, a sine with a little saw on the attack: the bass sits under the kick rather than on it
const dubBass = wave('dub bass', {
  osc: [{ pos: 0, level: .95, filter: 'lp24', cut: 380, res: .12, drive: .25, a: .004, d: .45, s: .6, r: .12 },
    { pos: .667, level: .22, cut: 650, res: .2, famt: .3, fd: .12, fs: 0, a: .002, d: .16, s: 0, r: .08 }],
  voicing: { mode: 'mono', always: true }, glide: .025, gate: .3, top: { level: 1.05, velSense: .35 } });
// the stab: two detuned saws through a low-passed envelope that closes fast, chorus to widen it
const dubChord = wave('dub chord', {
  osc: [{ pos: .667, uni: 3, detune: .14, spread: .6, level: .6, cut: 650, res: .32, famt: .45, fd: .2, fs: .05, a: .002, d: .32, s: 0, r: .3 },
    { pos: .667, semi: 12, level: .16, cut: 900, res: .2, famt: .3, fd: .14, fs: 0, a: .002, d: .2, s: 0, r: .2 }],
  lfo: [{ shape: 'sine', rate: .07, retrig: false }],
  mods: [{ src: 'lfo1', dst: 'osc1.cut', amt: .18, bi: true, off: false }],
  fx: { chorus: { on: true, rate: .25, depth: .6, mix: .35 } }, gate: .2, top: { level: .62, velSense: .45 } });

/* ---------------- the drums: two bars, below the motor ---------------- */
disc(DRUMS, 9, 3, 1, 'minpent');                          // five lanes; their pitch is not heard
const PD = Math.PI / 2, lane = { kick: 0, clap: 1, hat: 2, shaker: 3, rim: 4 };
from(3); laneReader(DRUMS, PD, lane.kick, drum('kick', 1.3, .2, { tune: 48, sweep: 3.2, pdecay: .04, decay: .42 }));
laneReader(DRUMS, PD, lane.clap, drum('clap', .62, .4, { cutoff: 1250, spread: .013, decay: .3 }));
laneReader(DRUMS, PD, lane.hat, drum('hat', .42, .5, { cutoff: 8500, decay: .13 }));
laneReader(DRUMS, PD, lane.shaker, drum('hat', .26, .8, { cutoff: 11500, decay: .03 }));
laneReader(DRUMS, PD, lane.rim, drum('rim', .32, .5, { tune: 1650, decay: .05 }));
const hits = (name, steps, v = 1, p = 1) => steps.forEach((s, i) => pin(DRUMS, PD, lane[name], s * BAR, 1, Array.isArray(v) ? v[i] : v, Array.isArray(p) ? p[i] : p));
hits('kick', [0, 4, 8, 12, 16, 20, 24, 28]);
hits('clap', [4, 12, 20, 28], [.95, .9, .95, .9]);
hits('clap', [31], .35, .5);                                 // a ghost into the turn, half the time
hits('hat', [2, 6, 10, 14, 18, 22, 26, 30], [.85, .75, .85, .75, .85, .75, .85, .9]);
// the shaker fills the sixteenths between, swaying in velocity; a few drop out at random
hits('shaker', [1, 3, 5, 7, 9, 11, 13, 15, 17, 19, 21, 23, 25, 27, 29, 31],
  [.55, .35, .5, .3, .55, .35, .5, .4, .55, .35, .5, .3, .55, .35, .5, .45], [1, .8, 1, .7, 1, .8, 1, .9, 1, .8, 1, .7, 1, .8, 1, .9]);
hits('rim', [7, 13, 23, 26, 29], [.7, .45, .7, .5, .4], [1, .6, 1, .55, .4]);

/* ---------------- the bass: two bars, to the right ---------------- */
disc(BASS, 9, 1, 1, 'minor');                              // A1 to G2: A B C D E F G
from(9); fullReader(BASS, 0, dubBass);
const A = 0, C = 2, D = 3, E = 4, G = 6;
for (const [s, ring, len, v] of [[3, A, 2, .95], [6, A, 1, .7], [10, A, 2, .9], [14, E, 1, .65],
  [19, A, 2, .95], [22, A, 1, .7], [26, C, 2, .85], [29, D, 1, .6], [30, E, 1, .7]]) pin(BASS, 0, ring, s * BAR, len, v);
void G;

/* ---------------- the chord stab: two bars, to the left ---------------- */
disc(CHORDS, 9, 3, 2, 'minor');                            // A3 to G5
from(11); fullReader(CHORDS, Math.PI, dubChord);
const Am7 = [0, 2, 4, 6], Am9 = [0, 2, 4, 6, 8], Dm7 = [3, 5, 7, 9];
for (const [s, chord, len, v] of [[3, Am7, 1, .9], [10, Am7, 2, .75], [19, Am9, 1, .9], [22, Am7, 1, .55], [28, Dm7, 2, .8]])
  for (const ring of chord) pin(CHORDS, Math.PI, ring, s * BAR, len, v);

/* ---------------- the knock on the motor ---------------- */
// an arm on the motor sweeps a wood block above it once a bar, on the last sixteenth but one
from(12); const pad = { id: id(), x: 0, y: -58, r: 11, voice: drum('wood', .34, .4, { tune: 1350, decay: .05 }) };
W.pads.push(pad);
{ const dx = pad.x - M.x, dy = pad.y - M.y, D0 = Math.hypot(dx, dy), len = 62, s = Math.min(Math.sqrt(D0 * D0 - pad.r * pad.r), len);
  const cosd = (s * s + D0 * D0 - pad.r * pad.r) / (2 * s * D0), edge = Math.atan2(dy, dx) - Math.acos(cosd);
  W.arms.push({ id: id(), cog: M.id, a: +wrap(edge - M.th - TAU * M.k * 14 * BAR).toFixed(5), r0: 0, len, flex: .5 }); }

/* ---------------- the dub: fields of delay and room ---------------- */
// in the order the page saves a field (fieldOut in field.js)
const FD = { r: 150, mode: 'delay', diffuse: .5, level: .8, insert: 0, sync: '1/8.', time: .375, fb: .45, tone: .5, ping: 1, size: .6, decay: 2.5, predelay: 10, verbMode: 0, verbColor: 2, kind: 'tube', drive: .5 };
from(14); const field = (p, o) => W.fields.push({ id: id(), x: +p.x.toFixed(3), y: +p.y.toFixed(3), ...FD, ...o });
const chordAt = at(CHORDS, Math.PI, .62);
field(chordAt, { r: 85, mode: 'delay', diffuse: .55, level: .85, sync: '1/8.', fb: .58, tone: .32, ping: 1 });   // the stab's echoes, ping-ponged and darkening
field(chordAt, { r: 125, mode: 'reverb', diffuse: .85, level: .6, size: .85, decay: 4.5, predelay: 25 });        // and the long room they fall into
field(pad, { r: 42, mode: 'delay', diffuse: .3, level: .7, sync: '1/4.', fb: .5, tone: .4, ping: 1 });            // the knock answers itself
const rimAt = at(DRUMS, PD, ringFrac(DRUMS.track, lane.rim));
field({ x: rimAt.x, y: rimAt.y + 24 }, { r: 30, mode: 'delay', diffuse: .2, level: .55, sync: '1/8.', fb: .35, tone: .45, ping: 1 });   // the rim only, a short throw

/* ---------------- emit ---------------- */
const num = x => +x.toFixed(6);
W.cogs = cogs.map(c => ({ id: c.id, x: num(c.x), y: num(c.y), n: c.n, motor: c.motor, layer: c.layer, th: num(wrap(c.th)), track: c.track }));
for (const r of W.readers) for (const k of ['x1', 'y1', 'x2', 'y2']) r[k] = num(r[k]);
const out = path.join(__dirname, '..', 'src', 'default-world.json');
const line = (k, v) => `"${k}":${JSON.stringify(v)}`, list = (k, a) => `"${k}":[${a.length ? '\n  ' + a.map(o => JSON.stringify(o)).join(',\n  ') + '\n' : ''}]`;
fs.writeFileSync(out, '{' + ['S', 'SPACE', 'climax', 'view'].map(k => line(k, W[k])).join(',\n') + ',\n' +
  ['cogs', 'arms', 'pads', 'readers', 'belts', 'strings', 'cams', 'pits', 'probes', 'fields'].map(k => list(k, W[k])).join(',\n') + '}\n');
console.log('wrote', out, fs.statSync(out).size, 'bytes; pins', cogs.filter(c => c.track).map(c => c.track.dots.length).join(','), 'rev/bar', cogs.map(c => c.k).join(','));
