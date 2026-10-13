#!/usr/bin/env python3
"""Writes the round-trip corpus: one DXF per release and kind of content, each
with a `.expect` sidecar saying what CadSlate2D must make of it.

Needs ezdxf 1.4.4 (`python3 -m venv .venv && .venv/bin/pip install ezdxf==1.4.4`),
and is run from the repository root:

    .venv/bin/python3 test-data/dxf/corpus/generate.py

The sidecars are the contract, written here by hand per kind of content and
adjusted per release only where a release cannot hold something (R12 has no
lineweights, MTEXT, or LWPOLYLINE; true colour needs R2004). The runner, `crates/cad-dxf/tests/corpus.rs`,
knows nothing of ezdxf: it imports each file, checks every line of its sidecar,
and exports, imports, and exports again byte for byte. Its format is at the top
of `corpus.rs` and in `test-data/dxf/README.md`.
"""

from __future__ import annotations

import math
from pathlib import Path

import ezdxf
from ezdxf.enums import TextEntityAlignment

HERE = Path(__file__).resolve().parent
RELEASES = ["R12", "R2000", "R2010", "R2018"]


def modern(release: str) -> bool:
    """Lineweights, MTEXT, LWPOLYLINE, and true colour need R2000 or later."""
    return release != "R12"


def new(release: str):
    doc = ezdxf.new(release)
    return doc, doc.modelspace()


def save(doc, name: str, expect: list[str]) -> None:
    doc.saveas(HERE / f"{name}.dxf")
    body = "\n".join(line for line in expect if line is not None)
    (HERE / f"{name}.expect").write_text(body + "\n")


def layers_besides_zero(doc) -> int:
    return sum(1 for layer in doc.layers if layer.dxf.name != "0")


def curves(release: str) -> None:
    doc, msp = new(release)
    msp.add_line((0, 0), (100, 0))
    msp.add_line((0, 0), (0, 50))
    msp.add_line((-10.5, 2.25), (10.5, -2.25))
    msp.add_circle((50, 50), 25)
    msp.add_circle((0, 0), 0.5)
    msp.add_arc((0, 0), 30, 0, 90)
    save(doc, f"{release.lower()}-curves", [
        "imported lines 3",
        "imported circles 2",
        "imported arcs 1",
        "entities 6",
        "line 0 0 0 100 0",
        "line 2 -10.5 2.25 10.5 -2.25",
        "circle 0 50 50 25",
        "circle 1 0 0 0.5",
        "arc 0 0 0 30 0 90",
        f"tables layers {layers_besides_zero(doc)}",
    ])


def polylines(release: str) -> None:
    doc, msp = new(release)
    open_run = [(0, 0), (40, 0), (40, 30)]
    closed_box = [(100, 0), (160, 0), (160, 40), (100, 40)]
    # A slot: two straight sides and two half-circle ends (bulge 1).
    slot = [(0, 100, 1), (100, 100, 0), (100, 140, 1), (0, 140, 0)]
    if modern(release):
        msp.add_lwpolyline(open_run)
        msp.add_lwpolyline(closed_box, close=True)
        msp.add_lwpolyline(slot, format="xyb", close=True)
    else:
        msp.add_polyline2d(open_run)
        msp.add_polyline2d(closed_box, close=True)
        msp.add_polyline2d(slot, format="xyb", close=True)
    # A 3D polyline: not a 2D drawing's; reported.
    msp.add_polyline3d([(0, 0, 0), (10, 10, 10), (20, 0, 5)])
    save(doc, f"{release.lower()}-polylines", [
        "imported polylines 3",
        "unsupported POLYLINE 1",
        "entities 3",
        "polyline 0 open 3",
        "polyline 1 closed 4",
        "polyline 2 closed 4",
        "bulge 2 0 1",
        "bulge 2 1 0",
        "bulge 2 2 1",
    ])


def text(release: str) -> None:
    doc, msp = new(release)
    doc.styles.add("ROMAN", font="romans.shx")
    msp.add_text("Left", dxfattribs={"height": 2.5, "insert": (0, 0)})
    msp.add_text("Centre", dxfattribs={"height": 5, "style": "ROMAN", "rotation": 30}).set_placement(
        (50, 0), align=TextEntityAlignment.CENTER
    )
    msp.add_text("Right", dxfattribs={"height": 5}).set_placement((100, 0), align=TextEntityAlignment.RIGHT)
    # Fit text is stretched between two points: read as left text, reported.
    msp.add_text("Fitted", dxfattribs={"height": 5}).set_placement(
        (0, 50), (80, 50), align=TextEntityAlignment.FIT
    )
    save(doc, f"{release.lower()}-text", [
        "imported texts 4",
        "tables text_styles 1",
        "dropped 1",
        'text 0 "Left" 2.5',
        'text 1 "Centre" 5',
        'text 2 "Right" 5',
        'text 3 "Fitted" 5',
    ])


def mtext(release: str) -> None:
    doc, msp = new(release)
    msp.add_mtext("First line\\PSecond line", dxfattribs={"char_height": 2.5, "width": 60, "insert": (0, 0)})
    # Formatting codes are dropped and reported; the words stay.
    msp.add_mtext("{\\fArial|b1;Bold} and \\C1;red", dxfattribs={"char_height": 2.5, "width": 60, "insert": (0, -20)})
    long = "".join(chr(ord("a") + (i % 26)) for i in range(300))
    msp.add_mtext(long, dxfattribs={"char_height": 2.5, "width": 400, "insert": (0, -40)})
    save(doc, f"{release.lower()}-mtext", [
        "imported texts 3",
        "formatting-dropped 1",
        'mtext 0 "First line\\nSecond line" 60',
        'mtext 1 "Bold and red" 60',
        f'mtext 2 "{long}" 400',
    ])


def layers(release: str) -> None:
    doc, msp = new(release)
    doc.linetypes.add("DASHED", pattern=[0.75, 0.5, -0.25])
    doc.linetypes.add("CENTER", pattern=[2.0, 1.25, -0.25, 0.25, -0.25])
    walls = doc.layers.add("WALLS", color=1, linetype="DASHED")
    walls.lock()
    hidden = doc.layers.add("HIDDEN", color=4, linetype="CENTER")
    hidden.off()
    if modern(release):
        walls.dxf.lineweight = 50
    msp.add_line((0, 0), (10, 0), dxfattribs={"layer": "WALLS"})
    attribs = {"layer": "HIDDEN", "color": 5, "linetype": "DASHED"}
    if modern(release):
        attribs["lineweight"] = 35
    msp.add_line((0, 10), (10, 10), dxfattribs=attribs)
    msp.add_circle((5, 5), 2, dxfattribs={"color": 0, "linetype": "BYBLOCK"})
    save(doc, f"{release.lower()}-layers", [
        "imported lines 2",
        "imported circles 1",
        f"tables layers {layers_besides_zero(doc)}",
        "tables linetypes 2",
        "layer WALLS colour 1 on locked linetype DASHED lineweight " + ("50" if modern(release) else "default"),
        "layer HIDDEN colour 4 off unlocked linetype CENTER lineweight default",
        "entity 0 layer WALLS colour bylayer linetype bylayer lineweight bylayer",
        "entity 1 layer HIDDEN colour 5 linetype DASHED lineweight " + ("35" if modern(release) else "bylayer"),
        "entity 2 layer 0 colour byblock linetype byblock lineweight bylayer",
    ])


def blocks(release: str) -> None:
    doc, msp = new(release)
    doc.layers.add("FURNITURE", color=4)
    chair = doc.blocks.new("CHAIR")
    chair.add_line((0, 0), (10, 0), dxfattribs={"color": 0})
    chair.add_arc((5, 0), 5, 0, 180, dxfattribs={"layer": "FURNITURE"})
    pair = doc.blocks.new("PAIR")
    pair.add_blockref("CHAIR", (0, 0))
    pair.add_blockref("CHAIR", (20, 0))
    msp.add_blockref("PAIR", (100, 0))
    msp.add_blockref("PAIR", (100, 100), dxfattribs={"xscale": 2, "yscale": 2, "zscale": 2, "rotation": 90})
    msp.add_blockref("CHAIR", (0, 50), dxfattribs={"xscale": -1})
    msp.add_blockref("CHAIR", (0, 80), dxfattribs={"yscale": -1})
    msp.add_blockref("CHAIR", (50, 50), dxfattribs={"xscale": 2, "yscale": 1})
    msp.add_blockref("CHAIR", (0, -50), dxfattribs={"column_count": 3, "column_spacing": 30})
    msp.add_blockref("GHOST", (200, 200))
    save(doc, f"{release.lower()}-blocks", [
        "imported block_references 6",
        "tables blocks 2",
        "block CHAIR 2",
        "block PAIR 2",
        "entities 6",
        "reference 0 PAIR 100 0 1 0 plain",
        "reference 1 PAIR 100 100 2 90 plain",
        "reference 2 CHAIR 0 50 1 180 mirrored",
        "reference 3 CHAIR 0 80 1 0 mirrored",
        "reference 4 CHAIR 50 50 2 0 plain",
        "reference 5 CHAIR 0 -50 1 0 plain",
        'unsupported "(INSERT with a non-uniform scale)" 1',
        'unsupported "(INSERT array)" 1',
        'unsupported "(INSERT of an undefined block)" 1',
    ])


def header(release: str) -> None:
    doc, msp = new(release)
    doc.linetypes.add("DOT", pattern=[0.25, 0.0, -0.25])
    doc.layers.add("PIPES", color=3)
    doc.header["$CLAYER"] = "PIPES"
    doc.header["$CECOLOR"] = 2
    doc.header["$CELTYPE"] = "DOT"
    doc.header["$LTSCALE"] = 5.0
    if modern(release):
        doc.header["$INSUNITS"] = 4  # millimetres
        doc.header["$CELWEIGHT"] = 25
    msp.add_line((0, 0), (1, 1))
    save(doc, f"{release.lower()}-header", [
        "imported lines 1",
        "unit " + ("mm" if modern(release) else "none"),
        "current layer PIPES",
        "current colour 2",
        "current linetype DOT",
        "current lineweight " + ("25" if modern(release) else "bylayer"),
        "ltscale 5",
    ])


def unsupported(release: str) -> None:
    doc, msp = new(release)
    msp.add_line((0, 0), (1, 0))
    msp.add_point((5, 5))
    msp.add_solid([(0, 0), (1, 0), (1, 1), (0, 1)])
    msp.add_3dface([(0, 0, 0), (1, 0, 0), (1, 1, 0), (0, 1, 0)])
    expect = [
        "imported lines 1",
        "entities 1",
        "unsupported POINT 1",
        "unsupported SOLID 1",
        "unsupported 3DFACE 1",
    ]
    if modern(release):
        msp.add_spline([(0, 0), (10, 10), (20, 0)])
        # Imported since #181 and #182; they stay here as the one ellipse
        # and the one spline ezdxf wrote.
        msp.add_ellipse((50, 50), major_axis=(20, 0), ratio=0.5)
        expect[0:2] = ["imported lines 1", "imported ellipses 1", "imported splines 1", "entities 3"]
    save(doc, f"{release.lower()}-unsupported", expect)


def true_colour(release: str) -> None:
    doc, msp = new(release)
    teal = doc.layers.add("TEAL")
    teal.rgb = (0, 128, 128)
    msp.add_line((0, 0), (10, 0), dxfattribs={"true_color": ezdxf.rgb2int((255, 128, 0))})
    msp.add_circle((5, 5), 2, dxfattribs={"true_color": ezdxf.rgb2int((255, 0, 0))})
    msp.add_line((0, 20), (10, 20), dxfattribs={"layer": "TEAL", "color": 3})
    save(doc, f"{release.lower()}-true-colour", [
        "imported lines 2",
        "imported circles 1",
        "dropped 2",
        'unsupported "(layer true colour)" 1',
        "entity 0 layer 0 colour 30 linetype bylayer lineweight bylayer",
        "entity 1 layer 0 colour 1 linetype bylayer lineweight bylayer",
        "entity 2 layer TEAL colour 3 linetype bylayer lineweight bylayer",
    ])


def dimensions(release: str) -> None:
    """Every kind of dimension the drawing has a measure for, each rendered so
    the file carries its anonymous block (which CadSlate2D does not read), a
    style of its own, the drafter's text, a turned text, and an ordinate
    dimension, which is reported."""
    doc, msp = new(release)
    arch = doc.dimstyles.add("ARCH")
    arch.dxf.dimtxt = 3.5
    msp.add_linear_dim(base=(0, 20), p1=(0, 0), p2=(100, 0), dimstyle="ARCH").render()
    msp.add_aligned_dim(p1=(0, -50), p2=(30, -10), distance=10).render()
    msp.add_linear_dim(base=(-20, -50), p1=(0, -50), p2=(30, -10), angle=90).render()
    msp.add_angular_dim_2l(
        base=(15, 115), line1=((0, 100), (50, 100)), line2=((0, 100), (0, 150))
    ).render()
    msp.add_angular_dim_3p(base=(60, -160), center=(0, -200), p1=(50, -200), p2=(0, -150)).render()
    msp.add_radius_dim(center=(200, 0), radius=30, angle=45).render()
    msp.add_diameter_dim(center=(300, 0), radius=30, angle=45).render()
    msp.add_linear_dim(
        base=(0, -300), p1=(0, -320), p2=(60, -320), text="TYP", text_rotation=30
    ).render()
    msp.add_ordinate_x_dim(feature_location=(400, 10), offset=(0, 20)).render()
    save(doc, f"{release.lower()}-dimensions", [
        "imported dimensions 8",
        'unsupported "(ordinate DIMENSION)" 1',
        "tables dim_styles 1",
        "entities 8",
        "dimension 0 linear 100",
        "dimension 1 linear 50",  # ezdxf writes an aligned dimension as a rotated one
        "dimension 2 linear 40",
        "dimension 3 angular 90",
        "dimension 4 angular 90",
        "dimension 5 radius 30",
        "dimension 6 diameter 60",
        "dimension 7 linear 60",
    ])


def hatches(release: str) -> None:
    """Solid and pattern hatches on polyline and edge paths — lines, arcs
    either way round, a full circle as a hole — with a spline boundary, a
    gradient, and a pattern CadSlate2D's table lacks, each reported. R12 has
    no HATCH."""
    doc, msp = new(release)
    h = msp.add_hatch()
    h.paths.add_polyline_path([(0, 0), (100, 0), (100, 100), (0, 100)], is_closed=True, flags=1)
    e = h.paths.add_edge_path(flags=16)
    e.add_arc((50, 50), 20, 0, 360)
    h = msp.add_hatch()
    h.set_pattern_fill("ANSI31", scale=2, angle=30)
    h.paths.add_polyline_path([(200, 0, 0), (300, 0, 1), (300, 100, 0), (200, 100, 0)], is_closed=True)
    h = msp.add_hatch()
    h.set_pattern_fill("NET", scale=3)
    e = h.paths.add_edge_path()
    e.add_line((600, 0), (600, 100))
    e.add_arc((650, 100), 50, 0, 180, ccw=False)
    e.add_line((700, 100), (700, 0))
    e.add_line((700, 0), (600, 0))
    h = msp.add_hatch()
    e = h.paths.add_edge_path()
    e.add_spline(fit_points=[(800, 0), (850, 50), (900, 0)])
    e.add_line((900, 0), (800, 0))
    h = msp.add_hatch()
    h.set_pattern_fill("AR-CONC", scale=1)
    h.paths.add_polyline_path([(200, 200), (300, 200), (300, 300), (200, 300)], is_closed=True)
    # Gradients came with R2004: ezdxf writes none for R2000.
    gradient = release != "R2000"
    if gradient:
        h = msp.add_hatch()
        h.set_gradient((255, 0, 0), (0, 0, 255))
        h.paths.add_polyline_path([(0, 200), (100, 200), (100, 300), (0, 300)], is_closed=True)
    count = 6 if gradient else 5
    save(doc, f"{release.lower()}-hatches", [
        f"imported hatches {count}",
        'unsupported "(HATCH pattern not in the table)" 1',
        'unsupported "(gradient HATCH)" 1' if gradient else None,
        f"entities {count}",
        "hatch 0 SOLID 2",
        "hatch 1 ANSI31 1",
        "hatch 2 NET 1",
        "hatch 3 SOLID 1  # bounded by a spline and a line",
        "hatch 4 SOLID 1  # AR-CONC",
        "hatch 5 SOLID 1  # the gradient" if gradient else None,
    ])


def attributes(release: str) -> None:
    doc, msp = new(release)
    tag = doc.blocks.new("TAG")
    tag.add_line((0, 0), (30, 0))
    tag.add_attdef("ROOM", (2, 2), dxfattribs={"height": 5})
    ref = msp.add_blockref("TAG", (0, 0))
    ref.add_auto_attribs({"ROOM": "101"})
    msp.add_blockref("TAG", (50, 0), dxfattribs={"rotation": 45})
    save(doc, f"{release.lower()}-attributes", [
        "imported block_references 2",
        "tables blocks 1",
        "block TAG 1",
        "unsupported ATTDEF 1",
        'unsupported "(INSERT attributes)" 1',
        "reference 1 TAG 50 0 1 45 plain",
    ])


def mixed(release: str) -> None:
    doc, msp = new(release)
    doc.layers.add("WALLS", color=1)
    door = doc.blocks.new("DOOR")
    door.add_line((0, 0), (0, 10))
    door.add_arc((0, 0), 10, 0, 90)
    msp.add_line((0, 0), (100, 0), dxfattribs={"layer": "WALLS"})
    msp.add_line((100, 0), (100, 60), dxfattribs={"layer": "WALLS"})
    msp.add_circle((50, 30), 5)
    if modern(release):
        msp.add_lwpolyline([(10, 10), (40, 10), (40, 50)])
    else:
        msp.add_polyline2d([(10, 10), (40, 10), (40, 50)])
    msp.add_text("Room", dxfattribs={"height": 5, "insert": (50, 40)})
    msp.add_blockref("DOOR", (20, 0))
    save(doc, f"{release.lower()}-mixed", [
        "imported lines 2",
        "imported circles 1",
        "imported polylines 1",
        "imported texts 1",
        "imported block_references 1",
        "tables blocks 1",
        "entities 6",
        "entity 0 layer WALLS colour bylayer linetype bylayer lineweight bylayer",
    ])


def degenerate(release: str) -> None:
    doc, msp = new(release)
    msp.add_line((0, 0), (1, 0))
    msp.add_circle((0, 0), 0)
    msp.add_arc((0, 0), -5, 0, 90)
    # ezdxf will not write a scale of zero: this reference comes out whole.
    tag = doc.blocks.new("TAG")
    tag.add_line((0, 0), (1, 0))
    msp.add_blockref("TAG", (0, 0), dxfattribs={"xscale": 0})
    if modern(release):
        msp.add_lwpolyline([(5, 5)])
    else:
        msp.add_polyline2d([(5, 5)])
    save(doc, f"{release.lower()}-degenerate", [
        "imported lines 1",
        "imported block_references 1",
        "entities 2",
        "reference 0 TAG 0 0 1 0 plain",
        'unsupported "(invalid geometry)" 3',
    ])


def main() -> None:
    for old in HERE.glob("*.dxf"):
        old.unlink()
    for old in HERE.glob("*.expect"):
        old.unlink()
    for release in RELEASES:
        for kind in [curves, polylines, text, layers, blocks, header, unsupported, attributes, mixed, degenerate, dimensions]:
            kind(release)
        if modern(release):
            mtext(release)
            hatches(release)
        # True colour (group 420) came with R2004: ezdxf writes none for R2000.
        if release in ("R2010", "R2018"):
            true_colour(release)
    print(f"{len(list(HERE.glob('*.dxf')))} files written to {HERE}")


if __name__ == "__main__":
    main()
