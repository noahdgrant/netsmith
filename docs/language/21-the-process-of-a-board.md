## The process of a board

A board is made by one manufacturing process. Its design rules and stackup come from a definition in the standard library, and the board has it as a member named `process` (ADR 0008, ADR 0111).

```nsm solve
import std::parts::connector::Header1x2
import std::mfg::jlcpcb::TwoLayer

board Bare {
    .process: TwoLayer {
        .stackup.thickness &= 1.6mm
        .min_track &= 0.15mm..1m
    }
    .header: Header1x2
}
```

Rules.

- Each rule is the least the factory can make, written as a range. `.min_track &= 0.1mm..1m` says the least track is at least 0.1 mm. A board narrows it upward for margin. The rule used is the bottom of what is left.
- Ranges are closed, so `1m` stands for no upper limit.
- A process states every rule its fabricator's page gives, or says in its doc comment why it does not (issue #431). The router keeps each rule and netsmith's design rules check checks it: the copper rules a KiCad project file can hold go there (`min_clearance`, `min_track_width`, `min_hole_to_hole`...), and the others are netsmith's own rules in the check: `hole_to_track` (a via's or a non-plated hole to a track, 0.2 mm, and a plated pad's hole, `.min_plated_hole_clearance`, 0.28 mm), `pad_to_pad` (surface-mount pads of different nets, 0.15 mm), `pad_hole_to_hole` (two holes when one is a pad's, 0.45 mm; two vias' holes are 0.2 mm, `.min_via_hole_to_hole`), `mask_to_track` (a solder mask opening to a track of another net, 0.09 mm), `slot_width`, `npth_drill` and `plated_ring` (the copper round a plated pad's hole, `.min_annular_ring`; a via's is `.min_via_ring`).
- The via the router draws is `.via_drill` and `.via_diameter`, 0.2 mm in 0.45 mm: an offer of the process (`?=`), the smallest via JLCPCB makes at its standard price. A board replaces it with `.process.via_drill &= 0.3mm` and `.process.via_diameter &= 0.6mm`; a via the process cannot make is refused when the board is placed.
- A board must choose `.stackup.thickness`. The process offers 0.4 mm to 2 mm, and netsmith will not pick.
- A two-layer board may choose its copper weight: `.stackup.copper_weight &= 2` is 2 oz, and the rules that depend on the weight follow (track and space 0.16 mm, annular ring 0.254 mm, mask bridge 0.2 mm), with the current a track and a via carry (see Track width against current). `TwoLayer` is a generic and each weight a provider (see _An interface that provides a generic: a process_); a board that says nothing is made with 1 oz. JLCPCB's page offers 2.5, 3.5 and 4.5 oz too but gives no annular ring or mask bridge for them, so std has no provider for them and a board that asks for one is refused. `FourLayer` and `SixLayer` are 1 oz only: JLCPCB lists no build-up with 2 oz outer copper.
- A board with no `process` builds and views as a schematic. `fab` stops and says to add `import std::mfg::jlcpcb::TwoLayer` and `.process: TwoLayer`. There is no default.
- `.max_temperature_rise` is the rise in kelvin that the track width check designs to (see Track width against current). The process leaves it unstated and it is 10 K when the board does too: `.process.max_temperature_rise &= 20K`.
- `std::mfg::jlcpcb::FourLayer` has two inner layers and `std::mfg::jlcpcb::SixLayer` four (`.inner1` to `.inner4`, counted from the top). The board says what each is (ADR 0111). A plane is one sheet of copper on a net, and the board names the net: `.process.inner1.net &= "GND"`. A signal layer names no net: `.process.inner2.use &= "signal"`. The router draws on the outer layers only, so a signal layer stays empty for now; it is written to the board file as a signal layer, plotted as an inner copper Gerber and checked like the outer layers. An inner layer that is neither is an error, and so is a plane on a net the board does not have. See `examples/four_layer.nsm` and `examples/six_layer.nsm`. Each offers 1.6 mm only, the one thickness whose build-up JLCPCB's impedance page gives (`JLC04161H-7628`, `JLC06161H-3313`); `fab-notes.txt` names the build-up and says what each inner layer is. JLCPCB's pages give no build-up of 8 or 10 layers, so std has no JLCPCB process for them; see the fictional fabricator below.

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
