// Parametric bolt-through standoff ("extension") for a gate fastener / gate stop.
//
// Same part as gate_fastener_extension.py, for anyone who would rather tweak it
// in OpenSCAD. All dimensions in millimetres.
//
//        outer end (gate side)
//          [ pocket ]  <- hex head, nut, or the rubber cap sits in here
//          ----------  <- the lip: shoulder the bolt head tightens against
//          [  bore  ]  <- bolt shank passes straight through
//        bracket end   <- flat against the white gate fastener

/* [Fit] */
// Total length. This is how much further out the bolt head ends up, so set it
// to the gap you measured.
length            = 25;
// Nominal bolt diameter. 1/2" = 12.7, 3/8" = 9.525, 5/16" = 7.94.
bolt_diameter     = 12.7;
// Added to the bolt diameter to give the through bore.
bolt_clearance    = 0.6;

/* [Pocket] */
// "hex" also stops a nut spinning; "round" suits a washer or the rubber cap.
pocket_kind       = "hex";  // [hex, round]
// Across the flats for hex, diameter for round. 3/4" = 19.05 is the standard
// head size on a 1/2" bolt.
pocket_size       = 19.05;
pocket_clearance  = 0.5;
// Depth of the pocket. Everything below it is bore; the step between the two
// is the lip.
pocket_depth      = 9;

/* [Body] */
// Material left around the pocket. This sets the outer diameter.
wall              = 5;
chamfer           = 1;

/* [Hidden] */
$fn = 180;
eps = 0.01;

bore_d    = bolt_diameter + bolt_clearance;
pocket_d  = pocket_size + pocket_clearance;
// Circumscribed radius, so the wall is measured from the pocket corners.
pocket_r  = (pocket_kind == "hex") ? pocket_d / sqrt(3) : pocket_d / 2;
outer_d   = 2 * (pocket_r + wall);

assert(pocket_r > bore_d / 2,
       "Pocket must be larger than the bore or there is no lip.");
assert(length > pocket_depth + chamfer,
       "length must exceed pocket_depth + chamfer.");

module pocket_solid(h, grow = 0) {
    if (pocket_kind == "hex")
        cylinder(h = h, r = pocket_d / sqrt(3) + grow, $fn = 6);
    else
        cylinder(h = h, d = pocket_d + 2 * grow);
}

module body() {
    // Chamfered at both ends so it seats flat and the print releases cleanly.
    hull() {
        translate([0, 0, chamfer])
            cylinder(h = length - 2 * chamfer, d = outer_d);
        cylinder(h = eps, d = outer_d - 2 * chamfer);
        translate([0, 0, length - eps])
            cylinder(h = eps, d = outer_d - 2 * chamfer);
    }
}

difference() {
    body();

    // Through bore, with a lead-in chamfer at the bracket end.
    translate([0, 0, -eps])
        cylinder(h = length + 2 * eps, d = bore_d);
    translate([0, 0, -eps])
        cylinder(h = chamfer + eps, d1 = bore_d + 2 * chamfer, d2 = bore_d);

    // Pocket, with a lead-in so the head or nut starts square.
    translate([0, 0, length - pocket_depth])
        pocket_solid(pocket_depth + eps);
    translate([0, 0, length - 1])
        hull() {
            pocket_solid(eps);
            translate([0, 0, 1 + eps]) pocket_solid(eps, grow = 0.6);
        }
}
