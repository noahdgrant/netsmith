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
- A process states the rules for fiducials and test points too (issue #761), from JLCPCB's help article on edge rails and fiducials, its fiducial blog and its test point blog, each read on 2026-11-16 (the pages and their wording are in the header of `std/mfg/jlcpcb.nsm`). Each is a limit or a guideline by the page's own words, as the owner decided on 2026-11-15: what the page says JLCPCB cannot make or place is a **limit**, an error of the design rules check that stops `fab`, and advice, or wording that is not clear, is a **guideline**, a warning (`design rules warnings` in `fab-notes.txt`, and in the editor's problems list). The limits, on every `Fiducial` of the board: `fiducial_edge` (its copper at least `.min_fiducial_edge`, 3.35 mm, from the board edge), `fiducial_mask` (its copper is exposed, no solder mask over it) and `fiducial_keepout` (no other copper, via or pad inside its solder mask opening, its keep-out; a pour is not checked, because its fill is not computed). The guidelines: `fiducial_count` (each side that has parts JLCPCB assembles has at least `.min_fiducials`, 2, fiducials: the page's "a minimum of two, but three are strongly recommended"; the help article says JLCPCB adds edge rails with fiducials to an assembled board, so a board with fewer is not refused), `fiducial_size` (copper of `.fiducial_diameter`, 1 mm, an offer of the process like the via, and a mask opening at least `.fiducial_mask_ratio`, 2, times the copper), and for every `TestPoint` the "absolute minimum" of the page's tables, advice for a probe fixture: `test_point_pad` (the pad at least `.min_test_point_pad`, 0.8 mm), `test_point_pitch` (two on a side at least `.min_test_point_pitch`, 1.27 mm, centre to centre), `test_point_body` (copper at least `.min_test_point_body`, 0.5 mm, from the body of a part on its side) and `test_point_edge` (at least `.min_test_point_edge`, 3.2 mm, from the board edge). The page's recommended values are named in the finding and not enforced. A board narrows each upward as any rule (`.process.min_test_point_edge &= 4mm..`); the design file carries them, and a limit is a lower bound like the others. Not checked yet: the larger pad of a power or ground test point (1.0 mm), the larger clearance to a part taller than 6.4 mm (2.0 mm), and a board that needs fiducials the panel's rails do not give.
- The via the router draws is `.via_drill` and `.via_diameter`, 0.2 mm in 0.45 mm: an offer of the process (`?=`), the smallest via JLCPCB makes at its standard price. A board replaces it with `.process.via_drill &= 0.3mm` and `.process.via_diameter &= 0.6mm`; a via the process cannot make is refused when the board is placed.
- The surface finish is `.finish`, a word the process offers (`?=`): `HASL lead-free` on `TwoLayer` and `FourLayer`, `ENIG` on `SixLayer`. `.finishes` lists what the process offers, which is JLCPCB's "Surface Finish" row: `HASL leaded`, `HASL lead-free`, `ENIG` and `OSP`, and `ENIG` and `OSP` from six layers, where the page says FR-4 boards do not support HASL. A board that wants another says it, `.process.finish &= "ENIG"`, and a word that the process does not offer is refused, listing the offer. `fab` writes the finish to the board file (`copper_finish`), to `fab-notes.txt` (the `process:` line) and to `order.txt` (`finish ENIG`), and the order form has to match it. The default is netsmith's choice, not a number of the page, which names none; the page also says boards of 0.4 mm or less do not support HASL, which the check does not refuse yet, so a board that thin states its finish.
- A board must choose `.stackup.thickness`. The process offers 0.4 mm to 2 mm, and netsmith will not pick.
- A two-layer board may choose its copper weight: `.stackup.copper_weight &= 2` is 2 oz, and the rules that depend on the weight follow (track and space 0.16 mm, annular ring 0.254 mm, mask bridge 0.2 mm), with the current a track and a via carry (see Track width against current). `TwoLayer` is a generic and each weight a provider (see _An interface that provides a generic: a process_); a board that says nothing is made with 1 oz. JLCPCB's page offers 2.5, 3.5 and 4.5 oz too but gives no annular ring or mask bridge for them, so std has no provider for them and a board that asks for one is refused. `FourLayer` and `SixLayer` are 1 oz only: JLCPCB lists no build-up with 2 oz outer copper.
- A board with no `process` builds and views as a schematic. `fab` stops and says to add `import std::mfg::jlcpcb::TwoLayer` and `.process: TwoLayer`. There is no default.
- `.max_temperature_rise` is the rise in kelvin that the track width check designs to (see Track width against current). The process leaves it unstated and it is 10 K when the board does too: `.process.max_temperature_rise &= 20K`.
- `std::mfg::jlcpcb::FourLayer` has two inner layers and `std::mfg::jlcpcb::SixLayer` four (`.inner1` to `.inner4`, counted from the top). The board says what each is (ADR 0111). A plane is one sheet of copper on a net, and the board names the net: `.process.inner1.net &= "GND"`. A signal layer names no net: `.process.inner2.use &= "signal"`. The router draws on the outer layers only, so a signal layer stays empty for now; it is written to the board file as a signal layer, plotted as an inner copper Gerber and checked like the outer layers. An inner layer that is neither is an error, and so is a plane on a net the board does not have. See `examples/four_layer.nsm` and `examples/six_layer.nsm`. A board that says nothing is made 1.6 mm thick, in the build-up `JLC04161H-7628` (four layers) or `JLC06161H-3313` (six); `fab-notes.txt` names the build-up and says what each inner layer is. Both are generics, as `TwoLayer` is, with a provider for the page's default build-up of each of 0.8, 1.0, 1.2, 1.6 and 2.0 mm (`JlcFourLayerD04121H7628` is `JLC04121H-7628`: a letter that orders the providers, then the build-up's name, see the doc comment of `FourLayer`): `.process.stackup.thickness &= 1.2mm` makes the board 1.2 mm thick, in that build-up, and `.process.stackup.build &= "JLC04121H-7628"` asks for a build-up by its name. The page lists many more build-ups (other prepregs, several sheets between two copper layers, a bare core between sheets); they have no provider, so a board that asks for one is told that no provider has it (the doc comments of `FourLayer` and `SixLayer` in `std/mfg/jlcpcb.nsm` say which are missing). JLCPCB's pages give no build-up of 8 or 10 layers, so std has no JLCPCB process for them; see the fictional fabricator below.

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
