/* ---------- wave synth: patches, the library, the note path ---------- */
// A wave voice is the patch itself: {kind:'wave', note, octave, semi, level, velSense, name, osc[3], sub, noise,
// env[3], lfo[4], macro[8], macroNames[8], mods[], vol, glide, gate}. Knob values live where wavePath puts them.
function waveInit(){
  const p={name:'init',osc:[],sub:{},noise:{},tune:{},env:[],lfo:[],macro:[],macroNames:[],mods:[],
    voicing:{mode:'poly',always:true,scale:false},
    fxOrder:[...WAVE_FX],
    fx:{hyper:{on:false},flanger:{on:false},phaser:{on:false},dist:{on:false,mode:'tube'},chorus:{on:false},delay:{on:false,sync:'1/8.',ping:true},eq:{on:false},comp:{on:false}}};
  for(let i=0;i<WAVE_OSCS;i++)p.osc.push({on:i===0,table:'basic',filter:'lp24',warpMode:'bend',warp2Mode:'off',harm:Array.from({length:WAVE_HARMS},(_,k)=>k===0?1:0)});
  for(let i=0;i<WAVE_ENVS;i++)p.env.push({});
  for(let i=0;i<WAVE_LFOS;i++)p.lfo.push({shape:'sine',retrig:true,sync:'off'});
  for(let i=0;i<WAVE_MACROS;i++){p.macro.push(0);p.macroNames.push('macro '+(i+1))}
  for(const P of WAVE_PARAMS)waveSet(p,P[0],P[4]);
  p.osc[0].pos=2/3; // a saw, as a synth's init patch starts
  return p;
}
// Anything loaded (a save, an import, a preset) is laid over a fresh init, checked value by value.
function wavePatch(j){
  const p=waveInit();if(!j||typeof j!=='object')return p;
  if(typeof j.name==='string')p.name=j.name.slice(0,40);
  for(const P of WAVE_PARAMS){let x;try{x=+waveGet(j,P[0])}catch(e){continue}if(Number.isFinite(x))waveSet(p,P[0],Math.min(P[3],Math.max(P[2],x)))}
  (j.osc||[]).slice(0,WAVE_OSCS).forEach((o,i)=>{if(!o)return;const q=p.osc[i];q.on=!!o.on;if(WAVE_TABLES.includes(o.table))q.table=o.table;if(WAVE_FILTERS.includes(o.filter))q.filter=o.filter;if(WARP_MODES.includes(o.warpMode))q.warpMode=o.warpMode;if(WARP_MODES.includes(o.warp2Mode))q.warp2Mode=o.warp2Mode;
    if(Array.isArray(o.harm))q.harm=Array.from({length:WAVE_HARMS},(_,k)=>Math.min(1,Math.max(0,+o.harm[k]||0)))});
  (j.lfo||[]).slice(0,WAVE_LFOS).forEach((l,i)=>{if(!l)return;if(LFO_SHAPES.includes(l.shape))p.lfo[i].shape=l.shape;p.lfo[i].retrig=l.retrig!==false;if(l.sync in LFO_SYNCS)p.lfo[i].sync=l.sync});
  const vc=j.voicing||{};if(VOICE_MODES.includes(vc.mode))p.voicing.mode=vc.mode;if('always' in vc)p.voicing.always=!!vc.always;p.voicing.scale=!!vc.scale;
  p.fxOrder=waveFxOrder(j.fxOrder);
  const fx=j.fx||{};for(const k of WAVE_FX){const q=fx[k];if(!q)continue;p.fx[k].on=!!q.on;
    if(k==='dist'&&DIST_MODES.includes(q.mode))p.fx.dist.mode=q.mode;if(k==='delay'){if(q.sync in DELAY_SYNCS)p.fx.delay.sync=q.sync;p.fx.delay.ping=q.ping!==false}}
  (j.macroNames||[]).slice(0,WAVE_MACROS).forEach((n,i)=>{if(typeof n==='string')p.macroNames[i]=n.slice(0,16)});
  p.mods=(j.mods||[]).filter(m=>m&&WAVE_SOURCES.includes(m.src)&&WAVE_INDEX.has(m.dst)).map(m=>({src:m.src,dst:m.dst,amt:Math.min(1,Math.max(-1,+m.amt||0)),bi:!!m.bi,off:!!m.off,
    aux:WAVE_SOURCES.includes(m.aux)?m.aux:'',auxInv:!!m.auxInv,curve:Math.min(1,Math.max(-1,+m.curve||0))}));
  return p;
}
// The voice-level keys every kind shares, kept apart from the patch so a loaded patch keeps the part's pitch and level.
const WAVE_VOICE_KEYS=['kind','note','octave','semi','level','velSense'];
// The synth transposes itself (tune.oct, tune.semi, in the patch); a part's octave and semitones from before that,
// or carried over from another kind, fold into the patch's, so the part's own stay at 0 and its dial needs no pitch.
function waveVoice(j,keep={}){
  const v={kind:'wave',note:60,octave:0,semi:0,level:.7,velSense:.7,...keep,...wavePatch(j)};
  if(v.octave||v.semi){v.tune.oct=Math.min(4,Math.max(-4,v.tune.oct+(v.octave|0)));v.tune.semi=Math.min(12,Math.max(-12,v.tune.semi+(v.semi|0)));v.octave=v.semi=0}
  v.dial=waveDial(j&&j.dial);return v;
}
// The part's dial carries its volume, and up to WAVE_DIAL knobs of the patch the player pins there.
const WAVE_DIAL=12;
const waveDial=d=>Array.isArray(d)?[...new Set(d.filter(x=>typeof x==='string'&&WAVE_INDEX.has(x)))].slice(0,WAVE_DIAL):[];
const patchOf=v=>{const p={};for(const k in v)if(!WAVE_VOICE_KEYS.includes(k))p[k]=v[k];return p};
function loadPatch(v,j){const keep={};for(const k of WAVE_VOICE_KEYS)keep[k]=v[k];for(const k of Object.keys(v))delete v[k];Object.assign(v,waveVoice(j,keep))}

// Factory presets: each is an init patch with a few knobs turned and routes drawn.
const WAVE_PRESETS={
  init:()=>waveInit(),
  'super saw':()=>{const p=waveInit();Object.assign(p.osc[0],{pos:.667,uni:7,detune:.45,spread:1,cut:6000,res:.15,famt:.25,fd:.9,fs:.4});
    Object.assign(p.osc[1],{on:true,pos:.667,oct:-1,level:.45,uni:3,detune:.3,cut:4000});p.sub.level=.3;p.fx.chorus.on=true;p.fx.delay.on=true;p.fx.delay.mix=.18;p.name='super saw';return p},
  pluck:()=>{const p=waveInit();Object.assign(p.osc[0],{pos:.667,d:.35,s:0,r:.25,cut:400,res:.3,famt:.55,fd:.25,fs:0});
    Object.assign(p.osc[1],{on:true,pos:1,semi:12,level:.25,d:.2,s:0,r:.2,cut:900,famt:.5,fd:.15,fs:0});p.mods=[{src:'vel',dst:'osc1.cut',amt:.3,bi:false,off:false}];p.name='pluck';return p},
  pad:()=>{const p=waveInit();p.gate=1.2;Object.assign(p.osc[0],{table:'vocal',pos:.2,uni:5,detune:.35,a:.9,d:1.5,s:.8,r:2.2,cut:2500,filter:'lp12'});
    Object.assign(p.osc[1],{on:true,table:'sweep',pos:.6,oct:1,level:.3,uni:3,a:1.6,r:2.5,cut:3500});
    p.lfo[0].shape='tri';p.lfo[0].rate=.15;p.lfo[1].rate=.4;
    p.mods=[{src:'lfo1',dst:'osc1.pos',amt:.6,bi:false,off:false},{src:'lfo2',dst:'osc2.cut',amt:.25,bi:true,off:false}];p.fx.chorus.on=true;p.fx.delay.on=true;p.fx.delay.sync='1/4.';p.fx.delay.fb=.55;p.name='pad';return p},
  'reese bass':()=>{const p=waveInit();Object.assign(p.osc[0],{pos:.667,oct:-1,uni:2,detune:.3,spread:.3,cut:900,res:.2,drive:.4});
    Object.assign(p.osc[1],{on:true,pos:.667,oct:-1,fine:14,level:.6,cut:900});p.sub.level=.5;p.sub.oct=-1;p.lfo[0].rate=.3;
    p.mods=[{src:'lfo1',dst:'osc1.cut',amt:.25,bi:true,off:false}];p.name='reese bass';return p},
  wobble:()=>{const p=waveInit();Object.assign(p.osc[0],{pos:.9,oct:-1,cut:250,res:.55,drive:.5});p.sub.level=.5;p.gate=.6;
    p.lfo[0].rate=3;p.lfo[0].shape='tri';p.macroNames[0]='wobble';p.macro[0]=.7;
    p.lfo[0].sync='1/8';p.fx.dist.on=true;p.fx.dist.drive=.45;p.fx.comp.on=true;
    p.mods=[{src:'lfo1',dst:'osc1.cut',amt:.55,bi:false,off:false},{src:'m1',dst:'dist.drive',amt:.5,bi:false,off:false}];p.name='wobble';return p},
  'additive bell':()=>{const p=waveInit();Object.assign(p.osc[0],{table:'harmonics',harm:[1,0,.6,0,0,.4,0,.25,0,0,.3,0,0,0,.2,0,0,0,0,.15,0,0,0,0,0,0,0,0,0,0,.1,0],
    d:2.5,s:0,r:2,filter:'off'});Object.assign(p.osc[1],{on:true,table:'organ',pos:.5,oct:2,level:.2,d:.6,s:0,r:.6,filter:'off'});p.gate=.1;p.name='additive bell';return p},
  'formant lead':()=>{const p=waveInit();Object.assign(p.osc[0],{table:'vocal',uni:3,detune:.2,cut:5000,glide:0});p.glide=.08;p.gate=.4;
    p.lfo[0].rate=5;p.mods=[{src:'env2',dst:'osc1.pos',amt:1,bi:false,off:false},{src:'lfo1',dst:'osc1.fine',amt:.04,bi:true,off:false}];
    Object.assign(p.env[1],{a:.01,d:.8,s:.2});p.name='formant lead';return p},
};

/* the library: saved patches in local storage, and JSON files in and out */
const LIB_KEY=STORE+'.patches';
const patchLib={
  mem:{},
  all(){try{const s=localStorage.getItem(LIB_KEY);return s?JSON.parse(s):{}}catch(e){return this.mem}},
  put(all){try{localStorage.setItem(LIB_KEY,JSON.stringify(all));return true}catch(e){this.mem=all;return false}},
  save(name,p){const all=this.all();all[name]={...p,name};return this.put(all)},
  remove(name){const all=this.all();delete all[name];this.put(all)},
};
function exportPatch(v){
  const p=patchOf(v),text=JSON.stringify({cogsWave:1,...p},null,1),name=(p.name||'patch').replace(/[^\w-]+/g,'-')+'.cogs-wave.json';
  try{const a=el('a',{href:URL.createObjectURL(new Blob([text],{type:'application/json'})),download:name});document.body.append(a);a.click();a.remove()}catch(e){}
  try{navigator.clipboard?.writeText(text).catch(()=>{})}catch(e){}
  return text;
}
function importPatch(v,text){const j=JSON.parse(text);if(!j||typeof j!=='object'||!Array.isArray(j.osc))throw new Error('not a wave patch: no osc list');loadPatch(v,j)}

/* the note path */
let waveNode=null,waveVoices=0,waveNotes=0,waveLast=null;const wavePosted=new WeakMap();let waveBpm=0;let waveIds=0;const waveIdOf=new WeakMap();
function wavePost(v){
  if(!waveNode)return 0;let id=waveIdOf.get(v);if(!id){id=++waveIds;waveIdOf.set(v,id)}
  const bpm=S.bpm||120;if(bpm!==waveBpm){waveBpm=bpm;waveNode.port.postMessage({type:'tempo',bpm})}
  const s=JSON.stringify(v);if(wavePosted.get(v)!==s){wavePosted.set(v,s);waveNode.port.postMessage({type:'patch',id,p:JSON.parse(s)})}
  return id;
}
function waveNote(v,midi,when,amp,pan,key,x,hold=0,dark=0){
  const id=wavePost(v);if(!id)return;waveNotes++;waveLast=voiceMidi(v,midi);
  waveNode.port.postMessage({type:'note',id,at:when*ac.sampleRate,key,midi:voiceMidi(v,midi),amp,vel:clamp(x??amp,0,1),pan,hold,dark});
}
function startWave(){
  waveNode=new AudioWorkletNode(ac,'cogs-wave',{numberOfInputs:0,numberOfOutputs:1,outputChannelCount:[2]});
  waveNode.port.onmessage=e=>{if(e.data.type==='stats')waveVoices=e.data.voices};
  waveNode.connect(engineNode);
}
