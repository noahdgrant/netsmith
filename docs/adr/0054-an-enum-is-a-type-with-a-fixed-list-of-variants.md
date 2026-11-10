---
status: proposed
---

# An enum is a type with a fixed list of variants

Written overnight on 2026-11-05 by an agent told to pick an option and go on. The syntax `enum` and `Enum::Variant`, the prelude and the refusal of text are the owner's to accept or change. Tests 1 (std, every project's `.assembly` lines and later the bus roles use it) and 3 (a text value). Cut from `git show 8023ee9:docs/adr/0054-an-enum-is-a-type-with-a-fixed-list-of-variants.md`.

Amends ADR 0042 (a module may expose assembly) and ADR 0024 (assembly is a field of the part): the value is no longer text. ADR 0042 had rejected an `Assembly` type of fixed words "for now ... it can be added if wrong words in modules prove a real cost". The owner asked for it on 2026-11-05. The cost it removes: `.assembly &= "hnad"` was accepted by the checker and the solver and refused only when the board was lowered, with a message that named the part but not the line.

## Decision

A file may declare an enum, like a definition: `enum Name { A, B, C }`. The variants are names separated by commas or line ends, each may have `///` lines, and a name is written once. An enum has no members and no body of statements.

- A variant is written with its enum: `Drive::Dimmed`. The enum is found as a type is, in the file or by `import`; a file imports the enum, not its variants.
- A member has the enum as its type (`.drive: Drive`). It takes a value with `&=`, a default with `?=` (ADR 0051), and is compared with `==`. Two `&=` of different variants are the usual empty-intersection error, which prints the variants as written.
- Only a variant of the field's own enum fits. A quoted word, a number, a variant of another enum and a `String` are refused, each saying what the field holds and what the value is. `>=` and `<=` are refused, with the reason: variants have no order.
- A variant the enum lacks is refused where it is written, with the variants listed and the nearest named when one is a case slip or at most two edits away.
- Inside the checker a variant is the text of its name, so the solver, the catalog columns and the exporters see a text domain they already handle. The field's kind (`FieldKind::Enum`) is what keeps a text from standing in for a variant and prints a value as `Drive::Dimmed`.

`AssemblyMode` is the first enum, in `std/prelude.nsm`: `Fab`, `Hand` and `None`, the three that `netsmith fab`, the BOM and the placement file accepted (ADR 0024); ADR 0084 added `Consigned`. The built-in `.assembly` of every part has this type, and a module's value is `.assembly_mode: AssemblyMode` (ADR 0042's idiom, now typed). The names of the prelude are in scope in every file with no `import`, because a part's `.assembly` exists without one. The checker reads the prelude itself, so a checker with no standard library still knows it; `import std::prelude::AssemblyMode` also works.

The manifest (`assembly = "fab"`) and the catalog files (the `assembly` column) are not source text. They keep the words `fab`, `hand` and `none`, and lowering maps a word to the variant. A catalog column of an enum type holds the variant's name as it is (`X7R`, ADR 0055).

## Migration

A text where `.assembly` or a module's `.assembly_mode` belongs is an error, not a second way to say it (`docs/DESIGN-RULES.md`, "One correct way"). The message names the variant it was meant to be (`write AssemblyMode::Hand`). A module with `.assembly_mode: String` that ties a part by `.j.assembly &= .assembly_mode` is refused at the tie.

This broke every `.nsm` that stated assembly. The std library, the examples, the guide and the tests were changed with it. A project outside this repository changes its own lines mechanically (`"hand"` to `AssemblyMode::Hand`, `String` to `AssemblyMode`), and one run lists them all.

## What was rejected

- Accepting both text and variants for one release: two spellings of one thing for as long as it lasts, and the checker would carry text into a typed field. If the owner wants a release with both, it is a warning in `check_value` in place of the error, and nothing else changes.
- `AssemblyMode.Hand`, with a dot: a dot starts a member path, and a bare name inside a block is looked up outward as a member, so it would read as a path whose first segment does not exist. `::` already separates a name from what is in it and a path never has it, so it needs no new token.
- Bare variant names (`.assembly &= Hand`): a variant would share the namespace of members, nets and rails, and a reader could not tell a variant from a member.
- A marker such as `#Hand` or `'hand'`: a new token for what `Enum::Variant` says; `'...'` is the quoted name of a member (`.'5'`).
- A Rust built-in `AssemblyMode` with no source: the owner asked for std to define it, and a definition a reader can open says more than a name in a table.
- An import for `AssemblyMode` in every file that states `.assembly`: an import for the type of an undeclared member has no use beyond the line itself.
- Numbers or order for variants, and a variant carrying a value: each is a feature of its own and needs its own decision.

## Cost

Diagnostics print `AssemblyMode::Hand` only where they were changed (the contradiction message of the checker and the solver, and the solver's `==`); another place that prints a domain may print the bare name in quotes. The editor's go-to-definition does not follow a click on the enum's name inside `Enum::Variant`. `std::prelude::AssemblyMode::Hand` parses and is refused with "qualified type names are not supported yet". Nothing lists the variants of an enum back to a program, and an enum has no default variant. The `none` of the manifest and the catalog files is the variant `None`; the guide says so where the manifest is shown.

## Left open

- The spelling: `enum Name { ... }` and `Name::Variant`, with `::` used for a value as well as for a path.
- `AssemblyMode` in scope with no import.
- The hard error for text, in place of a release that accepts both.
- Projects outside this repository whose modules still declare `.assembly_mode: String`, which this change refuses.
