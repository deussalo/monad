/* ---------- MIDI in: a keyboard plays the selected reader (or pad) ---------- */
// Two ways in. Bluetooth: Web Bluetooth straight to a BLE-MIDI keyboard (a Korg microKEY Air, or anything that speaks
// the BLE-MIDI service), no pairing app needed; Chrome on Android or desktop. MIDI: Web MIDI, for USB keyboards and
// Bluetooth ones the system already connected. The notes go to the part last selected that has a voice; its synth,
// octave and all, transposes them as it does every note. Sustain (CC64) holds note-offs; the mod wheel (CC1) turns
// a synth's macro 1 unless a cam is turning it.
const BLE_MIDI_SERVICE='03b80e5a-ede8-4b33-a751-6ce34ec4c700',BLE_MIDI_CHAR='7772e5db-3868-4112-a1a9-f2669d106bf3';
const midi={target:null,devices:new Map(),held:new Map(),sustain:false,sustained:new Set(),msg:'',last:null,mod:null,modAt:0,count:0};
const midiSay=t=>{midi.msg=t;midiSync();if(typeof showInteraction==='function')showInteraction(()=>'MIDI: '+t)};
// the part that plays: the selection when it has a voice, otherwise the last one that did
function midiOwner(){const o=sel&&sel.o;if(o&&o.voice&&(sel.type==='reader'||sel.type==='pad'))midi.target=o;
  const t=midi.target;return t&&(W.readers.includes(t)||W.pads.includes(t))?t:null}
function midiNoteOn(note,vel){
  const o=midiOwner();if(!o){midiSay('select a reader to play it');return}
  if(!ac)initAudio();if(!ac||engineState!=='ready')return;
  midiNoteOff(note,true);
  const v=o.voice,vx=vel/127,amp=v.level*(1-v.velSense+v.velSense*Math.pow(vx,S.velCurve)),key=o.id*128+note;
  playVoice(v,note,ac.currentTime+.003,amp,0,key,vx,30);
  midi.held.set(note,{key,o});o.flash=clock.view;o.lastMidi=voiceMidi(v,note);midi.last=note;midi.count++;
}
function midiNoteOff(note,now){
  const h=midi.held.get(note);if(!h)return;
  if(midi.sustain&&!now){midi.sustained.add(note);return}
  midi.held.delete(note);midi.sustained.delete(note);if(ac)damp(h.key,ac.currentTime);
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
    midiSay('connecting '+(dev.name||'device')+'…');
    const server=await dev.gatt.connect(),svc=await server.getPrimaryService(BLE_MIDI_SERVICE),ch=await svc.getCharacteristic(BLE_MIDI_CHAR);
    ch.addEventListener('characteristicvaluechanged',e=>bleMidi(new Uint8Array(e.target.value.buffer,e.target.value.byteOffset,e.target.value.byteLength)));
    await ch.startNotifications();
    midi.devices.set('ble:'+dev.id,dev.name||'Bluetooth MIDI');
    dev.addEventListener('gattserverdisconnected',()=>{midi.devices.delete('ble:'+dev.id);midiPanic();midiSay((dev.name||'device')+' disconnected')});
    midiSay((dev.name||'device')+' connected: it plays the selected reader');
  }catch(e){midiSay(e.name==='NotFoundError'?'no device chosen':e.name==='SecurityError'||e.name==='NotAllowedError'?'Bluetooth is blocked here: '+MIDI_HELP:'Bluetooth failed: '+e.message)}
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
    midiSay(n?`${n} input${n>1?'s':''} listening: they play the selected reader`:'no MIDI inputs yet: plug one in, or connect a Bluetooth one, and they appear');
  }catch(e){midiSay('MIDI is blocked here: '+MIDI_HELP)}
}
// The row in a part's sheet: the two ways in, what is connected, and what it plays.
let midiRows=[];
function midiSync(){for(const f of midiRows)f()}
function midiRow(f,owner){
  const st=el('span',{className:'midi-status'});
  const sync=()=>{const names=[...midi.devices.values()];
    st.textContent=names.length?`${names.join(', ')} → ${midi.target===owner||sel&&sel.o===owner?'this part':'the selected part'}`:midi.msg||'play this part from a keyboard'};
  midiRows=midiRows.filter(s=>s.el?.isConnected);sync.el=st;midiRows.push(sync);
  f.append(el('div',{className:'midi-row'},el('button',{textContent:'Bluetooth MIDI',onclick:()=>{initAudio();midi.target=owner;connectBluetooth()}}),
    el('button',{textContent:'MIDI in',onclick:()=>{initAudio();midi.target=owner;connectMidi()}}),st));sync();
}
