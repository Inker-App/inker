#!/usr/bin/env python3
"""Write the INKER app icon as SVG (and an HTML wrapper for rasterizing).

The icon is the 7x7 MARK bitmap as halftone dots on a dark rounded square,
the same drawing as the site favicon, laid out on Apple's 1024 px template:
an 824 px square centred with 100 px of transparent margin.

    make-icon.py --svg Resources/AppIcon.svg --html /tmp/icon.html

`make icon` (macOS) turns Resources/AppIcon-1024.png into AppIcon.icns.
"""

import argparse
import pathlib

MARK = [
    ".#####.",
    "#######",
    "##.#.##",
    "#######",
    "#.#.#.#",
    "#.#.#.#",
    ".#.#.#.",
]
CANVAS = 1024
SQUARE = 824
MARGIN = (CANVAS - SQUARE) // 2
RADIUS = 185  # Apple's template corner, ~22.4% of the square
BACKGROUND = "#0D0F10"
PAPER = "#E6E4DE"
# The favicon's grid: 36 units across, dots every 4 units from unit 6.
UNITS = 36
PITCH = 4
FIRST = 6


def dots():
    """(cx, cy, r) in pixels for every inked cell of the mark."""
    scale = SQUARE / UNITS
    out = []
    for row, line in enumerate(MARK):
        for col, ch in enumerate(line):
            if ch != "#":
                continue
            cx = MARGIN + (FIRST + col * PITCH) * scale
            cy = MARGIN + (FIRST + row * PITCH) * scale
            out.append((round(cx, 2), round(cy, 2), round(PITCH / 2 * scale, 2)))
    return out


def svg():
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{CANVAS}" height="{CANVAS}" viewBox="0 0 {CANVAS} {CANVAS}">',
        f'<rect x="{MARGIN}" y="{MARGIN}" width="{SQUARE}" height="{SQUARE}" rx="{RADIUS}" fill="{BACKGROUND}"/>',
        f'<g fill="{PAPER}">',
    ]
    parts += [f'<circle cx="{cx}" cy="{cy}" r="{r}"/>' for cx, cy, r in dots()]
    parts += ["</g>", "</svg>"]
    return "\n".join(parts) + "\n"


def html():
    return (
        "<!doctype html><html><head><meta charset='utf-8'><style>"
        "html,body{margin:0;background:transparent;width:1024px;height:1024px;overflow:hidden}"
        "</style></head><body>" + svg() + "</body></html>\n"
    )


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--svg", type=pathlib.Path)
    ap.add_argument("--html", type=pathlib.Path)
    args = ap.parse_args()
    if args.svg:
        args.svg.write_text(svg())
    if args.html:
        args.html.write_text(html())


if __name__ == "__main__":
    main()
