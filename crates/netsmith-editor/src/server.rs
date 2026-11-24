//! The HTTP server: routes, the guard against other web pages, and the socket that tells
//! the page when the files changed (ADR 0019).
//!
//! **Why it is shaped so.** One request answers one question, and the server keeps no editing
//! state: the page holds the text of each open file and the backend holds the last build of
//! what is on disk. A path in a request is a name in the project's tree (`src/main.nsm`,
//! `std/sources.nsm`), and the backend accepts only those, so a request cannot read or write
//! another file and `..` means nothing. A save is atomic (a temporary file beside it, then a
//! rename) and answers `409` when the file changed on disk since the `X-Stamp` the page read it
//! at, so a save never overwrites another program's edit. Nothing leaves the machine at run
//! time: the page is embedded in the binary, with no CDN.
//!
//! **The routes, by what they answer.**
//!
//! | Request | Answer |
//! |---|---|
//! | `GET /` and its files | the page, from the binary |
//! | `GET /api/project` | the tree: top-level files, modules, the manifest and the standard library (read-only), with the definitions in each |
//! | `GET /api/analysis`, `POST /api/analysis/run`, `POST /api/analysis/cancel` | the reading of the files on disk; run it again after a cancel; stop the one that runs (ADR 0062) |
//! | `GET`/`PUT /api/file?path=` | `{text, stamp, readonly}`; the `PUT` takes the text and `X-Stamp` |
//! | `POST /api/fs/*` | create, rename and delete files and folders of the project |
//! | `GET /api/board`, `schematic.svg`, `power`, `system`, `panel`, `bom` | the views of a board or a system, computed from the solved model and not from the text |
//! | `GET /api/schematic/export?board=&format=` | the schematic as a file to save: `pdf` (a page for each sheet), `svg` or `png`, the bytes `netsmith view schematic` writes (issue #1091); a system's PDF has its own sheet, then its members' |
//! | `GET /api/schematic-hints`, `PUT /api/schematic-hints` | the schematic layout file beside the board's source (ADR 0061): `422` with the line for text that does not read, `409` for a stale stamp |
//! | `GET /api/git`, `GET /api/compare`, `GET /api/git/file` | what the project can be compared with, what changed in the circuit, a file at a ref (ADR 0059) |
//! | `POST /api/pins/lock`, `POST /api/pins/move` | lock or unlock a pin request, and move it to another pin of its part: the `lock` and the `pin` and `source` lines of its entry in the pin file (ADR 0101), and no other line; `422` with the reason when the choice is refused |
//! | `POST /api/pcb/route`, `cancel`, `edit`, `drc`; `GET /api/pcb/scene`, `findings` | place and route on demand; `cancel` stops the router and keeps what it routed |
//! | `GET /api/pcb/fit?board=&part=` (`near=`, `step=`, `side=`) | where one part fits with every other part where it is on the placed board, what `netsmith layout fit` says: a pose for each turn and face, the nearest place to `near` where nothing is found or the closest try with what is in the way (`PcbFit`, issue #1002); nothing is written; `400` for a part the board has none of, a `near` that is neither a part nor `X,Y`, a `step` not above 0 or a `side` that is not `top` or `bottom` |
//! | `GET /api/pcb/summary?board=` (`near=`, `within=`, `between=`) | the placed board summed up as data, what `netsmith layout summary` prints: totals, area and free space of each face, module boxes, decoupling distances, nets, the parts near a part and the distance between two pads (`PcbSummary`, issue #1003); `400` for a part or pad the board has none of |
//! | `GET /api/pcb/copper?board=` | the board's copper areas as data, what `netsmith board copper` prints: every copper layer with its use (`plane`, `pour`, `signal`), net and a pour's fields, the zones of the routes file counted apart from the process's pours, and the teardrop fills, counted only when the board has been routed here (`PcbCopper`, issue #1085); from the source, no placing, no routing |
//! | `GET /api/pcb/fill?board=` | how the fill of the board's pours, planes and zones joins each pad of its net, as the PCB view has the board placed (and routed) and as the Gerbers fill it: `solid`, `thermal` with the spokes that reach the fill, or `none`, the counts, the pads of other nets it keeps clear of, the copper islands kept and removed (a removed one with its outline, which the 2D view hatches, issue #1188), the starved thermal reliefs and the switching regulators' ground pads a fill does not join solid (`PcbFill`, issues #1187 and #1586), what `netsmith board fill` prints |
//! | `GET /api/pcb/stitching?board=` | the stitching vias of the pours that the export will have and the routes file does not, of the board as the PCB view has it placed (and routed): those the router laid, then those the stitching `fab` runs would add to the copper there, each with its net, centre and diameter (`PcbStitching`, issue #1489), what `netsmith view pcb --stitching` draws |
//! | `GET /api/pcb/rule-areas?board=` | the rule areas of the board as the PCB view has it placed (and routed): the footprints' (a library's keep-out, an RF module's antenna area) and the board's that are not keepouts of the physical file, each with its name, part, copper layers, outlines, holes and the kinds it forbids (`PcbRuleAreas`, issue 1543), the `rule_areas` of `netsmith board rules`, what `netsmith view pcb` hatches |
//! | `GET /api/pcb/nets?board=` | each net's routed and straight length, vias, layer changes and layers, and each diff pair's lengths, skew against the router's target, coupled length, gaps and vias side by side, of the board as the PCB view has it placed (and routed) (`PcbNets`, issue #1223), what `netsmith board nets` prints |
//! | `POST /api/pcb/place/cancel` | stops the placings of the PCB view that run (issue #764): a `scene` (or the placing of a `route`) that is placing answers `499` and keeps nothing; while a `scene` places, the socket carries `progress` with the task `place` (the placer's lines, as `fab` prints them) and `pcb_live` with the parts where the placer has them, and while a `route` routes, `pcb_live` with the copper laid so far |
//! | `POST /api/progress` | a line of a run the editor did not start (`netsmith board fab --to-editor`, an `OutsideProgress`), passed to every page as `outside`; `204` |
//! | `POST /api/pcb/regenerate` | one module placed again with every other part held (issue #825): the proposal, the edit that makes it (sent to `edit` as any edit, one step of undo), where its parts go, the nets routed again, and the findings and totals before and after; nothing is written; while it runs the socket carries `progress` with the task `regenerate`; `409` for a stale stamp, `422` with the reason when it is refused |
//! | `POST /api/pcb/routes` | an edit of the routes file (draw or move a track or via, delete, lock, unlock, put a record back for undo), as `netsmith routes` makes it (the changes may be of several nets: a track drawn with shoved copper), answered with the record of each net it changed before and after; `409` for a stale stamp, `422` with the reason when it is refused |
//! | `GET /api/pcb/pours?board=` | the fill of the placed board's pours, planes and routes-file zones, by copper layer, as SVG paths (`PcbPours`), for the 2D view to draw under the copper; asked after the scene, since filling takes longer than placing |
//! | `POST /api/pcb/walk` | the way for a straight track of a net from its last corner to a point, round the copper the routes file's check reads (issue #726): the corners (at most 45 degrees) or why there is none, always `200`, the router's copper the track crosses (`crossed`, not an obstacle: an edit throws it away), and with `shove` the unlocked shapes of the routes file the way moves aside (`shoved`, or `stuck`: why none could); a track on several layers sends its runs before the last layer change (`earlier`) and the vias at the changes (issue 1009); nothing is written |
//! | `POST /api/pcb/shove` | whether a via, or the shapes a drag moves, may go where they are, by name what they are too near (`reason`), and with `shove` the unlocked shapes of the routes file moved aside (issue 1009); always `200`; nothing is written |
//! | `POST /api/pcb/reroute` | the router's nets a track being drawn (a `PcbWalk`) crosses, routed again round it with everything else fixed, as a preview (issue 1009); nothing is written |
//! | `GET /api/pcb/kicad?board=` | the board as `scene` has it placed and not routed, as a zip (`<board>.placed-kicad.zip`) of what `netsmith layout export` writes: the `.kicad_pcb`, the `.kicad_pro` and the list of the placement's findings, also while the placement has findings (issue #1001) |
//! | `GET /api/pcb/3d?board=` | the board as `scene` has it placed, as glTF 2.0 binary (`model/gltf-binary`, ADR 0105), the bytes `netsmith dev board3d --glb` writes; the layout stamp of that placement in `X-Netsmith-Layout-Stamp` |
//! | `GET /api/gerber?board=` (`layer=`), `POST /api/gerber?name=` (`layer=`), `GET /api/gerber?opened=` (`layer=`) | the files of a fab package read by netsmith's Gerber reader, what `netsmith view gerber` reads (issue #1107): the summary and the SVG of one view (`GerberView`); `board` reads the package `fab` last wrote for it, a `POST` reads the package in its body (a `.zip`, or one Gerber or drill file, up to 32 MiB) and keeps it, `opened` draws another view of the one kept; `404` with no such package, `400` for a view the package has no file for |
//! | `POST /api/build`, `/api/fab`, `/api/order` and their `/cancel` | the commands of the same names; a cancelled job answers `499` and keeps nothing it was making (ADR 0062) |
//! | `GET /api/download`, `/api/order/download` | a file `fab` or an order wrote |
//! | `POST /api/definition` | where the name under the cursor is defined |
//! | `GET /api/view`, `POST /api/view`, `GET /api/view/picture` | the 3D view of the open page, as an agent reads and drives it, with the names of `netsmith view 3d` (P8a of issue 430, docs/BOARD3D.md): its state as JSON, a change answered with the state once the page has drawn it, a PNG of the canvas; `409` with no page open, `504` when the page does not answer |
//! | WebSocket `/api/events` | `rebuilt`, `analysing`, `progress`, `cancelled`, `pcb_live` and `outside` to the page; `check` from the page and `checked` back; `view` and `view_picture` to one page, `focus`, `view_state`, `view_error` and a binary frame (a picture) back |
//!
//! **The guard.** The server listens on `127.0.0.1` only. A request whose Host is not
//! `127.0.0.1:<port>` or `localhost:<port>` is refused (a rebinding page), an `Origin` header, if
//! any, must be the server's own, and a request that changes anything (`PUT`, `POST`) must carry
//! `X-Netsmith: 1`, which a page on another origin cannot send without a preflight the server
//! never answers.
//!
//! **Live diagnostics are a check of the text on the page.** A moment after typing stops the
//! page sends `check` over the socket with the text of the file. The backend builds the project
//! with that text in place of the file's (and of a module in place of its file), in memory, and
//! answers with the problems; nothing is written. A save writes the file, the backend sees the
//! new modification time, analyses again and the server pushes `rebuilt`. Changes are found by
//! polling modification times every half second and not by file events: on WSL the files are
//! often on the Windows side, where inotify events do not arrive. The watcher's baseline is the
//! version read before the server accepts its first connection, and a socket subscribes to the
//! pushes before its handshake is answered; with either taken later, a change made right after
//! a page connects could go unannounced and the page would wait for a push that never came.
//!
//! **The server listens before the first analysis.** The binary opens the workspace without
//! analysing and the watcher makes the first analysis behind the listening socket, as it makes
//! each later one. Until it ends, the page and its files, `GET /api/project`, `GET /api/file`,
//! `PUT /api/file`, `POST /api/definition`, `POST /api/pcb/cancel` and the socket are answered
//! at once, because none of them needs the analysis. Every other route reads it. A request that
//! sends `X-Netsmith-Pending: 1` is told at once `503`, `Retry-After: 1` and
//! `{"pending":true,"elapsedMs":12345}`: `503` is the status a page that is not there already
//! gets, and a client that ignores the body fails where a `202` would have read as success. A
//! request without the header waits until the analysis is done and is then answered as before,
//! so a page that does not know of pending works, only later. A `check` over the socket waits
//! for the first analysis like any request, but the socket does not wait with it: each socket
//! answers its checks on a task of its own, one after the other, and goes on carrying
//! `progress`. `Backend::pending` is lock-free, because the analysis holds the state's lock for
//! as long as it runs.
//!
//! **An agent drives the page, not a copy of it.** `/api/view` reaches the page that had the focus
//! last ([`crate::pages`]) over its socket and waits for its answer: the page applies a change with
//! the code its keys and palette run, so an agent sees what a person would, and the state it reads
//! is the page's own. The server checks only the shape of a change; the page checks the ids, the
//! section and the board against the scene it shows, as `netsmith view 3d` checks them. Nothing is
//! written and the design does not change: the view is read-only (ADR 0105).
//!
//! **Long jobs do not hold the server.** Routing, `fab`, a build and a compare run on a thread
//! of their own and keep no lock on the editor's state while they run: the state is locked only
//! to read what the job starts from and to store what it made, and a board placed or routed for
//! files that have changed meanwhile is dropped. The jobs report through the functions they
//! already take, the backend forwards those lines to the socket as `progress` with the task and
//! the board, and the page shows the last in its status bar with a clock. A buffer of 256
//! messages for each socket keeps a slow page from losing `rebuilt` among a big project's lines.

use std::{
    collections::HashMap,
    io,
    net::TcpListener,
    sync::{Arc, Mutex},
    time::Duration,
};

use axum::{
    Json, Router,
    body::Body,
    extract::{
        Query, Request, State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Serialize, de::DeserializeOwned};
use tokio::sync::{broadcast, mpsc};

use crate::{
    api::{CancelAnswer, ClientMessage, Definition, Pending, ServerMessage, ViewChange},
    assets,
    backend::{AnalysisEvent, ApiError, Backend, Observer},
    pages::{PageAnswer, Pages},
};
use netsmith_core::job::Cancel;

/// How often the watcher asks the backend whether a file changed.
const WATCH_INTERVAL: Duration = Duration::from_millis(500);
/// A request body larger than this is not a source file.
const MAX_BODY: usize = 4 * 1024 * 1024;
/// A fab package the page sends to the Gerber view may be larger.
const MAX_PACKAGE: usize = 32 * 1024 * 1024;
/// The header a request that changes something must carry. A page on another origin cannot
/// send a custom header without a preflight, which this server never answers.
pub const GUARD_HEADER: &str = "x-netsmith";
/// A request that has this header with the value `1` is told at once that the first analysis
/// is still running (`503` and a [`Pending`] body) by every route that needs the analysis.
/// Without it the request waits until the analysis is done and is then answered as ever, so
/// a page that does not know about pending keeps working.
pub const PENDING_HEADER: &str = "x-netsmith-pending";
/// Messages a socket may fall behind by before it misses one. A big project's analysis sends
/// a few dozen `progress` lines, and `rebuilt` must not be among those that are lost.
const EVENT_BACKLOG: usize = 256;
/// How long `GET /api/view` and the picture wait for the page.
const VIEW_READ_WAIT: Duration = Duration::from_secs(10);
/// How long a change waits: the first may load the 3D view's chunk and the board's scene.
const VIEW_CHANGE_WAIT: Duration = Duration::from_secs(45);
/// The largest frame a page may send: a picture of a large canvas.
const MAX_FRAME: usize = 64 * 1024 * 1024;

/// The routes that wait for the analysis: all those that read the snapshot of the project, or
/// change files and then read it again. `/api/project`, the files, `/api/definition`, the
/// socket, `/api/pcb/cancel` and the page itself never wait for it.
const NEEDS_ANALYSIS: &[&str] = &[
    "/api/analysis",
    "/api/fs/create-file",
    "/api/fs/create-dir",
    "/api/fs/rename",
    "/api/fs/delete",
    "/api/board",
    "/api/schematic.svg",
    "/api/schematic/export",
    "/api/schematic-hints",
    "/api/power",
    "/api/system",
    "/api/panel",
    "/api/bom",
    "/api/swap",
    "/api/compare",
    "/api/catalog/refresh",
    "/api/pins/lock",
    "/api/pins/move",
    "/api/pcb/route",
    "/api/pcb/scene",
    "/api/pcb/pours",
    "/api/pcb/kicad",
    "/api/pcb/edit",
    "/api/pcb/regenerate",
    "/api/pcb/routes",
    "/api/pcb/walk",
    "/api/pcb/shove",
    "/api/pcb/reroute",
    "/api/pcb/findings",
    "/api/pcb/summary",
    "/api/pcb/copper",
    "/api/pcb/fill",
    "/api/pcb/stitching",
    "/api/pcb/rule-areas",
    "/api/pcb/nets",
    "/api/pcb/fit",
    "/api/pcb/drc",
    "/api/build",
    "/api/fab",
    "/api/download",
    "/api/gerber",
    "/api/orders",
    "/api/order",
    "/api/order/download",
];

/// The analysis that is running, for a page that connects while it does.
struct Running {
    since: u64,
    last: Option<ServerMessage>,
}

type Live = Arc<Mutex<Option<Running>>>;

struct AppState {
    backend: Arc<dyn Backend>,
    events: broadcast::Sender<String>,
    running: Live,
    hosts: Vec<String>,
    jobs: Arc<Jobs>,
    pages: Arc<Pages>,
}

/// The jobs that run for a request and can be cancelled (ADR 0062): each has the token the
/// backend watches, filed under the kind of job, so that `POST /api/<kind>/cancel` reaches it
/// and a request that goes away takes its job with it.
#[derive(Default)]
struct Jobs {
    next: std::sync::atomic::AtomicU64,
    running: Mutex<Vec<(&'static str, u64, Cancel)>>,
}

/// A job's place in [`Jobs`], given up when it is dropped. It goes into the thread that runs the
/// job, so it is held until the job ends, whether or not the request that asked for it is still
/// there to hear the answer.
struct Registered {
    jobs: Arc<Jobs>,
    id: u64,
}

impl Jobs {
    /// Starts a job of `kind`: its place in the registry, to be held by the job, and its token.
    fn start(self: &Arc<Self>, kind: &'static str) -> (Registered, Cancel) {
        let id = self.next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let token = Cancel::new();
        self.running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push((kind, id, token.clone()));
        (
            Registered {
                jobs: Arc::clone(self),
                id,
            },
            token,
        )
    }

    /// Cancels every job of `kind` that is running. True if there was one.
    fn cancel(&self, kind: &str) -> bool {
        let running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        let mut any = false;
        for (k, _, token) in running.iter() {
            if *k == kind {
                token.cancel();
                any = true;
            }
        }
        any
    }
}

impl Drop for Registered {
    fn drop(&mut self) {
        self.jobs
            .running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|(_, id, _)| *id != self.id);
    }
}

/// Cancels a job if it is dropped before [`CancelIfDropped::answered`]: held by the handler of a
/// request, it is dropped with the handler's future when the client disconnects. Only the jobs
/// that have nothing worth finishing for a page that left (a compare) hold one; a `fab` has files
/// to write and goes on.
struct CancelIfDropped(Option<Cancel>);

impl CancelIfDropped {
    /// The request got its answer: the job is over, however it ended.
    fn answered(mut self) {
        self.0 = None;
    }
}

impl Drop for CancelIfDropped {
    fn drop(&mut self) {
        if let Some(token) = &self.0 {
            token.cancel();
        }
    }
}

type Shared = Arc<AppState>;

/// A running server. Dropping it does not stop it; the process ends it.
pub struct Server {
    pub port: u16,
}

/// Serve `backend` on `listener` until the process ends. Blocks.
pub fn serve(listener: TcpListener, backend: Arc<dyn Backend>) -> io::Result<Server> {
    serve_with_backlog(listener, backend, EVENT_BACKLOG)
}

/// [`serve`] with a socket that may fall `backlog` messages behind before it misses one
/// (`serve` allows 256). For the tests of what a socket that lost messages is told.
pub fn serve_with_backlog(
    listener: TcpListener,
    backend: Arc<dyn Backend>,
    backlog: usize,
) -> io::Result<Server> {
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    let (events, _) = broadcast::channel(backlog.max(1));
    let running: Live = Arc::default();
    // Told before the first analysis can start, so that its first line is not missed.
    backend.observe(observer(&events, &running));
    // What the files are now, taken before the first connection is accepted: a change
    // after this is a change the sockets must hear of, however late the watcher first runs.
    // A backend that has not analysed yet is at version 0: the watcher announces the first
    // analysis as a change, so a page that connected early hears `rebuilt`.
    let baseline = if backend.pending().is_some() {
        0
    } else {
        backend.analysis().version
    };
    let state = Arc::new(AppState {
        backend,
        events,
        running,
        hosts: vec![format!("127.0.0.1:{port}"), format!("localhost:{port}")],
        jobs: Arc::default(),
        pages: Arc::default(),
    });
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::from_std(listener)?;
        tokio::spawn(watch(Arc::clone(&state), baseline));
        axum::serve(listener, router(state)).await
    })?;
    Ok(Server { port })
}

fn router(state: Shared) -> Router {
    Router::new()
        .route("/api/project", get(project))
        .route("/api/analysis", get(analysis))
        .route("/api/analysis/cancel", post(cancel_analysis))
        .route("/api/analysis/run", post(run_analysis))
        .route("/api/analysis/auto", post(auto_analysis))
        .route("/api/file", get(read_file).put(write_file))
        .route("/api/fs/create-file", post(create_file))
        .route("/api/fs/create-dir", post(create_dir))
        .route("/api/fs/rename", post(rename_path))
        .route("/api/fs/delete", post(delete_path))
        .route("/api/board", get(board))
        .route("/api/schematic.svg", get(schematic))
        .route("/api/schematic/export", get(schematic_export))
        .route(
            "/api/schematic-hints",
            get(schematic_hints).put(write_schematic_hints),
        )
        .route("/api/power", get(power))
        .route("/api/system", get(system))
        .route("/api/panel", get(panel))
        .route("/api/bom", get(bom))
        .route("/api/swap", get(swap).post(swap_apply))
        .route("/api/git", get(git))
        .route("/api/git/file", get(git_file))
        .route("/api/compare", get(compare))
        .route("/api/compare/cancel", post(cancel_compare))
        .route("/api/catalog/refresh", post(refresh_catalogs))
        .route("/api/pins/lock", post(pin_lock))
        .route("/api/pins/move", post(pin_move))
        .route("/api/pcb/route", post(route_pcb))
        .route("/api/pcb/cancel", post(cancel_route))
        .route("/api/pcb/place/cancel", post(cancel_placing))
        .route("/api/progress", post(outside_progress))
        .route("/api/pcb/scene", get(pcb_scene))
        .route("/api/pcb/pours", get(pcb_pours))
        .route("/api/pcb/3d", get(pcb_3d))
        .route("/api/pcb/kicad", get(pcb_kicad))
        .route("/api/pcb/edit", post(pcb_edit))
        .route("/api/pcb/regenerate", post(pcb_regenerate))
        .route("/api/pcb/routes", post(pcb_routes))
        .route("/api/pcb/walk", post(pcb_walk))
        .route("/api/pcb/shove", post(pcb_shove))
        .route("/api/pcb/reroute", post(pcb_reroute))
        .route("/api/pcb/findings", get(pcb_findings))
        .route("/api/pcb/summary", get(pcb_summary))
        .route("/api/pcb/copper", get(pcb_copper))
        .route("/api/pcb/fill", get(pcb_fill))
        .route("/api/pcb/stitching", get(pcb_stitching))
        .route("/api/pcb/rule-areas", get(pcb_rule_areas))
        .route("/api/pcb/nets", get(pcb_nets))
        .route("/api/pcb/fit", get(pcb_fit))
        .route("/api/pcb/drc", post(pcb_drc))
        .route("/api/build", post(build))
        .route("/api/build/cancel", post(cancel_build))
        .route("/api/fab", post(fab))
        .route("/api/fab/cancel", post(cancel_fab))
        .route("/api/download", get(download))
        .route(
            "/api/gerber",
            get(gerber)
                .post(gerber_open)
                .layer(axum::extract::DefaultBodyLimit::max(MAX_PACKAGE)),
        )
        .route("/api/orders", get(orders))
        .route("/api/order", post(fab_order))
        .route("/api/order/cancel", post(cancel_order))
        .route("/api/order/download", get(order_download))
        .route("/api/definition", post(definition))
        .route("/api/format", post(format_source))
        .route("/api/view", get(view_state).post(view_change))
        .route("/api/view/picture", get(view_picture))
        .route("/api/events", get(events))
        .fallback(get(asset))
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY))
        .layer(middleware::from_fn_with_state(Arc::clone(&state), gate))
        .layer(middleware::from_fn_with_state(Arc::clone(&state), guard))
        .with_state(state)
}

/// Refuses a request that a page on another origin could have made: a Host that is not
/// this server's (a DNS rebinding page), an Origin that is not this server's, or a change
/// with no `X-Netsmith` header.
async fn guard(State(state): State<Shared>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let host = headers.get(header::HOST).and_then(|v| v.to_str().ok());
    if !host.is_some_and(|h| state.hosts.iter().any(|ok| ok == h)) {
        return refuse("this editor answers only to 127.0.0.1 and localhost");
    }
    if let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        let own = state.hosts.iter().any(|h| origin == format!("http://{h}"));
        if !own {
            return refuse("this editor answers only to pages it served");
        }
    }
    let changes = !matches!(*request.method(), Method::GET | Method::HEAD);
    if changes && !headers.contains_key(GUARD_HEADER) {
        return refuse("a request that changes something needs the X-Netsmith header");
    }
    next.run(request).await
}

/// Answers a route that needs the analysis with `503` and `{pending, elapsedMs}` while the
/// first analysis runs, to a request that asked for that (`PENDING_HEADER`). Any other
/// request goes on and waits inside the backend until the analysis is done. A failure of
/// the project's data (`ApiError::unprocessable`) is answered with its JSON form instead of
/// text to a request that has `application/json` in its `Accept` header.
async fn gate(State(state): State<Shared>, request: Request, next: Next) -> Response {
    let asks = request
        .headers()
        .get(PENDING_HEADER)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == "1");
    if asks
        && NEEDS_ANALYSIS.contains(&request.uri().path())
        && let Some(elapsed_ms) = state.backend.pending()
    {
        let body = Pending {
            pending: true,
            elapsed_ms,
        };
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::RETRY_AFTER, "1"),
            ],
            serde_json::to_string(&body).unwrap_or_default(),
        )
            .into_response();
    }
    let wants_json = request
        .headers()
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("application/json"));
    let response = next.run(request).await;
    // A failure of the project's data has a structured form for a page that asks for it.
    if wants_json && let Some(body) = response.extensions().get::<StructuredError>() {
        return (
            response.status(),
            [(header::CONTENT_TYPE, "application/json")],
            body.0.clone(),
        )
            .into_response();
    }
    response
}

/// The JSON body of a failure that has one, riding on the response until [`gate`] knows
/// whether the page accepts it.
#[derive(Clone)]
struct StructuredError(String);

fn refuse(message: &str) -> Response {
    ApiError::forbidden(message).into_response()
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (
            status,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!("{}\n", self.message),
        )
            .into_response();
        if let Some(json) = self.json() {
            response.extensions_mut().insert(StructuredError(json));
        }
        response
    }
}

/// Runs a backend call on a thread that may block.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| ApiError::internal(format!("the request failed: {e}")))
}

async fn json<T: Serialize + Send + 'static>(
    f: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<Json<T>, ApiError> {
    blocking(f).await?.map(Json)
}

type Query1 = Query<HashMap<String, String>>;

fn param(query: &HashMap<String, String>, key: &str) -> Result<String, ApiError> {
    query
        .get(key)
        .cloned()
        .ok_or_else(|| ApiError::bad_request(format!("the request needs `{key}`")))
}

fn body<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ApiError> {
    serde_json::from_slice(bytes).map_err(|e| ApiError::bad_request(format!("bad request: {e}")))
}

async fn project(State(s): State<Shared>) -> Result<Json<crate::api::Project>, ApiError> {
    blocking(move || served_project(&*s.backend))
        .await
        .map(Json)
}

/// What `GET /api/project` answers: the backend's tree with what this binary knows of itself
/// added (the hash of its page and the `git describe` it was built from). `netsmith project show`
/// prints it, so the command line and the editor say the same (issue #1138).
pub fn served_project(backend: &(impl Backend + ?Sized)) -> crate::api::Project {
    let mut project = backend.project();
    project.bundle_hash = crate::assets::bundle_hash().map(str::to_string);
    project.git_describe = project
        .git_describe
        .or_else(|| crate::assets::git_describe().map(str::to_string));
    project
}

async fn analysis(State(s): State<Shared>) -> Result<Json<crate::api::Analysis>, ApiError> {
    blocking(move || s.backend.analysis()).await.map(Json)
}

/// Stops the analysis of the files that is running, if one is (ADR 0062). It does not wait for
/// it to end: the socket says `cancelled` when it has.
async fn cancel_analysis(State(s): State<Shared>) -> Json<CancelAnswer> {
    Json(CancelAnswer {
        cancelled: s.backend.cancel_analysis(),
    })
}

/// Sets whether a change to a file is analysed at once or only when the page asks.
async fn auto_analysis(
    State(s): State<Shared>,
    Json(body): Json<crate::api::AutoBuild>,
) -> Json<crate::api::AutoBuild> {
    s.backend.set_auto_analysis(body.auto);
    Json(body)
}

/// Analyses the files again now, as the page's "Run again" after a cancelled analysis. The
/// answer is the reading; the socket carries the progress and `rebuilt` as for any analysis.
async fn run_analysis(State(s): State<Shared>) -> Result<Json<crate::api::Analysis>, ApiError> {
    blocking(move || s.backend.run_analysis()).await.map(Json)
}

/// Cancels the running jobs of `kind`.
fn cancelling(s: &Shared, kind: &'static str) -> Json<CancelAnswer> {
    Json(CancelAnswer {
        cancelled: s.jobs.cancel(kind),
    })
}

async fn cancel_compare(State(s): State<Shared>) -> Json<CancelAnswer> {
    cancelling(&s, "compare")
}

async fn cancel_build(State(s): State<Shared>) -> Json<CancelAnswer> {
    cancelling(&s, "build")
}

async fn cancel_fab(State(s): State<Shared>) -> Json<CancelAnswer> {
    cancelling(&s, "fab")
}

async fn cancel_order(State(s): State<Shared>) -> Json<CancelAnswer> {
    cancelling(&s, "order")
}

/// Tells every page that a job of `task` about `board` was cancelled, if `result` says it was.
fn told_if_cancelled<T>(s: &Shared, task: &str, board: &str, result: &Result<T, ApiError>) {
    if let Err(e) = result
        && e.is_cancelled()
    {
        send(
            &s.events,
            &ServerMessage::Cancelled {
                task: task.to_string(),
                board: board.to_string(),
                message: e.message.clone(),
            },
        );
    }
}

async fn read_file(State(s): State<Shared>, Query(q): Query1) -> Result<Response, ApiError> {
    let path = param(&q, "path")?;
    json(move || s.backend.read_file(&path))
        .await
        .map(IntoResponse::into_response)
}

async fn write_file(
    State(s): State<Shared>,
    Query(q): Query1,
    headers: HeaderMap,
    text: String,
) -> Result<Json<crate::api::Saved>, ApiError> {
    let path = param(&q, "path")?;
    let stamp = headers
        .get("x-stamp")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::bad_request("a save needs the X-Stamp header"))?
        .to_string();
    json(move || s.backend.write_file(&path, &text, &stamp)).await
}

async fn create_file(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<crate::api::Changed>, ApiError> {
    let new: crate::api::NewFile = body(&bytes)?;
    json(move || s.backend.create_file(&new.path, &new.text)).await
}

async fn create_dir(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<crate::api::Changed>, ApiError> {
    let new: crate::api::NewFolder = body(&bytes)?;
    json(move || s.backend.create_dir(&new.path)).await
}

async fn rename_path(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<crate::api::Changed>, ApiError> {
    let rename: crate::api::Rename = body(&bytes)?;
    json(move || s.backend.rename_path(&rename.from, &rename.to)).await
}

async fn delete_path(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<crate::api::Changed>, ApiError> {
    let delete: crate::api::Delete = body(&bytes)?;
    json(move || s.backend.delete_path(&delete.path, delete.confirm)).await
}

async fn board(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::BoardDetail>, ApiError> {
    let name = param(&q, "name")?;
    json(move || s.backend.board(&name)).await
}

fn svg(text: String) -> Response {
    (
        [(header::CONTENT_TYPE, "image/svg+xml; charset=utf-8")],
        text,
    )
        .into_response()
}

async fn schematic(State(s): State<Shared>, Query(q): Query1) -> Result<Response, ApiError> {
    let board = param(&q, "board")?;
    blocking(move || s.backend.schematic(&board))
        .await?
        .map(svg)
}

/// The schematic as a file to save: `format` is `pdf`, `svg` or `png` (issue #1091).
async fn schematic_export(State(s): State<Shared>, Query(q): Query1) -> Result<Response, ApiError> {
    let (board, format) = (param(&q, "board")?, param(&q, "format")?);
    let attachment = blocking(move || s.backend.schematic_export(&board, &format)).await??;
    Ok(attached(attachment))
}

async fn schematic_hints(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::SchematicHints>, ApiError> {
    let board = param(&q, "board")?;
    json(move || s.backend.schematic_hints(&board)).await
}

/// The text of the layout file as the body, as `PUT /api/file` takes a source's; `X-Stamp` is
/// optional here (a page that read the file sends it, so that a hand edit is not overwritten).
async fn write_schematic_hints(
    State(s): State<Shared>,
    Query(q): Query1,
    headers: HeaderMap,
    text: String,
) -> Result<Json<crate::api::SchematicHints>, ApiError> {
    let board = param(&q, "board")?;
    let stamp = headers
        .get("x-stamp")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    json(move || {
        s.backend
            .write_schematic_hints(&board, &text, stamp.as_deref())
    })
    .await
}

async fn power(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PowerTree>, ApiError> {
    let board = param(&q, "board")?;
    json(move || s.backend.power(&board)).await
}

async fn system(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::SystemView>, ApiError> {
    let name = param(&q, "name")?;
    json(move || s.backend.system(&name)).await
}

async fn panel(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PanelView>, ApiError> {
    let name = param(&q, "name")?;
    json(move || s.backend.panel(&name)).await
}

async fn bom(State(s): State<Shared>, Query(q): Query1) -> Result<Json<crate::api::Bom>, ApiError> {
    let board = param(&q, "board")?;
    json(move || s.backend.bom(&board)).await
}

async fn swap(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::Swap>, ApiError> {
    let board = param(&q, "board")?;
    let refdes = param(&q, "refdes")?;
    json(move || s.backend.swap(&board, &refdes)).await
}

async fn swap_apply(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<crate::api::Swapped>, ApiError> {
    let edit: crate::api::SwapApply = body(&bytes)?;
    json(move || s.backend.swap_apply(&edit)).await
}

async fn route_pcb(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PcbReport>, ApiError> {
    let board = param(&q, "board")?;
    let say = progress(&s, "route", &board);
    let show = live(&s);
    json(move || s.backend.route_watched(&board, &say, &show)).await
}

/// Stops the route that is running, if one is.
async fn cancel_route(State(s): State<Shared>) -> StatusCode {
    s.backend.cancel_route();
    StatusCode::NO_CONTENT
}

/// Stops the placings of the PCB view that run (issue #764): their requests answer `499`.
async fn cancel_placing(State(s): State<Shared>) -> StatusCode {
    s.backend.cancel_placing();
    StatusCode::NO_CONTENT
}

/// A way for the placer and the router to show every page what they have made so far.
fn live(s: &Shared) -> crate::backend::Show {
    let events = s.events.clone();
    Arc::new(move |live: &crate::api::PcbLive| {
        send(&events, &ServerMessage::PcbLive { live: live.clone() });
    })
}

/// A line of a run the editor did not start (`netsmith board fab --to-editor`, issue #764), passed to
/// every page as `outside`.
async fn outside_progress(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<StatusCode, ApiError> {
    let progress: crate::api::OutsideProgress = body(&bytes)?;
    send(&s.events, &ServerMessage::Outside { progress });
    Ok(StatusCode::NO_CONTENT)
}

/// A way for a job on a thread of its own to tell every page what it is doing.
fn progress(s: &Shared, task: &str, board: &str) -> crate::backend::Say {
    let events = s.events.clone();
    let (task, board) = (task.to_string(), board.to_string());
    Arc::new(move |message: &str| {
        send(
            &events,
            &ServerMessage::Progress {
                task: task.clone(),
                board: board.clone(),
                message: message.to_string(),
                stage: None,
                done: None,
                total: None,
                unit: None,
                unit_done: None,
                unit_total: None,
            },
        );
    })
}

/// Whether the project is in git and what it can be compared with.
async fn git(State(s): State<Shared>) -> Result<Json<crate::api::GitInfo>, ApiError> {
    blocking(move || s.backend.git()).await.map(Json)
}

/// A file of the project as it is at a ref.
async fn git_file(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::FileContent>, ApiError> {
    let (rev, path) = (param(&q, "ref")?, param(&q, "path")?);
    json(move || s.backend.git_file(&rev, &path)).await
}

/// `?ref=<rev>` and, for one board, `&board=<name>`, and, to compare with another ref instead of
/// the files on disk, `&against=<rev>`. The request is answered when the comparison is done; while it runs the socket carries `progress` messages with the task
/// `compare` and the ref as their board.
async fn compare(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::Compare>, ApiError> {
    let rev = param(&q, "ref")?;
    let board = q.get("board").cloned().filter(|b| !b.is_empty());
    let against = q.get("against").cloned().filter(|b| !b.is_empty());
    let observe = job_observer(&s, "compare", &rev);
    // A page that goes away (the connection drops, and with it this future) takes the compare
    // with it: nobody is left to read the answer, and it would hold the next compare up.
    let (registered, token) = s.jobs.start("compare");
    let disconnect = CancelIfDropped(Some(token.clone()));
    let shared = Arc::clone(&s);
    let asked = rev.clone();
    let result = blocking(move || {
        let _registered = registered;
        s.backend
            .compare_against(&rev, against.as_deref(), board.as_deref(), &observe, &token)
    })
    .await?;
    disconnect.answered();
    told_if_cancelled(&shared, "compare", &asked, &result);
    result.map(Json)
}

/// The events of an analysis that a job runs for itself, as `progress` messages of `task`
/// about `key`. Only the progress is passed on: the page that asked knows when it ends.
fn job_observer(s: &Shared, task: &str, key: &str) -> Observer {
    let events = s.events.clone();
    let (task, key) = (task.to_string(), key.to_string());
    Arc::new(move |event| {
        if let AnalysisEvent::Progress {
            stage,
            done,
            total,
            message,
            work,
            ..
        } = event
        {
            send(
                &events,
                &ServerMessage::Progress {
                    task: task.clone(),
                    board: key.clone(),
                    message,
                    stage: Some(stage),
                    done: Some(done),
                    total: Some(total),
                    unit: work.as_ref().map(|w| w.unit.clone()),
                    unit_done: work.as_ref().map(|w| w.done),
                    unit_total: work.as_ref().and_then(|w| w.total),
                },
            );
        }
    })
}

/// `?dry=1` writes nothing, `?offline=1` asks no one (ADR 0039).
async fn refresh_catalogs(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::CatalogRefresh>, ApiError> {
    let say = progress(&s, "catalog", "");
    let flag = |name: &str| q.get(name).is_some_and(|v| v == "1");
    let (dry, offline) = (flag("dry"), flag("offline"));
    json(move || s.backend.refresh_catalogs(dry, offline, &say)).await
}

async fn pcb_scene(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PcbScene>, ApiError> {
    let board = param(&q, "board")?;
    let say = progress(&s, "place", &board);
    let show = live(&s);
    json(move || s.backend.pcb_scene_watched(&board, &say, &show)).await
}

async fn pcb_pours(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PcbPours>, ApiError> {
    let board = param(&q, "board")?;
    json(move || s.backend.pcb_pours(&board)).await
}

/// The header that carries the layout stamp of the placement a `.glb` was drawn from.
pub const LAYOUT_STAMP_HEADER: &str = "x-netsmith-layout-stamp";

/// The board's 3D scene as glTF 2.0 binary (ADR 0105), with the layout stamp of its placement
/// in [`LAYOUT_STAMP_HEADER`].
async fn pcb_3d(State(s): State<Shared>, Query(q): Query1) -> Result<Response, ApiError> {
    let board = param(&q, "board")?;
    let scene = blocking(move || s.backend.pcb_3d(&board)).await??;
    let mut response = ([(header::CONTENT_TYPE, "model/gltf-binary")], scene.glb).into_response();
    if let Ok(stamp) = HeaderValue::from_str(&scene.layout_stamp) {
        response.headers_mut().insert(LAYOUT_STAMP_HEADER, stamp);
    }
    Ok(response)
}

/// The placed board, not routed, as a zip of KiCad files to download (issue #1001).
async fn pcb_kicad(State(s): State<Shared>, Query(q): Query1) -> Result<Response, ApiError> {
    let board = param(&q, "board")?;
    let attachment = blocking(move || s.backend.pcb_kicad(&board)).await??;
    Ok(attached(attachment))
}

async fn pcb_findings(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PcbFindings>, ApiError> {
    let board = param(&q, "board")?;
    json(move || s.backend.pcb_findings(&board)).await
}

/// The board's copper areas, from the source and the routes file (issue #1085).
async fn pcb_copper(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PcbCopper>, ApiError> {
    let board = param(&q, "board")?;
    json(move || s.backend.pcb_copper(&board)).await
}

/// How the fill of the board's pours joins its pads (issue #1187).
async fn pcb_fill(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PcbFill>, ApiError> {
    let board = param(&q, "board")?;
    json(move || s.backend.pcb_fill(&board)).await
}

/// The stitching vias the export will have that the routes file does not (issue #1489).
async fn pcb_stitching(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PcbStitching>, ApiError> {
    let board = param(&q, "board")?;
    json(move || s.backend.pcb_stitching(&board)).await
}

/// The rule areas of the board, less the keepouts of the physical file (issue 1543).
async fn pcb_rule_areas(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PcbRuleAreas>, ApiError> {
    let board = param(&q, "board")?;
    json(move || s.backend.pcb_rule_areas(&board)).await
}

/// The table of the board's nets and diff pairs (issue #1223).
async fn pcb_nets(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PcbNets>, ApiError> {
    let board = param(&q, "board")?;
    json(move || s.backend.pcb_nets(&board)).await
}

/// How far a `near` part's neighbours are listed when the request has no `within`, in mm
/// (`layout summary --within`'s default).
const SUMMARY_WITHIN: f64 = 2.0;

async fn pcb_summary(
    State(s): State<Shared>,
    Query(q): Query<Vec<(String, String)>>,
) -> Result<Json<crate::api::PcbSummary>, ApiError> {
    let mut board = None;
    let mut near = Vec::new();
    let mut within = SUMMARY_WITHIN;
    let mut between = Vec::new();
    for (key, value) in q {
        match key.as_str() {
            "board" => board = Some(value),
            "near" => near.push(value),
            "within" => {
                within = value
                    .parse::<f64>()
                    .ok()
                    .filter(|w| w.is_finite() && *w >= 0.0)
                    .ok_or_else(|| {
                        ApiError::bad_request(format!(
                            "`within` is a distance in mm, 0 or more, not `{value}`"
                        ))
                    })?;
            }
            "between" => match value.split_once(',') {
                Some((a, b)) if !a.is_empty() && !b.is_empty() && !b.contains(',') => {
                    between.push((a.to_string(), b.to_string()));
                }
                _ => {
                    return Err(ApiError::bad_request(format!(
                        "`between` is two pads, `PART.PAD,PART.PAD`, not `{value}`"
                    )));
                }
            },
            _ => {}
        }
    }
    let board =
        board.ok_or_else(|| ApiError::bad_request("the request needs `board`".to_string()))?;
    json(move || s.backend.pcb_summary(&board, &near, within, &between)).await
}

/// The grid of `GET /api/pcb/fit` unless it has a `step`, in mm (`layout fit --step`'s default).
const FIT_STEP: f64 = 0.5;

async fn pcb_fit(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PcbFit>, ApiError> {
    let board = param(&q, "board")?;
    let part = param(&q, "part")?;
    let near = q.get("near").cloned();
    let step = match q.get("step") {
        None => FIT_STEP,
        Some(value) => value
            .parse::<f64>()
            .ok()
            .filter(|w| w.is_finite() && *w > 0.0)
            .ok_or_else(|| {
                ApiError::bad_request(format!("`step` is a distance in mm above 0, not `{value}`"))
            })?,
    };
    let side = match q.get("side").map(String::as_str) {
        None => None,
        Some(side @ ("top" | "bottom")) => Some(side.to_string()),
        Some(other) => {
            return Err(ApiError::bad_request(format!(
                "`side` is `top` or `bottom`, not `{other}`"
            )));
        }
    };
    json(move || {
        s.backend
            .pcb_fit(&board, &part, near.as_deref(), step, side.as_deref())
    })
    .await
}

async fn pcb_drc(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PcbFindings>, ApiError> {
    let board = param(&q, "board")?;
    let say = progress(&s, "drc", &board);
    json(move || s.backend.pcb_drc(&board, &say)).await
}

async fn pin_lock(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<crate::api::PinWritten>, ApiError> {
    let edit: crate::api::PinLock = body(&bytes)?;
    json(move || s.backend.pin_lock(&edit)).await
}

async fn pin_move(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<crate::api::PinWritten>, ApiError> {
    let edit: crate::api::PinMove = body(&bytes)?;
    json(move || s.backend.pin_move(&edit)).await
}

async fn pcb_edit(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<crate::api::PcbEdited>, ApiError> {
    let edit: crate::api::PcbEdit = body(&bytes)?;
    json(move || s.backend.pcb_edit(&edit)).await
}

async fn pcb_regenerate(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<crate::api::PcbRegenerated>, ApiError> {
    let ask: crate::api::PcbRegenerate = body(&bytes)?;
    let say = progress(&s, "regenerate", &ask.board);
    json(move || s.backend.pcb_regenerate(&ask, &say)).await
}

async fn pcb_routes(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<crate::api::PcbRoutesEdited>, ApiError> {
    let edit: crate::api::PcbRoutesEdit = body(&bytes)?;
    json(move || s.backend.pcb_routes(&edit)).await
}

async fn pcb_walk(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<crate::api::PcbWalked>, ApiError> {
    let walk: crate::api::PcbWalk = body(&bytes)?;
    json(move || s.backend.pcb_walk(&walk)).await
}

async fn pcb_shove(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<crate::api::PcbShoveAnswer>, ApiError> {
    let shove: crate::api::PcbShove = body(&bytes)?;
    json(move || s.backend.pcb_shove(&shove)).await
}

async fn pcb_reroute(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<crate::api::PcbRerouted>, ApiError> {
    let walk: crate::api::PcbWalk = body(&bytes)?;
    json(move || s.backend.pcb_reroute(&walk)).await
}

async fn build(State(s): State<Shared>) -> Result<Json<crate::api::BuildReport>, ApiError> {
    let say = progress(&s, "build", "");
    let (registered, token) = s.jobs.start("build");
    let events = Arc::clone(&s);
    let result = blocking(move || {
        let _registered = registered;
        s.backend.build(&say, &token)
    })
    .await?;
    told_if_cancelled(&events, "build", "", &result);
    result.map(Json)
}

async fn fab(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::FabReport>, ApiError> {
    let board = param(&q, "board")?;
    let say = progress(&s, "fab", &board);
    let allow = q.get("allow").is_some_and(|a| a == "1");
    let (registered, token) = s.jobs.start("fab");
    let shared = Arc::clone(&s);
    let asked = board.clone();
    let result = blocking(move || {
        let _registered = registered;
        s.backend.fab(&board, allow, &say, &token)
    })
    .await?;
    told_if_cancelled(&shared, "fab", &asked, &result);
    result.map(Json)
}

async fn orders(State(s): State<Shared>) -> Result<Json<Vec<crate::api::OrderInfo>>, ApiError> {
    blocking(move || s.backend.orders()).await.map(Json)
}

async fn fab_order(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::OrderReport>, ApiError> {
    let name = param(&q, "name")?;
    let say = progress(&s, "order", &name);
    let allow = q.get("allow").is_some_and(|a| a == "1");
    let (registered, token) = s.jobs.start("order");
    let shared = Arc::clone(&s);
    let asked = name.clone();
    let result = blocking(move || {
        let _registered = registered;
        s.backend.fab_order(&name, allow, &say, &token)
    })
    .await?;
    told_if_cancelled(&shared, "order", &asked, &result);
    result.map(Json)
}

async fn order_download(State(s): State<Shared>, Query(q): Query1) -> Result<Response, ApiError> {
    let (name, file) = (param(&q, "name")?, param(&q, "file")?);
    let attachment = blocking(move || s.backend.order_download(&name, &file)).await??;
    Ok(attached(attachment))
}

async fn download(State(s): State<Shared>, Query(q): Query1) -> Result<Response, ApiError> {
    let (board, file) = (param(&q, "board")?, param(&q, "file")?);
    let attachment = blocking(move || s.backend.download(&board, &file)).await??;
    Ok(attached(attachment))
}

/// A view of a fab package (issue #1107): the one `fab` wrote for `board`, or the one the page
/// opened, by its name.
async fn gerber(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::GerberView>, ApiError> {
    let layer = q.get("layer").cloned();
    match (q.get("board").cloned(), q.get("opened").cloned()) {
        (Some(board), None) => json(move || s.backend.gerber(&board, layer.as_deref())).await,
        (None, Some(name)) => json(move || s.backend.gerber_opened(&name, layer.as_deref())).await,
        _ => Err(ApiError::bad_request(
            "the request needs `board` or `opened`, not both",
        )),
    }
}

/// A package the page sends to the Gerber view: its bytes, named by `name`.
async fn gerber_open(
    State(s): State<Shared>,
    Query(q): Query1,
    bytes: axum::body::Bytes,
) -> Result<Json<crate::api::GerberView>, ApiError> {
    let name = param(&q, "name")?;
    let layer = q.get("layer").cloned();
    json(move || {
        s.backend
            .gerber_open(&name, bytes.to_vec(), layer.as_deref())
    })
    .await
}

/// A file as a download.
fn attached(attachment: crate::backend::Attachment) -> Response {
    let disposition = format!(
        "attachment; filename=\"{}\"",
        attachment.name.replace('"', "")
    );
    (
        [
            (header::CONTENT_TYPE, attachment.content_type),
            (header::CONTENT_DISPOSITION, disposition),
        ],
        attachment.bytes,
    )
        .into_response()
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct DefinitionRequest {
    path: String,
    text: String,
    offset: usize,
}

async fn definition(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<Option<Definition>>, ApiError> {
    let request: DefinitionRequest = body(&bytes)?;
    json(move || {
        s.backend
            .definition(&request.path, &request.text, request.offset)
    })
    .await
}

#[derive(serde::Deserialize)]
struct FormatRequest {
    text: String,
}

/// The text of a source in the layout `netsmith project fmt` writes (issue #1103), for the page's
/// _Format the file_ and its format on save. Nothing is written: the page applies it as an edit.
async fn format_source(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<String>, ApiError> {
    let request: FormatRequest = body(&bytes)?;
    json(move || s.backend.format(&request.text)).await
}

/// The web app: the file asked for, or the page itself.
async fn asset(request: Request<Body>) -> Response {
    let path = request.uri().path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match assets::find(path) {
        // The placeholder of a binary built without Node: a page that says so, with a 503.
        Some((mime, bytes)) if path == "index.html" && !assets::bundle_is_present() => (
            StatusCode::SERVICE_UNAVAILABLE,
            [
                (header::CONTENT_TYPE, HeaderValue::from_static(mime)),
                (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            ],
            bytes,
        )
            .into_response(),
        Some((mime, bytes)) => (
            [
                (header::CONTENT_TYPE, HeaderValue::from_static(mime)),
                (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            ],
            bytes,
        )
            .into_response(),
        None => ApiError::not_found("not found").into_response(),
    }
}

/// Tells every socket when the files on disk changed since `baseline`, as the backend
/// notices. The baseline comes from the caller, not from the first poll: a change made
/// before the first poll runs (a loaded machine, a page that connects at once) would
/// otherwise become the baseline and never be announced.
async fn watch(state: Shared, baseline: u64) {
    let mut seen = baseline;
    loop {
        let backend = Arc::clone(&state.backend);
        if let Ok(analysis) = tokio::task::spawn_blocking(move || backend.analysis()).await {
            if seen != analysis.version {
                send(
                    &state.events,
                    &ServerMessage::Rebuilt {
                        elapsed_ms: analysis.elapsed_ms,
                        analysis: analysis.clone(),
                    },
                );
            }
            seen = analysis.version;
        }
        tokio::time::sleep(WATCH_INTERVAL).await;
    }
}

/// What an agent is told when no page is open.
fn no_page(state: &Shared) -> ApiError {
    let port = state.hosts.first().map_or(String::new(), |h| {
        h.rsplit(':').next().unwrap_or("").to_string()
    });
    ApiError::conflict(format!(
        "no editor page is open: open http://127.0.0.1:{port} in a browser, then ask again (the 3D view is driven in the page, which draws it)"
    ))
}

/// Ask the page focused last, and wait up to `wait` for its answer.
async fn ask_page(
    state: &Shared,
    make: impl FnOnce(u32) -> ServerMessage,
    wait: Duration,
) -> Result<PageAnswer, ApiError> {
    let Some((ask, answered)) = state.pages.ask(make) else {
        return Err(no_page(state));
    };
    match tokio::time::timeout(wait, answered).await {
        Ok(Ok(PageAnswer::Error { status, message })) => Err(ApiError::new(status, message)),
        Ok(Ok(answer)) => Ok(answer),
        Ok(Err(_)) => Err(ApiError::conflict(
            "the editor page closed before it answered: open it again, then ask again",
        )),
        Err(_) => {
            state.pages.forget(ask);
            Err(ApiError::new(
                504,
                format!(
                    "the editor page did not answer within {} s: it may be busy (a large board loads for a while), hidden in a background tab, or a page from before this server (reload it)",
                    wait.as_secs()
                ),
            ))
        }
    }
}

fn view_answer(answer: PageAnswer) -> Result<Response, ApiError> {
    match answer {
        PageAnswer::State(view) => Ok(Json(*view).into_response()),
        _ => Err(ApiError::internal("the page answered with something else")),
    }
}

/// `GET /api/view`: the 3D view of the page focused last.
async fn view_state(State(s): State<Shared>) -> Result<Response, ApiError> {
    let answer = ask_page(
        &s,
        |id| ServerMessage::View { id, change: None },
        VIEW_READ_WAIT,
    )
    .await?;
    view_answer(answer)
}

/// `POST /api/view`: change the 3D view of the page focused last, and answer with what it
/// shows once it has drawn the change.
async fn view_change(
    State(s): State<Shared>,
    body: axum::body::Bytes,
) -> Result<Response, ApiError> {
    let change: ViewChange = if body.iter().all(u8::is_ascii_whitespace) {
        ViewChange::default()
    } else {
        serde_json::from_slice(&body)
            .map_err(|e| ApiError::bad_request(format!("the change does not read: {e}")))?
    };
    if change.theme.is_some() {
        return Err(ApiError::bad_request(
            "`theme` cannot be changed here: the editor's theme is the person's own setting for the whole page, not a view of the board. `GET /api/view` says which it is (light or dark); `netsmith view 3d --theme` draws a picture in either",
        ));
    }
    if change.ortho.is_some() && change.perspective.is_some() && change.ortho == change.perspective
    {
        return Err(ApiError::bad_request(
            "`ortho` and `perspective` say opposite things: give one",
        ));
    }
    if change.view.is_some() && change.camera.is_some() {
        return Err(ApiError::bad_request(
            "`view` and `camera` both place the camera: give one",
        ));
    }
    if change.highlight.is_some() && change.select.is_some() {
        return Err(ApiError::bad_request(
            "`highlight` and `select` both set the selection: give one (`highlight` also lights limits)",
        ));
    }
    let answer = ask_page(
        &s,
        |id| ServerMessage::View {
            id,
            change: Some(change),
        },
        VIEW_CHANGE_WAIT,
    )
    .await?;
    view_answer(answer)
}

/// `GET /api/view/picture`: a PNG of the 3D canvas of the page focused last, drawn for the ask.
async fn view_picture(State(s): State<Shared>) -> Result<Response, ApiError> {
    match ask_page(&s, |id| ServerMessage::ViewPicture { id }, VIEW_READ_WAIT).await? {
        PageAnswer::Picture(png) => Ok((
            [
                (header::CONTENT_TYPE, "image/png"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            png,
        )
            .into_response()),
        _ => Err(ApiError::internal("the page answered with something else")),
    }
}

fn send(events: &broadcast::Sender<String>, message: &ServerMessage) {
    if let Ok(text) = serde_json::to_string(message) {
        // Nobody listening is not a failure.
        let _ = events.send(text);
    }
}

/// What an analysis tells the server, as the page hears it, and remembered for the page
/// that connects while it runs. The state is updated before the message is sent, so a socket
/// that subscribes either sees the state or gets the message (or both).
fn observer(events: &broadcast::Sender<String>, running: &Live) -> Observer {
    let (events, running) = (events.clone(), Arc::clone(running));
    Arc::new(move |event| {
        let mut live = running.lock().unwrap_or_else(|e| e.into_inner());
        match event {
            AnalysisEvent::Started { since } => {
                *live = Some(Running { since, last: None });
                send(&events, &ServerMessage::Analysing { since });
            }
            AnalysisEvent::Progress {
                stage,
                board,
                done,
                total,
                message,
                work,
            } => {
                let message = ServerMessage::Progress {
                    task: "analyse".to_string(),
                    board,
                    message,
                    stage: Some(stage),
                    done: Some(done),
                    total: Some(total),
                    unit: work.as_ref().map(|w| w.unit.clone()),
                    unit_done: work.as_ref().map(|w| w.done),
                    unit_total: work.as_ref().and_then(|w| w.total),
                };
                if let Some(run) = live.as_mut() {
                    run.last = Some(message.clone());
                }
                send(&events, &message);
            }
            AnalysisEvent::Failed { message } => send(
                &events,
                &ServerMessage::Progress {
                    task: "analyse".to_string(),
                    board: String::new(),
                    message: format!("the analysis stopped on an internal error: {message}"),
                    stage: None,
                    done: None,
                    total: None,
                    unit: None,
                    unit_done: None,
                    unit_total: None,
                },
            ),
            AnalysisEvent::Cancelled => send(
                &events,
                &ServerMessage::Cancelled {
                    task: "analyse".to_string(),
                    board: String::new(),
                    message: "the analysis was cancelled".to_string(),
                },
            ),
            AnalysisEvent::Finished => *live = None,
        }
    })
}

async fn events(State(s): State<Shared>, upgrade: WebSocketUpgrade) -> Response {
    // Subscribe before the handshake is answered, not when the upgraded task first runs:
    // a page that has its answer may see a file change at once, and a push sent before
    // the subscription would reach no one.
    let pushes = s.events.subscribe();
    // An analysis that is running now: the page that connects in the middle of it is told
    // so, and how far it has got, as the page that was there from the start was.
    let running = running_messages(&s.running);
    // A page's seat among the open pages, also before the handshake is answered: an agent that
    // asks the moment the page is up finds it.
    let seat = s.pages.open();
    upgrade
        .max_frame_size(MAX_FRAME)
        .max_message_size(MAX_FRAME)
        .on_upgrade(move |socket| conversation(s, socket, pushes, running, seat))
}

/// What a page that lost messages needs to hear to be as one that lost none: the analysis that
/// is running, as a page that connects in the middle of it is told, or else a `rebuilt` of the
/// current files.
async fn current_state(state: &Shared) -> Vec<String> {
    let running = running_messages(&state.running);
    if !running.is_empty() {
        return running;
    }
    let backend = Arc::clone(&state.backend);
    let Ok(analysis) = tokio::task::spawn_blocking(move || backend.analysis()).await else {
        return Vec::new();
    };
    serde_json::to_string(&ServerMessage::Rebuilt {
        elapsed_ms: analysis.elapsed_ms,
        analysis,
    })
    .into_iter()
    .collect()
}

/// The analysis that is running now, as `analysing` and the last `progress`, as text.
fn running_messages(running: &Live) -> Vec<String> {
    let live = running.lock().unwrap_or_else(|e| e.into_inner());
    live.iter()
        .flat_map(|run| {
            std::iter::once(ServerMessage::Analysing { since: run.since }).chain(run.last.clone())
        })
        .filter_map(|m| serde_json::to_string(&m).ok())
        .collect()
}

/// What a check that could not be made answers with: no boards and one error.
fn failed_check(message: &str) -> crate::api::Analysis {
    use crate::api::{Analysis, Problem, ProblemStage, Severity};
    Analysis {
        version: 0,
        errors: 1,
        warnings: 0,
        notes: 0,
        problems: vec![Problem {
            severity: Severity::Error,
            message: message.to_string(),
            rendered: format!("error: {message}\n"),
            at: None,
            stage: Some(ProblemStage::Check),
            board: None,
            path: None,
            cause: None,
        }],
        boards: Vec::new(),
        systems: Vec::new(),
        panels: Vec::new(),
        module_uses: None,
        elapsed_ms: None,
        cancelled: None,
        stale: None,
    }
}

/// Ends the task that answers a socket's checks when the socket is gone.
struct EndsWithSocket(tokio::task::JoinHandle<()>);

impl Drop for EndsWithSocket {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// One page's socket: what the server pushes, and the checks the page asks for. A check
/// waits for the analysis of the files on disk (if one is running), so it runs on a task of
/// its own, one after the other: the socket goes on carrying `progress` meanwhile.
async fn conversation(
    state: Shared,
    mut socket: WebSocket,
    mut events: broadcast::Receiver<String>,
    running: Vec<String>,
    (seat, mut told): (crate::pages::Seat, mpsc::UnboundedReceiver<String>),
) {
    for text in running {
        if socket.send(Message::Text(text.into())).await.is_err() {
            return;
        }
    }
    let (asks, mut queue) = mpsc::unbounded_channel::<(u32, String, String)>();
    let (answers, mut replies) = mpsc::unbounded_channel::<String>();
    let backend = Arc::clone(&state.backend);
    let _worker = EndsWithSocket(tokio::spawn(async move {
        while let Some((id, path, text)) = queue.recv().await {
            let backend = Arc::clone(&backend);
            let checked = tokio::task::spawn_blocking(move || backend.check(&path, &text)).await;
            // Every check is answered: a refusal or a panic is a `checked` whose one problem
            // says so, so that the page does not wait for its id for ever.
            let analysis = match checked {
                Ok(Ok(analysis)) => analysis,
                Ok(Err(e)) => failed_check(&e.message),
                Err(e) => failed_check(&format!("the check stopped on an internal error: {e}")),
            };
            if let Ok(reply) = serde_json::to_string(&ServerMessage::Checked { id, analysis }) {
                let _ = answers.send(reply);
            }
        }
    }));
    loop {
        tokio::select! {
            pushed = events.recv() => match pushed {
                Ok(text) => {
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        return;
                    }
                }
                // A page that fell behind lost messages, and a lost `rebuilt` would not come
                // again: it is told where things are now.
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    for text in current_state(&state).await {
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            return;
                        }
                    }
                }
                Err(broadcast::error::RecvError::Closed) => return,
            },
            Some(reply) = replies.recv() => {
                if socket.send(Message::Text(reply.into())).await.is_err() {
                    return;
                }
            }
            Some(text) = told.recv() => {
                if socket.send(Message::Text(text.into())).await.is_err() {
                    return;
                }
            }
            received = socket.recv() => {
                let Some(Ok(message)) = received else { return };
                let text = match message {
                    Message::Text(text) => text,
                    // A picture: the ask's id, 4 bytes little-endian, then the PNG.
                    Message::Binary(bytes) => {
                        if let Some((id, png)) = bytes.split_first_chunk::<4>() {
                            let id = u32::from_le_bytes(*id);
                            state.pages.answer(seat.id, id, PageAnswer::Picture(png.to_vec()));
                        }
                        continue;
                    }
                    _ => continue,
                };
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(ClientMessage::Check { id, path, text }) => {
                        let _ = asks.send((id, path, text));
                    }
                    Ok(ClientMessage::Focus) => state.pages.focus(seat.id),
                    Ok(ClientMessage::ViewState { id, state: view }) => {
                        state.pages.answer(seat.id, id, PageAnswer::State(view));
                    }
                    Ok(ClientMessage::ViewError { id, message, status }) => state.pages.answer(
                        seat.id,
                        id,
                        PageAnswer::Error {
                            status: status.unwrap_or(400),
                            message,
                        },
                    ),
                    Err(_) => {}
                }
            }
        }
    }
}
