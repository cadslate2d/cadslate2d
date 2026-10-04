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
use cad_document::{Entity, LineEntity};

use crate::{Dirty, KeyAction, Modifiers, PointerButton, Session};

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
        let mut session = Session::new();
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

    /// Clicks on the screen position that currently shows `point`.
    pub fn click_world(&mut self, point: Point2) -> Dirty {
        let at = self.session.view().world_to_screen(point);
        self.click(at.x, at.y)
    }

    pub fn prompt(&self) -> String {
        self.session.prompt()
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

    /// Every line in the drawing, in allocation order.
    pub fn lines(&self) -> Vec<LineEntity> {
        let drawing = self.session.drawing();
        let line = |id| drawing.entity(id).map(|Entity::Line(line)| *line);
        self.ids().into_iter().filter_map(line).collect()
    }
}
