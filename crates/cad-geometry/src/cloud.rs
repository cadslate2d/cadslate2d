//! Revision clouds (#215): a closed outline marked with a ring of arcs.
//!
//! The cloud's vertices lie on the outline: each side of it — a straight
//! span or an arc — is cut into equal pieces, as few as keep each piece's
//! chord within the longest arc asked for, so a corner of the outline is
//! always a vertex and no chord cuts across one. Every piece then becomes
//! an arc of a third of a turn bowing away from the outline's inside, or
//! towards it when reversed. A side shorter than the shortest arc asked for
//! is one arc, however short.

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;

use crate::{Path, Primitive, bulge_primitive, polyline_spans};

/// The bulge of every arc of a cloud: a third of a turn, `tan(π / 6)`.
pub const CLOUD_BULGE: f64 = 0.577_350_269_189_625_8;

/// The most arcs one cloud has: past it, the arc length asked for is too
/// small for the outline.
pub const MAX_CLOUD_ARCS: usize = 10_000;

impl Path {
    /// The revision cloud along this closed outline, with arcs whose chords
    /// are at most `max` and — where a side of the outline is long enough
    /// to allow it — at least `min`, bowing outwards, or inwards when
    /// `reverse`. `None` when the outline is open or encloses nothing,
    /// when the lengths are not `0 < min <= max`, or when the cloud would
    /// have more than [`MAX_CLOUD_ARCS`] arcs.
    pub fn revision_cloud(&self, min: f64, max: f64, reverse: bool) -> Option<Self> {
        let lengths_fine = min.is_finite() && max.is_finite() && min > 0.0 && min <= max;
        if !self.closed || !lengths_fine {
            return None;
        }
        let tolerance = LinearTolerance::DEFAULT.around(&self.vertices);
        let area = self.area();
        if !area.is_finite() || tolerance.is_zero(area.abs().sqrt()) {
            return None;
        }
        let bulge_of = |index: usize| self.bulges.get(index).copied().unwrap_or(0.0);
        let mut vertices: Vec<Point2> = Vec::new();
        for (from, to) in polyline_spans(self.vertices.len(), true, bulge_of) {
            let (start, end) = (self.vertices[from], self.vertices[to]);
            if tolerance.coincident(start, end) {
                continue;
            }
            match bulge_primitive(start, end, bulge_of(from)) {
                Primitive::Arc(arc) => {
                    let pieces = arc_pieces(arc.radius, arc.sweep, max)?;
                    // A positive bulge runs anticlockwise from `start`.
                    let anticlockwise = bulge_of(from) > 0.0;
                    vertices.push(start);
                    for k in 1..pieces {
                        let t = k as f64 / pieces as f64;
                        let angle = if anticlockwise {
                            arc.start + arc.sweep * t
                        } else {
                            arc.start + arc.sweep * (1.0 - t)
                        };
                        vertices.push(arc.point_at(angle));
                    }
                }
                Primitive::Segment(_) | Primitive::Circle(_) => {
                    let pieces = pieces(start.distance_to(end) / max)?;
                    vertices.extend(
                        (0..pieces).map(|k| start + (end - start) * (k as f64 / pieces as f64)),
                    );
                }
            }
            if vertices.len() > MAX_CLOUD_ARCS {
                return None;
            }
        }
        // A positive bulge bows to the right of the way it runs: outwards
        // round an outline that runs anticlockwise.
        let outwards = if area > 0.0 { 1.0 } else { -1.0 };
        let bulge = CLOUD_BULGE * outwards * if reverse { -1.0 } else { 1.0 };
        Some(Self {
            bulges: vec![bulge; vertices.len()],
            vertices,
            closed: true,
        })
    }
}

/// How many pieces a side `ratio` times the longest piece is cut into:
/// the fewest that are no longer, and at least one.
fn pieces(ratio: f64) -> Option<usize> {
    let count = ratio.ceil();
    (count.is_finite() && count <= MAX_CLOUD_ARCS as f64).then_some((count as usize).max(1))
}

/// The fewest equal pieces of an arc whose chords are at most `max`.
fn arc_pieces(radius: f64, sweep: f64, max: f64) -> Option<usize> {
    let chord = |count: usize| 2.0 * radius * (sweep / (2.0 * count as f64)).sin();
    // The arc's length over `max` is enough, since a chord is shorter
    // than its arc; fewer may do.
    let mut count = pieces(radius * sweep / max)?;
    while count > 1 && chord(count - 1) <= max {
        count -= 1;
    }
    Some(count)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn rectangle(w: f64, h: f64) -> Path {
        Path {
            vertices: vec![p(0.0, 0.0), p(w, 0.0), p(w, h), p(0.0, h)],
            bulges: vec![],
            closed: true,
        }
    }

    /// Every cloud chord, vertex to vertex.
    fn chords(cloud: &Path) -> Vec<f64> {
        let n = cloud.vertices.len();
        (0..n)
            .map(|i| cloud.vertices[i].distance_to(cloud.vertices[(i + 1) % n]))
            .collect()
    }

    #[test]
    fn a_rectangle_is_clouded_side_by_side_with_its_corners_as_vertices() {
        let cloud = rectangle(40.0, 20.0)
            .revision_cloud(5.0, 10.0, false)
            .unwrap();
        // 40 / 10 and 20 / 10: four and two arcs a side.
        assert_eq!(cloud.vertices.len(), 12);
        for corner in [p(0.0, 0.0), p(40.0, 0.0), p(40.0, 20.0), p(0.0, 20.0)] {
            assert!(cloud.vertices.contains(&corner));
        }
        assert!(
            chords(&cloud)
                .iter()
                .all(|chord| (chord - 10.0).abs() < 1e-12)
        );
        // Anticlockwise, the arcs bow out: the cloud is bigger.
        assert_eq!(cloud.bulges, vec![CLOUD_BULGE; 12]);
        assert!(cloud.area() > 800.0);
        let inward = rectangle(40.0, 20.0)
            .revision_cloud(5.0, 10.0, true)
            .unwrap();
        assert!(inward.area() < 800.0);
        // Clockwise, the bulges turn round and still bow out.
        let mut clockwise = rectangle(40.0, 20.0);
        clockwise.vertices.reverse();
        let cloud = clockwise.revision_cloud(5.0, 10.0, false).unwrap();
        assert!(cloud.area() < -800.0);
    }

    #[test]
    fn a_circle_is_clouded_with_equal_chords() {
        // Two half-turns: a circle of radius 10.
        let circle = Path {
            vertices: vec![p(10.0, 0.0), p(-10.0, 0.0)],
            bulges: vec![1.0, 1.0],
            closed: true,
        };
        let cloud = circle.revision_cloud(3.0, 6.0, false).unwrap();
        let chords = chords(&cloud);
        assert!(chords.iter().all(|chord| *chord <= 6.0 && *chord >= 3.0));
        assert!(
            cloud
                .vertices
                .iter()
                .all(|v| (v.distance_to(Point2::ORIGIN) - 10.0).abs() < 1e-9)
        );
    }

    #[test]
    fn what_cannot_be_clouded_is_refused() {
        let mut open = rectangle(10.0, 10.0);
        open.closed = false;
        assert!(open.revision_cloud(1.0, 2.0, false).is_none());
        assert!(
            rectangle(10.0, 10.0)
                .revision_cloud(3.0, 2.0, false)
                .is_none()
        );
        assert!(
            rectangle(10.0, 10.0)
                .revision_cloud(0.0, 2.0, false)
                .is_none()
        );
        assert!(
            rectangle(10.0, 0.0)
                .revision_cloud(1.0, 2.0, false)
                .is_none(),
            "no area"
        );
        assert!(
            rectangle(1e6, 1e6)
                .revision_cloud(1.0, 1.0, false)
                .is_none(),
            "too many arcs"
        );
        // A side shorter than the shortest arc is one arc.
        let thin = rectangle(30.0, 2.0)
            .revision_cloud(5.0, 10.0, false)
            .unwrap();
        assert_eq!(thin.vertices.len(), 8);
    }

    /// An outline with every side at least `max` long: a rectangle with
    /// one side bowed by a bulge, either way round. Bowed in, it stays
    /// wide enough that clouds of opposite sides cannot meet, which the
    /// area comparison below would misread.
    fn outline() -> impl Strategy<Value = (Path, f64, f64)> {
        (
            1.0f64..50.0,
            3.0f64..10.0,
            3.0f64..10.0,
            -0.2f64..0.8,
            0.1f64..0.5,
            any::<bool>(),
        )
            .prop_map(|(max, w, h, bulge, fraction, clockwise)| {
                let mut path = rectangle(max * w, max * h);
                path.bulges = vec![0.0, bulge, 0.0, 0.0];
                if clockwise {
                    path.vertices.reverse();
                    // The bowed side runs the other way, at the same index.
                    path.bulges = vec![0.0, -bulge, 0.0, 0.0];
                }
                (path, max * fraction, max)
            })
    }

    proptest! {
        /// Every chord is within the asked lengths, every vertex is on the
        /// outline, and the arcs bow out of it.
        #[test]
        fn a_cloud_keeps_its_chords_within_the_lengths_and_its_vertices_on_the_outline(
            (outline, min, max) in outline(),
            reverse in any::<bool>(),
        ) {
            let cloud = outline.revision_cloud(min, max, reverse).unwrap();
            let slack = 1e-9 * max;
            for chord in chords(&cloud) {
                prop_assert!(chord <= max + slack, "chord {} over {}", chord, max);
                prop_assert!(chord >= min - slack, "chord {} under {}", chord, min);
            }
            let primitives = outline.primitives();
            for vertex in &cloud.vertices {
                let off = primitives
                    .iter()
                    .map(|primitive| primitive.distance_to_point(*vertex))
                    .fold(f64::INFINITY, f64::min);
                prop_assert!(off < 1e-9 * max * 100.0, "{:?} is {} off", vertex, off);
            }
            let (inner, outer) = (outline.area().abs(), cloud.area().abs());
            let bows_right_way = if reverse { outer < inner } else { outer > inner };
            prop_assert!(bows_right_way, "{} against {}", outer, inner);
        }
    }
}
