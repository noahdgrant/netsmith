//! Every board the tests draw, as one list, so a rule that must hold for all of them is
//! tested on all of them: the three example boards, supplies of several modules (sheets
//! with nets between them), the bench board and a part of thirty pins with hidden
//! duplicates. Each is drawn with and without its rails told.

#![allow(dead_code)]

use netsmith_core::Design;
use netsmith_kicad::testlib::{self, TempDir};
use netsmith_kicad::{Mapping, SymbolLib};
use netsmith_view::layout::Layout;
use netsmith_view::sheets::layout_sheets_rails;
use netsmith_view::{Group, Rails};

pub struct Case {
    pub name: String,
    pub design: Design,
    pub groups: Vec<Group>,
    pub rails: Rails,
    pub mapping: Mapping,
    pub lib: SymbolLib,
    /// Keeps the symbol libraries until the test ends.
    pub dir: Option<TempDir>,
}

impl Case {
    /// The board drawn as the editor draws it: rails told, sheets by module.
    pub fn drawn(&self) -> Layout {
        layout_sheets_rails(
            &self.design,
            &self.groups,
            &self.rails,
            &self.mapping,
            &self.lib,
        )
        .unwrap()
    }

    /// The same with nothing known about rails.
    pub fn drawn_plain(&self) -> Layout {
        layout_sheets_rails(
            &self.design,
            &self.groups,
            &Rails::default(),
            &self.mapping,
            &self.lib,
        )
        .unwrap()
    }
}

fn rails(ground: &[&str], positive: &[&str]) -> Rails {
    let mut r = Rails::default();
    r.ground.extend(ground.iter().map(|s| s.to_string()));
    r.positive.extend(positive.iter().map(|s| s.to_string()));
    r
}

fn sample(design: Design, rails: Rails) -> Case {
    Case {
        name: design.name.clone(),
        design,
        groups: Vec::new(),
        rails,
        mapping: Mapping::standard(),
        lib: SymbolLib::at(testlib::sample_symbols()),
        dir: None,
    }
}

pub fn corpus() -> Vec<Case> {
    let mut out = vec![
        sample(super::led(), rails(&["GND"], &["VCC"])),
        sample(super::status_indicator(), Rails::default()),
        sample(super::regulator(), rails(&["GND"], &["VIN", "3V3"])),
    ];
    for modules in [3, 7] {
        let (design, groups) = super::supplies(modules);
        out.push(Case {
            name: format!("Supplies{modules}"),
            design,
            groups,
            rails: rails(&["GND"], &["VIN"]),
            mapping: Mapping::standard(),
            lib: SymbolLib::at(testlib::sample_symbols()),
            dir: None,
        });
    }
    let b = super::bench::bench();
    out.push(Case {
        name: "Bench".into(),
        design: b.design,
        groups: b.groups,
        rails: b.rails,
        mapping: b.mapping,
        lib: b.lib,
        dir: Some(b.dir),
    });
    let (dir, mapping, lib) = super::mcu::setup();
    out.push(Case {
        name: "Mcu30".into(),
        design: super::mcu::board(),
        groups: Vec::new(),
        rails: rails(&["GND"], &["SUPPLY"]),
        mapping,
        lib,
        dir: Some(dir),
    });
    // Two parts joined by a UART, as one drawing, and as two sheets: the buses.
    for (name, caps) in [("UartBoard", 0), ("UartSheets", 8)] {
        let (dir, mapping, lib) = super::bus::setup();
        let (design, groups) = super::bus::board(4, caps);
        out.push(Case {
            name: name.into(),
            design,
            groups: if caps == 0 { Vec::new() } else { groups },
            rails: super::bus::rails(),
            mapping,
            lib,
            dir: Some(dir),
        });
    }
    // The same boards with notes (doc comments and a relation): every rule holds with them too.
    let mut led = super::led();
    led.name = "LedNotes".into();
    note_paths(&mut led, None);
    led.notes = notes(&["j1", "r1", "d1"], "");
    out.push(sample(led, rails(&["GND"], &["VCC"])));
    let (mut design, groups) = super::supplies(3);
    note_paths(&mut design, Some(&groups));
    design.notes = notes(&["supply1.u1", "supply2.r2", "supply3.d3"], "supply2");
    out.push(Case {
        name: "SuppliesNotes".into(),
        design,
        groups,
        rails: rails(&["GND"], &["VIN"]),
        mapping: Mapping::standard(),
        lib: SymbolLib::at(testlib::sample_symbols()),
        dir: None,
    });
    out
}

/// Paths for the parts of a test design: the lower-case reference, under the name of the group
/// that holds it when there are groups.
fn note_paths(design: &mut Design, groups: Option<&[Group]>) {
    for p in &mut design.parts {
        p.path = p.refdes.to_lowercase();
    }
    for g in groups.unwrap_or_default() {
        for &p in &g.parts {
            design.parts[p].path = format!("{}.{}", g.name, design.parts[p].path);
        }
    }
}

/// A doc comment on each of `parts`, a relation across the first two, the board's doc comment
/// and, when `module` is not empty, that module's.
fn notes(parts: &[&str], module: &str) -> Vec<netsmith_core::Note> {
    use netsmith_core::{Note, NoteAt, NoteKind};
    let at = |line| {
        Some(NoteAt {
            file: "board.nsm".into(),
            line,
        })
    };
    let mut out = vec![Note {
        path: String::new(),
        kind: NoteKind::Doc,
        text: "The test board: every note of it is drawn without lying on anything.".into(),
        at: at(1),
        anchors: Vec::new(),
        result: None,
    }];
    if !module.is_empty() {
        out.push(Note {
            path: module.into(),
            kind: NoteKind::Doc,
            text: "A supply with a regulator, a divider and a light.".into(),
            at: at(5),
            anchors: Vec::new(),
            result: None,
        });
    }
    for (i, p) in parts.iter().enumerate() {
        out.push(Note {
            path: (*p).into(),
            kind: NoteKind::Doc,
            text: format!("The part `{p}` is the one this note is about; it says why it is there in a sentence or two of text that wraps."),
            at: at(10 + i),
            anchors: vec![(*p).into()],
            result: None,
        });
    }
    out.push(Note {
        path: String::new(),
        kind: NoteKind::Relation,
        text: format!(".{}.value == .{}.value / 10", parts[0], parts[1]),
        at: at(30),
        anchors: parts[..2].iter().map(|p| p.to_string()).collect(),
        result: Some("x = 330Ω..340Ω".into()),
    });
    out
}
