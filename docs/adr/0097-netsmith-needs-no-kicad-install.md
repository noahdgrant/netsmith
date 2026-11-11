---
status: proposed
---

# netsmith needs no KiCad install

Replaces ADR 0021 where it requires the installed KiCad libraries for `build` and `fab`. It keeps what ADR 0021 built: the loader for `.kicad_sym` files, the mapping (`std/kicad.toml`, a project's `kicad.toml`), project libraries and the pinout validation. It continues ADR 0041 (its user decision of 2026-11-02, a person with the `netsmith` binary alone) and ADR 0043 (generated footprints) and ADR 0087 (`std/libs/`), and decides what they left open: the symbols, and the footprints no generator makes. Issue #5 tracks the work. It meets two of the three tests: it is hard to reverse (projects' mappings, the exported files and the CI jobs depend on where libraries come from), and it rejects obvious alternatives (the three below).

**Decision.** netsmith carries its own symbols and footprints and needs no KiCad install to write, check, solve, see, build or fab a board. The `.kicad_sch` and `.kicad_pcb` files are still written, in KiCad's formats. Only the need for an installed KiCad goes. Today `build` and `fab` stop when `crates/netsmith/src/libraries.rs` finds no installed library (`netsmith_kicad::install`), so a machine without KiCad cannot build a board whose parts map to a symbol of KiCad's.

The measure: `std/kicad.toml` has 81 `[[part]]` rows. They use 36 distinct symbol ids, 35 of them KiCad's and `netsmith:LT8641`, and 57 distinct footprint ids, 56 of them KiCad's and one `netsmith:` id. The 35 symbols are about a dozen glyphs: the resistor, capacitor, inductor, LED, four diodes, the N and P FET, the fuse, the crystal, the switch and the ferrite bead, plus the connector, USB-C, coaxial, test point, mounting hole and fiducial symbols, the AP1117 regulator and two `74xGxx` gates. The generators of `crates/netsmith-pcb/src/generate/` make the chip, crystal, diode, header, SOT, gullwing, QFN, QFP and BGA footprints. By name, 15 of the 56 KiCad footprints have no generator: the three USB-C receptacles, the SMA coaxial connector, the two tactile switches, the four mounting holes, the three test points, the fiducial and the Tag-Connect TC2030. Each step below is a pull request of its own, in this order:

1. Symbols of ICs are drawn from the part's pin table, one box per unit, as ADR 0050 draws a part today and as the editor already draws a part with no symbol. A part then needs no library symbol.
2. The symbols of the passives, diodes, FETs, fuse, crystal, switch and ferrite bead are generated, like the footprints.
3. The 15 footprints above are generated or, where the package is one maker's drawing, written into `std/libs/` as ADR 0087 says.
4. netsmith's own symbols and footprints become the default source. `std/kicad.toml` names ids that netsmith answers, as it already answers `netsmith:` ids "from the binary before anything else is read" (ADR 0087).

KiCad stays in two roles. It is an optional cross-check: `kicad-cli` for ERC and DRC (`crates/netsmith/tests/erc.rs`, `scripts/check-kicad.sh`) and the installed libraries for the tests that compare. And it is an optional source: when a project's `kicad.toml` names a `[[library]]` of KiCad files, or a KiCad id that netsmith does not make, that library is read as ADR 0021 describes, installed or the project's own. The lookup of installed libraries (`install.rs`) stays for that case and stops being a default.

**Rejected: bundling KiCad's libraries in the binary.** They are CC-BY-SA 4.0 with an exception for designs and the files a tool writes. Redistribution of library files, a subset or a flattened symbol is outside the exception, and ADR 0041 (section 2) could not settle whether a binary that embeds them is a redistribution. The README says netsmith bundles none. Size is the smaller reason: the stock symbol files are megabytes for a few kilobytes of use. Writing symbols and footprints ourselves, from package drawings and IPC-7351B and not from KiCad's generator code (GPL), avoids both.

**Rejected: requiring KiCad as today.** Every user then installs a large program they may never open, for files the tool could draw. It also ties the output to what a KiCad patch release does: ADR 0041 measured that installed `Device.kicad_sym` differs from upstream 9.0.9.1 in 644 lines, so the same design builds differently on two machines. The lookup is unverified on Linux, macOS, snap and flatpak (issue #6).

**Rejected: fetching the libraries at run time.** ADR 0041 offered a `libs fetch` into a cache. A build that needs the network and a remote repository is not reproducible, since the repository moves between releases, and it fails offline. It also leaves netsmith needing a program other than itself, which is the dependency this ADR removes.

**Cost.** netsmith owns a drawing of every symbol and footprint it makes, and must keep each one right. A wrong pad is a wrong board, so each generated footprint is proved against KiCad's own to 0.01 mm (`crates/netsmith-pcb/tests/generated_vs_kicad.rs`), and each generated symbol needs a test of the same kind: same pins, same electrical types, same pin positions. Our symbols will not look like KiCad's own, so a board opened in KiCad shows different glyphs than before, and a design ported from KiCad looks different on the sheet. A project that maps a part to a KiCad id netsmith does not make still needs KiCad's libraries, and `build` says which, as it does today. The mapping stays the one place KiCad's names appear.

**Left open.**

- The look of the generated symbols: whether the passives match KiCad's shapes closely or follow another standard, and the pin names and numbers it shows. This is for the owner.
- How a project's own part gets a footprint with no KiCad: ADR 0104 (accepted) makes it a generator name and its parameters in `kicad.toml` (`gen:`), or a `.kicad_mod` file listed as a `[[library]]`.
- How `generated_vs_kicad` keeps running in CI: it skips where KiCad's libraries are absent, so the `kicad` job (KiCad 9 with `kicad-symbols` and `kicad-footprints`) must stay, run it, and fail where it now skips, so that a missing install is not a pass.
- Whether KiCad 8 is supported for the cross-check. It is untried today.
