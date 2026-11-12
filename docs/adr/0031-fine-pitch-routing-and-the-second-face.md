# Fine-pitch routing, vias in pads, and parts on the back
status: folded
Now in the header of `crates/netsmith-place-route/src/route.rs` (the router, what it tried and its choices, accepted as built by the owner, 2026-11-11), the headers of `crates/netsmith-pcb/src/anneal.rs` (busy parts) and `place.rs` (the back face and the two-face rule), the doc comment of `.via_in_pad` in `std/mfg/jlcpcb.nsm` and the test `crates/netsmith-pcb/tests/hole_faces.rs`. Full text: `git show 0eaf326:docs/adr/0031-fine-pitch-routing-and-the-second-face.md`.
