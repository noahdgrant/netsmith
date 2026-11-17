//! `netsmith project refresh`: bring the agent files of a project up to the netsmith that
//! runs (issue #797).
//!
//! The files `init` writes for agents (`AGENTS.md`, the skills, the language guide) are the
//! project's to edit, so a refresh rewrites one only when netsmith wrote its text and nobody
//! changed it since. It knows that from the record `init` keeps beside the skills
//! ([`RECORD`]: the SHA-256 of each file as written), and for a project made before the
//! record from a frozen list of every earlier version ([`HISTORY`]). A file that is missing
//! is written; a file that differs from both is left alone and listed with a line diff, so
//! nothing a person wrote is replaced without their asking. `CLAUDE.md` is written when it is
//! absent and never rewritten.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use netsmith_fabricate::models::sha256;
use netsmith_fabricate::project::Project;

use super::{
    AGENTS, AGENTS_TEXT, CLAUDE, CLAUDE_TEXT, GUIDE, GUIDE_CHAPTERS, GUIDE_CHAPTERS_DIR,
    GUIDE_TEXT, SKILLS, SKILLS_DIR, SKILLS_LINK, link_skills, symlink_dir,
};

/// The record of what netsmith wrote: a line `<sha256>  <path>` for each file, as
/// `sha256sum` prints them. It is the project's, so it is committed with the files.
pub(super) const RECORD: &str = ".agents/netsmith-files.sha256";

/// The versions the files had before the record existed: `<sha256> <path>` per line, and `#`
/// lines for comments.
const HISTORY: &str = include_str!("../../templates/agent-files-history.txt");

/// A file netsmith keeps up to date: where it goes in the project, what netsmith writes
/// there now, and whether a refresh may create it when it is missing.
struct Managed {
    path: String,
    text: &'static str,
    create: bool,
}

/// Every file `init` writes for agents, by the path it has in the project. Where
/// `.claude/skills` is a folder of copies (no symlinks, ADR 0015) the copies are managed too,
/// but only those that are already there: it may be the project's own folder.
fn managed(dir: &Path) -> Vec<Managed> {
    let file = |path: String, text, create| Managed { path, text, create };
    let mut all = vec![
        file(AGENTS.to_string(), AGENTS_TEXT, true),
        file(GUIDE.to_string(), GUIDE_TEXT, true),
    ];
    for (name, text) in GUIDE_CHAPTERS {
        all.push(file(format!("{GUIDE_CHAPTERS_DIR}/{name}"), text, true));
    }
    for (skill, text) in SKILLS {
        all.push(file(format!("{SKILLS_DIR}/{skill}/SKILL.md"), text, true));
    }
    let copies = dir
        .join(SKILLS_LINK)
        .symlink_metadata()
        .is_ok_and(|m| m.is_dir() && !m.file_type().is_symlink());
    if copies {
        for (skill, text) in SKILLS {
            all.push(file(format!("{SKILLS_LINK}/{skill}/SKILL.md"), text, false));
        }
    }
    all
}

/// The record's lines: the path to the SHA-256 netsmith wrote there.
fn read_record(dir: &Path) -> BTreeMap<String, String> {
    let text = fs::read_to_string(dir.join(RECORD)).unwrap_or_default();
    text.lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once("  "))
        .map(|(hash, path)| (path.to_string(), hash.to_string()))
        .collect()
}

/// Whether `hash` is the SHA-256 of an earlier version of the file at `path`.
fn in_history(path: &str, hash: &str) -> bool {
    HISTORY
        .lines()
        .filter(|l| !l.starts_with('#'))
        .filter_map(|l| l.split_once(' '))
        .any(|(h, p)| h == hash && p == path)
}

/// Writes the record of `dir`: for each managed file whose text is what netsmith writes now,
/// that text's SHA-256; for one that differs, the entry it had (so a file a person
/// edits back to an old version of netsmith's is still known as unedited). Returns whether
/// it changed the file.
pub(super) fn write_record(dir: &Path) -> Result<bool, String> {
    let before = read_record(dir);
    let mut now = BTreeMap::new();
    for m in managed(dir) {
        let current = sha256(m.text.as_bytes());
        match fs::read(dir.join(&m.path)) {
            Ok(bytes) if sha256(&bytes) == current => {
                now.insert(m.path, current);
            }
            _ => {
                if let Some(old) = before.get(&m.path) {
                    now.insert(m.path, old.clone());
                }
            }
        }
    }
    if now == before && dir.join(RECORD).is_file() {
        return Ok(false);
    }
    let mut text = String::from(
        "# What `netsmith project init` wrote (the SHA-256 of each file). `netsmith project refresh` rewrites\n# a file only while it still has the text recorded here; commit this file with the others.\n",
    );
    for (path, hash) in &now {
        let _ = writeln!(text, "{hash}  {path}");
    }
    let target = dir.join(RECORD);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(&target, text).map_err(|e| format!("{}: {e}", target.display()))?;
    Ok(true)
}

/// The text to show after `refresh`: what was written, and what was left alone, with a diff.
pub fn refresh(dir: &Path) -> Result<String, String> {
    let project = Project::find(dir)?.ok_or_else(|| {
        "there is no netsmith.toml here or in a folder above: `netsmith project init` makes a project, and `netsmith project refresh` brings its agent files up to date".to_string()
    })?;
    let root = project.root;
    let record = read_record(&root);

    let mut created = Vec::new();
    let mut updated = Vec::new();
    let mut current = 0usize;
    let mut left = Vec::new();
    let mut report = String::new();
    let mut notes = String::new();
    for m in managed(&root) {
        let target = root.join(&m.path);
        let write = |text: &str| -> Result<(), String> {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            fs::write(&target, text).map_err(|e| format!("{}: {e}", target.display()))
        };
        let Ok(meta) = target.symlink_metadata() else {
            if m.create {
                write(m.text)?;
                created.push(m.path);
            }
            continue;
        };
        if !meta.is_file() {
            left.push(format!("{} is not a plain file\n", m.path));
            continue;
        }
        let bytes = fs::read(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        let hash = sha256(&bytes);
        if hash == sha256(m.text.as_bytes()) {
            current += 1;
        } else if record.get(&m.path) == Some(&hash) || in_history(&m.path, &hash) {
            write(m.text)?;
            updated.push(m.path);
        } else {
            let mut said = format!("{} has your changes\n", m.path);
            said.push_str(&diff_of(&m.path, &String::from_utf8_lossy(&bytes), m.text));
            left.push(said);
        }
    }

    let claude = root.join(CLAUDE);
    if claude.symlink_metadata().is_err() {
        fs::write(&claude, CLAUDE_TEXT).map_err(|e| format!("{}: {e}", claude.display()))?;
        created.push(CLAUDE.to_string());
    } else if let Some(note) = claude_note(&claude) {
        notes.push_str(&note);
    }
    for path in &created {
        report.push_str(&format!("created {path}\n"));
    }
    for path in &updated {
        report.push_str(&format!("updated {path}\n"));
    }
    report.push_str(&notes);
    report.push_str(&link_skills(&root, symlink_dir)?);
    let wrote = write_record(&root)?;
    if wrote {
        report.push_str(&format!("wrote {RECORD}\n"));
    }
    report.push_str(&format!(
        "{current} files were already current; {} created, {} updated, {} left alone\n",
        created.len(),
        updated.len(),
        left.len()
    ));
    report.push_str(&dropped(&root));
    if !left.is_empty() {
        report.push_str(
            "\nleft alone, because they are not what netsmith wrote and you may have edited them (in each diff `-` is your file and `+` is what netsmith writes now; to take netsmith's text, delete the file and run `netsmith project refresh` again):\n\n",
        );
        for said in &left {
            report.push_str(said);
            report.push('\n');
        }
    }
    Ok(report)
}

/// The line diff of a file of the project to what netsmith writes now, or a note when the
/// files are too long to compare.
fn diff_of(path: &str, yours: &str, ours: &str) -> String {
    const LIMIT: usize = 4_000_000;
    if yours.lines().count() * ours.lines().count() > LIMIT {
        return format!("(too long to show a diff of {path})\n");
    }
    let full = crate::dxf::diff(path, yours, ours);
    // Only the changed lines, with two lines of the text around each.
    const CONTEXT: usize = 2;
    let lines: Vec<&str> = full.lines().collect();
    let (head, body) = lines.split_at(2.min(lines.len()));
    let changed: Vec<usize> = (0..body.len())
        .filter(|&i| !body[i].starts_with(' '))
        .collect();
    let mut out = String::new();
    for l in head {
        out.push_str(l);
        out.push('\n');
    }
    let mut next = 0;
    for (i, l) in body.iter().enumerate() {
        let near = changed.iter().any(|&c| c.abs_diff(i) <= CONTEXT);
        if near {
            if i > next {
                out.push_str("...\n");
            }
            out.push_str(l);
            out.push('\n');
            next = i + 1;
        }
    }
    out
}

/// What to say about a `CLAUDE.md` that exists: nothing when it imports `AGENTS.md`.
pub(super) fn claude_note(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    if text.lines().any(|l| l.trim() == CLAUDE_TEXT.trim()) {
        return None;
    }
    Some(format!(
        "note: {CLAUDE} exists and was not touched; add the line `{}` so Claude Code reads {AGENTS}\n",
        CLAUDE_TEXT.trim()
    ))
}

/// Files of an earlier netsmith that this one no longer writes: a chapter or a skill that was
/// renamed or removed. They are left in place, and named, because a refresh deletes nothing.
fn dropped(root: &Path) -> String {
    let mut out = String::new();
    let known_chapter = |n: &str| GUIDE_CHAPTERS.iter().any(|(c, _)| *c == n);
    let names = |dir: &str| -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(root.join(dir))
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    };
    for n in names(GUIDE_CHAPTERS_DIR) {
        if n.ends_with(".md") && !known_chapter(&n) {
            out.push_str(&format!(
                "note: {GUIDE_CHAPTERS_DIR}/{n} is not a chapter of this guide any more; remove it if you did not write it\n"
            ));
        }
    }
    for n in names(SKILLS_DIR) {
        if n.starts_with("netsmith-") && !SKILLS.iter().any(|(s, _)| *s == n) {
            out.push_str(&format!(
                "note: {SKILLS_DIR}/{n} is not a netsmith skill any more; remove it if you did not write it\n"
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_history_lists_hashes_of_files_that_init_writes() {
        let paths: Vec<String> = managed(Path::new("/nowhere"))
            .into_iter()
            .map(|m| m.path)
            .collect();
        let mut lines = 0;
        for line in HISTORY.lines().filter(|l| !l.starts_with('#')) {
            let (hash, path) = line.split_once(' ').unwrap();
            assert!(
                hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()),
                "{line}"
            );
            // A chapter that a later guide dropped is not in the list, so only the files
            // that are still written are checked against it.
            if path.starts_with(GUIDE_CHAPTERS_DIR) && !paths.iter().any(|p| p == path) {
                continue;
            }
            assert!(
                paths.iter().any(|p| p == path),
                "{path} is not a file init writes"
            );
            lines += 1;
        }
        assert!(lines > 100, "{lines}");
    }

    #[test]
    fn the_history_knows_the_first_agents_md_and_the_current_text_is_current() {
        let first = include_str!("../../tests/fixtures/agents-first-version.md");
        assert!(in_history(AGENTS, &sha256(first.as_bytes())));
        assert!(!in_history("README.md", &sha256(first.as_bytes())));
        assert!(!in_history(AGENTS, &sha256(b"mine\n")));
    }

    #[test]
    fn the_record_is_read_and_written_in_sha256sum_form() {
        let dir = netsmith_testkit::TempDir::new("init-record");
        assert!(read_record(&dir).is_empty());
        fs::create_dir_all(dir.join(".agents")).unwrap();
        fs::write(dir.join(AGENTS), AGENTS_TEXT).unwrap();
        assert!(write_record(&dir).unwrap());
        let record = read_record(&dir);
        assert_eq!(record.len(), 1);
        assert_eq!(record[AGENTS], sha256(AGENTS_TEXT.as_bytes()));
        assert!(!write_record(&dir).unwrap(), "nothing changed");
        // An edit does not drop the entry: edited back, the file is known again.
        fs::write(dir.join(AGENTS), "mine\n").unwrap();
        assert!(!write_record(&dir).unwrap());
        assert_eq!(read_record(&dir).len(), 1);
    }
}
