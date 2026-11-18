// Renders a cogs page offline, faster or slower than real time but without a dropout, through the page's own sim and
// worklets: an OfflineAudioContext stands in for the page's AudioContext, and rendering stops every 2048 frames so the
// sim can schedule ahead, as its 15 ms timer would. Writes raw stereo floats (little endian, 48 kHz) and an event log.
// node render-capstan.js <page.html> <out.f32> [fromBar] [bars] [bpm]; SOLO=<regex on an event's source> keeps only those parts
const { chromium } = require(process.env.COGS_PW || '/tmp/capstan/playwright-core');
const fs = require('fs'), path = require('path');
const [page_, out, from_ = '0', bars_ = '192', bpm_ = '119'] = process.argv.slice(2);
const fromBar = +from_, bars = +bars_, bpm = +bpm_, RATE = 48000;
(async () => {
  const b = await chromium.launch({ executablePath: process.env.MONAD_CHROME || '/srv/rig/.cache/ms-playwright/chromium-1248/chrome-linux64/chrome', args: ['--no-sandbox'] });
  const ctx = await b.newContext({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });
  const p = await ctx.newPage(); const errs = [];
  p.on('pageerror', e => errs.push(String(e))); p.on('console', m => { if (m.type() === 'error') errs.push(m.text()); });
  const fd = fs.openSync(out, 'w');
  await p.exposeFunction('__chunk', b64 => { fs.writeSync(fd, Buffer.from(b64, 'base64')); });
  await p.exposeFunction('__say', s => console.log(s));
  await p.addInitScript(({ secs, RATE, solo }) => {
    const sI = window.setInterval.bind(window); window.setInterval = (f, ms, ...a) => ms === 15 ? 0 : sI(f, ms, ...a);
    window.requestAnimationFrame = () => 0;
    window.__secs = secs; window.__solo = solo ? new RegExp(solo) : null;
    window.AudioContext = function () {
      const o = new OfflineAudioContext(2, Math.ceil(secs * RATE), RATE), resume = o.resume.bind(o);
      Object.defineProperty(o, 'state', { get: () => 'running' });
      o.resume = () => Promise.resolve(); o.__resume = resume; return o;
    };
  }, { secs: bars * 4 * 60 / bpm, RATE, solo: process.env.SOLO || '' });
  await p.goto('file://' + path.resolve(page_));
  const info = await p.evaluate(async ({ fromBar, bpm }) => {
    initAudio();
    for (let i = 0; i < 400 && engineState !== 'ready'; i++) await new Promise(r => setTimeout(r, 50));
    await new Promise(r => setTimeout(r, 300));
    const c = window.__cogs;
    if (engineState !== 'ready') throw new Error('engine: ' + engineState);
    toStart();
    // start at a later bar: every plate is where it would be
    const sec = fromBar * 4 * 60 / bpm;
    for (const q of W.cogs) { turn(q, sec); q.th0 = q.th; }
    syncClock(); setPaused(false); postFields(); postMotions(); listen(true);
    window.__events = [];
    const solo = window.__solo; const real = fire; fire = (v, t, vn, at, src, gain, play) => { if (solo && !solo.test(src)) return; window.__events.push([+(t + sec).toFixed(4), src, +vn.toFixed(0), +(gain * v.level).toFixed(3)]); return real(v, t, vn, at, src, gain, play); };
    return { jam: W.cogs.filter(q => q.jam).map(q => q.id), w: W.cogs.map(q => [q.id, +(q.w / (Math.PI * 2) * 4 * 60 / bpm).toFixed(6)]), overlaps: overlaps(), fields: W.fields.map(f => f.slot), bounds: bounds(), zoom: S.zoom };
  }, { fromBar, bpm });
  console.log(JSON.stringify(info));
  const t0 = Date.now();
  await p.evaluate(async () => {
    const STEP = 2048 / 48000, N = Math.floor(ac.length / 2048);
    let i = 1;
    const beat = () => { tick(); postFields(); postMotions(); };
    const arm = () => { const q = i++; if (q >= N) return; ac.suspend(q * STEP).then(() => { beat(); if (q % 2000 === 0) __say(`rendered ${(q * STEP).toFixed(0)} s`); arm(); ac.__resume(); }); };
    beat(); arm();
    const buf = await ac.startRendering();
    window.__buf = buf;
  });
  console.log('rendered in', ((Date.now() - t0) / 1000).toFixed(0), 's');
  const n = await p.evaluate(() => __buf.length), CH = 48000 * 10;
  for (let s = 0; s < n; s += CH) {
    await p.evaluate(async ([s, CH]) => {
      const L = __buf.getChannelData(0).subarray(s, s + CH), R = __buf.getChannelData(1).subarray(s, s + CH), o = new Float32Array(L.length * 2);
      for (let i = 0; i < L.length; i++) { o[2 * i] = L[i]; o[2 * i + 1] = R[i]; }
      const u = new Uint8Array(o.buffer); let t = ''; for (let i = 0; i < u.length; i += 0x8000) t += String.fromCharCode.apply(null, u.subarray(i, i + 0x8000));
      await __chunk(btoa(t));
    }, [s, CH]);
  }
  fs.closeSync(fd);
  const ev = await p.evaluate(() => ({ events: __events, stats: { late: stats.late, skipped: stats.skipped, n: stats.events } }));
  fs.writeFileSync(out + '.events.json', JSON.stringify(ev));
  console.log('events', ev.events.length, 'late', ev.stats.late, 'skipped', ev.stats.skipped, 'page errors', JSON.stringify(errs.slice(0, 5)));
  await b.close();
})().catch(e => { console.error(e); process.exit(1); });
