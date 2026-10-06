//! The session facade.

use std::cell::RefCell;

use cad_core::{Aabb, EntityId, PixelTolerance, Point2, ScreenPoint, View};
use cad_document::{Color, Drawing, FOREGROUND_INDEX, Paint, Revision, index_paint};
use cad_input::{ExpectedInput, Input, InputKind};

use crate::aids::{self, POLAR_APERTURE, Track, TrackKind};
use crate::command::angle_degrees;
use crate::commands::{GripEditCommand, grip};
use crate::grips::{grip_at, grips_of};
use crate::history::InputHistory;
use crate::log::CommandLog;
use crate::snap::find_snap_from;
use crate::{
    Command, CommandContext, CommandInput, CommandRegistry, Dirty, DraftingSettings, Grip,
    HostRequest, HoveredObject, KeyAction, LayerRow, Modifiers, OverlayItem, PointerButton,
    Selection, SelectionSummary, SnapModes, SnapPoint, Step, Toggle, Toggles, UiState,
};

const IDLE_PROMPT: &str = "Command:";
/// Wheel delta (CSS pixels) to zoom exponent. One 100px notch zooms by ~22%.
const ZOOM_PER_WHEEL_PIXEL: f64 = 0.002;
/// Caps a single wheel event so a runaway delta cannot jump the view.
const MAX_WHEEL_DELTA: f64 = 500.0;
/// How close, on screen, a click must be to an entity to select it. A
/// display/picking tolerance: never used for geometric equality (DESIGN §18.3).
const PICK_TOLERANCE: PixelTolerance = PixelTolerance(5.0);

/// The point a pointer click would give right now, and what put it there:
/// an object snap, a drafting aid, or nothing but the cursor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aim {
    pub point: Point2,
    pub snap: Option<SnapPoint>,
    pub track: Option<Track>,
}

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
    /// First corner of a window or crossing selection in progress.
    selection_box: Option<ScreenPoint>,
    log: CommandLog,
    history: InputHistory,
    /// Set by a command that needs the host; taken by the host afterwards.
    host_request: Option<HostRequest>,
    /// Name of the file this drawing was opened from, if any.
    document_name: Option<String>,
    /// The drawing's revision when it was opened or last exported.
    saved_revision: Revision,
    /// The drawing came from a file with content that could not be imported.
    export_is_lossy: bool,
    /// The command Enter repeats at the idle prompt.
    last_command: Option<&'static str>,
    toggles: Toggles,
    settings: DraftingSettings,
    /// Where the primary button went down on a grip. Releasing it somewhere
    /// else drops the grip there, so a grip can be dragged as well as clicked.
    grip_press: Option<ScreenPoint>,
    /// The selection as the Properties panel shows it, with the drawing
    /// revision and selection generation it was computed for. Reused while
    /// neither changes, so pointer movement over a large selection costs
    /// nothing.
    summary: RefCell<Option<(Revision, u64, SelectionSummary)>>,
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
        Self::with_registry(drawing, CommandRegistry::standard())
    }

    /// A session with a caller-supplied set of commands.
    pub fn with_registry(drawing: Drawing, registry: CommandRegistry) -> Self {
        Self {
            saved_revision: drawing.revision(),
            drawing,
            view: View::new(1.0, 1.0),
            registry,
            active: None,
            selection: Selection::default(),
            last_point: Point2::ORIGIN,
            cursor: None,
            pan_anchor: None,
            selection_box: None,
            log: CommandLog::new(),
            history: InputHistory::default(),
            host_request: None,
            document_name: None,
            export_is_lossy: false,
            last_command: None,
            toggles: Toggles::default(),
            settings: DraftingSettings::default(),
            grip_press: None,
            summary: RefCell::new(None),
        }
    }

    /// The same session with the drafting toggles set as given, instead of
    /// the defaults: for restoring preferences, and for tests that want a
    /// bare drawing area.
    pub fn with_toggles(mut self, toggles: Toggles) -> Self {
        self.toggles = toggles;
        self
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

    pub fn snap_modes(&self) -> SnapModes {
        self.settings.snap_modes
    }

    pub fn settings(&self) -> &DraftingSettings {
        &self.settings
    }

    /// The grips on screen: those of the selected entities, at the idle
    /// prompt and while one of them is being dragged.
    pub fn grips(&self) -> Vec<Grip> {
        let shown = self
            .active
            .as_ref()
            .is_none_or(|active| active.command.grip().is_some());
        if shown {
            grips_of(&self.drawing, &self.selection)
        } else {
            Vec::new()
        }
    }

    /// The grip a click would take right now: the one under the cursor at
    /// the idle prompt. Grips come before entities and before empty space.
    pub fn hovered_grip(&self) -> Option<Grip> {
        let cursor = self.cursor?;
        let idle =
            self.active.is_none() && self.selection_box.is_none() && self.pan_anchor.is_none();
        if !idle {
            return None;
        }
        grip_at(&self.grips(), &self.view, cursor)
    }

    /// The entity a click would pick right now, while a command asks for one
    /// object. Found through the spatial index, like every pick.
    pub fn hovered_object(&self) -> Option<EntityId> {
        let cursor = self.cursor?;
        let asks_for_object = self.active.is_some() && self.expected().kind == InputKind::Object;
        if !asks_for_object || self.pan_anchor.is_some() {
            return None;
        }
        // Nothing is offered on an object a click would not take.
        self.pick(cursor).filter(|id| !self.drawing.is_locked(*id))
    }

    /// The grip being dragged, if a grip edit is in progress.
    pub fn active_grip(&self) -> Option<Grip> {
        self.active
            .as_ref()
            .and_then(|active| active.command.grip())
    }

    /// The object snap under the cursor, if a click would take one.
    ///
    /// Snapping applies only while a click would be taken as a point, and
    /// only to the pointer — typed coordinates are already exact.
    pub fn snap(&self) -> Option<SnapPoint> {
        let cursor = self.cursor?;
        let command = self.point_target()?;
        if !self.toggles.is_on(Toggle::Osnap) || !command.takes_object_snap() {
            return None;
        }
        find_snap_from(
            &self.drawing,
            &self.view,
            cursor,
            self.settings.snap_modes,
            command.transient_sources(),
            command.base_point(),
        )
    }

    /// The world point a pointer click would give, by the fixed precedence
    /// object snap > ORTHO or POLAR > free cursor (typed coordinates bypass
    /// all three). `None` while the pointer is off the canvas.
    ///
    /// It is derived on demand from the cursor, view, and drawing, so what
    /// the overlay shows and what a click commits cannot disagree.
    pub fn aim(&self) -> Option<Aim> {
        let cursor = self.cursor_world()?;
        let free = Aim {
            point: cursor,
            snap: None,
            track: None,
        };
        if let Some(snap) = self.snap() {
            return Some(Aim {
                point: snap.point,
                snap: Some(snap),
                ..free
            });
        }
        let base = self
            .point_target()
            .and_then(|command| command.tracking_base());
        let track = base.and_then(|base| {
            if self.toggles.is_on(Toggle::Ortho) {
                Some(aids::ortho(base, cursor))
            } else if self.toggles.is_on(Toggle::Polar) {
                let reach = self.view.pixels_to_world(POLAR_APERTURE.0);
                aids::polar(base, cursor, self.settings.polar_increment, reach)
            } else {
                None
            }
        });
        Some(track.map_or(free, |track| Aim {
            point: track.point,
            track: Some(track),
            ..free
        }))
    }

    /// The prompt shown on the command line right now.
    pub fn prompt(&self) -> String {
        self.active.as_ref().map_or_else(
            || IDLE_PROMPT.to_owned(),
            |active| active.command.prompt().render(&active.command.expected()),
        )
    }

    pub fn ui_state(&self) -> UiState {
        let aim = self.aim();
        UiState {
            prompt: self.prompt(),
            command: self.active.as_ref().map(|active| active.name),
            log_total: self.log.total(),
            cursor: aim.map(|aim| (aim.point.x, aim.point.y)),
            snap: aim.and_then(|aim| aim.snap).map(|snap| snap.mode.label()),
            tracking: aim.and_then(|aim| aim.track).map(|track| track.label()),
            selection_count: self.selection.len(),
            space_accepts: self.expected().space_accepts(),
            can_undo: self.drawing.can_undo(),
            can_redo: self.drawing.can_redo(),
            toggles: self.toggles.bits(),
            document_name: self.document_name.clone(),
            unsaved_changes: self.drawing.revision() != self.saved_revision,
            export_is_lossy: self.export_is_lossy,
            layers: self.layer_rows(),
            linetypes: self
                .drawing
                .linetypes()
                .iter()
                .map(|(_, def)| def.name.clone())
                .collect(),
            selection: self.selection_summary(),
            text_editor: self
                .active
                .as_ref()
                .and_then(|active| active.command.editor())
                .map(str::to_owned),
        }
    }

    /// The selection as the Properties panel shows it, computed once per
    /// change of the selection or the drawing.
    pub fn selection_summary(&self) -> SelectionSummary {
        let key = (self.drawing.revision(), self.selection.generation());
        let mut cached = self.summary.borrow_mut();
        match &*cached {
            Some((revision, generation, summary)) if (*revision, *generation) == key => {
                summary.clone()
            }
            _ => {
                let summary = SelectionSummary::of(&self.drawing, &self.selection);
                *cached = Some((key.0, key.1, summary.clone()));
                summary
            }
        }
    }

    /// The layer table as the Layers panel shows it.
    fn layer_rows(&self) -> Vec<LayerRow> {
        let current = self.drawing.current_layer();
        self.drawing
            .layers()
            .iter()
            .map(|(id, layer)| {
                // A layer's colour is a number; an indirection, which a
                // layer should not have, shows as the colour a new layer has.
                let color = match layer.color {
                    Color::Index(index) => index,
                    Color::ByLayer | Color::ByBlock => FOREGROUND_INDEX,
                };
                let rgb = match index_paint(color) {
                    Paint::Rgb(rgb) => Some(rgb),
                    Paint::Foreground => None,
                };
                LayerRow {
                    name: layer.name.clone(),
                    color,
                    rgb,
                    on: layer.on,
                    locked: layer.locked,
                    current: id == current,
                    linetype: layer.linetype.describe(self.drawing.linetypes()),
                    lineweight: layer.lineweight.describe(),
                }
            })
            .collect()
    }

    /// Log lines numbered `since` and later, oldest first.
    pub fn log_since(&self, since: u64) -> Vec<String> {
        self.log.since(since).map(str::to_owned).collect()
    }

    /// What should be drawn on top of the committed scene, in draw order.
    /// Purely descriptive; `cad-render` turns it into geometry.
    pub fn overlay(&self) -> Vec<OverlayItem> {
        let mut items = Vec::new();
        if self.toggles.is_on(Toggle::Grid) {
            items.push(OverlayItem::Grid {
                spacing: self.settings.grid_spacing,
            });
        }
        items.extend(self.selection.iter().map(OverlayItem::Highlight));
        let grips = self.grips();
        if !grips.is_empty() {
            items.push(OverlayItem::Grips {
                at: grips.iter().map(|grip| grip.position).collect(),
            });
        }
        if let Some(hot) = self.active_grip().or_else(|| self.hovered_grip()) {
            items.push(OverlayItem::HotGrip { at: hot.position });
        }
        let (Some(cursor), Some(aim)) = (self.cursor_world(), self.aim()) else {
            return items;
        };
        if let Some(track) = aim.track.filter(|track| track.kind == TrackKind::Polar) {
            items.push(OverlayItem::TrackingLine {
                from: track.from,
                through: track.point,
            });
        }
        if let Some(active) = &self.active {
            // Previews follow the point a click would give, not the raw cursor.
            active.command.preview(aim.point, &mut items);
            // Where a click would pick an object, show which, and what for.
            let hovered = self
                .hovered_object()
                .and_then(|id| Some((id, self.drawing.entity(id)?)));
            if let Some((id, entity)) = hovered {
                items.push(OverlayItem::Hover(id));
                let hovered = HoveredObject {
                    id,
                    entity,
                    at: cursor,
                    drawing: &self.drawing,
                };
                active.command.preview_pick(&hovered, &mut items);
            }
        }
        if let Some(start) = self.selection_box {
            items.push(OverlayItem::SelectionBox {
                from: self.view.screen_to_world(start),
                to: cursor,
                crossing: is_crossing(start, self.cursor.unwrap_or(start)),
            });
        }
        if let Some(snap) = aim.snap {
            items.push(OverlayItem::SnapMarker {
                mode: snap.mode,
                at: snap.point,
            });
        }
        // The crosshair stays with the pointer; markers show where a click lands.
        items.push(OverlayItem::Crosshair { at: cursor });
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
        modifiers: Modifiers,
    ) -> Dirty {
        self.cursor = Some(at);
        match button {
            PointerButton::Middle => {
                self.pan_anchor = Some(at);
                Dirty::NONE
            }
            // A click during a command is a point, exactly like a typed coordinate.
            // At the idle prompt and at "Select objects:", the pointer builds
            // the selection set; anywhere else a click is a point.
            PointerButton::Primary if self.is_selecting() => {
                // A grip under the cursor comes before picking and box selection.
                match self.hovered_grip().filter(|_| !modifiers.shift) {
                    Some(grip) => {
                        self.grip_press = Some(at);
                        self.begin(grip::NAME, Box::new(GripEditCommand::new(grip)))
                    }
                    None => self.selection_click(at, modifiers),
                }
            }
            // A command asking for one object gets the entity under the cursor.
            PointerButton::Primary if self.expected().kind == InputKind::Object => {
                match self.pick(at) {
                    // A command acts on what it picks, and nothing acts on
                    // an object on a locked layer.
                    Some(entity) if self.drawing.is_locked(entity) => {
                        self.log.push(self.locked_message(entity));
                        Dirty::UI
                    }
                    Some(entity) => self.feed(CommandInput::Pick {
                        entity,
                        at: self.view.screen_to_world(at),
                    }),
                    None => {
                        self.log.push("No object there.".to_owned());
                        Dirty::UI
                    }
                }
            }
            PointerButton::Primary => match self.aim() {
                Some(aim) => self.feed_point(aim.point).unwrap_or(Dirty::NONE),
                None => Dirty::NONE,
            },
            PointerButton::Secondary => Dirty::NONE,
        }
    }

    pub fn pointer_up(
        &mut self,
        at: ScreenPoint,
        button: PointerButton,
        modifiers: Modifiers,
    ) -> Dirty {
        match button {
            PointerButton::Middle => {
                self.pan_anchor = None;
                Dirty::NONE
            }
            PointerButton::Primary => {
                // A grip that was pressed and dragged is dropped on release;
                // one that was only clicked stays in hand until the next click.
                if let Some(press) = self.grip_press.take() {
                    if dragged(press, at) && self.active_grip().is_some() {
                        self.cursor = Some(at);
                        return match self.aim() {
                            Some(aim) => self.feed_point(aim.point).unwrap_or(Dirty::NONE),
                            None => Dirty::NONE,
                        };
                    }
                    return Dirty::NONE;
                }
                // Releasing after a drag completes the box; a plain click
                // leaves it open for a second click, as in classic CAD.
                match self.selection_box {
                    Some(start) if dragged(start, at) => {
                        self.finish_selection_box(start, at, modifiers)
                    }
                    _ => Dirty::NONE,
                }
            }
            PointerButton::Secondary => Dirty::NONE,
        }
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
        // A paragraph from the editor is echoed on one line.
        let echo = format!("{} {}", self.prompt(), text.trim().replace('\n', " ⏎ "));
        self.log.push(echo.trim_end().to_owned());
        self.history.record(text);

        let resolved = cad_input::resolve(
            text,
            &self.expected(),
            self.registry.aliases(),
            self.drawing.unit(),
        );
        let dirty = match resolved {
            Ok(input) => self
                .dispatch(input)
                .unwrap_or_else(|message| self.notify(message)),
            // The command stays active and prompts again.
            Err(diagnostic) => self.notify(diagnostic.message),
        };
        dirty | Dirty::UI
    }

    pub fn toggles(&self) -> Toggles {
        self.toggles
    }

    /// Starts a command from the UI — a toolbar button or a menu item — and
    /// echoes it in the command line as if it had been typed (DESIGN §5, §15).
    pub fn invoke_from_ui(&mut self, name: &str) -> Dirty {
        self.log.push(format!("{IDLE_PROMPT} {}", name.trim()));
        self.invoke(name)
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
        self.last_command = Some(name);
        self.begin(name, command)
    }

    /// Makes `command` the active command, cancelling any other first.
    fn begin(&mut self, name: &'static str, command: Box<dyn Command>) -> Dirty {
        let cancelled = self.cancel_active();
        // One invocation is one undo step, however many transactions it commits.
        self.drawing.begin_group();
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
            KeyAction::Open => self.invoke("OPEN"),
            KeyAction::Save => self.invoke("DXFOUT"),
            KeyAction::Delete if self.active.is_none() && !self.selection.is_empty() => {
                self.invoke("ERASE")
            }
            KeyAction::Delete => Dirty::NONE,
            // A toggle flips whatever else is going on: the active command is
            // not cancelled and its state is untouched.
            KeyAction::Toggle(toggle) => {
                let on = self.toggles.flip(toggle);
                let state = if on { "on" } else { "off" };
                self.log.push(format!("<{} {state}>", toggle.label()));
                // Switching one of two rival aids on switches the other off.
                let rival = toggle
                    .rival()
                    .filter(|rival| on && self.toggles.is_on(*rival));
                if let Some(rival) = rival {
                    self.toggles.flip(rival);
                    self.log.push(format!("<{} off>", rival.label()));
                }
                // Drafting aids change what the overlay shows under the cursor.
                Dirty::OVERLAY | Dirty::UI
            }
        }
    }

    // ── host integration ──

    /// The request raised by the last input that reported `Dirty::HOST`.
    pub fn take_host_request(&mut self) -> Option<HostRequest> {
        self.host_request.take()
    }

    /// Replaces the drawing with one the host has loaded.
    ///
    /// Opening is not an edit: the new drawing starts with an empty history,
    /// and all interaction state tied to the old one is dropped. `lossy`
    /// records that the source file held content that could not be imported.
    pub fn open_drawing(
        &mut self,
        drawing: Drawing,
        name: &str,
        lossy: bool,
        messages: Vec<String>,
    ) -> Dirty {
        self.saved_revision = drawing.revision();
        self.drawing = drawing;
        self.document_name = Some(name.to_owned());
        self.export_is_lossy = lossy;
        self.active = None;
        self.selection_box = None;
        self.selection.clear();
        self.last_point = Point2::ORIGIN;
        if let Some(extents) = self.drawing.extents() {
            self.view.fit(&extents);
        }
        messages
            .into_iter()
            .for_each(|message| self.log.push(message));
        Dirty::SCENE | Dirty::OVERLAY | Dirty::VIEW | Dirty::UI
    }

    /// Records that the host has exported the drawing in its current state.
    pub fn mark_exported(&mut self) -> Dirty {
        self.saved_revision = self.drawing.revision();
        self.notify("Drawing exported as DXF.")
    }

    /// Writes a host-side message, such as a failed import, to the command log.
    pub fn notify(&mut self, message: impl Into<String>) -> Dirty {
        self.log.push(message.into());
        Dirty::UI
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

    /// The active command, if a pointer click would reach it as a point: it
    /// asks for a point, or for a distance or angle it can measure from its
    /// base point. Not while the middle button is panning.
    fn point_target(&self) -> Option<&dyn Command> {
        let command = self.active.as_ref()?.command.as_ref();
        let takes_a_point = match command.expected().kind {
            InputKind::Point => true,
            InputKind::Distance | InputKind::Angle => command.base_point().is_some(),
            InputKind::Command
            | InputKind::Selection
            | InputKind::Object
            | InputKind::Nothing
            | InputKind::Text => false,
        };
        (takes_a_point && self.pan_anchor.is_none()).then_some(command)
    }

    /// The entity under a screen position, if any.
    ///
    /// The spatial index proposes candidates from a pick box sized in pixels;
    /// only those candidates get the exact distance test. The drawing is never
    /// scanned (DESIGN §26.3, criterion 12).
    fn pick(&self, at: ScreenPoint) -> Option<EntityId> {
        let point = self.view.screen_to_world(at);
        let reach = self.view.pixels_to_world(PICK_TOLERANCE.0);
        self.drawing
            .query_shown(&self.view.pick_bounds(at, PICK_TOLERANCE))
            .into_iter()
            .filter_map(|id| {
                Some((
                    id,
                    self.drawing
                        .entity(id)?
                        .distance_to(point, self.drawing.blocks()),
                ))
            })
            .filter(|(_, distance)| *distance <= reach)
            // Nearest wins; ties go to the older entity so picking is deterministic.
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
            .map(|(id, _)| id)
    }

    /// True when the pointer is building a selection set: at the idle prompt
    /// (noun–verb) and while a command asks for objects (verb–noun).
    fn is_selecting(&self) -> bool {
        self.active
            .as_ref()
            .is_none_or(|active| active.command.expected().kind == InputKind::Selection)
    }

    /// One click while selecting: finish an open box, pick the entity under
    /// the cursor, or — on empty space — start a box. Shift removes instead
    /// of adding.
    fn selection_click(&mut self, at: ScreenPoint, modifiers: Modifiers) -> Dirty {
        if let Some(start) = self.selection_box {
            return self.finish_selection_box(start, at, modifiers);
        }
        match self.pick(at) {
            Some(id) => self.change_selection(&[id], modifiers.shift),
            None => {
                self.selection_box = Some(at);
                Dirty::OVERLAY
            }
        }
    }

    /// Completes a selection box. Dragged left to right it is a *window*
    /// (entities entirely inside); right to left it is a *crossing*
    /// (entities any part of which is inside) — DESIGN §16.1.
    fn finish_selection_box(
        &mut self,
        start: ScreenPoint,
        end: ScreenPoint,
        modifiers: Modifiers,
    ) -> Dirty {
        self.selection_box = None;
        let bounds = Aabb::from_points(
            self.view.screen_to_world(start),
            self.view.screen_to_world(end),
        );
        let crossing = is_crossing(start, end);
        let mut hits: Vec<EntityId> = self
            .drawing
            .query_shown(&bounds)
            .into_iter()
            .filter(|&id| {
                self.drawing.entity(id).is_some_and(|entity| {
                    if crossing {
                        entity.crosses(&bounds, self.drawing.blocks())
                    } else {
                        entity.is_inside(&bounds, self.drawing.blocks())
                    }
                })
            })
            .collect();
        hits.sort_unstable();
        self.change_selection(&hits, modifiers.shift) | Dirty::OVERLAY
    }

    /// Adds `ids` to the selection, or removes them, and reports the result
    /// while a command is collecting objects.
    ///
    /// Objects on a locked layer are never added: a selection is what a
    /// command will change. Leaving them out is said, so a click that
    /// selects nothing is not a mystery.
    fn change_selection(&mut self, ids: &[EntityId], remove: bool) -> Dirty {
        let locked = if remove {
            0
        } else {
            ids.iter().filter(|id| self.drawing.is_locked(**id)).count()
        };
        let changed = ids
            .iter()
            .filter(|&&id| {
                if remove {
                    self.selection.remove(id)
                } else {
                    !self.drawing.is_locked(id) && self.selection.insert(id)
                }
            })
            .count();
        if self.active.is_some() {
            let verb = if remove { "removed" } else { "found" };
            self.log
                .push(format!("{changed} {verb}, {} total", self.selection.len()));
        }
        match locked {
            0 => {}
            1 => self.log.push("1 was on a locked layer.".to_owned()),
            several => self.log.push(format!("{several} were on a locked layer.")),
        }
        if changed > 0 || locked > 0 || self.active.is_some() {
            Dirty::OVERLAY | Dirty::UI
        } else {
            Dirty::NONE
        }
    }

    /// Why a pick on `id` is refused: it names the layer.
    fn locked_message(&self, id: EntityId) -> String {
        let layer = self
            .drawing
            .layer_of(id)
            .map_or("", |layer| layer.name.as_str());
        format!("That object is on the locked layer \"{layer}\".")
    }

    /// What the current prompt accepts.
    fn expected(&self) -> ExpectedInput {
        self.active
            .as_ref()
            .map_or_else(ExpectedInput::command, |active| active.command.expected())
    }

    /// Routes one resolved input. An `Err` is a message for the user; the
    /// command, if any, stays active.
    fn dispatch(&mut self, input: Input) -> Result<Dirty, &'static str> {
        match input {
            Input::Command(name) => Ok(self.invoke(&name)),
            Input::Point(point) => self.feed_point(point.resolve(self.relative_origin())),
            Input::Distance(distance) => Ok(self.feed(CommandInput::Distance(distance))),
            Input::Angle(angle) => Ok(self.feed(CommandInput::Angle(angle))),
            Input::Keyword(keyword) => Ok(self.feed(CommandInput::Keyword(keyword))),
            Input::Text(text) => Ok(self.feed(CommandInput::Text(text))),
            Input::DirectDistance(distance) => {
                let target = self.direct_distance_target(distance).ok_or(NO_DIRECTION)?;
                Ok(self.feed(CommandInput::Point(target)))
            }
            // Enter at the idle prompt repeats the last command (DESIGN §13.4).
            Input::Accept if self.active.is_none() => Ok(self
                .last_command
                .map_or(Dirty::NONE, |name| self.invoke(name))),
            // Enter while a grip is in hand drops it where the pointer is.
            Input::Accept if self.active_grip().is_some() => match self.aim() {
                Some(aim) => self.feed_point(aim.point),
                None => Ok(self.feed(CommandInput::Accept)),
            },
            Input::Accept => Ok(self.feed(CommandInput::Accept)),
        }
    }

    /// What `@` coordinates are relative to: the active command's base point
    /// when it has one (so it follows an in-command Undo), otherwise the last
    /// point given to any command.
    fn relative_origin(&self) -> Point2 {
        let base = self
            .active
            .as_ref()
            .and_then(|active| active.command.base_point());
        base.unwrap_or(self.last_point)
    }

    /// Where direct-distance entry lands: `distance` from the command's base
    /// point towards the cursor. `None` without a base point or a direction.
    fn direct_distance_target(&self, distance: f64) -> Option<Point2> {
        let base = self.active.as_ref()?.command.base_point()?;
        let towards = self.aim()?.point - base;
        let length = towards.length();
        (length > 0.0).then(|| base + towards * (distance / length))
    }

    /// Hands a world point to the active command as whatever it asked for:
    /// the point itself, or the distance or angle it makes with the command's
    /// base point. Typed coordinates and pointer clicks both arrive here.
    fn feed_point(&mut self, point: Point2) -> Result<Dirty, &'static str> {
        let Some(active) = &self.active else {
            return Ok(Dirty::NONE);
        };
        let from_base = active.command.base_point().map(|base| point - base);
        let input = match (active.command.expected().kind, from_base) {
            (InputKind::Point, _) => CommandInput::Point(point),
            (InputKind::Distance, Some(offset)) => CommandInput::Distance(offset.length()),
            (InputKind::Angle, Some(offset)) => CommandInput::Angle(angle_degrees(offset)),
            (InputKind::Distance | InputKind::Angle, None) => return Err("A number is required."),
            (InputKind::Object, _) => return Err("Pick an object with the pointer."),
            (InputKind::Nothing | InputKind::Command | InputKind::Selection, _) => {
                return Err("An option keyword is required.");
            }
            (InputKind::Text, _) => return Err("Type the text and press Enter."),
        };
        Ok(self.feed(input))
    }

    /// Escape cancels the active command; at the idle prompt it clears the
    /// selection. Two Escapes therefore always reach idle with nothing selected.
    fn escape(&mut self) -> Dirty {
        // An open selection box is the innermost thing to cancel.
        if self.selection_box.take().is_some() && self.active.is_none() {
            return Dirty::OVERLAY;
        }
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
        self.selection_box = None;
        match self.active.take() {
            Some(_) => {
                self.drawing.end_group();
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
            settings: &mut self.settings,
            toggles: self.toggles,
            log: &mut self.log,
            host_request: &mut self.host_request,
        };
        if run(active.command.as_mut(), &mut ctx) == Step::Continue {
            self.active = Some(active);
        } else {
            self.drawing.end_group();
        }

        // Whatever the command did, the selection must never name an entity
        // that is gone, that is no longer shown, or that has been locked.
        let drawing = &self.drawing;
        self.selection
            .retain(|id| drawing.is_shown(*id) && !drawing.is_locked(*id));

        let mut dirty = Dirty::OVERLAY | Dirty::UI;
        if self.drawing.revision() != revision {
            dirty |= Dirty::SCENE;
        }
        if self.host_request.is_some() {
            dirty |= Dirty::HOST;
        }
        dirty
    }
}

/// Pointer travel, in pixels, beyond which press-and-release is a drag.
const DRAG_THRESHOLD: f64 = 4.0;

fn dragged(from: ScreenPoint, to: ScreenPoint) -> bool {
    (to.x - from.x).hypot(to.y - from.y) > DRAG_THRESHOLD
}

/// A selection box drawn right to left is a crossing box.
fn is_crossing(start: ScreenPoint, end: ScreenPoint) -> bool {
    end.x < start.x
}

const NO_DIRECTION: &str = "Point the cursor in a direction, then enter the distance.";

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
        assert_eq!(h.log().last().unwrap(), "A point is required.");
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
    fn enter_at_the_idle_prompt_before_any_command_does_nothing_but_echo() {
        let mut h = Harness::new();
        h.enter("");
        assert_eq!(h.prompt(), "Command:");
        assert_eq!(h.log(), vec!["Command:"]);
        assert!(h.lines().is_empty());
    }

    /// DESIGN §13.4: Enter (and Space) at the idle prompt repeat the last command.
    #[test]
    fn enter_at_the_idle_prompt_repeats_the_last_command() {
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "10,0", ""]);
        assert_eq!(h.prompt(), "Command:");

        h.enter("");
        assert_eq!(h.session.ui_state().command, Some("LINE"));
        assert_eq!(h.prompt(), "Specify first point:");

        // A cancelled command is still the one that repeats.
        h.escape();
        h.enter("");
        assert_eq!(h.session.ui_state().command, Some("LINE"));
    }

    // ── toggles and UI-invoked commands ──

    #[test]
    fn a_toggle_flips_mid_command_without_disturbing_it() {
        use crate::Toggle;
        let mut h = Harness::new();
        h.enter_all(&["L", "0,0", "10,0"]);
        let prompt = h.prompt();
        let revision = h.session.drawing().revision();

        let dirty = h.session.key(KeyAction::Toggle(Toggle::Ortho));
        assert_eq!(dirty, Dirty::OVERLAY | Dirty::UI);
        assert!(h.session.toggles().is_on(Toggle::Ortho));
        assert_eq!(
            h.session.ui_state().toggles,
            Toggle::Ortho.bit() | Toggle::Osnap.bit()
        );
        assert_eq!(h.log().last().unwrap(), "<ORTHO on>");

        // The command carries on exactly where it was.
        assert_eq!(h.prompt(), prompt);
        assert_eq!(h.session.drawing().revision(), revision);
        h.enter_all(&["10,10", ""]);
        assert_eq!(h.lines().len(), 2);

        h.session.key(KeyAction::Toggle(Toggle::Ortho));
        assert_eq!(h.log().last().unwrap(), "<ORTHO off>");
        assert_eq!(h.session.ui_state().toggles, Toggle::Osnap.bit());
    }

    #[test]
    fn toggles_survive_opening_another_drawing_and_are_not_undoable() {
        use crate::Toggle;
        let mut h = Harness::new();
        h.session.key(KeyAction::Toggle(Toggle::Grid));
        h.enter_all(&["L", "0,0", "10,0", ""]);
        h.undo();
        assert!(
            h.session.toggles().is_on(Toggle::Grid),
            "undo reverts the drawing, not the drafting aids"
        );
        h.session
            .open_drawing(Drawing::new(), "other.dxf", false, Vec::new());
        assert!(h.session.toggles().is_on(Toggle::Grid));
    }

    /// DESIGN §15: toolbar buttons must not bypass the command architecture.
    #[test]
    fn a_ui_invoked_command_is_identical_to_a_typed_one_and_is_echoed() {
        let mut typed = Harness::new();
        typed.enter("CIRCLE");
        let mut clicked = Harness::new();
        clicked.session.invoke_from_ui("CIRCLE");

        assert_eq!(clicked.prompt(), typed.prompt());
        assert_eq!(clicked.session.ui_state(), typed.session.ui_state());
        assert_eq!(clicked.log(), typed.log(), "the button echoes as if typed");

        // Like the classic ^C^C, a button cancels a command in progress first.
        clicked.enter("0,0");
        clicked.session.invoke_from_ui("LINE");
        assert_eq!(clicked.session.ui_state().command, Some("LINE"));
        assert!(clicked.log().contains(&"*Cancel*".to_owned()));
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
    fn clicking_empty_space_at_the_idle_prompt_creates_nothing() {
        let mut h = Harness::new();
        h.click(400.0, 300.0);
        assert!(h.lines().is_empty());
        assert_eq!(h.prompt(), "Command:");
        assert!(h.session.selection().is_empty());
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
