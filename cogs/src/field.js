/* ---------- the FX field: a borderless sphere of effect ---------- */
// A field is a region of the stage with no wall. Anything that sounds inside it (a ball, a pad an arm strikes, a pin
// a reader reads) is also sent to the field's effect: a delay, a reverb or a distortion. How far in it is sets how
// much is sent, and also bends the effect's parameters (nearer the centre: longer and darker echoes, a bigger and
// longer room, harder drive), so things moving through a field are heard moving through it.
// The diffuseness is how strongly the distance counts: at 0 the field is the same everywhere inside, with a sharp
// edge; at 1 it fades from full at the centre to nothing at the rim, and its parameters move the most. It is drawn
// as exactly that: a glow whose falloff is the send's.
// Each field takes one of FIELD_SLOTS slots of the fields worklet; a field past them is silent (and says so).
const FIELD_SLOTS_N=8;
const FIELD_MODES=['delay','reverb','dist'],FIELD_MODE_LABEL={delay:'delay',reverb:'reverb',dist:'distortion'};
const FIELD_SYNC_OPTS=['free','1/16','1/8','1/8.','1/4','1/4.','1/2','1/2.'];
const FIELD_DIST_KINDS=['tube','fold','fuzz','crush'];
const FIELD_COLOR={delay:'#5aa9e6',reverb:'#a98be0',dist:'#e66a4a'};
const FIELD_DEFAULT={r:150,mode:'delay',diffuse:.5,level:.8,insert:0,sync:'1/8.',time:.375,fb:.45,tone:.5,ping:1,size:.6,decay:2.5,predelay:10,verbMode:0,verbColor:2,kind:'tube',drive:.5};
// The knobs of each mode as [key,label,min,max,step] or [key,label,[options]]; then those of every field.
const FIELD_DEFS={
  delay:[['sync','time',FIELD_SYNC_OPTS],['time','free time s',.02,2.5,.005],['fb','feedback',0,.95,.01],['tone','tone',0,1,.01],['ping','ping-pong',[0,1]]],
  reverb:[['size','size',0,1,.01],['decay','decay s',.2,20,.01],['predelay','predelay ms',0,200,1]],
  dist:[['kind','kind',FIELD_DIST_KINDS],['drive','drive',0,1,.01],['tone','tone',0,1,.01]]};
const FIELD_COMMON=[['diffuse','diffuse',0,1,.01],['level','return',0,1.5,.01],['insert','takes dry',0,1,.01],['r','radius',30,800,1]];
// The reverb's own algorithm and colour, when the main reverb (verb.js) is in: its names, as it lists them.
const fieldVerbDefs=()=>typeof VERB_MODES!=='undefined'?[['verbMode','algorithm',VERB_MODES.map((_,i)=>i)],...(typeof VERB_COLORS!=='undefined'?[['verbColor','color',VERB_COLORS.map((_,i)=>i)]]:[])]:[];
const fieldDefs=f=>[...(f.mode==='reverb'?fieldVerbDefs():[]),...FIELD_DEFS[f.mode]];
const fieldOptLabel=(k,x)=>k==='verbMode'?VERB_MODES[x]:k==='verbColor'?VERB_COLORS[x]:k==='ping'?(x?'on':'off'):k==='kind'?x:String(x);
const newField=(id,p)=>({id,x:p.x,y:p.y,...FIELD_DEFAULT,slot:-1,flashes:[]});
const FIELD_KEYS=Object.keys(FIELD_DEFAULT);

// The send at a point: 1 inside the core, falling smoothly to 0 at the rim; the core is the radius less the diffuseness.
function fieldSend(f,p){
  const r=Math.hypot(p.x-f.x,p.y-f.y)/f.r;if(r>=1)return 0;
  const core=1-clamp(f.diffuse,0,1);if(r<=core)return 1;
  const u=(r-core)/(1-core);return 1-u*u*(3-2*u);
}
const fieldQ=(f,p)=>clamp(1-Math.hypot(p.x-f.x,p.y-f.y)/f.r,0,1);
// Every field a sound at `at` is in: [slot, amount] pairs, the dry level left (each takes `insert` of what it sends),
// and the hits that move each field's parameters.
function fieldSendsAt(at){
  if(!W.fields.length||!at)return null;
  let fx=null,dry=1,hits=null;
  for(const f of W.fields){if(f.slot<0)continue;const a=fieldSend(f,at);if(a<=.001)continue;
    (fx||(fx=[])).push(f.slot,+a.toFixed(4));(hits||(hits=[])).push([f,fieldQ(f,at),a]);dry*=1-clamp(f.insert,0,1)*a}
  return fx?{fx,dry,hits}:null;
}
// The sends for the sound being played now: fire() sets them around its play(), the voices read them.
let fxNow=null;
const fieldHitsOut=[];
function fieldHit(f,q,a,when,at,t){
  fieldHitsOut.push(f.slot,when*ac.sampleRate,+q.toFixed(4),+a.toFixed(4));
  if(fieldHitsOut.length===4)queueMicrotask(()=>{if(fieldsNode)fieldsNode.port.postMessage({type:'hits',d:fieldHitsOut.slice()});fieldHitsOut.length=0});
  f.flashes.push({x:at.x,y:at.y,t,a});if(f.flashes.length>16)f.flashes.shift();
}
function withFields(at,when,t,play){
  const s=fieldsNode?fieldSendsAt(at):null;
  if(!s){play();return}
  for(const[f,q,a]of s.hits)fieldHit(f,q,a,when,at,t);
  fxNow=s;try{play()}finally{fxNow=null}
}

/* the audio: one worklet node for every field, each field on its own input */
let fieldsNode=null,fieldsStats=null;const fieldsPosted=new Map();let fieldsBpm=0;
function startFields(){
  fieldsNode=new AudioWorkletNode(ac,'cogs-fields',{numberOfInputs:FIELD_SLOTS_N,numberOfOutputs:1,outputChannelCount:[2],channelCount:2,channelCountMode:'explicit'});
  fieldsNode.port.onmessage=e=>{if(e.data.type==='stats')fieldsStats=e.data};
  fieldsNode.connect(master);
  for(let k=0;k<FIELD_SLOTS_N;k++){engineNode.connect(fieldsNode,k+1,k);if(waveNode)waveNode.connect(fieldsNode,k+1,k)}
  fieldsPosted.clear();fieldsBpm=0;
}
// Each field to its slot: the lowest free one, kept while it lives. A slot whose field is gone is cleared.
function fieldSlots(){
  const used=new Set();
  for(const f of W.fields)if(f.slot>=0&&f.slot<FIELD_SLOTS_N&&!used.has(f.slot))used.add(f.slot);else f.slot=-1;
  for(const f of W.fields)if(f.slot<0){for(let k=0;k<FIELD_SLOTS_N;k++)if(!used.has(k)){f.slot=k;used.add(k);break}}
}
const fieldParams=f=>({mode:f.mode,diffuse:f.diffuse,level:f.level,sync:f.sync==='free'?null:f.sync,time:f.time,fb:f.fb,tone:f.tone,ping:!!f.ping,
  size:f.size,decay:f.decay,predelay:f.predelay,kind:f.kind,drive:f.drive,verb:{mode:f.verbMode,color:f.verbColor}});
// Called every frame: only what changed is posted.
function postFields(){
  fieldSlots();
  if(!fieldsNode)return;
  const bpm=S.bpm||120;if(bpm!==fieldsBpm){fieldsBpm=bpm;fieldsNode.port.postMessage({type:'tempo',bpm})}
  const live=new Set();
  for(const f of W.fields){if(f.slot<0)continue;live.add(f.slot);const p=fieldParams(f),sig=JSON.stringify(p);
    if(fieldsPosted.get(f.slot)!==sig){fieldsPosted.set(f.slot,sig);fieldsNode.port.postMessage({type:'field',slot:f.slot,p})}}
  for(const k of [...fieldsPosted.keys()])if(!live.has(k)){fieldsPosted.delete(k);fieldsNode.port.postMessage({type:'field',slot:k,p:null})}
}
// Where the field's parameters are now, from the worklet: 0 at the rim, 1 at the centre.
const fieldFocus=f=>fieldsStats&&f.slot>=0?fieldsStats.q[f.slot]:null;

/* drawing: a glow with no edge, its falloff the send's; ripples where things sounded in it */
function drawField(f){
  const col=FIELD_COLOR[f.mode]||FIELD_COLOR.delay,core=1-clamp(f.diffuse,0,1),silent=f.slot<0;
  const g=cx.createRadialGradient(f.x,f.y,0,f.x,f.y,f.r);
  const a=silent?.05:.16;
  g.addColorStop(0,hexA(col,a*1.25));
  if(core>.02)g.addColorStop(core*.98,hexA(col,a));
  for(let i=1;i<=4;i++){const u=i/5,r=core+(1-core)*u;g.addColorStop(Math.min(.999,r),hexA(col,a*(1-u*u*(3-2*u))))}
  g.addColorStop(1,hexA(col,0));
  cx.fillStyle=g;circle(f.x,f.y,f.r,true);
  // where the effect's parameters are now: a faint ring at that distance from the centre
  const q=fieldFocus(f);
  // (dotted and faint, and only while sound is in it: the field itself has no line round it)
  if(q!=null&&f.diffuse>.02&&fieldsStats&&fieldsStats.quiet[f.slot]<ac.sampleRate*.3){cx.strokeStyle=hexA(col,.22);cx.lineWidth=PX;cx.setLineDash([1*PX,5*PX]);circle(f.x,f.y,Math.max(10*PX,(1-q)*f.r));cx.setLineDash([])}
  for(const h of f.flashes){if(clock.view<h.t)continue;const k=(clock.view-h.t)/.5;if(k>1)continue;
    cx.globalAlpha=(1-k)*.6*Math.min(1,h.a*1.5);cx.strokeStyle=col;cx.lineWidth=1.5*PX;circle(h.x,h.y,(4+k*26)*PX);cx.globalAlpha=1}
  if(f.flashes.length&&clock.view-f.flashes[f.flashes.length-1].t>.5)f.flashes.length=0;
  // the centre: the mode's glyph in a small ring, which is what is grabbed
  cx.strokeStyle=col;cx.lineWidth=1.2*PX;cx.globalAlpha=silent?.4:.9;circle(f.x,f.y,9*PX);
  fieldGlyph(f.mode,f.x,f.y,6*PX);cx.globalAlpha=1;cx.lineWidth=PX;
}
function fieldGlyph(mode,x,y,s){
  cx.beginPath();
  if(mode==='delay'){for(let i=0;i<3;i++){const r=s*(.35+i*.35);cx.moveTo(x-s*.6+r*Math.cos(-.9),y+r*Math.sin(-.9));cx.arc(x-s*.6,y,r,-.9,.9)}}
  else if(mode==='reverb'){cx.arc(x,y,s*.3,0,TAU);cx.moveTo(x+s*.75,y);cx.arc(x,y,s*.75,0,TAU)}
  else{cx.moveTo(x-s,y);for(let i=0;i<4;i++)cx.lineTo(x-s+(i+.5)*s/2,y+(i%2?s*.7:-s*.7));cx.lineTo(x+s,y)}
  cx.stroke();
}
// Selected: the rim it fades to, dashed, with a handle on it to pull the radius, and the core it is full within.
function drawFieldSel(f){
  const col=FIELD_COLOR[f.mode];cx.strokeStyle=col;cx.lineWidth=PX;cx.globalAlpha=.6;cx.setLineDash([4*PX,5*PX]);circle(f.x,f.y,f.r);
  const core=1-clamp(f.diffuse,0,1);if(core>.02&&core<.98){cx.globalAlpha=.3;circle(f.x,f.y,f.r*core)}
  cx.setLineDash([]);cx.globalAlpha=1;halo(f.x,f.y,9);
  const h=fieldHandle(f);cx.fillStyle=col;circle(h.x,h.y,4*PX,true);cx.strokeStyle=FG;circle(h.x,h.y,4*PX);
}
const fieldHandle=f=>({x:f.x+f.r*Math.SQRT1_2,y:f.y+f.r*Math.SQRT1_2});
function drawFieldIcon(){
  const g=cx.createRadialGradient(0,0,0,0,0,22);g.addColorStop(0,hexA(FIELD_COLOR.delay,.45));g.addColorStop(.5,hexA(FIELD_COLOR.delay,.25));g.addColorStop(1,hexA(FIELD_COLOR.delay,0));
  cx.fillStyle=g;circle(0,0,22,true);cx.strokeStyle=FIELD_COLOR.delay;cx.lineWidth=1.2;circle(0,0,7);fieldGlyph('delay',0,0,5);
}
const hexA=(hex,a)=>{const n=parseInt(hex.slice(1),16);return `rgba(${n>>16},${n>>8&255},${n&255},${clamp(a,0,1).toFixed(3)})`};

/* settings */
function setFieldMode(f,m){if(!FIELD_MODES.includes(m))return;f.mode=m;if(m==='dist'&&f.insert===0)f.insert=.8;else if(m!=='dist'&&f.insert===.8)f.insert=0;f.tone=m==='dist'?.6:.5}
function fieldRows(fs,f){
  selRow(fs,'effect',FIELD_MODES,()=>f.mode,x=>{setFieldMode(f,x);changed();buildInspector()},x=>FIELD_MODE_LABEL[x]);
  for(const d of fieldDefs(f)){
    if(Array.isArray(d[2]))selRow(fs,d[1],d[2].map(String),()=>String(f[d[0]]),x=>{f[d[0]]=typeof d[2][0]==='number'?+x:x;changed()},x=>fieldOptLabel(d[0],typeof d[2][0]==='number'?+x:x));
    else numRow(fs,d[1],d[2],d[3],d[4],()=>f[d[0]],x=>{f[d[0]]=x;changed()},FIELD_DEFAULT[d[0]]);
  }
  for(const d of FIELD_COMMON)numRow(fs,d[1],d[2],d[3],d[4],()=>f[d[0]],x=>{f[d[0]]=x;changed()},FIELD_DEFAULT[d[0]]);
  roRow(fs,'now',()=>f.slot<0?`silent: only ${FIELD_SLOTS_N} fields sound at once`:fieldFocus(f)==null?'-':`parameters at ${Math.round(fieldFocus(f)*100)}% in`);
}
// Serialized: its place and its settings; the slot and the ripples are its own business.
const fieldOut=f=>{const o={id:f.id,x:f.x,y:f.y};for(const k of FIELD_KEYS)o[k]=f[k];return o};
function fieldIn(j){
  const f=newField(j.id,{x:+j.x||0,y:+j.y||0});
  for(const k of FIELD_KEYS){const d=FIELD_DEFAULT[k],x=j[k];if(x==null)continue;if(typeof d==='number'&&Number.isFinite(+x))f[k]=+x;else if(typeof d==='string'&&typeof x==='string')f[k]=x}
  if(!FIELD_MODES.includes(f.mode))f.mode='delay';if(!FIELD_SYNC_OPTS.includes(f.sync))f.sync='1/8.';if(!FIELD_DIST_KINDS.includes(f.kind))f.kind='tube';
  f.r=clamp(f.r,30,800);f.diffuse=clamp(f.diffuse,0,1);f.insert=clamp(f.insert,0,1);
  return f;
}
