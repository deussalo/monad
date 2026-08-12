#!/usr/bin/env node
/**
 * audio-test.js — Playwright collision-storm test for monad.html audio engine.
 *
 * What it does:
 *   1. Loads monad.html in a headless mobile Chromium context (390×844, hasTouch)
 *   2. Arms the AudioContext via a tap on #audio
 *   3. Confirms window.__monadAudio() === 'running'
 *   4. Opens the dev diagnostics panel (makes updateVoiceStat write the DOM)
 *   5. Spawns ~12 orbs across the canvas by touch-tapping
 *   6. Maximises wind to drive continuous collisions
 *   7. Opens the shell voicing hub (tap glyph) while the storm runs
 *   8. Holds for 10 s, polling peak voice/partial counts every 200 ms
 *   9. Reports: peak voices, peak modal partials, AC state changes, over-budget frames
 *
 * Run:
 *   LD_LIBRARY_PATH=/srv/rig/rig/webapp/scripts/playwright-libs/lib \
 *     node /srv/rig/monad/parts/audio-test.js
 *
 * Exit code: 0 = pass, 1 = fail (AC interrupted or pool exceeded), 2 = harness error.
 */

'use strict';

/* ── Playwright (installed in rig webapp) ─────────────────────────────────── */
const { chromium } = require(
  '/srv/rig/rig/webapp/node_modules/.pnpm/node_modules/playwright'
);

const CHROME    = '/srv/rig/.cache/ms-playwright/chromium-1187/chrome-linux/chrome';
const LIB_PATH  = '/srv/rig/rig/webapp/scripts/playwright-libs/lib';
const MONAD_URL = 'file:///srv/rig/monad/monad.html';

/* ── Test parameters ─────────────────────────────────────────────────────── */
const SPAWN_COUNT        = 12;
const SPAWN_PAUSE_MS     = 100;   // gap between orb spawns (physics resolution)
const SETTLE_MS          = 1500;  // wait after spawning before storm clock starts
const STORM_DURATION_MS  = 10000; // 10 s collision storm
const POLL_INTERVAL_MS   = 200;   // voice-stat polling cadence

/* Pre-patch voice-pool ceiling; post-patch LOW_POWER modal max is 8*3=24 */
const VOICE_POOL_SIZE    = 64;    // hard ceiling: voices array length

function sleep(ms) { return new Promise(r => setTimeout(r, ms)); }

/* ── Helpers ─────────────────────────────────────────────────────────────── */
/** Scatter spawn positions across the central 2/3 of a 390×844 viewport. */
function spawnPositions(n) {
  const pts = [];
  const cols = 4;
  const xBase = 50, xStep = 72;
  const yBase = 220, yStep = 110;
  for (let i = 0; i < n; i++) {
    pts.push({
      x: xBase + (i % cols) * xStep + (Math.floor(i / cols) % 2) * 30,
      y: yBase + Math.floor(i / cols) * yStep,
    });
  }
  return pts;
}

/* ── Main ─────────────────────────────────────────────────────────────────── */
async function run() {
  console.log('=== monad audio-test: collision-storm ===\n');

  /* ── Launch Chromium ───────────────────────────────────────────────────── */
  const browser = await chromium.launch({
    executablePath: CHROME,
    env: { ...process.env, LD_LIBRARY_PATH: LIB_PATH },
    args: [
      '--use-gl=angle',
      '--use-angle=swiftshader',
      '--enable-unsafe-swiftshader',
      '--autoplay-policy=no-user-gesture-required',  // allow AudioContext in file://
    ],
    headless: true,
  });

  const context = await browser.newContext({
    hasTouch: true,
    isMobile: true,
    viewport: { width: 390, height: 844 },
  });

  const page = await context.newPage();

  /* Capture console for debugging */
  const consoleLines = [];
  page.on('console', msg => { if (msg.type() === 'error') consoleLines.push(msg.text()); });
  page.on('pageerror', err => consoleLines.push('PAGE ERROR: ' + err.message));

  /* ── Load page ─────────────────────────────────────────────────────────── */
  console.log('  loading monad.html …');
  await page.goto(MONAD_URL, { waitUntil: 'load' });
  await sleep(600);   // let rAF loop start

  /* ── Inject frame-time monitor (before audio starts) ─────────────────── */
  await page.evaluate(() => {
    window.__storm = {
      peakVoices:  0,
      peakModals:  0,
      overBudget:  0,   // frames where dt > 25 ms
      acStateLog:  [],  // {t, from, to} entries
      _lastRafT:   0,
    };

    const origRAF = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = function (cb) {
      return origRAF(function (t) {
        if (window.__storm._lastRafT) {
          if (t - window.__storm._lastRafT > 25) window.__storm.overBudget++;
        }
        window.__storm._lastRafT = t;
        cb(t);
      });
    };

    /* Poll AC state every 250 ms so we catch async suspensions */
    window.__storm._acPoll = setInterval(() => {
      if (typeof window.__monadAudio !== 'function') return;
      const state = window.__monadAudio();
      const log   = window.__storm.acStateLog;
      const prev  = log.length ? log[log.length - 1].to : 'running';
      if (state !== prev) log.push({ t: performance.now(), from: prev, to: state });
    }, 250);
  });

  /* ── Open diagnostics (needed for updateVoiceStat to write DOM) ────────── */
  await page.evaluate(() => {
    const dev = document.getElementById('dev');
    if (dev) dev.hidden = false;
  });

  /* ── Arm audio ─────────────────────────────────────────────────────────── */
  console.log('  arming audio …');
  await page.tap('#audio');

  let acRunning = false;
  try {
    await page.waitForFunction(
      () => typeof window.__monadAudio === 'function' &&
            window.__monadAudio() === 'running',
      { timeout: 6000 }
    );
    acRunning = true;
    console.log('  AudioContext: running ✓');
  } catch (_) {
    console.log('  AudioContext: FAILED to reach running state');
  }

  /* ── Maximise wind ─────────────────────────────────────────────────────── */
  await page.evaluate(() => {
    /* elWind is scoped to the page — reach it via the DOM element */
    const w = document.getElementById('wind');
    if (w) { w.value = w.max || '1'; w.dispatchEvent(new Event('input', { bubbles: true })); }
  });

  /* ── Spawn orbs ────────────────────────────────────────────────────────── */
  console.log(`  spawning ${SPAWN_COUNT} orbs …`);
  const positions = spawnPositions(SPAWN_COUNT);
  for (const pos of positions) {
    await page.touchscreen.tap(pos.x, pos.y);
    await sleep(SPAWN_PAUSE_MS);
  }

  /* ── Open voicing hub (single tap on the glyph — reaches state=1) ───────
   *  The glyph (#root-glyph .hitpad) is positioned by layoutShell().
   *  A safe hit: bottom-right of the SVG layer, which layoutShell places the
   *  glyph at.  monad.html positions rootG at roughly (W-56, H-56) in CSS px.
   */
  await sleep(SETTLE_MS);
  await page.touchscreen.tap(334, 788);   // approx glyph position at 390×844
  await sleep(400);

  /* ── Collision storm ─────────────────────────────────────────────────────
   *  Continuously drag across canvas to maximise collision events.
   *  Each drag-through sweeps 6 contact points.                            */
  console.log(`  storm running for ${STORM_DURATION_MS / 1000} s …`);
  const stormStart = Date.now();
  let swipeDir = 1;

  const stormLoop = (async () => {
    while (Date.now() - stormStart < STORM_DURATION_MS) {
      /* Sweep a horizontal line through the orb cluster */
      const y    = 280 + (swipeDir > 0 ? 0 : 60);
      const xA   = 60;
      const xB   = 330;
      await page.mouse.move(swipeDir > 0 ? xA : xB, y);
      await page.mouse.down();
      for (let x = xA; x <= xB; x += 30) {
        await page.mouse.move(swipeDir > 0 ? x : xB - (x - xA), y);
        await sleep(8);
      }
      await page.mouse.up();
      swipeDir = -swipeDir;
      await sleep(60);
    }
  })();

  /* ── Poll voice stats while storm runs ──────────────────────────────────── */
  while (Date.now() - stormStart < STORM_DURATION_MS) {
    const counts = await page.evaluate(() => {
      const v = parseInt(document.getElementById('statVoices')?.textContent || '0', 10) || 0;
      const m = parseInt(document.getElementById('statModes')?.textContent  || '0', 10) || 0;
      if (v > window.__storm.peakVoices) window.__storm.peakVoices = v;
      if (m > window.__storm.peakModals) window.__storm.peakModals = m;
      return { v, m, ac: window.__monadAudio() };
    });
    process.stdout.write(
      `\r  t=${((Date.now() - stormStart) / 1000).toFixed(1)}s` +
      `  voices=${counts.v}  modals=${counts.m}  ac=${counts.ac}   `
    );
    await sleep(POLL_INTERVAL_MS);
  }

  await stormLoop;   // let swipe loop finish its current iteration
  console.log();

  /* ── Collect final results ───────────────────────────────────────────────── */
  const report = await page.evaluate(() => ({
    peakVoices:   window.__storm.peakVoices,
    peakModals:   window.__storm.peakModals,
    overBudget:   window.__storm.overBudget,
    acStateLog:   window.__storm.acStateLog,
    finalAcState: window.__monadAudio ? window.__monadAudio() : 'unknown',
  }));

  clearInterval(await page.evaluate(() => window.__storm._acPoll));
  await browser.close();

  /* ── Print report ────────────────────────────────────────────────────────── */
  console.log('\n=== RESULTS ===');
  console.log(`  Peak concurrent source voices:  ${report.peakVoices}  (pool=${VOICE_POOL_SIZE})`);
  console.log(`  Peak concurrent modal partials: ${report.peakModals}  (pre-patch ceiling=54 / post-LOW_POWER=24)`);
  console.log(`  Frames > 25 ms (over budget):   ${report.overBudget}`);
  console.log(`  AudioContext final state:        ${report.finalAcState}`);

  if (report.acStateLog.length) {
    console.log('  AudioContext state changes:');
    for (const e of report.acStateLog) {
      console.log(`    t=${(e.t / 1000).toFixed(2)}s  ${e.from} → ${e.to}`);
    }
  } else {
    console.log('  AudioContext state changes:      none ✓');
  }

  if (consoleLines.length) {
    console.log('\n  Page errors / console errors:');
    for (const l of consoleLines.slice(0, 10)) console.log('    ' + l);
  }

  /* ── Pass/fail ───────────────────────────────────────────────────────────── */
  let pass = true;
  const fails = [];

  if (!acRunning || report.finalAcState !== 'running') {
    fails.push(`AudioContext not running at end (state=${report.finalAcState})`);
    pass = false;
  }
  if (report.acStateLog.length > 0) {
    fails.push(`AudioContext state changed ${report.acStateLog.length} time(s) during storm`);
    pass = false;
  }
  if (report.peakVoices > VOICE_POOL_SIZE) {
    fails.push(`Peak voices ${report.peakVoices} exceeds pool size ${VOICE_POOL_SIZE}`);
    pass = false;
  }
  if (report.overBudget > 30) {
    /* SwiftShader software rendering always has higher frame times; only flag
     * a sustained pattern.  A real phone would have a lower threshold (~5). */
    fails.push(`${report.overBudget} frames exceeded 25 ms budget (sustained glitch risk)`);
    pass = false;
  }

  if (fails.length) {
    console.log('\nFAIL:');
    for (const f of fails) console.log('  ' + f);
  } else {
    console.log('\nPASS');
  }

  process.exit(pass ? 0 : 1);
}

run().catch(err => {
  console.error('HARNESS ERROR:', err);
  process.exit(2);
});
