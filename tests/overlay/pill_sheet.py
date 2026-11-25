"""Puts the raw pill frames from the overlay tests on white, black, a photo and a busy app,
and writes one contact sheet.

  cargo test -p reach-core --lib pill_frames -- --ignored      (writes tests/overlay/pill/*.rgba)
  python -I tests/overlay/pill_sheet.py <prefix> <out.png> "<title>" [ref.png]

Rows are the frames whose name starts with <prefix>, in name order. With ref.png the closest
reference is placed beside the sheet (a side-by-side for the design review).
"""
import os, struct, sys
from PIL import Image, ImageDraw, ImageFont

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
FONT = ImageFont.truetype(r'C:\Windows\Fonts\segoeui.ttf', 14)
BOLD = ImageFont.truetype(r'C:\Windows\Fonts\seguisb.ttf', 20)
PHOTO = r'C:\Windows\Web\Wallpaper\Spotlight\img14.jpg'   # Windows' own stock photo
BUSY = os.path.join(ROOT, 'tests', 'ui', 'done-light.png')  # the app's own screen, lots of text
BUSY_D = os.path.join(ROOT, 'tests', 'ui', 'running-dark.png')


def frame(path):
    b = open(path, 'rb').read()
    w, h = struct.unpack('<II', b[:8])
    px = bytearray(b[8:])
    # premultiplied BGRA -> straight RGBA
    out = bytearray(len(px))
    for i in range(0, len(px), 4):
        bl, g, r, a = px[i], px[i + 1], px[i + 2], px[i + 3]
        if a:
            out[i], out[i + 1], out[i + 2] = min(255, r * 255 // a), min(255, g * 255 // a), min(255, bl * 255 // a)
        out[i + 3] = a
    return Image.frombytes('RGBA', (w, h), bytes(out))


def grounds(w, h, i):
    photo = Image.open(PHOTO).convert('RGBA')
    # a bright part of the photo (the legibility worst case for a light pill) and a mid part
    px, py = (photo.width * 2 // 3, photo.height // 5) if i % 2 == 0 else (photo.width // 3, photo.height // 2)
    busy = Image.open(BUSY if i % 2 == 0 else BUSY_D).convert('RGBA')
    bx, by = (busy.width // 3, 120 + (i * 37) % max(1, busy.height - h - 240))
    return [('white', Image.new('RGBA', (w, h), '#ffffff')), ('black', Image.new('RGBA', (w, h), '#000000')),
            ('photo', photo.crop((px, py, px + w, py + h))), ('busy app', busy.crop((bx, by, bx + w, by + h)))]


def main():
    prefix, out, title = sys.argv[1], sys.argv[2], sys.argv[3]
    ref = sys.argv[4] if len(sys.argv) > 4 else None
    d = os.path.join(HERE, 'pill')
    names = sorted(f for f in os.listdir(d) if f.startswith(prefix) and f.endswith('.rgba'))
    frames = [(n[:-5], frame(os.path.join(d, n))) for n in names]
    if not frames: sys.exit('no frames for ' + prefix)
    cw = max(f.width for _, f in frames) + 16
    rows = []
    for i, (name, f) in enumerate(frames):
        ch = f.height + 16
        row = Image.new('RGBA', (220 + 4 * cw, ch), '#ffffff')
        ImageDraw.Draw(row).text((12, ch // 2 - 9), name, fill='#0d0d0d', font=FONT)
        for j, (_, g) in enumerate(grounds(cw, ch, i)):
            g = g.copy(); g.alpha_composite(f, (8, 8)); row.paste(g, (220 + j * cw, 0))
        rows.append(row)
    W = 220 + 4 * cw
    sheet = Image.new('RGBA', (W, 70 + sum(r.height + 4 for r in rows)), '#ffffff')
    dr = ImageDraw.Draw(sheet)
    dr.text((12, 12), title, fill='#0d0d0d', font=BOLD)
    for j, (label, _) in enumerate(grounds(10, 10, 0)):
        dr.text((220 + j * cw + 8, 44), label, fill='#6b6b6b', font=FONT)
    y = 70
    for r in rows:
        sheet.paste(r, (0, y)); y += r.height + 4
    if ref:
        rim = Image.open(ref).convert('RGBA')
        k = min(1.0, (sheet.height - 70) / rim.height, 900 / rim.width)
        rim = rim.resize((max(1, int(rim.width * k)), max(1, int(rim.height * k))), Image.LANCZOS)
        both = Image.new('RGBA', (sheet.width + rim.width + 32, max(sheet.height, rim.height + 70)), '#ffffff')
        both.paste(sheet, (0, 0)); both.paste(rim, (sheet.width + 16, 70))
        ImageDraw.Draw(both).text((sheet.width + 16, 44), 'reference: ' + os.path.basename(ref), fill='#6b6b6b', font=FONT)
        sheet = both
    os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
    sheet.convert('RGB').save(out)
    print(out)


if __name__ == '__main__':
    main()
