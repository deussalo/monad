// Writes cogs/worlds/cogs-capstan.html: cogs/cogs.html opening the Capstan world, with its own save slot, so a machine
// saved from another cogs page on the same origin cannot open in its place.
// node cogs/build.js && node cogs/worlds/make-capstan.js && node cogs/worlds/make-capstan-html.js
const fs = require('fs'), path = require('path');
const html = fs.readFileSync(path.join(__dirname, '..', 'cogs.html'), 'utf8');
const world = fs.readFileSync(path.join(__dirname, 'capstan.json'), 'utf8').trim();
const def = fs.readFileSync(path.join(__dirname, '..', 'src', 'default-world.json'), 'utf8').trim();
if (!html.includes(def)) throw new Error('default world not found in cogs.html (run node cogs/build.js first)');
let out = html.replace(def, () => world);
const m = out.match(/STORE='([^']+)'/);
if (!m) throw new Error('save slot not found in cogs.html');
out = out.replace(m[0], () => `STORE='${m[1]}.capstan'`);
fs.writeFileSync(path.join(__dirname, 'cogs-capstan.html'), out);
console.log('wrote cogs-capstan.html', out.length, 'bytes; save slot', m[1] + '.capstan');
