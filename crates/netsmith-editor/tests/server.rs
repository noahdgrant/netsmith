//! The server over a real socket, with a backend that knows no compiler.

use std::{
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use netsmith_editor::{
    AnalysisEvent, ApiError, Attachment, Backend, Observer,
    api::{
        Analysis, BoardDetail, Bom, BuildReport, CatalogRefresh, Changed, ClientMessage, Compare,
        CompareHead, Definition, Download, FabReport, FileContent, GitCommit, GitInfo,
        OrderBoardInfo, OrderInfo, OrderReport, PanelView, PcbReport, PowerTree, ProblemsDiff,
        Project, Saved, ServerMessage, SourceDiff, SystemView,
    },
};

/// Answers each question with something recognisable.
struct Fake {
    version: AtomicU64,
    /// The first analysis is running: `analysis` and `check` wait for it, as a real one does.
    pending: AtomicBool,
    observer: Mutex<Option<Observer>>,
    /// A compare of the ref `slow` runs until this is set.
    release_compare: AtomicBool,
}

impl Fake {
    fn wait_for_the_first_analysis(&self) {
        while self.pending.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn tell(&self, event: AnalysisEvent) {
        let observer = self.observer.lock().unwrap().clone();
        (observer.expect("the server has set an observer"))(event);
    }
}

fn commit(hash: &str) -> GitCommit {
    GitCommit {
        hash: hash.repeat(5),
        short: hash[..7].into(),
        subject: "a subject".into(),
        author: "An Author".into(),
        date: "2026-11-05T10:00:00+01:00".into(),
    }
}

fn analysis(version: u64) -> Analysis {
    Analysis {
        version,
        errors: 0,
        warnings: 0,
        notes: 0,
        problems: Vec::new(),
        boards: Vec::new(),
        systems: Vec::new(),
        panels: Vec::new(),
        module_uses: None,
        elapsed_ms: None,
    }
}

impl Backend for Fake {
    fn title(&self) -> String {
        "fake".into()
    }
    fn project(&self) -> Project {
        Project {
            title: "fake".into(),
            has_manifest: false,
            kicad: true,
            files: Vec::new(),
            folders: Vec::new(),
            version: None,
            git_describe: None,
            root: None,
            kicad_path: None,
            bundle_hash: None,
        }
    }
    fn create_file(&self, path: &str, text: &str) -> Result<Changed, ApiError> {
        match path {
            "src/taken.nsm" => Err(ApiError::conflict(format!("{path} is already there"))),
            _ if text.contains('\0') => Err(ApiError::bad_request("a NUL")),
            _ => Ok(Changed { path: path.into() }),
        }
    }
    fn create_dir(&self, path: &str) -> Result<Changed, ApiError> {
        if path.contains("..") {
            return Err(ApiError::forbidden("it leaves the project"));
        }
        Ok(Changed { path: path.into() })
    }
    fn rename_path(&self, from: &str, to: &str) -> Result<Changed, ApiError> {
        Ok(Changed {
            path: format!("{from}->{to}"),
        })
    }
    fn delete_path(&self, path: &str, confirm: bool) -> Result<Changed, ApiError> {
        if !confirm {
            return Err(ApiError::new(428, format!("{path} needs a yes")));
        }
        Ok(Changed { path: path.into() })
    }
    fn read_file(&self, path: &str) -> Result<FileContent, ApiError> {
        if path == "a.nsm" {
            Ok(FileContent {
                path: path.into(),
                text: "board A {}".into(),
                stamp: "1".into(),
                readonly: false,
            })
        } else {
            Err(ApiError::not_found(format!(
                "`{path}` is not a file of this project"
            )))
        }
    }
    fn write_file(&self, path: &str, text: &str, stamp: &str) -> Result<Saved, ApiError> {
        if stamp != "1" {
            return Err(ApiError::conflict(format!("{path} changed on disk")));
        }
        Ok(Saved {
            stamp: format!("saved {}", text.len()),
        })
    }
    fn analysis(&self) -> Analysis {
        self.wait_for_the_first_analysis();
        Analysis {
            elapsed_ms: Some(77),
            ..analysis(self.version.load(Ordering::SeqCst))
        }
    }
    fn pending(&self) -> Option<u32> {
        self.pending.load(Ordering::SeqCst).then_some(123)
    }
    fn observe(&self, observer: Observer) {
        *self.observer.lock().unwrap() = Some(observer);
    }
    fn check(&self, path: &str, text: &str) -> Result<Analysis, ApiError> {
        self.wait_for_the_first_analysis();
        // The text length stands for what the check found, so the test can see it arrived.
        Ok(analysis(1000 + path.len() as u64 + text.len() as u64))
    }
    fn board(&self, name: &str) -> Result<BoardDetail, ApiError> {
        Err(ApiError::not_found(format!("there is no board `{name}`")))
    }
    fn schematic(&self, board: &str) -> Result<String, ApiError> {
        Ok(format!("<svg data-board=\"{board}\"/>"))
    }
    fn power(&self, board: &str) -> Result<PowerTree, ApiError> {
        Ok(PowerTree {
            title: board.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
            stages: Vec::new(),
        })
    }
    fn system(&self, name: &str) -> Result<SystemView, ApiError> {
        Err(ApiError::not_found(name))
    }
    fn panel(&self, name: &str) -> Result<PanelView, ApiError> {
        Err(ApiError::not_found(name))
    }
    fn bom(&self, board: &str) -> Result<Bom, ApiError> {
        Err(ApiError::not_found(board))
    }
    fn refresh_catalogs(
        &self,
        dry_run: bool,
        offline: bool,
        say: &netsmith_editor::Say,
    ) -> Result<CatalogRefresh, ApiError> {
        say("acme_res 1/2: C25104");
        Ok(CatalogRefresh {
            dry_run,
            offline,
            files: Vec::new(),
            requests: 2,
            error: None,
            summary: "total: 1 verified".into(),
            notice: "JLCPCB data is not fetched".into(),
        })
    }
    fn cancel_route(&self) {}

    fn route(&self, board: &str, _say: &netsmith_editor::Say) -> Result<PcbReport, ApiError> {
        Err(ApiError::internal(format!("cannot place {board}")))
    }
    fn pcb_scene(&self, board: &str) -> Result<netsmith_editor::api::PcbScene, ApiError> {
        if board == "Broken" {
            return Err(ApiError::unprocessable(
                "Broken: net VDD joins pad 131 of U4, but footprint F has only pads 1, 2",
                "footprint_lacks_pad",
                &[("board", "Broken"), ("part", "U4"), ("pad", "131")],
            ));
        }
        Err(ApiError::not_found(board))
    }
    fn pcb_findings(&self, board: &str) -> Result<netsmith_editor::api::PcbFindings, ApiError> {
        Err(ApiError::not_found(board))
    }
    fn pcb_drc(
        &self,
        board: &str,
        _say: &netsmith_editor::Say,
    ) -> Result<netsmith_editor::api::PcbFindings, ApiError> {
        Err(ApiError::not_found(board))
    }
    fn pcb_edit(
        &self,
        edit: &netsmith_editor::api::PcbEdit,
    ) -> Result<netsmith_editor::api::PcbEdited, ApiError> {
        Err(ApiError::conflict(format!("cannot edit {}", edit.board)))
    }
    fn build(&self, say: &netsmith_editor::Say) -> BuildReport {
        say("writing the schematics");
        BuildReport {
            ok: true,
            written: vec!["out".into()],
            output: String::new(),
            elapsed_ms: 0,
        }
    }
    fn fab(
        &self,
        board: &str,
        _allow_drc_violations: bool,
        _say: &netsmith_editor::Say,
    ) -> Result<FabReport, ApiError> {
        Err(ApiError::not_found(board))
    }
    fn download(&self, _board: &str, file: &str) -> Result<Attachment, ApiError> {
        Ok(Attachment {
            name: format!("{file}.csv"),
            content_type: "text/csv".into(),
            bytes: b"a,b\n".to_vec(),
        })
    }
    fn orders(&self) -> Vec<OrderInfo> {
        vec![OrderInfo {
            name: "devkit".into(),
            boards: vec![OrderBoardInfo {
                name: "Unit".into(),
                quantity: 10,
            }],
        }]
    }
    fn fab_order(
        &self,
        name: &str,
        _allow_drc_violations: bool,
        say: &netsmith_editor::Say,
    ) -> Result<OrderReport, ApiError> {
        if name != "devkit" {
            return Err(ApiError::not_found(format!("no order `{name}`")));
        }
        say("Unit: placing 3 parts");
        Ok(OrderReport {
            name: name.into(),
            ok: true,
            dir: "fab/orders/devkit".into(),
            summary: "order devkit".into(),
            output: String::new(),
            downloads: vec![Download {
                key: "summary".into(),
                name: "order.txt".into(),
            }],
            elapsed_ms: 1,
        })
    }
    fn order_download(&self, name: &str, file: &str) -> Result<Attachment, ApiError> {
        Ok(Attachment {
            name: format!("{name}-{file}.txt"),
            content_type: "text/plain".into(),
            bytes: b"order\n".to_vec(),
        })
    }
    fn git(&self) -> GitInfo {
        GitInfo {
            available: true,
            reason: None,
            root: Some("/repo".into()),
            project_dir: Some("hw".into()),
            branch: Some("main".into()),
            head: Some(commit("a1b2c3d4")),
            dirty: true,
            project_dirty: false,
            refs: Vec::new(),
            commits: vec![commit("a1b2c3d4")],
        }
    }
    fn compare(
        &self,
        rev: &str,
        board: Option<&str>,
        observe: &Observer,
    ) -> Result<Compare, ApiError> {
        if rev == "nope" {
            return Err(ApiError::not_found(format!("there is no commit `{rev}`")));
        }
        observe(AnalysisEvent::Progress {
            stage: "extract".into(),
            board: String::new(),
            done: 0,
            total: 1,
            message: format!("reading {rev} out of git"),
        });
        if rev == "slow" {
            while !self.release_compare.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(10));
            }
        }
        Ok(Compare {
            rev: rev.into(),
            base: commit("deadbeef"),
            head: CompareHead {
                commit: None,
                dirty: false,
                errors: 0,
            },
            error: None,
            base_problems: Vec::new(),
            cached: false,
            elapsed_ms: 5,
            summary: format!("board is {board:?}"),
            changed: false,
            source: SourceDiff {
                files_changed: 0,
                added: 0,
                removed: 0,
                files: Vec::new(),
                error: None,
            },
            boards: Vec::new(),
            problems: ProblemsDiff {
                new: Vec::new(),
                resolved: Vec::new(),
                unchanged: 0,
            },
        })
    }
    fn git_file(&self, rev: &str, path: &str) -> Result<FileContent, ApiError> {
        if path.contains("..") {
            return Err(ApiError::forbidden("it leaves the project"));
        }
        Ok(FileContent {
            path: path.into(),
            text: format!("{path} at {rev}"),
            stamp: rev.into(),
            readonly: true,
        })
    }
    fn definition(&self, _: &str, _: &str, offset: usize) -> Result<Option<Definition>, ApiError> {
        Ok((offset == 0).then(|| Definition {
            signature: None,
            doc: None,
            at: netsmith_editor::api::Location {
                file: "a.nsm".into(),
                line: 1,
                column: 1,
                end_line: 1,
                end_column: 2,
            },
        }))
    }
}

struct Reply {
    status: u16,
    headers: String,
    body: String,
}

struct Server {
    port: u16,
    backend: Arc<Fake>,
}

fn start() -> Server {
    start_with(1, false)
}

/// A server whose first analysis is running (version 0 until `finish_the_first_analysis`).
fn start_pending() -> Server {
    start_with(0, true)
}

fn start_with(version: u64, pending: bool) -> Server {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let backend = Arc::new(Fake {
        version: AtomicU64::new(version),
        pending: AtomicBool::new(pending),
        observer: Mutex::new(None),
        release_compare: AtomicBool::new(false),
    });
    let served = Arc::clone(&backend);
    thread::spawn(move || netsmith_editor::serve(listener, served));
    // The listener is bound, so connections wait in its queue until the server runs.
    Server { port, backend }
}

impl Server {
    /// The first analysis ends, with the project at version 1.
    fn finish_the_first_analysis(&self) {
        self.backend.version.store(1, Ordering::SeqCst);
        self.backend.pending.store(false, Ordering::SeqCst);
    }

    fn request(
        &self,
        method: &str,
        target: &str,
        extra: &[(&str, &str)],
        body: &str,
        host: Option<&str>,
    ) -> Reply {
        let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port)).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let host = host.map_or(format!("127.0.0.1:{}", self.port), str::to_string);
        let mut head = format!(
            "{method} {target} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Length: {}\r\n",
            body.len()
        );
        for (k, v) in extra {
            head.push_str(&format!("{k}: {v}\r\n"));
        }
        head.push_str("\r\n");
        s.write_all(head.as_bytes()).unwrap();
        s.write_all(body.as_bytes()).unwrap();
        let mut raw = Vec::new();
        s.read_to_end(&mut raw).unwrap();
        let raw = String::from_utf8_lossy(&raw).into_owned();
        let (head, body) = raw.split_once("\r\n\r\n").expect("a complete response");
        Reply {
            status: head.split(' ').nth(1).unwrap().parse().unwrap(),
            headers: head.to_ascii_lowercase(),
            body: body.to_string(),
        }
    }

    fn get(&self, target: &str) -> Reply {
        self.request("GET", target, &[], "", None)
    }
}

const GUARD: (&str, &str) = ("X-Netsmith", "1");

#[test]
fn the_project_carries_the_hash_of_the_embedded_page_and_the_checkout_it_was_built_from() {
    let s = start();
    let json: serde_json::Value = serde_json::from_str(&s.get("/api/project").body).unwrap();
    // What the binary knows of itself is added by the server, to any backend's answer.
    match netsmith_editor::bundle_hash() {
        Some(hash) => assert_eq!(json["bundleHash"], hash),
        None => assert!(json.get("bundleHash").is_none(), "{json}"),
    }
    match netsmith_editor::git_describe() {
        Some(describe) => assert_eq!(json["gitDescribe"], describe),
        None => assert!(json.get("gitDescribe").is_none(), "{json}"),
    }
    // With a page built in, the hash is the committed bundle's SOURCE-HASH or the hash of
    // the sources it was built from: 16 hex digits.
    if let Some(hash) = netsmith_editor::bundle_hash() {
        assert!(
            hash.len() == 16 && hash.chars().all(|c| c.is_ascii_hexdigit()),
            "{hash}"
        );
    }
    // The fake backend knows no version, root or KiCad: they are left out.
    for key in ["version", "root", "kicadPath"] {
        assert!(json.get(key).is_none(), "{key}: {json}");
    }
}

#[test]
fn json_endpoints_answer_with_json() {
    let s = start();
    let r = s.get("/api/project");
    assert_eq!(r.status, 200);
    assert!(r.headers.contains("application/json"), "{}", r.headers);
    assert!(r.body.contains("\"title\":\"fake\"") && r.body.contains("\"hasManifest\":false"));
    let r = s.get("/api/analysis");
    assert!(r.body.contains("\"version\":1"), "{}", r.body);
    let r = s.get("/api/file?path=a.nsm");
    assert!(
        r.body.contains("board A {}") && r.body.contains("\"stamp\":\"1\""),
        "{}",
        r.body
    );
    let r = s.get("/api/power?board=X%2Fy");
    assert!(
        r.body.contains("\"title\":\"X/y\""),
        "the query is decoded: {}",
        r.body
    );
}

#[test]
fn a_svg_is_served_as_svg_and_a_download_as_an_attachment() {
    let s = start();
    let r = s.get("/api/schematic.svg?board=Led");
    assert!(r.headers.contains("image/svg+xml") && r.body.contains("data-board=\"Led\""));
    let r = s.get("/api/download?board=Led&file=bom");
    assert!(
        r.headers
            .contains("content-disposition: attachment; filename=\"bom.csv\""),
        "{}",
        r.headers
    );
    assert_eq!(r.body, "a,b\n");
}

#[test]
fn a_refusal_carries_its_status_and_message() {
    let s = start();
    let r = s.get("/api/file?path=nope.nsm");
    assert_eq!(r.status, 404);
    assert!(
        r.body.contains("`nope.nsm` is not a file of this project"),
        "{}",
        r.body
    );
    assert_eq!(
        s.get("/api/file").status,
        400,
        "a missing parameter is a bad request"
    );
    assert!(s.get("/api/file").body.contains("`path`"));
    let r = s.request("POST", "/api/pcb/route?board=B", &[GUARD], "", None);
    assert_eq!(r.status, 500);
    assert!(r.body.contains("cannot place B"));
}

#[test]
fn a_failure_of_the_projects_data_is_422_and_has_a_structured_body_for_a_page_that_asks() {
    let s = start();
    // Without `Accept: application/json` the body is the message, as it is for any refusal.
    let r = s.get("/api/pcb/scene?board=Broken");
    assert_eq!(r.status, 422, "not a fault of the server");
    assert!(r.headers.contains("text/plain"), "{}", r.headers);
    assert!(
        r.body
            .starts_with("Broken: net VDD joins pad 131 of U4, but footprint F has only"),
        "{}",
        r.body
    );
    // With it, the same status and a JSON body: the code, the message and the fields.
    let r = s.request(
        "GET",
        "/api/pcb/scene?board=Broken",
        &[("Accept", "application/json")],
        "",
        None,
    );
    assert_eq!(r.status, 422);
    assert!(r.headers.contains("application/json"), "{}", r.headers);
    let body: serde_json::Value = serde_json::from_str(&r.body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({
            "code": "footprint_lacks_pad",
            "message": "Broken: net VDD joins pad 131 of U4, but footprint F has only pads 1, 2",
            "board": "Broken",
            "part": "U4",
            "pad": "131",
        })
    );
    // A refusal with no detail stays text even for a page that accepts JSON.
    let r = s.request(
        "GET",
        "/api/pcb/scene?board=Nope",
        &[("Accept", "application/json")],
        "",
        None,
    );
    assert_eq!(r.status, 404);
    assert!(r.headers.contains("text/plain"), "{}", r.headers);
}

#[test]
fn saving_needs_the_stamp_and_reports_a_conflict() {
    let s = start();
    let ok = s.request(
        "PUT",
        "/api/file?path=a.nsm",
        &[GUARD, ("X-Stamp", "1")],
        "new text",
        None,
    );
    assert_eq!(ok.status, 200, "{}", ok.body);
    assert!(ok.body.contains("\"stamp\":\"saved 8\""), "{}", ok.body);
    let stale = s.request(
        "PUT",
        "/api/file?path=a.nsm",
        &[GUARD, ("X-Stamp", "0")],
        "x",
        None,
    );
    assert_eq!(stale.status, 409);
    assert!(stale.body.contains("changed on disk"));
    let none = s.request("PUT", "/api/file?path=a.nsm", &[GUARD], "x", None);
    assert_eq!(none.status, 400);
    assert!(none.body.contains("X-Stamp"));
}

#[test]
fn the_file_operations_are_posts_with_a_json_body_that_need_the_guard_header() {
    let s = start();
    let json = ("Content-Type", "application/json");
    let ask = |path: &str, body: &str, guard: bool| {
        let headers = if guard { vec![GUARD, json] } else { vec![json] };
        s.request("POST", path, &headers, body, None)
    };
    for (path, body) in [
        ("/api/fs/create-file", r#"{"path":"src/a.nsm","text":""}"#),
        ("/api/fs/create-dir", r#"{"path":"src/a"}"#),
        ("/api/fs/rename", r#"{"from":"src/a.nsm","to":"src/b.nsm"}"#),
        ("/api/fs/delete", r#"{"path":"src/a.nsm","confirm":true}"#),
    ] {
        assert_eq!(
            ask(path, body, false).status,
            403,
            "{path} without the header"
        );
        let r = ask(path, body, true);
        assert_eq!(r.status, 200, "{path}: {}", r.body);
        assert!(r.body.contains("\"path\":\"src/"), "{path}: {}", r.body);
        assert_eq!(
            ask(path, "not json", true).status,
            400,
            "{path} with a bad body"
        );
        assert_eq!(s.get(path).status, 405, "{path} is not a GET");
    }
    // The backend's refusals come through with their status and message.
    let r = ask(
        "/api/fs/create-file",
        r#"{"path":"src/taken.nsm","text":""}"#,
        true,
    );
    assert_eq!(r.status, 409);
    assert!(r.body.contains("already there"), "{}", r.body);
    let r = ask("/api/fs/create-dir", r#"{"path":"src/../x"}"#, true);
    assert_eq!(r.status, 403);
    let r = ask(
        "/api/fs/delete",
        r#"{"path":"src/a","confirm":false}"#,
        true,
    );
    assert_eq!(r.status, 428);
}

#[test]
fn definition_takes_a_json_body() {
    let s = start();
    let body = r#"{"path":"a.nsm","text":"x","offset":0}"#;
    let r = s.request(
        "POST",
        "/api/definition",
        &[GUARD, ("Content-Type", "application/json")],
        body,
        None,
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(r.body.contains("\"endColumn\":2"), "{}", r.body);
    let r = s.request("POST", "/api/definition", &[GUARD], "not json", None);
    assert_eq!(r.status, 400);
}

#[test]
fn another_page_cannot_use_the_server() {
    let s = start();
    // A rebinding page arrives with its own Host.
    assert_eq!(
        s.request("GET", "/api/project", &[], "", Some("evil.example"))
            .status,
        403
    );
    // A page of another origin names itself.
    let r = s.request(
        "GET",
        "/api/project",
        &[("Origin", "https://evil.example")],
        "",
        None,
    );
    assert_eq!(r.status, 403);
    // Its changes carry no X-Netsmith header.
    let r = s.request("POST", "/api/build", &[], "", None);
    assert_eq!(r.status, 403);
    assert!(r.body.contains("X-Netsmith"));
    // The server's own page may.
    let own = format!("http://127.0.0.1:{}", s.port);
    let r = s.request("POST", "/api/build", &[GUARD, ("Origin", &own)], "", None);
    assert_eq!(r.status, 200);
    assert!(r.body.contains("\"written\":[\"out\"]"));
}

#[test]
fn the_page_is_served_or_says_why_not() {
    let s = start();
    let r = s.get("/");
    if netsmith_editor::bundle_is_present() {
        assert_eq!(r.status, 200);
        assert!(r.headers.contains("text/html") && r.body.contains("<div id=\"app\">"));
        // Nothing is loaded from elsewhere.
        for bad in ["src=\"http", "href=\"http", "@import"] {
            assert!(!r.body.contains(bad), "{bad}");
        }
    } else {
        assert_eq!(r.status, 503);
        assert!(r.body.contains("Node 20"), "{}", r.body);
    }
    assert_eq!(s.get("/no/such/file.js").status, 404);
}

/// How long a test waits for the server before it fails; far more than a loaded machine needs.
const WAIT: Duration = Duration::from_secs(20);

type Socket = tungstenite::WebSocket<TcpStream>;

/// A page's socket whose every read and write gives up after `WAIT`.
fn open_socket(port: u16) -> Socket {
    let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    stream.set_read_timeout(Some(WAIT)).unwrap();
    stream.set_write_timeout(Some(WAIT)).unwrap();
    let url = format!("ws://127.0.0.1:{port}/api/events");
    match tungstenite::client(url.as_str(), stream) {
        Ok((socket, _)) => socket,
        Err(e) => panic!("the socket did not open within {WAIT:?}: {e}"),
    }
}

/// The next message the server sends, or a failure that says what was awaited.
fn next_message(socket: &mut Socket, awaiting: &str) -> ServerMessage {
    let deadline = Instant::now() + WAIT;
    loop {
        match socket.read() {
            Ok(tungstenite::Message::Text(t)) => return serde_json::from_str(&t).unwrap(),
            Ok(_) => {}
            Err(e) => panic!("no message arrived while awaiting {awaiting} (waited {WAIT:?}): {e}"),
        }
        assert!(
            Instant::now() < deadline,
            "only other frames arrived while awaiting {awaiting} (waited {WAIT:?})"
        );
    }
}

/// A file changes after a page connected: the page must be told, however late the
/// server's watcher first looks.
fn rebuild_is_pushed_and_check_is_answered() {
    let s = start();
    let mut socket = open_socket(s.port);
    // The backend's version goes up and the page is told.
    s.backend.version.store(2, Ordering::SeqCst);
    match next_message(&mut socket, "the `rebuilt` push for version 2") {
        ServerMessage::Rebuilt { analysis, .. } => assert_eq!(analysis.version, 2),
        other => panic!("expected `rebuilt`, got {other:?}"),
    }
    // The page asks for a check of unsaved text.
    let ask = ClientMessage::Check {
        id: 7,
        path: "a.nsm".into(),
        text: "abc".into(),
    };
    socket
        .send(tungstenite::Message::text(
            serde_json::to_string(&ask).unwrap(),
        ))
        .unwrap();
    loop {
        if let ServerMessage::Checked { id, analysis } =
            next_message(&mut socket, "the `checked` answer to check 7")
        {
            assert_eq!(id, 7);
            assert_eq!(analysis.version, 1000 + 5 + 3);
            break;
        }
    }
}

#[test]
fn the_socket_pushes_a_rebuild_and_answers_a_check() {
    rebuild_is_pushed_and_check_is_answered();
}

/// The watcher's first look used to become its baseline, so a change made before it ran
/// was never announced; whether it did depended on the machine's load.
#[test]
fn a_change_right_after_connecting_is_pushed_every_time() {
    for _ in 0..25 {
        rebuild_is_pushed_and_check_is_answered();
    }
}

/// A job that takes a while tells the pages what it is doing, over the socket, as it goes.
#[test]
fn a_running_job_pushes_its_progress() {
    let s = start();
    let mut socket = open_socket(s.port);
    let reply = s.request("POST", "/api/build", &[GUARD], "", None);
    assert_eq!(reply.status, 200, "{}", reply.body);
    loop {
        if let ServerMessage::Progress {
            task,
            board,
            message,
            ..
        } = next_message(&mut socket, "the progress of the build")
        {
            assert_eq!(
                (task.as_str(), board.as_str(), message.as_str()),
                ("build", "", "writing the schematics")
            );
            break;
        }
    }
}

#[test]
fn a_catalog_refresh_changes_files_so_needs_the_guard_header_takes_two_flags_and_reports_progress()
{
    let s = start();
    let mut socket = open_socket(s.port);
    assert_eq!(
        s.request("POST", "/api/catalog/refresh", &[], "", None)
            .status,
        403
    );
    let r = s.request(
        "POST",
        "/api/catalog/refresh?dry=1&offline=0",
        &[GUARD],
        "",
        None,
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(
        r.body.contains(r#""dryRun":true"#) && r.body.contains(r#""offline":false"#),
        "{}",
        r.body
    );
    loop {
        if let ServerMessage::Progress {
            task,
            board,
            message,
            ..
        } = next_message(&mut socket, "the progress of the refresh")
        {
            assert_eq!(
                (task.as_str(), board.as_str(), message.as_str()),
                ("catalog", "", "acme_res 1/2: C25104")
            );
            break;
        }
    }
    let r = s.request("POST", "/api/catalog/refresh?offline=1", &[GUARD], "", None);
    assert!(
        r.body.contains(r#""dryRun":false"#) && r.body.contains(r#""offline":true"#),
        "{}",
        r.body
    );
}

#[test]
fn cancel_is_a_post_that_needs_the_guard_header() {
    let s = start();
    assert_eq!(
        s.request("POST", "/api/pcb/cancel", &[], "", None).status,
        403
    );
    assert_eq!(
        s.request("POST", "/api/pcb/cancel", &[GUARD], "", None)
            .status,
        204
    );
}

#[test]
fn orders_are_listed_run_with_progress_and_downloaded() {
    let s = start();
    let r = s.get("/api/orders");
    assert_eq!(
        r.body,
        r#"[{"name":"devkit","boards":[{"name":"Unit","quantity":10}]}]"#
    );

    let mut socket = open_socket(s.port);
    assert_eq!(
        s.request("POST", "/api/order?name=devkit", &[], "", None)
            .status,
        403,
        "running an order changes files and needs the guard header"
    );
    let r = s.request("POST", "/api/order?name=devkit", &[GUARD], "", None);
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(
        r.body.contains(r#""dir":"fab/orders/devkit""#),
        "{}",
        r.body
    );
    loop {
        if let ServerMessage::Progress {
            task,
            board,
            message,
            ..
        } = next_message(&mut socket, "the progress of the order")
        {
            assert_eq!(
                (task.as_str(), board.as_str(), message.as_str()),
                ("order", "devkit", "Unit: placing 3 parts")
            );
            break;
        }
    }

    assert_eq!(
        s.request("POST", "/api/order?name=nope", &[GUARD], "", None)
            .status,
        404
    );
    assert_eq!(
        s.request("POST", "/api/order", &[GUARD], "", None).status,
        400
    );

    let r = s.get("/api/order/download?name=devkit&file=summary");
    assert!(
        r.headers
            .contains("content-disposition: attachment; filename=\"devkit-summary.txt\""),
        "{}",
        r.headers
    );
    assert_eq!(r.body, "order\n");
}

const PENDING: (&str, &str) = ("X-Netsmith-Pending", "1");

/// The observer is installed before the first connection can arrive, so the test waits for
/// the server to have set it (it runs on the server's thread).
fn wait_for_observer(s: &Server) {
    let deadline = Instant::now() + WAIT;
    while s.backend.observer.lock().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "the server never set an observer"
        );
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn while_the_first_analysis_runs_the_page_and_the_files_are_served_and_a_request_that_asks_is_told_pending()
 {
    let s = start_pending();
    // The page, the tree and a file never wait for the analysis.
    assert_eq!(s.get("/").status, 200);
    assert_eq!(s.get("/api/project").status, 200);
    assert_eq!(s.get("/api/file?path=a.nsm").status, 200);
    // A route that needs it answers `503` and says how long it has run, to a request that
    // asks; it did nothing.
    for (method, target) in [
        ("GET", "/api/analysis"),
        ("GET", "/api/board?name=A"),
        ("GET", "/api/power?board=A"),
        ("GET", "/api/pcb/findings?board=A"),
        ("POST", "/api/build"),
        ("POST", "/api/fab?board=A"),
        ("POST", "/api/fs/create-file"),
    ] {
        let r = s.request(method, target, &[GUARD, PENDING], "", None);
        assert_eq!(r.status, 503, "{method} {target}: {}", r.body);
        assert!(
            r.headers.contains("application/json") && r.headers.contains("retry-after: 1"),
            "{target}: {}",
            r.headers
        );
        assert_eq!(r.body, r#"{"pending":true,"elapsedMs":123}"#, "{target}");
    }
    // Only for a request that asks, and only for `1`.
    assert_ne!(
        s.request("GET", "/api/project", &[PENDING], "", None)
            .status,
        503
    );
    // Over: the same request is answered as ever.
    s.finish_the_first_analysis();
    let r = s.request("GET", "/api/analysis", &[PENDING], "", None);
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(
        r.body.contains("\"version\":1") && r.body.contains("\"elapsedMs\":77"),
        "{}",
        r.body
    );
}

#[test]
fn a_request_that_does_not_ask_waits_for_the_first_analysis_and_is_then_answered() {
    let s = start_pending();
    let port = s.port;
    let asked = Instant::now();
    let waiting = thread::spawn(move || {
        let s = Server {
            port,
            backend: Arc::new(Fake {
                version: AtomicU64::new(0),
                pending: AtomicBool::new(false),
                observer: Mutex::new(None),
                release_compare: AtomicBool::new(false),
            }),
        };
        // `X-Netsmith-Pending: 0` is not asking.
        s.request(
            "GET",
            "/api/analysis",
            &[("X-Netsmith-Pending", "0")],
            "",
            None,
        )
    });
    thread::sleep(Duration::from_millis(400));
    assert!(
        !waiting.is_finished(),
        "it answered before the analysis ended"
    );
    s.finish_the_first_analysis();
    let r = waiting.join().unwrap();
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(r.body.contains("\"version\":1"), "{}", r.body);
    assert!(asked.elapsed() >= Duration::from_millis(400));
}

/// What the page that was there from the start hears, and what a page that connects in the
/// middle is told at once; then `rebuilt` with the time taken.
#[test]
fn the_socket_carries_the_analysis_from_its_start_to_rebuilt() {
    let s = start_pending();
    wait_for_observer(&s);
    let mut early = open_socket(s.port);
    s.backend.tell(AnalysisEvent::Started {
        since: 1_700_000_000_000,
    });
    s.backend.tell(AnalysisEvent::Progress {
        stage: "solve".into(),
        board: "Main".into(),
        done: 2,
        total: 5,
        message: "solving Main (3 of 5)".into(),
    });
    assert_eq!(
        next_message(&mut early, "analysing"),
        ServerMessage::Analysing {
            since: 1_700_000_000_000
        }
    );
    match next_message(&mut early, "progress") {
        ServerMessage::Progress {
            task,
            board,
            message,
            stage,
            done,
            total,
        } => assert_eq!(
            (
                task.as_str(),
                board.as_str(),
                message.as_str(),
                stage.as_deref(),
                done,
                total
            ),
            (
                "analyse",
                "Main",
                "solving Main (3 of 5)",
                Some("solve"),
                Some(2),
                Some(5)
            )
        ),
        other => panic!("expected progress, got {other:?}"),
    }
    // A page that connects now is told the same, without waiting for the next line.
    let mut late = open_socket(s.port);
    assert_eq!(
        next_message(&mut late, "analysing, for a late page"),
        ServerMessage::Analysing {
            since: 1_700_000_000_000
        }
    );
    assert!(matches!(
        next_message(&mut late, "the last progress, for a late page"),
        ServerMessage::Progress { done: Some(2), .. }
    ));
    // The wire form is what the page reads.
    let wire = serde_json::to_string(&ServerMessage::Analysing { since: 5 }).unwrap();
    assert_eq!(wire, r#"{"type":"analysing","since":5}"#);
    s.backend.tell(AnalysisEvent::Finished);
    s.finish_the_first_analysis();
    for socket in [&mut early, &mut late] {
        match next_message(socket, "rebuilt after the first analysis") {
            ServerMessage::Rebuilt {
                analysis,
                elapsed_ms,
            } => assert_eq!((analysis.version, elapsed_ms), (1, Some(77))),
            other => panic!("expected `rebuilt`, got {other:?}"),
        }
    }
    // Over, so a page that connects now hears nothing of an analysis.
    let mut after = open_socket(s.port);
    s.backend.version.store(2, Ordering::SeqCst);
    assert!(matches!(
        next_message(&mut after, "rebuilt, with no `analysing` before it"),
        ServerMessage::Rebuilt { .. }
    ));
}

/// A check of typed text waits for the first analysis; the socket must go on carrying what the
/// analysis says meanwhile, and answer the check after.
#[test]
fn a_check_that_waits_for_the_first_analysis_does_not_stop_the_socket() {
    let s = start_pending();
    wait_for_observer(&s);
    let mut socket = open_socket(s.port);
    let ask = ClientMessage::Check {
        id: 3,
        path: "a.nsm".into(),
        text: "abc".into(),
    };
    socket
        .send(tungstenite::Message::text(
            serde_json::to_string(&ask).unwrap(),
        ))
        .unwrap();
    thread::sleep(Duration::from_millis(200));
    s.backend.tell(AnalysisEvent::Started { since: 9 });
    assert_eq!(
        next_message(&mut socket, "analysing while a check waits"),
        ServerMessage::Analysing { since: 9 }
    );
    s.backend.tell(AnalysisEvent::Finished);
    s.finish_the_first_analysis();
    let mut seen = Vec::new();
    loop {
        match next_message(&mut socket, "the check's answer and `rebuilt`") {
            ServerMessage::Checked { id, .. } => {
                assert_eq!(id, 3);
                seen.push("checked");
            }
            ServerMessage::Rebuilt { .. } => seen.push("rebuilt"),
            other => panic!("unexpected {other:?}"),
        }
        if seen.len() == 2 {
            break;
        }
    }
}

#[test]
fn git_and_compare_answer_in_json_and_the_ref_is_a_field_called_ref() {
    let s = start();
    let reply = s.get("/api/git");
    assert_eq!(reply.status, 200, "{}", reply.body);
    let v: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(v["available"], true);
    assert_eq!(v["projectDir"], "hw");
    assert_eq!(v["head"]["short"], "a1b2c3d");
    assert!(
        v.get("reason").is_none(),
        "an absent field is left out: {v}"
    );

    let reply = s.get("/api/compare?ref=feature%2Fx&board=Demo%2Fmain");
    assert_eq!(reply.status, 200, "{}", reply.body);
    let v: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(v["ref"], "feature/x");
    assert_eq!(v["summary"], "board is Some(\"Demo/main\")");
    assert_eq!(
        v["base"]["hash"],
        "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
    );
    assert!(v.get("error").is_none() && v["cached"] == false, "{v}");
    let reply = s.get("/api/compare?ref=HEAD");
    let v: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(v["summary"], "board is None");
    // The answer is the type the page reads.
    let _: Compare = serde_json::from_str(&reply.body).unwrap();

    assert_eq!(s.get("/api/compare").status, 400);
    let reply = s.get("/api/compare?ref=nope");
    assert_eq!(reply.status, 404);
    assert!(
        reply.body.contains("there is no commit `nope`"),
        "{}",
        reply.body
    );

    let reply = s.get("/api/git/file?ref=HEAD~1&path=src%2Fmain.nsm");
    let v: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(
        (v["text"].as_str(), v["readonly"].as_bool()),
        (Some("src/main.nsm at HEAD~1"), Some(true))
    );
    assert_eq!(s.get("/api/git/file?ref=HEAD&path=..%2Fa").status, 403);
    assert_eq!(s.get("/api/git/file?path=a.nsm").status, 400);
}

#[test]
fn a_compare_reports_its_progress_over_the_socket_as_the_task_compare() {
    let s = start();
    let mut socket = open_socket(s.port);
    assert_eq!(s.get("/api/compare?ref=v1.2").status, 200);
    loop {
        if let ServerMessage::Progress {
            task,
            board,
            message,
            stage,
            done,
            total,
        } = next_message(&mut socket, "the progress of the compare")
        {
            assert_eq!(
                (task.as_str(), board.as_str(), message.as_str()),
                ("compare", "v1.2", "reading v1.2 out of git")
            );
            assert_eq!(
                (stage.as_deref(), done, total),
                (Some("extract"), Some(0), Some(1))
            );
            break;
        }
    }
}

#[test]
fn a_compare_that_is_running_holds_up_no_other_route() {
    let s = start();
    let port = s.port;
    let backend = Arc::clone(&s.backend);
    let slow = thread::spawn(move || {
        let s = Server { port, backend };
        s.get("/api/compare?ref=slow")
    });
    // It has begun when it has said so.
    let mut socket = open_socket(port);
    let deadline = Instant::now() + Duration::from_secs(10);
    let began = loop {
        let message = next_message(&mut socket, "the first line of the compare");
        if matches!(message, ServerMessage::Progress { ref task, .. } if task == "compare") {
            break true;
        }
        assert!(Instant::now() < deadline);
    };
    assert!(began);
    assert!(!slow.is_finished());
    // While it waits the page's other questions are answered.
    assert_eq!(s.get("/api/project").status, 200);
    assert_eq!(s.get("/api/git").status, 200);
    assert_eq!(s.get("/api/analysis").status, 200);
    assert!(!slow.is_finished(), "the compare is still running");
    s.backend.release_compare.store(true, Ordering::SeqCst);
    let reply = slow.join().unwrap();
    assert_eq!(reply.status, 200, "{}", reply.body);
}

#[test]
fn while_the_first_analysis_runs_git_is_answered_and_a_compare_that_asks_is_told_pending() {
    let s = start_pending();
    let reply = s.request("GET", "/api/git", &[("X-Netsmith-Pending", "1")], "", None);
    assert_eq!(reply.status, 200, "git needs no analysis: {}", reply.body);
    let reply = s.request(
        "GET",
        "/api/git/file?ref=HEAD&path=a.nsm",
        &[("X-Netsmith-Pending", "1")],
        "",
        None,
    );
    assert_eq!(reply.status, 200, "{}", reply.body);
    let reply = s.request(
        "GET",
        "/api/compare?ref=HEAD",
        &[("X-Netsmith-Pending", "1")],
        "",
        None,
    );
    assert_eq!(reply.status, 503, "{}", reply.body);
    assert!(reply.body.contains("\"pending\":true"), "{}", reply.body);
}
