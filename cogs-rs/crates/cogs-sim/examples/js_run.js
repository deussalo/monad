// The JS reference, headless: runs cogs/src's sim (sim.js, motion.js, pit.js, probe.js, field.js and what they read)
// in node with the browser stubbed, loads the same worlds as examples/run.rs, advances them in the same 15 ms ticks,
// and writes <name>.js.events.json / .js.save.json / .js.after.json beside the Rust dump for comparison.
// node js_run.js <dir> [secs]
const fs = require('fs'), path = require('path'), vm = require('vm');
const SRC = path.join(__dirname, '../../../../cogs/src'), dir = process.argv[2] || '/tmp/cogs-sim-cmp', secs = +(process.argv[3] || 10);
const INCLUDE = /^\s*(?:\/\/|\/\*)@include (\S+?)(?:\*\/)?\s*$/;
const expand = f => fs.readFileSync(path.join(SRC, f), 'utf8').split('\n').map(l => { const m = l.match(INCLUDE); return m ? expand(m[1]) : l; }).join('\n');
const files = ['verb.js', 'climax.js', 'sim.js', 'wave.spec.js', 'wave.js', 'motion.js', 'pit.js', 'probe.js', 'field.js'];
const stub = `
const document={querySelector:()=>null,addEventListener(){},documentElement:{}};const window={};
const addEventListener=()=>{},matchMedia=()=>({matches:false}),requestAnimationFrame=()=>0;
function select(){};function applyAudioSettings(){};function syncRows(){};function refreshInspector(){};function buzz(){};
const ui={},G={},PITCH_KEYS=new Set(['note','octave','semi']);function fitTarget(){return {x:0,y:0,z:1}}
`;
const driver = `
var __rec=null;
playVoice=(v,midi,when,amp,pan,key,x,hold=0)=>{__rec.midi=midi;__rec.key=key;__rec.hold=hold};
pluckSound=(s,midi,u)=>{__rec.midi=midi;__rec.key=s.id*128+Math.round(midi);__rec.hold=0};
const __evs=[];
fire=function(v,t,vn,at,src,gain,play){
  stats.events++;
  const vx=clamp(vn/S.vRef,0,1),amp=gain*v.level*(1-v.velSense+v.velSense*Math.pow(vx,S.velCurve));
  if(amp<=0)return;
  __rec={};play(t,amp,0,vx,0);__evs.push([t,src,__rec.midi,amp,vn,__rec.hold,__rec.key]);
};
globalThis.__go=(j,secs)=>{
  deserialize(j);const save=serialize();stats.steps=0;
  clock.view=0;simT=0;__evs.length=0;
  const tick=.015,n=Math.round(secs/tick);
  for(let i=1;i<=n;i++)advance(i*tick);
  return {events:__evs.slice(),save:JSON.parse(JSON.stringify(save)),after:JSON.parse(JSON.stringify(serialize())),steps:stats.steps};
};
`;
const code = stub + files.map(expand).join('\n;\n') + driver;
const ctx = vm.createContext({ console, Math, JSON, performance, setTimeout: () => 0, clearTimeout: () => 0, queueMicrotask: () => 0, structuredClone,
  Float32Array, Float64Array, Uint8Array, Map, Set, Array, Object, Number, String, Error, Promise, isFinite, parseFloat, parseInt });
vm.runInContext(code, ctx, { filename: 'cogs.js' });
for (const [name, p] of [['default', 'default-world.json'], ['capstan', '../worlds/capstan.json'], ['dubstep', '../worlds/dubstep.json']]) {
  const j = JSON.parse(fs.readFileSync(path.join(SRC, p), 'utf8'));
  const r = ctx.__go(j, secs);
  fs.writeFileSync(path.join(dir, name + '.js.events.json'), JSON.stringify(r.events));
  fs.writeFileSync(path.join(dir, name + '.js.save.json'), JSON.stringify(r.save));
  fs.writeFileSync(path.join(dir, name + '.js.after.json'), JSON.stringify(r.after));
  console.log(name, r.events.length, 'events', r.steps, 'steps');
}
