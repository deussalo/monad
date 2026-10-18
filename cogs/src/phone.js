/* ---------- phone: shelf, glyph, bezel ---------- */
const SHELF_H=76,INTERACTION_H=36;
let safeTop=0,safeBot=0;
function onResize(){const s=getComputedStyle($('#safe'));safeTop=parseFloat(s.paddingTop)||0;safeBot=parseFloat(s.paddingBottom)||0}
const shelfTop=()=>CH-SHELF_H-INTERACTION_H-safeBot;
const stage=()=>({l:0,r:CW,t:safeTop,b:shelfTop()});
const offStage=s=>s.y>=shelfTop();
function drawChrome(){const b=activeBezel();if(b)drawBezel(b);if(ui.carry&&ui.carry.ghost)drawGhost(ui.carry.ghost);drawShelf();drawGlyph();syncEditor();drawInteraction()}
const panelShown=()=>!$('#dev').hidden,inspectorLive=panelShown;
const worldCovered=()=>panelShown()&&!sheetMin()&&!!$("#dev").dataset.editor;
function togglePanel(open=$('#dev').hidden,mode=''){
  $('#dev').dataset.editor=mode;$('#dev').hidden=$('#scrim').hidden=!open;setSheetMin(false);
  if(open){syncRows();buildInspector()}
}
// Tucked, the sheet keeps only its top strip on screen: the machine above takes touches and keeps playing,
// and selecting another disc or pad retitles the strip to that part.
const sheetMin=()=>$('#dev').classList.contains('min');
function setSheetMin(min){
  const dev=$('#dev');dev.classList.toggle('min',min);document.body.classList.toggle('sheet-min',min&&!dev.hidden);
  dev.style.transform='';if(min){dev.scrollTop=0;document.documentElement.style.setProperty('--peek',$('#sheet-top').offsetHeight+safeBot+'px')}
  $('#scrim').hidden=dev.hidden||min;
}
function sheetDrag(){
  const dev=$('#dev'),top=$('#sheet-top');let g=null;
  top.onpointerdown=e=>{
    if(e.target.closest('button,select'))return;
    top.setPointerCapture(e.pointerId);const r=dev.getBoundingClientRect();
    g={y0:e.clientY,t0:performance.now(),base:sheetMin()?r.height-top.offsetHeight-safeBot:0,h:r.height,dy:0,v:0,lt:performance.now(),ly:e.clientY};
    dev.classList.add('dragging');
  };
  top.onpointermove=e=>{
    if(!g)return;const now=performance.now();g.dy=e.clientY-g.y0;g.v=(e.clientY-g.ly)/Math.max(1,now-g.lt);g.ly=e.clientY;g.lt=now;
    if(Math.abs(g.dy)>6)dev.style.transform=`translateY(${clamp(g.base+g.dy,0,g.h)}px)`;
  };
  top.onpointerup=top.onpointercancel=e=>{
    if(!g)return;const {dy,v,base,h}=g,min=sheetMin();g=null;dev.classList.remove('dragging');
    if(e.type==='pointercancel'){setSheetMin(min);return}
    if(Math.abs(dy)<=6){setSheetMin(!min);buzz(6);return}
    const fling=dy+v*180;
    if(min){if(fling<-30)setSheetMin(false);else if(fling>40)togglePanel(false);else setSheetMin(true)}
    else setSheetMin(base+fling>h*.3);
    buzz(6);
  };
}
function placeRoll(c,f){
  const phone=$('#dev').dataset.editor==='notes';
  rollSection(c,{strip:f,tools:f,row:phone?32:11,label:phone?28:30,minCell:phone?28:6});
  if(!phone)return;
  roll.box.style.maxHeight='calc(86vh - 190px)';roll.box.style.overflow='auto';
  // The selection's actions, always in place so the strip never jumps: duplicate, copy, paste, delete.
  const c0=c,act=(icon,label,fn)=>{const b=el('button',{ariaLabel:label,innerHTML:rx(icon,20),onclick:()=>{fn();buzz(6)}});b.dataset.roll=label;return b};
  const bar=el('div',{className:'roll-sel'},act('CardStackPlus','duplicate',()=>dupSel(c0)),act('Copy','copy',()=>copySel(c0)),
    act('Clipboard','paste',()=>pasteClip(c0)),act('Trash','delete notes',()=>deleteSel(c0.track)));
  // The velocity lane stays tucked away until this toggle shows it; the choice holds for every disc.
  const vel=act('Velocity','velocity',()=>{setRollVel(!rollVel);vel.ariaPressed=String(rollVel)});vel.ariaPressed=String(rollVel);bar.prepend(vel);
  // The snap picker rides the bar, left of the actions: just the division, tap it to choose another.
  const snap=roll.snapRow.querySelector('select');snap.ariaLabel='snap';snap.className='roll-snap';bar.prepend(snap);roll.snapRow.remove();
  // The bar takes the sheet head's row, in place of the title, beside undo, redo and close: one row of tools.
  const head=$('#editor-head');head.querySelector('.roll-sel')?.remove();head.insertBefore(bar,$('#editor-undo'));
  f.prepend(roll.box);
  // The disc's tuning as compact chips under the strip, not full-width form rows; each number gets − and + beside it.
  const fields=el('div',{className:'disc-fields'},...['root','octave','range','scale','steps'].map(k=>f.querySelector(`:scope>.row[data-k="${k}"]`)).filter(Boolean));
  roll.box.after(fields);
  for(const sr of fields.querySelectorAll('[data-k="steps"],[data-k="octave"],[data-k="range"]')){const num=sr.querySelector('input[type=number]'),bump=d=>()=>{num.value=+num.value+d;num.dispatchEvent(new Event('input'));buzz(4)},k=sr.dataset.k;
    const box=el('div',{className:'stepper'},el('button',{textContent:'−',ariaLabel:k+' down',onclick:bump(-1)}),num,el('button',{textContent:'+',ariaLabel:k+' up',onclick:bump(1)}));
    sr.querySelector('input[type=range]').remove();sr.append(box)}
  syncRollBar();
}
const PART_BAR_ICON={notes:rx('Piano',22),patch:rx('MixerVertical',22),shape:rx('MixerVertical',22)};
const editorMode=()=>sel?.type==='cog'&&sel.o.track?'notes':sel?.o.voice?'patch':sel?.type==='cam'?'shape':'';
let editorOwner=null,editorOpenedAt=-Infinity;
// Open the selection's sheet (notes, patch or a cam's shape), as the part bar's edit button does.
function openPartEditor(){const mode=editorMode();if(panelShown()&&sheetMin()&&$('#dev').dataset.editor===mode){setSheetMin(false);return}
  if(panelShown()&&$('#dev').dataset.editor===mode&&editorOwner===sel?.o)return;if(!mode)return;editorOwner=sel.o;editorOpenedAt=performance.now();togglePanel(true,mode);syncEditor()}
function syncRollBar(){
  const bar=document.querySelector('.roll-sel');if(bar&&(!roll||$('#dev').dataset.editor!=='notes')){bar.remove();return}if(!bar)return;const some=rollSel.size>0;
  for(const b of bar.querySelectorAll('button')){const off=b.dataset.roll==='velocity'?false:b.dataset.roll==='paste'?!rollClip.length:!some;if(b.disabled!==off)b.disabled=off}
}
function syncEditor(){syncRollBar();
  const mode=editorMode(),button=$('#edit-part'),start=$('#to-start'),busy=(panelShown()&&!sheetMin())||!!ui.carry;
  button.hidden=!mode||busy;$('#delete-part').hidden=!(sel||group.length)||busy;
  $('#copy-parts').hidden=!(group.length||sel&&sel.type!=='dot')||busy;$('#paste-parts').hidden=!partClip||busy;
  {const ct=shelfTop()-102+'px';if($('#clip-bar').style.top!==ct)$('#clip-bar').style.top=ct}
  start.hidden=$('#history').hidden=busy;const top=shelfTop()-54+'px';if(start.style.top!==top)start.style.top=$('#part-bar').style.top=$('#history').style.top=top;
  const noUndo=!edits.past.length,noRedo=!edits.future.length;
  if($('#undo-all').disabled!==noUndo)$('#undo-all').disabled=$('#editor-undo').disabled=noUndo;
  if($('#redo-all').disabled!==noRedo)$('#redo-all').disabled=$('#editor-redo').disabled=noRedo;
  if(button.dataset.mode!==mode){button.dataset.mode=mode;button.ariaLabel=mode==='notes'?'Edit notes':mode==='shape'?'Edit shape':'Edit patch';button.innerHTML=PART_BAR_ICON[mode]}
  if(!panelShown()||!$('#dev').dataset.editor)return;
  if(!mode){togglePanel(false);return}
  if(editorOwner!==sel.o){const tucked=sheetMin();editorOwner=sel.o;togglePanel(true,mode);if(tucked)setSheetMin(true)}
  const title=$('#editor-title'),labelled=`${sel.type==='cog'?'disc':sel.type==='pit'?'sphere':sel.type==='field'?'fx':sel.type} ${sel.o.id} · ${mode}`;
  if(title.textContent!==labelled)title.textContent=labelled;
}
const GESTURE_READOUT={
  bezel:(b,i)=>{const p=b.sectors[i];return p.action?`${p.label}: copy link`:`${p.label}: ${p.format(p.get())}`},
  carry:kind=>`place part: ${kind}`,armed:kind=>`place part: ${kind}`,
  glyph:()=>panelShown()?'controls: open':`playback: ${clock.paused?'paused':'playing'}`,
  arm:a=>`arm length: ${Math.round(a.len)} px · angle: ${Math.round(wrap(a.a)*180/Math.PI)}°`,
  pad:(o,s,rim)=>rim?`pad radius: ${settingText(o.r)} px`:`pad voice: ${kindLabel(o.voice.kind)}${o.voice.kind==='synth'||o.voice.kind==='wave'?' · '+noteName(voiceMidi(o.voice,o.voice.note)):''}`,
  span:(type,o,s,end)=>o.f200?`${o.kind} note: ${noteName(Math.round(stringMidi(o)))} · ${Math.round(stringLen(o))} px`:`${o.tines?'comb':'reader'} ${end?'length':'voice'}: ${end?Math.round(stringLen(o))+' px':kindLabel(o.voice.kind)}`,
  belt:b=>`belt crossed: ${onOff(b.cross)}`,
  cog:c=>`${c.track?'disc':c.motor?'motor':'cog'} position: ${Math.round(c.x)}, ${Math.round(c.y)}`,
  notes:(c,s)=>{const cell=cellAt(c,screenToWorld(gesture?.last||s));return cell?`disc note: ${noteName(ringNote(c.track,cell.ring))}`:`disc notes: ${c.track.dots.length} pins`},
  pan:()=>marquee?`select parts: ${group.length}`:`view: ${Math.round(S.zoom*100)}% · ${Math.round(view.x)}, ${Math.round(view.y)}`,
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
  if(roll&&e.target===roll.lane)return ()=>roll.status||'velocity';
  if(!roll||(e.target!==roll.cv&&e.target!==roll.keys))return null;
  const c=roll.c,cell=e.target===roll.keys?{ring:trackRings(c.track)-1-Math.floor((e.clientY-roll.keys.getBoundingClientRect().top)/roll.row)}:rollCell(e);
  if(!cell||cell.ring<0||cell.ring>=trackRings(c.track))return null;
  const label=noteName(ringNote(c.track,cell.ring));
  return ()=>e.target===roll.keys?`audition note: ${label}`:roll?.status||`disc note: ${label} · ${onOff(dotAt(c.track,cell.ring,cell.a)>=0)}`;
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
  $('#delete-part').innerHTML=rx('Trash',22);$('#to-start').innerHTML=rx('TrackPrevious',22);
  $('#editor-undo').innerHTML=rx('Reset',20);$('#editor-redo').innerHTML=rx('Reset',20,true);$('#editor-close').innerHTML=rx('Cross2',20);
  $('#copy-parts').innerHTML=rx('Copy',22);$('#paste-parts').innerHTML=rx('Clipboard',22);
  $('#copy-parts').onclick=()=>{if(copyParts()){buzz(8);showInteraction(()=>`copy parts: ${group.length||1}`)}};
  // A finger has no hover: paste lands in the middle of the open stage, selected, ready to drag into place.
  $('#paste-parts').onclick=()=>{const h=ui.hover;ui.hover={x:CW/2,y:(safeTop+shelfTop())/2};pasteParts();ui.hover=h;buzz(8)};
  $('#undo-all').innerHTML=rx('Reset',22);$('#redo-all').innerHTML=rx('Reset',22,true);
  $('#undo-all').onclick=()=>{edits.undo();buzz(6)};$('#redo-all').onclick=()=>{edits.redo();buzz(6)};
  $('#editor-undo').onclick=()=>edits.undo();$('#editor-redo').onclick=()=>edits.redo();
  sheetDrag();
  const openEditor=openPartEditor;
  $('#edit-part').onpointerup=$('#edit-part').onclick=openEditor;
  $('#delete-part').onclick=()=>removeSel();$('#to-start').onclick=()=>{toStart();buzz(10)};
}
// The shelf is a tray wider than the phone: each part has a slot at least SLOT_MIN wide, and a sideways drag scrolls
// it (with a fling). A drag up off a slot carries that part in. It never changes, so it is painted once per size and
// blitted at the scroll.
const SLOT_MIN=58,FLING_MS=260;
const slotW=()=>Math.max(SLOT_MIN,CW/PARTS.length),shelfMax=()=>Math.max(0,slotW()*PARTS.length-CW);
let shelfX=0,shelfFling=null,shelfArt=null;
const setShelfX=x=>{shelfX=clamp(x,0,shelfMax())};
const slotAt=x=>PARTS[clamp(Math.floor((x+shelfX)/slotW()),0,PARTS.length-1)];
function drawShelf(){
  const top=shelfTop(),h=CH-top,cw=slotW()*PARTS.length;
  if(!shelfArt||shelfArt.cw!==cw||shelfArt.h!==h||shelfArt.dpr!==dpr){
    const art=document.createElement('canvas');art.width=Math.round(cw*dpr);art.height=Math.round(h*dpr);
    const main=cx;cx=art.getContext('2d');paintShelf(h,cw);cx=main;shelfArt={cw,h,dpr,art};
  }
  if(shelfFling){const t=performance.now(),dt=Math.min(50,t-shelfFling.t);shelfFling.t=t;setShelfX(shelfX+shelfFling.v*dt);shelfFling.v*=Math.exp(-dt/FLING_MS);
    if(Math.abs(shelfFling.v)<.02||shelfX<=0||shelfX>=shelfMax())shelfFling=null}
  setShelfX(shelfX);
  cx.setTransform(1,0,0,1,0,0);cx.fillStyle=BG;cx.fillRect(0,Math.round(top*dpr),Math.round(CW*dpr),Math.round(h*dpr));
  cx.globalAlpha=ui.scrap?.35:1;cx.drawImage(shelfArt.art,Math.round(shelfX*dpr),0,Math.round(CW*dpr),Math.round(h*dpr),0,Math.round(top*dpr),Math.round(CW*dpr),Math.round(h*dpr));cx.globalAlpha=1;
  cx.setTransform(dpr,0,0,dpr,0,0);
  cx.fillStyle=FAINT;cx.fillRect(0,top,CW,1);
  // more parts past either edge: the tray fades out there
  for(const[on,x0,x1]of [[shelfX>1,0,28],[shelfX<shelfMax()-1,CW,CW-28]])if(on){const g=cx.createLinearGradient(x0,0,x1,0);g.addColorStop(0,BG);g.addColorStop(1,'rgba(0,0,0,0)');cx.fillStyle=g;cx.fillRect(Math.min(x0,x1),top+1,28,h-1)}
  if(ui.scrap){cx.fillStyle=BAD;cx.globalAlpha=.2+.08*Math.sin(performance.now()/90);cx.fillRect(0,top,CW,h);cx.globalAlpha=1}
  const sw=slotW();
  PARTS.forEach((kind,i)=>{if(ui.armed===kind){cx.strokeStyle=HOT;cx.lineWidth=1;cx.strokeRect(sw*i-shelfX+2,top+2,sw-4,h-4)}});
}
function paintShelf(h,cw){
  const sw=slotW();
  cx.setTransform(dpr,0,0,dpr,0,0);
  cx.fillStyle=BG;cx.fillRect(0,0,cw,h);
  PARTS.forEach((kind,i)=>{
    const x=sw*(i+.5),y=SHELF_H/2,s=ICON_SCALE[kind]||.8;
    cx.save();cx.setTransform(dpr*s,0,0,dpr*s,dpr*x,dpr*y);PX=1/s;drawPartIcon(kind);cx.restore();
  });
}
// A press on the shelf: a flat sideways drag scrolls the tray; anything steeper carries the part under the finger
// in. A scroll that then rises off the tray picks up the part the finger is over as it leaves.
function shelfGesture(s0){
  let kind=slotAt(s0.x);const x0=shelfX;let mode=null,inner=null,last={x:s0.x,t:performance.now()},v=0;shelfFling=null;
  return {o:null,readout:()=>mode==='scroll'?`parts: ${PARTS.indexOf(slotAt(CW/2))+1} of ${PARTS.length}`:`place part: ${kind}`,
    move(s){
      if(!mode){mode=s.y<shelfTop()||2*Math.abs(s.y-s0.y)>=Math.abs(s.x-s0.x)||!shelfMax()?'carry':'scroll';if(mode==='carry')inner=G.carry(kind)}
      else if(mode==='scroll'&&s.y<shelfTop()-8){mode='carry';kind=slotAt(clamp(s.x,0,CW-1));inner=G.carry(kind);buzz(6)}
      if(mode==='carry')return inner.move(s);
      setShelfX(x0-(s.x-s0.x));const t=performance.now();if(t>last.t){v=(last.x-s.x)/(t-last.t);last={x:s.x,t}}
    },
    up(s){if(mode==='carry')return inner.up(s);if(mode==='scroll'&&performance.now()-last.t<80&&Math.abs(v)>.15)shelfFling={v,t:performance.now()}},
    cancel(){inner?.cancel?.()},tap(){}};
}
// A wheel or trackpad over the shelf scrolls it.
function shelfWheel(s,e){if(s.y<shelfTop())return false;setShelfX(shelfX+(Math.abs(e.deltaX)>Math.abs(e.deltaY)?e.deltaX:e.deltaY));return true}
// For the tests: bring a part's slot into view and say where it is.
function shelfSlot(kind){const i=PARTS.indexOf(kind),sw=slotW();if(sw*i<shelfX||sw*(i+1)>shelfX+CW)setShelfX(sw*(i+.5)-CW/2);return [sw*(i+.5)-shelfX,shelfTop()+SHELF_H/2]}
const glyphAt=()=>({x:CW-32,y:shelfTop()-32});
function drawGlyph(){
  const {x,y}=glyphAt(),bpm=S.bpm||Math.abs(W.cogs.map(motorRpm).find(Boolean)||0),a=clock.view*Math.PI*bpm/60;
  cx.save();cx.translate(x,y);cx.globalAlpha=clock.paused?.45:1;
  cx.strokeStyle=DIM;cx.lineWidth=1;circle(0,0,13);
  cx.rotate(a);cx.strokeStyle=FG;line(-13,0,13,0);cx.fillStyle=FG;circle(-13,0,2,true);circle(13,0,2,true);
  cx.restore();
}
function chromeHit(s){
  if(s.y>=shelfTop())return ()=>shelfGesture(s);
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
        if(p.get()!==was&&(p.options||steps))buzz(3);
      },
      tap(){if(p.action)p.action();else if(p.options){stepChoice(p,1);buzz(4)}done()},
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
  air:'M3 9h12a3 3 0 1 0-3-3M3 15h15a3 3 0 1 1-3 3',verb:'M3 18a9 9 0 0 1 18 0M7 18a5 5 0 0 1 10 0M11 18a1 1 0 0 1 2 0',era:'M12 3a9 9 0 1 0 .01 0M12 7v5l3 3',shimmer:'M12 2l2 7 7 3-7 3-2 7-2-7-7-3 7-3z',freeze:'M12 2v20M3.3 7l17.4 10M3.3 17L20.7 7',
  tempo:'M8 21l3-18h2l3 18zM12 16l6-10',beats:'M5 12h.01M10 12h.01M15 12h.01M20 12h.01',comb:'M3 5h18M5 5v14M9 5v12M13 5v10M17 5v8M21 5v6',
  odds:'M12 3a9 9 0 1 0 .01 0M12 3v9h9',string:'M3 19L21 5M3 19h.01M21 5h.01',quant:'M4 20V14M9 20V9M14 20V12M19 20V4',
  seal:'M12 3a9 9 0 1 0 .01 0M12 7l1.5 3.2 3.5.4-2.6 2.4.7 3.5-3.1-1.8-3.1 1.8.7-3.5L7 10.6l3.5-.4z',
  semi:'M4 14h16M4 10h16M9 5v14M15 5v14',partials:'M3 20V8M7 20V5M11 20V9M15 20V11M19 20V13M23 20v-5',even:'M3 20V8M11 20V12M19 20V15',attack:'M3 20L10 5h11',
  range:'M12 3v18M8 7l4-4 4 4M8 17l4 4 4-4',octave:'M4 18V6M4 12h16M20 18V6',knob:'M12 4a8 8 0 1 0 .01 0M12 12L16 6',link:'M10 14a4 4 0 0 1 0-6l3-3a4 4 0 0 1 6 6l-1.5 1.5M14 10a4 4 0 0 1 0 6l-3 3a4 4 0 0 1-6-6l1.5-1.5',spread:'M4 20V9M8 20V6M12 20V9M16 20V12'};
const icons=new Map();const iconPath=k=>{let p=icons.get(k);if(!p){p=new Path2D(ICON[k]);icons.set(k,p)}return p};
const VOICE_ICON={note:'fork',octave:'octave',semi:'semi',tune:'fork',body:'bell',partials:'partials',bright:'bright',even:'even',bend:'bend',hard:'hard',attack:'attack',
  decay:'decay',pdecay:'pdecay',damp:'damp',sweep:'sweep',tone:'tone',cutoff:'cutoff',spread:'spread',level:'level',velSense:'vel'};
// The bezel is a ring of 44 px sectors, so it carries the first six of a voice's parameters; the patch sheet has them all.
const BEZEL_PARAMS=6;
const settingText=x=>typeof x==='number'?String(+x.toFixed(3)):String(x);
const onOff=x=>x?'on':'off';
const numSector=(icon,label,o,k,min,max,step,after)=>({icon,label,min,max,step,format:['note','base','combBase'].includes(k)?noteName:settingText,
  log:min>0&&max/min>=20,get:()=>o[k],set:x=>{o[k]=x;after&&after()}});
const defSector=(icon,o,def,after)=>numSector(icon,def[1],o,def[0],def[2],def[3],def[4],after);
const stepSector=(icon,label,min,max,get,set,format=settingText)=>({icon,label,min,max,step:1,format,get,set});
const choiceSector=(icon,label,options,get,set,format=settingText)=>({icon,label,options,get,set,format,toggle:format===onOff});
// The synth's dial: its volume, a pad's note, and the knobs pinned to it in the patch sheet (◎ DIAL), as they turn there.
function waveSectors(owner,withNote){
  const v=owner.voice,out=[choiceSector('kind','voice',Object.keys(VOICES),()=>owner.voice.kind,k=>{owner.voice=makeVoice(k,{level:v.level,velSense:v.velSense,note:v.note})},kindLabel)];
  if(withNote)out.push(defSector('fork',v,VOICE_DEFS.wave[0]));
  out.push(numSector('level','volume',v,'level',0,1.5,.01));
  for(const path of v.dial||[]){const P=WAVE_PARAMS[WAVE_INDEX.get(path)];if(!P)continue;
    const int=P[5]==='int',post=()=>wavePost(v);
    out.push(int?{icon:'knob',label:waveLabel(v,path),min:P[2],max:P[3],step:1,format:x=>waveText(P,x),get:()=>waveGet(v,path),set:x=>{waveSet(v,path,x);post()}}
      :{icon:'knob',label:waveLabel(v,path),min:0,max:1,step:.001,format:n=>waveText(P,waveDenorm(P,n)),get:()=>waveNorm(P,waveGet(v,path)),set:n=>{waveSet(v,path,waveDenorm(P,n));post()}})}
  return out;
}
function voiceSectors(owner,withNote){
  if(owner.voice.kind==='wave')return waveSectors(owner,withNote);
  const v=owner.voice,out=[choiceSector('kind','voice',Object.keys(VOICES),()=>owner.voice.kind,k=>{owner.voice=makeVoice(k,{level:v.level,velSense:v.velSense,...(v.note!=null?{note:v.note,octave:v.octave,semi:v.semi}:{})})},kindLabel)];
  for(const def of [...VOICE_DEFS[v.kind].filter(d=>withNote||d[0]!=='note').slice(0,BEZEL_PARAMS),COMMON_DEFS[0]])
    out.push(Array.isArray(def[2])?choiceSector(VOICE_ICON[def[0]],def[1],def[2],()=>v[def[0]],x=>{v[def[0]]=x}):defSector(VOICE_ICON[def[0]],v,def));
  return out;
}
function readerSectors(o){
  const out=voiceSectors(o,false);
  if(o.tines)out.push(numSector('comb','comb tines',o,'tines',1,36,1),numSector('fork','comb note',o,'combBase',12,108,1),
    choiceSector('scale','comb scale',Object.keys(SCALES),()=>o.combScale,x=>{o.combScale=x}));
  return out;
}
const stringSectors=o=>[choiceSector('string','instrument',['string','bar'],()=>o.kind,x=>{o.kind=x}),numSector('fork','tuning Hz',o,'f200',20,2000,1),
  choiceSector('quant','quantize',[0,1],()=>o.quant,x=>{o.quant=x},onOff),numSector('decay','decay s',o,'decay',0,8,.01),numSector('damp','damping',o,'damp',0,2,.01),
  numSector('hard','hardness',o,'hard',0,1,.01),numSector('level','level',o,'level',0,1.5,.01),numSector('vel','velocity sense',o,'velSense',0,1,.01)];
function trackSectors(c){
  const t=c.track;
  const trim=()=>trimDots(t);
  // numbers are sliders, as every number on every dial is: drag along the ring, a step at a time
  return [choiceSector('scale','scale',Object.keys(SCALES),()=>t.scale,x=>{t.scale=x;trim()}),stepSector('fork','root',0,11,()=>t.root,x=>{t.root=x},i=>NOTE_NAMES[i]),
    stepSector('octave','octave',OCTAVES[0],OCTAVES.at(-1),()=>t.octave,x=>{t.octave=x}),stepSector('range','range',RANGES[0],RANGES.at(-1),()=>t.range,x=>{t.range=x;trim()},r=>r+' oct'),
    numSector('div','steps',t,'div',1,96,1)];
}
// A cam's dial: its shape, phase, the travel's window, the macro it turns, and the link to a reader.
const camSectors=m=>[choiceSector('sweep','shape',[...Object.keys(CAM_SHAPES),'drawn'],()=>m.shape,x=>{if(x!=='drawn')m.prof=camShape(x);m.shape=x;m.ver++}),
  numSector('odds','phase',m,'phase',0,1,.01),numSector('level','low',m,'lo',0,1,.01),numSector('level','high',m,'hi',0,1,.01),
  stepSector('knob','macro',1,WAVE_MACROS,()=>m.macro,x=>{m.macro=x},x=>'M'+x),
  {icon:'link',label:m.to?`linked to reader ${m.to.id}: tap to relink`:'link to a synth reader',action:()=>startLink(m)}];
// A probe's dial: the axis it reads, the window of its travel, the macro or parameter it drives, and the link.
function probeSectors(pr){
  const out=[choiceSector('sweep','axis',PROBE_AXES,()=>pr.axis,x=>{pr.axis=x;pr._key=null},x=>PROBE_AXIS_LABEL[x]),
    numSector('level','low',pr,'lo',0,1,.01),numSector('level','high',pr,'hi',0,1,.01)];
  if(pr.to&&pr.to.voice.kind==='wave')out.push(stepSector('knob','macro',1,WAVE_MACROS,()=>pr.macro,x=>{pr.macro=x},x=>'M'+x));
  else if(pr.to){const defs=probeDefs(pr.to.voice);if(defs.length)out.push(choiceSector('knob','drives',defs.map(d=>d[0]),()=>probeDef(pr)?.[0],x=>{pr.param=x},k=>defs.find(d=>d[0]===k)[1]))}
  out.push({icon:'link',label:pr.to?`drives ${voicedName(pr.to)}: tap to relink`:'link to a reader, pad or sphere',action:()=>startLink(pr)});
  return out;
}
// An FX field's dial: its effect, that effect's knobs, then how diffuse it is, its return and how much dry it takes.
function fieldSectors(f){
  const out=[choiceSector('fxmode','effect',FIELD_MODES,()=>f.mode,x=>setFieldMode(f,x),x=>FIELD_MODE_LABEL[x])];
  const icon={sync:'tempo',time:'tempo',fb:'tail',tone:'tone',ping:'spread',size:'size',decay:'tail',predelay:'attack',kind:'bend',drive:'hard',verbMode:'size',verbColor:'air'};
  for(const d of fieldDefs(f))out.push(Array.isArray(d[2])?choiceSector(icon[d[0]]||'knob',d[1],d[2],()=>f[d[0]],x=>{f[d[0]]=x},x=>fieldOptLabel(d[0],x)):numSector(icon[d[0]]||'knob',d[1],f,d[0],d[2],d[3],d[4]));
  out.push(numSector('spread','diffuse',f,'diffuse',0,1,.01),numSector('level','return',f,'level',0,1.5,.01),numSector('wet','takes dry',f,'insert',0,1,.01));
  return out;
}
const dotSectors=d=>[numSector('odds','probability',d,'p',0,1,.01),numSector('level','velocity',d,'v',0,1,.01),numSector('div','length steps',d,'len',1,ui.dotOf.track.div,1)];
const ROOM_ICON={mode:'verb',color:'era',wet:'wet',size:'size',decay:'tail',highCut:'air',shimmer:'shimmer'};
// the room's reverb: its mode and color step by name, its knobs turn
function roomSectors(){
  const out=ROOM_SPACE.filter(k=>k!=='freeze').map(k=>SPACE_CHOICES[k]?choiceSector(ROOM_ICON[k],'reverb '+k,SPACE_CHOICES[k].map((_,i)=>i),()=>SPACE[k],x=>{SPACE[k]=x;roomEdited()},i=>SPACE_CHOICES[k][i])
    :defSector(ROOM_ICON[k],SPACE,SPACE_DEFS.find(d=>d[0]===k),roomEdited));
  out.push(choiceSector('freeze','reverb freeze',[0,1],()=>SPACE.freeze,x=>{SPACE.freeze=x;roomEdited()},onOff));
  out.push(defSector('level',S,S_DEFS.find(d=>d[0]==='master'),roomEdited));
  out.push(choiceSector('quant','ratio snap',[0,1],()=>S.ratios,x=>{S.ratios=x;changed(true)},onOff));
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
  else if(sel.type==='cam')b=ring(camTip(o),0,camSectors(o));
  else if(sel.type==='probe'){if(probeAlive(o))b=ring(probePoint(o),0,probeSectors(o))}
  else if(sel.type==='field')b=ring(o,0,fieldSectors(o));
  else if(sel.type==='string')b=ring(mid(o),stringLen(o)*S.zoom/2,stringSectors(o));
  else if(sel.type==='dot')b=ring(ui.dotAt,0,dotSectors(o));
  else if(sel.type==='cog'&&o.track){
    // A disc zoomed into a score takes taps on its face as pins; its tuning is then in the notes sheet.
    // Otherwise a disc larger than the screen allows gets its bezel over its face at the largest ring that fits.
    const R=Math.min(Math.max(92,radius(o)*S.zoom+40),Math.min(CW,shelfTop()-safeTop)/2-BEZEL_W-8),c=toScreen(o);
    if(!noteMode(o))b={cx:c.x,cy:c.y,R,sectors:trackSectors(o),owner:o};
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
    else if(p.icon==='fxmode'){cx.strokeStyle=ink;cx.lineWidth=1.2;circle(ix,iy,8);const z=PX;PX=1;fieldGlyph(p.get(),ix,iy,5);PX=z}
    else{cx.save();cx.translate(ix-9,iy-9);cx.scale(.75,.75);cx.strokeStyle=ink;cx.lineWidth=1.8;cx.lineJoin=cx.lineCap='round';cx.stroke(iconPath(p.icon));cx.restore()}
  });
  cx.restore();
}
function bezelHit(b,s){
  const d=Math.hypot(s.x-b.cx,s.y-b.cy);if(Math.abs(d-b.R)>BEZEL_W+6)return -1;
  return Math.floor(wrap(Math.atan2(s.y-b.cy,s.x-b.cx)+Math.PI/2)/(TAU/b.sectors.length));
}

const PROBE={shelfTop,SHELF:PARTS,shelfSlot,get shelfX(){return shelfX},glyphAt,activeBezel,get rollSel(){return rollSel.size}};
