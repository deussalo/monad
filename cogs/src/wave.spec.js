/* ---------- wave synth: one parameter registry for the patch editor, the mod matrix and the audio worklet ---------- */
// Three wavetable oscillators (A, B, C) whose tables are built additively, each with its own amp envelope and its own
// filter with a filter envelope; a sub and a noise source on ENV1; three global envelopes, four LFOs and eight
// macros that any knob can take as modulation, as in Serum. Every value here can be a mod destination.
const WAVE_OSCS=3,WAVE_ENVS=3,WAVE_LFOS=4,WAVE_MACROS=8,WAVE_HARMS=32;
const WAVE_TABLES=['basic','harmonics','sweep','pulse','vocal','organ'];
const WAVE_FILTERS=['off','lp12','lp24','hp12','hp24','bp','notch','peak'];
const LFO_SHAPES=['sine','tri','saw up','saw down','square','s&h','smooth'];
const WAVE_SOURCES=[...Array.from({length:WAVE_ENVS},(_,i)=>'env'+(i+1)),...Array.from({length:WAVE_LFOS},(_,i)=>'lfo'+(i+1)),
  ...Array.from({length:WAVE_MACROS},(_,i)=>'m'+(i+1)),'vel','note','rand'];
// [path, label, min, max, default, curve]: lin, log (a ratio range), time (cubic, fine at the short end), int.
function waveParamList(){
  const P=[];
  for(let i=1;i<=WAVE_OSCS;i++){const o='osc'+i+'.';P.push(
    [o+'pos','wt pos',0,1,0,'lin'],[o+'oct','octave',-4,4,0,'int'],[o+'semi','semi',-12,12,0,'int'],[o+'fine','fine ct',-100,100,0,'lin'],
    [o+'level','level',0,1,i===1?.75:0,'lin'],[o+'pan','pan',-1,1,0,'lin'],[o+'uni','unison',1,8,1,'int'],[o+'detune','detune',0,1,.25,'lin'],
    [o+'spread','stereo',0,1,.8,'lin'],[o+'phase','phase',0,1,0,'lin'],[o+'warp','bend',-1,1,0,'lin'],
    [o+'a','attack',0,8,.003,'time'],[o+'d','decay',0,8,.8,'time'],[o+'s','sustain',0,1,.7,'lin'],[o+'r','release',0,8,.3,'time'],
    [o+'cut','cutoff',20,20000,20000,'log'],[o+'res','res',0,1,.1,'lin'],[o+'drive','drive',0,1,0,'lin'],[o+'key','key track',0,1,0,'lin'],
    [o+'famt','env amt',-1,1,0,'lin'],[o+'fa','f attack',0,8,.003,'time'],[o+'fd','f decay',0,8,.5,'time'],[o+'fs','f sustain',0,1,0,'lin'],[o+'fr','f release',0,8,.3,'time'])}
  P.push(['sub.level','sub',0,1,0,'lin'],['sub.oct','sub oct',-3,0,-1,'int'],['noise.level','noise',0,1,0,'lin'],['noise.color','color',0,1,.6,'lin']);
  for(let i=1;i<=WAVE_ENVS;i++){const e='env'+i+'.';P.push([e+'a','attack',0,8,.003,'time'],[e+'d','decay',0,8,.6,'time'],[e+'s','sustain',0,1,i===1?.8:0,'lin'],[e+'r','release',0,8,.3,'time'])}
  for(let i=1;i<=WAVE_LFOS;i++){const l='lfo'+i+'.';P.push([l+'rate','rate Hz',.02,40,2,'log'],[l+'phase','phase',0,1,0,'lin'])}
  for(let i=1;i<=WAVE_MACROS;i++)P.push(['m'+i,'macro '+i,0,1,0,'lin']);
  P.push(['vol','volume',0,1,.8,'lin'],['glide','glide',0,2,0,'time'],['gate','gate s',.02,8,.35,'time']);
  return P;
}
const WAVE_PARAMS=waveParamList(),WAVE_INDEX=new Map(WAVE_PARAMS.map((p,i)=>[p[0],i]));
const waveNorm=(p,x)=>{const[,,a,b,,c]=p;if(c==='log')return Math.log(x/a)/Math.log(b/a);const n=(x-a)/(b-a);return c==='time'?Math.cbrt(Math.max(0,n)):n};
const waveDenorm=(p,n)=>{const[,,a,b,,c]=p;n=Math.min(1,Math.max(0,n));if(c==='log')return a*Math.pow(b/a,n);if(c==='time')return a+(b-a)*n*n*n;const x=a+(b-a)*n;return c==='int'?Math.round(x):x};
// A path's home in a patch: osc1.cut is patch.osc[0].cut, env2.a is patch.env[1].a, m3 is patch.macro[2].
function wavePath(p,path){
  let m=path.match(/^(osc|env|lfo)(\d)\.(\w+)$/);if(m)return [p[m[1]][+m[2]-1],m[3]];
  m=path.match(/^m(\d)$/);if(m)return [p.macro,+m[1]-1];
  m=path.match(/^(sub|noise)\.(\w+)$/);if(m)return [p[m[1]],m[2]];
  return [p,path];
}
const waveGet=(p,path)=>{const[o,k]=wavePath(p,path);return o[k]};
const waveSet=(p,path,x)=>{const[o,k]=wavePath(p,path);o[k]=x};
// Additive spectra: harmonic amplitudes (index k is harmonic k+1) for each frame of each built-in table.
function waveSpectra(name,user){
  const H=512,frame=f=>{const a=new Float64Array(H);for(let k=0;k<H;k++)a[k]=f(k+1);return a};
  if(name==='basic')return [frame(k=>k===1?1:0),frame(k=>k%2?(((k-1)/2)%2?-1:1)/(k*k):0),frame(k=>1/k),frame(k=>k%2?1/k:0)];
  if(name==='sweep')return Array.from({length:8},(_,f)=>frame(k=>k<=1<<f?1/k:0));
  if(name==='pulse')return Array.from({length:8},(_,f)=>{const w=.5-.46*f/7;return frame(k=>Math.sin(Math.PI*k*w)/k)});
  if(name==='organ')return [[1,.5,0,.3],[1,1,.6,.6,0,.4,0,.3],[1,.8,.8,.5,.5,.6,.2,.4,.1,.3,.1,.2]].map(d=>frame(k=>d[k-1]||0));
  if(name==='vocal'){
    const F=[[800,1150,2900],[400,1600,2700],[350,2300,3000],[450,800,2830],[325,700,2530]],f0=110;
    return F.map(fs=>frame(k=>{const h=k*f0;let a=0;for(let j=0;j<3;j++)a+=Math.exp(-(((h-fs[j])/(90+40*j))**2))*[1,.6,.3][j];return a/Math.sqrt(k)+(k===1?.05:0)}));
  }
  const u=user&&user.length?user:[1];
  return [frame(k=>(u[k-1]||0)),frame(k=>k%2?(u[k-1]||0):0)];
}
