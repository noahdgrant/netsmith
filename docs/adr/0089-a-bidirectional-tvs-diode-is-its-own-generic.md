---
status: provisional, awaiting the owner's review (the smallest reading of issue 63's "a bidirectional TVS (`Tvs` is drawn with the Zener symbol)" is built, by an agent, with the alternatives it did not take under "Choices for the owner")
---

# A bidirectional TVS diode is its own generic

## The problem

`std::parts::diode::Tvs` is a unidirectional TVS diode: a cathode to the protected line, an anode to ground, a clamp that conducts one way and a forward drop the other. It is drawn with `Device:D_Zener`. A line that swings both ways, or a power input that may be wired backwards (the input of a vehicle board), wants a bidirectional part, which is two junctions back to back, is drawn with two opposed triangles, and has no anode and cathode. The standard library had none, so a board used `Tvs` and drew it wrong, or kept a project part of its own.

## Decision

`std::parts::diode::TvsBidirectional` is a `generic`, beside `Tvs` and with the same fields and checks, read for either direction:

- Pins `.a1 passive "A1" "1"` and `.a2 passive "A2" "2"`: the pin names and pads of KiCad 9's `Device:D_TVS`, whose description reads "Bidirectional transient-voltage-suppression diode" (found in the installed `Device.kicad_sym`, with pins `A1` and `A2`, passive). The pads follow the two-terminal diode footprints that `Tvs` uses (`Diode_SMD:D_SOD-123`, `D_SOD-323`, `D_SMA`, `D_SMB`, `D_SMC`, pads 1 and 2).
- Fields `.standoff_voltage`, `.clamping_voltage`, `.peak_pulse_current`, `.breakdown_voltage` (an optional catalog column as for `Tvs`, ADR 0073 and 0078), `.protected_limit` and `.bias`. Relations `.bias <= .standoff_voltage` and `.clamping_voltage <= .protected_limit`. Bridge `.a1 ~> .a2`.
- `.bias` is the rail's voltage whichever pin is on `pos`: the part is symmetric, so the checker gives it the voltage as it does for a capacitor (ADR 0013), and a part wired either way round is the same. There is no wrong way to wire it, and `Tvs`'s check that a clamp is not forward biased (ADR 0088) does not apply.
- `std/kicad.toml` maps it for the five packages that `Tvs` has, to `Device:D_TVS`.
- A catalog `tvs_bidirectional` with two rows, each read from the Littelfuse SMDJ series datasheet table (revised 11/20/15), which lists the bidirectional part number beside the unidirectional one of its line: the SMDJ12CA (C224064) and the SMDJ36CA (C224075), both with the minimum breakdown voltage. `data/jlcpcb/README.md` records the sources, and a test pins every number.
- The designator letter is `D`.

The language is unchanged: it is std source, one catalog file and two mapping rows.

## Choices for the owner

- **A member on `Tvs`** (`.bidirectional`, or an enum `Direction`) instead of a second generic. The smallest std surface, and one name for a TVS diode. It costs more than it saves. `std/kicad.toml` maps a kind and a package to a symbol, so a member of the part cannot choose the symbol: the mapping would need a condition on a value, which is a change to the mapping's language. `Tvs`'s pins are named `anode` and `cathode`, which mislead for a part that has neither, and the pin table cannot differ by a member. One catalog would hold rows of both kinds, and a design that did not say which would be offered both. Not taken.
- **The name.** `TvsBidirectional` states the difference and sorts beside `Tvs`. `BidirectionalTvs`, `TvsBi` and `TvsDual` (which KiCad uses for three-pin parts) were passed over. Renaming `Tvs` to `TvsUnidirectional` is the symmetric choice, and breaks every board that uses `Tvs`: not taken.
- **The pins' names.** `a1` and `a2` follow the symbol and the datasheets of the parts (`A1`, `A2`, or `1`, `2`); a neutral `a` and `b` would match `Inductor` and `Fuse` and need `pin_names` in the mapping. The names are the symbol's.
- **The bias of a part with no rail.** For a bidirectional diode across no rail a design states `.bias` as the largest voltage the line can reach in either direction, a magnitude. A design that wants the two directions apart (a stand-off that differs by polarity: an asymmetric bidirectional part, which the datasheets also sell) has no field for it here.
- **Arrays and dual-line parts.** A three-pin TVS (two anodes to a common cathode, `D_TVS_Dual_*` in KiCad) and multi-line ESD arrays are not covered; each is its own part.
- **More rows.** Two rows were read; the unidirectional `tvs.csv` has ten. Rows for the MDD SMAJ and SMBJ bidirectional types need the same reading of their datasheet and are left for whoever needs them (`netsmith-dev-add-part`).

## Cost

One more generic that a design must pick by name: a board that wants a bidirectional clamp writes `TvsBidirectional`, and a board that kept `Tvs` has the Zener symbol as before. The solver counts the generic in the same way as the others (the catalog is offered the design's own values and rows that state the column it names). The Littelfuse rows are Extended parts; no Basic bidirectional TVS row was read.

## Left open

- The forward and reverse stand-off of an asymmetric bidirectional part, and arrays (above).
- The kicad row for the part is checked against the installed KiCad libraries by `crates/netsmith/tests/kicad_rows.rs`; a CI job without KiCad skips it.
