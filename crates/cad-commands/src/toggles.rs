//! Drafting toggles: the status-bar switches of classic CAD (DESIGN §5, §13.4).
//!
//! This is the framework only. The toggles hold state and can be flipped from
//! the status bar or by function key, at any time and without disturbing the
//! active command. The drafting behaviours they control arrive in M2.

/// A drafting aid that can be switched on and off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toggle {
    /// Running object snaps (F3).
    Osnap,
    /// Grid display (F7).
    Grid,
    /// Orthogonal constraint (F8).
    Ortho,
    /// Polar tracking (F10).
    Polar,
}

impl Toggle {
    pub const ALL: [Self; 4] = [Self::Osnap, Self::Grid, Self::Ortho, Self::Polar];

    /// This toggle's bit in [`Toggles::bits`].
    pub const fn bit(self) -> u32 {
        1 << self as u32
    }

    /// The label on the status bar.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Osnap => "OSNAP",
            Self::Grid => "GRID",
            Self::Ortho => "ORTHO",
            Self::Polar => "POLAR",
        }
    }
}

/// The state of every toggle. Session state: not part of the drawing and not
/// undoable.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Toggles {
    bits: u32,
}

impl Toggles {
    pub fn is_on(self, toggle: Toggle) -> bool {
        self.bits & toggle.bit() != 0
    }

    /// Flips `toggle` and returns its new state.
    pub fn flip(&mut self, toggle: Toggle) -> bool {
        self.bits ^= toggle.bit();
        self.is_on(toggle)
    }

    /// One bit per toggle, for the UI.
    pub fn bits(self) -> u32 {
        self.bits
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_starts_off() {
        let toggles = Toggles::default();
        assert!(Toggle::ALL.iter().all(|toggle| !toggles.is_on(*toggle)));
        assert_eq!(toggles.bits(), 0);
    }

    #[test]
    fn flipping_one_toggle_leaves_the_others_alone() {
        let mut toggles = Toggles::default();
        assert!(toggles.flip(Toggle::Ortho));
        assert!(toggles.is_on(Toggle::Ortho));
        assert!(
            !toggles.is_on(Toggle::Osnap)
                && !toggles.is_on(Toggle::Grid)
                && !toggles.is_on(Toggle::Polar)
        );
        assert!(!toggles.flip(Toggle::Ortho));
        assert_eq!(toggles.bits(), 0);
    }

    #[test]
    fn every_toggle_has_its_own_bit_and_label() {
        let bits: Vec<u32> = Toggle::ALL.iter().map(|toggle| toggle.bit()).collect();
        assert_eq!(bits, vec![1, 2, 4, 8]);
        let labels: Vec<&str> = Toggle::ALL.iter().map(|toggle| toggle.label()).collect();
        assert_eq!(labels, vec!["OSNAP", "GRID", "ORTHO", "POLAR"]);
    }
}
