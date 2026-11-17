//! `netsmith project order`: list the project's orders, or make one (ADR 0035). It was
//! `netsmith fab --order` before issue #1090; the work is `fab::run_order`.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Args as ClapArgs;

use super::common::{self, sync_pins};
use super::fab::{self, Constraints, OrderRequest};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "An order names its boards in netsmith.toml (`[[order]]`, ADR 0035), so `project order` takes no file: with no name it lists the project's orders, and with one it makes the fab package of each board of the order, as `board fab` makes it, and the order's own files (`order.txt`) into one folder. An order is a release: it never moves a pin by itself, and a seed of the layout file that would change stops it. A board a plain `board fab` made into the same folder from the same inputs is copied, not made again. It prints text only."
)]
pub struct Args {
    /// The order to make; with none, list the project's orders.
    #[arg(value_name = "NAME")]
    pub name: Option<String>,
    /// The folder to write into (default the project's fab folder).
    #[arg(long, value_name = "DIR")]
    pub out: Option<PathBuf>,
    /// Seconds the router may take for a board; 0 is no limit (default 300).
    #[arg(long, value_name = "SECS")]
    pub route_timeout: Option<u64>,
    /// Package a board that fails the design rules check (never an unmet `[constraint]`).
    #[arg(long)]
    pub allow_drc_violations: bool,
    /// Compare the files and the findings with KiCad's own plot and check (needs kicad-cli).
    #[arg(long)]
    pub cross_check: bool,
    /// Make a guideline finding an error for this run.
    #[arg(long)]
    pub strict: bool,
    /// Stop when the pick lock or the pin file of a source file of the order would change.
    #[arg(long)]
    pub locked: bool,
    /// Whether an unmet requirement on where a part goes stops the order, over netsmith.toml.
    #[arg(long, value_enum)]
    pub constraints: Option<Constraints>,
    /// Leave the boards' 3D scenes (`<board>.glb`) out of the packages.
    #[arg(long)]
    pub no_glb: bool,
    /// Leave the boards as STEP (`<board>.step`) out of the packages.
    #[arg(long)]
    pub no_step: bool,
    /// Ask no supplier for stock or price; say in `order.txt` that they were not checked.
    #[arg(long)]
    pub offline: bool,
    /// Print the result of each board, not the stages.
    #[arg(long)]
    pub quiet: bool,
    #[command(flatten)]
    pub placing: common::Placing,
    #[command(flatten)]
    pub watching: common::Watching,
}

impl Args {
    /// The order to make, or the list of them.
    pub fn request(&self) -> OrderRequest {
        match &self.name {
            Some(name) => OrderRequest::Make(name.clone()),
            None => OrderRequest::List,
        }
    }
}

/// Lists the project's orders, or makes one.
pub fn run(args: Args) -> ExitCode {
    if let Some(code) = sync_pins(None, args.locked) {
        return code;
    }
    args.watching.tell_progress("order", args.quiet);
    let code = common::with_stage_events(|| {
        fab::run_order(
            &args.request(),
            args.out.as_deref(),
            args.route_timeout,
            args.allow_drc_violations,
            args.cross_check,
            args.strict,
            args.locked,
            args.constraints.map(|c| c == Constraints::Warn),
            args.no_glb,
            args.no_step,
            args.offline,
            &args.placing,
        )
    });
    crate::progress::ended(
        code == ExitCode::SUCCESS,
        if code == ExitCode::SUCCESS {
            "order: done"
        } else {
            "order: failed (its output says why)"
        },
    );
    code
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::{Leaf, kind, parse};
    use crate::commands::fab::OrderRequest;
    use crate::commands::order;
    use clap::error::ErrorKind;

    fn order(args: &[&str]) -> order::Args {
        let mut line = vec!["project", "order"];
        line.extend_from_slice(args);
        match parse(&line) {
            Ok(Leaf::Order(order)) => order,
            other => panic!("{args:?}: {other:?}"),
        }
    }

    #[test]
    fn project_order_takes_a_name_or_lists_the_orders() {
        assert_eq!(
            order(&["devkit"]).request(),
            OrderRequest::Make("devkit".into())
        );
        assert_eq!(order(&[]).request(), OrderRequest::List);
        // A flag is not its name.
        let o = order(&["--out", "x"]);
        assert_eq!((o.request(), o.out), (OrderRequest::List, Some("x".into())));
        let o = order(&["--out", "x", "d", "--route-timeout", "9", "--offline"]);
        assert_eq!(o.request(), OrderRequest::Make("d".into()));
        assert_eq!(o.route_timeout, Some(9));
        assert!(o.offline);
        // One order at a time, and not the flags of `board fab` alone.
        assert_eq!(
            kind(&["project", "order", "a", "b"]),
            ErrorKind::UnknownArgument
        );
        for flag in ["--save-placed", "--reuse-routes", "--format", "--pins"] {
            assert!(parse(&["project", "order", "d", flag]).is_err(), "{flag}");
        }
        assert!(parse(&["board", "fab", "--order", "d"]).is_err());
    }
}
