//! `netsmith import footprint` (issue #1424): a land pattern from a vendor's reference board or a
//! distributor's KiCad export, written as a `.kicad_mod` into a footprint library of the project,
//! for a row of `kicad.toml` to name and to check against the datasheet's `table` (#1281).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Args as ClapArgs;
use netsmith_kicad::{FootprintSource, Mapping};

/// The folder of a library that `kicad.toml` does not list yet: `libs/<name>.pretty`.
const LIBRARY_DIR: &str = "libs";

#[derive(Debug, PartialEq, ClapArgs)]
#[command(
    after_long_help = "FILE is a KiCad board (`.kicad_pcb`), whose part `--ref` names, a footprint file (`.kicad_mod`), or a `.pretty` folder that holds one. The footprint is written as it is drawn, its pads, courtyard, silkscreen, fabrication layer and 3D model, without what the board adds: its place and angle on the board are taken off, a footprint on the bottom face is turned back to the top, and the nets, ids, links to the schematic and the symbol's own fields are left out. The reference becomes `REF**` and the value the footprint's name.\n\nThe project is the one that holds the current folder. `--name LIB:NAME` names the library and the footprint; `--name NAME` the footprint alone, in the one footprint library that `kicad.toml` lists. With no `--name` the footprint keeps the name the file gives it. A library that `kicad.toml` does not list is the folder `libs/LIB.pretty`, and the command prints the `[[library]]` stanza to add. It prints a draft `[[part]]` row too, whose `table` is to be filled from the datasheet's package drawing (a `TODO` does not parse, so the row does not build until it is). A footprint file that is already there is not written over: delete it first. A file that cannot be read, a part the board does not have, or no project is exit 2."
)]
pub struct Args {
    /// The board, footprint file or `.pretty` folder to take the footprint from.
    #[arg(value_name = "FILE")]
    pub file: PathBuf,
    /// The reference designator of the part whose footprint a board file gives.
    #[arg(long = "ref", value_name = "REF")]
    pub reference: Option<String>,
    /// The library and the footprint's name, `LIB:NAME`, or the name alone.
    #[arg(long, value_name = "NAME")]
    pub name: Option<String>,
}

pub fn run(args: Args) -> ExitCode {
    match import(&args) {
        Ok(report) => {
            print!("{report}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

/// What the command did, for stdout.
fn import(args: &Args) -> Result<String, String> {
    let (file, board) = source_file(&args.file)?;
    let source = match (board, args.reference.as_deref()) {
        (true, Some(reference)) => FootprintSource::Board { reference },
        (true, None) => {
            return Err(format!(
                "{}: a board holds many footprints: name the part with `--ref REF`",
                file.display()
            ));
        }
        (false, Some(_)) => {
            return Err(format!(
                "{}: `--ref` names a part of a board, and a footprint file has one footprint",
                file.display()
            ));
        }
        (false, None) => FootprintSource::File,
    };
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    let project = netsmith_fabricate::project::Project::find(&cwd)?.ok_or_else(|| {
        format!(
            "{}: no netsmith.toml here or in a folder above: run the command in the project",
            cwd.display()
        )
    })?;
    let root = project.root;
    let mapping = Mapping::for_project(Some(&root)).map_err(|e| e.to_string())?;
    let (library, name) = match args.name.as_deref() {
        Some(given) => match given.split_once(':') {
            Some((library, name)) => (Some(library.to_string()), Some(name.to_string())),
            None => (None, Some(given.to_string())),
        },
        None => (None, None),
    };
    let target = library_folder(&mapping, &root, library.as_deref())?;
    let shown = file.display().to_string();
    let text = fs::read_to_string(&file).map_err(|e| format!("{shown}: {e}"))?;
    let imported = netsmith_kicad::import_footprint(&text, source, name.as_deref())
        .map_err(|e| format!("{shown}: {e}"))?;
    if imported.name.contains(['/', '\\', ':']) || imported.name.starts_with('.') {
        return Err(format!(
            "`{}` is not a footprint name: a name is a file's name, with no `/`, `\\` or `:`; give another with `--name`",
            imported.name
        ));
    }
    // The file is read back as every footprint of a project is when a board is built, so a
    // footprint netsmith cannot read is never written.
    let read = netsmith_pcb::Footprint::parse(&imported.name, &imported.text)
        .map_err(|e| format!("{shown}: the footprint taken out cannot be read: {e}"))?;
    let path = target.folder.join(format!("{}.kicad_mod", imported.name));
    if path.exists() {
        return Err(format!(
            "{} is there already: delete it to import the footprint again, or give another name with `--name`",
            relative(&root, &path)
        ));
    }
    fs::create_dir_all(&target.folder).map_err(|e| format!("{}: {e}", target.folder.display()))?;
    fs::write(&path, &imported.text).map_err(|e| format!("{}: {e}", path.display()))?;

    let pads = read.pad_numbers().len();
    let mut out = format!(
        "wrote {} ({pads} pad{}",
        relative(&root, &path),
        if pads == 1 { "" } else { "s" }
    );
    if imported.from_bottom {
        out.push_str(", turned from the bottom face to the top");
    }
    out.push_str(")\n");
    if imported.name != imported.source_name {
        out.push_str(&format!(
            "the file names it `{}`; it is `{}` here\n",
            imported.source_name, imported.name
        ));
    }
    out.push_str("\nadd to kicad.toml:\n\n");
    if !target.listed {
        out.push_str(&format!(
            "[[library]]\nname = {:?}\nfootprints = {:?}\n\n",
            target.name,
            relative(&root, &target.folder)
        ));
    }
    out.push_str(&format!(
        "[[part]]\nkind = \"TODO\"\npackage = \"TODO\"\ndefault = true\nfootprint = \"{}:{}\"\ntable = \"pads={pads}, pitch=TODO, body=TODO\"\n",
        target.name, imported.name
    ));
    out.push_str(
        "\nfill `table` from the datasheet's package drawing, not from the footprint: `build` measures the footprint against it\n",
    );
    Ok(out)
}

/// The file to read and whether it is a board: a `.pretty` folder gives its one footprint.
fn source_file(given: &Path) -> Result<(PathBuf, bool), String> {
    if given.is_dir() {
        let mut mods: Vec<PathBuf> = fs::read_dir(given)
            .map_err(|e| format!("{}: {e}", given.display()))?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "kicad_mod"))
            .collect();
        mods.sort();
        return match mods.as_slice() {
            [one] => Ok((one.clone(), false)),
            [] => Err(format!(
                "{}: the folder has no .kicad_mod file",
                given.display()
            )),
            many => Err(format!(
                "{}: the folder has {} footprints ({}): name the .kicad_mod file of the one to import",
                given.display(),
                many.len(),
                many.iter()
                    .filter_map(|p| p.file_name()?.to_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        };
    }
    match given.extension().and_then(|x| x.to_str()) {
        Some("kicad_pcb") => Ok((given.to_path_buf(), true)),
        Some("kicad_mod") => Ok((given.to_path_buf(), false)),
        _ => Err(format!(
            "{}: not a KiCad board (`.kicad_pcb`), footprint (`.kicad_mod`) or `.pretty` folder",
            given.display()
        )),
    }
}

/// The library a footprint goes into.
struct Library {
    name: String,
    folder: PathBuf,
    /// Whether `kicad.toml` lists it already.
    listed: bool,
}

fn library_folder(mapping: &Mapping, root: &Path, asked: Option<&str>) -> Result<Library, String> {
    let with_footprints: Vec<_> = mapping
        .libraries()
        .iter()
        .filter_map(|l| Some((l.name.as_str(), l.footprints.as_ref()?)))
        .collect();
    let Some(name) = asked else {
        return match with_footprints.as_slice() {
            [(name, folder)] => Ok(Library {
                name: name.to_string(),
                folder: folder.to_path_buf(),
                listed: true,
            }),
            [] => {
                Err("kicad.toml lists no footprint library: name one with `--name LIB:NAME`".into())
            }
            many => Err(format!(
                "kicad.toml lists {} footprint libraries ({}): name one with `--name LIB:NAME`",
                many.len(),
                many.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
            )),
        };
    };
    if name.is_empty() || name.contains(['/', '\\']) {
        return Err(format!("`{name}` is not a library name"));
    }
    if name == netsmith_kicad::GENERATED_FOOTPRINT_LIBRARY || name == "netsmith" {
        return Err(format!(
            "`{name}` is a library of netsmith's own (generated or standard footprints): give the project's library another name"
        ));
    }
    if let Some((_, folder)) = with_footprints.iter().find(|(n, _)| *n == name) {
        return Ok(Library {
            name: name.to_string(),
            folder: folder.to_path_buf(),
            listed: true,
        });
    }
    if mapping.libraries().iter().any(|l| l.name == name) {
        return Err(format!(
            "kicad.toml lists the library `{name}` with no `footprints` folder: add one to its stanza, or name another library"
        ));
    }
    Ok(Library {
        name: name.to_string(),
        folder: root.join(LIBRARY_DIR).join(format!("{name}.pretty")),
        listed: false,
    })
}

/// `path` as the project names it: relative to its root, with `/`.
fn relative(root: &Path, path: &Path) -> String {
    let shown = path.strip_prefix(root).unwrap_or(path);
    shown
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::Leaf;
    use crate::commands::common::testing::{kind, parse};
    use crate::commands::footprint;
    use clap::error::ErrorKind;

    #[test]
    fn import_footprint_takes_a_file_a_reference_and_a_name() {
        assert_eq!(
            parse(&[
                "import",
                "footprint",
                "ref.kicad_pcb",
                "--ref",
                "U3",
                "--name",
                "own:Driver"
            ]),
            Ok(Leaf::Footprint(footprint::Args {
                file: "ref.kicad_pcb".into(),
                reference: Some("U3".into()),
                name: Some("own:Driver".into()),
            }))
        );
        assert_eq!(
            parse(&["import", "footprint", "a.kicad_mod"]),
            Ok(Leaf::Footprint(footprint::Args {
                file: "a.kicad_mod".into(),
                reference: None,
                name: None,
            }))
        );
        assert_eq!(
            kind(&["import", "footprint"]),
            ErrorKind::MissingRequiredArgument
        );
    }
}
