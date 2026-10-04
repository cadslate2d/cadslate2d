//! Semantic input events (ADR-0002).

/// Keyboard modifiers held during a pointer or key event.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        shift: false,
        ctrl: false,
        alt: false,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Middle,
    Secondary,
}
