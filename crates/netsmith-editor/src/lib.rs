//! The editor's server: JSON endpoints and a WebSocket over the pieces of netsmith that
//! know the language (ADR 0019). This crate knows nothing of the compiler, but for the token a
//! job is cancelled with ([`Cancel`], ADR 0062). It holds the
//! types that cross to the page ([`api`]), the [`Backend`] a project implements, the HTTP
//! server that calls it, and the web app that is built from `web/` and embedded at compile
//! time.
//!
//! **Why the crate does not know the compiler.** `netsmith-editor` holds the server, the types
//! and the page, and a [`Backend`] trait with a method for each thing the page asks. `netsmith`
//! implements it (`crates/netsmith/src/editor/`, a `Workspace` over a `Target`), because the work
//! is the pipeline that lives there (`build_with`, `fab_with`, `Project`), and a crate the
//! binary uses could not use the binary's library back. The split lets the server be tested with
//! no compiler behind it and keeps `axum` out of every crate but one. `netsmith-view` keeps its
//! layout and SVG drawing; the one-page viewer it once had went with `netsmith view`, which the
//! editor replaced (ADR 0001: one way to look at a design).
//!
//! **The language stays text** (ADR 0003): the editor writes `.nsm` files, the files beside
//! them that are the editor's own (the PCB's and the schematic's layout files), and never a
//! second copy of the design. The views are computed from the solved model, not from text: the
//! power tree from the rails as ADR 0007 and ADR 0014 define them, the system view from the
//! system's model, and the bill of materials from the same grouping `bom.csv` uses, so what
//! the page shows is what `fab` would write.
//!
//! **Later, not built.** A desktop shell (Tauri) around the same page is a later option: the
//! page already talks only to a base URL. Refreshing price and stock online is an opt-in
//! `netsmith catalog refresh` that writes dated data into `data/jlcpcb/`; the solver never needs
//! the network, and the editor shows the dates it has.
//!
//! **Limits.** One user, one browser: the server has no accounts and a second tab edits the same
//! files. Diagnostics and the views describe the whole project at the last check and are not
//! incremental, so a project of hundreds of parts feels the solve. The editor does not complete
//! names. `netsmith.toml` is shown and not edited, and is read once: after changing `top_level`,
//! restart. A `system`'s boards show their power trees together, not an end to end tree through
//! connectors (ADR 0010).

pub mod api;
mod assets;
mod backend;
mod pages;
mod server;

pub use assets::{
    bundle_hash, bundle_is_present, find as find_asset, git_describe, missing_bundle_message,
};
pub use backend::{
    AnalysisEvent, ApiError, Attachment, Backend, ErrorDetail, Observer, Pcb3d, Say, Work,
};
pub use netsmith_core::job::Cancel;
pub use server::{PENDING_HEADER, Server, serve, serve_with_backlog};
