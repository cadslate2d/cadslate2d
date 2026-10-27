//! Drafting aids that move the pointer (DESIGN §13.4): grid snap, which
//! puts the free cursor on a grid, and ORTHO and POLAR, which constrain
//! its direction.
//!
//! ORTHO and POLAR work from the anchor of the rubber band — the point the
//! active command is drawing from. Every aid applies to the pointer only.
//! The order of precedence for a point is fixed:
//!
//! ```text
//! typed coordinates  >  object snap  >  grid snap, then ORTHO or POLAR  >  free cursor
//! ```
//!
//! ORTHO and POLAR are never on together: both claim the same rubber band.
//! They constrain the grid-snapped point, as a classic drafter expects.

use cad_core::tolerance::AngularTolerance;
use cad_core::{PixelTolerance, Point2, Vec2};
use cad_geometry::unit_at_degrees;

/// How close, on screen, the cursor must be to a polar direction to lock
/// onto it. A display tolerance, like the snap aperture (§18.3).
pub const POLAR_APERTURE: PixelTolerance = PixelTolerance(8.0);

/// Which aid constrained the pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackKind {
    Ortho,
    Polar,
    /// Object snap tracking (#220): along a path from an acquired point.
    Object,
}

/// A pointer position constrained to a direction from a base point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    pub kind: TrackKind,
    /// The point the direction is measured from.
    pub from: Point2,
    /// Where a click would land.
    pub point: Point2,
    /// The constrained direction, in degrees counter-clockwise from +X, in `[0, 360)`.
    pub angle: f64,
}

impl Track {
    pub fn distance(&self) -> f64 {
        self.from.distance_to(self.point)
    }

    /// The readout for the status bar, in the classic form
    /// `Polar: 50.0000 < 45°`.
    pub fn label(&self) -> String {
        let aid = match self.kind {
            TrackKind::Ortho => "Ortho",
            TrackKind::Polar => "Polar",
            TrackKind::Object => "Tracking",
        };
        // Up to two decimals, without trailing zeros: 45°, 22.5°, 33.33°.
        let angle = format!("{:.2}", self.angle);
        let angle = angle.trim_end_matches('0').trim_end_matches('.');
        format!("{aid}: {:.4} < {angle}°", self.distance())
    }
}

/// How close, on screen, the cursor must be to an object tracking path to
/// follow it (#220).
pub const TRACKING_APERTURE: PixelTolerance = PixelTolerance(8.0);

/// The most points object snap tracking holds at once; acquiring another
/// lets the oldest go.
pub const MAX_ACQUIRED: usize = 7;

/// Where object snap tracking puts the pointer (#220): on one alignment
/// path, or where two cross.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tracked {
    pub point: Point2,
    /// The path the point is on, from its acquired point.
    pub first: Track,
    /// The other path, when the point is where two cross.
    pub second: Option<Track>,
}

impl Tracked {
    /// The status bar's readout: `Tracking: 50.0000 < 0°`, or where two
    /// paths cross.
    pub fn label(&self) -> String {
        match self.second {
            None => self.first.label(),
            Some(_) => "Tracking: intersection".to_owned(),
        }
    }
}

/// OTRACK (#220): the cursor on an alignment path — a line through an
/// acquired point at a multiple of `increment` degrees — when it is within
/// `reach` (world units) of one, and at the crossing of two from different
/// points when it is within reach of both. `None` leaves the cursor free.
pub fn object_tracking(
    acquired: &[Point2],
    cursor: Point2,
    increment: f64,
    reach: f64,
) -> Option<Tracked> {
    if !(increment.is_finite() && increment > 0.0) {
        return None;
    }
    let lines = ((180.0 / increment).round() as usize).clamp(1, 360);
    // Every path near the cursor: its point, its direction, and how far.
    let mut near: Vec<(Point2, Vec2, f64)> = Vec::new();
    for from in acquired {
        let offset = cursor - *from;
        if offset.length() <= reach {
            continue;
        }
        for step in 0..lines {
            let along = unit_at_degrees(step as f64 * increment);
            let aside = (offset.x * along.y - offset.y * along.x).abs();
            if aside <= reach {
                near.push((*from, along, aside));
            }
        }
    }
    let track = |from: Point2, point: Point2| {
        let offset = point - from;
        Track {
            kind: TrackKind::Object,
            from,
            point,
            angle: offset.y.atan2(offset.x).to_degrees().rem_euclid(360.0),
        }
    };
    // Two paths from different points crossing near the cursor.
    let mut crossing: Option<(f64, Tracked)> = None;
    for (index, (from_a, along_a, _)) in near.iter().enumerate() {
        for (from_b, along_b, _) in &near[index + 1..] {
            let sine = along_a.x * along_b.y - along_a.y * along_b.x;
            if from_a == from_b || AngularTolerance::DEFAULT.is_zero(sine) {
                continue;
            }
            let between = *from_b - *from_a;
            let t = (between.x * along_b.y - between.y * along_b.x) / sine;
            let point = *from_a + *along_a * t;
            let off = point.distance_to(cursor);
            if off <= 2.0 * reach && crossing.is_none_or(|(best, _)| off < best) {
                crossing = Some((
                    off,
                    Tracked {
                        point,
                        first: track(*from_a, point),
                        second: Some(track(*from_b, point)),
                    },
                ));
            }
        }
    }
    if let Some((_, tracked)) = crossing {
        return Some(tracked);
    }
    let (from, along, _) = near.into_iter().min_by(|a, b| a.2.total_cmp(&b.2))?;
    let point = from + along * (cursor - from).dot(along);
    Some(Tracked {
        point,
        first: track(from, point),
        second: None,
    })
}

/// SNAP (#219): `point` moved to the nearest point of the grid whose
/// spacing across and up is `spacing`, from the origin. A spacing that is
/// not a positive number leaves that coordinate free.
pub fn grid_snap(point: Point2, spacing: [f64; 2]) -> Point2 {
    let snap = |value: f64, step: f64| {
        if step.is_finite() && step > 0.0 {
            // Adding zero turns a negative zero — a point just below the
            // origin rounded up to it — into zero, which a file writes as 0.
            (value / step).round() * step + 0.0
        } else {
            value
        }
    };
    Point2::new(snap(point.x, spacing[0]), snap(point.y, spacing[1]))
}

/// ORTHO: the cursor constrained to horizontal or vertical from `base`,
/// whichever it is further along.
pub fn ortho(base: Point2, cursor: Point2) -> Track {
    let offset = cursor - base;
    let (point, angle) = if offset.x.abs() >= offset.y.abs() {
        let angle = if offset.x >= 0.0 { 0.0 } else { 180.0 };
        (Point2::new(cursor.x, base.y), angle)
    } else {
        let angle = if offset.y >= 0.0 { 90.0 } else { 270.0 };
        (Point2::new(base.x, cursor.y), angle)
    };
    Track {
        kind: TrackKind::Ortho,
        from: base,
        point,
        angle,
    }
}

/// ORTHO on an isometric plane (#336): the cursor constrained to the
/// plane's axis, either way, nearest the direction it is in from `base`.
pub fn iso_ortho(base: Point2, cursor: Point2, plane: cad_core::iso::Plane) -> Track {
    let offset = cursor - base;
    let along = plane
        .axes()
        .into_iter()
        .max_by(|a, b| offset.dot(*a).abs().total_cmp(&offset.dot(*b).abs()))
        .expect("two axes");
    let distance = offset.dot(along);
    let direction = if distance >= 0.0 { along } else { along * -1.0 };
    Track {
        kind: TrackKind::Ortho,
        from: base,
        point: base + along * distance,
        angle: direction
            .y
            .atan2(direction.x)
            .to_degrees()
            .rem_euclid(360.0),
    }
}

/// POLAR: if the cursor is within `reach` (world units) of a direction that
/// is a multiple of `increment` degrees from `base`, the cursor projected
/// onto that direction. `None` leaves the cursor free.
pub fn polar(base: Point2, cursor: Point2, increment: f64, reach: f64) -> Option<Track> {
    let offset = cursor - base;
    // Too close to the base point, every direction is within reach.
    let usable = increment.is_finite() && increment > 0.0;
    if !usable || offset.length() <= reach {
        return None;
    }
    let steps = (offset.y.atan2(offset.x).to_degrees() / increment).round();
    let angle = (steps * increment).rem_euclid(360.0);
    let along = unit_at_degrees(angle);
    let distance = offset.x * along.x + offset.y * along.y;
    let aside = (offset.x * along.y - offset.y * along.x).abs();
    (distance > 0.0 && aside <= reach).then(|| Track {
        kind: TrackKind::Polar,
        from: base,
        point: base + along * distance,
        angle,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn object_tracking_follows_a_path_and_takes_a_crossing() {
        let corner = p(0.0, 0.0);
        let other = p(100.0, 50.0);
        // Near the horizontal through the corner: on it.
        let tracked = object_tracking(&[corner], p(60.0, 3.0), 90.0, 5.0).unwrap();
        assert_eq!(tracked.point, p(60.0, 0.0));
        assert_eq!((tracked.first.angle, tracked.second), (0.0, None));
        assert_eq!(tracked.label(), "Tracking: 60.0000 < 0°");
        // Level with one point and above the other: the crossing.
        let tracked = object_tracking(&[corner, other], p(2.0, 48.0), 90.0, 5.0).unwrap();
        assert!(tracked.point.distance_to(p(0.0, 50.0)) < 1e-9);
        assert_eq!(tracked.label(), "Tracking: intersection");
        // Far from every path, or at the point itself: free.
        assert_eq!(object_tracking(&[corner], p(40.0, 40.0), 90.0, 5.0), None);
        assert_eq!(object_tracking(&[corner], p(1.0, 1.0), 90.0, 5.0), None);
        // With a polar increment of 45°, the diagonal too.
        let tracked = object_tracking(&[corner], p(30.0, 32.0), 45.0, 5.0).unwrap();
        assert!(tracked.point.distance_to(p(31.0, 31.0)) < 1e-9);
        assert!((tracked.first.angle - 45.0).abs() < 1e-9);
    }

    #[test]
    fn isometric_ortho_follows_the_planes_axes() {
        use cad_core::iso::Plane;
        let base = p(10.0, 10.0);
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        // Up and a little right: on the top plane, along 30°; on the left,
        // straight up; on the right, up too.
        let cursor = p(14.0, 20.0);
        let top = iso_ortho(base, cursor, Plane::Top);
        assert!(close(top.angle, 30.0), "{}", top.angle);
        let left = iso_ortho(base, cursor, Plane::Left);
        assert!(close(left.angle, 90.0));
        assert!(close(left.point.x, 10.0));
        // Down and left on the top plane: 210°, the x axis backwards.
        let back = iso_ortho(base, p(0.0, 4.0), Plane::Top);
        assert!(close(back.angle, 210.0), "{}", back.angle);
        // The point is on the axis through the base, at the cursor's
        // distance along it.
        for plane in Plane::ALL {
            for cursor in [p(3.0, 25.0), p(-8.0, -2.0), p(30.0, 1.0)] {
                let track = iso_ortho(base, cursor, plane);
                let along = track.point - base;
                let unit = Vec2::from_polar(1.0, track.angle.to_radians());
                assert!(close(along.x * unit.y - along.y * unit.x, 0.0));
                assert!(plane.axes().iter().any(|a| close(a.dot(unit).abs(), 1.0)));
            }
        }
    }

    #[test]
    fn grid_snap_takes_the_nearest_grid_point_on_each_axis() {
        assert_eq!(grid_snap(p(21.0, 29.0), [10.0, 10.0]), p(20.0, 30.0));
        assert_eq!(grid_snap(p(-14.0, 6.0), [10.0, 5.0]), p(-10.0, 5.0));
        assert_eq!(grid_snap(p(2.6, 0.0), [2.5, 2.5]), p(2.5, 0.0));
        assert_eq!(grid_snap(p(3.0, 7.0), [0.0, f64::NAN]), p(3.0, 7.0));
        // Just below zero snaps to zero, not to negative zero.
        let zero = grid_snap(p(-1.2, -2.7), [10.0, 10.0]);
        assert!(
            zero.x.is_sign_positive() && zero.y.is_sign_positive(),
            "{zero:?}"
        );
    }

    #[test]
    fn ortho_keeps_the_dominant_axis() {
        let base = p(10.0, 10.0);
        assert_eq!(ortho(base, p(60.0, 30.0)).point, p(60.0, 10.0));
        assert_eq!(ortho(base, p(20.0, 80.0)).point, p(10.0, 80.0));
        assert_eq!(ortho(base, p(-40.0, 0.0)).point, p(-40.0, 10.0));
        assert_eq!(ortho(base, p(15.0, -90.0)).point, p(10.0, -90.0));
    }

    #[test]
    fn ortho_reports_the_axis_direction() {
        let base = p(0.0, 0.0);
        let angles: Vec<f64> = [p(5.0, 1.0), p(1.0, 5.0), p(-5.0, 1.0), p(1.0, -5.0)]
            .iter()
            .map(|cursor| ortho(base, *cursor).angle)
            .collect();
        assert_eq!(angles, vec![0.0, 90.0, 180.0, 270.0]);
    }

    #[test]
    fn ortho_on_the_diagonal_or_at_the_base_is_horizontal() {
        assert_eq!(ortho(p(0.0, 0.0), p(5.0, 5.0)).point, p(5.0, 0.0));
        assert_eq!(ortho(p(3.0, 4.0), p(3.0, 4.0)).point, p(3.0, 4.0));
    }

    #[test]
    fn polar_locks_onto_a_nearby_increment_direction() {
        // 3 units off the 45° ray, 100 along it.
        let along = 100.0 / 2f64.sqrt();
        let cursor = p(along + 2.0, along - 2.0);
        let track = polar(p(0.0, 0.0), cursor, 45.0, 8.0).unwrap();
        assert_eq!(track.angle, 45.0);
        assert!(track.point.distance_to(p(along, along)) < 1e-9);
        assert_eq!(track.kind, TrackKind::Polar);
    }

    #[test]
    fn polar_releases_when_the_cursor_strays() {
        // 100 along +X, 20 above it: no multiple of 90° within 8 units.
        assert_eq!(polar(p(0.0, 0.0), p(100.0, 20.0), 90.0, 8.0), None);
        // The same cursor is within reach of nothing at 45° either…
        assert_eq!(polar(p(0.0, 0.0), p(100.0, 20.0), 45.0, 8.0), None);
        // …but 15° passes close by: 100·sin(15° − 11.31°) ≈ 6.6.
        let track = polar(p(0.0, 0.0), p(100.0, 20.0), 15.0, 8.0).unwrap();
        assert_eq!(track.angle, 15.0);
    }

    #[test]
    fn polar_axis_directions_are_exact() {
        let base = p(12.5, -7.25);
        let up = polar(base, p(13.0, 90.0), 90.0, 8.0).unwrap();
        assert_eq!(up.point, p(12.5, 90.0));
        assert_eq!(up.angle, 90.0);
        let left = polar(base, p(-50.0, -6.0), 45.0, 8.0).unwrap();
        assert_eq!(left.point, p(-50.0, -7.25));
        assert_eq!(left.angle, 180.0);
        let down = polar(base, p(12.0, -80.0), 30.0, 8.0).unwrap();
        assert_eq!(down.point, p(12.5, -80.0));
        assert_eq!(down.angle, 270.0);
    }

    #[test]
    fn polar_needs_a_direction_and_a_positive_increment() {
        let base = p(0.0, 0.0);
        assert_eq!(
            polar(base, p(3.0, 0.0), 90.0, 8.0),
            None,
            "inside the reach of the base"
        );
        assert_eq!(polar(base, p(100.0, 0.0), 0.0, 8.0), None);
        assert_eq!(polar(base, p(100.0, 0.0), -45.0, 8.0), None);
        assert_eq!(polar(base, p(100.0, 0.0), f64::NAN, 8.0), None);
    }

    #[test]
    fn the_readout_is_distance_then_angle() {
        let track = polar(p(0.0, 0.0), p(50.0, 1.0), 90.0, 8.0).unwrap();
        assert_eq!(track.label(), "Polar: 50.0000 < 0°");
        assert_eq!(
            ortho(p(0.0, 0.0), p(1.0, -25.0)).label(),
            "Ortho: 25.0000 < 270°"
        );
        let track = polar(p(0.0, 0.0), p(100.0, 41.0), 22.5, 8.0).unwrap();
        assert!(track.label().ends_with("< 22.5°"), "{}", track.label());
    }
}
