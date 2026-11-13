//! `netsmith init`: scaffold a project in a directory, like `cargo init` (ADR 0011).
//!
//! It never overwrites a file. `netsmith.toml` and `src/main.nsm` are the project, so if
//! either exists it refuses, lists what exists, and writes nothing. `README.md` is left
//! alone if there is one, and `.gitignore` only gains the lines it lacks. For the agents
//! that work in the project it also writes `AGENTS.md`, the user skills in
//! `.agents/skills/` (linked from `.claude/skills`) and the language guide (ADR 0015);
//! each of those that exists is kept.

use std::{fs, io, path::Path};

use netsmith_fabricate::project::{MANIFEST, Manifest, SOURCE_DIR, is_project_name};

/// The starter board is the LED example, so the example and the scaffold cannot drift
/// apart; only the board's name is the project's.
const TEMPLATE: &str = include_str!("../../../examples/led.nsm");
const TEMPLATE_BOARD: &str = "LedExample";

/// Names a starter board could clash with: what the template imports.
const TEMPLATE_IMPORTS: [&str; 5] = ["Header1x2", "Resistor", "Led", "Rail", "TwoLayer"];

const MAIN: &str = "src/main.nsm";
const README: &str = "README.md";
const GITIGNORE: &str = ".gitignore";
const AGENTS: &str = "AGENTS.md";
const GUIDE: &str = "docs/netsmith/LANGUAGE.md";
const SKILLS_DIR: &str = ".agents/skills";
const SKILLS_LINK: &str = ".claude/skills";

/// What the project's `AGENTS.md` says. It is not the repository's own `AGENTS.md`, which
/// is for people working on netsmith.
const AGENTS_TEXT: &str = include_str!("../templates/AGENTS.md");

/// The language guide, the one `docs/LANGUAGE.md` of this repository.
const GUIDE_TEXT: &str = include_str!("../../../docs/LANGUAGE.md");

/// The skills for working in a project: the repository's own `netsmith-*` skills, not the
/// `netsmith-dev-*` ones that are for working on netsmith. A test checks that this lists
/// every one of them.
const SKILLS: [(&str, &str); 7] = [
    (
        "netsmith-design-a-board",
        include_str!("../../../.agents/skills/netsmith-design-a-board/SKILL.md"),
    ),
    (
        "netsmith-port-a-kicad-board",
        include_str!("../../../.agents/skills/netsmith-port-a-kicad-board/SKILL.md"),
    ),
    (
        "netsmith-prepare-fab-order",
        include_str!("../../../.agents/skills/netsmith-prepare-fab-order/SKILL.md"),
    ),
    (
        "netsmith-read-errors",
        include_str!("../../../.agents/skills/netsmith-read-errors/SKILL.md"),
    ),
    (
        "netsmith-use-constraints",
        include_str!("../../../.agents/skills/netsmith-use-constraints/SKILL.md"),
    ),
    (
        "netsmith-write-a-module",
        include_str!("../../../.agents/skills/netsmith-write-a-module/SKILL.md"),
    ),
    (
        "netsmith-write-nsm",
        include_str!("../../../.agents/skills/netsmith-write-nsm/SKILL.md"),
    ),
];

/// Make a project in `dir` named `name`, or after the directory if `None`. On success the
/// text to show: what was made and what to run next.
pub fn init(dir: &Path, name: Option<&str>) -> Result<String, String> {
    let dir = std::path::absolute(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let name = match name {
        Some(n) => n.to_string(),
        None => dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    if !is_project_name(&name) {
        return Err(format!(
            "`{name}` cannot be a project name (letters, digits, `-` and `_`): say which with `netsmith init --name <name>`"
        ));
    }

    // The nearest netsmith.toml wins when a command looks for its project, so a project
    // inside a project could never be reached from the outer one's commands.
    if let Some(parent) = dir.parent()
        && let Some(found) = parent.ancestors().find(|d| d.join(MANIFEST).is_file())
    {
        return Err(format!(
            "this directory is inside the project at {} (it has a {MANIFEST}); a project cannot hold another",
            found.display()
        ));
    }

    let mut exists = Vec::new();
    for file in [MANIFEST, MAIN] {
        if dir.join(file).symlink_metadata().is_ok() {
            exists.push(file.to_string());
        }
    }
    let source_dir = dir.join(SOURCE_DIR);
    if source_dir.exists() && !source_dir.is_dir() {
        exists.push(format!("{SOURCE_DIR} (not a directory)"));
    }
    if !exists.is_empty() {
        return Err(format!(
            "not overwriting what is already here:\n  {}\nnothing was written; remove them or run in an empty directory",
            exists.join("\n  ")
        ));
    }

    let manifest = Manifest::new(&name);
    let board = board_name(&name);
    let main = TEMPLATE.replace(TEMPLATE_BOARD, &board);
    let ignore: Vec<String> = [&manifest.out, &manifest.fab]
        .iter()
        .map(|d| format!("/{d}/"))
        .collect();

    let write = |path: &str, text: &str| -> Result<(), String> {
        let target = dir.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::write(&target, text).map_err(|e| format!("{}: {e}", target.display()))
    };

    let mut report = String::new();
    write(
        MANIFEST,
        &format!("{}{}", manifest.render(), order_example(&board)),
    )?;
    report.push_str(&format!("created {MANIFEST}\n"));
    write(MAIN, &main)?;
    report.push_str(&format!("created {MAIN} (board {board})\n"));

    let ignore_path = dir.join(GITIGNORE);
    if ignore_path.exists() {
        let existing = fs::read_to_string(&ignore_path)
            .map_err(|e| format!("{}: {e}", ignore_path.display()))?;
        let missing: Vec<&String> = ignore.iter().filter(|l| !ignores(&existing, l)).collect();
        if missing.is_empty() {
            report.push_str(&format!("kept {GITIGNORE} (it already ignores them)\n"));
        } else {
            let mut text = existing;
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            for line in &missing {
                text.push_str(line);
                text.push('\n');
            }
            write(GITIGNORE, &text)?;
            let added: Vec<&str> = missing.iter().map(|l| l.as_str()).collect();
            report.push_str(&format!(
                "updated {GITIGNORE} (added {})\n",
                added.join(", ")
            ));
        }
    } else {
        write(GITIGNORE, &(ignore.join("\n") + "\n"))?;
        report.push_str(&format!("created {GITIGNORE}\n"));
    }

    if dir.join(README).exists() {
        report.push_str(&format!("kept {README}\n"));
    } else {
        write(README, &readme(&name))?;
        report.push_str(&format!("created {README}\n"));
    }

    if dir.join(AGENTS).symlink_metadata().is_ok() {
        report.push_str(&format!(
            "note: {AGENTS} exists and was not touched; add a line telling agents to read {GUIDE} and use the skills in {SKILLS_DIR}/\n"
        ));
    } else {
        write(AGENTS, AGENTS_TEXT)?;
        report.push_str(&format!("created {AGENTS}\n"));
    }

    if dir.join(GUIDE).symlink_metadata().is_ok() {
        report.push_str(&format!("kept {GUIDE}\n"));
    } else {
        write(GUIDE, GUIDE_TEXT)?;
        report.push_str(&format!("created {GUIDE} (the language guide)\n"));
    }

    let mut made = Vec::new();
    for (skill, text) in SKILLS {
        let path = format!("{SKILLS_DIR}/{skill}/SKILL.md");
        if dir.join(&path).symlink_metadata().is_err() {
            write(&path, text)?;
            made.push(skill);
        }
    }
    if made.len() == SKILLS.len() {
        report.push_str(&format!(
            "created {SKILLS_DIR}/ with {} skills\n",
            made.len()
        ));
    } else {
        report.push_str(&format!(
            "created {} skills in {SKILLS_DIR}/, kept the others\n",
            made.len()
        ));
    }
    report.push_str(&link_skills(&dir, symlink_dir)?);

    if !in_git_repo(&dir) {
        report.push_str("note: this is not a git repository; netsmith did not run git (run `git init` if you want one)\n");
    }
    report.push_str(
        "\nnext:\n  netsmith build   # a KiCad schematic in out/\n  netsmith editor  # edit the source and see the board in a browser\n  netsmith fab     # files to upload to JLCPCB in fab/\n",
    );
    Ok(report)
}

/// Make `.claude/skills` a relative link to `../.agents/skills`, which is how Claude Code
/// finds the skills the other agents read from `.agents/skills`. Where `link` fails (a
/// platform or a file system with no symlinks) copy the skills instead, and say so. An
/// existing `.claude/skills` is kept.
fn link_skills(
    dir: &Path,
    link: impl Fn(&Path, &Path) -> io::Result<()>,
) -> Result<String, String> {
    let at = dir.join(SKILLS_LINK);
    if at.symlink_metadata().is_ok() {
        return Ok(format!("kept {SKILLS_LINK}\n"));
    }
    if let Some(parent) = at.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    if link(Path::new("../.agents/skills"), &at).is_ok() {
        return Ok(format!(
            "created {SKILLS_LINK} (a link to ../.agents/skills)\n"
        ));
    }
    for (skill, text) in SKILLS {
        let target = at.join(skill).join("SKILL.md");
        fs::create_dir_all(target.parent().unwrap())
            .and_then(|()| fs::write(&target, text))
            .map_err(|e| format!("{}: {e}", target.display()))?;
    }
    Ok(format!(
        "created {SKILLS_LINK} as a copy of {SKILLS_DIR}: symlinks are not available here, so keep the two the same\n"
    ))
}

#[cfg(unix)]
fn symlink_dir(target: &Path, link: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn symlink_dir(target: &Path, link: &Path) -> io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

#[cfg(not(any(unix, windows)))]
fn symlink_dir(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}

/// A project name as the name of its starter board: `my-sensor` is `MySensor`. A name that
/// would start with a digit, or clash with something the board imports, gains `Board`.
fn board_name(project: &str) -> String {
    let mut name = String::new();
    for word in project.split(['-', '_']).filter(|w| !w.is_empty()) {
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            name.extend(first.to_uppercase());
            name.push_str(chars.as_str());
        }
    }
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert_str(0, "Board");
    } else if TEMPLATE_IMPORTS.contains(&name.as_str()) {
        name.push_str("Board");
    }
    name
}

/// Whether a `.gitignore`'s text already ignores `entry` (`/out/`), however it is spelled.
fn ignores(gitignore: &str, entry: &str) -> bool {
    let bare = |s: &str| s.trim().trim_matches('/').to_string();
    gitignore.lines().any(|l| bare(l) == bare(entry))
}

/// Whether `dir` is inside a git work tree. (A `.git` file is a worktree or submodule.)
fn in_git_repo(dir: &Path) -> bool {
    dir.ancestors().any(|d| d.join(".git").exists())
}

fn readme(name: &str) -> String {
    format!(
        "# {name}\n\
         \n\
         A netsmith project. `netsmith.toml` lists the files that hold boards; `src/main.nsm`\n\
         is the first. Other files under `src/` are modules: `src/power.nsm` is imported with\n\
         `import src::power::Name`.\n\
         \n\
         ```sh\n\
         netsmith build   # a KiCad schematic in out/\n\
         netsmith editor  # edit the source and see the board in a browser\n\
         netsmith fab     # files to upload to JLCPCB in fab/\n\
         netsmith fab --order first  # only the boards of an order in netsmith.toml, with quantities\n\
         ```\n\
         \n\
         Real parts come from catalogs. A part the standard JLCPCB catalogs lack goes in a CSV\n\
         file named after its catalog (`catalogs/resistor.csv`): every `*.csv` in `catalogs/` is a\n\
         catalog (`catalog_dirs` in `netsmith.toml`); every row that names a part needs its `source`.\n\
         \n\
         For people and coding agents: `AGENTS.md` says how to work here, `docs/netsmith/LANGUAGE.md`\n\
         is the language guide, and `.agents/skills/` has the netsmith skills (`.claude/skills`\n\
         links to it). netsmith wrote them once and never overwrites them: they are yours to edit.\n"
    )
}

/// A commented example order for the manifest `init` writes (ADR 0035), so that the way to
/// choose boards and quantities for a fab order is where a person looks for it.
fn order_example(board: &str) -> String {
    format!(
        "\n# An order is the boards to make in one run of `netsmith fab --order first`, and how many of each.\n# A key is a board as its folder is named (`Board`, or `System/member`); a system's name alone is all its members.\n#\n# [order.first.boards]\n# {board} = 5\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_name_becomes_a_board_name() {
        assert_eq!(board_name("my-sensor"), "MySensor");
        assert_eq!(board_name("sensor_node"), "SensorNode");
        assert_eq!(board_name("led"), "Led".to_string() + "Board");
        assert_eq!(board_name("rail"), "RailBoard");
        assert_eq!(board_name("3v3"), "Board3v3");
        assert_eq!(board_name("-"), "Board");
    }

    #[test]
    fn the_template_names_the_board_the_scaffold_renames() {
        assert!(TEMPLATE.contains(&format!("board {TEMPLATE_BOARD} {{")));
        for import in TEMPLATE_IMPORTS {
            assert!(TEMPLATE.contains(import), "{import}");
        }
    }

    fn repo(path: &str) -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(path)
    }

    #[test]
    fn the_scaffolded_guide_and_skills_are_the_repository_files() {
        let read = |path: &str| fs::read_to_string(repo(path)).unwrap();
        assert_eq!(GUIDE_TEXT, read("docs/LANGUAGE.md"));
        assert_eq!(
            AGENTS_TEXT,
            read("crates/netsmith-tools/templates/AGENTS.md")
        );
        for (skill, text) in SKILLS {
            assert_eq!(text, read(&format!(".agents/skills/{skill}/SKILL.md")));
        }
    }

    #[test]
    fn the_scaffold_has_every_user_skill_and_no_contributor_skill() {
        let mut on_disk: Vec<String> = fs::read_dir(repo(".agents/skills"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| !n.starts_with("netsmith-dev-"))
            .collect();
        on_disk.sort();
        let listed: Vec<&str> = SKILLS.iter().map(|(n, _)| *n).collect();
        assert_eq!(listed, on_disk, "SKILLS must list every netsmith-* skill");
        // Every skill the project's AGENTS.md names is one it gets, and the reverse.
        for (skill, _) in SKILLS {
            assert!(AGENTS_TEXT.contains(&format!("`{skill}`")), "{skill}");
        }
        assert!(!AGENTS_TEXT.contains("netsmith-dev-"));
    }

    #[test]
    fn skills_are_copied_where_a_symlink_cannot_be_made() {
        let dir = netsmith_testkit::TempDir::new("init-copy");
        let said = link_skills(&dir, |_, _| Err(io::ErrorKind::PermissionDenied.into())).unwrap();
        assert!(
            said.contains("copy") && said.contains("symlinks are not available"),
            "{said}"
        );
        let meta = dir.join(SKILLS_LINK).symlink_metadata().unwrap();
        assert!(meta.is_dir() && !meta.is_symlink());
        for (skill, text) in SKILLS {
            let copy = fs::read_to_string(dir.join(SKILLS_LINK).join(skill).join("SKILL.md"));
            assert_eq!(copy.unwrap(), text);
        }
        // An existing `.claude/skills` is kept, whatever it is.
        let said = link_skills(&dir, |_, _| unreachable!()).unwrap();
        assert!(said.starts_with("kept"), "{said}");
    }

    #[test]
    fn a_gitignore_entry_is_found_however_it_is_spelled() {
        assert!(ignores("target\nout\n", "/out/"));
        assert!(ignores("/out/\n", "/out/"));
        assert!(ignores("  out/  \n", "/out/"));
        assert!(!ignores("outer/\n# out\n", "/out/"));
        assert!(!ignores("", "/fab/"));
    }
}
