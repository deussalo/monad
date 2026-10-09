/* ---------- boot ---------- */
mount();resize();
const saved=storage.get(),shared=location.hash.match(/^#w=([\w-]+)/);
if(saved)try{deserialize(JSON.parse(saved))}catch(e){console.error('cogs: saved world rejected:',e.message);demo()}
else{demo();const f=fitTarget();Object.assign(view,{x:f.x,y:f.y});S.zoom=f.z}
edits.reset();
if(shared)loadShared(shared[1]).then(()=>edits.reset()).catch(e=>console.error('cogs: shared world rejected:',e.message));
clock.off=nowSec()-simT;
window.__cogs=Object.defineProperties({W,S,SPACE,stats,clock,get simT(){return simT},serialize,deserialize,solve,toScreen,radius,ringR,armTip,beltLines,stringLen,
  get sel(){return sel&&{type:sel.type,id:sel.o.id}},get group(){return group.map(g=>g.type).sort()},get armed(){return ui.armed},get flying(){return !!ui.cam},
  engine:()=>({state:engineState,ac:ac&&ac.state,...engineStats}),
  worldKey,undo:()=>edits.undo(),redo:()=>edits.redo(),get undos(){return edits.past.length},get redos(){return edits.future.length},get pending(){return edits.pending},commit:()=>edits.commit()},Object.getOwnPropertyDescriptors(PROBE));
setInterval(tick,15);requestAnimationFrame(frame);
