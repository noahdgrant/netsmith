//! `netsmith part from-symbol` (#453): a draft part file from a KiCad symbol's pins.

use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args as ClapArgs, Subcommand};

#[derive(Debug, PartialEq, ClapArgs)]
pub struct Args {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, PartialEq, Subcommand)]
pub enum Command {
    /// Print a draft part file from a KiCad symbol's pins: names, pads and kinds.
    #[command(
        name = "from-symbol",
        after_long_help = "The symbol is read from a symbol library (`.kicad_sym`) or from a schematic's own symbols (`.kicad_sch`). The draft has one row for each pin name of a unit, with the name the symbol prints, its pads and the kind its electrical type gives; `TODO_KIND` where the type decides none, which does not parse, so the file does not build until each is decided. The header says to check every row against the datasheet's pin table. A file that cannot be read, or a symbol it does not have, is exit 2."
    )]
    FromSymbol(FromSymbolArgs),
}

#[derive(Debug, PartialEq, ClapArgs)]
pub struct FromSymbolArgs {
    /// The symbol library or schematic to read.
    #[arg(value_name = "LIB.KICAD_SYM | FILE.KICAD_SCH")]
    pub file: PathBuf,
    /// The symbol's name (`Library:Name` in a schematic, or the name alone when it is unique).
    #[arg(value_name = "SYMBOL")]
    pub symbol: String,
}

pub fn run(args: Args) -> ExitCode {
    match args.command {
        Command::FromSymbol(a) => from_symbol(&a),
    }
}

/// The draft goes to stdout; a file that cannot be read or a symbol it does not have is exit 2.
fn from_symbol(args: &FromSymbolArgs) -> ExitCode {
    let shown = args.file.display().to_string();
    let found = fs::read_to_string(&args.file)
        .map_err(|e| format!("{shown}: {e}"))
        .and_then(|text| netsmith_kicad::symbol_in(&text, &shown, &args.symbol));
    match found {
        Ok(found) => {
            print!(
                "{}",
                netsmith_tools::part_draft::draft(&found, &shown, &args.symbol)
            );
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
    use crate::commands::common::testing::{kind, parse};
    use crate::commands::{Command, part};
    use clap::error::ErrorKind;

    #[test]
    fn part_from_symbol_takes_a_file_and_a_symbol() {
        assert!(matches!(
            parse(&["part", "from-symbol", "a.kicad_sch", "U1"]),
            Ok(Command::Part(part::Args {
                command: part::Command::FromSymbol(part::FromSymbolArgs { ref symbol, .. })
            })) if symbol == "U1"
        ));
        assert_eq!(
            kind(&["part", "from-symbol", "a.kicad_sch"]),
            ErrorKind::MissingRequiredArgument
        );
    }
}
