//! What shows the drafter which constraints hold the geometry (#269,
//! ADR-0038): a bar of small glyphs beside each constrained object, one per
//! geometric constraint on it, and each dimensional constraint as a
//! dimension where it was placed, labelled with its name and value. Both are
//! overlay content — never the committed scene, never plotted — and either
//! can be hidden (CONSTRAINTBAR, DCDISPLAY).
//!
//! The bars are laid out in screen pixels, beside a point of the object, so
//! they keep their size at any zoom; the session hit-tests the same layout
//! to find the glyph under the pointer, whose constraint's objects it then
//! highlights.

use cad_core::{EntityId, Point2, ScreenPoint, View};
use cad_document::{
    Axis, Constraint, ConstraintId, DimensionEntity, Drawing, Entity, Kind, LayoutId, Measure, Part,
};
use cad_geometry::Segment;

/// The side of a glyph's square, in CSS pixels.
pub const GLYPH_PIXELS: f64 = 14.0;
/// The space between two glyphs of a bar, in CSS pixels.
pub const GLYPH_GAP_PIXELS: f64 = 2.0;
/// How far right of and below its object's point a bar starts, in CSS
/// pixels.
pub const BAR_OFFSET_PIXELS: f64 = 8.0;

/// The glyph of a geometric constraint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Glyph {
    Horizontal,
    Vertical,
    Parallel,
    Perpendicular,
    Collinear,
    Concentric,
    Coincident,
    Tangent,
    Symmetric,
    Equal,
    Fix,
}

impl Glyph {
    /// The glyph of a constraint's kind; none for a dimensional one, which
    /// is shown as a dimension.
    pub fn of(kind: &Kind) -> Option<Self> {
        Some(match kind {
            Kind::Horizontal => Self::Horizontal,
            Kind::Vertical => Self::Vertical,
            Kind::Parallel => Self::Parallel,
            Kind::Perpendicular => Self::Perpendicular,
            Kind::Collinear => Self::Collinear,
            Kind::Concentric => Self::Concentric,
            Kind::Coincident => Self::Coincident,
            Kind::Tangent { .. } => Self::Tangent,
            Kind::Symmetric => Self::Symmetric,
            Kind::Equal => Self::Equal,
            Kind::Fix { .. } => Self::Fix,
            _ => return None,
        })
    }
}

/// One object's bar: where it is anchored, and its glyphs, each with the
/// constraint it stands for.
#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub entity: EntityId,
    pub at: Point2,
    pub glyphs: Vec<(ConstraintId, Glyph)>,
}

fn middle(a: Point2, b: Point2) -> Point2 {
    Point2::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
}

/// The point of an object a bar is anchored at: a line's middle, a
/// circle's top, an arc's middle, a polyline's first segment's middle, a
/// point.
fn anchor(entity: &Entity) -> Option<Point2> {
    Some(match entity {
        Entity::Line(line) => middle(line.start, line.end),
        Entity::Circle(circle) => Point2::new(circle.center.x, circle.center.y + circle.radius),
        Entity::Arc(arc) => {
            let a = arc.arc();
            let sweep = (arc.end_angle - arc.start_angle).rem_euclid(std::f64::consts::TAU);
            a.point_at(arc.start_angle + sweep / 2.0)
        }
        Entity::Polyline(polyline) => {
            let first = *polyline.vertices.first()?;
            let second = polyline.vertices.get(1).copied().unwrap_or(first);
            middle(first, second)
        }
        Entity::Point(point) => point.position,
        _ => return None,
    })
}

/// The bars of the objects shown in `space`, in ID order.
pub fn bars(drawing: &Drawing, space: LayoutId) -> Vec<Bar> {
    let mut by_entity: std::collections::BTreeMap<EntityId, Vec<(ConstraintId, Glyph)>> =
        std::collections::BTreeMap::new();
    for (id, constraint) in drawing.constraints().iter() {
        let Some(glyph) = Glyph::of(&constraint.kind) else {
            continue;
        };
        let mut entities: Vec<EntityId> = constraint.references.iter().map(|r| r.entity).collect();
        entities.dedup();
        for entity in entities {
            by_entity.entry(entity).or_default().push((id, glyph));
        }
    }
    by_entity
        .into_iter()
        .filter(|(entity, _)| drawing.is_shown_in(*entity, space))
        .filter_map(|(entity, glyphs)| {
            Some(Bar {
                entity,
                at: anchor(drawing.entity(entity)?)?,
                glyphs,
            })
        })
        .collect()
}

/// The screen square of glyph `index` of a bar anchored at `at`: its top
/// left and bottom right corners.
pub fn glyph_square(view: &View, at: Point2, index: usize) -> (ScreenPoint, ScreenPoint) {
    let anchor = view.world_to_screen(at);
    let left = anchor.x + BAR_OFFSET_PIXELS + index as f64 * (GLYPH_PIXELS + GLYPH_GAP_PIXELS);
    let top = anchor.y + BAR_OFFSET_PIXELS;
    (
        ScreenPoint::new(left, top),
        ScreenPoint::new(left + GLYPH_PIXELS, top + GLYPH_PIXELS),
    )
}

/// The bar and glyph under `pointer`, if any.
pub fn glyph_at(view: &View, bars: &[Bar], pointer: ScreenPoint) -> Option<(usize, usize)> {
    bars.iter().enumerate().find_map(|(b, bar)| {
        (0..bar.glyphs.len()).find_map(|g| {
            let (low, high) = glyph_square(view, bar.at, g);
            let inside =
                (low.x..=high.x).contains(&pointer.x) && (low.y..=high.y).contains(&pointer.y);
            inside.then_some((b, g))
        })
    })
}

/// `d1=120`, `ang1=30°`.
fn label(constraint: &Constraint) -> String {
    let value = |v: f64| {
        let text = format!("{v:.4}");
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    };
    match constraint.kind {
        Kind::Angle { value: v } => format!("{}={}°", constraint.name, value(v.to_degrees().abs())),
        ref kind => format!(
            "{}={}",
            constraint.name,
            value(kind.value().unwrap_or_default().abs())
        ),
    }
}

/// A dimensional constraint as a dimension, laid out from the geometry as
/// it is and the place it was given, labelled with its name and value; none
/// for one never placed, or whose geometry cannot be measured.
pub fn dimension_of(drawing: &Drawing, constraint: &Constraint) -> Option<Entity> {
    let place = constraint.place?;
    let point = |i: usize| {
        let r = constraint.references.get(i)?;
        r.part.position_on(drawing.entity(r.entity)?)
    };
    let edge = |i: usize| -> Option<Segment> {
        let r = constraint.references.get(i)?;
        match (drawing.entity(r.entity)?, r.part) {
            (Entity::Line(line), _) => Some(Segment::new(line.start, line.end)),
            (Entity::Polyline(p), Part::Segment(n)) => {
                let n = n as usize;
                Some(Segment::new(
                    p.vertices[n],
                    p.vertices[(n + 1) % p.vertices.len()],
                ))
            }
            _ => None,
        }
    };
    let circle = || -> Option<(Point2, f64)> {
        match drawing.entity(constraint.references.first()?.entity)? {
            Entity::Circle(c) => Some((c.center, c.radius)),
            Entity::Arc(a) => Some((a.center, a.radius)),
            _ => None,
        }
    };
    let measure = match constraint.kind {
        Kind::Distance { axis, .. } => {
            let (a, b) = (point(0)?, point(1)?);
            match axis {
                Axis::Horizontal => Measure::Linear {
                    a,
                    b,
                    rotation: 0.0,
                },
                Axis::Vertical => Measure::Linear {
                    a,
                    b,
                    rotation: std::f64::consts::FRAC_PI_2,
                },
                Axis::Aligned => Measure::Aligned { a, b },
            }
        }
        Kind::Angle { .. } => {
            let (first, second) = (edge(0)?, edge(1)?);
            let vertex = cad_geometry::intersect::line_line(&first, &second)?;
            let far = |s: Segment| {
                if s.start.distance_to(vertex) > s.end.distance_to(vertex) {
                    s.start
                } else {
                    s.end
                }
            };
            Measure::Angular {
                vertex,
                a: far(first),
                b: far(second),
            }
        }
        Kind::Radius { .. } => {
            let (center, radius) = circle()?;
            Measure::Radius { center, radius }
        }
        Kind::Diameter { .. } => {
            let (center, radius) = circle()?;
            Measure::Diameter { center, radius }
        }
        _ => return None,
    };
    let mut dimension = DimensionEntity::new(measure, place);
    dimension.text_override = Some(label(constraint));
    Some(Entity::Dimension(dimension))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_document::{CircleEntity, LineEntity, Reference};

    #[test]
    fn glyphs_sit_in_a_row_beside_the_anchor_and_are_found_under_the_pointer() {
        let view = View::new(800.0, 600.0);
        let at = Point2::ORIGIN;
        let (low, high) = glyph_square(&view, at, 0);
        let anchor = view.world_to_screen(at);
        assert_eq!(low.x - anchor.x, BAR_OFFSET_PIXELS);
        assert_eq!(high.x - low.x, GLYPH_PIXELS);
        let (next, _) = glyph_square(&view, at, 1);
        assert_eq!(next.x - low.x, GLYPH_PIXELS + GLYPH_GAP_PIXELS);
        let bars = [Bar {
            entity: EntityId::from_raw(1),
            at,
            glyphs: vec![
                (ConstraintId::from_raw(1), Glyph::Horizontal),
                (ConstraintId::from_raw(2), Glyph::Fix),
            ],
        }];
        let middle = |(a, b): (ScreenPoint, ScreenPoint)| {
            ScreenPoint::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0)
        };
        assert_eq!(
            glyph_at(&view, &bars, middle(glyph_square(&view, at, 1))),
            Some((0, 1))
        );
        assert_eq!(glyph_at(&view, &bars, anchor), None);
    }

    #[test]
    fn a_placed_dimensional_constraint_is_a_dimension_labelled_with_its_name() {
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let line = tx
            .insert(LineEntity::new(Point2::ORIGIN, Point2::new(120.0, 0.0)))
            .unwrap();
        let circle = tx
            .insert(CircleEntity::new(Point2::new(200.0, 0.0), 10.0))
            .unwrap();
        tx.commit();
        let width = Constraint::dimensional(
            "d1",
            Kind::Distance {
                axis: Axis::Horizontal,
                value: 120.0,
            },
            vec![
                Reference::new(line, Part::Start),
                Reference::new(line, Part::End),
            ],
        );
        assert_eq!(dimension_of(&drawing, &width), None, "never placed");
        let Some(Entity::Dimension(d)) =
            dimension_of(&drawing, &width.clone().placed(Point2::new(60.0, -15.0)))
        else {
            panic!()
        };
        assert_eq!(d.text_override.as_deref(), Some("d1=120"));
        assert_eq!(d.line_point, Point2::new(60.0, -15.0));
        let radius = Constraint::dimensional(
            "rad1",
            Kind::Radius { value: 10.0 },
            vec![Reference::new(circle, Part::Edge)],
        )
        .placed(Point2::new(220.0, 10.0));
        let Some(Entity::Dimension(d)) = dimension_of(&drawing, &radius) else {
            panic!()
        };
        assert_eq!(
            d.measure,
            Measure::Radius {
                center: Point2::new(200.0, 0.0),
                radius: 10.0
            }
        );
    }
}
