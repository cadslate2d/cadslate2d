//! A scripting harness for headless command tests (DESIGN §41).
//!
//! It drives a [`Session`] through exactly the public methods `cad-wasm`
//! calls, so a flow that passes here is the flow the browser runs.
//!
//! ```ignore
//! use cad_commands::testing::Harness;
//!
//! let mut cad = Harness::new();
//! cad.enter_all(&["L", "0,0", "100,0", ""]);
//! assert_eq!(cad.lines().len(), 1);
//! ```

use cad_core::{EntityId, Point2, ScreenPoint};
use cad_document::{Drawing, Entity, LineEntity, Properties};

use crate::{Dirty, KeyAction, Modifiers, PointerButton, Session, Toggle, Toggles};

/// A session on an 800×600 canvas centred on the world origin at one pixel
/// per unit, so screen `(400, 300)` is world `(0, 0)`.
#[derive(Debug)]
pub struct Harness {
    pub session: Session,
}

impl Default for Harness {
    fn default() -> Self {
        Self::new()
    }
}

impl Harness {
    pub fn new() -> Self {
        // No grid: overlay assertions then see only what the test put there.
        // Tests about the grid switch it on.
        let mut session = Session::new().with_toggles(Toggles::only(&[Toggle::Osnap]));
        session.resize(800.0, 600.0);
        Self { session }
    }

    /// A harness on a drawing that already holds `entities`.
    pub fn with_entities(entities: impl IntoIterator<Item = Entity>) -> Self {
        Self::with_drawing(Drawing::from_entities(entities))
    }

    /// A harness on a drawing built elsewhere: with layers, say, and
    /// entities that have properties.
    pub fn with_drawing(drawing: Drawing) -> Self {
        let mut session =
            Session::with_drawing(drawing).with_toggles(Toggles::only(&[Toggle::Osnap]));
        session.resize(800.0, 600.0);
        Self { session }
    }

    /// Types one line and presses Enter.
    pub fn enter(&mut self, line: &str) -> Dirty {
        self.session.submit_text(line)
    }

    /// Types several lines, pressing Enter after each.
    pub fn enter_all(&mut self, lines: &[&str]) {
        for line in lines {
            self.enter(line);
        }
    }

    pub fn escape(&mut self) -> Dirty {
        self.session.key(KeyAction::Escape)
    }

    /// Presses the undo shortcut.
    pub fn undo(&mut self) -> Dirty {
        self.session.key(KeyAction::Undo)
    }

    /// Presses the redo shortcut.
    pub fn redo(&mut self) -> Dirty {
        self.session.key(KeyAction::Redo)
    }

    /// Moves the pointer to a screen position.
    pub fn move_to(&mut self, x: f64, y: f64) -> Dirty {
        self.session
            .pointer_move(ScreenPoint::new(x, y), Modifiers::NONE)
    }

    /// Moves the pointer to the screen position that currently shows `point`.
    pub fn move_to_world(&mut self, point: Point2) -> Dirty {
        let at = self.session.view().world_to_screen(point);
        self.move_to(at.x, at.y)
    }

    /// Flips a drafting toggle, as its function key does.
    pub fn toggle(&mut self, toggle: Toggle) -> Dirty {
        self.session.key(KeyAction::Toggle(toggle))
    }

    /// Presses and releases the primary button at a screen position.
    pub fn click(&mut self, x: f64, y: f64) -> Dirty {
        let at = ScreenPoint::new(x, y);
        let dirty = self
            .session
            .pointer_down(at, PointerButton::Primary, Modifiers::NONE);
        dirty
            | self
                .session
                .pointer_up(at, PointerButton::Primary, Modifiers::NONE)
    }

    /// Clicks with Shift held.
    pub fn shift_click_world(&mut self, point: Point2) -> Dirty {
        let at = self.session.view().world_to_screen(point);
        let shift = Modifiers {
            shift: true,
            ..Modifiers::NONE
        };
        let dirty = self.session.pointer_down(at, PointerButton::Primary, shift);
        dirty | self.session.pointer_up(at, PointerButton::Primary, shift)
    }

    /// Draws a selection box with two clicks, from `from` to `to` (world).
    /// Left to right is a window; right to left is a crossing box.
    pub fn select_box(&mut self, from: Point2, to: Point2) -> Dirty {
        self.click_world(from) | self.click_world(to)
    }

    /// Presses the Delete key.
    pub fn delete(&mut self) -> Dirty {
        self.session.key(KeyAction::Delete)
    }

    /// Clicks on the screen position that currently shows `point`.
    pub fn click_world(&mut self, point: Point2) -> Dirty {
        let at = self.session.view().world_to_screen(point);
        self.click(at.x, at.y)
    }

    pub fn prompt(&self) -> String {
        self.session.prompt()
    }

    /// Types several lines and reports everything they dirtied together.
    pub fn enter_all_dirty(&mut self, lines: &[&str]) -> Dirty {
        lines
            .iter()
            .fold(Dirty::NONE, |dirty, line| dirty | self.enter(line))
    }

    /// The whole command log.
    pub fn log(&self) -> Vec<String> {
        self.session.log_since(0)
    }

    /// Every entity ID in the drawing, in allocation order.
    pub fn ids(&self) -> Vec<EntityId> {
        let mut ids: Vec<EntityId> = self
            .session
            .drawing()
            .entities()
            .map(|(id, _)| id)
            .collect();
        ids.sort_unstable();
        ids
    }

    /// Every entity in the drawing, in allocation order.
    pub fn entities(&self) -> Vec<Entity> {
        let drawing = self.session.drawing();
        self.ids()
            .into_iter()
            .filter_map(|id| drawing.entity(id).cloned())
            .collect()
    }

    /// The properties of every entity in the drawing, in allocation order.
    pub fn properties(&self) -> Vec<Properties> {
        let drawing = self.session.drawing();
        self.ids()
            .into_iter()
            .filter_map(|id| drawing.properties(id))
            .collect()
    }

    /// Every line in the drawing, in allocation order.
    pub fn lines(&self) -> Vec<LineEntity> {
        let drawing = self.session.drawing();
        let line = |id| drawing.entity(id).and_then(Entity::as_line).copied();
        self.ids().into_iter().filter_map(line).collect()
    }
}
