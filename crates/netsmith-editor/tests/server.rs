//! The server over a real socket, with a backend that knows no compiler.

use std::{
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use netsmith_editor::{
    ApiError, Attachment, Backend,
    api::{
        Analysis, BoardDetail, Bom, BuildReport, CatalogRefresh, ClientMessage, Definition,
        Download, FabReport, FileContent, OrderBoardInfo, OrderInfo, OrderReport, PanelView,
        PcbReport, PowerTree, Project, Saved, ServerMessage, SystemView,
    },
};

/// Answers each question with something recognisable.
struct Fake {
    version: AtomicU64,
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
        }
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
        analysis(self.version.load(Ordering::SeqCst))
    }
    fn check(&self, path: &str, text: &str) -> Result<Analysis, ApiError> {
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
    fn definition(&self, _: &str, _: &str, offset: usize) -> Result<Option<Definition>, ApiError> {
        Ok((offset == 0).then(|| Definition {
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
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let backend = Arc::new(Fake {
        version: AtomicU64::new(1),
    });
    let served = Arc::clone(&backend);
    thread::spawn(move || netsmith_editor::serve(listener, served));
    // The listener is bound, so connections wait in its queue until the server runs.
    Server { port, backend }
}

impl Server {
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
        ServerMessage::Rebuilt { analysis } => assert_eq!(analysis.version, 2),
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
