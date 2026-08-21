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

## Measure these two things first

Everything else is safe to leave alone. These two decide whether the print is
usable, and the defaults are a guess from the photos:

| What | Default | How to measure |
|---|---|---|
| **The empty space** between the face of the rubber cap and the post | `--length 22` | Hold a ruler in the gap with the rod pushed home. Set `--length` to that gap **plus** the pocket depth (5 mm), since the cap sinks into the pocket. |
| **The rubber cap's diameter** | `--pocket-size 25.4` (1") | Calipers across the widest part of the black cap. Too small and it will not seat; too big and it rattles. |

Worth a quick check too: the rod is assumed to be **1/2" (12.7 mm)**. Measure
the rod itself, not the hole it sits in — in the photos it could plausibly be
3/8" (9.53) or 5/16" (7.94), and a 1/2" bore on a 3/8" rod will rattle. Change
it with `--bolt-diameter`.

## Ready-made STLs

`stl/` covers a range of gaps, all with a 13.3 mm bore for a 1/2" rod and a
25.9 mm × 5 mm pocket for a 1" rubber cap:

`spacer_10mm_cap` · `spacer_14mm_cap` · `spacer_18mm_cap` · `spacer_22mm_cap` ·
`spacer_26mm_cap` · `spacer_30mm_cap` · `spacer_35mm_cap`

The number is the part's overall length, so pick `gap + 5`. There is also
`spacer_22mm_hexnut.stl`, which swaps the round pocket for a 19.55 mm hex one
if you would rather trap a nut than the rubber cap.

## Rolling your own size

```
python3 gate_fastener_extension.py --length 27 --pocket-size 24.5 -o mine.stl
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
- Print the shortest variant first as a fit test before committing to the real
  one; it is a few minutes and confirms the bore and pocket sizes.
