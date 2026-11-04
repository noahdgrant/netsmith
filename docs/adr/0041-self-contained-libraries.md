---
status: accepted (user decisions 2026-11-02)
---

# Self-contained libraries: symbols, footprints and the schematic netlist without KiCad

**User decision (2026-11-02):** netsmith generates footprints itself (original generators for common packages, IPC-7351 style, checked against KiCad's own footprints to 0.01 mm) rather than depending on KiCad's library bytes. KiCad's `.kicad_sym`/`.kicad_mod` stay the file format. Packages without a generator still need a library file: until the owner decides otherwise, those come from a project's own libraries or a `libs fetch` from KiCad's repository into a cache (nothing is bundled in the binary). Symbols: simple generated symbols for the standard kinds; the editor draws symbols from its own data.

ADR 0021 ("Direction") says a person with the `netsmith` binary alone must be able to write, check, solve, see and fabricate a design. ADR 0034 (fabrication files) and ADR 0037 (design rules check) removed `kicad-cli` from that path. Two dependencies are left, and this ADR designs their removal:

1. **The symbol and footprint libraries** (`.kicad_sym`, `.kicad_mod`), which are read at run time from the installed KiCad 9 (found as ADR 0021 describes: `NETSMITH_KICAD_SYMBOLS` / `NETSMITH_KICAD_FOOTPRINTS`, then beside `kicad-cli`, then the usual install places, among them `/mnt/c/Program Files/KiCad/9.0/share/kicad/...`), and from a project's own libraries (ADR 0021).
2. **`netsmith compare` on a `.kicad_sch`**, which asks `kicad-cli sch export netlist` for the connectivity.

The `.kicad_sch` and `.kicad_pcb` that netsmith writes are an export, and stay (ADR 0003, ADR 0021). This ADR changes no code. It records measurements, the licence facts as far as they could be read, the options with their costs, a recommendation, a staged plan, and the questions only the owner can answer. Nothing here is legal advice.

## 1. Inventory (measured 2026-11-02)

Measured against the installed KiCad 9.0 libraries (symbols 233 MB in 225 files; footprints 213 MB in 146 `.pretty` folders; 3D models 4.8 GB) and against the upstream tag `9.0.9.1`, whose `tar.gz` archives are 10.8 MB (symbols) and 12.0 MB (footprints).

**What `std/kicad.toml` uses.** 67 rows, 32 distinct symbols from 7 symbol libraries, 50 distinct footprints from 17 footprint libraries.

| | What is used | Size of the libraries they come from | Size of what is used |
|---|---|---|---|
| Symbols | 32 (`Device` 10, `Connector` 8, `Connector_Generic` 7, `Mechanical` 3, `Transistor_FET` 2, `Regulator_Linear` 1, `Switch` 1) | 18.8 MB in 7 files (`Connector_Generic` 7.2, `Connector` 5.2, `Regulator_Linear` 2.5, `Device` 2.4, `Transistor_FET` 1.2, `Switch` 0.4, `Mechanical` 0.03) | 109 kB of text, a parent included when a symbol `extends` one |
| Footprints | 50 | 25.1 MB in 17 `.pretty` folders (the two pin-header libraries alone are 15.9 MB, because they hold every size) | 213 kB in 50 files, the largest 20 kB |

Two things are used besides the rows: `power:PWR_FLAG` (the power-flag rule of `crates/netsmith-kicad/src/mapping.rs`, a symbol in `power.kicad_sym`), and the 3D model path inside each footprint (`${KICAD9_3DMODEL_DIR}/...`), which the exported `.kicad_pcb` keeps verbatim. netsmith never opens a 3D model; it only writes the path, so KiCad finds it when it opens the exported board. 4.8 GB of models need not be fetched.

**What the examples need.** Building the six examples (`examples/*.nsm`) writes 8 distinct symbols (`Device:R`, `Device:C`, `Device:LED`, `Connector_Generic:Conn_01x02`, `Regulator_Linear:AP1117-15`, `Mechanical:Fiducial`, `Mechanical:MountingHole`, `power:PWR_FLAG`) and 9 distinct footprints (`R_0402`, `C_0402`, `C_0603`, `LED_0603`, `SOT-23`, `SOT-223-3_TabPin2`, `PinHeader_1x02_P2.54mm_Vertical`, `Fiducial_1mm_Mask2mm`, `MountingHole_3.2mm_M3`).

**What a typical four-layer product needs beyond the standard rows.** This is an estimate, not a measurement; there is no representative design in the repo, and the owner's own designs must not be measured here. The proxy that was measured is KiCad's 48 demo schematics (`share/kicad/demos`), 14 projects, which are older and simpler than a modern product:

- Stock footprints per project: 0 to 26 distinct, median about 12 (the union over all demos is 123 stock footprints, 1.0 MB). The footprints that a product needs are a long tail: chip passives, SOT, SOIC, TSSOP, QFN/DFN, LQFP, a connector or two.
- Stock symbols: the demos are a poor proxy, because 336 of their 382 distinct symbols are project-local (the schematic embeds the symbol, and the library is the project's own). That is itself the finding: **a real design's ICs are mostly not in the stock libraries under the name or pinout the design wants**, and ADR 0021 already treats those as project libraries. The stock part of a product is the passives, diodes, LEDs, transistors, connectors and a few standard ICs.
- Expected size of the gap: 25 to 60 distinct footprints and 15 to 40 distinct symbols per product, of which 5 to 15 are ICs (microcontroller, regulators, memory, a USB or RF part) that come from a manufacturer's or LCSC/EasyEDA's download or are drawn by hand into a project library. The std rows cover the first group; the second group is outside any stock-library plan.
- Whole stock set for reference: `tar.gz` 22.8 MB, about 446 MB unpacked. Nobody needs all of it, and bundling all of it into a binary is not on the table.

**What this implies.** The std set is small (about 320 kB), a good product is a few MB at most, and the monolithic symbol library files (a 7 MB file to get one 3 kB symbol) are the only awkward size. Parse cost is not a problem: `netsmith build examples/four_layer.nsm` reads about 12 MB of symbol libraries (over the slow `/mnt/c` mount) and takes 0.37 s in a release build.

## 2. Licensing (facts read, and what is not known)

**Sources read.** `https://www.kicad.org/libraries/license/`, the `LICENSE.md` of the `kicad-symbols` and `kicad-footprints` repositories on GitLab (the canonical home, `gitlab.com/kicad/libraries/...`; the GitHub copies are archived and cover KiCad 5), the README of `kicad-footprint-generator`, and the `LICENSE.md` that the KiCad demo projects carry.

**The licence text.** The libraries are CC BY-SA 4.0 with one exception, in the KiCad project's words: "To the extent that the creation of electronic designs that use 'Licensed Material' can be considered to be 'Adapted Material', then the copyright holder waives article 3 of the license with respect to these designs and any generated files which use data provided as part of the 'Licensed Material'." The explanation under it says that using library data in a project does not by itself require the design or its generated files to be CC BY-SA, that users need not attribute the libraries within a design, and then: "if you wish to redistribute the KiCad libraries, or parts thereof (including in modified form) as a collection then the exception above does not apply. Redistributed library collections must be shared under the same license agreement. Under these circumstances, the libraries must also retain attribution information, including the license documents which are distributed with the library files."

**What follows from the text, as a reading and not as advice.**

- *Using* the libraries, which is what a design and the files netsmith writes do, is covered by the exception: the `.kicad_sch` that netsmith exports holds copies of symbols in its `lib_symbols`, and the `.kicad_pcb` holds copies of footprints. Those are "generated files which use data provided as part of the Licensed Material", so a netsmith user's output carries no CC BY-SA obligation. This is the reason the exception matters to netsmith, and it holds equally if the libraries were fetched or bundled.
- *Redistributing* library files, or parts of them, is outside the exception. The page says "parts thereof" and "including in modified form": a subset of 32 symbols and 50 footprints, and a symbol flattened from its `extends` parent, are parts and a modification. For such a redistribution the CC BY-SA 4.0 conditions apply to the redistributed files: attribution (the creator and copyright notices that come with the files, the licence notice, the disclaimer, a link to the licence), an indication that they were modified, and ShareAlike (the files, and anything that is an adaptation of them, under CC BY-SA 4.0 or a licence CC lists as compatible; MIT is not on that list).
- *Does embedding in a binary count as redistribution?* Compiling a file into an executable (`include_str!`) copies it into what netsmith ships. The page does not say that a collection must be a separate file set, and it does not say that embedding in a program is excepted. Not known. The cautious reading is that it is a redistribution of the files, which then carry CC BY-SA 4.0 and need the notice.
- *Does it infect netsmith's own licence?* ShareAlike applies to "Adapted Material", a work based on the licensed material. Source code that reads library files at run time is not based on them. Data files that sit beside the code, in their own directory with their own licence file, are in the ordinary position of an aggregate of separately licensed works. Whether a binary that embeds the files is one work or two is the open question of the previous bullet; Creative Commons' guidance on collections was not fetched for this ADR. A generator that copies a library's geometry would be an adaptation in any case. Not known, and not for the implementer to decide.
- *The tool's own licence is not settled yet.* The workspace says `license = "MIT"` in `Cargo.toml`, but there is no `LICENSE` file at the root. An MIT crate and a CC BY-SA directory in one repository is common; but "MIT" is then true of the code only, and the README must say so.

**Other facts that bear on the options.**

- `kicad-footprint-generator` (which wrote files such as `R_0402_1005Metric`: their header says `generator "kicad-footprint-generator"`) is GPL-3.0-or-later with some MIT and LGPL files. Translating its Python into Rust would make the Rust a derivative of GPL code. A footprint generator for netsmith must work from IPC-7351 and the package drawings, not from that code. Whether the IPC-SM-782 tables can be read freely is the owner's to check (IPC standards are paid documents).
- The 3D models are a third repository (`kicad-packages3D`, whose licence was not read). They are not needed (§1).
- KiCad's demo projects carry a `LICENSE.md` of their own (CC BY-NC-SA 4.0 with a commercial-use exemption, in the one that was read). They are fit as a local test corpus when read from the user's KiCad install; they must not be copied into the repo.
- The installed KiCad rewrites files (Windows line endings; `generator "pcbnew"` in place of the upstream `generator "kicad-footprint-generator"`). Installed `Device.kicad_sym` differs from the upstream `9.0.9.1` one in 644 lines after line endings are ignored. So **a checksum can only pin upstream content**, never an installed copy, and the symbol content really does move between 9.0 patch releases. A pin matters for reproducible output.
- A footprint's name and the pad numbers are facts about a package, and the pinouts in `std/` are netsmith's own source. Whether pad geometry computed from the IPC formulas can be freely reproduced is not something this ADR can settle.

**The options.**

| | A. Bundle a subset in the repo | B. Fetch on first use | C. netsmith's own drawings | D. A mix |
|---|---|---|---|---|
| What ships with netsmith | the 32 symbols and 50 footprints (about 320 kB, a few MB for a wider std set) in a separate directory with the licence files and a `NOTICE`, built into the release or beside it | a lock file (URL, tag, commit, checksums) and no library bytes | generator code and its tests, MIT | B for everything, C for the most common rows |
| Needs the owner's licence decision | yes: CC BY-SA 4.0 for the directory; embedding in the binary is unclear (§2) | no redistribution by netsmith, so no | no for what is generated; the two unknowns above (IPC tables, whether a copied drawing is an adaptation) | the parts of A and C that are used |
| Network | never | once per machine or project, 22.8 MB if whole archives, 20 MB for the std set with per-file fetch of the symbol libraries (they are monolithic) | never | once, unless C covers the design |
| Binary alone | yes | after one fetch | yes, for what is covered | yes for the common rows |
| Reproducible | yes, pinned in git | yes if pinned by commit and checksummed | yes, by construction | yes |
| Cost | S | M | L, and each package is a risk to a real board | M, then L, one package at a time |
| Weak point | licence exposure; has to be updated by hand | offline use, a second step for a new user, GitLab availability | a footprint that is slightly wrong makes a bad board; the symbol drawings (R, C, LED) are trivially original, a QFN is not | two mechanisms |

C needs care to be worth doing. KiCad's own footprints for chip passives, SOT, SOIC, QFN and headers are themselves *generated* from parameters (the `kicad-footprint-generator` data directory holds the parameters), so generated footprints named like KiCad's can be tested against the real ones when a library is in the cache: pad centre, pad size, courtyard and a tolerance of 0.01 mm. That makes the generator checked, not trusted, and means a design made with generated footprints and one made with fetched footprints agree. Without that test, C should not be built.

## 3. Formats: one library format

**Keep KiCad's `.kicad_sym` and `.kicad_mod` as the library format, and add no format of our own.** The reasons:

1. *One way to say a thing* (ADR 0001). A second format, even a compact one, makes every part answerable to two descriptions and every project choose.
2. *The export is KiCad's format.* The exported `.kicad_sch` embeds each symbol verbatim in `lib_symbols`, and the `.kicad_pcb` embeds the footprint verbatim with its `(model ...)`. A native format needs a converter in each direction and a test that the round trip is lossless. With KiCad's format the loader (`symbols.rs`, 812 lines; `footprint.rs`, 1027 lines) and the emitters already exist and are tested.
3. *Everything arrives in this format.* Manufacturers' downloads (SnapEDA, Ultra Librarian, Component Search Engine), the community converters (`easyeda2kicad` for LCSC and EasyEDA parts) and KiCad's own editors all write it, and a person can open and edit it in the symbol and footprint editors. A project's libraries are already read this way (ADR 0021).
4. *Size and speed do not call for a format.* §1: the std set is 320 kB and the monolithic libraries load in a fraction of a second; a vendored subset (below) is smaller than any index would be.
5. *What atopile does* (read from its public material; the part of its documentation that explains the mechanism was not reachable, so this is partly inferred): a part is installed by its LCSC number (`parts_install` in the tool's own skill file; `ato create part` is the command the owner named, and was not found in the public material read), the footprint and symbol are produced from EasyEDA's data (the route `easyeda2kicad` takes) in KiCad's file formats, and a thin `.ato` wrapper over the generated `component` file names the pins. The point to take is that atopile also chose "KiCad's format on disk, converted from a manufacturer-neutral source on demand", not a format of its own. It is a precedent, not evidence about what is best here. The EasyEDA route has its own terms of use (the converters warn about them) and is not recommended for the first version.

**The library story that follows.**

- `netsmith libs fetch` downloads the pinned KiCad libraries (§6, stage 2) into a cache and checks them. `netsmith libs vendor` copies the symbols and footprints that a project's board uses into the project (`libs/kicad.kicad_sym`, `libs/kicad.pretty/`, generated `[[library]]` stanzas in the project's `kicad.toml`, a `LICENSE` and `NOTICE` for the copied files). The project is then offline and its output does not depend on a cache. This is a generator, not a second way to configure (ADR 0021 did the same for `kicad-libs import`).
- The lookup order for a library name becomes: the project's own `[[library]]`, then the cache. The installed KiCad stays reachable only through the existing variables, as the way to point at any folder (an installed KiCad, a checkout, a mirror) and not as a default. That removes the hard-coded `/mnt/c/Program Files/KiCad/9.0` default, which is the one place the tool assumes KiCad.
- The std rows keep naming stock KiCad `Library:Name`s, so the same row works with fetched, vendored or (later) generated content.

## 4. Drawing without KiCad (confirmed by reading the code)

- The schematic viewer and editor draw symbols with their own renderer (`netsmith-view`, `glyph.rs`): `Glyph::for_part` takes the mapping and a `SymbolLib` and reads the `Sexp` of the symbol (rectangles, polylines, circles, arcs, pins). No KiCad program is run. Where a symbol is missing it draws a box with the part's pins and the reason.
- The PCB view and the placement, router, design rules check and fabrication files read the `.kicad_mod` / `.kicad_pcb` text through `netsmith-pcb::footprint` and `netsmith-fab::artwork` only. No KiCad program is run (ADR 0034, ADR 0037).
- `grep` for `kicad-cli` and `KicadCli` finds these remaining users, all optional: `fab --cross-check` and the editor's Findings witness (a second opinion), `tests/erc.rs` and `scripts/check-kicad.sh` (ERC, which netsmith does not do itself: issue #61), and `compare` on a `.kicad_sch` (§5). The `Program Files` default path survives only in `symbols.rs`, `footprint.rs` and `kicad.rs` (the CLI locator).
- The 3D model files are never read (§1). The editor has no 3D view (issue #19).

So after stages 1 to 3 below the only thing a binary-alone user cannot do is ERC and the optional cross-checks, as issue #61 says.

## 5. The netlist: reading `.kicad_sch` directly

**What exists.** `netsmith-netlist` builds a `Netlist` (components with value and footprint, nets with numbered pins) from KiCad netlist XML, from a `.kicad_pcb` (footprints and the net of each pad, in Rust, already) and from parts. `compare` works on connectivity and ignores net names and designators (ADR 0017), so **the new reader has to get the groups of pins right, not the net names** (names matter only where they merge nets). `from_kicad_schematic` calls `kicad-cli` and then `from_kicad_xml`. The S-expression reader is `netsmith-fab::sexp`; the schematic writer's geometry (`netsmith-kicad::schematic`) knows the symbol-to-sheet transform.

**What a `.kicad_sch` holds that the reader must interpret.** The file is self-contained for this purpose: it embeds every symbol in `lib_symbols`, with its pins and their positions, so reading it needs no library. In the KiCad demo corpus (48 files, format versions 20230121 to 20250114) the counts show what it must handle: 6469 wires, 1195 junctions, 2072 local labels, 224 global labels, 252 hierarchical labels, 122 power symbols, 460 no-connect flags, 17 sheet blocks, 191 bus wires, 134 bus entries, and 67 lines that name a unit or body style above 1.

The rules, in the order the algorithm applies them:

1. **Placed symbols to pin points.** Each `(symbol (lib_id ..) (at x y angle) (mirror ..) (unit n) (convert n) ...)` takes its pins from `lib_symbols[lib_id]`: the pins of unit 0 (shared) and of the placed unit and body style, transformed by rotation and mirror, y flipped (symbol y is up, sheet y is down), to a point on the sheet. Coordinates are compared as integers (nm, rounded), as KiCad does. A symbol with `(in_bom ..)`, `(dnp ..)` or `(exclude_from_sim ..)` stays a component (as `kicad-cli` exports it); one marked `(on_board no)` is checked against what the exporter does (§ risks).
2. **Wires, junctions, pins and labels to nets (per sheet).** Union-find over points: the two ends of a wire are joined; a pin or a label point on a wire's middle joins it; two wires crossing, or a wire end on another wire's middle, join only with a junction (to be checked against `kicad-cli`: KiCad's rule changed across versions); stacked pins at one point are one connection. A no-connect flag does not change the netlist (it changes the ERC) and is ignored except to drop the "unconnected" net name.
3. **Names merge nets.** Local labels merge equal names inside one sheet instance; hierarchical labels merge with the sheet pin of the same name in the parent; global labels merge across the whole design; a power symbol (a `lib_symbol` with `(power)`) is a global label named by its `Value`; a hidden `power_in` pin of an ordinary part (the old "invisible power pin" convention) joins the global net named after the pin. Net-tie footprints (`net_tie_pad_groups`) join nets on the board, not in the schematic, and are not in the demos; they are stated as not handled.
4. **Hierarchy.** `(sheet ..)` blocks name a `Sheetfile` relative to the parent file. The same file may be instantiated more than once, so the walk is by sheet instance, not by file: a local label and a refdes are per instance. A component's designator is the `reference` in `(instances (project .. (path "/<root>/<sheet>" (reference "R1") (unit 1))))` whose path equals this instance's path; formats before KiCad 6 keep the reference in the symbol and are refused with the version named.
5. **Units.** A multi-unit part is several placed symbols with one reference; they are one component, and their pins are the union. Pins with the same number (stacked pins) are one pin. Body styles (`convert`) choose which graphics and pins are in play.
6. **Buses.** A bus wire carries a label `D[0..7]` (a vector bus), `{A B}` (a group) or an alias name; bus entries join a wire to one member of the bus; a hierarchical label or sheet pin carries a bus; `(bus_alias ..)` expands a name. Members join by name, as labels do. This is the largest single piece and the most likely to disagree with KiCad in an edge case, because the rules (member naming, prefix of a group, bus on a hierarchical pin) are subtle.
7. **From nets to the `Netlist`.** Pins grouped by net, power and flag symbols dropped (as `from_parts` already drops `#` designators), one-pin nets dropped as unconnected.

**Effort.** Roughly 1200 to 1800 lines of Rust and as many of tests, in `netsmith-netlist` (a `sch.rs` beside `parse.rs`), reusing `netsmith-fab::sexp`. Stages: 4a flat sheets with wires, junctions, local labels, power symbols (M); 4b hierarchy, global labels, hierarchical labels, sheet instances (M); 4c units, hidden power pins, bus labels, bus entries, aliases (M); 4d the validation harness and edge cases (M). The `.kicad_pcb` path needs only a check that it handles `net_tie` and `board_only` the way the schematic path will.

**Validation (all of it can be run today; KiCad 9 `kicad-cli` works from WSL, 6.7 s for one netlist).**

1. Differential test against `kicad-cli sch export netlist` on three corpora: (a) every `.kicad_sch` that netsmith generates (the six examples, and every board of the test suites), (b) the 48 demo schematics read from the installed KiCad (not copied into the repo), (c) hand-made small schematics for each rule in the list above (bus group, a sheet used twice, a global label alone on a sheet, a hidden power pin, a pin on a wire's middle, a wire end on a wire's middle with and without a junction, a stacked pin, a multi-unit part, a net tie), which are kept in the repo as text fixtures. KiCad's own QA data (the `qa/data/eeschema` folder of the KiCad source, which this ADR did not fetch) is the expected next corpus if it holds netlist cases; to be checked.
2. The comparison is `netsmith compare --by-refdes` between our netlist and the XML: any difference in the pin groups fails; a difference in net names alone does not.
3. A property check: for the generated boards, the netlist read from the exported `.kicad_sch` equals the netlist of the `.kicad_pcb` and of the design itself (`netlist_of_design`), by `compare`. That needs no KiCad at all and runs in CI.
4. After the stage, `compare` on a `.kicad_sch` no longer asks for `KICAD_CLI`; `kicad-cli` stays as an optional witness (`compare --cross-check`), the same shape as ADR 0034 and ADR 0037.

**Risks.** The junction and wire-end rules differ between KiCad versions; the bus rules are intricate; a designator written in a pre-KiCad-6 file is not in `instances`; KiCad may keep or drop a component the reader keeps (`on_board no`, `exclude_from_board`) and the harness has to show which; hidden power pins create nets that no wire shows; a file in a format newer than 20250114 must be refused by version, not misread. A design that cannot be read is an error naming the file, line and version, never a guess (`compare` already prefers "could not decide" to a wrong answer).

## 6. Recommendation and staged plan

**Recommendation.**

1. Keep KiCad's two file formats as the only library format (§3).
2. Make **fetch the default**, with a lock file and a project cache, so that netsmith redistributes nothing and the binary stays small; add **`libs vendor`** so a project can be offline and self-contained. This needs no licence decision from the owner for the tool itself, and it gives a person with the binary a working tool after one command.
3. Decide separately and later whether to give the binary a no-network floor: either option A for the 8 symbols and 9 footprints of the examples (after the owner's licence decision) or option C for passives and a few packages, tested against the fetched footprints. This is optional; it only matters for a user who has no network and no cache on first use.
4. Read `.kicad_sch` directly (§5); it is independent of the libraries and can run in parallel.

**Stages** (sizes: S about a day of work, M two or three days, L about a week; they are guesses).

| Stage | What | Size | Needs |
|---|---|---|---|
| 1 | Library lookup is "project libraries, then the cache folder"; remove the hard-coded KiCad path as a default (the variables stay as the way to point at any folder); the error says `run netsmith libs fetch` | S | owner Q4 (cache place) |
| 2 | `netsmith libs fetch`: a lock file in the repo (`std/kicad-libs.lock`: repository URLs, tag `9.0.9.1`, which is commit `930ff67...` in the symbols repository and `8ec1aeb...` in the footprints one, and a sha256 for each file netsmith reads); downloads the symbol libraries and footprint files the std rows and the project's rows name (or all with `--all`) from the pinned commit's raw URLs into the cache, verifies each file's sha256 and refuses on a mismatch naming file, expected and found; `--from <dir or archive>` for an offline mirror; tests use a local directory mirror, never the network | M | owner Q1, Q3 |
| 3 | `netsmith libs vendor`: copies the used symbols (flattened, one `.kicad_sym` per project) and footprints into `libs/`, writes the `[[library]]` stanzas, `NOTICE` and the licence text, and sets the mapping of the std rows to the vendored names or keeps them stock; idempotent | M | owner Q2, Q5 |
| 4a to 4d | `.kicad_sch` reader (§5), with the differential harness | L (4 x M) | none |
| 5 | `compare` and the editor stop looking for `kicad-cli` unless asked; the issues #5 and #61 updated | S | stage 4 |
| 6 | (optional) built-in symbols and footprints: chip passives, LED, diodes, SOT-23, headers first, tested against the fetched ones to 0.01 mm | L, one package at a time | owner Q2, Q6 |

Order: 1, 2 then 4a; 3 and 4b to 4d next; 5; 6 only if wanted. 1 to 3 together are the answer to "no KiCad installed". 4 and 5 are the answer to "compare without kicad-cli".

**Risks.**

- *Upstream moves.* GitLab can rename, move or rate-limit; the pin and the per-file checksums catch content changes but not availability. A mirror option (`--from`) and a vendored project are the answers.
- *Pinned content versus the user's installed KiCad.* A project that has been vendored from `9.0.9.1` has symbols that a newer KiCad would draw differently; a lock records what was copied, and `libs vendor --update` is a deliberate step.
- *Archive checksums.* The tag archive from GitLab had the same sha256 in two downloads, but GitLab does not promise stable archives, so the lock hashes the files, not the archive.
- *Licence.* Everything in §2; the safe default is that netsmith redistributes no library bytes, and the vendored copies sit in the user's project under a licence file.
- *Wrong footprints from generators (stage 6).* A wrong footprint is a wrong board. That is why the generator is tested against the real one and why it is optional.
- *A symbol in a project library that changes under a design.* Already ADR 0021's problem; vendoring makes it visible in version control.
- *The netlist reader disagreeing with KiCad.* §5; the harness is the control.

## 7. Questions for the owner

1. **Licensing choice.** Is netsmith allowed to ship any KiCad library bytes (option A, inside the repo or the binary)? If yes, in a separate directory with CC BY-SA 4.0 and a `NOTICE`, or in the binary too? If no, option B (fetch) only. The text read cannot settle whether a binary that embeds the files is a redistribution of them; an independent opinion should be taken if A is chosen. Also: should the repository get a `LICENSE` file for the MIT code?
2. **Footprints and symbols of our own.** Should netsmith also generate footprints (and draw the simple symbols) itself, for the common packages, tested against KiCad's? This removes the bytes question for those parts and gives a no-network floor, but costs a week or more and carries the risk of a wrong pad. Is the IPC-7351 route acceptable (IPC documents are paid), or should the dimensions come from manufacturers' drawings, package by package?
3. **Which libraries and version to pin.** KiCad `9.0.9.1` (the newest 9.0 tag found) for symbols and footprints, or the version the owner's KiCad is? The installed copy on this machine is a 9.0 build whose `Device` library differs from `9.0.9.1`. Only the libraries the std rows name, or the stock libraries in general (every user project that names a stock `Library:Name` would then fetch more)?
4. **Where the cache lives.** `~/.cache/netsmith/kicad-libs/<tag>/` (per user, shared by projects), or a folder in the project (`.netsmith/libs/`, ignored by git), or both with the project's winning? The proposal is per user, plus `libs vendor` for the project.
5. **Offline use.** Is a first-use download acceptable for "the binary alone is enough", or must a fresh binary work with no network? If the latter, stage 6 or option A is required, not optional, for at least the examples' parts.
6. **Project versus user.** Should a project always vendor what it uses (and commit it), so that a design is reproducible from its repository without a fetch, or is a lock file enough? Vendoring makes the project carry CC BY-SA files, with their notice, in its own repository (the design exception covers the design and its generated files, not the library copies themselves).
7. **LCSC and EasyEDA parts.** Should a later step import a part by LCSC number into a project library (the atopile route)? It is outside this ADR, and the data's terms of use have to be read first.
