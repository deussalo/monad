/* ---------- wave synth editor: knobs in tabs, the mod sources, the matrix and the patch library ---------- */
// Serum's way to modulate: pick a source (tap its chip to arm it, or drag the chip onto a knob), then drag a knob to
// set how far that source moves it; the knob shows each route as a blue arc beside its value. Double tap resets a
// knob, or, armed, removes that route. The matrix tab lists every route to edit, bypass or delete.
const waveSay=read=>{if(typeof showInteraction==='function')showInteraction(read)};
const waveUi={tab:'A',arm:null,pin:false,msg:''};
const WAVE_TABS=['A','B','C','SUB','ENV','LFO','FX','MACRO','MATRIX','PATCH'];
const srcLabel=s=>s.replace(/^env/,'ENV').replace(/^lfo/,'LFO').replace(/^m(\d)$/,'M$1').toUpperCase();
function waveLabel(v,path){
  let m=path.match(/^osc(\d)\.(\w+)$/);const P=WAVE_PARAMS[WAVE_INDEX.get(path)];
  if(m)return 'ABC'[+m[1]-1]+' '+P[1];
  m=path.match(/^(env|lfo)(\d)\./);if(m)return m[1].toUpperCase()+m[2]+' '+P[1];
  m=path.match(WAVE_FX_RE);if(m)return m[1]+' '+P[1];
  m=path.match(/^m(\d)$/);if(m)return v.macroNames[+m[1]-1];
  if(path.startsWith('tune.'))return 'synth '+P[1];
  return P[1];
}
function waveText(P,x){
  const c=P[5];
  if(/^(eq\.|comp\.(thresh|gain))/.test(P[0]))return (x>0?'+':'')+x.toFixed(1)+' dB';
  if(P[0]==='comp.ratio')return x.toFixed(1)+':1';
  if(c==='time')return x<1?Math.round(x*1000)+' ms':x.toFixed(2)+' s';
  if(c==='log')return x>=1000?(x/1000).toFixed(x>=10000?1:2)+'k':x<10?x.toFixed(2):String(Math.round(x));
  if(c==='int')return (x>0&&P[2]<0?'+':'')+x;
  if(P[0].endsWith('.fine'))return (x>0?'+':'')+Math.round(x)+' ct';
  if(P[2]<0)return (x>0?'+':'')+Math.round(x*100);
  return Math.round(x*100)+'%';
}
const routesTo=(v,path)=>v.mods.filter(m=>m.dst===path);
function waveKnob(v,path,edited){
  const P=WAVE_PARAMS[WAVE_INDEX.get(path)],S_=52,cv=el('canvas'),val=el('b'),name=el('span',{textContent:P[1]});
  const box=el('div',{className:'knob'},cv,name,val);box.dataset.path=path;
  cv.width=cv.height=S_*dpr;cv.style.width=cv.style.height=S_+'px';
  const A0=Math.PI*.75,SW=Math.PI*1.5;
  const draw=()=>{
    const x=cv.getContext('2d'),c=S_/2,n=waveNorm(P,waveGet(v,path)),cs=getComputedStyle(document.documentElement);
    const col=k=>cs.getPropertyValue(k).trim()||'#888';
    x.setTransform(dpr,0,0,dpr,0,0);x.clearRect(0,0,S_,S_);x.lineCap='round';
    x.lineWidth=4;x.strokeStyle=col('--line');x.beginPath();x.arc(c,c,c-6,A0,A0+SW);x.stroke();
    const z=P[2]<0&&P[3]>0?waveNorm(P,0):0;
    x.strokeStyle=col('--hot');x.beginPath();x.arc(c,c,c-6,A0+SW*Math.min(z,n),A0+SW*Math.max(z,n)+.001);x.stroke();
    let r=c-12;
    for(const m of routesTo(v,path)){
      const lo=m.bi?n-Math.abs(m.amt):Math.min(n,n+m.amt),hi=m.bi?n+Math.abs(m.amt):Math.max(n,n+m.amt);
      x.globalAlpha=m.off?.3:1;x.lineWidth=3;x.strokeStyle=waveUi.arm===m.src?col('--hot'):col('--sel');
      x.beginPath();x.arc(c,c,r,A0+SW*Math.max(0,lo),A0+SW*Math.min(1,hi)+.001);x.stroke();r-=4;if(r<6)break;
    }
    x.globalAlpha=1;
    const a=A0+SW*n;x.strokeStyle=col('--fg');x.lineWidth=2;x.beginPath();x.moveTo(c+Math.cos(a)*(c-16),c+Math.sin(a)*(c-16));x.lineTo(c+Math.cos(a)*(c-4),c+Math.sin(a)*(c-4));x.stroke();
    const rt=waveUi.arm&&routesTo(v,path).find(m=>m.src===waveUi.arm);
    val.textContent=rt?`${srcLabel(rt.src)} ${rt.amt>0?'+':''}${Math.round(rt.amt*100)}`:waveText(P,waveGet(v,path));
    box.classList.toggle('armed',!!waveUi.arm||waveUi.pin);box.classList.toggle('pinned',!!v.dial?.includes(path));box.classList.toggle('routed',routesTo(v,path).length>0);
  };
  let drag=null,lastTap=0;
  cv.style.touchAction='none';
  cv.onpointerdown=e=>{
    cv.setPointerCapture(e.pointerId);e.preventDefault();
    // pinning: a tap puts the knob on the part's dial, or takes it off
    if(waveUi.pin){const d=v.dial||(v.dial=[]),i=d.indexOf(path);if(i>=0)d.splice(i,1);else if(d.length<WAVE_DIAL)d.push(path);
      else{waveSay(()=>`the dial is full: ${WAVE_DIAL} knobs`);return}
      waveSay(()=>`${waveLabel(v,path)}: ${i>=0?'off':'on'} the dial`);edited();return}
    let route=null;
    if(waveUi.arm){route=routesTo(v,path).find(m=>m.src===waveUi.arm);if(!route){route={src:waveUi.arm,dst:path,amt:0,bi:waveUi.arm.startsWith('lfo'),off:false};v.mods.push(route);drag={fresh:true}}}
    drag={...drag,y:e.clientY,n0:waveNorm(P,waveGet(v,path)),route,a0:route?route.amt:0,moved:false};
  };
  cv.onpointermove=e=>{
    if(!drag)return;const dy=drag.y-e.clientY;if(!drag.moved&&Math.abs(dy)<3)return;drag.moved=true;
    const d=dy/(e.shiftKey?600:150);
    if(drag.route)drag.route.amt=clamp(+(drag.a0+d).toFixed(3),-1,1);
    else waveSet(v,path,waveDenorm(P,drag.n0+d));
    draw();wavePost(v);waveSay(()=>`${waveLabel(v,path)}: ${val.textContent}`);
  };
  cv.onpointerup=cv.onpointercancel=()=>{
    const d=drag;drag=null;if(!d)return;
    if(!d.moved){const t=performance.now();
      if(t-lastTap<320){if(d.route)v.mods.splice(v.mods.indexOf(d.route),1);else waveSet(v,path,P[4]);lastTap=0;edited();return}
      lastTap=t;if(d.fresh&&d.route&&!d.route.amt){v.mods.splice(v.mods.indexOf(d.route),1)}draw();return}
    if(d.route&&!d.route.amt)v.mods.splice(v.mods.indexOf(d.route),1);
    edited();
  };
  // a chip dropped here routes its source at half depth
  box.addEventListener('wave-drop',e=>{const src=e.detail;if(!routesTo(v,path).some(m=>m.src===src))v.mods.push({src,dst:path,amt:.5,bi:src.startsWith('lfo'),off:false});edited()});
  return {box,draw};
}
const choice=(label,options,get,set,fmt=o=>o)=>{
  const s=el('select',{ariaLabel:label},...options.map(o=>el('option',{value:o,textContent:fmt(o)})));
  s.value=get();s.onchange=()=>set(s.value);return el('label',{className:'wave-choice'},el('span',{textContent:label}),s);
};
const toggle=(label,get,set)=>{const b=el('button',{className:'wave-toggle',textContent:label});const sync=()=>{b.ariaPressed=String(!!get())};b.onclick=()=>{set(!get());sync()};sync();return b};
// The table's current frame, drawn from its spectrum, so the knob's wt pos shows as a shape.
function wavePreview(v,i){
  const cv=el('canvas',{className:'wave-shape'}),W=160,H=48;cv.width=W*dpr;cv.height=H*dpr;cv.style.width=W+'px';cv.style.height=H+'px';
  const draw=()=>{
    const o=v.osc[i],sp=waveSpectra(o.table,o.harm),fp=o.pos*(sp.length-1),f0=Math.floor(fp),f1=Math.min(f0+1,sp.length-1),ff=fp-f0,x=cv.getContext('2d');
    const ys=[];let peak=1e-9;
    for(let s=0;s<=96;s++){let y=0;const t=s/96;for(let k=1;k<=64;k++){const a=sp[f0][k-1]*(1-ff)+sp[f1][k-1]*ff;if(a)y+=a*Math.sin(TAU*k*t)}ys.push(y);peak=Math.max(peak,Math.abs(y))}
    const cs=getComputedStyle(document.documentElement);
    x.setTransform(dpr,0,0,dpr,0,0);x.clearRect(0,0,W,H);x.strokeStyle=cs.getPropertyValue('--line');x.beginPath();x.moveTo(0,H/2);x.lineTo(W,H/2);x.stroke();
    x.strokeStyle=o.on?cs.getPropertyValue('--hot'):cs.getPropertyValue('--dim');x.lineWidth=1.5;x.beginPath();
    ys.forEach((y,s)=>{const px=s/96*W,py=H/2-y/peak*(H/2-4);s?x.lineTo(px,py):x.moveTo(px,py)});x.stroke();
  };
  draw();return {box:cv,draw};
}
// The additive table's own harmonics: drag across the bars to draw levels.
function harmEditor(v,i,edited){
  const cv=el('canvas',{className:'wave-harm'}),W=Math.min(320,innerWidth-48),H=72;cv.width=W*dpr;cv.height=H*dpr;cv.style.width=W+'px';cv.style.height=H+'px';cv.style.touchAction='none';
  const o=v.osc[i],bw=W/WAVE_HARMS;
  const draw=()=>{const x=cv.getContext('2d'),cs=getComputedStyle(document.documentElement);x.setTransform(dpr,0,0,dpr,0,0);x.clearRect(0,0,W,H);
    for(let k=0;k<WAVE_HARMS;k++){const h=o.harm[k]*(H-4);x.fillStyle=cs.getPropertyValue('--faint');x.fillRect(k*bw+1,0,bw-2,H);x.fillStyle=cs.getPropertyValue('--hot');x.fillRect(k*bw+1,H-h,bw-2,h)}};
  let on=false;
  const set=e=>{const r=cv.getBoundingClientRect(),k=Math.floor((e.clientX-r.left)/bw);if(k<0||k>=WAVE_HARMS)return;o.harm[k]=clamp(+(1-(e.clientY-r.top)/H).toFixed(2),0,1);draw();wavePost(v);
    waveSay(()=>`harmonic ${k+1}: ${Math.round(o.harm[k]*100)}%`)};
  cv.onpointerdown=e=>{cv.setPointerCapture(e.pointerId);on=true;set(e)};cv.onpointermove=e=>{if(on)set(e)};cv.onpointerup=cv.onpointercancel=()=>{if(on){on=false;edited()}};
  draw();return {box:cv,draw};
}
// A route's curve: drag along it, the bend shows as the line it draws from source in (left) to depth out (up).
function curveEditor(m,edited){
  const cv=el('canvas',{className:'wave-curve',title:'curve: drag up or down'}),W=48,H=32;cv.width=W*dpr;cv.height=H*dpr;cv.style.width=W+'px';cv.style.height=H+'px';cv.style.touchAction='none';
  const draw=()=>{const x=cv.getContext('2d'),cs=getComputedStyle(document.documentElement);x.setTransform(dpr,0,0,dpr,0,0);x.clearRect(0,0,W,H);
    x.strokeStyle=cs.getPropertyValue('--sel');x.lineWidth=1.5;x.beginPath();for(let i=0;i<=24;i++){const u=i/24,y=modCurve(u,m.curve||0);i?x.lineTo(u*W,H-2-y*(H-4)):x.moveTo(0,H-2-y*(H-4))}x.stroke()};
  let d=null;
  cv.onpointerdown=e=>{cv.setPointerCapture(e.pointerId);d={y:e.clientY,c:m.curve||0}};
  cv.onpointermove=e=>{if(!d)return;m.curve=clamp(+(d.c+(d.y-e.clientY)/80).toFixed(2),-1,1);draw()};
  cv.onpointerup=cv.onpointercancel=()=>{if(d){d=null;edited()}};
  cv.ondblclick=()=>{m.curve=0;draw();edited()};
  draw();return el('label',{className:'wave-choice'},el('span',{textContent:'curve'}),cv);
}
// A two-octave keyboard to play the patch while shaping it: hold a key to sustain, slide to play the next,
// several fingers play a chord; ‹ › shift the octave. The voice's octave and semitones transpose what it plays.
const waveKeysAt={oct:4};
function waveKeys(owner){
  const cv=el('canvas',{className:'wave-keys'}),H=56,box=el('div',{className:'wave-keybar'});
  const held=new Map(),WHITE=[0,2,4,5,7,9,11],BLACK={1:0,3:1,6:3,8:4,10:5};
  const lab=el('span',{className:'wave-oct'});
  const geom=()=>{const W=cv.clientWidth||300;return {W,ww:W/14}};
  const draw=()=>{const {W,ww}=geom();if(cv.width!==Math.round(W*dpr)){cv.width=Math.round(W*dpr);cv.height=H*dpr}
    const x=cv.getContext('2d'),cs=getComputedStyle(document.documentElement),on=new Set([...held.values()].map(h=>h.midi)),base=12*(waveKeysAt.oct+1);
    x.setTransform(dpr,0,0,dpr,0,0);x.clearRect(0,0,W,H);
    for(let i=0;i<14;i++){const m=base+12*Math.floor(i/7)+WHITE[i%7];x.fillStyle=on.has(m)?cs.getPropertyValue('--hot'):cs.getPropertyValue('--fg');x.fillRect(i*ww+1,0,ww-2,H)}
    for(let o=0;o<2;o++)for(const[k,after]of Object.entries(BLACK)){const m=base+12*o+ +k,xx=(o*7+after+1)*ww-ww*.3;x.fillStyle=on.has(m)?cs.getPropertyValue('--hot'):cs.getPropertyValue('--bg');x.fillRect(xx,0,ww*.6,H*.6)}
    const v=owner.voice,sh=12*(v.octave||0)+(v.semi||0)+12*(v.tune?.oct||0)+(v.tune?.semi||0);lab.textContent='C'+waveKeysAt.oct+(sh?' '+(sh>0?'+':'')+sh:'');lab.title=sh?`the synth transposes by ${sh} semitones`:''};
  const noteAt=e=>{const r=cv.getBoundingClientRect(),{ww}=geom(),px=e.clientX-r.left,py=e.clientY-r.top,base=12*(waveKeysAt.oct+1);
    if(px<0||px>=ww*14)return null;
    if(py<H*.6)for(let o=0;o<2;o++)for(const[k,after]of Object.entries(BLACK)){const xx=(o*7+after+1)*ww-ww*.3;if(px>=xx&&px<xx+ww*.6)return base+12*o+ +k}
    const i=Math.floor(px/ww);return base+12*Math.floor(i/7)+WHITE[i%7]};
  const on=(id,m)=>{if(!ac)initAudio();if(!ac||engineState!=='ready')return;const v=owner.voice,key=owner.id*128+m;
    // a key is a MIDI note into the voice: its octave and semitones transpose it, as they do every note the voice plays
    playVoice(v,m,ac.currentTime+.005,.85*(v.level??.7),0,key,.8,8);held.set(id,{midi:m,key});buzz(4);draw()};
  const off=id=>{const h=held.get(id);if(!h)return;held.delete(id);if(ac)damp(h.key,ac.currentTime);draw()};
  cv.style.touchAction='none';
  cv.onpointerdown=e=>{cv.setPointerCapture(e.pointerId);const m=noteAt(e);if(m!=null)on(e.pointerId,m)};
  cv.onpointermove=e=>{const h=held.get(e.pointerId);if(!h)return;const m=noteAt(e);if(m!=null&&m!==h.midi){off(e.pointerId);on(e.pointerId,m)}};
  cv.onpointerup=cv.onpointercancel=e=>off(e.pointerId);
  const shift=d=>{waveKeysAt.oct=clamp(waveKeysAt.oct+d,1,7);draw()};
  box.append(el('button',{textContent:'‹',ariaLabel:'octave down',onclick:()=>shift(-1)}),cv,el('button',{textContent:'›',ariaLabel:'octave up',onclick:()=>shift(1)}),lab);
  requestAnimationFrame(draw);new ResizeObserver(draw).observe(cv);
  return box;
}
function waveEditor(f,owner){
  const v=owner.voice,root=el('div',{className:'wave'}),body=el('div',{className:'wave-body'});
  let parts=[];
  const redraw=()=>{for(const p of parts)p.draw?.()};
  const edited=()=>{wavePost(v);changed();redraw()};
  const rebuild=()=>{show(waveUi.tab)};
  // sources: tap to arm, drag onto a knob to route
  const chips=el('div',{className:'wave-src'});
  const hint=el('div',{className:'wave-hint'});
  const syncChips=()=>{for(const c of chips.children)c.ariaPressed=String(c.dataset.src?c.dataset.src===waveUi.arm:waveUi.pin);
    hint.toggleAttribute('data-armed',!!waveUi.arm||waveUi.pin);hint.textContent=waveUi.pin?`Tap knobs to put them on the part's dial, or take them off: ${(v.dial||[]).length} of ${WAVE_DIAL}. Tap DIAL to finish.`:waveUi.arm?`${srcLabel(waveUi.arm)} armed: drag a knob to set its depth, double tap to remove. Tap ${srcLabel(waveUi.arm)} to finish.`:'Tap a source, or drag it onto a knob, to modulate.';redraw()};
  // the dial chip: armed, a tap on a knob pins it to the dial that opens when the part is tapped
  chips.append(el('button',{className:'wave-pin',textContent:'◎ DIAL',onclick:()=>{waveUi.pin=!waveUi.pin;waveUi.arm=null;syncChips()}}));
  for(const s of WAVE_SOURCES){
    const b=el('button',{textContent:srcLabel(s)});b.dataset.src=s;b.style.touchAction='none';
    let start=null;
    b.onpointerdown=e=>{b.setPointerCapture(e.pointerId);start={x:e.clientX,y:e.clientY,moved:false}};
    b.onpointermove=e=>{if(start&&Math.hypot(e.clientX-start.x,e.clientY-start.y)>10){start.moved=true;b.classList.add('dragging')}};
    b.onpointerup=e=>{const st=start;start=null;b.classList.remove('dragging');if(!st)return;
      if(st.moved){const k=document.elementFromPoint(e.clientX,e.clientY)?.closest('.knob');if(k)k.dispatchEvent(new CustomEvent('wave-drop',{detail:s}));return}
      waveUi.arm=waveUi.arm===s?null:s;waveUi.pin=false;syncChips()};
    chips.append(b);
  }
  const tabs=el('div',{className:'wave-tabs'});
  for(const t of WAVE_TABS){const b=el('button',{textContent:t,onclick:()=>{waveUi.tab=t;show(t)}});b.dataset.tab=t;tabs.append(b)}
  const play=el('button',{className:'wave-play',textContent:'▶ play',onclick:()=>{if(!ac)initAudio();audition(owner,true)}});
  const group=(title,...kids)=>el('div',{className:'wave-group'},el('h4',{textContent:title}),el('div',{className:'knobs'},...kids));
  const K=path=>{const k=waveKnob(v,path,edited);parts.push(k);return k.box};
  const show=t=>{
    parts=[];body.replaceChildren();for(const b of tabs.children)b.ariaPressed=String(b.dataset.tab===t);
    const i='ABC'.indexOf(t);
    if(i>=0){
      const o=v.osc[i],O=k=>K(`osc${i+1}.${k}`),pv=wavePreview(v,i);parts.push(pv);
      body.append(el('div',{className:'wave-row'},toggle(`osc ${t}`,()=>o.on,x=>{o.on=x;edited()}),
        choice('table',WAVE_TABLES,()=>o.table,x=>{o.table=x;edited();rebuild()}),choice('filter',WAVE_FILTERS,()=>o.filter,x=>{o.filter=x;edited()}),
        choice('warp',WARP_MODES,()=>o.warpMode,x=>{o.warpMode=x;edited()}),choice('warp 2',WARP_MODES,()=>o.warp2Mode,x=>{o.warp2Mode=x;edited()})),pv.box);
      if(o.table==='harmonics'){const h=harmEditor(v,i,edited);parts.push(h);body.append(h.box)}
      body.append(group('wave',O('pos'),O('warp'),O('warp2'),O('phase'),O('level'),O('pan')),group('pitch',O('oct'),O('semi'),O('fine')),
        group('unison',O('uni'),O('detune'),O('blend'),O('spread')),group('amp envelope',O('a'),O('d'),O('s'),O('r')),
        group('filter',O('cut'),O('res'),O('drive'),O('key')),group('filter envelope',O('famt'),O('fa'),O('fd'),O('fs'),O('fr')));
    }else if(t==='SUB')body.append(group('sub (on ENV1)',K('sub.level'),K('sub.oct')),group('noise (on ENV1)',K('noise.level'),K('noise.color')),group('voice',K('tune.oct'),K('tune.semi'),K('vol'),K('glide'),K('gate')),
      el('div',{className:'wave-row'},choice('voicing',VOICE_MODES,()=>v.voicing.mode,x=>{v.voicing.mode=x;edited()}),
        toggle('glide always',()=>v.voicing.always,x=>{v.voicing.always=x;edited()}),toggle('glide scales',()=>v.voicing.scale,x=>{v.voicing.scale=x;edited()})),
      el('p',{className:'wave-hint',textContent:'Mono plays one note at a time and restarts the envelopes; legato restarts them only after a gap. Glide always slides every note; otherwise only overlapping ones. Glide scales makes short steps faster.'}));
    else if(t==='ENV')for(let e=1;e<=WAVE_ENVS;e++)body.append(group(`ENV${e}`+(e===1?' (also sub and noise)':''),...['a','d','s','r'].map(k=>K(`env${e}.${k}`))));
    else if(t==='LFO')for(let l=1;l<=WAVE_LFOS;l++){const L=v.lfo[l-1];
      body.append(el('div',{className:'wave-group'},el('h4',{textContent:'LFO'+l}),el('div',{className:'wave-row'},
        choice('shape',LFO_SHAPES,()=>L.shape,x=>{L.shape=x;edited()}),toggle('retrig',()=>L.retrig,x=>{L.retrig=x;edited()}),
        choice('sync',Object.keys(LFO_SYNCS),()=>L.sync,x=>{L.sync=x;edited()})),
        el('div',{className:'knobs'},K(`lfo${l}.rate`),K(`lfo${l}.phase`))))}
    else if(t==='FX'){
      // the rack, run top to bottom; ↑ ↓ move a module. Its knobs take macros from the matrix (the rack is shared by every note)
      const fx=v.fx,order=v.fxOrder=waveFxOrder(v.fxOrder);
      const KNOBS={hyper:['voices','rate','detune','dim','mix'],dist:['drive','mix'],flanger:['rate','depth','fb','spread','mix'],phaser:['rate','depth','freq','fb','spread','mix'],
        chorus:['rate','depth','mix'],delay:['time','fb','tone','mix'],comp:['thresh','ratio','gain'],eq:['low','high']};
      const NAMES={hyper:'hyper / dimension',dist:'distortion',flanger:'flanger',phaser:'phaser',chorus:'chorus',delay:'delay',comp:'compressor',eq:'eq'};
      const extra={dist:()=>[choice('mode',DIST_MODES,()=>fx.dist.mode,x=>{fx.dist.mode=x;edited()})],
        delay:()=>[choice('sync',Object.keys(DELAY_SYNCS),()=>fx.delay.sync,x=>{fx.delay.sync=x;edited()}),toggle('ping-pong',()=>fx.delay.ping,x=>{fx.delay.ping=x;edited()})]};
      const move=(k,d)=>{const i=order.indexOf(k),j=i+d;if(j<0||j>=order.length)return;order.splice(i,1);order.splice(j,0,k);edited();show('FX')};
      body.append(el('p',{className:'wave-hint',textContent:'The rack runs top to bottom on the patch\'s summed voices. Macros can drive its knobs; per-note sources cannot.'}));
      order.forEach((k,i)=>{const sec=el('div',{className:'wave-group wave-fx'},el('div',{className:'wave-row'},toggle(NAMES[k],()=>fx[k].on,x=>{fx[k].on=x;edited();sec.classList.toggle('off',!x)}),
          ...(extra[k]?.()||[]),el('span',{className:'wave-grow'}),
          el('button',{textContent:'↑',ariaLabel:'move '+k+' up',disabled:i===0,onclick:()=>move(k,-1)}),el('button',{textContent:'↓',ariaLabel:'move '+k+' down',disabled:i===order.length-1,onclick:()=>move(k,1)})),
        el('div',{className:'knobs'},...KNOBS[k].map(q=>K(k+'.'+q))));
        sec.dataset.fx=k;sec.classList.toggle('off',!fx[k].on);body.append(sec)});
    }
    else if(t==='MACRO'){
      const ks=[];for(let m=1;m<=WAVE_MACROS;m++){const kb=K('m'+m),name=el('input',{value:v.macroNames[m-1],className:'wave-name',ariaLabel:'macro '+m+' name'});
        name.onchange=()=>{v.macroNames[m-1]=name.value.slice(0,16)||'macro '+m;kb.querySelector('span').textContent=v.macroNames[m-1];edited()};
        kb.querySelector('span').textContent=v.macroNames[m-1];ks.push(el('div',{className:'wave-macro'},kb,name))}
      body.append(el('div',{className:'knobs'},...ks));
    }else if(t==='MATRIX'){
      const dsts=WAVE_PARAMS.map(P=>P[0]);
      const list=el('div',{className:'wave-matrix'});
      const fill=()=>{list.replaceChildren();
        if(!v.mods.length)list.append(el('p',{className:'wave-hint',textContent:'No routes yet. Arm a source above and drag a knob, or add one here.'}));
        v.mods.forEach((m,j)=>{
          const amt=el('input',{type:'range',min:-1,max:1,step:.01,value:m.amt,ariaLabel:'amount'}),num=el('b',{textContent:Math.round(m.amt*100)});
          amt.oninput=()=>{m.amt=+amt.value;num.textContent=Math.round(m.amt*100);wavePost(v)};amt.onchange=edited;
          const row=el('div',{className:'wave-route'},
            choice('source',WAVE_SOURCES,()=>m.src,x=>{m.src=x;edited()},srcLabel),choice('to',dsts,()=>m.dst,x=>{m.dst=x;edited()},p=>waveLabel(v,p)),
            el('div',{className:'wave-amt'},amt,num),toggle('±',()=>m.bi,x=>{m.bi=x;edited()}),toggle('bypass',()=>m.off,x=>{m.off=x;edited()}),
            choice('aux',['',...WAVE_SOURCES],()=>m.aux||'',x=>{m.aux=x;edited()},x=>x?srcLabel(x):'none'),toggle('aux inv',()=>m.auxInv,x=>{m.auxInv=x;edited()}),
            curveEditor(m,edited),
            el('button',{textContent:'×',ariaLabel:'delete route',onclick:()=>{v.mods.splice(j,1);edited();fill()}}));
          list.append(row)})};
      fill();
      body.append(list,el('button',{textContent:'+ route',onclick:()=>{v.mods.push({src:'lfo1',dst:'osc1.cut',amt:.25,bi:true,off:false});edited();fill()}}));
    }else if(t==='PATCH'){
      const name=el('input',{value:v.name||'',className:'wave-name',ariaLabel:'patch name'});name.onchange=()=>{v.name=name.value.slice(0,40);edited()};
      const msg=el('p',{className:'wave-hint',textContent:waveUi.msg});const say=s=>{waveUi.msg=msg.textContent=s};
      const load=j=>{loadPatch(v,j);edited();say(`loaded ${v.name}`);show('PATCH')};
      const factory=choice('factory',Object.keys(WAVE_PRESETS),()=>'init',x=>load(WAVE_PRESETS[x]()));
      const lib=patchLib.all(),names=Object.keys(lib).sort(),user=el('div',{className:'wave-lib'});
      for(const n of names)user.append(el('div',{className:'wave-row'},el('button',{textContent:n,onclick:()=>load(lib[n])}),
        el('button',{textContent:'×',ariaLabel:'delete '+n,onclick:()=>{patchLib.remove(n);say(`deleted ${n}`);show('PATCH')}})));
      const ta=el('textarea',{placeholder:'paste a patch JSON here',ariaLabel:'patch json'}),file=el('input',{type:'file',accept:'.json,application/json'});
      file.onchange=async()=>{const fl=file.files[0];if(!fl)return;try{importPatch(v,await fl.text());edited();say(`imported ${v.name}`);show('PATCH')}catch(e){say('import failed: '+e.message)}};
      body.append(el('div',{className:'wave-row'},el('span',{textContent:'name'}),name),
        el('div',{className:'wave-row'},el('button',{textContent:'save to library',onclick:()=>{const ok=patchLib.save(v.name||'patch',patchOf(v));say(ok?`saved ${v.name}`:'saved for this session only: storage is off here; export to keep it');show('PATCH')}}),
          el('button',{textContent:'init',onclick:()=>load(waveInit())})),
        factory,el('h4',{textContent:'library'}),names.length?user:el('p',{className:'wave-hint',textContent:'Nothing saved yet.'}),
        el('h4',{textContent:'JSON'}),el('div',{className:'wave-row'},el('button',{textContent:'export',onclick:()=>{ta.value=exportPatch(v);say('exported: downloaded and copied')}}),
          el('button',{textContent:'import text',onclick:()=>{try{importPatch(v,ta.value);edited();say(`imported ${v.name}`);show('PATCH')}catch(e){say('import failed: '+e.message)}}})),
        el('label',{className:'wave-file'},el('span',{textContent:'import file'}),file),ta,msg);
    }
    redraw();
  };
  // presets in reach without the patch tab: ◀ name ▶ steps through the factory patches, then the library
  const browse=d=>{const lib=patchLib.all(),names=[...Object.keys(WAVE_PRESETS).map(n=>['f',n]),...Object.keys(lib).sort().map(n=>['u',n])];
    let i=names.findIndex(([,n])=>n===v.name);i=((i<0?(d>0?-1:0):i)+d+names.length)%names.length;const[kind,n]=names[i];
    loadPatch(v,kind==='f'?WAVE_PRESETS[n]():lib[n]);edited();pname.textContent=v.name;show(waveUi.tab);if(!ac)initAudio();audition(owner,true)};
  const pname=el('span',{className:'wave-pname',textContent:v.name||'patch'});
  const top=el('div',{className:'wave-top'},el('button',{textContent:'◀',ariaLabel:'previous patch',onclick:()=>browse(-1)}),pname,
    el('button',{textContent:'▶',ariaLabel:'next patch',onclick:()=>browse(1)}),el('span',{className:'wave-grow'}),play);
  root.append(top,chips,hint,tabs,body,waveKeys(owner));
  f.append(root);show(waveUi.tab);syncChips();
  // the sources stay in reach while the knobs scroll: they stick under the phone sheet's head, or at the panel's top
  requestAnimationFrame(()=>{const head=document.querySelector('#sheet-top');chips.style.top=(head&&head.offsetParent?head.offsetHeight:0)+'px'});
  rows.push(redraw);
}
