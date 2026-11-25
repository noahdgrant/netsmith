//! `netsmith mcp` (issue #1106): the server is started over stdio and initialised, its tools are
//! the commands of clap's model with their arguments, `board check` and `board build` of an
//! example through it give what the command line's `--format json` gives, and the project's
//! files, the guide and the skills are its resources and prompts.

use std::fs;
use std::io::{BufRead, BufReader, Lines, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use netsmith_editor::api::{
    CameraPlace, CaseMode, ClientMessage, ColourBy, RenderView, ServerMessage, ViewState,
};
use netsmith_fabricate::project::Target;
use netsmith_testkit::TempDir;
use netsmith_workspace::Workspace;
use serde_json::{Value, json};

const LED: &str = include_str!("../../../examples/led.nsm");

/// A project with the LED example as its board.
fn led(label: &str) -> TempDir {
    let root = TempDir::new(label);
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("netsmith.toml"), "[project]\nname = \"led\"\n").unwrap();
    fs::write(root.join("src/main.nsm"), LED).unwrap();
    root
}

/// `netsmith` with the environment of the other tests of the binary: no KiCad, the sample
/// libraries.
fn netsmith(cwd: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_netsmith"));
    command
        .current_dir(cwd)
        .env("KICAD_CLI", "/nonexistent/kicad-cli")
        .env("NETSMITH_KICAD_SYMBOLS", netsmith_testkit::sample_symbols())
        .env(
            "NETSMITH_KICAD_FOOTPRINTS",
            netsmith_testkit::sample_footprints(),
        );
    command
}

/// The command line's own JSON for `args` (with `--format json`), and its exit code.
fn command_line(cwd: &Path, args: &[&str]) -> (Value, Option<i32>) {
    let run = netsmith(cwd)
        .args(args)
        .args(["--format", "json"])
        .output()
        .unwrap();
    let doc = serde_json::from_slice(&run.stdout).unwrap_or_else(|e| {
        panic!(
            "{args:?}: not JSON ({e}): {}{}",
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        )
    });
    (doc, run.status.code())
}

/// A running `netsmith mcp`, initialised.
struct Server {
    child: Child,
    input: Option<ChildStdin>,
    output: Lines<BufReader<ChildStdout>>,
    next: u64,
    /// What `initialize` answered as the server's instructions.
    instructions: String,
}

impl Server {
    fn start(cwd: &Path) -> Server {
        Server::start_with(cwd, &[])
    }

    /// `netsmith mcp` with `args`, started in `cwd`.
    fn start_with(cwd: &Path, args: &[&str]) -> Server {
        let mut child = netsmith(cwd)
            .arg("mcp")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut server = Server {
            child,
            input,
            output,
            next: 1,
            instructions: String::new(),
        };
        let init = server.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0" },
            }),
        );
        assert_eq!(init["result"]["protocolVersion"], "2025-06-18", "{init}");
        assert_eq!(init["result"]["serverInfo"]["name"], "netsmith", "{init}");
        for capability in ["tools", "resources", "prompts"] {
            assert!(
                init["result"]["capabilities"][capability].is_object(),
                "{init}"
            );
        }
        server.instructions = init["result"]["instructions"].as_str().unwrap().to_string();
        server.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        server
    }

    fn send(&mut self, message: Value) {
        let input = self.input.as_mut().unwrap();
        writeln!(input, "{message}").unwrap();
        input.flush().unwrap();
    }

    /// Sends a request and returns its answer, past any notification before it.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.ask(method, params);
        self.answer_of(id)
    }

    /// Sends a request and returns its id, to read the answer later with [`Server::answer_of`].
    fn ask(&mut self, method: &str, params: Value) -> u64 {
        let id = self.next;
        self.next += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        id
    }

    /// The answer to the request `id`, past any notification before it.
    fn answer_of(&mut self, id: u64) -> Value {
        loop {
            let line = self
                .output
                .next()
                .expect("the server answers before it ends")
                .unwrap();
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id {
                return message;
            }
            assert!(message.get("id").is_none(), "an answer to another request: {line}");
        }
    }

    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let answer = self.request(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        );
        answer["result"].clone()
    }
}

impl Drop for Server {
    /// Closing stdin ends the server.
    fn drop(&mut self) {
        drop(self.input.take());
        let _ = self.child.wait();
    }
}

/// Every command of clap's model that is a tool, as its words, with its model.
fn commands() -> Vec<(Vec<String>, clap::Command)> {
    fn walk(path: &[String], command: &clap::Command, all: &mut Vec<(Vec<String>, clap::Command)>) {
        let subs: Vec<_> = command
            .get_subcommands()
            .filter(|s| s.get_name() != "help")
            .collect();
        if subs.is_empty() && !path.is_empty() {
            all.push((path.to_vec(), command.clone()));
        }
        for sub in subs {
            let mut path = path.to_vec();
            path.push(sub.get_name().to_string());
            walk(&path, sub, all);
        }
    }
    let mut all = Vec::new();
    walk(&[], &crate::cli_help::model(), &mut all);
    all.retain(|(words, _)| words != &["mcp"] && words != &["view", "editor"]);
    all
}

#[test]
fn every_tool_is_a_command_with_its_arguments_and_every_command_a_tool() {
    let root = led("mcp-tools");
    let mut server = Server::start(&root);
    let listed = server.request("tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().unwrap().clone();
    let commands = commands();
    assert_eq!(tools.len(), commands.len(), "{listed}");
    for (words, model) in &commands {
        let name = words.join("_").replace('-', "_");
        let tool = tools
            .iter()
            .find(|t| t["name"] == name.as_str())
            .unwrap_or_else(|| panic!("no tool for `netsmith {}`", words.join(" ")));
        let mut properties: Vec<String> = tool["inputSchema"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        properties.sort();
        let mut args: Vec<String> = model
            .get_arguments()
            .filter(|a| !a.is_hide_set())
            .map(|a| a.get_id().to_string())
            .filter(|id| id != "help" && id != "version")
            .collect();
        args.sort();
        assert_eq!(properties, args, "`netsmith {}`", words.join(" "));
    }
    // The server and the editor's server are not tools.
    for name in ["mcp", "view_editor"] {
        assert!(tools.iter().all(|t| t["name"] != name), "{name}");
    }
}

#[test]
fn check_and_build_through_the_server_are_the_command_lines_json() {
    let root = led("mcp-check-build");
    let mut server = Server::start(&root);
    for (tool, args) in [
        ("board_check", &["board", "check"]),
        ("board_build", &["board", "build"]),
    ] {
        let (expected, code) = command_line(&root, args);
        assert_eq!(code, Some(0), "{expected:#}");
        let result = server.call(tool, json!({}));
        assert_eq!(result["isError"], false, "{tool}: {result:#}");
        assert_eq!(result["structuredContent"], expected, "{tool}");
        // The JSON is the text too, for a host that reads only text.
        let text: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, expected, "{tool}");
    }
    assert!(root.join("out").is_dir(), "the build wrote its folder");
}

/// The server says the warning of a project written for another release as it opens it, and the
/// commands it runs do not say it again, until the project's release changes (issue #1657).
#[test]
fn a_project_of_another_release_is_warned_of_once_as_the_server_opens_it() {
    const WARNED: &str = "the project is written for netsmith";
    let root = led("mcp-release");
    let manifest = |written_for: &str| {
        fs::write(
            root.join("netsmith.toml"),
            format!("[project]\nname = \"led\"\nnetsmith = \"{written_for}\"\n"),
        )
        .unwrap();
    };
    manifest("0.0.1");
    let mut server = Server::start(&root);
    assert_eq!(
        server.instructions.matches(WARNED).count(),
        1,
        "{}",
        server.instructions
    );
    let result = server.call("board_check", json!({}));
    assert_eq!(result["isError"], false, "{result:#}");
    assert!(!result.to_string().contains(WARNED), "{result:#}");
    // The command line on its own says it.
    let (alone, _) = command_line(&root, &["board", "check"]);
    assert!(alone.to_string().contains(WARNED), "{alone:#}");
    // Another release was not said: the command says it.
    manifest("0.0.2");
    let result = server.call("board_check", json!({}));
    assert_eq!(
        result["structuredContent"]["findings"][0]["code"],
        "release",
        "{result:#}"
    );
}

#[test]
fn a_command_with_json_added_later_returns_its_json_through_the_server() {
    let root = led("mcp-explain-lock");
    let mut server = Server::start(&root);
    // `board explain` and `project lock --check` (issue #1110): the document is the command line's.
    for (tool, args, call) in [
        ("board_explain", &["board", "explain", "r1"][..], json!({ "path": "r1" })),
        (
            "project_lock",
            &["project", "lock", "--check"][..],
            json!({ "check": true }),
        ),
    ] {
        let (expected, code) = command_line(&root, args);
        let result = server.call(tool, call);
        assert_eq!(result["isError"], code != Some(0), "{tool}: {result:#}");
        assert_eq!(result["structuredContent"], expected, "{tool}");
    }
}

#[test]
fn a_failed_command_is_a_tool_error_with_its_exit_code() {
    let root = led("mcp-errors");
    let broken = LED.replacen(".r1: Resistor", ".r1: Resistr", 1);
    assert_ne!(broken, LED);
    fs::write(root.join("src/main.nsm"), broken).unwrap();
    let mut server = Server::start(&root);
    // What the command checked failed: exit code 1, with its findings.
    let (expected, code) = command_line(&root, &["board", "check"]);
    assert_eq!(code, Some(1));
    let result = server.call("board_check", json!({}));
    assert_eq!(result["isError"], true, "{result:#}");
    assert_eq!(result["structuredContent"], expected);
    let texts = result["content"].to_string();
    assert!(texts.contains("exit code 1"), "{texts}");
    // It could not run: exit code 2, with clap's message.
    let result = server.call("board_check", json!({ "file": "missing.nsm" }));
    assert_eq!(result["isError"], true, "{result:#}");
    assert!(result["content"].to_string().contains("exit code 2"), "{result:#}");
    // An argument the command lacks is the tool's error, and the server goes on.
    let result = server.call("board_check", json!({ "frobnicate": true }));
    assert_eq!(result["isError"], true, "{result:#}");
    let answer = server.request("tools/call", json!({ "name": "frobnicate" }));
    assert!(answer["error"].is_object(), "{answer}");
    assert_eq!(server.request("ping", json!({}))["result"], json!({}));
}

#[test]
fn the_projects_files_and_the_guide_are_resources_and_the_skills_prompts() {
    let root = led("mcp-content");
    let mut server = Server::start(&root);
    let listed = server.request("resources/list", json!({}));
    let uris: Vec<&str> = listed["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uri"].as_str().unwrap())
        .collect();
    for uri in [
        "netsmith://project/netsmith.toml",
        "netsmith://project/src/main.nsm",
        "netsmith://guide/LANGUAGE.md",
        "netsmith://guide/language/27-commands.md",
    ] {
        assert!(uris.contains(&uri), "{uri} in {uris:?}");
    }
    let read = server.request(
        "resources/read",
        json!({ "uri": "netsmith://project/src/main.nsm" }),
    );
    assert_eq!(read["result"]["contents"][0]["text"], LED, "{read}");
    let read = server.request(
        "resources/read",
        json!({ "uri": "netsmith://project/../netsmith.toml" }),
    );
    assert!(read["error"].is_object(), "{read}");

    let prompts = server.request("prompts/list", json!({}));
    let names: Vec<&str> = prompts["result"]["prompts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"netsmith-write-nsm"), "{names:?}");
    let got = server.request(
        "prompts/get",
        json!({ "name": "netsmith-write-nsm", "arguments": { "request": "add an LED" } }),
    );
    let text = got["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(text.ends_with("The request: add an LED"), "{text}");
}

/// The listed tools are short (issue #1133): a host sends the whole list to its model. The limits
/// count bytes of the list, not a time; the list was about 140 KB with each command's whole help,
/// and is under 80 KB now.
///
/// A PR that adds a flag and trips a limit cuts text; it does not raise the limit.
const MOST_BYTES: usize = 90_000;

/// The most bytes of one help line in the list: a command's, or an argument's (issue #1721).
const MOST_LINE_BYTES: usize = 120;

/// The most bytes of help one tool may have in the list, its own line and its arguments' together
/// (issue #1721), so that no command grows without a limit while the whole list has room.
const MOST_TOOL_HELP_BYTES: usize = 2_000;

/// How to make a help line shorter, said by each limit's failure.
const HOW_TO_CUT: &str = "Cut text, do not raise the limit. A tool carries only the first \
    paragraph of a command's or a flag's doc comment: keep that to what an agent needs to use it \
    (what it does, its syntax and choices), and move the rest to a paragraph after an empty `///` \
    line, which `--help` and the command's help resource keep. A flag that many commands take, \
    with one help, is said once in the server's instructions (`tools::share`): give it the same \
    help in each command (a `#[command(flatten)]` struct, as `Placing`)";

/// The help one tool carries in the list.
struct Help {
    /// `netsmith board fab`.
    command: String,
    /// The bytes of the tool as listed.
    bytes: usize,
    /// Each help line, as what to change (`netsmith board fab --strict`) and its bytes, the
    /// longest first.
    lines: Vec<(String, usize)>,
}

impl Help {
    fn of(tool: &Value, commands: &[(Vec<String>, clap::Command)]) -> Help {
        let (words, model) = commands
            .iter()
            .find(|(words, _)| tool["name"] == words.join("_").replace('-', "_").as_str())
            .unwrap_or_else(|| panic!("no command for {}", tool["name"]));
        let command = format!("netsmith {}", words.join(" "));
        let own = tool["description"].as_str().unwrap_or_default();
        let mut lines = vec![(format!("`{command}` (the command's own help)"), own.len())];
        for (key, property) in tool["inputSchema"]["properties"].as_object().unwrap() {
            let arg = model
                .get_arguments()
                .find(|a| a.get_id() == key.as_str())
                .unwrap_or_else(|| panic!("`{command}` has no argument {key}"));
            let flag = match arg.get_long() {
                Some(long) => format!("--{long}"),
                None => format!("<{key}>"),
            };
            let help = property["description"].as_str().unwrap_or_default();
            lines.push((format!("`{command} {flag}`"), help.len()));
        }
        lines.sort_by_key(|(_, bytes)| std::cmp::Reverse(*bytes));
        Help {
            command,
            bytes: serde_json::to_string(tool).unwrap().len(),
            lines,
        }
    }

    fn total(&self) -> usize {
        self.lines.iter().map(|(_, bytes)| bytes).sum()
    }

    /// The longest `n` lines, one to a row.
    fn longest(&self, n: usize) -> String {
        self.lines
            .iter()
            .take(n)
            .map(|(what, bytes)| format!("\n    {what}: {bytes} bytes"))
            .collect()
    }
}

#[test]
fn the_tool_list_is_short() {
    let root = led("mcp-short");
    let mut server = Server::start(&root);
    let listed = server.request("tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().unwrap();
    let bytes = serde_json::to_string(tools).unwrap().len();
    eprintln!("{} tools, {bytes} bytes", tools.len());
    let commands = commands();
    let mut helps: Vec<Help> = tools.iter().map(|t| Help::of(t, &commands)).collect();
    helps.sort_by_key(|h| std::cmp::Reverse(h.bytes));
    let biggest: String = helps
        .iter()
        .take(5)
        .map(|h| {
            format!(
                "\n  `{}`, {} bytes; its longest help:{}",
                h.command,
                h.bytes,
                h.longest(3)
            )
        })
        .collect();
    assert!(
        bytes < MOST_BYTES,
        "the tool list is {bytes} bytes, over the {MOST_BYTES} it may have. {HOW_TO_CUT}. The \
         biggest tools:{biggest}"
    );
    for tool in tools {
        // The first line of the command's help, and one line for each argument.
        let description = tool["description"].as_str().unwrap();
        assert!(!description.contains('\n'), "{description}");
        for (key, property) in tool["inputSchema"]["properties"].as_object().unwrap() {
            // An argument many tools share has its help in the instructions, not here.
            let Some(help) = property["description"].as_str() else {
                assert!(
                    server.instructions.contains(&format!("`{key}`: ")),
                    "{}: {key} has no help, here or in the instructions",
                    tool["name"]
                );
                continue;
            };
            assert!(!help.contains('\n'), "{}: {key}: {help}", tool["name"]);
        }
    }
    // The choices of an argument are in its schema. That a call asks for `json` is said once, in
    // the instructions, so the command line's own default is not.
    let check = tools.iter().find(|t| t["name"] == "board_check").unwrap();
    let format = &check["inputSchema"]["properties"]["format"];
    assert_eq!(format["enum"], json!(["text", "json"]), "{format}");
    assert!(format.get("default").is_none(), "{format}");
    assert!(
        server.instructions.contains("a call asks for `json`"),
        "{}",
        server.instructions
    );
}

#[test]
fn no_help_line_and_no_tool_has_more_help_than_its_share() {
    let root = led("mcp-help-share");
    let mut server = Server::start(&root);
    let listed = server.request("tools/list", json!({}));
    let commands = commands();
    let mut over = Vec::new();
    for tool in listed["result"]["tools"].as_array().unwrap() {
        let help = Help::of(tool, &commands);
        for (what, bytes) in &help.lines {
            if *bytes > MOST_LINE_BYTES {
                over.push(format!(
                    "{what}: its help line is {bytes} bytes, over {MOST_LINE_BYTES}"
                ));
            }
        }
        if help.total() > MOST_TOOL_HELP_BYTES {
            over.push(format!(
                "`{}`: {} bytes of help, over {MOST_TOOL_HELP_BYTES}; cut the longest:{}",
                help.command,
                help.total(),
                help.longest(8)
            ));
        }
    }
    assert!(over.is_empty(), "{}\n{HOW_TO_CUT}.", over.join("\n"));
}

#[test]
fn every_command_has_a_help_resource_whose_text_is_its_help() {
    let root = led("mcp-help");
    let mut server = Server::start(&root);
    let listed = server.request("resources/list", json!({}));
    let uris: Vec<String> = listed["result"]["resources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["uri"].as_str().unwrap().to_string())
        .filter(|u| u.starts_with("netsmith://help/"))
        .collect();
    let commands = commands();
    // One help resource for each tool, and no other.
    let mut expected: Vec<String> = commands
        .iter()
        .map(|(words, _)| format!("netsmith://help/{}", words.join("-")))
        .collect();
    expected.sort();
    let mut listed_uris = uris.clone();
    listed_uris.sort();
    assert_eq!(listed_uris, expected);
    let mut unique = expected.clone();
    unique.dedup();
    assert_eq!(unique.len(), expected.len(), "two commands share a name");
    for (words, _) in &commands {
        let uri = format!("netsmith://help/{}", words.join("-"));
        let read = server.request("resources/read", json!({ "uri": uri }));
        let text = read["result"]["contents"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("{uri}: {read}"));
        let help = netsmith(&root).args(words).arg("--help").output().unwrap();
        assert_eq!(
            text,
            String::from_utf8_lossy(&help.stdout),
            "`netsmith {} --help`",
            words.join(" ")
        );
    }
    let read = server.request(
        "resources/read",
        json!({ "uri": "netsmith://help/no-such-command" }),
    );
    assert!(read["error"].is_object(), "{read}");
}

#[test]
fn project_is_the_folder_the_server_works_in() {
    let root = led("mcp-project");
    // Started in a folder that is not the project, which the host cannot choose.
    let elsewhere = TempDir::new("mcp-elsewhere");
    let project = root.to_str().unwrap();
    let mut server = Server::start_with(&elsewhere, &["--project", project]);
    let (expected, code) = command_line(&root, &["board", "check"]);
    assert_eq!(code, Some(0), "{expected:#}");
    let result = server.call("board_check", json!({}));
    assert_eq!(result["isError"], false, "{result:#}");
    assert_eq!(result["structuredContent"], expected);
    // The commands write there, not in the folder the server was started in.
    let built = server.call("board_build", json!({}));
    assert_eq!(built["isError"], false, "{built:#}");
    assert!(root.join("out").is_dir());
    assert!(!elsewhere.join("out").exists());
    // The resources are the project's, and a path outside it is still refused.
    let listed = server.request("resources/list", json!({}));
    assert!(
        listed["result"]["resources"]
            .to_string()
            .contains("netsmith://project/src/main.nsm"),
        "{listed}"
    );
    let read = server.request(
        "resources/read",
        json!({ "uri": "netsmith://project/src/main.nsm" }),
    );
    assert_eq!(read["result"]["contents"][0]["text"], LED, "{read}");
    fs::write(elsewhere.join("secret.txt"), "x").unwrap();
    for uri in [
        "netsmith://project/../secret.txt",
        &format!("netsmith://project/{}/secret.txt", elsewhere.to_str().unwrap()),
    ] {
        let read = server.request("resources/read", json!({ "uri": uri }));
        assert!(read["error"].is_object(), "{uri}: {read}");
    }
}

#[test]
fn a_project_that_is_not_a_folder_is_an_error() {
    let root = led("mcp-no-folder");
    let run = netsmith(&root)
        .args(["mcp", "--project", "no-such-folder"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(run.status.code(), Some(2));
    let said = String::from_utf8_lossy(&run.stderr);
    assert!(said.contains("not a folder"), "{said}");
}

/// The editor's server over the project `root` (the LED example), on a free port.
fn start_editor(root: &Path) -> u16 {
    use std::net::{Ipv4Addr, TcpListener};
    let workspace = Workspace::open(Target::resolve_from(root, None).unwrap()).unwrap();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || netsmith_editor::serve(listener, std::sync::Arc::new(workspace)));
    port
}

type Page = tungstenite::WebSocket<std::net::TcpStream>;

/// A page's socket, as the browser opens it.
fn open_page(port: u16) -> Page {
    let stream = std::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).unwrap();
    let url = format!("ws://127.0.0.1:{port}/api/events");
    tungstenite::client(url.as_str(), stream).unwrap().0
}

/// The next ask for the view, skipping the pushes of the server.
fn next_ask(page: &mut Page) -> ServerMessage {
    loop {
        let tungstenite::Message::Text(text) = page.read().expect("an ask of the view") else {
            continue;
        };
        let m: ServerMessage = serde_json::from_str(&text).unwrap();
        if matches!(m, ServerMessage::View { .. } | ServerMessage::ViewPicture { .. }) {
            return m;
        }
    }
}

fn page_state() -> ViewState {
    ViewState {
        board: Some("Led".into()),
        member: None,
        shown: true,
        view: Some(RenderView::Iso),
        camera: Some(CameraPlace {
            position: [40.0, -30.0, 40.0],
            target: [10.0, 8.0, 0.0],
            up: Some([0.0, 0.0, 1.0]),
        }),
        ortho: Some(true),
        section: None,
        exploded: Some(false),
        highlight: vec!["J1".into()],
        selection: vec![],
        modules: vec![],
        hover: None,
        xray: false,
        case: Some(CaseMode::SeeThrough),
        case_hidden: None,
        colour_by: ColourBy::Provenance,
        height_preview: None,
        theme: "dark".into(),
        command: None,
    }
}

/// Attached to an editor by its port, `view_editor_3d` changes the page the person looks at, as
/// the person's own page answers, and the picture of its canvas is in the result as an image.
#[test]
fn attached_the_page_tool_changes_the_page_with_no_port_and_returns_its_picture() {
    let root = led("mcp-attach");
    let port = start_editor(&root);
    let mut page = open_page(port);
    let mut server = Server::start_with(&root, &["--editor", &port.to_string()]);

    let init = server.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
    let told = init["result"]["instructions"].as_str().unwrap();
    assert!(told.contains(&format!("127.0.0.1:{port}")), "{told}");
    let listed = server.request("tools/list", json!({}));
    let tool = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "view_editor_3d")
        .unwrap();
    assert_eq!(tool["inputSchema"]["properties"]["port"]["default"], port);

    // The call names no port and no editor but the attached one answers.
    let dir = TempDir::new("mcp-attach-picture");
    let png = dir.join("j1.png");
    let id = server.ask(
        "tools/call",
        json!({
            "name": "view_editor_3d",
            "arguments": { "highlight": "J1", "view": "top", "picture": png.to_str().unwrap() },
        }),
    );
    let ServerMessage::View { id: asked, change: Some(change) } = next_ask(&mut page) else {
        panic!("a change was asked of the page")
    };
    assert_eq!(change.highlight, Some(vec!["J1".to_string()]));
    let mut after = page_state();
    after.view = Some(RenderView::Top);
    let reply = ClientMessage::ViewState { id: asked, state: Box::new(after) };
    page.send(tungstenite::Message::text(serde_json::to_string(&reply).unwrap())).unwrap();
    let ServerMessage::ViewPicture { id: asked } = next_ask(&mut page) else {
        panic!("a picture was asked of the page")
    };
    let mut frame = asked.to_le_bytes().to_vec();
    frame.extend_from_slice(b"\x89PNG\r\n\x1a\nthe picture");
    page.send(tungstenite::Message::binary(frame)).unwrap();

    let result = server.answer_of(id)["result"].clone();
    assert_eq!(result["isError"], false, "{result:#}");
    // The state the page answered is the result, as the command's JSON and as structured content.
    assert_eq!(result["structuredContent"]["view"], "top", "{result:#}");
    assert_eq!(result["structuredContent"]["highlight"], json!(["J1"]), "{result:#}");
    // The picture is saved where the call said and comes back as an image.
    let saved = fs::read(&png).unwrap();
    assert_eq!(saved, b"\x89PNG\r\n\x1a\nthe picture");
    let image = result["content"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "image")
        .unwrap_or_else(|| panic!("no image in {result:#}"));
    assert_eq!(image["mimeType"], "image/png");
    let data = image["data"].as_str().unwrap();
    assert!(data.starts_with("iVBORw0KG"), "{data}");
    assert_eq!(data, base64_of(&saved));
}

/// A standard base64, written out here so that the test does not use the code it tests.
fn base64_of(bytes: &[u8]) -> String {
    let digits: Vec<char> =
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/".chars().collect();
    let bits: String = bytes.iter().map(|b| format!("{b:08b}")).collect();
    let mut out = String::new();
    for chunk in bits.as_bytes().chunks(6) {
        let mut six = String::from_utf8(chunk.to_vec()).unwrap();
        while six.len() < 6 {
            six.push('0');
        }
        out.push(digits[usize::from_str_radix(&six, 2).unwrap()]);
    }
    while !out.len().is_multiple_of(4) {
        out.push('=');
    }
    out
}

/// With no port a call looks for the project's editor; where none is running the call says so
/// and the other tools are not touched by it.
#[test]
fn attached_with_no_port_and_no_editor_the_page_tool_says_how_to_start_one() {
    let root = led("mcp-attach-find");
    let mut server = Server::start_with(&root, &["--editor"]);
    let result = server.call("view_editor_3d", json!({}));
    assert_eq!(result["isError"], true, "{result:#}");
    let told = result["content"].to_string();
    assert!(
        told.contains("no editor of the project") && told.contains("netsmith view editor"),
        "{told}"
    );
    // The server goes on, and a tool that does not need the editor runs.
    let result = server.call("board_check", json!({}));
    assert_eq!(result["isError"], false, "{result:#}");
}

/// Not attached, the server is as it was: no word of an editor and no port filled in.
#[test]
fn not_attached_the_page_tool_has_its_own_default_port() {
    let root = led("mcp-no-attach");
    let mut server = Server::start(&root);
    let init = server.request("initialize", json!({ "protocolVersion": "2025-06-18" }));
    assert!(!init["result"]["instructions"].as_str().unwrap().contains("attached"));
    let listed = server.request("tools/list", json!({}));
    let tool = listed["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "view_editor_3d")
        .unwrap();
    assert_eq!(tool["inputSchema"]["properties"]["port"]["default"], 8787);
}
