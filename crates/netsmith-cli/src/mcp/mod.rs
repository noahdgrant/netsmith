//! `netsmith mcp`: a Model Context Protocol server for the command line, on stdin and stdout
//! (issue #1106). An agent host (Claude Code, Claude Desktop, an editor) starts it and finds the
//! commands as tools ([`tools`]), the project's files and the language guide as resources and the
//! skills as prompts ([`content`]); a tool call runs the command ([`run`]).
//!
//! The protocol is written here, by hand, and not with the official Rust SDK (`rmcp`). The SDK's
//! licence (Apache-2.0) would do for this MIT repository, but it is built on an async runtime
//! (tokio, futures, tokio-util, tracing, async-trait) that the command line otherwise does not
//! use, and its tools are declared with macros on functions, where these are read from clap's
//! model as the process runs. What this server speaks is small: JSON-RPC 2.0, one message per
//! line on stdio, with `initialize`, `ping`, `tools/list` and `tools/call`, `resources/list` and
//! `resources/read`, `prompts/list` and `prompts/get`, and the `notifications/progress` and
//! `notifications/cancelled` notifications. Each tool call runs on a thread of its own, so the
//! loop goes on reading and a cancel reaches the call it names.

mod content;
mod run;
mod tools;

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::{Map, Value, json};

use run::Job;
use tools::Tool;

/// The versions of the protocol this server speaks, the newest first. A host that asks for
/// another is answered with the newest, and it decides whether to go on.
const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// What the server tells the host's model when it starts.
const INSTRUCTIONS: &str = "netsmith is a language and tools for designing electronics as code. Each tool runs one command of the `netsmith` command line, in the project folder of this server, as a person runs it; the commands that write files write them there. A tool has the first line of its command's help: the whole `--help` of a command is the resource netsmith://help/<its words joined by `-`> (netsmith://help/board-fab), to read when a command's flags or output are in doubt. Where a command has `--format json` or `--progress json` a call asks for `json` unless its arguments give another; the result is then the command's JSON document, also as structured content, and what the command says on stderr follows. An exit code of 1 (what the command checked failed) or 2 (it could not run, a usage mistake included) makes the result an error. Read the resource netsmith://guide/LANGUAGE.md and the chapters it links before writing .nsm source, and use the prompts (netsmith's skills) for a task they name.";

/// JSON-RPC's codes.
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
/// The protocol's code for a resource that is not there.
const RESOURCE_NOT_FOUND: i64 = -32002;

/// Serves on this process's stdin and stdout until stdin closes; the commands run from this
/// binary in `project`, or in the current folder when there is none (issue #1133).
pub fn serve_stdio(project: Option<&Path>) -> std::io::Result<()> {
    let dir = match project {
        // Not `canonicalize`: on Windows it gives the `\\?\` form, which the commands print.
        Some(project) => std::path::absolute(project)?,
        None => std::env::current_dir()?,
    };
    if !dir.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("--project {}: not a folder", dir.display()),
        ));
    }
    let server = Server::new(std::env::current_exe()?, dir);
    server.serve(std::io::stdin().lock(), std::io::stdout());
    Ok(())
}

struct Server {
    /// The `netsmith` binary a tool call runs.
    program: PathBuf,
    /// The folder the commands run in: `--project`, or the folder the server was started in.
    dir: PathBuf,
    tools: Vec<Tool>,
    /// The tool calls running, by their request id (as its JSON text).
    jobs: Mutex<HashMap<String, Arc<Job>>>,
}

/// Where the messages go: one line each, never two at once.
struct Out<W: Write>(Mutex<W>);

impl<W: Write> Out<W> {
    fn send(&self, message: &Value) {
        let mut out = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let _ = writeln!(out, "{message}");
        let _ = out.flush();
    }

    fn answer(&self, id: &Value, result: Value) {
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "result": result }));
    }

    fn error(&self, id: &Value, code: i64, message: &str) {
        self.send(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message },
        }));
    }
}

impl Server {
    fn new(program: PathBuf, dir: PathBuf) -> Server {
        Server {
            program,
            dir,
            tools: tools::tools(),
            jobs: Mutex::new(HashMap::new()),
        }
    }

    /// Reads messages from `input` and answers on `output` until `input` ends; then stops the
    /// commands still running, as a terminal that closes stops them, and returns once they end.
    fn serve(&self, input: impl BufRead, output: impl Write + Send) {
        let out = Out(Mutex::new(output));
        std::thread::scope(|scope| {
            for line in input.split(b'\n') {
                let Ok(line) = line else { break };
                let line = String::from_utf8_lossy(&line);
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let message: Value = match serde_json::from_str(line) {
                    Ok(m) => m,
                    Err(e) => {
                        out.error(&Value::Null, PARSE_ERROR, &format!("not JSON: {e}"));
                        continue;
                    }
                };
                let Some(method) = message.get("method").and_then(Value::as_str) else {
                    // An answer to a request: this server sends none.
                    if message.get("id").is_none() || !message.is_object() {
                        out.error(&Value::Null, INVALID_REQUEST, "not a JSON-RPC message");
                    }
                    continue;
                };
                let params = message.get("params").cloned().unwrap_or(json!({}));
                let Some(id) = message.get("id").cloned() else {
                    self.notified(method, &params);
                    continue;
                };
                if method == "tools/call" {
                    let out = &out;
                    scope.spawn(move || self.call(out, id, &params));
                } else {
                    match self.request(method, &params) {
                        Ok(result) => out.answer(&id, result),
                        Err((code, message)) => out.error(&id, code, &message),
                    }
                }
            }
            for job in self.jobs.lock().unwrap_or_else(|e| e.into_inner()).values() {
                job.cancel();
            }
        });
    }

    fn notified(&self, method: &str, params: &Value) {
        if method == "notifications/cancelled"
            && let Some(id) = params.get("requestId")
            && let Some(job) = self
                .jobs
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&id.to_string())
        {
            job.cancel();
        }
        // `notifications/initialized` and the rest need nothing.
    }

    fn request(&self, method: &str, params: &Value) -> Result<Value, (i64, String)> {
        let root = || content::project_root(&self.dir);
        match method {
            "initialize" => {
                let asked = params["protocolVersion"].as_str().unwrap_or_default();
                let version = VERSIONS
                    .iter()
                    .find(|v| **v == asked)
                    .unwrap_or(&VERSIONS[0]);
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": {
                        "tools": { "listChanged": false },
                        "resources": { "listChanged": false, "subscribe": false },
                        "prompts": { "listChanged": false },
                    },
                    "serverInfo": {
                        "name": "netsmith",
                        "title": "netsmith",
                        "version": env!("CARGO_PKG_VERSION"),
                    },
                    "instructions": INSTRUCTIONS,
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({
                "tools": self.tools.iter().map(Tool::listed).collect::<Vec<_>>(),
            })),
            "resources/list" => Ok(content::resources(root().as_deref(), &self.tools)),
            "resources/templates/list" => Ok(json!({ "resourceTemplates": [] })),
            "resources/read" => {
                let uri = params["uri"]
                    .as_str()
                    .ok_or((INVALID_PARAMS, "`uri` is missing".to_string()))?;
                content::read(root().as_deref(), &self.tools, uri)
                    .map_err(|e| (RESOURCE_NOT_FOUND, e))
            }
            "prompts/list" => Ok(content::prompts()),
            "prompts/get" => {
                let name = params["name"]
                    .as_str()
                    .ok_or((INVALID_PARAMS, "`name` is missing".to_string()))?;
                content::prompt(name, params["arguments"]["request"].as_str())
                    .map_err(|e| (INVALID_PARAMS, e))
            }
            _ => Err((METHOD_NOT_FOUND, format!("no method `{method}`"))),
        }
    }

    /// A tool call: runs the command and answers with what it printed, unless it was cancelled
    /// (a cancelled request is not answered).
    fn call<W: Write>(&self, out: &Out<W>, id: Value, params: &Value) {
        let name = params["name"].as_str().unwrap_or_default();
        let Some(tool) = self.tools.iter().find(|t| t.name == name) else {
            out.error(&id, INVALID_PARAMS, &format!("no tool `{name}`"));
            return;
        };
        let arguments = match params.get("arguments") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(a)) => a.clone(),
            Some(_) => {
                out.error(&id, INVALID_PARAMS, "`arguments` is not an object");
                return;
            }
        };
        // A mistake in the arguments is the tool's error, so the model can mend it.
        let argv = match tool.argv(&arguments) {
            Ok(argv) => argv,
            Err(e) => {
                out.answer(
                    &id,
                    failed(&format!("{e} (exit code 2: the command could not run)")),
                );
                return;
            }
        };
        let key = id.to_string();
        let job = Arc::new(Job::default());
        self.jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.clone(), job.clone());
        let token = params["_meta"].get("progressToken").cloned();
        let mut told = 0u64;
        let ran = run::run(&self.program, &argv, &self.dir, &job, |line| {
            if let Some(token) = &token {
                told += 1;
                out.send(&json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/progress",
                    "params": { "progressToken": token, "progress": told, "message": line },
                }));
            }
        });
        self.jobs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
        if job.is_cancelled() {
            return;
        }
        match ran {
            Ok(ran) => out.answer(&id, result(tool.wants_json(&arguments), ran)),
            Err(e) => out.answer(&id, failed(&format!("the command could not start: {e}"))),
        }
    }
}

/// A tool's result for what its command did.
fn result(json: bool, ran: run::Ran) -> Value {
    let mut content = Vec::new();
    let mut answer = Map::new();
    if !ran.stdout.trim().is_empty() {
        content.push(text(&ran.stdout));
        if json && let Ok(value) = serde_json::from_str::<Value>(&ran.stdout) {
            let structured = match value {
                Value::Object(o) => Value::Object(o),
                other => json!({ "result": other }),
            };
            answer.insert("structuredContent".into(), structured);
        }
    }
    if !ran.stderr.trim().is_empty() {
        content.push(text(&format!("stderr:\n{}", ran.stderr)));
    }
    let failure = match ran.code {
        Some(0) => None,
        Some(1) => Some("exit code 1: what the command checked failed".to_string()),
        Some(2) => Some("exit code 2: the command could not run".to_string()),
        Some(n) => Some(format!("exit code {n}")),
        None => Some("the command was stopped before it ended".to_string()),
    };
    if let Some(failure) = &failure {
        content.push(text(failure));
    } else if content.is_empty() {
        content.push(text("done (exit code 0)"));
    }
    answer.insert("content".into(), Value::Array(content));
    answer.insert("isError".into(), Value::Bool(failure.is_some()));
    Value::Object(answer)
}

fn failed(message: &str) -> Value {
    json!({ "content": [text(message)], "isError": true })
}

fn text(t: &str) -> Value {
    json!({ "type": "text", "text": t })
}

#[cfg(all(test, unix))]
mod tests {
    use std::io::BufReader;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;

    use super::*;

    /// A program in place of `netsmith` that says one line on stderr and waits to be killed.
    fn waits(dir: &Path) -> PathBuf {
        let path = dir.join("waits");
        std::fs::write(&path, "#!/bin/sh\necho started >&2\nexec sleep 100000\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn send(input: &mut impl Write, message: Value) {
        writeln!(input, "{message}").unwrap();
    }

    /// A cancelled call is not answered, its command is stopped, and the server goes on.
    #[test]
    fn a_cancelled_call_has_no_answer_and_the_server_goes_on() {
        let dir = netsmith_testkit::TempDir::new("mcp-server-cancel");
        let program = waits(&dir);
        let (requests, mut input) = std::io::pipe().unwrap();
        let (answers, output) = std::io::pipe().unwrap();
        let server = Arc::new(Server::new(program, dir.to_path_buf()));
        let serving = {
            let server = server.clone();
            std::thread::spawn(move || server.serve(BufReader::new(requests), output))
        };
        let mut lines = BufReader::new(answers).lines();
        send(
            &mut input,
            json!({
                "jsonrpc": "2.0", "id": 7, "method": "tools/call",
                "params": { "name": "board_check", "arguments": {}, "_meta": { "progressToken": "t" } },
            }),
        );
        // The command has started when its first line is told.
        let progress: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
        assert_eq!(progress["method"], "notifications/progress");
        assert_eq!(progress["params"]["message"], "started");
        assert_eq!(progress["params"]["progressToken"], "t");
        send(
            &mut input,
            json!({
                "jsonrpc": "2.0", "method": "notifications/cancelled",
                "params": { "requestId": 7 },
            }),
        );
        send(
            &mut input,
            json!({ "jsonrpc": "2.0", "id": 8, "method": "ping" }),
        );
        let pong: Value = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
        assert_eq!(pong["id"], 8, "{pong}");
        // The cancel reached the call: its job is cancelled, or gone because its command ended,
        // which a command that waits does only when it is killed.
        let jobs = server.jobs.lock().unwrap();
        assert!(jobs.get("7").is_none_or(|job| job.is_cancelled()));
        drop(jobs);
        drop(input);
        // The server returns when its commands have ended: the cancelled one was killed.
        serving.join().unwrap();
        let rest: Vec<String> = lines.map(Result::unwrap).collect();
        assert!(
            rest.iter().all(|l| !l.contains("\"id\":7")),
            "the cancelled call was answered: {rest:?}"
        );
    }
}
