//! Running `kicad-cli`. Everything the rest of the crate needs from KiCad goes through the
//! [`KicadCli`] trait, so it can be tested with a stand-in and without KiCad installed.
//!
//! The real implementation also runs Windows KiCad from WSL (`kicad-cli.exe`, found by
//! [`KicadCliExe::locate`]): it copies the board to a Windows temp directory, converts paths
//! with `wslpath`, and reads the results back, as `scripts/check-kicad.sh` does.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::exclusive;

/// A file KiCad wrote (or one netsmith plotted).
pub use netsmith_geom::Output;

#[derive(Debug)]
pub enum CliError {
    /// `kicad-cli` could not be started or its files could not be read.
    Io(String),
    /// Windows KiCad is installed but this process cannot start Windows programs (under WSL,
    /// from a shell that has no interop, such as `systemd-run --user`): `cmd.exe` could not
    /// be run. Nothing that needs Windows KiCad can work here, which is not a failure of
    /// the code under test; see [`KicadCliExe::interop`].
    NoWindows(String),
    /// `kicad-cli` ran and failed.
    Failed { command: String, output: String },
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(m) | Self::NoWindows(m) => write!(f, "{m}"),
            Self::Failed { command, output } => {
                write!(f, "`{command}` failed:\n{}", output.trim_end())
            }
        }
    }
}

impl std::error::Error for CliError {}

/// What packaging asks of KiCad. Every method takes the `.kicad_pcb` path; a `.kicad_pro`
/// next to it (design rules, net classes) is used when present.
///
/// A board with zones (the planes of a board of four layers) is written without their
/// fill, which only KiCad can compute. Each method has KiCad fill the zones first, on its
/// own copy, so a check and a plot see the copper that the board house will make.
pub trait KicadCli {
    /// KiCad's version as `kicad-cli version` prints it (`9.0.0`). The cross-check needs it
    /// to know which known difference of a release applies ([`crate::crosscheck`]).
    fn version(&self) -> Result<String, CliError>;
    /// KiCad's design rules check as its JSON report, with every severity and every
    /// violation of tracks, so that it can be compared with netsmith's own
    /// ([`crate::drc::kicad`]). The text has `violations` and `unconnected_items`, each with
    /// `type`, `description`, `severity` and `items` that carry a `pos` in mm.
    fn drc_json(&self, pcb: &Path) -> Result<String, CliError>;
    /// Gerber files for the given layers (`F.Cu`, `Edge.Cuts`, ...), named
    /// `<board>-F_Cu.gbr`, with the board's drill/place origin as the origin.
    fn gerbers(&self, pcb: &Path, layers: &[String]) -> Result<Vec<Output>, CliError>;
    /// Excellon drill files, `<board>-PTH.drl` and `<board>-NPTH.drl`, in millimetres, with
    /// the same origin as the Gerbers. A kind with no holes has no file.
    fn drill(&self, pcb: &Path) -> Result<Vec<Output>, CliError>;
    /// The text of the CSV position file in millimetres, with the same origin.
    fn pos(&self, pcb: &Path) -> Result<String, CliError>;
    /// The text of the schematic's netlist in KiCad's XML format (`kicad-cli sch export
    /// netlist --format kicadxml`). Takes the root `.kicad_sch`; the other sheets, symbol
    /// libraries and project file in the same directory are used when present.
    fn netlist(&self, sch: &Path) -> Result<String, CliError>;
}

pub struct KicadCliExe {
    exe: PathBuf,
}

impl KicadCliExe {
    pub fn new(exe: impl Into<PathBuf>) -> Self {
        Self { exe: exe.into() }
    }

    /// Finds `kicad-cli` as [`netsmith_kicad::Search::kicad_cli`] does: `$KICAD_CLI`
    /// (and nothing else if it is set), then `kicad-cli` on `PATH`, then, under WSL, the
    /// newest Windows KiCad (`/mnt/c/Program Files/KiCad/<version>/bin/kicad-cli.exe`).
    /// `None` if there is none.
    pub fn locate() -> Option<Self> {
        netsmith_kicad::Search::machine().kicad_cli().map(Self::new)
    }

    /// [`KicadCliExe::locate`], for a test: `None` (the test skips, and says why on stderr, once
    /// per call) when there is no KiCad or when it is Windows KiCad and this process cannot start
    /// Windows programs ([`KicadCliExe::interop`], `CliError::NoWindows`). Any other fault is
    /// not a reason to skip: the KiCad is returned and the test fails when it uses it.
    pub fn locate_runnable() -> Option<Self> {
        let cli = Self::locate()?;
        match cli.interop() {
            Err(CliError::NoWindows(why)) => {
                eprintln!("skipping: Windows KiCad cannot be run from this shell: {why}");
                None
            }
            _ => Some(cli),
        }
    }

    /// The program.
    pub fn exe(&self) -> &Path {
        &self.exe
    }

    /// Whether this is a Windows program (`kicad-cli.exe`), which sees Windows paths and is
    /// run from WSL: the files it reads and writes go in a folder on the Windows side
    /// ([`KicadCliExe::scratch_base`]) and its arguments are Windows paths
    /// ([`KicadCliExe::arg`]).
    pub fn windows(&self) -> bool {
        self.exe.extension().is_some_and(|e| e == "exe")
    }

    /// Whether this program can be run from here: always for a Linux `kicad-cli`; for Windows
    /// KiCad, whether `cmd.exe` can be started. `Err(CliError::NoWindows(..))` says Windows
    /// programs cannot be started from this process, so a test that needs KiCad should skip,
    /// while any other error is a real failure and stays one.
    pub fn interop(&self) -> Result<(), CliError> {
        self.scratch_base().map(|_| ())
    }

    /// The Windows temp directory as a WSL path, asked of `cmd.exe` once per process.
    fn windows_temp() -> Result<PathBuf, CliError> {
        static TEMP: OnceLock<PathBuf> = OnceLock::new();
        if let Some(dir) = TEMP.get() {
            return Ok(dir.clone());
        }
        let ran = exclusive::run(|| {
            let mut command = Command::new("cmd.exe");
            command.args(["/c", "echo %TEMP%"]);
            command
        })
        .map_err(|e| {
            CliError::NoWindows(format!("cannot ask Windows for its temp directory: {e}"))
        })?;
        let win = String::from_utf8_lossy(&ran.output.stdout)
            .trim()
            .to_string();
        if !ran.success() || win.is_empty() {
            return Err(CliError::Failed {
                command: "cmd.exe /c echo %TEMP%".into(),
                output: ran.report(),
            });
        }
        let dir = PathBuf::from(wslpath("-u", &win)?);
        Ok(TEMP.get_or_init(|| dir).clone())
    }

    /// A scratch directory (on the Windows side for Windows KiCad) for the file `name`.
    fn scratch(&self, name: &str) -> Result<Stage<'_>, CliError> {
        let io = |what: &str, e: std::io::Error| CliError::Io(format!("{what}: {e}"));
        let dir = self.scratch_base()?.join(exclusive::scratch_name());
        std::fs::create_dir_all(&dir)
            .map_err(|e| io(&format!("cannot create {}", dir.display()), e))?;
        Ok(Stage {
            cli: self,
            dir,
            name: name.to_string(),
        })
    }

    fn stage(&self, pcb: &Path) -> Result<Stage<'_>, CliError> {
        let io = |what: &str, e: std::io::Error| CliError::Io(format!("{what}: {e}"));
        let stage = self.scratch(
            &pcb.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )?;
        let stem = pcb.with_extension("");
        for ext in ["kicad_pcb", "kicad_pro", "kicad_dru"] {
            let from = stem.with_extension(ext);
            if from.is_file() {
                let to = stage.dir.join(from.file_name().unwrap_or_default());
                std::fs::copy(&from, &to)
                    .map_err(|e| io(&format!("cannot copy {}", from.display()), e))?;
            }
        }
        let board = stage.dir.join(&stage.name);
        if std::fs::read_to_string(&board)
            .map_err(|e| io(&format!("cannot read {}", board.display()), e))?
            // A zone of the board, not one inside a footprint, which is indented further.
            .contains("\n\t(zone")
        {
            stage.fill_zones()?;
        }
        Ok(stage)
    }

    /// The folder to make scratch folders in: the Windows temp folder for Windows KiCad,
    /// else the system's.
    pub fn scratch_base(&self) -> Result<PathBuf, CliError> {
        if self.windows() {
            Self::windows_temp()
        } else {
            Ok(std::env::temp_dir())
        }
    }

    /// `path` as this program wants it in an argument: a Windows path for Windows KiCad.
    pub fn arg(&self, path: &Path) -> Result<String, CliError> {
        if self.windows() {
            wslpath("-w", &path.to_string_lossy())
        } else {
            Ok(path.to_string_lossy().into_owned())
        }
    }

    /// The program that runs KiCad's Python: next to `kicad-cli.exe` on Windows, otherwise
    /// the system's, whose `pcbnew` module is KiCad's.
    pub fn python(&self) -> PathBuf {
        if self.windows() {
            self.exe.with_file_name("python.exe")
        } else {
            PathBuf::from("python3")
        }
    }
}

/// Fills every zone of a board with KiCad's own filler and saves it. It writes to a
/// folder of its own, so the project file beside the board is left as it was. It prints
/// [`FILLED`] when it has saved the board, so that a run that ended without doing so (WSL
/// started the program and it exited with status 0 without running the script) is seen.
const FILL_ZONES: &str = "import sys, pcbnew\n\
board = pcbnew.LoadBoard(sys.argv[1])\n\
filled = pcbnew.ZONE_FILLER(board).Fill(board.Zones())\n\
saved = pcbnew.SaveBoard(sys.argv[2], board)\n\
if saved is False:\n\
\tsys.exit('pcbnew.SaveBoard could not write ' + sys.argv[2])\n\
print('netsmith-zones-filled')\n";

/// What [`FILL_ZONES`] prints last.
const FILLED: &str = "netsmith-zones-filled";

/// How long a file that a Windows program reported saving is waited for to show up on the
/// WSL side.
const FILE_APPEARS: std::time::Duration = std::time::Duration::from_secs(15);

/// Waits for `file` to exist (a file a Windows program wrote can show up late in WSL).
fn wait_for_file(file: &Path) -> bool {
    let started = std::time::Instant::now();
    while !file.is_file() {
        if started.elapsed() > FILE_APPEARS {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    true
}

fn wslpath(flag: &str, path: &str) -> Result<String, CliError> {
    let out = Command::new("wslpath")
        .args([flag, path])
        .output()
        .map_err(|e| CliError::Io(format!("cannot run wslpath: {e}")))?;
    let converted = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || converted.is_empty() {
        return Err(wslpath_failed(flag, path, &out.stdout, &out.stderr));
    }
    Ok(converted)
}

/// The error of a `wslpath <flag> <asked>` that gave no path: the command as it was run (with
/// the path that was asked about, not what it printed) and everything it said.
fn wslpath_failed(flag: &str, asked: &str, stdout: &[u8], stderr: &[u8]) -> CliError {
    CliError::Failed {
        command: format!("wslpath {flag} {asked:?}"),
        output: format!(
            "{}{}",
            String::from_utf8_lossy(stdout),
            String::from_utf8_lossy(stderr)
        ),
    }
}

/// A scratch directory holding a copy of the board, removed when dropped.
struct Stage<'a> {
    cli: &'a KicadCliExe,
    dir: PathBuf,
    /// File name of the board inside `dir`.
    name: String,
}

impl Drop for Stage<'_> {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Stage<'_> {
    /// How `kicad-cli` should be told about a path inside the stage.
    fn arg(&self, path: &Path) -> Result<String, CliError> {
        self.cli.arg(path)
    }

    /// Runs `kicad-cli pcb <args> <board>`.
    fn run(&self, args: &[&str]) -> Result<(), CliError> {
        self.run_group("pcb", args)
    }

    /// Runs `kicad-cli <group> <args> <the staged file>`.
    fn run_group(&self, group: &str, args: &[&str]) -> Result<(), CliError> {
        let board = self.arg(&self.dir.join(&self.name))?;
        let ran = exclusive::run(|| {
            let mut command = Command::new(&self.cli.exe);
            command.arg(group).args(args).arg(&board);
            command
        })?;
        if ran.success() {
            Ok(())
        } else {
            Err(CliError::Failed {
                command: format!(
                    "{} {group} {} {board}",
                    self.cli.exe.display(),
                    args.join(" ")
                ),
                output: ran.report(),
            })
        }
    }

    /// Replaces the staged board with the same board with its zones filled.
    fn fill_zones(&self) -> Result<(), CliError> {
        let script = self.dir.join("fill_zones.py");
        std::fs::write(&script, FILL_ZONES)
            .map_err(|e| CliError::Io(format!("cannot write {}: {e}", script.display())))?;
        let filled_dir = self.dir.join("filled");
        std::fs::create_dir_all(&filled_dir)
            .map_err(|e| CliError::Io(format!("cannot create {}: {e}", filled_dir.display())))?;
        let (board, filled) = (self.dir.join(&self.name), filled_dir.join(&self.name));
        let python = self.cli.python();
        let (script_arg, board_arg, filled_arg) =
            (self.arg(&script)?, self.arg(&board)?, self.arg(&filled)?);
        let command = format!("{} {script_arg} {board_arg} {filled_arg}", python.display());
        let mut earlier = Vec::new();
        for attempt in 1..=exclusive::ATTEMPTS {
            let ran = exclusive::run(|| {
                let mut command = Command::new(&python);
                command.arg(&script_arg).arg(&board_arg).arg(&filled_arg);
                command
            })
            .map_err(|e| {
                CliError::Io(format!(
                    "the board has zones, and filling them needs KiCad's Python: {e}"
                ))
            })?;
            let said = String::from_utf8_lossy(&ran.output.stdout);
            let failure = if !ran.success() {
                // A failure of the script is KiCad's to explain; one of WSL's was started
                // again by `run` already.
                return Err(CliError::Failed {
                    command,
                    output: ran.report(),
                });
            } else if !said.contains(FILLED) {
                "it exited with status 0 but never reached the end of the script".to_string()
            } else if !wait_for_file(&filled) {
                format!(
                    "it saved the board, but {} did not appear within {} seconds",
                    filled.display(),
                    FILE_APPEARS.as_secs()
                )
            } else {
                break;
            };
            let note = format!("{failure}; it printed {:?}", said.trim());
            if attempt == exclusive::ATTEMPTS {
                let mut text = ran.report();
                for (i, e) in earlier.iter().enumerate() {
                    text.push_str(&format!(
                        "\nearlier attempt {} of {}: {e}",
                        i + 1,
                        exclusive::ATTEMPTS
                    ));
                }
                return Err(CliError::Failed {
                    command,
                    output: format!("{note}\n{text}"),
                });
            }
            eprintln!("netsmith: filling the zones, attempt {attempt}: {note}; starting it again");
            earlier.push(note);
        }
        std::fs::copy(&filled, &board)
            .map_err(|e| CliError::Io(format!("cannot read the filled board: {e}")))?;
        Ok(())
    }

    /// A subdirectory path as `kicad-cli -o` wants it: with a trailing separator.
    fn out_dir(&self, name: &str) -> Result<(PathBuf, String), CliError> {
        let dir = self.dir.join(name);
        std::fs::create_dir_all(&dir)
            .map_err(|e| CliError::Io(format!("cannot create {}: {e}", dir.display())))?;
        let mut arg = self.arg(&dir)?;
        arg.push(if self.cli.windows() { '\\' } else { '/' });
        Ok((dir, arg))
    }

    fn collect(dir: &Path) -> Result<Vec<Output>, CliError> {
        let io = |e: std::io::Error| CliError::Io(format!("cannot read {}: {e}", dir.display()));
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir).map_err(io)? {
            let entry = entry.map_err(io)?;
            files.push(Output {
                name: entry.file_name().to_string_lossy().into_owned(),
                bytes: std::fs::read(entry.path()).map_err(io)?,
            });
        }
        files.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(files)
    }
}

impl KicadCli for KicadCliExe {
    fn version(&self) -> Result<String, CliError> {
        let ran = exclusive::run(|| {
            let mut command = Command::new(&self.exe);
            command.arg("version");
            command
        })?;
        let said = String::from_utf8_lossy(&ran.output.stdout)
            .trim()
            .to_string();
        if ran.success() && !said.is_empty() {
            Ok(said)
        } else {
            Err(CliError::Failed {
                command: format!("{} version", self.exe.display()),
                output: ran.report(),
            })
        }
    }

    fn drc_json(&self, pcb: &Path) -> Result<String, CliError> {
        let stage = self.stage(pcb)?;
        let report = stage.dir.join("drc.json");
        stage.run(&[
            "drc",
            "--severity-all",
            "--all-track-errors",
            "--format",
            "json",
            "--units",
            "mm",
            "-o",
            &stage.arg(&report)?,
        ])?;
        std::fs::read_to_string(&report)
            .map_err(|e| CliError::Io(format!("kicad-cli wrote no DRC report: {e}")))
    }

    fn gerbers(&self, pcb: &Path, layers: &[String]) -> Result<Vec<Output>, CliError> {
        let stage = self.stage(pcb)?;
        let (dir, arg) = stage.out_dir("gerbers")?;
        stage.run(&[
            "export",
            "gerbers",
            "--layers",
            &layers.join(","),
            "--no-protel-ext",
            "--use-drill-file-origin",
            "-o",
            &arg,
        ])?;
        Stage::collect(&dir)
    }

    fn drill(&self, pcb: &Path) -> Result<Vec<Output>, CliError> {
        let stage = self.stage(pcb)?;
        let (dir, arg) = stage.out_dir("drill")?;
        stage.run(&[
            "export",
            "drill",
            "--format",
            "excellon",
            "--drill-origin",
            "plot",
            "--excellon-units",
            "mm",
            "--excellon-zeros-format",
            "decimal",
            "--excellon-oval-format",
            "route",
            "--excellon-separate-th",
            "-o",
            &arg,
        ])?;
        Stage::collect(&dir)
    }

    fn pos(&self, pcb: &Path) -> Result<String, CliError> {
        let stage = self.stage(pcb)?;
        let file = stage.dir.join("pos.csv");
        stage.run(&[
            "export",
            "pos",
            "--format",
            "csv",
            "--units",
            "mm",
            "--side",
            "both",
            "--use-drill-file-origin",
            "-o",
            &stage.arg(&file)?,
        ])?;
        std::fs::read_to_string(&file)
            .map_err(|e| CliError::Io(format!("kicad-cli wrote no position file: {e}")))
    }

    fn netlist(&self, sch: &Path) -> Result<String, CliError> {
        let io = |what: &str, e: std::io::Error| CliError::Io(format!("{what}: {e}"));
        let stage = self.scratch(
            &sch.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        )?;
        // The root sheet needs its sub-sheets, its project and its symbols beside it.
        let from_dir = match sch.parent() {
            Some(p) if !p.as_os_str().is_empty() => p,
            _ => Path::new("."),
        };
        let entries = std::fs::read_dir(from_dir)
            .map_err(|e| io(&format!("cannot read {}", from_dir.display()), e))?;
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let wanted = path.is_file()
                && (["kicad_sch", "kicad_pro", "kicad_sym"]
                    .iter()
                    .any(|e| path.extension().is_some_and(|x| x == *e))
                    || name == "sym-lib-table");
            if wanted {
                std::fs::copy(&path, stage.dir.join(&name))
                    .map_err(|e| io(&format!("cannot copy {}", path.display()), e))?;
            }
        }
        let file = stage.dir.join("netlist.xml");
        stage.run_group(
            "sch",
            &[
                "export",
                "netlist",
                "--format",
                "kicadxml",
                "-o",
                &stage.arg(&file)?,
            ],
        )?;
        std::fs::read_to_string(&file)
            .map_err(|e| CliError::Io(format!("kicad-cli wrote no netlist: {e}")))
    }
}

#[cfg(test)]
mod wslpath_tests {
    use super::wslpath_failed;

    #[test]
    fn a_failed_wslpath_names_the_path_it_was_asked_about_and_what_it_said() {
        // `wslpath` printed nothing on stdout: the message must still say which path.
        let said = wslpath_failed(
            "-w",
            "/tmp/a b/board.kicad_pcb",
            b"",
            b"wslpath: no such file\n",
        )
        .to_string();
        assert!(
            said.contains(r#"wslpath -w "/tmp/a b/board.kicad_pcb""#),
            "{said}"
        );
        assert!(said.contains("wslpath: no such file"), "{said}");
        assert!(!said.contains(r#"wslpath -w """#), "{said}");
        // What it printed on stdout is in the output, not in the command.
        let said = wslpath_failed("-u", "C:\\x", b"partial\n", b"").to_string();
        assert!(
            said.contains(r#"wslpath -u "C:\\x""#) && said.contains("partial"),
            "{said}"
        );
    }
}
