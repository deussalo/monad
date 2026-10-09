// Inlines cogs/src parts into each published page: the artifact sandbox loads one self-contained file.
const fs = require('fs'), path = require('path');
const SRC = path.join(__dirname, 'src'), INCLUDE = /^\s*(?:\/\/|\/\*)@include (\S+?)(?:\*\/)?\s*$/;
const expand = file => fs.readFileSync(path.join(SRC, file), 'utf8').replace(/\n$/, '').split('\n')
  .map(l => { const m = l.match(INCLUDE); return m ? expand(m[1]) : l; }).join('\n');
for (const [page, out] of [['phone.html', 'cogs.html']])
  fs.writeFileSync(path.join(__dirname, out), expand(page) + '\n');
