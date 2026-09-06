// PERF-SUITE (dev tool, never shipped).
// Drives monad.html in real headless Chromium and measures:
//   * FPS / frame p50 / p95 across increasing orb counts
//   * adaptive-quality level the engine settles on at each load
//   * audio: dense strike storms -> peak simultaneous voices, voice/modal
//     drops, and frame p95 while audio is busiest (underrun/glitch proxy).
// Run:  node perf-suite.js [--phones] [--audio-load <n>] [--orbs <max>]
const { chromium } = require('/srv/rig/monad/node_modules/playwright-core');
const FILE = 'file://' + '/srv/rig/monad/monad.html';
const CHROME = '/srv/rig/.cache/ms-playwright/chromium-1187/chrome-linux/chrome';
const LIBS = [
  '/srv/rig/lit/tools/render-bench/vendor-libs/extracted/usr/lib/x86_64-linux-gnu',
  '/srv/rig/lit/tools/render-bench/vendor-libs/lib'
].join(':');

const ARGS = {
  phones: process.argv.includes('--phones'),
  audioLoad: +((process.argv.find(a=>a.startsWith('--audio-load='))||'').split('=')[1] || 360),
  maxOrbs: +((process.argv.find(a=>a.startsWith('--orbs='))||'').split('=')[1] || 52),
};

async function measureFrames(page, ms) {
  return page.evaluate((ms) => new Promise(res => {
    const arr=[]; let last=performance.now(); const t0=last;
    (function tick(){
      const now=performance.now();
      if (now-last>0 && now-last<200) arr.push(now-last);
      last=now;
      if (now-t0<ms) requestAnimationFrame(tick);
      else {
        const sec=(now-t0)/1000;
        const srt=[...arr].sort((a,b)=>a-b);
        const pk=a=>a.length?a[a.length>>1]:0;
        res({ frames:arr.length, sec,
          fps:arr.length/sec,
          p50:pk(srt),
          p95:srt.length? srt[Math.floor(srt.length*.95)]:0 });
      }
    })();
  }), ms);
}

async function liveOrbs(page){ return page.evaluate(()=>window.__monadTest?window.__monadTest.liveOrbs:0); }

async function spawnOrbs(page, target) {
  await page.evaluate((target) => new Promise(res => {
    const body=document.getElementById('bodies');
    let i=0;
    (function step(){
      if (window.__monadTest && window.__monadTest.liveOrbs>=target) return res();
      const x=80+(i*37)%(innerWidth-160), y=90+(i*53)%(innerHeight-180);
      const base={bubbles:true,cancelable:true,pointerId:(i%3)+4,isPrimary:true,
        pointerType:'mouse',clientX:x,clientY:y};
      body.dispatchEvent(new PointerEvent('pointerdown',base));
      body.dispatchEvent(new PointerEvent('pointermove',{...base,clientX:x+34,clientY:y+34}));
      body.dispatchEvent(new PointerEvent('pointerup',{...base,clientX:x+34,clientY:y+34}));
      i++;
      setTimeout(step,28);
    })();
  }), target);
  await page.waitForTimeout(900);
}

(async () => {
  process.env.LD_LIBRARY_PATH = LIBS + (process.env.LD_LIBRARY_PATH?':'+process.env.LD_LIBRARY_PATH:'');
  const isPhone=ARGS.phones;
  const b=await chromium.launch({ executablePath:CHROME,
    args:['--use-gl=angle','--use-angle=swiftshader','--enable-unsafe-swiftshader','--mute-audio',
          '--autoplay-policy=no-user-gesture-required'] });
  const page=await b.newPage({ viewport:isPhone?{width:390,height:844}:{width:1440,height:900},
    deviceScaleFactor:isPhone?3:1 });
  const errors=[]; page.on('pageerror',e=>errors.push(e.message));

  await page.goto(FILE); await page.waitForTimeout(1500);
  const rep={ device:isPhone?'phone':'desktop', scenarios:{}, audio:null, errors };
  rep.env=await page.evaluate(()=>({
    fluid:window.MonadFluid&&window.MonadFluid.available?window.MonadFluid.info():'2d-only',
    audio:window.__monadAudio?window.__monadAudio():'n/a',
    orbs:window.__monadTest?window.__monadTest.liveOrbs:0 }));

  const stops=isPhone?[8,20,34]:[8,24,40,ARGS.maxOrbs];
  for (const t of [...new Set(stops)].sort((a,b)=>a-b)) {
    if ((await liveOrbs(page))<t) await spawnOrbs(page,t);
    await page.waitForTimeout(900);
    const m=await measureFrames(page,3500);
    const q=await page.evaluate(()=>window.__monadTest?window.__monadTest.qLevel:null);
    rep.scenarios['orbs'+t]={ fps:+m.fps.toFixed(1), p50:+m.p50.toFixed(1), p95:+m.p95.toFixed(1), q };
    console.log(`  orbs=${m.frames<10?'(low)':''} ${await liveOrbs(page)}  fps ${m.fps.toFixed(1)}  p50 ${m.p50.toFixed(1)}ms  p95 ${m.p95.toFixed(1)}ms  q ${q}`);
  }
  const heavyKey=Object.keys(rep.scenarios).pop();
  rep.frame=rep.scenarios[heavyKey];

  // ---------- AUDIO stress ----------
  await page.evaluate(()=>{ const d=document.getElementById('dev'); if(d)d.hidden=false;
    try{window.__monadTest&&(SOURCE.maxVoices=64)}catch(e){} });
  await page.waitForTimeout(500);
  const audioBefore=await page.evaluate(()=>window.__monadAudioStats?window.__monadAudioStats():null);

  const strikes=ARGS.audioLoad;
  await page.evaluate((strikes)=>new Promise(res=>{
    try{window.__monadTest&&(SOURCE.maxVoices=64)}catch(e){}
    let k=0;
    (function round(){
      const n=window.__monadTest?window.__monadTest.liveOrbs:0;
      for(let i=0;i<n&&k<strikes;i++,k++){
        setTimeout(()=>{ if(window.__monadTest)window.__monadTest.strike(70); },(k%14)*1.4);
      }
      if(k<strikes)setTimeout(round,280); else setTimeout(res,1300);
    })();
  }), strikes);
  await page.waitForTimeout(2400);

  const audioAfter=await page.evaluate(()=>window.__monadAudioStats?window.__monadAudioStats():null);
  const audioFrame=await measureFrames(page,2500);
  rep.audio={ before:audioBefore, after:audioAfter, strikesRequested:strikes,
    whileRingingFps:+audioFrame.fps.toFixed(1), whileRingingP95:+audioFrame.p95.toFixed(1) };
  console.log('  audio before ', JSON.stringify(audioBefore));
  console.log('  audio after  ', JSON.stringify(audioAfter));
  console.log(`  audio frame   fps ${audioFrame.fps.toFixed(1)}  p95 ${audioFrame.p95.toFixed(1)}ms`);

  // ---------- polyphony-ceiling stress (the real headroom figure) ----------
  await page.evaluate(()=>{ try{window.__monadTest&&(SOURCE.maxVoices=110)}catch(e){} });
  await page.waitForTimeout(400);
  const peakVoices=await page.evaluate((strikes)=>new Promise(res=>{
    let peak=0,k=0;
    const iv=setInterval(()=>{ const st=window.__monadAudioStats(); if(st)peak=Math.max(peak,st.voices); },40);
    (function round(){
      const n=window.__monadTest?window.__monadTest.liveOrbs:0;
      for(let i=0;i<n&&k<strikes;i++,k++){
        setTimeout(()=>{ if(window.__monadTest)window.__monadTest.strike(72); },(k%3)*0.9);
      }
      if(k<strikes)setTimeout(round,80); else setTimeout(()=>{clearInterval(iv);res(peak);},2600);
    })();
  }),500);
  const polyAfter=await page.evaluate(()=>window.__monadAudioStats?window.__monadAudioStats():null);
  rep.audio.polyphonyCeiling={ peakSimultaneousVoices:peakVoices, after:polyAfter };
  console.log('  polyphony ceiling  peak simultaneous voices', peakVoices, ' (drops', polyAfter.voiceDrops+polyAfter.modalDrops, ')');

  await page.screenshot({ path:'shot-perf.png' });
  await b.close();
  console.log('\nPERF REPORT\n'+JSON.stringify(rep,null,2));
})().catch(e=>{ console.error('PERF FAIL',e); process.exit(2); });