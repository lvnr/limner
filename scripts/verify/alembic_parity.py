#!/usr/bin/env python3
"""Compares Limner's stroke geometry with Quill's own Alembic export of the same project.

Usage:
    cargo run -q -p limner-cli -- dump-rings <quill project> local/out/rings.bin
    python3 -I scripts/verify/alembic_parity.py local/out/rings.bin <export.abc>

The Alembic file must have been exported by Quill with baked transforms and extra
attributes (vertex_id, brush_id). Only aggregate numbers are printed.
"""
import collections
import math
import mmap
import struct
import sys

rings_path, abc_path = sys.argv[1], sys.argv[2]

# ---------- Limner dump ----------
buf = open(rings_path, "rb").read()
assert buf[:4] == b"LRNG"
(layer_count,) = struct.unpack_from("<I", buf, 4)
o = 8
layers = {}
for _ in range(layer_count):
    (n,) = struct.unpack_from("<I", buf, o); o += 4
    path = buf[o:o + n].decode("utf8"); o += n
    visible, mirrored = buf[o], buf[o + 1]; o += 2
    (stroke_count,) = struct.unpack_from("<I", buf, o); o += 4
    strokes = []
    for _ in range(stroke_count):
        brush, points, ring = struct.unpack_from("<III", buf, o); o += 12
        count = points * ring * 3
        strokes.append((brush, points, ring, struct.unpack_from("<%dd" % count, buf, o))); o += count * 8
    # Quill names exported objects by the layer path joined with "-".
    layers.setdefault(path.replace("/", "-"), []).append(dict(path=path, visible=visible, mirrored=mirrored, strokes=strokes))

# ---------- Alembic (Ogawa) ----------
f = open(abc_path, "rb")
mm = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
ISDATA = 1 << 63
POD = {0: "?", 1: "B", 2: "b", 3: "H", 4: "h", 5: "I", 6: "i", 7: "Q", 8: "q", 9: "e", 10: "f", 11: "d", 12: None, 13: None}

def group(pos):
    if pos == 0:
        return []
    (n,) = struct.unpack_from("<Q", mm, pos)
    return list(struct.unpack_from("<%dQ" % n, mm, pos + 8)) if n else []

def data(child):
    pos = child & ~ISDATA
    if pos == 0:
        return b""
    (n,) = struct.unpack_from("<Q", mm, pos)
    return mm[pos + 8: pos + 8 + n]

root = group(struct.unpack_from("<Q", mm, 8)[0])
im = data(root[5]); p = 0; imeta = [""]
while p < len(im):
    n = im[p]; p += 1; imeta.append(im[p:p + n].decode("latin1")); p += n

def rs(b, p, hint):
    if hint == 0:
        return b[p], p + 1
    if hint == 1:
        return struct.unpack_from("<H", b, p)[0], p + 2
    return struct.unpack_from("<I", b, p)[0], p + 4

def prop_headers(b):
    p = 0; out = []
    while p < len(b):
        (info,) = struct.unpack_from("<I", b, p); p += 4
        ptype = info & 3; hint = (info & 0xC) >> 2; h = dict(ptype=ptype)
        if ptype != 0:
            h["pod"] = (info & 0xF0) >> 4; h["extent"] = (info & 0xFF000) >> 12
            _, p = rs(b, p, hint)
            if info & 0x200:
                _, p = rs(b, p, hint); _, p = rs(b, p, hint)
            if info & 0x100:
                _, p = rs(b, p, hint)
        n, p = rs(b, p, hint); h["name"] = b[p:p + n].decode("latin1"); p += n
        if ((info & 0xFF00000) >> 20) == 0xFF:
            n, p = rs(b, p, hint); p += n
        out.append(h)
    return out

def obj_headers(b):
    p = 0; out = []; end = len(b) - 32
    while p < end:
        (n,) = struct.unpack_from("<I", b, p); p += 4
        name = b[p:p + n].decode("utf8", "replace"); p += n
        mdi = b[p]; p += 1
        if mdi == 0xFF:
            (m,) = struct.unpack_from("<I", b, p); p += 4; meta = b[p:p + m].decode("latin1"); p += m
        else:
            meta = imeta[mdi]
        out.append((name, meta))
    return out

def read_props(gpos, prefix, out):
    ch = group(gpos)
    if not ch:
        return
    for i, h in enumerate(prop_headers(data(ch[-1]))):
        full = prefix + "/" + h["name"] if prefix else h["name"]
        if h["ptype"] == 0:
            read_props(ch[i] & ~ISDATA, full, out)
        else:
            sub = group(ch[i] & ~ISDATA); raw = data(sub[0])[16:] if sub else b""
            fmt = POD[h["pod"]]
            if fmt:
                cnt = len(raw) // struct.calcsize(fmt)
                out[full.split("/")[-1]] = struct.unpack_from("<%d%s" % (cnt, fmt), raw, 0)

meshes = {}
def walk(gpos, name, meta, parents):
    ch = group(gpos); kids = obj_headers(data(ch[-1])) if len(ch) > 1 else []
    if "PolyMesh" in meta:
        props = {}; read_props(ch[0] & ~ISDATA, "", props); meshes[name] = (props, parents)
    for i, (kn, km) in enumerate(kids):
        walk(ch[1 + i] & ~ISDATA, kn, km, parents + [name])
walk(root[2] & ~ISDATA, "ABC", "", [])

# ---------- compare ----------
RING_ABC = {0: 2, 1: 2, 2: 8, 3: 8, 4: 5}
tally = collections.Counter()
worst = collections.defaultdict(float)
orderings = collections.Counter()
for mesh_name, (props, parents) in sorted(meshes.items()):
    layer_name = mesh_name[:-5] if mesh_name.endswith("_mesh") else mesh_name
    candidates = layers.get(layer_name)
    if not candidates:
        tally["mesh without a matching layer"] += 1
        continue
    P = props["P"]; vid = props.get("vertex_id"); brush_ids = props.get("brush_id")
    if vid is None or brush_ids is None:
        tally["mesh without extra attributes (skipped)"] += 1
        continue
    nv = len(P) // 3
    segments = []; start = 0
    for i in range(1, nv):
        if vid[i] == 0 and vid[i - 1] != 0:
            segments.append((start, i)); start = i
    segments.append((start, nv))
    matched = None
    for cand in candidates:
        if len(cand["strokes"]) == len(segments) and all(vid[b - 1] + 1 == s[1] for (a, b), s in zip(segments, cand["strokes"])):
            matched = cand; break
    if matched is None:
        tally["mesh whose stroke layout matches no layer"] += 1
        continue
    tally["meshes compared"] += 1
    tally["mirrored layers" if matched["mirrored"] else "unmirrored layers"] += 1
    for si, ((a, b), (brush, points, ring, mine)) in enumerate(zip(segments, matched["strokes"])):
        if brush_ids[si] != brush:
            tally["brush mismatch"] += 1
            continue
        r = RING_ABC[brush]
        if (b - a) != points * r:
            tally["ring size mismatch"] += 1
            continue
        tally["strokes compared"] += 1
        tally["points compared"] += points
        for pi in range(points):
            theirs = [P[3 * (a + pi * r + j): 3 * (a + pi * r + j) + 3] for j in range(r)]
            ours = [mine[3 * (pi * ring + j): 3 * (pi * ring + j) + 3] for j in range(ring)]
            unique = theirs[:ring]
            direct = max(math.dist(x, y) for x, y in zip(unique, ours))
            key = ("ring vertices, brush %d" % brush)
            if direct < 1e-3:
                orderings[(brush, "same order")] += 1
                worst[key] = max(worst[key], direct)
            else:
                # Try the other cyclic orders to describe how the export numbers its ring.
                best = (direct, "same order")
                for shift in range(ring):
                    for rev in (False, True):
                        cand = [ours[(shift + (-j if rev else j)) % ring] for j in range(ring)]
                        e = max(math.dist(x, y) for x, y in zip(unique, cand))
                        if e < best[0]:
                            best = (e, "shift %d%s" % (shift, " reversed" if rev else ""))
                orderings[(brush, best[1])] += 1
                worst[key + " (after reordering)"] = max(worst[key + " (after reordering)"], best[0])
            if r > ring:
                worst["seam duplicate distance"] = max(worst["seam duplicate distance"], math.dist(theirs[0], theirs[r - 1]))
            centre_theirs = [sum(v[d] for v in unique) / ring for d in range(3)]
            centre_ours = [sum(v[d] for v in ours) / ring for d in range(3)]
            worst["ring centre"] = max(worst["ring centre"], math.dist(centre_theirs, centre_ours))

print("Alembic meshes: %d, Limner paint layers: %d" % (len(meshes), layer_count))
for k, v in sorted(tally.items()):
    print("  %-45s %d" % (k, v))
print("ring vertex ordering (brush, mapping): count")
for k, v in sorted(orderings.items()):
    print("  %-45s %d" % (str(k), v))
print("largest distances (scene units)")
for k, v in sorted(worst.items()):
    print("  %-45s %.3e" % (k, v))
