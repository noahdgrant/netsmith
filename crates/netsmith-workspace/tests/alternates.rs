//! The bill of materials of a board with an `alternate` part (issue 1312): the part the board is
//! drawn with is on its line, and the alternate is listed not fitted (DNP), on the same
//! designator. Every part here is invented; no test reaches a supplier.

use netsmith_testkit::TempDir;
use std::fs;

use netsmith_editor::Backend;
use netsmith_fabricate::project::Target;
use netsmith_fabricate::refresh::Network;
use netsmith_workspace::Workspace;

const SOURCE: &str = r#"import std::parts::connector::Header1x2
import std::sources::Rail

part ModuleA {
    .vcc: Rail
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
    }
}

part ModuleB {
    .variants {
        "MOD-B"
    }
    .vcc: Rail
    pins {
        .vcc.pos power "VCC" "1"
        .vcc.neg power "GND" "2"
    }
}

board Tracker {
    .input: Rail { .voltage &= 3.3V +/- 5%; .pos.net &= "VCC"; .neg.net &= "GND" }
    .j: Header1x2
    .input.pos ~ .j.p1
    .input.neg ~ .j.p2
    .u: ModuleA { .assembly &= AssemblyMode::Hand }
    alternate .u: ModuleB { .assembly &= AssemblyMode::Hand }
    .input ~ .u.vcc
}
"#;

#[test]
fn the_alternate_is_listed_not_fitted_on_the_same_designator() {
    let root = TempDir::new("alternates-bom");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"tracker\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), SOURCE).unwrap();
    let editor = Workspace::open(Target::resolve_from(root.path(), None).unwrap())
        .unwrap()
        .with_network(Network {
            now: Some(1_793_620_800),
            offline: true,
            cache_dir: None,
            ..Network::default()
        });
    let analysis = editor.analysis();
    assert_eq!(analysis.errors, 0, "{:?}", analysis.problems);
    let bom = editor.bom(&analysis.boards[0].name).unwrap();
    let fitted = bom
        .lines
        .iter()
        .chain(&bom.hand)
        .find(|l| l.kind == "ModuleA")
        .unwrap_or_else(|| panic!("ModuleA is on a line: {bom:?}"));
    let [dnp] = bom.not_fitted.as_slice() else {
        panic!("one line not fitted: {:?}", bom.not_fitted)
    };
    assert_eq!(dnp.kind, "ModuleB");
    assert_eq!(dnp.mpn.as_deref(), Some("MOD-B"));
    assert_eq!(dnp.designators, fitted.designators);
    assert!(
        !bom.lines.iter().chain(&bom.hand).any(|l| l.kind == "ModuleB"),
        "the alternate is on no line that is fitted: {bom:?}"
    );
}
