const {chromium}=require('playwright-core');
const assert=require('node:assert/strict');
const path=require('node:path');
(async()=>{
  const browser=await chromium.launch({executablePath:(process.env.MONAD_CHROME||'/srv/rig/.cache/ms-playwright/chromium-1228/chrome-linux64/chrome'),args:['--disable-gpu']});
  try{
    const page=await browser.newPage({viewport:{width:390,height:844},hasTouch:true,isMobile:true});
    const errors=[];page.on('pageerror',e=>errors.push(e.message));
    await page.addInitScript(()=>{
      window.audioAudit={oscillators:[],filters:[]};
      const original=AudioContext.prototype.createOscillator;
      AudioContext.prototype.createOscillator=function(){
        const oscillator=original.call(this),record={started:false,start:0,stop:null,liveTypeChanges:0};
        window.audioAudit.oscillators.push(record);
        const type=Object.getOwnPropertyDescriptor(OscillatorNode.prototype,'type');
        Object.defineProperty(oscillator,'type',{get(){return type.get.call(this)},set(value){if(record.started)record.liveTypeChanges++;type.set.call(this,value)}});
        const start=oscillator.start.bind(oscillator),stop=oscillator.stop.bind(oscillator);
        oscillator.start=(at=0)=>{record.started=true;record.start=at;start(at)};
        oscillator.stop=at=>{record.stop=at;stop(at)};
        return oscillator;
      };
      const filter=AudioContext.prototype.createBiquadFilter;
      AudioContext.prototype.createBiquadFilter=function(){const node=filter.call(this);window.audioAudit.filters.push(node);return node;};
    });
    await page.goto((process.argv[2]||'file://'+path.join(__dirname,'monad.html'))+'#engine=legacy');
    await page.touchscreen.tap(190,400);await page.waitForTimeout(300);
    assert.equal(await page.evaluate(()=>window.__monadAudio()),'running');
    const switching=await page.evaluate(()=>{
      const test=window.__monadTest;
      for(let i=0;i<100;i++)test.strike(100);
      const wave=document.getElementById('wave');wave.value='sawtooth';wave.dispatchEvent(new Event('change'));
      for(let i=0;i<60;i++)test.strike(100);
      const records=audioAudit.oscillators,changed=records.filter(r=>r.stop!==null);
      return {liveTypeChanges:records.reduce((n,r)=>n+r.liveTypeChanges,0),replaced:changed.length,
        replacementsScheduled:changed.every(old=>records.some(next=>next.start===old.stop))};
    });
    assert.equal(switching.liveTypeChanges,0,'Waveform switches must not mutate an audible oscillator');
    assert.ok(switching.replaced>0);assert.ok(switching.replacementsScheduled);
    await page.evaluate(()=>{
      const {R,applyReverb}=window.__monadTest;
      Object.assign(R,{feedback:100,decay:120,lowRatio:4,highRatio:2,lowCross:1000,highCross:1000,size:2.3,damping:20000});applyReverb();
    });
    await page.waitForTimeout(1600);
    const shelves=await page.evaluate(()=>{
      const R=window.__monadTest.R,low=audioAudit.filters.filter(f=>f.type==='lowshelf'),high=audioAudit.filters.filter(f=>f.type==='highshelf');
      return low.map((filter,i)=>{const loop=Math.min(.9993,10**(-3*(R.delays[i]*R.size/1000)/R.decay));
        return {boost:Math.max(0,filter.gain.value)+Math.max(0,high[i].gain.value),budget:20*Math.log10(.9993/loop)};});
    });
    assert.equal(shelves.length,8);for(const shelf of shelves)assert.ok(shelf.boost<=shelf.budget+.00001,'Combined shelf boosts must stay within the loop budget');
    await page.screenshot({path:'shot-original-refined.png'});
    assert.deepEqual(errors,[]);console.log(JSON.stringify({switching,shelves,errors},null,2));
  }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1});
