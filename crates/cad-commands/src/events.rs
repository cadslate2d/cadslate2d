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

/// Non-text keys. Text entry, including Enter and Space-as-Enter, arrives
/// through `Session::submit_text` instead, because the in-progress edit
/// buffer belongs to the UI (ADR-0002 §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    /// Cancel the active command, or clear the selection when idle.
    Escape,
    /// The undo shortcut (Ctrl+Z). Same as invoking `UNDO`.
    Undo,
    /// The redo shortcut (Ctrl+Y, Ctrl+Shift+Z). Same as invoking `REDO`.
    Redo,
    /// The open shortcut (Ctrl+O). Same as invoking `OPEN`.
    Open,
    /// The save shortcut (Ctrl+S). Same as invoking `DXFOUT`.
    Save,
    /// Delete: erases the selection when no command is active.
    Delete,
    /// A status-bar toggle or its function key (F3, F7, F8, F10).
    Toggle(crate::Toggle),
}

/// Something only the host can do, requested by a command. Rust never calls
/// into the host; the host asks for the pending request after an input
/// reports `Dirty::HOST` (ADR-0002).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostRequest {
    /// Let the user pick a drawing file, then hand it to `Session::open_drawing`.
    OpenFile,
    /// Save the drawing to the file it came from or last went to, where the
    /// browser can write there again; ask where, otherwise.
    SaveFile,
    /// Save the drawing to a file the user chooses — a dialog, or a
    /// download where there is none.
    ExportFile,
}
