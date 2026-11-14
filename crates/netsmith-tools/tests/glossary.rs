//! The glossary is a file for each first letter of a term, `docs/glossary/<letter>.md`, with
//! `GLOSSARY.md` as its index (issue #615), so that two pull requests that add terms rarely
//! touch the same lines. A term is in the file of its first letter, and the terms of a file are
//! in alphabetical order, so a new one goes where it sorts.

use std::fs;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// (file name, the terms of its entries in order).
fn files() -> Vec<(String, Vec<String>)> {
    let dir = root().join("docs/glossary");
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    paths.sort();
    paths
        .iter()
        .map(|path| {
            let text = fs::read_to_string(path).unwrap();
            let terms = text
                .lines()
                .filter_map(|l| l.strip_prefix("**").and_then(|l| l.strip_suffix("**:")))
                .map(str::to_string)
                .collect();
            (path.file_name().unwrap().to_string_lossy().to_string(), terms)
        })
        .collect()
}

#[test]
fn a_term_is_in_the_file_of_its_first_letter_in_alphabetical_order() {
    let files = files();
    assert!(files.len() >= 10, "found only {} files", files.len());
    for (file, terms) in &files {
        assert!(!terms.is_empty(), "docs/glossary/{file} has no entry");
        for term in terms {
            let letter = term.chars().find(|c| c.is_alphabetic()).unwrap_or('?');
            assert_eq!(
                format!("{}.md", letter.to_lowercase()),
                *file,
                "docs/glossary/{file}: `{term}` is in the file of its first letter"
            );
        }
        for pair in terms.windows(2) {
            assert!(
                pair[0].to_lowercase() <= pair[1].to_lowercase(),
                "docs/glossary/{file}: `{}` sorts before `{}`: put a term where it sorts",
                pair[1],
                pair[0]
            );
        }
    }
}

#[test]
fn the_index_links_every_file() {
    let index = fs::read_to_string(root().join("GLOSSARY.md")).expect("GLOSSARY.md exists");
    for (file, _) in files() {
        assert!(
            index.contains(&format!("](docs/glossary/{file})")),
            "GLOSSARY.md does not link docs/glossary/{file}"
        );
    }
}
