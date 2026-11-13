---
status: accepted
---

# An inner layer is a plane or a signal layer, and a build-up is listed layer by layer

This supersedes ADR 0012, which made every inner layer a plane and described one build of four layers. What ADR 0012 decided about planes still holds and is restated here: the board names a plane's net on the process (`.process.inner1.net &= "GND"`, ADR 0005), lowering refuses a plane with no net or on a net the board does not have, a plane is a KiCad zone over the whole board with the process's inner-layer hole space (0.3 mm) and KiCad's thermal reliefs, a through-hole pad or a via of its net reaches it where it stands and any other pad gets a via, and the board file holds the zones unfilled.

Each inner layer of a process is an `InnerLayer` (`std::mfg::stackup`), the field `.innerN` counted from the top, with two fields: `.use`, `"plane"` (offered with `?=`) or `"signal"`, and `.net`. A plane names its net as before; a signal layer is said with `.process.inner2.use &= "signal"` and names none. The language has no boolean, so a word stands for the choice, as on a `Keepout`. Lowering refuses a signal layer that names a net (the message says to leave out the `.use` line for a plane) and a `.use` that is neither word. A board that says nothing about an inner layer is refused as before, and the message now names both spellings. The interface was `Plane`; it is renamed, because a signal layer is not one. Only std named it.

A signal layer is written to the `.kicad_pcb` layer table as `signal`, and a plane as `power`, KiCad's type for a plane layer. It is plotted as an inner copper Gerber and checked by netsmith's design rules check and KiCad's like the outer layers. The router is not changed: it draws on the outer layers only, so a signal layer holds only the copper of through-hole pads until the router routes on it (#431). `CopperLayer` stays the two outer layers. `fab-notes.txt` says what each inner layer is and names the build-up.

A build-up is listed layer by layer. `Stackup` has `.build` (the fabricator's name, `JLC06161H-3313`), `.inner_copper`, and `.dielectric1` to `.dielectric9`, each a `DielectricLayer` with `.kind` (`"prepreg"` or `"core"`), `.glass` (the prepreg's glass style) and its `.thickness` and `.epsilon`, from the top: a board of n layers states n - 1. The fields `.prepreg`, `.core`, `.prepreg_epsilon` and `.core_epsilon` are gone, because a six-layer build mixes glass styles (3313 outside, 2116 in the middle) and has two cores. Nine are declared, for ten layers; more layers need more fields. The interface is not called `Dielectric`, which is a capacitor's class.

`SixLayer` is JLCPCB's six-layer process, every number from the capability page and the impedance page (fetched 2026-11-12) in its doc comment. JLCPCB does not make HASL on six layers or more, so the board file's finish is ENIG there. Only 1.6 mm is offered: the impedance page's text has the 1.6 mm tables only. That page has no build-up of 8 or 10 layers, and no other JLCPCB page found does, so std has no process for them; each would be one more definition once a page gives its build-up.

Rejected: a `Signal` interface beside `Plane`, chosen by the type of `.innerN`. A process fixes the types of its fields, so the board could not choose. Rejected: a plane with no net counted as a signal layer. A forgotten net would become an empty layer with no error. Rejected: KiCad's `mixed` type for a signal layer that may later carry a pour. Nothing pours on a signal layer yet.

The cost: `FourLayer` was rewritten in the new build-up fields. A board needs no change: `.innerN.net` reads as before.

Left open: tracks on inner layers (the router, #431), blind and buried vias, a split plane, other thicknesses and copper weights, 8- and 10-layer processes until JLCPCB publishes their build-ups, and a `.finish` field.
