# parts/ — historical delivery artifacts. NOT the source of truth.

**`monad.html` is the product and the only source of truth.** It is a single
self-contained file by design. Everything in this directory is either a
delivery artifact that was inlined into it, a scratch harness, or a proposal
that was evaluated and declined.

Editing a file in here changes nothing. The copy that runs is the one inlined
in `monad.html`.

| File | Status |
|------|--------|
| `voicing.js` | Inlined into `monad.html`. The copy here is frozen at delivery and has since diverged. |
| `palette.js` | Inlined into `monad.html`. Same — frozen, diverged. |
| `fluid-lead.js` | The first fluid engine. Inlined, then substantially rewritten in place (`c37b909`, `6814bd0`). Superseded. |
| `fluid2.js` | A proposed replacement engine. **Evaluated and NOT adopted** — the shipped engine was already verified, measured and deployed, and swapping it wholesale traded known-good for unknown with no demonstrated gain. Its header calls itself a "drop-in replacement"; it is not pending work. One genuine bug it identified (compounding quality scale) was extracted and fixed in `12aef97`. |
| `audio-patch.md`, `strike-patch.md` | Patch specs, applied by hand. Historical record of intent. |
| `*-test.html`, `*-test.js`, `fluid-harness.html` | Scratch harnesses from development. The maintained tests are `verify*.js` in the repo root. |

## Maintained tests (repo root)

`verify.js` (shell/wordless audit) · `verify-audio.js` (gesture activation) ·
`verify-storm.js` (collision storm) · `verify-strikes.js` (every impact sounds)
· `verify-nodepool.js` (warm-pool zero-allocation invariant)

Run with:
`LD_LIBRARY_PATH=/srv/rig/rig/webapp/scripts/playwright-libs/lib node verify.js`
