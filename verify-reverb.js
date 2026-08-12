// Verification: reverb stability at hostile settings. Dev tool, never shipped.
// Drives the real audio graph: max feedback + max decay + max low/high decay
// ratios, injects an impulse, and asserts the tail neither grows nor
// accumulates infrasonic energy. Before the per-band loop-gain cap and the
// in-loop DC blocker, this configuration self-oscillated at the low crossover.
const { chromium } = require('/srv/rig/monad/node_modules/playwright-core');
const path = require('path');

(async () => {
  const browser = await chromium.launch({
    executablePath:'/srv/rig/.cache/ms-playwright/chromium-1187/chrome-linux/chrome',
    args: ['--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader','--mute-audio','--autoplay-policy=no-user-gesture-required']
  });
  const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
  const errors = [];
  page.on('pageerror', e => errors.push('PAGEERROR: ' + (e && e.stack || e)));

  await page.goto('file://' + path.join(__dirname, 'monad.html'));
  await page.waitForTimeout(800);
  // gesture arms audio
  await page.mouse.click(700, 400);
  await page.waitForTimeout(600);

  const result = await page.evaluate(() => new Promise(res => {
    const B = window.__monadBuses;
    if (!B || !B.ac) return res({ fail: 'audio not armed' });
    const ac = B.ac;

    // hostile settings
    const R = window.__monadTest.R;
    R.feedback = 100; R.decay = 120; R.lowRatio = 4; R.highRatio = 2;
    R.damping = 20000; R.diffusion = .86; R.wet = 1.2; R.input = 1.2;
    window.__monadTest.applyReverb();

    const an = ac.createAnalyser();
    an.fftSize = 8192;
    B.wetBus.connect(an);

    // impulse into the reverb input
    setTimeout(() => {
      const len = Math.round(ac.sampleRate * 0.005);
      const buf = ac.createBuffer(1, len, ac.sampleRate);
      const ch = buf.getChannelData(0);
      for (let i = 0; i < len; i++) ch[i] = (1 - i / len) * 0.8;
      const src = ac.createBufferSource();
      src.buffer = buf;
      src.connect(B.revIn);
      src.start();
    }, 300);

    const td = new Float32Array(an.fftSize);
    const fd = new Float32Array(an.frequencyBinCount);
    const rms = [];
    let peak = 0, nonFinite = 0, lowBinPeak = -200;
    const t0 = performance.now();
    const iv = setInterval(() => {
      an.getFloatTimeDomainData(td);
      let s = 0;
      for (let i = 0; i < td.length; i++) {
        const v = td[i];
        if (!isFinite(v)) nonFinite++;
        else { s += v * v; if (Math.abs(v) > peak) peak = Math.abs(v); }
      }
      rms.push({ t: performance.now() - t0, rms: Math.sqrt(s / td.length) });
      if (performance.now() - t0 > 3800) {
        // spectral check late in the tail: nothing infrasonic may persist
        an.getFloatFrequencyData(fd);
        const hzPerBin = ac.sampleRate / an.fftSize;
        const lowTop = Math.max(1, Math.floor(18 / hzPerBin));
        for (let i = 0; i <= lowTop; i++) if (fd[i] > lowBinPeak) lowBinPeak = fd[i];
      }
      if (performance.now() - t0 > 6200) {
        clearInterval(iv);
        const win = (a, b) => {
          const xs = rms.filter(r => r.t >= a && r.t < b).map(r => r.rms);
          return xs.length ? xs.reduce((p, c) => p + c, 0) / xs.length : 0;
        };
        res({
          early: win(600, 1800),      // tail right after the impulse
          late: win(4800, 6200),      // tail 5s in
          peak, nonFinite, lowBinPeak,
          samples: rms.length
        });
      }
    }, 200);
  }));

  const out = { ...result };
  if (result.fail) errors.push(result.fail);
  else {
    if (result.nonFinite) errors.push('non-finite samples in wet output');
    if (result.peak > 2) errors.push('wet peak ' + result.peak.toFixed(2) + ' > 2 — runaway');
    // At feedback=100/decay=120 the tail is near-unity by design; mod drift and
    // comb beating wobble the RMS a few dB. Real runaway compounds without
    // bound — a doubling over 4s (+6 dB) is the discriminating line.
    if (result.early > 1e-4 && result.late > result.early * 2)
      errors.push('tail grew: early ' + result.early.toExponential(2) + ' -> late ' + result.late.toExponential(2));
    if (result.lowBinPeak > -30)
      errors.push('infrasonic energy persists at ' + result.lowBinPeak.toFixed(1) + ' dBFS (<18 Hz)');
  }

  // Phase 2: moderate settings must actually DECAY — the reverb still works
  // as a reverb, not just "doesn't explode".
  const decayResult = await page.evaluate(() => new Promise(res => {
    const B = window.__monadBuses, ac = B.ac, R = window.__monadTest.R;
    R.feedback = 60; R.decay = 2; R.lowRatio = 1; R.highRatio = .3;
    window.__monadTest.applyReverb();
    const an = ac.createAnalyser(); an.fftSize = 8192;
    B.wetBus.connect(an);
    const td = new Float32Array(an.fftSize);
    const go = () => {
      const len = Math.round(ac.sampleRate * 0.005);
      const buf = ac.createBuffer(1, len, ac.sampleRate);
      const ch = buf.getChannelData(0);
      for (let i = 0; i < len; i++) ch[i] = (1 - i / len) * 0.8;
      const src = ac.createBufferSource();
      src.buffer = buf; src.connect(B.revIn); src.start();
      const t0 = performance.now(); const samples = [];
      const iv = setInterval(() => {
        an.getFloatTimeDomainData(td);
        let s = 0; for (let i = 0; i < td.length; i++) s += td[i] * td[i];
        samples.push({ t: performance.now() - t0, rms: Math.sqrt(s / td.length) });
        if (performance.now() - t0 > 4200) {
          clearInterval(iv);
          const win = (a, b) => { const xs = samples.filter(r => r.t >= a && r.t < b).map(r => r.rms); return xs.length ? xs.reduce((p, c) => p + c, 0) / xs.length : 0; };
          res({ early: win(300, 900), late: win(3400, 4200) });
        }
      }, 150);
    };
    // let the phase-1 near-infinite tail die down first at the new short decay
    setTimeout(go, 3000);
  }));
  out.decay = decayResult;
  if (decayResult.early > 1e-4 && decayResult.late > decayResult.early * 0.35)
    errors.push('2s-decay tail did not decay: ' + decayResult.early.toExponential(2) + ' -> ' + decayResult.late.toExponential(2));

  console.log(JSON.stringify({ out, errors }, null, 2));
  await browser.close();
  process.exit(errors.length ? 1 : 0);
})().catch(e => { console.error('HARNESS FAIL', e); process.exit(2); });
