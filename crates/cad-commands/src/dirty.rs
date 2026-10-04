//! Dirty flags returned by every input call (ADR-0002).

use std::ops::{BitOr, BitOrAssign};

/// What changed as a result of an input. The host pulls only what is flagged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dirty(u32);

impl Dirty {
    pub const NONE: Self = Self(0);
    /// The committed scene revision changed.
    pub const SCENE: Self = Self(1);
    /// Transient/overlay geometry changed.
    pub const OVERLAY: Self = Self(1 << 1);
    /// Pan, zoom, or canvas size changed.
    pub const VIEW: Self = Self(1 << 2);
    /// Prompt, log, status, or toggles changed.
    pub const UI: Self = Self(1 << 3);
    /// A command needs the host to act (open or save a file). Take the
    /// request with `Session::take_host_request`.
    pub const HOST: Self = Self(1 << 4);

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl BitOr for Dirty {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Dirty {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_combine_and_are_queryable() {
        let mut dirty = Dirty::VIEW | Dirty::OVERLAY;
        assert!(dirty.contains(Dirty::VIEW));
        assert!(dirty.contains(Dirty::VIEW | Dirty::OVERLAY));
        assert!(!dirty.contains(Dirty::SCENE));
        dirty |= Dirty::SCENE;
        assert!(dirty.contains(Dirty::SCENE));
        assert_eq!(dirty.bits(), 0b0111);
    }

    #[test]
    fn none_is_empty() {
        assert!(Dirty::NONE.is_empty());
        assert!(Dirty::default().is_empty());
        assert!(!Dirty::UI.is_empty());
    }
}
