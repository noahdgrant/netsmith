# KiCad libraries: installed and project
status: superseded by ADR-0097
ADR 0097 replaces the requirement that `build` and `fab` find an installed KiCad; what this ADR built stays: the symbol loader, multi-unit symbols and the mapping with project libraries (headers of `crates/netsmith-kicad/src/symbols.rs` and `mapping.rs`), the pinout validation (`crates/netsmith-pcb/src/validate.rs`) and the lookup of installed libraries and of `kicad-cli` (`crates/netsmith-kicad/src/install.rs`). Full text: `git show 0eaf326:docs/adr/0021-kicad-libraries-installed-and-project.md`.
