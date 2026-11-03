---
status: accepted
---

# Compare designs by connectivity

A design ported to netsmith has other designators and other net names than the KiCad design it came from: netsmith numbers parts in its own order and names a net after the pins it joins. Comparing the two netlists by name would report every line as different. `netsmith compare` therefore compares the circuit: a graph of components and nets joined by numbered pins, in which a component carries only its value and footprint (the footprint without its library, so `Resistor_SMD:R_0402_1005Metric` and `R_0402_1005Metric` are the same).

Two such graphs are the same circuit when some pairing of their components and nets keeps every pin on corresponding nets. The comparison refines a colouring of the nodes of both graphs together, each node taking a colour from its own and its neighbours' until nothing splits, and the graphs differ if any colour has a different number of nodes on the two sides. Parts that are alike and wired alike (two equal resistors in parallel, the rows of an LED array) leave a colour with several nodes on each side; the comparison then gives one node of A and a candidate of B a colour of their own, refines again, and tries the next candidate if that fails. The search is bounded (20 000 refinements) and ends as "could not decide" with the group it was stuck on, because a wrong "match" or "different" would be worse than none.

When the graphs differ, a second pass pairs what it can (a component alone of its colour on each side, then the components whose pins are on nets already paired, then a guess among alike parts, preferring equal designators) and reports what is left: components with no counterpart, counterparts that differ in value or footprint, and nets whose pins differ, each pin named in its own side's designators with where its counterpart is in the other. The report is a best explanation, not a proof: among interchangeable parts the pairing it shows is one of several.

The options say how much to trust names. `--by-refdes` and `--map` fix designators as identity, wholly or in part; `--ignore-unconnected` leaves out nets of one pin; `--loose-values` reads values as numbers. Without them nothing is assumed beyond value, footprint and wiring. Pin numbers are always identity, so a resistor with its pins swapped differs, as it does in KiCad's own netlist.

The code is the crate `netsmith-netlist`, which reads KiCad's netlist XML with `roxmltree` and a `.kicad_pcb` with the S-expression reader of `netsmith-fab`; a `.kicad_sch` goes through `kicad-cli` (`KicadCli::netlist`) and a `.nsm` file through the normal build.
