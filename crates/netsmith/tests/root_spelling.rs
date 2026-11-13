//! The pick between providers does not depend on how a tool is given the name of the file to
//! build (ADR 0068 sections 4 and 11): `src/main.nsm`, `./src/main.nsm` and the absolute path
//! are one file, one module, and its providers sort under the module path `src::main`.
//! Every part and name is invented.

use std::fs;

use netsmith::outcome_with;
use netsmith::project::Target;
use netsmith_solve::Catalogs;
use netsmith_testkit::TempDir;

const RF: &str = "\
generic CoaxReceptacle {
    .designator &= \"J\"
    pins {
        .signal passive
        .shield passive
    }
}
";

const ACME: &str = "\
import src::rf::CoaxReceptacle

part AcmeMicroCoax provides CoaxReceptacle {
    pins {
        .signal passive \"S\" \"1\"
        .shield passive \"G\" \"2\"
    }
}
";

/// A provider in the file that is built, which sorts after `src::acme`.
const MAIN: &str = "\
import src::rf::CoaxReceptacle
import src::acme::AcmeMicroCoax

part ZetaCoax provides CoaxReceptacle {
    pins {
        .signal passive \"S\" \"1\"
        .shield passive \"G\" \"2\"
    }
}

board Feed {
    .rf: CoaxReceptacle
}
";

/// The part that is picked for `rf` when the file is built under `spelling` of its name.
fn picked(root: &TempDir, spelling: &str) -> String {
    let target = Target::resolve_from(root, Some(spelling)).unwrap();
    let file = &target.files[0];
    let text = fs::read_to_string(&file.path).unwrap();
    let outcome = outcome_with(
        &file.name,
        &text,
        &target.loader().unwrap(),
        &Catalogs::jlcpcb(),
        None,
    )
    .unwrap_or_else(|e| {
        panic!(
            "{spelling}: {:?}",
            e.iter().map(|p| &p.message).collect::<Vec<_>>()
        )
    });
    outcome.boards[0].design.parts[0].kind.clone()
}

#[test]
fn the_picked_provider_is_the_same_whichever_way_the_file_is_named() {
    let root = TempDir::new("root-spelling");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/rf.nsm"), RF).unwrap();
    fs::write(root.join("src/acme.nsm"), ACME).unwrap();
    fs::write(root.join("src/main.nsm"), MAIN).unwrap();
    let base = root.display().to_string();
    // `src::acme` sorts before `src::main`, so Acme is the first provider and is picked.
    for spelling in [
        format!("{base}/src/main.nsm"),
        format!("{base}/./src/main.nsm"),
        format!("{base}/src/../src/main.nsm"),
        format!("{base}//src//main.nsm"),
    ] {
        assert_eq!(picked(&root, &spelling), "AcmeMicroCoax", "{spelling}");
    }
}
