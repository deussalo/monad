/* ---------- desk: the right toolbar and the roll dock ---------- */
const bar=$('#bar'),rail=$('#rail'),dock=$('#dock'),dockTools=$('#dock-tools'),dockStrip=$('#dock-strip');
function onResize(){}
const barLeft=()=>bar.getBoundingClientRect().left;
const stage=()=>({l:0,r:barLeft(),t:0,b:dock.hidden?CH:dock.getBoundingClientRect().top});
const offStage=s=>s.x>=barLeft();
const chromeHit=()=>null;
function reveal(){}
const panelShown=()=>bar.classList.contains('open'),inspectorLive=()=>true;
function togglePanel(open=!panelShown()){bar.classList.toggle('open',open);if(open)syncRows()}
function placeRoll(c){
  const t=c.track;
  dockTools.replaceChildren(el('h3',{textContent:`disc ${c.id} · ${t.rings} rings · ${t.div} steps`}));dockStrip.replaceChildren();
  rollSection(c,{strip:dockStrip,tools:dockTools,row:clamp(Math.floor((innerHeight*.44-16)/t.rings),14,26),label:44});
}
function drawChrome(){if(ui.carry&&ui.carry.ghost)drawGhost(ui.carry.ghost);syncChrome()}
const railButtons={};
function railButton(id,label,key,title,act,icon){
  const b=el('button',{title:`${title} (${key})`,onclick:act,onmousedown:e=>e.preventDefault()});
  if(icon)b.append(icon);b.append(el('span',{textContent:label}),el('kbd',{textContent:key}));
  b.dataset.rail=id;rail.append(b);railButtons[id]=b;return b;
}
function partIcon(kind){
  const c=el('canvas'),r=devicePixelRatio||1,s=(ICON_SCALE[kind]||.8)*.6,main=cx,px=PX;
  c.width=40*r;c.height=30*r;cx=c.getContext('2d');cx.setTransform(r*s,0,0,r*s,20*r,15*r);PX=1/s;drawPartIcon(kind);cx=main;PX=px;
  return c;
}
let chromeState='';
function syncChrome(){
  const open=bar.classList.contains('open'),right=bar.offsetWidth+'px';
  if(dock.style.right!==right)dock.style.right=right;
  if(dock.hidden===!!roll)dock.hidden=!roll;
  const state=[ui.armed,clock.paused,edits.past.length,edits.future.length,open,ui.scrap].join();
  if(state===chromeState)return;chromeState=state;
  bar.classList.toggle('scrap',ui.scrap);
  for(const kind of PARTS)railButtons[kind].classList.toggle('on',ui.armed===kind);
  railButtons.select.classList.toggle('on',!ui.armed);
  railButtons.panel.firstChild.textContent=open?'panel ›':'‹ panel';
  railButtons.pause.firstChild.textContent=clock.paused?'play':'pause';railButtons.pause.classList.toggle('on',clock.paused);
  railButtons.undo.disabled=!edits.past.length;railButtons.redo.disabled=!edits.future.length;
}
function mount(){
  buildPanel();
  const legend=t=>[...panel.children].find(f=>f.firstChild.textContent===t);
  panel.insertBefore(legend('stats'),legend('json'));
  railButton('panel','panel','?','show or hide the panel',()=>togglePanel()).classList.add('toggle');
  for(const kind of PARTS)railButton(kind,kind,KEY_OF[kind],'place a '+kind,()=>armPart(ui.armed===kind?null:kind),partIcon(kind));
  rail.append(el('hr'));
  railButton('select','select','s','select and move: disarm the part',()=>armPart(null));
  railButton('undo','undo','ctrl z','undo',()=>edits.undo());
  railButton('redo','redo','ctrl ⇧z','redo',()=>edits.redo());
  railButton('pause','pause','space','pause or play',()=>setPaused(!clock.paused));
  railButton('fit','fit','0','fit the machine in view',flyFit);
  railButton('delete','delete','del','delete the selection',removeSel);
}
const PROBE={stage,
  get open(){return bar.classList.contains('open')},
  get roll(){if(!roll||dock.hidden)return null;const g=rollGeom(),r=roll.cv.getBoundingClientRect();
    return {x:r.left,y:r.top,label:roll.label,row:roll.row,cw:g.cw,rings:roll.c.track.rings,div:roll.c.track.div,id:roll.c.id,
      pins:roll.c.track.dots.map(d=>[rollCol(roll.c,d),d.ring,rollSel.has(d)?1:0]).sort((a,b)=>a[0]-b[0]||a[1]-b[1])}}};
