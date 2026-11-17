//! The editor's routes held to the command line (issue #1098): an agent must be able to drive and
//! read everything a person does in the editor, so every route of the editor's server
//! (`crates/netsmith-editor/src/server.rs`, the `.route("/api/...", get(..).post(..))` table of
//! `router`) has a row in [`ROUTES`] below, one for each method of the route, that says one of
//! three things:
//!
//! - `Command`: the command (or commands) that does the same through the same code, as the words
//!   clap's model names it by (`board pins lock`). Each must be a command that exists.
//! - `Pending`: the command that will do it, with the issue that builds it, for a command that
//!   is not merged yet. The test accepts the row until a command of that name exists and then
//!   fails, saying the row is to be changed to `Command`: a gap is never forgotten, and never
//!   left open by an unnoticed merge.
//! - `PageOnly`: why the route is an affordance of the page and needs no command: one line (a
//!   preview of a track being drawn, the socket, the page's file reads and writes, a cancel that
//!   a command line has as Ctrl+C). It is a reviewed list, so a new route is a decision made in
//!   the pull request that adds it.
//!
//! A route of the server with no row fails the test, and so does a row with no route: add the
//! command (or the reason) with the route, so that the next editor feature cannot leave a gap
//! unseen. The table is documented in the guide's commands chapter,
//! `docs/language/27-commands.md` (_Every editor route has a command_).

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::cli_table::leaves;

/// The chapter that documents the table.
const CHAPTER: &str = "docs/language/27-commands.md";

/// How a route is reached from the command line.
enum Way {
    /// The commands, by their words, that do what the route does with the same code.
    Command(&'static [&'static str]),
    /// The command that will, and the issue that builds it. No route is pending now; the variant
    /// stays for the next route that comes before its command.
    #[allow(dead_code)]
    Pending(&'static str, u32),
    /// Why the route is the page's alone.
    PageOnly(&'static str),
}
use Way::{Command, PageOnly, Pending};

const CANCEL: &str =
    "stops a running job of the page; a command stops with Ctrl+C (an MCP call, with the host's cancel)";
const FILES: &str = "the project tree's file operations; an agent has the file system";
const PREVIEW: &str =
    "the preview of a track being drawn (what it would cross, push or reroute); the page asks it at each move of the pointer, and `routes add-track` states its result";

/// Every route, as `METHOD /path`, and how a command line reaches it.
const ROUTES: &[(&str, Way)] = &[
    // The project and its analysis.
    // `project show` prints both, with the orders: the boards, systems, panels and files.
    ("GET /api/project", Command(&["project show"])),
    ("GET /api/analysis", Command(&["board check", "project show"])),
    ("POST /api/analysis/cancel", PageOnly(CANCEL)),
    (
        "POST /api/analysis/run",
        PageOnly("the page's Run again after a cancelled analysis; a command analyses when it runs"),
    ),
    (
        "POST /api/analysis/auto",
        PageOnly(
            "whether the page analyses at each change of a file; a command analyses when it runs",
        ),
    ),
    // The page's text editor and project tree.
    (
        "GET /api/file",
        PageOnly("the page reads the source into its text editor; an agent reads the file"),
    ),
    (
        "PUT /api/file",
        PageOnly("the page saves its text editor's text, with the stamp of what it read; an agent writes the file"),
    ),
    ("POST /api/fs/create-file", PageOnly(FILES)),
    ("POST /api/fs/create-dir", PageOnly(FILES)),
    ("POST /api/fs/rename", PageOnly(FILES)),
    ("POST /api/fs/delete", PageOnly(FILES)),
    ("POST /api/format", Command(&["project fmt"])),
    (
        "POST /api/definition",
        PageOnly("where a name is defined, for a click in the page's source editor; an agent searches the sources"),
    ),
    // The solved design and its views.
    ("GET /api/board", Command(&["board show"])),
    ("GET /api/bom", Command(&["board bom"])),
    ("GET /api/power", Command(&["board power"])),
    ("GET /api/system", Command(&["board system"])),
    ("GET /api/panel", Command(&["board panel"])),
    ("GET /api/swap", Command(&["board swap"])),
    ("POST /api/swap", Command(&["board swap"])),
    ("POST /api/pins/lock", Command(&["board pins lock"])),
    ("POST /api/pins/move", Command(&["board pins move"])),
    // The schematic.
    ("GET /api/schematic.svg", Command(&["view schematic"])),
    ("GET /api/schematic/export", Command(&["view schematic"])),
    ("GET /api/schematic-hints", Command(&["schematic show"])),
    ("PUT /api/schematic-hints", Command(&["schematic set"])),
    // Git and the comparison.
    (
        "GET /api/git",
        PageOnly("the refs the compare menu offers; an agent has `git` itself"),
    ),
    (
        "GET /api/git/file",
        PageOnly("a file at a ref, for the page's read-only view; an agent has `git show REF:PATH`"),
    ),
    ("GET /api/compare", Command(&["board compare"])),
    ("POST /api/compare/cancel", PageOnly(CANCEL)),
    // The catalogs.
    ("POST /api/catalog/refresh", Command(&["catalog refresh"])),
    // The PCB: placing, routing and the copper.
    ("POST /api/pcb/route", Command(&["board fab"])),
    ("POST /api/pcb/cancel", PageOnly(CANCEL)),
    ("POST /api/pcb/place/cancel", PageOnly(CANCEL)),
    ("GET /api/pcb/scene", Command(&["layout parts"])),
    // The fill of the pours is drawn into the 2D picture of the copper.
    ("GET /api/pcb/pours", Command(&["view pcb"])),
    ("GET /api/pcb/3d", Command(&["view 3d", "dev board3d"])),
    ("GET /api/pcb/kicad", Command(&["layout export"])),
    ("POST /api/pcb/edit", Command(&["layout set"])),
    ("POST /api/pcb/regenerate", Command(&["layout regenerate"])),
    (
        "POST /api/pcb/routes",
        Command(&[
            "routes add-track",
            "routes add-via",
            "routes add-zone",
            "routes delete",
            "routes lock",
            "routes unlock",
        ]),
    ),
    ("POST /api/pcb/walk", PageOnly(PREVIEW)),
    ("POST /api/pcb/shove", PageOnly(PREVIEW)),
    ("POST /api/pcb/reroute", PageOnly(PREVIEW)),
    ("GET /api/pcb/findings", Command(&["board drc"])),
    ("GET /api/pcb/summary", Command(&["layout summary"])),
    ("GET /api/pcb/copper", Command(&["board copper"])),
    ("GET /api/pcb/fit", Command(&["layout fit"])),
    // KiCad's check of the board runs in `fab` when KiCad is installed.
    ("POST /api/pcb/drc", Command(&["board fab"])),
    // Build, fab and orders.
    ("POST /api/build", Command(&["board build"])),
    ("POST /api/build/cancel", PageOnly(CANCEL)),
    ("POST /api/fab", Command(&["board fab"])),
    ("POST /api/fab/cancel", PageOnly(CANCEL)),
    // A download is a file the command writes into its folder.
    ("GET /api/download", Command(&["board fab"])),
    // The Gerber view: the package `fab` wrote, or one the page sends, read and drawn as
    // `view gerber` reads and draws it (issue #1107).
    ("GET /api/gerber", Command(&["view gerber"])),
    ("POST /api/gerber", Command(&["view gerber"])),
    ("GET /api/orders", Command(&["project order", "project show"])),
    ("POST /api/order", Command(&["project order"])),
    ("POST /api/order/cancel", PageOnly(CANCEL)),
    ("GET /api/order/download", Command(&["project order"])),
    // The 3D view of a running editor.
    ("GET /api/view", Command(&["view editor-3d"])),
    ("POST /api/view", Command(&["view editor-3d"])),
    ("GET /api/view/picture", Command(&["view editor-3d"])),
    // What the commands tell the page.
    (
        "POST /api/progress",
        PageOnly("the way a run of `board fab --to-editor` or `dev board3d --to-editor` shows its lines in the open page"),
    ),
    (
        "GET /api/events",
        PageOnly("the page's socket: progress, `rebuilt` and the live copper, which a command prints as it runs"),
    ),
];

/// Every route of the server's `router` as `METHOD /path`, read from the source.
fn server_routes() -> BTreeSet<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../netsmith-editor/src/server.rs");
    let text = fs::read_to_string(&path).unwrap();
    let mut found = BTreeSet::new();
    let mut rest = text.as_str();
    while let Some(at) = rest.find(".route(") {
        rest = &rest[at + ".route(".len()..];
        // The call ends at its closing parenthesis.
        let mut depth = 1;
        let end = rest
            .char_indices()
            .find(|&(_, c)| {
                match c {
                    '(' => depth += 1,
                    ')' => depth -= 1,
                    _ => {}
                }
                depth == 0
            })
            .map(|(i, _)| i)
            .expect(".route( is closed");
        let call = &rest[..end];
        let open = call.find('"').expect("a route has its path first") + 1;
        let close = open + call[open..].find('"').expect("the path is closed");
        let route = &call[open..close];
        let handlers = &call[close..];
        for method in ["get", "post", "put", "delete", "patch"] {
            let token = format!("{method}(");
            let named = handlers.match_indices(&token).any(|(i, _)| {
                !handlers[..i].ends_with(|c: char| c.is_alphanumeric() || c == '_')
            });
            if named {
                found.insert(format!("{} {route}", method.to_uppercase()));
            }
        }
        rest = &rest[end..];
    }
    found
}

#[test]
fn every_route_of_the_server_has_a_row() {
    let routes = server_routes();
    assert!(routes.len() > 50, "found only {routes:?}");
    let rows: BTreeSet<&str> = ROUTES.iter().map(|(route, _)| *route).collect();
    assert_eq!(rows.len(), ROUTES.len(), "a route has two rows");
    let missing: Vec<_> = routes.iter().filter(|r| !rows.contains(r.as_str())).collect();
    assert!(
        missing.is_empty(),
        "the editor's server has routes that the table of tests/editor_routes.rs does not know: \
         {missing:?}. Give each the command that does the same through the same code (`Command`), \
         the command that is still to come (`Pending`), or the reason it is the page's alone \
         (`PageOnly`). An agent must be able to do everything the editor does (issue #1098)."
    );
    let gone: Vec<_> = rows.iter().filter(|r| !routes.contains(**r)).collect();
    assert!(
        gone.is_empty(),
        "tests/editor_routes.rs has rows for routes the server no longer has: {gone:?}"
    );
}

#[test]
fn every_row_names_a_command_that_exists_or_says_why_not() {
    let commands: BTreeSet<String> = leaves().into_iter().map(|(name, _)| name).collect();
    for (route, way) in ROUTES {
        match way {
            Command(names) => {
                assert!(!names.is_empty(), "{route} names no command");
                for name in *names {
                    assert!(
                        commands.contains(*name),
                        "{route}: `netsmith {name}` is not a command"
                    );
                }
            }
            Pending(name, issue) => {
                assert!(
                    !commands.contains(*name),
                    "{route}: `netsmith {name}` exists now (issue #{issue}): make the row \
                     `Command(&[\"{name}\"])`"
                );
            }
            PageOnly(why) => {
                assert!(
                    why.len() > 20 && !why.contains('\n'),
                    "{route}: a page-only route says why in one line: `{why}`"
                );
            }
        }
    }
}

#[test]
fn the_guide_says_where_the_table_is() {
    let text = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(CHAPTER))
        .unwrap();
    assert!(
        text.contains("tests/editor_routes.rs"),
        "{CHAPTER} documents the table of editor routes (`crates/netsmith-cli/tests/editor_routes.rs`)"
    );
}
