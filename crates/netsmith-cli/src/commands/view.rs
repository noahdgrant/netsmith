/// `netsmith view`: read or change the 3D view of a running editor's page. A refusal (the
/// editor's own message), no editor and no page open all exit 2.
fn run_view(request: &netsmith::view_client::Request) -> ExitCode {
    match netsmith::view_client::run(request) {
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
    if ids.0.iter().any(|id| id.starts_with("limit:")) {
        return Err(
            "takes parts or one net, as a click does; a `limit:` is lit with --highlight"
                .to_string(),
        );
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

/// Drive the 3D view of a running editor, as `render` flags say, and save its picture.
///
/// `view` drives the 3D view of a running editor (start `netsmith editor` and open its page;
/// docs/BOARD3D.md, "Driving a running editor") with the names of `render`, so that an agent
/// changes the verb to switch between the two. With no flag that changes the page it prints the
/// state of the page (a short summary, or with `--json` the editor's JSON as it came); with any,
/// it changes the page, which draws it, and prints the state that results. The page driven is
/// the one focused last.
///
/// A change the page refuses (an id the board does not have, a section, a preview height) exits
/// 2 with the editor's message, as does an editor that is not running or has no page open.
/// There is no `--theme`: the theme is the person's own setting for the whole editor (the state
/// reports it). Nothing changes the design or a file.
#[derive(Debug, PartialEq, Args)]
#[command(args_override_self = true)]
pub struct ViewArgs {
    /// The editor's port.
    #[arg(long, default_value_t = DEFAULT_PORT, value_parser = clap::value_parser!(u16).range(1..))]
    pub port: u16,
    /// The board (`Name`, or `System/member`).
    #[arg(long, value_name = "NAME")]
    pub board: Option<String>,
    /// Where the camera looks from, as with `render`.
    #[arg(long, value_enum)]
    pub view: Option<ViewName>,
    /// Cut the scene as `render --section` does, or `none` for no cut.
    #[arg(long, value_name = "AXIS=MM|none", value_parser = parse_view_section)]
    pub section: Option<String>,
    /// Light a part, `net:<name>` or `limit:<name>` (comma-separated or repeated); `none` lights nothing.
    #[arg(long, value_name = "ID[,ID...]|none", value_parser = parse_ids, conflicts_with = "select")]
    pub highlight: Vec<Ids>,
    /// Select parts or one `net:<name>`, as a click does (comma-separated or repeated); `none` clears it.
    ///
    /// The editor's selection holds parts or one net, so `--select` takes no limit.
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

impl ViewArgs {
    /// The mistakes clap cannot say: an id with `none`, and the flags of `render` that `view`
    /// has not.
    fn check(&self) -> Result<(), String> {
        joined_ids("--highlight", &self.highlight)?;
        joined_ids("--select", &self.select)?;
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
        Ok(())
    }

    /// The request to the editor. [`parse_args`] has made the same checks.
    pub fn request(&self) -> Result<crate::view_client::Request, String> {
        self.check()?;
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

fn parse_view_section(given: &str) -> Result<String, String> {
    if given.trim() == "none" {
        return Ok("none".to_string());
    }
    crate::render::parse_section(given)?;
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
