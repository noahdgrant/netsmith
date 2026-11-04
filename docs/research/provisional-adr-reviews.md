# Review of the provisional ADRs

Review summaries for issue #10, written while the owner was away. Each entry says what the ADR decided, why, what it costs to reverse, what I found that disagrees with the code or another ADR, and a recommendation: **accept**, **change** or **revert**. I did not edit these ADRs. The two ADRs the owner added to the issue in a comment (0047 and 0045 section 11) are in issues #33 and #29 and are not repeated here.

I read each ADR in full and checked claims against the code where a claim could be checked by reading or searching (no build or test was run). Where I could not check, I say so.

## One reply

| ADR | Recommendation | What the owner approves |
|---|---|---|
| 0021 KiCad libraries | accept | keep the mapping in TOML and project libraries; note the unverified install paths |
| 0022 Catalogs as CSV | accept | CSV per catalog, a `source` on every row, the file order as policy |
| 0031 Fine pitch, vias in pads, back face | accept, one follow-up | routing and via in pad; the hole-part two-face rule needs a KiCad cross-check |
| 0033 Derived placement | accept, one follow-up | annealing, the derived wall; replace the private-board numbers with a public board |
| 0034 Own fabrication files | accept the design, keep it provisional | until a package has been accepted by JLCPCB's order preview |
| 0037 Own design rules check | accept, one edit | list the rules that are not KiCad's; confirm silkscreen and mask-web findings are warnings |
| 0039 Catalog refresh | change | rewrite as one current-state ADR |
| 0045 section 10 Overhang | change as decided on #11, accept the rest | edge fields only; locked parts state every edge; `fab` warns |

A reply such as "accept all; 0039 and 0045 as recommended" is enough for me or the next agent to act on.

## A finding across all of them

The front matter of 0021, 0022, 0031, 0033, 0034 and 0037 says `accepted (user review 2026-11-02)`. 0045 says `accepted (user decisions 2026-11-02)` with section 10 provisional. Their bodies say "provisional, to be reviewed by the maintainer", and issue #10 says they "were decided without review". These three cannot all be true. If the 2026-11-02 review did happen, the bodies are stale. If it did not, the front matter is wrong. Either way the status lines should be updated when the owner answers (for example `accepted (owner review 2026-11-03)`). I recommend the second wording once you have read them.

## ADR 0021: KiCad libraries, installed and project

**Decided.** Read symbols and footprints from the installed KiCad libraries. The mapping from (kind, package) to symbol and footprint is `std/kicad.toml`, and a project adds its own in `kicad.toml` with `[[library]]` stanzas. Multi-unit symbols are supported. A `passive` part pin suits any symbol type but a supply or no-connect. `build` and `fab` need the libraries; `check`, `solve`, `compare` and the editor's reading do not. The ADR ends with the owner's direction that netsmith must not need KiCad installed.

**Why.** KiCad's libraries are large, licensed CC-BY-SA, and kept up to date by others, so netsmith carries none (README, Licensing note). A mapping file is the one place KiCad's names appear (ADR 0002).

**Cost to reverse.** Medium to high. Every standard part has a row in `std/kicad.toml`; the pinout validation (`validate.rs`) is built on this reading; and the bundled copies are deleted. Reversing means shipping symbols, which the licence note avoids. The direction at the end of the ADR points the other way, toward netsmith's own generators (ADR 0043).

**Findings.**
- Only the Windows KiCad 9 seen from WSL was tried on a real install. Linux, macOS, snap and flatpak paths follow the known layouts and are tested against invented trees. The ADR says so. This is the most likely thing to fail for another user.
- The ADR says the five examples export "byte for byte" as before; the examples directory now has more than five, so that sentence describes a past state.
- "The value and hand-solder flag still come partly from a `match` in `netsmith-lower`" is in Limits. I did not check whether that still holds.

**Recommendation: accept.** Add to the status line that the non-WSL install lookups are unverified until someone runs them. No code change.

## ADR 0022: catalog rows are CSV files

**Decided.** One CSV per catalog, named by the catalog. Column names are field paths; values use the language's own syntax; text is bare. Fixed columns for the part's identity and supplier data. A row with an `lcsc` or `mpn` must have a `source`. The order of rows is the selection policy. A project lists its own catalog files in `netsmith.toml`. Every mistake names `file:line`.

**Why.** Adding a part used to be a Rust edit in the solver crate, and a private project could not have its own parts. A spreadsheet and a diff both read CSV. The `source` rule enforces "no part number comes from memory" in the loader.

**Cost to reverse.** Medium. The 13 standard files under `data/jlcpcb/catalog/` and every project's `catalogs/` would need a converter, but the loader is isolated. The rule that a row needs a `source` is cheap to keep either way.

**Findings.**
- ADR 0047 supersedes items 4 and 6 (the price, stock and date columns; the stock-0 rule). The front matter says so. The code agrees: `crates/netsmith-solve/src/table.rs` says the solver does not read stock. The body of 0022 still reads as if both stand, so a reader of the body alone is misled.
- The glossary lists `checked` and `price_breaks` as fixed columns. 0022 does not (0039 added them). The column list lives in three places (0022, 0039, `GLOSSARY.md`) and the header comment in `table.rs`.
- Item 6 says file order is the policy and "a project that wants cheapest first writes its rows that way". ADR 0047 aims at a preferred-parts list where file order stops being the solver's policy (its open question 3). If that lands, item 6 changes again.

**Recommendation: accept.** Mark in the body which items 0047 supersedes (a note beside item 4 and 6), or fold the column list into one place. No code change.

## ADR 0031: fine-pitch routing, vias in pads, and parts on the back

**Decided.** The router tries a tight room for lines that fail the first pass, uses a via in the pad as a last resort when the process allows it (`.via_in_pad`, up to 0.55 mm for JLCPCB), necks tracks down, and negotiates for lines still left. The `.kicad_pro` Default net class gets a priority. A part has a `Side`; a part on the back is its footprint mirrored. **Provisional amendment:** a through-hole part takes its courtyard on both faces only if the footprint draws it on both `F.CrtYd` and `B.CrtYd` or on neither; if it draws it on one only, the other face is judged by the pad boxes grown by the clearance. The ADR 0045 section 10 amendment makes the fit test try the annealer before refusing.

**Why.** A real module had pads on a 1.1 mm pitch that the router could not reach. A two-face rule was needed so a back-face part can sit under a hole part's body.

**Cost to reverse.** The routing steps: low to medium, since they only try more. The two-face rule: low in code (`Board::room_on`, `Engine::rooms_on`, `could_fit`) but it changes where parts may go, so every placed board can move.

**Findings.**
- The two-face rule depends on a library convention (which courtyard layers a footprint draws). The ADR does not say it was cross-checked with KiCad's own courtyard check, which it does for the DRC in ADR 0037. I could not tell from the ADR or by reading whether KiCad agrees. That is the follow-up.
- The text says "The placer takes `PlaceOptions { sides, fit, keepouts }`, which the sidecar layout file will fill". The layout file exists (ADR 0032) and `PlaceOptions` has more fields (`overhang`, `wants`, `fixed`, `wall`). Stale sentence.
- "Connectors are not moved to the back" under Left open: ADR 0033 says connectors are put at the edges; still not on the back. Consistent.
- JLCPCB's order form for a filled via-in-pad on the two-layer process is listed as unverified. Still the owner's to check.

**Recommendation: accept, with one follow-up:** compare the pad-box rule with KiCad's courtyard check on a footprint that draws one face's courtyard only, as the cross-check does elsewhere. Update the stale `PlaceOptions` sentence.

## ADR 0033: derived placement

**Decided.** The placer is a simulated annealing over poses (origin on a 0.05 mm grid, a turn, which side the reference text is on), with a seeded random generator written in the crate. The cost has terms for wire length, overlap, a wall, module cohesion, ratsnest crossings, decoupling pairs, connectors at the edges, area, turns and silkscreen. **Provisional:** the wall, once a fixed 2.0 mm, is now `Rules::edge_wall`, the process's copper-to-edge space plus clearance plus one track (0.4 mm for JLCPCB). A placer given no rules keeps 2.0 mm.

**Why.** A derived placement needs no syntax (ADR 0030). The 2.0 mm wall cost about 364 mm2 of each face on a 35 x 60 mm board and nothing recorded why it was 2.0.

**Cost to reverse.** The placement of every existing board changes if the cost, the moves or the schedule change; a test says so (`place.rs`, "A change to the cost or the moves changes them: say so"). The wall can be reverted by one constant. Reversing the algorithm would mean writing another placer.

**Findings.**
- The Measured table is for boards in this repository's tests and for a private 73-part board "a copy of it was measured; its data is not in this repository". Those numbers cannot be reproduced by anyone else.
- The ADR says the wall "can never be less than the copper to edge space". `docs/LANGUAGE.md` says 0.4 mm for JLCPCB, which matches 0.2 + 0.1 + 0.1 if those are the process numbers; I did not read `jlcpcb.nsm` to confirm each.
- KiCad's check finds `starved_thermal` on one seed in four of the private board; netsmith's does not check for it (ADR 0037, "The pour"). So the gate can pass a board KiCad flags. It is a known gap, stated twice.
- "Faces. The placer shares the faces only when a size to fit needs it" was amended by ADR 0045 section 10; both say so.
- "Hierarchical placement is not built" overlaps with issue #16 (modules as sheets) only in vocabulary, not design.

**Recommendation: accept, one follow-up:** add a public reference board whose wire, crossings and check results can be re-measured, and move the private numbers out or label them as not reproducible. Consider whether the wall should be a guideline-level warning for a very thin one, since silk can then collide (the ADR notes it). No code change now.

## ADR 0034: netsmith writes its own fabrication files

**Decided.** `netsmith-fab` writes Gerber X2 for every layer, Excellon drill files, the placement file and a stroke font of its own for silkscreen text, from the board file. It fills zones itself. KiCad is only an optional witness (`fab --cross-check`, which compares layers as bitmaps, holes hole by hole, and the placement row by row).

**Why.** The owner's direction (ADR 0021): netsmith must not need KiCad.

**Cost to reverse.** High. Everything in `fab` depends on it. Going back to `kicad-cli` plots would reinstate the dependency.

**Findings.**
- The ADR says: "JLCPCB has not been given these files." A cross-check against KiCad's plot is evidence of agreement with KiCad, not of acceptance by the factory. `docs/ROADMAP.md` item 5 ("JLCPCB DFM") is still open for the same reason. This is the single biggest risk in the whole set of ADRs.
- The pour is written with clear polarity inside a copper file, which "KiCad does not" write; the ADR asks to look at the planes in the order preview first.
- Silkscreen letters differ from KiCad's (own font), and the comparison tolerance for silk is 0.6 mm.
- ADR 0034's "The design rules check without KiCad" section points forward to 0037, correctly.
- The pick-and-place rotation is verified only for the packages in `rotation.rs` (README, Known limits). Parts on the back have rotation unverified.

**Recommendation: accept the design, and keep the status provisional until one real package has been uploaded to JLCPCB's order preview and its own check, and the planes and silkscreen have been looked at.** `netsmith-prepare-fab-order` already lists the preview checklist; one board and one result written into the ADR would close this. The owner is the only one who can do it.

## ADR 0037: netsmith's own design rules check

**Decided.** The check lives in `netsmith-fab` over the `.kicad_pcb` and `.kicad_pro`, so it sees what the board house will make. Findings are data: a rule (named as KiCad names it), a severity, a message with numbers, a place and the items. It gates `fab` and the editor's Fab button. Rules cover clearance, shorts, connectivity, sizes, holes, edges, courtyards, silkscreen, mask bridges, rule areas, dangling items and the outline, plus two of netsmith's own (`hole_to_edge`, `via_in_pad`). The exporter writes `silk_over_copper`, `silk_overlap`, `silk_edge_clearance` and `solder_mask_bridge` as warnings and everything else as errors.

**Why.** `fab` used to refuse without KiCad. The cross-check compares findings by rule and place in both directions: 28 broken boards, 127 findings agree, none differ; 37 other boards, 163 agree.

**Cost to reverse.** High; it replaced KiCad as the gate. Low for individual rules.

**Findings.**
- The ADR says a finding's rule is "named as KiCad names it". Later work adds rules that are not KiCad's: the editor reports `constraint` (ADR 0045 section 8), `too_tall` and `height_unchecked` (section 11), in `crates/netsmith/src/editor/findings.rs`. The rules table in 0037 does not list them. Not wrong, but the ADR is the place a reader looks.
- The severities for silkscreen and mask web are a policy ("a fabricator clips silkscreen ... neither stops a board being made"). The reasoning is in the ADR, but the rule list in `crates/netsmith-pcb/src/rules.rs` (lines near 143) encodes it. Nobody has confirmed it as a decision.
- The ADR lists what is not checked (a pour's fill, islands, starved thermals, differential pairs, impedance, custom rules). The gate therefore passes boards that KiCad's flags in those categories. It also says so for ADR 0033.
- The statement that "`--allow-drc-violations` waives only real violations" now has an exception: it never waives a placement requirement (ADR 0045 section 8). Both are documented, in different ADRs.

**Recommendation: accept.** One edit: add the three non-KiCad rule names, and say who decided the warnings (this would be the owner's reply). No code change.

## ADR 0039: `netsmith catalog refresh`

**Decided.** An opt-in command, the only thing that touches the network. First built to update price, stock and library from JLCPCB and LCSC. The 2026-11-02 owner decisions: JLCPCB is not asked (its `robots.txt` disallows the path); then the command became a check against LCSC's record that writes one cell, `checked`, and never writes `price`, `stock`, `price_breaks`, `library` or `date`. Rows disagree as CONFLICT or MISSING and are never changed. The sections that no longer hold stay in the text, marked "Superseded".

**Why.** The factory's numbers are JLCPCB's, LCSC's differ (stock 1,085,700 against 2,059,008 for C25104), and the terms of use are unread.

**Cost to reverse.** Low for the command (it is separate). The `checked` column is already in 13 files.

**Findings.**
- The front matter repeats one sentence twice ("ADR 0047 (provisional, 2026-11-03) supersedes the data-in-files parts ..."). It reads as a merge accident.
- The ADR keeps its original text as a record and marks what is superseded. The result needs three readings to learn what is current: the first decision, the "2026-11-02 decision", the "second 2026-11-02 decision", and then ADR 0047. Item 9 (stock 0 not offered) stands in 0039 "on the hand-entered stock" and is withdrawn by 0047, which the ADR 0039 body does not say next to item 9.
- LCSC's terms of use are unread (item 8). The command is opt-in and polite (sequential, 500 ms, cache, honest User-Agent), which the ADR says is courtesy and not permission. README and the command say this.
- The official APIs are future work, so JLCPCB's own price, stock and library cannot be fetched.

**Recommendation: change.** Rewrite 0039 as a one-page ADR of what is true now (a check, `checked`, the fetcher's refusals, the unread terms) and move the history to a "How it got here" section at the end. Same decisions, shorter. Keep the unread-terms warning. No code change.

## ADR 0045 section 10: overhang (K2)

**Decided (before this review).** A part may hang over an edge only if the design says so: a requirement on its own `.place.left`, `.right`, `.top` or `.bottom` whose whole window is at or beyond the same edge of the outline, or a lock in the layout file (every edge it is past). The courtyard is clipped to the outline for the fit and overlap; pads are always checked; a courtyard past an edge nobody stated is the old error with the numbers; the body's centre may be off the board in a stated direction.

**Why.** A real design has a plug mostly off the board, which the placer refused.

**Cost to reverse.** Low: the section says it changes in one place (`netsmith-pcb::overhang` and `constraints::stated_edges`).

**Owner decisions on issue #11, 2026-11-03.** Edge fields only (no `.x`/`.y`), a locked part must state every edge it is past, `fab` warns on a stated overhang, silkscreen over the edge stays a warning, no limit on how far. These are recorded in the ADR text and the code changes they need are listed in the pull request.

**Findings.**
- With the decisions, two parts of the current code disagree with them: `Overhang::Locked` allows every edge (`overhang.rs`, `board.rs`), and `fab` prints `overhang: ...` as a note, not a warning (`fab.rs`).
- The glossary entry _Overhang_ and `docs/LANGUAGE.md` ("A part that hangs over the board edge") describe the old lock rule. They change with the code, not before.
- The `netsmith-use-constraints` skill says the same (step 5).
- Items (c) the derived wall, (d) the hole-part rule, (h) a module's overhang and (i) the annealer's stricter rule are still open. The ADR keeps them as "Open (the user's to decide)". Issue #11's last unchecked question, "Can a module state an overhang for its parts?", is item (h).

**Recommendation for the open items.**
- (c) and (d): accept; they are the same as the ADR 0033 and 0031 reviews above.
- (h) A module cannot state an overhang for its parts. Keep it refused. A module's author does not know the case. If a module needs one, it can offer a value as ADR 0042 describes.
- (i) Make the annealer agree with the check (issue #2, third item): two parts that overhang the same edge may overlap beyond it.

**Recommendation: change as decided on #11; accept the rest.**
