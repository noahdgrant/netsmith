use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_editor::Backend;
use netsmith_editor::api::{Bom, BomLine};

use super::views::{self, Kind, Printing};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`board bom` prints the bill of materials the editor's Bill of materials view shows for a board, with each line's price, stock and the day they were read, and the total of the lines that have a price: the lines the board house assembles, grouped as `bom.csv` is, then the parts soldered by hand, then the parts not fitted (DNP): each `alternate` part, on the designator of the part whose footprint it may take. It is made by the code the editor's `GET /api/bom` calls, so the two are the same; `--format json` prints that route's answer. It writes nothing and needs no `fab`.\n\nA price and stock come from the project's supply (the catalogs and the providers `netsmith.toml` names, ADR 0047); a line whose price is unknown says why, and a line that no catalog row stands for says what is wanted. With `--offline` no provider that would use the network is asked: an answer kept earlier is used at any age, and the note says that stock and price were not checked.\n\nWith no file it acts on the project's boards; a source with several boards needs `--board`. The exit code is 0 when the bill is printed, 1 when the board does not build, and 2 when the command cannot run."
)]
pub struct Args {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The board to print (`Board`, or `System/Board` or just `Board` for a board of a system); needed when there are several.
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// Ask no supplier for stock or price; say in the note that they were not checked.
    ///
    /// No provider that would use the network is asked, and an answer kept earlier is used at any
    /// age, as `board fab --offline` does.
    #[arg(long)]
    pub offline: bool,
    #[command(flatten)]
    pub printing: Printing,
}

/// Prints the bill of materials of one board.
pub fn run(args: Args) -> ExitCode {
    let (workspace, analysis) = match views::open(args.file.as_deref(), args.offline) {
        Ok(w) => w,
        Err(code) => return code,
    };
    let board = match views::choose(&analysis, &Kind::Board, args.board.as_deref()) {
        Ok(b) => b,
        Err(code) => return code,
    };
    views::print(
        workspace.bom(&board),
        &analysis,
        args.printing.format,
        (|v| serde_json::to_string_pretty(v), text),
    )
}

fn usd(amount: f64) -> String {
    format!("${amount:.4}")
}

/// What a line says apart from its price: a consigned part, a price that is old or unknown, a
/// part no row stands for, the generic a part provides.
fn remarks(line: &BomLine) -> String {
    let mut says = Vec::new();
    if line.consigned {
        says.push("consigned: you buy it and ship it to the board house".to_string());
    }
    if let Some(wanted) = &line.unresolved {
        says.push(format!("unresolved: {wanted}"));
    }
    if let Some(why) = &line.unknown {
        says.push(format!("price unknown: {why}"));
    }
    if line.stale {
        says.push(match line.age_days {
            Some(days) => format!("stale ({days} days old)"),
            None => "stale".to_string(),
        });
    }
    if let Some(generic) = &line.via {
        says.push(format!("provides {generic}"));
    }
    says.join("; ")
}

fn lines(lines: &[BomLine]) -> String {
    let dash = || "-".to_string();
    let mut rows = vec![
        [
            "QTY", "REFS", "VALUE", "PACKAGE", "CHANNEL", "SKU", "LIBRARY", "UNIT", "LINE",
            "STOCK", "READ", "NOTES",
        ]
        .map(String::from)
        .to_vec(),
    ];
    for line in lines {
        rows.push(vec![
            line.quantity.to_string(),
            line.designators.join(","),
            line.value.clone(),
            line.package.clone().unwrap_or_else(dash),
            line.channel.clone().unwrap_or_else(dash),
            line.sku.clone().unwrap_or_else(dash),
            line.library.clone().unwrap_or_else(dash),
            line.unit_usd.map_or_else(dash, usd),
            line.line_usd.map_or_else(dash, usd),
            line.stock.map_or_else(dash, |s| s.to_string()),
            line.date.clone().unwrap_or_else(dash),
            remarks(line),
        ]);
    }
    views::table(&rows)
}

/// The text of the bill: the lines the board house places, those soldered by hand, the total and
/// where the prices come from.
fn text(bom: &Bom) -> String {
    let mut out = format!(
        "{}: bill of materials, {} line(s) assembled, {} soldered by hand\n\n",
        bom.board,
        bom.lines.len(),
        bom.hand.len()
    );
    out.push_str(&lines(&bom.lines));
    if !bom.hand.is_empty() {
        out.push_str("\nSoldered by hand (the board house does not place these):\n");
        out.push_str(&lines(&bom.hand));
    }
    if !bom.not_fitted.is_empty() {
        out.push_str(
            "\nNot fitted (DNP lines of bom.csv: the footprint is on the board, the value is chosen on the bench; and each alternate, on the footprint of the part it may take the place of, in no line of bom.csv and listed in bom-alternates.csv of the fab package):\n",
        );
        out.push_str(&lines(&bom.not_fitted));
    }
    out.push('\n');
    match bom.total_usd {
        Some(total) => out.push_str(&format!("Total of the priced lines: ${total:.2}\n")),
        None => out.push_str("Total: no line has a price\n"),
    }
    out.push_str(&format!(
        "{}\nA price or stock read more than {} days ago is marked stale.\n",
        bom.note, bom.stale_after_days
    ));
    out
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{Leaf, parse};

    #[test]
    fn it_takes_a_file_a_board_offline_and_a_format() {
        let Ok(Leaf::Bom(a)) = parse(&[
            "board",
            "bom",
            "a.nsm",
            "--board",
            "B",
            "--offline",
            "--format",
            "json",
        ]) else {
            panic!()
        };
        assert_eq!(a.file.as_deref(), Some("a.nsm"));
        assert_eq!(a.board.as_deref(), Some("B"));
        assert!(a.offline);
        assert_eq!(a.printing.format, crate::commands::common::Format::Json);
        assert!(parse(&["board", "bom", "--notes", "all"]).is_err());
    }
}
