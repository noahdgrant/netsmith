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
