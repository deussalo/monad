// The reverb (src/verb.js), offline in node, from the built page's worklet script: no slapback on a dry click or snare
// (prompt onset, dense echoes over the first 50 ms, no second attack), decay times that follow the knob, every mode and
// color finite and level, freeze, mode switches without a click, the engine's dry path, idling and warm-up, a migrated
// old room, and the render time of a 128-frame block.
const fs = require('fs'), path = require('path'), vm = require('vm');
let pass = true;
const check = (label, ok, got) => { if (!ok) pass = false; console.log(('  ' + label).padEnd(78), JSON.stringify(got), ok ? '✓' : '✗'); };

const page = fs.readFileSync(path.join(__dirname, 'cogs.html'), 'utf8');
const engineSrc = page.match(/<script type="text\/x-cogs-worklet" id="engine">([\s\S]*?)<\/script>/)[1];
const FS = 48000, B = 128;
const procs = {};
// the worklet's globals, in node's own context: a vm context reaches its globals through a proxy and runs 2x slower
const ctx = globalThis;
Object.assign(ctx, { sampleRate: FS, currentFrame: 0, registerProcessor: (n, c) => { procs[n] = c; },
  AudioWorkletProcessor: class { constructor() { this.port = { postMessage() {}, onmessage: null }; } } });
vm.runInThisContext(engineSrc + '\n;globalThis.Verb=Verb;globalThis.VERB_DEFAULT=VERB_DEFAULT;globalThis.VERB_MODES=VERB_MODES;globalThis.VERB_COLORS=VERB_COLORS;globalThis.VERB_PRESETS=VERB_PRESETS;globalThis.VERB_DEFS=VERB_DEFS;');
const { Verb, VERB_DEFAULT, VERB_MODES, VERB_COLORS, VERB_PRESETS, VERB_DEFS } = ctx;

// Run a signal through a reverb (wet only); returns the stereo wet.
function render(params, input, secs) {
  const v = new Verb(FS, { ...VERB_DEFAULT, ...params, wet: 1 }), N = Math.ceil(FS * secs / B) * B;
  const L = new Float32Array(N), R = new Float32Array(N), iL = new Float32Array(B), iR = new Float32Array(B), oL = new Float32Array(B), oR = new Float32Array(B);
  for (let b = 0; b < N / B; b++) {
    for (let i = 0; i < B; i++) { const x = input[b * B + i] || 0; iL[i] = x; iR[i] = x; }
    oL.fill(0); oR.fill(0); v.process(iL, iR, oL, oR, B); L.set(oL, b * B); R.set(oR, b * B);
  }
  return { L, R, v };
}
const ms = FS / 1000;
const click = new Float32Array(1); click[0] = 1;
// a dry snare as the drum pad makes it: band-passed noise and a 190 Hz body under a 160 ms decay
const snare = (() => {
  const n = Math.round(.2 * FS), x = new Float32Array(n); let seed = 1, b1 = 0, b2 = 0;
  for (let i = 0; i < n; i++) {
    seed = (seed * 1664525 + 1013904223) >>> 0; const w = seed / 2 ** 31 - 1;
    b1 += .35 * (w - b1); b2 += .2 * (b1 - b2); const env = Math.exp(-i / (FS * .16) * 6.9);
    x[i] = env * (.65 * (b1 - b2) * 3 + .35 * Math.sin(2 * Math.PI * 190 * i / FS));
  }
  return x;
})();
// normalised echo density (Abel & Huang): the share of samples past one standard deviation, over a Gaussian's 0.3173
function ned(x, a, b) { let m = 0; for (let i = a; i < b; i++) m += x[i] * x[i]; const sd = Math.sqrt(m / (b - a)); let c = 0; for (let i = a; i < b; i++) if (Math.abs(x[i]) > sd) c++; return c / (b - a) / .3173105; }
function highpass(x, hz) { const a = Math.exp(-2 * Math.PI * hz / FS), y = new Float32Array(x.length); let l1 = 0, l2 = 0;
  for (let i = 0; i < x.length; i++) { l1 = (1 - a) * x[i] + a * l1; const h = x[i] - l1; l2 = (1 - a) * h + a * l2; y[i] = h - l2; } return y; }
const envDb = (L, R, at, w) => { let e = 0; for (let i = at; i < at + w; i++) e += L[i] * L[i] + R[i] * R[i]; return 10 * Math.log10(e / w + 1e-30); };

console.log('SLAPBACK: a click and a snare through every mode, at the default knobs');
const modeRows = [];
for (let m = 0; m < VERB_MODES.length; m++) for (const color of [1, 2]) {
  const pre = VERB_DEFAULT.predelay * ms, { L, R } = render({ mode: m, color }, click, 1.1);
  const d = [10, 20, 30, 40].map(t => ned(L, Math.round(pre + t * ms), Math.round(pre + (t + 10) * ms)));
  const dens = d.reduce((a, b) => a + b) / d.length;
  // 1 ms bins over 5-60 ms: the loudest against the median, a lone echo stands out of the wash
  const bins = []; for (let t = 5; t < 60; t++) bins.push(envDb(L, R, Math.round(pre + t * ms), ms));
  const crest = Math.max(...bins) - [...bins].sort((a, b) => a - b)[bins.length >> 1];
  // onset: the first 2 ms bin within 20 dB of the loudest in the first second (the old room's came at ~0.7 s)
  const env = []; for (let t = 0; t < 1000; t += 2) env.push(envDb(L, R, Math.round(t * ms), 2 * ms));
  const top = Math.max(...env), onset = 2 * env.findIndex(e => e > top - 20);
  // the snare: no second attack. Above 500 Hz (where a slap is heard; below it a room's few low modes beat, as any
  // room's do) the wet's 20 ms envelope never climbs 3 dB back after it has peaked. An ideal reverb scores 1-2 dB here.
  const s = render({ mode: m, color }, snare, 1.1), hp = highpass(s.L, 500), hq = highpass(s.R, 500), se = [];
  for (let t = 0; t < 700; t += 5) se.push(envDb(hp, hq, Math.round(t * ms), 20 * ms));
  const peakAt = se.indexOf(Math.max(...se)); let rerise = 0, low = Infinity;
  for (let i = peakAt; i < se.length; i++) { low = Math.min(low, se[i]); rerise = Math.max(rerise, se[i] - low); }
  const s5 = []; for (let t = 0; t < 1000; t += 5) s5.push(envDb(s.L, s.R, Math.round(t * ms), 5 * ms));
  const sOn = 5 * s5.findIndex(e => e > Math.max(...s5) - 20);
  modeRows.push({ m: VERB_MODES[m] + ' ' + VERB_COLORS[color], dens, crest, onset, sOn, rerise });
}
const worst = k => modeRows.reduce((a, r) => (k === 'dens' ? r[k] < a[k] : r[k] > a[k]) ? r : a);
const f = x => +x.toFixed(2);
for (const r of modeRows) if (r.dens < .5 || r.crest > 10 || r.onset > VERB_DEFAULT.predelay + 15 || r.sOn > VERB_DEFAULT.predelay + 25 || r.rerise > 3)
  check(`${r.m}: dense, prompt, one attack`, false, { dens: f(r.dens), crest: f(r.crest), onset: r.onset, snareOnset: r.sOn, rerise: f(r.rerise) });
check('click: echo density 10-50 ms after the predelay ≥ 0.5 in every mode (worst)', worst('dens').dens >= .5, [worst('dens').m, f(worst('dens').dens)]);
check('click: no 1 ms bin in 5-60 ms stands > 10 dB over the median (worst)', worst('crest').crest <= 10, [worst('crest').m, f(worst('crest').crest)]);
check('click: the wet is up within 15 ms of the predelay (latest)', worst('onset').onset <= VERB_DEFAULT.predelay + 15, [worst('onset').m, worst('onset').onset + ' ms']);
check('snare: the wet is up within 25 ms of the predelay (an echo parts at ~50) (latest)', worst('sOn').sOn <= VERB_DEFAULT.predelay + 25, [worst('sOn').m, worst('sOn').sOn + ' ms']);
check('snare: after its peak the wet (>500 Hz) never climbs back 3 dB: no slapback (worst)', worst('rerise').rerise <= 3, [worst('rerise').m, f(worst('rerise').rerise) + ' dB']);
// the predelay knob is in milliseconds (the old room read it as seconds and played every tail ~0.7 s late)
{ const { L, R } = render({ mode: 3, color: 2, predelay: 40 }, click, .3); let on = 0; for (let i = 0; i < L.length; i++) if (Math.abs(L[i]) + Math.abs(R[i]) > 1e-4) { on = i; break; }
  check('predelay 40 means the first wet sample lands 40-45 ms after the dry', on >= 40 * ms && on <= 45 * ms, +(on / ms).toFixed(1) + ' ms'); }

console.log('EVERY MODE AND COLOR');
const level = [], t60 = [];
for (let m = 0; m < VERB_MODES.length; m++) for (let c = 0; c < 3; c++) {
  const { L, R } = render({ mode: m, color: c, decay: 2 }, click, 3);
  let e = 0, peak = 0, finite = true; for (let i = 0; i < L.length; i++) { e += L[i] * L[i] + R[i] * R[i]; peak = Math.max(peak, Math.abs(L[i]), Math.abs(R[i])); if (!Number.isFinite(L[i] + R[i])) finite = false; }
  level.push({ k: VERB_MODES[m] + ' ' + VERB_COLORS[c], db: 10 * Math.log10(e), peak, finite });
  if (m !== VERB_MODES.indexOf('nonlin')) {
    const E = new Float64Array(L.length); let acc = 0; for (let i = L.length - 1; i >= 0; i--) { acc += L[i] * L[i] + R[i] * R[i]; E[i] = acc; }
    let a = 0, b = 0; for (let i = 0; i < L.length; i++) { const d = 10 * Math.log10(E[i] / E[0]); if (!a && d < -5) a = i; if (d < -25) { b = i; break; } }
    t60.push({ k: VERB_MODES[m] + ' ' + VERB_COLORS[c], t: 3 * (b - a) / FS });
  }
}
check('all 60 finite, peak below 0.2 for a full-scale click', level.every(r => r.finite && r.peak < .2), f(Math.max(...level.map(r => r.peak))));
const dbs = level.map(r => r.db), mid = [...dbs].sort((a, b) => a - b)[dbs.length >> 1];
check('loudness within ±3 dB of the median across modes and colors', dbs.every(d => Math.abs(d - mid) <= 3), [f(Math.min(...dbs) - mid), f(Math.max(...dbs) - mid)]);
check('decay 2 s: every ring measures T60 1.7-2.4 s (Schroeder, -5 to -25 dB)', t60.every(r => r.t > 1.7 && r.t < 2.4), [f(Math.min(...t60.map(r => r.t))), f(Math.max(...t60.map(r => r.t)))]);
{ const d6 = render({ mode: 0, decay: 6 }, click, 6); const E = []; let acc = 0; for (let i = d6.L.length - 1; i >= 0; i--) { acc += d6.L[i] ** 2 + d6.R[i] ** 2; E[i] = acc; }
  let a = 0, b = 0; for (let i = 0; i < E.length; i++) { const d = 10 * Math.log10(E[i] / E[0]); if (!a && d < -5) a = i; if (d < -35) { b = i; break; } }
  const t = 2 * (b - a) / FS; check('decay 6 s: T60 5-7 s', t > 5 && t < 7, f(t)); }
for (const [name, p] of Object.entries(VERB_PRESETS)) {
  const { L } = render(p, snare, 1.5); let ok = true, pk = 0; for (const x of L) { if (!Number.isFinite(x)) ok = false; pk = Math.max(pk, Math.abs(x)); }
  if (!ok || pk > 1 || pk < 1e-3) check(`preset ${name} sounds`, false, pk);
}
check('every preset sounds, finite and below clipping', true, Object.keys(VERB_PRESETS).length + ' presets');
{ // nonlin: attack bends the gate from a cut-off decay to a reverse swell
  const shape = a => { const { L, R } = render({ mode: VERB_MODES.indexOf('nonlin'), attack: a, size: .5 }, click, .5); return [envDb(L, R, 20 * ms, 40 * ms), envDb(L, R, 200 * ms, 40 * ms)]; };
  const cut = shape(0), rev = shape(1); check('nonlin: attack 0 falls, attack 1 swells (dB early vs late)', cut[0] > cut[1] + 6 && rev[1] > rev[0] + 6, [cut.map(f), rev.map(f)]); }

console.log('FREEZE, SWITCHES, GLIDES');
{ const v = new Verb(FS, { ...VERB_DEFAULT, wet: 1, decay: 1 }), iL = new Float32Array(B), oL = new Float32Array(B), oR = new Float32Array(B);
  // the mean energy over the run, and the largest step between neighbouring samples (carried across runs)
  let last = 0;
  const run = (blocks, x) => { let e = 0, jump = 0; for (let b = 0; b < blocks; b++) { iL.fill(0); if (x) for (let i = 0; i < B; i++) iL[i] = Math.sin(i * .3 + b) * .3; oL.fill(0); oR.fill(0); v.process(iL, iL, oL, oR, B);
    for (let i = 0; i < B; i++) { jump = Math.max(jump, Math.abs(oL[i] - last)); last = oL[i]; } e += v.energy / blocks; } return { e, jump }; };
  run(100, true); v.set({ freeze: 1 }); run(375); const e0 = run(10).e, e1 = run(375 * 4).e;
  check('freeze holds the tail: energy after 4 s within 3 dB of where it froze', Math.abs(10 * Math.log10(e1 / e0)) < 3, f(10 * Math.log10(e1 / e0)));
  v.set({ freeze: 0 }); run(375 * 4); const e2 = run(10).e;
  check('unfrozen, it decays away (decay 1 s, 4 s later: below -60 dB)', 10 * Math.log10(e2 / e0 + 1e-30) < -60, f(10 * Math.log10(e2 / e0 + 1e-30)));
  run(100, true); const before = run(5, true).jump; v.set({ mode: 5, color: 0 }); const sw = run(4, true).jump, after = run(200, true) && run(5, true).jump;
  check('a mode and color switch mid-sound fades through silence: no step past the sounds\' own', sw <= Math.max(before, after) * 1.5 + 1e-3, [f(before), f(sw), f(after)]);
  v.set({ size: 0 }); const g = run(400, true); v.set({ size: 1, predelay: 300 }); const g2 = run(400, true);
  check('size and predelay sweeps stay finite and bounded', Number.isFinite(g.e + g2.e) && g2.jump < 1, f(g2.jump)); }
{ const v = new Verb(FS, { ...VERB_DEFAULT, wet: 1, shimmer: 1, decay: 30, modDepth: 1 }), iL = new Float32Array(B), oL = new Float32Array(B), oR = new Float32Array(B); const mx = [0, 0];
  for (let b = 0; b < 375 * 12; b++) { for (let i = 0; i < B; i++) iL[i] = b < 190 ? Math.random() - .5 : 0; oL.fill(0); oR.fill(0); v.process(iL, iL, oL, oR, B); for (const x of oL) { const k = b < 375 * 3 ? 0 : 1; mx[k] = Math.max(mx[k], Math.abs(x)); } }
  check('full shimmer at 30 s decay: regenerates, but stays bounded (peak < 4 over 12 s)', mx.every(Number.isFinite) && mx[1] < 4, mx.map(f)); }

console.log('ENGINE');
const SPACE = { dry: 1, ...VERB_DEFAULT };
const Engine = procs['cogs-engine'], outs = () => Array.from({ length: 9 }, () => [new Float32Array(B), new Float32Array(B)]);
const proc = (e, inp) => { const o = outs(); ctx.currentFrame += B; e.process(inp ? [[inp, inp]] : [], o); return o[0]; };
let calls = 0; const realProcess = Verb.prototype.process;
Verb.prototype.process = function (...a) { calls++; return realProcess.apply(this, a); };
const t0 = Date.now(), eng = new Engine({ processorOptions: { voices: 64, space: SPACE } }), warmMs = Date.now() - t0;
check('the engine warms up through the reverb\'s paths and resets clean', calls > 300 && eng.space.energy === 0, { calls, warmMs });
{ const e = new Engine({ processorOptions: { voices: 8, space: { ...SPACE, wet: 0 } } }), x = new Float32Array(B).map((_, i) => Math.sin(i)); const o = proc(e, x);
  check('wet 0: the engine\'s output is the dry input, sample for sample', o[0].every((y, i) => y === Math.fround(x[i])), o[0][5]); }
{ const e = new Engine({ processorOptions: { voices: 8, space: SPACE } }); const hit = new Float32Array(B); hit.set(snare.subarray(0, B));
  proc(e, hit); for (let b = 1; b < snare.length / B; b++) proc(e, snare.subarray(b * B, b * B + B).length === B ? snare.subarray(b * B, b * B + B) : null);
  let blocks = 0, tail = 0; calls = 0; while (!e.idle && blocks < 3000) { const o = proc(e); tail = Math.max(tail, Math.abs(o[0][0])); blocks++; }
  const idleCalls = calls; for (let b = 0; b < 200; b++) proc(e);
  check('after a snare the room rings, then the engine idles and stops running the reverb', e.idle && tail > 1e-4 && calls === idleCalls, { idleAfter: +(blocks * B / FS).toFixed(2) + ' s', callsWhileIdle: calls - idleCalls });
  e.port.onmessage({ data: { type: 'space', ...SPACE, freeze: 1 } }); proc(e, hit); for (let b = 0; b < 3000; b++) proc(e);
  check('frozen, the engine never idles', !e.idle, e.idle); }
Verb.prototype.process = realProcess;

console.log('OLD ROOMS');
{ const sim = fs.readFileSync(path.join(__dirname, 'src', 'sim.js'), 'utf8'), verb = fs.readFileSync(path.join(__dirname, 'src', 'verb.js'), 'utf8');
  const part = sim.slice(sim.indexOf('const SPACE={'), sim.indexOf('\n}', sim.indexOf('function migrateSpace')) + 2);
  const m = vm.runInNewContext(verb + '\nconst clamp=(x,a,b)=>Math.min(b,Math.max(a,x));\n' + part + '\nconst SPACE_DEFAULT={...SPACE};migrateSpace', {});
  const old = { dry: 1, wet: .24, predelay: 22, size: .75, decay: 1.9, lowMult: .8, highMult: .5, lowCross: 200, highCross: 4500, diffusion: .7, density: .6, modRate: .3, modDepth: .5, shimmer: 0, shimmerPitch: 12, freeze: 0, width: 1.2, duck: .55, lowCut: 180, highCut: 12000 };
  const n = m(old);
  check('an old room maps across: its keys renamed, the duck gone, mode and color at the defaults',
    n.wet === .24 && n.predelay === 22 && n.decay === 1.9 && n.bassMult === .8 && Math.abs(n.highShelf + 6.02) < .01 && n.early === .7 && n.late === .6 && !('duck' in n) && !('highMult' in n) && n.mode === 0 && n.size > .45 && n.size < .52,
    { size: f(n.size), highShelf: f(n.highShelf), mode: n.mode, color: n.color });
  check('a new room passes through, clamped', JSON.stringify(m({ mode: 5, color: 0, wet: 9 })) === JSON.stringify({ mode: 5, color: 0, wet: 1.5 }), m({ mode: 5, color: 0, wet: 9 })); }
check('VERB_DEFS lists every default key', VERB_DEFS.every(d => d[0] in VERB_DEFAULT) && Object.keys(VERB_DEFAULT).every(k => VERB_DEFS.some(d => d[0] === k)), VERB_DEFS.length);

console.log('CPU: render time of one 128-frame block (budget 2.67 ms at 48 kHz)');
const cpu = {};
for (const [name, p] of [['concert hall 1980s', { mode: 0, color: 1 }], ['concert hall now', { mode: 0, color: 2 }], ['plate 1970s', { mode: 2, color: 0 }],
  ['random space now', { mode: 5, color: 2 }], ['palace 1980s', { mode: 19, color: 1 }], ['nonlin', { mode: 14 }], ['hall + shimmer', { mode: 0, shimmer: .5 }]]) {
  const v = new Verb(FS, { ...VERB_DEFAULT, ...p }), iL = new Float32Array(B).map(() => Math.random() - .5), oL = new Float32Array(B), oR = new Float32Array(B);
  for (let i = 0; i < 2000; i++) v.process(iL, iL, oL, oR, B);
  const n = 8000, t = process.hrtime.bigint(); for (let i = 0; i < n; i++) v.process(iL, iL, oL, oR, B); cpu[name] = Number(process.hrtime.bigint() - t) / 1e3 / n;
}
for (const [k, us] of Object.entries(cpu)) console.log(('    ' + k).padEnd(28), us.toFixed(1).padStart(6), 'µs', (us / 26.67).toFixed(1).padStart(5) + '% of the block');
const most = Math.max(...Object.values(cpu));
check('the heaviest mode renders a block in under 12% of its budget (320 µs)', most < 320, +most.toFixed(1) + ' µs');

console.log(pass ? 'verb: all checks pass' : 'verb: FAILURES');
process.exit(pass ? 0 : 1);
