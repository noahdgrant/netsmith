/// Exit 0 when the designs match, 1 when they differ, 2 when they could not be compared or
/// the comparison could not be decided.
fn run_compare(request: &Request, format: Format) -> ExitCode {
    let kicad = KicadCliExe::locate();
    match compare_files(request, kicad.as_ref().map(|k| k as &dyn KicadCli)) {
        Ok(done) if format == Format::Json => {
            print!("{}", done.json);
            match done.verdict {
                Verdict::Match => ExitCode::SUCCESS,
                Verdict::Different => ExitCode::FAILURE,
                Verdict::Undecided => ExitCode::from(2),
            }
        }
        Ok(done) => match done.verdict {
            Verdict::Match => {
                print!("{}", done.text);
                ExitCode::SUCCESS
            }
            Verdict::Different => {
                print!("{}", done.text);
                ExitCode::FAILURE
            }
            Verdict::Undecided => {
                eprint!("{}", done.text);
                ExitCode::from(2)
            }
        },
        Err(e) => {
            eprintln!("error: {}", e.trim_end());
            ExitCode::from(2)
        }
    }
}

/// Are two designs the same circuit?
///
/// `compare` checks a design against another by connectivity (ADR 0017), for example a port
/// against the KiCad design it came from. Each of `<A>` and `<B>` is a `.kicad_sch`, a
/// `.kicad_pcb`, a KiCad netlist (the XML of `kicad-cli sch export netlist --format kicadxml`)
/// or a `.nsm` file, which is built as `build` does. Net names and, by default, designators may
/// differ between the two; components are matched by value, footprint (without its library) and
/// how their pins are wired. It prints `match` with the designators and net names that
/// correspond, or each difference: a component with no counterpart, a pair that differs in
/// value or footprint, a net whose pins differ (each pin in its own side's designators), a net
/// with no counterpart.
///
/// Power ports and flags (`#PWR`, `#FLG`) are not parts. A pin joined to nothing is on no net,
/// so it differs only from a pin that is joined to something. Reading a `.kicad_sch` needs
/// KiCad's command-line tool, found as for `fab --cross-check`.
///
/// The exit code is 0 for a match, 1 for a difference and 2 for a mistake or a comparison that
/// could not be decided (many interchangeable parts and a search past its bound).
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct CompareArgs {
    /// The first design.
    pub a: String,
    /// The second design.
    pub b: String,
    /// The board of a `.nsm` file that has several (`Name`, or `System/member`).
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// A file of `A_ref B_ref` lines that fix pairs of designators.
    ///
    /// The rest are matched by wiring. Blank lines and `#` comments are ignored.
    #[arg(long, value_name = "FILE", conflicts_with = "by_refdes")]
    pub map: Option<PathBuf>,
    /// Designators are identity: a component matches only the one with the same designator.
    #[arg(long)]
    pub by_refdes: bool,
    /// Leave out nets of a single pin on both sides.
    #[arg(long)]
    pub ignore_unconnected: bool,
    /// Values that read as the same number agree (`4k7` and `4.7k`, `100n` and `0.1uF`).
    #[arg(long)]
    pub loose_values: bool,
    /// Text for a person, or one JSON document on stdout.
    ///
    /// `json` holds the `verdict` and the pairs of a match or each difference.
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}
