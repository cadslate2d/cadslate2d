//! A building section (#338, ADR-0040): what a vertical cut through the
//! plan, square to one of its axes, meets — worked out from the rooms the
//! cut crosses, their doors and windows, and the brief's heights and roof.
//! In metres on the section's plane: `u` along the cut, rightwards as the
//! section is seen, `z` up from the floor.
//!
//! - **Cut**, hatched: each wall the cut crosses, at its thickness, from
//!   the floor to its top — broken by a door or a window the cut passes
//!   through, the window's glass drawn; the floor slab; a flat roof's slab
//!   between the outer walls, whose parapets rise past it; a pitched
//!   roof's slopes where the cut meets them.
//! - **Beyond**, outlined: the doors and windows of each crossed room's
//!   far wall.
//!
//! A pitched roof's height anywhere is the walls' top plus its pitch times
//! the distance to the nearest eaves line it slopes from: a gable's two
//! long sides, a hip's four.

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;

use crate::elevation::{GROUND_PAST, ROOF_THICKNESS};
use crate::{Resolved, Roof, Spec};

/// Where a section cuts and which way it looks: a line square to the
/// plan's x axis (`along_x` false: the line x = `at`) or to its y axis
/// (`along_x` true: the line y = `at`), looking towards the greater
/// coordinate or the lesser.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cut {
    pub along_x: bool,
    pub at: f64,
    pub looking_up: bool,
}

/// A section, as outlines and lines on its plane.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    /// What the cut passes through: closed outlines to hatch.
    pub cut: Vec<Vec<Point2>>,
    /// What is seen beyond the cut: closed outlines.
    pub beyond: Vec<Vec<Point2>>,
    /// Lines: a cut window's glass.
    pub lines: Vec<(Point2, Point2)>,
    pub ground: (Point2, Point2),
    /// The section's length along the cut, and its height.
    pub width: f64,
    pub height: f64,
    /// Where a plan coordinate along the cut falls on the section:
    /// `u = (x − origin) × direction`.
    pub origin: f64,
    pub direction: f64,
}

impl Section {
    /// Where the plan coordinate `along` the cut falls on the section.
    pub fn u(&self, along: f64) -> f64 {
        (along - self.origin) * self.direction
    }
}

fn p(u: f64, z: f64) -> Point2 {
    Point2::new(u, z)
}

fn rectangle(u0: f64, z0: f64, u1: f64, z1: f64) -> Vec<Point2> {
    let (a, b) = (u0.min(u1), u0.max(u1));
    vec![p(a, z0), p(b, z0), p(b, z1), p(a, z1)]
}

/// Why a cut is refused rather than drawn wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CutError {
    /// The line misses the building.
    Misses,
    /// The line runs inside a wall, along it: the section would be the
    /// wall's length of solid wall, which is not what a section is for.
    AlongWall,
}

/// The cut of the plan along `cut`, or why it is refused.
pub fn section(spec: &Spec, plan: &Resolved, cut: Cut) -> Result<Section, CutError> {
    let layout = &plan.layout;
    let building = spec.building;
    let fp = layout.footprint;
    let outline = layout.outline;
    let tolerance = 1e-6 * (fp.w + fp.h).max(1.0);
    let near = |a: f64, b: f64| (a - b).abs() <= tolerance;
    // Along the cut and across it, in plan coordinates.
    let along = |q: Point2| if cut.along_x { q.x } else { q.y };
    let across = |q: Point2| if cut.along_x { q.y } else { q.x };
    let (start, end) = if cut.along_x {
        (fp.x, fp.right())
    } else {
        (fp.y, fp.top())
    };
    let (side_lo, side_hi) = if cut.along_x {
        (fp.y, fp.top())
    } else {
        (fp.x, fp.right())
    };
    if !(cut.at > side_lo && cut.at < side_hi) {
        return Err(CutError::Misses);
    }
    // Inside a wall running along the cut: within half its thickness of a
    // room's side along it.
    let linear = LinearTolerance::DEFAULT.at_magnitude(fp.w + fp.h);
    let (outer_lo, outer_hi) = outline_sides(layout, !cut.along_x);
    for room in &layout.rooms {
        let r = room.rect;
        let sides = if cut.along_x {
            [r.y, r.top()]
        } else {
            [r.x, r.right()]
        };
        for side in sides {
            let exterior = linear.eq(side, outer_lo) || linear.eq(side, outer_hi);
            let thickness = if exterior {
                spec.exterior_wall
            } else {
                spec.interior_wall
            };
            if (cut.at - side).abs() <= thickness / 2.0 {
                return Err(CutError::AlongWall);
            }
        }
    }
    // Facing +y the right hand is +x; facing +x it is −y.
    let direction = if cut.along_x == cut.looking_up {
        1.0
    } else {
        -1.0
    };
    let origin = if direction > 0.0 { start } else { end };
    let len = end - start;
    let mut out = Section {
        width: len,
        ground: (p(-GROUND_PAST, 0.0), p(len + GROUND_PAST, 0.0)),
        origin,
        direction,
        ..Section::default()
    };
    let u = |x: f64| (x - origin) * direction;

    // The roof's underside at `u`: the walls' top plus the pitch times the
    // distance to the nearest eaves it slopes from; `None` for a flat roof.
    let rise = building.roof_pitch.to_radians().tan();
    let ridge_along_x = fp.w >= fp.h;
    let slopes_along_cut = ridge_along_x != cut.along_x;
    let from_side = (cut.at - side_lo).min(side_hi - cut.at);
    // How far the cut is from the eaves it does not run across.
    let other = match (building.roof, slopes_along_cut) {
        (Roof::Flat, _) => None,
        // Across the ridge: a gable's slopes only; a hip's ends too.
        (Roof::Gable, true) => Some(f64::INFINITY),
        (Roof::Hip, true) => Some(from_side),
        // Along the ridge: the slope it lies in.
        (_, false) => Some(from_side),
    };
    let wall = spec.storey_height;
    let underside = |u: f64| -> f64 {
        let b = other.unwrap_or(0.0);
        let a = if slopes_along_cut || building.roof == Roof::Hip {
            u.min(len - u)
        } else {
            f64::INFINITY
        };
        wall + a.min(b) * rise
    };

    // The walls the cut crosses: the sides of the rooms it passes through.
    let mut centres: Vec<f64> = Vec::new();
    let mut crossed: Vec<(usize, f64, f64)> = Vec::new();
    for (i, room) in layout.rooms.iter().enumerate() {
        let r = room.rect;
        let (lo, hi, a0, a1) = if cut.along_x {
            (r.x, r.right(), r.y, r.top())
        } else {
            (r.y, r.top(), r.x, r.right())
        };
        if cut.at > a0 + tolerance && cut.at < a1 - tolerance {
            crossed.push((i, lo, hi));
            for c in [lo, hi] {
                if !centres.iter().any(|&w| near(w, c)) {
                    centres.push(c);
                }
            }
        }
    }
    let (outline_lo, outline_hi) = if cut.along_x {
        (outline.x, outline.right())
    } else {
        (outline.y, outline.top())
    };
    let te = spec.exterior_wall;
    let flat_top = wall + building.slab + building.parapet;
    let (all_doors, all_windows) = (&plan.doors, &plan.windows);
    // An opening through the wall at `c` where the cut crosses it.
    let opening_at = |from: Point2, to: Point2, c: f64| {
        near(along(from), c)
            && near(along(to), c)
            && cut.at > across(from).min(across(to))
            && cut.at < across(from).max(across(to))
    };
    for &c in &centres {
        let exterior = near(c, outline_lo) || near(c, outline_hi);
        let t = if exterior { te } else { spec.interior_wall };
        let (u0, u1) = (u(c - t / 2.0), u(c + t / 2.0));
        let (u0, u1) = (u0.min(u1), u0.max(u1));
        let top = match (exterior, building.roof) {
            (true, Roof::Flat) => flat_top,
            (true, _) => underside(u0).min(underside(u1)),
            (false, _) => wall,
        };
        let window = all_windows.iter().find(|w| opening_at(w.from, w.to, c));
        let door = all_doors.iter().find(|d| opening_at(d.from, d.to, c));
        if let Some(window) = window {
            out.cut.push(rectangle(u0, 0.0, u1, building.sill));
            out.cut.push(rectangle(u0, building.window_head, u1, top));
            let middle = u(c);
            let glass = window.thickness / 6.0;
            for g in [middle - glass, middle + glass] {
                out.lines
                    .push((p(g, building.sill), p(g, building.window_head)));
            }
        } else if door.is_some() {
            out.cut.push(rectangle(u0, building.door_head, u1, top));
        } else {
            out.cut.push(rectangle(u0, 0.0, u1, top));
        }
    }

    // The floor, and the roof.
    out.cut.push(rectangle(0.0, -building.slab, len, 0.0));
    match building.roof {
        Roof::Flat => {
            out.cut
                .push(rectangle(te, wall, len - te, wall + building.slab));
            out.height = flat_top;
        }
        Roof::Gable | Roof::Hip => {
            // The underside's corners: past the eaves, where the slopes
            // meet the plateau or each other, and past the far eaves.
            let e = building.eaves;
            let b = other.unwrap_or(0.0);
            let mut us = vec![-e];
            if slopes_along_cut || building.roof == Roof::Hip {
                if b < len / 2.0 {
                    us.extend([b, len - b]);
                } else {
                    us.push(len / 2.0);
                }
            }
            us.push(len + e);
            let under: Vec<Point2> = us.iter().map(|&u| p(u, underside(u))).collect();
            let mut ring = under.clone();
            ring.extend(under.iter().rev().map(|q| p(q.x, q.y + ROOF_THICKNESS)));
            out.height = ring.iter().map(|q| q.y).fold(wall, f64::max);
            out.cut.push(ring);
        }
    }

    // Beyond: the far wall of each room crossed, its doors and windows.
    let far = |i: usize| {
        let r = layout.rooms[i].rect;
        match (cut.along_x, cut.looking_up) {
            (true, true) => r.top(),
            (true, false) => r.y,
            (false, true) => r.right(),
            (false, false) => r.x,
        }
    };
    for &(i, lo, hi) in &crossed {
        let wall_at = far(i);
        let on_wall =
            |from: Point2, to: Point2| near(across(from), wall_at) && near(across(to), wall_at);
        let span = |from: Point2, to: Point2| {
            let (a, b) = (along(from).min(along(to)), along(from).max(along(to)));
            let (a, b) = (a.max(lo), b.min(hi));
            (b > a).then(|| (u(a), u(b)))
        };
        for w in all_windows.iter().filter(|w| on_wall(w.from, w.to)) {
            if let Some((a, b)) = span(w.from, w.to) {
                out.beyond
                    .push(rectangle(a, building.sill, b, building.window_head));
            }
        }
        for d in all_doors.iter().filter(|d| on_wall(d.from, d.to)) {
            if let Some((a, b)) = span(d.from, d.to) {
                out.beyond.push(rectangle(a, 0.0, b, building.door_head));
            }
        }
    }
    Ok(out)
}

/// The outline's x sides — its left and right — or its y sides.
fn outline_sides(layout: &crate::Layout, x_sides: bool) -> (f64, f64) {
    let o = layout.outline;
    if x_sides {
        (o.x, o.right())
    } else {
        (o.y, o.top())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Building, Room, Setbacks, Terrain, solve};

    fn house(roof: Roof) -> (Spec, Resolved) {
        let spec = Spec {
            terrain: Terrain {
                width: 14.0,
                depth: 20.0,
            },
            setbacks: Setbacks {
                front: 3.0,
                ..Setbacks::default()
            },
            rooms: vec![
                Room {
                    entrance: true,
                    ..Room::new("living", 24.0)
                },
                Room::new("kitchen", 10.0),
                Room::new("bedroom", 12.0),
            ],
            adjacent: vec![["living".into(), "kitchen".into()]],
            exterior_wall: 0.25,
            interior_wall: 0.15,
            door: 0.9,
            max_circulation: 0.15,
            windows: Some(1.2),
            storey_height: 2.8,
            building: Building {
                roof,
                ..Building::default()
            },
            seed: 0,
        };
        let layout = solve(&spec).unwrap();
        let plan = Resolved::of(&spec, &layout);
        (spec, plan)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn widths(section: &Section) -> Vec<f64> {
        // The cut walls: outlines standing on the floor or broken above it.
        section
            .cut
            .iter()
            .filter(|ring| ring.len() == 4)
            .map(|ring| ring[1].x - ring[0].x)
            .filter(|w| *w < 0.3)
            .collect()
    }

    #[test]
    fn every_wall_the_cut_crosses_is_cut_at_its_thickness() {
        let (spec, plan) = house(Roof::Flat);
        let layout = &plan.layout;
        let fp = layout.footprint;
        for along_x in [true, false] {
            let at = if along_x {
                fp.y + fp.h * 0.37
            } else {
                fp.x + fp.w * 0.37
            };
            let s = section(
                &spec,
                &plan,
                Cut {
                    along_x,
                    at,
                    looking_up: true,
                },
            )
            .unwrap();
            let w = widths(&s);
            // Two outer walls at 0.25 at least, and every wall's cut is
            // 0.25 or 0.15 thick.
            assert!(w.iter().filter(|&&t| close(t, 0.25)).count() >= 2, "{w:?}");
            assert!(w.iter().all(|&t| close(t, 0.25) || close(t, 0.15)), "{w:?}");
            // The outer walls are at the section's two ends.
            assert!(
                s.cut
                    .iter()
                    .any(|r| close(r[0].x, 0.0) && close(r[1].x, 0.25))
            );
            let len = s.width;
            assert!(
                s.cut
                    .iter()
                    .any(|r| close(r[1].x, len) && close(r[0].x, len - 0.25))
            );
            // The roof slab between them, the floor under all.
            assert!(
                s.cut
                    .iter()
                    .any(|r| close(r[0].y, 2.8) && close(r[2].y, 3.0))
            );
            assert!(
                s.cut
                    .iter()
                    .any(|r| close(r[0].y, -0.2) && close(r[2].y, 0.0))
            );
            assert!(close(s.height, 3.5));
        }
    }

    #[test]
    fn a_window_cut_through_breaks_its_wall_and_shows_its_glass() {
        let (spec, plan) = house(Roof::Flat);
        let layout = &plan.layout;
        // Through the middle of a window in the front wall.
        let window = plan
            .windows
            .clone()
            .into_iter()
            .find(|w| (w.from.y - layout.outline.y).abs() < 1e-6)
            .expect("a front window");
        let x = (window.from.x + window.to.x) / 2.0;
        let s = section(
            &spec,
            &plan,
            Cut {
                along_x: false,
                at: x,
                looking_up: true,
            },
        )
        .unwrap();
        // The front wall: below the sill and above the head.
        let u = s.u(layout.outline.y);
        let at_front: Vec<&Vec<Point2>> =
            s.cut.iter().filter(|r| r[0].x < u && r[1].x > u).collect();
        assert!(
            at_front
                .iter()
                .any(|r| close(r[0].y, 0.0) && close(r[2].y, 0.9))
        );
        assert!(at_front.iter().any(|r| close(r[0].y, 2.1)));
        assert_eq!(s.lines.len(), 2, "two lines of glass");
    }

    #[test]
    fn a_cut_inside_a_wall_is_refused() {
        let (spec, plan) = house(Roof::Flat);
        let layout = &plan.layout;
        // Along the front wall's centreline, and a hair inside its face.
        for at in [layout.outline.y, layout.outline.y + 0.125] {
            let cut = Cut {
                along_x: true,
                at,
                looking_up: true,
            };
            assert_eq!(section(&spec, &plan, cut), Err(CutError::AlongWall), "{at}");
        }
        // Along an interior wall: the side two rooms share.
        let shared = layout
            .rooms
            .iter()
            .map(|r| r.rect.y)
            .find(|&y| y > layout.outline.y + 1e-6)
            .expect("rooms one above another");
        let cut = Cut {
            along_x: true,
            at: shared + 0.07,
            looking_up: true,
        };
        assert_eq!(section(&spec, &plan, cut), Err(CutError::AlongWall));
    }

    #[test]
    fn a_pitched_roof_rises_from_its_eaves_at_its_pitch() {
        let (spec, plan) = house(Roof::Gable);
        let layout = &plan.layout;
        let fp = layout.footprint;
        let rise = 30_f64.to_radians().tan();
        // Across the ridge: the gable's two slopes and their apex.
        let ridge_along_x = fp.w >= fp.h;
        let across = Cut {
            along_x: !ridge_along_x,
            at: if ridge_along_x {
                fp.x + fp.w / 2.0
            } else {
                fp.y + fp.h / 2.0
            },
            looking_up: true,
        };
        let s = section(&spec, &plan, across).unwrap();
        let roof = s.cut.last().unwrap();
        let span = s.width;
        assert!(
            roof.iter()
                .any(|q| close(q.x, span / 2.0) && close(q.y, 2.8 + span / 2.0 * rise))
        );
        assert!(
            roof.iter()
                .any(|q| close(q.x, -0.5) && close(q.y, 2.8 - 0.5 * rise))
        );
        assert!(close(s.height, 2.8 + span / 2.0 * rise + ROOF_THICKNESS));
        // Along the ridge, a quarter in: a level band at the slope's height
        // there.
        let quarter = fp.w.min(fp.h) / 4.0;
        let along = Cut {
            along_x: ridge_along_x,
            at: if ridge_along_x {
                fp.y + quarter
            } else {
                fp.x + quarter
            },
            looking_up: false,
        };
        let s = section(&spec, &plan, along).unwrap();
        let roof = s.cut.last().unwrap();
        let z = 2.8 + quarter * rise;
        assert!(
            roof[..roof.len() / 2].iter().all(|q| close(q.y, z)),
            "{roof:?}"
        );
    }

    #[test]
    fn a_cut_outside_the_building_is_none_and_the_view_sets_the_way_u_runs() {
        let (spec, plan) = house(Roof::Flat);
        let layout = &plan.layout;
        let fp = layout.footprint;
        assert!(
            section(
                &spec,
                &plan,
                Cut {
                    along_x: true,
                    at: fp.y - 1.0,
                    looking_up: true
                }
            ) == Err(CutError::Misses)
        );
        let at = fp.y + fp.h / 2.0;
        let up = section(
            &spec,
            &plan,
            Cut {
                along_x: true,
                at,
                looking_up: true,
            },
        )
        .unwrap();
        let down = section(
            &spec,
            &plan,
            Cut {
                along_x: true,
                at,
                looking_up: false,
            },
        )
        .unwrap();
        // Facing +y, x runs rightwards; facing −y, leftwards.
        assert!(close(up.u(fp.x), 0.0) && close(up.u(fp.right()), fp.w));
        assert!(close(down.u(fp.right()), 0.0) && close(down.u(fp.x), fp.w));
    }
}
