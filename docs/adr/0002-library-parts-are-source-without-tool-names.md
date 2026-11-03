# Library parts are written in the language and never name a CAD tool

The standard library is `.nsm` source, not built into the compiler, and it states only what is always true of a part. It holds no KiCad symbol or footprint names and no net names or values a design might need to change. Per-tool names live in separate export mappings. This keeps parts reusable when KiCad is replaced by another tool or an in-house editor, and it matters because a narrowed value cannot be loosened later, so a library default would be a permanent constraint on every design.
