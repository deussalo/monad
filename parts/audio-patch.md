# monad audio-patch.md — B2 glitch-reduction patch spec

Owner: monad-lead applies every hunk by hand.  
Anchors are exact copy-pasteable strings; each is unique in monad.html as of the
commit this was written against. No line numbers — file shifts break those.

---

## Change 1 — latencyHint: 'playback' on LOW\_POWER

**Why.**  
`'playback'` asks the OS for the largest feasible audio buffer, maximising the
render-quantum headroom on a phone under physics+GPU load. Desktop users keep
`'balanced'` (shorter latency, adequate CPU). On a phone that is already in
`LOW_POWER` mode the extra 20-40 ms of output latency is inaudible against the
reverb tail; underrun crackle is not.

**Find this exact string (unique in file):**
```
ac=new AC({latencyHint:'balanced'});
```

**Replace with:**
```
ac=new AC({latencyHint:LOW_POWER?'playback':'balanced'});
```

---

## Change 2a — MAX\_MODAL\_EVENTS: 18 → 8 on LOW\_POWER

**Why.**  
18 events × 3 partials = 54 concurrent oscillators at peak. On a phone that is
54 sine-wave generators in the audio render thread on top of the FDN reverb and
saw voices. Halving to 8 (24 partials) halves that sub-cost with inaudible
effect under reverb.

**Find this exact string (unique in file — it sits at the end of a longer `const` declaration):**
```
MAX_MODAL_EVENTS=18,MAX_MODAL_PARTIALS=MAX_MODAL_EVENTS*3;
```

**Replace with:**
```
MAX_MODAL_EVENTS=LOW_POWER?8:18,MAX_MODAL_PARTIALS=MAX_MODAL_EVENTS*3;
```

---

## Change 2b — drop 3rd+ partial above 2 kHz on LOW\_POWER

**Why.**  
Higher modal partials (index ≥ 2) above 2 kHz contribute almost nothing
perceptually — they are masked by the reverb wash and the saw voice — but each
is a full OscillatorNode at audio-thread cost. On LOW\_POWER skip them.

**Find this exact string (unique in file):**
```
    if(f>15000)continue;
```

**Replace with:**
```
    if(f>15000)continue;
    if(LOW_POWER&&i>=2&&f>2000)continue;
```

(The replacement line inherits the same 4-space indent as the anchor line.)

---

## Change 3a — gate driftTimer when AudioContext is suspended

**Why.**  
On Android Chrome the AudioContext can flip to `'suspended'` when the page is
briefly backgrounded or audio focus is lost. Without the guard the timer fires
8 `setTargetAtTime` automations into a suspended context; on resume those
automations are either expired or flood the schedule, causing a glitch burst.

**Find this exact string (unique in file):**
```
    if(!fdn||!ac)return;
```

**Replace with:**
```
    if(!fdn||!ac||ac.state!=='running')return;
```

---

## Change 3b — slow driftTimer from 950 ms to 1500 ms

**Why.**  
The FDN delay modulation is a barely-perceptible chorus shimmer; even at 1.5 s
intervals it morphs faster than listeners notice. Reducing timer frequency by
37 % lowers background wakeup pressure on both the JS thread and, indirectly,
the audio thread (fewer automation insertions per minute).

**Find this exact string (unique in file):**
```
  },950);
```

**Replace with:**
```
  },1500);
```

---

## Change 4 — time-based voice reclaim in chooseFreeVoice

**Why.**  
`osc.onended` is unreliable on Android Chrome: it sometimes fires seconds late
or not at all when the page is under load or audio hardware switched. Voices
that should be free (past their scheduled `trueStop` time) stay `busy`, pool
exhaustion returns `null` from `chooseFreeVoice`, and strikes are silently
dropped. Adding a time-based reclaim pass ensures voices are freed even when
the callback is delayed.

The reclaim sets `v.osc=null` before returning, so if `onended` fires later
the `if(v.osc===osc)` guard in the callback is false — no double-decrement of
`activeSourceVoices`.

**Find this exact string (unique in file — the closing of chooseFreeVoice):**
```
  for(let n=0;n<limit;n++){const i=(nextVoiceSearch+n)%limit;if(!voices[i].busy){nextVoiceSearch=(i+1)%limit;return voices[i]}}
  return null;
}
```

**Replace with:**
```
  for(let n=0;n<limit;n++){const i=(nextVoiceSearch+n)%limit;if(!voices[i].busy){nextVoiceSearch=(i+1)%limit;return voices[i]}}
  if(ac){const now=ac.currentTime;for(let n=0;n<limit;n++){const i=(nextVoiceSearch+n)%limit;const v=voices[i];if(v.busy&&now>v.end+0.05){try{if(v.osc)v.osc.disconnect()}catch(e){}v.busy=false;v.osc=null;activeSourceVoices=Math.max(0,activeSourceVoices-1);nextVoiceSearch=(i+1)%limit;return v}}}
  return null;
}
```

The grace period is `v.end + 0.05` (50 ms past `trueStop`), chosen so that:
- We never reclaim a voice before its oscillator has actually finished outputting
- We reclaim reliably on Android where `onended` may be 100–500 ms late
- The 50 ms gap is inaudible (the voice gain is already 0 by `stop` time)

---

## Change 5 — per-frame ac.currentTime / getOutputTimestamp audit

**No code change required.**

`ac.currentTime` is read at five call sites:

| Site | Context | Per-frame? |
|------|---------|-----------|
| driftTimer callback | `setInterval` at 950 ms | No |
| `applyReverb()` | Called from `flushAudioParams()` which gates on `audioParamsDirty && (now-lastParamFlush >= 30)` | No — at most 1×/30 ms, only when dirty |
| `playModalBody()` | Only on collision events | No |
| `strikeNow()` | Only on collision events | No |
| `osc.onended` callback | Only when an oscillator ends | No |

`getOutputTimestamp` is not called anywhere in the file.

`flushAudioParams(now)` is called every frame (line ~3630) but returns
immediately if `!audioParamsDirty`, so it never reaches `applyReverb()` on
quiet frames. No `ac.currentTime` poll per animation frame.

---

## Application order

Apply in document order:
1. Change 2a (constant declaration — earliest in file)
2. Change 1 (`initAudio` function)
3. Changes 3a + 3b (`buildReverb` / driftTimer — same region, apply together)
4. Change 2b (`playModalBody` partial loop)
5. Change 4 (`chooseFreeVoice` function)

Changes 2a and 1 must land before `buildReverb` is called (it references
`MAX_MODAL_EVENTS` and `LOW_POWER`), but that is already ensured by document
order since all three constants are defined before `initAudio`.
