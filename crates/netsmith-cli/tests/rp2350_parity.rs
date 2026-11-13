//! The parity checker of the RP2350A Minimal port (`netsmith_tools::parity`): small boards written as
//! KiCad text stand for what `fab` writes, and the checker must pass a copy of the reference's
//! must-match items and fail, naming the item, each way that a board can differ. One test runs
//! the checker on Raspberry Pi's own board against itself, which proves that it reads a real
//! KiCad 10 file. One more builds the example with `fab` and gates on the checker.

use std::path::PathBuf;

use crate::common;

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/rp2350_minimal")
}

// ---- review of the checker (#372): each way a board that is not interchangeable passed -------

/// The longest `fab` may route in the test, in seconds. The router needs 35 s to 100 s on an idle
/// machine to finish this board (README, "The result"), and the tests of a slice share the CPU, so
/// the test does not wait for it: the board file is written either way, marked INCOMPLETE when
/// the router is stopped, and the checker reads the places of the parts, which the placer fixes
/// before the router starts. Placement takes about 3 s.
const ROUTE_SECONDS: &str = "20";

/// Builds the example with `netsmith fab`, with no KiCad (the project needs none, ADR 0097) and
/// the router stopped after [`ROUTE_SECONDS`], and gates on the checker: every must-match item
/// must be `ok` on the board it writes, complete or INCOMPLETE. Any other failure of `fab` (no
/// board written, a requirement not met, a design that does not build) fails the test.
#[test]
fn rp2350_minimal_pcb_matches_raspberry_pi() {
    common::fab_matches_the_reference(&common::ParityGate {
        project: project(),
        tag: "rp2350-parity-fab",
        parts: 43,
        min_ok: 225,
        route_seconds: ROUTE_SECONDS,
        regenerate: "from examples/rp2350_minimal run `netsmith fab --locked`, then\n  netsmith \
                     parity fab/Rp2350Minimal/Rp2350Minimal.kicad_pcb \
                     --svg reference/parity-overlay.svg",
        known_failures: &[],
    });
}
