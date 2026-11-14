## How to read the examples

Every block marked `nsm` in this guide is a complete file. The test `crates/netsmith-tools/tests/guide.rs` checks each one, solves those marked `solve`, and compares every message with what the tool prints. A block marked `nsm-fragment` is a piece of a file and is not run.

To run an example, save it and use `netsmith build`:

```sh
netsmith build main.nsm --out out
```

`build` checks the file, solves it, and writes a KiCad schematic: one sheet, and a sheet for each module of the board (see A module in the KiCad schematic). In the netsmith source repository, `cargo run -p netsmith-cli -- build main.nsm` does the same, and `cargo run -p netsmith-check --example check -- main.nsm` and `cargo run -p netsmith-solve --example solve -- main.nsm` print what the checker and the solver worked out.
