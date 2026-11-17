//! `netsmith project fmt` (issue #1103): the project's sources, or the files named, in the one
//! layout of `netsmith_lang::format`. `--check` writes nothing and says which files would change.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_fabricate::project::Target;
use netsmith_lang::FormatError;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`fmt` rewrites `.nsm` files in the one layout of the language (the guide's chapter on comments, _The layout of a file_): 4 spaces of indent, 100 columns, one space between tokens, the rows of a pin table in columns, the imports sorted. It keeps every comment and the line breaks the file has, breaks a line that is too long, and changes nothing the file says: the formatted file parses to the same program, which it checks before it writes. Formatting a formatted file changes nothing. With no file it formats every file of the project: the top-level files, the modules under src/ and the physical files beside them.\n\nIt prints each file it changes (with `--check`, each file it would change). The exit code is 0 when it is done (with `--check`, when every file is formatted), 1 with `--check` when a file is not, and 2 when a file cannot be formatted (it does not parse) or read; the other files are formatted all the same."
)]
pub struct Args {
    /// The source files; with none, every file of the project.
    #[arg(value_name = "FILE.NSM")]
    pub files: Vec<String>,
    /// Write nothing: print the files that are not formatted, and exit 1 if there is one.
    #[arg(long)]
    pub check: bool,
}

pub fn run(args: Args) -> ExitCode {
    let files = match files(&args.files) {
        Ok(files) => files,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let mut failed = false;
    let mut unformatted = false;
    for (name, path) in &files {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("{name}: {e}");
                failed = true;
                continue;
            }
        };
        let formatted = match netsmith_lang::format(&text) {
            Ok(formatted) => formatted,
            Err(FormatError::Parse(e)) => {
                let (line, column) = e.line_col(&text);
                eprintln!(
                    "{name}:{line}:{column}: {}; a file that does not parse is not formatted",
                    e.message
                );
                failed = true;
                continue;
            }
            Err(e @ FormatError::Changed(_)) => {
                eprintln!("{name}: {e}");
                failed = true;
                continue;
            }
        };
        if formatted == text {
            continue;
        }
        unformatted = true;
        if args.check {
            println!("{name}");
            continue;
        }
        if let Err(e) = fs::write(path, formatted) {
            eprintln!("{name}: {e}");
            failed = true;
            continue;
        }
        println!("{name}");
    }
    if failed {
        ExitCode::from(2)
    } else if args.check && unformatted {
        eprintln!("fmt: the files above are not formatted; `netsmith project fmt` formats them");
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// The files to format, each with the name it is printed by: the ones named, or every file of the
/// project the current folder is in.
fn files(named: &[String]) -> Result<Vec<(String, PathBuf)>, String> {
    if !named.is_empty() {
        return Ok(named
            .iter()
            .map(|n| (n.clone(), PathBuf::from(n)))
            .collect());
    }
    let target = Target::resolve(None)?;
    let project = target
        .project
        .as_ref()
        .expect("a target with no file is a project's");
    let mut paths: BTreeSet<PathBuf> = BTreeSet::new();
    for file in &target.files {
        paths.insert(file.path.clone());
    }
    for module in project.modules()? {
        paths.insert(module.path);
    }
    let physicals: Vec<PathBuf> = paths.iter().filter_map(|p| physical_of(p)).collect();
    paths.extend(physicals);
    let cwd = std::env::current_dir().map_err(|e| format!("current directory: {e}"))?;
    Ok(paths
        .into_iter()
        .map(|path| {
            let name = path
                .strip_prefix(&cwd)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            (name, path)
        })
        .collect())
}

/// The physical file beside a source (`main.nsm` has `main.physical.nsm`), if there is one.
fn physical_of(source: &Path) -> Option<PathBuf> {
    let name = source.file_name()?.to_str()?;
    let stem = name.strip_suffix(".nsm")?;
    if stem.ends_with(".physical") {
        return None;
    }
    let physical = source.with_file_name(format!("{stem}.physical.nsm"));
    physical.is_file().then_some(physical)
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{Leaf, parse};

    #[test]
    fn fmt_takes_files_and_check() {
        let Ok(Leaf::Fmt(f)) = parse(&["project", "fmt", "a.nsm", "b.nsm", "--check"]) else {
            panic!()
        };
        assert_eq!(f.files, ["a.nsm", "b.nsm"]);
        assert!(f.check);
        let Ok(Leaf::Fmt(f)) = parse(&["project", "fmt"]) else {
            panic!()
        };
        assert!(f.files.is_empty());
        assert!(!f.check);
    }
}
