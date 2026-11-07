---
status: proposed for owner review (written overnight on 2026-11-05 by an agent that was told to pick an option and go on; the syntax `enum` and `Enum::Variant`, the prelude and the refusal of text are the owner's to accept or change)
---

# An enum is a type with a fixed list of variants

Amends ADR 0042 (a module may expose assembly) and ADR 0024 (assembly is a field of the part): the value is no longer text. ADR 0042 recorded, under _Considered_, an `Assembly` type of the fixed words `fab`, `hand` and `none`, "rejected for now ... it can be added if wrong words in modules prove a real cost". The owner asked for it on 2026-11-05, and this is that type, made as a language construct and not as a special case of one field.

The cost it removes: `.assembly &= "hnad"` was accepted by the checker and the solver and refused only when the board was lowered, with a message that named the part but not the line. A module's `.assembly_mode: String` accepted any word for the same reason.

## Decision

A file may declare an enum, like a definition:

```nsm-fragment
/// How a lamp is driven.
enum Drive {
    /// Always on.
    Steady,
    Dimmed,
}
```

- `enum Name { A, B, C }`. The variants are names separated by commas or line ends, each may have `///` lines, and a name is written once. An enum has no members and no body of statements.
- A variant is written with its enum: `Drive::Dimmed`. The enum is found as a type is, in the file or by `import`.
- A member has the enum as its type: `.drive: Drive`. It takes a value with `&=`, a default with `?=` (ADR 0051), and is compared with `==`. Two `&=` of different variants on one field are the usual empty-intersection error, which prints the variants as they are written (`it is already Drive::Dimmed, which does not overlap Drive::Steady`).
- Only a variant of the field's own enum fits. A quoted word, a number, a variant of another enum and a `String` are refused, each saying what the field holds and what the value is. `>=` and `<=` are refused for variants, with the reason (variants have no order). `==` compares two variants of one enum.
- A variant that the enum lacks is refused where it is written, with the variants listed and the nearest named when one is a case slip or at most two edits away: ``error: `AssemblyMode` has no variant `Hnad`; its variants are `AssemblyMode::Fab`, `AssemblyMode::Hand` and `AssemblyMode::None`; did you mean `AssemblyMode::Hand`?``
- Inside the checker a variant is the text of its name, so the solver, the catalog columns and the exporters see a text domain they already handle. The field's kind (`FieldKind::Enum`) is what keeps a text from standing in for a variant and what prints a value as `Drive::Dimmed` in messages.

`AssemblyMode` is the first enum, with the variants `Fab`, `Hand` and `None`, the three that `netsmith fab`, the BOM and the placement file already accept (ADR 0024). It is defined in `std/prelude.nsm`. The built-in `.assembly` of every part has this type, and a module's value is `.assembly_mode: AssemblyMode` (ADR 0042's idiom, now typed). The names of the prelude are in scope in every file with no `import`, because a part's `.assembly` exists without one: a board that writes `.j1.assembly &= AssemblyMode::Hand` must not have to import the type of a member it never declared. The prelude is read by the checker itself (so a checker with no standard library still knows it) and is listed with the standard library as `std::prelude`, where `import std::prelude::AssemblyMode` also works.

The manifest (`assembly = "fab"` in `netsmith.toml`) and the catalog files (the `assembly` column) are not source text. They keep the lower-case words `fab`, `hand` and `none`; lowering maps a word to the variant (`fab` is `Fab`). A catalog column of an enum type holds the variant's name as it is (`X7R`, ADR 0055).

## Migration

A text where `.assembly` or a module's `.assembly_mode` belongs is an error, not a second way to say it (`docs/DESIGN-RULES.md`, "One correct way"). The message names the variant it was meant to be:

```text
error: `r1.assembly` holds a variant of `AssemblyMode`, but this value is text; a variant of `AssemblyMode` is not written as text: write `AssemblyMode::Hand` (the variants are `AssemblyMode::Fab`, `AssemblyMode::Hand` and `AssemblyMode::None`)
```

A module with `.assembly_mode: String` that ties a part by `.j.assembly &= .assembly_mode` is also refused, at the tie, with the same "holds a variant of `AssemblyMode`" wording; the fix is the module's declaration.

This breaks every existing `.nsm` that states assembly: every `&= "hand"`, `&= "fab"` and `&= "none"` on `.assembly` and every module that offers `.assembly_mode: String`. The std library, the examples, the guide and the tests are changed in this work. A project outside this repository changes its own lines: the edit is mechanical (`"hand"` to `AssemblyMode::Hand`, `String` to `AssemblyMode` on the module's value). A project that checks its own board against a binary that has this change will list every line in one run, because the checker reports all of them.

## What was rejected

Accepting both text and variants for one release. It would keep old projects building, and it is two spellings of one thing for as long as it lasts; the checker would have to carry text into a typed field, and the solver would see two kinds of value for one field. The message above says exactly what to write, and the number of places in a project is small. If the owner wants a release with both, it is a warning in `check_value` for a text that names a variant, in place of the error, and nothing else changes.

`AssemblyMode.Hand`, with a dot. A dot starts a member path, and a bare `AssemblyMode` inside a block is looked up outward as a member (`supply.voltage`), so `AssemblyMode.Hand` would be read as a path whose first segment is a member that does not exist. `::` is already the separator between a name and what is in it (`std::parts::passive::Resistor`) and a path never has it, so it needs no new token and no rule about what the first name is.

Bare variant names (`.assembly &= Hand`). A name with no dot is looked up outward, so a variant would share the namespace of members, nets and rails; a member called `hand` would change the meaning of a line elsewhere, and a reader could not tell a variant from a member. It would also make a field's type decide how a name is read.

A marker such as `#Hand` or `'hand'`. A new token for what `Enum::Variant` already says, and `'...'` is the quoted name of a member (`.'5'`).

A Rust built-in `AssemblyMode` known to the checker with no source. The owner asked for std to define it, and a definition that a reader can open (`std/prelude.nsm`, in the editor's tree) says more than a name that exists only in a table. The checker still reads it itself, as the one place that a built-in member needs a std type.

An import for `AssemblyMode` in every file that states `.assembly`. A part's `.assembly` is built in and needs no declaration, and an import for the type of an undeclared member is a rule with no use beyond the line itself.

Numbers or order for variants (`>=`), and a variant carrying a value. Not needed for one enum; each is a feature of its own and each would need its own decision.

## Cost and limits

Diagnostics print a variant as `AssemblyMode::Hand` only in the places that were changed: the contradiction message of the checker and the solver, and the `==` check of the solver. A message in another place that prints a domain may still print the bare name in quotes. The editor's go-to-definition does not follow a click on the enum's name inside `Enum::Variant` (it follows a type in a declaration). The qualified form `std::prelude::AssemblyMode::Hand` parses, and the checker refuses it with the existing "qualified type names are not supported yet" message, as for a qualified type in a declaration. The editor's highlighter reads `enum` as a keyword, and a capitalised name before `::` as a type.

A user enum is imported like a definition. Nothing in the language lists the variants of an enum back to a program (no iteration, no `count`), and an enum has no default variant: an unset field is unset, as for any value.

The `none` of the project's manifest and the catalog files is the variant `None`; a reader who sees both may take the capital for a different thing. The guide says so where the manifest is shown.

## What to look at

- The spelling: `enum Name { ... }` and `Name::Variant`, and that `::` is used for a value as well as for a path.
- `AssemblyMode` in scope with no import, through `std/prelude.nsm`, and its variant names (`Fab`, `Hand`, `None`; the catalog and manifest words stay lower case).
- The hard error for text, in place of a release that accepts both.
- That `==` and `&=` and `?=` take variants and `>=` and `<=` do not.
- The listing of the variants and the nearest name in the error for a wrong variant.
- ADR 0042's `.assembly_mode`, now `AssemblyMode`, and the modules in projects outside this repository that still declare it as `String`, which this change refuses.
