"""Contact sheet of real-display captures from tests/overlay/stage.ps1, beside a reference.

  python -I tests/overlay/stage_sheet.py <label> <out.png> "<title>" [ref.png]

Reads tests/overlay/stage/<label>-{light,dark}-{100,125,150}/<bg>-<n>-<state>.jpg. Rows: theme and
scale, then background; columns: states. Captures are shown at their real pixel size, cropped to the
cursor's corner of the stage.
"""
import os, sys
from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
FONT = ImageFont.truetype(r'C:\Windows\Fonts\segoeui.ttf', 14)
BOLD = ImageFont.truetype(r'C:\Windows\Fonts\seguisb.ttf', 20)
GROUNDS = ['white', 'black', 'photo', 'busy', 'busy-dark']
STATES = ['1-thinking', '2-acting', '3-typing', '4-card', '5-paused', '6-stopped', '7-done', '8-error']


def main():
    label, out, title = sys.argv[1], sys.argv[2], sys.argv[3]
    ref = sys.argv[4] if len(sys.argv) > 4 else None
    rows = []
    for theme in ('light', 'dark'):
        for pct in (100, 125, 150):
            d = os.path.join(HERE, 'stage', f'{label}-{theme}-{pct}')
            if not os.path.isdir(d): continue
            k = pct / 100
            cw, ch = int(360 * k), int(172 * k)   # the stage's top-left part, where the cursor and card are
            for g in GROUNDS:
                cells = []
                for s in STATES:
                    p = os.path.join(d, f'{g}-{s}.jpg')
                    if os.path.exists(p): cells.append(Image.open(p).convert('RGB').crop((0, 0, cw, ch)))
                if cells: rows.append((f'{theme} {pct} % · {g}', cells))
    if not rows: sys.exit('no captures for ' + label)
    cw = max(c.width for _, cs in rows for c in cs) + 8
    W = 200 + len(STATES) * cw
    H = 70 + sum(max(c.height for c in cs) + 8 for _, cs in rows)
    sheet = Image.new('RGB', (W, H), '#ffffff')
    dr = ImageDraw.Draw(sheet)
    dr.text((12, 12), title, fill='#0d0d0d', font=BOLD)
    for j, s in enumerate(STATES): dr.text((200 + j * cw, 44), s.split('-', 1)[1], fill='#6b6b6b', font=FONT)
    y = 70
    for name, cs in rows:
        h = max(c.height for c in cs)
        dr.text((12, y + h // 2 - 9), name, fill='#0d0d0d', font=FONT)
        for j, c in enumerate(cs): sheet.paste(c, (200 + j * cw, y))
        y += h + 8
    if ref:
        rim = Image.open(ref).convert('RGB')
        k = min(1.0, 900 / rim.width)
        rim = rim.resize((int(rim.width * k), int(rim.height * k)), Image.LANCZOS)
        both = Image.new('RGB', (W + rim.width + 32, max(H, rim.height + 70)), '#ffffff')
        both.paste(sheet, (0, 0)); both.paste(rim, (W + 16, 70))
        ImageDraw.Draw(both).text((W + 16, 44), 'reference: ' + os.path.basename(ref), fill='#6b6b6b', font=FONT)
        sheet = both
    sheet.save(out)
    print(out)


if __name__ == '__main__':
    main()
