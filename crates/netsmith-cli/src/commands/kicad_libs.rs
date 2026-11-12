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
    Import(ImportArgs),
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct ImportArgs {
    /// The project's symbol library table.
    #[arg(value_name = "SYM-LIB-TABLE")]
    pub sym_lib_table: String,
    /// The project's footprint library table.
    #[arg(value_name = "FP-LIB-TABLE")]
    pub fp_lib_table: Option<String>,
}

pub fn run(args: Args) -> ExitCode {
    match args.command {
        Command::Import(a) => run_kicad_libs_import(&a.sym_lib_table, a.fp_lib_table.as_deref()),
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
