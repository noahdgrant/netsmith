//! Every file of the standard library lints with no finding of the rules that read one file
//! (issue #1104): the names and the imports.

use netsmith_check::Loader;

#[test]
fn every_standard_file_lints_clean() {
    let loader = Loader::std();
    let mut found = Vec::new();
    let mut files = 0;
    for (key, name, text) in loader.modules() {
        if !key.starts_with("std::") {
            continue;
        }
        files += 1;
        let lints = netsmith_lint::lint_file(name, text, &loader).unwrap_or_else(|e| panic!("{e}"));
        found.extend(lints.iter().map(|l| l.render()));
    }
    assert!(files > 10, "only {files} standard files");
    assert!(found.is_empty(), "{}", found.join("\n"));
}
