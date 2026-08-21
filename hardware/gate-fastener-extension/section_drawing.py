#!/usr/bin/env python3
"""
Dimensioned cross-section of the extension, written as SVG.

The outline is traced from the same profile the STL is built from, and every
number is read off that profile, so the drawing cannot drift out of step with
the model.

    python3 section_drawing.py [--length 25] [-o renders/section_drawing.svg]
"""

import argparse
import os

import gate_fastener_extension as gfe

SEG = 96
INK = "#2f3237"
THIN = "#7b818b"
FILL = "#f0a35f"
FILL_EDGE = "#c9772f"
SHEET = "#fcfcfa"


def esc(v):
    return f"{v:.2f}"


def build(length, scale=13.0, **kw):
    levels, stats = gfe.part_levels(length=length, segments=SEG, **kw)
    theta = 0.0  # section through the pocket flats
    outer = [(z, o(theta)) for (z, o, _) in levels]
    inner = [(z, i(theta)) for (z, _, i) in levels]

    h = stats["length"]
    ro = stats["outer_diameter"] / 2.0
    rb = stats["bore_diameter"] / 2.0
    pd = stats["pocket_depth"]
    pa = stats["pocket_across"]

    left, top = 140.0, 126.0
    ro_px = ro * scale
    cy = top + ro_px
    width = left + h * scale + 168
    height = top + 2 * ro_px + 116

    def X(z):
        return left + z * scale

    def Y(r):
        return cy - r * scale

    def half(sign):
        pts = [(X(z), Y(r * sign)) for z, r in outer]
        pts += [(X(z), Y(r * sign)) for z, r in reversed(inner)]
        return " ".join(f"{esc(x)},{esc(y)}" for x, y in pts)

    labels = []  # (x0, y0, x1, y1, text) for the layout self-check

    s = []
    a = s.append
    a(f'<svg xmlns="http://www.w3.org/2000/svg" width="{width:.0f}" '
      f'height="{height:.0f}" viewBox="0 0 {width:.0f} {height:.0f}" '
      f'font-family="Helvetica Neue, Helvetica, Arial, sans-serif">')
    a('<defs>')
    a(f'<marker id="ar" markerWidth="9" markerHeight="9" refX="8" refY="3" '
      f'orient="auto"><path d="M0,0 L8,3 L0,6 z" fill="{INK}"/></marker>')
    a(f'<marker id="al" markerWidth="9" markerHeight="9" refX="0" refY="3" '
      f'orient="auto"><path d="M8,0 L0,3 L8,6 z" fill="{INK}"/></marker>')
    a('<pattern id="hatch" width="7" height="7" patternUnits="userSpaceOnUse" '
      'patternTransform="rotate(45)">'
      f'<line x1="0" y1="0" x2="0" y2="7" stroke="{FILL_EDGE}" '
      'stroke-width="1.1" opacity="0.5"/></pattern>')
    a('</defs>')
    a(f'<rect width="100%" height="100%" fill="{SHEET}"/>')

    for sign in (1, -1):
        pts = half(sign)
        a(f'<polygon points="{pts}" fill="{FILL}" stroke="none"/>')
        a(f'<polygon points="{pts}" fill="url(#hatch)" '
          f'stroke="{FILL_EDGE}" stroke-width="1.6" stroke-linejoin="round"/>')

    a(f'<line x1="{esc(X(-1.2))}" y1="{esc(Y(0))}" x2="{esc(X(h + 3.0))}" '
      f'y2="{esc(Y(0))}" stroke="{THIN}" stroke-width="1" '
      'stroke-dasharray="14 4 3 4"/>')

    def text(x, y, label, size=15, anchor="middle", fill=INK, weight="400"):
        a(f'<text x="{esc(x)}" y="{esc(y)}" fill="{fill}" font-size="{size}" '
          f'text-anchor="{anchor}" font-weight="{weight}">{label}</text>')
        w = len(label) * size * 0.52
        x0 = {"middle": x - w / 2, "start": x, "end": x - w}[anchor]
        labels.append((x0, y - size, x0 + w, y + 4, label))

    def ext(x, y1, y2):
        a(f'<line x1="{esc(x)}" y1="{esc(y1)}" x2="{esc(x)}" y2="{esc(y2)}" '
          f'stroke="{THIN}" stroke-width="0.8"/>')

    def exth(y, x1, x2):
        a(f'<line x1="{esc(x1)}" y1="{esc(y)}" x2="{esc(x2)}" y2="{esc(y)}" '
          f'stroke="{THIN}" stroke-width="0.8"/>')

    def dim_line(x1, y1, x2, y2):
        a(f'<line x1="{esc(x1)}" y1="{esc(y1)}" x2="{esc(x2)}" y2="{esc(y2)}" '
          f'stroke="{INK}" stroke-width="1.1" marker-start="url(#al)" '
          'marker-end="url(#ar)"/>')

    # Overall length, below the part.
    y_len = Y(-ro) + 58
    ext(X(0), Y(-ro) + 6, y_len + 8)
    ext(X(h), Y(-ro) + 6, y_len + 8)
    dim_line(X(0), y_len, X(h), y_len)
    text((X(0) + X(h)) / 2, y_len + 20, f"{h:.2f} overall (1-1/4 in)")

    # Pocket, above the part. One label rather than two crossing dimensions.
    y_pd = Y(ro) - 34
    ext(X(h - pd), Y(ro) - 6, y_pd - 8)
    ext(X(h), Y(ro) - 6, y_pd - 8)
    dim_line(X(h - pd), y_pd, X(h), y_pd)
    pocket_label = (f"hex pocket {pa:.2f} A/F \u00d7 {pd:.1f} deep"
                    if stats["pocket_kind"] == "hex"
                    else f"cap pocket \u2300{pa:.2f} \u00d7 {pd:.1f} deep")
    text((X(h - pd) + X(h)) / 2, y_pd - 12, pocket_label, anchor="middle")

    # Outer diameter, right.
    x_od = X(h) + 74
    exth(Y(ro), X(h) - 4, x_od + 8)
    exth(Y(-ro), X(h) - 4, x_od + 8)
    dim_line(x_od, Y(ro), x_od, Y(-ro))
    text(x_od + 10, Y(0) + 5, f"\u2300{stats['outer_diameter']:.1f}", anchor="start")

    # Bore diameter, left.
    x_bore = X(0) - 70
    exth(Y(rb), X(0) + 4, x_bore - 8)
    exth(Y(-rb), X(0) + 4, x_bore - 8)
    dim_line(x_bore, Y(rb), x_bore, Y(-rb))
    text(x_bore - 10, Y(0) + 5, f"\u2300{stats['bore_diameter']:.1f}", anchor="end")

    # Leader onto the lip, with the label sitting in the empty bore.
    lip_y = Y((rb + pa / 2) / 2)
    a(f'<path d="M{esc(X(4.0))},{esc(Y(2.6))} L{esc(X(9.6))},{esc(Y(2.6))} '
      f'L{esc(X(h - pd))},{esc(lip_y)}" fill="none" stroke="{INK}" '
      'stroke-width="1.1" marker-end="url(#ar)"/>')
    text(X(4.0), Y(2.6) - 8, f"the lip \u2014 {stats['lip_width']:.1f} wide",
         anchor="start")

    # Which way round it goes.
    text(X(0) + 8, Y(-ro) + 30, "foot \u2014 on the post", size=14,
         anchor="start", fill=THIN)
    text(X(h) - 8, Y(-ro) + 30, "rubber cap end", size=14, anchor="end", fill=THIN)

    text(left, 34, "Rubber cap / post spacer \u2014 section", size=19,
         anchor="start", weight="600")
    text(left, 58,
         f"all dimensions mm \u00b7 wall {stats['wall']:.1f} \u00b7 "
         f"bore for a {stats['bore_diameter'] - 0.6:.1f} mm rod",
         size=14, anchor="start", fill=THIN)

    a('</svg>')

    clashes = []
    for i, la in enumerate(labels):
        for lb in labels[i + 1:]:
            if (la[0] < lb[2] and lb[0] < la[2]
                    and la[1] < lb[3] and lb[1] < la[3]):
                clashes.append((la[4], lb[4]))
    for x0, y0, x1, y1, txt in labels:
        if x0 < 2 or y0 < 2 or x1 > width - 2 or y1 > height - 2:
            clashes.append((txt, "OFF-SHEET"))
    return "\n".join(s), stats, clashes


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--length", type=float, default=1.25 * gfe.INCH)
    p.add_argument("-o", "--output", default=None)
    args = p.parse_args()
    here = os.path.dirname(os.path.abspath(__file__))
    out = args.output or os.path.join(here, "renders", "section_drawing.svg")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    svg, stats, clashes = build(args.length)
    for a_, b_ in clashes:
        print(f"  layout warning: {a_!r} overlaps {b_!r}")
    with open(out, "w") as fh:
        fh.write(svg)
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
