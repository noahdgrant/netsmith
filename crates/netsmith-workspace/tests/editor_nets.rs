//! The table of the board's nets and diff pairs in the editor's PCB summary tab (issue #1223):
//! `GET /api/pcb/nets` answers what `netsmith board nets` prints, of the board as the PCB view
//! has it placed (and routed).

use std::fs;
use std::sync::Arc;

use netsmith_editor::{Backend, Say};
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;

/// Two headers joined pin to pin, the two lines stated a diff pair with a `DiffPair`.
const LINK: &str = r#"import std::mfg::jlcpcb::TwoLayer
import std::parts::connector::Header1x2
import std::sources::DiffPair

board Link {
    .process: TwoLayer
    .process.stackup.thickness &= 1.6mm
    .near: Header1x2
    .far: Header1x2
    .lines: DiffPair
    .near.p1 ~ .far.p1
    .near.p2 ~ .far.p2
    .lines.p ~ .near.p1
    .lines.n ~ .near.p2
}
"#;

#[test]
fn the_table_of_the_routed_board_has_both_nets_and_the_pair_within_its_skew() {
    let root = TempDir::new("editor-nets");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"nets\"\n").unwrap();
    fs::write(root.join("src").join("main.nsm"), LINK).unwrap();
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    // Placed and not routed: no copper.
    let bare = editor.pcb_nets("Link").unwrap();
    assert_eq!(bare.board, "Link");
    assert!(!bare.routed && bare.nets.is_empty() && bare.pairs.is_empty());
    let say: Say = Arc::new(|_: &str| {});
    editor.route("Link", &say).unwrap();
    let nets = editor.pcb_nets("Link").unwrap();
    assert!(nets.routed);
    assert_eq!(nets.nets.len(), 2, "{nets:?}");
    assert_eq!(nets.pairs.len(), 1, "{nets:?}");
    let pair = &nets.pairs[0];
    println!("{pair:?}");
    let length = |net: &str| nets.nets.iter().find(|n| n.net == net).unwrap().length;
    assert!((length(&pair.p) - pair.length_p).abs() < 1e-9);
    assert!((length(&pair.n) - pair.length_n).abs() < 1e-9);
    assert!(!pair.over && pair.skew <= pair.limit, "{pair:?}");
    assert!((pair.coupled - pair.share * pair.length_p).abs() < 1e-6, "{pair:?}");
    assert_eq!(editor.pcb_nets("Nope").unwrap_err().status, 404);
}
