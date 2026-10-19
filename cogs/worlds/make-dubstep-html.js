// Writes cogs/worlds/cogs-dubstep.html: cogs/cogs.html with the dubstep world as its default world.
// node cogs/build.js && node cogs/worlds/make-dubstep.js && node cogs/worlds/make-dubstep-html.js
const fs = require('fs'), path = require('path');
const html = fs.readFileSync(path.join(__dirname, '..', 'cogs.html'), 'utf8');
const world = fs.readFileSync(path.join(__dirname, 'dubstep.json'), 'utf8').trim();
const def = fs.readFileSync(path.join(__dirname, '..', 'src', 'default-world.json'), 'utf8').trim();
if (!html.includes(def)) throw new Error('default world not found in cogs.html (run node cogs/build.js first)');
// Nord, hardcoded: the page otherwise takes Rig's theme only inside Rig's preview iframe and falls back to grey
// standalone. These set the page's own variables, so they hold in and out of Rig (values from Rig's themes/nord.css).
// Derived shades are Rig's color-mix formulas resolved to hex, since the canvas reads these variables as strings.
const nord = ':root{--bg:#2e3440;--fg:#d8dee9;--dim:#9299a5;--faint:#636975;--line:#565c68;' +
  '--hot:#ebcb8b;--sel:#81a1c1;--bad:#bf616a;--read:#88c0d0;--pad:#464d5d;' +
  '--rig-bg:#2e3440;--rig-surface:#3b4252;--rig-text:#d8dee9}';
if (!html.includes('</style>')) throw new Error('no </style> in cogs.html');
const out = html.replace(def, () => world).replace('</style>', () => nord + '</style>');
fs.writeFileSync(path.join(__dirname, 'cogs-dubstep.html'), out);
console.log('wrote cogs-dubstep.html');
