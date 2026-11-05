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
    out
}
