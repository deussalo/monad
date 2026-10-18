/* ---------- wave synth: one parameter registry for the patch editor, the mod matrix and the audio worklet ---------- */
// Three wavetable oscillators (A, B, C) whose tables are built additively, each with its own amp envelope and its own
// filter with a filter envelope; a sub and a noise source on ENV1; three global envelopes, four LFOs and eight
// macros that any knob can take as modulation, as in Serum. Every value here can be a mod destination.
const WAVE_OSCS=3,WAVE_ENVS=4,WAVE_LFOS=4,WAVE_MACROS=8,WAVE_HARMS=32;
const WAVE_TABLES=['basic','harmonics','sweep','pulse','vocal','organ'];
const WAVE_FILTERS=['off','lp12','lp24','hp12','hp24','bp','notch','peak'];
const LFO_SHAPES=['sine','tri','saw up','saw down','square','s&h','smooth'];
// Tempo divisions, in beats: LFOs and the delay lock to the machine's bpm (120 without a bpm lock).
const LFO_SYNCS={off:0,'1/32':1/8,'1/16':1/4,'1/8T':1/3,'1/8':1/2,'1/4T':2/3,'1/4':1,'1/2':2,'1 bar':4,'2 bars':8,'4 bars':16};
const DELAY_SYNCS={off:0,'1/16':1/4,'1/8T':1/3,'1/8':1/2,'1/8.':3/4,'1/4T':2/3,'1/4':1,'1/4.':3/2,'1/2':2};
// The FX rack, after the voices of a patch are summed, in this order, as Serum's: each on or off.
// The reverb is the room's own (verb.js), one per patch, with its mode and color.
const WAVE_FX=['hyper','dist','flanger','phaser','chorus','delay','reverb','comp','eq'];
const WAVE_FX_RE=/^(hyper|dist|flanger|phaser|chorus|delay|reverb|eq|comp)\./;
// A patch's rack order: its own list, any module it leaves out keeping its place at the end.
const waveFxOrder=o=>{const a=Array.isArray(o)?o.filter((k,i)=>WAVE_FX.includes(k)&&o.indexOf(k)===i):[];return [...a,...WAVE_FX.filter(k=>!a.includes(k))]};
const DIST_MODES=['tube','hard','fold','crush'];
const VOICE_MODES=['poly','mono','legato'];
// Warp modes, two per oscillator as in Serum 2: phase warps (bend, sync, pwm, asym, mirror, fm) bend where the table is read,
// amplitude warps (flip, quantize, am, rm) reshape what it reads. FM, AM and RM take a sine at the pitch of another
// oscillator: A takes B's, B and C take A's.
const WARP_MODES=['off','bend','sync','pwm','asym','mirror','fm','flip','quantize','am','rm'];
// A route's source through its curve (−1 bends toward the bottom, +1 toward the top), as the matrix's curve column.
const modCurve=(x,c)=>!c?x:c>0?1-Math.pow(1-x,1+3*c):Math.pow(x,1-3*c);
const WAVE_SOURCES=[...Array.from({length:WAVE_ENVS},(_,i)=>'env'+(i+1)),...Array.from({length:WAVE_LFOS},(_,i)=>'lfo'+(i+1)),
  ...Array.from({length:WAVE_MACROS},(_,i)=>'m'+(i+1)),'vel','note','rand'];
// [path, label, min, max, default, curve]: lin, log (a ratio range), time (cubic, fine at the short end), int.
function waveParamList(){
  const P=[];
  for(let i=1;i<=WAVE_OSCS;i++){const o='osc'+i+'.';P.push(
    [o+'pos','position',0,1,0,'lin'],[o+'oct','octave',-4,4,0,'int'],[o+'semi','semi',-12,12,0,'int'],[o+'fine','fine',-100,100,0,'lin'],
    [o+'level','level',0,1,i===1?.75:0,'lin'],[o+'pan','pan',-1,1,0,'lin'],[o+'uni','unison',1,8,1,'int'],[o+'detune','detune',0,1,.25,'lin'],[o+'blend','blend',0,1,.5,'lin'],
    [o+'spread','stereo',0,1,.8,'lin'],[o+'phase','phase',0,1,0,'lin'],[o+'warp','warp',-1,1,0,'lin'],[o+'warp2','warp 2',-1,1,0,'lin'],
    [o+'a','attack',0,8,.003,'time'],[o+'d','decay',0,8,.8,'time'],[o+'s','sustain',0,1,.7,'lin'],[o+'r','release',0,8,.3,'time'],
    [o+'cut','cutoff',20,20000,20000,'log'],[o+'res','resonance',0,1,.1,'lin'],[o+'drive','drive',0,1,0,'lin'],[o+'key','key track',0,1,0,'lin'],
    [o+'famt','env amt',-1,1,0,'lin'],[o+'fa','f attack',0,8,.003,'time'],[o+'fd','f decay',0,8,.5,'time'],[o+'fs','f sustain',0,1,0,'lin'],[o+'fr','f release',0,8,.3,'time'])}
  P.push(['sub.level','sub',0,1,0,'lin'],['sub.oct','sub oct',-3,0,-1,'int'],['noise.level','noise',0,1,0,'lin'],['noise.color','color',0,1,.6,'lin']);
  for(let i=1;i<=WAVE_ENVS;i++){const e='env'+i+'.';P.push([e+'a','attack',0,8,.003,'time'],[e+'d','decay',0,8,.6,'time'],[e+'s','sustain',0,1,i===1?.8:0,'lin'],[e+'r','release',0,8,.3,'time'])}
  for(let i=1;i<=WAVE_LFOS;i++){const l='lfo'+i+'.';P.push([l+'rate','rate',.02,40,2,'log'],[l+'phase','phase',0,1,0,'lin'])}
  for(let i=1;i<=WAVE_MACROS;i++)P.push(['m'+i,'macro '+i,0,1,0,'lin']);
  P.push(['tune.oct','octave',-4,4,0,'int'],['tune.semi','semi',-12,12,0,'int'],['vol','volume',0,1,.8,'lin'],['glide','glide',0,2,0,'time'],['gate','gate',.02,8,.35,'time']);
  P.push(['hyper.voices','voices',1,7,4,'int'],['hyper.rate','rate',.05,5,.5,'log'],['hyper.detune','detune',0,1,.4,'lin'],['hyper.mix','mix',0,1,.5,'lin'],['hyper.dim','dimension',0,1,.3,'lin'],
    ['flanger.rate','rate',.05,10,.3,'log'],['flanger.depth','depth',0,1,.7,'lin'],['flanger.fb','feedback',0,.95,.5,'lin'],['flanger.spread','stereo',0,1,.25,'lin'],['flanger.mix','mix',0,1,.5,'lin'],
    ['phaser.rate','rate',.05,10,.4,'log'],['phaser.depth','depth',0,1,.7,'lin'],['phaser.freq','freq',100,8000,800,'log'],['phaser.fb','feedback',0,.9,.4,'lin'],['phaser.spread','stereo',0,1,.25,'lin'],['phaser.mix','mix',0,1,.5,'lin'],
    ['dist.drive','drive',0,1,.3,'lin'],['dist.mix','mix',0,1,1,'lin'],
    ['chorus.rate','rate',.05,5,.6,'log'],['chorus.depth','depth',0,1,.5,'lin'],['chorus.mix','mix',0,1,.5,'lin'],
    ['delay.time','time',.01,2,.3,'time'],['delay.fb','feedback',0,.95,.4,'lin'],['delay.tone','tone',0,1,.6,'lin'],['delay.mix','mix',0,1,.25,'lin'],
    ['reverb.mix','mix',0,1,.3,'lin'],['reverb.predelay','predelay',0,.25,.01,'time'],['reverb.decay','decay',.2,20,2,'log'],['reverb.size','size',0,1,.55,'lin'],
    ['reverb.attack','attack',0,1,.15,'lin'],['reverb.early','early diff',0,1,.75,'lin'],['reverb.late','late diff',0,1,.7,'lin'],['reverb.mod','mod depth',0,1,.5,'lin'],
    ['reverb.rate','mod rate',.05,10,.6,'log'],['reverb.lowcut','low cut',10,1000,80,'log'],['reverb.highcut','high cut',1000,20000,9000,'log'],
    ['eq.low','low dB',-15,15,0,'lin'],['eq.high','high dB',-15,15,0,'lin'],
    ['comp.thresh','threshold',-40,0,-12,'lin'],['comp.ratio','ratio',1,20,4,'log'],['comp.gain','makeup',0,18,3,'lin']);
  return P;
}
const WAVE_PARAMS=waveParamList(),WAVE_INDEX=new Map(WAVE_PARAMS.map((p,i)=>[p[0],i]));
const waveNorm=(p,x)=>{const[,,a,b,,c]=p;if(c==='log')return Math.log(x/a)/Math.log(b/a);const n=(x-a)/(b-a);return c==='time'?Math.cbrt(Math.max(0,n)):n};
const waveDenorm=(p,n)=>{const[,,a,b,,c]=p;n=Math.min(1,Math.max(0,n));if(c==='log')return a*Math.pow(b/a,n);if(c==='time')return a+(b-a)*n*n*n;const x=a+(b-a)*n;return c==='int'?Math.round(x):x};
// A path's home in a patch: osc1.cut is patch.osc[0].cut, env2.a is patch.env[1].a, m3 is patch.macro[2].
function wavePath(p,path){
  let m=path.match(/^(osc|env|lfo)(\d)\.(\w+)$/);if(m)return [p[m[1]][+m[2]-1],m[3]];
  m=path.match(/^m(\d)$/);if(m)return [p.macro,+m[1]-1];
  m=path.match(/^(sub|noise|tune)\.(\w+)$/);if(m)return [p[m[1]],m[2]];
  m=path.match(/^(hyper|dist|flanger|phaser|chorus|delay|reverb|eq|comp)\.(\w+)$/);if(m)return [(p.fx||{})[m[1]],m[2]];
  return [p,path];
}
const waveGet=(p,path)=>{const[o,k]=wavePath(p,path);return o?o[k]:undefined};
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
