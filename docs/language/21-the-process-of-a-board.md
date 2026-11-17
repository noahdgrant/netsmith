## The process of a board

A board is made by one manufacturing process. Its design rules and stackup come from a definition in the standard library, and the board has it as a member named `process` (ADR 0008, ADR 0111).

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Bare {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
        .min_track &= 0.15mm..
    }
    .header: Header1x2
}
```

Rules.

- Each rule is the least the factory can make, written as a range. `.min_track &= 0.1mm..` says the least track is at least 0.1 mm. A board narrows it upward for margin. The rule used is the bottom of what is left.
- A rule is a lower bound, a range with an open end (`0.1mm..`, see _A range with an open end_ in the chapter on values): there is no upper limit.
- A process states every rule its fabricator's page gives, or says in its doc comment why it does not (issue #431). The router keeps each rule and netsmith's design rules check checks it: the copper rules a KiCad project file can hold go there (`min_clearance`, `min_track_width`, `min_hole_to_hole`...), and the others are netsmith's own rules in the check: `hole_to_track` (a via's or a non-plated hole to a track, 0.2 mm, and a plated pad's hole, `.min_plated_hole_clearance`, 0.28 mm), `pad_to_pad` (surface-mount pads of different nets, 0.15 mm), `pad_hole_to_hole` (two holes when one is a pad's, 0.45 mm; two vias' holes are 0.2 mm, `.min_via_hole_to_hole`), `mask_to_track` (a solder mask opening to a track of another net, 0.09 mm; the router keeps it from an opening its footprint draws in the mask too, on both faces, and the check measures it from a pad's opening), `slot_width`, `npth_drill` and `plated_ring` (the copper round a plated pad's hole, `.min_annular_ring`; a via's is `.min_via_ring`).
- A process states the rules for fiducials and test points too (issue #761), from JLCPCB's help article on edge rails and fiducials, its fiducial blog and its test point blog, each read on 2026-11-16 (the pages and their wording are in the header of `std/mfg/jlcpcb.nsm`). Each is a limit or a guideline by the page's own words, as the owner decided on 2026-11-15: what the page says JLCPCB cannot make or place is a **limit**, an error of the design rules check that stops `fab`, and advice, or wording that is not clear, is a **guideline**, a warning (`design rules warnings` in `fab-notes.txt`, and in the editor's problems list). The limits, on every `Fiducial` of the board: `fiducial_edge` (its copper at least `.min_fiducial_edge`, 3.35 mm, from the board edge), `fiducial_mask` (its copper is exposed, no solder mask over it) and `fiducial_keepout` (no other copper, via or pad inside its solder mask opening, its keep-out; a pour is not checked, because its fill is not computed). The guidelines: `fiducial_count` (each side that has parts JLCPCB assembles has at least `.min_fiducials`, 2, fiducials: the page's "a minimum of two, but three are strongly recommended"; the help article says JLCPCB adds edge rails with fiducials to an assembled board, so a board with fewer is not refused), `fiducial_size` (copper of `.fiducial_diameter`, 1 mm, an offer of the process like the via, and a mask opening at least `.fiducial_mask_ratio`, 2, times the copper), and for every `TestPoint` the "absolute minimum" of the page's tables, advice for a probe fixture: `test_point_pad` (the pad at least `.min_test_point_pad`, 0.8 mm, and at least `.min_power_test_point_pad`, 1.0 mm, where the test point is on a power or ground net: a net that a pin the part's pin table calls `power` is on, as a chip's supply and ground pins are (ADR 0016); a net a rail joins to passive parts alone is not known as one; the finding is one and names the figure that applies), `test_point_pitch` (two on a side at least `.min_test_point_pitch`, 1.27 mm, centre to centre), `test_point_body` (copper at least `.min_test_point_body`, 0.5 mm, from the body of a part on its side, and at least `.min_test_point_tall_body`, 2.0 mm, from the body of a part whose stated or catalog `.height` is above `.tall_part_height`, 6.4 mm, an offer of the process; a part with no height is judged as any other, never as tall, and a package standard's height does not make one tall) and `test_point_edge` (at least `.min_test_point_edge`, 3.2 mm, from the board edge). The page's recommended values are named in the finding and not enforced. A board narrows each upward as any rule (`.process.min_test_point_edge &= 4mm..`); the design file carries them, and a limit is a lower bound like the others. Three more rules come from the same fiducial blog (issue #1006), each a guideline by its wording. `fiducial_angle`: on a **round** board (its outline is one circle, or arcs that close one, that the whole edge fits in; a rounded rectangle or a circular cutout is not round), the fiducials of each side that has any are three in an "L", one with a neighbour `.fiducial_round_angle` (degrees, 90) either way about the board's centre: the page's "three global fiducials ... spaced approximately 90 degrees apart (e.g., 0, 90, and 180 degrees) in a non-symmetrical, non-linear L pattern". The page says "approximately" and gives no figure, so the check allows 10 degrees either way, netsmith's own tolerance; a side with none is the business of `fiducial_count`. `finish_fine_pitch`: a board finished with HASL (`.process.finish`, either kind) that has a part with two surface mount pads on a side the board house assembles at most `.fine_pitch` (0.4 mm, the page's example) apart gets one finding, at the finest part, because "a flat, less-reflective finish like ENIG or OSP is ideal" and "the recommended choice for designs with ultra-fine-pitch components (e.g., 0.4mm pitch BGAs)" while HASL "leaves a slightly uneven, domed, and reflective solder surface", which is "perfectly acceptable for most standard SMT designs"; the finding names `.process.finish &= "ENIG"`. A board with no such part is never told anything about its finish. The rails of a panel (chapter 18) are judged by the fiducial rules too (`fiducial_edge`, `fiducial_mask`, `fiducial_keepout`, `fiducial_count`, `fiducial_size`), on the panel's own board: the page puts a panel's fiducials "on the tooling rails" and gives them no figure of their own, so `fab` holds the three it puts on each assembled side of the rails, 3.85 mm from the panel's edge, to the process's rules, and a V-cut line drawn on the panel's `Edge.Cuts` is not the edge they are measured to. The cells' own test points and finish are judged on the board, not again on the panel. A net that a `Rail` of the design has a pin on, `pos` or `neg`, is a power or ground net to `test_point_pad` as well as a net with a pin the part's pin table calls `power`, so a rail that only resistors and capacitors are on has the 1.0 mm figure too.
- The via the router draws is `.via_drill` and `.via_diameter`, 0.2 mm in 0.45 mm: an offer of the process (`?=`), the smallest via JLCPCB makes at its standard price. A board replaces it with `.process.via_drill &= 0.3mm` and `.process.via_diameter &= 0.6mm`; a via the process cannot make is refused when the board is placed.
- The surface finish is `.finish`, a word the process offers (`?=`): `HASL lead-free` on `TwoLayer` and `FourLayer`, `ENIG` on `SixLayer`. `.finishes` lists what the process offers, which is JLCPCB's "Surface Finish" row: `HASL leaded`, `HASL lead-free`, `ENIG` and `OSP`, and `ENIG` and `OSP` from six layers, where the page says FR-4 boards do not support HASL. A board that wants another says it, `.process.finish &= "ENIG"`, and a word that the process does not offer is refused, listing the offer. `fab` writes the finish to the board file (`copper_finish`), to `fab-notes.txt` (the `process:` line) and to `order.txt` (`finish ENIG`), and the order form has to match it. The default is netsmith's choice, not a number of the page, which names none; the page also says boards of 0.4 mm or less do not support HASL, which the check does not refuse yet, so a board that thin states its finish.
- The solder mask colour is `.stackup.mask_colour`, a variant of the `Color` an LED's `.color` is (`Red`, `Green`, `Blue`, `Yellow`, `White`, `Black` or `Purple`; issue #771), and a board that wants one states it: `.process.stackup.mask_colour &= Color::Black` with `import std::parts::passive::Color`. It is a choice of the board and no rule of the process, so no process states it, and a board that does not is drawn and written as it was before the member existed. Where it shows: the 3D view (the editor's, `netsmith render` and the `.glb` of `netsmith board3d`) draws the mask in it, over copper in a lighter tone of it; the board file's stackup writes it as the colour of both mask layers (KiCad's own default is green); and `order.txt` says `solder mask Black` beside the finish, which the order form has to match. JLCPCB's page lists green, red, yellow, blue, purple, white and black masks (the page's wording of the mask bridge names each), and netsmith does not check the colour against it. A colour has the one tone each drawing gives it: it is no photograph, and a white or black mask is drawn in a light grey or a dark grey that reads on the theme's background. The mask bridge holds for any colour (the rule is the worst case, black and white).
- A board must choose `.stackup.thickness`. The process offers 0.4 mm to 2 mm, and netsmith will not pick.
- A two-layer board may choose its copper weight: `.stackup.copper_weight &= 2` is 2 oz, and the rules that depend on the weight follow (track and space 0.16 mm, annular ring 0.254 mm, mask bridge 0.2 mm), with the current a track and a via carry (see Track width against current). `TwoLayer` is a generic and each weight a provider (see _An interface that provides a generic: a process_); a board that says nothing is made with 1 oz. JLCPCB's page offers 2.5, 3.5 and 4.5 oz too but gives no annular ring or mask bridge for them, so std has no provider for them and a board that asks for one is refused. `FourLayer` and `SixLayer` are 1 oz only: JLCPCB lists no build-up with 2 oz outer copper.
- A board with no `process` builds and views as a schematic. `fab` stops and says to add `import std::mfg::jlcpcb::TwoLayer` and `.process: TwoLayer`. There is no default.
- `.max_temperature_rise` is the rise in kelvin that the track width check designs to (see Track width against current). The process leaves it unstated and it is 10 K when the board does too: `.process.max_temperature_rise &= 20K`.
- `.min_round_track_angle` is the least angle in degrees between a track's centre line and the tangent of a round copper edge (a round pad or via, an oval's end, a rounded corner) where the track leaves it, which the acid trap check judges (see the design rules check). No fabricator publishes a figure and the process leaves it unstated, so round copper is not judged; a board states one to opt in: `.process.min_round_track_angle &= 60`.
- `std::mfg::jlcpcb::FourLayer` has two inner layers and `std::mfg::jlcpcb::SixLayer` four (`.inner1` to `.inner4`, counted from the top). The board says what each is (ADR 0111). A plane is one sheet of copper on a net, and the board names the net: `.process.inner1.net &= "GND"`. A signal layer names no net: `.process.inner2.use &= "signal"`. The router draws tracks on it as on the outer layers, and a line changes layer through a via, which goes through every layer (on an HDI process, below, through the blind or buried via of fewest layers that joins the two); a net with a known current (see Load current) is drawn on a signal layer at the width its current needs in the thinner inner copper, which is wider than outside (see "Track width from current" in The shape of a board). `netsmith routes add-track --layer inner2` and the editor's track tool draw on a signal layer too; a plane holds no track. A track on a signal layer keeps the process's space between inner copper and a hole (0.3 mm on JLCPCB) from every hole. A signal layer is written to the board file as a signal layer, plotted as an inner copper Gerber and checked like the outer layers. An inner layer that is neither is an error, and so is a plane on a net the board does not have. See `examples/four_layer.nsm` and `examples/six_layer.nsm`. A board that says nothing is made 1.6 mm thick, in the build-up `JLC04161H-7628` (four layers) or `JLC06161H-3313` (six); `fab-notes.txt` names the build-up and says what each inner layer is. Both are generics, as `TwoLayer` is, with a provider for the page's default build-up of each of 0.8, 1.0, 1.2, 1.6 and 2.0 mm (`JlcFourLayerD04121H7628` is `JLC04121H-7628`: a letter that orders the providers, then the build-up's name, see the doc comment of `FourLayer`): `.process.stackup.thickness &= 1.2mm` makes the board 1.2 mm thick, in that build-up, and `.process.stackup.build &= "JLC04121H-7628"` asks for a build-up by its name. The page lists many more build-ups (other prepregs, several sheets between two copper layers, a core with no copper between sheets). Each is a provider in `std/mfg/jlcpcb_buildups.nsm`, named for the build-up (`JLC04161H-7628C` is `JlcFourLayer04161H7628C`), and a board names it as the type of its process after importing it, as in the example below; nothing is searched then. They are in a file of their own because each provider a board reaches is one more choice of the provider search (Generics and providers), so a board that does not import the file never tries them, and asking for one of them by `.stackup.build` without importing the file is told that no provider has it. A dielectric layer of several sheets is stated ply by ply, from the top (`.stackup.dielectric1.ply1` to `.ply4`, each a `DielectricPly` with the fields of a one-sheet layer); the board file lists each ply as a sublayer of the layer, and the track impedance reads the layer as one dielectric, the plies' thicknesses added and their permittivities taken in series. The doc comment of `std/mfg/jlcpcb_buildups.nsm` says which of the page's build-ups are left out and why. JLCPCB's pages give no build-up of 8 or 10 layers, so std has no JLCPCB process for them; see the fictional fabricator below.
- A board with a build-up gets its track impedance in `fab-notes.txt` (issues #759 and #1007): for each layer a track may be on (the outer layers and the signal layers, not the planes), the width of a 50 ohm track and the impedance of the router's track width, worked out from the build-up's dielectric layers, their thickness and relative permittivity, and the copper's thickness. An outer track is a microstrip over the next copper layer (Hammerstad and Jensen, 1980) and an inner one a stripline between the copper above and below (Wheeler, 1978); the doc comment of `netsmith_pcb::impedance` names the sources and the accuracy each states. A figure outside a formula's domain is refused with the reason, never extrapolated. An outer track is counted under the solder mask where the process states JLCPCB's calculator parameters (`.stackup.impedance_mask_thickness`, 1.2 mil, and `.stackup.impedance_mask_epsilon`, 3.8, from "Calculation Parameters Used" in its calculator's user guide, read 2026-11-16), with the copper thickness that calculator uses (`.stackup.impedance_outer_copper`, 1.6 mil); a process that states none, as ACME's does not, is counted without a mask, and the header line says which. An inner track between a prepreg and a core is an approximation of unstated error, and so is the mask's effect (JLCPCB's blog says a mask alone lowers a track by about 1 to 3 ohm, which the tests hold the figure to); a line says when a layer beside a track is not a plane. A two-layer process records no build-up, so its board has no lines. A net with an impedance target (below) is drawn at the width that gives it, and its lines say what that width gives. The lines are for a person who will order impedance control, to compare with the fabricator's own calculator. `data/jlcpcb/impedance_calculator.csv` has eight widths that JLCPCB's calculator gave on 2026-11-16 (single tracks and pairs, outer and inner layers, the four-layer 7628 and six-layer 3313 build-ups) and a test holds this calculator's impedance at each width within 5 % of the target for a single track and 8 % for a pair (it shows 4 % and 6 %; JLCPCB's own test passes a board within 10 %). The calculator's own method is not published, so the figures can differ by that much.

A board with a black mask:

```nsm solve
import std::parts::connector::Header1x2
import std::parts::passive::Color
import std::mfg::jlcpcb::TwoLayer

board Dark {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
        .stackup.mask_colour &= Color::Black
    }
    .header: Header1x2
}
```

Fiducials and test points are parts of `std::parts::mechanical` (`#[kind(fiducial)]`, `#[kind(test_point)]`), and the rules above judge them where the placer puts them. A fiducial is physical intent, so a board declares it in the `case { }` section of its physical file (`.fid1: Fiducial`, chapter 23), and the placer puts it in a corner of the outline with its copper `.min_fiducial_edge` from the edge, growing a board with no stated size to hold it. A test point is in the circuit. A board narrows a rule for margin, here the space of its test point to the board edge:

```nsm
import std::parts::mechanical::TestPoint
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::TwoLayer

board Marks {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
        .min_test_point_edge &= 4mm..
    }
    .supply: Rail {
        .pos.net &= "5V"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .tp: TestPoint
    .supply.pos ~> .r1 ~> .supply.neg
    .tp.p1 ~ .supply.pos
}
```

A four-layer board 1.2 mm thick:

```nsm solve
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::FourLayer

board Thin {
    .process: FourLayer {
        .stackup.thickness &= 1.2mm
        .inner1.net &= "GND"
        .inner2.net &= "5V"
    }
    .supply: Rail {
        .pos.net &= "5V"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .r1 ~> .supply.neg
}
```

A four-layer board in a build-up that is not the default of its thickness, three sheets of 7628 prepreg between each outer layer and its plane:

```nsm solve
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb_buildups::JlcFourLayer04161H7628C

board Sheets {
    .process: JlcFourLayer04161H7628C {
        .inner1.net &= "GND"
        .inner2.net &= "5V"
    }
    .supply: Rail {
        .pos.net &= "5V"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .r1 ~> .supply.neg
}
```

A board that wants a different finish says so:

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Gold {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
        .finish &= "ENIG"
    }
    .header: Header1x2
}
```

A six-layer board with planes outside and signal layers inside:

```nsm
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::SixLayer

board Six {
    .process: SixLayer {
        .inner1.net &= "GND"
        .inner2.use &= "signal"
        .inner3.use &= "signal"
        .inner4.net &= "5V"
    }
    .supply: Rail {
        .pos.net &= "5V"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .r1 ~> .supply.neg
}
```

### Blind and buried vias

`std::mfg::jlcpcb::HdiFourLayer`, `HdiSixLayer` and `HdiEightLayer` are JLCPCB's HDI service (issue #1015), whose vias need not go through the board. The numbers are from JLCPCB's two HDI pages, quoted in the header of `std/mfg/jlcpcb.nsm`. Each face has `.hdi_order` build-up layers, 1 to 3 (the pages' "1st-order" to "3rd-order"; a four-layer board has room for 1, a six-layer one for 2): a laser-drilled **blind via** joins each pair of neighbouring layers among the top `.hdi_order + 1` and the bottom `.hdi_order + 1`, through one dielectric layer. With `.buried_vias &= "core"` a mechanically drilled **buried via** joins the two inner layers the build-up layers end on, through the core; `"none"`, the default, has none. A through via joins every layer, as on any process.

- The router changes layer through the via of fewest layers that joins the two (the first of the order above on a tie): from `top` to `inner1` a blind via, from `top` to `inner2` on a first-order board a through via. A blind or buried via keeps clear of copper on its own layers only, and a plane it does not reach needs no room for it. A skip via ("L1-L3") is not drawn: the pages make one as stacked vias, and the router joins the same layers with blind vias side by side, which the pages say are easier to make.
- The blind via the router draws is `.laser_drill` in `.laser_via_diameter`, 0.1 mm in 0.25 mm, an offer of the process as `.via_drill` is. The buried and the through via are `.via_drill` and `.via_diameter`. The process holds them to its rules when the board is placed: the laser's hole of `.min_laser_drill` to `.max_laser_drill` (0.075 to 0.15 mm), `.min_hdi_via_ring` (0.075 mm) of copper round a blind or a buried via's hole, a dielectric under a blind via of `.min_laser_dielectric` to `.max_laser_dielectric` (0.05 to 0.127 mm) and no thicker than its hole times `.max_laser_aspect` (1:1), a core under a buried via of at least `.min_buried_core` (0.3 mm) and a buried hole of at most `.max_buried_drill` (0.55 mm), and any drilled hole no deeper than `.max_drill_aspect` (16) times itself.
- The HDI pages publish no build-up, so the board states its own: `.stackup.thickness` (at most 2.4 mm), `.copper_weight`, `.build` (a name of the board's), `.inner_copper` and every `.dielectricN`. The pages say the prepreg "requires DFM confirmation": the build-up is the board's to confirm with JLCPCB before an order.
- The board file writes a blind via between neighbouring layers as KiCad's `micro` via and a buried one as `blind`, each with the two layers it ends on, and the project file states the laser's least hole and copper (`min_microvia_drill`, `min_microvia_diameter`). The Gerbers draw a via's copper and mask opening on its own layers only, and the drill files are one for each span besides `-PTH.drl` and `-NPTH.drl`, named as KiCad names them: `-front-in1.drl`, `-in1-in4.drl`, `-in4-back.drl`. The routes file writes a blind or a buried via with its layers, `{ at = [3.0, 4.0], diameter = 0.25, drill = 0.1, layers = ["top", "inner1"] }`, and `netsmith routes import` reads KiCad's.
- netsmith's design rules check has two rules of its own for them, both errors: `via_span` (a blind or a buried via between layers the process does not join with one, on any process, or whose hole or copper is not one the process makes for it) and `via_aspect_ratio` (a via drilled through more dielectric than its hole allows). Two holes of vias that share no layer are not checked against each other.
- A blind via stands in a pad (issue #1071). The pages say JLCPCB fills and planarizes its blind vias by electroplating, which is every one it makes, so this needs no field of its own: on an HDI process the router may put the blind via from a pad's face to the layer under it in a surface-mount pad at least as wide as the via's copper (0.25 mm), whatever `.via_in_pad` says, which is about epoxy-filled drilled vias and still holds them. A line out of a fine-pitch pad walled in by its neighbours goes down through it to the layer under the pad, where that is a signal layer. `fab-notes.txt` says so in a line of its own, `blind vias in pads: N blind via(s) stand in pads (U1.9, ...)`, apart from the `vias in pads` line of the drilled ones, and the design rules check's `via_in_pad` does not count a micro via on an HDI process.
- A person chooses a via's span by hand: `netsmith routes add-via --net GND --at 3,4 --layers top,inner1` adds the blind via from `top` to `inner1`, of the process's size for it (the laser's 0.1 mm in 0.25 mm unless `--drill` and `--diameter` say otherwise), and the two layers may be named in either order. The span must be one the process makes, or the via is refused with the spans it does make; `top,bottom`, like no `--layers`, is the through via, written with no layers. In the editor the via tool's picker in the toolbar offers the through via and the process's blind and buried vias, and a track drawn across a layer change gets the via of fewest layers that joins the two layers, as the router's does.
- The 3D view (and the `.glb` and STEP files) draws a blind or a buried via's barrel and rings on its own layers only. The board's solid is not drilled for it: the process fills a blind via with plating and a buried one with resin.

A six-layer HDI board of the first order with buried vias, in a build-up of its own:

```nsm solve place file=crates/netsmith-tools/tests/fixtures/hdi_dense.nsm
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::HdiSixLayer

board Dense {
    .process: HdiSixLayer {
        .buried_vias &= "core"
        .stackup.thickness &= 1.2mm
        .stackup.copper_weight &= 1
        .stackup.build &= "HDI-6L-1.2"
        .stackup.inner_copper &= 0.0152mm
        .stackup.dielectric1.kind &= "prepreg"
        .stackup.dielectric1.glass &= "1080"
        .stackup.dielectric1.thickness &= 0.086mm
        .stackup.dielectric1.epsilon &= 3.91
        .stackup.dielectric2.kind &= "core"
        .stackup.dielectric2.thickness &= 0.4mm
        .stackup.dielectric2.epsilon &= 4.6
        .stackup.dielectric3.kind &= "prepreg"
        .stackup.dielectric3.glass &= "2116"
        .stackup.dielectric3.thickness &= 0.15mm
        .stackup.dielectric3.epsilon &= 4.16
        .stackup.dielectric4.kind &= "core"
        .stackup.dielectric4.thickness &= 0.4mm
        .stackup.dielectric4.epsilon &= 4.6
        .stackup.dielectric5.kind &= "prepreg"
        .stackup.dielectric5.glass &= "1080"
        .stackup.dielectric5.thickness &= 0.086mm
        .stackup.dielectric5.epsilon &= 3.91
        .inner1.use &= "signal"
        .inner2.net &= "GND"
        .inner3.net &= "5V"
        .inner4.use &= "signal"
    }
    .supply: Rail {
        .pos.net &= "5V"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .r1 ~> .supply.neg
}
```

A four-layer board has room for one build-up layer on each face:

```nsm fails
import std::mfg::jlcpcb::HdiFourLayer

board Deep {
    .process: HdiFourLayer {
        .hdi_order &= 2
    }
}
```

```text
error: contradiction on `process.hdi_order`: it is already 1, which does not overlap 2
  --> main.nsm:5:9
  |
5 |         .hdi_order &= 2
  |         ^^^^^^^^^^^^^^^
```

A board of eight or ten layers is made on ACME's process, `std::mfg::acme::EightLayer` (`.inner1` to `.inner6`) or `std::mfg::acme::TenLayer` (`.inner1` to `.inner8`). ACME is a fictional fabricator (ADR 0112). Its numbers were chosen, not sourced, and stand for no real fabricator's capability: the rules are JLCPCB's multilayer rows, and each build-up is a symmetric 1.6 mm stack of common prepregs and cores (`ACME-8L-1.6`, `ACME-10L-1.6`, listed in `std/mfg/acme.nsm`). The inner layers are planes or signal layers, as above. `fab` writes the package as for any process: a Gerber for each copper layer, the stackup in the board file, the 3D scene. `fab-notes.txt` says the process is fictional and the package is not for ordering, and `fab --order` refuses an order that names the board. See `examples/ten_layer.nsm`.

```nsm
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::acme::EightLayer

board Eight {
    .process: EightLayer {
        .inner1.net &= "GND"
        .inner2.use &= "signal"
        .inner3.use &= "signal"
        .inner4.use &= "signal"
        .inner5.use &= "signal"
        .inner6.net &= "5V"
    }
    .supply: Rail {
        .pos.net &= "5V"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .r1 ~> .supply.neg
}
```

`netsmith fab` on a board without one prints (this block is not run by the guide test):

```text
error: LedExample: `LedExample` does not say how it is made, and a PCB needs design rules: add `import std::mfg::jlcpcb::TwoLayer` and `.process: TwoLayer` to the board
```

Misuse.

```nsm fails
import std::mfg::jlcpcb::TwoLayer

board Thick {
    .process: TwoLayer {
        .stackup.thickness &= 3mm
    }
}
```

```text
error: contradiction on `process.stackup.thickness`: it is already 400um..2mm, which does not overlap 3mm
  --> main.nsm:5:9
  |
5 |         .stackup.thickness &= 3mm
  |         ^^^^^^^^^^^^^^^^^^^^^^^^^
```

```nsm fails
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::FourLayer

board Planes {
    .process: FourLayer {
        .stackup.thickness &= 1.6mm
        .inner1.net &= "VCC"
        .inner2.net &= "GND"
    }
    .supply: Rail {
        .pos.net &= "5V"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .r1 ~> .supply.neg
}
```

```text
error: Planes: `process.inner1` is a plane on the net `VCC`, but the board has no net of that name (its nets: `5V`, `GND`)
```

A signal layer carries the tracks of any net, so it names none:

```nsm fails
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::FourLayer

board Signal {
    .process: FourLayer {
        .inner1.net &= "GND"
        .inner2.use &= "signal"
        .inner2.net &= "5V"
    }
    .supply: Rail {
        .pos.net &= "5V"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .r1 ~> .supply.neg
}
```

```text
error: Signal: `process.inner2` is a signal layer and names the net `5V`: a signal layer carries the tracks of any net, so it names none; for a plane on `5V`, leave out `.use &= "signal"`
```

A finish the process does not offer is refused, with the offer listed. JLCPCB does not make HASL on six layers or more:

```nsm fails
import std::parts::passive::Resistor
import std::sources::Rail
import std::mfg::jlcpcb::SixLayer

board Hasl {
    .process: SixLayer {
        .finish &= "HASL lead-free"
        .inner1.net &= "GND"
        .inner2.use &= "signal"
        .inner3.use &= "signal"
        .inner4.net &= "5V"
    }
    .supply: Rail {
        .pos.net &= "5V"
    }
    .r1: Resistor { .resistance &= 1kohm +/- 1% }
    .supply.pos ~> .r1 ~> .supply.neg
}
```

```text
error: Hasl: `process.finish` is `HASL lead-free`, but a `SixLayer` board is made with one of: `ENIG`, `OSP`
```

### Impedance targets

A design states the impedance of the tracks of a net with a `TrackImpedance` of `std::sources` (issue #1007): one pin, `.line`, that `~` joins to the pin or net the track is on, and `.impedance`, the target from the track to its reference layer, one value. An edge-coupled differential pair is a `PairImpedance`: the pins `.p` and `.n` joined to the pair's two nets, `.impedance`, the target between the two tracks, and `.spacing`, the space between the tracks edge to edge, which the figure needs because the width that gives a differential impedance depends on it (the width of a pair is the width of each track). It is a field of a std interface, as a rail's `.min_track` is, so it adds no syntax and a module carries it out through its interface.

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::SixLayer
import std::sources::{PairImpedance, TrackImpedance}

board Antenna {
    .process: SixLayer {
        .inner1.net &= "GND"
        .inner2.use &= "signal"
        .inner3.use &= "signal"
        .inner4.net &= "3V3"
    }
    .rf: Header1x2
    .rf.p1.net &= "RF"
    .rf.p2.net &= "GND"
    .usb: Header1x2
    .usb.p1.net &= "USB_P"
    .usb.p2.net &= "USB_N"
    .power: Header1x2
    .power.p1.net &= "3V3"
    .power.p2.net &= "GND"
    /// The antenna line is a 50 ohm track.
    .rf_line: TrackImpedance {
        .impedance &= 50ohm
    }
    .rf_line.line ~ .rf.p1
    /// USB 2.0's data pair is 90 ohm between its tracks, which are 0.15 mm apart.
    .usb_pair: PairImpedance {
        .impedance &= 90ohm
        .spacing &= 0.15mm
    }
    .usb_pair.p ~ .usb.p1
    .usb_pair.n ~ .usb.p2
}
```

The router draws a net with a target at the width that gives it on each layer a track may be on (issue #1025), from the build-up, to the nearest 0.01 mm: on the board above `RF` is about 0.16 mm wide on the outer layers and 0.15 mm on the signal layers, whatever its rails or the process's least track say. The two nets of a `PairImpedance` are a diff pair to the router, whatever their names: each is drawn at the width that gives the pair its differential impedance at `.spacing`, keeps at least `.spacing` from its partner (and from any other copper, which nearer than the partner would couple to the track more strongly; it is the clearance of the pair's net class), and, where it runs beside its partner, the router's pass that keeps a pair together (_Diff pair_) counts it beside only within a grid cell (and the router's margin) of `.spacing`, so the gap it keeps is the spacing to at most about 0.12 mm more. A pair whose pads are farther apart than its spacing is pulled together to it (issue #1054): the pass may make a net of such a pair longer by what bends of 45 degrees from its pads to the spacing add, a bend there costs the search a fifth of what it costs elsewhere, and the net is routed again at the least room the router keeps, so it runs coupled for most of its length and parts only near its pads. It still needs no more vias, and a pair with no target is moved only where it gets no longer than 0.2 mm, as before. The notes say how much of each pair runs at a gap that gives its impedance. The project file gives each such net a class of its own, with the outer width as its track width and, for a pair, the spacing as its `diff_pair_gap`.

`fab-notes.txt` then has, for each target and each layer a track may be on, the width that gives it (from the build-up, as above) and the impedance of the width the router draws for the net there, and, for a routed pair, how much of it runs beside its partner at a gap whose differential impedance is within the tolerance (this block is not run by the guide test; `...` stands for the lines of the other layers, and on the last line for the figures of the routed board):

```text
track impedance target: net `RF` (rf_line), 50 ohm single-ended
  F.Cu: microstrip over In1.Cu (0.0994 mm of 3313 prepreg, er 4.1): 0.156 mm wide tracks give 50 ohm; the 0.16 mm tracks the router draws give 49.5 ohm (-1 %, within JLCPCB's test tolerance of 10 %)
  In2.Cu: stripline between In1.Cu (0.55 mm of core, er 4.6) and In3.Cu (0.1088 mm of 2116 prepreg, er 4.16): 0.153 mm wide tracks give 50 ohm; the 0.15 mm tracks the router draws give 50.6 ohm (+1 %, within JLCPCB's test tolerance of 10 %); In3.Cu is not a plane on this board, so the figures hold only where a plane or a pour is there
  ...
track impedance target: nets `USB_P` and `USB_N` (usb_pair), 90 ohm differential, the tracks 0.15 mm apart (edge to edge)
  F.Cu: microstrip over In1.Cu (0.0994 mm of 3313 prepreg, er 4.1): 0.156 mm wide tracks give 90 ohm; the 0.16 mm tracks the router draws give 88.9 ohm (-1 %, within JLCPCB's test tolerance of 10 %)
  In2.Cu: stripline between In1.Cu (0.55 mm of core, er 4.6) and In3.Cu (0.1088 mm of 2116 prepreg, er 4.16): 0.130 mm wide tracks give 90 ohm; the 0.13 mm tracks the router draws give 90.1 ohm (+0 %, within JLCPCB's test tolerance of 10 %); In3.Cu is not a plane on this board, so the figures hold only where a plane or a pour is there
  ...
track impedance pair: nets `USB_P` and `USB_N` (usb_pair): ... mm of the ... mm of `USB_P`'s track runs beside `USB_N` at a gap whose differential impedance is within JLCPCB's test tolerance (... %), at gaps of ... to ... mm edge to edge; the rest is where the pair parts at its ends, or the router could not keep it together
```

Each line says whether the drawn width is within 10 % of the target, which is how JLCPCB's impedance test passes a board (its blog: a 50 ohm target passes between 45 and 55 ohm, a 100 ohm differential pair between 90 and 110 ohm, read 2026-11-16). The design rules check holds every track of such a net to its width on its layer, away from the net's pads (`track_impedance`, a warning; within the neck-down reach of a pad a track may be narrower to leave it), and the route scorecard gives each diff pair's gap where it runs coupled.

What the board cannot give is a finding of the guideline `impedance target`, a `warning:` from `fab` and a line in `fab-notes.txt`, and an error with `guidelines = "error"` or `--strict`: a two-layer board, which records no build-up, so no width gives a target and the net is drawn at its own width; a layer where no width gives the target (outside a formula's domain), where the net is drawn at its own width; a layer where the width that gives it is narrower than the narrowest track the process makes or the least width the net's current needs, where it is drawn at that floor and the finding gives the impedance that has; a pair whose `.spacing` is under the process's clearance, which keeps its tracks apart by the clearance; and a net that two targets name, which the first, in the order of their paths, sizes.

A differential pair's figure uses the same approximations as a single track's and one more: the even and odd modes of Kirschning and Jansen (outer layers) and of Cohn (inner layers, checked against a numerical solution in the tests), with the solder mask's effect taken for each mode as for a single track and the strip thickness counted once in the width each mode sees; the doc comment of `netsmith_pcb::impedance` says what each source states and where this departs from it. A `.impedance` that is a range, a pair with no `.spacing`, a pin on no net, and a pair whose two pins are on one net are errors that say what to write. A two-layer board records no build-up, so its targets have no lines.
