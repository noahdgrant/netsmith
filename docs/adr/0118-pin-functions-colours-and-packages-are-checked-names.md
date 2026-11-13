---
status: accepted
---

# Pin functions, colours and packages are checked names, and a variant follows the import system

The owner decided on 2026-11-13 (issue #627, epic #638, pull request #684) that values with a fixed set of answers are names the checker knows. A pin's function is a bare name checked against the part's pin table. An LED's colour is an enum in std. A package stays text but is checked against the part's tables. Each misspelling is an error at its line with "did you mean". Before, `.mcu.pa9.function &= "USART1_TX"` and `.color &= "RED"` were text, and a wrong package was found by the solver with no row, or by the exporter.

## Decision

- **Function.** `.u.pa9.function &= USART1_TX`: on the right of `&=`, `?=` or `==` beside the `function` of a pin, a bare name is the function, as the pin table writes it after `fn` (ADR 0016). It is a name of the part's pin table and not an enum variant, so no import brings it in. A function the pin does not offer is refused at the statement with the pin's functions (`crates/netsmith-check/src/faults.rs`). A misspelling names the nearest function of the pin, else of the part, by the checker's `closest`. A function of another pin says which pins offer it.
- **Colour.** `std::parts::passive` declares `enum Color { Red, Green, Blue, White, Yellow }`, the colours the catalogs have. The yellow-green and the emerald-green rows are both `Green`. A colour is added with the rows that have it, as `Dielectric` is (ADR 0055). A catalog's `color` column holds the variant's name (`Red`), as every enum column does: the rows of `data/jlcpcb/catalog/led.csv` and of the example catalogs changed from `RED` to `Red`, and no pick moved.
- **A variant follows the import system, like any other name.** The same rule holds for every enum (`Color`, `AssemblyMode`, `Dielectric`, the bus roles):
  - It is written `Color::Red` where the enum is in scope: declared in the file, imported, or in the prelude.
  - It is written bare only where the file imports the variant itself: `import std::parts::passive::Color::Red`, or several with the braces the import already has, `import std::parts::passive::Color::{Red, Green}`. There is no glob import.
  - An import resolves `a::b::Color::Red` as a variant when `a::b::Color` is no module and `a::b` declares an enum `Color` (`apply_import` in `crates/netsmith-check/src/checker.rs`). A variant the enum lacks is refused at the import, with the nearest.
  - A bare name that is not imported is a member of an enclosing block, as always. Where there is none, the error names the qualified form and the import (`` write `Color::Red`, or import the variant: `import std::parts::passive::Color::Red` ``), for the nearest variant of the field's enum, or of an enum the file sees.
- **Package.** A package stays quoted text. A bare form for simple names was rejected: `"0603"` is not a name, so packages would have two spellings by their shape. It is checked in two places, each at the statement that names the package, with the nearest name:
  - the checker refuses a package that is none of the part's `pins "..."` headers;
  - the solver refuses a package that no row of the part's catalog is in, before any row is tried (`unknown_package` in `crates/netsmith-solve/src/solve.rs`).
- **Old spellings are errors, not silent changes.** A quoted function says `write USART1_TX, without quotes`, and the pin is still read as having that function, so that message is the only one about the line. A quoted colour says `write Color::Red`, the message every enum gives text (ADR 0054). One run of `netsmith build` lists every line to change.

## Rejected

- A bare name read as a variant of the enum of the field beside it (built first in #684): a second scoping rule, with a bare name meaning one thing beside an enum field and another elsewhere. The import rule is the one names already have.
- A `Function` type per part, so that `.function` is an enum: a pin's functions differ by pin and by peripheral class (ADR 0071), and a type per part would duplicate the pin table.
- Quoted functions kept as a second spelling: one correct way (`docs/DESIGN-RULES.md`).
- Checking a package against `kicad.toml`: the mapping is the exporter's, and a project may have none for a generated footprint (ADR 0104). Its own error stays where it is.

## Cost

A file that states a colour imports `Color` (`import std::parts::passive::{Led, Color}`). A file can name a variant bare in two places only: with its own import, or as a member of an enclosing block of that name; the import wins. The pin-table check of a package runs only for a part with tables per package, and the catalog check only for a part with a catalog. A mounting hole or a fiducial whose package is a footprint name is checked by the exporter, as before. A project catalog with `RED` in its `color` column is refused until it writes `Red`. The editor's go-to-definition does not follow an import of a variant.

## Left open

A pin's function in the pins file of ADR 0101 (`<stem>.pins.toml`) stays text: it is not source.
