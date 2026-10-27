//! Floor-plan layout synthesis (ADR-0039, #277, #278): rooms of given
//! areas placed on a rectangular terrain, or the reasons none fits.
//!
//! A plan is a *slicing* of the building's footprint: cut it across or
//! along, and each part again, until each part is a room. A slicing is
//! written as a normalised Polish expression (Wong and Liu), and searched
//! by simulated annealing from a seed. For a given slicing and footprint
//! width the rooms' sizes follow: each cut divides its rectangle in the
//! proportion of the floor areas on either side, the walls' thicknesses
//! taken off, so every room's area is its target as nearly as the walls
//! allow. The search moves the cuts and the width to meet the hard
//! constraints — inside the buildable area, minimum widths, aspect ratios,
//! adjacency with a door, every room reachable from the entrance — and to
//! lower the soft terms:
//!
//! `E = w_a·E_area + w_r·E_ratio + w_c·E_connectivity + w_p·E_preferences`.
//!
//! The same brief and seed always make the same plan; other seeds make the
//! alternatives. Everything is in metres, on wall centrelines: a room's
//! rectangle runs to the middle of its walls, and its area is what lies
//! inside them.

mod draft;
mod elevation;
mod search;
mod section;
mod spec;
mod verify;

use cad_core::tolerance::LinearTolerance;

pub use draft::{
    Door, Face, LEAF_THICKNESS, Label, WINDOW_CLEARANCE, WINDOW_MIN, Walls, Window, doors, labels,
    walls, windows,
};
pub use elevation::{Elevation, Side, elevation};
pub use search::{alternatives, alternatives_within, searches_for, solve};
pub use section::{Cut, Section, section};
pub use spec::{Building, MAX_ROOMS, Roof, Room, Setbacks, Spec, SpecError, Terrain};
pub use verify::verify;

/// An axis-aligned rectangle: its lower left corner and its size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn top(&self) -> f64 {
        self.y + self.h
    }
    pub fn area(&self) -> f64 {
        self.w.max(0.0) * self.h.max(0.0)
    }
    /// Its shorter side over its longer, 1 for a square; 0 when empty.
    pub fn aspect(&self) -> f64 {
        let (short, long) = (self.w.min(self.h), self.w.max(self.h));
        if short <= 0.0 {
            f64::INFINITY
        } else {
            long / short
        }
    }
    /// Shrunk by `left`, `right`, `bottom`, `top`.
    #[must_use]
    pub fn inset(&self, left: f64, right: f64, bottom: f64, top: f64) -> Self {
        Self::new(
            self.x + left,
            self.y + bottom,
            self.w - left - right,
            self.h - bottom - top,
        )
    }

    /// How long a stretch of edge this and `other` share: they touch side
    /// to side, and overlap along it. Zero when they do not.
    pub fn shared(&self, other: &Rect, tolerance: LinearTolerance) -> f64 {
        let overlap = |a0: f64, a1: f64, b0: f64, b1: f64| (a1.min(b1) - a0.max(b0)).max(0.0);
        if tolerance.eq(self.right(), other.x) || tolerance.eq(other.right(), self.x) {
            overlap(self.y, self.top(), other.y, other.top())
        } else if tolerance.eq(self.top(), other.y) || tolerance.eq(other.top(), self.y) {
            overlap(self.x, self.right(), other.x, other.right())
        } else {
            0.0
        }
    }

    /// The length of each of its sides — left, right, bottom, top — that
    /// lies on `outline`'s edge; zero for a side inside it.
    pub fn outside_sides(&self, outline: &Rect, tolerance: LinearTolerance) -> [f64; 4] {
        let on = |a: f64, b: f64, length: f64| if tolerance.eq(a, b) { length } else { 0.0 };
        [
            on(self.x, outline.x, self.h),
            on(self.right(), outline.right(), self.h),
            on(self.y, outline.y, self.w),
            on(self.top(), outline.top(), self.w),
        ]
    }

    /// The longest of its sides on `outline`'s edge: where a door or a
    /// window to the outside can go.
    pub fn longest_outside(&self, outline: &Rect, tolerance: LinearTolerance) -> f64 {
        self.outside_sides(outline, tolerance)
            .into_iter()
            .fold(0.0, f64::max)
    }
}

/// What the circulation space is called.
pub const CIRCULATION: &str = "circulation";

/// A room of a plan.
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedRoom {
    pub name: String,
    /// To the middle of its walls.
    pub rect: Rect,
    /// Inside its walls: its floor.
    pub inner: Rect,
}

impl PlacedRoom {
    /// Its floor area.
    pub fn area(&self) -> f64 {
        self.inner.area()
    }
}

/// A plan: the building's outline, its rooms, and the walls that take a
/// door — the pairs asked to open onto each other, and enough others that
/// every room is reached from the entrance.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    /// The outside faces of the exterior walls.
    pub footprint: Rect,
    /// The exterior walls' centreline: what the rooms tile.
    pub outline: Rect,
    /// The brief's rooms in its order, then the circulation space if the
    /// plan has one.
    pub rooms: Vec<PlacedRoom>,
    /// The circulation space's index in `rooms`: a hall or passage the
    /// search adds where the rooms alone cannot be shaped or reached.
    pub circulation: Option<usize>,
    pub entrance: usize,
    /// Pairs of rooms, by index, with a door between them.
    pub doors: Vec<(usize, usize)>,
    /// The soft terms' sum: lower is better. The alternatives are ranked by it.
    pub energy: f64,
    /// The seed that made it.
    pub seed: u64,
}

/// Why no plan was made. A refusal says which of three things happened,
/// and the AI and the command say it as they are told: a brief that
/// cannot be read, one that has no plan, and one whose plan the search
/// did not find.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The brief cannot be read: a size that is not one, a name twice.
    Invalid(String),
    /// No plan exists, whatever the search: a necessary condition fails.
    /// Each reason a sentence.
    Infeasible { reasons: Vec<String> },
    /// None was found in the search's budget, though one may exist: the
    /// constraints the nearest attempt broke, each a sentence.
    Exhausted {
        reasons: Vec<String>,
        searches: usize,
    },
}

impl Refusal {
    /// What it says, a sentence each.
    pub fn reasons(&self) -> Vec<String> {
        match self {
            Self::Invalid(reason) => vec![reason.clone()],
            Self::Infeasible { reasons } => reasons.clone(),
            Self::Exhausted { reasons, searches } => {
                let mut said = vec![format!(
                    "No plan was found in {searches} searches, though one may exist; the nearest broke these:"
                )];
                said.extend(reasons.iter().cloned());
                said
            }
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reasons().join(" "))
    }
}

impl std::error::Error for Refusal {}

#[cfg(test)]
mod tests;
