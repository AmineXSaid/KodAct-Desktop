"""Render the Forge cube picture turning, for the "working" logo.

The three faces visible in assets/images/cube-original.webp are unwarped
into square textures, put back on a cube, and the cube is drawn from the
same height the picture was taken at, turning about its vertical axis one
full turn in STEPS frames. The app shows the frames in order (eased per
quarter turn), so the logo turns without ever jumping.

    python3 -I scripts/make_turn.py   # writes assets/images/turn/turn-NN.png
"""

import math
import pathlib

import numpy as np
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "assets/images/cube-original.webp"
OUT = ROOT / "assets/images/turn"

STEPS = 72  # 5° apart
REST = math.radians(45)
SIZE = 64  # the largest drawn is 32 px at 2x
SS = 4  # supersampling
TEX = 512

# The picture's cube corners (pixels in the 1254 px original).
BACK, LEFT, RIGHT, FRONT = (662, 123), (132, 258), (1135, 265), (550, 467)
LEFT_LOW, RIGHT_LOW, FRONT_LOW = (139, 961), (1129, 975), (550, 1226)


def coeffs(dst, src):
    """PIL PERSPECTIVE coefficients mapping points in `dst` to `src`."""
    a, b = [], []
    for (x, y), (u, v) in zip(dst, src):
        a.append([x, y, 1, 0, 0, 0, -u * x, -u * y])
        a.append([0, 0, 0, x, y, 1, -v * x, -v * y])
        b += [u, v]
    return np.linalg.solve(np.array(a, float), np.array(b, float)).tolist()


def face(img, quad):
    """The face with corners `quad` (top-left, top-right, bottom-right,
    bottom-left) as a TEX square."""
    sq = [(0, 0), (TEX, 0), (TEX, TEX), (0, TEX)]
    return img.transform((TEX, TEX), Image.PERSPECTIVE, coeffs(sq, quad), Image.BICUBIC)


def main():
    img = Image.open(SRC).convert("RGBA")
    # Seen from the front edge: top's far corner up, sides left and right.
    top = face(img, [LEFT, BACK, RIGHT, FRONT])
    left = face(img, [LEFT, FRONT, FRONT_LOW, LEFT_LOW])
    right = face(img, [FRONT, RIGHT, RIGHT_LOW, FRONT_LOW])
    sides = [left, right, left, right]

    n = SIZE * SS
    tilt = math.radians(30)
    dist = 7.5
    focal = n * 1.95
    light = np.array([-0.55, 0.75, 0.45])
    light /= np.linalg.norm(light)
    OUT.mkdir(parents=True, exist_ok=True)
    for f in OUT.glob("turn-*.png"):
        f.unlink()

    def camera(p, yaw):
        """World to camera space: turned by `yaw`, then tipped toward the
        camera (at the origin, looking along +z) so the top shows."""
        x, y, z = p
        c, s = math.cos(yaw), math.sin(yaw)
        x, z = c * x + s * z, -s * x + c * z
        ct, st = math.cos(tilt), math.sin(tilt)
        y, z = ct * y + st * z, -st * y + ct * z
        return np.array([x, y, z + dist])

    def project(p, yaw):
        x, y, z = camera(p, yaw)
        return (n / 2 + focal * x / z, n / 2 - focal * y / z + n * 0.02)

    def turned(v, yaw):
        x, y, z = v
        c, s = math.cos(yaw), math.sin(yaw)
        return np.array([c * x + s * z, y, -s * x + c * z])

    def shade(nrm):
        return 0.6 + 0.4 * max(0.0, float(np.dot(nrm, light)))

    # The picture's own lighting is in each texture: a face is relit by how
    # much more (or less) light it gets now than where the picture shows it.
    rest_normal = [turned((0, 0, 1), REST), turned((1, 0, 0), REST)]

    # Side k faces outward at angle k*90° (k=0 front-left at rest).
    for i in range(STEPS):
        yaw = REST - 2 * math.pi * i / STEPS
        canvas = Image.new("RGBA", (n, n))
        quads = []
        for k in range(4):
            a0 = math.radians(90 * k)
            # corners of side k: from its left edge to its right edge, seen from outside
            p = [(-1, 1, 1), (1, 1, 1), (1, -1, 1), (-1, -1, 1)]
            c, s = math.cos(a0), math.sin(a0)
            pts = [(c * x + s * z, y, -s * x + c * z) for x, y, z in p]
            nrm = turned((s, 0, c), yaw)
            quads.append((pts, nrm, sides[k], rest_normal[k % 2]))
        quads.append(([(-1, 1, -1), (1, 1, -1), (1, 1, 1), (-1, 1, 1)], np.array([0, 1, 0]), top, np.array([0, 1, 0])))
        for pts, nrm, tex, rest in quads:
            cam = [camera(q, yaw) for q in pts]
            centre = sum(cam) / 4
            normal = np.cross(cam[1] - cam[0], cam[3] - cam[0])
            # Corners run clockwise seen from outside; with y up on screen the
            # cross product points away from the camera for a face toward it.
            if np.dot(normal, centre) <= 0:
                continue
            proj = [project(q, yaw) for q in pts]
            # relight: lambert, kept near the picture's own contrast
            gain = shade(nrm) / shade(rest)
            t = tex.copy()
            rgb = np.clip(np.array(t, float)[..., :3] * gain, 0, 255)
            arr = np.array(t)
            arr[..., :3] = rgb.astype(np.uint8)
            t = Image.fromarray(arr, "RGBA")
            sq = [(0, 0), (TEX, 0), (TEX, TEX), (0, TEX)]
            warped = t.transform((n, n), Image.PERSPECTIVE, coeffs(proj, sq), Image.BICUBIC)
            # mask the quad exactly
            from PIL import ImageDraw
            m = Image.new("L", (n, n))
            ImageDraw.Draw(m).polygon(proj, fill=255)
            warped.putalpha(Image.fromarray(np.minimum(np.array(warped.getchannel("A")), np.array(m))))
            canvas.alpha_composite(warped)
        canvas.resize((SIZE, SIZE), Image.LANCZOS).save(OUT / f"turn-{i:02}.png", optimize=True)
    print(f"{STEPS} frames in {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
