//! Builds the editor's page by hand (ADR 0114): `build` builds it from `web/` (Node 20 or newer,
//! `npm ci` and `npm run build`) into `web/dist`, which is not committed, and `hash` prints the
//! hash of the sources. `cargo build` does the same by itself when Node is there. Run them through
//! `scripts/web-bundle`.

#[allow(dead_code)]
#[path = "../build_web.rs"]
mod build_web;

use std::{
    env,
    path::{Path, PathBuf},
    process::ExitCode,
};

fn run(command: &str, web: &Path) -> Result<(), String> {
    match command {
        "hash" => {
            println!("{}", build_web::source_hash(web));
            Ok(())
        }
        "build" => {
            let dirs = build_web::search_dirs(env::var_os("PATH"), env::var_os("HOME"));
            build_web::ensure(web, &dirs, build_web::LOCK_WAIT)?;
            println!(
                "{} is the page built from the sources (hash {})",
                web.join("dist").display(),
                build_web::source_hash(web)
            );
            Ok(())
        }
        _ => Err("usage: web_bundle build | hash".into()),
    }
}

fn main() -> ExitCode {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let command = env::args().nth(1).unwrap_or_default();
    match run(&command, &manifest.join("web")) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
