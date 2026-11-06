//! The rules every schematic drawing keeps (`netsmith_view::check`), on the example boards as
//! the editor draws them with KiCad's own symbols: the drawing is the board's netlist, no wire
//! runs along another net's wire, no wire goes through a symbol's body, no wire is drawn that
//! serves nothing (an end in mid-air, a double line, a hook, a dot without three arms) and no
//! two texts overlap. KiCad's symbol libraries are needed, so these skip themselves where
//! KiCad is not installed.

use std::path::PathBuf;

use netsmith::editor::schematic_layout;
use netsmith::project::Target;
use netsmith::{Libraries, outcome_with};
use netsmith_view::check::check;

fn examples() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

/// Every board of every example (a single file or a project), as `(name, violations)`.
fn violations(kinds: &[&str]) -> Option<Vec<String>> {
    let mut out = Vec::new();
    drawings(|built, layout| {
        for v in check(&built.design, layout) {
            if kinds.contains(&v.kind()) {
                out.push(format!("{}: {v}", built.design.name));
            }
        }
    })?;
    Some(out)
}

/// Draws every board of every example as the editor does and hands each drawing to `f`; none
/// where KiCad's symbols are not installed.
fn drawings(mut f: impl FnMut(&netsmith::Built, &netsmith_view::Layout)) -> Option<()> {
    let libraries = Libraries::standard();
    if !libraries.symbols.root().is_dir() {
        eprintln!(
            "skipped: no KiCad symbols at {}",
            libraries.symbols.root().display()
        );
        return None;
    }
    let mut targets = Vec::new();
    for name in [
        "led",
        "led_panel",
        "linear_regulator",
        "status_indicator",
        "four_layer",
        "two_boards",
        "outline_and_pours",
        // Its invented parts have no KiCad symbol: they are drawn as boxes.
        "carrier_and_cards",
    ] {
        let path = examples().join(format!("{name}.nsm"));
        targets.push(Target::resolve_from(
            &examples(),
            Some(&path.to_string_lossy()),
        ));
    }
    for project in ["schematic_bench", "enclosure_sensor"] {
        targets.push(Target::resolve_from(&examples().join(project), None));
    }
    for target in targets {
        let target = target.unwrap_or_else(|e| panic!("{e}"));
        let (loader, catalogs) = (target.loader().unwrap(), target.catalogs().unwrap());
        let root = target.project.as_ref().map(|p| p.root.clone());
        let libraries = Libraries::for_project(root.as_deref()).unwrap_or_else(|e| panic!("{e}"));
        for source in &target.files {
            let text = std::fs::read_to_string(&source.path).unwrap();
            let outcome = outcome_with(&source.name, &text, &loader, &catalogs, None)
                .unwrap_or_else(|p| panic!("{}: {}", source.name, p[0].rendered));
            for built in &outcome.boards {
                let layout = schematic_layout(built, &libraries).unwrap();
                f(built, &layout);
            }
        }
    }
    Some(())
}

#[test]
fn every_example_drawing_is_its_netlist() {
    let Some(found) = violations(&[
        "short",
        "open",
        "extra-pin",
        "pin-net",
        "floating-wire",
        "floating-flag",
    ]) else {
        return;
    };
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn no_example_has_a_wire_along_another_nets_wire() {
    let Some(found) = violations(&["wire-overlap"]) else {
        return;
    };
    assert!(found.is_empty(), "{}", found.join("\n"));
}

// Fixed in stage 2 (2026-11-05): `examples/enclosure_sensor`, board `EnclosureSensor`: the
// 16-pin connector `J2` has its pins on top, and the wire of pin 13 (`net-pads.tp9.p1`) to its
// flag, which stands in a column below everything, ran straight down through the connector's
// body, because the flag was outside the router's grid. See
// `crates/netsmith-view/tests/flags.rs` for the smallest case.
#[test]
fn no_example_has_a_wire_through_a_symbol() {
    let Some(found) = violations(&["wire-through-body"]) else {
        return;
    };
    assert!(found.is_empty(), "{}", found.join("\n"));
}

// Found in stage 4 (2026-11-05) on a private board of the owner's, whose rail ran past its last
// tap: an end of a wire that touches no pin, symbol, flag, label or other wire is a fault the
// check could not name before. Seen on these examples too: `status_indicator` (two dead ends and
// a missing dot), `two_boards` (a dead end), `carrier_and_cards` (five dead ends) and
// `schematic_bench` (eight dead ends, four double lines, three missing dots).
#[test]
fn no_example_has_a_wire_that_serves_nothing() {
    let Some(found) = violations(&[
        "dead-end",
        "redundant-wire",
        "zero-length-wire",
        "u-turn",
        "unused-junction",
        "missing-junction",
        "wire-along-pin",
        "wire-hugs-symbol",
    ]) else {
        return;
    };
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn no_example_has_two_texts_over_each_other_or_a_wire_over_a_text() {
    let Some(found) = violations(&["text-overlap", "wire-over-text"]) else {
        return;
    };
    assert!(found.is_empty(), "{}", found.join("\n"));
}

// Notes (ADR 0052): the doc comments and `==` relations of the examples are drawn as boxes of
// text, on the editor's own path (the checker's model, lowering, `schematic_layout`). A note lies
// on no symbol, text, wire, flag or other note, and its text stays in its frame.
#[test]
fn no_example_has_a_note_on_anything_or_text_out_of_its_frame() {
    let Some(found) = violations(&["note-overlap", "note-text-overflow"]) else {
        return;
    };
    assert!(found.is_empty(), "{}", found.join("\n"));
}

#[test]
fn the_examples_draw_the_doc_comments_and_relations_they_state() {
    let mut seen: Vec<(String, usize, usize, usize)> = Vec::new();
    let Some(()) = drawings(|built, layout| {
        use netsmith_core::NoteKind;
        // What the design carries is what is drawn, once each.
        assert_eq!(
            built.design.notes.len(),
            layout.notes.len(),
            "{}",
            built.design.name
        );
        let docs = layout
            .notes
            .iter()
            .filter(|n| n.kind == NoteKind::Doc)
            .count();
        let relations = layout.notes.len() - docs;
        seen.push((
            built.design.name.clone(),
            layout.notes.len(),
            docs,
            relations,
        ));
        // Nothing from the standard library: a note says where it is written.
        for n in &built.design.notes {
            let at = n.at.as_ref().expect("a note says where it is written");
            assert!(!at.file.starts_with("std/"), "{}: {n:?}", built.design.name);
        }
    }) else {
        return;
    };
    let find = |name: &str| {
        seen.iter()
            .find(|s| s.0 == name)
            .unwrap_or_else(|| panic!("{name}: {seen:?}"))
    };
    // `led.nsm` has a doc comment on its header and one relation; `LedExample` has nothing else.
    let led = find("LedExample");
    assert_eq!((led.2, led.3), (1, 1), "{seen:?}");
    // A module's doc comment is the sheet's note, a doc comment on a rail is a note too.
    assert!(find("SchematicBench").2 >= 7, "{seen:?}");
}
