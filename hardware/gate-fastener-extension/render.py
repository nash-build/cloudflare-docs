#!/usr/bin/env python3
"""
Software renderer for the gate fastener extension. No dependencies.

Rasterises the same geometry the STL generator produces, so what you see here
is the actual mesh rather than an illustration of it. Writes RGBA PNGs with a
transparent background, which keeps them legible on light and dark pages alike.

    python3 render.py                 # all views into renders/
"""

import math
import os
import struct
import zlib

import gate_fastener_extension as gfe

# ---------------------------------------------------------------------------
# Small mesh helpers. Everything is built with its axis along +Z and its
# section cap lying in the y = 0 plane, then transformed into place, so a
# single clipping plane sections the whole assembly.
# ---------------------------------------------------------------------------

SEG = 96


def _pt(profile, theta, z):
    r = profile(theta)
    return (r * math.cos(theta), r * math.sin(theta), z)


def cap_from_levels(levels):
    """Flat fill of the y = 0 plane, built from the revolve profile itself.

    Without this a clipped render shows the hollow inside of a shell; with it
    the cut reads as solid material.
    """
    tris = []
    for sign, theta in ((1.0, 0.0), (-1.0, math.pi)):
        for (za, oa, ia), (zb, ob, ib) in zip(levels, levels[1:]):
            xi_a, xo_a = ia(theta) * sign, oa(theta) * sign
            xi_b, xo_b = ib(theta) * sign, ob(theta) * sign
            quad = [
                (xi_a, 0.0, za),
                (xo_a, 0.0, za),
                (xo_b, 0.0, zb),
                (xi_b, 0.0, zb),
            ]
            if sign > 0:
                tris.append((quad[0], quad[1], quad[2]))
                tris.append((quad[0], quad[2], quad[3]))
            else:
                tris.append((quad[0], quad[2], quad[1]))
                tris.append((quad[0], quad[3], quad[2]))
    return tris


def cylinder(radius, height, z0=0.0, segments=SEG, hole=0.0):
    prof_o = gfe.circle(radius * 2)
    prof_i = gfe.circle(hole * 2) if hole else None
    levels = [(z0, prof_o, prof_i or gfe.circle(0.0)),
              (z0 + height, prof_o, prof_i or gfe.circle(0.0))]
    if hole:
        return gfe.build_mesh(levels, segments), cap_from_levels(levels)
    # Solid: build the wall plus two disc caps by hand.
    tris = []
    for i in range(segments):
        t0 = 2 * math.pi * i / segments
        t1 = 2 * math.pi * (i + 1) / segments
        a = _pt(prof_o, t0, z0)
        b = _pt(prof_o, t1, z0)
        c = _pt(prof_o, t1, z0 + height)
        d = _pt(prof_o, t0, z0 + height)
        tris += [(a, b, c), (a, c, d)]
        tris.append(((0, 0, z0), b, a))
        tris.append(((0, 0, z0 + height), d, c))
    cap = [((0, 0, z0), (radius, 0, z0), (radius, 0, z0 + height)),
           ((0, 0, z0), (radius, 0, z0 + height), (0, 0, z0 + height)),
           ((0, 0, z0), (-radius, 0, z0 + height), (-radius, 0, z0)),
           ((0, 0, z0), (0, 0, z0 + height), (-radius, 0, z0 + height))]
    return tris, cap


def hex_head(across_flats, height, z0=0.0):
    """Hex prism with flats facing +/-Z in world terms once laid on its side."""
    r = across_flats / math.sqrt(3)
    tris = []
    pts = [(r * math.cos(math.radians(a)), r * math.sin(math.radians(a)))
           for a in range(0, 360, 60)]
    for i in range(6):
        x0, y0 = pts[i]
        x1, y1 = pts[(i + 1) % 6]
        a = (x0, y0, z0)
        b = (x1, y1, z0)
        c = (x1, y1, z0 + height)
        d = (x0, y0, z0 + height)
        tris += [(a, b, c), (a, c, d)]
        tris.append(((0, 0, z0), b, a))
        tris.append(((0, 0, z0 + height), d, c))
    cap = [((0, 0, z0), (r, 0, z0), (r, 0, z0 + height)),
           ((0, 0, z0), (r, 0, z0 + height), (0, 0, z0 + height)),
           ((0, 0, z0), (-r, 0, z0 + height), (-r, 0, z0)),
           ((0, 0, z0), (0, 0, z0 + height), (-r, 0, z0 + height))]
    return tris, cap


def hex_nut(across_flats, height, bore, z0=0.0, segments=SEG):
    outer = gfe.hexagon(across_flats)
    inner = gfe.circle(bore)
    levels = [(z0, outer, inner), (z0 + height, outer, inner)]
    return gfe.build_mesh(levels, segments), cap_from_levels(levels)


def box(sx, sy, sz, centre=(0, 0, 0)):
    cx, cy, cz = centre
    x0, x1 = cx - sx / 2, cx + sx / 2
    y0, y1 = cy - sy / 2, cy + sy / 2
    z0, z1 = cz - sz / 2, cz + sz / 2
    v = [(x0, y0, z0), (x1, y0, z0), (x1, y1, z0), (x0, y1, z0),
         (x0, y0, z1), (x1, y0, z1), (x1, y1, z1), (x0, y1, z1)]
    faces = [(0, 3, 2, 1), (4, 5, 6, 7), (0, 1, 5, 4),
             (1, 2, 6, 5), (2, 3, 7, 6), (3, 0, 4, 7)]
    tris = []
    for a, b, c, d in faces:
        tris += [(v[a], v[b], v[c]), (v[a], v[c], v[d])]
    cap = []
    if y0 < 0.0 < y1:
        cap = [((x0, 0, z0), (x1, 0, z0), (x1, 0, z1)),
               ((x0, 0, z0), (x1, 0, z1), (x0, 0, z1))]
    return tris, cap


def transform(tris, rot_y=0.0, translate=(0, 0, 0)):
    c, s = math.cos(rot_y), math.sin(rot_y)
    tx, ty, tz = translate
    out = []
    for tri in tris:
        nt = []
        for x, y, z in tri:
            nx = x * c + z * s
            nz = -x * s + z * c
            nt.append((nx + tx, y + ty, nz + tz))
        out.append(tuple(nt))
    return out


# ---------------------------------------------------------------------------
# Rasteriser: perspective camera, z-buffer, Blinn-Phong, optional y = 0 clip.
# ---------------------------------------------------------------------------


def _norm(v):
    m = math.sqrt(v[0] ** 2 + v[1] ** 2 + v[2] ** 2) or 1.0
    return (v[0] / m, v[1] / m, v[2] / m)


def _cross(a, b):
    return (a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0])


def _dot(a, b):
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]


KEY = _norm((0.55, -0.85, 0.75))
FILL = _norm((-0.75, -0.45, 0.15))
SKY = (0.58, 0.62, 0.70)
GROUND = (0.20, 0.19, 0.18)


def shade(normal, point, eye, colour, shininess, spec_strength):
    n = normal
    amb = 0.5 + 0.5 * n[2]
    ambient = tuple(GROUND[i] + (SKY[i] - GROUND[i]) * amb for i in range(3))
    view = _norm((eye[0] - point[0], eye[1] - point[1], eye[2] - point[2]))

    key = max(0.0, _dot(n, KEY))
    fill = max(0.0, _dot(n, FILL)) * 0.35
    rim = (1.0 - max(0.0, _dot(n, view))) ** 2.5 * 0.30

    half = _norm((KEY[0] + view[0], KEY[1] + view[1], KEY[2] + view[2]))
    spec = max(0.0, _dot(n, half)) ** shininess * spec_strength

    out = []
    for i in range(3):
        lit = colour[i] * (0.34 * ambient[i] * 3.0 + 0.78 * key + fill) + rim * 0.5
        lit += spec
        out.append(max(0.0, min(1.0, lit)))
    return out


def render(objects, width, height, eye, target, fov=26.0, clip_y=None,
           ss=2, shadow=None):
    """clip_y discards everything below y = clip_y, i.e. the half nearest the
    camera when the camera sits on the -y side, opening the section up."""
    W, H = width * ss, height * ss
    focal = (H / 2.0) / math.tan(math.radians(fov) / 2.0)

    forward = _norm((target[0] - eye[0], target[1] - eye[1], target[2] - eye[2]))
    right = _norm(_cross(forward, (0, 0, 1)))
    up = _cross(right, forward)

    colour_buf = [0.0] * (W * H * 3)
    alpha_buf = [0.0] * (W * H)
    depth_buf = [1e30] * (W * H)

    if shadow is not None:
        cx, cy, radius, strength = shadow
        sx0 = W / 2.0 + (_dot((cx - eye[0], cy - eye[1], -eye[2]), right)
                         / _dot((cx - eye[0], cy - eye[1], -eye[2]), forward)) * focal
        sy0 = H / 2.0 - (_dot((cx - eye[0], cy - eye[1], -eye[2]), up)
                         / _dot((cx - eye[0], cy - eye[1], -eye[2]), forward)) * focal
        dist = math.sqrt(sum((eye[i] - (cx, cy, 0)[i]) ** 2 for i in range(3)))
        rpx = radius / dist * focal
        rypx = rpx * 0.34
        x0 = max(0, int(sx0 - rpx * 1.6)); x1 = min(W, int(sx0 + rpx * 1.6) + 1)
        y0 = max(0, int(sy0 - rypx * 1.6)); y1 = min(H, int(sy0 + rypx * 1.6) + 1)
        for py in range(y0, y1):
            dy = (py + 0.5 - sy0) / rypx
            for px in range(x0, x1):
                dx = (px + 0.5 - sx0) / rpx
                d = math.sqrt(dx * dx + dy * dy)
                if d >= 1.5:
                    continue
                a = max(0.0, 1.0 - d / 1.5) ** 2.2 * strength
                alpha_buf[py * W + px] = a

    for obj in objects:
        tris = obj["tris"]
        base = obj["colour"]
        cut_colour = obj.get("cut_colour", base)
        shininess = obj.get("shininess", 28.0)
        spec_strength = obj.get("spec", 0.20)
        is_cap = obj.get("is_cap", False)

        for tri in tris:
            (ax, ay, az), (bx, by, bz), (cx3, cy3, cz3) = tri
            n = _cross((bx - ax, by - ay, bz - az), (cx3 - ax, cy3 - ay, cz3 - az))
            nl = math.sqrt(n[0] ** 2 + n[1] ** 2 + n[2] ** 2)
            if nl < 1e-12:
                continue
            n = (n[0] / nl, n[1] / nl, n[2] / nl)

            centroid = ((ax + bx + cx3) / 3, (ay + by + cy3) / 3, (az + bz + cz3) / 3)
            to_eye = (eye[0] - centroid[0], eye[1] - centroid[1], eye[2] - centroid[2])
            facing = _dot(n, to_eye)
            if clip_y is None and facing <= 0:
                continue
            flipped = facing <= 0
            shading_n = (-n[0], -n[1], -n[2]) if flipped else n
            colour = cut_colour if (is_cap or flipped) else base

            proj = []
            ok = True
            for (vx, vy, vz) in tri:
                rel = (vx - eye[0], vy - eye[1], vz - eye[2])
                d = _dot(rel, forward)
                if d < 1e-3:
                    ok = False
                    break
                proj.append((W / 2.0 + _dot(rel, right) / d * focal,
                             H / 2.0 - _dot(rel, up) / d * focal,
                             1.0 / d, vy))
            if not ok:
                continue

            (px0, py0, iw0, wy0), (px1, py1, iw1, wy1), (px2, py2, iw2, wy2) = proj
            area = (px1 - px0) * (py2 - py0) - (px2 - px0) * (py1 - py0)
            if abs(area) < 1e-9:
                continue
            inv_area = 1.0 / area

            minx = max(0, int(min(px0, px1, px2)))
            maxx = min(W - 1, int(max(px0, px1, px2)) + 1)
            miny = max(0, int(min(py0, py1, py2)))
            maxy = min(H - 1, int(max(py0, py1, py2)) + 1)
            if minx > maxx or miny > maxy:
                continue

            lit = shade(shading_n, centroid, eye, colour, shininess, spec_strength)
            r_, g_, b_ = lit

            for py in range(miny, maxy + 1):
                yc = py + 0.5
                row = py * W
                for px in range(minx, maxx + 1):
                    xc = px + 0.5
                    w0 = ((px1 - xc) * (py2 - yc) - (px2 - xc) * (py1 - yc)) * inv_area
                    if w0 < 0:
                        continue
                    w1 = ((px2 - xc) * (py0 - yc) - (px0 - xc) * (py2 - yc)) * inv_area
                    if w1 < 0:
                        continue
                    w2 = 1.0 - w0 - w1
                    if w2 < 0:
                        continue
                    iw = w0 * iw0 + w1 * iw1 + w2 * iw2
                    if iw <= 0:
                        continue
                    d = 1.0 / iw
                    idx = row + px
                    if d >= depth_buf[idx]:
                        continue
                    if clip_y is not None:
                        wy = (w0 * wy0 * iw0 + w1 * wy1 * iw1 + w2 * wy2 * iw2) * d
                        if wy < clip_y:
                            continue
                    depth_buf[idx] = d
                    o = idx * 3
                    colour_buf[o] = r_
                    colour_buf[o + 1] = g_
                    colour_buf[o + 2] = b_
                    alpha_buf[idx] = 1.0

    # Downsample, averaging premultiplied colour so edges do not fringe.
    out = bytearray(width * height * 4)
    inv = 1.0 / (ss * ss)
    for y in range(height):
        for x in range(width):
            ar = ag = ab = aa = 0.0
            for sy in range(ss):
                row = (y * ss + sy) * W + x * ss
                for sx in range(ss):
                    idx = row + sx
                    a = alpha_buf[idx]
                    if a:
                        o = idx * 3
                        ar += colour_buf[o] * a
                        ag += colour_buf[o + 1] * a
                        ab += colour_buf[o + 2] * a
                        aa += a
            o = (y * width + x) * 4
            if aa > 0:
                out[o] = int(max(0.0, min(1.0, ar / aa)) ** (1 / 1.75) * 255)
                out[o + 1] = int(max(0.0, min(1.0, ag / aa)) ** (1 / 1.75) * 255)
                out[o + 2] = int(max(0.0, min(1.0, ab / aa)) ** (1 / 1.75) * 255)
                out[o + 3] = int(min(1.0, aa * inv) * 255)
    return out


def write_png(path, rgba, width, height):
    raw = bytearray()
    stride = width * 4
    for y in range(height):
        raw.append(0)
        raw += rgba[y * stride:(y + 1) * stride]

    def chunk(tag, data):
        c = struct.pack(">I", len(data)) + tag + data
        return c + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    png += chunk(b"IEND", b"")
    with open(path, "wb") as fh:
        fh.write(png)


# ---------------------------------------------------------------------------
# Scenes
# ---------------------------------------------------------------------------

PLASTIC = (0.86, 0.40, 0.13)
CUT = (0.99, 0.74, 0.47)
STEEL = (0.63, 0.66, 0.71)
WHITE_METAL = (0.86, 0.86, 0.83)
RUBBER = (0.11, 0.11, 0.13)
WOOD = (0.32, 0.21, 0.14)


def orbit(target, distance, azimuth, elevation):
    az, el = math.radians(azimuth), math.radians(elevation)
    return (target[0] + distance * math.cos(el) * math.cos(az),
            target[1] + distance * math.cos(el) * math.sin(az),
            target[2] + distance * math.sin(el))


def part_objects(length=1.25 * gfe.INCH, **kw):
    levels, stats = gfe.part_levels(length=length, segments=SEG, **kw)
    shell = gfe.build_mesh(levels, SEG)
    cap = cap_from_levels(levels)
    return shell, cap, stats


def scene_iso(path, size=(880, 700)):
    shell, cap, stats = part_objects()
    h = stats["length"]
    target = (0, 0, h * 0.46)
    eye = orbit(target, 104, -58, 34)
    objs = [{"tris": shell, "colour": PLASTIC, "spec": 0.26, "shininess": 34}]
    px = render(objs, size[0], size[1], eye, target, fov=26,
                shadow=(0, 0, stats["outer_diameter"] * 0.62, 0.34))
    write_png(path, px, *size)
    return stats


def scene_section(path, size=(880, 700)):
    shell, cap, stats = part_objects()
    h = stats["length"]
    target = (0, 0, h * 0.46)
    eye = orbit(target, 104, -72, 24)
    objs = [
        {"tris": shell, "colour": PLASTIC, "cut_colour": CUT,
         "spec": 0.26, "shininess": 34},
        {"tris": cap, "colour": CUT, "cut_colour": CUT,
         "is_cap": True, "spec": 0.05, "shininess": 8},
    ]
    px = render(objs, size[0], size[1], eye, target, fov=26, clip_y=0.0)
    write_png(path, px, *size)
    return stats


def scene_exploded(path, size=(1240, 540)):
    """Exploded along the axis, in the order the parts actually stack.

    Post, screw, printed spacer, rubber cap on its rod, white gate fastener,
    nut. The screw drops down the wide bore, its head seats where the bore necks
    down, and its threads bite the tunnel and drive on into the post. The rod's
    tip has the bore to sit in once the cap is nested in the pocket.
    """
    shell, _, stats = part_objects()
    rod_r = 0.5 * gfe.INCH / 2.0
    lay = math.pi / 2

    objs = []

    def add(tris, colour, offset_x, spec=0.2, shine=28):
        objs.append({"tris": transform(tris, lay, (offset_x, 0, 0)),
                     "colour": colour, "spec": spec, "shininess": shine})

    # Wood post the screw drives into.
    post, _ = box(20, 52, 50, centre=(-88, 0, 0))
    objs.append({"tris": post, "colour": WOOD, "spec": 0.05, "shininess": 10})

    # Screw: shank then head, pointing at the post.
    shank, _ = cylinder(2.1, 18)
    add(shank, STEEL, -72, spec=0.50, shine=70)
    head, _ = cylinder(3.75, 3.0)
    add(head, STEEL, -54, spec=0.50, shine=70)

    # The printed spacer: foot toward the post, pocket toward the cap.
    add(shell, PLASTIC, -44, spec=0.26, shine=34)

    # Black rubber cap, which drops into that pocket, and the rod it rides on.
    rubber, _ = cylinder(1.25 * gfe.INCH / 2, 9.0, hole=rod_r + 0.15)
    add(rubber, RUBBER, -4, spec=0.14, shine=18)
    rod, _ = cylinder(rod_r, 59)
    add(rod, STEEL, -9, spec=0.50, shine=70)

    # White gate fastener, built as a frame so the rod hole is a real hole.
    hw, half, plate_x, plate_t = 7.0, 18.0, 16.0, 5.0
    for centre, sy, sz in (
        ((plate_x, 0, (hw + half) / 2), 2 * half, half - hw),
        ((plate_x, 0, -(hw + half) / 2), 2 * half, half - hw),
        ((plate_x, (hw + half) / 2, 0), half - hw, 2 * hw),
        ((plate_x, -(hw + half) / 2, 0), half - hw, 2 * hw),
    ):
        tris, _ = box(plate_t, sy, sz, centre)
        objs.append({"tris": tris, "colour": WHITE_METAL, "spec": 0.32,
                     "shininess": 44})

    # Nut that pulls the rod up.
    nut, _ = hex_nut(0.75 * gfe.INCH, 8.4, 0.5 * gfe.INCH + 0.4)
    add(nut, STEEL, 30, spec=0.50, shine=70)

    target = (-24, 0, 0)
    eye = orbit(target, 178, -72, 17)
    px = render(objs, size[0], size[1], eye, target, fov=27)
    write_png(path, px, *size)
    return stats


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    out = os.path.join(here, "renders")
    os.makedirs(out, exist_ok=True)
    for name, fn in (("part_iso", scene_iso),
                     ("part_section", scene_section),
                     ("assembly_exploded", scene_exploded)):
        path = os.path.join(out, name + ".png")
        fn(path)
        print(f"wrote {path} ({os.path.getsize(path) / 1024:.0f} KB)")


if __name__ == "__main__":
    main()
