/* ---------- phone: shelf, glyph, bezel ---------- */
const SHELF_H=76,INTERACTION_H=36;
let safeTop=0,safeBot=0;
function onResize(){const s=getComputedStyle($('#safe'));safeTop=parseFloat(s.paddingTop)||0;safeBot=parseFloat(s.paddingBottom)||0}
const shelfTop=()=>CH-SHELF_H-INTERACTION_H-safeBot;
const stage=()=>({l:0,r:CW,t:safeTop,b:shelfTop()});
const offStage=s=>s.y>=shelfTop();
function drawChrome(){const b=activeBezel();if(b)drawBezel(b);if(ui.carry&&ui.carry.ghost)drawGhost(ui.carry.ghost);drawShelf();drawGlyph();syncEditor();drawInteraction()}
const panelShown=()=>!$('#dev').hidden,inspectorLive=panelShown;
function togglePanel(open=$('#dev').hidden,mode=''){
  $('#dev').dataset.editor=mode;$('#dev').hidden=$('#scrim').hidden=!open;
  if(open){syncRows();buildInspector()}
}
function placeRoll(c,f){
  const phone=$('#dev').dataset.editor==='notes';
  rollSection(c,{strip:f,tools:f,row:phone?32:11,label:phone?44:30,minCell:phone?28:6});
  if(!phone)return;
  roll.box.style.maxHeight='36vh';roll.box.style.overflow='auto';
  const nav=el('div',{className:'roll-nav'}),box=roll.box;
  for(const [label,dx,dy]of [['← time',-1,0],['time →',1,0],['↑ pitch',0,-1],['pitch ↓',0,1]])
    nav.append(el('button',{textContent:label,onclick:()=>box.scrollBy({left:dx*box.clientWidth*.75,top:dy*128,behavior:'smooth'})}));
  f.prepend(box,nav);
}
const editorMode=()=>sel?.type==='cog'&&sel.o.track?'notes':sel?.o.voice?'patch':'';
let editorOwner=null,editorOpenedAt=-Infinity;
function syncEditor(){
  const mode=editorMode(),button=$('#edit-part');
  button.hidden=!mode||panelShown()||!!ui.carry;
  const label=mode==='notes'?'♫ notes':'♫ patch';if(button.textContent!==label)button.textContent=label;
  if(!panelShown()||!$('#dev').dataset.editor)return;
  if(!mode){togglePanel(false);return}
  if(editorOwner!==sel.o){editorOwner=sel.o;togglePanel(true,mode)}
  const title=$('#editor-title'),labelled=`${sel.type==='cog'?'disc':sel.type} ${sel.o.id} · ${mode}`;
  if(title.textContent!==labelled)title.textContent=labelled;
}
const GESTURE_READOUT={
  bezel:(b,i)=>{const p=b.sectors[i];return p.action?`${p.label}: copy link`:`${p.label}: ${p.format(p.get())}`},
  carry:kind=>`place part: ${kind}`,armed:kind=>`place part: ${kind}`,
  glyph:()=>panelShown()?'controls: open':`playback: ${clock.paused?'paused':'playing'}`,
  arm:a=>`arm length: ${Math.round(a.len)} px · angle: ${Math.round(wrap(a.a)*180/Math.PI)}°`,
  pad:(o,s,rim)=>rim?`pad radius: ${settingText(o.r)} px`:`pad voice: ${o.voice.kind}${o.voice.kind==='modal'?' · '+noteName(voiceMidi(o.voice,o.voice.note)):''}`,
  span:(type,o,s,end)=>o.f200?`${o.kind} note: ${noteName(Math.round(stringMidi(o)))} · ${Math.round(stringLen(o))} px`:`reader ${end?'length':'mode'}: ${end?Math.round(stringLen(o))+' px':readerMode(o)+' · '+o.voice.kind}`,
  belt:b=>`belt crossed: ${onOff(b.cross)}`,
  cog:c=>`${c.track?'disc':'cog'} position: ${Math.round(c.x)}, ${Math.round(c.y)}`,
  notes:(c,s)=>{const cell=cellAt(c,screenToWorld(gesture?.last||s));return cell?`disc note: ${noteName(ringNote(c.track,cell.ring))}`:`disc notes: ${c.track.dots.length} pins`},
  pan:()=>`view: ${Math.round(S.zoom*100)}% · ${Math.round(view.x)}, ${Math.round(view.y)}`,
  group:()=>`move parts: ${group.length}`,marquee:()=>`select parts: ${group.length}`,
};
let interactionRead=null,interactionUntil=0;
function showInteraction(read,held=false){interactionRead=read;interactionUntil=held?Infinity:performance.now()+1800;drawInteraction()}
function drawInteraction(){
  const output=$('#interaction');output.hidden=!interactionRead||performance.now()>interactionUntil;
  if(output.hidden)return;const text=interactionRead();if(output.textContent!==text)output.textContent=text;
}
function controlReadout(target){
  const control=target.closest('input,select,textarea,button');if(!control)return null;
  const row=control.closest('[data-k]'),label=row?.dataset.k||control.ariaLabel||control.textContent;
  return ()=>`${label||'json'}: ${control.matches('select')?control.selectedOptions[0]?.textContent:control.role==='switch'?onOff(control.ariaChecked==='true'):control.matches('input')?control.value:control.matches('textarea')?'edit':'pressed'}`;
}
function rollReadout(e){
  if(!roll||(e.target!==roll.cv&&e.target!==roll.keys))return null;
  const c=roll.c,cell=e.target===roll.keys?{ring:c.track.rings-1-Math.floor((e.clientY-roll.keys.getBoundingClientRect().top)/roll.row)}:rollCell(e);
  if(!cell||cell.ring<0||cell.ring>=c.track.rings)return null;
  const label=noteName(ringNote(c.track,cell.ring));
  return ()=>e.target===roll.keys?`audition note: ${label}`:`disc note: ${label} · ${onOff(dotAt(c.track,cell.ring,cell.a)>=0)}`;
}
function mount(){
  for(const[kind,describe]of Object.entries(GESTURE_READOUT)){
    const make=G[kind];G[kind]=(...args)=>{const g=make(...args);g.readout=()=>describe(...args);return g};
  }
  cv.addEventListener('pointerdown',()=>showInteraction(gesture?.g.readout||(()=>`view zoom: ${Math.round(S.zoom*100)}%`),true));
  document.addEventListener('pointerdown',e=>{if(e.target===cv)return;const read=rollReadout(e)||controlReadout(e.target);if(read)showInteraction(read,true)});
  document.addEventListener('pointermove',e=>{if(e.buttons){const read=rollReadout(e);if(read)showInteraction(read,true)}});
  for(const type of ['input','change','click'])document.addEventListener(type,e=>{const read=controlReadout(e.target);if(read)showInteraction(read)});
  for(const type of ['pointerup','pointercancel'])addEventListener(type,()=>{if(interactionRead){interactionUntil=performance.now()+1800;drawInteraction()}});
  buildPanel(btn=>btn('close',()=>togglePanel(false)));
  // Chrome can retarget the opening tap's click to the newly exposed scrim.
  $('#scrim').onclick=()=>{if(performance.now()-editorOpenedAt>200)togglePanel(false)};
  $('#editor-close').onclick=()=>togglePanel(false);
  $('#editor-undo').onclick=()=>edits.undo();$('#editor-redo').onclick=()=>edits.redo();
  const openEditor=()=>{const mode=editorMode();if(panelShown()||!mode)return;editorOwner=sel.o;editorOpenedAt=performance.now();togglePanel(true,mode);syncEditor()};
  $('#edit-part').onpointerup=$('#edit-part').onclick=openEditor;
}
// The shelf never changes, so it is painted once per size and blitted.
let shelfArt=null;
function drawShelf(){
  const top=shelfTop(),h=CH-top;
  if(!shelfArt||shelfArt.w!==CW||shelfArt.h!==h||shelfArt.dpr!==dpr){
    const art=document.createElement('canvas');art.width=Math.round(CW*dpr);art.height=Math.round(h*dpr);
    const main=cx;cx=art.getContext('2d');paintShelf(h);cx=main;shelfArt={w:CW,h,dpr,art};
  }
  cx.setTransform(1,0,0,1,0,0);cx.globalAlpha=ui.scrap?.35:1;cx.drawImage(shelfArt.art,0,Math.round(top*dpr));cx.globalAlpha=1;
  cx.setTransform(dpr,0,0,dpr,0,0);
  if(ui.scrap){cx.fillStyle=BAD;cx.globalAlpha=.2+.08*Math.sin(performance.now()/90);cx.fillRect(0,top,CW,h);cx.globalAlpha=1}
  const sw=CW/PARTS.length;
  PARTS.forEach((kind,i)=>{if(ui.armed===kind){cx.strokeStyle=HOT;cx.lineWidth=1;cx.strokeRect(sw*i+2,top+2,sw-4,h-4)}});
}
function paintShelf(h){
  const sw=CW/PARTS.length;
  cx.setTransform(dpr,0,0,dpr,0,0);
  cx.fillStyle=BG;cx.fillRect(0,0,CW,h);cx.fillStyle=FAINT;cx.fillRect(0,0,CW,1);
  PARTS.forEach((kind,i)=>{
    const x=sw*(i+.5),y=SHELF_H/2,s=ICON_SCALE[kind]||.8;
    cx.save();cx.setTransform(dpr*s,0,0,dpr*s,dpr*x,dpr*y);PX=1/s;drawPartIcon(kind);cx.restore();
  });
}
const glyphAt=()=>({x:CW-32,y:safeTop+32});
function drawGlyph(){
  const {x,y}=glyphAt(),bpm=S.bpm||Math.abs(W.cogs.map(motorRpm).find(Boolean)||0),a=clock.view*Math.PI*bpm/60;
  cx.save();cx.translate(x,y);cx.globalAlpha=clock.paused?.45:1;
  cx.strokeStyle=DIM;cx.lineWidth=1;circle(0,0,13);
  cx.rotate(a);cx.strokeStyle=FG;line(-13,0,13,0);cx.fillStyle=FG;circle(-13,0,2,true);circle(13,0,2,true);
  cx.restore();
}
function chromeHit(s){
  if(s.y>=shelfTop())return ()=>G.carry(PARTS[clamp(Math.floor(s.x/(CW/PARTS.length)),0,PARTS.length-1)]);
  const gl=glyphAt();if(Math.hypot(s.x-gl.x,s.y-gl.y)<26)return ()=>G.glyph();
  const b=activeBezel(),i=b?bezelHit(b,s):-1;return i>=0?()=>G.bezel(b,i,s):null;
}
// Keep a new bezel on screen: slide the camera, never zoom.
function reveal(){
  const b=activeBezel();if(!b)return;
  const m=b.R+BEZEL_W+8,dx=b.cx<m?b.cx-m:b.cx>CW-m?b.cx-(CW-m):0,dy=b.cy<safeTop+m?b.cy-safeTop-m:b.cy>shelfTop()-m?b.cy-(shelfTop()-m):0;
  if(dx||dy)fly(view.x+dx/S.zoom,view.y+dy/S.zoom,S.zoom);
}
Object.assign(G,{
  // The press that opens the panel lifts over it, and the browser turns that lift into a click on whatever
  // now lies under the finger; panel and scrim ignore input until the lift has passed, whichever event reports it.
  glyph(){
    const covers=[$('#dev'),$('#scrim')],listen=on=>{for(const t of ['pointerup','pointercancel','touchend','touchcancel'])(on?addEventListener:removeEventListener)(t,lift,true)};
    const thaw=()=>{listen(false);clearTimeout(ui.thaw);for(const e of covers)e.style.pointerEvents=''};
    const lift=()=>{listen(false);clearTimeout(ui.thaw);ui.thaw=setTimeout(thaw,120)};
    const timer=setTimeout(()=>{togglePanel(true);for(const e of covers)e.style.pointerEvents='none';listen(true);clearTimeout(ui.thaw);ui.thaw=setTimeout(thaw,1500);buzz(15)},600);
    return {tap(){clearTimeout(timer);setPaused(!clock.paused)},up(){clearTimeout(timer)},cancel(){clearTimeout(timer)}};
  },
  bezel(b,i,s0){
    const p=b.sectors[i],v0=p.action?0:p.get(),n0=p.options||p.action?0:norm(p),steps=stepsOf(p),per=Math.max(6,DRAG_PX/steps);ui.sector=i;
    const done=()=>{ui.sector=null;changed(true)};
    return {o:b.owner,
      move(s){
        if(p.action||p.toggle)return;
        const dy=s0.y-s.y,was=p.get();
        if(p.options)stepChoice(p,Math.round(dy/OPTION_PX),v0);
        else if(steps)p.set(clamp(v0+Math.round(dy/per)*p.step,p.min,p.max));
        else setNorm(p,n0+dy/DRAG_PX);
        if(p.get()!==was){if(p.options||steps)buzz(3);audition(b.owner,!!(p.options||steps))}
      },
      tap(){if(p.action)p.action();else{if(p.options){stepChoice(p,1);buzz(4)}audition(b.owner,true)}done()},
      up:done,cancel:done,
    };
  },
});

// A bezel is a ring of sectors around its owner. Each sector is one parameter: drag up or down from it to turn it.
const ICON={
  fork:'M9 3v8a3 3 0 0 0 6 0V3M12 14v7M9.5 21h5',bell:'M6 17c0-7 2.5-11 6-11s6 4 6 11zM4 17h16M11 20h2',bend:'M3 18C10 18 12 6 21 6',
  bright:'M12 8a4 4 0 1 1-.01 0M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M19 5l-2 2M7 17l-2 2',hard:'M12 3l7 9-7 9-7-9z',
  decay:'M3 5h3l13 14h2',pdecay:'M3 5h1l5 14h12',damp:'M2 12c1.5-7 3-7 4.5 0s3 5 4.5 0 3-3 4.5 0 3 2 4.5 0',level:'M3 19L21 5v14z',
  vel:'M3 12h11M10 8l4 4-4 4M19 4v16',sweep:'M3 4c1 11 7 15 18 15',tone:'M2 12c2.5-8 5-8 7.5 0s5 8 7.5 0 3-4 5-4',cutoff:'M2 8h11c4 0 5 9 9 11',
  scale:'M3 20h4v-4h4v-4h4v-4h4V4',div:'M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0M12 3v18M3 12h18M5.6 5.6l12.8 12.8M18.4 5.6L5.6 18.4',
  rings:'M3 12a9 9 0 1 0 18 0a9 9 0 1 0-18 0M7 12a5 5 0 1 0 10 0a5 5 0 1 0-10 0M11 12a1 1 0 1 0 2 0a1 1 0 1 0-2 0',
  wet:'M12 3c4 6 6 9 6 12a6 6 0 0 1-12 0c0-3 2-6 6-12z',size:'M3 20V10l9-6 9 6v10zM3 20h18',tail:'M3 20h18M4 20V6M8 20v-9M12 20v-6M16 20v-4M20 20v-2',
  air:'M3 9h12a3 3 0 1 0-3-3M3 15h15a3 3 0 1 1-3 3',shimmer:'M12 2l2 7 7 3-7 3-2 7-2-7-7-3 7-3z',freeze:'M12 2v20M3.3 7l17.4 10M3.3 17L20.7 7',
  tempo:'M8 21l3-18h2l3 18zM12 16l6-10',beats:'M5 12h.01M10 12h.01M15 12h.01M20 12h.01',comb:'M3 5h18M5 5v14M9 5v12M13 5v10M17 5v8M21 5v6',
  odds:'M12 3a9 9 0 1 0 .01 0M12 3v9h9',string:'M3 19L21 5M3 19h.01M21 5h.01',quant:'M4 20V14M9 20V9M14 20V12M19 20V4',
  seal:'M12 3a9 9 0 1 0 .01 0M12 7l1.5 3.2 3.5.4-2.6 2.4.7 3.5-3.1-1.8-3.1 1.8.7-3.5L7 10.6l3.5-.4z'};
const icons=new Map();const iconPath=k=>{let p=icons.get(k);if(!p){p=new Path2D(ICON[k]);icons.set(k,p)}return p};
const VOICE_ICON={note:'fork',octave:'scale',tune:'fork',body:'bell',bend:'bend',bright:'bright',hard:'hard',decay:'decay',pdecay:'pdecay',damp:'damp',
  sweep:'sweep',tone:'tone',cutoff:'cutoff',level:'level',velSense:'vel'};
const settingText=x=>typeof x==='number'?String(+x.toFixed(3)):String(x);
const onOff=x=>x?'on':'off';
const numSector=(icon,label,o,k,min,max,step,after)=>({icon,label,min,max,step,format:['note','base','combBase'].includes(k)?noteName:settingText,
  log:min>0&&max/min>=20,get:()=>o[k],set:x=>{o[k]=x;after&&after()}});
const defSector=(icon,o,def,after)=>numSector(icon,def[1],o,def[0],def[2],def[3],def[4],after);
const choiceSector=(icon,label,options,get,set,format=settingText)=>({icon,label,options,get,set,format,toggle:format===onOff});
function voiceSectors(owner,withNote){
  const v=owner.voice,out=[choiceSector('kind','voice',Object.keys(VOICES),()=>owner.voice.kind,k=>{owner.voice=makeVoice(k,{level:v.level,velSense:v.velSense})})];
  for(const def of [...VOICE_DEFS[v.kind],...COMMON_DEFS]){
    if(def[0]==='note'&&!withNote)continue;
    out.push(Array.isArray(def[2])?choiceSector(VOICE_ICON[def[0]],def[1],def[2],()=>v[def[0]],x=>{v[def[0]]=x}):defSector(VOICE_ICON[def[0]],v,def));
  }
  return out;
}
function readerSectors(o){
  const out=[...voiceSectors(o,false),choiceSector('comb','reader mode',READER_MODES,()=>readerMode(o),mode=>setReaderMode(o,mode))];
  if(o.tines)out.push(numSector('comb','comb tines',o,'tines',1,36,1),numSector('fork','comb note',o,'combBase',12,108,1),
    choiceSector('scale','comb scale',Object.keys(SCALES),()=>o.combScale,x=>{o.combScale=x}));
  return out;
}
const stringSectors=o=>[choiceSector('string','instrument',['string','bar'],()=>o.kind,x=>{o.kind=x}),numSector('fork','tuning Hz',o,'f200',20,2000,1),
  choiceSector('quant','quantize',[0,1],()=>o.quant,x=>{o.quant=x},onOff),numSector('decay','decay s',o,'decay',0,8,.01),numSector('damp','damping',o,'damp',0,2,.01),
  numSector('hard','hardness',o,'hard',0,1,.01),numSector('level','level',o,'level',0,1.5,.01),numSector('vel','velocity sense',o,'velSense',0,1,.01)];
function trackSectors(c){
  const t=c.track;
  return [choiceSector('scale','scale',Object.keys(SCALES),()=>t.scale,x=>{t.scale=x}),numSector('fork','root note',t,'base',12,108,1),
    numSector('div','steps',t,'div',1,96,1),numSector('rings','rings',t,'rings',1,36,1,()=>{t.dots=t.dots.filter(d=>d.ring<t.rings)})];
}
const dotSectors=d=>[numSector('odds','probability',d,'p',0,1,.01),numSector('level','velocity',d,'v',0,1,.01)];
const ROOM=[['wet','wet'],['size','size'],['tail','decay'],['air','highMult'],['shimmer','shimmer']];
function roomSectors(){
  const out=ROOM.map(([icon,k])=>defSector(icon,SPACE,SPACE_DEFS.find(d=>d[0]===k),roomEdited));
  out.push(choiceSector('freeze','reverb freeze',[0,1],()=>SPACE.freeze,x=>{SPACE.freeze=x;roomEdited()},onOff));
  out.push(defSector('level',S,S_DEFS.find(d=>d[0]==='master'),roomEdited));
  out.push(numSector('tempo','tempo bpm',S,'bpm',0,300,1,()=>changed(true)),numSector('beats','beats/bar',S,'beats',1,16,1,()=>changed(true)));
  out.push({icon:'seal',label:'share',action:share});
  return out;
}
async function share(){
  ui.sealAt=performance.now();
  try{const url=await shareURL();ui.shared=url;await navigator.clipboard.writeText(url);ui.sealOk=true;buzz(20)}
  catch(e){ui.sealOk=false;console.error('cogs: share copy failed:',e.name);if(ui.shared){togglePanel(true);$('#panel textarea').value=ui.shared}}
}
const norm=p=>p.log?Math.log(p.get()/p.min)/Math.log(p.max/p.min):(p.get()-p.min)/(p.max-p.min);
const stepsOf=p=>!p.options&&!p.action&&p.step>=1&&(p.max-p.min)/p.step<=120?(p.max-p.min)/p.step:0;
function setNorm(p,n){n=clamp(n,0,1);const x=p.log?p.min*Math.pow(p.max/p.min,n):p.min+(p.max-p.min)*n;p.set(clamp(+(Math.round(x/p.step)*p.step).toFixed(6),p.min,p.max))}
function stepChoice(p,d,from=p.get()){const n=p.options.length,i=p.options.indexOf(from);p.set(p.options[((i+d)%n+n)%n])}
function activeBezel(){
  if(ui.carry)return null;
  let b=null;const o=sel&&sel.o,ring=(at,size,sectors)=>{const c=toScreen(at);return {cx:c.x,cy:c.y,R:clamp(size+46,92,132),sectors,owner:o}};
  if(!sel){if(ui.room)b={cx:CW/2,cy:(safeTop+shelfTop())/2,R:Math.min(CW,shelfTop()-safeTop)/2-44,sectors:roomSectors(),owner:SPACE}}
  else if(sel.type==='pad')b=ring(o,o.r*S.zoom,voiceSectors(o,true));
  else if(sel.type==='reader')b=ring(mid(o),stringLen(o)*S.zoom/2,readerSectors(o));
  else if(sel.type==='string')b=ring(mid(o),stringLen(o)*S.zoom/2,stringSectors(o));
  else if(sel.type==='dot')b=ring(ui.dotAt,0,dotSectors(o));
  else if(sel.type==='cog'&&o.track){
    const R=Math.max(92,radius(o)*S.zoom+40);
    if(R+BEZEL_W+8<=Math.min(CW,shelfTop()-safeTop)/2){const c=toScreen(o);b={cx:c.x,cy:c.y,R,sectors:trackSectors(o),owner:o}}
  }
  if(!b){ui.bezelOwner=null;return null}
  if(ui.bezelOwner!==b.owner){ui.bezelOwner=b.owner;ui.bezelAt=performance.now()}
  return b;
}
const BEZEL_W=19,DRAG_PX=240,OPTION_PX=32;
function drawBezel(b){
  const n=b.sectors.length,span=TAU/n,k=clamp((performance.now()-ui.bezelAt)/160,0,1),e=1-Math.pow(1-k,4),R=b.R*(.7+.3*e);
  cx.save();cx.globalAlpha=e;
  b.sectors.forEach((p,i)=>{
    const a0=-Math.PI/2+i*span+.025,a1=a0+span-.05,am=(a0+a1)/2,active=ui.sector===i&&ui.bezelOwner===b.owner;
    cx.beginPath();cx.arc(b.cx,b.cy,R+BEZEL_W,a0,a1);cx.arc(b.cx,b.cy,R-BEZEL_W,a1,a0,true);cx.closePath();
    cx.fillStyle=active?PAD:BG;cx.fill();cx.strokeStyle=FAINT;cx.lineWidth=1;cx.stroke();
    const rv=R+BEZEL_W-5;let ink=p.toggle?(p.get()?HOT:DIM):active?HOT:FG;
    if(p.action){const age=performance.now()-(ui.sealAt||-1e9);if(age<900)ink=ui.sealOk?HOT:BAD}
    else if(p.options){
      const m=p.options.length,cur=p.options.indexOf(p.get());
      for(let j=0;j<m;j++){const a=a0+.12+(a1-a0-.24)*(m===1?.5:j/(m-1));cx.fillStyle=j===cur?HOT:DIM;circle(b.cx+Math.cos(a)*rv,b.cy+Math.sin(a)*rv,j===cur?2.6:1.5,true)}
    }else{
      const v=clamp(norm(p),0,1),ae=a0+.08+(a1-a0-.16)*v;
      cx.lineCap='round';cx.lineWidth=1;cx.strokeStyle=DIM;cx.beginPath();cx.arc(b.cx,b.cy,rv,a0+.08,a1-.08);cx.stroke();
      cx.lineWidth=2;cx.strokeStyle=FG;cx.beginPath();cx.arc(b.cx,b.cy,rv,a0+.08,Math.max(a0+.081,ae));cx.stroke();cx.lineCap='butt';
      cx.fillStyle=HOT;circle(b.cx+Math.cos(ae)*rv,b.cy+Math.sin(ae)*rv,2.6,true);
    }
    const ix=b.cx+Math.cos(am)*(R-4),iy=b.cy+Math.sin(am)*(R-4);
    if(p.icon==='kind'){cx.strokeStyle=ink;cx.lineWidth=1.2;circle(ix,iy,8);emblem(p.get(),ix,iy,8)}
    else{cx.save();cx.translate(ix-9,iy-9);cx.scale(.75,.75);cx.strokeStyle=ink;cx.lineWidth=1.8;cx.lineJoin=cx.lineCap='round';cx.stroke(iconPath(p.icon));cx.restore()}
  });
  cx.restore();
}
function bezelHit(b,s){
  const d=Math.hypot(s.x-b.cx,s.y-b.cy);if(Math.abs(d-b.R)>BEZEL_W+6)return -1;
  return Math.floor(wrap(Math.atan2(s.y-b.cy,s.x-b.cx)+Math.PI/2)/(TAU/b.sectors.length));
}

const PROBE={shelfTop,SHELF:PARTS,glyphAt,activeBezel};
