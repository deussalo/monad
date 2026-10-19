// Builds cogs/worlds/dubstep.json: a 32-bar, 140 bpm half-time dubstep machine in F minor.
// node cogs/worlds/make-dubstep.js
// Time is measured in bars (4 beats). Every cog turns k revolutions per bar (solved from the gearing below),
// so a pin / arm / string is placed by working out the angle that reaches its reader at bar time t.
const fs = require('fs'), path = require('path');
const TAU = Math.PI * 2, TOOTH = 12;
const R = n => n * TOOTH / TAU;
const wrap = a => ((a % TAU) + TAU) % TAU;
const BPM = 140;

const W = { S: { bpm: BPM, beats: 4, seed: 7, meshTol: 4, coaxTol: 6, snapTol: 16, vRef: 500, velCurve: .72, master: .6, lookahead: .2, maxStep: .05, zoom: 1, windows: 0, ids: 0, ratios: 0 },
  SPACE: { dry: 1, wet: .22, predelay: 18, size: .9, decay: 2.2, lowMult: .8, highMult: .45, lowCross: 200, highCross: 4500, diffusion: .75, density: .6, modRate: .3, modDepth: .5, shimmer: 0, shimmerPitch: 12, freeze: 0, width: 1.2, duck: .35, lowCut: 160, highCut: 11000 },
  // the master's line amp pushed for a dubstep master: hotter drive, more grit, the top end excited, the sub lifted
  climax: { on: 1, drive: 5, sat: 55, classA: 1, excite: 6, air: 3.5, airFreq: 9000, low: 2.5, lowFreq: 60, high: 1, highFreq: 2500, mix: 1, out: -1 },
  view: { x: 0, y: 0 }, cogs: [], arms: [], pads: [], readers: [], belts: [], strings: [], cams: [], pits: [], probes: [], fields: [] };
let nid = 1; const id = () => nid++;

/* ---------------- cogs and gearing ---------------- */
const cogs = [];
function cog(o) { const c = { id: id(), x: 0, y: 0, n: 24, motor: null, layer: 0, th: 0, track: null, ...o }; cogs.push(c); return c; }
const near = (a, b) => Math.hypot(a.x - b.x, a.y - b.y);
// mesh: place b touching a at angle ang (radians, screen coords), tooth-phase it like the engine's interlock
function meshAt(a, b, ang) {
  const d = R(a.n) + R(b.n); b.x = a.x + Math.cos(ang) * d; b.y = a.y + Math.sin(ang) * d;
  const phi = Math.atan2(b.y - a.y, b.x - a.x), tooth = (phi - a.th) * a.n / TAU;
  b.th = wrap(phi + Math.PI - (.5 - tooth) * TAU / b.n);
}
const belts = [];
const belt = (a, b, cross = false) => { belts.push({ id: id(), a: a.id, b: b.id, cross }); };
function solveGears() {
  const adj = new Map(cogs.map(c => [c, []]));
  for (let i = 0; i < cogs.length; i++) for (let j = i + 1; j < cogs.length; j++) {
    const a = cogs[i], b = cogs[j], d = near(a, b);
    if (d <= 6) { adj.get(a).push([b, 1]); adj.get(b).push([a, 1]); }
    else if (a.layer === b.layer) {
      const gap = d - R(a.n) - R(b.n);
      if (Math.abs(gap) <= 4) { adj.get(a).push([b, -a.n / b.n]); adj.get(b).push([a, -b.n / a.n]); }
      else if (gap < 0) console.warn('JAM overlap', a.id, b.id);
    }
  }
  const byId = new Map(cogs.map(c => [c.id, c]));
  for (const b of belts) { const A = byId.get(b.a), B = byId.get(b.b), s = b.cross ? -1 : 1; adj.get(A).push([B, s * A.n / B.n]); adj.get(B).push([A, s * B.n / A.n]); }
  const m = cogs.find(c => c.motor); m.k = 1; const q = [m];
  for (let i = 0; i < q.length; i++) for (const [b, r] of adj.get(q[i])) {
    const k = q[i].k * r;
    if (b.k === undefined) { b.k = k; q.push(b); } else if (Math.abs(b.k - k) > 1e-9) console.warn('GEAR CONFLICT', b.id);
  }
  for (const c of cogs) if (c.k === undefined) console.warn('cog not driven', c.id);
}

/* ---------------- placing things by time ---------------- */
const angleAt = (c, phi, t) => wrap(phi - c.th - TAU * c.k * t);          // an angle on cog c that is at world angle phi at bar t
const R16 = 1 / 16;

// A disc: dots by [ring, step16, lenSteps, velocity]
function disc(c, { root = 5, octave = 3, range = 1, scale = 'minor' }) {
  c.track = { root, octave, range, scale, div: Math.round(16 / Math.abs(c.k)), dots: [] };
  return c;
}
const SC = { minor: [0, 2, 3, 5, 7, 8, 10], minpent: [0, 3, 5, 7, 10], pentatonic: [0, 2, 4, 7, 9], major: [0, 2, 4, 5, 7, 9, 11] };
const trackBase = t => 12 * (t.octave + 1) + t.root;
const ringMidi = (t, i) => trackBase(t) + SC[t.scale][i % SC[t.scale].length] + 12 * Math.floor(i / SC[t.scale].length);
const ringFor = (t, m) => { for (let i = 0; i < SC[t.scale].length * t.range; i++) if (ringMidi(t, i) === m) return i; throw new Error('note ' + m + ' not on disc'); };
const nRings = t => SC[t.scale].length * t.range;
const ringFrac = (t, i) => nRings(t) > 1 ? .25 + .65 * i / (nRings(t) - 1) : .6;
function pin(c, phi, ring, bar, lenSteps = 1, v = 1, p = 1) {
  c.track.dots.push({ ring, a: +angleAt(c, phi, bar).toFixed(5), p, v, len: lenSteps });
}
// A reader lying along the radius of disc c at world angle phi, covering ring fractions f0..f1
function reader(c, phi, f0, f1, voice, extra = {}) {
  const r = R(c.n), u = [Math.cos(phi), Math.sin(phi)];
  const o = { id: id(), on: c.id, x1: c.x + u[0] * f0 * r, y1: c.y + u[1] * f0 * r, x2: c.x + u[0] * f1 * r, y2: c.y + u[1] * f1 * r, voice, tines: 0, combBase: 60, combScale: 'major', ...extra, phi, disc: c };
  W.readers.push(o); return o;
}
const laneReader = (c, phi, ring, voice) => { const f = ringFrac(c.track, ring), h = .65 / (nRings(c.track) - 1) / 2 * .9; return reader(c, phi, f - h, f + h, voice); };
const fullReader = (c, phi, voice, extra) => { const t = c.track, h = .65 / (nRings(t) - 1) / 2; return reader(c, phi, ringFrac(t, 0) - h, ringFrac(t, nRings(t) - 1) + h, voice, extra); };

function pad(x, y, r, voice) { const p = { id: id(), x, y, r, voice }; W.pads.push(p); return p; }
// An arm on cog c reaching pad p at bar t
function armHit(c, p, bar, r0, len) {
  const dx = p.x - c.x, dy = p.y - c.y, D = Math.hypot(dx, dy), rho = p.r;
  const s = Math.min(Math.max(Math.sqrt(D * D - rho * rho), Math.max(r0, 1e-6)), r0 + len), cosd = (s * s + D * D - rho * rho) / (2 * s * D);
  if (!(cosd < 1 && cosd > -1)) throw new Error('arm cannot reach pad');
  const beta = Math.atan2(dy, dx), half = Math.acos(cosd), edge = c.k > 0 ? beta - half : beta + half;
  const a = wrap(edge - c.th - TAU * c.k * bar);
  const arm = { id: id(), cog: c.id, a: +a.toFixed(5), r0, len, flex: .5 }; W.arms.push(arm); return arm;
}

/* ---------------- voices ---------------- */
const drum = (kind, o) => ({ kind, level: .8, velSense: .5, ...o });
const mod = (src, dst, amt, extra = {}) => ({ src, dst, amt, bi: false, off: false, aux: '', auxInv: false, curve: 0, ...extra });
const osc = o => ({ on: true, table: 'basic', filter: 'lp24', warpMode: 'bend', warp2Mode: 'off', ...o });
const off = { on: false };
function wave(name, p) {
  const v = { kind: 'wave', name, note: 60, octave: 0, semi: 0, level: .7, velSense: .6,
    osc: [off, off, off].map((d, i) => p.osc[i] ? osc(p.osc[i]) : { ...d }),
    sub: p.sub || { level: 0, oct: -1 }, noise: p.noise || { level: 0, color: .6 }, tune: p.tune || {},
    env: p.env || [], lfo: p.lfo || [], macro: p.macro || [], macroNames: p.macroNames || [], mods: p.mods || [],
    voicing: p.voicing || { mode: 'poly', always: true }, fx: p.fx || {}, vol: p.vol ?? .8, glide: p.glide || 0, gate: p.gate ?? .35, ...(p.top || {}) };
  if (p.fxOrder) v.fxOrder = p.fxOrder;
  return v;
}
const GATE = (slot = 8, amt = .9) => mod('m' + slot, 'vol', amt);       // a cam / probe on macro 8 is this part's fader

const V = {
  sub: wave('sub', { osc: [{ pos: 0, level: .95, filter: 'off', a: .004, d: 1.5, s: 1, r: .1, drive: .35 }, { table: 'basic', pos: 0, oct: 1, level: .16, filter: 'lp24', cut: 300, a: .004, d: 1, s: .8, r: .1 }],
    voicing: { mode: 'mono', always: true }, vol: 0, mods: [GATE(8, .95)], gate: .3, top: { level: 1, velSense: .2 } }),
  wobble: wave('wobble', {
    osc: [{ pos: .667, uni: 4, detune: .32, spread: .7, blend: .5, level: .8, cut: 230, res: .55, drive: .5, key: .2, a: .004, d: 1, s: 1, r: .08, warpMode: 'fm', warp: 0 },
      { table: 'pulse', pos: .15, oct: -1, uni: 2, detune: .2, level: .55, cut: 220, res: .45, drive: .3, a: .004, d: 1, s: 1, r: .08 },
      { table: 'vocal', pos: 0, oct: 0, uni: 3, detune: .25, level: 0, filter: 'bp', cut: 900, res: .5, a: .004, d: 1, s: 1, r: .08 }],
    lfo: [{ shape: 'tri', sync: '1/8', retrig: true }, { shape: 'tri', sync: '1/8T', retrig: true }, { shape: 'sine', sync: '1/16', retrig: true }],
    macroNames: ['cam wub', 'rate', 'growl', '', '', '', '', 'gate'], macro: [0, 0, 0, 0, 0, 0, 0, 0],
    mods: [mod('lfo1', 'osc1.cut', .46, { aux: 'm2', auxInv: true }), mod('lfo2', 'osc1.cut', .46, { aux: 'm2' }),
      mod('lfo1', 'osc2.cut', .4, { aux: 'm2', auxInv: true }), mod('lfo2', 'osc2.cut', .4, { aux: 'm2' }),
      mod('m1', 'osc1.warp', .55), mod('m1', 'dist.drive', .25), mod('m1', 'osc1.pos', .12),
      mod('m3', 'osc3.level', .5), mod('m4', 'osc3.pos', .8), mod('m4', 'osc1.warp2', .3), GATE(8, .8)],
    voicing: { mode: 'mono', always: true }, vol: 0,
    fx: { dist: { on: true, mode: 'tube', drive: .3, mix: .85 }, comp: { on: true, thresh: -14, ratio: 4, gain: 3 }, eq: { on: true, low: 1, high: 2 }, phaser: { on: true, rate: .25, depth: .5, freq: 600, fb: .3, mix: .25 } },
    fxOrder: ['dist', 'phaser', 'comp', 'eq', 'hyper', 'flanger', 'chorus', 'delay'], gate: .3 }),
  lead: wave('screech', {
    osc: [{ table: 'sweep', pos: .8, uni: 5, detune: .45, spread: 1, level: .7, filter: 'hp12', cut: 700, res: .3, a: .002, d: .5, s: .6, r: .12, warpMode: 'sync', warp: .2 },
      { table: 'basic', pos: .667, oct: 1, uni: 3, detune: .3, level: .35, filter: 'hp12', cut: 900, a: .002, d: .4, s: .5, r: .1, warpMode: 'fm', warp: .35 }],
    lfo: [{ shape: 'saw down', sync: '1/8', retrig: true }, { shape: 'sine', sync: '1/16', retrig: true }],
    macroNames: ['', '', '', '', '', '', '', 'gate'], macro: [0, 0, 0, 0, 0, 0, 0, 0],
    mods: [mod('lfo1', 'osc1.warp', .75), mod('lfo2', 'osc2.warp', .4, { bi: true }), mod('lfo1', 'osc1.cut', .12), GATE(8, .8)],
    fx: { dist: { on: true, mode: 'fold', drive: .38, mix: .8 }, phaser: { on: true, rate: .8, depth: .7, freq: 1500, fb: .5, mix: .4 }, delay: { on: true, sync: '1/4', ping: true, time: .3, fb: .4, tone: .5, mix: .25 } },
    fxOrder: ['dist', 'phaser', 'delay', 'comp', 'eq', 'hyper', 'flanger', 'chorus'], vol: 0, glide: .04, gate: .2, voicing: { mode: 'mono', always: true } }),
  pad: wave('dark pad', {
    osc: [{ pos: .667, uni: 6, detune: .5, spread: 1, level: .55, filter: 'lp12', cut: 700, res: .2, a: 1, d: 1.5, s: .85, r: 1.8 },
      { table: 'vocal', pos: .3, oct: 1, uni: 3, detune: .3, level: .22, filter: 'lp12', cut: 1200, a: 1.4, d: 1, s: .9, r: 2 }],
    lfo: [{ shape: 'tri', rate: .12, retrig: false }],
    macroNames: ['ball', '', '', '', '', '', '', 'gate'], macro: [.3, 0, 0, 0, 0, 0, 0, 0],
    mods: [mod('lfo1', 'osc1.cut', .18), mod('m1', 'osc1.cut', .3), mod('m1', 'osc2.pos', .5), GATE(8, .85)],
    fx: { chorus: { on: true, rate: .3, depth: .7, mix: .5 }, hyper: { on: true, voices: 4, rate: .4, detune: .5, mix: .4, dim: .5 } },
    vol: 0, gate: 1, top: { level: 1.45 } }),
  riser: wave('riser', {
    osc: [{ pos: .667, uni: 6, detune: .6, spread: 1, level: .5, filter: 'bp', cut: 300, res: .55, famt: .9, fa: 4.5, fd: 1, fs: 1, fr: .2, a: 3.5, d: 1, s: 1, r: .25 }],
    noise: { level: .22, color: .85 }, env: [{ a: 3.5, d: 1, s: 1, r: .25 }],
    mods: [mod('env2', 'osc1.fine', .3)], fx: { phaser: { on: true, rate: .3, depth: .8, freq: 900, fb: .5, mix: .5 } }, vol: .6, gate: 2, top: { level: 1.4 } }),
  bell: wave('glass bell', {
    osc: [{ table: 'harmonics', harm: [1, 0, .6, 0, 0, .4, 0, .25, 0, 0, .3, 0, 0, 0, .2, 0, 0, 0, 0, .15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, .1, 0], filter: 'lp12', cut: 3500, a: .002, d: 2.2, s: 0, r: 1.5 },
      { table: 'organ', pos: .5, oct: 1, level: .12, filter: 'off', a: .002, d: .5, s: 0, r: .6 }],
    macroNames: ['', '', '', '', '', '', '', 'gate'], mods: [GATE(8, .8)],
    fx: { chorus: { on: true, rate: .4, depth: .5, mix: .4 } }, vol: 0, gate: .1, top: { level: 1.4 } }),
};
const patchLevel = (v, l) => { v.level = l; return v; };

/* =================== the machine =================== */
// --- the clock: the motor M turns once a bar
const M = cog({ n: 48, x: 0, y: 470, motor: { rpm: 35, revs: 1 }, th: 0 });
const P0 = cog({ n: 16, x: M.x, y: M.y, layer: 1 });            // pulley on the motor: belt to the drum disc, 1/4
const P0b = cog({ n: 12, x: M.x, y: M.y, layer: 2 });           // pulley: belt to the harp wheel, 1/2

// --- the drum disc X1: four bars to a turn
const X1 = cog({ n: 64, x: 0, y: 0 });
const H1 = cog({ n: 12, x: 0, y: 0, layer: 1 });                // hub pulley of the drum disc (k=1/4): feeds the four 16-bar discs
belt(P0, X1); 
// --- four 16-bar discs around it
const X2 = cog({ n: 48, x: -235, y: -235 });   // pads + riser
const X3 = cog({ n: 48, x: 235, y: -235 });    // screech lead
const XS = cog({ n: 48, x: -235, y: 235 });    // sub
const XW = cog({ n: 48, x: 235, y: 235 });     // wobble
for (const d of [X2, X3, XS, XW]) belt(H1, d);
// --- the song cogs: 32 bars to a turn, cams on them hold the arrangement
const H2 = cog({ n: 12, x: X2.x, y: X2.y, layer: 1 });
const S1 = cog({ n: 24, x: -200, y: -470 });
belt(H2, S1);
const S2 = cog({ n: 24 }); meshAt(S1, S2, 0);
const S3 = cog({ n: 24 }); meshAt(S2, S3, 0);
// --- the rhythm gears off the motor
const Hw = cog({ n: 24 }); meshAt(M, Hw, Math.PI * 5 / 6);        // hat wheel, 2 turns a bar
const Wc = cog({ n: 12 }); meshAt(Hw, Wc, Math.PI * 1.1);          // wobble cam wheel: 4 a bar
const Wg = cog({ n: 16 }); meshAt(M, Wg, -Math.PI * 2 / 3);        // growl cam wheel: 3 a bar (3 against 4)
// --- harp / knocker wheel: 2 bars a turn
const HW = cog({ n: 24, x: 205, y: 610 }); belt(P0b, HW);

solveGears();
const k = c => c.k;
console.log('rates (rev/bar):', Object.fromEntries([['M', M], ['X1', X1], ['XS', XS], ['X2', X2], ['S1', S1], ['S2', S2], ['Hw', Hw], ['Wc', Wc], ['Wg', Wg], ['HW', HW]].map(([n, c]) => [n, +c.k.toFixed(4)])));

/* ---- drums on X1 ---- */
disc(X1, { root: 0, octave: 3, range: 1, scale: 'minpent' });
const PHI1 = -Math.PI / 2;   // readers at the top of the disc
const drums = {
  kick: laneReader(X1, PHI1, 0, drum('kick', { level: 1.15, velSense: .15, tune: 47, sweep: 3.2, pdecay: .045, decay: .5 })),
  snare: laneReader(X1, PHI1, 1, drum('snare', { level: .85, velSense: .3, tune: 185, tone: .4, cutoff: 2400, decay: .24 })),
  clap: laneReader(X1, PHI1, 2, drum('clap', { level: .55, velSense: .3, cutoff: 1500, spread: .012, decay: .3 })),
  tom: laneReader(X1, PHI1, 3, drum('tom', { level: .9, velSense: .3, tune: 100, sweep: 2, pdecay: .07, tone: .1, decay: .35 })),
  rim: laneReader(X1, PHI1, 4, drum('rim', { level: .4, velSense: .3, tune: 1800, decay: .06 })),
};
const lane = { kick: 0, snare: 1, clap: 2, tom: 3, rim: 4 };
const S16 = s => s / 16;                    // step -> bars
function drumPins(name, steps, v = 1) { for (const s of steps) pin(X1, PHI1, lane[name], S16(s), 1, Array.isArray(v) ? v[steps.indexOf(s)] : v); }
drumPins('kick', [0, 6, 16, 26, 32, 38, 48, 54, 58]);
drumPins('snare', [8, 24, 40, 56], 1); pin(X1, PHI1, 1, S16(62), 1, .45);
drumPins('clap', [8, 24, 40, 56]);
drumPins('tom', [60, 61, 62, 63], [.55, .7, .85, 1]);
drumPins('rim', [14, 30, 46, 52], [.8, .6, .8, .5]);

/* ---- hats: the hat wheel, arms at 16th positions ---- */
const chhPad = pad(0, 0, 9, drum('hat', { level: .0, velSense: .2, cutoff: 8500, decay: .05 }));
const ohPad = pad(0, 0, 10, drum('hat', { level: .0, velSense: .2, cutoff: 6800, decay: .25 }));
{ const u = Math.PI * 1.05, D1 = 75, D2 = 108;
  Object.assign(chhPad, { x: Hw.x + Math.cos(u) * D1, y: Hw.y + Math.sin(u) * D1 });
  Object.assign(ohPad, { x: Hw.x + Math.cos(u) * D2, y: Hw.y + Math.sin(u) * D2 });
  // one revolution is half a bar = 8 sixteenths; arms at these steps (swung pickups)
  for (const st of [0, 2, 3, 4, 6, 7]) armHit(Hw, chhPad, S16(st), 20, 62);
  armHit(Hw, ohPad, S16(6), 20, 100); }

/* ---- sub on XS (16 bars) ---- */
disc(XS, { root: 5, octave: 1, range: 1, scale: 'minor' });
const PHIS = Math.PI * 0.5;
const subRd = fullReader(XS, PHIS, V.sub);
// chord roots by bar: F F Db Eb | F F Db Eb | F F Db Eb | F Db Eb C
const ROOT = [29, 29, 37, 39, 29, 29, 37, 39, 29, 29, 37, 39, 29, 37, 39, 36];
const SUBP = [[[0, 12], [12, 4]], [[0, 6], [8, 8]], [[0, 12], [14, 2]], [[0, 8], [8, 4], [12, 4]]];
for (let b = 0; b < 16; b++) for (const [s, l] of SUBP[b % 4]) pin(XS, PHIS, ringFor(XS.track, ROOT[b]), b + S16(s), l);

/* ---- wobble on XW (comb reader, 16 bars) ---- */
disc(XW, { root: 5, octave: 2, range: 1, scale: 'minor' });
const PHIW = Math.PI * 0.5;
// comb: 7 tines, minor scale from F2 -- the pin's place along the tine is its pitch
const wobRd = fullReader(XW, PHIW, V.wobble, { tines: 7, combBase: 41, combScale: 'minor' });
const WROOT = ROOT.map(m => m + 12);
// per bar: [step, len, ring offset above the bar's root]
const WOBP = [[[0, 14, 0]], [[0, 6, 0], [8, 8, 2]], [[0, 14, 0]], [[0, 6, 0], [8, 3, 3], [12, 4, 6]]];
for (let b = 0; b < 16; b++) for (const [s, l, o] of WOBP[b % 4]) pin(XW, PHIW, ringFor(XW.track, WROOT[b]) + o, b + S16(s), l);

/* ---- pad chords + riser on X2 ---- */
disc(X2, { root: 5, octave: 3, range: 2, scale: 'minor' });
const PHI2 = -Math.PI / 2 - .3;
const padRd = reader(X2, PHI2, ringFrac(X2.track, 0) - .02, ringFrac(X2.track, 11) + .02, V.pad);
const riseRd = reader(X2, PHI2, ringFrac(X2.track, 13) - .02, ringFrac(X2.track, 13) + .03, V.riser);
const CH = { F: [53, 56, 60, 65], Db: [61, 65, 68, 72], Eb: [63, 67, 70], C: [60, 63, 67, 72] };
const chordBars = [['F', 0, 32], ['Db', 32, 16], ['Eb', 48, 16]];
for (let g = 0; g < 4; g++) {
  const last = g === 3;
  const seq = last ? [['F', 0, 16], ['Db', 16, 16], ['Eb', 32, 16], ['C', 48, 16]] : chordBars;
  for (const [ch, st, len] of seq) for (const m of CH[ch]) pin(X2, PHI2, ringFor(X2.track, m), g * 4 + S16(st), len);
}
pin(X2, PHI2, 13, 6, 32);   // the riser: bars 6-8 and 22-24

/* ---- screech lead on X3 (16 bars; plays in the second half) ---- */
disc(X3, { root: 5, octave: 4, range: 2, scale: 'minpent' });
const PHI3 = -Math.PI / 2;
const leadRd = fullReader(X3, PHI3, V.lead, { tines: 0 });
const MOT = [[0, 3, 77], [6, 2, 77], [10, 4, 80], [16, 6, 84], [24, 2, 82], [28, 3, 80]];
for (let h = 0; h < 4; h++) for (const [s, l, m] of MOT) {
  const mm = (h === 3 && s === 28) ? 77 : m;
  pin(X3, PHI3, ringFor(X3.track, mm), 8 + h * 2 + S16(s), l);
}

/* ---- the song cogs: arrangement cams ---- */
const cams = [];
function cam(c, ang, fn, to, macro = 8) {
  const m = { id: id(), cog: c.id, ang, phase: 0, shape: 'drawn', prof: [], to: to ? to.id : null, macro, lo: 0, hi: 1 };
  m.phase = +(((ang - c.th) / TAU) % 1 + 1).toFixed(6) % 1;
  const P = 1 / Math.abs(c.k);
  m.prof = Array.from({ length: 64 }, (_, i) => { const u = i / 64; let t = (-u / c.k) % P; if (t < 0) t += P; return +fn(t).toFixed(3); });
  W.cams.push(m); cams.push(m); return m;
}
// a gate on [a,b) bars with rampbars of ramp up before a, and down after b
const gate = (...spans) => t => { let v = 0; for (const [a, b, lvl = 1] of spans) {
  if (t >= a - .5 && t < a) v = Math.max(v, lvl * (t - (a - .5)) / .5); else if (t >= a && t < b) v = Math.max(v, lvl); else if (t >= b && t < b + .5) v = Math.max(v, lvl * (1 - (t - b) / .5)); } return v; };
const gDrop = gate([8, 16], [24, 32]);
const camDrop = cam(S1, -Math.PI / 2, gDrop, wobRd, 8);
const camHats = cam(S1, Math.PI * .75, gate([4, 8, .55], [8, 16, 1], [16, 24, .6], [24, 32, 1]));
const camPad = cam(S2, -Math.PI / 2, gate([0, 8, 1], [8, 16, .45], [16, 24, 1], [24, 32, .45]), padRd, 8);
const camLead = cam(S2, Math.PI / 2, gate([12, 16, .6], [24, 32, 1]), leadRd, 8);
const camBell = cam(S3, -Math.PI / 2, gate([0, 8, .6], [16, 24, 1], [24, 32, .35]));
// the 1/8 <-> triplet wobble switch and the growl fade: cams on the hub pulley (a 4-bar turn) and the growl wheel
// wobble rate: triplets in the last bar of each 4
const camRate = cam(H1, -Math.PI / 2, t => ((t % 4) >= 3 ? 1 : 0), wobRd, 2);
// the wub: a triangle cam on the quarter-note wheel; macro 1
const camWub = { id: id(), cog: Wc.id, ang: -Math.PI / 2, phase: 0, shape: 'triangle', prof: [], to: wobRd.id, macro: 1, lo: 0, hi: 1 };
camWub.phase = +(((camWub.ang - Wc.th) / TAU) % 1 + 1).toFixed(6) % 1; W.cams.push(camWub);
// growl: a ramp cam turning three times a bar, only matters in drop 2 (macro 3, gated by its own cam on S3)
const camGrowlGate = cam(S3, Math.PI / 2, gate([20, 24, .3], [24, 32, 1]), null, 3);
const camGrowl = { id: id(), cog: Wg.id, ang: -Math.PI / 2, phase: 0, shape: 'ramp down', prof: [], to: null, macro: 4, lo: 0, hi: 1 };
camGrowl.phase = +(((camGrowl.ang - Wg.th) / TAU) % 1 + 1).toFixed(6) % 1; W.cams.push(camGrowl);
for (const m of [camWub, camGrowl]) { const names = { triangle: t => 1 - Math.abs(2 * t - 1), 'ramp down': t => 1 - t }; m.prof = Array.from({ length: 64 }, (_, i) => +names[m.shape](i / 64).toFixed(3)); }
// camRate / camGrowlGate drive wobble macros 2 and 3, camWub macro 1, (camDrop macro 8)
camGrowl.to = lead => null; camGrowl.to = null;

// probes: arrangement cams level the drums and the pads
let probes = [];
function probe(host, to, o) { const p = { id: id(), host: host.kind, on: host.id, ball: 0, r: 0, a: 0, axis: 'reach', to, macro: 1, param: null, lo: 0, hi: 1, ...o }; W.probes.push(p); return p; }
const onCam = c => ({ kind: 'cam', id: c.id });
for (const [n, hi] of [['kick', 1], ['snare', 1], ['clap', 1], ['tom', 1], ['rim', 1]]) probe(onCam(camDrop), 'reader:' + drums[n].id, { param: 'level', hi: drums[n].voice.level / 1.5 });
probe(onCam(camDrop), 'reader:' + subRd.id, { macro: 8 });
probe(onCam(camHats), 'pad:' + chhPad.id, { param: 'level', hi: .42 / 1.5 });
probe(onCam(camHats), 'pad:' + ohPad.id, { param: 'level', hi: .35 / 1.5 });
probe(onCam(camGrowlGate), 'reader:' + wobRd.id, { macro: 3 });
// (camRate is linked to wobble macro 2 directly through cam.to)

/* ---- harp strings + the sphere, driven by the knocker wheel HW (k = 1/2) ---- */
const HWr0 = 14, HWlen = 100, Rt = HWr0 + HWlen;
const harpArm = [0, 1].map(i => { const arm = { id: id(), cog: HW.id, a: 0, r0: HWr0, len: HWlen, flex: .7 }; W.arms.push(arm); return arm; });
{ // a plucked fan: strings F3 Ab3 C4 F4 at bars 0, .1875, .375, .5625 of each bar... arm 2 half a turn later (k=1/2 => 2 bars/turn)
  const notes = [53, 56, 60, 65], f200 = 100;
  const times = [0, 0.1875, 0.375, 0.5625];
  harpArm[0].a = 0;   // arm 0 passes angle phi at time t: a = phi - th - 2pi k t  -> choose phi per string
  const base = 0.35 * Math.PI;   // where the fan starts (screen angle)
  notes.forEach((m, i) => {
    const f0 = 440 * Math.pow(2, (m - 69) / 12), L = f200 * 200 / f0, phi = base + TAU * HW.k * times[i];
    const u = .28, r1 = Rt - u * L, r2 = Rt + (1 - u) * L, ux = Math.cos(phi), uy = Math.sin(phi);
    W.strings.push({ id: id(), x1: HW.x + ux * r1, y1: HW.y + uy * r1, x2: HW.x + ux * r2, y2: HW.y + uy * r2, kind: 'string', f200, quant: true, decay: 2.2, damp: .9, hard: .5, level: 1.0, velSense: .2 });
  });
  harpArm[0].a = +angleAt(HW, base, 0).toFixed(5);
  harpArm[1].a = +angleAt(HW, base, 1).toFixed(5);   // a bar later (half a turn) it meets the fan again
}
// the sphere, in the arm's path on the far side
const PIT = { id: id(), x: 235, y: 785, w: 220, h: 220, voice: V.bell, notes: [65, 68, 70, 72, 75, 77], balls: [] };
{ const notes = [65, 72, 75, 77], R0 = 55;
  notes.forEach((m, i) => { const a = i * 1.1; PIT.balls.push({ id: i + 1, note: m, bx: PIT.x + Math.cos(a) * R0, by: PIT.y + Math.sin(a) * R0, bvx: Math.cos(a + 1.3) * 90, bvy: Math.sin(a + 1.3) * 90 }); }); }
W.pits.push(PIT);
probe({ kind: 'ball', id: PIT.id }, 'reader:' + padRd.id, { ball: 3, axis: 'y', macro: 1 });
probe(onCam(camBell), 'pit:' + PIT.id, { macro: 8 });

/* ---- crash on the song cog: arms at bar 8 and 24 ---- */
const crash = pad(S3.x + 5, S3.y - 78, 14, drum('cymbal', { level: .55, velSense: 0, cutoff: 5500, tone: .45, decay: 2.6 }));
for (const b of [8, 24]) armHit(S3, crash, b, 0, 70);

/* ---- FX fields: sound that happens inside them is thrown to the effect ---- */
const mid = r => ({ x: (r.x1 + r.x2) / 2, y: (r.y1 + r.y2) / 2 });
const FD = { diffuse: .6, level: .8, insert: 0, sync: '1/8.', time: .375, fb: .45, tone: .5, ping: 1, size: .6, decay: 2.5, predelay: 10, verbMode: 0, verbColor: 2, kind: 'tube', drive: .5 };
const field = (x, y, r, mode, o) => W.fields.push({ id: id(), x, y, r, mode, ...FD, ...o });
const pt = (c, phi, f) => ({ x: c.x + Math.cos(phi) * f * R(c.n), y: c.y + Math.sin(phi) * f * R(c.n) });
{ const tomP = pt(X1, PHI1, ringFrac(X1.track, 3)), snP = pt(X1, PHI1, ringFrac(X1.track, 1));
  field(tomP.x, tomP.y, 85, 'delay', { sync: '1/4.', fb: .5, tone: .35, diffuse: .8, level: .7 });          // tom and rim echoes thrown out
  field(snP.x, snP.y, 70, 'reverb', { size: .7, decay: 2.2, diffuse: .7, level: .8, insert: .2 });          // the snare lands in a room
  const wp = pt(XW, PHIW, .55); field(wp.x, wp.y, 110, 'dist', { kind: 'fold', drive: .6, tone: .5, diffuse: 1, level: .7, insert: .15 });   // low wobble notes drive harder
  const lp = pt(X3, PHI3, .6); field(lp.x, lp.y, 120, 'delay', { sync: '1/8.', fb: .45, diffuse: .8, level: .6 });
  const pp = pt(X2, PHI2, .6); field(pp.x, pp.y, 130, 'reverb', { size: .9, decay: 6, diffuse: .8, level: .9, predelay: 30 });
  field(PIT.x, PIT.y, 150, 'delay', { sync: '1/4.', fb: .55, tone: .4, diffuse: .9, level: .6 });
  field(PIT.x + 20, PIT.y - 20, 190, 'reverb', { size: .95, decay: 8, diffuse: .9, level: .8 }); }

/* ---- pads for the extras ---- */
// (hat pads sit where the arms of the hat wheel sweep)

/* ---------------- emit ---------------- */
for (const c of cogs) {
  const o = { id: c.id, x: +c.x.toFixed(3), y: +c.y.toFixed(3), n: c.n, motor: c.motor, layer: c.layer, th: +wrap(c.th).toFixed(5), track: c.track };
  W.cogs.push(o);
}
W.belts = belts;
for (const r of W.readers) { delete r.disc; delete r.phi; }
for (const f of [...W.cams]) { if (f.to && typeof f.to !== 'number') f.to = null; }
// view: fit the whole machine
let x0 = 1e9, x1 = -1e9, y0 = 1e9, y1 = -1e9;
for (const c of cogs) { const r = R(c.n); x0 = Math.min(x0, c.x - r); x1 = Math.max(x1, c.x + r); y0 = Math.min(y0, c.y - r); y1 = Math.max(y1, c.y + r); }
x0 = Math.min(x0, PIT.x - 110); x1 = Math.max(x1, PIT.x + 110); y1 = Math.max(y1, PIT.y + 110); y0 -= 70;
W.S.zoom = +Math.min(390 / (x1 - x0 + 24), 690 / (y1 - y0 + 30)).toFixed(3);
W.view = { x: +((x0 + x1) / 2).toFixed(1), y: +((y0 + y1) / 2 + 50 / W.S.zoom).toFixed(1) };
const out = path.join(__dirname, 'dubstep.json');
fs.writeFileSync(out, JSON.stringify(W));
console.log('wrote', out, fs.statSync(out).size, 'bytes; pins:', cogs.filter(c => c.track).map(c => c.track.dots.length).join(','), 'extent', Math.round(x1 - x0), Math.round(y1 - y0));
