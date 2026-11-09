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
    AnalysisEvent, ApiError, Attachment, Backend, Cancel, Observer, Work,
    api::{
        Analysis, BoardDetail, Bom, BuildReport, CatalogRefresh, Changed, ClientMessage, Compare,
        CompareHead, Definition, Download, FabReport, FileContent, GitCommit, GitInfo,
        OrderBoardInfo, OrderInfo, OrderReport, PanelView, PcbReport, PowerTree, ProblemsDiff,
        Project, Saved, SchematicHints, ServerMessage, SourceDiff, SystemView,
    },
};

/// Answers each question with something recognisable.
struct Fake {
    /// What the page last said about analysing on change.
    auto: AtomicBool,
    version: AtomicU64,
    /// The first analysis is running: `analysis` and `check` wait for it, as a real one does.
    pending: AtomicBool,
    observer: Mutex<Option<Observer>>,
    /// A compare of the ref `slow` runs until this is set.
    release_compare: AtomicBool,
    /// What the jobs that run until they are cancelled (`hang`) saw, as `<job> cancelled`.
    cancelled_jobs: Mutex<Vec<String>>,
    /// How many of those are running.
    hanging: AtomicU64,
    /// `build` runs until it is cancelled.
    hang_build: AtomicBool,
    /// The analysis is running, and `cancel_analysis` says so (and counts).
    analysing: AtomicBool,
    cancels_of_analysis: AtomicU64,
    reruns: AtomicU64,
}

impl Fake {
    fn new(version: u64, pending: bool) -> Fake {
        Fake {
            auto: AtomicBool::new(true),
            version: AtomicU64::new(version),
            pending: AtomicBool::new(pending),
            observer: Mutex::new(None),
            release_compare: AtomicBool::new(false),
            cancelled_jobs: Mutex::new(Vec::new()),
            hanging: AtomicU64::new(0),
            analysing: AtomicBool::new(false),
            cancels_of_analysis: AtomicU64::new(0),
            reruns: AtomicU64::new(0),
            hang_build: AtomicBool::new(false),
        }
    }

    fn wait_for_the_first_analysis(&self) {
        while self.pending.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(10));
        }
    }

    /// Runs until `cancel` is set (a job that checks it, as the real ones do), then says so.
    fn hang(&self, what: &str, cancel: &Cancel) -> ApiError {
        self.hanging.fetch_add(1, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(20);
        while !cancel.is_cancelled() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let was = cancel.is_cancelled();
        self.hanging.fetch_sub(1, Ordering::SeqCst);
        self.cancelled_jobs.lock().unwrap().push(format!(
            "{what} {}",
            if was { "cancelled" } else { "timed out" }
        ));
        ApiError::cancelled(format!("{what} was cancelled"))
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
        cancelled: None,
        stale: None,
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
    fn cancel_analysis(&self) -> bool {
        self.cancels_of_analysis.fetch_add(1, Ordering::SeqCst);
        self.analysing.load(Ordering::SeqCst)
    }
    fn set_auto_analysis(&self, auto: bool) {
        self.auto.store(auto, Ordering::SeqCst);
    }
    fn run_analysis(&self) -> Analysis {
        self.reruns.fetch_add(1, Ordering::SeqCst);
        self.analysis()
    }
    fn observe(&self, observer: Observer) {
        *self.observer.lock().unwrap() = Some(observer);
    }
    fn check(&self, path: &str, text: &str) -> Result<Analysis, ApiError> {
        self.wait_for_the_first_analysis();
        match path {
            "refused.nsm" => return Err(ApiError::internal("the check could not start: boom")),
            "panics.nsm" => panic!("a bug in the check"),
            _ => {}
        }
        // The text length stands for what the check found, so the test can see it arrived.
        Ok(analysis(1000 + path.len() as u64 + text.len() as u64))
    }
    fn board(&self, name: &str) -> Result<BoardDetail, ApiError> {
        Err(ApiError::not_found(format!("there is no board `{name}`")))
    }
    fn schematic(&self, board: &str) -> Result<String, ApiError> {
        Ok(format!("<svg data-board=\"{board}\"/>"))
    }
    fn schematic_hints(&self, board: &str) -> Result<SchematicHints, ApiError> {
        if board == "Nope" {
            return Err(ApiError::not_found("there is no board `Nope`"));
        }
        Ok(hints_of(board, "", "none"))
    }
    fn write_schematic_hints(
        &self,
        board: &str,
        text: &str,
        stamp: Option<&str>,
    ) -> Result<SchematicHints, ApiError> {
        if stamp.is_some_and(|s| s != "none") {
            return Err(ApiError::conflict("the file changed on disk"));
        }
        if text.contains("garbage") {
            return Err(ApiError::unprocessable(
                "src/a.sch.toml:3: expected a key",
                "schematic_hints_invalid",
                &[("file", "src/a.sch.toml"), ("line", "3")],
            ));
        }
        Ok(hints_of(board, text, "written"))
    }
    fn power(&self, board: &str) -> Result<PowerTree, ApiError> {
        Ok(PowerTree {
            title: board.into(),
            nodes: Vec::new(),
            edges: Vec::new(),
            stages: Vec::new(),
            issues: Vec::new(),
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
    fn build(&self, say: &netsmith_editor::Say, cancel: &Cancel) -> Result<BuildReport, ApiError> {
        say("writing the schematics");
        if self.hang_build.load(Ordering::SeqCst) {
            return Err(self.hang("build", cancel));
        }
        Ok(BuildReport {
            ok: true,
            written: vec!["out".into()],
            output: String::new(),
            elapsed_ms: 0,
        })
    }
    fn fab(
        &self,
        board: &str,
        _allow_drc_violations: bool,
        _say: &netsmith_editor::Say,
        cancel: &Cancel,
    ) -> Result<FabReport, ApiError> {
        if board == "hang" {
            return Err(self.hang("fab", cancel));
        }
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
        cancel: &Cancel,
    ) -> Result<OrderReport, ApiError> {
        if name == "hang" {
            return Err(self.hang("order", cancel));
        }
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
        cancel: &Cancel,
    ) -> Result<Compare, ApiError> {
        if rev == "nope" {
            return Err(ApiError::not_found(format!("there is no commit `{rev}`")));
        }
        if rev == "hang" {
            return Err(self.hang("compare", cancel));
        }
        observe(AnalysisEvent::Progress {
            stage: "extract".into(),
            board: String::new(),
            done: 0,
            total: 1,
            message: format!("reading {rev} out of git"),
            work: None,
        });
        if rev == "slow" {
            while !self.release_compare.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(10));
            }
        }
        Ok(Compare {
            rev: rev.into(),
            against: None,
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
            systems: Vec::new(),
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

fn hints_of(board: &str, text: &str, stamp: &str) -> SchematicHints {
    SchematicHints {
        board: board.to_string(),
        file: "src/a.sch.toml".to_string(),
        exists: !text.is_empty(),
        stamp: stamp.to_string(),
        text: text.to_string(),
        errors: Vec::new(),
        boards: Default::default(),
        findings: Vec::new(),
        version: 1,
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
    start_with_backlog(version, pending, None)
}

/// A server whose socket may fall `backlog` messages behind before it loses one (`None`: as
/// many as the real one).
fn start_with_backlog(version: u64, pending: bool, backlog: Option<usize>) -> Server {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let backend = Arc::new(Fake::new(version, pending));
    let served = Arc::clone(&backend);
    thread::spawn(move || match backlog {
        Some(backlog) => netsmith_editor::serve_with_backlog(listener, served, backlog),
        None => netsmith_editor::serve(listener, served),
    });
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
fn the_schematic_hints_are_read_with_get_and_written_with_a_put_that_needs_the_guard_header() {
    let s = start();
    let r = s.get("/api/schematic-hints?board=B");
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(r.headers.contains("application/json"), "{}", r.headers);
    let body: serde_json::Value = serde_json::from_str(&r.body).unwrap();
    assert_eq!(body["board"], "B");
    assert_eq!(body["exists"], false);
    assert_eq!(body["stamp"], "none");
    assert_eq!(body["errors"], serde_json::json!([]));
    assert_eq!(body["findings"], serde_json::json!([]));
    assert_eq!(
        s.get("/api/schematic-hints").status,
        400,
        "a board is needed"
    );
    assert_eq!(s.get("/api/schematic-hints?board=Nope").status, 404);

    // A change needs the header, like every change.
    let r = s.request(
        "PUT",
        "/api/schematic-hints?board=B",
        &[],
        "version = 1\n",
        None,
    );
    assert_eq!(r.status, 403);
    assert!(r.body.contains("X-Netsmith"), "{}", r.body);
    // The body is the text of the file; the stamp is optional, and a stale one is a conflict.
    let r = s.request(
        "PUT",
        "/api/schematic-hints?board=B",
        &[GUARD],
        "version = 1\n",
        None,
    );
    assert_eq!(r.status, 200, "{}", r.body);
    let body: serde_json::Value = serde_json::from_str(&r.body).unwrap();
    assert_eq!(
        (body["exists"].clone(), body["stamp"].clone()),
        (true.into(), "written".into())
    );
    let r = s.request(
        "PUT",
        "/api/schematic-hints?board=B",
        &[GUARD, ("X-Stamp", "5")],
        "version = 1\n",
        None,
    );
    assert_eq!(r.status, 409);
    // A text that does not read is `422`, with the line in the structured body.
    let r = s.request(
        "PUT",
        "/api/schematic-hints?board=B",
        &[GUARD, ("Accept", "application/json")],
        "garbage",
        None,
    );
    assert_eq!(r.status, 422);
    let body: serde_json::Value = serde_json::from_str(&r.body).unwrap();
    assert_eq!(body["code"], "schematic_hints_invalid");
    assert_eq!(body["line"], "3");
    assert_eq!(body["file"], "src/a.sch.toml");
}

#[test]
fn the_schematic_hints_wait_for_the_first_analysis_like_the_views() {
    let s = start_pending();
    let r = s.request(
        "GET",
        "/api/schematic-hints?board=B",
        &[("X-Netsmith-Pending", "1")],
        "",
        None,
    );
    assert_eq!(r.status, 503, "{}", r.body);
    assert!(r.body.contains("\"pending\":true"), "{}", r.body);
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
            backend: Arc::new(Fake::new(0, false)),
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
        work: None,
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
            ..
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
            ..
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
    // The page's socket is open before the compare is asked for: a line said before a socket
    // is there is not sent to it, and a request that is quick would say its first line first.
    let mut socket = open_socket(port);
    let slow = thread::spawn(move || {
        let s = Server { port, backend };
        s.get("/api/compare?ref=slow")
    });
    // It has begun when it has said so.
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

/// Sends a check for `path` and waits for the answer to it.
fn ask_check(socket: &mut Socket, id: u32, path: &str) -> Analysis {
    let ask = ClientMessage::Check {
        id,
        path: path.into(),
        text: "abc".into(),
    };
    socket
        .send(tungstenite::Message::text(
            serde_json::to_string(&ask).unwrap(),
        ))
        .unwrap();
    loop {
        if let ServerMessage::Checked { id: got, analysis } =
            next_message(socket, &format!("the `checked` answer to check {id}"))
        {
            assert_eq!(got, id);
            return analysis;
        }
    }
}

/// A check the backend refuses or panics in was not answered, so the page waited for its id for
/// ever. Every check is answered, with one error that says what happened.
#[test]
fn a_check_that_fails_is_answered_with_its_id_and_an_error() {
    let s = start();
    let mut socket = open_socket(s.port);
    let refused = ask_check(&mut socket, 11, "refused.nsm");
    assert_eq!(refused.errors, 1, "{refused:?}");
    assert!(
        refused.problems[0].message.contains("boom"),
        "{:?}",
        refused.problems
    );
    let panicked = ask_check(&mut socket, 12, "panics.nsm");
    assert_eq!(panicked.errors, 1, "{panicked:?}");
    assert!(
        panicked.problems[0].message.contains("internal error"),
        "{:?}",
        panicked.problems
    );
    // The socket still works.
    assert_eq!(ask_check(&mut socket, 13, "a.nsm").version, 1000 + 5 + 3);
}

/// An analysis that ends in a panic: the page is told, and then `rebuilt` takes it out of
/// "analysing" (the version is new: the last good reading and a problem).
#[test]
fn a_failed_analysis_is_told_and_ends_with_rebuilt() {
    let s = start();
    wait_for_observer(&s);
    let mut socket = open_socket(s.port);
    s.backend.tell(AnalysisEvent::Started { since: 5 });
    s.backend.tell(AnalysisEvent::Failed {
        message: "index out of bounds".into(),
    });
    s.backend.tell(AnalysisEvent::Finished);
    s.backend.version.store(2, Ordering::SeqCst);
    assert_eq!(
        next_message(&mut socket, "analysing"),
        ServerMessage::Analysing { since: 5 }
    );
    match next_message(&mut socket, "the failure") {
        ServerMessage::Progress { task, message, .. } => {
            assert_eq!(task, "analyse");
            assert!(message.contains("index out of bounds"), "{message}");
        }
        other => panic!("expected progress, got {other:?}"),
    }
    assert!(matches!(
        next_message(&mut socket, "rebuilt after the failure"),
        ServerMessage::Rebuilt { .. }
    ));
    // A page that connects now is not told of an analysis that is over.
    let mut late = open_socket(s.port);
    s.backend.version.store(3, Ordering::SeqCst);
    assert!(matches!(
        next_message(&mut late, "rebuilt, with no `analysing` before it"),
        ServerMessage::Rebuilt { .. }
    ));
}

/// A socket that is not read falls behind and loses messages, among them a `rebuilt` that does
/// not come again. It is told where things are now.
#[test]
fn a_socket_that_lost_messages_is_told_the_current_state() {
    let s = start_with_backlog(1, false, Some(1));
    wait_for_observer(&s);
    let mut socket = open_socket(s.port);
    // The page does not read: lines of a megabyte fill the connection until the server's end
    // waits, and the one message the channel keeps is replaced by each one after it.
    s.backend.tell(AnalysisEvent::Started { since: 5 });
    for n in 0..40u32 {
        s.backend.tell(AnalysisEvent::Progress {
            stage: "draw".into(),
            board: "Main".into(),
            done: n,
            total: 40,
            message: "x".repeat(1 << 20),
            work: None,
        });
    }
    s.backend.tell(AnalysisEvent::Finished);
    // The files change; the watcher says so (`rebuilt`, version 2) while the socket waits...
    s.backend.version.store(2, Ordering::SeqCst);
    thread::sleep(Duration::from_millis(1800));
    // ... and a message after it takes its place in the channel.
    s.backend.tell(AnalysisEvent::Progress {
        stage: "draw".into(),
        board: "Main".into(),
        done: 0,
        total: 1,
        message: "after".into(),
        work: None,
    });
    let deadline = Instant::now() + WAIT;
    loop {
        if let ServerMessage::Rebuilt { analysis, .. } = next_message(
            &mut socket,
            "a `rebuilt` of version 2 after the lost messages",
        ) {
            assert_eq!(analysis.version, 2);
            break;
        }
        assert!(Instant::now() < deadline, "no `rebuilt` of version 2");
    }
}

// ---- cancellation (ADR 0062) ----------------------------------------------------------------

/// Waits until `condition` holds, or fails saying what was awaited.
fn wait_until(awaiting: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(Instant::now() < deadline, "never happened: {awaiting}");
        thread::sleep(Duration::from_millis(5));
    }
}

/// A request in a thread of its own, answered when the job ends.
fn in_background(
    s: &Server,
    method: &'static str,
    target: &'static str,
) -> thread::JoinHandle<Reply> {
    let port = s.port;
    thread::spawn(move || {
        Server {
            port,
            backend: Arc::new(Fake::new(1, false)),
        }
        .request(method, target, &[GUARD], "", None)
    })
}

fn cancelled_jobs(s: &Server) -> Vec<String> {
    s.backend.cancelled_jobs.lock().unwrap().clone()
}

/// The next `cancelled` message, skipping the others.
fn next_cancelled(socket: &mut Socket) -> (String, String, String) {
    loop {
        if let ServerMessage::Cancelled {
            task,
            board,
            message,
        } = next_message(socket, "a `cancelled` message")
        {
            return (task, board, message);
        }
    }
}

#[test]
fn the_cancel_routes_are_posts_with_the_guard_header_that_answer_whether_a_job_ran() {
    let s = start();
    for route in [
        "/api/analysis/cancel",
        "/api/compare/cancel",
        "/api/build/cancel",
        "/api/fab/cancel",
        "/api/order/cancel",
    ] {
        assert_eq!(
            s.request("POST", route, &[], "", None).status,
            403,
            "{route}"
        );
        assert_eq!(s.get(route).status, 405, "{route} is not a GET");
        let r = s.request("POST", route, &[GUARD], "", None);
        assert_eq!(r.status, 200, "{route}");
        assert_eq!(
            r.body.trim(),
            r#"{"cancelled":false}"#,
            "{route}: nothing is running to cancel"
        );
    }
    // The analysis is the backend's: the route says what the backend says.
    s.backend.analysing.store(true, Ordering::SeqCst);
    let r = s.request("POST", "/api/analysis/cancel", &[GUARD], "", None);
    assert_eq!(r.body.trim(), r#"{"cancelled":true}"#);
    // "Run again" asks the backend to analyse, and answers with the reading.
    assert_eq!(
        s.request("POST", "/api/analysis/run", &[], "", None).status,
        403
    );
    let r = s.request("POST", "/api/analysis/run", &[GUARD], "", None);
    assert_eq!(r.status, 200);
    assert_eq!(s.backend.reruns.load(Ordering::SeqCst), 1);
    let again: Analysis = serde_json::from_str(&r.body).unwrap();
    assert_eq!(again.version, 1);
}

#[test]
fn a_compare_is_cancelled_by_its_route_and_by_a_disconnect() {
    let s = start();
    let mut socket = open_socket(s.port);

    // By its route: the request is answered 499, the page is told, the job saw its token.
    let compare = in_background(&s, "GET", "/api/compare?ref=hang");
    wait_until("the compare is running", || {
        s.backend.hanging.load(Ordering::SeqCst) == 1
    });
    let r = s.request("POST", "/api/compare/cancel", &[GUARD], "", None);
    assert_eq!(r.body.trim(), r#"{"cancelled":true}"#);
    let answered = compare.join().unwrap();
    assert_eq!(answered.status, 499, "{}", answered.body);
    assert_eq!(
        next_cancelled(&mut socket),
        (
            "compare".to_string(),
            "hang".to_string(),
            "compare was cancelled".to_string()
        )
    );
    assert_eq!(cancelled_jobs(&s), ["compare cancelled"]);
    assert_eq!(
        s.request("POST", "/api/compare/cancel", &[GUARD], "", None)
            .body
            .trim(),
        r#"{"cancelled":false}"#,
        "it is over"
    );

    // By a disconnect: the page closes the connection (a page that reloads, a tab that
    // closes) and the job, which nobody will read, is cancelled.
    let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, s.port)).unwrap();
    write!(
        client,
        "GET /api/compare?ref=hang HTTP/1.1\r\nHost: 127.0.0.1:{}\r\n\r\n",
        s.port
    )
    .unwrap();
    wait_until("the second compare is running", || {
        s.backend.hanging.load(Ordering::SeqCst) == 1
    });
    drop(client);
    wait_until("the compare of the page that left was cancelled", || {
        cancelled_jobs(&s).len() == 2
    });
    assert_eq!(
        cancelled_jobs(&s),
        ["compare cancelled", "compare cancelled"]
    );
}

#[test]
fn a_build_a_fab_and_an_order_are_cancelled_by_their_routes_and_outlive_a_disconnect() {
    let s = start();
    let mut socket = open_socket(s.port);
    s.backend.hang_build.store(true, Ordering::SeqCst);
    for (kind, start_request, cancel_route, board) in [
        ("build", ("POST", "/api/build"), "/api/build/cancel", ""),
        (
            "fab",
            ("POST", "/api/fab?board=hang"),
            "/api/fab/cancel",
            "hang",
        ),
        (
            "order",
            ("POST", "/api/order?name=hang"),
            "/api/order/cancel",
            "hang",
        ),
    ] {
        let running = in_background(&s, start_request.0, start_request.1);
        wait_until(&format!("the {kind} is running"), || {
            s.backend.hanging.load(Ordering::SeqCst) == 1
        });
        let r = s.request("POST", cancel_route, &[GUARD], "", None);
        assert_eq!(r.body.trim(), r#"{"cancelled":true}"#, "{kind}");
        let answered = running.join().unwrap();
        assert_eq!(answered.status, 499, "{kind}: {}", answered.body);
        let (task, told_board, _) = next_cancelled(&mut socket);
        assert_eq!((task.as_str(), told_board.as_str()), (kind, board));
        // The structured form, for a page that asks for it.
        let again = in_background(&s, start_request.0, start_request.1);
        wait_until(&format!("the {kind} is running again"), || {
            s.backend.hanging.load(Ordering::SeqCst) == 1
        });
        s.request("POST", cancel_route, &[GUARD], "", None);
        again.join().unwrap();
        next_cancelled(&mut socket);
    }
    assert_eq!(cancelled_jobs(&s).len(), 6);
    assert!(cancelled_jobs(&s).iter().all(|c| c.ends_with(" cancelled")));

    // A page that goes away does not take a `fab` with it: it has files to write.
    let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, s.port)).unwrap();
    write!(
        client,
        "POST /api/fab?board=hang HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nX-Netsmith: 1\r\nContent-Length: 0\r\n\r\n",
        s.port
    )
    .unwrap();
    wait_until("the fab is running", || {
        s.backend.hanging.load(Ordering::SeqCst) == 1
    });
    drop(client);
    thread::sleep(Duration::from_millis(600));
    assert_eq!(
        s.backend.hanging.load(Ordering::SeqCst),
        1,
        "the fab goes on without its page"
    );
    let r = s.request("POST", "/api/fab/cancel", &[GUARD], "", None);
    assert_eq!(r.body.trim(), r#"{"cancelled":true}"#);
    wait_until("the fab ends", || {
        s.backend.hanging.load(Ordering::SeqCst) == 0
    });
}

#[test]
fn a_cancelled_job_answers_with_the_structured_body_a_page_that_asks_for_json_reads() {
    let s = start();
    let running = in_background(&s, "GET", "/api/compare?ref=hang");
    wait_until("the compare is running", || {
        s.backend.hanging.load(Ordering::SeqCst) == 1
    });
    s.request("POST", "/api/compare/cancel", &[GUARD], "", None);
    let _ = running.join().unwrap();
    // The same request, asked for as JSON.
    let port = s.port;
    let asking = thread::spawn(move || {
        Server {
            port,
            backend: Arc::new(Fake::new(1, false)),
        }
        .request(
            "GET",
            "/api/compare?ref=hang",
            &[("Accept", "application/json")],
            "",
            None,
        )
    });
    wait_until("the compare is running again", || {
        s.backend.hanging.load(Ordering::SeqCst) == 1
    });
    s.request("POST", "/api/compare/cancel", &[GUARD], "", None);
    let r = asking.join().unwrap();
    assert_eq!(r.status, 499);
    let body: serde_json::Value = serde_json::from_str(&r.body).unwrap();
    assert_eq!(body["code"], "cancelled", "{body}");
    assert_eq!(body["message"], "compare was cancelled");
}

#[test]
fn what_the_solver_has_counted_and_a_cancelled_analysis_reach_the_page() {
    let s = start_pending();
    wait_for_observer(&s);
    let mut socket = open_socket(s.port);
    s.backend.tell(AnalysisEvent::Started { since: 7 });
    s.backend.tell(AnalysisEvent::Progress {
        stage: "solve".into(),
        board: "Main".into(),
        done: 0,
        total: 1,
        message: "solving Main (1 of 1): 41 of 118 parts chosen".into(),
        work: Some(Work {
            unit: "parts chosen".into(),
            done: 41,
            total: Some(118),
        }),
    });
    s.backend.tell(AnalysisEvent::Progress {
        stage: "design".into(),
        board: "Main".into(),
        done: 0,
        total: 1,
        message: "designing Main (1 of 1): 2 alternative parts re-solved".into(),
        work: Some(Work {
            unit: "alternative parts re-solved".into(),
            done: 2,
            total: None,
        }),
    });
    s.backend.tell(AnalysisEvent::Cancelled);
    s.backend.tell(AnalysisEvent::Finished);
    assert_eq!(
        next_message(&mut socket, "analysing"),
        ServerMessage::Analysing { since: 7 }
    );
    // The count rides on the progress message, as optional fields beside the board's own.
    let counted = |m: ServerMessage| match m {
        ServerMessage::Progress {
            task,
            stage,
            done,
            total,
            unit,
            unit_done,
            unit_total,
            ..
        } => (task, stage, done, total, unit, unit_done, unit_total),
        other => panic!("not a progress message: {other:?}"),
    };
    assert_eq!(
        counted(next_message(&mut socket, "the solver's count")),
        (
            "analyse".to_string(),
            Some("solve".to_string()),
            Some(0),
            Some(1),
            Some("parts chosen".to_string()),
            Some(41),
            Some(118)
        )
    );
    // No total is invented for what has none.
    assert_eq!(
        counted(next_message(&mut socket, "the count of alternatives")),
        (
            "analyse".to_string(),
            Some("design".to_string()),
            Some(0),
            Some(1),
            Some("alternative parts re-solved".to_string()),
            Some(2),
            None
        )
    );
    assert_eq!(
        next_cancelled(&mut socket),
        (
            "analyse".to_string(),
            String::new(),
            "the analysis was cancelled".to_string()
        )
    );
    // On the wire the fields are camelCase and left out when there is nothing to say.
    let plain = serde_json::to_value(ServerMessage::Progress {
        task: "fab".into(),
        board: "B".into(),
        message: "placing".into(),
        stage: None,
        done: None,
        total: None,
        unit: None,
        unit_done: None,
        unit_total: None,
    })
    .unwrap();
    assert_eq!(
        plain,
        serde_json::json!({"type": "progress", "task": "fab", "board": "B", "message": "placing"})
    );
}

#[test]
fn the_page_says_whether_a_change_is_analysed_at_once() {
    let s = start();
    assert_eq!(
        s.request("POST", "/api/analysis/auto", &[], r#"{"auto":false}"#, None)
            .status,
        403
    );
    let r = s.request(
        "POST",
        "/api/analysis/auto",
        &[GUARD, ("Content-Type", "application/json")],
        r#"{"auto":false}"#,
        None,
    );
    assert_eq!(r.status, 200, "{}", r.body);
    assert!(!s.backend.auto.load(Ordering::SeqCst));
    s.request(
        "POST",
        "/api/analysis/auto",
        &[GUARD, ("Content-Type", "application/json")],
        r#"{"auto":true}"#,
        None,
    );
    assert!(s.backend.auto.load(Ordering::SeqCst));
}
