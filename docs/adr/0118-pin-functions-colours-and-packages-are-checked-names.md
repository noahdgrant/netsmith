---
status: accepted
---

# Pin functions, colours and packages are checked names, not free text

The owner decided on 2026-11-13 (issue #627, epic #638) that values with a fixed set of answers are names the checker knows: a pin's function is a bare name checked against the part's pin table, an LED's colour is an enum in std, and a package stays text where its name has dashes or parentheses but is checked against the part's tables. Each misspelling is an error at its line with "did you mean". Before, `.mcu.pa9.function &= "USART1_TX"` and `.color &= "RED"` were text, and a wrong package was found by the solver with no row, or by the exporter.

## Decision

- **Function.** `.u.pa9.function &= USART1_TX`: on the right of `&=`, `?=` or `==` beside the `function` of a pin, a bare name is the function, as the pin table writes it after `fn` (functions were already plain names there, ADR 0016). A function the pin does not offer is refused at the statement with the pin's functions (`crates/netsmith-check/src/faults.rs`); a misspelling names the nearest function of the pin, else of the part, by the checker's `closest`; a function of another pin says which pins offer it. A module passes a choice on with `.mcu.pa9.function == .tx.function`, as before.
- **Colour.** `std::parts::passive` declares `enum Color { Red, Green, Blue, White, Yellow }`, the colours the catalogs have; a colour is added with the rows that have it, as `Dielectric` is (ADR 0055). `Led`'s `.color` has that type, and `.led: Led { .color &= Red }` needs no import. A catalog's `color` column holds the variant's name (`Red`), as every enum column does; the rows of `data/jlcpcb/catalog/led.csv` and of the example catalogs changed from `RED` to `Red`, and no pick moved.
- **A bare variant beside a field of an enum.** The colour is not a special case: on the right of `&=`, `?=` or `==` whose other side is a field of an enum, a bare name that is a variant of that enum is the variant (`lower_beside` in `crates/netsmith-check/src/lower.rs`). The field says which enum it is, so two enums may still share a variant name. A bare name that is no variant is a member of an enclosing block, as a bare name always was; with no such member it is refused with the variants and the nearest.
- **Package.** A package stays quoted text. A bare form for simple names was rejected: `"0603"` is not a name, so packages would have two spellings by their shape. The check is in two places, each at the statement that names the package with the nearest name: the checker refuses a package that is none of the part's `pins "..."` headers, and the solver refuses a package that no row of the part's catalog is in, before any row is tried (`unknown_package` in `crates/netsmith-solve/src/solve.rs`). A catalog whose rows do not all name a package is not judged there.
- **Old spellings are errors, not silent changes.** A quoted function says `write USART1_TX, without quotes`; a quoted colour says `write Red` with the variants listed (the message every enum gives text, now naming the bare variant). One run of `netsmith build` lists every line to change.

## Rejected

- A `Function` type per part, so that `.function` is an enum: a pin's functions differ by pin and by peripheral class (ADR 0071), and a type per part would duplicate the pin table.
- Quoted functions kept as a second spelling: one correct way (`docs/DESIGN-RULES.md`).
- `Color::Red` only: the owner chose the bare form; ADR 0054 rejected bare variants because a reader could not tell them from members, and the field beside the name settles that here.
- Checking a package against `kicad.toml`: the mapping is the exporter's and a project may have none for a generated footprint (ADR 0104); its own error stays where it is.

## Cost

A bare name on the right of a function or an enum field now means something other than a member of an outer block when it is a variant or a function. The pin-table check of a package runs only for a part with tables per package, and the catalog check only for a part with a catalog: a mounting hole or a fiducial whose package is a footprint name is checked by the exporter, as before. A project catalog with `RED` in its `color` column is refused until it writes `Red`.

## Proposed, for the owner

Not settled by the decision, built provisionally:

1. Bare variants work for every enum field, and `Enum::Variant` stays accepted, so `AssemblyMode::Hand` and `Hand` are two spellings of one value. Proposed: retire `Enum::Variant` where a field gives the type and migrate `AssemblyMode`, `Dielectric`, `Qualification` and the bus roles, which supersedes the spelling of ADR 0054; or keep the bare form for `Color` only.
2. The variants of `Color`: the five the catalogs have, each catalog's `GREEN` (the yellow-green and the emerald green) being `Green`.
3. A quoted function keeps the pin read as having that function, so the migration error is the only message about the line.

## Left open

A pin's function in the pins file of ADR 0101 (`<stem>.pins.toml`) stays text: it is not source.
