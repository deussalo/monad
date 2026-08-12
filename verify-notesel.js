// Verification: custom note palette (radial keyboard). Dev tool, never shipped.
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

  const clickPad = async (label, nth = 0) => {
    const box = await page.evaluate(([l, n]) => {
      const els = [...document.querySelectorAll('.bloom.in .hitpad[aria-label="' + l + '"]')];
      const el = els[n]; if (!el) return null;
      const r = el.getBoundingClientRect();
      return { x: r.x + r.width / 2, y: r.y + r.height / 2 };
    }, [label, nth]);
    if (!box) throw new Error('no visible pad: ' + label + '#' + nth);
    await page.mouse.click(box.x, box.y);
    await page.waitForTimeout(380);
  };

  const out = {};
  // open shell -> voicing -> custom palette
  await page.mouse.click(195, 844 - 56);
  await page.waitForTimeout(500);
  await clickPad('voicing');
  out.voicingItems = await page.evaluate(() => document.querySelectorAll('#shellSvg .bloom.in').length);
  await clickPad('custom note palette');
  out.noteselItems = await page.evaluate(() => document.querySelectorAll('#shellSvg .bloom.in').length);
  if (out.noteselItems !== 16) errors.push('expected 16 notesel nodes, got ' + out.noteselItems);

  // no visible node may sit off-screen
  out.offscreen = await page.evaluate(() => {
    return [...document.querySelectorAll('#shellSvg .bloom.in')].filter(g => {
      const r = g.getBoundingClientRect();
      return r.left < 0 || r.right > innerWidth || r.top < 0;
    }).length;
  });
  if (out.offscreen) errors.push(out.offscreen + ' notesel nodes off-screen at 390px');

  // toggle three keys (C, E-ish, a black key)
  await clickPad('note', 0);
  await clickPad('note', 2);
  await clickPad('note', 8); // first black key node (index 7..11 are blacks)
  out.afterThree = await page.evaluate(() => ({
    mode: window.MonadVoicing.current,
    notes: window.MonadVoicing.getCustomNotes()
  }));
  if (out.afterThree.mode !== 'custom') errors.push('mode did not switch to custom');
  if (out.afterThree.notes.length !== 3) errors.push('expected 3 notes, got ' + out.afterThree.notes.length);

  await page.screenshot({ path: 'shot-notesel.png' });

  // octave up, add one more -> 4 notes, spanning two octaves
  await clickPad('octave up');
  await clickPad('note', 0);
  out.afterOct = await page.evaluate(() => window.MonadVoicing.getCustomNotes());
  if (out.afterOct.length !== 4) errors.push('expected 4 notes after octave-up add, got ' + out.afterOct.length);
  const octs = new Set(out.afterOct.map(m => Math.floor((m - 24) / 12)));
  if (octs.size < 2) errors.push('notes did not span two octaves: ' + out.afterOct);

  // untoggle works: tap the same key again
  await clickPad('note', 0);
  out.afterUntoggle = await page.evaluate(() => window.MonadVoicing.getCustomNotes().length);
  if (out.afterUntoggle !== 3) errors.push('untoggle failed, ' + out.afterUntoggle + ' notes');

  // scatter preserves pitch classes, may change octaves
  const pcsBefore = await page.evaluate(() => new Set(window.MonadVoicing.getCustomNotes().map(m => m % 12)).size);
  await clickPad('scatter across octaves');
  out.afterScatter = await page.evaluate(() => window.MonadVoicing.getCustomNotes());
  const pcsAfter = new Set(out.afterScatter.map(m => m % 12)).size;
  if (pcsAfter !== pcsBefore) errors.push('scatter changed pitch classes: ' + pcsBefore + ' -> ' + pcsAfter);

  // the pool is what the music generates from: 40 draws all come from the pool
  out.draws = await page.evaluate(() => {
    const pool = new Set(window.MonadVoicing.getCustomNotes());
    let bad = 0;
    for (let i = 0; i < 40; i++) if (!pool.has(window.MonadVoicing.nextNote())) bad++;
    return bad;
  });
  if (out.draws) errors.push(out.draws + ' nextNote() draws left the custom pool');

  // preset round-trip
  out.preset = await page.evaluate(() => {
    const p = window.__monadTest.buildPreset();
    const saved = p.voicing.custom.slice();
    window.MonadVoicing.setCustomNotes([60]);
    window.__monadTest.applyPresetObject(JSON.parse(JSON.stringify(p)));
    const back = window.MonadVoicing.getCustomNotes();
    return { saved, back, mode: window.MonadVoicing.current };
  });
  if (JSON.stringify(out.preset.saved) !== JSON.stringify(out.preset.back)) errors.push('preset round-trip lost custom notes');
  if (out.preset.mode !== 'custom') errors.push('preset round-trip lost custom mode');

  console.log(JSON.stringify({ out, errors }, null, 2));
  await browser.close();
  process.exit(errors.length ? 1 : 0);
})().catch(e => { console.error('HARNESS FAIL', e); process.exit(2); });
