//! The server attached to a running editor (issue #1108): `netsmith mcp --editor [PORT]`.
//!
//! The editor is a server of its own (`netsmith view editor`), and a person looks at its page.
//! The commands that read or change what the page shows, the state of its 3D view, its picture, a
//! selection, a highlighted part or net, are the ones of `view editor-3d`: they talk to the
//! editor over HTTP (`/api/view`, which asks the page over its socket and so makes the page's
//! own events), and they take the editor's port. Attached, the server **fills that port in**:
//! a call of the tool `view_editor_3d` runs the same child process as any tool, with
//! `--port <the editor's>` where the call names none, so what the tool does is what
//! `netsmith view editor-3d --port N` does, and the agent and the person share one page. No tool is
//! added: the tool list is the command line's, and a call that names a `port` itself gets that one.
//!
//! * `--editor PORT` names the editor's port. It is not looked at until a call needs it, so the
//!   host may start the server before the person starts the editor, and an editor that is
//!   restarted on the same port is found again; with nothing listening the command says so, as
//!   it does on the command line.
//! * `--editor` alone **finds** the editor of the project: it asks each of the ports from the
//!   command line's default, [`SEARCH`], for the project it serves (`GET /api/project`, whose
//!   `root` is the folder of its `netsmith.toml`) and takes the one that serves this server's
//!   project. A call that finds none, or finds editors of other projects only, is an error that
//!   says so and says to start one or to give the port.
//! * `picture` of `view_editor_3d`, the file the page's canvas is saved to, is also returned in
//!   the result as an image, so that a model that can see one looks at what the person sees.
//!
//! The call is still a child process ([`super::run`]); a command that runs in the editor's own
//! process (a tool that needs the editor's open file or its undo steps, not only its page) is
//! another way of running the same call, and the one place that picks it is
//! [`super::Server::execute`].

use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value, json};

use crate::commands::common::DEFAULT_PORT;

/// The ports `--editor` looks at for the editor of the project: the default port and the nine
/// after it.
pub const SEARCH: RangeInclusive<u16> = DEFAULT_PORT..=DEFAULT_PORT + 9;

/// The only tool that talks to the page: `view editor-3d`.
pub const PAGE_TOOL: [&str; 2] = ["view", "editor-3d"];

/// How the server is attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Attach {
    /// The editor on this port.
    Port(u16),
    /// The editor that serves the server's project, found among [`SEARCH`] when a call needs it.
    Find,
}

/// A request that has no answer in this long is a hang, not a slow editor (the editor answers
/// `/api/project` at once, also before its first analysis ends).
const HANG: Duration = Duration::from_secs(30);

impl Attach {
    /// The port of the editor a call goes to, for the server of the project in `dir`.
    pub fn port(self, dir: &Path) -> Result<u16, String> {
        match self {
            Attach::Port(port) => Ok(port),
            Attach::Find => find_in(SEARCH, &served_by_project(dir), served_root),
        }
    }

    /// What the server tells the model about the editor it is attached to.
    pub fn instructions(self) -> String {
        let which = match self {
            Attach::Port(port) => format!("the editor on 127.0.0.1:{port}"),
            Attach::Find => "the editor that is running for this project".to_string(),
        };
        format!(
            " This server is attached to {which}: `view_editor_3d` reads and changes the page the person has open (its state, the selection, the highlighted parts and nets, the camera and the cut), with the editor's own events, and `picture` is also returned as an image; it needs no `port`. The other tools run as before."
        )
    }

    /// The call's arguments with the editor's port filled in, for the tool `view_editor_3d`
    /// where the call names no port.
    pub fn arguments(
        self,
        words: &[String],
        arguments: &Map<String, Value>,
        dir: &Path,
    ) -> Result<Map<String, Value>, String> {
        let mut arguments = arguments.clone();
        if words.iter().eq(PAGE_TOOL.iter()) && arguments.get("port").is_none_or(Value::is_null) {
            arguments.insert("port".into(), json!(self.port(dir)?));
        }
        Ok(arguments)
    }

    /// Says in the tool's schema that the port is the editor's.
    pub fn describe(self, schema: &mut Value) {
        let port = &mut schema["properties"]["port"];
        if port.is_object() {
            port["description"] =
                json!("The editor's port. Leave it out: this server is attached to an editor.");
            match self {
                Attach::Port(n) => port["default"] = json!(n),
                Attach::Find => {
                    port.as_object_mut().map(|p| p.remove("default"));
                }
            }
        }
    }
}

/// The folder the editor of this server's project serves: where its `netsmith.toml` is (the
/// folder itself for a project with none, as for a single file).
fn served_by_project(dir: &Path) -> PathBuf {
    let root = super::content::project_root(dir).unwrap_or_else(|| dir.to_path_buf());
    root.canonicalize().unwrap_or(root)
}

/// The first of `ports` where `served(port)` is `root`, or why there is none. `served` says
/// the folder the editor on a port serves, `None` for a port with no editor.
fn find_in(
    ports: impl IntoIterator<Item = u16>,
    root: &Path,
    served: impl Fn(u16) -> Option<PathBuf>,
) -> Result<u16, String> {
    let mut others = Vec::new();
    let mut first = None;
    let mut last = None;
    for port in ports {
        first.get_or_insert(port);
        last = Some(port);
        match served(port) {
            Some(served) if served == root => return Ok(port),
            Some(served) => others.push(format!("{port}: {}", served.display())),
            None => {}
        }
    }
    let looked = match (first, last) {
        (Some(a), Some(b)) => format!("127.0.0.1:{a} to {b}"),
        _ => "no port".to_string(),
    };
    let mut message = format!(
        "no editor of the project {} is running (looked at {looked}): start one in the project's folder with `netsmith view editor` and open its page, or give its port with `netsmith mcp --editor <PORT>`",
        root.display()
    );
    if !others.is_empty() {
        message.push_str(&format!(
            "; the editors found serve other projects ({})",
            others.join(", ")
        ));
    }
    Err(message)
}

/// The folder the netsmith editor on `port` serves, `None` when nothing there answers as an
/// editor.
fn served_root(port: u16) -> Option<PathBuf> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent("netsmith-mcp")
        .timeout_global(Some(HANG))
        .http_status_as_error(false)
        // The editor is on this machine: a proxy of the environment is not for it.
        .proxy(None)
        .build()
        .into();
    let mut response = agent
        .get(format!("http://127.0.0.1:{port}/api/project"))
        .header("X-Netsmith", "1")
        .call()
        .ok()?;
    if response.status().as_u16() != 200 {
        return None;
    }
    let text = response.body_mut().read_to_string().ok()?;
    let project: Value = serde_json::from_str(&text).ok()?;
    let root = PathBuf::from(project.get("root")?.as_str()?);
    Some(root.canonicalize().unwrap_or(root))
}

/// The picture a call of `view_editor_3d` saved, as an MCP image: `arguments`' `picture` is a
/// path (relative to `dir`, as the command reads it) of the PNG the command wrote.
pub fn picture(dir: &Path, words: &[String], arguments: &Map<String, Value>) -> Option<Value> {
    if !words.iter().eq(PAGE_TOOL.iter()) {
        return None;
    }
    let path = dir.join(arguments.get("picture")?.as_str()?);
    let bytes = std::fs::read(path).ok()?;
    Some(json!({ "type": "image", "data": base64(&bytes), "mimeType": "image/png" }))
}

/// Standard base64 with padding.
fn base64(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                text.push(DIGITS[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                text.push('=');
            }
        }
    }
    text
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn words(of: &[&str]) -> Vec<String> {
        of.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn base64_is_the_standard_encoding() {
        for (plain, coded) in [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ] {
            assert_eq!(base64(plain.as_bytes()), coded, "{plain}");
        }
        assert_eq!(base64(&[0xfb, 0xff, 0xfe]), "+//+");
    }

    #[test]
    fn the_editor_of_the_project_is_the_first_port_that_serves_its_folder() {
        let root = Path::new("/work/led");
        let asked = Cell::new(0);
        let served = |port: u16| {
            asked.set(asked.get() + 1);
            match port {
                10 => Some(PathBuf::from("/work/other")),
                12 => Some(root.to_path_buf()),
                13 => Some(root.to_path_buf()),
                _ => None,
            }
        };
        assert_eq!(find_in(10..=14, root, served), Ok(12));
        // Ports 10, 11 and 12 were asked, and 13 and 14 not: the search ends at the editor.
        assert_eq!(asked.get(), 3);
    }

    #[test]
    fn no_editor_of_the_project_is_an_error_that_names_what_was_found() {
        let root = Path::new("/work/led");
        let e = find_in(10..=12, root, |_| None).unwrap_err();
        assert!(
            e.contains("no editor of the project /work/led")
                && e.contains("127.0.0.1:10 to 12")
                && e.contains("--editor <PORT>")
                && !e.contains("other projects"),
            "{e}"
        );
        let e = find_in(10..=12, root, |p| {
            (p == 11).then(|| PathBuf::from("/work/other"))
        })
        .unwrap_err();
        assert!(e.contains("other projects (11: /work/other)"), "{e}");
    }

    #[test]
    fn only_the_page_tool_gets_the_port_and_a_port_the_call_names_stays() {
        let attach = Attach::Port(9001);
        let dir = Path::new(".");
        let none = Map::new();
        let page = words(&PAGE_TOOL);
        assert_eq!(
            attach.arguments(&page, &none, dir).unwrap()["port"],
            json!(9001)
        );
        let named = json!({ "port": 9002 }).as_object().unwrap().clone();
        assert_eq!(
            attach.arguments(&page, &named, dir).unwrap()["port"],
            json!(9002)
        );
        assert!(
            attach
                .arguments(&words(&["board", "check"]), &none, dir)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_picture_is_returned_only_for_the_page_tool_and_the_file_it_wrote() {
        let dir = netsmith_testkit::TempDir::new("mcp-attach-picture");
        std::fs::write(dir.join("a.png"), b"foo").unwrap();
        let args = json!({ "picture": "a.png" }).as_object().unwrap().clone();
        let page = words(&PAGE_TOOL);
        let image = picture(&dir, &page, &args).unwrap();
        assert_eq!(image["data"], "Zm9v");
        assert_eq!(image["mimeType"], "image/png");
        assert!(picture(&dir, &words(&["board", "check"]), &args).is_none());
        let gone = json!({ "picture": "b.png" }).as_object().unwrap().clone();
        assert!(picture(&dir, &page, &gone).is_none());
    }
}
