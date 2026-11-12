//! The gate of the Easyduino ESP32-S3 port: the board `netsmith fab` writes for the project in
//! `examples/easyduino_esp32s3` must match Easyduino's where the owner said it must (the
//! outline, the holes and the parts of `reference/must_match.toml`), as judged by the parity
//! checker (`netsmith::parity`, which `rp2350_parity.rs` tests against synthetic boards).
//! Everything else of the board is reported by the checker and not gated.

use std::path::PathBuf;

use crate::common;

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/easyduino_esp32s3")
}

/// How long the router may run before `fab` writes the board marked INCOMPLETE (this board
/// routes completely in about 2 s).
const ROUTE_SECONDS: &str = "20";

/// Builds the example with `netsmith fab --locked` and no KiCad (the project needs none, ADR
/// 0097) and gates on the checker: the outline, the receptacles' four pegs, J1, J2, SW1, SW2 and
/// the headers (U2) must be `ok` on the board it writes, complete or INCOMPLETE; the committed
/// overlay must be the picture of that board.
#[test]
fn easyduino_pcb_matches_the_original_where_it_must() {
    common::fab_matches_the_reference(&common::ParityGate {
        project: project(),
        tag: "easyduino-parity-fab",
        parts: 35,
        min_ok: 202,
        route_seconds: ROUTE_SECONDS,
        regenerate: "from examples/easyduino_esp32s3 run `netsmith fab --locked`, then\n  netsmith \
                     parity fab/EasyduinoEsp32s3/EasyduinoEsp32s3.kicad_pcb \
                     --table reference/placements.toml --svg \
                     reference/parity-overlay.svg",
        known_failures: &[],
    });
}
