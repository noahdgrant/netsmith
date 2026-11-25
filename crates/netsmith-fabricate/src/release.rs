//! The netsmith release a project is written for (`netsmith = "0.2.0"` in `netsmith.toml`). A
//! command of another minor release warns of it and refuses nothing ([`mismatch`], issue #1657),
//! and a command whose check fails on a project that names none says that the project was made
//! before the key ([`no_release_note`], issue #740, ADR 0134).
//!
//! Another minor release may pick other parts, find other faults on the same copper and write
//! other output, and may change how a board spells what it already wrote (ADR 0001 keeps one
//! spelling, with no grace period), so a project that built on an earlier release can fail with
//! many errors. The two sentences say so, and where every change is listed: `docs/MIGRATING.md`.
//! A patch release is silent: it changes no spelling.

/// The release of this netsmith, the one `netsmith project init` writes into a new manifest.
pub const RELEASE: &str = env!("CARGO_PKG_VERSION");

/// Where the changes that need an edit of a project's source are listed, release by release.
pub const MIGRATING: &str = "https://github.com/noahdgrant/netsmith/blob/main/docs/MIGRATING.md";

/// Where the guide says how to install a release beside another.
const SIDE_BY_SIDE: &str = "language guide, \"Running the project's release\"";

/// A release as a manifest writes it: `major.minor.patch`, or `major.minor`, which any patch of
/// that minor release matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Release {
    pub major: u64,
    pub minor: u64,
    pub patch: Option<u64>,
}

/// A release number, `major.minor.patch` or `major.minor`, each a whole number, or `None`.
pub fn parse(text: &str) -> Option<Release> {
    let whole = |part: &str| -> Option<u64> {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    let mut parts = text.split('.');
    let major = whole(parts.next()?)?;
    let minor = whole(parts.next()?)?;
    let patch = match parts.next() {
        Some(part) => Some(whole(part)?),
        None => None,
    };
    parts.next().is_none().then_some(Release {
        major,
        minor,
        patch,
    })
}

/// The warning of a project written for (`written_for`, the manifest's `netsmith`, `None` when
/// it has none) another minor release than this one, or another major release. `None` when it
/// names none, and when only the patch differs. A manifest's value has been checked by [`parse`]
/// when it was read.
pub fn mismatch(written_for: Option<&str>) -> Option<String> {
    mismatch_for(written_for?, RELEASE)
}

fn mismatch_for(written_for: &str, this: &str) -> Option<String> {
    let (written, running) = (parse(written_for)?, parse(this)?);
    let wanted = (written.major, written.minor);
    let (major, minor) = wanted;
    let differs = "another minor release may pick other parts, find other faults and write other output, and an error may be a spelling";
    match wanted.cmp(&(running.major, running.minor)) {
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Less => Some(format!(
            "the project is written for netsmith {written_for} (`netsmith` in netsmith.toml), and this is netsmith {this}: {differs} that changed since; nothing is refused. {MIGRATING} lists every change after {written_for} with what to write instead. Run netsmith {major}.{minor} beside this one for the project's own results ({SIDE_BY_SIDE}), or, to move the project, check it with this release and set `netsmith = \"{this}\"`"
        )),
        std::cmp::Ordering::Greater => Some(format!(
            "the project is written for netsmith {written_for} (`netsmith` in netsmith.toml), which is newer than this netsmith {this}: {differs} this release does not know yet; nothing is refused. Install netsmith {major}.{minor} ({SIDE_BY_SIDE})"
        )),
    }
}

/// What a command whose check failed says of a project that names no release (`written_for` is
/// `None`): it was made before the key. A project written for another minor release has had
/// [`mismatch`].
pub fn no_release_note(written_for: Option<&str>) -> Option<String> {
    written_for.is_none().then(|| no_release_note_for(RELEASE))
}

fn no_release_note_for(this: &str) -> String {
    format!(
        "netsmith.toml does not say which netsmith release the project is written for, so it was made with netsmith 0.1.0 or earlier, and this is netsmith {this}: an error above may be a spelling that changed since. {MIGRATING} lists every such change with what to write instead. When the project builds again, add `netsmith = \"{this}\"` to the [project] table of netsmith.toml"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_is_two_or_three_whole_numbers() {
        let release = |major, minor, patch| {
            Some(Release {
                major,
                minor,
                patch,
            })
        };
        assert_eq!(parse("0.1.0"), release(0, 1, Some(0)));
        assert_eq!(parse("1.12.3"), release(1, 12, Some(3)));
        assert_eq!(parse("0.9"), release(0, 9, None));
        for bad in [
            "",
            "0",
            "0.1.",
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
    fn this_release_is_a_release_with_a_patch() {
        assert!(
            parse(RELEASE).is_some_and(|r| r.patch.is_some()),
            "{RELEASE}"
        );
    }

    #[test]
    fn the_same_minor_release_is_silent_whatever_the_patch() {
        assert_eq!(mismatch(Some(RELEASE)), None);
        for written_for in ["0.2.0", "0.2.3", "0.2.9", "0.2"] {
            assert_eq!(mismatch_for(written_for, "0.2.3"), None, "{written_for}");
        }
        assert_eq!(mismatch(None), None);
    }

    #[test]
    fn an_older_project_is_pointed_to_the_migration_guide() {
        let w = mismatch_for("0.1.0", "0.2.0").unwrap();
        assert!(w.contains("written for netsmith 0.1.0"), "{w}");
        assert!(w.contains("this is netsmith 0.2.0"), "{w}");
        assert!(w.contains(MIGRATING), "{w}");
        assert!(w.contains("Run netsmith 0.1 beside this one"), "{w}");
        assert!(w.contains("set `netsmith = \"0.2.0\"`"), "{w}");
        // A release with no patch, and compared as numbers, not as text: 0.10 is newer than 0.9.
        assert!(mismatch_for("0.9", "0.10.0").unwrap().contains(MIGRATING));
        // Another major release is another minor release too.
        assert!(mismatch_for("1.2.0", "2.2.0").is_some());
    }

    #[test]
    fn a_project_with_no_release_was_made_before_the_key() {
        assert_eq!(no_release_note(Some("0.1.0")), None);
        assert!(no_release_note(None).is_some());
        let n = no_release_note_for("0.2.0");
        assert!(n.contains("0.1.0 or earlier"), "{n}");
        assert!(n.contains(MIGRATING), "{n}");
        assert!(n.contains("add `netsmith = \"0.2.0\"`"), "{n}");
    }

    #[test]
    fn a_newer_project_asks_for_that_netsmith() {
        let w = mismatch_for("0.3.1", "0.2.0").unwrap();
        assert!(w.contains("newer than this netsmith 0.2.0"), "{w}");
        assert!(w.contains("Install netsmith 0.3 "), "{w}");
        assert!(!w.contains(MIGRATING), "{w}");
    }
}
