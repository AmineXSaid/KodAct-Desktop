import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert("RGB")
W, H = im.size
px = im.load()
# The filled Allow button: KodAct blue, in the chat column.
xs, ys = [], []
for y in range(40, H - 120, 2):
    for x in range(250, 760, 2):
        r, g, b = px[x, y]
        if r < 30 and 100 < g < 135 and b > 200:
            xs.append(x); ys.append(y)
if len(xs) > 60:
    # The lowest button cluster (the newest ask).
    yb = max(ys)
    pts = [(x, y) for x, y in zip(xs, ys) if y > yb - 30]
    print(sum(p[0] for p in pts) // len(pts), sum(p[1] for p in pts) // len(pts))
