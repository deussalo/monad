/* ---------- boot ---------- */
mount();resize();
const saved=storage.get(),shared=location.hash.match(/^#w=([\w-]+)/);
if(saved)try{deserialize(JSON.parse(saved))}catch(e){storage.keep(STORE+'.rejected',saved);notice(`Your saved machine could not be loaded (${e.message}); it is kept as ${STORE}.rejected. Showing the demo.`);demo()}
else demo();
edits.reset();
if(shared)loadShared(shared[1]).then(()=>edits.reset()).catch(e=>notice(`This share link could not be loaded: ${e.message}`));
clock.off=nowSec()-simT;
window.__cogs=Object.defineProperties({W,S,SPACE,stats,clock,get simT(){return simT},serialize,deserialize,solve,toScreen,radius,ringR,armTip,renderTh,beltLines,stringLen,
  get sel(){return sel&&{type:sel.type,id:sel.o.id}},get group(){return group.map(g=>g.type).sort()},get armed(){return ui.armed},get flying(){return !!ui.cam},
  engine:()=>({state:engineState,ac:ac&&ac.state,...engineStats,wave:waveVoices}),
  worldKey,listener,undo:()=>edits.undo(),redo:()=>edits.redo(),get undos(){return edits.past.length},get redos(){return edits.future.length},get pending(){return edits.pending},commit:()=>edits.commit()},Object.getOwnPropertyDescriptors(PROBE));
setInterval(tick,15);requestAnimationFrame(frame);
