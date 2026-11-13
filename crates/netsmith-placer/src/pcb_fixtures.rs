//! The placed boards `netsmith-pcb`'s tests start from (`crates/netsmith-pcb/tests/fixtures`),
//! kept in step with the placer and the board model by this test.
//!
//! `netsmith-pcb` cannot depend on the placer, even in its tests, so its tests read a committed
//! `Board` (JSON) in place of placing a design (issue #608). Each fixture is the board the
//! placer makes of one of this crate's test designs with the standard mapping and the sample
//! footprints. When the placer, the board model or a design here changes and the fixtures no
//! longer match, this test fails and says how to write them again:
//!
//! ```text
//! NETSMITH_UPDATE_FIXTURES=1 cargo test -p netsmith-placer pcb_fixtures
//! ```
//!
//! then review the diff of the files.

use std::path::PathBuf;

use netsmith_core::Design;
use netsmith_pcb::Board;

use crate::board_from_design;
use crate::testlib::{fixtures, four_layer_design, led_design, six_layer_design, ten_layer_design};

fn folder() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../netsmith-pcb/tests/fixtures")
}

/// The fixture of `design`, as the placer makes it.
fn placed(design: &Design) -> String {
    let board: Board = board_from_design(design, &netsmith_kicad::Mapping::standard(), &fixtures())
        .unwrap_or_else(|e| panic!("{e}"));
    let mut text = serde_json::to_string(&board).unwrap();
    text.push('\n');
    text
}

#[test]
fn the_boards_in_netsmith_pcbs_fixtures_are_what_the_placer_makes_of_these_designs() {
    let update = std::env::var_os("NETSMITH_UPDATE_FIXTURES").is_some();
    let mut stale = Vec::new();
    for (name, design) in [
        ("led", led_design()),
        ("four_layer", four_layer_design()),
        ("six_layer", six_layer_design()),
        ("ten_layer", ten_layer_design()),
    ] {
        let path = folder().join(format!("{name}.board.json"));
        let made = placed(&design);
        if update {
            std::fs::create_dir_all(folder()).unwrap();
            std::fs::write(&path, &made).unwrap();
        } else if std::fs::read_to_string(&path).ok().as_deref() != Some(made.as_str()) {
            stale.push(name);
        }
    }
    assert!(
        stale.is_empty(),
        "the fixtures {stale:?} of netsmith-pcb differ from what the placer makes: write them again with \
         `NETSMITH_UPDATE_FIXTURES=1 cargo test -p netsmith-placer pcb_fixtures` and review the diff"
    );
}
