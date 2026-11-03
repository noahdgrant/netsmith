# One correct way to say each thing

When two spellings mean the same thing, the language keeps one. A pin's identifier is `.pad` only, with no `.number` alias, and a supplied rail is a plain `Rail` rather than a source part that wraps one, because the wrapper gave every terminal two paths (`.supply.pos` and `.supply.out.pos`). This costs some typing and means no convenience aliases, in exchange for code that reads the same everywhere and a checker that has one path to follow. Ergonomic problems are fixed by removing the redundant construct or adding a better one, never with a second spelling.

Updated by ADR 0018: the plain `=` is removed too, so a constant is narrowed with `&=` and an equation is `==`.
