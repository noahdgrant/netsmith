//! The schematic as a file to save (`GET /api/schematic/export`, issue #1091): the drawing the
//! schematic view shows, written by `netsmith_view::export` as `netsmith view schematic` and
//! `board fab` write it, so the three give the same bytes.

use netsmith_editor::{ApiError, Attachment};
use netsmith_fabricate::schematic::{self, Format, export};

use super::Workspace;

impl Workspace {
    pub(super) fn schematic_export_of(
        &self,
        board: &str,
        format: &str,
    ) -> Result<Attachment, ApiError> {
        let format = match format {
            "pdf" => Format::Pdf,
            "svg" => Format::Svg,
            "png" => Format::Png,
            other => {
                return Err(ApiError::bad_request(format!(
                    "a schematic is exported as `pdf`, `svg` or `png`, not `{other}`"
                )));
            }
        };
        let state = self.fresh();
        let snapshot = &state.snapshot;
        let svg_of = |label: &str| -> Result<String, ApiError> {
            snapshot
                .board(label)?
                .svg
                .clone()
                .map_err(ApiError::internal)
        };
        // A system: its own sheet, then each member's; a picture of it is its own sheet.
        let svgs = match snapshot.system(board) {
            Some(system) => {
                let mut svgs = vec![schematic::system_svg(&system.solved)];
                if format == Format::Pdf {
                    for (member, _, _) in &system.solved.boards {
                        svgs.push(svg_of(&format!("{board}/{member}"))?);
                    }
                }
                svgs
            }
            None => vec![svg_of(board)?],
        };
        let target = self.target();
        let heading = schematic::heading(
            target.project.as_ref().map(|p| p.manifest.name.as_str()),
            target
                .project
                .as_ref()
                .and_then(|p| p.manifest.revision.as_deref()),
        );
        let title = format!("{board} schematic");
        let bytes = match format {
            Format::Pdf => {
                let svgs: Vec<&str> = svgs.iter().map(String::as_str).collect();
                export::pdf(&title, &svgs, &heading)
            }
            Format::Svg => export::svg_of(&svgs[0], None).map(String::into_bytes),
            Format::Png => export::png(&svgs[0], None),
        }
        .map_err(|e| ApiError::internal(e.to_string()))?;
        let file: String = board
            .chars()
            .map(|c| if c == '/' { '-' } else { c })
            .collect();
        Ok(Attachment {
            name: format!("{file}-schematic.{}", format.extension()),
            content_type: format.media_type().to_string(),
            bytes,
        })
    }
}
