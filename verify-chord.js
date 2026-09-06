// Verification: voicing catalogue revamp + tone wheel + chord builder.
// Dev tool, never shipped.
const { chromium } = require('/srv/rig/monad/node_modules/playwright-core');
const path = require('path');

(async () => {
  const browser = await chromium.launch({
    executablePath:'/srv/rig/.cache/ms-playwright/chromium-1187/chrome-linux/chrome',
    args: ['--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader','--mute-audio','--autoplay-policy=no-user-gesture-required']
  });
  const page = await browser.newPage({ viewport: { width: 390, height: 844 } });
  const errors = [];
  page.on('console', m => { if (m.type() === 'error') errors.push(m.text()); });
  page.on('pageerror', e => errors.push('PAGEERROR: ' + (e && e.stack || e)));

  await page.goto('file://' + path.join(__dirname, 'monad.html'));
  await page.waitForTimeout(900);

  const out = {};

  // ── catalogue: 6 voicings, the two keepers unchanged, no drift ──────────
  out.catalogue = await page.evaluate(() => window.MonadVoicing.modes.map(m => m.id));
  if (out.catalogue.length !== 6) errors.push('expected 6 catalogue modes, got ' + out.catalogue.length);
  if (out.catalogue[0] !== 'quartal' || out.catalogue[1] !== 'quintal') errors.push('first two modes changed: ' + out.catalogue);
  if (out.catalogue.includes('drift')) errors.push('drift survived the cut');
  out.keepers = await page.evaluate(() => ({
    quartal: window.MonadVoicing.modes[0].semitones,
    quintal: window.MonadVoicing.modes[1].semitones
  }));
  if (JSON.stringify(out.keepers.quartal) !== '[0,5,10,15,20,25]') errors.push('quartal intervals changed');
  if (JSON.stringify(out.keepers.quintal) !== '[0,7,14,15,22,29]') errors.push('quintal intervals changed');
  // no duplicated interval stack anywhere in the catalogue
  out.dupStacks = await page.evaluate(() => {
    const seen = new Set(); let dup = 0;
    for (const m of window.MonadVoicing.modes) {
      const k = m.semitones.join(',');
      if (seen.has(k)) dup++; seen.add(k);
    }
    return dup;
  });
  if (out.dupStacks) errors.push(out.dupStacks + ' duplicated voicing stacks in catalogue');
  // every note any mode can produce stays inside MIDI range at max transpose
  out.range = await page.evaluate(() => {
    const MV = window.MonadVoicing; const bad = [];
    for (const m of MV.modes) {
      MV.setMode(m.id);
      for (const t of [0,2,4,5,-5,-4,-3,-2]) {
        MV.setTranspose(t);
        for (const n of MV.noteSet()) if (n < 12 || n > 108) bad.push(m.id + '@' + t + ':' + n);
      }
    }
    MV.setTranspose(0);
    return bad;
  });
  if (out.range.length) errors.push('notes out of MIDI range: ' + out.range.join(' '));

  // ── transpose semantics: noteSet shifts as a block ──────────────────────
  out.transpose = await page.evaluate(() => {
    const MV = window.MonadVoicing;
    MV.setMode('quintal'); MV.setTranspose(0);
    const base = MV.noteSet();
    MV.setTranspose(5);
    const up = MV.noteSet();
    MV.setTranspose(-5);
    const down = MV.noteSet();
    MV.setTranspose(0);
    return {
      upOk: JSON.stringify(up) === JSON.stringify(base.map(n => n + 5)),
      downOk: JSON.stringify(down) === JSON.stringify(base.map(n => n - 5))
    };
  });
  if (!out.transpose.upOk) errors.push('transpose +5 did not shift noteSet as a block');
  if (!out.transpose.downOk) errors.push('transpose -5 did not shift noteSet as a block');
  // custom pool transposes too, without rewriting the stored selection
  out.customXp = await page.evaluate(() => {
    const MV = window.MonadVoicing;
    MV.setCustomNotes([48, 55, 60]); MV.setMode('custom');
    MV.setTranspose(2);
    const set = MV.noteSet(), stored = MV.getCustomNotes();
    MV.setTranspose(0);
    return { set, stored };
  });
  if (JSON.stringify(out.customXp.set) !== '[50,57,62]') errors.push('custom pool did not transpose: ' + out.customXp.set);
  if (JSON.stringify(out.customXp.stored) !== '[48,55,60]') errors.push('transpose rewrote the stored custom pool');

  // ── chordNotes: a playable six-note chord on a fixed mid root ───────────
  out.chord = await page.evaluate(() => {
    const MV = window.MonadVoicing;
    MV.setMode('quintal'); MV.setTranspose(0);
    const home = MV.chordNotes();
    MV.setTranspose(-5);
    const five = MV.chordNotes();
    MV.setTranspose(0);
    return { home, five };
  });
  if (JSON.stringify(out.chord.home) !== '[52,59,66,67,74,81]') errors.push('quintal chordNotes wrong: ' + out.chord.home);
  if (JSON.stringify(out.chord.five) !== '[47,54,61,62,69,76]') errors.push('transposed chordNotes wrong: ' + out.chord.five);

  // ── shell: voicing ring carries the tone wheel ──────────────────────────
  await page.evaluate(() => { window.__monadTest.applyPresetObject({voicing:{mode:'quintal',transpose:0}}); });
  await page.mouse.click(195, 844 - 56);
  await page.waitForTimeout(500);
  const pad = async (label, nth = 0) => {
    const box = await page.evaluate(([l, n]) => {
      const els = [...document.querySelectorAll('.bloom.in .hitpad[aria-label="' + l + '"]')];
      const el = els[n]; if (!el) return null;
      const r = el.getBoundingClientRect();
      return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
    }, [label, nth]);
    if (!box) throw new Error('no visible pad: ' + label + '#' + nth);
    return box;
  };
  const click = async (label, nth = 0) => {
    const b = await pad(label, nth);
    await page.mouse.click(b.x, b.y);
    await page.waitForTimeout(320);
  };
  await click('voicing');
  out.rings = await page.evaluate(() => ({
    nodes: document.querySelectorAll('#shellSvg .bloom.in').length,
    tones: document.querySelectorAll('.bloom.in .hitpad[aria-label="tone"]').length,
    voicings: document.querySelectorAll('.bloom.in .hitpad[aria-label="voicing"]').length
  }));
  if (out.rings.tones !== 8) errors.push('expected 8 wheel tones, got ' + out.rings.tones);
  if (out.rings.voicings !== 6) errors.push('expected 6 voicing pads, got ' + out.rings.voicings);
  // nothing off-screen on a narrow phone
  out.offscreen = await page.evaluate(() =>
    [...document.querySelectorAll('#shellSvg .bloom.in')].filter(g => {
      const r = g.getBoundingClientRect();
      return r.left < 0 || r.right > innerWidth || r.top < 0;
    }).length);
  if (out.offscreen) errors.push(out.offscreen + ' voicing-ring nodes off-screen at 390px');
  // wheel tones sit strictly inside the voicing arc
  out.nested = await page.evaluate(() => {
    const c = (el) => { const r = el.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; };
    const glyph = c(document.querySelector('#root-glyph'));
    const d = (el) => { const p = c(el); return Math.hypot(p.x - glyph.x, p.y - glyph.y); };
    const tone = Math.max(...[...document.querySelectorAll('.bloom.in .hitpad[aria-label="tone"]')].map(d));
    const voice = Math.min(...[...document.querySelectorAll('.bloom.in .hitpad[aria-label="voicing"]')].map(d));
    return { tone: Math.round(tone), voice: Math.round(voice) };
  });
  if (out.nested.tone >= out.nested.voice) errors.push('tone wheel is not inside the voicing arc: ' + JSON.stringify(out.nested));

  // ── chord builder: press a tone -> transpose + retune + chord strike ────
  // (tone index 4 is the fifth, wheel order [0,2,4,5,-5,-4,-3,-2])
  await click('tone', 4);
  out.afterTone = await page.evaluate(() => ({
    transpose: window.MonadVoicing.transpose,
    orbsRetuned: window.__monadTest.orbNotes.every(n => window.MonadVoicing.noteSet().includes(n))
  }));
  if (out.afterTone.transpose !== -5) errors.push('tone press set transpose ' + out.afterTone.transpose + ', expected -5');
  if (!out.afterTone.orbsRetuned) errors.push('tone press did not retune the world');
  // hold the tone, press a voicing: chord is built on the held root
  const toneBox = await pad('tone', 2), voiceBox = await pad('voicing', 3);
  await page.mouse.move(toneBox.x, toneBox.y);
  await page.mouse.down();
  await page.waitForTimeout(120);
  out.heldRoot = await page.evaluate(() => window.MonadVoicing.transpose);
  await page.mouse.up();
  await page.touchscreen.tap(voiceBox.x, voiceBox.y).catch(() => page.mouse.click(voiceBox.x, voiceBox.y));
  await page.waitForTimeout(320);
  out.built = await page.evaluate(() => ({
    mode: window.MonadVoicing.current,
    transpose: window.MonadVoicing.transpose
  }));
  if (out.heldRoot !== 4) errors.push('held tone did not set root on pointerdown (got ' + out.heldRoot + ')');
  if (out.built.transpose !== 4) errors.push('voicing press lost the held root');
  if (out.built.mode !== 'lydian') errors.push('voicing press picked ' + out.built.mode + ', expected lydian');

  await page.screenshot({ path: 'shot-tonewheel.png' });

  // ── preset round-trip keeps transpose ───────────────────────────────────
  out.preset = await page.evaluate(() => {
    const p = window.__monadTest.buildPreset();
    window.MonadVoicing.setTranspose(0);
    window.__monadTest.applyPresetObject(JSON.parse(JSON.stringify(p)));
    return { saved: p.voicing.transpose, back: window.MonadVoicing.transpose };
  });
  if (out.preset.saved !== 4 || out.preset.back !== 4) errors.push('preset round-trip lost transpose: ' + JSON.stringify(out.preset));

  console.log(JSON.stringify({ out, errors }, null, 2));
  await browser.close();
  process.exit(errors.length ? 1 : 0);
})().catch(e => { console.error('HARNESS FAIL', e); process.exit(2); });
