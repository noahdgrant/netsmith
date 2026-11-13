---
name: netsmith-dev-extend-language
description: Add or change a language construct in netsmith, from the parser through the checker, solver, design stage and exporters, with tests at each stage and the guide updated. Use when a change adds syntax, a check, a solver rule, or a std definition.
---

# Extend the language

Entry point: `AGENTS.md`. First settle the design with the user (`netsmith-dev-decision`). Do not start code for a language-design choice that is unanswered.

## Order of work

Work from the front of the pipeline. Finish and test each stage before the next.

1. `crates/netsmith-lang`: lexer, parser, syntax tree, printer. A construct must print back to the source it came from (`to_source`).
2. `crates/netsmith-check`: `checker.rs` (names, declarations), `connect.rs` (connections, nets, chains, balances), `lower.rs` (expressions, narrowing), `model.rs`. Every refusal names the field or path involved.
3. `crates/netsmith-solve`: propagation, picking, verification. Catalog rows are CSV in `data/jlcpcb/catalog/` (`src/table.rs` reads them, `src/jlcpcb.rs` loads the embedded ones).
4. `crates/netsmith-design` and `crates/netsmith-core`: the tool-independent `Design`.
5. Exporters: `netsmith-kicad`, `netsmith-pcb`, `netsmith-fab`, `netsmith-view`, and the editor (`netsmith-editor`, `crates/netsmith-workspace/src/`) if it shows the construct.
6. `crates/netsmith`: commands and the project manifest.

Tests live in each crate's `tests/`. Add a test for the behaviour and one for each failure message, asserting that it names the numbers and fields.

## Rules the language keeps

- One correct way to say a thing. Remove the redundant construct or add a better one, never a second spelling.
- Statement order never matters.
- Libraries (`std/`) state only what is always true and name no CAD tool.
- A module is configured through its interface only.
- Only a board names nets.
- Do not change behaviour that contradicts an ADR. Report the contradiction.

## Adding a std definition

Put it in `std/**.nsm` with `///` docs that state its source for any number. A new `part` states its designator letters (`.designator &= "R"`, IEEE 315 and ASME Y14.44 usage); a test fails without them. A new file also needs an entry in `Loader::std()` in `crates/netsmith-check/src/loader.rs`. List the definition in the catalogue at the end of `docs/LANGUAGE.md`. The guide test fails until you do.

## Update the documents

- `docs/LANGUAGE.md`: the construct's section, its example and its message. Examples marked `nsm` are run by `crates/netsmith/tests/guide.rs`. Run `GUIDE_BLESS=1 cargo test -p netsmith --test main guide::` to refresh message blocks, then read the diff.
- `GLOSSARY.md` for new terms, `docs/DESIGN-RULES.md` for design rules, a GitHub issue for what stays open, an ADR when the choice qualifies.

## Finish

Run the gates in `netsmith-dev-verify`. Report what you decided, what you left open, and anything unverified.
