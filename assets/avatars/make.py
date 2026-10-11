#!/usr/bin/env python3
"""Forge's avatars: twelve faceless, cute head-and-shoulders portraits, six
women (one in a hijab) and six men, told apart by hair and accessories (no eyes, mouth or
nose). Writes one SVG per avatar next to this file, on a 64-unit viewBox;
the app sets the pixel size when it draws one.

    python3 assets/avatars/make.py
"""
import colorsys
import math
import os

OUT = os.path.dirname(os.path.abspath(__file__))

WOMEN = [("bow", 345), ("spacebuns", 20), ("flowerbun", 262), ("ponytail", 0), ("headband", 174), ("hijab", 300)]
MEN = [("quiff", 215), ("beanie", 140), ("cap", 32), ("beard", 190), ("curls", 48), ("phones", 280)]

CX, CY, R = 32, 26.5, 13.6  # the head


def hsl(h, s, l):
    r, g, b = colorsys.hls_to_rgb(h / 360, l / 100, s / 100)
    return "#%02x%02x%02x" % (round(r * 255), round(g * 255), round(b * 255))


def pt(r, deg):
    return (CX + r * math.cos(math.radians(deg)), CY - r * math.sin(math.radians(deg)))


def cap(r=14.6, a0=200, a1=-20, line=22.2):
    """Hair over the crown: an arc round the top of the head, back along a soft hairline."""
    (x0, y0), (x1, y1) = pt(r, a0), pt(r, a1)
    return (f"M{x0:.2f} {y0:.2f}A{r} {r} 0 1 1 {x1:.2f} {y1:.2f}"
            f"C{x1-2:.2f} {line+3:.2f} {CX+6:.2f} {line:.2f} {CX:.2f} {line:.2f}"
            f"C{CX-6:.2f} {line:.2f} {x0+2:.2f} {line+3:.2f} {x0:.2f} {y0:.2f}Z")


def parts(style, hair, accent):
    """What goes behind the head, and what goes over it."""
    P = lambda d, c=hair: f'<path d="{d}" fill="{c}"/>'
    C = lambda x, y, r, c=hair: f'<circle cx="{x:.2f}" cy="{y:.2f}" r="{r}" fill="{c}"/>'
    long_back = lambda: P(f"M{CX-16} {CY+20}C{CX-17.8} {CY+2} {CX-16.4} {CY-15.6} {CX} {CY-15.8}"
                          f"C{CX+16.4} {CY-15.6} {CX+17.8} {CY+2} {CX+16} {CY+20}Z")
    back = front = ""
    if style == "bow":
        back = long_back()
        front = (P(cap()) + P(f"M{CX+6} {CY-14.8}l6.6 -4.4c1.3 -.9 2.7 .1 2.7 1.5v6.8c0 1.4 -1.4 2.4 -2.7 1.5z"
                             f"M{CX+6} {CY-14.8}l-6.6 -4.4c-1.3 -.9 -2.7 .1 -2.7 1.5v6.8c0 1.4 1.4 2.4 2.7 1.5z", accent)
                 + C(CX + 6, CY - 14.8, 2.2, accent))
    elif style == "spacebuns":
        front = C(CX - 11.2, CY - 12.8, 5.2) + C(CX + 11.2, CY - 12.8, 5.2) + P(cap(line=22.0))
    elif style == "flowerbun":
        petals = "".join(C(CX + 5.4 + 2.2 * math.cos(math.radians(a)), CY - 18.4 - 2.2 * math.sin(math.radians(a)), 1.9, accent) for a in range(0, 360, 72))
        front = C(CX, CY - 16.2, 5.6) + P(cap(line=22.4)) + petals + C(CX + 5.4, CY - 18.4, 1.4, "#fff4c2")
    elif style == "ponytail":
        back = P(f"M{CX+11.4} {CY-7.6}C{CX+20.6} {CY-7.6} {CX+22.6} {CY+4} {CX+19.2} {CY+13.6}"
                 f"C{CX+18} {CY+6} {CX+15.6} {CY+2} {CX+12} {CY}Z")
        front = P(cap(line=22.0)) + C(CX + 13.8, CY - 8.2, 3.0, accent)
    elif style == "headband":
        back = long_back()
        front = P(cap(line=22.6)) + (f'<path d="M{CX-13.8} {CY-6.4}C{CX-8.6} {CY-12.8} {CX+8.6} {CY-12.8} {CX+13.8} {CY-6.4}" '
                                     f'stroke="{accent}" stroke-width="2.6" fill="none" stroke-linecap="round"/>')
    elif style == "quiff":
        (x0, y0), (x1, y1) = pt(14.4, 198), pt(14.4, -18)
        front = P(f"M{x0:.2f} {y0:.2f}C{CX-14.6} {CY-12} {CX-8} {CY-17.6} {CX+1} {CY-17.4}"
                  f"C{CX+8} {CY-17.6} {CX+12} {CY-15} {CX+12.8} {CY-12.6}C{CX+14.6} {CY-9} {x1+.4:.2f} {y1-2:.2f} {x1:.2f} {y1:.2f}"
                  f"C{CX+11} {CY-6.4} {CX+5} {CY-6.8} {CX+1} {CY-9.6}C{CX-3} {CY-6} {CX-9} {CY-4.4} {x0:.2f} {y0:.2f}Z")
    elif style == "beanie":
        front = (P(f"M{CX-15} {CY+1}C{CX-15} {CY-10.6} {CX-8.2} {CY-16.8} {CX} {CY-16.8}S{CX+15} {CY-10.6} {CX+15} {CY+1}Z")
                 + f'<rect x="{CX-16.2}" y="{CY-3.2}" width="32.4" height="6.2" rx="3.1" fill="{accent}"/>' + C(CX, CY - 18.8, 4.2, accent))
    elif style == "cap":
        front = (P(f"M{CX-14.6} {CY-1}C{CX-14.6} {CY-11} {CX-8} {CY-16.4} {CX} {CY-16.4}S{CX+14.6} {CY-11} {CX+14.6} {CY-1}Z")
                 + P(f"M{CX-14.6} {CY-3}H{CX-21.4}C{CX-21.4} {CY-.6} {CX-19.4} {CY+.6} {CX-16.6} {CY+.6}H{CX-14.6}Z")
                 + C(CX, CY - 16.2, 1.6, accent) + f'<rect x="{CX-15.2}" y="{CY-4.2}" width="30.4" height="3" rx="1.5" fill="{accent}"/>')
    elif style == "beard":
        # Sideburns down from the hairline, along the jaw to a full chin,
        # with a small moustache; the cheeks stay clear.
        beard = (f"M{CX-14} {CY-0.6}C{CX-14.2} {CY+11} {CX-8} {CY+18.6} {CX} {CY+18.6}"
                 f"C{CX+8} {CY+18.6} {CX+14.2} {CY+11} {CX+14} {CY-0.6}"
                 f"L{CX+11.6} {CY-0.6}C{CX+11.4} {CY+6.4} {CX+8.6} {CY+10.2} {CX+5} {CY+10.6}"
                 f"C{CX+3} {CY+10.8} {CX+1.4} {CY+10} {CX} {CY+9.4}"
                 f"C{CX-1.4} {CY+10} {CX-3} {CY+10.8} {CX-5} {CY+10.6}"
                 f"C{CX-8.6} {CY+10.2} {CX-11.4} {CY+6.4} {CX-11.6} {CY-0.6}Z")
        moustache = (f"M{CX-6.2} {CY+9.6}C{CX-4.4} {CY+7} {CX-1.6} {CY+6.6} {CX} {CY+7.8}"
                     f"C{CX+1.6} {CY+6.6} {CX+4.4} {CY+7} {CX+6.2} {CY+9.6}C{CX+3.6} {CY+9} {CX+1.6} {CY+9.4} {CX} {CY+10.4}"
                     f"C{CX-1.6} {CY+9.4} {CX-3.6} {CY+9} {CX-6.2} {CY+9.6}Z")
        front = P(cap(line=22.4)) + P(beard) + P(moustache)
    elif style == "curls":
        front = "".join(C(CX + 12.8 * math.cos(math.radians(d)), CY - 12.8 * math.sin(math.radians(d)), 4.2) for d in range(10, 172, 23)) + P(cap(r=14.2, line=22.6))
    elif style == "phones":
        # Short hair under a pair of headphones.
        front = (P(cap(line=22.6))
                 + f'<path d="M{CX-15.6} {CY+1}A15.6 15.6 0 0 1 {CX+15.6} {CY+1}" fill="none" stroke="{accent}" stroke-width="2.8" stroke-linecap="round"/>'
                 + "".join(f'<rect x="{x:.2f}" y="{CY-3}" width="6.4" height="10.4" rx="3.1" fill="{accent}"/>' for x in (CX - 18.8, CX + 12.4)))
    return back, front


def hijab(h):
    """A headscarf framing the face, draped over the shoulders, with a soft fold under the chin."""
    bg1, bg2 = hsl(h, 70, 87), hsl(h, 56, 77)
    face, body, scarf, fold, blush, pin = "#fff5ec", hsl(h, 40, 70), hsl(h, 40, 56), hsl(h, 38, 46), "#ff9fb4", hsl((h + 150) % 360, 70, 66)
    drape = (f"M{CX-17.4} 64C{CX-18.6} {CY+24} {CX-17.4} {CY+8} {CX-16.4} {CY}C{CX-15.8} {CY-11.4} {CX-9} {CY-17.4} {CX} {CY-17.4}"
             f"C{CX+9} {CY-17.4} {CX+15.8} {CY-11.4} {CX+16.4} {CY}C{CX+17.4} {CY+8} {CX+18.6} {CY+24} {CX+17.4} 64Z")
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64"><defs>'
            f'<linearGradient id="bg-hijab" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{bg1}"/><stop offset="1" stop-color="{bg2}"/></linearGradient>'
            f'<clipPath id="round-hijab"><circle cx="32" cy="32" r="32"/></clipPath></defs>'
            f'<g clip-path="url(#round-hijab)"><rect width="64" height="64" fill="url(#bg-hijab)"/>'
            f'<path d="M{CX-21} 64C{CX-21} {CY+R+9} {CX-14} {CY+R+4} {CX} {CY+R+4}C{CX+14} {CY+R+4} {CX+21} {CY+R+9} {CX+21} 64Z" fill="{body}"/>'
            f'<path d="{drape}" fill="{scarf}"/>'
            f'<ellipse cx="{CX}" cy="{CY+1.2}" rx="10.4" ry="12" fill="{face}"/>'
            f'<path d="M{CX-12.4} {CY+14.4}C{CX-6} {CY+19.6} {CX+6} {CY+19.6} {CX+12.4} {CY+14.4}" stroke="{fold}" stroke-width="1.6" fill="none" stroke-linecap="round"/>'
            f'<circle cx="{CX+11.6}" cy="{CY+9.6}" r="1.5" fill="{pin}"/>'
            f'<ellipse cx="{CX-6.8}" cy="{CY+5.6}" rx="2.5" ry="1.6" fill="{blush}" opacity=".62"/>'
            f'<ellipse cx="{CX+6.8}" cy="{CY+5.6}" rx="2.5" ry="1.6" fill="{blush}" opacity=".62"/>'
            f"</g></svg>\n")


def svg(style, h):
    if style == "hijab":
        return hijab(h)
    bg1, bg2 = hsl(h, 80, 86), hsl(h, 66, 76)
    head, neck, body, hair = "#fff5ec", "#f3dfd0", hsl(h, 56, 64), hsl(h, 38, 32)
    accent, blush = hsl((h + 150) % 360, 78, 64), "#ff9fb4"
    back, front = parts(style, hair, accent)
    nw, ntop, nbot = 7.6, CY + R - 4, CY + R + 8
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64" width="64" height="64"><defs>'
            f'<linearGradient id="bg-{style}" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{bg1}"/><stop offset="1" stop-color="{bg2}"/></linearGradient>'
            f'<clipPath id="round-{style}"><circle cx="32" cy="32" r="32"/></clipPath></defs>'
            f'<g clip-path="url(#round-{style})"><rect width="64" height="64" fill="url(#bg-{style})"/>{back}'
            f'<rect x="{CX-nw/2}" y="{ntop}" width="{nw}" height="{nbot-ntop}" rx="2.6" fill="{neck}"/>'
            f'<path d="M{CX-19} 64C{CX-19} {CY+R+9} {CX-12} {CY+R+3.6} {CX-nw/2} {CY+R+3.6}C{CX-2} {CY+R+6.4} {CX+2} {CY+R+6.4} {CX+nw/2} {CY+R+3.6}'
            f'C{CX+12} {CY+R+3.6} {CX+19} {CY+R+9} {CX+19} 64Z" fill="{body}"/>'
            f'<circle cx="{CX}" cy="{CY}" r="{R}" fill="{head}"/>'
            f'<ellipse cx="{CX-7.8}" cy="{CY+5}" rx="2.7" ry="1.75" fill="{blush}" opacity=".62"/>'
            f'<ellipse cx="{CX+7.8}" cy="{CY+5}" rx="2.7" ry="1.75" fill="{blush}" opacity=".62"/>'
            f"{front}</g></svg>\n")


if __name__ == "__main__":
    for style, hue in WOMEN + MEN:
        with open(os.path.join(OUT, f"{style}.svg"), "w") as f:
            f.write(svg(style, hue))
