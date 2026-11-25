"""Logo workbench: directions as SVG on a 24-unit grid, rendered in headless Edge, judged on
contact sheets, then exported.

  python -I assets/brand/logo.py before          # docs/design-review/logo-before.png (the current mark)
  python -I assets/brand/logo.py round N A B ..  # docs/brand/logo-round-N.png for the named directions
  python -I assets/brand/logo.py final NAME [SMALL] [LIGHT]  # SVG, PNGs, .ico, app icons, wordmarks

Each direction is a glyph in a 24 x 24 box (keylines 2..22) using two colours: INK and ACC
(the one accent). Every render happens in one Edge call per sheet: all SVGs go on one page,
the page is screenshot once, and the cells are cut out.
"""
import os, subprocess, sys, tempfile, shutil
from PIL import Image, ImageDraw, ImageFont, ImageFilter

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
BRAND = os.path.join(ROOT, 'assets', 'brand')
EDGE = r'C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe'
TMP = tempfile.mkdtemp(prefix='reach-logo-')
FONT = r'C:\Windows\Fonts\segoeui.ttf'
FONT_B = r'C:\Windows\Fonts\seguisb.ttf'
INTER = 'file:///' + os.path.join(ROOT, 'app', 'node_modules', '@fontsource-variable', 'inter', 'files', 'inter-latin-wght-normal.woff2').replace(os.sep, '/')

# colours: ink on light, ink on dark, the accent for light and dark grounds, the app tile
INK, INK_D = '#0d0d0d', '#ffffff'
ACC, ACC_D = '#2563ff', '#2563ff'   # the default accent (cobalt, config::accent_rgb), on light and dark
TILE = '#141518'
TILES = {}                            # per-direction tile colour (default TILE)

# the agent's on-screen pointer (overlay.rs CURSOR, with the painter's (0.9, 1.4) offset)
CUR = [(0.9, 1.4), (0.9, 24.2), (7.9, 18.0), (17.8, 17.3)]


def cur(tx, ty, k):
    return ' '.join(f'{tx + x * k:.2f},{ty + y * k:.2f}' for x, y in CUR)


def tag(x, y, w, h, rtl):
    """The pointer's tag: a rounded pill whose top-left corner (where it meets the pointer) is small."""
    r = h / 2
    return (f'M{x + rtl:.2f} {y:.2f} H{x + w - r:.2f} A{r:.2f} {r:.2f} 0 0 1 {x + w - r:.2f} {y + h:.2f} '
            f'H{x + r:.2f} A{r:.2f} {r:.2f} 0 0 1 {x:.2f} {y + h - r:.2f} V{y + rtl:.2f} A{rtl:.2f} {rtl:.2f} 0 0 1 {x + rtl:.2f} {y:.2f} Z')


def pointer(acc, line, k, tx, ty, tg=None, dot=None):
    """Pointer (and tag) in acc with one merged outline in line, like the overlay paints them."""
    o = ''
    if line:
        if tg: o += f'<path d="{tg}" fill="{line}" stroke="{line}" stroke-width="2.6" stroke-linejoin="round"/>'
        o += f'<polygon points="{cur(tx, ty, k)}" fill="{line}" stroke="{line}" stroke-width="{2.6 + 1.6 * k:.2f}" stroke-linejoin="round"/>'
    if tg: o += f'<path d="{tg}" fill="{acc}"/>'
    if dot: o += f'<circle cx="{dot[0]}" cy="{dot[1]}" r="{dot[2]}" fill="{dot[3]}"/>'
    o += f'<polygon points="{cur(tx, ty, k)}" fill="{acc}" stroke="{acc}" stroke-width="{1.6 * k:.2f}" stroke-linejoin="round"/>'
    return o


def kite(tx, ty, s, d=1, a=0.40, b=0.56):
    """The agent's pointer: a kite with its tip at (tx, ty), s units long, pointing up-left
    (d=1) or down-right (d=-1). Kite space: tip (0,0), wings (1,a) and (a,1), notch (b,b)."""
    pts = [(0, 0), (1, a), (b, b), (a, 1)]
    return ' '.join(f'{tx + d * u * s:.2f},{ty + d * v * s:.2f}' for u, v in pts)


def poly(points, fill, round_=0.9):
    # stroke in the fill colour rounds the corners slightly without changing the silhouette much
    return f'<polygon points="{points}" fill="{fill}" stroke="{fill}" stroke-width="{round_}" stroke-linejoin="round"/>'


# ---------- directions (glyph bodies; {ink} {acc} {bg} are filled per render) ----------
DIRS = {
    # 1. Twin: your pointer (outline) and the agent's own (solid) side by side
    'twin': lambda ink, acc, bg: (
        f'<polygon points="{kite(3.2, 3.2, 9.5)}" fill="none" stroke="{ink}" stroke-width="1.7" stroke-linejoin="round"/>'
        + f'<polygon points="{kite(9.2, 9.2, 12.6)}" fill="{acc}" stroke="{bg}" stroke-width="2.6" stroke-linejoin="round" paint-order="stroke"/>'
        + poly(kite(9.2, 9.2, 12.6), acc)),
    # 2. Tip and dot: the agent's pointer with its status dot, the pill in its smallest form
    'tipdot': lambda ink, acc, bg: poly(kite(3.5, 3.5, 13.5), ink) + f'<circle cx="17.6" cy="17.6" r="3.4" fill="{acc}"/>',
    # 3. Monogram: an R whose leg is a pointer reaching down-right
    'monogram': lambda ink, acc, bg: (
        f'<rect x="4.5" y="3" width="4" height="18" rx="1" fill="{ink}"/>'
        + f'<path d="M6.5 5 H12 a4.6 4.6 0 0 1 0 9.2 H6.5" fill="none" stroke="{ink}" stroke-width="4"/>'
        + poly(kite(21, 21, 9.5, -1), acc)),
    # 4. Reach: an arm that sweeps up and ends in the pointer's tip
    'reach': lambda ink, acc, bg: (
        f'<path d="M20.5 21 A12.5 12.5 0 0 1 8.6 9.4" fill="none" stroke="{ink}" stroke-width="3.4" stroke-linecap="round"/>'
        + poly(kite(3, 3, 9.6), acc)),
    # 5. Window: the agent's pointer at work on a window
    'window': lambda ink, acc, bg: (
        f'<rect x="2.6" y="3.6" width="14.8" height="12.6" rx="3" fill="none" stroke="{ink}" stroke-width="1.9"/>'
        + f'<polygon points="{kite(11.4, 10.6, 10.6)}" fill="{acc}" stroke="{bg}" stroke-width="2.6" stroke-linejoin="round" paint-order="stroke"/>'
        + poly(kite(11.4, 10.6, 10.6), acc)),
    # 6. Trail: the pointer moving on its own, leaving two fading dots
    'trail': lambda ink, acc, bg: poly(kite(3, 3, 12.4), ink) + f'<circle cx="16.6" cy="16.6" r="2.3" fill="{acc}"/><circle cx="20.4" cy="20.4" r="1.45" fill="{acc}" opacity="0.55"/>',
    # round 2
    'tipdot-b': lambda ink, acc, bg: poly(kite(3, 3, 14.6, a=0.38, b=0.60), ink) + f'<circle cx="17.9" cy="17.9" r="3.1" fill="{acc}"/>',
    'tippill': lambda ink, acc, bg: poly(kite(3.2, 3, 13.6), ink) + f'<rect x="12.6" y="16.4" width="9.4" height="5.2" rx="2.6" fill="{acc}"/>',
    'tipdot-round': lambda ink, acc, bg: f'<polygon points="{kite(4.2, 4.2, 12.6, a=0.40, b=0.54)}" fill="{ink}" stroke="{ink}" stroke-width="2.4" stroke-linejoin="round"/><circle cx="17.8" cy="17.8" r="3.3" fill="{acc}"/>',
    'monogram-b': lambda ink, acc, bg: (
        f'<rect x="4" y="3" width="4.2" height="18" rx="1.2" fill="{ink}"/>'
        + f'<path d="M6 5.1 H11.4 a4.3 4.3 0 0 1 0 8.6 H6" fill="none" stroke="{ink}" stroke-width="4.2"/>'
        + f'<polygon points="{kite(21.4, 21.4, 10.4, -1)}" fill="{acc}" stroke="{bg}" stroke-width="2.2" stroke-linejoin="round" paint-order="stroke"/>' + poly(kite(21.4, 21.4, 10.4, -1), acc)),
    # round 3: tipdot-b refined (dot sits on the pointer's axis, a hair of gap) and the pill version
    'tipdot-c': lambda ink, acc, bg: poly(kite(2.6, 2.6, 15.2, a=0.38, b=0.60), ink, 1.2) + f'<circle cx="18.4" cy="18.4" r="3.2" fill="{acc}"/>',
    'tippill-b': lambda ink, acc, bg: poly(kite(2.6, 2.4, 14.4, a=0.38, b=0.60), ink, 1.2) + f'<rect x="11.8" y="16.6" width="10.6" height="5.4" rx="2.7" fill="{acc}"/>',
    # round 4 (cursor redesign): the logo is the agent's own pointer, in its teal
    # A. the pointer alone, teal with a white outline, on the dark tile
    'pointer': lambda ink, acc, bg: pointer(acc, '#ffffff' if bg != '#ffffff' else '', 0.80, 4.6, 1.6),
    # B. the pointer and its tag as one object, the way it appears on screen
    'pointer-tag': lambda ink, acc, bg: pointer(acc, '#ffffff' if bg != '#ffffff' else '', 0.62, 2.4, 1.6,
                                                tag(8.6, 13.0, 13.4, 7.6, 1.3), (12.4, 16.8, 1.25, '#ffffff')),
    # C. the same object in white on a teal tile
    'teal-tile': lambda ink, acc, bg: pointer('#ffffff' if bg != '#ffffff' else acc, '', 0.62, 2.4, 1.6,
                                              tag(8.6, 13.0, 13.4, 7.6, 1.3), (12.4, 16.8, 1.25, ACC if bg != '#ffffff' else '#ffffff')),
    # round 5: the tag clear of the pointer's notch, a little lower and shorter, a larger dot
    'teal-tile-b': lambda ink, acc, bg: pointer('#ffffff' if bg != '#ffffff' else acc, '', 0.66, 2.6, 1.4,
                                                tag(9.8, 13.9, 12.4, 7.4, 1.2), (13.5, 17.6, 1.4, ACC if bg != '#ffffff' else '#ffffff')),
}
TILES['teal-tile'] = TILES['teal-tile-b'] = ACC


# ---------- 2026-11-25: a hand reaching (six directions, then refinements) ----------
# Each hand is drawn pointing straight up in a local frame, then turned 45 degrees so it reaches
# up and to the right, toward a small target (the agent's pointer or a dot) near the top-right corner.
def cap(x1, y1, x2, y2, w, c):
    return f'<path d="M{x1} {y1} L{x2} {y2}" stroke="{c}" stroke-width="{w}" stroke-linecap="round" fill="none"/>'


def turned(body, deg=45, cx=12, cy=13):
    return f'<g transform="rotate({deg} {cx} {cy})">{body}</g>'


def tgt(c, ink, bg):
    """Target colour: the accent, or the ink when the ground is already the accent (the app tile)."""
    return ink if bg == ACC else c


def h_palm(c):      # open palm, fingers together, thumb out
    f = ''.join(cap(x, 12, x, top, 2.5, c) for x, top in ((8.25, 6.4), (10.75, 4.6), (13.25, 5.0), (15.75, 6.8)))
    return f + f'<rect x="7" y="10.5" width="10" height="9" rx="3.2" fill="{c}"/>' + cap(7.6, 16.4, 4.6, 12.6, 2.6, c) + f'<rect x="8.6" y="18" width="6.8" height="9" fill="{c}"/>'


def h_point(c):     # a fist with the index finger out
    return (f'<rect x="7" y="11" width="10" height="8.4" rx="3.6" fill="{c}"/>' + cap(8.8, 12, 8.8, 3.4, 3.2, c)
            + f'<rect x="8.6" y="18" width="6.8" height="9" fill="{c}"/>')


def h_pinch(c):     # thumb and index finger open like a beak
    return (f'<path d="M7 19 C5.2 14 6 8.6 9.6 4.6" stroke="{c}" stroke-width="3" stroke-linecap="round" fill="none"/>'
            + f'<path d="M15.4 19 C16.2 15 15.2 11.2 12.6 8.6" stroke="{c}" stroke-width="3" stroke-linecap="round" fill="none"/>'
            + f'<rect x="7" y="15.4" width="9.4" height="6" rx="3" fill="{c}"/><rect x="8.4" y="20" width="6.6" height="8" fill="{c}"/>')


def h_blocks(c):    # a hand built from rounded rectangles
    f = ''.join(f'<rect x="{x}" y="{t}" width="2.4" height="{12.5 - t}" rx="1.2" fill="{c}"/>' for x, t in ((7, 6.6), (9.6, 4.2), (12.2, 4.8), (14.8, 7.2)))
    return f + f'<rect x="7" y="11" width="10.2" height="8.6" rx="2" fill="{c}"/><rect x="3.4" y="12.2" width="2.4" height="6" rx="1.2" transform="rotate(-35 4.6 15.2)" fill="{c}"/><rect x="8.6" y="18.6" width="7" height="9" rx="1" fill="{c}"/>'


def dot(c, x=20.4, y=3.6, r=2.2):
    return f'<circle cx="{x}" cy="{y}" r="{r}" fill="{c}"/>'


def mini_pointer(c, tx=16.4, ty=1.2, k=0.30):
    return f'<polygon points="{cur(tx, ty, k)}" fill="{c}" stroke="{c}" stroke-width="0.9" stroke-linejoin="round"/>'


DIRS.update({
    'hand-palm': lambda ink, acc, bg: turned(h_palm(ink), cx=11, cy=14) + dot(tgt(acc, ink, bg)),
    'hand-point': lambda ink, acc, bg: turned(h_point(ink), cx=11, cy=14) + mini_pointer(tgt(acc, ink, bg)),
    'hand-pinch': lambda ink, acc, bg: turned(h_pinch(ink), cx=11, cy=14) + dot(tgt(acc, ink, bg), 18.6, 5.2, 1.9),
    'hand-negative': lambda ink, acc, bg: (f'<rect x="1.5" y="1.5" width="21" height="21" rx="6" fill="{ink}"/>'
                                           + f'<g>{turned(h_point(bg if bg != "none" else "#ffffff"), cx=11, cy=15)}</g>' + dot(bg if bg != "none" else "#ffffff", 18.6, 5.4, 1.8)),
    'hand-arm': lambda ink, acc, bg: (cap(-2, 26, 8.4, 15.6, 6.4, ink) + turned(h_palm(ink).replace('height="9" fill', 'height="3" fill'), cx=12.6, cy=11.6)
                                      + dot(tgt(acc, ink, bg), 21, 3, 2)),
    'hand-blocks': lambda ink, acc, bg: turned(h_blocks(ink), cx=11, cy=14) + dot(tgt(acc, ink, bg)),
})
for _n in ('hand-palm', 'hand-point', 'hand-pinch', 'hand-negative', 'hand-arm', 'hand-blocks'): TILES[_n] = ACC


# round 2: fingers fanned a little, a shorter thumb, the hand smaller with a clear gap to its target
def h_palm2(c, wrist=6.0):
    f = ''
    for ang, top in ((-13, 6.2), (-4.5, 4.4), (4.5, 4.8), (13, 6.6)):
        f += f'<g transform="rotate({ang} 12 14)">{cap(12, 12.5, 12, top, 2.55, c)}</g>'
    return (f + f'<rect x="7.4" y="10.6" width="9.2" height="8.6" rx="3.4" fill="{c}"/>'
            + cap(8.2, 16.6, 5.2, 13.4, 2.6, c) + f'<rect x="8.9" y="17.6" width="6.2" height="{wrist}" rx="1" fill="{c}"/>')


def h_point2(c):
    return (f'<rect x="7.2" y="11.2" width="9.6" height="8.4" rx="3.8" fill="{c}"/>' + cap(9.2, 12.4, 9.2, 3.6, 2.7, c)
            + cap(7.8, 14.6, 11.6, 14.6, 2.2, c)  # the thumb across the fist
            + f'<rect x="8.9" y="18.6" width="6.2" height="6" rx="1" fill="{c}"/>')


DIRS.update({
    'palm-b': lambda ink, acc, bg: turned(h_palm2(ink), cx=11.2, cy=14.6) + dot(tgt(acc, ink, bg), 20.2, 3.8, 2.3),
    'palm-cursor': lambda ink, acc, bg: turned(h_palm2(ink), cx=10.8, cy=15) + mini_pointer(tgt(acc, ink, bg), 16.8, 1.0, 0.27),
    'point-b': lambda ink, acc, bg: turned(h_point2(ink), cx=10.4, cy=15.4) + dot(tgt(acc, ink, bg), 20.0, 4.0, 2.3),
    'arm-b': lambda ink, acc, bg: cap(-3, 27, 7.6, 16.4, 5.4, ink) + turned(h_palm2(ink, 2), cx=12.4, cy=12) + dot(tgt(acc, ink, bg), 21, 3, 2.1),
})
for _n in ('palm-b', 'palm-cursor', 'point-b', 'arm-b'): TILES[_n] = ACC


# round 3: back to the round-1 palm (it reads as a hand); fingers a hair apart, a shorter thumb,
# a clear gap to the target. The arm versions enter from the corner and are clipped by the tile.
def h_palm3(c, wrist=10):
    f = ''.join(cap(x, 12, x, top, 2.35, c) for x, top in ((8.3, 6.6), (10.85, 4.7), (13.4, 5.1), (15.95, 7.0)))
    return (f + f'<rect x="7.1" y="10.6" width="10.05" height="8.8" rx="3.2" fill="{c}"/>' + cap(7.7, 16.2, 5.3, 13.0, 2.5, c)
            + f'<rect x="8.7" y="18" width="6.7" height="{wrist}" fill="{c}"/>')


DIRS.update({
    'palm-c': lambda ink, acc, bg: turned(h_palm3(ink), cx=10.4, cy=15.0) + dot(tgt(acc, ink, bg), 20.6, 3.4, 2.1),
    'palm-c-ball': lambda ink, acc, bg: turned(h_palm3(ink), cx=10.0, cy=15.4) + dot(tgt(acc, ink, bg), 20.2, 3.8, 2.7),
    'arm-c': lambda ink, acc, bg: turned(h_palm3(ink, 16), cx=11.0, cy=14.0) + dot(tgt(acc, ink, bg), 20.8, 3.2, 2.1),
    'arm-cursor': lambda ink, acc, bg: turned(h_palm3(ink, 16), cx=10.6, cy=14.6) + mini_pointer(tgt(acc, ink, bg), 17.2, 0.6, 0.26),
})
for _n in ('palm-c', 'palm-c-ball', 'arm-c', 'arm-cursor'): TILES[_n] = ACC


# round 4: the 16-24 px versions keep only the silhouette (fingers as one mitten, no gaps)
def h_mitten(c, wrist=16):
    return (f'<rect x="7.1" y="4.9" width="10.05" height="14.5" rx="4.2" fill="{c}"/>' + cap(7.7, 16.2, 5.1, 12.6, 2.9, c)
            + f'<rect x="8.7" y="18" width="6.7" height="{wrist}" fill="{c}"/>')


DIRS.update({
    'arm-d': lambda ink, acc, bg: turned(h_palm3(ink, 16), cx=11.0, cy=14.4) + dot(tgt(acc, ink, bg), 20.6, 3.4, 2.3),
    'arm-d-small': lambda ink, acc, bg: turned(h_mitten(ink), cx=11.0, cy=14.4) + dot(tgt(acc, ink, bg), 20.4, 3.6, 2.7),
    'palm-d-small': lambda ink, acc, bg: turned(h_mitten(ink, 10), cx=10.4, cy=15.0) + dot(tgt(acc, ink, bg), 20.4, 3.6, 2.6),
})
for _n in ('arm-d', 'arm-d-small', 'palm-d-small'): TILES[_n] = ACC


# Round 2: the hand traced from docs/brand/concepts/concept-final.png (assets/brand/trace.py -> hand.json).
# One hand reaching toward a point; "small" closes the finger gaps for 16-24 px.
import json as _json
_H = _json.load(open(os.path.join(BRAND, 'hand.json')))


def hand(c, small=False, dot_c=None):
    cx, cy, r = _H['dot']
    return f'<path d="{_H["small" if small else "hand"]}" fill="{c}"/><circle cx="{cx}" cy="{cy}" r="{max(r, 1.9) if small else r}" fill="{dot_c or c}"/>'


WHITE_TILE = '#ffffff'
DIRS.update({
    'hand-a': lambda ink, acc, bg: hand(ACC),                       # accent hand on the near-black tile
    'hand-a-s': lambda ink, acc, bg: hand(ACC, True),
    'hand-b': lambda ink, acc, bg: hand(ACC),                       # accent hand on a white tile
    'hand-b-s': lambda ink, acc, bg: hand(ACC, True),
    'hand-c': lambda ink, acc, bg: hand('#ffffff' if bg != '#ffffff' else ACC),   # white hand on an accent tile
    'hand-c-s': lambda ink, acc, bg: hand('#ffffff' if bg != '#ffffff' else ACC, True),
})
TILES.update({'hand-b': WHITE_TILE, 'hand-b-s': WHITE_TILE, 'hand-c': ACC, 'hand-c-s': ACC})

CURRENT = '<path d="M0 0 V158 L40 120 L66 178 L94 166 L68 108 H122 Z" transform="translate(80 48) scale(.9)" fill="#fff"/>'


def glyph_svg(name, size, ink, acc, bg='none', pad=0):
    """The bare glyph (no tile) in a size x size SVG; bg only matters for knockout strokes."""
    body = DIRS[name](ink, acc, bg if bg != 'none' else '#ffffff')
    k = (size - 2 * pad) / 24
    rect = f'<rect width="{size}" height="{size}" fill="{bg}"/>' if bg != 'none' else ''
    return f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 {size} {size}">{rect}<g transform="translate({pad} {pad}) scale({k})">{body}</g></svg>'


def tile_svg(name, size, radius=0.22, inset=None):
    if inset is None: inset = 0.10 if size <= 24 else 0.13 if size <= 48 else 0.17  # small icons: glyph fills more of the tile
    """App icon: the glyph in white and the light accent on the dark tile."""
    body = DIRS[name](INK_D, ACC_D, TILES.get(name, TILE)) if name != 'current' else None
    r = size * radius
    if name == 'current':
        return f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 256 256"><rect width="256" height="256" rx="56" fill="#0d0d0d"/>{CURRENT}</svg>'
    p = size * inset; k = (size - 2 * p) / 24
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 {size} {size}">'
            f'<rect width="{size}" height="{size}" rx="{r:.2f}" fill="{TILES.get(name, TILE)}"/>'
            f'<rect x="0.5" y="0.5" width="{size - 1}" height="{size - 1}" rx="{r - 0.5:.2f}" fill="none" stroke="#ffffff" stroke-opacity="0.08"/>'
            f'<clipPath id="t{name}{size}"><rect width="{size}" height="{size}" rx="{r:.2f}"/></clipPath>'
            f'<g clip-path="url(#t{name}{size})"><g transform="translate({p:.2f} {p:.2f}) scale({k:.4f})">{body}</g></g></svg>')


def render_many(svgs):
    """[(svg_text, w, h)] -> [PIL RGBA], one Edge screenshot for all of them."""
    gap, x, y, row_h, W = 8, 0, 0, 0, 2400
    placed = []
    for s, w, h in svgs:
        if x + w > W: x, y, row_h = 0, y + row_h + gap, 0
        placed.append((s, x, y, w, h)); x += w + gap; row_h = max(row_h, h)
    H = max(200, y + row_h + gap)  # headless Edge stalls on very short windows
    html = os.path.join(TMP, 'sheet.html'); png = os.path.join(TMP, 'sheet.png')
    cells = ''.join(f'<div style="position:absolute;left:{x}px;top:{y}px;width:{w}px;height:{h}px;line-height:0">{s}</div>' for s, x, y, w, h in placed)
    open(html, 'w', encoding='utf-8').write(f'<!doctype html><html><head><style>@font-face{{font-family:"Inter Variable";src:url("{INTER}") format("woff2");font-weight:100 900}}html,body{{margin:0;background:transparent}}</style></head><body>{cells}</body></html>')
    if os.path.exists(png): os.remove(png)
    subprocess.run([EDGE, '--headless=new', '--disable-gpu', '--hide-scrollbars', f'--user-data-dir={TMP}\\prof', '--default-background-color=00000000',
                    f'--window-size={W},{H}', '--force-device-scale-factor=1', f'--screenshot={png}', 'file:///' + html.replace('\\', '/')],
                   check=True, timeout=120, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    sheet = Image.open(png).convert('RGBA')
    return [sheet.crop((x, y, x + w, y + h)) for _, x, y, w, h in placed]


def font(px, bold=False):
    return ImageFont.truetype(FONT_B if bold else FONT, px)


def big(im, k):
    return im.resize((im.width * k, im.height * k), Image.NEAREST)


def on(bg, im, xy):
    bg.alpha_composite(im, xy)


def row_for(name, label):
    """One contact-sheet row for a direction: sizes, one colour, taskbars, title bar, installer, social card."""
    S = [(tile_svg(name, n), n, n) for n in (256, 64, 48, 32, 24, 16)]
    if name != 'current':
        S += [(glyph_svg(name, 64, INK, INK, '#ffffff', 4), 64, 64), (glyph_svg(name, 64, INK_D, INK_D, '#1f1f1f', 4), 64, 64),
              (glyph_svg(name, 64, INK, ACC, '#ffffff', 4), 64, 64), (glyph_svg(name, 64, INK_D, ACC_D, '#1f1f1f', 4), 64, 64),
              (glyph_svg(name, 16, INK, ACC, '#ffffff', 1), 16, 16), (glyph_svg(name, 16, INK_D, ACC_D, '#202020', 1), 16, 16)]
    r = render_many(S)
    t256, t64, t48, t32, t24, t16 = r[:6]
    row = Image.new('RGBA', (1880, 300), '#ffffff')
    d = ImageDraw.Draw(row)
    d.text((16, 10), label, fill='#0d0d0d', font=font(18, True))
    on(row, t256, (16, 36))
    x = 290
    for im in (t64, t48, t32, t24, t16):
        on(row, im, (x, 36 + 64 - im.height)); x += im.width + 14
    on(row, big(t16, 4), (290, 120)); on(row, big(t32, 2), (370, 120))
    d.text((290, 192), '16 px x4   32 px x2', fill='#6b6b6b', font=font(13))
    if name != 'current':
        x = 470
        for im in r[6:10]: on(row, im, (x, 36)); x += 72
        d.text((470, 104), 'one colour        accent', fill='#6b6b6b', font=font(13))
        g16l, g16d = r[10], r[11]
    else:
        g16l = g16d = t16
    # taskbars: Windows 11 dark and light, 48 px tall, a 24 px icon among neutral placeholders
    for i, (bar, ph) in enumerate((('#202020', '#3a3a3a'), ('#eeeeee', '#cfcfcf'))):
        tb = Image.new('RGBA', (300, 48), bar); td = ImageDraw.Draw(tb)
        for k in range(4): td.rounded_rectangle((20 + k * 44, 12, 44 + k * 44, 36), 6, fill=ph)
        on(tb, t24, (20 + 4 * 44, 12))
        td.rounded_rectangle((20 + 4 * 44 + 8, 42, 20 + 4 * 44 + 16, 45), 2, fill='#4ea3f1' if i == 0 else '#0f6cbd')
        # tray, 16 px
        on(tb, t16, (276, 16))
        on(row, tb, (470, 130 + i * 56))
    # title bar
    tbar = Image.new('RGBA', (330, 40), '#fafafa'); dd = ImageDraw.Draw(tbar)
    on(tbar, t16, (12, 12)); dd.text((36, 10), 'Reach', fill='#0d0d0d', font=font(14))
    for k, ch in enumerate(('\u2014', '\u25a1', '\u2715')): dd.text((240 + k * 30, 10), ch, fill='#555555', font=font(13))
    on(row, tbar, (800, 36))
    tbar2 = Image.new('RGBA', (330, 40), '#202020'); dd = ImageDraw.Draw(tbar2)
    on(tbar2, t16, (12, 12)); dd.text((36, 10), 'Reach', fill='#ffffff', font=font(14))
    on(row, tbar2, (800, 84))
    # installer: the setup exe icon in Explorer (48 px) with its name
    inst = Image.new('RGBA', (160, 110), '#ffffff'); di = ImageDraw.Draw(inst)
    on(inst, t48, (56, 8)); di.text((80, 64), 'Reach_0.3.0_x64', fill='#0d0d0d', font=font(12), anchor='ma'); di.text((80, 80), '-setup.exe', fill='#0d0d0d', font=font(12), anchor='ma')
    on(row, inst, (800, 134))
    # social preview, 1280 x 640 shown at a quarter
    soc = Image.new('RGBA', (1280, 640), '#0d0d0d'); ds = ImageDraw.Draw(soc)
    t200 = t256.resize((200, 200), Image.LANCZOS); on(soc, t200, (150, 220))
    ds.text((400, 240), 'Reach', fill='#ffffff', font=font(110, True))
    ds.text((404, 380), 'Lets an AI agent use your Windows apps, step by step.', fill='#b4b4b4', font=font(34))
    soc = soc.resize((320, 160), Image.LANCZOS)
    on(row, soc, (1150, 36))
    d.text((1150, 204), 'social preview (1280 x 640 at 1/4)', fill='#6b6b6b', font=font(13))
    # glyph at 16 on light/dark, magnified
    if name != 'current':
        on(row, big(g16l, 4), (1500, 36)); on(row, big(g16d, 4), (1580, 36))
        d.text((1500, 104), 'glyph 16 px x4', fill='#6b6b6b', font=font(13))
    return row


def sheet(rows, title, out):
    W = 1880; H = 60 + sum(r.height for r in rows)
    im = Image.new('RGBA', (W, H), '#ffffff'); d = ImageDraw.Draw(im)
    d.text((16, 16), title, fill='#0d0d0d', font=font(22, True))
    y = 60
    for r in rows:
        im.alpha_composite(r, (0, y)); d.line((0, y, W, y), fill='#e5e5e5'); y += r.height
    os.makedirs(os.path.dirname(out), exist_ok=True)
    im.convert('RGB').save(out)
    print(out)


def final(name, small=None, light=None):
    """Export the winner: SVGs, PNGs, .ico, app icons, wordmarks, and keep the old files.
    `small` is the silhouette-only version used at 24 px and below; `light` is the version for light
    backgrounds (the light wordmark in the README header, mark-light.svg)."""
    small = small or name
    light = light or name
    old = os.path.join(BRAND, 'old', 'v5-round1')
    if not os.path.exists(old):
        os.makedirs(old)
        for f in ('mark.svg', 'mark-small.svg', 'glyph.svg', 'glyph-dark.svg', 'reach.ico', 'wordmark-dark.svg', 'wordmark-light.svg'):
            if os.path.exists(os.path.join(BRAND, f)): shutil.copy2(os.path.join(BRAND, f), old)
        shutil.copytree(os.path.join(BRAND, 'png'), os.path.join(old, 'png'))
        shutil.copytree(os.path.join(ROOT, 'app', 'src-tauri', 'icons'), os.path.join(old, 'app-icons'))
    open(os.path.join(BRAND, 'mark.svg'), 'w', encoding='utf-8').write(tile_svg(name, 256) + '\n')
    open(os.path.join(BRAND, 'mark-light.svg'), 'w', encoding='utf-8').write(tile_svg(light, 256) + '\n')
    open(os.path.join(BRAND, 'glyph.svg'), 'w', encoding='utf-8').write(glyph_svg(name, 24, INK, ACC) + '\n')
    open(os.path.join(BRAND, 'glyph-dark.svg'), 'w', encoding='utf-8').write(glyph_svg(name, 24, INK_D, ACC_D) + '\n')
    sizes = [16, 20, 24, 32, 48, 64, 128, 256, 512]
    ims = render_many([(tile_svg(small if n <= 24 else name, n), n, n) for n in sizes])
    open(os.path.join(BRAND, 'mark-small.svg'), 'w', encoding='utf-8').write(tile_svg(small, 24) + chr(10))
    os.makedirs(os.path.join(BRAND, 'png'), exist_ok=True)
    for n, im in zip(sizes, ims): im.save(os.path.join(BRAND, 'png', f'reach-{n}.png'))
    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    by = dict(zip(sizes, ims))
    by[256].save(os.path.join(BRAND, 'reach.ico'), sizes=[(n, n) for n in ico_sizes], append_images=[by[n] for n in ico_sizes if n != 256])
    icons = os.path.join(ROOT, 'app', 'src-tauri', 'icons')
    by[32].save(os.path.join(icons, '32x32.png')); by[128].save(os.path.join(icons, '128x128.png'))
    by[256].save(os.path.join(icons, '128x128@2x.png')); by[512].save(os.path.join(icons, 'icon.png'))
    shutil.copy2(os.path.join(BRAND, 'reach.ico'), os.path.join(icons, 'icon.ico'))
    # CLI tray icon: shown at 16 px, so the small silhouette even in the 32 px file
    render_many([(tile_svg(small, 32), 32, 32)])[0].save(os.path.join(ROOT, 'assets', 'reach-32.png'))
    by[256].save(os.path.join(ROOT, 'assets', 'reach-256.png'))
    # wordmarks: the tile and "Reach" in Inter 600
    for dark in (False, True):
        ink = '#ececec' if dark else INK
        body = tile_svg(name if dark else light, 256).split('>', 1)[1].rsplit('</svg>', 1)[0]
        wm = (f'<svg xmlns="http://www.w3.org/2000/svg" width="760" height="256" viewBox="0 0 760 256">{body}'
              f'<text x="300" y="173" font-family="\'Inter Variable\',\'Inter\',\'Segoe UI\',system-ui,sans-serif" font-weight="600" font-size="150" letter-spacing="-4" fill="{ink}">Reach</text></svg>')
        open(os.path.join(BRAND, f'wordmark-{"dark" if dark else "light"}.svg'), 'w', encoding='utf-8').write(wm + '\n')
        render_many([(wm.replace('width="760" height="256"', 'width="1520" height="512"', 1), 1520, 512)])[0].save(os.path.join(BRAND, 'png', f'wordmark-{"dark" if dark else "light"}.png'))
    # the social preview is built by docs/images/build.py (it needs a real UI crop and the pill)
    shutil.copy2(os.path.join(ROOT, 'docs', 'images', 'social-preview.png'), os.path.join(BRAND, 'png', 'social-preview.png'))
    print('exported', name)



# Round 4 (user decision, 2026-11-25): the pointing hand from concept-v3.png, no tile, one colour for hand
# and dot that follows the surface: near-black #111 on light, white on dark. 16 px: hand only (no dot).
# Icons that can't follow the theme (exe, installer, shortcut) use STATIC (see the test sheet).
DARK_INK, WHITE, GREY = '#111111', '#ffffff', '#8a8a8a'
COBALT = '#2563ff'  # old marks only


def mark_svg(size, c, small=None, dot=None, outline=None):
    """The bare mark in a size x size SVG (transparent). small: curled fingers closed (24 px and below).
    dot: from 24 px up. outline: a thin outline colour (about 0.8 px at any size), drawn outside the shape."""
    small = size <= 24 if small is None else small
    dot = size > 16 if dot is None else dot
    cx, cy, r = _H['dot']
    st = ''
    if outline:
        sw = 2 * max(0.8, size / 96) * 20.8 / size   # 0.8 px at small sizes, thicker on big icons
        st = f' stroke="{outline}" stroke-width="{sw:.3f}" stroke-linejoin="round" paint-order="stroke"'
    body = f'<path d="{_H["small" if small else "hand"]}" fill="{c}" fill-rule="evenodd"{st}/>'
    if dot: body += f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="{c}"{st}/>'
    return f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="1.6 1.6 20.8 20.8">{body}</svg>'


STATIC_CHOICES = [('white, thin #111 outline', WHITE, DARK_INK), ('mid grey #8a8a8a', GREY, None)]


def bare_sheet(out):
    """Test sheet: each version at 16, 24, 32 and 256 px on light (#f3f3f3) and dark (#202020) taskbars."""
    variants = [('light surfaces: #111', DARK_INK, None), ('dark surfaces: white', WHITE, None)] + \
               [(f'static: {l}', c, o) for l, c, o in STATIC_CHOICES]
    sizes = [16, 24, 32, 256]
    ims = render_many([(mark_svg(n, c, outline=o), n, n) for _, c, o in variants for n in sizes])
    W, rowh = 1500, 300
    im = Image.new('RGBA', (W, 60 + rowh * len(variants)), '#ffffff'); dr = ImageDraw.Draw(im)
    dr.text((16, 16), 'Logo: 16 / 24 / 32 px (and x4) and 256 px on light and dark taskbars', fill='#0d0d0d', font=font(22, True))
    for i, (label, *_) in enumerate(variants):
        y = 60 + i * rowh
        dr.text((16, y + 8), label, fill='#0d0d0d', font=font(16, True))
        for j, bg in enumerate(('#f3f3f3', '#202020')):
            panel = Image.new('RGBA', (720, 256), bg)
            x = 12
            for k, n in enumerate(sizes[:3]):
                m = ims[i * 4 + k]
                panel.alpha_composite(m, (x, 16)); panel.alpha_composite(big(m, 4), (x, 64)); x += max(n * 4, 40) + 20
            panel.alpha_composite(ims[i * 4 + 3].resize((200, 200), Image.LANCZOS), (500, 28))
            im.alpha_composite(panel, (16 + j * 740, y + 36))
    im.convert('RGB').save(out); print(out)


def bare(static=0):
    """Export the mark: SVGs (light and dark), theme PNGs for the tray and taskbar, the static .ico and
    app icons (STATIC_CHOICES[static]), and the wordmarks. Keeps the previous files in old/v7-concept-final."""
    old = os.path.join(BRAND, 'old', 'v7-concept-final')
    if not os.path.exists(old):
        os.makedirs(old)
        for f in ('mark.svg', 'mark-light.svg', 'mark-dark.svg', 'mark-small.svg', 'mark-small-dark.svg', 'glyph.svg', 'glyph-dark.svg',
                  'reach.ico', 'wordmark-dark.svg', 'wordmark-light.svg'):
            if os.path.exists(os.path.join(BRAND, f)): shutil.copy2(os.path.join(BRAND, f), old)
        shutil.copytree(os.path.join(BRAND, 'png'), os.path.join(old, 'png'))
        shutil.copytree(os.path.join(ROOT, 'app', 'src-tauri', 'icons'), os.path.join(old, 'app-icons'))
        for f in ('reach-mark.svg', 'reach-mark-dark.svg'):
            shutil.copy2(os.path.join(ROOT, 'app', 'src', 'assets', 'brand', f), os.path.join(old, 'app-' + f))
        for f in ('reach-32.png', 'reach-256.png'): shutil.copy2(os.path.join(ROOT, 'assets', f), old)
    w = lambda f, t: open(os.path.join(BRAND, f), 'w', encoding='utf-8').write(t + '\n')
    w('mark-light.svg', mark_svg(256, DARK_INK)); w('mark-dark.svg', mark_svg(256, WHITE))
    w('mark.svg', mark_svg(256, DARK_INK)); w('mark-small.svg', mark_svg(24, DARK_INK)); w('mark-small-dark.svg', mark_svg(24, WHITE))
    w('glyph.svg', mark_svg(24, DARK_INK)); w('glyph-dark.svg', mark_svg(24, WHITE))
    # the app's own mark (sidebar, About): 24 px drawing, shown at 20-32 px
    app = os.path.join(ROOT, 'app', 'src', 'assets', 'brand')
    open(os.path.join(app, 'reach-mark.svg'), 'w', encoding='utf-8').write(mark_svg(24, DARK_INK) + '\n')
    open(os.path.join(app, 'reach-mark-dark.svg'), 'w', encoding='utf-8').write(mark_svg(24, WHITE) + '\n')
    os.makedirs(os.path.join(BRAND, 'png'), exist_ok=True)
    # tray and taskbar: one PNG per taskbar theme (the light taskbar gets the #111 mark)
    tsizes = [16, 20, 24, 32, 48]
    tims = render_many([(mark_svg(n, c), n, n) for c in (DARK_INK, WHITE) for n in tsizes])
    for i, theme in enumerate(('light', 'dark')):
        for n, im in zip(tsizes, tims[i * len(tsizes):(i + 1) * len(tsizes)]):
            im.save(os.path.join(BRAND, 'png', f'tray-{theme}-{n}.png'))
    # static icons: the version that reads on both taskbars
    _, sc, so = STATIC_CHOICES[static]
    sizes = [16, 20, 24, 32, 48, 64, 128, 256, 512]
    ims = render_many([(mark_svg(n, sc, outline=so), n, n) for n in sizes])
    for n, im in zip(sizes, ims): im.save(os.path.join(BRAND, 'png', f'reach-{n}.png'))
    by = dict(zip(sizes, ims)); ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    by[256].save(os.path.join(BRAND, 'reach.ico'), sizes=[(n, n) for n in ico_sizes], append_images=[by[n] for n in ico_sizes if n != 256])
    icons = os.path.join(ROOT, 'app', 'src-tauri', 'icons')
    by[32].save(os.path.join(icons, '32x32.png')); by[128].save(os.path.join(icons, '128x128.png'))
    by[256].save(os.path.join(icons, '128x128@2x.png')); by[512].save(os.path.join(icons, 'icon.png'))
    shutil.copy2(os.path.join(BRAND, 'reach.ico'), os.path.join(icons, 'icon.ico'))
    by[32].save(os.path.join(ROOT, 'assets', 'reach-32.png')); by[256].save(os.path.join(ROOT, 'assets', 'reach-256.png'))
    # wordmarks: the mark and "Reach" in Inter 600, transparent; light = #111, dark = white
    for dark in (False, True):
        ink = WHITE if dark else DARK_INK
        body = mark_svg(256, ink).split('>', 1)[1].rsplit('</svg>', 1)[0]
        wm = (f'<svg xmlns="http://www.w3.org/2000/svg" width="760" height="256" viewBox="0 0 760 256">'
              f'<svg x="0" y="0" width="256" height="256" viewBox="1.6 1.6 20.8 20.8">{body}</svg>'
              f'<text x="290" y="173" font-family="\'Inter Variable\',\'Inter\',\'Segoe UI\',system-ui,sans-serif" font-weight="600" font-size="150" letter-spacing="-4" fill="{ink}">Reach</text></svg>')
        name = 'dark' if dark else 'light'
        w(f'wordmark-{name}.svg', wm)
        render_many([(wm.replace('width="760" height="256"', 'width="1520" height="512"', 1), 1520, 512)])[0].save(os.path.join(BRAND, 'png', f'wordmark-{name}.png'))
    print('exported; static icon:', STATIC_CHOICES[static][0])

if __name__ == '__main__':
    cmd = sys.argv[1] if len(sys.argv) > 1 else ''
    try:
        if cmd == 'before':
            sheet([row_for('current', 'Current mark (white pointer on black)')], 'Logo before: 16, 24, 32, 48, 64 and 256 px, taskbars, title bar, installer, social',
                  os.path.join(ROOT, 'docs', 'design-review', 'logo-before.png'))
        elif cmd == 'round':
            n, names = sys.argv[2], sys.argv[3:]
            sheet([row_for(x, x) for x in names], f'Logo round {n}', os.path.join(ROOT, 'docs', 'brand', f'logo-round-{n}.png'))
        elif cmd == 'final':
            final(*sys.argv[2:5])
        elif cmd == 'bare-sheet':
            bare_sheet(os.path.join(ROOT, 'docs', 'brand', 'logo-bare-sheet.png'))
        elif cmd == 'bare':
            bare(int(sys.argv[2]) if len(sys.argv) > 2 else 0)
        else:
            print(__doc__)
    finally:
        shutil.rmtree(TMP, ignore_errors=True)
