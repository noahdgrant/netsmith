//! `netsmith view` against the editor's real server with a page simulated over its socket
//! (P8b of issue 430): the state, a change and a picture, a refusal of the page, no page open,
//! and nothing listening. The page here answers as the browser's does; the editor's own tests
//! (`crates/netsmith-editor/tests/server.rs`) pin the routes, these pin the command.

use netsmith_testkit::TempDir;
use std::{
    fs,
    net::{Ipv4Addr, TcpListener, TcpStream},
    process::{Child, Command, Output, Stdio},
    sync::Arc,
    thread,
    time::Duration,
};

use netsmith::editor::Workspace;
use netsmith::project::Target;
use netsmith_editor::api::{
    CameraPlace, CaseMode, ClientMessage, ColourBy, RenderView, ServerMessage, ViewChange,
    ViewState,
};

const LED: &str = include_str!("../../../examples/led.nsm");
const WAIT: Duration = Duration::from_secs(60);

type Socket = tungstenite::WebSocket<TcpStream>;

struct Editor {
    port: u16,
    _root: TempDir,
}

/// The editor's server over a small project, on a free port.
fn start() -> Editor {
    let root = TempDir::new("view-cli");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("netsmith.toml"),
        "[project]\nname = \"demo\"\ntop_level = [\"src/main.nsm\"]\n",
    )
    .unwrap();
    fs::write(root.join("src/main.nsm"), LED).unwrap();
    let workspace = Workspace::open(Target::resolve_from(&root, None).unwrap()).unwrap();
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || netsmith_editor::serve(listener, Arc::new(workspace)));
    Editor { port, _root: root }
}

/// A page's socket, as the browser opens it.
fn open_page(port: u16) -> Socket {
    let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    stream.set_read_timeout(Some(WAIT)).unwrap();
    stream.set_write_timeout(Some(WAIT)).unwrap();
    let url = format!("ws://127.0.0.1:{port}/api/events");
    tungstenite::client(url.as_str(), stream).unwrap().0
}

/// The next ask for the view, skipping the pushes of the server.
fn next_ask(page: &mut Socket) -> ServerMessage {
    loop {
        let tungstenite::Message::Text(text) = page.read().expect("an ask of the view") else {
            continue;
        };
        let m: ServerMessage = serde_json::from_str(&text).unwrap();
        if matches!(
            m,
            ServerMessage::View { .. } | ServerMessage::ViewPicture { .. }
        ) {
            return m;
        }
    }
}

fn state() -> ViewState {
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
        section: Some("x=13".into()),
        exploded: Some(false),
        highlight: vec!["J1".into(), "limit:lid".into()],
        selection: vec!["J1".into()],
        hover: None,
        xray: false,
        case: Some(CaseMode::SeeThrough),
        colour_by: ColourBy::Provenance,
        height_preview: None,
        theme: "dark".into(),
        command: Some("netsmith render src/main.nsm --board Led --view iso".into()),
    }
}

fn answer(page: &mut Socket, id: u32, state: ViewState) {
    let m = ClientMessage::ViewState {
        id,
        state: Box::new(state),
    };
    page.send(tungstenite::Message::text(
        serde_json::to_string(&m).unwrap(),
    ))
    .unwrap();
}

/// `netsmith view <args>` as a child, so that the test plays the page while it waits.
fn view(port: u16, args: &[&str]) -> Child {
    Command::new(env!("CARGO_BIN_EXE_netsmith"))
        .arg("view")
        .args(["--port", &port.to_string()])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn finished(child: Child) -> Output {
    child.wait_with_output().unwrap()
}

#[test]
fn with_no_change_it_prints_the_state_as_a_summary_or_as_the_routes_json() {
    let editor = start();
    let mut page = open_page(editor.port);

    let child = view(editor.port, &[]);
    let ServerMessage::View { id, change } = next_ask(&mut page) else {
        unreachable!()
    };
    assert_eq!(change, None, "a read asks for no change");
    answer(&mut page, id, state());
    let out = finished(child);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    for line in [
        "board: Led",
        "view: iso, orthographic",
        "camera: eye (40.0, -30.0, 40.0) looking at (10.0, 8.0, 0.0) (mm)",
        "section: x=13",
        "highlight: J1, limit:lid",
        "selection: J1",
        "x-ray: off",
        "case: see-through",
        "colour by: provenance",
        "height preview: off",
        "theme: dark",
        "render: netsmith render src/main.nsm --board Led --view iso",
    ] {
        assert!(shown.contains(line), "no `{line}` in:\n{shown}");
    }

    // `--json` is the route's JSON as it came: the state, read back, is the page's.
    let child = view(editor.port, &["--json"]);
    let ServerMessage::View { id, .. } = next_ask(&mut page) else {
        unreachable!()
    };
    answer(&mut page, id, state());
    let out = finished(child);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let read: ViewState = serde_json::from_str(&text(&out.stdout)).unwrap();
    assert_eq!(read, state());
}

/// `--board System` asks the page for the System view (ADR 0110) with a member's ids as they are
/// written, and the summary says which member the page's board is.
#[test]
fn a_system_name_asks_for_the_system_view_and_the_summary_names_the_member() {
    let editor = start();
    let mut page = open_page(editor.port);

    let child = view(
        editor.port,
        &["--board", "TwoBoards", "--select", "net:led/GND"],
    );
    let ServerMessage::View { id, change } = next_ask(&mut page) else {
        unreachable!()
    };
    let change = change.expect("a change");
    assert_eq!(change.board.as_deref(), Some("TwoBoards"));
    assert_eq!(change.select, Some(vec!["net:led/GND".to_string()]));
    let mut system = state();
    system.board = Some("TwoBoards".into());
    system.member = Some("led".into());
    system.highlight = vec!["net:led/GND".into()];
    system.selection = vec!["net:led/GND".into()];
    answer(&mut page, id, system);
    let out = finished(child);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    for line in [
        "board: TwoBoards",
        "member: led (the page's board, in the System view)",
        "selection: net:led/GND",
    ] {
        assert!(shown.contains(line), "no `{line}` in:\n{shown}");
    }
}

#[test]
fn a_change_reaches_the_page_with_renders_names_and_the_picture_is_saved() {
    let editor = start();
    let mut page = open_page(editor.port);
    let dir = TempDir::new("view-cli-picture");
    let png = dir.join("j1.png");

    let child = view(
        editor.port,
        &[
            "--board",
            "Led",
            "--view",
            "top",
            "--section",
            "none",
            "--highlight",
            "J1,net:GND",
            "--xray",
            "--case",
            "solid",
            "--perspective",
            "--colour-by",
            "kind",
            "--height-preview",
            "2",
            "--layers-apart",
            "--picture",
            png.to_str().unwrap(),
        ],
    );
    let ServerMessage::View {
        id,
        change: Some(asked),
    } = next_ask(&mut page)
    else {
        panic!("a change was asked")
    };
    let json = serde_json::to_value(&asked).unwrap();
    assert_eq!(
        json,
        serde_json::json!({
            "board": "Led", "view": "top", "section": "none",
            "highlight": ["J1", "net:GND"], "xray": true, "case": "solid", "ortho": false,
            "colourBy": "kind", "heightPreview": 2.0, "exploded": true,
        })
    );
    let mut after = state();
    after.view = Some(RenderView::Top);
    after.section = None;
    after.xray = true;
    answer(&mut page, id, after);

    // The picture is asked for after the change, and comes back as a binary frame.
    let ServerMessage::ViewPicture { id } = next_ask(&mut page) else {
        panic!("a picture was asked")
    };
    let mut frame = id.to_le_bytes().to_vec();
    frame.extend_from_slice(b"\x89PNG\r\n\x1a\nthe picture");
    page.send(tungstenite::Message::binary(frame)).unwrap();

    let out = finished(child);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let shown = text(&out.stdout);
    assert!(
        shown.contains("view: top, orthographic") && shown.contains("x-ray: on"),
        "{shown}"
    );
    assert!(
        shown.contains(&format!("picture: {}", png.display())),
        "{shown}"
    );
    assert_eq!(fs::read(&png).unwrap(), b"\x89PNG\r\n\x1a\nthe picture");
}

#[test]
fn a_picture_alone_opens_the_view_with_an_empty_change() {
    let editor = start();
    let mut page = open_page(editor.port);
    let dir = TempDir::new("view-cli-picture-only");
    let png = dir.join("a.png");
    let child = view(editor.port, &["--json", "--picture", png.to_str().unwrap()]);
    let ServerMessage::View { id, change } = next_ask(&mut page) else {
        unreachable!()
    };
    assert_eq!(change, Some(ViewChange::default()));
    answer(&mut page, id, state());
    let ServerMessage::ViewPicture { id } = next_ask(&mut page) else {
        panic!("a picture was asked")
    };
    let mut frame = id.to_le_bytes().to_vec();
    frame.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    page.send(tungstenite::Message::binary(frame)).unwrap();
    let out = finished(child);
    assert!(out.status.success(), "{}", text(&out.stderr));
    // With `--json` the output is the JSON alone.
    serde_json::from_str::<ViewState>(&text(&out.stdout)).unwrap();
    assert!(png.exists());
}

#[test]
fn a_change_the_page_refuses_exits_two_with_its_message_and_writes_nothing() {
    let editor = start();
    let mut page = open_page(editor.port);
    let dir = TempDir::new("view-cli-refused");
    let png = dir.join("never.png");
    let child = view(
        editor.port,
        &["--highlight", "J9", "--picture", png.to_str().unwrap()],
    );
    let ServerMessage::View { id, .. } = next_ask(&mut page) else {
        unreachable!()
    };
    let m = ClientMessage::ViewError {
        id,
        message: "highlight names an id that the board does not have: `J9`; it has J1, R1".into(),
        status: None,
    };
    page.send(tungstenite::Message::text(
        serde_json::to_string(&m).unwrap(),
    ))
    .unwrap();
    let out = finished(child);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty(), "{}", text(&out.stdout));
    let said = text(&out.stderr);
    assert!(said.contains("`J9`") && said.contains("J1, R1"), "{said}");
    assert!(!png.exists(), "a refused change takes no picture");
}

#[test]
fn with_no_page_open_it_says_which_address_to_open() {
    let editor = start();
    for args in [&[][..], &["--view", "top"], &["--json"]] {
        let out = finished(view(editor.port, args));
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(out.stdout.is_empty());
        let said = text(&out.stderr);
        assert!(said.contains("no editor page is open"), "{said}");
        assert!(
            said.contains(&format!("http://127.0.0.1:{}", editor.port)),
            "{said}"
        );
    }
}

#[test]
fn with_nothing_listening_it_says_to_start_the_editor() {
    // A port that was free a moment ago.
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let out = finished(view(port, &["--view", "top"]));
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let said = text(&out.stderr);
    assert!(
        said.contains("no editor is listening") && said.contains("netsmith editor"),
        "{said}"
    );
    assert!(said.contains(&port.to_string()), "{said}");
}

#[test]
fn a_theme_is_refused_before_anything_is_asked_and_says_why() {
    let editor = start();
    let out = finished(view(editor.port, &["--theme", "dark"]));
    assert_eq!(out.status.code(), Some(2));
    let said = text(&out.stderr);
    assert!(said.contains("person's own setting"), "{said}");
    assert!(said.contains("render --theme"), "{said}");
}

#[test]
fn another_server_on_the_port_is_not_taken_for_an_editor() {
    // Something that answers 200 with a page that is not a view state.
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    thread::spawn(move || {
        use std::io::{Read, Write};
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut buf = [0u8; 2048];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\n<html>",
            );
        }
    });
    let out = finished(view(port, &[]));
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("is that a netsmith editor?"),
        "{}",
        text(&out.stderr)
    );
}
