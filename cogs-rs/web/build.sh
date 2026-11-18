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
$LOCK cargo build -p cogs-app --lib --release --target wasm32-unknown-unknown
rm -rf web/dist && mkdir -p web/dist
wasm-bindgen --target no-modules --no-typescript --out-dir web/dist "$T/cogs.wasm"
cp "$T/cogs_worklet.wasm" web/dist/worklet.wasm
cp web/index.html web/cogs-web.js web/worklet.js web/dist/
python3 web/single.py web/dist
ls -la web/dist
