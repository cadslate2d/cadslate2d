//! OPEN, SAVE, and DXFOUT: file commands that need the host.
//!
//! A browser file picker or download cannot be driven from Rust, and Rust
//! never calls into JavaScript (ADR-0002). These commands therefore only
//! record a request; the host carries it out and reports back through
//! `Session::open_drawing` or `Session::mark_exported`.

use crate::{Command, CommandContext, HostRequest, Step};

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
/// (DESIGN §25.3). What "can" means is the host's to know.
#[derive(Debug)]
pub(crate) struct SaveCommand;

impl Command for SaveCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        ctx.request_host(HostRequest::SaveFile);
        Step::Finished
    }

    immediate_command!();
}

#[derive(Debug)]
pub(crate) struct ExportCommand;

impl Command for ExportCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        ctx.request_host(HostRequest::ExportFile);
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
    use crate::{Dirty, HostRequest, KeyAction};

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
            ("OPEN", HostRequest::OpenFile),
            ("dxfin", HostRequest::OpenFile),
            ("DXFOUT", HostRequest::ExportFile),
            ("save", HostRequest::SaveFile),
            ("qsave", HostRequest::SaveFile),
            ("SAVEAS", HostRequest::ExportFile),
            ("export", HostRequest::ExportFile),
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
        h.session
            .open_drawing(Drawing::new(), "empty.dxf", Vec::new(), Vec::new());
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

        let dirty = h.session.mark_exported(None);
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
        h.session
            .open_drawing(imported(), "mixed.dxf", report.clone(), losses.clone());
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
        h.session.mark_exported(None);
        h.session.dismiss_import_report();
        let state = h.session.ui_state();
        assert_eq!(state.export_losses, losses);
        assert_eq!(state.import_report, None);

        // Another drawing: its own account, a new serial, no losses.
        h.session.open_drawing(
            imported(),
            "clean.dxf",
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
}
