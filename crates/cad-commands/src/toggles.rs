//! Drafting toggles: the status-bar switches of classic CAD (DESIGN §5, §13.4).
//!
//! The toggles hold state and can be flipped from the status bar or by
//! function key, at any time and without disturbing the active command. The
//! session consults them where the drafting aid applies.

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

    /// The toggle that cannot be on at the same time as this one. ORTHO and
    /// POLAR both claim the direction of the rubber band.
    pub const fn rival(self) -> Option<Self> {
        match self {
            Self::Ortho => Some(Self::Polar),
            Self::Polar => Some(Self::Ortho),
            Self::Osnap | Self::Grid => None,
        }
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Toggles {
    bits: u32,
}

impl Default for Toggles {
    /// Object snaps and the grid are on from the start; the rest are off.
    fn default() -> Self {
        Self::only(&[Toggle::Osnap, Toggle::Grid])
    }
}

impl Toggles {
    /// Exactly these toggles on.
    pub fn only(on: &[Toggle]) -> Self {
        Self {
            bits: on.iter().fold(0, |bits, toggle| bits | toggle.bit()),
        }
    }

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
    fn object_snaps_and_the_grid_start_on() {
        let toggles = Toggles::default();
        assert!(toggles.is_on(Toggle::Osnap) && toggles.is_on(Toggle::Grid));
        assert!(!toggles.is_on(Toggle::Ortho) && !toggles.is_on(Toggle::Polar));
        assert_eq!(toggles.bits(), Toggle::Osnap.bit() | Toggle::Grid.bit());
    }

    #[test]
    fn flipping_one_toggle_leaves_the_others_alone() {
        let mut toggles = Toggles::only(&[Toggle::Osnap]);
        assert!(toggles.flip(Toggle::Ortho));
        assert!(toggles.is_on(Toggle::Ortho) && toggles.is_on(Toggle::Osnap));
        assert!(!toggles.is_on(Toggle::Grid) && !toggles.is_on(Toggle::Polar));
        assert!(!toggles.flip(Toggle::Ortho));
        assert!(!toggles.flip(Toggle::Osnap));
        assert_eq!(toggles, Toggles::only(&[]));
    }

    #[test]
    fn every_toggle_has_its_own_bit_and_label() {
        let bits: Vec<u32> = Toggle::ALL.iter().map(|toggle| toggle.bit()).collect();
        assert_eq!(bits, vec![1, 2, 4, 8]);
        let labels: Vec<&str> = Toggle::ALL.iter().map(|toggle| toggle.label()).collect();
        assert_eq!(labels, vec!["OSNAP", "GRID", "ORTHO", "POLAR"]);
    }
}
