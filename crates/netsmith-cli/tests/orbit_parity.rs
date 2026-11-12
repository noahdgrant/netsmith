//! The PCB that `netsmith fab` writes for ORBIT (`examples/orbit/`) against the original's, as
//! the parity checker (`netsmith::parity`) judges it: the gate of the port's board stage. The
//! checker itself is tested in `rp2350_parity.rs`, on the RP2350A Minimal; this file only builds
//! ORBIT and gates on the must-match items of `reference/must_match.toml`.

use std::path::PathBuf;

use crate::common;

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/orbit")
}

/// The longest `fab` may route in the test, in seconds (see `rp2350_parity.rs`): the board file is
/// written either way, marked INCOMPLETE when the router is stopped, and the checker reads the
/// places of the parts, which the placer fixes before the router starts.
const ROUTE_SECONDS: &str = "20";

/// Builds ORBIT with `netsmith fab --locked` and no KiCad (the project needs none, ADR 0097) and
/// gates on the checker: the outline, the four plated holes, USB1, BAT1, J1, J2, SW1, SW2, BOOT0
/// and PWR1 must be `ok` on the board it writes, complete or INCOMPLETE; the committed overlay
/// must be the picture of that board.
#[test]
fn orbit_pcb_matches_the_original() {
    common::fab_matches_the_reference(&common::ParityGate {
        project: project(),
        tag: "orbit-parity-fab",
        parts: 48,
        min_ok: 129,
        route_seconds: ROUTE_SECONDS,
        regenerate: "from examples/orbit run `netsmith fab --locked`, then\n  netsmith \
                     parity fab/Orbit/Orbit.kicad_pcb --table \
                     reference/placements.toml --map reference/designators.toml --svg \
                     reference/parity-overlay.svg",
        known_failures: &[],
    });
}
