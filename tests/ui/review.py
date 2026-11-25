"""Side-by-side design review sheet: public reference (left) vs our screen (right), one row per pair.
python -I tests/ui/review.py <round-number> <shots-dir>   ->  docs/design-review/round-<n>.png
References live in docs/references (git-ignored), so the sheet is for private comparison too."""
import os, sys
from PIL import Image, ImageDraw, ImageFont

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
REF = os.path.join(ROOT, 'docs', 'references')
PAIRS = [  # (reference, our screenshot, label)
    ('app/river/river-app-start.png', 'start-light.png', 'Start, light: River'),
    ('app/river/river-app-start.png', 'start-dark.png', 'Start, dark: River (light only)'),
    ('app/river/river-app-running.png', 'running-light.png', 'Running: River'),
    ('app/river/river-app-first-step.png', 'running-dark-1600.png', 'Running, dark, 1600: River (light only)'),
    ('app/river/river-app-needs-you.png', 'approval-light.png', 'Approval: River'),
    ('app/river/river-app-needs-you.png', 'approval-light-150.png', 'Approval, 150 % at 900 x 600: River'),
    ('app/river/river-app-running.png', 'done-light.png', 'Done: River (running)'),
    ('claude/claude-settings-light-2.png', 'settings-light.png', 'Settings: Claude'),
]
H = 520

def fit(im):
    return im.resize((round(im.width * H / im.height), H), Image.LANCZOS)

def main(n, shots):
    font = ImageFont.truetype('segoeui.ttf', 20)
    rows = []
    for ref, ours, label in PAIRS:
        a, b = os.path.join(REF, ref), os.path.join(shots, ours)
        if not (os.path.exists(a) and os.path.exists(b)): print('skip', label); continue
        rows.append((label, fit(Image.open(a).convert('RGB')), fit(Image.open(b).convert('RGB'))))
    W = max(r[1].width + r[2].width for r in rows) + 72
    sheet = Image.new('RGB', (W, sum(H + 56 for _ in rows) + 24), '#e8e8e8')
    d = ImageDraw.Draw(sheet); y = 12
    for label, a, b in rows:
        d.text((24, y + 6), f'{label}   |   Reach: right', fill='#0d0d0d', font=font)
        sheet.paste(a, (24, y + 40)); sheet.paste(b, (48 + a.width, y + 40)); y += H + 56
    out = os.path.join(ROOT, 'docs', 'design-review'); os.makedirs(out, exist_ok=True)
    sheet.save(os.path.join(out, f'round-{n}.png')); print('wrote', os.path.join(out, f'round-{n}.png'), sheet.size)

if __name__ == '__main__':
    main(sys.argv[1], sys.argv[2])
