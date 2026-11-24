# ADR-0042's spike (#523): hand-sketch-like images of known geometry, from a
# seed. Each sketch is drawn in its own units onto paper, with pencil strokes
# that wobble and overshoot and written sizes, then the sheet is turned a
# little and shaded as a photograph would be. Writes each image and a
# ground-truth JSON: where two corners of the outline land, in pixels.
import json
import math
import random
import sys

from PIL import Image, ImageDraw, ImageFilter, ImageFont

OUT = sys.argv[1]
FONT = "/usr/share/fonts/truetype/dejavu/DejaVuSansCondensed-Oblique.ttf"
W, H = 2400, 1800  # the paper, before the photograph
INK = (55, 55, 62)


def paper(rng):
    img = Image.new("RGB", (W, H), (246, 243, 236))
    px = img.load()
    for _ in range(60000):
        x, y = rng.randrange(W), rng.randrange(H)
        v = rng.randint(-10, 6)
        r, g, b = px[x, y]
        px[x, y] = (r + v, g + v, b + v)
    return img


def stroke(d, rng, a, b, width=4, overshoot=14):
    # A pencil line: jittered along its length, overshooting its ends, drawn
    # twice slightly apart.
    (x0, y0), (x1, y1) = a, b
    length = math.hypot(x1 - x0, y1 - y0) or 1
    ux, uy = (x1 - x0) / length, (y1 - y0) / length
    for k in range(2):
        o0 = rng.uniform(0, overshoot)
        o1 = rng.uniform(0, overshoot)
        sx, sy = x0 - ux * o0, y0 - uy * o0
        ex, ey = x1 + ux * o1, y1 + uy * o1
        n = max(4, int(length / 40))
        pts = []
        drift = rng.uniform(-3, 3)
        for i in range(n + 1):
            t = i / n
            j = rng.uniform(-2.2, 2.2) + drift * math.sin(math.pi * t)
            pts.append((sx + (ex - sx) * t - uy * j + k * 1.5, sy + (ey - sy) * t + ux * j + k * 1.5))
        d.line(pts, fill=INK, width=width - k, joint="curve")


def text(img, rng, s, at, size=56, angle=0.0):
    font = ImageFont.truetype(FONT, size)
    box = font.getbbox(s)
    tile = Image.new("RGBA", (box[2] + 20, box[3] + 20), (0, 0, 0, 0))
    ImageDraw.Draw(tile).text((10, 10), s, font=font, fill=INK + (235,))
    tile = tile.rotate(angle + rng.uniform(-3, 3), expand=True, resample=Image.BICUBIC)
    img.paste(tile, (int(at[0] - tile.width / 2), int(at[1] - tile.height / 2)), tile)


def dim(img, d, rng, a, b, label, side, off=110):
    # A sketched dimension: a line with arrowheads, offset from the edge
    # a–b on `side` (+1 left of a→b, -1 right), its size written beside it.
    (x0, y0), (x1, y1) = a, b
    L = math.hypot(x1 - x0, y1 - y0)
    ux, uy = (x1 - x0) / L, (y1 - y0) / L
    # The page's y runs down: the side is mirrored from the drawing's.
    nx, ny = uy * side, -ux * side
    p0 = (x0 + nx * off, y0 + ny * off)
    p1 = (x1 + nx * off, y1 + ny * off)
    stroke(d, rng, (x0 + nx * 25, y0 + ny * 25), (x0 + nx * (off + 25), y0 + ny * (off + 25)), width=2, overshoot=4)
    stroke(d, rng, (x1 + nx * 25, y1 + ny * 25), (x1 + nx * (off + 25), y1 + ny * (off + 25)), width=2, overshoot=4)
    stroke(d, rng, p0, p1, width=2, overshoot=0)
    for p, s in ((p0, 1), (p1, -1)):
        tip = p
        for w in (-1, 1):
            back = (tip[0] + ux * s * 28 + nx * w * 10, tip[1] + uy * s * 28 + ny * w * 10)
            d.line([tip, back], fill=INK, width=3)
    mid = ((p0[0] + p1[0]) / 2 + nx * 45, (p0[1] + p1[1]) / 2 + ny * 45)
    angle = -math.degrees(math.atan2(uy, ux))
    if angle < -90 or angle > 90:
        angle += 180
    text(img, rng, label, mid, angle=angle)


def photograph(img, rng, marks):
    # Turned a little and shaded from one side, as a phone photo of a sheet
    # on a desk; the marks follow the turn.
    angle = rng.uniform(-3.5, 3.5)
    cx, cy = W / 2, H / 2
    turned = img.rotate(angle, resample=Image.BICUBIC, fillcolor=(120, 112, 100))
    a = math.radians(angle)
    moved = []
    for x, y in marks:
        dx, dy = x - cx, y - cy
        # PIL turns counterclockwise on screen: y down.
        moved.append((cx + dx * math.cos(a) + dy * math.sin(a), cy - dx * math.sin(a) + dy * math.cos(a)))
    shade = Image.linear_gradient("L").rotate(90).resize((W, H))
    dark = Image.new("RGB", (W, H), (0, 0, 0))
    turned = Image.composite(turned, Image.blend(turned, dark, 0.18), shade)
    return turned.filter(ImageFilter.GaussianBlur(0.6)), moved


def sketch(name, rng, outline, closed, dims, scale, origin, extras=(), notes=()):
    # `outline` in the drawing's units; `scale` pixels per unit; `origin`
    # where the drawing's (0, 0) lands on the paper, y up.
    img = paper(rng)
    d = ImageDraw.Draw(img)
    to = lambda p: (origin[0] + p[0] * scale, origin[1] - p[1] * scale)
    pts = [to(p) for p in outline]
    edges = list(zip(pts, pts[1:] + ([pts[0]] if closed else [])))
    for a, b in edges:
        stroke(d, rng, a, b)
    for kind, args in extras:
        if kind == "line":
            stroke(d, rng, to(args[0]), to(args[1]))
        elif kind == "circle":
            (cx, cy), r = to(args[0]), args[1] * scale
            for k in range(2):
                j = rng.uniform(-3, 3)
                d.ellipse([cx - r + j, cy - r - j, cx + r + j, cy + r + j], outline=INK, width=4 - k)
    for a, b, label, side in dims:
        dim(img, d, rng, to(a), to(b), label, side)
    for s, at in notes:
        text(img, rng, s, to(at), size=48)
    marks = [to(outline[0]), to(outline[2] if len(outline) > 2 else outline[1])]
    photo, marks = photograph(img, rng, marks)
    photo.save(f"{OUT}/{name}.jpg", quality=92)
    return {"pixels_before_host": [W, H], "marks": marks, "mark_points": [outline[0], outline[2] if len(outline) > 2 else outline[1]]}


rng = random.Random(366)
truth = {}
# An L-shaped room, metres written, drawn in millimetres.
room = [(0, 0), (6000, 0), (6000, 2100), (3200, 2100), (3200, 4500), (0, 4500)]
truth["sketch-room"] = sketch("sketch-room", rng, room, True,
    [((0, 0), (6000, 0), "6.00", -1), ((0, 0), (0, 4500), "4.50", 1),
     ((6000, 0), (6000, 2100), "2.10", -1), ((0, 4500), (3200, 4500), "3.20", 1)],
    0.27, (500, 1500))
# A plate with four holes, millimetres.
plate = [(0, 0), (120, 0), (120, 80), (0, 80)]
holes = [("circle", ((x, y), 5)) for x in (15, 105) for y in (15, 65)]
truth["sketch-plate"] = sketch("sketch-plate", rng, plate, True,
    [((0, 0), (120, 0), "120", -1), ((0, 0), (0, 80), "80", 1),
     ((0, 80), (15, 80), "15", 1), ((120, 80), (120, 65), "15", 1)],
    11.0, (560, 1380), extras=holes, notes=[("4× Ø10", (60, 40))])
# A house's walls as single lines, metres written.
house = [(0, 0), (10000, 0), (10000, 7500), (0, 7500)]
truth["sketch-house"] = sketch("sketch-house", rng, house, True,
    [((0, 0), (10000, 0), "10.00", -1), ((0, 0), (0, 7500), "7.50", 1),
     ((0, 7500), (4000, 7500), "4.00", 1), ((10000, 0), (10000, 3000), "3.00", -1)],
    0.16, (520, 1520), extras=[("line", ((4000, 0), (4000, 7500))), ("line", ((4000, 3000), (10000, 3000)))])
# A bracket's profile, millimetres.
bracket = [(0, 0), (80, 0), (80, 5), (5, 5), (5, 60), (0, 60)]
truth["sketch-bracket"] = sketch("sketch-bracket", rng, bracket, True,
    [((0, 0), (80, 0), "80", -1), ((0, 0), (0, 60), "60", 1), ((80, 0), (80, 5), "t = 5", -1)],
    17.0, (700, 1450))
# No size anywhere: a shelf unit, and a garden.
shelf = [(0, 0), (60, 0), (60, 100), (0, 100)]
truth["nosize-shelf"] = sketch("nosize-shelf", rng, shelf, True, [], 13.0, (800, 1550),
    extras=[("line", ((0, 30), (60, 30))), ("line", ((0, 60), (60, 60))), ("line", ((0, 85), (60, 85)))],
    notes=[("bookshelf", (30, 108))])
garden = [(0, 0), (90, 0), (100, 45), (60, 70), (0, 60)]
truth["nosize-garden"] = sketch("nosize-garden", rng, garden, True, [], 14.0, (500, 1450),
    extras=[("line", ((65, 10), (85, 10))), ("line", ((85, 10), (85, 25))), ("line", ((85, 25), (65, 25))), ("line", ((65, 25), (65, 10)))],
    notes=[("shed", (75, 32)), ("lawn", (35, 30)), ("my garden", (45, 78))])
json.dump(truth, open(f"{OUT}/truth-sketches.json", "w"), indent=1)
print("\n".join(truth))
