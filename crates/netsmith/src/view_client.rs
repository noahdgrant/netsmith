//! `netsmith view`: the 3D view of a running editor, driven from the command line (P8b of issue
//! 430; docs/BOARD3D.md, "Driving a running editor"). It is a client of the editor's `/api/view`
//! routes (`netsmith_editor::api::ViewChange` is what it sends, `ViewState` what it reads), with
//! the flags of `netsmith render` where they apply, so that an agent switches between the two by
//! changing the verb: `render` draws a picture of the source with no editor, `view` shows the
//! editor's page, the one a person looks at.
//!
//! **Nothing here is the design.** The routes move the camera, cut and light the picture and set
//! the page's selection; no file is written but the PNG that `--picture` names.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    time::Duration,
};

use netsmith_editor::api::{ViewChange, ViewState};

/// What `netsmith view` was asked to do: read the state of the page, or change it first.
#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub port: u16,
    /// The change to make; the default (nothing set) is a read.
    pub change: ViewChange,
    /// Print the route's JSON as it came, not the summary.
    pub json: bool,
    /// Save the page's picture here after the change.
    pub picture: Option<PathBuf>,
}

impl Request {
    /// Whether the request changes the page: a change flag, or a picture (which opens the 3D
    /// view first when it is not on screen, as an empty change does).
    pub fn changes(&self) -> bool {
        self.change != ViewChange::default() || self.picture.is_some()
    }
}

/// The longest a change may take: the route waits 45 s for the page, and the first change loads
/// the 3D view and the board's scene.
const WAIT: Duration = Duration::from_secs(60);
/// The most bytes read from the editor's answer: a PNG of a large canvas.
const MAX_ANSWER: u64 = 256 * 1024 * 1024;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// Talks to the editor on `request.port` and returns what to print on stdout. The error is what
/// to tell the person (the route's own message for a refusal); the exit status is 2 for it.
pub fn run(request: &Request) -> Result<String, String> {
    let editor = Editor::new(request.port);
    let (body, state) = if request.changes() {
        let change = serde_json::to_string(&request.change)
            .map_err(|e| format!("the change cannot be written as JSON: {e}"))?;
        editor.state("POST", Some(&change))?
    } else {
        editor.state("GET", None)?
    };
    let mut text = if request.json {
        let mut text = body;
        if !text.ends_with('\n') {
            text.push('\n');
        }
        text
    } else {
        summary(&state)
    };
    if let Some(path) = &request.picture {
        editor.picture(path)?;
        if !request.json {
            text.push_str(&format!("picture: {}\n", path.display()));
        }
    }
    Ok(text)
}

/// The editor on one port of this machine.
struct Editor {
    port: u16,
    agent: ureq::Agent,
}

/// What the editor answered.
struct Answer {
    status: u16,
    body: Vec<u8>,
}

impl Editor {
    fn new(port: u16) -> Editor {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .user_agent("netsmith-view")
            .timeout_global(Some(WAIT))
            // A refusal carries the reason in its body, for the person to read.
            .http_status_as_error(false)
            // The editor is on this machine: a proxy of the environment is not for it.
            .proxy(None)
            .build()
            .into();
        Editor { port, agent }
    }

    fn url(&self, route: &str) -> String {
        format!("http://127.0.0.1:{}{route}", self.port)
    }

    /// One request. The `X-Netsmith` header goes on every one (the guard wants it of a change,
    /// and a read does not mind it).
    fn call(&self, method: &str, route: &str, body: Option<&str>) -> Result<Answer, String> {
        let url = self.url(route);
        let sent = if method == "POST" {
            self.agent
                .post(&url)
                .header("X-Netsmith", "1")
                .header("Content-Type", "application/json")
                .send(body.unwrap_or("{}"))
        } else {
            self.agent.get(&url).header("X-Netsmith", "1").call()
        };
        let mut response = sent.map_err(|e| self.unreachable(&e))?;
        let status = response.status().as_u16();
        let body = response
            .body_mut()
            .with_config()
            .limit(MAX_ANSWER)
            .read_to_vec()
            .map_err(|e| format!("the editor's answer broke off: {e}"))?;
        Ok(Answer { status, body })
    }

    /// Why nothing answered. A port with nothing on it is the common case: say to start one.
    fn unreachable(&self, error: &ureq::Error) -> String {
        let refused = match error {
            ureq::Error::Io(e) => matches!(
                e.kind(),
                ErrorKind::ConnectionRefused | ErrorKind::ConnectionReset | ErrorKind::NotFound
            ),
            ureq::Error::ConnectionFailed => true,
            _ => false,
        };
        if refused {
            format!(
                "no editor is listening on 127.0.0.1:{port}: start one with `netsmith editor` (it prints its address; `--port <n>` says which to drive, {default} unless told otherwise) and open the page it prints",
                port = self.port,
                default = crate::cli::DEFAULT_PORT
            )
        } else {
            format!(
                "the editor on 127.0.0.1:{} did not answer: {error}",
                self.port
            )
        }
    }

    /// `GET` or `POST /api/view`: the JSON as it came, and the state it says.
    fn state(&self, method: &str, body: Option<&str>) -> Result<(String, ViewState), String> {
        let answer = self.call(method, "/api/view", body)?;
        let text = String::from_utf8_lossy(&answer.body).into_owned();
        if answer.status != 200 {
            return Err(self.refusal(method, &answer, &text));
        }
        let state: ViewState = serde_json::from_str(&text).map_err(|e| {
            format!(
                "port {} answered, but not with a view state ({e}): is that a netsmith editor?",
                self.port
            )
        })?;
        Ok((text, state))
    }

    /// `GET /api/view/picture`, written to `path`.
    fn picture(&self, path: &Path) -> Result<(), String> {
        let answer = self.call("GET", "/api/view/picture", None)?;
        if answer.status != 200 {
            let text = String::from_utf8_lossy(&answer.body).into_owned();
            return Err(self.refusal("GET", &answer, &text));
        }
        if !answer.body.starts_with(PNG_SIGNATURE) {
            return Err("the editor answered a picture that is not a PNG".to_string());
        }
        fs::write(path, &answer.body).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// The words for an answer that is not a success: the route's own message, which says what
    /// is wrong (the ids the board has, the address to open); with a hint for an editor that
    /// does not have the route.
    fn refusal(&self, method: &str, answer: &Answer, text: &str) -> String {
        let text = text.trim();
        if answer.status == 405 || (answer.status == 404 && method == "GET") {
            return format!(
                "the editor on 127.0.0.1:{} has no /api/view: it is older than `netsmith view`, or not a netsmith editor; restart it with `netsmith editor`",
                self.port
            );
        }
        if text.is_empty() {
            return format!(
                "the editor on 127.0.0.1:{} answered {}",
                self.port, answer.status
            );
        }
        text.to_string()
    }
}

/// The state as a few lines for a person (an agent that wants every field uses `--json`).
pub fn summary(state: &ViewState) -> String {
    let mut out = String::new();
    let mut line = |label: &str, value: String| out.push_str(&format!("{label}: {value}\n"));
    line(
        "board",
        state.board.clone().unwrap_or_else(|| "none".into()),
    );
    if !state.shown {
        line(
            "3D view",
            "not on screen (a change, or `--picture`, opens it)".into(),
        );
    } else {
        let mut view = state
            .view
            .map(|v| render_name(&v))
            .unwrap_or_else(|| "turned from every named view".into());
        match state.ortho {
            Some(true) => view.push_str(", orthographic"),
            Some(false) => view.push_str(", perspective"),
            None => {}
        }
        line("view", view);
        if let Some(camera) = &state.camera {
            line(
                "camera",
                format!(
                    "eye {} looking at {} (mm)",
                    point(camera.position),
                    point(camera.target)
                ),
            );
        }
        line(
            "section",
            state.section.clone().unwrap_or_else(|| "none".into()),
        );
        if let Some(apart) = state.exploded {
            line("layers apart", yes_no(apart));
        }
    }
    line("highlight", list(&state.highlight));
    line("selection", list(&state.selection));
    if let Some(hover) = &state.hover {
        line("hover", hover.clone());
    }
    line("x-ray", yes_no(state.xray));
    line(
        "colour by",
        serde_json::to_value(state.colour_by)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default(),
    );
    line(
        "height preview",
        state
            .height_preview
            .map_or_else(|| "off".to_string(), |mm| format!("{mm} mm")),
    );
    line("theme", state.theme.clone());
    if let Some(command) = &state.command {
        line("render", command.clone());
    }
    out
}

fn render_name(view: &netsmith_editor::api::RenderView) -> String {
    serde_json::to_value(view)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn point(p: [f64; 3]) -> String {
    let n = |v: f64| format!("{:.1}", (v * 10.0).round() / 10.0);
    format!("({}, {}, {})", n(p[0]), n(p[1]), n(p[2]))
}

fn yes_no(on: bool) -> String {
    if on { "on" } else { "off" }.to_string()
}

fn list(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}
