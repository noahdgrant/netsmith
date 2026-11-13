---
status: accepted
---

# Boards of eight and ten layers are made by a fictional fabricator, and cannot be ordered

The showcase boards (#467 to #470) need eight and ten copper layers. JLCPCB publishes no build-up of 8 or 10 layers (ADR 0111), and std states a fabricator's number only with its source. The owner decided (2026-11-13) not to give JLCPCB a made-up build-up, and to add a fabricator that does not exist instead: ACME, `std::mfg::acme`, with the processes `EightLayer` and `TenLayer`.

ACME's numbers are chosen, not sourced, and its file says so in its header and on each process. The rules are JLCPCB's multilayer rows, so a board moved between the two keeps its rules. Each build-up is a symmetric 1.6 mm stack of common prepreg and core thicknesses with typical FR-4 permittivities (`ACME-8L-1.6`, 1.6064 mm; `ACME-10L-1.6`, 1.6168 mm). The processes have the fields of JLCPCB's, with an `InnerLayer` for each inner layer, so nothing downstream knows ACME apart: the router's rules, the KiCad stackup, the Gerbers, the design rules check and the 3D scene take the build-up as they take JLCPCB's.

A board on ACME is designed, routed, checked and written as a fab package, and cannot be ordered. `netsmith_core::FICTIONAL_FABRICATORS` names ACME, and `Process::is_fictional` says whether a process is one of them. `fab-notes.txt` says on its first lines that the fabricator does not exist and the package is not for ordering, and the process line ends "a fictional process: this package is not for ordering" in place of "order it with these". `fab --order` (and the editor's order, the same function) refuses an order that names such a board, or a panel of one, before anything is built, and says why. Nothing in netsmith sends a board to a fabricator or prices one, so these are the places an order happens.

Rejected: an 8- and a 10-layer JLCPCB process with a guessed build-up. A JLCPCB process reads as JLCPCB's capability, and a guessed number in it would go unnoticed into an order. Rejected: a `.fictional` field on the process, in the language. The language has no boolean, a word field would be one more thing every process states, and only std defines a fabricator; one list in Rust says the same. Rejected: refusing `fab` itself for an ACME board. The package is what the showcase ports are checked by.

The cost: std holds numbers with no source, against the rule for data (`data/jlcpcb/README.md`), and one name in Rust has to match `.fabricator` in std. A process of a new fictional fabricator needs its name added to the list.

Left open: ACME processes of other thicknesses, copper weights or layer counts; real 8- and 10-layer processes once a fabricator publishes their build-ups, which would make ACME's unnecessary.
