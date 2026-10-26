//! A plan as drafting geometry (#279, #280): its walls as the lines a
//! drafter draws, its doors, and a label in each room.
//!
//! The walls are what lies between the floors: the building's outer face,
//! and each room's floor inside its walls. So a wall's two faces are a
//! floor's side and the next floor's side, or the outer face — exactly the
//! wall's thickness apart — and where walls meet, at an L, a T, or an X,
//! nothing crosses: each floor's sides end at its own corners, and the
//! square where the walls meet is wall. A floor's side along an exterior
//! wall is that wall's inner face; any other is an interior wall's face.
//!
//! The doors are placed once the rooms are settled: one on each wall the
//! search chose — the pairs asked to open onto each other, and a spanning
//! tree from the entrance — at the middle of the stretch the two rooms
//! share, so it keeps clear of the walls at either end; and the way in, on
//! the entrance's front if that is long enough, else on its longest side
//! outside. A door is an opening cut through both faces of its wall,
//! closed by a jamb at each end, and a leaf drawn open into the room
//! further from the entrance, with its swing.

use std::f64::consts::FRAC_PI_2;

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;

use crate::search::DOOR_CLEARANCE;
use crate::{Layout, Rect, Spec};

/// One face of a wall: a side of a room's floor, or part of one where a
/// door cuts it.
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
    /// The exterior walls' outer face, counter-clockwise from the lower
    /// left: closed round the footprint, or open at the entrance, from one
    /// of its jambs round to the other.
    pub outside: Vec<Point2>,
    pub outside_closed: bool,
    /// The exterior walls' inner faces.
    pub exterior: Vec<Face>,
    /// The interior walls' faces, two to each wall.
    pub interior: Vec<Face>,
    /// The short lines across a wall that close it at a door's opening:
    /// the entrance's on an exterior wall, the rest on interior ones.
    pub exterior_jambs: Vec<(Point2, Point2)>,
    pub interior_jambs: Vec<(Point2, Point2)>,
}

/// A door: its opening through a wall, and its leaf drawn open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Door {
    /// The rooms it joins, by index; the second `None` for the way in.
    pub rooms: (usize, Option<usize>),
    /// The opening along the wall's centreline, from one jamb to the other.
    pub from: Point2,
    pub to: Point2,
    /// The thickness of the wall it is in.
    pub thickness: f64,
    /// The room it opens into: the one further from the entrance, or the
    /// entrance itself.
    pub into: usize,
    /// The hinge, at one jamb on the face of the room it opens into, and
    /// the leaf's end, a door's width into that room.
    pub hinge: Point2,
    pub leaf: Point2,
    /// The swing about the hinge, from the leaf to the other jamb,
    /// counter-clockwise, radians.
    pub swing: (f64, f64),
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

/// A door's opening, by axis: on the wall whose centreline is `x = at`
/// (`vertical`) or `y = at`, from `lo` to `hi` along it. `sides` says, for
/// each room it joins, which side of the centreline its floor is on: −1
/// for left or below, +1 for right or above; the way in has the outside
/// on the other side of the entrance.
#[derive(Clone, Copy, Debug)]
struct Cut {
    vertical: bool,
    at: f64,
    lo: f64,
    hi: f64,
    thickness: f64,
    /// (room, side) for each room the door joins.
    sides: [(usize, f64); 2],
    outside: bool,
}

impl Cut {
    fn point(&self, along: f64, across: f64) -> Point2 {
        if self.vertical {
            Point2::new(across, along)
        } else {
            Point2::new(along, across)
        }
    }
    /// The face's line on `side` of the centreline.
    fn face(&self, side: f64) -> f64 {
        self.at + side * self.thickness / 2.0
    }
}

/// The openings of a plan's doors, worked out from its rooms.
fn cuts(spec: &Spec, layout: &Layout, tolerance: LinearTolerance) -> Vec<Cut> {
    let near = |a: f64, b: f64| tolerance.eq(a, b);
    let d = spec.door;
    let mut cuts = Vec::new();
    for &(a, b) in &layout.doors {
        let (p, q) = (layout.rooms[a].rect, layout.rooms[b].rect);
        // Which side of the shared centreline each is on, and the stretch.
        let (vertical, at, lo, hi, side_a) = if near(p.right(), q.x) {
            (true, p.right(), p.y.max(q.y), p.top().min(q.top()), -1.0)
        } else if near(q.right(), p.x) {
            (true, p.x, p.y.max(q.y), p.top().min(q.top()), 1.0)
        } else if near(p.top(), q.y) {
            (false, p.top(), p.x.max(q.x), p.right().min(q.right()), -1.0)
        } else {
            (false, p.y, p.x.max(q.x), p.right().min(q.right()), 1.0)
        };
        let middle = (lo + hi) / 2.0;
        cuts.push(Cut {
            vertical,
            at,
            lo: middle - d / 2.0,
            hi: middle + d / 2.0,
            thickness: spec.interior_wall,
            sides: [(a, side_a), (b, -side_a)],
            outside: false,
        });
    }
    // The way in: the entrance's front if long enough, else its longest
    // side on the outline.
    let e = layout.entrance;
    let r = layout.rooms[e].rect;
    let o = layout.outline;
    let span = spec.door + 2.0 * DOOR_CLEARANCE + spec.exterior_wall;
    // (vertical, at, lo, hi, side of the room)
    let sides = [
        (near(r.y, o.y), false, r.y, r.x, r.right(), 1.0),
        (
            near(r.right(), o.right()),
            true,
            r.right(),
            r.y,
            r.top(),
            -1.0,
        ),
        (near(r.top(), o.top()), false, r.top(), r.x, r.right(), -1.0),
        (near(r.x, o.x), true, r.x, r.y, r.top(), 1.0),
    ];
    let front = sides[0];
    let chosen = if front.0 && front.4 - front.3 >= span {
        Some(front)
    } else {
        sides
            .into_iter()
            .filter(|s| s.0)
            .max_by(|a, b| (a.4 - a.3).total_cmp(&(b.4 - b.3)))
    };
    if let Some((_, vertical, at, lo, hi, side)) = chosen {
        let middle = (lo + hi) / 2.0;
        cuts.push(Cut {
            vertical,
            at,
            lo: middle - d / 2.0,
            hi: middle + d / 2.0,
            thickness: spec.exterior_wall,
            sides: [(e, side), (e, -side)],
            outside: true,
        });
    }
    cuts
}

/// How many doors each room is from the entrance through `layout`'s doors.
fn depths(layout: &Layout) -> Vec<usize> {
    let mut depth = vec![usize::MAX; layout.rooms.len()];
    depth[layout.entrance] = 0;
    let mut grew = true;
    while grew {
        grew = false;
        for &(a, b) in &layout.doors {
            for (x, y) in [(a, b), (b, a)] {
                if depth[x] != usize::MAX && depth[x] + 1 < depth[y] {
                    depth[y] = depth[x] + 1;
                    grew = true;
                }
            }
        }
    }
    depth
}

/// The doors of `layout`: one on each wall its search chose, and the way in.
pub fn doors(spec: &Spec, layout: &Layout) -> Vec<Door> {
    let tolerance =
        LinearTolerance::DEFAULT.at_magnitude(spec.terrain.width.max(spec.terrain.depth));
    let depth = depths(layout);
    cuts(spec, layout, tolerance)
        .into_iter()
        .map(|cut| {
            let (a, side_a) = cut.sides[0];
            let (b, side_b) = cut.sides[1];
            let (into, side) = if cut.outside || depth[a] >= depth[b] {
                (a, side_a)
            } else {
                (b, side_b)
            };
            let hinge = cut.point(cut.lo, cut.face(side));
            let leaf = cut.point(cut.lo, cut.face(side) + side * spec.door);
            // The leaf points into the room; the other jamb along the wall.
            let angle = |p: Point2| (p.y - hinge.y).atan2(p.x - hinge.x);
            let jamb = angle(cut.point(cut.hi, cut.face(side)));
            let open = angle(leaf);
            let swing = if (open - jamb).rem_euclid(std::f64::consts::TAU) <= FRAC_PI_2 + 1e-9 {
                (jamb, open)
            } else {
                (open, jamb)
            };
            Door {
                rooms: (a, if cut.outside { None } else { Some(b) }),
                from: cut.point(cut.lo, cut.at),
                to: cut.point(cut.hi, cut.at),
                thickness: cut.thickness,
                into,
                hinge,
                leaf,
                swing,
            }
        })
        .collect()
}

/// `face` less the stretch from `lo` to `hi` along it, where it lies on
/// the line `at`; whole otherwise.
fn cut_face(
    face: Face,
    vertical: bool,
    at: f64,
    lo: f64,
    hi: f64,
    tolerance: LinearTolerance,
) -> Vec<Face> {
    let along = |p: Point2| if vertical { p.y } else { p.x };
    let across = |p: Point2| if vertical { p.x } else { p.y };
    let is_vertical = tolerance.eq(face.start.x, face.end.x);
    if is_vertical != vertical || !tolerance.eq(across(face.start), at) {
        return vec![face];
    }
    let (f0, f1) = (along(face.start), along(face.end));
    let (low, high) = (f0.min(f1), f0.max(f1));
    if hi <= low || lo >= high {
        return vec![face];
    }
    let point = |v: f64| {
        if vertical {
            Point2::new(at, v)
        } else {
            Point2::new(v, at)
        }
    };
    let mut pieces = Vec::new();
    for (s, e) in [(low, lo.max(low)), (hi.min(high), high)] {
        if e - s > tolerance.value() {
            // Kept in the face's own direction.
            let (start, end) = if f0 <= f1 {
                (point(s), point(e))
            } else {
                (point(e), point(s))
            };
            pieces.push(Face {
                start,
                end,
                room: face.room,
            });
        }
    }
    pieces
}

/// The walls of `layout`, as faces cut by its doors.
pub fn walls(spec: &Spec, layout: &Layout) -> Walls {
    let tolerance =
        LinearTolerance::DEFAULT.at_magnitude(spec.terrain.width.max(spec.terrain.depth));
    let cuts = cuts(spec, layout, tolerance);
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
            let mut pieces = vec![Face {
                start: floor[side],
                end: floor[(side + 1) % 4],
                room: index,
            }];
            for cut in cuts.iter() {
                for (room, s) in cut.sides {
                    if room != index {
                        continue;
                    }
                    pieces = pieces
                        .into_iter()
                        .flat_map(|f| {
                            cut_face(f, cut.vertical, cut.face(s), cut.lo, cut.hi, tolerance)
                        })
                        .collect();
                }
            }
            if on_outline[side] {
                exterior.extend(pieces);
            } else {
                interior.extend(pieces);
            }
        }
    }
    let mut exterior_jambs = Vec::new();
    let mut interior_jambs = Vec::new();
    for cut in &cuts {
        let (a, b) = (cut.face(-1.0), cut.face(1.0));
        let jambs = [
            (cut.point(cut.lo, a), cut.point(cut.lo, b)),
            (cut.point(cut.hi, a), cut.point(cut.hi, b)),
        ];
        if cut.outside {
            exterior_jambs.extend(jambs);
        } else {
            interior_jambs.extend(jambs);
        }
    }
    // The outer face, open at the way in.
    let ring = corners(&layout.footprint);
    let entrance = cuts.iter().find(|c| c.outside);
    let (outside, outside_closed) = match entrance {
        None => (ring.to_vec(), true),
        Some(cut) => {
            // The outer face's line is on the side away from the room.
            let line = cut.face(-cut.sides[0].1);
            // Which edge of the ring, counter-clockwise from the lower left.
            let f = layout.footprint;
            let edge = if cut.vertical {
                if tolerance.eq(line, f.right()) { 1 } else { 3 }
            } else if tolerance.eq(line, f.y) {
                0
            } else {
                2
            };
            // Along the edge's own direction, the jamb met first and last.
            let (first, last) = match edge {
                0 | 1 => (cut.lo, cut.hi),
                _ => (cut.hi, cut.lo),
            };
            let mut points = vec![cut.point(last, line)];
            for k in 1..=4 {
                points.push(ring[(edge + k) % 4]);
            }
            points.push(cut.point(first, line));
            (points, false)
        }
    };
    Walls {
        outside,
        outside_closed,
        exterior,
        interior,
        exterior_jambs,
        interior_jambs,
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
