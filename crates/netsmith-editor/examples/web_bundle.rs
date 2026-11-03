//! The committed page bundle (`crates/netsmith-editor/web-dist`): `update` builds the page from
//! `web/` afresh (Node 20 or newer, `npm ci` and `npm run build`) and replaces the committed
//! bundle with it; `check` builds it afresh and fails, naming the files, when the committed
//! bundle is not what the sources build to; `hash` prints the hash of the sources. Run them
//! through `scripts/web-bundle`.

#[allow(dead_code)]
#[path = "../build_web.rs"]
mod build_web;

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

/// Builds `web/dist` from the sources, ignoring any bundle that is already there.
fn rebuild(web: &Path) -> Result<(), String> {
    let _ = fs::remove_file(web.join("dist").join(build_web::DONE_MARKER));
    let dirs = build_web::search_dirs(env::var_os("PATH"), env::var_os("HOME"));
    build_web::ensure(web, &dirs, build_web::LOCK_WAIT).map(|_| ())
}

fn run(command: &str, web: &Path, committed: &Path) -> Result<(), String> {
    match command {
        "hash" => {
            println!("{}", build_web::source_hash(web));
            Ok(())
        }
        "update" => {
            rebuild(web)?;
            build_web::commit_bundle(web, committed)?;
            println!(
                "{} is the page built from the sources (hash {}); commit it",
                committed.display(),
                build_web::source_hash(web)
            );
            Ok(())
        }
        "check" => {
            rebuild(web)?;
            let hash = build_web::source_hash(web);
            let stamp = fs::read_to_string(committed.join(build_web::BUNDLE_STAMP))
                .map(|s| s.trim().to_string())
                .unwrap_or_default();
            let mut problems = Vec::new();
            if stamp != hash {
                problems.push(format!(
                    "{} says the bundle is of sources {stamp:?}, the sources are {hash}",
                    build_web::BUNDLE_STAMP
                ));
            }
            problems.extend(build_web::bundle_differences(&web.join("dist"), committed));
            if problems.is_empty() {
                println!("web-dist is what the sources build to (hash {hash})");
                Ok(())
            } else {
                Err(format!(
                    "{} is not what the sources build to:\n  {}\nrun scripts/web-bundle update and commit the result",
                    committed.display(),
                    problems.join("\n  ")
                ))
            }
        }
        _ => Err("usage: web_bundle update | check | hash".into()),
    }
}

fn main() -> ExitCode {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let command = env::args().nth(1).unwrap_or_default();
    match run(&command, &manifest.join("web"), &manifest.join("web-dist")) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
