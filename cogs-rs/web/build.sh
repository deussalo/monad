#!/bin/sh
# The web build: web/dist/ (index.html, cogs.js, cogs_bg.wasm, worklet.js, worklet.wasm, cogs-web.js) and the same in
# one self-contained page, web/dist/cogs.html. Serve the folder over http (or open cogs.html as a file).
# Run from cogs-rs/; cargo goes through the machine's heavy lock.
set -e
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
LOCK="python3 /srv/rig/rig/scripts/heavy_lock.py --entry cogs-web-build --"
[ -f /srv/rig/rig/scripts/heavy_lock.py ] || LOCK=""
T=target/wasm32-unknown-unknown/release
# the worklet first: building it also writes a no-gui cogs.wasm, which the app's build then replaces
$LOCK cargo build -p cogs-worklet --release --target wasm32-unknown-unknown
$LOCK cargo build -p cogs-app --lib --profile web --target wasm32-unknown-unknown
rm -rf web/dist && mkdir -p web/dist
wasm-bindgen --target no-modules --no-typescript --out-dir web/dist target/wasm32-unknown-unknown/web/cogs.wasm
cp "$T/cogs_worklet.wasm" web/dist/worklet.wasm
# wasm-opt, if there: COGS_WASM_OPT, on PATH, or from `npm install binaryen` in ~/.cache/cogs-binaryen (web/README.md)
OPT="${COGS_WASM_OPT:-$(command -v wasm-opt || true)}"
[ -n "$OPT" ] || { [ -x "$HOME/.cache/cogs-binaryen/node_modules/.bin/wasm-opt" ] && OPT="$HOME/.cache/cogs-binaryen/node_modules/.bin/wasm-opt"; } || true
if [ -n "$OPT" ]; then
  F="--enable-bulk-memory --enable-nontrapping-float-to-int --enable-sign-ext --enable-reference-types --enable-multivalue --enable-mutable-globals --strip-debug --strip-producers"
  $OPT -Oz $F web/dist/cogs_bg.wasm -o web/dist/cogs_bg.wasm
  $OPT -O3 $F web/dist/worklet.wasm -o web/dist/worklet.wasm
else echo "no wasm-opt: the wasm is as cargo made it"; fi
cp web/index.html web/cogs-web.js web/worklet.js web/dist/
python3 web/single.py web/dist
ls -la web/dist
