#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 Noah Grant; see LICENSE in this folder.
"""Write the two STEP files of the RP2350A Minimal example's case: `base.step` and `lid.step`.

Run it from anywhere (`python3 examples/rp2350_minimal/case/make_case.py`); it writes beside
itself and its output is the same on every run. Python 3's standard library only.

Every solid is a box (six planar faces, AP214, as netsmith's case fixtures are written), so a
wall with a notch or a plate with slots is cut into boxes that touch and do not overlap.

The frame of both files is the base's: millimetres, z up, the origin at the outer bottom-left
corner of the base, the floor's underside at z = 0. The physical file places them in the
board's frame (the outline's bottom-left corner on the board's top face): the base at
(-WALL - GAP, -WALL - GAP, -FLOOR - STANDOFF - BOARD), the lid at the same x and y and
z = LID_Z. Every figure below is from the board (`netsmith board3d --json` of the example)
and is in the board's frame unless it says otherwise.
"""

from pathlib import Path

# The board: the outline is 52 x 51 mm, 1.6 mm thick.
BOARD_W, BOARD_H, BOARD = 52.0, 51.0, 1.6
# The four mounting holes (H1 to H4), centres in the board's frame.
HOLES = [(12.03, 47.0), (39.97, 47.0), (39.97, 3.82), (12.03, 3.82)]

GAP = 0.5  # between the board's edge and the inside of a wall
WALL = 2.0  # wall thickness
FLOOR = 2.0  # floor thickness
# The board rests on four bosses this tall: the header's tails stand about 1.6 mm out of the
# bottom face (TSW-118-07: a 3.18 mm tail under the 2.54 mm body, through 1.6 mm of board), so
# 4 mm leaves them and their solder 2 mm over the floor.
STANDOFF = 4.0
BOSS = 6.0  # a boss's square, round the hole
BOSS_BORE = 2.2  # the square bore of a boss, for an M2.5 thread-forming screw
# The lid's underside, above the board's top face. The tallest part under it is J1, the
# micro-USB receptacle (2.99 mm); a micro-B plug's overmould stands about 3.5 mm above the
# receptacle's mouth, so 6.5 mm keeps the lid clear of it with 1.5 mm to spare.
LID_Z = 6.5
LID = 2.0  # lid thickness

# J1's body: x 21.85 to 30.15, at the top edge, 2.99 mm tall. The notch in the top wall lets a
# micro-B plug's overmould (about 10.6 x 7 mm) seat, and is 3 mm wider than the receptacle and
# 3 mm deeper than its bottom on each side, where `case_reach` looks for the wall's outer face.
USB_NOTCH_X = (18.5, 33.5)
USB_NOTCH_Z = -3.4  # its bottom; it is open to the wall's top, which the lid closes
# J4, the JST-SH debug port (x 30.8 to 36.4), plugs from the bottom edge's side: a notch in the
# bottom wall lets its cable out, from the board's top face up.
DEBUG_NOTCH_X = (30.3, 36.9)
# J2 and J3, the 2 x 18 headers (x 0.6 to 5.68 and 46.32 to 51.4, y 2.55 to 48.27, 8.38 mm
# tall), stand through the lid: a slot 1 mm round each body lets a 2 x 18 socket down on them.
HEADER_SLOTS = [((-0.4, 6.68), (1.55, 49.27)), ((45.32, 52.4), (1.55, 49.27))]
# SW1 (BOOTSEL) and SW2 (RESET), centres: a 2 mm hole over each lets a pin press it.
BUTTONS = [(17.2, 45.8), (37.8, 13.525)]
BUTTON_HOLE = 2.0

# The base's frame, from the board's.
OX, OY = -(WALL + GAP), -(WALL + GAP)
OZ = -(FLOOR + STANDOFF + BOARD)
OUT_W = BOARD_W + 2 * (GAP + WALL)
OUT_H = BOARD_H + 2 * (GAP + WALL)
WALL_TOP = LID_Z - OZ  # the walls rise to the lid's underside


def board(x, y, z):
    """A point of the board's frame in the base's frame."""
    return (x - OX, y - OY, z - OZ)


def cut(box, holes):
    """`box` less `holes`, as boxes that touch and do not overlap.

    A box is ((x0, y0, z0), (x1, y1, z1)). The box is cut on every plane of a hole into cells;
    the cells outside every hole are joined along x, then y, then z, where they line up.
    """
    lo, hi = box
    breaks = []
    for k in range(3):
        b = {lo[k], hi[k]}
        for h in holes:
            for v in (h[0][k], h[1][k]):
                if lo[k] < v < hi[k]:
                    b.add(v)
        breaks.append(sorted(b))

    def inside_hole(c):
        return any(all(h[0][k] <= c[k] and c[k] <= h[1][k] for k in range(3)) for h in holes)

    cells = set()
    for i in range(len(breaks[0]) - 1):
        for j in range(len(breaks[1]) - 1):
            for k in range(len(breaks[2]) - 1):
                mid = tuple((breaks[a][n] + breaks[a][n + 1]) / 2 for a, n in zip(range(3), (i, j, k)))
                if not inside_hole(mid):
                    cells.add((i, j, k))
    # Greedy: from each free cell in order, grow along x, then y, then z, while every cell of
    # the grown face is free.
    out = []
    taken = set()
    for c in sorted(cells):
        if c in taken:
            continue
        end = list(c)
        for axis in range(3):
            while True:
                nxt = end[:]
                nxt[axis] += 1
                span = [range(c[a], (nxt if a == axis else end)[a] + 1) for a in range(3)]
                block = {(i, j, k) for i in span[0] for j in span[1] for k in span[2]}
                if block <= cells and not (block & taken):
                    end = nxt
                else:
                    break
        block = {(i, j, k) for i in range(c[0], end[0] + 1) for j in range(c[1], end[1] + 1) for k in range(c[2], end[2] + 1)}
        taken |= block
        out.append(
            (
                tuple(breaks[a][c[a]] for a in range(3)),
                tuple(breaks[a][end[a] + 1] for a in range(3)),
            )
        )
    return out


def base_solids():
    solids = [("floor", ((0.0, 0.0, 0.0), (OUT_W, OUT_H, FLOOR)))]
    z0, z1 = FLOOR, WALL_TOP
    usb = (
        (board(USB_NOTCH_X[0], 0, 0)[0], OUT_H - WALL, board(0, 0, USB_NOTCH_Z)[2]),
        (board(USB_NOTCH_X[1], 0, 0)[0], OUT_H, z1),
    )
    debug = (
        (board(DEBUG_NOTCH_X[0], 0, 0)[0], 0.0, board(0, 0, 0.0)[2]),
        (board(DEBUG_NOTCH_X[1], 0, 0)[0], WALL, z1),
    )
    walls = [
        ("wall_left", ((0.0, 0.0, z0), (WALL, OUT_H, z1)), []),
        ("wall_right", ((OUT_W - WALL, 0.0, z0), (OUT_W, OUT_H, z1)), []),
        ("wall_bottom", ((WALL, 0.0, z0), (OUT_W - WALL, WALL, z1)), [debug]),
        ("wall_top", ((WALL, OUT_H - WALL, z0), (OUT_W - WALL, OUT_H, z1)), [usb]),
    ]
    for name, box, holes in walls:
        parts = cut(box, holes)
        if len(parts) == 1:
            solids.append((name, parts[0]))
        else:
            solids += [(f"{name}_{n}", p) for n, p in enumerate(parts, 1)]
    top = board(0, 0, -BOARD)[2]  # the board's bottom face
    for n, (hx, hy) in enumerate(HOLES, 1):
        cx, cy, _ = board(hx, hy, 0)
        box = ((cx - BOSS / 2, cy - BOSS / 2, FLOOR), (cx + BOSS / 2, cy + BOSS / 2, top))
        bore = ((cx - BOSS_BORE / 2, cy - BOSS_BORE / 2, FLOOR), (cx + BOSS_BORE / 2, cy + BOSS_BORE / 2, top))
        solids += [(f"boss{n}_{m}", p) for m, p in enumerate(cut(box, [bore]), 1)]
    return solids


def lid_solids():
    plate = ((0.0, 0.0, 0.0), (OUT_W, OUT_H, LID))
    holes = []
    for (x0, x1), (y0, y1) in HEADER_SLOTS:
        holes.append(((x0 - OX, y0 - OY, 0.0), (x1 - OX, y1 - OY, LID)))
    for bx, by in BUTTONS:
        r = BUTTON_HOLE / 2
        holes.append(((bx - r - OX, by - r - OY, 0.0), (bx + r - OX, by + r - OY, LID)))
    return [(f"lid_{n}", p) for n, p in enumerate(cut(plate, holes), 1)]


def num(v):
    return f"{round(v, 4) + 0.0:.4f}"


def step(name, solids):
    """An AP214 file of closed box solids, one MANIFOLD_SOLID_BREP each."""
    lines = []
    n = [0]

    def ent(text):
        n[0] += 1
        lines.append(f"#{n[0]}={text};")
        return n[0]

    length = ent("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))")
    angle = ent("(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))")
    solid_angle = ent("(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_UNIT())")
    unc = ent(f"UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-07),#{length},'distance_accuracy_value','')")
    ctx = ent(
        f"(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{unc}))"
        f"GLOBAL_UNIT_ASSIGNED_CONTEXT((#{length},#{angle},#{solid_angle}))REPRESENTATION_CONTEXT('',''))"
    )
    breps = []
    for sname, ((x0, y0, z0), (x1, y1, z1)) in solids:
        p = {}
        for i, x in enumerate((x0, x1)):
            for j, y in enumerate((y0, y1)):
                for k, z in enumerate((z0, z1)):
                    p[(i, j, k)] = ent(f"CARTESIAN_POINT('',({num(x)},{num(y)},{num(z)}))")
        # Each face: its corners counterclockwise seen from outside, a point on it, its normal
        # and a direction in it.
        faces = [
            ([(0, 0, 0), (0, 1, 0), (1, 1, 0), (1, 0, 0)], (x0, y0, z0), "0.0,0.0,-1.0", "1.0,0.0,0.0"),
            ([(0, 0, 1), (1, 0, 1), (1, 1, 1), (0, 1, 1)], (x0, y0, z1), "0.0,0.0,1.0", "1.0,0.0,0.0"),
            ([(0, 0, 0), (1, 0, 0), (1, 0, 1), (0, 0, 1)], (x0, y0, z0), "0.0,-1.0,0.0", "1.0,0.0,0.0"),
            ([(0, 1, 0), (0, 1, 1), (1, 1, 1), (1, 1, 0)], (x0, y1, z0), "0.0,1.0,0.0", "1.0,0.0,0.0"),
            ([(0, 0, 0), (0, 0, 1), (0, 1, 1), (0, 1, 0)], (x0, y0, z0), "-1.0,0.0,0.0", "0.0,1.0,0.0"),
            ([(1, 0, 0), (1, 1, 0), (1, 1, 1), (1, 0, 1)], (x1, y0, z0), "1.0,0.0,0.0", "0.0,1.0,0.0"),
        ]
        ids = []
        for corners, origin, normal, ref in faces:
            loop = ent("POLY_LOOP('',(" + ",".join(f"#{p[c]}" for c in corners) + "))")
            bound = ent(f"FACE_OUTER_BOUND('',#{loop},.T.)")
            o = ent(f"CARTESIAN_POINT('',({num(origin[0])},{num(origin[1])},{num(origin[2])}))")
            d = ent(f"DIRECTION('',({normal}))")
            r = ent(f"DIRECTION('',({ref}))")
            ax = ent(f"AXIS2_PLACEMENT_3D('',#{o},#{d},#{r})")
            plane = ent(f"PLANE('',#{ax})")
            ids.append(ent(f"ADVANCED_FACE('',(#{bound}),#{plane},.T.)"))
        shell = ent("CLOSED_SHELL('',(" + ",".join(f"#{i}" for i in ids) + "))")
        breps.append(ent(f"MANIFOLD_SOLID_BREP('{sname}',#{shell})"))
    o = ent("CARTESIAN_POINT('',(0.,0.,0.))")
    d = ent("DIRECTION('',(0.,0.,1.))")
    r = ent("DIRECTION('',(1.,0.,0.))")
    ax = ent(f"AXIS2_PLACEMENT_3D('',#{o},#{d},#{r})")
    rep = ent(f"ADVANCED_BREP_SHAPE_REPRESENTATION('{name}',(" + ",".join(f"#{b}" for b in breps) + f",#{ax}),#{ctx})")
    app = ent("APPLICATION_CONTEXT('automotive design')")
    pctx = ent(f"PRODUCT_CONTEXT('',#{app},'mechanical')")
    prod = ent(f"PRODUCT('{name}','{name}','',(#{pctx}))")
    form = ent(f"PRODUCT_DEFINITION_FORMATION('','',#{prod})")
    dctx = ent(f"PRODUCT_DEFINITION_CONTEXT('part definition',#{app},'design')")
    pdef = ent(f"PRODUCT_DEFINITION('design','',#{form},#{dctx})")
    pds = ent(f"PRODUCT_DEFINITION_SHAPE('','',#{pdef})")
    ent(f"SHAPE_DEFINITION_REPRESENTATION(#{pds},#{rep})")
    head = [
        "ISO-10303-21;",
        "HEADER;",
        "FILE_DESCRIPTION(('the case of the netsmith example RP2350A Minimal, MIT license, written by make_case.py'),'2;1');",
        f"FILE_NAME('{name}','1970-01-01T00:00:00',(''),(''),'make_case.py','','');",
        "FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));",
        "ENDSEC;",
        "DATA;",
    ]
    return "\n".join(head + lines + ["ENDSEC;", "END-ISO-10303-21;", ""])


def main():
    here = Path(__file__).resolve().parent
    for name, solids in (("base", base_solids()), ("lid", lid_solids())):
        (here / f"{name}.step").write_text(step(name, solids), encoding="ascii", newline="\n")
        print(f"{name}.step: {len(solids)} solids")
    print(f"place both at .x = {OX:g}mm, .y = {OY:g}mm; the base at .z = {OZ:g}mm, the lid at .z = {LID_Z:g}mm")


if __name__ == "__main__":
    main()
