use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_fabricate::package_check::{self, Context};
use netsmith_fabricate::project::Project;

use super::common::{Format, print_json};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`catalog check` compares each catalog row's package with the footprint it names (issue #1658): the footprint of the `kicad.toml` row of the part's kind and that package (the project's mapping, then the standard one), for each kind whose definition's `catalog` statement names the catalog (every footprint the mapping gives the package, for a catalog no definition names yet). Each footprint is measured against the known size of the package: its count of pads, the body it draws on its fabrication layer (`F.Fab`, short side and long side, within 0.1 mm of the package's range) and a courtyard at least the package's smallest body. A mismatch names the row, both numbers and the drawing the package's numbers are from. It asks no supplier and changes no file.\n\nThe packages netsmith knows the size of, each from a maker's package drawing (`data/jlcpcb/README.md` lists them): the chips 0201, 0402, 0603, 0805, 1206 and 1210, SOD-123, SOD-323, SMA, SMB, SMC, SOT-23, SOT-23-5, SOT-23-6, TSOT-23-5, SOT-323, SOT-353, SOT-363 and SOIC-8, with the other names a supplier writes for some (SC-70, DO-214AC). A row of another package is listed as not measured.\n\nWith no path it checks the project's catalog files (its `catalog_dirs` folders and `catalogs` list) and the built-in catalogs inside the binary; a path is a catalog file or a folder of them. The description a supplier's record gives is compared with the package by `catalog add`, which reads the record.\n\nThe exit code is 0 when every footprint measured is its row's package, 1 when one is not, and 2 when the check could not be made."
)]
pub struct Args {
    /// Catalog files, or folders of them; none is the project's catalogs and the built-in ones.
    #[arg(value_name = "CATALOG.CSV|DIR")]
    pub paths: Vec<PathBuf>,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` prints one JSON document on stdout and nothing else there: `command` (`catalog check`), `ok` (false when a row's footprint is not its package, as the exit code 1 says), `rows` (the rows with a package), `measured`, the `mismatches` (each its `file`, `line`, `catalog`, `mpn`, `package` and `problems`, each problem naming both numbers and the source), `not_known` (the packages netsmith knows no size of), `no_footprint` (`catalog` and `package` that no `kicad.toml` row gives a footprint), `unread` (`footprint` and `why`, a footprint that could not be read) and `known` (the packages netsmith knows the size of).
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}

/// The catalog files a path names: the file, or the `*.csv` files of a folder, in name order.
fn files_at(path: &Path, cwd: &Path) -> Result<Vec<(String, String, String)>, String> {
    let shown = |p: &Path| {
        p.strip_prefix(cwd)
            .unwrap_or(p)
            .display()
            .to_string()
            .replace('\\', "/")
    };
    let mut paths = Vec::new();
    if path.is_dir() {
        for entry in std::fs::read_dir(path).map_err(|e| format!("{}: {e}", shown(path)))? {
            let p = entry.map_err(|e| e.to_string())?.path();
            if p.extension().is_some_and(|x| x == "csv") {
                paths.push(p);
            }
        }
        paths.sort();
    } else if path.is_file() {
        paths.push(path.to_path_buf());
    } else {
        return Err(format!("{}: no such file or directory", shown(path)));
    }
    paths
        .into_iter()
        .map(|p| {
            let text = std::fs::read_to_string(&p).map_err(|e| format!("{}: {e}", shown(&p)))?;
            let name = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            Ok((name, shown(&p), text))
        })
        .collect()
}

pub fn run(args: Args) -> ExitCode {
    let fail = |e: String| {
        eprintln!("error: {e}");
        ExitCode::from(2)
    };
    let cwd = match env::current_dir() {
        Ok(d) => d,
        Err(e) => return fail(e.to_string()),
    };
    let project = match Project::find(&cwd) {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    let context = match Context::of(project.as_ref()) {
        Ok(c) => c,
        Err(e) => return fail(e),
    };
    let mut files = Vec::new();
    if args.paths.is_empty() {
        if let Some(project) = &project {
            let catalog_files = match project.catalog_files() {
                Ok(f) => f,
                Err(e) => return fail(e),
            };
            for file in catalog_files {
                match std::fs::read_to_string(&file.path) {
                    Ok(text) => files.push((file.name, file.file, text)),
                    Err(e) => return fail(format!("{}: {e}", file.file)),
                }
            }
        }
        for (name, shown, text) in netsmith_solve::built_in_files() {
            files.push((name.to_string(), shown.to_string(), text.to_string()));
        }
    } else {
        for path in &args.paths {
            match files_at(&cwd.join(path), &cwd) {
                Ok(found) => files.extend(found),
                Err(e) => return fail(e),
            }
        }
    }
    let report = match package_check::check_files(&context, &files) {
        Ok(r) => r,
        Err(e) => return fail(e),
    };
    if args.format == Format::Json {
        print_json(&package_check::json(&report));
    } else {
        print!("{}", package_check::render(&report));
    }
    if report.mismatches().next().is_some() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
