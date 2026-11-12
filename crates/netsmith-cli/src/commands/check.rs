/// Check and solve the boards, write nothing, print the findings only.
///
/// `check` checks and solves the boards as `build` does and writes nothing: no schematic, no
/// pick lock, no pin file. It prints the findings only (errors, warnings and notes, as `--notes`
/// says), not the bill of materials. With no file it acts on the project's boards.
///
/// The exit code is 0 when every board builds, 1 when one does not, and 2 when the command
/// cannot run (a file that cannot be read, no boards).
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct CheckArgs {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// Make a guideline finding an error for this run, as with `build`.
    #[arg(long)]
    pub strict: bool,
    /// Text for a person, or one JSON document on stdout, as with `build`.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
    /// Each note, a count for each cause, or none, as with `build`.
    #[arg(long, value_enum, default_value = "all")]
    pub notes: NotesMode,
}
