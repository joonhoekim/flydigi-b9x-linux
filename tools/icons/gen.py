#!/usr/bin/env python3
"""Writes the B9X app icon drafts as SVG beside this file, in the dot-grid style of
the subline icons (github.com/joonhoekim/subline, tools/icons/gen.py).

Each map is a square grid: '#' on, '+' dim, '.' off, 'R' red, 'B' blue.
Grids of any size are centred, so an odd size gives a symmetric figure an exact centre.

    tools/icons/gen.py            # write the drafts, and APP_ICON as the app icon
    tools/icons/gen.py --preview  # also write preview.svg, every draft at 128 and 48 px
"""
import sys
from pathlib import Path

APP_ID = "io.github.joonhoekim.FlydigiB9X"
# The draft installed as the app icon, into data/icons/.
APP_ICON = "front"

ON, RED, BLUE = "#ffffff", "#ff453a", "#3d8bff"
GLOW = 0.22
GLOW_ACCENT = min(1.0, GLOW * 1.6)
PITCH, R, R_HALO = 8.5, 3.1, 4.9

MAPS = {
    # Something cold: an eight-armed snowflake, blue at the heart.
    "snowflake": [
        "....+#+....",
        ".+...#...+.",
        "..#..#..#..",
        "...#.#.#...",
        "+...###...+",
        "#####B#####",
        "+...###...+",
        "...#.#.#...",
        "..#..#..#..",
        ".+...#...+.",
        "....+#+....",
    ],
    # The cooler from the front: lit ring, mesh inside. The ring is RGB on the
    # device, hence a red and a blue dot on it.
    "front": [
        "............",
        "....####....",
        "..##+.+.#R..",
        ".##+.+.+.##.",
        ".#+.+.+.+.#.",
        ".#.+.+.+.+#.",
        ".#+.+.+.+.#.",
        ".#.+.+.+.+#.",
        ".##.+.+.+##.",
        "..B#.+.+##..",
        "....####....",
        "............",
    ],
    # Seen from the front left: the face as a narrower ellipse, the body behind it
    # with vent slots on its side.
    "angle": [
        "............",
        "...#######..",
        "..#..#+.+.R.",
        ".#++#+.+.+.#",
        ".#..#.+.+.+#",
        ".#++#+.+.+.#",
        ".#..#.+.+.+#",
        ".#++#+.+.+.#",
        ".#..#.+.+.+#",
        "..#..B.+.+#.",
        "...#######..",
        "............",
    ],
}

KIND = {"#": "on", "+": "dim", "R": "red", "B": "blue"}


def num(v):
    return f"{v:.2f}".rstrip("0").rstrip(".")


def dot(cx, cy, r):
    return f"M{num(cx - r)} {num(cy)}a{num(r)} {num(r)} 0 1 0 {num(2 * r)} 0a{num(r)} {num(r)} 0 1 0 {num(-2 * r)} 0"


def body(rows):
    n = len(rows)
    origin = 64 - PITCH * (n - 1) / 2
    paths = {k: [] for k in ("off", "dim", "on", "red", "blue", "onHalo", "redHalo", "blueHalo")}
    for y, row in enumerate(rows):
        for x, c in enumerate(row):
            k = KIND.get(c, "off")
            cx, cy = origin + PITCH * x, origin + PITCH * y
            paths[k].append(dot(cx, cy, R))
            if k in ("on", "red", "blue"):
                paths[k + "Halo"].append(dot(cx, cy, R_HALO))
    layers = [
        ("off", 'fill="#ffffff" fill-opacity="0.1"'),
        ("dim", 'fill="#f0f0f0" fill-opacity="0.38"'),
        ("onHalo", f'fill="{ON}" fill-opacity="{num(GLOW)}"'),
        ("redHalo", f'fill="{RED}" fill-opacity="{num(GLOW_ACCENT)}"'),
        ("blueHalo", f'fill="{BLUE}" fill-opacity="{num(GLOW_ACCENT)}"'),
        ("on", f'fill="{ON}"'),
        ("red", f'fill="{RED}"'),
        ("blue", f'fill="{BLUE}"'),
    ]
    out = ['<rect x="4" y="4" width="120" height="120" rx="28" fill="#000000" stroke="#ffffff" stroke-opacity="0.16" stroke-width="2"/>']
    out += [f'<path {attrs} d="{"".join(paths[k])}"/>' for k, attrs in layers if paths[k]]
    return "\n".join(out)


def icon(rows):
    return f'<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 128 128">\n{body(rows)}\n</svg>\n'


def preview():
    """All drafts side by side at 128 px, and at 48 px below, on a mid-grey background."""
    names = list(MAPS)
    w, h = 40 + 168 * len(names), 260
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}" viewBox="0 0 {w} {h}">',
           f'<rect width="{w}" height="{h}" fill="#3a3a3c"/>']
    for i, name in enumerate(names):
        x = 40 + 168 * i
        out.append(f'<svg x="{x}" y="20" width="128" height="128" viewBox="0 0 128 128">{body(MAPS[name])}</svg>')
        out.append(f'<svg x="{x + 40}" y="160" width="48" height="48" viewBox="0 0 128 128">{body(MAPS[name])}</svg>')
        out.append(f'<text x="{x + 64}" y="240" fill="#ffffff" font-family="sans-serif" font-size="16" text-anchor="middle">{name}</text>')
    out.append("</svg>")
    return "\n".join(out) + "\n"


if __name__ == "__main__":
    here = Path(__file__).parent
    for name, rows in MAPS.items():
        assert all(len(r) == len(rows) for r in rows), name
        (here / f"{name}.svg").write_text(icon(rows))
        print(here / f"{name}.svg")
    if APP_ICON:
        dest = here.parent.parent / "data" / "icons" / "hicolor" / "scalable" / "apps" / f"{APP_ID}.svg"
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(icon(MAPS[APP_ICON]))
        print(dest)
    if "--preview" in sys.argv:
        (here / "preview.svg").write_text(preview())
        print(here / "preview.svg")
