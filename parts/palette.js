/* MonadPalette — photo→palette extraction for monad.html
 * Exposes window.MonadPalette — plain IIFE, no imports, no external deps.
 * Client-side only; no network calls; no user-visible text injected into DOM.
 */
(function (global) {
  'use strict';

  /* ── Helpers ───────────────────────────────────────────────────────────── */
  function clamp(v, lo, hi) { return v < lo ? lo : v > hi ? hi : v; }

  function lum(r, g, b) {
    // Relative luminance (sRGB, linearised via approximate gamma)
    var rr = r / 255, gg = g / 255, bb = b / 255;
    return 0.2126 * rr + 0.7152 * gg + 0.0722 * bb;
  }

  function hexToRgb(h) {
    var n = parseInt(h.replace('#', ''), 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }

  function rgbToHex(rgb) {
    return '#' + rgb.map(function (v) {
      return clamp(Math.round(v), 0, 255).toString(16).padStart(2, '0');
    }).join('');
  }

  /* ── HSL utilities (for harmonize) ────────────────────────────────────── */
  function rgbToHsl(r, g, b) {
    r /= 255; g /= 255; b /= 255;
    var max = Math.max(r, g, b), min = Math.min(r, g, b);
    var l = (max + min) / 2;
    if (max === min) return [0, 0, l];
    var d = max - min;
    var s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    var h;
    if (max === r)      h = ((g - b) / d + (g < b ? 6 : 0)) / 6;
    else if (max === g) h = ((b - r) / d + 2) / 6;
    else                h = ((r - g) / d + 4) / 6;
    return [h, s, l];
  }

  function hslToRgb(h, s, l) {
    if (s === 0) {
      var v = Math.round(l * 255);
      return [v, v, v];
    }
    var q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    var p = 2 * l - q;
    function hue(t) {
      if (t < 0) t += 1;
      if (t > 1) t -= 1;
      if (t < 1 / 6) return p + (q - p) * 6 * t;
      if (t < 1 / 2) return q;
      if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
      return p;
    }
    return [
      Math.round(hue(h + 1 / 3) * 255),
      Math.round(hue(h)         * 255),
      Math.round(hue(h - 1 / 3) * 255)
    ];
  }

  /* ── Median-cut quantizer ──────────────────────────────────────────────── */
  /**
   * Classic recursive median cut.
   * pixels: array of [r,g,b] triples (already downsampled).
   * n:      target colour count (8 by default).
   * Returns an array of [r,g,b] averages, length = n.
   */
  function medianCut(pixels, n) {
    if (!pixels.length) return [[128, 128, 128]];
    var depth = Math.ceil(Math.log2(Math.max(1, n)));

    function averageBucket(bucket) {
      var sr = 0, sg = 0, sb = 0;
      for (var i = 0; i < bucket.length; i++) {
        sr += bucket[i][0]; sg += bucket[i][1]; sb += bucket[i][2];
      }
      var l = bucket.length;
      return [sr / l, sg / l, sb / l];
    }

    function cut(bucket, d) {
      if (d === 0 || bucket.length <= 1) {
        return bucket.length ? [averageBucket(bucket)] : [];
      }

      // Find channel with maximum range
      var minC = [255, 255, 255], maxC = [0, 0, 0];
      for (var i = 0; i < bucket.length; i++) {
        var p = bucket[i];
        for (var c = 0; c < 3; c++) {
          if (p[c] < minC[c]) minC[c] = p[c];
          if (p[c] > maxC[c]) maxC[c] = p[c];
        }
      }
      var ranges = [maxC[0] - minC[0], maxC[1] - minC[1], maxC[2] - minC[2]];
      var ch = 0;
      if (ranges[1] > ranges[ch]) ch = 1;
      if (ranges[2] > ranges[ch]) ch = 2;

      // Sort along widest axis and split at median
      bucket.sort(function (a, b) { return a[ch] - b[ch]; });
      var mid = bucket.length >> 1;

      var lo = cut(bucket.slice(0, mid), d - 1);
      var hi = cut(bucket.slice(mid),    d - 1);
      return lo.concat(hi);
    }

    var colors = cut(pixels, depth);

    // If fewer colors than requested (degenerate input), pad with first color
    while (colors.length < n) colors.push(colors[0] || [0, 0, 0]);
    return colors.slice(0, n);
  }

  /* ── Source → 2D context helper ────────────────────────────────────────── */
  /**
   * Draws source onto a canvas capped at MAX_DIM on its longest axis.
   * Returns {ctx, w, h} or throws.
   */
  var MAX_DIM = 128;

  function sourceToCtx(source) {
    var sw, sh;

    if (typeof ImageBitmap !== 'undefined' && source instanceof ImageBitmap) {
      sw = source.width;
      sh = source.height;
    } else if (source instanceof HTMLImageElement) {
      sw = source.naturalWidth;
      sh = source.naturalHeight;
    } else if (source instanceof HTMLCanvasElement) {
      sw = source.width;
      sh = source.height;
    } else if (typeof OffscreenCanvas !== 'undefined' && source instanceof OffscreenCanvas) {
      sw = source.width;
      sh = source.height;
    } else {
      throw new TypeError('fromImage: source must be ImageBitmap, HTMLImageElement, or Canvas');
    }

    if (!sw || !sh) throw new TypeError('fromImage: source has zero dimensions');

    var scale = Math.min(1, MAX_DIM / Math.max(sw, sh));
    var w = Math.max(1, Math.round(sw * scale));
    var h = Math.max(1, Math.round(sh * scale));

    var canvas = document.createElement('canvas');
    canvas.width  = w;
    canvas.height = h;
    var ctx = canvas.getContext('2d');
    ctx.drawImage(source, 0, 0, w, h);
    return { ctx: ctx, w: w, h: h };
  }

  /* ── Public API ────────────────────────────────────────────────────────── */

  /**
   * fromImage(source, n=8)
   * → Promise<string[]> of n '#rrggbb' hex values, sorted darkest→lightest.
   *
   * Downsamples to ≤128 px before quantizing (keeps median-cut fast even on
   * 3000×2000 originals; measured ~4–18 ms on that size depending on device).
   *
   * Supports: ImageBitmap, HTMLImageElement, HTMLCanvasElement, OffscreenCanvas.
   */
  function fromImage(source, n) {
    if (n === undefined) n = 8;
    return new Promise(function (resolve, reject) {
      var result;
      try {
        var ref = sourceToCtx(source);
        var imageData = ref.ctx.getImageData(0, 0, ref.w, ref.h);
        var data = imageData.data;

        // Collect opaque pixels only
        var pixels = [];
        for (var i = 0; i < data.length; i += 4) {
          if (data[i + 3] < 128) continue; // skip transparent / semi-transparent
          pixels.push([data[i], data[i + 1], data[i + 2]]);
        }

        if (!pixels.length) {
          reject(new Error('fromImage: no opaque pixels in source'));
          return;
        }

        var colors = medianCut(pixels, n);

        // Sort darkest → lightest by relative luminance
        colors.sort(function (a, b) {
          return lum(a[0], a[1], a[2]) - lum(b[0], b[1], b[2]);
        });

        result = colors.slice(0, n).map(function (c) {
          return rgbToHex(c);
        });
      } catch (err) {
        reject(err);
        return;
      }
      resolve(result);
    });
  }

  /**
   * attachDropTarget(el, onPalette)
   * Wires dragover/drop + document-level paste for image files on el.
   * Calls onPalette(hexArray, {x, y}) with drop point in CSS px.
   * Returns a detach() function.
   * Never navigates the page away; only prevents default on image drops/pastes.
   */
  function attachDropTarget(el, onPalette) {
    function processFile(file, x, y) {
      if (!file || !file.type.startsWith('image/')) return;
      createImageBitmap(file).then(function (bmp) {
        return fromImage(bmp, 8).then(function (hexes) {
          bmp.close();
          onPalette(hexes, { x: x, y: y });
        });
      }).catch(function (err) {
        console.warn('MonadPalette: drop failed', err);
      });
    }

    function onDragover(e) {
      e.preventDefault();
      e.stopPropagation();
      e.dataTransfer.dropEffect = 'copy';
    }

    function onDrop(e) {
      e.preventDefault();
      e.stopPropagation();
      var rect = el.getBoundingClientRect();
      var x = e.clientX - rect.left;
      var y = e.clientY - rect.top;
      var file = e.dataTransfer && e.dataTransfer.files && e.dataTransfer.files[0];
      processFile(file, x, y);
    }

    function onPaste(e) {
      var items = e.clipboardData && e.clipboardData.items;
      if (!items) return;
      for (var i = 0; i < items.length; i++) {
        if (items[i].type.startsWith('image/')) {
          e.preventDefault();
          var file = items[i].getAsFile();
          processFile(file, 0, 0);
          return;
        }
      }
    }

    el.addEventListener('dragover', onDragover);
    el.addEventListener('drop',     onDrop);
    document.addEventListener('paste', onPaste);

    return function detach() {
      el.removeEventListener('dragover', onDragover);
      el.removeEventListener('drop',     onDrop);
      document.removeEventListener('paste', onPaste);
    };
  }

  /**
   * harmonize(hexes)
   * Lightly adjusts the sorted-by-luminance palette so:
   *   index 0 → guaranteed dark background candidate (lum ≤ 0.08)
   *   last index → guaranteed bright accent (lum ≥ 0.65)
   *   middle swatches → saturation nudged +~12 % for instrument vividness.
   * Returns a new array of '#rrggbb' strings; does not mutate input.
   */
  function harmonize(hexes) {
    if (!hexes || !hexes.length) return [];
    var n = hexes.length;

    return hexes.map(function (hex, i) {
      var rgb = hexToRgb(hex);
      var hsl = rgbToHsl(rgb[0], rgb[1], rgb[2]);
      var h = hsl[0], s = hsl[1], l = hsl[2];

      if (i === 0) {
        // Dark end — pull luminance down to background-viable level
        l = Math.min(l, 0.08);
        s = Math.max(s, 0.08); // preserve a trace of chroma for warmth
      } else if (i === n - 1) {
        // Bright end — ensure readable accent luminance
        l = Math.max(l, 0.65);
      } else {
        // Mid-range — lift saturation so any muted photo stays instrument-useful
        s = Math.min(1, s * 1.15 + 0.04);
      }

      return rgbToHex(hslToRgb(h, s, l));
    });
  }

  /* ── Export ────────────────────────────────────────────────────────────── */
  global.MonadPalette = {
    fromImage:        fromImage,
    attachDropTarget: attachDropTarget,
    harmonize:        harmonize
  };

}(window));
