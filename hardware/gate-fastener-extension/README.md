# Rubber cap / post spacer

The rod carrying the black rubber cap is too short to reach the wood post, so
the cap hangs in mid air. This is a 3D printable sleeve that goes on that rod
and fills the empty space: a flat foot lands on the post, and a pocket at the
other end swallows the rubber cap.

The rod passes straight through a clearance bore. The pocket is counterbored
larger than that bore, and the shoulder left between the two is the **lip** —
what the cap, and the bolt head behind it, catch on as the rod is tightened.

```
   wood post                                    white gate fastener
       │                                                │
       │  ┌────────────────┐                            │
       │  │                │  ▓▓▓▓▓▓                    │
       │  │  ┌──────────┐  │  ▓ cap ▓                   │
       ├──┤  │   bore   │  ├──▓▓▓▓▓▓──────[ rod ]───────┤──[nut]
       │  │  └──────────┘  │  ▓▓▓▓▓▓                    │
       │  │                │  ▓▓▓▓▓▓                    │
       │  └────────────────┘                            │
       │   ↑ foot          ↑ lip / pocket
```

The whole stack, in order: **post → printed spacer → rubber cap → white gate
fastener → nut.**

## What it looks like

`renders/` holds views of the actual mesh, not sketches of it — `render.py`
rasterises the same geometry the STL is built from, and `section_drawing.py`
traces its dimensions off the same profile. Both are dependency-free:

```
python3 render.py            # part_iso, part_section, assembly_exploded (PNG)
python3 section_drawing.py   # section_drawing.svg, dimensioned
```

- `part_iso.png` — the printed part, pocket up, in its print orientation.
- `part_section.png` — cut through the axis: cap pocket, the lip, the bore.
- `assembly_exploded.png` — the stack above, exploded along the rod. The orange
  part is the one you print; everything grey is hardware you already have.
- `section_drawing.svg` — the same section, dimensioned.

## As built

| | |
|---|---|
| Overall height | **31.75 mm** (1-1/4") |
| Outside diameter | 40.25 mm |
| Through bore | 13.30 mm, for a 1/2" rod |
| Cap pocket | ⌀32.25 mm × 5.0 mm deep, for the 1-1/4" rubber cap |
| The lip | 9.48 mm wide |
| Wall around the pocket | 4.0 mm |

The pocket is the cap's 31.75 mm plus 0.5 mm so it drops in rather than needing
a press. The bore is the rod's 12.7 mm plus 0.6 mm for the same reason.

One thing worth checking before you print: the part is 1-1/4" **tall**, and the
cap sinks 5 mm into the pocket, so it pushes the cap 26.75 mm closer to the
post — not the full 1-1/4". If what you actually measured was a 1-1/4" *gap* to
close, print `spacer_fills_1-1_4in_gap.stl` (36.75 mm tall) instead.

The rod is assumed to be **1/2" (12.7 mm)**. Measure the rod itself, not the
hole it sits in — in the photos it could plausibly be 3/8" (9.53), and a 1/2"
bore on a 3/8" rod will rattle. Change it with `--bolt-diameter`.

## Ready-made STLs

All share the 13.3 mm bore and the ⌀32.25 × 5 mm cap pocket:

- **`spacer_1-1_4in.stl`** — 31.75 mm tall. The one to print.
- `spacer_fills_1-1_4in_gap.stl` — 36.75 mm, if you meant a 1-1/4" gap.
- `spacer_1-1_4in_minus2mm.stl` / `spacer_1-1_4in_plus2mm.stl` — 29.75 and
  33.75 mm, for dialling in the fit.
- `spacer_1-1_4in_hexnut.stl` — same height, but a 19.55 mm hex pocket that
  traps a nut instead of the rubber cap.

## Rolling your own size

```
python3 gate_fastener_extension.py --length 34 --pocket-size 31.75 -o mine.stl
python3 gate_fastener_extension.py --help      # every parameter
```

## Printing

- **Orientation:** foot down on the bed, pocket up. That puts the layer lines
  across the rod axis, so the clamping load is in compression rather than
  pulling layers apart.
- **Walls / infill:** 4 perimeters, 40% infill. This part is squeezed between a
  nut and a wooden post, and 20% sparse infill will creep over time.
- **Material:** PETG or ASA if it lives outdoors. PLA will soften in direct sun
  and go brittle over a season or two.
- **No supports needed** — the pocket is a flat-bottomed recess and the bore is
  vertical.
- At 40 % infill this is roughly 20 g and a couple of hours. If you want to
  check the bore and pocket sizes first, slice `spacer_1-1_4in.stl` and stop it
  after ~8 mm of height — that is enough to test the rod fit.
