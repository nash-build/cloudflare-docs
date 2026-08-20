# Gate fastener extension

A 3D printable standoff that pushes the rubber cap on a gate stop further out to
close a gap, without replacing the white metal fastener.

The bolt passes through a clearance bore. The outer end is counterbored larger
than that bore, and the shoulder left between the two is the **lip** — that is
what the bolt head bears on as you tighten it down against the white fastener.
Tightening clamps the standoff flat to the fastener and moves the head, and
whatever is on it, outward by the full length of the part.

```
   gate side
     ┌───────────────┐
     │   ▓▓ pocket ▓▓│   hex head / nut / rubber cap seats here
     ├──┐         ┌──┤ ← the lip
     │  │  bore   │  │   bolt shank passes straight through
     │  │         │  │
     └──┴─────────┴──┘
   ═══════════════════   flat against the white gate fastener
```

## Measure these three things first

The defaults are a guess at your hardware. Two of them are worth checking with
calipers before you spend an hour of print time:

| What | Default | Notes |
|---|---|---|
| **The gap** you need to close | `--length 25` | Set this to the gap. The part extends reach by exactly its own length. Add 2–3 mm if you want the rubber to squash slightly when the gate closes. |
| **Bolt diameter** | `--bolt-diameter 12.7` (1/2") | Measure the threaded rod itself, not the hole. In the photos it could plausibly be 3/8" (9.525) or 5/16" (7.94) — a 1/2" bore on a 3/8" rod will rattle. |
| **Head or nut size across the flats** | `--pocket-size 19.05` (3/4") | Standard for a 1/2" bolt. Measure flat-to-flat with the jaws, not corner-to-corner. |

## Printing the ready-made STLs

`stl/` has five variants, all with a 13.3 mm bore for a 1/2" bolt:

- `extension_15mm_hex.stl`, `extension_25mm_hex.stl`, `extension_35mm_hex.stl` —
  hex pocket, 19.55 mm across the flats, 9 mm deep. The hex also stops a nut
  from spinning while you tighten from the other side.
- `extension_25mm_round.stl` — 22.9 mm round pocket, for a head plus a washer.
- `extension_25mm_rubbercap.stl` — 25.4 mm round pocket, 5 mm deep, sized to
  cradle the black rubber cap itself so the lip retains it.

Print settings that matter, given this part takes a gate swinging into it:

- **Orientation:** flat face on the bed, pocket facing up. Layers then run
  across the bolt axis, so the impact load compresses the layers instead of
  peeling them apart. No supports needed in this orientation.
- **Walls:** 4–5 perimeters. Perimeters carry far more of this load than infill.
- **Infill:** 40–50%, gyroid or cubic.
- **Material:** PETG or ASA outdoors. PLA will creep under a tightened bolt and
  goes brittle in UV within a season or two.

Roughly 15 cm³ of plastic for the 25 mm version — about 20 g, under two hours.

## Changing the dimensions

Either tool produces the same part. Python needs no dependencies:

```
python3 gate_fastener_extension.py --length 32 --bolt-diameter 9.525 \
        --pocket-size 14.29 -o my_extension.stl
```

`--help` lists every parameter. `gate_fastener_extension.scad` is the same part
for OpenSCAD, with the parameters grouped for the customizer.

The outer diameter is derived rather than set: it is the pocket's circumscribed
radius plus `--wall` (5 mm by default), so it grows automatically if you enlarge
the pocket. The generator refuses to build a part where the pocket is not larger
than the bore, since that would leave no lip at all.

## One alternative worth knowing

If the gap is more than about 40 mm, a longer bolt is stronger, cheaper and
faster than printing a standoff this tall — a plastic column that long starts
acting as a lever on the fastener's mounting screws every time the gate hits it.
Under about 40 mm the printed standoff is fine.
