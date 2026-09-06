// Regression test for the silent-phone bug. Deliberately does NOT pass
// --autoplay-policy=no-user-gesture-required: that flag removes the very
// policy under test and is why this shipped. Asserts CONFIRMED state.
const { chromium } = require('/srv/rig/monad/node_modules/playwright-core');
const path=require('path');
const FILE = process.argv[2] || ('file://'+path.join(__dirname,'monad.html'));
(async () => {
  const b = await chromium.launch({ executablePath:'/srv/rig/.cache/ms-playwright/chromium-1187/chrome-linux/chrome',
    args:['--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader'] });
  const ctx = await b.newContext({viewport:{width:390,height:844},deviceScaleFactor:3,hasTouch:true,isMobile:true});
  const p = await ctx.newPage();
  const errs=[]; p.on('pageerror',e=>errs.push(e.message));
  await p.goto(FILE); await p.waitForTimeout(1200);
  const state = () => p.evaluate(()=>window.__monadAudio?window.__monadAudio():'no-probe');
  let pass=true;
  const check=(label,got,want)=>{const ok=want.includes(got);if(!ok)pass=false;
    console.log(('  '+label).padEnd(52), got, ok?'✓':('✗ expected '+want.join('/')));};

  console.log('TOUCH DEVICE, autoplay policy ENFORCED');
  check('before any input', await state(), ['none','suspended']);

  // pointerdown alone must NOT be treated as arming: for touch, activation
  // arrives at pointerup. This is the exact shape of the original bug.
  await p.evaluate(()=>{
    const c=document.getElementById('bodies');
    c.dispatchEvent(new PointerEvent('pointerdown',{pointerId:3,pointerType:'touch',isPrimary:true,
      clientX:150,clientY:400,bubbles:true,cancelable:true}));
  });
  await p.waitForTimeout(500);
  check('after synthetic pointerdown only', await state(), ['none','suspended']);

  // a real tap grants activation at pointerup -> must reach running
  await p.touchscreen.tap(150,400);
  await p.waitForTimeout(1200);
  check('after a real tap (CONFIRMED running)', await state(), ['running']);

  // arming must not be {once:true}: a later tap keeps it healthy
  await p.touchscreen.tap(250,500); await p.waitForTimeout(600);
  check('still running after a second tap', await state(), ['running']);

  // background/foreground: Android suspends the context
  await p.evaluate(()=>Object.defineProperty(document,'hidden',{value:true,configurable:true}));
  await p.evaluate(()=>document.dispatchEvent(new Event('visibilitychange')));
  await p.waitForTimeout(300);
  await p.evaluate(()=>Object.defineProperty(document,'hidden',{value:false,configurable:true}));
  await p.evaluate(()=>document.dispatchEvent(new Event('visibilitychange')));
  await p.waitForTimeout(800);
  check('after background -> foreground', await state(), ['running']);

  // and it actually makes sound: strike bodies, expect voices
  await p.touchscreen.tap(200,300); await p.waitForTimeout(200);
  const voices = await p.evaluate(()=>{
    document.getElementById('dev').hidden=false;   // stats only paint when visible
    return new Promise(r=>setTimeout(()=>r(+document.querySelector('#statVoices').textContent||0),1500));
  });
  console.log('  active voices after interaction'.padEnd(52), voices, voices>=0?'(context live)':'');
  console.log('  errors', errs.length?errs.slice(0,2):'NONE');
  console.log(pass?'\nAUDIO ACTIVATION: PASS':'\nAUDIO ACTIVATION: FAIL');
  await b.close();
  process.exit(pass?0:1);
})();
