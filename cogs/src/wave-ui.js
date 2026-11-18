/* ---------- wave synth editor: knobs in tabs, the mod sources, the matrix and the patch library ---------- */
// Serum's way to modulate: pick a source (tap its chip to arm it, or drag the chip onto a knob), then drag a knob to
// set how far that source moves it; the knob shows each route as a blue arc beside its value. Double tap resets a
// knob, or, armed, removes that route. Drag out sideways for finer steps (the readout says how fine). The matrix tab lists every route to edit, bypass or delete.
const waveSay=read=>{if(typeof showInteraction==='function')showInteraction(read)};
const waveUi={tab:'MAIN',arm:null,pin:false,msg:''};
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
  if(P[0]==='reverb.decay')return x.toFixed(x<10?2:1)+' s';
  if(c==='log'&&/\.(rate|freq|cut|lowcut|highcut)$/.test(P[0]))return (x>=1000?(x/1000).toFixed(x>=10000?1:2)+'k':x<10?x.toFixed(2):String(Math.round(x)))+' Hz';
  if(c==='log')return x>=1000?(x/1000).toFixed(x>=10000?1:2)+'k':x<10?x.toFixed(2):String(Math.round(x));
  if(c==='int')return (x>0&&P[2]<0?'+':'')+x;
  if(P[0].endsWith('.fine'))return (x>0?'+':'')+Math.round(x)+' ct';
  if(P[2]<0)return (x>0?'+':'')+Math.round(x*100);
  return Math.round(x*100)+'%';
}
const routesTo=(v,path)=>v.mods.filter(m=>m.dst===path);
// The palette, read once a frame at most: a tab of knobs drawing together asks the style system once, not per knob.
let wavePalAt=-1,wavePalMap=null;
const wavePal=k=>{const t=performance.now();if(!wavePalMap||t-wavePalAt>250){wavePalAt=t;wavePalMap=new Map();wavePalMap.cs=getComputedStyle(document.documentElement)}
  let c=wavePalMap.get(k);if(c===undefined){c=wavePalMap.cs.getPropertyValue(k).trim()||'#888';wavePalMap.set(k,c)}return c};
// Macro knobs a cam or probe turns: they show where it has them, live, while the sheet is open.
const waveLive=new Set();
const camDriver=(v,path)=>{const m=path.match(/^m(\d)$/);return m?macroDriver(v,+m[1]):null};
function waveKnob(v,path,edited){
  const P=WAVE_PARAMS[WAVE_INDEX.get(path)],S_=52,cv=el('canvas'),val=el('b'),name=el('span',{textContent:P[1]});
  const box=el('div',{className:'knob'},cv,name,val);box.dataset.path=path;
  cv.width=cv.height=S_*dpr;cv.style.width=cv.style.height=S_+'px';
  const A0=Math.PI*.75,SW=Math.PI*1.5;
  const draw=()=>{
    const cam=camDriver(v,path),x=cv.getContext('2d'),c=S_/2,n=cam?cam.now():waveNorm(P,waveGet(v,path)),col=wavePal;
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
    // where the modulation has it now, on the newest note: a dot riding the ring
    k.dot=!cam&&waveIdOf.get(v)===waveWatching&&waveMod.at.has(path);
    if(k.dot){const la=A0+SW*clamp(waveMod.at.get(path),0,1);x.fillStyle=col('--sel');x.beginPath();x.arc(c+Math.cos(la)*(c-6),c+Math.sin(la)*(c-6),3.5,0,TAU);x.fill()}
    const a=A0+SW*n;x.strokeStyle=col('--fg');x.lineWidth=2;x.beginPath();x.moveTo(c+Math.cos(a)*(c-16),c+Math.sin(a)*(c-16));x.lineTo(c+Math.cos(a)*(c-4),c+Math.sin(a)*(c-4));x.stroke();
    const rt=waveUi.arm&&routesTo(v,path).find(m=>m.src===waveUi.arm);
    val.textContent=cam?`${cam.label} ${Math.round(n*100)}%`:rt?`${srcLabel(rt.src)} ${rt.amt>0?'+':''}${Math.round(rt.amt*100)}`:waveText(P,waveGet(v,path));
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
    drag={...drag,x:e.clientX,y:e.clientY,ly:e.clientY,acc:0,n0:waveNorm(P,waveGet(v,path)),route,a0:route?route.amt:0,moved:false};
  };
  cv.onpointermove=e=>{
    if(!drag)return;if(!drag.moved&&Math.abs(drag.y-e.clientY)<3)return;drag.moved=true;
    // fine control: the further the finger strays sideways from the knob, the finer each vertical step (shift on a desk)
    const fine=1+Math.abs(e.clientX-drag.x)/60;drag.acc+=(drag.ly-e.clientY)/((e.shiftKey?600:150)*fine);drag.ly=e.clientY;const d=drag.acc;
    if(drag.route)drag.route.amt=clamp(+(drag.a0+d).toFixed(3),-1,1);
    else waveSet(v,path,waveDenorm(P,drag.n0+d));
    draw();wavePost(v);waveSay(()=>`${waveLabel(v,path)}: ${val.textContent}${fine>=1.5?` · fine ×${fine.toFixed(1)}`:''}`);
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
  if(/^m\d$/.test(path))waveLive.add(()=>{if(camDriver(v,path)&&box.isConnected)draw()});
  const k={box,draw,path,dot:false};return k;
}
const choice=(label,options,get,set,fmt=o=>o)=>{
  const s=el('select',{ariaLabel:label},...options.map(o=>el('option',{value:o,textContent:fmt(o)})));
  s.value=get();s.onchange=()=>set(s.value);return el('label',{className:'wave-choice'},el('span',{textContent:label}),s);
};
const spaced=b=>el('div',{className:'wave-choice'},el('span',{textContent:'\u00a0'}),b);
const toggle=(label,get,set)=>{const b=el('button',{className:'wave-toggle',textContent:label});const sync=()=>{b.ariaPressed=String(!!get())};b.onclick=()=>{set(!get());sync()};sync();return b};
// The table's current frame, drawn from its spectrum, so the knob's wt pos shows as a shape.
function wavePreview(v,i){
  const cv=el('canvas',{className:'wave-shape'}),W=160,H=48;cv.width=W*dpr;cv.height=H*dpr;cv.style.width=W+'px';cv.style.height=H+'px';
  const draw=()=>{
    const o=v.osc[i],sp=waveSpectra(o.table,o.harm),fp=o.pos*(sp.length-1),f0=Math.floor(fp),f1=Math.min(f0+1,sp.length-1),ff=fp-f0,x=cv.getContext('2d');
    const ys=[];let peak=1e-9;
    for(let s=0;s<=96;s++){let y=0;const t=s/96;for(let k=1;k<=64;k++){const a=sp[f0][k-1]*(1-ff)+sp[f1][k-1]*ff;if(a)y+=a*Math.sin(TAU*k*t)}ys.push(y);peak=Math.max(peak,Math.abs(y))}
    const cs={getPropertyValue:wavePal};
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
  const draw=()=>{const x=cv.getContext('2d'),cs={getPropertyValue:wavePal};x.setTransform(dpr,0,0,dpr,0,0);x.clearRect(0,0,W,H);
    for(let k=0;k<WAVE_HARMS;k++){const h=o.harm[k]*(H-4);x.fillStyle=cs.getPropertyValue('--faint');x.fillRect(k*bw+1,0,bw-2,H);x.fillStyle=cs.getPropertyValue('--hot');x.fillRect(k*bw+1,H-h,bw-2,h)}};
  let on=false;
  const set=e=>{const r=cv.getBoundingClientRect(),k=Math.floor((e.clientX-r.left)/bw);if(k<0||k>=WAVE_HARMS)return;o.harm[k]=clamp(+(1-(e.clientY-r.top)/H).toFixed(2),0,1);draw();wavePost(v);
    waveSay(()=>`harmonic ${k+1}: ${Math.round(o.harm[k]*100)}%`)};
  cv.onpointerdown=e=>{cv.setPointerCapture(e.pointerId);on=true;set(e)};cv.onpointermove=e=>{if(on)set(e)};cv.onpointerup=cv.onpointercancel=()=>{if(on){on=false;edited()}};
  draw();return {box:cv,draw};
}
// A route's curve: drag along it, the bend shows as the line it draws from source in (left) to depth out (up).
function curveEditor(m,edited){
  const cv=el('canvas',{className:'wave-curve'}),W=48,H=32;cv.width=W*dpr;cv.height=H*dpr;cv.style.width=W+'px';cv.style.height=H+'px';cv.style.touchAction='none';
  const draw=()=>{const x=cv.getContext('2d'),cs={getPropertyValue:wavePal};x.setTransform(dpr,0,0,dpr,0,0);x.clearRect(0,0,W,H);
    x.strokeStyle=cs.getPropertyValue('--sel');x.lineWidth=1.5;x.beginPath();for(let i=0;i<=24;i++){const u=i/24,y=modCurve(u,m.curve||0);i?x.lineTo(u*W,H-2-y*(H-4)):x.moveTo(0,H-2-y*(H-4))}x.stroke()};
  let d=null;
  cv.onpointerdown=e=>{cv.setPointerCapture(e.pointerId);d={y:e.clientY,c:m.curve||0}};
  cv.onpointermove=e=>{if(!d)return;m.curve=clamp(+(d.c+(d.y-e.clientY)/80).toFixed(2),-1,1);draw()};
  cv.onpointerup=cv.onpointercancel=()=>{if(d){d=null;edited()}};
  cv.ondblclick=()=>{m.curve=0;draw();edited()};
  draw();return el('label',{className:'wave-choice'},el('span',{textContent:'curve'}),cv);
}
// The editor's tabs. MAIN holds what a player reaches for: the macros and the knobs pinned to the part's dial.
const WAVE_TABS=['MAIN','A','B','C','VOICE','MOD','FX','MATRIX','PATCH'];
const WAVE_QUICK=['osc1.cut','osc1.res','osc1.a','osc1.r','osc1.detune','vol'];
// Bluetooth MIDI from the synth's head: one tap to connect a keyboard; it then plays this part.
const BT_ICON='<svg width="16" height="16" viewBox="0 0 15 15" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M4.5 4.5l6 5.5-3 2.5v-10l3 2.5-6 5.5"/></svg>';
function waveBluetooth(owner){
  const label=el('span'),b=el('button',{className:'wave-bt',ariaLabel:'connect Bluetooth MIDI',onclick:()=>{initAudio();midi.target=owner;connectBluetooth()}});
  b.innerHTML=BT_ICON;b.append(label);
  const sync=()=>{const ble=[...midi.devices].filter(([k])=>k.startsWith('ble:')).map(([,n])=>n);
    label.textContent=ble.length?ble.join(', '):'MIDI';b.ariaPressed=String(ble.length>0);};
  midiRows=midiRows.filter(s=>s.el?.isConnected);sync.el=b;midiRows.push(sync);sync();
  return b;
}
function waveEditor(f,owner,part){
  const v=owner.voice,root=el('div',{className:'wave'}),body=el('div',{className:'wave-body'});
  let parts=[];
  const redraw=()=>{for(const p of parts)p.draw?.();if(root.isConnected)waveWatch(v)};
  const edited=()=>{wavePost(v);changed();redraw()};
  const rebuild=()=>{show(waveUi.tab)};
  // sources: tap to arm, drag onto a knob to route
  const chips=el('div',{className:'wave-src'});
  const syncChips=()=>{for(const c of chips.children){c.ariaPressed=String(c.dataset.src?c.dataset.src===waveUi.arm:waveUi.pin);const d=c.dataset.src&&camDriver(v,c.dataset.src);c.classList.toggle('driven',!!d);
}
    for(const p of parts)p.draw?.()};
  // the dial chip: armed, a tap on a knob pins it to the dial that opens when the part is tapped
  chips.append(el('button',{className:'wave-pin',textContent:'DIAL',onclick:()=>{waveUi.pin=!waveUi.pin;waveUi.arm=null;syncChips()}}));
  for(const s of WAVE_SOURCES){
    const b=el('button',{textContent:srcLabel(s)});b.dataset.src=s;b.dataset.kind=s.replace(/\d+$/,'');b.style.touchAction='none';
    let start=null;
    b.onpointerdown=e=>{b.setPointerCapture(e.pointerId);start={x:e.clientX,y:e.clientY,moved:false}};
    b.onpointermove=e=>{if(start&&Math.hypot(e.clientX-start.x,e.clientY-start.y)>10){start.moved=true;b.classList.add('dragging')}};
    b.onpointerup=e=>{const st=start;start=null;b.classList.remove('dragging');if(!st)return;
      if(st.moved){const k=document.elementFromPoint(e.clientX,e.clientY)?.closest('.knob');if(k)k.dispatchEvent(new CustomEvent('wave-drop',{detail:s}));return}
      waveUi.arm=waveUi.arm===s?null:s;waveUi.pin=false;syncChips()};
    chips.append(b);
  }
  const tabs=el('div',{className:'wave-tabs',role:'tablist'});
  for(const t of WAVE_TABS){const b=el('button',{textContent:t,role:'tab',onclick:()=>{waveUi.tab=t;show(t)}});b.dataset.tab=t;tabs.append(b)}
  const play=el('button',{className:'wave-play wave-icon',innerHTML:rx('Play',16),ariaLabel:'play',onclick:()=>{if(!ac)initAudio();audition(owner,true);waveWatch(v)}});
  const group=(title,...kids)=>el('div',{className:'wave-group'},el('h4',{textContent:title}),el('div',{className:'knobs'},...kids));
  const K=path=>{const k=waveKnob(v,path,edited);parts.push(k);return k.box};
  const show=t=>{
    if(!WAVE_TABS.includes(t))t=waveUi.tab='MAIN';
    parts=[];waveLive.clear();body.replaceChildren();root.dataset.tab=t;for(const b of tabs.children)b.ariaSelected=b.ariaPressed=String(b.dataset.tab===t);
    const i='ABC'.indexOf(t);
    if(t==='MAIN'){
      const ks=[];for(let m=1;m<=WAVE_MACROS;m++){const kb=K('m'+m),name=el('input',{value:v.macroNames[m-1],className:'wave-name',ariaLabel:'macro '+m+' name'});
        name.onchange=()=>{v.macroNames[m-1]=name.value.slice(0,16)||'macro '+m;kb.querySelector('span').textContent=v.macroNames[m-1];edited();syncChips()};
        kb.querySelector('span').textContent=v.macroNames[m-1];ks.push(el('div',{className:'wave-macro'},kb,name))}
      const pinned=(v.dial||[]).filter(p=>WAVE_INDEX.has(p)),quick=pinned.length?pinned:WAVE_QUICK;
      const q=group(pinned.length?'on the dial':'quick',...quick.map(p=>{const b=K(p);b.querySelector('span').textContent=waveLabel(v,p);return b}));
      body.append(el('div',{className:'wave-group'},el('h4',{textContent:'macros'}),el('div',{className:'knobs'},...ks)),q);
    }else if(i>=0){
      const o=v.osc[i],O=k=>K(`osc${i+1}.${k}`),pv=wavePreview(v,i);parts.push(pv);
      body.append(el('div',{className:'wave-row'},spaced(toggle(`osc ${t}`,()=>o.on,x=>{o.on=x;edited();body.classList.toggle('osc-off',!x)})),
        choice('table',WAVE_TABLES,()=>o.table,x=>{o.table=x;edited();rebuild()}),choice('filter',WAVE_FILTERS,()=>o.filter,x=>{o.filter=x;edited()})),pv.box);
      body.classList.toggle('osc-off',!o.on);
      if(o.table==='harmonics'){const h=harmEditor(v,i,edited);parts.push(h);body.append(h.box)}
      body.append(group('wave',O('pos'),O('phase'),O('level'),O('pan')),
        el('div',{className:'wave-group'},el('h4',{textContent:'warp'}),el('div',{className:'wave-row'},
          choice('warp',WARP_MODES,()=>o.warpMode,x=>{o.warpMode=x;edited()}),choice('warp 2',WARP_MODES,()=>o.warp2Mode,x=>{o.warp2Mode=x;edited()})),el('div',{className:'knobs'},O('warp'),O('warp2'))),
        group('pitch',O('oct'),O('semi'),O('fine')),
        group('unison',O('uni'),O('detune'),O('blend'),O('spread')),group('amp envelope',O('a'),O('d'),O('s'),O('r')),
        group('filter',O('cut'),O('res'),O('drive'),O('key')),group('filter envelope',O('famt'),O('fa'),O('fd'),O('fs'),O('fr')));
    }else if(t==='VOICE')body.append(group('voice',K('tune.oct'),K('tune.semi'),K('vol'),K('glide'),K('gate')),
      el('div',{className:'wave-row'},choice('voicing',VOICE_MODES,()=>v.voicing.mode,x=>{v.voicing.mode=x;edited()}),
        spaced(toggle('glide always',()=>v.voicing.always,x=>{v.voicing.always=x;edited()})),spaced(toggle('glide scales',()=>v.voicing.scale,x=>{v.voicing.scale=x;edited()}))),
      group('sub (on ENV1)',K('sub.level'),K('sub.oct')),group('noise (on ENV1)',K('noise.level'),K('noise.color')));
    else if(t==='MOD'){
      for(let e=1;e<=WAVE_ENVS;e++)body.append(group(`ENV${e}`+(e===1?' · also sub and noise':''),...['a','d','s','r'].map(k=>K(`env${e}.${k}`))));
      for(let l=1;l<=WAVE_LFOS;l++){const L=v.lfo[l-1];
        body.append(el('div',{className:'wave-group'},el('h4',{textContent:'LFO'+l}),el('div',{className:'wave-row'},
          choice('shape',LFO_SHAPES,()=>L.shape,x=>{L.shape=x;edited()}),choice('sync',Object.keys(LFO_SYNCS),()=>L.sync,x=>{L.sync=x;edited()}),
          spaced(toggle('retrig',()=>L.retrig,x=>{L.retrig=x;edited()}))),
          el('div',{className:'knobs'},K(`lfo${l}.rate`),K(`lfo${l}.phase`))))}
    }
    else if(t==='FX'){
      // the rack, run top to bottom; ↑ ↓ move a module. Its knobs take macros from the matrix (the rack is shared by every note)
      const fx=v.fx,order=v.fxOrder=waveFxOrder(v.fxOrder);
      const KNOBS={hyper:['voices','rate','detune','dim','mix'],dist:['drive','mix'],flanger:['rate','depth','fb','spread','mix'],phaser:['rate','depth','freq','fb','spread','mix'],
        chorus:['rate','depth','mix'],delay:['time','fb','tone','mix'],reverb:['mix','predelay','decay','size','attack','early','late','mod','rate','lowcut','highcut'],comp:['thresh','ratio','gain'],eq:['low','high']};
      const NAMES={hyper:'hyper / dimension',dist:'distortion',flanger:'flanger',phaser:'phaser',chorus:'chorus',delay:'delay',reverb:'reverb',comp:'compressor',eq:'eq'};
      const extra={dist:()=>[choice('mode',DIST_MODES,()=>fx.dist.mode,x=>{fx.dist.mode=x;edited()})],
        reverb:()=>[choice('mode',VERB_MODES,()=>fx.reverb.mode,x=>{fx.reverb.mode=x;edited()}),choice('color',VERB_COLORS,()=>fx.reverb.color,x=>{fx.reverb.color=x;edited()})],
        delay:()=>[choice('sync',Object.keys(DELAY_SYNCS),()=>fx.delay.sync,x=>{fx.delay.sync=x;edited()}),spaced(toggle('ping-pong',()=>fx.delay.ping,x=>{fx.delay.ping=x;edited()}))]};
      const move=(k,d)=>{const i=order.indexOf(k),j=i+d;if(j<0||j>=order.length)return;order.splice(i,1);order.splice(j,0,k);edited();show('FX')};
      // a module that is off folds to its name: the rack reads as a list of what is running
      order.forEach((k,i)=>{const sec=el('div',{className:'wave-group wave-fx'},el('div',{className:'wave-row wave-fx-head'},toggle(NAMES[k],()=>fx[k].on,x=>{fx[k].on=x;edited();sec.classList.toggle('off',!x)}),
          el('span',{className:'wave-grow'}),
          el('button',{textContent:'↑',className:'wave-icon',ariaLabel:'move '+k+' up',disabled:i===0,onclick:()=>move(k,-1)}),el('button',{textContent:'↓',className:'wave-icon',ariaLabel:'move '+k+' down',disabled:i===order.length-1,onclick:()=>move(k,1)})),
        el('div',{className:'wave-fx-body'},...(extra[k]?[el('div',{className:'wave-row'},...extra[k]())]:[]),el('div',{className:'knobs'},...KNOBS[k].map(q=>K(k+'.'+q)))));
        sec.dataset.fx=k;sec.classList.toggle('off',!fx[k].on);body.append(sec)});
    }else if(t==='MATRIX'){
      const dsts=WAVE_PARAMS.map(P=>P[0]);
      const list=el('div',{className:'wave-matrix'});
      // a route is one line, source → knob, and its depth under it; ⋯ opens the rest (bipolar, bypass, aux, curve)
      const fill=()=>{list.replaceChildren();
        v.mods.forEach((m,j)=>{
          const amt=el('input',{type:'range',min:-1,max:1,step:.01,value:m.amt,ariaLabel:'amount'}),num=el('b',{textContent:Math.round(m.amt*100)});
          amt.oninput=()=>{m.amt=+amt.value;num.textContent=Math.round(m.amt*100);wavePost(v)};amt.onchange=edited;
          const more=el('div',{className:'wave-route-more'},toggle('± bipolar',()=>m.bi,x=>{m.bi=x;edited()}),toggle('bypass',()=>m.off,x=>{m.off=x;row.classList.toggle('off',x);edited()}),
            choice('aux',['',...WAVE_SOURCES],()=>m.aux||'',x=>{m.aux=x;edited()},x=>x?srcLabel(x):'none'),toggle('aux inv',()=>m.auxInv,x=>{m.auxInv=x;edited()}),curveEditor(m,edited));
          more.hidden=!waveUi.open?.has(m);
          const dots=el('button',{className:'wave-icon',textContent:'⋯',ariaLabel:'more for this route',onclick:()=>{(waveUi.open||=new WeakSet());more.hidden=!more.hidden;more.hidden?waveUi.open.delete(m):waveUi.open.add(m);dots.ariaPressed=String(!more.hidden)}});
          dots.ariaPressed=String(!more.hidden);
          const row=el('div',{className:'wave-route'},
            el('div',{className:'wave-route-head'},choice('source',WAVE_SOURCES,()=>m.src,x=>{m.src=x;edited()},srcLabel),el('span',{className:'wave-arrow',textContent:'→'}),
              choice('to',dsts,()=>m.dst,x=>{m.dst=x;edited()},p=>waveLabel(v,p)),dots,
              el('button',{className:'wave-icon',textContent:'×',ariaLabel:'delete route',onclick:()=>{v.mods.splice(j,1);edited();fill()}})),
            el('div',{className:'wave-amt'},amt,num),more);
          row.classList.toggle('off',!!m.off);list.append(row)})};
      fill();
      body.append(list,el('button',{className:'wave-add',textContent:'+ route',onclick:()=>{v.mods.push({src:'lfo1',dst:'osc1.cut',amt:.25,bi:true,off:false});edited();fill()}}));
    }else if(t==='PATCH'){
      const name=el('input',{value:v.name||'',className:'wave-name',ariaLabel:'patch name'});name.onchange=()=>{v.name=name.value.slice(0,40);pname.textContent=v.name||'patch';edited()};
      const msg=el('p',{className:'wave-hint',textContent:waveUi.msg});const say=s=>{waveUi.msg=msg.textContent=s};
      const load=j=>{loadPatch(v,j);edited();pname.textContent=v.name||'patch';say(`loaded ${v.name}`);show('PATCH')};
      const factory=choice('factory',Object.keys(WAVE_PRESETS),()=>'init',x=>load(WAVE_PRESETS[x]()));
      const lib=patchLib.all(),names=Object.keys(lib).sort(),user=el('div',{className:'wave-lib'});
      for(const n of names)user.append(el('div',{className:'wave-row'},el('button',{className:'wave-lib-name',textContent:n,onclick:()=>load(lib[n])}),
        el('button',{className:'wave-icon',textContent:'×',ariaLabel:'delete '+n,onclick:()=>{patchLib.remove(n);say(`deleted ${n}`);show('PATCH')}})));
      const ta=el('textarea',{placeholder:'paste a patch JSON here',ariaLabel:'patch json'}),file=el('input',{type:'file',accept:'.json,application/json',hidden:true});
      file.onchange=async()=>{const fl=file.files[0];if(!fl)return;try{importPatch(v,await fl.text());edited();say(`imported ${v.name}`);show('PATCH')}catch(e){say('import failed: '+e.message)}};
      body.append(el('div',{className:'wave-row'},name,el('button',{textContent:'save to library',onclick:()=>{const ok=patchLib.save(v.name||'patch',patchOf(v));say(ok?`saved ${v.name}`:'saved for this session only: storage is off here; export to keep it');show('PATCH')}})),
        el('div',{className:'wave-row'},factory,el('span',{className:'wave-grow'}),spaced(el('button',{textContent:'init',onclick:()=>load(waveInit())}))),
        ...(names.length?[el('h4',{textContent:'library'}),user]:[]),
        el('h4',{textContent:'this patch as JSON'}),el('div',{className:'wave-row'},el('button',{textContent:'export',onclick:()=>{ta.value=exportPatch(v);say('exported: downloaded and copied')}}),
          el('button',{textContent:'import text',onclick:()=>{try{importPatch(v,ta.value);edited();pname.textContent=v.name||'patch';say(`imported ${v.name}`);show('PATCH')}catch(e){say('import failed: '+e.message)}}}),
          el('button',{textContent:'import file',onclick:()=>file.click()}),file),
        ta,
        msg);
    }
    redraw();
  };
  // presets in reach without the patch tab: ◀ name ▶ steps through the factory patches, then the library
  const browse=d=>{const lib=patchLib.all(),names=[...Object.keys(WAVE_PRESETS).map(n=>['f',n]),...Object.keys(lib).sort().map(n=>['u',n])];
    let i=names.findIndex(([,n])=>n===v.name);i=((i<0?(d>0?-1:0):i)+d+names.length)%names.length;const[kind,n]=names[i];
    loadPatch(v,kind==='f'?WAVE_PRESETS[n]():lib[n]);edited();pname.textContent=v.name;show(waveUi.tab);if(!ac)initAudio();audition(owner,true)};
  const pname=el('span',{className:'wave-pname',textContent:v.name||'patch'});
  const top=el('div',{className:'wave-top'},el('button',{className:'wave-icon',textContent:'◀',ariaLabel:'previous patch',onclick:()=>browse(-1)}),pname,
    el('button',{className:'wave-icon',textContent:'▶',ariaLabel:'next patch',onclick:()=>browse(1)}),el('span',{className:'wave-grow'}),play,waveBluetooth(owner));
  // the routing bar stays in reach while the knobs scroll: the sources, then the tabs
  const stick=el('div',{className:'wave-stick'},chips,tabs);
  root.append(top,...(part?[part]:[]),stick,body);
  f.append(root);show(waveUi.tab);syncChips();
  waveModHooks.add(()=>{if(!root.isConnected)return false;for(const p of parts)if(p.path&&(p.dot||waveMod.at.has(p.path)))p.draw();return true});waveWatch(v);
  // the bar sticks under the phone sheet's head when that is on top, or at the panel's top
  requestAnimationFrame(()=>{const head=document.querySelector('#sheet-top');stick.style.top=(head&&head.offsetParent&&getComputedStyle(head).order==='0'?head.offsetHeight:0)+'px'});
  rows.push(redraw);
}
