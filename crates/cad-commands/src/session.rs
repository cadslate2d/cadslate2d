//! The session facade.

use cad_core::{Point2, ScreenPoint, View};
use cad_document::Drawing;
use cad_input::{ExpectedInput, Input};

use crate::history::InputHistory;
use crate::log::CommandLog;
use crate::{
    Command, CommandContext, CommandInput, CommandRegistry, Dirty, KeyAction, Modifiers,
    OverlayItem, PointerButton, Selection, Step, UiState,
};

const IDLE_PROMPT: &str = "Command:";
/// Wheel delta (CSS pixels) to zoom exponent. One 100px notch zooms by ~22%.
const ZOOM_PER_WHEEL_PIXEL: f64 = 0.002;
/// Caps a single wheel event so a runaway delta cannot jump the view.
const MAX_WHEEL_DELTA: f64 = 500.0;

#[derive(Debug)]
struct ActiveCommand {
    name: &'static str,
    command: Box<dyn Command>,
}

/// One open drawing and all interaction state around it.
///
/// Typed text, pointer events, and keys all converge here and reach the
/// active command through the same path, so a clicked point and a typed
/// coordinate are indistinguishable to a command (DESIGN §26.3, criterion 8).
#[derive(Debug)]
pub struct Session {
    drawing: Drawing,
    view: View,
    registry: CommandRegistry,
    active: Option<ActiveCommand>,
    selection: Selection,
    /// The reference for relative coordinates: the last point given to a command.
    last_point: Point2,
    /// Pointer position while it is over the canvas.
    cursor: Option<ScreenPoint>,
    /// Last pointer position while a middle-button pan is in progress.
    pan_anchor: Option<ScreenPoint>,
    log: CommandLog,
    history: InputHistory,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// A session on an empty drawing.
    pub fn new() -> Self {
        Self::with_drawing(Drawing::new())
    }

    /// A session on an existing drawing, for example one just imported.
    pub fn with_drawing(drawing: Drawing) -> Self {
        Self {
            drawing,
            view: View::new(1.0, 1.0),
            registry: CommandRegistry::standard(),
            active: None,
            selection: Selection::default(),
            last_point: Point2::ORIGIN,
            cursor: None,
            pan_anchor: None,
            log: CommandLog::new(),
            history: InputHistory::default(),
        }
    }

    // ── read access ──

    pub fn drawing(&self) -> &Drawing {
        &self.drawing
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    /// The prompt shown on the command line right now.
    pub fn prompt(&self) -> String {
        self.active
            .as_ref()
            .map_or_else(|| IDLE_PROMPT.to_owned(), |active| active.command.prompt())
    }

    pub fn ui_state(&self) -> UiState {
        UiState {
            prompt: self.prompt(),
            command: self.active.as_ref().map(|active| active.name),
            log_total: self.log.total(),
            cursor: self.cursor_world().map(|point| (point.x, point.y)),
            selection_count: self.selection.len(),
            space_accepts: true,
            can_undo: self.drawing.can_undo(),
            can_redo: self.drawing.can_redo(),
        }
    }

    /// Log lines numbered `since` and later, oldest first.
    pub fn log_since(&self, since: u64) -> Vec<String> {
        self.log.since(since).map(str::to_owned).collect()
    }

    /// What should be drawn on top of the committed scene, in draw order.
    /// Purely descriptive; `cad-render` turns it into geometry.
    pub fn overlay(&self) -> Vec<OverlayItem> {
        let mut items: Vec<OverlayItem> =
            self.selection.iter().map(OverlayItem::Highlight).collect();
        if let Some(at) = self.cursor_world() {
            if let Some(active) = &self.active {
                active.command.preview(at, &mut items);
            }
            items.push(OverlayItem::Crosshair { at });
        }
        items
    }

    // ── view and pointer input ──

    /// The canvas changed size, in CSS pixels.
    pub fn resize(&mut self, width: f64, height: f64) -> Dirty {
        self.view.resize(width, height);
        Dirty::VIEW | Dirty::OVERLAY
    }

    pub fn pointer_move(&mut self, at: ScreenPoint, _modifiers: Modifiers) -> Dirty {
        self.cursor = Some(at);
        let mut dirty = Dirty::OVERLAY | Dirty::UI;
        if let Some(previous) = self.pan_anchor.replace_if_some(at) {
            self.view
                .pan_by_pixels(at.x - previous.x, at.y - previous.y);
            dirty |= Dirty::VIEW;
        }
        dirty
    }

    /// The pointer left the canvas: hide the crosshair and any preview.
    pub fn pointer_leave(&mut self) -> Dirty {
        match self.cursor.take() {
            Some(_) => Dirty::OVERLAY | Dirty::UI,
            None => Dirty::NONE,
        }
    }

    pub fn pointer_down(
        &mut self,
        at: ScreenPoint,
        button: PointerButton,
        _modifiers: Modifiers,
    ) -> Dirty {
        self.cursor = Some(at);
        match button {
            PointerButton::Middle => {
                self.pan_anchor = Some(at);
                Dirty::NONE
            }
            PointerButton::Primary if self.expects_point() => {
                self.feed(CommandInput::Point(self.view.screen_to_world(at)))
            }
            PointerButton::Primary | PointerButton::Secondary => Dirty::NONE,
        }
    }

    pub fn pointer_up(
        &mut self,
        _at: ScreenPoint,
        button: PointerButton,
        _modifiers: Modifiers,
    ) -> Dirty {
        if button == PointerButton::Middle {
            self.pan_anchor = None;
        }
        Dirty::NONE
    }

    /// Wheel input at `at`. Negative `delta_y` (wheel up) zooms in about the cursor.
    pub fn wheel(&mut self, at: ScreenPoint, delta_y: f64, _modifiers: Modifiers) -> Dirty {
        if !delta_y.is_finite() || delta_y == 0.0 {
            return Dirty::NONE;
        }
        let delta = delta_y.clamp(-MAX_WHEEL_DELTA, MAX_WHEEL_DELTA);
        self.view
            .zoom_about(at, (-delta * ZOOM_PER_WHEEL_PIXEL).exp());
        Dirty::VIEW | Dirty::OVERLAY | Dirty::UI
    }

    // ── keyboard and command-line input ──

    /// One completed command-line entry (Enter, or Space when it accepts).
    ///
    /// The text is interpreted against what the active command expects, so
    /// the same text can be a command at the idle prompt and an error — or,
    /// later, a keyword — inside a command (DESIGN §13.3).
    pub fn submit_text(&mut self, text: &str) -> Dirty {
        let echo = format!("{} {}", self.prompt(), text.trim());
        self.log.push(echo.trim_end().to_owned());
        self.history.record(text);

        let expected = self
            .active
            .as_ref()
            .map_or(ExpectedInput::Command, |a| a.command.expected());
        let dirty = match cad_input::resolve(text, expected, self.registry.aliases()) {
            Ok(Input::Command(name)) => self.invoke(&name),
            Ok(Input::Point(point)) => {
                self.feed(CommandInput::Point(point.resolve(self.last_point)))
            }
            Ok(Input::Accept) => self.feed(CommandInput::Accept),
            Err(diagnostic) => {
                // The command stays active and prompts again.
                self.log.push(diagnostic.message);
                Dirty::NONE
            }
        };
        dirty | Dirty::UI
    }

    /// Starts a command by name or alias, cancelling any active command first.
    ///
    /// This is the single entry point for typed commands, shortcuts, and UI
    /// actions, so none of them can bypass the command architecture (§15).
    pub fn invoke(&mut self, name: &str) -> Dirty {
        let created = self
            .registry
            .aliases()
            .resolve(name)
            .and_then(|canonical| self.registry.create(canonical));
        let Some((name, command)) = created else {
            self.log
                .push(format!("Unknown command \"{}\".", name.trim()));
            return Dirty::UI;
        };
        let cancelled = self.cancel_active();
        cancelled
            | self.advance(ActiveCommand { name, command }, |command, ctx| {
                command.start(ctx)
            })
    }

    pub fn key(&mut self, action: KeyAction) -> Dirty {
        match action {
            KeyAction::Escape => self.escape(),
            // Shortcuts are just another way to invoke a command.
            KeyAction::Undo => self.invoke("UNDO"),
            KeyAction::Redo => self.invoke("REDO"),
        }
    }

    /// Recalls the previous submitted line, if any.
    pub fn history_previous(&mut self) -> Option<String> {
        self.history.previous().map(str::to_owned)
    }

    /// Recalls the next submitted line; an empty string means "back to a new line".
    pub fn history_next(&mut self) -> Option<String> {
        self.history.next().map(str::to_owned)
    }

    // ── internals ──

    fn cursor_world(&self) -> Option<Point2> {
        self.cursor.map(|cursor| self.view.screen_to_world(cursor))
    }

    fn expects_point(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(|active| active.command.expected() == ExpectedInput::Point)
    }

    /// Escape cancels the active command; at the idle prompt it clears the
    /// selection. Two Escapes therefore always reach idle with nothing selected.
    fn escape(&mut self) -> Dirty {
        if self.active.is_some() {
            return self.cancel_active();
        }
        if self.selection.is_empty() {
            return Dirty::NONE;
        }
        self.selection.clear();
        Dirty::OVERLAY | Dirty::UI
    }

    fn cancel_active(&mut self) -> Dirty {
        match self.active.take() {
            Some(_) => {
                self.log.push("*Cancel*".to_owned());
                Dirty::OVERLAY | Dirty::UI
            }
            None => Dirty::NONE,
        }
    }

    /// Hands one input to the active command. Typed and pointer input both
    /// arrive here.
    fn feed(&mut self, input: CommandInput) -> Dirty {
        let Some(active) = self.active.take() else {
            return Dirty::NONE;
        };
        if let CommandInput::Point(point) = input {
            self.last_point = point;
        }
        self.advance(active, |command, ctx| command.step(input, ctx))
    }

    /// Runs one step of `active`, keeps it active if it wants to continue, and
    /// reports what changed.
    fn advance(
        &mut self,
        mut active: ActiveCommand,
        run: impl FnOnce(&mut dyn Command, &mut CommandContext<'_>) -> Step,
    ) -> Dirty {
        let revision = self.drawing.revision();
        let mut ctx = CommandContext {
            drawing: &mut self.drawing,
            selection: &mut self.selection,
            log: &mut self.log,
        };
        if run(active.command.as_mut(), &mut ctx) == Step::Continue {
            self.active = Some(active);
        }

        // Whatever the command did, the selection must never name a dead entity.
        let drawing = &self.drawing;
        self.selection.retain(|id| drawing.entity(*id).is_some());

        let mut dirty = Dirty::OVERLAY | Dirty::UI;
        if self.drawing.revision() != revision {
            dirty |= Dirty::SCENE;
        }
        dirty
    }
}

trait ReplaceIfSome<T> {
    /// Replaces the value only when one is present, returning the old one.
    fn replace_if_some(&mut self, value: T) -> Option<T>;
}

impl<T> ReplaceIfSome<T> for Option<T> {
    fn replace_if_some(&mut self, value: T) -> Option<T> {
        self.as_mut().map(|slot| std::mem::replace(slot, value))
    }
}

#[cfg(test)]
mod tests {
    use cad_document::{Entity, LineEntity};

    use super::*;
    use crate::testing::Harness;

    fn at(x: f64, y: f64) -> ScreenPoint {
        ScreenPoint::new(x, y)
    }

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    // ── view ──

    #[test]
    fn resize_updates_the_view_and_flags_it() {
        let mut session = Session::new();
        let dirty = session.resize(1024.0, 768.0);
        assert!(dirty.contains(Dirty::VIEW));
        assert!(!dirty.contains(Dirty::SCENE));
        assert_eq!(
            (session.view().width(), session.view().height()),
            (1024.0, 768.0)
        );
    }

    #[test]
    fn wheel_up_zooms_in_about_the_cursor() {
        let mut h = Harness::new();
        let cursor = at(200.0, 100.0);
        let under_cursor = h.session.view().screen_to_world(cursor);

        let dirty = h.session.wheel(cursor, -100.0, Modifiers::NONE);

        assert!(dirty.contains(Dirty::VIEW));
        assert!(h.session.view().scale() > 1.0);
        let after = h.session.view().world_to_screen(under_cursor);
        assert!((after.x - cursor.x).abs() < 1e-9 && (after.y - cursor.y).abs() < 1e-9);
    }

    #[test]
    fn wheel_down_zooms_out_and_degenerate_input_is_ignored() {
        let mut h = Harness::new();
        h.session.wheel(at(400.0, 300.0), 100.0, Modifiers::NONE);
        let scale = h.session.view().scale();
        assert!(scale < 1.0);
        assert!(
            h.session
                .wheel(at(0.0, 0.0), 0.0, Modifiers::NONE)
                .is_empty()
        );
        assert!(
            h.session
                .wheel(at(0.0, 0.0), f64::NAN, Modifiers::NONE)
                .is_empty()
        );
        assert_eq!(h.session.view().scale(), scale);
    }

    #[test]
    fn middle_button_drag_pans_the_view() {
        let mut h = Harness::new();
        h.session
            .pointer_down(at(100.0, 100.0), PointerButton::Middle, Modifiers::NONE);
        let dirty = h.session.pointer_move(at(130.0, 80.0), Modifiers::NONE);

        assert!(dirty.contains(Dirty::VIEW));
        // Content follows the pointer: the world origin moved 30px right, 20px up.
        assert_eq!(
            h.session.view().world_to_screen(Point2::ORIGIN),
            at(430.0, 280.0)
        );

        h.session
            .pointer_up(at(130.0, 80.0), PointerButton::Middle, Modifiers::NONE);
        let dirty = h.session.pointer_move(at(500.0, 500.0), Modifiers::NONE);
        assert!(!dirty.contains(Dirty::VIEW));
        assert_eq!(
            h.session.view().world_to_screen(Point2::ORIGIN),
            at(430.0, 280.0)
        );
    }

    #[test]
    fn view_changes_never_touch_the_document() {
        let mut h = Harness::new();
        let revision = h.session.drawing().revision();
        h.session.wheel(at(10.0, 10.0), -300.0, Modifiers::NONE);
        h.session
            .pointer_down(at(0.0, 0.0), PointerButton::Middle, Modifiers::NONE);
        h.session.pointer_move(at(50.0, 50.0), Modifiers::NONE);
        h.session.resize(640.0, 480.0);
        assert_eq!(h.session.drawing().revision(), revision);
    }

    // ── command runtime ──

    #[test]
    fn the_idle_prompt_is_command() {
        let h = Harness::new();
        assert_eq!(h.prompt(), "Command:");
        assert_eq!(h.session.ui_state().command, None);
    }

    #[test]
    fn a_command_can_be_started_by_name_alias_or_invoke() {
        for start in ["LINE", "line", "L", "l"] {
            let mut h = Harness::new();
            h.enter(start);
            assert_eq!(
                h.session.ui_state().command,
                Some("LINE"),
                "typed {start:?}"
            );
            assert_eq!(h.prompt(), "Specify first point:");
        }
        let mut h = Harness::new();
        h.session.invoke("l");
        assert_eq!(h.session.ui_state().command, Some("LINE"));
    }

    #[test]
    fn unknown_commands_are_reported_and_leave_the_session_idle() {
        let mut h = Harness::new();
        let dirty = h.enter("FROB");
        assert!(dirty.contains(Dirty::UI));
        assert_eq!(h.log(), vec!["Command: FROB", "Unknown command \"FROB\"."]);
        assert_eq!(h.prompt(), "Command:");

        h.session.invoke("nope");
        assert_eq!(h.log().last().unwrap(), "Unknown command \"nope\".");
    }

    #[test]
    fn invalid_input_is_reported_and_the_command_stays_active() {
        let mut h = Harness::new();
        h.enter_all(&["L", "banana"]);
        assert_eq!(h.prompt(), "Specify first point:");
        assert_eq!(h.log().last().unwrap(), "A point is required, as x,y.");
        // The command still works afterwards.
        h.enter_all(&["0,0", "10,0", ""]);
        assert_eq!(h.lines().len(), 1);
    }

    #[test]
    fn submitted_text_is_echoed_after_its_prompt() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0"]);
        assert_eq!(h.log(), vec!["Command: L", "Specify first point: 0,0"]);
    }

    #[test]
    fn enter_at_the_idle_prompt_does_nothing_but_echo() {
        let mut h = Harness::new();
        h.enter("");
        assert_eq!(h.prompt(), "Command:");
        assert_eq!(h.log(), vec!["Command:"]);
        assert!(h.lines().is_empty());
    }

    #[test]
    fn escape_cancels_the_active_command() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0"]);
        let dirty = h.session.key(KeyAction::Escape);
        assert!(dirty.contains(Dirty::UI | Dirty::OVERLAY));
        assert_eq!(h.prompt(), "Command:");
        assert_eq!(h.log().last().unwrap(), "*Cancel*");
        assert!(h.lines().is_empty());
    }

    #[test]
    fn escape_when_idle_with_nothing_selected_changes_nothing() {
        let mut h = Harness::new();
        assert!(h.session.key(KeyAction::Escape).is_empty());
    }

    #[test]
    fn invoking_a_command_cancels_the_one_in_progress() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0"]);
        h.session.invoke("LINE");
        assert_eq!(h.prompt(), "Specify first point:");
        assert!(h.log().contains(&"*Cancel*".to_owned()));
    }

    /// DESIGN §26.3 criterion 8: pointer and typed coordinates feed the same
    /// command state machine.
    #[test]
    fn a_clicked_point_and_a_typed_point_are_equivalent() {
        let mut typed = Harness::new();
        typed.enter_all(&["L", "0,0", "100,50", ""]);

        let mut clicked = Harness::new();
        clicked.enter("L");
        clicked.click_world(p(0.0, 0.0));
        clicked.click_world(p(100.0, 50.0));
        clicked.enter("");

        assert_eq!(typed.lines(), clicked.lines());
        assert_eq!(
            typed.lines(),
            vec![LineEntity::new(p(0.0, 0.0), p(100.0, 50.0))]
        );
        assert_eq!(typed.prompt(), clicked.prompt());

        // And they can be mixed freely within one command.
        let mut mixed = Harness::new();
        mixed.enter("L");
        mixed.click_world(p(0.0, 0.0));
        mixed.enter_all(&["@100,50", ""]);
        assert_eq!(mixed.lines(), typed.lines());
    }

    #[test]
    fn relative_input_is_measured_from_the_last_point_given() {
        let mut h = Harness::new();
        h.enter_all(&["L", "10,10", "@5,0", "@0,5", ""]);
        assert_eq!(
            h.lines(),
            vec![
                LineEntity::new(p(10.0, 10.0), p(15.0, 10.0)),
                LineEntity::new(p(15.0, 10.0), p(15.0, 15.0))
            ]
        );
        // A clicked point also becomes the reference for the next relative input.
        h.enter("L");
        h.click_world(p(100.0, 100.0));
        h.enter_all(&["@1,1", ""]);
        assert_eq!(
            h.lines().last(),
            Some(&LineEntity::new(p(100.0, 100.0), p(101.0, 101.0)))
        );
    }

    #[test]
    fn relative_input_before_any_point_is_measured_from_the_origin() {
        let mut h = Harness::new();
        h.enter_all(&["L", "@3,4", "@1,0", ""]);
        assert_eq!(h.lines(), vec![LineEntity::new(p(3.0, 4.0), p(4.0, 4.0))]);
    }

    #[test]
    fn clicking_at_the_idle_prompt_creates_nothing() {
        let mut h = Harness::new();
        h.click(400.0, 300.0);
        assert!(h.lines().is_empty());
        assert_eq!(h.prompt(), "Command:");
    }

    #[test]
    fn dirty_flags_report_scene_changes_only_when_the_document_changed() {
        let mut h = Harness::new();
        assert!(!h.enter("L").contains(Dirty::SCENE));
        assert!(!h.enter("0,0").contains(Dirty::SCENE));
        assert!(h.enter("10,0").contains(Dirty::SCENE));
        assert!(!h.enter("").contains(Dirty::SCENE));
    }

    // ── overlay and UI state ──

    #[test]
    fn the_overlay_shows_a_crosshair_only_while_the_pointer_is_over_the_canvas() {
        let mut h = Harness::new();
        assert!(h.session.overlay().is_empty());

        h.move_to(400.0, 300.0);
        assert_eq!(
            h.session.overlay(),
            vec![OverlayItem::Crosshair { at: p(0.0, 0.0) }]
        );

        assert!(h.session.pointer_leave().contains(Dirty::OVERLAY));
        assert!(h.session.overlay().is_empty());
        assert!(h.session.pointer_leave().is_empty());
    }

    #[test]
    fn ui_state_reports_the_cursor_in_world_coordinates() {
        let mut h = Harness::new();
        assert_eq!(h.session.ui_state().cursor, None);
        h.move_to(410.0, 280.0);
        assert_eq!(h.session.ui_state().cursor, Some((10.0, 20.0)));
    }

    #[test]
    fn log_lines_can_be_fetched_incrementally() {
        let mut h = Harness::new();
        h.enter("L");
        let seen = h.session.ui_state().log_total;
        assert_eq!(seen, 1);
        h.enter("0,0");
        assert_eq!(h.session.log_since(seen), vec!["Specify first point: 0,0"]);
    }

    #[test]
    fn history_recalls_submitted_lines() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0"]);
        assert_eq!(h.session.history_previous().as_deref(), Some("0,0"));
        assert_eq!(h.session.history_previous().as_deref(), Some("L"));
        assert_eq!(h.session.history_previous(), None);
        assert_eq!(h.session.history_next().as_deref(), Some("0,0"));
        assert_eq!(h.session.history_next().as_deref(), Some(""));
        assert_eq!(h.session.history_next(), None);
    }

    #[test]
    fn a_session_can_start_from_an_existing_drawing() {
        let drawing =
            Drawing::from_entities([Entity::Line(LineEntity::new(p(0.0, 0.0), p(1.0, 1.0)))]);
        let session = Session::with_drawing(drawing);
        assert_eq!(session.drawing().len(), 1);
        assert!(!session.ui_state().can_undo);
    }
}
