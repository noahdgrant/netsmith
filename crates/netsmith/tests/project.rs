//! `netsmith init`, the `netsmith.toml` manifest, and projects of several files.

use netsmith_testkit::{TempDir, TempFile};
use std::fs;
use std::path::Path;

use netsmith::build_with;
use netsmith::editor::Workspace;
use netsmith::fab::fab_with;
use netsmith::init::init;
use netsmith::project::{Manifest, Project, Target};
use netsmith_check::Loader;
use netsmith_editor::Backend;
use netsmith_fab::KicadCliExe;
use netsmith_solve::Catalogs;

const LED: &str = include_str!("../../../examples/led.nsm");
const STATUS: &str = include_str!("../../../examples/status_indicator.nsm");

/// An empty directory of its own.
fn dir(name: &str) -> TempDir {
    TempDir::new(&format!("project-test-{name}"))
}

fn write(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn read(root: &Path, file: &str) -> String {
    fs::read_to_string(root.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

fn files_in(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).unwrap() {
            let path = e.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push(path.strip_prefix(root).unwrap().display().to_string());
            }
        }
    }
    out.sort();
    out
}

/// A project in a directory named `name`, as `netsmith init` makes it.
fn scaffold(name: &str) -> TempFile {
    let root = TempFile::new(&format!("project-test-{name}"), name);
    fs::create_dir_all(&root).unwrap();
    init(&root, None).unwrap_or_else(|e| panic!("{e}"));
    root
}

// ---- init ----------------------------------------------------------------------------

#[test]
fn init_makes_the_project_files_and_says_what_to_run() {
    let root = TempFile::new("project-test-fresh", "my-sensor");
    fs::create_dir_all(&root).unwrap();
    let said = init(&root, None).unwrap();

    let mut expected: Vec<String> = [
        ".agents/skills/netsmith-design-a-board/SKILL.md",
        ".agents/skills/netsmith-port-a-kicad-board/SKILL.md",
        ".agents/skills/netsmith-prepare-fab-order/SKILL.md",
        ".agents/skills/netsmith-read-errors/SKILL.md",
        ".agents/skills/netsmith-use-constraints/SKILL.md",
        ".agents/skills/netsmith-write-a-module/SKILL.md",
        ".agents/skills/netsmith-write-nsm/SKILL.md",
        ".gitignore",
        "AGENTS.md",
        "README.md",
        "docs/netsmith/LANGUAGE.md",
        "netsmith.toml",
        "src/main.nsm",
    ]
    .map(String::from)
    .into();
    // `.claude/skills` is a link, so the walk finds the skills a second time through it.
    expected.extend(
        expected
            .iter()
            .filter(|f| f.starts_with(".agents/"))
            .map(|f| f.replacen(".agents/", ".claude/", 1))
            .collect::<Vec<_>>(),
    );
    expected.sort();
    assert_eq!(files_in(&root), expected);
    let manifest = Manifest::parse(&read(&root, "netsmith.toml")).unwrap();
    // The commented example order is in the file, and the file is still a plain manifest.
    let text = read(&root, "netsmith.toml");
    assert!(
        text.contains("# [order.first.boards]\n# MySensor = 5\n"),
        "{text}"
    );
    assert!(manifest.orders.is_empty());
    assert_eq!(manifest, Manifest::new("my-sensor"));
    // Both defaults are written out, so a reader sees where the files are.
    assert!(
        text.contains("top_level = [\"src/main.nsm\"]\n")
            && text.contains("catalog_dirs = [\"catalogs\"]\n"),
        "{text}"
    );
    let main = read(&root, "src/main.nsm");
    assert!(main.contains("board MySensor {"), "{main}");
    assert!(!main.contains("LedExample"), "{main}");
    assert_eq!(
        read(&root, ".gitignore"),
        "/out/\n/fab/\n",
        "the output directories of the manifest"
    );
    assert!(read(&root, "README.md").starts_with("# my-sensor\n"));
    assert!(read(&root, "README.md").contains("AGENTS.md"));

    for line in [
        "netsmith.toml",
        "src/main.nsm",
        "netsmith build",
        "netsmith editor",
        "netsmith fab",
    ] {
        assert!(said.contains(line), "{line}: {said}");
    }
    assert!(
        said.contains("not a git repository") && said.contains("did not run git"),
        "{said}"
    );
    assert!(!root.join(".git").exists(), "init does not run git");
}

#[test]
fn init_says_nothing_about_git_inside_a_repository() {
    let repo = dir("repo");
    fs::create_dir_all(repo.join(".git")).unwrap();
    let root = repo.join("p");
    fs::create_dir_all(&root).unwrap();
    let said = init(&root, None).unwrap();
    assert!(!said.contains("not a git repository"), "{said}");
}

#[test]
fn init_takes_the_name_from_the_option_or_refuses_one_it_cannot_use() {
    let root = TempFile::new("project-test-named", "not a name!");
    fs::create_dir_all(&root).unwrap();
    let e = init(&root, None).unwrap_err();
    assert!(e.contains("`not a name!`") && e.contains("--name"), "{e}");
    assert!(
        files_in(&root).is_empty(),
        "nothing is written when it refuses"
    );

    init(&root, Some("sensor_node")).unwrap();
    assert!(read(&root, "netsmith.toml").contains("name = \"sensor_node\""));
    assert!(read(&root, "src/main.nsm").contains("board SensorNode {"));

    let e = init(&dir("badname"), Some("a b")).unwrap_err();
    assert!(e.contains("`a b`"), "{e}");
}

#[test]
fn init_refuses_to_overwrite_and_lists_what_exists() {
    let root = TempFile::new("project-test-exists", "p");
    write(&root, "netsmith.toml", "mine\n");
    write(&root, "src/main.nsm", "mine\n");
    let e = init(&root, None).unwrap_err();
    assert!(
        e.contains("netsmith.toml")
            && e.contains("src/main.nsm")
            && e.contains("nothing was written"),
        "{e}"
    );
    assert_eq!(read(&root, "netsmith.toml"), "mine\n");
    assert_eq!(read(&root, "src/main.nsm"), "mine\n");
    assert_eq!(
        files_in(&root),
        ["netsmith.toml", "src/main.nsm"],
        "no README, .gitignore, AGENTS.md, guide or skills either"
    );

    // One of the two is enough, and a second run on a scaffold changes nothing.
    let root = scaffold("twice");
    let before: Vec<String> = files_in(&root).iter().map(|f| read(&root, f)).collect();
    let e = init(&root, None).unwrap_err();
    assert!(
        e.contains("netsmith.toml") && e.contains("src/main.nsm"),
        "{e}"
    );
    let after: Vec<String> = files_in(&root).iter().map(|f| read(&root, f)).collect();
    assert_eq!(before, after);

    let root = TempFile::new("project-test-only-main", "p");
    write(&root, "src/main.nsm", "mine\n");
    let e = init(&root, None).unwrap_err();
    assert!(
        e.contains("src/main.nsm") && !e.contains("netsmith.toml"),
        "{e}"
    );
    assert_eq!(files_in(&root), ["src/main.nsm"]);

    let root = TempFile::new("project-test-src-file", "p");
    write(&root, "src", "a file\n");
    let e = init(&root, None).unwrap_err();
    assert!(e.contains("src (not a directory)"), "{e}");
}

#[test]
fn init_refuses_inside_another_project() {
    let outer = scaffold("outer");
    let inner = outer.join("src");
    let e = init(&inner, None).unwrap_err();
    assert!(
        e.contains(&outer.display().to_string()) && e.contains("netsmith.toml"),
        "{e}"
    );
    assert_eq!(files_in(&inner), ["main.nsm"]);
}

#[test]
fn init_adds_to_a_gitignore_and_keeps_a_readme() {
    let root = TempFile::new("project-test-keep", "p");
    write(&root, ".gitignore", "target");
    write(&root, "README.md", "my own readme\n");
    let said = init(&root, None).unwrap();
    assert_eq!(read(&root, ".gitignore"), "target\n/out/\n/fab/\n");
    assert_eq!(read(&root, "README.md"), "my own readme\n");
    assert!(
        said.contains("updated .gitignore") && said.contains("kept README.md"),
        "{said}"
    );

    // Whatever spelling already ignores a directory is enough.
    let root = TempFile::new("project-test-keep-out", "p");
    write(&root, ".gitignore", "out/\n# fab\n");
    init(&root, None).unwrap();
    assert_eq!(read(&root, ".gitignore"), "out/\n# fab\n/fab/\n");

    let root = TempFile::new("project-test-keep-all", "p");
    write(&root, ".gitignore", "/out/\n/fab/\n");
    let said = init(&root, None).unwrap();
    assert_eq!(read(&root, ".gitignore"), "/out/\n/fab/\n");
    assert!(said.contains("kept .gitignore"), "{said}");
}

#[test]
fn init_writes_the_agent_guidance_and_the_project_agrees_with_the_repository() {
    let root = scaffold("guided");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let repo_file = |f: &str| fs::read_to_string(repo.join(f)).unwrap();

    assert_eq!(
        read(&root, "docs/netsmith/LANGUAGE.md"),
        repo_file("docs/LANGUAGE.md")
    );
    let agents = read(&root, "AGENTS.md");
    for must in [
        "netsmith build",
        "netsmith editor",
        "netsmith fab",
        "docs/netsmith/LANGUAGE.md",
        "Run `netsmith build` before you say a design works",
        "needs a source",
        "Ask the user before a decision about the language",
    ] {
        assert!(agents.contains(must), "{must}: {agents}");
    }

    let skills = root.join(".agents/skills");
    let mut names: Vec<String> = fs::read_dir(&skills)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names.len(), 7, "{names:?}");
    for name in &names {
        assert!(!name.starts_with("netsmith-dev-"), "{name}");
        assert_eq!(
            read(&root, &format!(".agents/skills/{name}/SKILL.md")),
            repo_file(&format!(".agents/skills/{name}/SKILL.md"))
        );
        assert!(agents.contains(&format!("`{name}`")), "{name}");
    }
}

#[cfg(unix)]
#[test]
fn claude_code_finds_the_scaffolded_skills_through_a_relative_link() {
    let root = scaffold("linked");
    let link = root.join(".claude/skills");
    assert_eq!(
        fs::read_link(&link).unwrap(),
        Path::new("../.agents/skills")
    );
    assert_eq!(
        fs::read_to_string(link.join("netsmith-write-nsm/SKILL.md")).unwrap(),
        read(&root, ".agents/skills/netsmith-write-nsm/SKILL.md")
    );
}

#[test]
fn init_keeps_an_agents_md_and_says_what_to_add() {
    let root = TempFile::new("project-test-own-agents", "p");
    write(&root, "AGENTS.md", "mine\n");
    let said = init(&root, None).unwrap();
    assert_eq!(read(&root, "AGENTS.md"), "mine\n");
    let note = said
        .lines()
        .find(|l| l.contains("AGENTS.md exists"))
        .unwrap_or_else(|| panic!("{said}"));
    assert!(
        note.contains("not touched") && note.contains("docs/netsmith/LANGUAGE.md"),
        "{note}"
    );
    assert!(!said.contains("created AGENTS.md"), "{said}");
    assert!(root.join("docs/netsmith/LANGUAGE.md").is_file());
}

#[test]
fn init_keeps_a_guide_a_skill_and_a_claude_directory_that_exist() {
    let root = TempFile::new("project-test-own-guidance", "p");
    write(&root, "docs/netsmith/LANGUAGE.md", "old guide\n");
    write(
        &root,
        ".agents/skills/netsmith-write-nsm/SKILL.md",
        "my skill\n",
    );
    write(&root, ".claude/skills/other/SKILL.md", "other\n");
    let said = init(&root, None).unwrap();
    assert_eq!(read(&root, "docs/netsmith/LANGUAGE.md"), "old guide\n");
    assert_eq!(
        read(&root, ".agents/skills/netsmith-write-nsm/SKILL.md"),
        "my skill\n"
    );
    assert_eq!(read(&root, ".claude/skills/other/SKILL.md"), "other\n");
    assert!(
        said.contains("kept docs/netsmith/LANGUAGE.md")
            && said.contains("created 6 skills")
            && said.contains("kept .claude/skills"),
        "{said}"
    );
    assert!(
        root.join(".agents/skills/netsmith-read-errors/SKILL.md")
            .is_file()
    );
}

#[test]
fn the_scaffolded_board_is_the_led_example() {
    let root = scaffold("ledcopy");
    assert_eq!(
        read(&root, "src/main.nsm"),
        LED.replace("LedExample", "Ledcopy")
    );
}

#[test]
fn the_scaffolded_project_builds() {
    let root = scaffold("buildme");
    let target = Target::resolve_from(&root, None).unwrap();
    let loader = target.loader().unwrap();
    assert_eq!(target.files.len(), 1);
    assert_eq!(target.files[0].name, "src/main.nsm");
    let boards = build_with(
        &target.files[0].name,
        &fs::read_to_string(&target.files[0].path).unwrap(),
        &loader,
        &Catalogs::jlcpcb(),
        &netsmith::Libraries::sample(),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(boards.len(), 1);
    assert_eq!(boards[0].board, "Buildme");
    assert!(boards[0].report.contains("Resistor_SMD:R_0402_1005Metric"));
}

#[test]
fn the_scaffolded_project_passes_fab() {
    let Some(kicad) = KicadCliExe::locate_runnable() else {
        return;
    };
    let root = scaffold("fabme");
    let target = Target::resolve_from(&root, None).unwrap();
    let made = fab_with(
        &target.files[0].name,
        &fs::read_to_string(&target.files[0].path).unwrap(),
        &target.loader().unwrap(),
        &target.fab_out,
        &Catalogs::jlcpcb(),
        &netsmith::Libraries::standard(),
        Some(&kicad),
    )
    .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(made[0].board, "Fabme");
    assert!(root.join("fab/Fabme/Fabme-gerbers.zip").is_file());
}

// ---- finding the project --------------------------------------------------------------

#[test]
fn the_manifest_is_found_from_a_directory_below_the_root() {
    let root = scaffold("deep");
    write(&root, "docs/notes/x.txt", "");
    for start in [
        root.to_path_buf(),
        root.join("src"),
        root.join("docs/notes"),
    ] {
        let start = &start;
        let found = Project::find(start).unwrap().expect("a project");
        assert_eq!(found.root, *root, "from {}", start.display());
        let target = Target::resolve_from(start, None).unwrap();
        assert_eq!(target.files[0].path, root.join("src/main.nsm"));
        assert_eq!(target.build_out, root.join("out"));
        assert_eq!(target.fab_out, root.join("fab"));
        assert_eq!(target.title, "deep");
    }
}

#[test]
fn the_manifest_decides_the_files_and_the_output_directories() {
    let root = dir("custom");
    write(
        &root,
        "netsmith.toml",
        "[project]\nname = \"c\"\ntop_level = [\"src/a.nsm\", \"boards/b.nsm\"]\nout = \"build/sch\"\nfab = \"build/fab\"\n",
    );
    write(&root, "src/a.nsm", LED);
    write(&root, "boards/b.nsm", STATUS);
    let target = Target::resolve_from(&root, None).unwrap();
    let names: Vec<&str> = target.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["src/a.nsm", "boards/b.nsm"]);
    assert_eq!(target.build_out, root.join("build/sch"));
    assert_eq!(target.fab_out, root.join("build/fab"));
}

#[test]
fn with_no_manifest_and_no_file_the_message_says_what_to_do() {
    let root = dir("none");
    let e = Target::resolve_from(&root, None).unwrap_err();
    assert!(
        e.contains("no netsmith.toml") && e.contains("netsmith init"),
        "{e}"
    );
}

#[test]
fn a_listed_file_that_is_missing_is_named() {
    let root = scaffold("missing");
    fs::remove_file(root.join("src/main.nsm")).unwrap();
    let e = Target::resolve_from(&root, None).unwrap_err();
    assert!(e.contains("src/main.nsm") && e.contains("top_level"), "{e}");
}

#[test]
fn a_bad_manifest_is_reported_with_its_place() {
    let root = dir("bad");
    write(
        &root,
        "netsmith.toml",
        "[project]\nname = \"b\"\nkicad = 8\n",
    );
    let e = Target::resolve_from(&root, None).unwrap_err();
    assert!(
        e.contains("kicad = 8") && e.contains("netsmith.toml"),
        "{e}"
    );
    // It also stops a file named inside the project: the project is wrong.
    write(&root, "x.nsm", LED);
    assert!(Target::resolve_from(&root, Some("x.nsm")).is_err());
}

#[test]
fn a_file_with_no_manifest_works_as_before() {
    let root = dir("single");
    write(&root, "led.nsm", LED);
    let target = Target::resolve_from(&root, Some("led.nsm")).unwrap();
    assert!(target.project.is_none());
    assert_eq!(target.files.len(), 1);
    assert_eq!(target.files[0].name, "led.nsm");
    assert_eq!(target.build_out, Path::new("out"));
    assert_eq!(target.fab_out, Path::new("fab"));
    let boards = build_with(
        "led.nsm",
        LED,
        &target.loader().unwrap(),
        &Catalogs::jlcpcb(),
        &netsmith::Libraries::sample(),
    )
    .unwrap();
    assert_eq!(boards[0].board, "LedExample");
}

#[test]
fn a_named_file_in_a_project_keeps_the_default_directories_but_sees_its_modules() {
    let root = scaffold("named-in");
    write(&root, "src/power.nsm", "");
    let target = Target::resolve_from(&root.join("src"), Some("main.nsm")).unwrap();
    assert_eq!(target.build_out, Path::new("out"));
    assert_eq!(target.files[0].path, Path::new("main.nsm"));
    assert!(target.project.is_some());
}

// ---- several files ----------------------------------------------------------------------

/// The status indicator example split in two: its module in `src/parts/status.nsm`, and
/// the board that imports it in `src/main.nsm`.
fn split_project(name: &str) -> TempDir {
    let root = dir(name);
    let (library, board) = STATUS.split_once("board IndicatorExample").unwrap();
    write(&root, "netsmith.toml", &Manifest::new(name).render());
    write(&root, "src/parts/status.nsm", library);
    write(
        &root,
        "src/main.nsm",
        &format!(
            "import std::parts::connector::Header1x2\n\
             import std::sources::Rail\n\
             import std::logic::Logic\n\
             import std::mfg::jlcpcb::TwoLayer\n\
             import src::parts::status::StatusIndicator\n\
             \n\
             board IndicatorExample{board}"
        ),
    );
    root
}

fn build_project(root: &Path) -> Result<Vec<netsmith::Built>, String> {
    let target = Target::resolve_from(root, None)?;
    let mut all = Vec::new();
    for f in &target.files {
        let text = fs::read_to_string(&f.path).unwrap();
        all.extend(build_with(
            &f.name,
            &text,
            &target.loader()?,
            &Catalogs::jlcpcb(),
            &netsmith::Libraries::sample(),
        )?);
    }
    Ok(all)
}

#[test]
fn a_file_imports_another_file_of_the_project_by_its_path_under_src() {
    let root = split_project("split");
    let boards = build_project(&root).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(boards.len(), 1);
    assert_eq!(boards[0].board, "IndicatorExample");

    // It is the same board as the single-file example.
    let whole = netsmith::build_with(
        "status_indicator.nsm",
        STATUS,
        &Loader::std(),
        &Catalogs::jlcpcb(),
        &netsmith::Libraries::sample(),
    )
    .unwrap();
    assert_eq!(boards[0].schematic.len(), whole[0].schematic.len());
    assert_eq!(boards[0].report, whole[0].report);
}

#[test]
fn a_name_the_imported_file_lacks_is_an_error_naming_both() {
    let root = split_project("lacks");
    let main = read(&root, "src/main.nsm").replace("status::StatusIndicator", "status::Nothing");
    write(&root, "src/main.nsm", &main);
    let e = build_project(&root).err().expect("the build should fail");
    assert!(
        e.contains("module `src::parts::status` has no definition `Nothing`"),
        "{e}"
    );
    assert!(
        e.contains("src/main.nsm"),
        "diagnostics name the file under the root: {e}"
    );
}

#[test]
fn an_import_of_a_file_that_is_not_there_names_the_module() {
    let root = split_project("nofile");
    let main = read(&root, "src/main.nsm").replace("src::parts::status", "src::parts::light");
    write(&root, "src/main.nsm", &main);
    let e = build_project(&root).err().expect("the build should fail");
    assert!(e.contains("unknown module `src::parts::light`"), "{e}");
}

#[test]
fn a_mistake_in_an_imported_file_names_that_file() {
    let root = split_project("mistake");
    let library = read(&root, "src/parts/status.nsm").replace("1kohm", "1kV");
    write(&root, "src/parts/status.nsm", &library);
    let e = build_project(&root).err().expect("the build should fail");
    assert!(e.contains("src/parts/status.nsm"), "{e}");
}

#[test]
fn a_file_name_that_cannot_be_imported_is_refused() {
    let root = split_project("oddname");
    write(&root, "src/my-power.nsm", "");
    let e = build_project(&root).err().expect("the build should fail");
    assert!(
        e.contains("my-power.nsm") && e.contains("module name"),
        "{e}"
    );

    let root = split_project("oddname2");
    write(&root, "src/3v3/x.nsm", "");
    let e = build_project(&root).err().expect("the build should fail");
    assert!(e.contains("3v3") && e.contains("module name"), "{e}");
}

#[test]
fn modules_are_the_nsm_files_under_src_and_nothing_else() {
    let root = split_project("modules");
    write(&root, "src/notes.txt", "");
    write(&root, "other/x.nsm", "");
    let keys: Vec<String> = Project::load(&root)
        .unwrap()
        .modules()
        .unwrap()
        .into_iter()
        .map(|m| m.key)
        .collect();
    assert_eq!(keys, ["src::main", "src::parts::status"]);
}

// ---- the program ------------------------------------------------------------------------

#[test]
fn the_editor_shows_a_project_and_builds_again_when_a_module_changes() {
    let root = split_project("viewed");
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    let first = editor.analysis();
    assert_eq!(first.errors, 0, "{:?}", first.problems);
    assert_eq!(first.boards.len(), 1);
    assert_eq!(first.boards[0].name, "IndicatorExample");
    assert_eq!(editor.title(), "viewed");

    // The module is read from disk again, and the mistake is reported in the module's own
    // file, not in the file that imports it.
    let library = read(&root, "src/parts/status.nsm").replace("1kohm", "1kV");
    write(&root, "src/parts/status.nsm", &library);
    let second = editor.analysis();
    assert!(second.version > first.version);
    assert!(second.errors > 0, "{:?}", second.problems);
    assert!(
        second.problems.iter().any(|p| p
            .at
            .as_ref()
            .is_some_and(|at| at.file == "src/parts/status.nsm")),
        "{:?}",
        second.problems
    );
    // The last drawing that built is kept, marked stale.
    assert!(second.boards.iter().all(|b| b.stale), "{:?}", second.boards);
}

// ---- a system in a project --------------------------------------------------------------

const TWO_BOARDS: &str = include_str!("../../../examples/two_boards.nsm");

/// A project whose only top-level file holds a `system`.
fn system_project(name: &str) -> TempDir {
    let root = dir(name);
    write(
        &root,
        "netsmith.toml",
        "[project]\nname = \"pair\"\ntop_level = [\"src/pair.nsm\"]\n",
    );
    write(&root, "src/pair.nsm", TWO_BOARDS);
    root
}

#[test]
fn the_editor_lists_the_boards_of_a_projects_system() {
    let root = system_project("system-view");
    let editor = Workspace::open(Target::resolve_from(&root, None).unwrap())
        .unwrap_or_else(|e| panic!("{e}"));
    let analysis = editor.analysis();
    let names: Vec<&str> = analysis.boards.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(names, ["TwoBoards/power", "TwoBoards/led"], "{analysis:?}");
    assert_eq!(analysis.systems[0].name, "TwoBoards");
}

// ---- a project's own catalogs (ADR 0022) ---------------------------------------------

/// An invented regulator with a part number as its key, as a project module. Not a real
/// part; the catalog file below is invented too.
const ZZ9: &str = r#"import std::sources::Rail

/// An invented fixed-output regulator, picked by its part number.
part Zz9 {
    .vin:  Rail
    .vout: Rail
    .vin.neg ~ .vout.neg

    .dropout:     Voltage
    .max_current: Current
    .vin.voltage >= .vout.voltage + .dropout
    .vin.current == -.vout.current
    -.vout.current <= .max_current

    pins "SOT-23-5" {
        .vin.pos       power       "IN"     "1"
        .vin.neg       power       "GND"    "2"
        .en            input       "EN"     "3"
        .nc            no_connect  "NC"     "4"
        .vout.pos      power       "OUT"    "5"
    }

    .mpn: String
    .package: Package
    catalog zz9 by .mpn { .mpn, .vout.voltage, .dropout, .max_current, .package }
}
"#;

const ZZ9_BOARD: &str = r#"import std::sources::Rail
import src::ics::Zz9

board Zz9Board {
    .input: Rail {
        .voltage &= 5V +/- 5%
        .pos.net &= "VIN"
        .neg.net &= "GND"
    }
    .u: Zz9 { .mpn &= "ZZ9-33-S" }
    .output: Rail {
        .pos.net &= "3V3"
        .neg.net &= "GND"
        .current &= 0mA..50mA
    }
    .input ~ .u.vin
    .u.vout ~ .output
    .u.en ~ .u.vin.pos
}
"#;

const ZZ9_CSV: &str = "mpn,package,vout.voltage,dropout,max_current,library,price,stock,date,source\n\
ZZ9-33-S,SOT-23-5,3.3V +/- 2%,0.3V,0.5A,Basic,0.10,1000,2026-10-31,https://example.invalid/zz9-33-s.pdf\n";

/// A project whose `netsmith.toml` has the `manifest_lines` (about its catalogs), with the
/// regulator module and board, and `catalogs/zz9.csv`.
fn catalog_project(name: &str, manifest_lines: &str, csv: &str) -> TempDir {
    let root = dir(name);
    write(
        &root,
        "netsmith.toml",
        &format!("[project]\nname = \"zz\"\ntop_level = [\"src/main.nsm\"]\n{manifest_lines}\n"),
    );
    write(&root, "src/ics.nsm", ZZ9);
    write(&root, "src/main.nsm", ZZ9_BOARD);
    write(&root, "catalogs/zz9.csv", csv);
    root
}

fn picks(root: &Path) -> Vec<(String, Option<String>)> {
    let target = Target::resolve_from(root, None).unwrap();
    let text = fs::read_to_string(&target.files[0].path).unwrap();
    let checked = netsmith_check::check(&target.files[0].name, &text, &target.loader().unwrap());
    assert!(checked.is_ok(), "{}", checked.render());
    let solution = netsmith_solve::solve(&checked.boards[0], &target.catalogs().unwrap());
    assert!(
        solution.diagnostics.is_empty(),
        "{:?}",
        solution
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
    );
    solution
        .picks
        .iter()
        .map(|p| (p.candidate.clone(), p.mpn.clone()))
        .collect()
}

#[test]
fn a_board_uses_an_ic_that_only_the_projects_catalog_file_has() {
    // The manifest says nothing: `catalogs/*.csv` is read.
    let root = catalog_project("own-ic", "", ZZ9_CSV);
    assert_eq!(
        picks(&root),
        [("ZZ9-33-S".to_string(), Some("ZZ9-33-S".to_string()))]
    );
    // The standard catalogs are still there beside it.
    let catalogs = Target::resolve_from(&root, None)
        .unwrap()
        .catalogs()
        .unwrap();
    assert!(catalogs.contains("resistor") && catalogs.contains("zz9"));
}

#[test]
fn with_no_folder_and_no_listing_the_catalog_does_not_exist() {
    let root = catalog_project("unlisted", "catalog_dirs = []", ZZ9_CSV);
    let target = Target::resolve_from(&root, None).unwrap();
    assert!(!target.catalogs().unwrap().contains("zz9"));
}

/// Who assembles each part of the LED example (a resistor, an LED and a header) when it is
/// built the way the editor and `compare` build it, in a project with `manifest_line`.
fn assembled_by(name: &str, manifest_line: &str) -> Vec<(String, String)> {
    let root = dir(name);
    write(
        &root,
        "netsmith.toml",
        &format!("[project]\nname = \"asm\"\n{manifest_line}\n"),
    );
    write(&root, "src/main.nsm", LED);
    let target = Target::resolve_from(&root, None).unwrap();
    let text = fs::read_to_string(&target.files[0].path).unwrap();
    let outcome = netsmith::outcome_with(
        &target.files[0].name,
        &text,
        &target.loader().unwrap(),
        &target.catalogs().unwrap(),
        None,
    )
    .unwrap_or_else(|p| panic!("{}", netsmith::render_problems(&p)));
    outcome.boards[0]
        .sourcing
        .iter()
        .map(|s| (s.refdes.clone(), s.assembly.word().to_string()))
        .collect()
}

#[test]
fn the_manifest_default_assembly_reaches_the_build() {
    let pairs = |v: &[(&str, &str)]| -> Vec<(String, String)> {
        v.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    };
    // Today's rule: the header is through hole, so it is soldered by hand.
    assert_eq!(
        assembled_by("asm-none", ""),
        pairs(&[("D1", "fab"), ("J1", "hand"), ("R1", "fab")])
    );
    assert_eq!(
        assembled_by("asm-fab", "assembly = \"fab\""),
        pairs(&[("D1", "fab"), ("J1", "fab"), ("R1", "fab")])
    );
    assert_eq!(
        assembled_by("asm-hand", "assembly = \"hand\""),
        pairs(&[("D1", "hand"), ("J1", "hand"), ("R1", "hand")])
    );
}

#[test]
fn a_catalog_file_the_manifest_lists_but_is_missing_is_reported() {
    let root = catalog_project(
        "own-missing",
        "catalogs = [\"catalogs/nope.csv\"]\ncatalog_dirs = []",
        ZZ9_CSV,
    );
    let e = Target::resolve_from(&root, None)
        .unwrap()
        .catalogs()
        .unwrap_err();
    assert!(
        e.contains("netsmith.toml") && e.contains("catalogs/nope.csv"),
        "{e}"
    );
}

/// The catalog files of the project at `root`, as `catalog: file` pairs.
fn catalog_files(root: &Path) -> Result<Vec<String>, String> {
    Project::load(root)
        .and_then(|p| p.catalog_files())
        .map(|files| {
            files
                .into_iter()
                .map(|f| format!("{}: {}", f.name, f.file))
                .collect()
        })
}

#[test]
fn every_csv_of_the_catalogs_folder_is_a_catalog_in_file_name_order() {
    let root = dir("folder-default");
    write(&root, "netsmith.toml", "[project]\nname = \"p\"\n");
    for file in ["zeta", "alpha", "mid_1"] {
        write(&root, &format!("catalogs/{file}.csv"), "mpn\n");
    }
    // Not catalogs: other kinds of file, and a sub-folder, are not read (not even an
    // odd name inside it).
    write(&root, "catalogs/README.md", "notes");
    write(&root, "catalogs/old/odd-name.csv", "x");
    assert_eq!(
        catalog_files(&root).unwrap(),
        [
            "alpha: catalogs/alpha.csv",
            "mid_1: catalogs/mid_1.csv",
            "zeta: catalogs/zeta.csv"
        ]
    );
    // The same files whether the key is left out or written.
    write(
        &root,
        "netsmith.toml",
        "[project]\nname = \"p\"\ncatalog_dirs = [\"catalogs\"]\n",
    );
    assert_eq!(catalog_files(&root).unwrap().len(), 3);
    // `[]` reads no folder; a folder that does not exist holds nothing.
    write(
        &root,
        "netsmith.toml",
        "[project]\nname = \"p\"\ncatalog_dirs = []\n",
    );
    assert!(catalog_files(&root).unwrap().is_empty());
    write(
        &root,
        "netsmith.toml",
        "[project]\nname = \"p\"\ncatalog_dirs = [\"nowhere\"]\n",
    );
    assert!(catalog_files(&root).unwrap().is_empty());
}

#[test]
fn more_folders_and_single_files_add_to_the_default_in_the_order_written() {
    let root = dir("folder-more");
    write(
        &root,
        "proj/netsmith.toml",
        "[project]\nname = \"p\"\ncatalog_dirs = [\"catalogs\", \"../shared\"]\ncatalogs = [\"extra/z_one.csv\"]\n",
    );
    write(&root, "proj/catalogs/b.csv", "mpn\n");
    write(&root, "shared/a.csv", "mpn\n");
    write(&root, "proj/extra/z_one.csv", "mpn\n");
    // A file of `extra/` that the list does not name is not a catalog.
    write(&root, "proj/extra/other.csv", "mpn\n");
    assert_eq!(
        catalog_files(&root.join("proj")).unwrap(),
        [
            "b: catalogs/b.csv",
            "a: ../shared/a.csv",
            "z_one: extra/z_one.csv"
        ]
    );
}

#[test]
fn a_catalog_folder_that_is_a_file_is_refused() {
    let root = dir("folder-file");
    write(
        &root,
        "netsmith.toml",
        "[project]\nname = \"p\"\ncatalog_dirs = [\"parts\"]\n",
    );
    write(&root, "parts", "x");
    let e = catalog_files(&root).unwrap_err();
    assert!(e.contains("catalog_dirs \"parts\" is not a folder"), "{e}");
}

#[test]
fn the_manifest_checks_its_catalog_list() {
    let with = |line: &str| Manifest::parse(&format!("[project]\nname = \"a\"\n{line}\n"));
    let ok = with("catalogs = [\"catalogs/acme_ldo.csv\", \"parts.csv\"]").unwrap();
    assert_eq!(ok.catalogs, ["catalogs/acme_ldo.csv", "parts.csv"]);
    // Nothing is written for an empty `catalogs` list; a scaffold writes the folder.
    let scaffold = Manifest::new("a").render();
    assert!(!scaffold.contains("\ncatalogs ="), "{scaffold}");
    assert!(
        scaffold.contains("catalog_dirs = [\"catalogs\"]"),
        "{scaffold}"
    );
    let m = Manifest {
        catalogs: ok.catalogs,
        ..Manifest::new("a")
    };
    assert_eq!(Manifest::parse(&m.render()), Ok(m));
    for (line, expect) in [
        (
            "catalogs = [\"../x.csv\"]",
            "catalogs \"../x.csv\" must be a path inside",
        ),
        ("catalogs = [\"/etc/x.csv\"]", "/etc/x.csv"),
        (
            "catalogs = [\"catalogs/x.txt\"]",
            "must be named after its catalog and end in .csv",
        ),
        (
            "catalogs = [\"catalogs/3x.csv\"]",
            "must be named after its catalog",
        ),
        (
            "catalogs = [\"a/ldo.csv\", \"b/ldo.csv\"]",
            "two files for the catalog `ldo`",
        ),
    ] {
        let e = with(line).unwrap_err();
        assert!(e.contains(expect), "{line}: {e}");
    }
}
