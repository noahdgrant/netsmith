//! The editor's server: JSON endpoints and a WebSocket over the pieces of netsmith that
//! know the language (ADR 0019). This crate knows nothing of the compiler. It holds the
//! types that cross to the page ([`api`]), the [`Backend`] a project implements, the HTTP
//! server that calls it, and the web app that is built from `web/` and embedded at compile
//! time.

pub mod api;
mod assets;
mod backend;
mod server;

pub use assets::{
    bundle_hash, bundle_is_present, find as find_asset, git_describe, missing_bundle_message,
};
pub use backend::{AnalysisEvent, ApiError, Attachment, Backend, ErrorDetail, Observer, Say};
pub use server::{PENDING_HEADER, Server, serve};
