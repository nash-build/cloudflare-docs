#!/usr/bin/env python3
"""
Parametric bolt-through spacer that fills the empty space between the black
rubber cap and the wood post.

The rod carrying the rubber cap is too short to reach the post, so the cap hangs
in mid air. This part is a stepped sleeve that goes on that rod and bridges the
gap: a flat foot bears on the wood, and a pocket at the other end swallows the
rubber cap. The rod passes straight through a clearance bore. The pocket is
counterbored larger than the bore, and the shoulder left between the two is the
"lip" -- that is what the cap (or the bolt head behind it) catches on as the rod
is tightened up.

    cap end
      |  [ pocket: the black rubber cap seats in here ]
      |  ------------------- <- the lip (shoulder)
      |  [ bore: rod passes straight through ]
      |
    foot (flat against the wood post)

Everything below is in millimetres. Inch values are converted inline so the
numbers stay readable.

Usage:
    python3 gate_fastener_extension.py                 # default variant
    python3 gate_fastener_extension.py --help          # all parameters
    python3 gate_fastener_extension.py --length 32 --pocket hex -o out.stl
"""

import argparse
import math
import struct

INCH = 25.4

# ---------------------------------------------------------------------------
# 2D profiles: each is a function of angle (radians) returning a radius in mm.
# Sampling every profile by angle means the circular and hexagonal profiles
# share a vertex parameterisation, which keeps the mesh stitching trivial.
# ---------------------------------------------------------------------------


def circle(diameter):
    r = diameter / 2.0

    def f(_theta):
        return r

    return f


def hexagon(across_flats):
    """Hexagon with a flat facing theta = 0, sized across the flats."""
    a = across_flats / 2.0
    sixty = math.pi / 3.0
    thirty = math.pi / 6.0

    def f(theta):
        folded = ((theta + thirty) % sixty) - thirty
        return a / math.cos(folded)

    return f


def offset(profile, delta):
    return lambda theta: profile(theta) + delta


def max_radius(profile, segments):
    return max(profile(2 * math.pi * i / segments) for i in range(segments))


# ---------------------------------------------------------------------------
# Mesh construction
#
# The solid is described as a stack of levels, each level being (z, outer
# profile, inner profile). Consecutive levels at different z become sloped or
# straight walls; consecutive levels at the same z become a horizontal ring
# face (a step). The two ends are capped with ring faces.
# ---------------------------------------------------------------------------


def build_mesh(levels, segments):
    tris = []
    thetas = [2 * math.pi * i / segments for i in range(segments)]

    def pt(profile, theta, z):
        r = profile(theta)
        return (r * math.cos(theta), r * math.sin(theta), z)

    def wall(p_lo, p_hi, z_lo, z_hi, outward):
        for i in range(segments):
            t0, t1 = thetas[i], thetas[(i + 1) % segments]
            a = pt(p_lo, t0, z_lo)
            b = pt(p_lo, t1, z_lo)
            c = pt(p_hi, t1, z_hi)
            d = pt(p_hi, t0, z_hi)
            if outward:
                tris.append((a, b, c))
                tris.append((a, c, d))
            else:
                tris.append((a, c, b))
                tris.append((a, d, c))

    def ring(p_in, p_out, z, up):
        """Horizontal annulus at z. up=True -> normal +z (material below)."""
        for i in range(segments):
            t0, t1 = thetas[i], thetas[(i + 1) % segments]
            ai = pt(p_in, t0, z)
            bi = pt(p_in, t1, z)
            ao = pt(p_out, t0, z)
            bo = pt(p_out, t1, z)
            if up:
                tris.append((ai, ao, bo))
                tris.append((ai, bo, bi))
            else:
                tris.append((ai, bo, ao))
                tris.append((ai, bi, bo))

    z0, out0, in0 = levels[0]
    ring(in0, out0, z0, up=False)  # bracket-end face

    for (za, oa, ia), (zb, ob, ib) in zip(levels, levels[1:]):
        if abs(zb - za) > 1e-9:
            wall(oa, ob, za, zb, outward=True)
            wall(ia, ib, za, zb, outward=False)
        else:
            # Step. Whichever profile grew leaves an exposed horizontal face.
            if max_radius(ib, segments) > max_radius(ia, segments) + 1e-9:
                ring(ia, ib, za, up=True)  # counterbore shoulder: the lip
            elif max_radius(ia, segments) > max_radius(ib, segments) + 1e-9:
                ring(ib, ia, za, up=False)
            if max_radius(ob, segments) > max_radius(oa, segments) + 1e-9:
                ring(oa, ob, za, up=False)
            elif max_radius(oa, segments) > max_radius(ob, segments) + 1e-9:
                ring(ob, oa, za, up=True)

    zN, outN, inN = levels[-1]
    ring(inN, outN, zN, up=True)  # outer-end face

    return tris


def write_binary_stl(path, tris, header="gate fastener extension"):
    with open(path, "wb") as fh:
        fh.write(header.encode("ascii", "replace").ljust(80, b" ")[:80])
        fh.write(struct.pack("<I", len(tris)))
        for a, b, c in tris:
            ux, uy, uz = (b[0] - a[0], b[1] - a[1], b[2] - a[2])
            vx, vy, vz = (c[0] - a[0], c[1] - a[1], c[2] - a[2])
            nx, ny, nz = (uy * vz - uz * vy, uz * vx - ux * vz, ux * vy - uy * vx)
            mag = math.sqrt(nx * nx + ny * ny + nz * nz) or 1.0
            fh.write(struct.pack("<3f", nx / mag, ny / mag, nz / mag))
            for v in (a, b, c):
                fh.write(struct.pack("<3f", *v))
            fh.write(struct.pack("<H", 0))


# ---------------------------------------------------------------------------
# The part
# ---------------------------------------------------------------------------


def part_levels(
    length,
    bolt_diameter=0.5 * INCH,
    bolt_clearance=0.6,
    pocket_kind="round",
    pocket_size=1.0 * INCH,
    pocket_clearance=0.5,
    pocket_depth=5.0,
    wall=4.0,
    chamfer=1.0,
    segments=180,
):
    """Return the (z, outer profile, inner profile) stack plus derived sizes.

    Kept separate from meshing so other tools -- the renderer, for one -- can
    reuse the exact same geometry description.
    """
    bore = circle(bolt_diameter + bolt_clearance)
    if pocket_kind == "hex":
        pocket = hexagon(pocket_size + pocket_clearance)
    else:
        pocket = circle(pocket_size + pocket_clearance)

    bore_r = max_radius(bore, segments)
    pocket_r = max_radius(pocket, segments)

    if pocket_r <= bore_r:
        raise SystemExit(
            "The pocket must be larger than the bore, otherwise there is no lip "
            "for the bolt head to catch on."
        )
    if pocket_depth <= chamfer:
        raise SystemExit("--pocket-depth must be greater than --chamfer.")
    if length <= pocket_depth + chamfer:
        raise SystemExit("--length must be greater than --pocket-depth + --chamfer.")

    outer_d = 2.0 * (pocket_r + wall)
    outer = circle(outer_d)

    h = length
    pd = pocket_depth
    c = chamfer

    levels = [
        # z,          outer profile,        inner profile
        (0.0, offset(outer, -c), offset(bore, c)),  # foot, chamfered
        (c, outer, bore),
        (h - pd, outer, bore),
        (h - pd, outer, pocket),  # step -> the lip
        (h - c, outer, pocket),
        (h, offset(outer, -c), offset(pocket, 0.6)),  # outer face + lead-in
    ]

    stats = {
        "length": h,
        "pocket_kind": pocket_kind,
        "outer_diameter": outer_d,
        "bore_diameter": bolt_diameter + bolt_clearance,
        "pocket_across": pocket_size + pocket_clearance,
        "pocket_depth": pd,
        "lip_width": pocket_r - bore_r,
        "wall": wall,
    }
    return levels, stats


def make_part(segments=180, **kwargs):
    levels, stats = part_levels(segments=segments, **kwargs)
    tris = build_mesh(levels, segments)
    stats["triangles"] = len(tris)
    return tris, stats


def main():
    p = argparse.ArgumentParser(
        description="Generate an STL for a bolt-through gate fastener extension.",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument(
        "--length",
        type=float,
        default=25.0,
        help="Total length in mm. Set it to the empty space between the face "
        "of the rubber cap and the wood post, plus the pocket depth.",
    )
    p.add_argument(
        "--bolt-diameter",
        type=float,
        default=0.5 * INCH,
        help="Nominal rod diameter in mm (1/2 inch = 12.7). Measure the rod "
        "itself, not the hole it sits in.",
    )
    p.add_argument(
        "--bolt-clearance",
        type=float,
        default=0.6,
        help="Added to the bolt diameter to give the through bore.",
    )
    p.add_argument(
        "--pocket",
        choices=["hex", "round"],
        default="round",
        help="Shape of the pocket. 'round' cradles the rubber cap; 'hex' "
        "captures a nut instead and stops it spinning.",
    )
    p.add_argument(
        "--pocket-size",
        type=float,
        default=1.0 * INCH,
        help="Diameter of the round pocket, or across the flats for a hex one. "
        "For a round pocket this is the rubber cap's diameter -- MEASURE IT.",
    )
    p.add_argument("--pocket-clearance", type=float, default=0.5)
    p.add_argument(
        "--pocket-depth",
        type=float,
        default=5.0,
        help="How far the rubber cap sinks into the pocket. Everything below "
        "it is the bore, and the shoulder between the two is the lip.",
    )
    p.add_argument(
        "--wall",
        type=float,
        default=4.0,
        help="Material left around the pocket, which sets the outer diameter.",
    )
    p.add_argument("--chamfer", type=float, default=1.0)
    p.add_argument("--segments", type=int, default=180)
    p.add_argument("-o", "--output", default="gate_post_spacer.stl")
    args = p.parse_args()

    tris, stats = make_part(
        length=args.length,
        bolt_diameter=args.bolt_diameter,
        bolt_clearance=args.bolt_clearance,
        pocket_kind=args.pocket,
        pocket_size=args.pocket_size,
        pocket_clearance=args.pocket_clearance,
        pocket_depth=args.pocket_depth,
        wall=args.wall,
        chamfer=args.chamfer,
        segments=args.segments,
    )
    write_binary_stl(args.output, tris)

    print(f"wrote {args.output}")
    print(f"  length            {args.length:.2f} mm")
    print(f"  outer diameter    {stats['outer_diameter']:.2f} mm")
    print(f"  through bore      {stats['bore_diameter']:.2f} mm")
    print(f"  {args.pocket} pocket     {stats['pocket_across']:.2f} mm x "
          f"{args.pocket_depth:.2f} mm deep")
    print(f"  lip width         {stats['lip_width']:.2f} mm (radial)")
    print(f"  triangles         {stats['triangles']}")


if __name__ == "__main__":
    main()
