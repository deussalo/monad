// Writes cogs/worlds/cogs-dubstep.html: cogs/cogs.html with the dubstep world as its default world.
// node cogs/build.js && node cogs/worlds/make-dubstep.js && node cogs/worlds/make-dubstep-html.js
const fs = require('fs'), path = require('path');
const html = fs.readFileSync(path.join(__dirname, '..', 'cogs.html'), 'utf8');
const world = fs.readFileSync(path.join(__dirname, 'dubstep.json'), 'utf8').trim();
const def = fs.readFileSync(path.join(__dirname, '..', 'src', 'default-world.json'), 'utf8').trim();
if (!html.includes(def)) throw new Error('default world not found in cogs.html (run node cogs/build.js first)');
fs.writeFileSync(path.join(__dirname, 'cogs-dubstep.html'), html.replace(def, () => world));
console.log('wrote cogs-dubstep.html');
