//! NEW, OPEN, SAVE, SAVEAS, and DXFOUT: file commands that need the host.
//!
//! A browser file picker or download cannot be driven from Rust, and Rust
//! never calls into JavaScript (ADR-0002). These commands therefore only
//! record a request; the host carries it out and reports back through
//! `Session::open_drawing` or `Session::mark_exported`.

use cad_input::{ExpectedInput, InputKind, Keyword};

use crate::{Command, CommandContext, CommandInput, FileFormat, HostRequest, Prompt, Step};

const DXF: Keyword = Keyword::new("DXF");
const DWG2000: Keyword = Keyword::new("DWG2000");
const DWG2004: Keyword = Keyword::new("DWG2004");
const DWG2007: Keyword = Keyword::new("DWG2007");
/// The formats SAVEAS offers, in `FileFormat::ALL`'s order.
const FORMATS: [Keyword; 4] = [DXF, DWG2000, DWG2004, DWG2007];

fn format_of(keyword: Keyword) -> FileFormat {
    FORMATS
        .iter()
        .zip(FileFormat::ALL)
        .find(|(k, _)| **k == keyword)
        .map_or(FileFormat::Dxf, |(_, format)| format)
}

fn keyword_of(format: FileFormat) -> Keyword {
    FORMATS[FileFormat::ALL
        .iter()
        .position(|f| *f == format)
        .unwrap_or(0)]
}

/// NEW (#245): an empty drawing. Whether unsaved work may go is the
/// host's question to ask, as it is for OPEN.
#[derive(Debug)]
pub(crate) struct NewCommand;

impl Command for NewCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        ctx.request_host(HostRequest::NewDrawing);
        Step::Finished
    }

    immediate_command!();
}

#[derive(Debug)]
pub(crate) struct OpenCommand;

impl Command for OpenCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        ctx.request_host(HostRequest::OpenFile);
        Step::Finished
    }

    immediate_command!();
}

/// SAVE: back to the file where the browser can, asking where otherwise
/// (DESIGN §25.3), in the format the file came in (#319). What "can"
/// means is the host's to know.
#[derive(Debug)]
pub(crate) struct SaveCommand;

impl Command for SaveCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        ctx.request_host(HostRequest::SaveFile);
        Step::Finished
    }

    immediate_command!();
}

/// SAVEAS and EXPORT (#319): the format — DXF, or DWG 2000, 2004, or 2007
/// — then the host asks where. Enter keeps the format the file came in.
#[derive(Debug, Default)]
pub(crate) struct ExportCommand {
    /// The format the file came in: the default.
    current: FileFormat,
}

impl Command for ExportCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.current = ctx.document_format;
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Save as").with_default(keyword_of(self.current).name())
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::new(InputKind::Nothing).with_keywords(&FORMATS)
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let format = match input {
            CommandInput::Keyword(keyword) => format_of(keyword),
            CommandInput::Accept => self.current,
            _ => return Step::Continue,
        };
        ctx.request_host(HostRequest::ExportFile(format));
        Step::Finished
    }
}

/// DXFOUT: what its name says, with no question asked.
#[derive(Debug)]
pub(crate) struct DxfOutCommand;

impl Command for DxfOutCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        ctx.request_host(HostRequest::ExportFile(FileFormat::Dxf));
        Step::Finished
    }

    immediate_command!();
}

#[cfg(test)]
mod tests {
    use crate::ImportReport;
    use cad_core::Point2;
    use cad_document::{Drawing, Entity, LineEntity};

    use crate::testing::Harness;
    use crate::{Dirty, FileFormat, HostRequest, KeyAction};

    fn imported() -> Drawing {
        Drawing::from_entities([
            Entity::Line(LineEntity::new(
                Point2::new(1000.0, 1000.0),
                Point2::new(1200.0, 1000.0),
            )),
            Entity::Line(LineEntity::new(
                Point2::new(1200.0, 1000.0),
                Point2::new(1200.0, 1100.0),
            )),
        ])
    }

    #[test]
    fn file_commands_raise_a_host_request_instead_of_acting() {
        let cases = [
            ("NEW", HostRequest::NewDrawing),
            ("qnew", HostRequest::NewDrawing),
            ("OPEN", HostRequest::OpenFile),
            ("dxfin", HostRequest::OpenFile),
            ("DXFOUT", HostRequest::ExportFile(FileFormat::Dxf)),
            ("save", HostRequest::SaveFile),
            ("qsave", HostRequest::SaveFile),
        ];
        for (text, expected) in cases {
            let mut h = Harness::new();
            let dirty = h.enter(text);
            assert!(dirty.contains(Dirty::HOST), "{text}");
            assert_eq!(h.session.take_host_request(), Some(expected), "{text}");
            assert_eq!(
                h.session.take_host_request(),
                None,
                "a request is taken once"
            );
            assert_eq!(h.prompt(), "Command:");
        }
    }

    /// #245: a new drawing is empty, untitled, saved, and without history,
    /// in the old one's unit, seen at one pixel per unit about the origin;
    /// nothing tied to the old one survives.
    #[test]
    fn a_new_drawing_is_empty_untitled_and_keeps_the_unit() {
        let drawing = imported().with_unit(Some(cad_core::LengthUnit::Millimetres));
        let mut h = Harness::with_drawing(drawing);
        h.session.mark_exported(Some("plan.dxf"), FileFormat::Dxf);
        h.enter_all(&["L", "0,0", "10,0", ""]);
        h.enter("ALL");
        h.enter("MOVE");
        let dirty = h.session.new_drawing();
        assert!(dirty.contains(Dirty::SCENE | Dirty::VIEW | Dirty::UI));
        let drawing = h.session.drawing();
        assert!(drawing.is_empty());
        assert!(!drawing.can_undo());
        assert_eq!(drawing.unit(), Some(cad_core::LengthUnit::Millimetres));
        let state = h.session.ui_state();
        assert_eq!(state.document_name, None);
        assert!(!state.unsaved_changes);
        assert_eq!(state.selection_count, 0);
        assert_eq!(h.prompt(), "Command:", "the command in progress is gone");
        assert_eq!(h.session.view().center(), Point2::ORIGIN);
        assert_eq!(h.session.view().scale(), 1.0);
    }

    #[test]
    fn shortcuts_invoke_the_same_commands() {
        let mut h = Harness::new();
        assert!(h.session.key(KeyAction::Open).contains(Dirty::HOST));
        assert_eq!(h.session.take_host_request(), Some(HostRequest::OpenFile));
        assert!(h.session.key(KeyAction::Save).contains(Dirty::HOST));
        assert_eq!(h.session.take_host_request(), Some(HostRequest::SaveFile));
    }

    #[test]
    fn ordinary_input_never_raises_a_host_request() {
        let mut h = Harness::new();
        for line in ["L", "0,0", "10,0", "", "U", "nonsense"] {
            assert!(!h.enter(line).contains(Dirty::HOST), "{line}");
        }
        assert_eq!(h.session.take_host_request(), None);
    }

    #[test]
    fn opening_a_drawing_replaces_everything_and_frames_it() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "50,0", ""]);
        h.click(400.0, 300.0);
        h.enter_all(&["L", "5,5"]); // a command in progress

        let dirty = h.session.open_drawing(
            imported(),
            "plan.dxf",
            FileFormat::Dxf,
            vec!["Imported 2 lines.".to_owned()],
            Vec::new(),
        );

        assert!(dirty.contains(Dirty::SCENE | Dirty::VIEW | Dirty::OVERLAY | Dirty::UI));
        assert_eq!(h.lines().len(), 2);
        assert_eq!(h.prompt(), "Command:", "the active command is dropped");
        assert!(h.session.selection().is_empty());
        assert_eq!(h.log().last().unwrap(), "Imported 2 lines.");

        let state = h.session.ui_state();
        assert_eq!(state.document_name.as_deref(), Some("plan.dxf"));
        assert!(!state.unsaved_changes && state.export_losses.is_empty() && !state.can_undo);

        // "Zoom extents": the whole drawing is visible.
        let extents = h.session.drawing().extents().unwrap();
        assert!(h.session.view().visible_bounds().contains(&extents));
    }

    #[test]
    fn relative_input_starts_from_the_origin_again_after_opening() {
        let mut h = Harness::new();
        h.enter_all(&["L", "500,500", ""]);
        h.session.open_drawing(
            Drawing::new(),
            "empty.dxf",
            FileFormat::Dxf,
            Vec::new(),
            Vec::new(),
        );
        h.enter_all(&["L", "@1,1", "@1,0", ""]);
        assert_eq!(h.lines()[0].start, Point2::new(1.0, 1.0));
    }

    #[test]
    fn unsaved_changes_track_edits_and_exports() {
        let mut h = Harness::new();
        assert!(
            !h.session.ui_state().unsaved_changes,
            "a new drawing has nothing to lose"
        );

        h.enter_all(&["L", "0,0", "10,0", ""]);
        assert!(h.session.ui_state().unsaved_changes);

        let dirty = h.session.mark_exported(None, FileFormat::Dxf);
        assert!(dirty.contains(Dirty::UI));
        assert!(!h.session.ui_state().unsaved_changes);
        assert_eq!(h.log().last().unwrap(), "Drawing exported as DXF.");

        h.undo();
        assert!(
            h.session.ui_state().unsaved_changes,
            "undo after export is a change too"
        );
    }

    /// DESIGN §24.2: the user is warned before a potentially lossy export.
    #[test]
    fn a_lossy_import_is_remembered_until_another_drawing_is_opened() {
        let mut h = Harness::new();
        let losses = vec!["Not imported (unsupported): 1 HATCH.".to_owned()];
        let report = vec!["Imported 1 line.".to_owned(), losses[0].clone()];
        h.session.open_drawing(
            imported(),
            "mixed.dxf",
            FileFormat::Dxf,
            report.clone(),
            losses.clone(),
        );
        let state = h.session.ui_state();
        assert_eq!(state.export_losses, losses);
        assert_eq!(
            state.import_report,
            Some(ImportReport {
                serial: 1,
                lines: report
            })
        );
        assert_eq!(
            h.log()[h.log().len() - 2..],
            ["Imported 1 line.", "Not imported (unsupported): 1 HATCH."]
        );

        // Editing and exporting do not make the original content come back;
        // dismissing the report keeps the losses.
        h.enter_all(&["L", "0,0", "1,1", ""]);
        h.session.mark_exported(None, FileFormat::Dxf);
        h.session.dismiss_import_report();
        let state = h.session.ui_state();
        assert_eq!(state.export_losses, losses);
        assert_eq!(state.import_report, None);

        // Another drawing: its own account, a new serial, no losses.
        h.session.open_drawing(
            imported(),
            "clean.dxf",
            FileFormat::Dxf,
            vec!["Imported 1 line.".to_owned()],
            Vec::new(),
        );
        let state = h.session.ui_state();
        assert!(state.export_losses.is_empty());
        assert_eq!(state.import_report.map(|report| report.serial), Some(2));
    }

    /// #158: the host autosaves when the revision changes, and only then.
    #[test]
    fn the_ui_state_carries_the_revision_which_commits_change_and_the_pointer_does_not() {
        let mut h = Harness::new();
        let before = h.session.ui_state().revision;
        h.move_to(100.0, 100.0);
        h.move_to(120.0, 100.0);
        assert_eq!(h.session.ui_state().revision, before);
        h.enter_all(&["L", "0,0", "1,1", ""]);
        let after = h.session.ui_state().revision;
        assert_ne!(after, before);
        h.undo();
        assert_ne!(h.session.ui_state().revision, after, "undo is a change too");
    }

    #[test]
    fn notifications_reach_the_command_log() {
        let mut h = Harness::new();
        let dirty = h
            .session
            .notify("Cannot open broken.dxf: line 1: expected a DXF group code");
        assert!(dirty.contains(Dirty::UI));
        assert_eq!(
            h.log(),
            vec!["Cannot open broken.dxf: line 1: expected a DXF group code"]
        );
    }
    /// #319: SAVEAS asks the format, Enter keeping the file's own; the
    /// request carries the choice. `2mm` is no format.
    #[test]
    fn saveas_asks_the_format_and_enter_keeps_the_files_own() {
        let mut h = Harness::new();
        for (answer, format) in [
            ("DXF", FileFormat::Dxf),
            ("dwg2000", FileFormat::Dwg2000),
            ("DWG2004", FileFormat::Dwg2004),
            ("DWG2007", FileFormat::Dwg2007),
        ] {
            h.enter("SAVEAS");
            assert_eq!(
                h.prompt(),
                "Save as or [DXF/DWG2000/DWG2004/DWG2007] <DXF>:"
            );
            let dirty = h.enter(answer);
            assert!(dirty.contains(Dirty::HOST), "{answer}");
            assert_eq!(
                h.session.take_host_request(),
                Some(HostRequest::ExportFile(format))
            );
        }
        h.enter_all(&["EXPORT", "2mm"]);
        assert_eq!(h.log().last().unwrap(), "An option keyword is required.");
        assert_eq!(h.session.take_host_request(), None);
        h.enter("");
        assert_eq!(
            h.session.take_host_request(),
            Some(HostRequest::ExportFile(FileFormat::Dxf))
        );

        // A drawing that came in as DWG 2004 offers that, and SAVE keeps it.
        h.session.open_drawing(
            imported(),
            "my plan.dwg",
            FileFormat::Dwg2004,
            Vec::new(),
            Vec::new(),
        );
        h.enter("SAVEAS");
        assert_eq!(
            h.prompt(),
            "Save as or [DXF/DWG2000/DWG2004/DWG2007] <DWG2004>:"
        );
        h.enter("");
        assert_eq!(
            h.session.take_host_request(),
            Some(HostRequest::ExportFile(FileFormat::Dwg2004))
        );
        assert_eq!(h.session.document_format(), FileFormat::Dwg2004);
        // Saved as another format, under a name with a space: that is the
        // drawing's format and name from then on.
        h.session
            .mark_exported(Some("my plan 2.dwg"), FileFormat::Dwg2007);
        let state = h.session.ui_state();
        assert_eq!(state.document_name.as_deref(), Some("my plan 2.dwg"));
        assert_eq!(state.document_format, FileFormat::Dwg2007);
        assert_eq!(h.log().last().unwrap(), "Drawing exported as DWG 2007.");
    }
}
