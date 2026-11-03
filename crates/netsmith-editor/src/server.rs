//! The HTTP server: routes, the guard against other web pages, and the socket that tells
//! the page when the files changed.

use std::{collections::HashMap, io, net::TcpListener, sync::Arc, time::Duration};

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
use tokio::sync::broadcast;

use crate::{
    api::{ClientMessage, Definition, ServerMessage},
    assets,
    backend::{ApiError, Backend},
};

/// How often the watcher asks the backend whether a file changed.
const WATCH_INTERVAL: Duration = Duration::from_millis(500);
/// A request body larger than this is not a source file.
const MAX_BODY: usize = 4 * 1024 * 1024;
/// The header a request that changes something must carry. A page on another origin cannot
/// send a custom header without a preflight, which this server never answers.
pub const GUARD_HEADER: &str = "x-netsmith";

struct AppState {
    backend: Arc<dyn Backend>,
    events: broadcast::Sender<String>,
    hosts: Vec<String>,
}

type Shared = Arc<AppState>;

/// A running server. Dropping it does not stop it; the process ends it.
pub struct Server {
    pub port: u16,
}

/// Serve `backend` on `listener` until the process ends. Blocks.
pub fn serve(listener: TcpListener, backend: Arc<dyn Backend>) -> io::Result<Server> {
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    let (events, _) = broadcast::channel(16);
    // What the files are now, taken before the first connection is accepted: a change
    // after this is a change the sockets must hear of, however late the watcher first runs.
    let baseline = backend.analysis().version;
    let state = Arc::new(AppState {
        backend,
        events,
        hosts: vec![format!("127.0.0.1:{port}"), format!("localhost:{port}")],
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
        .route("/api/file", get(read_file).put(write_file))
        .route("/api/board", get(board))
        .route("/api/schematic.svg", get(schematic))
        .route("/api/power", get(power))
        .route("/api/system", get(system))
        .route("/api/panel", get(panel))
        .route("/api/bom", get(bom))
        .route("/api/catalog/refresh", post(refresh_catalogs))
        .route("/api/pcb/route", post(route_pcb))
        .route("/api/pcb/cancel", post(cancel_route))
        .route("/api/pcb/scene", get(pcb_scene))
        .route("/api/pcb/edit", post(pcb_edit))
        .route("/api/pcb/findings", get(pcb_findings))
        .route("/api/pcb/drc", post(pcb_drc))
        .route("/api/build", post(build))
        .route("/api/fab", post(fab))
        .route("/api/download", get(download))
        .route("/api/orders", get(orders))
        .route("/api/order", post(fab_order))
        .route("/api/order/download", get(order_download))
        .route("/api/definition", post(definition))
        .route("/api/events", get(events))
        .fallback(get(asset))
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY))
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

fn refuse(message: &str) -> Response {
    ApiError::forbidden(message).into_response()
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (
            status,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!("{}\n", self.message),
        )
            .into_response()
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
    blocking(move || s.backend.project()).await.map(Json)
}

async fn analysis(State(s): State<Shared>) -> Result<Json<crate::api::Analysis>, ApiError> {
    blocking(move || s.backend.analysis()).await.map(Json)
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
            },
        );
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
    blocking(move || s.backend.build(&say)).await.map(Json)
}

async fn fab(
    State(s): State<Shared>,
    Query(q): Query1,
) -> Result<Json<crate::api::FabReport>, ApiError> {
    let board = param(&q, "board")?;
    let say = progress(&s, "fab", &board);
    let allow = q.get("allow").is_some_and(|a| a == "1");
    json(move || s.backend.fab(&board, allow, &say)).await
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
    json(move || s.backend.fab_order(&name, allow, &say)).await
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

async fn events(State(s): State<Shared>, upgrade: WebSocketUpgrade) -> Response {
    // Subscribe before the handshake is answered, not when the upgraded task first runs:
    // a page that has its answer may see a file change at once, and a push sent before
    // the subscription would reach no one.
    let pushes = s.events.subscribe();
    upgrade.on_upgrade(move |socket| conversation(s, socket, pushes))
}

/// One page's socket: what the server pushes, and the checks the page asks for.
async fn conversation(
    state: Shared,
    mut socket: WebSocket,
    mut events: broadcast::Receiver<String>,
) {
    loop {
        tokio::select! {
            pushed = events.recv() => match pushed {
                Ok(text) => {
                    if socket.send(Message::Text(text.into())).await.is_err() {
                        return;
                    }
                }
                // A page that fell behind fetches the analysis again when the next arrives.
                Err(broadcast::error::RecvError::Lagged(_)) => {}
                Err(broadcast::error::RecvError::Closed) => return,
            },
            received = socket.recv() => {
                let Some(Ok(message)) = received else { return };
                let Message::Text(text) = message else { continue };
                let Ok(ClientMessage::Check { id, path, text }) =
                    serde_json::from_str::<ClientMessage>(&text)
                else {
                    continue;
                };
                let backend = Arc::clone(&state.backend);
                let checked = tokio::task::spawn_blocking(move || backend.check(&path, &text)).await;
                if let Ok(Ok(analysis)) = checked {
                    let reply = ServerMessage::Checked { id, analysis };
                    if let Ok(text) = serde_json::to_string(&reply)
                        && socket.send(Message::Text(text.into())).await.is_err()
                    {
                        return;
                    }
                }
            }
        }
    }
}
