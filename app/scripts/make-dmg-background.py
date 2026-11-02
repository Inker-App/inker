#!/usr/bin/env python3
"""Draw the disk image window: the octopus, the arrow, the layout.

The Finder window of the .dmg is a picture plus pinned icon positions, so
this script owns both. It ports the WorldOctopus board's raymarcher (the
same drawing the app renders in Metal) to Python and freezes one frame as
halftone dots: the octopus peeks up between the two icons, eyes on the
Applications folder, and a dotted arrow points the way.

    make-dmg-background.py --out app/dmg

writes background.png (1x), background@2x.png and layout.json. The release
build (scripts/make-dmg.sh) joins the PNGs into a Retina TIFF and feeds the
layout to dmgbuild. Pure Python, no image library: the PNG writer is here.
"""

import argparse
import json
import math
import pathlib
import struct
import sys
import zlib

BACKGROUND = "#0D0F10"
PAPER = "#E6E4DE"

# Points, 1x. Finder places icons by their centre.
LAYOUT = {
    "width": 660,
    "height": 480,
    "icon_size": 128,
    "text_size": 14,
    "inker": [165, 120],
    "applications": [495, 120],
}
# Head below the icon row, arms running off the bottom edge, turned a little
# toward Applications. `time` picks the frame of the board's sway: at 1.7 the
# left arm is raised toward the INKER icon.
OCTOPUS = {"centre": [330, 405], "scale": 150, "time": 1.7, "cell": 6, "tilt": 0.45, "yaw": 0.25}

# --- The octopus, as the board draws it -------------------------------------

TN, SEG, STEPS = 8, 8, 56
EYE = [(-0.25, -0.40, -0.5), (0.25, -0.40, -0.5)]
MATERIAL_BODY, MATERIAL_EYE = 1, 2
# A still frame has no blink or glance to sell the eyes, so they are brighter
# and the pupil slit wider than on the board (0.25 + 0.75 * lam, 0.075 x 0.028).
EYE_WHITE = (0.55, 0.45)
PUPIL = (0.09, 0.05)


def n3(x, y, z):
    l = math.hypot(x, y, z) or 1
    return (x / l, y / l, z / l)


def rx(a):
    c, s = math.cos(a), math.sin(a)
    return [1, 0, 0, 0, c, -s, 0, s, c]


def ry(a):
    c, s = math.cos(a), math.sin(a)
    return [c, 0, s, 0, 1, 0, -s, 0, c]


def mm(A, B):
    return [sum(A[i * 3 + k] * B[k * 3 + j] for k in range(3)) for i in range(3) for j in range(3)]


def tr(A):
    return [A[0], A[3], A[6], A[1], A[4], A[7], A[2], A[5], A[8]]


def mul(M, v):
    return (
        M[0] * v[0] + M[1] * v[1] + M[2] * v[2],
        M[3] * v[0] + M[4] * v[1] + M[5] * v[2],
        M[6] * v[0] + M[7] * v[1] + M[8] * v[2],
    )


def clamp(x, lo=-1.0, hi=1.0):
    return lo if x < lo else hi if x > hi else x


def build(t):
    """`build(T)`: eight tentacles, each 9 points with radii and a bounding sphere."""
    out = []
    for i in range(TN):
        a = i / TN * 6.2832 + 0.2 + 0.12 * math.sin(t * 0.7 + i * 1.3)
        ux, uz = math.cos(a), math.sin(a)
        th, px, py, pz = 1.25, ux * 0.32, 0.0, uz * 0.32
        pts, rad = [], []
        for k in range(SEG + 1):
            pts.append((px, py, pz))
            rad.append(0.16 * (1 - k / (SEG + 1.5)) + 0.02)
            th += -(0.06 + 0.42 * k / SEG) * (1 + 0.3 * math.sin(t * 1.4 + i * 1.9))
            cs, sn = math.cos(th), math.sin(th)
            px += ux * cs * 0.21
            pz += uz * cs * 0.21
            py += sn * 0.21
        mx = sum(p[0] for p in pts) / (SEG + 1)
        my = sum(p[1] for p in pts) / (SEG + 1)
        mz = sum(p[2] for p in pts) / (SEG + 1)
        br = max(math.hypot(p[0] - mx, p[1] - my, p[2] - mz) + r for p, r in zip(pts, rad))
        out.append({"pts": pts, "rad": rad, "bc": (mx, my, mz), "br": br})
    return out


def sdf(x, y, z, tentacles):
    """(distance, material, eye index) for a point: eye index is -1 off the eyes."""
    hx, hy, hz = x / 0.62, (y + 0.55) / 0.74, z / 0.6
    k0 = math.sqrt(hx * hx + hy * hy + hz * hz)
    k1 = math.sqrt(hx * hx / 0.3844 + hy * hy / 0.5476 + hz * hz / 0.36)
    d = k0 * (k0 - 1) / k1 if k1 else -0.6  # the ellipsoid centre, where the formula is 0/0
    for o in tentacles:
        bx, by, bz = x - o["bc"][0], y - o["bc"][1], z - o["bc"][2]
        if math.sqrt(bx * bx + by * by + bz * bz) - o["br"] > d + 0.15:
            continue
        dm = 1e9
        pts, rad = o["pts"], o["rad"]
        for k in range(SEG):
            ax, ay, az = pts[k]
            vx, vy, vz = pts[k + 1][0] - ax, pts[k + 1][1] - ay, pts[k + 1][2] - az
            qx, qy, qz = x - ax, y - ay, z - az
            h = (qx * vx + qy * vy + qz * vz) / (vx * vx + vy * vy + vz * vz)
            h = 0.0 if h < 0 else 1.0 if h > 1 else h
            ex, ey, ez = qx - vx * h, qy - vy * h, qz - vz * h
            dc = math.sqrt(ex * ex + ey * ey + ez * ez) - (rad[k] + (rad[k + 1] - rad[k]) * h)
            if dc < dm:
                dm = dc
        hh = max(0.14 - abs(d - dm), 0) / 0.14
        d = min(d, dm) - hh * hh * 0.035
    de, ei = 1e9, -1
    for e, E in enumerate(EYE):
        q = math.hypot(x - E[0], y - E[1], z - E[2]) - 0.12
        if q < de:
            de, ei = q, e
    if de < d:
        return de, MATERIAL_EYE, ei
    return d, MATERIAL_BODY, -1


def octopus_dots(width, height, cell, centre, scale, time, look, tilt=0.45, yaw=None):
    """(x, y, r) in points for every lit cell of one raymarched frame."""
    tentacles = build(time)
    cx0, cy0 = centre
    if yaw is None:
        yaw = 0.35 * math.sin(time * 0.25)  # the board's sway at this moment
    M = tr(mm(rx(tilt), ry(yaw)))
    L = mul(M, n3(-0.5, -0.65, -0.6))
    V = mul(M, (0, 0, -1))
    Hh = n3(L[0] + V[0], L[1] + V[1], L[2] + V[2])
    D = mul(M, (0, 0, 1))
    half = cell / 2
    dots = []
    for j in range(int(height // cell)):
        for i in range(int(width // cell)):
            X = ((i + 0.5) * cell - cx0) / scale
            Y = ((j + 0.5) * cell - cy0) / scale
            o = mul(M, (X, Y, -4))
            ox, oy, oz = o[0], o[1] + 0.1, o[2]
            bb = ox * D[0] + oy * D[1] + oz * D[2]
            cc = ox * ox + oy * oy + oz * oz - 4.0
            disc = bb * bb - cc
            b = 0.0
            if disc > 0:
                sq = math.sqrt(disc)
                t1, tt = -bb + sq, max(0.0, -bb - sq)
                hit = False
                px = py = pz = 0.0
                for _ in range(STEPS):
                    if tt >= t1:
                        break
                    px, py, pz = o[0] + D[0] * tt, o[1] + D[1] * tt, o[2] + D[2] * tt
                    dd, _, _ = sdf(px, py, pz, tentacles)
                    if dd < 0.004:
                        hit = True
                        break
                    tt += dd * 0.9
                if hit:
                    _, material, eye = sdf(px, py, pz, tentacles)
                    e = 0.005
                    a = sdf(px + e, py - e, pz - e, tentacles)[0]
                    b2 = sdf(px - e, py - e, pz + e, tentacles)[0]
                    c = sdf(px - e, py + e, pz - e, tentacles)[0]
                    d3 = sdf(px + e, py + e, pz + e, tentacles)[0]
                    nx, ny, nz = a - b2 - c + d3, -a - b2 + c + d3, -a + b2 - c + d3
                    nl = math.hypot(nx, ny, nz) or 1
                    nx, ny, nz = nx / nl, ny / nl, nz / nl
                    lam = max(0, nx * L[0] + ny * L[1] + nz * L[2])
                    nv = max(0, nx * V[0] + ny * V[1] + nz * V[2])
                    rim = (1 - nv) ** 3
                    sp = max(0, nx * Hh[0] + ny * Hh[1] + nz * Hh[2]) ** 36
                    if material == MATERIAL_EYE:
                        E = EYE[eye]
                        qx = px - E[0] - look[0] * 0.04
                        qy = py - E[1] - look[1] * 0.03
                        qz = pz - E[2]
                        b = EYE_WHITE[0] + EYE_WHITE[1] * lam
                        if qz < -0.06 and abs(qy) < PUPIL[1] and abs(qx) < PUPIL[0]:
                            b = 0.02
                        b += sp * 0.35
                    else:
                        tex = 0.88 + 0.12 * math.sin(px * 23 + math.sin(pz * 17)) * math.sin(py * 19)
                        b = (0.05 + 0.75 * lam) * tex + 0.14 * rim + sp * 0.08
            if b > 0.015:
                dots.append(((i + 0.5) * cell, (j + 0.5) * cell, half * min(1.05, math.sqrt(b))))
    return dots


def look_at(target):
    """The board's cursor-to-look mapping, aimed at a point in the window."""
    cx0, cy0 = OCTOPUS["centre"]
    return (clamp((target[0] - cx0) / 400), clamp((target[1] - cy0 + 120) / 400))


# --- The arrow ---------------------------------------------------------------


def arrow_dots(gap=24, pitch=10, r=2.5):
    """A dotted line from the INKER icon to Applications, with a chevron head."""
    (ix, y), (ax, _) = LAYOUT["inker"], LAYOUT["applications"]
    half = LAYOUT["icon_size"] / 2
    x0, x1 = ix + half + gap, ax - half - gap
    dots = []
    x = x0
    while x <= x1 - pitch:
        dots.append((x, y, r))
        x += pitch
    for k in range(1, 4):
        dots.append((x1 - k * pitch * 0.8, y - k * pitch * 0.8, r))
        dots.append((x1 - k * pitch * 0.8, y + k * pitch * 0.8, r))
    dots.append((x1, y, r))
    return dots


# --- PNG ---------------------------------------------------------------------


def rgb(hex6):
    return tuple(int(hex6[i : i + 2], 16) for i in (1, 3, 5))


def png(width, height, dots, scale, background, paper):
    """An 8-bit RGB PNG of anti-aliased dots on a flat background."""
    W, H = width * scale, height * scale
    bg, fg = rgb(background), rgb(paper)
    cover = bytearray(W * H)  # 0..255 paper coverage per pixel
    for x, y, r in dots:
        cx, cy, cr = x * scale, y * scale, r * scale
        for py in range(max(0, int(cy - cr - 1)), min(H, int(cy + cr + 2))):
            row = py * W
            for px in range(max(0, int(cx - cr - 1)), min(W, int(cx + cr + 2))):
                d = math.hypot(px + 0.5 - cx, py + 0.5 - cy)
                a = clamp(cr - d + 0.5, 0.0, 1.0)
                if a > 0:
                    v = int(a * 255 + 0.5)
                    if v > cover[row + px]:
                        cover[row + px] = v
    raw = bytearray()
    for py in range(H):
        raw.append(0)
        row = py * W
        for px in range(W):
            a = cover[row + px]
            if a == 0:
                raw += bytes(bg)
            elif a == 255:
                raw += bytes(fg)
            else:
                raw += bytes((bg[k] * (255 - a) + fg[k] * a) // 255 for k in range(3))

    def chunk(kind, body):
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


# --- Main --------------------------------------------------------------------


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--out", type=pathlib.Path, required=True, help="directory for background.png, background@2x.png, layout.json")
    ap.add_argument("--cell", type=int, default=OCTOPUS["cell"], help="dot pitch in points")
    ap.add_argument("--quick", action="store_true", help="skip the octopus (tests)")
    args = ap.parse_args(argv)
    args.out.mkdir(parents=True, exist_ok=True)
    W, H = LAYOUT["width"], LAYOUT["height"]
    dots = list(arrow_dots())
    if not args.quick:
        dots += octopus_dots(W, H, args.cell, OCTOPUS["centre"], OCTOPUS["scale"], OCTOPUS["time"], look_at(LAYOUT["applications"]), OCTOPUS["tilt"], OCTOPUS["yaw"])
    (args.out / "background.png").write_bytes(png(W, H, dots, 1, BACKGROUND, PAPER))
    (args.out / "background@2x.png").write_bytes(png(W, H, dots, 2, BACKGROUND, PAPER))
    (args.out / "layout.json").write_text(json.dumps(LAYOUT, indent=2) + "\n")
    print(f"{len(dots)} dots -> {args.out}", file=sys.stderr)


if __name__ == "__main__":
    main()
