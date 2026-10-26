//! A plan as drafting geometry (#279): its walls as the lines a drafter
//! draws, and a label in each room.
//!
//! The walls are what lies between the floors: the building's outer face,
//! and each room's floor inside its walls. So a wall's two faces are a
//! floor's side and the next floor's side, or the outer face — exactly the
//! wall's thickness apart — and where walls meet, at an L, a T, or an X,
//! nothing crosses: each floor's sides end at its own corners, and the
//! square where the walls meet is wall. A floor's side along an exterior
//! wall is that wall's inner face; any other is an interior wall's face.

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;

use crate::{Layout, Rect, Spec};

/// One face of a wall: a side of a room's floor.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Face {
    pub start: Point2,
    pub end: Point2,
    /// The room, by its index in the plan, whose floor this bounds.
    pub room: usize,
}

/// The walls of a plan.
#[derive(Clone, Debug, PartialEq)]
pub struct Walls {
    /// The exterior walls' outer face: the footprint's corners,
    /// counter-clockwise from the lower left, closed.
    pub outside: [Point2; 4],
    /// The exterior walls' inner faces.
    pub exterior: Vec<Face>,
    /// The interior walls' faces, two to each wall.
    pub interior: Vec<Face>,
}

/// A room's label: its name and its area, at the middle of its floor.
#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub at: Point2,
    pub name: String,
    /// Its floor's area, m².
    pub area: f64,
}

fn corners(r: &Rect) -> [Point2; 4] {
    [
        Point2::new(r.x, r.y),
        Point2::new(r.right(), r.y),
        Point2::new(r.right(), r.top()),
        Point2::new(r.x, r.top()),
    ]
}

/// The walls of `layout`, as faces.
pub fn walls(spec: &Spec, layout: &Layout) -> Walls {
    let tolerance =
        LinearTolerance::DEFAULT.at_magnitude(spec.terrain.width.max(spec.terrain.depth));
    let mut exterior = Vec::new();
    let mut interior = Vec::new();
    for (index, room) in layout.rooms.iter().enumerate() {
        let floor = corners(&room.inner);
        // Bottom, right, top, left: the order `outside_sides` gives
        // left, right, bottom, top in.
        let outside = room.rect.outside_sides(&layout.outline, tolerance);
        let on_outline = [
            outside[2] > 0.0,
            outside[1] > 0.0,
            outside[3] > 0.0,
            outside[0] > 0.0,
        ];
        for side in 0..4 {
            let face = Face {
                start: floor[side],
                end: floor[(side + 1) % 4],
                room: index,
            };
            if on_outline[side] {
                exterior.push(face);
            } else {
                interior.push(face);
            }
        }
    }
    Walls {
        outside: corners(&layout.footprint),
        exterior,
        interior,
    }
}

/// A label for every room, the circulation's included.
pub fn labels(layout: &Layout) -> Vec<Label> {
    layout
        .rooms
        .iter()
        .map(|room| Label {
            at: Point2::new(
                room.inner.x + room.inner.w / 2.0,
                room.inner.y + room.inner.h / 2.0,
            ),
            name: room.name.clone(),
            area: room.area(),
        })
        .collect()
}
