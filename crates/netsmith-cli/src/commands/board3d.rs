use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args as ClapArgs;

use super::common::with_placed_board;

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`board3d` places a board as `fab` does, with the layout file and the physical file, and prints its 3D scene (ADR 0105) without routing or writing anything but the `--glb` and `--step` files, and without KiCad. With neither `--json`, `--glb` nor `--step` it prints a short summary. A file of several boards needs `--board`. docs/BOARD3D.md has the frame and every field."
)]
pub struct Args {
    /// The source file; with none, the project's boards.
    #[arg(value_name = "FILE.NSM")]
    pub file: Option<String>,
    /// The board, when the file has several (`Name`, or `System/member`), or a system (`System`).
    ///
    /// A system's name gives its _System scene_ (ADR 0110): each member placed against the others
    /// by its mates, in the frame of the first member; the report has each member's motion and
    /// report, the mates with their mated heights, and the findings across boards. `--glb` and
    /// `--step` write it.
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// Print the report an agent reads: bodies, heights and where they came from, limits, overlaps.
    ///
    /// Each part's body box, its Z range and where its height came from (`stated`, `catalog` or
    /// `unknown`, never a guessed height), each height limit with the tallest part under it and the
    /// margin, the parts too tall, bodies that overlap in 3D, the parts of unknown height with the
    /// reason, and the height coverage; and the board's case models with what their fit check
    /// found (`case_collision`, `case_clearance`, `case_reach`, `case_boss`, `case_unchecked`,
    /// ADR 0109). With `--glb` as well it does both.
    #[arg(long)]
    pub json: bool,
    /// Write the scene as glTF 2.0 binary, in place of the summary.
    ///
    /// It is the file the editor's 3D view shows (docs/BOARD3D.md). For a system, the root is
    /// `system` with one node per member carrying its motion, and the ids gain the member
    /// (`led/J1`).
    #[arg(long, value_name = "FILE.GLB")]
    pub glb: Option<PathBuf>,
    /// Write the board and its parts as a STEP file (AP214), in place of the summary.
    ///
    /// For mechanical CAD: the board through its thickness with its cutouts and holes, each part a
    /// product named by its designator, as exact solids; a part of unknown height is its outline
    /// only, never a guessed solid (ADR 0107; docs/BOARD3D.md). For a system, a product per
    /// member, each placed by its motion.
    #[arg(long, value_name = "FILE.STEP")]
    pub step: Option<PathBuf>,
}

/// Prints the board's 3D scene, or writes it.
pub fn run(args: Args) -> ExitCode {
    run_board3d(
        args.file.as_deref(),
        args.board.as_deref(),
        args.json,
        args.glb.as_deref(),
        args.step.as_deref(),
    )
}

/// `netsmith board3d`: one board placed as `fab` places it (the pick lock and the layout file
/// beside its source are read, nothing is written) and its 3D scene printed: the JSON report on
/// stdout with `--json`, the scene as a `.glb` file with `--glb` and as a STEP file with
/// `--step` (and no summary), a summary with none of them. Exit 0 when the report is printed (and the file written), 1 when the board does not
/// build or place, 2 when the command cannot start or the file cannot be written.
fn run_board3d(
    file: Option<&str>,
    board: Option<&str>,
    json: bool,
    glb: Option<&Path>,
    step: Option<&Path>,
) -> ExitCode {
    let made = match with_placed_board(file, board, |placed| placed.run_any()) {
        Ok(made) => made,
        Err(code) => return code,
    };
    let (report, scene) = match made {
        netsmith::board3d::Made::Board(made) => *made,
        netsmith::board3d::Made::System(made) => {
            let (report, scene) = *made;
            for m in &scene.members {
                for w in &m.scene.warnings {
                    eprintln!("warning: {}/{}: {w}", scene.name, m.name);
                }
            }
            let files = [
                glb.map(|out| (out, netsmith_3d::gltf::system_glb(&scene))),
                step.map(|out| (out, netsmith_3d::step::system_step(&scene).into_bytes())),
            ];
            for (out, bytes) in files.into_iter().flatten() {
                if let Err(e) = fs::write(out, bytes) {
                    eprintln!("error: {}: {e}", out.display());
                    return ExitCode::from(2);
                }
            }
            if json {
                print!("{}", report.json());
            } else if glb.is_none() && step.is_none() {
                print!("{}", report.summary());
            }
            return ExitCode::SUCCESS;
        }
    };
    let label = &report.board.name;
    for w in &scene.warnings {
        eprintln!("warning: {label}: {w}");
    }
    let files = [
        glb.map(|out| (out, netsmith_3d::gltf::glb(&scene, label))),
        step.map(|out| (out, netsmith_3d::step::step(&scene, label).into_bytes())),
    ];
    for (out, bytes) in files.into_iter().flatten() {
        if let Err(e) = fs::write(out, bytes) {
            eprintln!("error: {}: {e}", out.display());
            return ExitCode::from(2);
        }
    }
    if json {
        print!("{}", report.json());
    } else if glb.is_none() && step.is_none() {
        print!("{}", report.summary());
    }
    ExitCode::SUCCESS
}
