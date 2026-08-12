# monad strike-patch.md — B3 "every impact must sound"

Owner: monad-lead applies every hunk by hand.  
Anchors are exact copy-pasteable strings, each unique in monad.html.  
Apply in document order (A before B before C … before G4).

---

## Root-cause audit

Five gates currently eat impacts — three are live killers, two are dormant guards
that become active once the live ones are fixed:

| Gate | Status | Culprit |
|------|--------|---------|
| `ORB_REFRACTORY_MS=20` in queueHit | **LIVE** — blocks ~50 % of hits at 60 fps; ~67 % at 120 fps | primary cause |
| `impact>5` collision gate (line in collide()) | **LIVE** — silent floor for gentle grazes | secondary cause |
| `imp>5` wall-bounce gate (line in integrate()) | **LIVE** — same floor for soft wall touches | secondary cause |
| `chooseFreeVoice()` returns null → voiceDrops | latent — rarely fires pre-B3, becomes relevant once refractory is cut | tertiary |
| `playModalBody()` strength≤.008 guard | dormant — never reached from collisions (>5 gate upstream); harmless | no change needed |

The 20 ms refractory is the principal offender: at 60 fps (frame ≈ 16.7 ms),
every consecutive-frame collision from the same orb is unconditionally dropped.
The orb-pair dedup `!previousContacts.has(key)` already prevents ONGOING contact
from re-firing each frame. The per-orb refractory is a second, over-broad guard
on top of per-pair dedup that swallows NEW distinct collision events.

---

## Estimated before/after ratio

`collisionCount` = every call to queueHit (all attempts, before any gate).  
`strikeCount` = calls that produced actual audio.

**Before patch:** With 20 ms refractory at 60 fps ≈ 40–50 % pass rate; voice-drop
adds another ~5 %. Estimated ratio ≈ **0.35–0.50**.

**After patch:** 3 ms refractory transparent at 60 fps + 120 fps; voice-stealing
eliminates voiceDrops. Estimated ratio ≈ **0.93–1.00** (remainder is legitimate
per-pair dedup of sustained contacts via `previousContacts`).

---

## Change A — impactAmplitude: lower silence floor 5 → 1

**Find this exact string (unique in file):**
```
function impactAmplitude(v){
  if(v<=5)return 0;

  const x=clamp((v-5)/165,0,1);
```

**Replace with:**
```
function impactAmplitude(v){
  if(v<=1)return 0;

  const x=clamp((v-1)/169,0,1);
```

**Why.** Lower the silence cut-off from 5 to 1 so gentle grazes produce a whisper
rather than silence. The curve ceiling stays at the same physical maximum (v=170
→ x=1.0). At v=5 the new amplitude is `SOURCE.level * 0.048` (≈ −26 dB relative
to peak) — audible but delicate. The existing limiter and `updatePolyHeadroom`
prevent any summing problem.

---

## Change B — wall-bounce gate: lower 5 → 1

**Find this exact string (unique in file):**
```
if(imp>5&&o.wall<=0){
```

**Replace with:**
```
if(imp>1&&o.wall<=0){
```

**Why.** Matches the new amplitude floor so gentle wall grazes produce quiet sound
rather than silence. The `o.wall=0.03` (30 ms) per-orb cooldown already prevents
rapid-fire wall bounces; the only change is that slower approaches now make a
whisper on first contact.

---

## Change C — orb-orb collision gate: lower 5 → 1

**Find this exact string (unique in file):**
```
&&impact>5){queueHit(a,impact);queueHit(b,impact)}
```

**Replace with:**
```
&&impact>1){queueHit(a,impact);queueHit(b,impact)}
```

**Why.** Matches the amplitude floor. Below impact=1 the computed amplitude is
zero (Change A), so this gate still prevents wasted queue entries for truly
motionless contacts. `!previousContacts.has(key)` already prevents sustained
contact from re-firing; this only allows first-contact grazes through.

---

## Change D — refractory: 20 ms → 3 ms

**Find this exact string (unique in file):**
```
HIT_BATCH_MS=10,ORB_REFRACTORY_MS=20;
```

**Replace with:**
```
HIT_BATCH_MS=10,ORB_REFRACTORY_MS=3;
```

**Why.** 20 ms blocked every consecutive-frame hit (16.7 ms at 60 fps). 3 ms
is below any realistic display frame period (even 360 Hz = 2.8 ms) but above
sub-millisecond OS timer noise. The within-batch dedup (`pendingHits.get(o)`)
and the per-pair dedup (`!previousContacts.has(key)`) already prevent true
duplicates; 3 ms is only a last-resort guard against sub-frame duplicate events.
The wall-bounce gate (`o.wall = 0.03`) separately prevents rapid-fire wall
re-strikes.

---

## Change E — voice stealing on pool exhaustion

**Find this exact string (unique in file — the closing of chooseFreeVoice):**
```
nextVoiceSearch=(i+1)%limit;return v}}}
  return null;
}
```

**Replace with:**
```
nextVoiceSearch=(i+1)%limit;return v}}}
  if(ac){const now=ac.currentTime;let best=null,bestScore=Infinity;for(let n=0;n<limit;n++){const v=voices[(nextVoiceSearch+n)%limit];if(!v.busy)continue;const span=Math.max(.001,v.end-v.peak);const fade=clamp((now-v.peak)/span,0,1);const score=v.amp*(1-fade);if(score<bestScore){bestScore=score;best=v}}if(best){try{if(best.osc){best.osc.stop(now+.001);best.osc.disconnect()}}catch(e){}best.osc=null;activeSourceVoices=Math.max(0,activeSourceVoices-1);best.busy=false;return best}}
  return null;
}
```

**Why.** On true voice-pool exhaustion (all 55 busy, time-based reclaim found
nothing past its stop time), steal the voice that is furthest through its decay
and quietest: `score = v.amp * (1 − fadeRatio)` — lowest score = best victim.
Stop the old oscillator at `now+1ms` (one render quantum of grace), disconnect
it, clear `v.osc` before returning. Setting `v.osc=null` before return ensures
the old `osc.onended` callback sees `v.osc !== osc` and does not
double-decrement `activeSourceVoices`; we decrement it ourselves here.

---

## Change F — modal body: skip on soft impacts

**Find this exact string (unique in file — in strikeNow):**
```
playModalBody(o,impact,false);
```

**Replace with:**
```
if(impact>12)playModalBody(o,impact,false);
```

**Why.** At impact < 12, the modal body's `eventLevel` is below −40 dBFS
relative to the reverb wash — inaudible — yet each call consumes a slot and
creates 1–3 OscillatorNodes. Gating at 12 reduces modal-slot pressure during
gentle collision storms, so the expensive layer fires only for meaningful
impacts. The primary saw voice still fires for ALL impacts above the 1-floor.
Keeps the B2 phone oscillator caps (MAX\_MODAL\_EVENTS, partial-frequency gate)
while making the cap far less likely to be reached on soft collisions.

---

## Change G — stats extension (collision / strike counters)

These four hunks extend `window.__monadAudioStats()` so the test harness can
report the sounded/collision ratio accurately.

### G1 — declare new counters

**Find this exact string (unique in file):**
```
let voiceDrops=0,modalDrops=0;
```

**Replace with:**
```
let voiceDrops=0,modalDrops=0,collisionCount=0,strikeCount=0;
```

### G2 — increment collisionCount at every queueHit call

**Find this exact string (unique in file):**
```
function queueHit(o,impact,physical=true){
  const now=performance.now();
```

**Replace with:**
```
function queueHit(o,impact,physical=true){
  collisionCount++;
  const now=performance.now();
```

`collisionCount` is incremented on every call to `queueHit`, including calls
that will be refracted or deduped. This gives the denominator for the real
sounded/attempted ratio — not just the post-gate ratio.

### G3 — increment strikeCount when audio fires

**Find this exact string (unique in file):**
```
osc.start(t);osc.stop(trueStop);
  updatePolyHeadroom(t);
```

**Replace with:**
```
osc.start(t);osc.stop(trueStop);
  strikeCount++;
  updatePolyHeadroom(t);
```

### G4 — expose in __monadAudioStats

**Find this exact string (unique in file):**
```
voiceDrops:voiceDrops,modalDrops:modalDrops,
```

**Replace with:**
```
voiceDrops:voiceDrops,modalDrops:modalDrops,collisionCount:collisionCount,strikeCount:strikeCount,
```

---

## Amplitude headroom note

Soft impacts produce very quiet amplitude (at v=5: ≈ `SOURCE.level * 0.048 ≈ 0.009`).
The existing `updatePolyHeadroom` already scales `voiceMix.gain` down by
`√(8/N)` when more than 8 voices are active. A 12-body storm at peak concurrency
adds ≈ `√(8/12) = 0.816` attenuation before the limiter. Summing 12 voices at
0.009 each ≈ 0.108 × 0.816 ≈ 0.088 into the limiter — well below clipping.
No additional headroom change is required.

---

## Application order

Apply in document order:

1. **G1** — counter declaration (earliest in file, near `voiceDrops`)
2. **D** — refractory constant (line ~1479)
3. **A** — impactAmplitude (line ~2234)
4. **E** — chooseFreeVoice stealing (line ~2263)
5. **F** — modal gate in strikeNow (line ~2407)
6. **G3** — strikeCount in strikeNow (line ~2440)
7. **B** — wall-bounce gate in integrate() (line ~2861)
8. **G2** — collisionCount in queueHit (line ~2469)
9. **C** — orb-orb collision gate in collide() (line ~2889)
10. **G4** — stats object (line ~1908 area — earlier in file, but depends on
    nothing; safe to apply any time after G1)
