import json, math, random
random.seed(7)
STOPS = [(0.0, (255, 104, 84)), (0.42, (255, 168, 62)), (0.585, (255, 186, 72)), (0.625, (40, 200, 186)), (1.0, (64, 132, 255))]
def ramp(t):
    for (t0, c0), (t1, c1) in zip(STOPS, STOPS[1:]):
        if t <= t1:
            u = (t - t0) / (t1 - t0)
            return [a + (b - a) * u for a, b in zip(c0, c1)]
    return list(STOPS[-1][1])
def lin(c):
    out = []
    for v in c:
        v /= 255.0
        out.append(v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4)
    return out

strokes = []
n = 6
for i in range(n):
    t = i / (n - 1)
    spread = (t - 0.5) * 0.50
    amp = 0.035 + 0.02 * math.sin(i * 1.9)
    k = 3.0 + 0.7 * math.cos(i * 1.3)
    phase = i * 1.1
    z = -1.24 - 0.05 * ((i * 3) % n)
    pts = []
    steps = 180
    for s_ in range(steps + 1):
        u = s_ / steps
        x = -0.92 + 1.84 * u
        env = math.sin(math.pi * u)
        y = 1.4 + spread * env ** 1.1 + amp * math.sin(k * x + phase) * env
        pts.append([round(x, 4), round(y, 4), round(z + 0.05 * math.sin(2.0 * x + phase), 4)])
    strokes.append({"brush": "cylinder", "points": pts, "width": round(0.007 + 0.006 * math.sin(math.pi * (0.2 + 0.6 * t)), 4), "color": lin(ramp([0.0, 0.22, 0.45, 0.68, 0.84, 1.0][i]))})
def quat_from_to(a, b):
    ax, ay, az = a; bx, by, bz = b
    cx, cy, cz = ay*bz - az*by, az*bx - ax*bz, ax*by - ay*bx
    w = 1.0 + ax*bx + ay*by + az*bz
    l = math.sqrt(cx*cx + cy*cy + cz*cz + w*w)
    return [cx/l, cy/l, cz/l, w/l]
d = (0.45, -0.55, -0.70)
l = math.sqrt(sum(v*v for v in d)); d = tuple(v/l for v in d)
QUAT = quat_from_to((0.0, 0.0, -1.0), d)
cmds = [
    {"cmd": "set_document", "background": lin([16, 18, 26]), "ambient_light": [0.42, 0.44, 0.5]},
    {"cmd": "set_layer_kind", "layer": 4, "values": {"shading": "lit"}},
    {"cmd": "add_layer", "kind": "light", "name": "Key"},
    {"cmd": "set_layer_kind", "layer": 5, "values": {"kind": "directional", "energy": 1.1, "color": [1.0, 0.94, 0.86]}},
    {"cmd": "set_layer", "layer": 5, "transform": {"rotation": QUAT, "scale": 1.0, "translation": [0.0, 2.0, 0.0], "flip": "none"}},
    {"cmd": "set_options", "values": {"show_grid": False, "show_extras": False}},
    {"cmd": "add_strokes", "space": "viewer", "select": False, "strokes": strokes},
    {"cmd": "set_color", "color": lin([246, 240, 230])},
]
args = []
for c in cmds:
    args += ["--exec", json.dumps(c)]
open("banner_args.json", "w").write(json.dumps(args))
