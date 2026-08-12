#!/usr/bin/env node
/**
 * strike-test.js — "every impact must sound" acceptance test for monad.html.
 *
 * Measures the ratio of physics collision events that actually produce audio.
 * Acceptance: sounded/collision ratio ≈ 1.0, voiceDrops = 0 under a 12-body storm.
 *
 * Key constraint: audio is armed via a REAL touchscreen tap on #audio.
 * The --autoplay-policy flag is intentionally absent — a real gesture must
 * activate the AudioContext at pointerup, exactly as on a phone.
 *
 * Run:
 *   LD_LIBRARY_PATH=/srv/rig/rig/webapp/scripts/playwright-libs/lib \
 *     node /srv/rig/monad/parts/strike-test.js
 *
 * Exit: 0 = pass, 1 = fail (ratio < 0.9 or voiceDrops > 0), 2 = harness error.
 *
 * Stats required in monad.html (see strike-patch.md Change G):
 *   window.__monadAudioStats() must include collisionCount and strikeCount.
 *   If they are absent (pre-patch), the test reports the raw voiceDrops/modalDrops
 *   only and exits with code 0 (informational run, not a pass/fail verdict).
 */

'use strict';

const { chromium } = require(
  '/srv/rig/rig/webapp/node_modules/.pnpm/node_modules/playwright'
);

const CHROME   = '/srv/rig/.cache/ms-playwright/chromium-1187/chrome-linux/chrome';
const LIB_PATH = '/srv/rig/rig/webapp/scripts/playwright-libs/lib';
const MONAD_URL = 'file:///srv/rig/monad/monad.html';

/* ── Test parameters ─────────────────────────────────────────────────────── */
const SPAWN_COUNT       = 12;
const SPAWN_PAUSE_MS    = 80;
const SETTLE_MS         = 1200;   // let initial spawn collisions settle
const STORM_DURATION_MS = 10000;  // 10-second storm
const POLL_MS           = 300;

/* Acceptance thresholds */
const MIN_RATIO         = 0.90;   // sounded/collision must be ≥ this
const MAX_VOICE_DROPS   = 0;      // zero tolerance under 12-body storm

function sleep(ms) { return new Promise(r => setTimeout(r, ms)); }

function spawnPositions(n) {
  /* Scatter across central third of 390×844 viewport — maximises collisions */
  const pts = [];
  const cols = 4, xBase = 60, xStep = 68, yBase = 240, yStep = 100;
  for (let i = 0; i < n; i++) {
    pts.push({
      x: xBase + (i % cols) * xStep + (Math.floor(i / cols) & 1) * 28,
      y: yBase + Math.floor(i / cols) * yStep,
    });
  }
  return pts;
}

async function run() {
  console.log('=== monad strike-test: every-impact-must-sound ===\n');

  /* ── Launch — NO --autoplay-policy flag ──────────────────────────────────── */
  const browser = await chromium.launch({
    executablePath: CHROME,
    env: { ...process.env, LD_LIBRARY_PATH: LIB_PATH },
    args: [
      '--use-gl=angle',
      '--use-angle=swiftshader',
      '--enable-unsafe-swiftshader',
      /* NOTE: --autoplay-policy=no-user-gesture-required is intentionally
         absent.  The AudioContext must be unlocked by a real user gesture. */
    ],
    headless: true,
  });

  const context = await browser.newContext({
    hasTouch: true,
    isMobile: true,
    viewport: { width: 390, height: 844 },
  });

  const page = await context.newPage();

  /* Collect JS errors for diagnosis */
  const jsErrors = [];
  page.on('pageerror', err => jsErrors.push(err.message));
  page.on('console',   msg => { if (msg.type() === 'error') jsErrors.push(msg.text()); });

  /* ── Navigate ──────────────────────────────────────────────────────────── */
  console.log('  loading monad.html …');
  await page.goto(MONAD_URL, { waitUntil: 'load' });
  await sleep(500);  // let rAF loop + physics init settle

  /* ── Inject frame-timing monitor (BEFORE arming audio) ─────────────────── */
  await page.evaluate(() => {
    window.__strikeTest = { overBudget: 0, _lastT: 0 };
    const origRAF = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = function (cb) {
      return origRAF(function (t) {
        if (window.__strikeTest._lastT && t - window.__strikeTest._lastT > 25)
          window.__strikeTest.overBudget++;
        window.__strikeTest._lastT = t;
        cb(t);
      });
    };
  });

  /* ── Arm audio via a REAL touchscreen tap ────────────────────────────────
   *
   * Playwright's touchscreen.tap() generates touchstart → touchend → click.
   * Chromium grants AudioContext activation at pointerup (= touchend here),
   * so ac.resume() inside the click handler runs in an activated context.
   * We do NOT use page.evaluate(() => document.getElementById('audio').click())
   * because a synthetic click dispatched from JS does not count as a user
   * gesture.
   */
  console.log('  arming audio via touchscreen tap on #audio …');

  /* Locate #audio element bounding box so we can tap its centre */
  const audioBox = await page.$eval('#audio', el => {
    const r = el.getBoundingClientRect();
    return { cx: r.left + r.width / 2, cy: r.top + r.height / 2 };
  });
  await page.touchscreen.tap(audioBox.cx, audioBox.cy);

  /* Wait for AudioContext to actually reach 'running'.
   * ac.resume() is async; poll with a tight deadline. */
  let acRunning = false;
  const acDeadline = Date.now() + 5000;
  while (Date.now() < acDeadline) {
    const state = await page.evaluate(() =>
      typeof window.__monadAudio === 'function' ? window.__monadAudio() : 'none'
    );
    if (state === 'running') { acRunning = true; break; }
    await sleep(120);
  }

  if (!acRunning) {
    const state = await page.evaluate(() =>
      typeof window.__monadAudio === 'function' ? window.__monadAudio() : 'none'
    );
    console.log(`\n  FAIL: AudioContext did not reach 'running' (state=${state})`);
    console.log('  The real-gesture tap did not activate the AudioContext.');
    console.log('  Check that #audio is visible and tappable at the measured coordinates.');
    await browser.close();
    process.exit(2);
  }
  console.log('  AudioContext: running ✓  (activated by real gesture)');

  /* ── Open dev diagnostics so updateVoiceStat writes DOM ─────────────────── */
  await page.evaluate(() => {
    const dev = document.getElementById('dev');
    if (dev) dev.hidden = false;
  });

  /* ── Read baseline stats ─────────────────────────────────────────────────── */
  const baseline = await page.evaluate(() => window.__monadAudioStats());
  console.log('  baseline stats:', JSON.stringify(baseline));

  const hasRatioStats = typeof baseline.collisionCount === 'number' &&
                        typeof baseline.strikeCount    === 'number';
  if (!hasRatioStats) {
    console.log('\n  NOTE: collisionCount / strikeCount not present in __monadAudioStats.');
    console.log('  Apply strike-patch.md Change G to monad.html, then re-run for ratio verdict.');
    console.log('  Continuing in informational mode (voiceDrops / modalDrops only).\n');
  }

  /* ── Maximise wind for collision storm ──────────────────────────────────── */
  await page.evaluate(() => {
    const w = document.getElementById('wind');
    if (w) { w.value = w.max || '1'; w.dispatchEvent(new Event('input', { bubbles: true })); }
  });

  /* ── Spawn 12 orbs ───────────────────────────────────────────────────────── */
  console.log(`  spawning ${SPAWN_COUNT} orbs …`);
  for (const pos of spawnPositions(SPAWN_COUNT)) {
    await page.touchscreen.tap(pos.x, pos.y);
    await sleep(SPAWN_PAUSE_MS);
  }
  await sleep(SETTLE_MS);

  /* ── Swipe storm loop ────────────────────────────────────────────────────── */
  console.log(`  storm running for ${STORM_DURATION_MS / 1000} s …`);
  const stormStart = Date.now();
  let swipeDir = 1;

  const stormLoop = (async () => {
    while (Date.now() - stormStart < STORM_DURATION_MS) {
      const y   = 310 + (swipeDir > 0 ? 0 : 80);
      const xA  = 55, xB = 330;
      await page.mouse.move(swipeDir > 0 ? xA : xB, y);
      await page.mouse.down();
      const steps = 10;
      for (let s = 0; s <= steps; s++) {
        const frac  = s / steps;
        const xCur  = swipeDir > 0 ? xA + (xB - xA) * frac : xB - (xB - xA) * frac;
        await page.mouse.move(xCur, y);
        await sleep(6);
      }
      await page.mouse.up();
      swipeDir = -swipeDir;
      await sleep(55);
    }
  })();

  /* ── Poll live stats while storm runs ───────────────────────────────────── */
  while (Date.now() - stormStart < STORM_DURATION_MS) {
    const stats = await page.evaluate(() => window.__monadAudioStats());
    const elapsed = ((Date.now() - stormStart) / 1000).toFixed(1);
    if (hasRatioStats) {
      const delta_c = stats.collisionCount - baseline.collisionCount;
      const delta_s = stats.strikeCount    - baseline.strikeCount;
      const ratio   = delta_c > 0 ? (delta_s / delta_c).toFixed(3) : '—';
      process.stdout.write(
        `\r  t=${elapsed}s  collisions=${delta_c}  strikes=${delta_s}` +
        `  ratio=${ratio}  vDrops=${stats.voiceDrops - baseline.voiceDrops}   `
      );
    } else {
      process.stdout.write(
        `\r  t=${elapsed}s  vDrops=${stats.voiceDrops}  mDrops=${stats.modalDrops}   `
      );
    }
    await sleep(POLL_MS);
  }

  await stormLoop;
  console.log();

  /* ── Final stats ────────────────────────────────────────────────────────── */
  const final = await page.evaluate(() => window.__monadAudioStats());
  await browser.close();

  const dCollisions = hasRatioStats ? final.collisionCount - baseline.collisionCount : null;
  const dStrikes    = hasRatioStats ? final.strikeCount    - baseline.strikeCount    : null;
  const dVDrops     = final.voiceDrops   - baseline.voiceDrops;
  const dMDrops     = final.modalDrops   - baseline.modalDrops;
  const ratio       = (hasRatioStats && dCollisions > 0)
                        ? dStrikes / dCollisions
                        : null;

  console.log('\n=== RESULTS ===');
  if (hasRatioStats) {
    console.log(`  Collision attempts (queueHit calls) : ${dCollisions}`);
    console.log(`  Strikes that sounded                : ${dStrikes}`);
    console.log(`  Sounded / attempted ratio           : ${ratio.toFixed(4)}`);
  }
  console.log(`  voiceDrops (pool exhausted)         : ${dVDrops}`);
  console.log(`  modalDrops (modal pool exhausted)   : ${dMDrops}`);
  console.log(`  Frames > 25 ms                      : ${await page.evaluate(() => window.__strikeTest?.overBudget ?? 'n/a').catch(() => 'n/a')}`);
  console.log(`  AudioContext final state             : ${final.state}`);

  if (jsErrors.length) {
    console.log('\n  JS errors during test:');
    jsErrors.slice(0, 8).forEach(e => console.log('    ' + e));
  }

  /* ── Verdict ─────────────────────────────────────────────────────────────── */
  if (!hasRatioStats) {
    console.log('\n  INFORMATIONAL (no ratio stats): apply strike-patch.md Change G first.');
    process.exit(0);
  }

  let pass = true;
  const fails = [];

  if (ratio < MIN_RATIO) {
    fails.push(`ratio ${ratio.toFixed(4)} < threshold ${MIN_RATIO}`);
    pass = false;
  }
  if (dVDrops > MAX_VOICE_DROPS) {
    fails.push(`voiceDrops=${dVDrops} (threshold ${MAX_VOICE_DROPS})`);
    pass = false;
  }
  if (final.state !== 'running') {
    fails.push(`AudioContext ended in state '${final.state}' (expected 'running')`);
    pass = false;
  }
  if (dCollisions < 10) {
    /* Sanity check: if the storm produced too few events the test is invalid */
    fails.push(`only ${dCollisions} collision events — storm may not have fired (physics or audio inactive?)`);
    pass = false;
  }

  if (fails.length) {
    console.log('\nFAIL:');
    fails.forEach(f => console.log('  ' + f));
  } else {
    console.log(`\nPASS  ratio=${ratio.toFixed(4)} ≥ ${MIN_RATIO}, voiceDrops=${dVDrops}`);
  }

  process.exit(pass ? 0 : 1);
}

run().catch(err => {
  console.error('HARNESS ERROR:', err);
  process.exit(2);
});
