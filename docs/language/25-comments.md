## Comments

| Form | Meaning |
|---|---|
| `// text` | A comment to the end of the line. |
| `/// text` | A doc comment on the declaration or definition that follows. |
| `//! text` | A doc comment on the file (at its top) or on the definition it is inside (at the start of its body). |

```nsm
//! A file with every comment form.

import std::parts::passive::Resistor

/// A resistor on its own.
board Documented {
    //! The board has one resistor.

    // Not a doc comment.
    /// The only part.
    .r1: Resistor
}
```

Rules.

- A `///` line attaches to a declaration or a definition. On a statement it is an error.
- Inside a part, `///` goes only on a declaration (`.resistance: Resistance`) or on a definition. A `pins` block, a row of a pin table, `catalog`, `tie`, `bridge`, a relation and a connection are statements: put a `//` comment on them. The message is the one below (`doc comments can only be attached to declarations`, and for a pin table row `a pin table takes `//` notes, not doc comments`).
- `////` is a plain comment.
- There are no block comments: only `//`, `///` and `//!` exist. `/*` is an error.
- Doc comments are kept in the syntax tree and printed back. The editor's schematic draws the ones on a board, a module, a part or an interface, and every `==` relation, as notes beside what they are about (ADR 0060); a `//` comment is never drawn. The relation a `~>` chain implies (ADR 0113, ADR 0127) is drawn too, at the chain's line and marked as the chain's, unless the source writes it by hand as well.

Misuse.

```nsm fails
import std::parts::passive::Resistor

board Stray {
    .r1: Resistor
    /// about nothing
    .r1.resistance &= 1kohm +/- 1%
}
```

```text
error: doc comments can only be attached to declarations
  --> main.nsm:5:5
  |
5 |     /// about nothing
  |     ^^^^^^^^^^^^^^^^^
```

A block comment is the other misuse.

```nsm fails
import std::parts::passive::Resistor

board Block {
    .r1: Resistor /* the only part */
}
```

```text
error: block comments do not exist; use `//`
  --> main.nsm:4:19
  |
4 |     .r1: Resistor /* the only part */
  |                   ^^
```

### The layout of a file

`netsmith project fmt` writes a file in the one layout of the language (issue #1103), and the formatter's output is the way to write it (ADR 0001: one correct way). This guide, the examples and the standard library are in it, and CI checks them with `netsmith project fmt --check`. The formatter changes only the layout: the formatted file parses to the same program (the imports in another order) with the same comments and doc comments, which it checks before it writes, and formatting a formatted file changes nothing. A file that does not parse is not formatted.

The rules.

- **Indent and width.** 4 spaces for each level of a block, and 100 columns to a line. A line that goes on with the statement of the line before (after a connection or relation operator, inside parentheses, the next corners of a polygon) is indented one level more than the statement's first line.
- **Line breaks are the formatter's.** A statement or a block written over several lines is joined onto one line when it fits, as rustfmt joins them, and is broken only when it does not: `.reg: BuckConverter { .vout.voltage &= 3.3V +/- 5% }`, `.supply.pos ~> .r1 ~> .led ~> .supply.neg`. The statements of a joined block are parted by `;`. Three things are never joined. The body of a definition (`board`, `part`, `module`, `interface`, `enum`, ..., and a physical file's `physical for`) and of a section of one (`pins`, `lines`, `support`, `layout`, `defaults`, `case`) is always one statement to a line, also when it was written on one line (an empty one, `{}`, aside), as rustfmt lays out the body of a function. A statement or block with a comment, a doc comment or an attribute inside it stays broken, since a comment is never moved onto a line with code (a comment at the end of its last line stays at the end of the joined line). And a block that does not fit keeps its `{` line and its `}` line, one statement to a line, while each statement in it is joined in its turn. A `;` at the end of a line goes.
- **A line that is too long.** A line longer than 100 columns is broken, in this order. A block on the line goes over several lines, one statement to a line: this is how a long `n x kind` statement breaks, in its block and never between the count and the kind. A list in braces (a catalog's columns, an import's names) goes over several lines too, as many of its entries to a line as fit, broken after a `,`. Then a connection breaks after each of its `~`, `~>` and `?~`, every operand after the first on a line of its own; a relation breaks after its operator (`&=`, `?=`, `==`, `>=`, `<=`), and a value that is still too long after `+/-`, `+`, `-`, `*` or `/`, as late as the width allows; a polygon breaks before a corner. A newline is allowed only after an operator, so a break is never before one. A comment does not count toward the width and is never broken, and a line that has no place to break (a long name, a long string) stays long.
- **Spaces.** One space between two tokens, and around an operator. None inside brackets (`(x, y)`, `[2]`, `#[role(bias)]`, and an import's braces, `{Capacitor, Resistor}`), before `,`, `;`, `:` and `.`, around `::` and `<` `>`, or between a sign and its number (`-40C`). A block's braces on one line have a space inside them, `{ .x &= 1V }`. A range has no spaces around `..` (`0mA..500mA`); a range of rows has them, `row .pads.tp1 .. .pads.tp12`. One space after `,` and after the `:` of a member's type; a function's kind touches its name, `UART1_TX:output`. `±` is written `+/-`.
- **Tables.** The rows of a `pins` table are aligned in columns: the pin, its kind, its name and its pads (the pads one column, each pad one space from the next), with what follows one space apart. The rows of a `lines` block are aligned by the line and its direction. A blank line starts a new table.
- **Comments.** A comment at the end of a line has two spaces before it. A comment alone on a line, a `///` and a `//!` take the indent of the block they are in. The text of a comment is kept as written: the formatter never wraps or joins comments.
- **Blank lines.** At most one blank line in a row, none at the start or the end of a block. One blank line after the file's `//!` lines, after the imports, and between two definitions; the formatter adds them where they are missing.
- **Imports.** The imports at the head of the file are sorted as the editor's _Source: Sort the imports_ sorts them: the standard library's (`std::`) first, then other projects', then the project's own (`src::`), each group in the order of its paths, with one blank line between two groups and none inside one. The names in an import's braces are sorted. A comment on the lines just above an import moves with it.

A file in this layout:

```nsm
//! A rail and its decoupling.

import std::parts::passive::Capacitor
import std::sources::Rail

/// Bulk and decoupling capacitors on a 3.3 V rail.
board Laid {
    .supply: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "3V3" }
    .decoupling: 4 x Capacitor { .capacitance &= 100nF +/- 10% }  // one by each supply pin
    .bulk: 2 x Capacitor {
        .capacitance &= 22uF +/- 10%
        .package &= "CASE-A-3216-18"
        .max_voltage >= 6.3V
    }

    .supply.pos ~> .decoupling ~> .supply.neg
    .supply.pos ~> .bulk ~> .supply.neg
}
```

The rail `.supply` may be written over three lines, and the formatter joins it. The group `.bulk` on one line, `.bulk: 2 x Capacitor { .capacitance &= 22uF +/- 10%; .package &= "CASE-A-3216-18"; .max_voltage >= 6.3V }`, is 109 columns at its indent: the formatter gives its block one line to each statement, however it was written. A connection too long for one line is broken after its operators:

```nsm-fragment
    .usb_connector.vbus.pos ~>
        .input_protection_fuse ~>
        .reverse_polarity_protection_diode ~>
        .input_bulk_capacitor_positive_terminal
```
