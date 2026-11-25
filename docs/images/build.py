"""Builds the README images in this folder.

  python docs/images/build.py

- screen-*.png are not built here: they are frames of real recorded runs (tests/demo/frames.py)
- how-it-works-light.svg / -dark.svg: the architecture diagram
- social-preview.png: 1280x640 GitHub social preview, rendered in headless Edge
"""
import os, subprocess, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
EDGE = r'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe'

THEMES = {
    'light': dict(bg='#ffffff', box='#fafafa', line='#d4d4d4', text='#0d0d0d', muted='#5d5d5d', accent='#0f6cbd', tint='#ebf3fc'),
    'dark': dict(bg='#212121', box='#303030', line='#4a4a4a', text='#ececec', muted='#b4b4b4', accent='#6cb2f5', tint='#1d2f42'),
}

def box(x, y, w, h, title, lines, t, reach=False):
    fill, stroke = (t['tint'], t['accent']) if reach else (t['box'], t['line'])
    s = f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="8" fill="{fill}" stroke="{stroke}" stroke-width="1.5"/>'
    s += f'<text x="{x + 14}" y="{y + 26}" font-size="15" font-weight="600" fill="{t["text"]}">{title}</text>'
    for i, l in enumerate(lines):
        s += f'<text x="{x + 14}" y="{y + 48 + i * 18}" font-size="13" fill="{t["muted"]}">{l}</text>'
    return s

def arrow(x1, y1, x2, y2, label, t, lx=None, ly=None, both=False, anchor='middle'):
    s = f'<line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{t["muted"]}" stroke-width="1.5" marker-end="url(#a)"{" marker-start=%s" % chr(34) + "url(#a)" + chr(34) if both else ""}/>'
    if label:
        lx = (x1 + x2) / 2 if lx is None else lx; ly = (y1 + y2) / 2 - 8 if ly is None else ly
        s += f'<text x="{lx}" y="{ly}" font-size="12" fill="{t["muted"]}" text-anchor="{anchor}">{label}</text>'
    return s

def diagram(t):
    W, H = 960, 330
    s = f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" font-family="Inter Variable, Inter, Segoe UI, system-ui, sans-serif">'
    s += f'<title>How Reach works</title><defs><marker id="a" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0 L10 5 L0 10 z" fill="{t["muted"]}"/></marker></defs>'
    s += f'<rect width="{W}" height="{H}" fill="{t["bg"]}"/>'
    s += box(20, 30, 120, 70, 'You', ['type a task,', 'approve or stop'], t)
    s += box(180, 20, 240, 110, 'Reach app', ['timeline and live view', 'approval cards', 'Stop, Ctrl+Alt+Esc', 'run log and replay'], t, True)
    s += box(540, 30, 200, 90, 'Agent CLI', ['Claude Code or Codex,', 'signed in with your', 'own account'], t)
    s += box(800, 30, 140, 90, 'The model', ['Anthropic (Claude)', 'or OpenAI', '(ChatGPT)'], t)
    s += box(180, 200, 240, 110, 'Reach engine', ['approval check before', 'risky presses', 'pause while you use the PC', 'stop at once'], t, True)
    s += box(540, 210, 200, 90, 'Your Windows apps', ['Excel, Notepad, forms,', 'any window'], t)
    s += arrow(140, 65, 178, 65, '', t, both=True)
    s += arrow(422, 75, 538, 75, 'task, replies', t, ly=67)
    s += arrow(742, 75, 798, 75, 'HTTPS', t, ly=67)
    s += arrow(620, 122, 425, 222, 'tool calls (MCP, this PC only)', t, lx=560, ly=172, anchor='start')
    s += arrow(422, 262, 538, 262, '', t)
    s += f'<text x="480" y="236" font-size="12" fill="{t["muted"]}" text-anchor="middle">UI Automation,</text><text x="480" y="252" font-size="12" fill="{t["muted"]}" text-anchor="middle">then mouse, keys</text>'
    s += arrow(300, 132, 300, 198, 'cards, Stop', t, lx=310, ly=170, both=True, anchor='start')
    return s + '</svg>\n'

for name, t in THEMES.items():
    open(os.path.join(HERE, f'how-it-works-{name}.svg'), 'w', encoding='utf-8').write(diagram(t))

# social preview: the white mark and wordmark on a plain near-black banner (no screen, no tile)
import sys
sys.path.insert(0, os.path.join(ROOT, 'tests', 'overlay'))
from pill_sheet import frame as load_frame
wm = open(os.path.join(ROOT, 'assets', 'brand', 'wordmark-dark.svg'), encoding='utf-8').read()
inter = 'file:///' + os.path.join(ROOT, 'app', 'node_modules', '@fontsource-variable', 'inter', 'files', 'inter-latin-wght-normal.woff2').replace(os.sep, '/')
html = f'''<!doctype html><html><head><style>@font-face{{font-family:"Inter Variable";src:url("{inter}") format("woff2");font-weight:100 900}}</style></head><body style="margin:0;width:1280px;height:640px;background:#111111;overflow:hidden;display:flex;align-items:center;justify-content:center">
{wm.replace('width="760" height="256"', 'width="760" height="256" style="margin-left:-20px"', 1)}
</body></html>'''
tmp = tempfile.mkdtemp(); hp = os.path.join(tmp, 's.html'); open(hp, 'w', encoding='utf-8').write(html)
out = os.path.join(HERE, 'social-preview.png')
subprocess.run([EDGE, '--headless=new', '--disable-gpu', '--hide-scrollbars', f'--user-data-dir={tmp}\\p', '--allow-file-access-from-files',
                '--window-size=1280,640', '--force-device-scale-factor=1', f'--screenshot={out}', 'file:///' + hp.replace('\\', '/')],
               check=True, timeout=60, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
print('ok')

# README pill images: the overlay's own frames (pointer + tag) for five states on a page background
from PIL import Image
ROUND = os.environ.get('REACH_PILL_ROUND_README', 'rr2')
for theme, bg in (('light', '#ffffff'), ('dark', '#212121')):
    names = ['3-typing', '4-waiting', '5-paused', '7-done', '8-error']
    fr = [load_frame(os.path.join(ROOT, 'tests', 'overlay', 'pill', f'{ROUND}-cobalt-{theme}-150-{n}.rgba')) for n in names]
    bb = [f.getbbox() for f in fr]; top, bot = min(b[1] for b in bb), max(b[3] for b in bb)
    fr = [f.crop((b[0], top, b[2], bot)) for f, b in zip(fr, bb)]   # pack by what's drawn, one baseline
    pad, gap = 24, 40
    page = Image.new('RGBA', (2 * pad + sum(f.width for f in fr) + gap * (len(fr) - 1), max(f.height for f in fr) + 2 * pad), bg)
    x = pad
    for f in fr:
        page.alpha_composite(f, (x, pad)); x += f.width + gap
    page.convert('RGB').save(os.path.join(HERE, f'pill-{theme}-page.png'))
print('pill pages ok')
