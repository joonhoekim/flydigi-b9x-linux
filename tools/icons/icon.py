#!/usr/bin/env python3
"""Writes the B9X app icon: the cooler seen from the front. A white housing with a
slightly smaller module on it: a glowing RGB ring around a triangular grille.

Proportions follow a product photo; nothing from the photo itself is used, and the
maker's logo is left out.

    tools/icons/icon.py            # write data/icons/hicolor/scalable/apps/<app id>.svg
    tools/icons/icon.py --preview  # also tools/icons/preview.svg, at 128, 64 and 32 px
"""
import colorsys
import math
import sys
from pathlib import Path

APP_ID = "io.github.joonhoekim.FlydigiB9X"

TILT = 0
FRONT = (64.0, 61.0, 55.0, 55.0)   # housing: cx, cy, rx, ry
DEPTH = (2.2, 3.4)                 # drop shadow offset: light from the upper left
MODULE_SCALE = 0.92                # ring module diameter relative to the housing
RING_WIDTH = 6.6


def f(v):
    return f"{v:.2f}".rstrip("0").rstrip(".")


def moved(e, k):
    """Ellipse `e` moved by `k` times the shadow offset."""
    return (e[0] + DEPTH[0] * k, e[1] + DEPTH[1] * k, e[2], e[3])


def scaled(e, s, dr=0.0):
    return (e[0], e[1], e[2] * s + dr * e[2] / e[3], e[3] * s + dr)


def ell(e, **attrs):
    cx, cy, rx, ry = e
    extra = "".join(f' {k.replace("_", "-")}="{v}"' for k, v in attrs.items())
    return (f'<ellipse cx="{f(cx)}" cy="{f(cy)}" rx="{f(rx)}" ry="{f(ry)}" '
            f'transform="rotate({TILT} {f(cx)} {f(cy)})"{extra}/>')


def point(e, t):
    """Point at angle t (radians, 0 = right, clockwise) on the tilted ellipse."""
    cx, cy, rx, ry = e
    x, y = rx * math.cos(t), ry * math.sin(t)
    a = math.radians(TILT)
    return cx + x * math.cos(a) - y * math.sin(a), cy + x * math.sin(a) + y * math.cos(a)


MODULE = scaled(FRONT, MODULE_SCALE)
RING = scaled(MODULE, 1, -(RING_WIDTH / 2 + 0.8))
GRILLE = scaled(MODULE, 1, -(RING_WIDTH + 2.4))


def ring_color(t):
    """Colour at fraction t clockwise from the top, as on the device: violet, magenta,
    red, orange, yellow, green, cyan, then washed out to near white at the upper left
    where the light catches it."""
    h = (0.8 + t) % 1
    glare = max(0.0, math.cos(2 * math.pi * (t - 0.86)))   # around 10 o'clock
    s = 0.95 - 0.75 * glare ** 2
    l = 0.66 + 0.2 * glare ** 2
    r, g, b = colorsys.hls_to_rgb(h, l, s)
    return "#%02x%02x%02x" % (round(r * 255), round(g * 255), round(b * 255))


def ring(width, segments=60):
    out = []
    for i in range(segments):
        t0 = 2 * math.pi * i / segments - math.pi / 2
        t1 = 2 * math.pi * (i + 1.1) / segments - math.pi / 2
        (x0, y0), (x1, y1) = point(RING, t0), point(RING, t1)
        out.append(f'<path d="M{f(x0)} {f(y0)}A{f(RING[2])} {f(RING[3])} {TILT} 0 1 {f(x1)} {f(y1)}" '
                   f'stroke="{ring_color(i / segments)}" stroke-width="{f(width)}" fill="none"/>')
    return "".join(out)


def grille_path(step=4.9):
    """Struts in three directions leave the triangular holes."""
    cx, cy = GRILLE[:2]
    d = []
    reach = 60
    for ang in (TILT, TILT + 60, TILT + 120):
        t = math.radians(ang)
        dx, dy = math.cos(t), math.sin(t)
        nx, ny = -dy, dx
        # Every family has a line through the centre, so all three meet at the
        # same points and the holes are triangles.
        n = int(reach / step)
        for i in range(-n, n + 1):
            px, py = cx + nx * i * step, cy + ny * i * step
            d.append(f"M{f(px - dx * reach)} {f(py - dy * reach)}L{f(px + dx * reach)} {f(py + dy * reach)}")
    return "".join(d)


def grille(width=1.15):
    """Raised metal struts: a shadow cast down-right into the holes, the strut in a
    silver gradient, and a thin highlight on its upper-left edge."""
    d = grille_path()
    return (f'<path d="{d}" stroke="#08115e" stroke-opacity="0.6" stroke-width="{f(width + 0.2)}" fill="none" transform="translate(0.35 0.5)"/>'
            f'<path d="{d}" stroke="url(#strut)" stroke-width="{f(width)}" fill="none"/>'
            f'<path d="{d}" stroke="#ffffff" stroke-opacity="0.6" stroke-width="0.35" fill="none" transform="translate(-0.2 -0.25)"/>')


def fan_blades(n=7):
    """The fan behind the grille: curved blades around a hub, darker than the air between."""
    cx, cy = GRILLE[:2]
    r0, r1 = 7, GRILLE[3] + 2
    out = []
    for i in range(n):
        a = 2 * math.pi * i / n
        sweep = 0.9    # radians the blade turns from hub to tip
        width = 0.42   # radians of blade at the tip
        p0 = (cx + r0 * math.cos(a), cy + r0 * math.sin(a))
        p1 = (cx + r1 * math.cos(a + sweep), cy + r1 * math.sin(a + sweep))
        p2 = (cx + r1 * math.cos(a + sweep + width), cy + r1 * math.sin(a + sweep + width))
        p3 = (cx + r0 * math.cos(a + 0.5), cy + r0 * math.sin(a + 0.5))
        c1 = (cx + r1 * 0.55 * math.cos(a + 0.15), cy + r1 * 0.55 * math.sin(a + 0.15))
        c2 = (cx + r1 * 0.6 * math.cos(a + sweep + width + 0.1), cy + r1 * 0.6 * math.sin(a + sweep + width + 0.1))
        out.append(f"M{f(p0[0])} {f(p0[1])}Q{f(c1[0])} {f(c1[1])} {f(p1[0])} {f(p1[1])}"
                   f"L{f(p2[0])} {f(p2[1])}Q{f(c2[0])} {f(c2[1])} {f(p3[0])} {f(p3[1])}Z")
    return f'<path d="{"".join(out)}" fill="#0f1a8f" fill-opacity="0.55"/>'


def snowflake(cx, cy, r, width):
    d = []
    for i in range(6):
        a = math.radians(60 * i - 90 + TILT)
        d.append(f"M{f(cx)} {f(cy)}L{f(cx + r * math.cos(a))} {f(cy + r * math.sin(a))}")
        bx, by = cx + 0.55 * r * math.cos(a), cy + 0.55 * r * math.sin(a)
        for s in (-1, 1):
            b = a + s * math.radians(42)
            d.append(f"M{f(bx)} {f(by)}L{f(bx + 0.38 * r * math.cos(b))} {f(by + 0.38 * r * math.sin(b))}")
    return f'<path d="{"".join(d)}" stroke="#ffffff" stroke-width="{f(width)}" stroke-linecap="round" fill="none"/>'


def badge():
    """Small rounded hexagon with a snowflake where the maker's logo would be."""
    cx, cy, r = GRILLE[0], GRILLE[1], 9
    pts = [(cx + r * math.cos(math.radians(a + TILT)), cy + r * math.sin(math.radians(a + TILT))) for a in range(-90, 270, 60)]
    d = "M" + "L".join(f"{f(x)} {f(y)}" for x, y in pts) + "Z"
    return (f'<path d="{d}" fill="#0a1a7a" fill-opacity="0.45" stroke="#0a1a7a" stroke-opacity="0.45" stroke-width="3.5" '
            f'stroke-linejoin="round" transform="translate(0.8 1.2)"/>'
            f'<path d="{d}" fill="url(#badge)" stroke="url(#badge)" stroke-width="3.5" stroke-linejoin="round"/>'
            + snowflake(cx, cy, 6.2, 1.8))


def content():
    return "\n".join([
        "<defs>",
        '<filter id="glow" x="-25%" y="-25%" width="150%" height="150%"><feGaussianBlur stdDeviation="2.2"/></filter>',
        '<filter id="soft" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="1.6"/></filter>',
        '<filter id="ground" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="2.6"/></filter>',
        '<linearGradient id="front" x1="0.1" y1="0" x2="0.9" y2="1"><stop offset="0" stop-color="#ffffff"/>'
        '<stop offset="1" stop-color="#e6e9ef"/></linearGradient>',
        '<linearGradient id="rim" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#ffffff"/>'
        '<stop offset="1" stop-color="#edf0f4"/></linearGradient>',
        # The blue fan behind the grille.
        '<radialGradient id="fan" cx="0.45" cy="0.4" r="0.7"><stop offset="0" stop-color="#3347ff"/>'
        '<stop offset="0.7" stop-color="#1f2fd9"/><stop offset="1" stop-color="#1a22a8"/></radialGradient>',
        # Ring light caught in the holes: pink and violet at the upper right, cyan and
        # green at the lower left, as in the photo.
        '<radialGradient id="glowPink" cx="0.78" cy="0.22" r="0.55"><stop offset="0" stop-color="#ff5fe0" stop-opacity="0.6"/>'
        '<stop offset="1" stop-color="#ff5fe0" stop-opacity="0"/></radialGradient>',
        '<radialGradient id="glowCyan" cx="0.2" cy="0.8" r="0.55"><stop offset="0" stop-color="#3dfcd0" stop-opacity="0.55"/>'
        '<stop offset="1" stop-color="#3dfcd0" stop-opacity="0"/></radialGradient>',
        '<radialGradient id="glowViolet" cx="0.35" cy="0.15" r="0.45"><stop offset="0" stop-color="#a77bff" stop-opacity="0.55"/>'
        '<stop offset="1" stop-color="#a77bff" stop-opacity="0"/></radialGradient>',
        '<radialGradient id="vignette" cx="0.5" cy="0.5" r="0.5"><stop offset="0.7" stop-color="#050a40" stop-opacity="0"/>'
        '<stop offset="1" stop-color="#050a40" stop-opacity="0.55"/></radialGradient>',
        '<linearGradient id="strut" gradientUnits="userSpaceOnUse" x1="30" y1="25" x2="98" y2="97">'
        '<stop offset="0" stop-color="#ffffff"/><stop offset="0.5" stop-color="#e2e6ee"/>'
        '<stop offset="1" stop-color="#b8c0ce"/></linearGradient>',
        '<linearGradient id="bezel" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#9aa3b4"/>'
        '<stop offset="0.5" stop-color="#eef1f6"/><stop offset="1" stop-color="#8e97a8"/></linearGradient>',
        '<linearGradient id="badge" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#4a74ff"/>'
        '<stop offset="1" stop-color="#1b3bd6"/></linearGradient>',
        f'<clipPath id="grille">{ell(GRILLE)}</clipPath>',
        f'<clipPath id="frontClip">{ell(FRONT)}</clipPath>',
        "</defs>",
        # housing with a soft drop shadow
        ell(moved(FRONT, 1), fill="#1b2233", fill_opacity="0.35", filter="url(#ground)"),
        ell(FRONT, fill="url(#front)"),
        ell(scaled(FRONT, 1, -0.7), fill="none", stroke="#ffffff", stroke_width="1.4"),
        ell(scaled(FRONT, 1, -0.4), fill="none", stroke="#b9c0cc", stroke_opacity="0.6", stroke_width="0.8"),
        # the ring module: its shadow on the housing, then its face
        f'<g clip-path="url(#frontClip)">{ell(moved(MODULE, 0.5), fill="#5a6478", fill_opacity="0.3", filter="url(#soft)")}</g>',
        ell(MODULE, fill="url(#rim)"),
        f'<g filter="url(#glow)" opacity="0.75">{ring(RING_WIDTH * 1.6)}</g>',
        ring(RING_WIDTH),
        ell(scaled(RING, 1, -RING_WIDTH / 2), fill="none", stroke="#ffffff", stroke_opacity="0.6", stroke_width="1.1"),
        ell(GRILLE, fill="url(#fan)"),
        f'<g clip-path="url(#grille)">{fan_blades()}'
        + ell(GRILLE, fill="url(#glowViolet)") + ell(GRILLE, fill="url(#glowPink)") + ell(GRILLE, fill="url(#glowCyan)")
        + ell(GRILLE, fill="url(#vignette)") + grille() + "</g>",
        ell(GRILLE, fill="none", stroke="url(#bezel)", stroke_width="1.8"),
        badge(),
    ])


def svg(size=128):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 128 128">\n'
            f"{content()}\n</svg>\n")


def preview():
    body = content()
    out = ['<svg xmlns="http://www.w3.org/2000/svg" width="560" height="300" viewBox="0 0 560 300">',
           '<rect width="280" height="300" fill="#f6f6f8"/><rect x="280" width="280" height="300" fill="#242428"/>']
    for ox in (0, 280):
        for size, x, y in ((128, 20, 30), (64, 168, 62), (32, 196, 200)):
            b = body.replace('id="', f'id="p{ox}{size}').replace("url(#", f"url(#p{ox}{size}")
            out.append(f'<svg x="{ox + x}" y="{y}" width="{size}" height="{size}" viewBox="0 0 128 128">{b}</svg>')
    out.append("</svg>")
    return "\n".join(out) + "\n"


if __name__ == "__main__":
    here = Path(__file__).parent
    dest = here.parent.parent / "data" / "icons" / "hicolor" / "scalable" / "apps" / f"{APP_ID}.svg"
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(svg())
    print(dest)
    if "--preview" in sys.argv:
        (here / "preview.svg").write_text(preview())
        print(here / "preview.svg")
