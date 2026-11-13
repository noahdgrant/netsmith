use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args as ClapArgs, ValueEnum};

use super::common::{DEFAULT_PORT, Ids, ViewName, parse_ids};

#[derive(Debug, PartialEq, ClapArgs)]
#[command(args_override_self = true)]
#[command(
    after_long_help = "`view` drives the 3D view of a running editor (start `netsmith editor` and open its page; docs/BOARD3D.md, \"Driving a running editor\") with the names of `render`, so that an agent changes the verb to switch between the two. With no flag that changes the page it prints the state of the page (a short summary, or with `--json` the editor's JSON as it came); with any, it changes the page, which draws it, and prints the state that results. The page driven is the one focused last.\n\nA change the page refuses (an id the board does not have, a section, a preview height) exits 2 with the editor's message, as does an editor that is not running or has no page open. There is no `--theme`: the theme is the person's own setting for the whole editor (the state reports it). Nothing changes the design or a file."
)]
pub struct Args {
    /// The editor's port.
    #[arg(long, default_value_t = DEFAULT_PORT, value_parser = clap::value_parser!(u16).range(1..))]
    pub port: u16,
    /// The board (`Name`, or `System/member`), or a system (`System`) for its System view.
    ///
    /// A system's name shows every member placed by its mates (ADR 0110), as `render --board
    /// System` draws it; the ids of `--highlight` and `--select` are then a member's (`led/J1`,
    /// `net:led/GND`).
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// Where the camera looks from.
    #[arg(long, value_enum)]
    pub view: Option<ViewName>,
    /// Cut the scene as `render --section` does, or `none` for no cut.
    #[arg(long, value_name = "AXIS=MM|none", value_parser = parse_view_section)]
    pub section: Option<String>,
    /// Light a part, `net:<name>`, `limit:<name>` or `case:<name>` (a case model), comma-separated
    /// or repeated; `none` for nothing.
    ///
    /// In the System view a part and a net are a member's: `led/J1`, `net:led/GND`; a case model
    /// is one board's, and the System view refuses `case:`. An id the board (or the system) does
    /// not have exits 2 with the list of the ones it has.
    #[arg(long, value_name = "ID[,ID...]|none", value_parser = parse_ids, conflicts_with = "select")]
    pub highlight: Vec<Ids>,
    /// Select parts or one `net:<name>`, as a click does (comma-separated or repeated); `none` clears it.
    ///
    /// The editor's selection holds parts or one net, so `--select` takes no limit and no case
    /// model; in the System view, of one member (`led/J1`), which becomes the page's board.
    #[arg(long, value_name = "ID[,ID...]|none", value_parser = parse_select)]
    pub select: Vec<Ids>,
    /// Draw everything see-through.
    #[arg(long, overrides_with = "no_xray")]
    pub xray: bool,
    /// Draw the bodies solid.
    #[arg(long, overrides_with = "xray")]
    pub no_xray: bool,
    /// The parallel projection.
    #[arg(long, conflicts_with = "perspective")]
    pub ortho: bool,
    /// The perspective projection.
    #[arg(long)]
    pub perspective: bool,
    /// What the bodies are coloured by.
    #[arg(long = "colour-by", alias = "color-by", value_enum)]
    pub colour_by: Option<ColourByName>,
    /// The height preview: a height in mm from 0.1 to 50, `on` or `off`.
    #[arg(long, value_name = "MM|on|off", value_parser = parse_height_preview)]
    pub height_preview: Option<netsmith_editor::api::PreviewSetting>,
    /// Pull the stackup apart.
    #[arg(long, overrides_with = "no_layers_apart")]
    pub layers_apart: bool,
    /// Put the stackup together again.
    #[arg(long, overrides_with = "layers_apart")]
    pub no_layers_apart: bool,
    /// Print the editor's JSON as it came, not the summary.
    #[arg(long)]
    pub json: bool,
    /// Save what the page's canvas shows after the change.
    ///
    /// It opens the 3D view if it is not on screen.
    #[arg(long, value_name = "FILE.PNG")]
    pub picture: Option<PathBuf>,
    /// Refused with the reason: the theme is the person's own setting.
    #[arg(long, hide = true)]
    pub theme: Option<String>,
    /// Refused with the reason: `view` saves with `--picture`.
    #[arg(long, hide = true)]
    pub out: Option<String>,
    /// Refused with the reason: `view` drives the editor's open project.
    #[arg(hide = true)]
    pub file: Option<String>,
}

impl Args {
    /// The request to the editor, or the mistake clap cannot see: an id with `none`, and the
    /// flags of `render` that `view` has not.
    pub fn request(&self) -> Result<crate::view_client::Request, String> {
        if self.theme.is_some() {
            return Err("`view` has no --theme: the theme is the person's own setting for the whole editor, not a view of the board (the state says which it is); `render --theme light|dark` draws a picture in either".to_string());
        }
        if self.out.is_some() {
            return Err(
                "`view` saves the page's picture with --picture <file.png>, not --out".to_string(),
            );
        }
        if let Some(file) = &self.file {
            return Err(format!(
                "`view` takes no file: it drives the editor's open project (`--board` says which board), not a source file; `netsmith render {file}` draws one without an editor"
            ));
        }
        let switch = |on: bool, off: bool| (on || off).then_some(on);
        let change = netsmith_editor::api::ViewChange {
            board: self.board.clone(),
            view: self.view.map(ViewName::editor),
            section: self.section.clone(),
            highlight: joined_ids("--highlight", &self.highlight)?,
            select: joined_ids("--select", &self.select)?,
            xray: switch(self.xray, self.no_xray),
            ortho: switch(self.ortho, self.perspective),
            colour_by: self.colour_by.map(ColourByName::editor),
            height_preview: self.height_preview,
            exploded: switch(self.layers_apart, self.no_layers_apart),
            ..Default::default()
        };
        Ok(crate::view_client::Request {
            port: self.port,
            change,
            json: self.json,
            picture: self.picture.clone(),
        })
    }
}

/// Reads or changes the 3D view of a running editor's page, and saves its picture.
pub fn run(args: Args) -> ExitCode {
    match args.request() {
        Ok(request) => run_view(&request),
        // A mistake on the command line, as clap prints one: exit 2.
        Err(e) => {
            clap::Error::raw(clap::error::ErrorKind::ValueValidation, format!("{e}\n")).exit()
        }
    }
}

/// `netsmith view`: read or change the 3D view of a running editor's page. A refusal (the
/// editor's own message), no editor and no page open all exit 2.
fn run_view(request: &crate::view_client::Request) -> ExitCode {
    match crate::view_client::run(request) {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(2)
        }
    }
}

fn parse_select(given: &str) -> Result<Ids, String> {
    let ids = parse_ids(given)?;
    if let Some(kind) = ids
        .0
        .iter()
        .find_map(|id| ["limit:", "case:"].into_iter().find(|k| id.starts_with(k)))
    {
        return Err(format!(
            "takes parts or one net, as a click does; a `{kind}` is lit with --highlight"
        ));
    }
    Ok(ids)
}

/// The ids of every `--highlight` (or `--select`): `None` when none is given, an empty list for
/// `none`, which no id may join.
fn joined_ids(flag: &str, given: &[Ids]) -> Result<Option<Vec<String>>, String> {
    let all: Vec<String> = given.iter().flat_map(|ids| ids.0.clone()).collect();
    if all.iter().any(|id| id == "none") {
        if all.iter().any(|id| id != "none") {
            return Err(format!("{flag} `none` clears it, so no id goes with it"));
        }
        return Ok(Some(Vec::new()));
    }
    Ok(if all.is_empty() { None } else { Some(all) })
}

fn parse_view_section(given: &str) -> Result<String, String> {
    if given.trim() == "none" {
        return Ok("none".to_string());
    }
    netsmith::render::parse_section(given)?;
    Ok(given.trim().to_string())
}

fn parse_height_preview(given: &str) -> Result<netsmith_editor::api::PreviewSetting, String> {
    use netsmith_editor::api::PreviewSetting;
    match given {
        "on" => Ok(PreviewSetting::On(true)),
        "off" => Ok(PreviewSetting::On(false)),
        mm => match mm.parse::<f64>() {
            Ok(v) if (0.1..=50.0).contains(&v) => Ok(PreviewSetting::Mm(v)),
            _ => Err(format!(
                "takes a height in mm from 0.1 to 50, `on` or `off`, not `{given}`"
            )),
        },
    }
}

/// `view --colour-by`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColourByName {
    /// Where each height came from.
    Provenance,
    /// The part's kind.
    Kind,
    /// The margin to the height limit over it.
    Margin,
}

impl ColourByName {
    fn editor(self) -> netsmith_editor::api::ColourBy {
        use netsmith_editor::api::ColourBy;
        match self {
            ColourByName::Provenance => ColourBy::Provenance,
            ColourByName::Kind => ColourBy::Kind,
            ColourByName::Margin => ColourBy::Margin,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::commands::common::testing::parse;
    use crate::commands::{Command, common};
    use std::path::PathBuf;

    /// The error of a `view` line: clap's, or the one its request gives.
    fn view_err(line: &[&str]) -> String {
        match parse(line) {
            Err(e) => e,
            Ok(Command::View(v)) => v.request().unwrap_err(),
            Ok(other) => panic!("{other:?}"),
        }
    }

    fn view_request(args: &[&str]) -> crate::view_client::Request {
        let mut line = vec!["view"];
        line.extend_from_slice(args);
        match parse(&line) {
            Ok(Command::View(v)) => v.request().unwrap(),
            other => panic!("{args:?}: {other:?}"),
        }
    }

    #[test]
    fn view_with_no_flag_reads_the_state_of_the_editor_on_the_default_port() {
        let request = view_request(&[]);
        assert_eq!(request.port, common::DEFAULT_PORT);
        assert!(!request.changes() && !request.json && request.picture.is_none());
        assert_eq!(request.change, netsmith_editor::api::ViewChange::default());
    }

    #[test]
    fn view_takes_the_flags_of_render_and_the_editors_own() {
        use netsmith_editor::api::{ColourBy, PreviewSetting, RenderView};
        let request = view_request(&[
            "--port",
            "9001",
            "--board",
            "Demo/main",
            "--view",
            "iso-bottom",
            "--section",
            "x=12.5",
            "--highlight",
            "J1,net:GND",
            "--highlight",
            "limit:lid,case:lid",
            "--xray",
            "--perspective",
            "--colour-by",
            "margin",
            "--height-preview",
            "2.5",
            "--layers-apart",
            "--json",
            "--picture",
            "j1.png",
        ]);
        let change = &request.change;
        assert_eq!(request.port, 9001);
        assert_eq!(change.board.as_deref(), Some("Demo/main"));
        assert_eq!(change.view, Some(RenderView::IsoBottom));
        assert_eq!(change.section.as_deref(), Some("x=12.5"));
        assert_eq!(
            change.highlight,
            Some(vec![
                "J1".into(),
                "net:GND".into(),
                "limit:lid".into(),
                "case:lid".into()
            ])
        );
        assert_eq!(change.xray, Some(true));
        assert_eq!(change.ortho, Some(false));
        assert_eq!(change.colour_by, Some(ColourBy::Margin));
        assert_eq!(change.height_preview, Some(PreviewSetting::Mm(2.5)));
        assert_eq!(change.exploded, Some(true));
        assert!(request.json && request.changes());
        assert_eq!(request.picture, Some(PathBuf::from("j1.png")));
        // The reverse of each switch, and the other spelling of colour.
        let request = view_request(&[
            "--no-xray",
            "--ortho",
            "--no-layers-apart",
            "--color-by",
            "kind",
            "--height-preview",
            "off",
            "--section",
            "none",
            "--select",
            "R1",
        ]);
        let change = &request.change;
        assert_eq!(change.xray, Some(false));
        assert_eq!(change.ortho, Some(true));
        assert_eq!(change.exploded, Some(false));
        assert_eq!(change.colour_by, Some(ColourBy::Kind));
        assert_eq!(change.height_preview, Some(PreviewSetting::On(false)));
        assert_eq!(change.section.as_deref(), Some("none"));
        assert_eq!(change.select, Some(vec!["R1".into()]));
    }

    #[test]
    fn view_switches_and_their_reverses_are_the_last_one_given() {
        let change =
            view_request(&["--xray", "--no-xray", "--no-layers-apart", "--layers-apart"]).change;
        assert_eq!((change.xray, change.exploded), (Some(false), Some(true)));
        assert_eq!(
            view_request(&["--no-xray", "--xray"]).change.xray,
            Some(true)
        );
    }

    #[test]
    fn view_says_none_to_light_or_select_nothing() {
        assert_eq!(
            view_request(&["--highlight", "none"]).change.highlight,
            Some(Vec::new())
        );
        assert_eq!(
            view_request(&["--select", "none"]).change.select,
            Some(Vec::new())
        );
        for args in [
            &["view", "--highlight", "none,J1"][..],
            &["view", "--highlight", "none", "--highlight", "J1"],
            &["view", "--select", "J1", "--select", "none"],
        ] {
            let e = view_err(args);
            assert!(e.contains("clears it"), "{args:?}: {e}");
        }
    }

    #[test]
    fn a_picture_alone_still_changes_the_page_so_that_the_view_is_open() {
        let request = view_request(&["--picture", "a.png"]);
        assert!(request.changes());
        assert_eq!(request.change, netsmith_editor::api::ViewChange::default());
    }

    #[test]
    fn view_mistakes_say_what_is_wrong() {
        for (args, says) in [
            (&["--theme", "dark"][..], "person's own setting"),
            (&["--view", "sideways"], "iso-bottom"),
            (&["--view"], "--view"),
            (&["--section", "w=1"], "--section needs"),
            (&["--section", "x"], "--section needs"),
            (&["--ortho", "--perspective"], "cannot be used with"),
            (
                &["--highlight", "J1", "--select", "J1"],
                "cannot be used with",
            ),
            (&["--select", "limit:lid"], "--highlight"),
            (
                &["--select", "J1,case:lid"],
                "a `case:` is lit with --highlight",
            ),
            (&["--colour-by", "shade"], "provenance"),
            (&["--height-preview", "0"], "0.1 to 50"),
            (&["--height-preview", "51"], "0.1 to 50"),
            (&["--height-preview", "tall"], "0.1 to 50"),
            (&["--port", "0"], "1..=65535"),
            (&["--port", "http"], "http"),
            (&["--picture"], "--picture"),
            (&["--out", "a.png"], "--picture"),
            (&["--size", "100x100"], "--size"),
            (&["examples/led.nsm"], "takes no file"),
        ] {
            let mut line = vec!["view"];
            line.extend_from_slice(args);
            let e = view_err(&line);
            assert!(e.contains(says), "{args:?}: {e}");
        }
    }
}
