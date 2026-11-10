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
//! | `GET /api/schematic-hints`, `PUT /api/schematic-hints` | the schematic layout file beside the board's source (ADR 0061): `422` with the line for text that does not read, `409` for a stale stamp |
//! | `GET /api/git`, `GET /api/compare`, `GET /api/git/file` | what the project can be compared with, what changed in the circuit, a file at a ref (ADR 0059) |
//! | `POST /api/pcb/route`, `cancel`, `edit`, `drc`; `GET /api/pcb/scene`, `findings` | place and route on demand; `cancel` stops the router and keeps what it routed |
//! | `POST /api/build`, `/api/fab`, `/api/order` and their `/cancel` | the commands of the same names; a cancelled job answers `499` and keeps nothing it was making (ADR 0062) |
//! | `GET /api/download`, `/api/order/download` | a file `fab` or an order wrote |
//! | `POST /api/definition` | where the name under the cursor is defined |
//! | WebSocket `/api/events` | `rebuilt`, `analysing`, `progress`, `cancelled` to the page; `check` from the page and `checked` back |
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
    api::{CancelAnswer, ClientMessage, Definition, Pending, ServerMessage},
    assets,
    backend::{AnalysisEvent, ApiError, Backend, Observer},
};
use netsmith_core::job::Cancel;

/// How often the watcher asks the backend whether a file changed.
const WATCH_INTERVAL: Duration = Duration::from_millis(500);
/// A request body larger than this is not a source file.
const MAX_BODY: usize = 4 * 1024 * 1024;
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
    "/api/schematic-hints",
    "/api/power",
    "/api/system",
    "/api/panel",
    "/api/bom",
    "/api/compare",
    "/api/catalog/refresh",
    "/api/pcb/route",
    "/api/pcb/scene",
    "/api/pcb/edit",
    "/api/pcb/findings",
    "/api/pcb/drc",
    "/api/build",
    "/api/fab",
    "/api/download",
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
        .route(
            "/api/schematic-hints",
            get(schematic_hints).put(write_schematic_hints),
        )
        .route("/api/power", get(power))
        .route("/api/system", get(system))
        .route("/api/panel", get(panel))
        .route("/api/bom", get(bom))
        .route("/api/git", get(git))
        .route("/api/git/file", get(git_file))
        .route("/api/compare", get(compare))
        .route("/api/compare/cancel", post(cancel_compare))
        .route("/api/catalog/refresh", post(refresh_catalogs))
        .route("/api/pcb/route", post(route_pcb))
        .route("/api/pcb/cancel", post(cancel_route))
        .route("/api/pcb/scene", get(pcb_scene))
        .route("/api/pcb/edit", post(pcb_edit))
        .route("/api/pcb/findings", get(pcb_findings))
        .route("/api/pcb/drc", post(pcb_drc))
        .route("/api/build", post(build))
        .route("/api/build/cancel", post(cancel_build))
        .route("/api/fab", post(fab))
        .route("/api/fab/cancel", post(cancel_fab))
        .route("/api/download", get(download))
        .route("/api/orders", get(orders))
        .route("/api/order", post(fab_order))
        .route("/api/order/cancel", post(cancel_order))
        .route("/api/order/download", get(order_download))
        .route("/api/definition", post(definition))
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
    blocking(move || {
        let mut project = s.backend.project();
        // What this binary knows of itself; the backend knows the rest.
        project.bundle_hash = crate::assets::bundle_hash().map(str::to_string);
        project.git_describe = project
            .git_describe
            .or_else(|| crate::assets::git_describe().map(str::to_string));
        project
    })
    .await
    .map(Json)
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

async fn route_pcb(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PcbReport>, ApiError> {
    let board = param(&q, "board")?;
    let say = progress(&s, "route", &board);
    json(move || s.backend.route(&board, &say)).await
}

/// Stops the route that is running, if one is.
async fn cancel_route(State(s): State<Shared>) -> StatusCode {
    s.backend.cancel_route();
    StatusCode::NO_CONTENT
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
    json(move || s.backend.pcb_scene(&board)).await
}

async fn pcb_findings(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PcbFindings>, ApiError> {
    let board = param(&q, "board")?;
    json(move || s.backend.pcb_findings(&board)).await
}

async fn pcb_drc(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::PcbFindings>, ApiError> {
    let board = param(&q, "board")?;
    let say = progress(&s, "drc", &board);
    json(move || s.backend.pcb_drc(&board, &say)).await
}

async fn pcb_edit(
    State(s): State<Shared>,
    bytes: axum::body::Bytes,
) -> Result<Json<crate::api::PcbEdited>, ApiError> {
    let edit: crate::api::PcbEdit = body(&bytes)?;
    json(move || s.backend.pcb_edit(&edit)).await
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

/// The web app: the file asked for, or the page itself.
async fn asset(request: Request<Body>) -> Response {
    let path = request.uri().path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };
    match assets::find(path) {
        Some((mime, bytes)) => (
            [
                (header::CONTENT_TYPE, HeaderValue::from_static(mime)),
                (header::CACHE_CONTROL, HeaderValue::from_static("no-store")),
            ],
            bytes,
        )
            .into_response(),
        None if path == "index.html" => {
            ApiError::new(503, assets::missing_bundle_message()).into_response()
        }
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
    upgrade.on_upgrade(move |socket| conversation(s, socket, pushes, running))
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
            received = socket.recv() => {
                let Some(Ok(message)) = received else { return };
                let Message::Text(text) = message else { continue };
                if let Ok(ClientMessage::Check { id, path, text }) =
                    serde_json::from_str::<ClientMessage>(&text)
                {
                    let _ = asks.send((id, path, text));
                }
            }
        }
    }
}
