//! The session facade.

use cad_core::{ScreenPoint, View};
use cad_document::Drawing;

use crate::{Dirty, Modifiers, PointerButton};

/// Wheel delta (CSS pixels) to zoom exponent. One 100px notch zooms by ~22%.
const ZOOM_PER_WHEEL_PIXEL: f64 = 0.002;
/// Caps a single wheel event so a runaway delta cannot jump the view.
const MAX_WHEEL_DELTA: f64 = 500.0;

/// One open drawing and all interaction state around it.
#[derive(Debug)]
pub struct Session {
    drawing: Drawing,
    view: View,
    /// Last pointer position while a middle-button pan is in progress.
    pan_anchor: Option<ScreenPoint>,
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
            pan_anchor: None,
        }
    }

    pub fn drawing(&self) -> &Drawing {
        &self.drawing
    }

    pub fn view(&self) -> &View {
        &self.view
    }

    /// The canvas changed size, in CSS pixels.
    pub fn resize(&mut self, width: f64, height: f64) -> Dirty {
        self.view.resize(width, height);
        Dirty::VIEW | Dirty::OVERLAY
    }

    pub fn pointer_move(&mut self, at: ScreenPoint, _modifiers: Modifiers) -> Dirty {
        let Some(previous) = self.pan_anchor else {
            return Dirty::NONE;
        };
        self.view
            .pan_by_pixels(at.x - previous.x, at.y - previous.y);
        self.pan_anchor = Some(at);
        Dirty::VIEW | Dirty::OVERLAY
    }

    pub fn pointer_down(
        &mut self,
        at: ScreenPoint,
        button: PointerButton,
        _modifiers: Modifiers,
    ) -> Dirty {
        if button == PointerButton::Middle {
            self.pan_anchor = Some(at);
        }
        Dirty::NONE
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
        Dirty::VIEW | Dirty::OVERLAY
    }
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;

    use super::*;

    fn session() -> Session {
        let mut session = Session::new();
        session.resize(800.0, 600.0);
        session
    }

    fn at(x: f64, y: f64) -> ScreenPoint {
        ScreenPoint::new(x, y)
    }

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
        let mut session = session();
        let cursor = at(200.0, 100.0);
        let under_cursor = session.view().screen_to_world(cursor);

        let dirty = session.wheel(cursor, -100.0, Modifiers::NONE);

        assert!(dirty.contains(Dirty::VIEW));
        assert!(session.view().scale() > 1.0);
        let after = session.view().world_to_screen(under_cursor);
        assert!((after.x - cursor.x).abs() < 1e-9 && (after.y - cursor.y).abs() < 1e-9);
    }

    #[test]
    fn wheel_down_zooms_out() {
        let mut session = session();
        session.wheel(at(400.0, 300.0), 100.0, Modifiers::NONE);
        assert!(session.view().scale() < 1.0);
    }

    #[test]
    fn degenerate_wheel_input_is_ignored() {
        let mut session = session();
        assert!(session.wheel(at(0.0, 0.0), 0.0, Modifiers::NONE).is_empty());
        assert!(
            session
                .wheel(at(0.0, 0.0), f64::NAN, Modifiers::NONE)
                .is_empty()
        );
        assert_eq!(session.view().scale(), 1.0);
    }

    #[test]
    fn middle_button_drag_pans_the_view() {
        let mut session = session();
        session.pointer_down(at(100.0, 100.0), PointerButton::Middle, Modifiers::NONE);
        let dirty = session.pointer_move(at(130.0, 80.0), Modifiers::NONE);

        assert!(dirty.contains(Dirty::VIEW));
        // Content follows the pointer: the world origin moved 30px right, 20px up.
        assert_eq!(
            session.view().world_to_screen(Point2::ORIGIN),
            at(430.0, 280.0)
        );

        session.pointer_up(at(130.0, 80.0), PointerButton::Middle, Modifiers::NONE);
        assert!(
            session
                .pointer_move(at(500.0, 500.0), Modifiers::NONE)
                .is_empty()
        );
        assert_eq!(
            session.view().world_to_screen(Point2::ORIGIN),
            at(430.0, 280.0)
        );
    }

    #[test]
    fn view_changes_never_touch_the_document() {
        let mut session = session();
        let revision = session.drawing().revision();
        session.wheel(at(10.0, 10.0), -300.0, Modifiers::NONE);
        session.pointer_down(at(0.0, 0.0), PointerButton::Middle, Modifiers::NONE);
        session.pointer_move(at(50.0, 50.0), Modifiers::NONE);
        session.resize(640.0, 480.0);
        assert_eq!(session.drawing().revision(), revision);
    }
}
