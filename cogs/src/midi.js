/* ---------- MIDI in: a keyboard plays the selected synth and records onto its plate ---------- */
// Two ways in. Bluetooth: Web Bluetooth straight to a BLE-MIDI keyboard (a Korg microKEY Air, or anything that speaks
// the BLE-MIDI service), no pairing app needed; Chrome on Android or desktop. MIDI: Web MIDI, for USB keyboards and
// Bluetooth ones the system already connected. The notes go to the synth of what is selected (a reader, a pad, a box,
// or a plate, which means its reader), else to the last one that was; its synth, octave and all, transposes them as it
// does every note. Sustain (CC64) holds note-offs; the mod wheel (CC1) turns a synth's macro 1 unless a cam is turning it.
const BLE_MIDI_SERVICE='03b80e5a-ede8-4b33-a751-6ce34ec4c700',BLE_MIDI_CHAR='7772e5db-3868-4112-a1a9-f2669d106bf3';
const midi={target:null,devices:new Map(),held:new Map(),sustain:false,sustained:new Set(),msg:'',last:null,mod:null,modAt:0,count:0,rec:null};
// The phone reads it out; the desk shows it in the panel, or for a moment on the stage while the panel is shut.
const midiSay=t=>{midi.msg=t;midiSync();if(typeof showInteraction==='function')showInteraction(()=>'MIDI: '+t);
  else if(!panelShown()){notice('MIDI: '+t);clearTimeout(midi.noticeT);midi.noticeT=setTimeout(()=>{$('#notice').hidden=true},3000)}};
// A plate's reader, and a reader's plate: the seat if there is one, otherwise whatever lies across.
const plateReader=c=>W.readers.find(r=>r.on===c)||W.readers.find(r=>segDist(c,r.x1,r.y1,r.x2,r.y2)<radius(c))||null;
const readerPlate=r=>r.on?.track?r.on:W.cogs.filter(c=>c.track&&segDist(c,r.x1,r.y1,r.x2,r.y2)<radius(c)).sort((a,b)=>b.layer-a.layer)[0]||null;
const midiPlayable=o=>!!o?.voice&&(W.readers.includes(o)||W.pads.includes(o)||W.pits.includes(o));
// The synth a selection means.
function midiVoiced(s){
  if(!s)return null;
  if(s.type==='cog')return s.o.track?plateReader(s.o):null;
  if(s.type==='dot')return plateReader(ui.dotOf);
  return midiPlayable(s.o)?s.o:null;
}
// the part that plays: the one being recorded through, the selection's synth, otherwise the last one selected
function midiOwner(){if(midi.rec)return midi.rec.rd;const o=midiVoiced(sel);if(o)midi.target=o;return midiPlayable(midi.target)?midi.target:null}
function midiNoteOn(note,vel){
  const o=midiOwner();if(!o){midiSay('select a synth to play it');return}
  midiNoteOff(note,true);
  const v=o.voice,vx=vel/127,key=o.id*128+note;
  midi.held.set(note,{key,o});o.flash=clock.view;o.lastMidi=voiceMidi(v,note);midi.last=note;midi.count++;
  recNoteOn(note,vx,v);
  if(!ac)initAudio();if(!ac||engineState!=='ready')return;
  playVoice(v,note,ac.currentTime+.003,v.level*velGain(v,vx),0,key,vx,30);
}
const velGain=(v,vx)=>1-v.velSense+v.velSense*Math.pow(vx,S.velCurve);
function midiNoteOff(note,now){
  const h=midi.held.get(note);if(!h)return;
  if(midi.sustain&&!now){midi.sustained.add(note);return}
  midi.held.delete(note);midi.sustained.delete(note);recNoteOff(note);if(ac&&engineNode)damp(h.key,ac.currentTime);
}
function midiPedal(down){midi.sustain=down;if(!down){for(const n of [...midi.sustained])midiNoteOff(n,true);midi.sustained.clear()}}
function midiMod(x){
  const o=midiOwner(),v=o&&o.voice;if(!v||v.kind!=='wave'||camsOn(v).some(c=>c.macro===1))return;
  v.macro[0]=x;midi.mod=v;
  // a wheel sends a stream: the patch goes to the worklet at most every 30 ms, and once more when it stops
  const t=performance.now();if(t-midi.modAt>30){midi.modAt=t;wavePost(v)}
  clearTimeout(midi.modTimer);midi.modTimer=setTimeout(()=>{wavePost(v);changed()},120);
}
function midiPanic(){for(const n of [...midi.held.keys()])midiNoteOff(n,true);midi.sustain=false;midi.sustained.clear()}
// one MIDI message, from either way in
function midiMessage(st,d1,d2){
  const t=st&0xf0;
  if(t===0x90&&d2>0)midiNoteOn(d1,d2);
  else if(t===0x80||t===0x90)midiNoteOff(d1);
  else if(t===0xb0){if(d1===64)midiPedal(d2>=64);else if(d1===1)midiMod(d2/127);else if(d1===123||d1===120)midiPanic()}
}
/* Recording: what the keyboard plays lands on the plate as pins, where the reader was when you played it, so it comes
   back round in time. A take adds to what is there and is one undo step. Notes snap to the roll's snap (off: as played). */
// What you hear now, on the sim's clock: exact between frames, since a key can land anywhere in one.
const heardNow=()=>clock.paused?clock.view:Math.max(clock.view,clock.audio&&heardOffset!==null?performance.now()/1000+heardOffset:nowSec()-clock.off);
// Where a reader meets the circle of radius R about c: its world angle, and how far along the reader.
function readerCrossing(c,R,rd){
  const dx=rd.x2-rd.x1,dy=rd.y2-rd.y1,fx=rd.x1-c.x,fy=rd.y1-c.y,A=dx*dx+dy*dy,B=2*(fx*dx+fy*dy),D=B*B-4*A*(fx*fx+fy*fy-R*R);
  if(!A||D<0)return null;
  for(const u of [(-B-Math.sqrt(D))/(2*A),(-B+Math.sqrt(D))/(2*A)])if(u>=0&&u<=1)return {u,phi:Math.atan2(fy+dy*u,fx+dx*u)};
  return null;
}
// The rings this reader can play, each with the note it sounds there (a comb's comes from where it is crossed).
function recRings(c,rd){
  const t=c.track,out=[];
  for(let ring=0;ring<trackRings(t);ring++){const h=readerCrossing(c,ringR(c,ring),rd);if(!h)continue;
    out.push({ring,phi:h.phi,note:rd.tines?scaleNote(rd.combBase,rd.combScale,Math.min(rd.tines-1,Math.floor(h.u*rd.tines))):ringNote(t,ring)})}
  return out;
}
// The plate makes room for the note: an empty one moves to its octave, then the range grows (existing pins keep their pitch).
function recFit(t,note){
  const n0=trackRings(t),b0=trackBase(t),L=SCALES[t.scale].length,top=RANGES.at(-1);
  if(!t.dots.length&&(note<trackBase(t)||note>ringNote(t,trackRings(t)-1)))t.octave=clamp(Math.floor((note-t.root)/12)-1,OCTAVES[0],OCTAVES.at(-1));
  while(note<trackBase(t)&&t.range<top&&t.octave>OCTAVES[0]){t.octave--;t.range++;for(const d of t.dots)d.ring+=L}
  while(note>ringNote(t,trackRings(t)-1)&&t.range<top)t.range++;
  return trackRings(t)!==n0||trackBase(t)!==b0;
}
// The nearest ring to the note, folding it by octaves into what the reader can reach.
function recPick(rings,note){
  if(!rings.length)return null;
  const ns=rings.map(r=>r.note),lo=Math.min(...ns),hi=Math.max(...ns);let n=note;
  while(n<lo)n+=12;while(n>hi&&n-12>=lo)n-=12;
  return rings.reduce((a,b)=>Math.abs(b.note-n)<Math.abs(a.note-n)?b:a);
}
const recLive=r=>W.cogs.includes(r.c)&&!!r.c.track&&W.readers.includes(r.rd);
function recNoteOn(note,vx,v){
  const r=midi.rec;if(!r)return;if(!recLive(r)){recStop();return}
  const c=r.c,t=c.track;
  if(clock.paused||!c.w){midiSay(clock.paused?'paused: press play to record':`plate ${c.id} is not turning: nothing recorded`);return}
  if(!r.rd.tines&&recFit(t,note))r.grew=true;
  const hit=recPick(recRings(c,r.rd),note);if(!hit){midiSay('the reader does not reach this plate\'s rings');return}
  const at=heardNow(),g=snapOf(c);let a=wrap(hit.phi-(c.th-c.w*(simT-at))),skip=0;
  if(g){const q=colAngle(c,Math.round(rollU(c,a)*t.div/g)*g),lead=-wrapPi(q-a)/c.w;
    // snapped ahead of the reader, the pin would sound again in a moment: you have just played it
    if(lead>0)skip=at+lead+.02;a=q}
  const d={ring:hit.ring,a,p:1,v:clamp(velGain(v,vx),.02,1),len:1,flash:at,skip:Infinity};
  t.dots=t.dots.filter(o=>!sameCell(o,d));t.dots.push(d);
  r.open.set(note,{d,at,skip,g});r.n++;if(hit.note!==note)r.moved++;
}
function recClose(h,c){
  const t=c.track,steps=(heardNow()-h.at)*Math.abs(c.w)*t.div/TAU;
  h.d.len=clamp(h.g?Math.max(h.g,Math.round(steps/h.g)*h.g):tidy(steps),MIN_LEN,t.div);h.d.skip=h.skip;
}
function recNoteOff(note){const r=midi.rec,h=r?.open.get(note);if(!h)return;r.open.delete(note);if(recLive(r))recClose(h,r.c)}
// What record would write to: the selected plate and its reader, or the selected reader and its plate.
function recTarget(){
  if(!sel)return null;
  const c=sel.type==='cog'&&sel.o.track?sel.o:sel.type==='dot'?ui.dotOf:sel.type==='reader'?readerPlate(sel.o):null;
  return c&&{c,rd:sel.type==='reader'?sel.o:plateReader(c)};
}
function recStart(){
  const q=recTarget();
  if(!q){midiSay('select a plate or its reader to record');return}
  if(!q.rd){midiSay('put a reader on this plate to record');return}
  initAudio();if(edits.pending)edits.commit();if(clock.paused)setPaused(false);
  midi.rec={...q,open:new Map(),n:0,moved:0,grew:false};midi.target=q.rd;
  midiSay(!q.c.w?`plate ${q.c.id} is not turning: give it a motor to record`:midi.devices.size?`recording onto plate ${q.c.id}`:`recording onto plate ${q.c.id}: connect a keyboard to play`);
}
function recStop(){
  const r=midi.rec;if(!r)return;midi.rec=null;
  if(!recLive(r)){midiSay('recording stopped: its plate is gone');return}
  for(const h of r.open.values())recClose(h,r.c);
  if(r.n){changed();edits.commit();if(r.grew&&inspectorLive())buildInspector()}
  midiSay(`${r.n} note${r.n===1?'':'s'} on plate ${r.c.id}`+(r.moved?` (${r.moved} moved to fit its scale or reach)`:''));
}
const recToggle=()=>{if(midi.rec)recStop();else recStart();buzz(8)};
// A red ring round the plate being recorded onto. Undo or a load can take it away mid-take.
function drawRec(){
  const r=midi.rec;if(!r)return;if(!recLive(r)){recStop();return}
  cx.lineWidth=2*PX;cx.strokeStyle=HOT;circle(r.c.x,r.c.y,radius(r.c)+6*PX);cx.lineWidth=PX;
}
// BLE-MIDI packets: a header byte, then timestamped messages; running status and several messages per packet allowed
function bleMidi(d){
  let i=1,status=0;
  while(i<d.length){
    if(d[i]&0x80){i++;if(i<d.length&&d[i]&0x80){status=d[i];i++}}
    if(status===0xf0){while(i<d.length&&d[i]!==0xf7)i++;i++;status=0;continue}
    if(!status||status>=0xf0){i++;continue}
    const n=(status&0xe0)===0xc0?1:2;if(i+n>d.length)break;
    midiMessage(status,d[i],n>1?d[i+1]:0);i+=n;
  }
}
// The page may live in a sandboxed frame that is not allowed MIDI or Bluetooth; it says so, and how to get them.
const MIDI_HELP='open cogs.html as its own page in Chrome (Android or desktop); iPhone and iPad browsers have neither Web MIDI nor Web Bluetooth';
async function connectBluetooth(){
  if(!navigator.bluetooth){midiSay('Bluetooth MIDI is not available here: '+MIDI_HELP);return}
  try{
    const dev=await navigator.bluetooth.requestDevice({filters:[{services:[BLE_MIDI_SERVICE]}]});
    // connected already: a second listener would play every note twice
    if(midi.devices.has('ble:'+dev.id)){midiSay((dev.name||'device')+' is already connected');return}
    midiSay('connecting '+(dev.name||'device')+'…');
    const server=await dev.gatt.connect(),svc=await server.getPrimaryService(BLE_MIDI_SERVICE),ch=await svc.getCharacteristic(BLE_MIDI_CHAR);
    ch.addEventListener('characteristicvaluechanged',e=>bleMidi(new Uint8Array(e.target.value.buffer,e.target.value.byteOffset,e.target.value.byteLength)));
    await ch.startNotifications();
    midi.devices.set('ble:'+dev.id,dev.name||'Bluetooth MIDI');
    dev.addEventListener('gattserverdisconnected',()=>{midi.devices.delete('ble:'+dev.id);midiPanic();midiSay((dev.name||'device')+' disconnected')});
    midiSay((dev.name||'device')+' connected: it plays the selected synth');
  }catch(e){midiSay(e.name==='NotFoundError'?'no device chosen':e.name==='SecurityError'&&navigator.userActivation&&!navigator.userActivation.isActive?'tap again to choose a Bluetooth keyboard':e.name==='SecurityError'||e.name==='NotAllowedError'?'Bluetooth is blocked here: '+MIDI_HELP:'Bluetooth failed: '+e.message)}
}
let midiAccess=null;
async function connectMidi(){
  if(!navigator.requestMIDIAccess){midiSay('Web MIDI is not available here: '+MIDI_HELP);return}
  try{
    midiAccess=midiAccess||await navigator.requestMIDIAccess();
    const hook=()=>{for(const k of [...midi.devices.keys()])if(k.startsWith('midi:'))midi.devices.delete(k);
      for(const inp of midiAccess.inputs.values()){if(inp.state!=='connected')continue;midi.devices.set('midi:'+inp.id,inp.name||'MIDI input');
        inp.onmidimessage=e=>{const d=e.data;if(d.length)midiMessage(d[0],d[1]||0,d[2]||0)}}midiSync()};
    midiAccess.onstatechange=hook;hook();
    const n=[...midi.devices.keys()].filter(k=>k.startsWith('midi:')).length;
    midiSay(n?`${n} input${n>1?'s':''} listening: they play the selected synth`:'no MIDI inputs yet: plug one in, or connect a Bluetooth one, and they appear');
  }catch(e){midiSay('MIDI is blocked here: '+MIDI_HELP)}
}
// One button to connect: a phone's keyboard is a Bluetooth one; on a computer, what is plugged in, else Bluetooth.
async function connectKeyboard(){
  initAudio();
  if(navigator.bluetooth&&matchMedia('(pointer:coarse)').matches){connectBluetooth();return}
  await connectMidi();
  if(navigator.bluetooth&&![...midi.devices.keys()].some(k=>k.startsWith('midi:')))connectBluetooth();
}
// The row in a part's sheet or the panel: the two ways in, what is connected, and what it plays.
let midiRows=[];
function midiSync(){for(const f of midiRows)f()}
function midiRow(f,owner){
  const st=el('span',{className:'midi-status'});
  const sync=()=>{const names=[...midi.devices.values()];
    st.textContent=names.length?`${names.join(', ')} → ${midi.rec?`recording onto plate ${midi.rec.c.id}`:owner&&(midi.target===owner||sel&&sel.o===owner)?'this part':'the selected synth'}`:midi.msg||(owner?'play this part from a keyboard':'play the selected synth from a keyboard')};
  midiRows=midiRows.filter(s=>s.el?.isConnected);sync.el=st;midiRows.push(sync);
  f.append(el('div',{className:'midi-row'},el('button',{textContent:'Bluetooth MIDI',onclick:()=>{initAudio();if(owner)midi.target=owner;connectBluetooth()}}),
    el('button',{textContent:'MIDI in',onclick:()=>{initAudio();if(owner)midi.target=owner;connectMidi()}}),st));sync();
}
// The panel's section: connect, and record onto the selected plate.
function midiSection(f){
  midiRow(f);
  const b=el('button',{onclick:recToggle}),sync=()=>{b.textContent=midi.rec?'stop recording':'record onto the plate';b.ariaPressed=String(!!midi.rec)};
  sync.el=b;midiRows.push(sync);sync();f.append(b);
}
