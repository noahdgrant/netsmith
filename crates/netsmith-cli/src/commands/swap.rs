//! `netsmith board swap` (issue #1094): the editor's swap of a part (issue #800) on the command
//! line. It opens the project as the editor does and asks the same backend (`GET /api/swap`,
//! `POST /api/swap`, `crates/netsmith-workspace/src/swap.rs`), so the alternatives, their checks
//! and the text written are the editor's.

use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_editor::api::{Swap, SwapAlternative, SwapApply, SwapCheck};
use netsmith_editor::{ApiError, Backend};
use netsmith_fabricate::project::Target;
use netsmith_workspace::Workspace;

use super::common;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`board swap` reads the project as the editor does and gives what the editor's swap gives for a generic part that no catalog row stands for: the nearest rows of its catalog, each with what the board's checks say with it in the part's place (`passes`, `fails` with what fails, or `not checked` past the board's cap of re-solves, ADR 0047), and the change to the source that chooses it.\n\nWith `--apply`, it writes that change, as the editor's Swap button does: the row's value (`240ohm +/- 1%`) over the right-hand side of the one statement that states the part's value, or as a statement in the block of the part's declaration when nothing states it, with the rest of the file as it was (a hand pick, ADR 0122). When the source has a pick lock, the part takes a new pick in it and the other parts keep theirs. The swap refuses, with the reason, where one edit would not change this part alone: the value stated in several places, in a definition other parts share, in a board's `defaults`, or in the standard library.\n\nThe exit code is 0 when the alternatives are listed or one is written, 1 when the board does not build or the part has nothing the swap can do (a row stands for it, or the swap refuses), and 2 when the command cannot run (no such part, board or alternative)."
)]
pub struct Args {
    /// The part, by its designator (`R1`) or its path from the board (`r1`, `reg.inductor`).
    #[arg(value_name = "PART")]
    pub part: String,
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// Only the board of this name (`Board`, or `System/Board` or just `Board` for a board of a system).
    #[arg(long, value_name = "BOARD")]
    pub board: Option<String>,
    /// Write this alternative into the source: its row name as listed (`240Ω 1% 0402 C25094`), its LCSC number (`C25094`) or its number in the list (`1`).
    #[arg(long, value_name = "ALTERNATIVE")]
    pub apply: Option<String>,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` is the editor's answer: without `--apply`, what `GET /api/swap` answers (`board`,
    /// `refdes`, `path`, `message`, `member`, `stated`, `refused` and the `alternatives`, each with
    /// its `name`, `line`, `lcsc`, `mpn`, `check`, `why`, `value` and `edit`); with it, the
    /// `alternative` written, with its `edit`, and what `POST /api/swap` answers (`file`, `lock`).
    #[arg(long, value_enum, default_value = "text")]
    pub format: common::Format,
}

/// A failure, and the exit code it gives.
struct Failed {
    message: String,
    code: u8,
}

impl From<ApiError> for Failed {
    fn from(e: ApiError) -> Failed {
        // A part with nothing to swap, a refused swap and a file changed under it are what the
        // command checked; a name that matches nothing is a usage mistake.
        let code = match e.status {
            409 | 422 => 1,
            _ => 2,
        };
        Failed {
            message: e.message,
            code,
        }
    }
}

fn failed(message: impl Into<String>, code: u8) -> Failed {
    Failed {
        message: message.into(),
        code,
    }
}

pub fn run(args: Args) -> ExitCode {
    match swap(&args) {
        Ok(out) => {
            print!("{out}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {}", e.message.trim_end());
            ExitCode::from(e.code)
        }
    }
}

fn swap(args: &Args) -> Result<String, Failed> {
    let target = Target::resolve(args.file.as_deref()).map_err(|e| failed(e, 2))?;
    let workspace = Workspace::open(target).map_err(|e| failed(e, 2))?;
    let analysis = workspace.analysis();
    let errors: Vec<&str> = analysis
        .problems
        .iter()
        .filter(|p| p.severity == netsmith_editor::api::Severity::Error)
        .map(|p| p.rendered.trim_end())
        .collect();
    // A board that does not build is not among the boards: the errors say why.
    let not_built = |message: String| {
        if errors.is_empty() {
            failed(message, 2)
        } else {
            failed(
                format!("the source does not build:\n{}", errors.join("\n")),
                1,
            )
        }
    };
    let names: Vec<&str> = analysis.boards.iter().map(|b| b.name.as_str()).collect();
    let board = match &args.board {
        Some(want) => analysis
            .boards
            .iter()
            .find(|b| b.name == *want)
            .or_else(|| {
                let mut members = analysis
                    .boards
                    .iter()
                    .filter(|b| b.system.is_some() && b.name.ends_with(&format!("/{want}")));
                members.next().filter(|_| members.next().is_none())
            })
            .map(|b| b.name.clone())
            .ok_or_else(|| {
                not_built(format!(
                    "there is no board `{want}`; the boards are: {}",
                    names.join(", ")
                ))
            })?,
        None => match names.as_slice() {
            [one] => one.to_string(),
            [] => return Err(not_built("there is no board".to_string())),
            many => {
                return Err(failed(
                    format!(
                        "there are {} boards; say which with `--board <name>`: {}",
                        many.len(),
                        many.join(", ")
                    ),
                    2,
                ));
            }
        },
    };
    let detail = workspace.board(&board)?;
    let refdes = detail
        .parts
        .iter()
        .find(|p| p.refdes == args.part)
        .or_else(|| detail.parts.iter().find(|p| p.path == args.part))
        .map(|p| p.refdes.clone())
        .ok_or_else(|| {
            failed(
                format!(
                    "board `{board}` has no part `{}` (by designator or path)",
                    args.part
                ),
                2,
            )
        })?;
    let swap = workspace.swap(&board, &refdes)?;
    let json = args.format == common::Format::Json;
    let Some(want) = &args.apply else {
        return Ok(if json {
            format!(
                "{}\n",
                serde_json::to_string_pretty(&swap).unwrap_or_default()
            )
        } else {
            listing(&swap)
        });
    };
    let alternative = chosen(&swap, want)?;
    let swapped = workspace.swap_apply(&SwapApply {
        board,
        refdes,
        alternative: alternative.name.clone(),
    })?;
    Ok(if json {
        let mut out = serde_json::to_value(&swapped).unwrap_or_default();
        out["alternative"] = serde_json::to_value(alternative).unwrap_or_default();
        format!(
            "{}\n",
            serde_json::to_string_pretty(&out).unwrap_or_default()
        )
    } else {
        let mut out = format!("{} is now {}", swap.path, alternative.name);
        if let Some(edit) = &alternative.edit {
            out += &format!(
                ": {}:{} `{}` -> `{}`",
                edit.at.file, edit.at.line, edit.old, edit.new
            );
        }
        out.push('\n');
        if let Some(lock) = &swapped.lock {
            out += &format!("{lock}: {} picked again\n", swap.path);
        }
        out
    })
}

/// The alternative `want` names: by its row name, its LCSC number or its number in the list.
fn chosen<'a>(swap: &'a Swap, want: &str) -> Result<&'a SwapAlternative, Failed> {
    let all = &swap.alternatives;
    let by_name = all.iter().find(|a| a.name == want);
    let by_lcsc = || {
        let mut found = all.iter().filter(|a| a.lcsc.as_deref() == Some(want));
        found.next().filter(|_| found.next().is_none())
    };
    let by_number = || {
        want.parse::<usize>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .and_then(|i| all.get(i))
    };
    by_name.or_else(by_lcsc).or_else(by_number).ok_or_else(|| {
        failed(
            format!(
                "`{want}` is not an alternative of {}; they are: {}",
                swap.refdes,
                all.iter()
                    .map(|a| a.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            2,
        )
    })
}

/// The swap as text: the part, why no row stands for it, and each alternative with its check
/// and the change that chooses it.
fn listing(swap: &Swap) -> String {
    let mut out = format!(
        "{} ({}) on {}: {}\n",
        swap.refdes,
        swap.path,
        swap.board,
        swap.message.trim_end()
    );
    if let (Some(member), Some(at)) = (&swap.member, &swap.stated) {
        out += &format!("  .{member} is stated at {}:{}\n", at.file, at.line);
    }
    if let Some(why) = &swap.refused {
        out += &format!("  the swap cannot write a value: {why}\n");
    }
    if swap.alternatives.is_empty() {
        out += "  no row of the catalog is near enough to offer\n";
    }
    for (i, alt) in swap.alternatives.iter().enumerate() {
        let check = match alt.check {
            SwapCheck::Passes => "passes every check".to_string(),
            SwapCheck::Fails => match &alt.why {
                Some(why) => format!("fails: {why}"),
                None => "fails".to_string(),
            },
            SwapCheck::NotChecked => "not checked (past the board's cap of re-solves)".to_string(),
        };
        out += &format!("  {}. {}: {check}\n", i + 1, alt.line);
        if let Some(edit) = &alt.edit {
            out += &format!(
                "     writes {}:{} `{}` -> `{}`\n",
                edit.at.file, edit.at.line, edit.old, edit.new
            );
        }
    }
    if let Some(first) = swap.alternatives.iter().find(|a| a.edit.is_some()) {
        let pick = first.lcsc.clone().unwrap_or_else(|| "1".to_string());
        out += &format!(
            "Write one into the source with `netsmith board swap {} --apply {pick}`.\n",
            swap.path
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::Leaf;
    use crate::commands::common::testing::parse;

    #[test]
    fn a_part_comes_first_and_a_file_after_it() {
        let Ok(Leaf::Swap(a)) = parse(&[
            "board", "swap", "r1", "a.nsm", "--board", "B", "--apply", "C25094", "--format", "json",
        ]) else {
            panic!("swap should parse");
        };
        assert_eq!(a.part, "r1");
        assert_eq!(a.file.as_deref(), Some("a.nsm"));
        assert_eq!(a.board.as_deref(), Some("B"));
        assert_eq!(a.apply.as_deref(), Some("C25094"));
        assert!(parse(&["board", "swap"]).is_err());
    }
}
