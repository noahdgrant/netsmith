use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_tools::findings::NotesMode;

use super::build::{BuildRun, Explain, run_build};
use super::common::Format;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`board explain` checks and solves the boards as `check` does, writes nothing, and prints what the solve left at PATH, as text or, with `--format json`, as one JSON document (the fields are under `--format`): for each part at the path or inside it (`reg` is every part of the module `reg`), the row it was picked as, what held it to that row, and the rows it passed over before it with why (a row that does not fit what the design states, a row a relation refused with the file and line of the relation, a row after which the parts that follow could not be chosen); then the members at the path or inside it with the ranges the solve left them (the current an inductor carries, the voltage a chain leaves at a node). It is the view of what `board build --emit-solved` writes.\n\nWhen a board fails, the error is printed as `check` prints it and the record of the failed group follows: for each part of it, the rows its first turn passed over, with the choices of the parts before it that the turn was made with. A part the pick lock holds to a row was offered no other, so it has no rows passed over: run `netsmith project lock --update`, or delete the lock, to see the rows the solver takes now.\n\nThe exit code is 0 when every board builds and something is at PATH, 1 when a board does not build or nothing is at PATH, and 2 when the command cannot run."
)]
pub struct Args {
    /// The part, module or member to explain, by its path from the board (`reg.inductor`, `reg.inductor.current`).
    #[arg(value_name = "PATH")]
    pub path: String,
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// Only the board of this name (`Board`, or `System/Board` or just `Board` for a board of a system).
    #[arg(long, value_name = "BOARD")]
    pub board: Option<String>,
    /// List every row a part passed over, not the first 20.
    #[arg(long)]
    pub all_rows: bool,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` prints one JSON document on stdout and nothing else there: `command` (`explain`),
    /// `ok`, the `path` asked for, the `boards` that have something at it, the `findings` and the
    /// count of the `notes`. Each board has its `board` name, `failed` when its solve failed, and
    /// the `picks`, the `unpicked` and the `members` at the path or inside it, as
    /// `board build --emit-solved` writes them: a pick is its `path`, `catalog`, `row`, `count`
    /// for a bank, `held`, and the rows `passed` over, each with its `row`, `why`, `message` and
    /// `at`; with `--all-rows` not given a part lists its first 20 rows and `passed_more` says how
    /// many it leaves out. When nothing is at the path, or a board does not build, the findings
    /// say so (as `board check --format json` does) and `ok` is false.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}

/// How many rows passed over a part's record lists unless `--all-rows` is given.
const ROWS: usize = 20;

pub fn run(args: Args) -> ExitCode {
    run_build(&BuildRun {
        file: args.file.as_deref(),
        out: None,
        strict: false,
        locked: false,
        format: args.format.into(),
        notes: NotesMode::None,
        write: false,
        emit_design: None,
        emit_solved: None,
        portable: false,
        explain: Some(Explain {
            path: &args.path,
            board: args.board.as_deref(),
            rows: (!args.all_rows).then_some(ROWS),
        }),
    })
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::Leaf;
    use crate::commands::common::testing::parse;

    #[test]
    fn a_path_comes_first_and_a_file_after_it() {
        let Ok(Leaf::Explain(a)) =
            parse(&["board", "explain", "reg.inductor", "a.nsm", "--board", "B"])
        else {
            panic!("explain should parse");
        };
        assert_eq!(a.path, "reg.inductor");
        assert_eq!(a.file.as_deref(), Some("a.nsm"));
        assert_eq!(a.board.as_deref(), Some("B"));
        assert!(parse(&["board", "explain"]).is_err());
    }
}
