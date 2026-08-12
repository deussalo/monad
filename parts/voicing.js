/* MonadVoicing — voicing engine for monad.html
 * Exposes window.MonadVoicing — plain IIFE, no imports, no external deps.
 * Drop-in replacement for makeStack()/nextNote() in monad.html.
 * tick(nowMs) must be called each animation frame for drift mode.
 */
(function (global) {
  'use strict';

  /* ── MIDI roots ────────────────────────────────────────────────────────────
   * Matches monad.html exactly: E1(28) E2(40) E3(52), weights [1, 4.4, 3.2].
   * Middle octave is most likely; bass octave rare — keeps music airy.
   */
  var ROOTS = [28, 40, 52];

  /* ── Mode catalogue ────────────────────────────────────────────────────────
   * semitones: 6 ascending offsets from root (same meaning as monad makeStack).
   * rootWeights: 3 weights parallel to ROOTS.
   * drift: true only for the drift entry.
   *
   * Interval choices are tuned for calm / windchime character at low-mid register.
   * No offset pushes root=52 past MIDI 84 (52+29=81 ≤ 84). ✓
   */
  var MODES = [
    {
      id: 'quartal',
      // Pure stacked perfect 4ths (+5 each).  Evenly spaced — gives the most
      // uniform constellation and a wide open, harp-like sound.
      semitones: [0, 5, 10, 15, 20, 25],
      rootWeights: [1, 4.4, 3.2],
      drift: false
    },
    {
      id: 'quintal',
      // DEFAULT. Exact replica of monad.html makeStack: E B F# G D A.
      // Steps: +7 +7 +1 +7 +7 — the +1 cluster is the characteristic voicing.
      semitones: [0, 7, 14, 15, 22, 29],
      rootWeights: [1, 4.4, 3.2],
      drift: false
    },
    {
      id: 'jazz',
      // Maj7/9 shell: root M3 P5 M7 M9 M10.
      // Dense upper cluster; rootWeights tilted high for brighter register.
      semitones: [0, 4, 7, 11, 14, 16],
      rootWeights: [1, 3.8, 4.2],
      drift: false
    },
    {
      id: 'natural_minor',
      // Aeolian hexatonic: R M2 m3 P4 P5 m7.
      // Compact span (10 st) — small constellation tightly clustered left.
      semitones: [0, 2, 3, 5, 7, 10],
      rootWeights: [1, 4.4, 3.2],
      drift: false
    },
    {
      id: 'dorian',
      // R M2 m3 P4 P5 M6.  Similar span to natural_minor but the raised 6th
      // shifts the last dot right — two clusters with a notable central gap.
      semitones: [0, 2, 3, 5, 7, 9],
      rootWeights: [1, 4.4, 3.2],
      drift: false
    },
    {
      id: 'egyptian',
      // Egyptian suspended pentatonic + octave: R M2 P4 P5 m7 Oct.
      // Three clear pairs with equal-ish gaps — very open sound.
      semitones: [0, 2, 5, 7, 10, 12],
      rootWeights: [1, 4.4, 3.2],
      drift: false
    },
    {
      id: 'hirajoshi',
      // Japanese: R M2 m3 P5 m6 Oct.
      // Two tight clusters (0-2-3 and 7-8) with a large gap between — the most
      // visually distinctive constellation (two blobs separated by void).
      semitones: [0, 2, 3, 7, 8, 12],
      rootWeights: [1, 4.0, 3.6],
      drift: false
    },
    {
      id: 'double_harmonic',
      // Double harmonic (Arabic / Hijaz-like hexatonic): R m2 M3 P4 P5 m6.
      // Three pairs of close intervals — dense, exotic clustering.
      semitones: [0, 1, 4, 5, 7, 8],
      rootWeights: [1, 4.4, 3.2],
      drift: false
    },
    {
      id: 'drift',
      // Starts as quintal; tick() slowly lerps toward random non-drift targets.
      // semitones here are kept in sync with _ds.current (rounded integers).
      semitones: [0, 7, 14, 15, 22, 29],
      rootWeights: [1, 4.4, 3.2],
      drift: true
    }
  ];

  /* ── Internal playback state ───────────────────────────────────────────── */
  var _currentId = 'quintal';
  var _stack = [];
  var _idx = 0;

  /* ── Drift engine state ────────────────────────────────────────────────── */
  var _ds = {
    current:       [0, 7, 14, 15, 22, 29].slice(),  // float intervals
    target:        [0, 7, 14, 15, 22, 29].slice(),
    currentW:      [1, 4.4, 3.2].slice(),            // float rootWeights
    targetW:       [1, 4.4, 3.2].slice(),
    nextShiftMs:   0,    // set on first tick
    lastTickMs:    0
  };

  /* ── Helpers ───────────────────────────────────────────────────────────── */
  function _weightedRoot(weights) {
    var total = 0;
    for (var i = 0; i < weights.length; i++) total += weights[i];
    var r = Math.random() * total;
    for (var j = 0; j < ROOTS.length; j++) {
      r -= weights[j];
      if (r <= 0) return ROOTS[j];
    }
    return ROOTS[1]; // fallback E2
  }

  function _makeStack(semitones, rootWeights) {
    var root = _weightedRoot(rootWeights);
    var s = [];
    for (var i = 0; i < semitones.length; i++) {
      s.push(Math.round(root + semitones[i]));
    }
    return s;
  }

  function _modeById(id) {
    for (var i = 0; i < MODES.length; i++) {
      if (MODES[i].id === id) return MODES[i];
    }
    return null;
  }

  /* ── Public API ────────────────────────────────────────────────────────── */

  /** Switch active mode; resets the current stack. */
  function setMode(id) {
    if (!_modeById(id)) return;
    _currentId = id;
    _stack = [];
    _idx = 0;
  }

  /**
   * Drop-in replacement for monad.html nextNote().
   * Same stack-then-advance semantics; weighted root selection per-mode.
   * Returns a MIDI integer. Notes stay in 28..81 range across all modes.
   */
  function nextNote() {
    var mode = _modeById(_currentId);
    if (!mode) return 52;
    if (!_stack.length || _idx >= 6) {
      _stack = _makeStack(mode.semitones, mode.rootWeights);
      _idx = 0;
    }
    return _stack[_idx++];
  }

  /**
   * Call each animation frame with performance.now().
   * Only does work in drift mode; safe to call in all modes.
   */
  function tick(nowMs) {
    if (_ds.lastTickMs === 0) {
      _ds.lastTickMs = nowMs;
      _ds.nextShiftMs = nowMs + 30000; // first drift after 30 s
      return;
    }

    var dt = nowMs - _ds.lastTickMs;
    _ds.lastTickMs = nowMs;

    if (_currentId !== 'drift') return;

    // Pick a new target mode when timer fires
    if (nowMs >= _ds.nextShiftMs) {
      var candidates = [];
      for (var k = 0; k < MODES.length; k++) {
        if (!MODES[k].drift) candidates.push(MODES[k]);
      }
      var t = candidates[Math.floor(Math.random() * candidates.length)];
      _ds.target  = t.semitones.slice().map(Number);
      _ds.targetW = t.rootWeights.slice();
      // Next shift in 60–120 s
      _ds.nextShiftMs = nowMs + 60000 + Math.random() * 60000;
    }

    // Exponential approach — 120 s time constant (≈63 % of distance in 2 min)
    var alpha = 1 - Math.exp(-dt / 120000);
    var i;
    for (i = 0; i < 6; i++) {
      _ds.current[i] += (_ds.target[i] - _ds.current[i]) * alpha;
    }
    for (i = 0; i < 3; i++) {
      _ds.currentW[i] += (_ds.targetW[i] - _ds.currentW[i]) * alpha;
    }

    // Sync the drift mode def so callers that read MODES[].semitones stay current
    var driftDef = _modeById('drift');
    for (i = 0; i < 6; i++) driftDef.semitones[i] = Math.round(_ds.current[i]);
    for (i = 0; i < 3; i++) driftDef.rootWeights[i] = _ds.currentW[i];
  }

  /**
   * Returns dot positions for drawing a 34 px constellation icon.
   *
   * dots: array of 6 {a: 0..1} where a is the normalized linear position.
   *       dot spacing = intervals (larger gap → further apart visually).
   * span: total semitone span; pass to drawing code as scale hint so the
   *       caller can compare modes on a common axis (e.g. span/29 * 34 px).
   *
   * Drift returns the current float state so icons animate smoothly as it morphs.
   */
  function constellation(id) {
    var mode = _modeById(id);
    if (!mode) return { dots: [], span: 0 };

    // Use float current for drift so the icon animates continuously
    var raw = (id === 'drift') ? _ds.current : mode.semitones;
    var first = raw[0];
    var last  = raw[raw.length - 1];
    var span  = last - first;
    if (span <= 0) span = 1;

    var dots = [];
    for (var i = 0; i < raw.length; i++) {
      dots.push({ a: (raw[i] - first) / span });
    }

    return { dots: dots, span: span };
  }

  /**
   * Plays the mode as a soft ascending arpeggio.
   * Calls strike(midiInt, amp, delaySeconds) — caller supplies the callback.
   * Uses E3 (MIDI 52) as audition root so preview is always in mid register.
   * ~90 ms spacing, amplitude decays 0.82× per step.
   */
  function audition(id, strike, nowMs) {
    var mode = _modeById(id);
    if (!mode || typeof strike !== 'function') return;

    var ROOT    = 52; // E3
    var SPACING = 0.09; // seconds between notes
    var amp     = 0.65;
    var AMP_DECAY = 0.82;

    for (var i = 0; i < mode.semitones.length; i++) {
      var midi = mode.semitones[i] + ROOT;
      if (midi < 0) midi = 0;
      if (midi > 127) midi = 127;
      strike(midi, amp, i * SPACING);
      amp *= AMP_DECAY;
    }
  }

  /* ── Export ────────────────────────────────────────────────────────────── */
  global.MonadVoicing = {
    /** Ordered mode catalogue — read-only by convention. */
    modes: MODES,

    /** ID of the currently active mode. */
    get current() { return _currentId; },

    setMode:       setMode,
    nextNote:      nextNote,
    tick:          tick,
    constellation: constellation,
    audition:      audition
  };

}(window));
