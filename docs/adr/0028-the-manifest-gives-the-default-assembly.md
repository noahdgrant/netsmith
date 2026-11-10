---
status: accepted
---

# The manifest gives the default assembly

Status notes: Accepted on the user's decision, 2026-11-02.

Amends ADR 0024 (assembly is a field of the part). ADR 0042: a module may offer the design's statement (row 5) for its parts as a value of its own.

## Decision

`netsmith.toml` has an optional key in `[project]`:

```toml
[project]
name = "demo"
assembly = "fab"   # "fab", "hand" or "none"
```

Any other word is refused when the manifest is read, naming the key and the three allowed words. The key is the default for every part of every board of the project that nothing else speaks for. Without it the rules of ADR 0024 stay as they were: a part is the board house's (`fab`), a through-hole part is `hand`, and a part the standard library marks `none` is `none`.

The order, from the weakest statement to the strongest:

| | Says | Example |
|---|---|---|
| 1 | The built-in rule | `fab`; `hand` for `Header1x2` and any through-hole footprint |
| 2 | The manifest's `assembly` | `assembly = "hand"` in `netsmith.toml` |
| 3 | The part kind's own statement | `MountingHole`: `.assembly &= "none"` |
| 4 | The catalog row's `assembly` cell | a row with `hand` |
| 5 | The design's `.assembly &= "..."` | `.j1: Header1x2 { .assembly &= "fab" }` |

A kind that states `none` (the mechanical kinds, the programming pads) keeps it whatever the manifest says, because its definition says it is always true of the part. Rows 3 and 5 are one text field, which narrows, so a design that contradicts its kind gets the usual empty-intersection error and the order between them never decides anything. For the same reason a catalog row (4) cannot override a kind's own `none`; no standard kind has both a catalog and a stated `none`.

## How it reaches the build

`Project::catalogs()` (and so `Target::catalogs()`, which `build`, `fab`, `editor` and `compare` all call) puts the default in the `Catalogs` it returns, the solver carries it in the `Solution`, and lowering applies it. A part given its assembly by the manifest counts as stated, so a through-hole footprint no longer turns it to `hand` behind the manifest's back.

## One default for the project

There is no per-board default. A board that wants parts treated differently says so on the parts (`.assembly &=`), and one way to state a default is enough (ADR 0001).

## Considered and rejected

A part stating its own orderable identity inline (`.lcsc`, `.mpn`, ...): rejected by the user, because a catalog row also carries price, stock and a source that change and are to be refreshed mechanically; one part file plus one CSV row is intended even for a single-variant IC.
