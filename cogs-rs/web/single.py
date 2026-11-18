# One self-contained page from web/dist: the wasm files and worklet.js inline (base64), the scripts in the page.
import base64, json, sys, os
d = sys.argv[1]
r = lambda n, m='r': open(os.path.join(d, n), m).read()
inline = {n: base64.b64encode(r(n, 'rb')).decode() for n in ['cogs_bg.wasm', 'worklet.wasm']}
inline['worklet.js'] = r('worklet.js')
page = r('index.html')
scripts = '<script>window.COGS_INLINE=' + json.dumps(inline) + '</script>\n<script>' + r('cogs.js') + '</script>\n<script>' + r('cogs-web.js') + '</script>'
assert '</script' not in json.dumps(inline) + r('cogs.js') + r('cogs-web.js')
page = page.replace('<script src="cogs.js"></script>\n<script src="cogs-web.js"></script>\n', '').replace('<!--cogs:inline-->', scripts)
open(os.path.join(d, 'cogs.html'), 'w').write(page)
print('cogs.html', os.path.getsize(os.path.join(d, 'cogs.html')) // 1024, 'KB')
