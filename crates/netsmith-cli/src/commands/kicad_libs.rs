use std::fs;
use std::process::ExitCode;

use clap::{Args as ClapArgs, Subcommand};

#[derive(Debug, PartialEq, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Command {
    /// Print the [[library]] stanzas for kicad.toml from a KiCad project's library tables (ADR 0021).
    Libraries(LibrariesArgs),
    /// Print the [[model]] rows for netsmith.toml from the 3D models a footprint library table's libraries name (the model table, issue #1005).
    ///
    /// Each footprint's first `(model ...)` line that names a STEP file becomes a row with its offset and rotation; notes on stderr count the footprints left out (no model line, a model that is not STEP, a scaled model).
    Models(ModelsArgs),
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct ModelsArgs {
    /// The project's footprint library table; `${KIPRJMOD}` is its folder.
    #[arg(value_name = "FP-LIB-TABLE")]
    pub fp_lib_table: String,
    /// Read only the library of this name (repeat for several).
    #[arg(long = "library", value_name = "NAME")]
    pub libraries: Vec<String>,
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct LibrariesArgs {
    /// The project's symbol library table.
    #[arg(value_name = "SYM-LIB-TABLE")]
    pub sym_lib_table: String,
    /// The project's footprint library table.
    #[arg(value_name = "FP-LIB-TABLE")]
    pub fp_lib_table: Option<String>,
}

pub fn run(args: Args) -> ExitCode {
    match args.command {
        Command::Libraries(a) => run_kicad_libs_import(&a.sym_lib_table, a.fp_lib_table.as_deref()),
        Command::Models(a) => run_kicad_libs_models(&a.fp_lib_table, &a.libraries),
    }
}

fn run_kicad_libs_models(table: &str, only: &[String]) -> ExitCode {
    let base = std::path::Path::new(table)
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let rows = fs::read_to_string(table)
        .map_err(|e| format!("{table}: {e}"))
        .and_then(|text| netsmith_kicad::model_rows(&text, base, only));
    match rows {
        Ok(found) => {
            for note in &found.notes {
                eprintln!("note: {note}");
            }
            print!("{}", found.rows);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

fn run_kicad_libs_import(sym: &str, fp: Option<&str>) -> ExitCode {
    let read = |path: &str| fs::read_to_string(path).map_err(|e| format!("{path}: {e}"));
    let stanzas = read(sym).and_then(|sym_text| {
        let fp_text = fp.map(read).transpose()?;
        netsmith_kicad::library_stanzas(Some(&sym_text), fp_text.as_deref())
    });
    match stanzas {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::Leaf;
    use crate::commands::common::testing::parse;
    use crate::commands::kicad_libs;

    #[test]
    fn kicad_libs_import_takes_one_or_two_library_tables() {
        assert_eq!(
            parse(&["import", "kicad-libs", "libraries", "sym-lib-table"]),
            Ok(Leaf::KicadLibs(kicad_libs::Args {
                command: kicad_libs::Command::Libraries(kicad_libs::LibrariesArgs {
                    sym_lib_table: "sym-lib-table".into(),
                    fp_lib_table: None
                })
            }))
        );
        assert_eq!(
            parse(&["import", "kicad-libs", "libraries", "s", "f"]),
            Ok(Leaf::KicadLibs(kicad_libs::Args {
                command: kicad_libs::Command::Libraries(kicad_libs::LibrariesArgs {
                    sym_lib_table: "s".into(),
                    fp_lib_table: Some("f".into())
                })
            }))
        );
        assert!(parse(&["import", "kicad-libs"]).is_err());
        assert!(parse(&["import", "kicad-libs", "export", "s"]).is_err());
        assert!(parse(&["import", "kicad-libs", "libraries", "s", "f", "g"]).is_err());
        assert_eq!(
            parse(&[
                "import",
                "kicad-libs",
                "models",
                "f",
                "--library",
                "a",
                "--library",
                "b"
            ]),
            Ok(Leaf::KicadLibs(kicad_libs::Args {
                command: kicad_libs::Command::Models(kicad_libs::ModelsArgs {
                    fp_lib_table: "f".into(),
                    libraries: vec!["a".into(), "b".into()]
                })
            }))
        );
    }
}
