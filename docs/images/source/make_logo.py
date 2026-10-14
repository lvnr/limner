import math, sys

def bez(p, t):
    (x0,y0),(x1,y1),(x2,y2),(x3,y3) = p
    u = 1-t
    x = u*u*u*x0 + 3*u*u*t*x1 + 3*u*t*t*x2 + t*t*t*x3
    y = u*u*u*y0 + 3*u*u*t*y1 + 3*u*t*t*y2 + t*t*t*y3
    return x, y

def lerp(a, b, t): return a + (b-a)*t
def mix(c1, c2, t): return tuple(lerp(a, b, t) for a, b in zip(c1, c2))
def hexc(c): return '#%02x%02x%02x' % tuple(max(0, min(255, round(v))) for v in c)

STOPS = [(0.0, (255, 104, 84)), (0.42, (255, 168, 62)), (0.585, (255, 186, 72)), (0.625, (40, 200, 186)), (1.0, (64, 132, 255))]
def ramp(t):
    for (t0, c0), (t1, c1) in zip(STOPS, STOPS[1:]):
        if t <= t1:
            return mix(c0, c1, (t - t0) / (t1 - t0))
    return STOPS[-1][1]

def ribbon(curve, width, turns, phase, n=220):
    pts = [bez(curve, i/n) for i in range(n+1)]
    quads = []
    for i in range(n):
        t0, t1 = i/n, (i+1)/n
        def edge(t, p, q):
            dx, dy = q[0]-p[0], q[1]-p[1]
            l = math.hypot(dx, dy) or 1
            nx, ny = -dy/l, dx/l
            taper = math.sin(math.pi * min(max(t, 0.0), 1.0)) ** 0.55
            th = phase + 2*math.pi*turns*t
            w = width * taper * math.cos(th)
            return (p[0] + nx*w, p[1] + ny*w), (p[0] - nx*w, p[1] - ny*w), math.cos(th), math.sin(th)
        a = pts[i]; b = pts[i+1]
        nxt = pts[min(i+2, n)]
        l0, r0, c0, s0 = edge(t0, a, b)
        l1, r1, c1, s1 = edge(t1, b, nxt if i+1 < n else b)
        tm = (t0+t1)/2
        facing = math.cos(phase + 2*math.pi*turns*tm)
        depth = math.sin(phase + 2*math.pi*turns*tm)
        base = ramp(tm)
        # The face turned toward us is lit; the back is the same colour in shadow.
        shade = 0.72 + 0.28*abs(facing)
        col = tuple(v*shade for v in base) if facing >= 0 else tuple(v*shade*0.78 for v in base)
        quads.append((depth, [l0, l1, r1, r0], col))
    return quads

def svg(size=256, background=True):
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="{size}" height="{size}">']
    out.append('<defs><linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">'
               '<stop offset="0" stop-color="#1d2130"/><stop offset="1" stop-color="#0f1117"/></linearGradient>'
               '<radialGradient id="glow" cx="0.32" cy="0.28" r="0.75"><stop offset="0" stop-color="#3d8bfd" stop-opacity="0.22"/>'
               '<stop offset="1" stop-color="#3d8bfd" stop-opacity="0"/></radialGradient></defs>')
    if background:
        out.append('<rect x="8" y="8" width="240" height="240" rx="56" fill="url(#bg)"/>')
        out.append('<rect x="8" y="8" width="240" height="240" rx="56" fill="url(#glow)"/>')
    curve = [(78, 42), (40, 196), (120, 236), (214, 150)]
    quads = ribbon(curve, 28, 1.15, 0.35)
    for depth, poly, col in sorted(quads, key=lambda q: q[0]):
        d = ' '.join(f'{x:.2f},{y:.2f}' for x, y in poly)
        c = hexc(col)
        out.append(f'<polygon points="{d}" fill="{c}" stroke="{c}" stroke-width="0.6" stroke-linejoin="round"/>')
    # A drop of paint at the start of the stroke.
    out.append('<circle cx="214" cy="150" r="0" fill="none"/>')
    out.append('</svg>')
    return '\n'.join(out)

open(sys.argv[1] if len(sys.argv) > 1 else "logo.svg", "w").write(svg())
