//! The gate of the Power Manifold charger module port: the board `netsmith fab` writes for the
//! project in `examples/power_manifold_charger` must match the original where the owner said it
//! must (the holes and the parts of `reference/must_match.toml`: the programming pads J2, the
//! card-edge connector J3 and the USB-C receptacle J4), as judged by the parity checker
//! (`netsmith_tools::parity`). The outline is the one known exception: the original's is a rectangle
//! with a card tab and a key slot (14 lines and an arc) and the language's `Outline` is a rounded
//! rectangle (README, "The PCB"). Everything else of the board is reported and not gated.

use std::path::PathBuf;

use crate::common;

fn project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/power_manifold_charger")
}

/// How long the router may run before `fab` writes the board marked INCOMPLETE (the board is
/// dense and does not route completely in this time; the parts' places do not depend on it).
const ROUTE_SECONDS: &str = "20";

/// Builds the example with `netsmith fab --locked` and no KiCad (the project needs none, ADR
/// 0097) and gates on the checker: the holes, J2, J3 and J4 must be `ok` on the board it writes,
/// complete or INCOMPLETE; the outline's corner radius and elements are the two known failures;
/// the committed overlay must be the picture of that board.
#[test]
fn power_manifold_pcb_matches_the_original_where_it_can() {
    common::fab_matches_the_reference(&common::ParityGate {
        project: project(),
        tag: "power-manifold-parity-fab",
        parts: 89,
        min_ok: 144,
        route_seconds: ROUTE_SECONDS,
        regenerate: "from examples/power_manifold_charger run `netsmith fab --locked`, then\n  netsmith \
                     parity fab/PowerManifoldCharger/PowerManifoldCharger.kicad_pcb \
                     --table reference/placements.toml --svg \
                     reference/parity-overlay.svg",
        known_failures: &["outline corner radius", "outline elements"],
    });
}
