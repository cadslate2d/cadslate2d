//! Hatch pattern lines (DESIGN §34; ADR-0021): a family of parallel lines,
//! each dashed alike, clipped to a region by the even-odd rule.
//!
//! A family is what one line of a `.pat` file describes: the lines' angle,
//! a point one of them passes through, how far each next line is shifted
//! along and across, and the dashes — positive a dash, negative a gap,
//! zero a dot — repeated along every line from where it is closest to the
//! point. The lines are fixed in the drawing, not on the screen: zooming
//! moves them with everything else.

use cad_core::{Point2, Vec2};

use crate::Segment;

/// One family of parallel pattern lines, in drawing units.
#[derive(Clone, Debug, PartialEq)]
pub struct LineFamily {
    /// The lines' direction, radians from +X.
    pub angle: f64,
    /// A point the first line passes through, where its dashes start.
    pub origin: Point2,
    /// How far each next line is shifted along the lines.
    pub shift: f64,
    /// How far apart the lines are, across them.
    pub spacing: f64,
    /// Dash (positive), gap (negative), dot (zero) lengths; empty for a
    /// continuous line.
    pub dashes: Vec<f64>,
}

impl LineFamily {
    /// The family turned by `angle` about the drawing origin, scaled by
    /// `scale`, then moved to `origin`: how a hatch places a pattern.
    #[must_use]
    pub fn placed(&self, angle: f64, scale: f64, origin: Point2) -> Self {
        let (sin, cos) = angle.sin_cos();
        let local = self.origin - Point2::ORIGIN;
        let turned = Vec2::new(local.x * cos - local.y * sin, local.x * sin + local.y * cos);
        Self {
            angle: self.angle + angle,
            origin: origin + turned * scale,
            shift: self.shift * scale,
            spacing: self.spacing * scale,
            dashes: self.dashes.iter().map(|dash| dash * scale).collect(),
        }
    }

    fn along(&self) -> Vec2 {
        Vec2::from_polar(1.0, self.angle)
    }

    fn across(&self) -> Vec2 {
        let along = self.along();
        Vec2::new(-along.y, along.x)
    }

    /// The offset from one line to the next, in the drawing.
    pub fn step(&self) -> Vec2 {
        self.along() * self.shift + self.across() * self.spacing
    }
}

/// The pieces of `family`'s lines inside the region `rings` bound by the
/// even-odd rule — at most `budget` of them, or `None` when the region
/// would need more: a pattern too fine for its region. A family whose
/// lines are not apart draws nothing.
pub fn pattern_lines(
    rings: &[Vec<Point2>],
    family: &LineFamily,
    budget: usize,
) -> Option<Vec<Segment>> {
    let mut out = Vec::new();
    let spacing = family.spacing.abs();
    let finite = family.angle.is_finite()
        && family.origin.is_finite()
        && family.shift.is_finite()
        && spacing.is_finite()
        && family.dashes.iter().all(|dash| dash.is_finite());
    if !finite || spacing <= 0.0 {
        return Some(out);
    }
    let (along, across) = (family.along(), family.across());
    // Line k passes through origin + k·step; its distance across is k·spacing
    // (signed by the family's spacing).
    let sign = family.spacing.signum();
    let across_of = |point: &Point2| (*point - family.origin).dot(across) * sign;
    let (low, high) = rings
        .iter()
        .flatten()
        .map(across_of)
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), d| {
            (lo.min(d), hi.max(d))
        });
    if low > high {
        return Some(out);
    }
    let first = (low / spacing).ceil();
    let last = (high / spacing).floor();
    if last - first + 1.0 > budget as f64 {
        return None;
    }
    let period: f64 = family.dashes.iter().map(|dash| dash.abs()).sum();
    let mut k = first;
    while k <= last {
        let base = family.origin + family.step() * k;
        let mut crossings: Vec<f64> = Vec::new();
        for ring in rings {
            let n = ring.len();
            for i in 0..n {
                let (a, b) = (ring[i], ring[(i + 1) % n]);
                let (da, db) = ((a - base).dot(across), (b - base).dot(across));
                // Half-open, so a vertex on the line counts once.
                if (da > 0.0) != (db > 0.0) {
                    let at = a + (b - a) * (da / (da - db));
                    crossings.push((at - base).dot(along));
                }
            }
        }
        crossings.sort_by(f64::total_cmp);
        let (pairs, _) = crossings.as_chunks::<2>();
        for &[from, to] in pairs {
            if !dashes(&family.dashes, period, from, to, |start, end| {
                out.push(Segment::new(base + along * start, base + along * end));
                out.len() <= budget
            }) {
                return None;
            }
        }
        k += 1.0;
    }
    Some(out)
}

/// Calls `emit` with the stretches of `[from, to]` the dashes cover — the
/// whole of it when there are none — and stops with `false` as soon as
/// `emit` does.
fn dashes(
    dashes: &[f64],
    period: f64,
    from: f64,
    to: f64,
    mut emit: impl FnMut(f64, f64) -> bool,
) -> bool {
    if dashes.is_empty() || period <= 0.0 {
        return emit(from, to);
    }
    let mut cell = (from / period).floor() * period;
    while cell <= to {
        let mut at = cell;
        for dash in dashes {
            if *dash >= 0.0 {
                let (start, end) = (at.max(from), (at + dash).min(to));
                if start <= end && !emit(start, end) {
                    return false;
                }
            }
            at += dash.abs();
        }
        cell += period;
    }
    true
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_4, TAU};

    use proptest::prelude::*;

    use super::*;
    use crate::{ring_contains, signed_area};

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn square(size: f64) -> Vec<Point2> {
        vec![p(0.0, 0.0), p(size, 0.0), p(size, size), p(0.0, size)]
    }

    fn horizontal(spacing: f64, dashes: Vec<f64>) -> LineFamily {
        LineFamily {
            angle: 0.0,
            origin: Point2::ORIGIN,
            shift: 0.0,
            spacing,
            dashes,
        }
    }

    fn length(segments: &[Segment]) -> f64 {
        segments.iter().map(Segment::length).sum()
    }

    #[test]
    fn horizontal_lines_fill_a_square_with_a_hole_by_the_even_odd_rule() {
        let rings = vec![
            square(10.0),
            vec![p(4.0, 4.0), p(6.0, 4.0), p(6.0, 6.0), p(4.0, 6.0)],
        ];
        let lines = pattern_lines(&rings, &horizontal(1.0, Vec::new()), 1000).unwrap();
        // y = 0 … 9: crossings are half-open, so a line on the lower edge
        // counts and one on the upper edge does not. y = 4 and y = 5 cross
        // the hole too and come in two pieces each.
        assert_eq!(lines.len(), 12);
        assert!((length(&lines) - (10.0 * 10.0 - 2.0 * 2.0)).abs() < 1e-9);
        assert!(lines.iter().all(|line| line.start.y == line.end.y));
        // A budget the region exceeds: too fine.
        assert_eq!(
            pattern_lines(&rings, &horizontal(0.001, Vec::new()), 1000),
            None
        );
        // Lines not apart draw nothing.
        assert_eq!(
            pattern_lines(&rings, &horizontal(0.0, Vec::new()), 1000),
            Some(Vec::new())
        );
    }

    #[test]
    fn dashes_repeat_from_the_origin_and_dots_are_points() {
        let rings = vec![square(10.0)];
        let dashed =
            pattern_lines(&rings, &horizontal(5.0, vec![2.0, -1.0, 0.0, -1.0]), 1000).unwrap();
        // The line y = 5: dashes at 0–2, 4–6, 8–10, dots at 3, 7.
        let pieces: Vec<(f64, f64)> = dashed
            .iter()
            .filter(|s| s.start.y == 5.0)
            .map(|s| (s.start.x, s.end.x))
            .collect();
        assert_eq!(
            pieces,
            [(0.0, 2.0), (3.0, 3.0), (4.0, 6.0), (7.0, 7.0), (8.0, 10.0)]
        );
        assert!(dashed.iter().all(|s| s.start.y == s.end.y));
    }

    #[test]
    fn a_placed_family_turns_scales_and_moves_with_its_hatch() {
        let family = LineFamily {
            angle: 0.0,
            origin: p(1.0, 0.0),
            shift: 0.5,
            spacing: 2.0,
            dashes: vec![1.0, -1.0],
        };
        let placed = family.placed(FRAC_PI_4 * 2.0, 3.0, p(10.0, 10.0));
        assert!((placed.angle - FRAC_PI_4 * 2.0).abs() < 1e-12);
        assert!(placed.origin.distance_to(p(10.0, 13.0)) < 1e-9);
        assert_eq!((placed.shift, placed.spacing), (1.5, 6.0));
        assert_eq!(placed.dashes, [3.0, -3.0]);
        let step = placed.step();
        assert!((step.x + 6.0).abs() < 1e-9 && (step.y - 1.5).abs() < 1e-9);
    }

    fn star(count: usize, radii: &[f64]) -> Vec<Point2> {
        (0..count)
            .map(|i| {
                let angle = i as f64 / count as f64 * TAU;
                Point2::ORIGIN + Vec2::from_polar(radii[i % radii.len()], angle)
            })
            .collect()
    }

    proptest! {
        /// Every piece lies inside the region, and the lines' total length
        /// is the area over the spacing: the count of lines scales with the
        /// area over the scale squared, as their length with the area over
        /// the scale.
        #[test]
        fn every_piece_lies_inside_and_the_length_is_the_area_over_the_spacing(
            count in 3usize..30,
            radii in prop::collection::vec(40.0f64..100.0, 1..6),
            angle in 0.0f64..TAU,
            spacing in 1.0f64..10.0,
            ox in -50.0f64..50.0, oy in -50.0f64..50.0,
        ) {
            let rings = vec![star(count, &radii), star(7, &[5.0, 8.0])];
            let family = LineFamily { angle, origin: p(ox, oy), shift: 0.3, spacing, dashes: Vec::new() };
            let lines = pattern_lines(&rings, &family, 100_000).unwrap();
            let inside = |point: Point2| rings.iter().filter(|ring| ring_contains(ring, point)).count() % 2 == 1;
            for line in &lines {
                let middle = line.start + (line.end - line.start) * 0.5;
                prop_assert!(inside(middle), "{line:?}");
            }
            let area = signed_area(&rings[0]).abs() - signed_area(&rings[1]).abs();
            let expected = area / spacing;
            // A band of half a spacing at each extreme is counted or not.
            let slack = 2.0 * 200.0 + expected * 0.05;
            prop_assert!((length(&lines) - expected).abs() < slack, "{} vs {expected}", length(&lines));
        }
    }
}
