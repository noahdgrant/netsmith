//! The netsmith release a project is written for (`netsmith = "0.2.0"` in `netsmith.toml`), and
//! what a command says when a project written for another release fails its check (issue #740,
//! ADR 0134).
//!
//! A release may change how a board spells what it already wrote (ADR 0001 keeps one spelling,
//! with no grace period), so a project that built on an earlier release can fail with many errors.
//! Each error names the spelling to write; [`note`] adds the one line that says why there are so
//! many and where every change is listed: `docs/MIGRATING.md`.

/// The release of this netsmith, the one `netsmith project init` writes into a new manifest.
pub const RELEASE: &str = env!("CARGO_PKG_VERSION");

/// Where the changes that need an edit of a project's source are listed, release by release.
pub const MIGRATING: &str = "https://github.com/noahdgrant/netsmith/blob/main/docs/MIGRATING.md";

/// A release number, `major.minor.patch`, each a whole number, or `None`.
pub fn parse(text: &str) -> Option<(u64, u64, u64)> {
    let mut parts = text.split('.');
    let mut next = || -> Option<u64> {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    let release = (next()?, next()?, next()?);
    parts.next().is_none().then_some(release)
}

/// What a command whose check failed says about the release the project is written for
/// (`written_for`, the manifest's `netsmith`, `None` when it has none), when that is not this
/// release: an older one, or none, points to `docs/MIGRATING.md`; a newer one asks for that
/// netsmith. `None` when the project is written for this release. A manifest's value has been
/// checked by [`parse`] when it was read.
pub fn note(written_for: Option<&str>) -> Option<String> {
    note_for(written_for, RELEASE)
}

fn note_for(written_for: Option<&str>, this: &str) -> Option<String> {
    let Some(written) = written_for else {
        return Some(format!(
            "netsmith.toml does not say which netsmith release the project is written for, so it was made with netsmith 0.1.0 or earlier, and this is netsmith {this}: an error above may be a spelling that changed since. {MIGRATING} lists every such change with what to write instead. When the project builds again, add `netsmith = \"{this}\"` to the [project] table of netsmith.toml"
        ));
    };
    match parse(written).cmp(&parse(this)) {
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Less => Some(format!(
            "the project is written for netsmith {written} (`netsmith` in netsmith.toml), and this is netsmith {this}: an error above may be a spelling that changed since. {MIGRATING} lists every change after {written} with what to write instead. When the project builds again, set `netsmith = \"{this}\"`"
        )),
        std::cmp::Ordering::Greater => Some(format!(
            "the project is written for netsmith {written} (`netsmith` in netsmith.toml), which is newer than this netsmith {this}: an error above may be a spelling this release does not know yet. Install netsmith {written} or later"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_is_three_whole_numbers() {
        assert_eq!(parse("0.1.0"), Some((0, 1, 0)));
        assert_eq!(parse("1.12.3"), Some((1, 12, 3)));
        for bad in [
            "",
            "0.1",
            "0.1.0.1",
            "v0.1.0",
            "0.1.0-rc1",
            "0..1",
            "a.b.c",
            "0.1.+1",
        ] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn this_release_is_a_release() {
        assert!(parse(RELEASE).is_some(), "{RELEASE}");
    }

    #[test]
    fn a_project_of_this_release_gets_no_note() {
        assert_eq!(note(Some(RELEASE)), None);
        assert_eq!(note_for(Some("0.2.0"), "0.2.0"), None);
    }

    #[test]
    fn an_older_project_is_pointed_to_the_migration_guide() {
        let n = note_for(Some("0.1.0"), "0.2.0").unwrap();
        assert!(n.contains("written for netsmith 0.1.0"), "{n}");
        assert!(n.contains("this is netsmith 0.2.0"), "{n}");
        assert!(n.contains(MIGRATING), "{n}");
        assert!(n.contains("set `netsmith = \"0.2.0\"`"), "{n}");
        // Compared as numbers, not as text: 0.10.0 is newer than 0.9.0.
        assert!(
            note_for(Some("0.9.0"), "0.10.0")
                .unwrap()
                .contains(MIGRATING)
        );
    }

    #[test]
    fn a_project_with_no_release_was_made_before_the_key() {
        let n = note_for(None, "0.2.0").unwrap();
        assert!(n.contains("0.1.0 or earlier"), "{n}");
        assert!(n.contains(MIGRATING), "{n}");
        assert!(n.contains("add `netsmith = \"0.2.0\"`"), "{n}");
    }

    #[test]
    fn a_newer_project_asks_for_that_netsmith() {
        let n = note_for(Some("0.3.0"), "0.2.0").unwrap();
        assert!(n.contains("newer than this netsmith 0.2.0"), "{n}");
        assert!(n.contains("Install netsmith 0.3.0 or later"), "{n}");
        assert!(!n.contains(MIGRATING), "{n}");
    }
}
