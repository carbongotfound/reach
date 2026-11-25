"""Traces the logo concept (docs/brand/concepts/concept-v3.png: a pointing hand, the index finger
reaching toward a dot) into smooth SVG paths on the 24-unit logo grid, and writes
assets/brand/hand.json for logo.py:

  python -I assets/brand/trace.py

  "hand":  the full mark. The arm is cut at an angle across the wrist, and the slits between the
           curled fingers are widened so they stay open at 32 px.
  "small": the same hand with the curled fingers closed into one shape, for 16-24 px.
  "dot":   [cx, cy, r], the point the finger reaches toward (position and gap as drawn).
Paths use fill-rule evenodd.
"""
import json, os
import cv2
import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
SRC = os.path.join(ROOT, 'docs', 'brand', 'concepts', 'concept-v3.png')
LO, HI = 2.0, 22.0          # keylines: the art fits inside
ARM = (1.0, -0.4)           # the arm's direction; the cut is square to it
CUT = (70, 313)             # a point on the cut (source px): on the arm's upper edge, before the wrist
SLIT = 13                   # px added across each finger slit (source is about 530 px wide)


def disk(n): return cv2.getStructuringElement(cv2.MORPH_ELLIPSE, (n, n))


def contour_path(c, k, ox, oy, step=6.0, sigma=3.0, tol=0.8):
    """Closed contour -> smooth cubic path: resample evenly, smooth, keep only the points that bend,
    then Catmull-Rom to Bezier."""
    p = c[:, 0, :].astype(float)
    seg = np.r_[0, np.cumsum(np.hypot(*np.diff(np.vstack([p, p[:1]]), axis=0).T))]
    n = max(16, int(seg[-1] / step))
    t = np.linspace(0, seg[-1], n, endpoint=False)
    loop = np.vstack([p, p[:1]])
    q = np.c_[np.interp(t, seg, loop[:, 0]), np.interp(t, seg, loop[:, 1])]
    r = int(3 * sigma)
    w = np.exp(-0.5 * (np.arange(-r, r + 1) / sigma) ** 2); w /= w.sum()
    q = np.c_[[np.convolve(np.r_[q[-r:, i], q[:, i], q[:r, i]], w, 'valid') for i in (0, 1)]].T
    keep = [0]
    for i in range(1, len(q) - 1):
        a, b, cc = q[keep[-1]], q[i], q[i + 1]
        cross = abs((b[0] - a[0]) * (cc[1] - a[1]) - (b[1] - a[1]) * (cc[0] - a[0])) / max(1e-6, np.hypot(*(cc - a)))
        if cross > tol or np.hypot(*(b - a)) > 80: keep.append(i)
    q = (q[keep] - [ox, oy]) * k + LO
    m = len(q)
    f = lambda v: f'{v:.2f}'.rstrip('0').rstrip('.')
    d = [f'M{f(q[0][0])} {f(q[0][1])}']
    for i in range(m):
        p0, p1, p2, p3 = q[i - 1], q[i], q[(i + 1) % m], q[(i + 2) % m]
        c1 = p1 + (p2 - p0) / 6; c2 = p2 - (p3 - p1) / 6
        d.append(f'C{f(c1[0])} {f(c1[1])} {f(c2[0])} {f(c2[1])} {f(p2[0])} {f(p2[1])}')
    return ''.join(d) + 'Z', m


def paths(mask, k, ox, oy, **kw):
    cs, _ = cv2.findContours(mask, cv2.RETR_CCOMP, cv2.CHAIN_APPROX_NONE)
    out = [contour_path(c, k, ox, oy, **kw) for c in cs if cv2.contourArea(c) > 40]
    return ''.join(d for d, _ in out), sum(m for _, m in out)


def main():
    g = cv2.imread(SRC, cv2.IMREAD_GRAYSCALE)
    white = (g > 128).astype(np.uint8) * 255
    n, lab, st, _ = cv2.connectedComponentsWithStats(white)
    dot_i = min(range(1, n), key=lambda i: st[i][4])                 # the smallest shape is the dot
    hand = ((lab > 0) & (lab != dot_i)).astype(np.uint8) * 255
    dot = (lab == dot_i).astype(np.uint8) * 255
    # the arm runs off the canvas: keep only what lies ahead of a straight cut square to the arm
    yy, xx = np.mgrid[0:g.shape[0], 0:g.shape[1]]
    d = np.array(ARM) / np.hypot(*ARM)
    hand = hand * (((xx - CUT[0]) * d[0] + (yy - CUT[1]) * d[1]) >= 0)
    # widen the slits: black within a few px of two different finger shapes, plus thin black inside the outline
    m, parts = cv2.connectedComponents(hand)
    ds = np.sort(np.stack([cv2.distanceTransform((parts != i).astype(np.uint8), cv2.DIST_L2, 5) for i in range(1, m)]), axis=0)
    slit = ((hand == 0) & (ds[1] <= 14)).astype(np.uint8) * 255
    black = 255 - hand
    thin = cv2.bitwise_and(black, 255 - cv2.morphologyEx(black, cv2.MORPH_OPEN, disk(15)))
    thin = cv2.bitwise_and(thin, cv2.morphologyEx(hand, cv2.MORPH_CLOSE, disk(31)))
    full = cv2.bitwise_and(hand, 255 - (cv2.dilate(slit, disk(SLIT)) | cv2.dilate(thin, disk(9))))
    small = cv2.morphologyEx(hand, cv2.MORPH_CLOSE, disk(31))
    small = max(cv2.findContours(small, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_NONE)[0], key=cv2.contourArea)
    sm = np.zeros_like(hand); cv2.drawContours(sm, [small], -1, 255, -1)
    # fit hand and dot together inside the keylines, centred
    x, y, w, h = cv2.boundingRect(cv2.findNonZero(full | dot))
    k = (HI - LO) / max(w, h)
    ox, oy = x - (max(w, h) - w) / 2, y - (max(w, h) - h) / 2
    (dx, dy), dr = cv2.minEnclosingCircle(cv2.findNonZero(dot))
    hp, hn = paths(full, k, ox, oy)
    sp, sn = paths(sm, k, ox, oy, step=8, sigma=4)
    out = {'hand': hp, 'small': sp,
           'dot': [round((dx - ox) * k + LO, 2), round((dy - oy) * k + LO, 2), round(dr * k, 2)]}
    json.dump(out, open(os.path.join(HERE, 'hand.json'), 'w'), indent=1)
    print('hand', hn, 'points; small', sn, 'points; dot', out['dot'])


if __name__ == '__main__':
    main()
