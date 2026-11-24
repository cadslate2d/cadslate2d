# ADR-0042's spike (#523): judges what the model drew from each image against
# what the image writes, and draws the answer back over its image through its
# `underlay`, for a look.
import json
import math
import re
import sys

from PIL import Image, ImageDraw

RESULTS, IMAGES, OUT = sys.argv[1], sys.argv[2], sys.argv[3]
FT = 304.8
IN = 25.4


def ft(feet, inches=0.0):
    return feet * FT + inches * IN


# What each image writes: the outline's overall size, and sizes it writes
# that a drawing to it must have (as a span of its coordinates or a
# dimension). `ask` where the right answer is a question.
TRUTH = {
    "sketch-room": {"box": (6000, 4500), "sizes": [6000, 4500, 2100, 3200]},
    "sketch-plate": {"box": (120, 80), "sizes": [120, 80, 15], "circles": (4, 10)},
    "sketch-house": {"box": (10000, 7500), "sizes": [10000, 7500, 4000, 3000]},
    "sketch-bracket": {"box": (80, 60), "sizes": [80, 60, 5]},
    "scan-leopold": {"box": (ft(22, 5.5), ft(18, 4)), "sizes": [ft(22, 5.5), ft(18, 4), ft(12, 6.25), ft(6), ft(10, 1)]},
    "scan-bugbee": {"box": (ft(40, 1), ft(37, 0.5)), "sizes": [ft(40, 1), ft(37, 0.5), ft(29, 0.75), ft(8, 1.5), ft(25, 1.5), ft(15, 1.25)]},
    "scan-telephone": {"box": (ft(28, 11.125), ft(20, 11.125)), "sizes": [ft(28, 11.125), ft(20, 11.125), ft(12, 6.25), ft(8, 5)]},
    "scan-library": {"box": (ft(64, 0.5), ft(100, 2.75)), "sizes": [ft(64, 0.5), ft(100, 2.75), ft(40)]},
    "pdf-bracket": {"box": (80, 60), "sizes": [80, 60]},
    "pdf-pump": {"sizes": [600, 250, 420]},
    "nosize-shelf": {"ask": True},
    "nosize-garden": {"ask": True},
}
# Where two corners of the outline's box lie in the image, in pixels: from
# the sketches' generator, and measured on our own plotted sheet.
SKETCH = json.load(open(f"{IMAGES}/../truth-sketches.json"))
MARKS = {}
for name, t in SKETCH.items():
    k = 2048 / t["pixels_before_host"][0]
    MARKS[name] = [(x * k, y * k) for x, y in t["marks"]], t["mark_points"]
MARKS["pdf-bracket"] = ([(710.7, 882.7), (1210.4, 508.9)], [(0, 0), (80, 60)])

NUM = r"(-?\d+(?:\.\d+)?)"
POINT = re.compile(r"\(" + NUM + r", " + NUM + r"\)")


def placement(last, w_img, h_img):
    """World point -> image pixel, from `marks` (two pixel/point pairs) or
    from an `underlay` (at, width, rotation)."""
    marks = last.get("marks")
    if marks and len(marks) == 2:
        p1, p2 = (complex(m["pixel"][0], h_img - m["pixel"][1]) for m in marks)
        w1, w2 = (complex(*m["point"]) for m in marks)
        if p1 == p2:
            return None
        a = (w2 - w1) / (p2 - p1)
        b = w1 - a * p1
        def to_px(pt):
            z = (complex(*pt) - b) / a
            return (z.real, h_img - z.imag)
        return to_px
    under = last.get("underlay")
    if under:
        width = under["width"]
        height = width * h_img / w_img
        ax, ay = under["at"]
        rot = math.radians(under.get("rotation") or 0)
        def to_px(pt):
            x, y = pt[0] - ax, pt[1] - ay
            x, y = x * math.cos(-rot) - y * math.sin(-rot), x * math.sin(-rot) + y * math.cos(-rot)
            return (x / width * w_img, h_img - y / height * h_img)
        return to_px
    return None


def shapes(described):
    segs, circles, dims, closed = [], [], [], 0
    for line in described:
        if line.startswith("line ") or line.startswith("polyline"):
            pts = [(float(a), float(b)) for a, b in POINT.findall(line)]
            ring = pts + ([pts[0]] if line.startswith("polyline closed") else [])
            closed += line.startswith("polyline closed")
            segs += list(zip(ring, ring[1:]))
        elif line.startswith("circle ") or line.startswith("arc "):
            (x, y), r = POINT.findall(line)[0], re.search(r"radius " + NUM, line).group(1)
            circles.append(((float(x), float(y)), float(r)))
        elif line.startswith("dimension "):
            dims.append(float(line.split()[2]))
    return segs, circles, dims, closed


def judge(record):
    name = record["name"]
    t = TRUTH[name]
    asks = record["asks"]
    last = next((a for a in reversed(asks) if a.get("valid")), asks[-1])
    out = {"name": name, "asks": len(asks), "seconds": round(record["seconds"]), "reply": (last.get("reply") or "")[:300]}
    document = last.get("intent")
    if t.get("ask"):
        out["verdict"] = "asked" if not document else "drew a guess"
        return out, last
    if not document:
        out["verdict"] = "no drawing"
        return out, last
    drawn = last.get("drawn") or {}
    if not drawn.get("ran"):
        out["verdict"] = f"did not run: {drawn.get('why')}"
        return out, last
    segs, circles, dims, closed = shapes(drawn["described"])
    xs = [p[0] for s in segs for p in s] + [c[0][0] + k * c[1] for c in circles for k in (-1, 0, 1)]
    ys = [p[1] for s in segs for p in s] + [c[0][1] + k * c[1] for c in circles for k in (-1, 0, 1)]
    box = (max(xs) - min(xs), max(ys) - min(ys)) if xs else (0, 0)
    tol = lambda s: max(1.0, 0.002 * s)
    spans = lambda vals, s: any(abs(abs(a - b) - s) <= tol(s) for a in vals for b in vals)
    read = [s for s in t["sizes"] if spans(sorted(set(xs)), s) or spans(sorted(set(ys)), s) or any(abs(d - s) <= tol(s) for d in dims)]
    out.update({
        "box": [round(box[0], 1), round(box[1], 1)],
        "box_right": "box" in t and all(abs(b - w) <= tol(w) for b, w in zip(box, t["box"])),
        "sizes_read": f"{len(read)}/{len(t['sizes'])}",
        "closed": closed,
        "dimensions": len(dims),
    })
    if "circles" in t:
        n, dia = t["circles"]
        out["holes"] = sum(1 for c in circles if abs(2 * c[1] - dia) <= 0.1)
    w_img, h_img = Image.open(f"{IMAGES}/{record['case']['image']}").size
    to_px = placement(last, w_img, h_img)
    if to_px and name in MARKS and xs:
        pix, points = MARKS[name]
        lo = (min(xs), min(ys))
        errs = []
        for (px, py), (mx, my) in zip(pix, points):
            qx, qy = to_px((lo[0] + mx, lo[1] + my))
            errs.append(math.hypot(qx - px, qy - py) / w_img * 100)
        out["placement_error_pct"] = round(max(errs), 2)
    return out, last


def overlay(record, last):
    drawn = (last.get("drawn") or {}).get("described")
    if not drawn:
        return
    img = Image.open(f"{IMAGES}/{record['case']['image']}").convert("RGB")
    to_px = placement(last, *img.size)
    if not to_px:
        return
    d = ImageDraw.Draw(img)
    segs, circles, _, _ = shapes(drawn)
    for a, b in segs:
        d.line([to_px(a), to_px(b)], fill=(220, 30, 30), width=4)
    for (c, r) in circles:
        (cx, cy) = to_px(c)
        (ex, ey) = to_px((c[0] + r, c[1]))
        rr = math.hypot(ex - cx, ey - cy)
        d.ellipse([cx - rr, cy - rr, cx + rr, cy + rr], outline=(220, 30, 30), width=4)
    img.save(f"{OUT}/overlay-{record['name']}.jpg", quality=80)

rows = []
for name in TRUTH:
    try:
        record = json.load(open(f"{RESULTS}/{name}.json"))
    except FileNotFoundError:
        rows.append({"name": name, "verdict": "no record"})
        continue
    row, last = judge(record)
    rows.append(row)
    overlay(record, last)
json.dump(rows, open(f"{OUT}/judged.json", "w"), indent=1)
for r in rows:
    print(json.dumps({k: v for k, v in r.items() if k != "reply"}))
