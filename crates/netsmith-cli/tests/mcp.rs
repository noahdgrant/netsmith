//! `netsmith mcp` (issue #1106): the server is started over stdio and initialised, its tools are
//! the commands of clap's model with their arguments, `board check` and `board build` of an
//! example through it give what the command line's `--format json` gives, and the project's
//! files, the guide and the skills are its resources and prompts.

use std::fs;
use std::io::{BufRead, BufReader, Lines, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use netsmith_testkit::TempDir;
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
        let id = self.next;
        self.next += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
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
        assert_eq!(tool["title"], format!("netsmith {}", words.join(" ")));
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

/// The listed tools are short (issue #1133): a host sends the whole list to its model. The limit
/// counts the bytes of the list, not a time; the list was about 140 KB with each command's
/// whole help, and is under half of that now.
const MOST_BYTES: usize = 70_000;

#[test]
fn the_tool_list_is_short() {
    let root = led("mcp-short");
    let mut server = Server::start(&root);
    let listed = server.request("tools/list", json!({}));
    let tools = listed["result"]["tools"].as_array().unwrap();
    let bytes = serde_json::to_string(tools).unwrap().len();
    eprintln!("{} tools, {bytes} bytes", tools.len());
    assert!(bytes < MOST_BYTES, "the tool list is {bytes} bytes");
    for tool in tools {
        // The first line of the command's help, and one line for each argument.
        let description = tool["description"].as_str().unwrap();
        assert!(!description.contains('\n'), "{description}");
        for (key, property) in tool["inputSchema"]["properties"].as_object().unwrap() {
            let help = property["description"].as_str().unwrap();
            assert!(!help.contains('\n'), "{}: {key}: {help}", tool["name"]);
        }
    }
    // The choices and the default of an argument are in its schema.
    let check = tools.iter().find(|t| t["name"] == "board_check").unwrap();
    let format = &check["inputSchema"]["properties"]["format"];
    assert_eq!(format["enum"], json!(["text", "json"]), "{format}");
    assert_eq!(format["default"], "json");
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
