//! Dimensions (DESIGN §34; ADR-0020).
//!
//! A dimension is what it measures — two points, three for an angle, a
//! circle for a radius — a point that says where it is drawn, a style,
//! and perhaps a text of its own. Everything else about it — extension
//! lines, the dimension line, the arrowheads, the text's words, place,
//! and angle — is laid out from those by the style the table holds,
//! whenever it is asked for: nothing derived is stored, so changing a
//! style is one row changing however many dimensions it draws (§19.6).
//! The layout is a pure function of the entity and the style.

use cad_core::tolerance::LinearTolerance;
use std::f64::consts::TAU;

use cad_core::{Aabb, Point2, Vec2};
use cad_geometry::intersect::line_line;
use cad_geometry::{
    Arc, Arrowhead, Segment, Transform, foot_on_line, normalized, ordinate_leader, perpendicular,
    readable_angle,
};
use cad_text::Font;

use crate::dim_style::{DimStyle, DimStyleId};
use crate::text::{Justification, TextEntity};

const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

/// What a dimension measures.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Measure {
    /// The distance between `a` and `b` along the direction at `rotation`
    /// (radians from +X): 0 for a horizontal dimension, π/2 for a vertical.
    Linear { a: Point2, b: Point2, rotation: f64 },
    /// The distance between `a` and `b`, measured along the line through them.
    Aligned { a: Point2, b: Point2 },
    /// The angle at `vertex` between the rays through `a` and `b`: the one
    /// on the side the dimension is placed.
    Angular {
        vertex: Point2,
        a: Point2,
        b: Point2,
    },
    /// The radius of the circle or arc.
    Radius { center: Point2, radius: f64 },
    /// Its diameter.
    Diameter { center: Point2, radius: f64 },
    /// The X of `feature` from `origin`, the datum, when `x`; else its Y
    /// (#227). Its leader runs from the feature, square to the axis it
    /// measures along, to where the dimension is placed.
    Ordinate {
        origin: Point2,
        feature: Point2,
        x: bool,
    },
}

impl Measure {
    /// The kind, as the panel and the messages name it.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Linear { .. } => "linear",
            Self::Aligned { .. } => "aligned",
            Self::Angular { .. } => "angular",
            Self::Radius { .. } => "radius",
            Self::Diameter { .. } => "diameter",
            Self::Ordinate { .. } => "ordinate",
        }
    }

    /// The points the dimension measures, in a fixed order: `a` and `b`;
    /// the vertex, `a`, and `b`; the centre; an ordinate's feature — its
    /// datum is where it measures from, not what it measures.
    pub fn points(&self) -> Vec<Point2> {
        match *self {
            Self::Linear { a, b, .. } | Self::Aligned { a, b } => vec![a, b],
            Self::Angular { vertex, a, b } => vec![vertex, a, b],
            Self::Radius { center, .. } | Self::Diameter { center, .. } => vec![center],
            Self::Ordinate { feature, .. } => vec![feature],
        }
    }

    /// The angle between two lines through `vertex` — one through `a`, one
    /// through `b` — that `place` falls in: of the four the lines make, the
    /// one whose arc through `place` is smallest. The dimension at `place`
    /// then measures that angle, never a reflex one.
    pub fn between_lines(vertex: Point2, a: Point2, b: Point2, place: Point2) -> Self {
        let across = |point: Point2| vertex - (point - vertex);
        let sweep = |a, b| DimensionEntity::new(Self::Angular { vertex, a, b }, place).value();
        [
            (a, b),
            (across(a), b),
            (a, across(b)),
            (across(a), across(b)),
        ]
        .into_iter()
        .min_by(|(a0, b0), (a1, b1)| sweep(*a0, *b0).total_cmp(&sweep(*a1, *b1)))
        .map(|(a, b)| Self::Angular { vertex, a, b })
        .expect("four candidates")
    }

    /// The angle between two lines, each given by two points, that `place`
    /// falls in (see [`Measure::between_lines`]); the rays go through each
    /// line's point farther from where they meet. `None` when the lines
    /// are parallel, or one has no length.
    pub fn between_line_segments(
        first: [Point2; 2],
        second: [Point2; 2],
        place: Point2,
    ) -> Option<Self> {
        let vertex = line_line(
            &Segment::new(first[0], first[1]),
            &Segment::new(second[0], second[1]),
        )?;
        let away = |points: [Point2; 2]| {
            if vertex.distance_to(points[0]) >= vertex.distance_to(points[1]) {
                points[0]
            } else {
                points[1]
            }
        };
        Some(Self::between_lines(
            vertex,
            away(first),
            away(second),
            place,
        ))
    }

    /// The same measure with point `index` (as [`Measure::points`] orders
    /// them) at `to`; `None` when there is no such point.
    fn with_point(self, index: usize, to: Point2) -> Option<Self> {
        Some(match (self, index) {
            (Self::Linear { b, rotation, .. }, 0) => Self::Linear { a: to, b, rotation },
            (Self::Linear { a, rotation, .. }, 1) => Self::Linear { a, b: to, rotation },
            (Self::Aligned { b, .. }, 0) => Self::Aligned { a: to, b },
            (Self::Aligned { a, .. }, 1) => Self::Aligned { a, b: to },
            (Self::Angular { a, b, .. }, 0) => Self::Angular { vertex: to, a, b },
            (Self::Angular { vertex, b, .. }, 1) => Self::Angular { vertex, a: to, b },
            (Self::Angular { vertex, a, .. }, 2) => Self::Angular { vertex, a, b: to },
            (Self::Radius { radius, .. }, 0) => Self::Radius { center: to, radius },
            (Self::Diameter { radius, .. }, 0) => Self::Diameter { center: to, radius },
            (Self::Ordinate { origin, x, .. }, 0) => Self::Ordinate {
                origin,
                feature: to,
                x,
            },
            _ => return None,
        })
    }

    fn transformed(&self, transform: &Transform) -> Self {
        let at = |point: Point2| transform.apply(point);
        match *self {
            Self::Linear { a, b, rotation } => Self::Linear {
                a: at(a),
                b: at(b),
                rotation: transform.apply_angle(rotation),
            },
            Self::Aligned { a, b } => Self::Aligned { a: at(a), b: at(b) },
            Self::Angular { vertex, a, b } => Self::Angular {
                vertex: at(vertex),
                a: at(a),
                b: at(b),
            },
            Self::Radius { center, radius } => Self::Radius {
                center: at(center),
                radius: radius * transform.scale_factor(),
            },
            Self::Diameter { center, radius } => Self::Diameter {
                center: at(center),
                radius: radius * transform.scale_factor(),
            },
            // Turned nearer a quarter turn than not, the X it measured is
            // a Y.
            Self::Ordinate { origin, feature, x } => {
                let turned = transform.apply_angle(0.0);
                Self::Ordinate {
                    origin: at(origin),
                    feature: at(feature),
                    x: x != (turned.sin().abs() > turned.cos().abs()),
                }
            }
        }
    }
}

/// How a dimension is chained to another (#212).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Chain {
    /// From the same extension line, across the other, with its dimension
    /// line this far further out: `DIMBASELINE`.
    Baseline(f64),
    /// From the other's extension line onwards, on the same dimension
    /// line: `DIMCONTINUE`.
    Continue,
}

/// Why a dimension cannot be as given.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DimensionError {
    /// A coordinate or the rotation is not a number.
    NotFinite,
    /// The two points measured are the same point.
    Coincident,
    /// A point of an angle lies on its vertex, or the dimension arc would
    /// pass through it: no ray, or no arc.
    OnVertex,
    /// A radius that is not positive.
    NoRadius,
}

impl DimensionError {
    /// The refusal in the user's terms.
    pub fn explain(self) -> &'static str {
        match self {
            Self::NotFinite => "A dimension's points must be numbers.",
            Self::Coincident => "The two points of a dimension must be apart.",
            Self::OnVertex => "The points of an angle must be away from its vertex.",
            Self::NoRadius => "The radius must be greater than zero.",
        }
    }
}

/// A dimension.
#[derive(Clone, Debug, PartialEq)]
pub struct DimensionEntity {
    pub measure: Measure,
    /// Where the dimension is placed: a point the dimension line passes
    /// through (linear, aligned), a point on the dimension arc (angular),
    /// or where the text is, which also sets the leader's angle (radius,
    /// diameter).
    pub line_point: Point2,
    pub style: DimStyleId,
    /// Text in place of the measured value, when the drafter wrote some.
    pub text_override: Option<String>,
    /// The text's angle, radians from +X, when the drafter set one; else
    /// the text reads along the dimension line, the right way up.
    pub text_angle: Option<f64>,
}

/// A dimension drawn: what the renderer draws, picking feels, and the
/// bounds hold. Built by [`DimensionEntity::layout`] from a style.
#[derive(Clone, Debug, PartialEq)]
pub struct DimensionLayout {
    /// Extension lines, the dimension line, a leader.
    pub lines: Vec<Segment>,
    /// The dimension arc of an angle.
    pub arcs: Vec<Arc>,
    pub arrowheads: Vec<Arrowhead>,
    pub text: TextEntity,
    /// The measured value: a distance, a radius or diameter, or degrees.
    pub value: f64,
}

impl DimensionLayout {
    fn new(text: TextEntity, value: f64) -> Self {
        Self {
            lines: Vec::new(),
            arcs: Vec::new(),
            arrowheads: Vec::new(),
            text,
            value,
        }
    }

    /// Every straight piece: the lines and the arrowheads' outlines.
    pub fn segments(&self) -> impl Iterator<Item = Segment> + '_ {
        self.lines
            .iter()
            .copied()
            .chain(self.arrowheads.iter().flat_map(|arrow| arrow.segments()))
    }

    pub fn bounds(&self) -> Aabb {
        self.segments()
            .map(|segment| segment.bounds())
            .chain(self.arcs.iter().map(Arc::bounds))
            .fold(self.text.bounds(), |bounds, other| bounds.union(&other))
    }

    /// The distance to the nearest line, arc, arrowhead, or the text's box.
    pub fn distance_to(&self, point: Point2) -> f64 {
        self.segments()
            .map(|segment| segment.distance_to_point(point))
            .chain(self.arcs.iter().map(|arc| arc.distance_to_point(point)))
            .fold(self.text.distance_to(point), f64::min)
    }
}

impl DimensionEntity {
    /// A dimension in the drawing's first style, saying what it measures.
    pub fn new(measure: Measure, line_point: Point2) -> Self {
        Self {
            measure,
            line_point,
            style: DimStyleId::FIRST,
            text_override: None,
            text_angle: None,
        }
    }

    #[must_use]
    pub fn in_style(self, style: DimStyleId) -> Self {
        Self { style, ..self }
    }

    /// The same dimension with text of the drafter's; `None` goes back to
    /// the measured value.
    #[must_use]
    pub fn saying(self, text: Option<String>) -> Self {
        Self {
            text_override: text,
            ..self
        }
    }

    /// The same dimension with its text at `angle` (radians from +X);
    /// `None` lets it read along the dimension line again.
    #[must_use]
    pub fn turned(self, angle: Option<f64>) -> Self {
        Self {
            text_angle: angle,
            ..self
        }
    }

    /// Whether the dimension can be laid out.
    pub fn check(&self) -> Result<(), DimensionError> {
        let finite = self.line_point.is_finite()
            && self.measure.points().iter().all(|point| point.is_finite())
            && self.text_angle.is_none_or(f64::is_finite);
        if !finite {
            return Err(DimensionError::NotFinite);
        }
        match self.measure {
            Measure::Linear { a, b, rotation } => {
                if !rotation.is_finite() {
                    return Err(DimensionError::NotFinite);
                }
                (!TOLERANCE.around(&[a, b]).coincident(a, b))
                    .then_some(())
                    .ok_or(DimensionError::Coincident)
            }
            Measure::Aligned { a, b } => (!TOLERANCE.around(&[a, b]).coincident(a, b))
                .then_some(())
                .ok_or(DimensionError::Coincident),
            Measure::Angular { vertex, a, b } => {
                let tolerance = TOLERANCE.around(&[vertex, a, b, self.line_point]);
                let on_vertex = |point| tolerance.coincident(vertex, point);
                (!on_vertex(a) && !on_vertex(b) && !on_vertex(self.line_point))
                    .then_some(())
                    .ok_or(DimensionError::OnVertex)
            }
            Measure::Radius { radius, .. } | Measure::Diameter { radius, .. } => {
                (radius.is_finite() && radius > 0.0)
                    .then_some(())
                    .ok_or(DimensionError::NoRadius)
            }
            Measure::Ordinate { origin, .. } => origin
                .is_finite()
                .then_some(())
                .ok_or(DimensionError::NotFinite),
        }
    }

    /// The points a grip takes hold of: what is measured, then where the
    /// dimension is placed.
    pub fn points(&self) -> Vec<Point2> {
        let mut points = self.measure.points();
        points.push(self.line_point);
        points
    }

    /// The same dimension with point `index` (as [`DimensionEntity::points`]
    /// orders them) at `to`; `None` when there is no such point.
    #[must_use]
    pub fn with_point(&self, index: usize, to: Point2) -> Option<Self> {
        let measured = self.measure.points().len();
        let (measure, line_point) = if index < measured {
            (self.measure.with_point(index, to)?, self.line_point)
        } else if index == measured {
            (self.measure, to)
        } else {
            return None;
        };
        Some(Self {
            measure,
            line_point,
            ..self.clone()
        })
    }

    /// Which of the two extension lines `at` is nearer — 0 for the one from
    /// `a`, 1 for the one from `b` — measured along a linear or aligned
    /// dimension, or by angle about an angular one's vertex (#212). `None`
    /// for a radius or a diameter, which has no extension lines to chain
    /// from.
    pub fn nearer_origin(&self, at: Point2) -> Option<usize> {
        let (a_off, b_off) = match self.measure {
            Measure::Linear { a, b, rotation } => {
                let along = Vec2::from_polar(1.0, rotation);
                (along.dot(at - a).abs(), along.dot(at - b).abs())
            }
            Measure::Aligned { a, b } => {
                let along = normalized(b - a).unwrap_or(Vec2::new(1.0, 0.0));
                (along.dot(at - a).abs(), along.dot(at - b).abs())
            }
            Measure::Angular { vertex, a, b } => {
                let angle = |point: Point2| {
                    let ray = point - vertex;
                    ray.y.atan2(ray.x)
                };
                let apart = |x: f64, y: f64| {
                    let turn = (x - y).rem_euclid(TAU);
                    turn.min(TAU - turn)
                };
                let pick = angle(at);
                (apart(pick, angle(a)), apart(pick, angle(b)))
            }
            Measure::Radius { .. } | Measure::Diameter { .. } | Measure::Ordinate { .. } => {
                return None;
            }
        };
        Some(usize::from(b_off < a_off))
    }

    /// The dimension `chain` makes from this one's extension line `origin`
    /// (0 from `a`, 1 from `b`) to `to`, of the same kind and style and
    /// with no text of its own (#212). A continued dimension shares this
    /// one's dimension line, or its arc's radius, and runs on away from it;
    /// a baseline dimension measures across this one, its line a spacing
    /// further from what is measured. `None` for a radius or a diameter,
    /// for an origin other than 0 or 1, or when `to` gives no direction.
    /// What it returns may still fail [`DimensionEntity::check`].
    pub fn chained(&self, origin: usize, to: Point2, chain: Chain) -> Option<Self> {
        let pick = |a: Point2, b: Point2| match origin {
            0 => Some(a),
            1 => Some(b),
            _ => None,
        };
        // Away from what is measured: the side the line is on, or the
        // positive one when it runs through the points.
        let outwards = |offset: f64| if offset < 0.0 { -1.0 } else { 1.0 };
        let (measure, line_point) = match self.measure {
            Measure::Linear { a, b, rotation } => {
                let from = pick(a, b)?;
                let normal = perpendicular(Vec2::from_polar(1.0, rotation));
                let line_point = match chain {
                    Chain::Continue => self.line_point,
                    Chain::Baseline(spacing) => {
                        let middle = a + (b - a) * 0.5;
                        let side = outwards(normal.dot(self.line_point - middle));
                        self.line_point + normal * (side * spacing)
                    }
                };
                (
                    Measure::Linear {
                        a: from,
                        b: to,
                        rotation,
                    },
                    line_point,
                )
            }
            Measure::Aligned { a, b } => {
                let from = pick(a, b)?;
                let normal = perpendicular(normalized(b - a)?);
                let offset = normal.dot(self.line_point - a);
                let mut turned = perpendicular(normalized(to - from)?);
                if turned.dot(normal) < 0.0 {
                    turned = -turned;
                }
                let offset = match chain {
                    Chain::Continue => offset,
                    Chain::Baseline(spacing) => offset + outwards(offset) * spacing,
                };
                (Measure::Aligned { a: from, b: to }, from + turned * offset)
            }
            Measure::Angular { vertex, a, b } => {
                let from = pick(a, b)?;
                let angle = |point: Point2| {
                    let ray = normalized(point - vertex)?;
                    Some(ray.y.atan2(ray.x))
                };
                let (angle_a, angle_b, angle_p) = (angle(a)?, angle(b)?, angle(self.line_point)?);
                // Which way this one turns from `a` to `b`: +1 anticlockwise.
                let sense =
                    if Arc::from_angles(vertex, 1.0, angle_a, angle_b).contains_angle(angle_p) {
                        1.0
                    } else {
                        -1.0
                    };
                // A continued one turns on past the origin, away from this
                // one; a baseline one turns from the origin across it.
                let turning = match (chain, origin) {
                    (Chain::Continue, 1) | (Chain::Baseline(_), 0) => sense,
                    _ => -sense,
                };
                let (start, end) = (angle(from)?, angle(to)?);
                let sweep = (turning * (end - start)).rem_euclid(TAU);
                let radius = vertex.distance_to(self.line_point)
                    + match chain {
                        Chain::Continue => 0.0,
                        Chain::Baseline(spacing) => spacing,
                    };
                let middle = start + turning * sweep / 2.0;
                (
                    Measure::Angular {
                        vertex,
                        a: from,
                        b: to,
                    },
                    vertex + Vec2::from_polar(radius, middle),
                )
            }
            Measure::Radius { .. } | Measure::Diameter { .. } | Measure::Ordinate { .. } => {
                return None;
            }
        };
        Some(Self {
            measure,
            line_point,
            style: self.style,
            text_override: None,
            text_angle: None,
        })
    }

    /// For a radius or diameter: where the leader meets the circle — at the
    /// placement point's angle — and the point opposite it. `None` for the
    /// other kinds.
    pub fn circle_points(&self) -> Option<(Point2, Point2)> {
        let (center, radius) = match self.measure {
            Measure::Radius { center, radius } | Measure::Diameter { center, radius } => {
                (center, radius)
            }
            _ => return None,
        };
        let outwards = normalized(self.line_point - center).unwrap_or(Vec2::new(1.0, 0.0));
        Some((center + outwards * radius, center - outwards * radius))
    }

    /// The measured value: a distance, degrees, a radius, a diameter.
    pub fn value(&self) -> f64 {
        match self.measure {
            Measure::Linear { a, b, rotation } => {
                (b - a).dot(Vec2::from_polar(1.0, rotation)).abs()
            }
            Measure::Aligned { a, b } => a.distance_to(b),
            Measure::Angular { vertex, a, b } => self
                .angular_arc(vertex, a, b)
                .map_or(0.0, |arc| arc.sweep.to_degrees()),
            Measure::Radius { radius, .. } => radius,
            Measure::Diameter { radius, .. } => 2.0 * radius,
            Measure::Ordinate { origin, feature, x } => {
                let from = feature - origin;
                if x { from.x.abs() } else { from.y.abs() }
            }
        }
    }

    /// What the text says: the drafter's words, or the value as the style
    /// shows it — degrees with their sign, `R` before a radius, `Ø` before
    /// a diameter.
    pub fn content(&self, style: &DimStyle) -> String {
        if let Some(text) = &self.text_override {
            return text.clone();
        }
        // A length reads through the style's linear factor; an angle does not.
        let shown = match self.measure {
            Measure::Angular { .. } => self.value(),
            _ => self.value() * style.linear_factor,
        };
        let value = style.format(shown);
        match self.measure {
            Measure::Linear { .. } | Measure::Aligned { .. } | Measure::Ordinate { .. } => value,
            Measure::Angular { .. } => format!("{value}°"),
            Measure::Radius { .. } => format!("R{value}"),
            Measure::Diameter { .. } => format!("Ø{value}"),
        }
    }

    /// The dimension arc of an angle: through the placement point, between
    /// the rays. `None` when a point lies on the vertex.
    fn angular_arc(&self, vertex: Point2, a: Point2, b: Point2) -> Option<Arc> {
        let angle = |point: Point2| {
            let ray = normalized(point - vertex)?;
            Some(ray.y.atan2(ray.x))
        };
        let (angle_a, angle_b, angle_p) = (angle(a)?, angle(b)?, angle(self.line_point)?);
        let radius = vertex.distance_to(self.line_point);
        let forwards = Arc::from_angles(vertex, radius, angle_a, angle_b);
        Some(if forwards.contains_angle(angle_p) {
            forwards
        } else {
            Arc::from_angles(vertex, radius, angle_b, angle_a)
        })
    }

    /// The dimension drawn by `style`.
    pub fn layout(&self, style: &DimStyle) -> DimensionLayout {
        // Every size at the style's scale (#327).
        let style = &style.scaled();
        let content = self.content(style);
        match self.measure {
            Measure::Linear { a, b, rotation } => {
                self.linear(a, b, Vec2::from_polar(1.0, rotation), style, content)
            }
            Measure::Aligned { a, b } => {
                let direction = normalized(b - a).unwrap_or(Vec2::new(1.0, 0.0));
                self.linear(a, b, direction, style, content)
            }
            Measure::Angular { vertex, a, b } => self.angular(vertex, a, b, style, content),
            Measure::Radius { center, radius } => {
                self.radial(center, radius, false, style, content)
            }
            Measure::Diameter { center, radius } => {
                self.radial(center, radius, true, style, content)
            }
            Measure::Ordinate { feature, x, .. } => self.ordinate(feature, x, style, content),
        }
    }

    /// An ordinate dimension (#227): a leader from the feature, square to
    /// the axis measured along — up or down for an X, across for a Y —
    /// jogging across to the placement point when that is not in line,
    /// and the text beyond its end, reading along it.
    fn ordinate(
        &self,
        feature: Point2,
        x: bool,
        style: &DimStyle,
        content: String,
    ) -> DimensionLayout {
        let axis = if x {
            Vec2::new(0.0, 1.0)
        } else {
            Vec2::new(1.0, 0.0)
        };
        let along = if (self.line_point - feature).dot(axis) < 0.0 {
            -axis
        } else {
            axis
        };
        let mut text = self.text_along(content, style, along);
        let width = text.width();
        let mut layout = DimensionLayout::new(text.clone(), self.value());
        layout.lines = ordinate_leader(
            feature,
            self.line_point,
            along,
            style.extension_offset,
            2.0 * style.arrow_size,
        );
        place_on(
            &mut text,
            self.line_point + along * (style.text_gap + width / 2.0),
        );
        layout.text = text;
        layout
    }

    /// A linear or aligned dimension: the dimension line through the
    /// placement point along `direction`, extension lines from the points
    /// to it, arrowheads at the feet, the text above the middle — or
    /// past the second foot when it does not fit between the arrowheads,
    /// which themselves go outside when the line is too short for them.
    fn linear(
        &self,
        a: Point2,
        b: Point2,
        direction: Vec2,
        style: &DimStyle,
        content: String,
    ) -> DimensionLayout {
        let foot_a = foot_on_line(a, self.line_point, direction);
        let foot_b = foot_on_line(b, self.line_point, direction);
        let span = foot_b - foot_a;
        let room = span.length();
        let along = normalized(span).unwrap_or(direction);
        let mut text = self.text_along(content, style, along);
        let width = text.width();
        let mut layout = DimensionLayout::new(text.clone(), self.value());
        extension_line(&mut layout.lines, a, foot_a, style);
        extension_line(&mut layout.lines, b, foot_b, style);

        let arrow = style.arrow_size;
        let arrows_inside = room >= 2.0 * arrow;
        let text_inside = room >= 2.0 * arrow + width + 2.0 * style.text_gap;
        let (line_start, mut line_end, past) = if arrows_inside {
            (foot_a, foot_b, 0.0)
        } else {
            let out = 2.0 * arrow;
            (foot_a - along * out, foot_b + along * out, out)
        };
        let inwards = if arrows_inside { 1.0 } else { -1.0 };
        layout.arrowheads = vec![
            Arrowhead::new(foot_a, -along * inwards, arrow),
            Arrowhead::new(foot_b, along * inwards, arrow),
        ];
        let anchor = if text_inside {
            foot_a + span * 0.5
        } else {
            // Past the second foot, with the line carried on beneath it.
            let start = past + style.text_gap;
            line_end = foot_b + along * (start + width + style.text_gap);
            foot_b + along * (start + width / 2.0)
        };
        layout.lines.push(Segment::new(line_start, line_end));
        place_above(&mut text, anchor, style);
        layout.text = text;
        layout
    }

    /// An angular dimension: the arc through the placement point between
    /// the rays, extension lines out to it from points inside it,
    /// arrowheads at its ends, the text outside its middle — or past its
    /// end, along the tangent, when it does not fit.
    fn angular(
        &self,
        vertex: Point2,
        a: Point2,
        b: Point2,
        style: &DimStyle,
        content: String,
    ) -> DimensionLayout {
        let Some(arc) = self.angular_arc(vertex, a, b) else {
            // Not a dimension that can be laid out; shown as its text at
            // the placement point so that it can at least be picked.
            let mut text = self.text_along(content, style, Vec2::new(1.0, 0.0));
            text.insertion = self.line_point;
            return DimensionLayout::new(text, 0.0);
        };
        let tangent = |angle: f64| perpendicular(Vec2::from_polar(1.0, angle));
        let mut text = self.text_along(content, style, Vec2::new(1.0, 0.0));
        let width = text.width();
        let mut layout = DimensionLayout::new(text.clone(), arc.sweep.to_degrees());
        for point in [a, b] {
            let distance = vertex.distance_to(point);
            if let Some(ray) = normalized(point - vertex)
                && distance < arc.radius
            {
                let start = vertex + ray * (distance + style.extension_offset).min(arc.radius);
                let end = vertex + ray * (arc.radius + style.extension_overshoot);
                layout.lines.push(Segment::new(start, end));
            }
        }

        let arrow = style.arrow_size;
        let length = arc.radius * arc.sweep;
        let arrows_inside = length >= 2.0 * arrow;
        let text_inside = length >= 2.0 * arrow + width + 2.0 * style.text_gap;
        let inwards = if arrows_inside { 1.0 } else { -1.0 };
        layout.arrowheads = vec![
            Arrowhead::new(arc.start_point(), -tangent(arc.start) * inwards, arrow),
            Arrowhead::new(arc.end_point(), tangent(arc.end()) * inwards, arrow),
        ];
        let past = if arrows_inside {
            layout.arcs.push(arc);
            0.0
        } else {
            // The arrowheads outside: the arc carried on past each by them.
            let extra = 2.0 * arrow / arc.radius;
            layout.arcs.push(Arc::new(
                vertex,
                arc.radius,
                arc.start - extra,
                arc.sweep + 2.0 * extra,
            ));
            2.0 * arrow
        };
        if text_inside {
            let middle = arc.start + arc.sweep / 2.0;
            text = self.text_along(text.content, style, tangent(middle));
            place_beside(
                &mut text,
                arc.point_at(middle),
                Vec2::from_polar(1.0, middle),
                style,
            );
        } else {
            let along = tangent(arc.end());
            let from = arc.end_point() + along * past;
            let start = style.text_gap;
            layout.lines.push(Segment::new(
                from,
                from + along * (start + width + style.text_gap),
            ));
            text = self.text_along(text.content, style, along);
            place_above(&mut text, from + along * (start + width / 2.0), style);
        }
        layout.text = text;
        layout
    }

    /// A radius or diameter dimension: a leader from the centre (or the far
    /// side of the circle) to the circle at the placement point's angle,
    /// with the text above it where the drafter put it; or, when the text
    /// is outside the circle or does not fit inside, a leader from the
    /// circle out to the text.
    fn radial(
        &self,
        center: Point2,
        radius: f64,
        diameter: bool,
        style: &DimStyle,
        content: String,
    ) -> DimensionLayout {
        let outwards = normalized(self.line_point - center).unwrap_or(Vec2::new(1.0, 0.0));
        let on_circle = center + outwards * radius;
        let far_side = center - outwards * radius;
        let distance = center.distance_to(self.line_point);
        let mut text = self.text_along(content, style, outwards);
        let width = text.width();
        let mut layout = DimensionLayout::new(text.clone(), self.value());
        let arrow = style.arrow_size;
        let gap = style.text_gap;

        // How far along the leader, from the centre, the text may reach
        // without touching an arrowhead.
        let inner_start = if diameter { arrow + gap - radius } else { 0.0 };
        let inner_end = radius - arrow - gap;
        let fits_inside = inner_end - inner_start >= width;
        if distance <= radius && fits_inside {
            let from = if diameter { far_side } else { center };
            layout.lines.push(Segment::new(from, on_circle));
            layout
                .arrowheads
                .push(Arrowhead::new(on_circle, outwards, arrow));
            if diameter {
                layout
                    .arrowheads
                    .push(Arrowhead::new(far_side, -outwards, arrow));
            }
            let at = distance.clamp(inner_start + width / 2.0, inner_end - width / 2.0);
            place_above(&mut text, center + outwards * at, style);
        } else {
            // The text's near end, along the leader: where the drafter put
            // it, but clear of the arrowhead on the circle.
            let near = distance.max(radius + arrow + gap);
            layout
                .lines
                .push(Segment::new(on_circle, center + outwards * (near - gap)));
            layout
                .arrowheads
                .push(Arrowhead::new(on_circle, -outwards, arrow));
            place_on(&mut text, center + outwards * (near + width / 2.0));
        }
        layout.text = text;
        layout
    }

    pub fn transformed(&self, transform: &Transform) -> Self {
        Self {
            measure: self.measure.transformed(transform),
            line_point: transform.apply(self.line_point),
            text_angle: self.text_angle.map(|angle| transform.apply_angle(angle)),
            ..self.clone()
        }
    }

    /// The dimension's text, centred, at the style's height, reading along
    /// `along` turned the right way up — or at the drafter's angle, as
    /// given; placed at the origin until it is placed.
    fn text_along(&self, content: String, style: &DimStyle, along: Vec2) -> TextEntity {
        let angle = self
            .text_angle
            .unwrap_or_else(|| readable_angle(along.y.atan2(along.x)));
        TextEntity::new(Point2::ORIGIN, style.text_height, angle, content)
            .justified(Justification::Center)
    }
}

/// An extension line from `point` towards its foot on the dimension line:
/// starting the style's offset away from the point, ending the overshoot
/// past the foot. A point on the line needs none.
fn extension_line(lines: &mut Vec<Segment>, point: Point2, foot: Point2, style: &DimStyle) {
    let Some(towards) = normalized(foot - point) else {
        return;
    };
    let distance = point.distance_to(foot);
    let start = point + towards * style.extension_offset.min(distance);
    let end = foot + towards * style.extension_overshoot;
    if start != end {
        lines.push(Segment::new(start, end));
    }
}

/// The text's up direction.
fn up_of(text: &TextEntity) -> Vec2 {
    perpendicular(Vec2::from_polar(1.0, text.rotation))
}

/// How far the text's box stays from the line it belongs to: the style's
/// gap, and never less than half an arrowhead's width, so that it clears
/// the arrowheads on the line.
fn clearance(style: &DimStyle) -> f64 {
    style.text_gap.max(style.arrow_size * Arrowhead::HALF_WIDTH)
}

/// Puts the text's box clear of `anchor` on the `side` of it: the box's
/// nearer edge the clearance away, its middle over the anchor.
fn place_beside(text: &mut TextEntity, anchor: Point2, side: Vec2, style: &DimStyle) {
    let metrics = Font::simplex().metrics(&text.content, text.height);
    let reach = if up_of(text).dot(side) >= 0.0 {
        metrics.descent
    } else {
        metrics.ascent
    };
    text.insertion = anchor + side * (clearance(style) + reach);
}

/// Puts the text's box above the line through `anchor`: on its own up side.
fn place_above(text: &mut TextEntity, anchor: Point2, style: &DimStyle) {
    let up = up_of(text);
    place_beside(text, anchor, up, style);
}

/// Centres the text's box on `anchor`.
fn place_on(text: &mut TextEntity, anchor: Point2) {
    let metrics = Font::simplex().metrics(&text.content, text.height);
    text.insertion = anchor - up_of(text) * ((metrics.ascent - metrics.descent) / 2.0);
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, FRAC_PI_4};

    use cad_geometry::Primitive;
    use proptest::prelude::*;

    use super::*;
    use crate::text::TextFrame;
    use crate::{Entity, GripKind, Properties, Tables};

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    /// The dimension line's distance from the measured points' line, along
    /// a linear dimension's normal: where a chain puts it.
    fn line_offset(dimension: &DimensionEntity) -> f64 {
        let Measure::Linear { a, rotation, .. } = dimension.measure else {
            panic!("a linear dimension");
        };
        perpendicular(Vec2::from_polar(1.0, rotation)).dot(dimension.line_point - a)
    }

    #[test]
    fn an_ordinate_measures_one_coordinate_from_its_datum_with_a_leader_to_its_text() {
        let style = DimStyle::named("O");
        let x = DimensionEntity::new(
            Measure::Ordinate {
                origin: p(50.0, 20.0),
                feature: p(80.0, 50.0),
                x: true,
            },
            p(80.0, 5.0),
        );
        assert_eq!(x.value(), 30.0);
        assert_eq!(x.content(&style), "30.00");
        let layout = x.layout(&style);
        // Down from the feature, square to the X axis, and straight.
        assert_eq!(
            layout.lines,
            vec![Segment::new(
                p(80.0, 50.0 - style.extension_offset),
                p(80.0, 5.0)
            )]
        );
        assert!(layout.arrowheads.is_empty());
        // The text reads along the leader, past its end.
        let text = &layout.text;
        assert!((text.rotation - FRAC_PI_2).abs() < 1e-12);
        assert!(text.frame().corners().iter().all(|corner| corner.y < 5.0));
        assert!(
            text.frame()
                .corners()
                .iter()
                .all(|corner| (corner.x - 80.0).abs() < style.text_height)
        );

        // A Y, placed to the left and off line: across, with a jog.
        let y = DimensionEntity::new(
            Measure::Ordinate {
                origin: p(50.0, 20.0),
                feature: p(125.0, 60.0),
                x: false,
            },
            p(45.0, 63.0),
        );
        assert_eq!(y.value(), 40.0);
        let layout = y.layout(&style);
        assert_eq!(layout.lines.len(), 3, "a jog");
        assert_eq!(
            layout.lines[0].start,
            p(125.0 - style.extension_offset, 60.0)
        );
        assert_eq!(layout.lines[2].end, p(45.0, 63.0));
        assert_eq!(layout.text.rotation, 0.0);
        assert!(
            layout
                .text
                .frame()
                .corners()
                .iter()
                .all(|corner| corner.x < 45.0)
        );

        // Its feature is its grip, then where it is placed; the datum
        // moves only with the whole.
        assert_eq!(y.points(), vec![p(125.0, 60.0), p(45.0, 63.0)]);
        let moved = y.with_point(0, p(130.0, 70.0)).unwrap();
        assert_eq!(moved.value(), 50.0);
        assert_eq!(y.nearer_origin(p(0.0, 0.0)), None, "no chaining");
        // A quarter turn makes the Y an X.
        let turned = y.transformed(&Transform::rotation(p(50.0, 20.0), 90.0));
        let Measure::Ordinate { x, .. } = turned.measure else {
            panic!("an ordinate");
        };
        assert!(x);
        assert!((turned.value() - 40.0).abs() < 1e-9);
    }

    #[test]
    fn a_continued_linear_dimension_shares_the_line_and_a_baseline_one_steps_out() {
        let style = DimStyleId::from_raw(3);
        let above = DimensionEntity::new(
            Measure::Linear {
                a: p(0.0, 0.0),
                b: p(10.0, 0.0),
                rotation: 0.0,
            },
            p(5.0, 10.0),
        )
        .in_style(style)
        .saying(Some("ten".into()));

        let next = above.chained(1, p(25.0, 0.0), Chain::Continue).unwrap();
        assert_eq!(
            next.measure,
            Measure::Linear {
                a: p(10.0, 0.0),
                b: p(25.0, 0.0),
                rotation: 0.0
            }
        );
        assert_eq!(line_offset(&next), 10.0);
        assert_eq!(next.value(), 15.0);
        assert_eq!((next.style, next.text_override.as_deref()), (style, None));

        let base = above
            .chained(0, p(25.0, 0.0), Chain::Baseline(3.75))
            .unwrap();
        assert_eq!(base.measure.points(), vec![p(0.0, 0.0), p(25.0, 0.0)]);
        assert_eq!(line_offset(&base), 13.75);
        // Below the points, a baseline steps further down.
        let below = DimensionEntity::new(above.measure, p(5.0, -10.0));
        let base = below
            .chained(0, p(25.0, 0.0), Chain::Baseline(3.75))
            .unwrap();
        assert_eq!(line_offset(&base), -13.75);
        // From the first extension line, a continued one runs back the other way.
        let back = above.chained(0, p(-8.0, 0.0), Chain::Continue).unwrap();
        assert_eq!(back.measure.points(), vec![p(0.0, 0.0), p(-8.0, 0.0)]);
        assert_eq!(line_offset(&back), 10.0);
        // A vertical one keeps its rotation.
        let vertical = DimensionEntity::new(
            Measure::Linear {
                a: p(0.0, 0.0),
                b: p(3.0, 10.0),
                rotation: FRAC_PI_2,
            },
            p(-6.0, 0.0),
        );
        let next = vertical
            .chained(0, p(1.0, 30.0), Chain::Baseline(2.0))
            .unwrap();
        assert_eq!(next.value(), 30.0);
        assert!((line_offset(&next) - 8.0).abs() < 1e-12);
    }

    #[test]
    fn an_aligned_chain_keeps_its_offset_on_the_same_side() {
        let normal = Vec2::new(-1.0, 1.0) * (0.5_f64).sqrt();
        let base = DimensionEntity::new(
            Measure::Aligned {
                a: p(0.0, 0.0),
                b: p(10.0, 10.0),
            },
            p(5.0, 5.0) + normal * 4.0,
        );
        let offset = |d: &DimensionEntity| {
            let Measure::Aligned { a, b } = d.measure else {
                panic!("aligned")
            };
            perpendicular(normalized(b - a).unwrap()).dot(d.line_point - a)
        };
        let next = base.chained(1, p(20.0, 20.0), Chain::Continue).unwrap();
        assert!((offset(&next) - 4.0).abs() < 1e-12);
        let stacked = base
            .chained(0, p(20.0, 20.0), Chain::Baseline(3.75))
            .unwrap();
        assert!((offset(&stacked) - 7.75).abs() < 1e-12);
        // Back from `a`, the line stays on the side it was in the world.
        let back = base.chained(0, p(-10.0, -10.0), Chain::Continue).unwrap();
        assert!(normal.dot(back.line_point - p(0.0, 0.0)) > 3.99);
    }

    #[test]
    fn an_angular_chain_turns_on_from_the_origin_the_way_the_angle_does() {
        let vertex = p(0.0, 0.0);
        let quarter = |place: Point2| {
            DimensionEntity::new(
                Measure::Angular {
                    vertex,
                    a: p(10.0, 0.0),
                    b: p(0.0, 10.0),
                },
                place,
            )
        };
        // The quarter anticlockwise from +X to +Y, its arc 8 out.
        let base = quarter(Point2::ORIGIN + Vec2::from_polar(8.0, FRAC_PI_4));
        let next = base.chained(1, p(-10.0, 0.0), Chain::Continue).unwrap();
        assert!((next.value() - 90.0).abs() < 1e-9);
        assert!(near(
            next.line_point,
            Point2::ORIGIN + Vec2::from_polar(8.0, 3.0 * FRAC_PI_4)
        ));
        let across = base
            .chained(0, p(-10.0, 0.0), Chain::Baseline(3.75))
            .unwrap();
        assert!((across.value() - 180.0).abs() < 1e-9);
        assert!(near(
            across.line_point,
            Point2::ORIGIN + Vec2::from_polar(11.75, FRAC_PI_2)
        ));
        // The reflex side: from +X clockwise round to +Y. Continued past
        // +Y it turns on clockwise, into the quarter it left out.
        let reflex = quarter(Point2::ORIGIN + Vec2::from_polar(8.0, 5.0 * FRAC_PI_4));
        assert!((reflex.value() - 270.0).abs() < 1e-9);
        let next = reflex.chained(1, p(10.0, 10.0), Chain::Continue).unwrap();
        assert!((next.value() - 45.0).abs() < 1e-9);
        assert!(near(
            next.line_point,
            Point2::ORIGIN + Vec2::from_polar(8.0, 3.0 * std::f64::consts::FRAC_PI_8)
        ));
    }

    #[test]
    fn a_chain_starts_from_the_nearer_extension_line_and_never_from_a_radius() {
        let linear = DimensionEntity::new(
            Measure::Linear {
                a: p(0.0, 0.0),
                b: p(10.0, 0.0),
                rotation: 0.0,
            },
            p(5.0, 10.0),
        );
        assert_eq!(linear.nearer_origin(p(1.0, 9.0)), Some(0));
        assert_eq!(linear.nearer_origin(p(9.0, 3.0)), Some(1));
        let angle = DimensionEntity::new(
            Measure::Angular {
                vertex: p(0.0, 0.0),
                a: p(10.0, 0.0),
                b: p(0.0, 10.0),
            },
            p(6.0, 6.0),
        );
        assert_eq!(angle.nearer_origin(p(8.0, 1.0)), Some(0));
        assert_eq!(angle.nearer_origin(p(1.0, 8.0)), Some(1));
        let radius = DimensionEntity::new(
            Measure::Radius {
                center: p(0.0, 0.0),
                radius: 5.0,
            },
            p(8.0, 0.0),
        );
        assert_eq!(radius.nearer_origin(p(5.0, 0.0)), None);
        assert_eq!(radius.chained(0, p(1.0, 1.0), Chain::Continue), None);
        assert_eq!(linear.chained(2, p(1.0, 1.0), Chain::Continue), None);
    }

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9
    }

    fn standard() -> DimStyle {
        DimStyle::named(DimStyle::STANDARD)
    }

    fn horizontal(a: Point2, b: Point2, line_point: Point2) -> DimensionEntity {
        DimensionEntity::new(
            Measure::Linear {
                a,
                b,
                rotation: 0.0,
            },
            line_point,
        )
    }

    #[test]
    fn a_horizontal_dimension_is_laid_out_by_the_standard_style() {
        let dim = horizontal(p(0.0, 0.0), p(100.0, 0.0), p(0.0, 20.0));
        let layout = dim.layout(&standard());
        assert_eq!(layout.value, 100.0);
        assert_eq!(layout.text.content, "100.00");
        // Extension lines start the offset away from the points and run the
        // overshoot past the dimension line, which comes last.
        assert_eq!(layout.lines.len(), 3);
        assert!(close(layout.lines[0].start, p(0.0, 0.625)));
        assert!(close(layout.lines[0].end, p(0.0, 21.25)));
        assert!(close(layout.lines[1].start, p(100.0, 0.625)));
        assert!(close(layout.lines[1].end, p(100.0, 21.25)));
        assert!(close(layout.lines[2].start, p(0.0, 20.0)));
        assert!(close(layout.lines[2].end, p(100.0, 20.0)));
        // Arrowheads at the feet, pointing outwards: bases inside.
        assert!(close(layout.arrowheads[0].tip, p(0.0, 20.0)));
        assert!(
            layout.arrowheads[0]
                .base
                .iter()
                .all(|corner| (corner.x - 2.5).abs() < 1e-9)
        );
        assert!(close(layout.arrowheads[1].tip, p(100.0, 20.0)));
        assert!(
            layout.arrowheads[1]
                .base
                .iter()
                .all(|corner| (corner.x - 97.5).abs() < 1e-9)
        );
        // The text: centred over the middle, reading along +X, its box the
        // gap above the line.
        assert_eq!(layout.text.rotation, 0.0);
        assert_eq!(layout.text.height, 2.5);
        assert_eq!(layout.text.justification, Justification::Center);
        assert!((layout.text.insertion.x - 50.0).abs() < 1e-9);
        let bottom = layout.text.frame().corners()[0].y;
        assert!((bottom - 20.625).abs() < 1e-9, "{bottom}");
    }

    #[test]
    fn a_vertical_dimension_reads_upwards_and_measures_only_the_rise() {
        let dim = DimensionEntity::new(
            Measure::Linear {
                a: p(0.0, 0.0),
                b: p(30.0, 40.0),
                rotation: FRAC_PI_2,
            },
            p(-10.0, 0.0),
        );
        let layout = dim.layout(&standard());
        assert!((layout.value - 40.0).abs() < 1e-12);
        assert!((layout.text.rotation - FRAC_PI_2).abs() < 1e-12);
        let line = layout.lines.last().unwrap();
        assert!(close(line.start, p(-10.0, 0.0)) && close(line.end, p(-10.0, 40.0)));
        // The same points: horizontal measures the run, aligned the distance.
        assert_eq!(
            horizontal(p(0.0, 0.0), p(30.0, 40.0), p(0.0, -10.0)).value(),
            30.0
        );
        let aligned = DimensionEntity::new(
            Measure::Aligned {
                a: p(0.0, 0.0),
                b: p(30.0, 40.0),
            },
            p(-8.0, 6.0),
        );
        assert_eq!(aligned.value(), 50.0);
        let layout = aligned.layout(&standard());
        let line = layout.lines.last().unwrap();
        assert!(close(line.start, p(-8.0, 6.0)) && close(line.end, p(22.0, 46.0)));
    }

    #[test]
    fn a_short_dimension_puts_its_arrowheads_and_then_its_text_outside() {
        let layout = horizontal(p(0.0, 0.0), p(3.0, 0.0), p(0.0, 10.0)).layout(&standard());
        // Too short for two arrowheads: they point inwards from outside, the
        // line runs two arrow lengths past each foot and on beneath the text.
        assert!(layout.arrowheads[0].base.iter().all(|c| c.x < 0.0));
        assert!(layout.arrowheads[1].base.iter().all(|c| c.x > 3.0));
        let line = layout.lines.last().unwrap();
        assert!((line.start.x + 5.0).abs() < 1e-9);
        let corners = layout.text.frame().corners();
        assert!(
            corners.iter().all(|c| c.x >= 3.0 + 5.0 + 0.625 - 1e-9),
            "{corners:?}"
        );
        assert!(line.end.x >= corners[1].x - 1e-9);
        // Room for the arrowheads but not the text: it goes past the end.
        let layout = horizontal(p(0.0, 0.0), p(8.0, 0.0), p(0.0, 10.0)).layout(&standard());
        assert!(layout.arrowheads[1].base.iter().all(|c| c.x < 8.0));
        assert!(layout.text.frame().corners().iter().all(|c| c.x > 8.0));
        // No rise at all: a zero still lays out.
        let flat = horizontal(p(0.0, 0.0), p(0.0, 10.0), p(5.0, 0.0)).layout(&standard());
        assert_eq!((flat.value, flat.text.content.as_str()), (0.0, "0.00"));
    }

    #[test]
    fn an_angular_dimension_measures_the_angle_on_the_side_it_is_placed() {
        let corner = Measure::Angular {
            vertex: p(0.0, 0.0),
            a: p(10.0, 0.0),
            b: p(0.0, 10.0),
        };
        let right = DimensionEntity::new(corner, p(20.0, 20.0));
        let layout = right.layout(&standard());
        assert!((layout.value - 90.0).abs() < 1e-9);
        assert_eq!(layout.text.content, "90.00°");
        assert_eq!(layout.arcs.len(), 1);
        let arc = layout.arcs[0];
        assert!(close(arc.center, p(0.0, 0.0)));
        assert!((arc.radius - 800f64.sqrt()).abs() < 1e-9);
        assert!((arc.sweep - FRAC_PI_2).abs() < 1e-9 && arc.start.abs() < 1e-9);
        // The points lie inside the arc: extension lines run out to it,
        // radially, from the offset past the points to the overshoot.
        assert_eq!(layout.lines.len(), 2);
        assert!(close(layout.lines[0].start, p(10.625, 0.0)));
        assert!(close(layout.lines[0].end, p(800f64.sqrt() + 1.25, 0.0)));
        assert!(close(layout.lines[1].start, p(0.0, 10.625)));
        for arrow in &layout.arrowheads {
            assert!((arrow.tip.distance_to(p(0.0, 0.0)) - arc.radius).abs() < 1e-9);
        }
        let outward = Vec2::from_polar(1.0, FRAC_PI_4);
        assert!(
            layout
                .text
                .frame()
                .corners()
                .iter()
                .all(|c| (*c - p(0.0, 0.0)).dot(outward) > arc.radius)
        );
        // Placed on the other side, it is the reflex angle.
        let reflex = DimensionEntity::new(corner, p(-5.0, -5.0));
        assert!((reflex.value() - 270.0).abs() < 1e-9);
        // Points beyond the arc need no extension lines: the arc meets
        // the rays. (The text does not fit on so short an arc, so a line
        // runs beneath it past the end: tangent, not radial.)
        let tight = DimensionEntity::new(corner, p(5.0, 5.0));
        let layout = tight.layout(&standard());
        assert!(layout.lines.len() <= 1);
        let along = Vec2::from_polar(1.0, FRAC_PI_2);
        assert!(
            layout
                .lines
                .iter()
                .all(|line| (line.end - line.start).dot(along).abs() < 1e-9)
        );
    }

    #[test]
    fn radius_and_diameter_dimensions_lead_to_the_circle_at_the_placement_angle() {
        let circle = |radius| Measure::Radius {
            center: p(0.0, 0.0),
            radius,
        };
        let inside = DimensionEntity::new(circle(20.0), p(10.0, 0.0));
        let layout = inside.layout(&standard());
        assert_eq!(layout.text.content, "R20.00");
        assert_eq!(layout.lines.len(), 1);
        assert!(
            close(layout.lines[0].start, p(0.0, 0.0)) && close(layout.lines[0].end, p(20.0, 0.0))
        );
        assert_eq!(layout.arrowheads.len(), 1);
        assert!(close(layout.arrowheads[0].tip, p(20.0, 0.0)));
        assert!(
            layout.arrowheads[0].base[0].x < 20.0,
            "pointing out at the circle"
        );
        assert!((layout.text.insertion.x - 10.0).abs() < 1e-9 && layout.text.insertion.y > 0.0);

        let outside = DimensionEntity::new(circle(20.0), p(0.0, 30.0));
        let layout = outside.layout(&standard());
        assert!(close(layout.lines[0].start, p(0.0, 20.0)));
        assert!(
            layout.arrowheads[0].base[0].y > 20.0,
            "pointing in from outside"
        );
        assert!(
            layout
                .text
                .frame()
                .corners()
                .iter()
                .all(|c| c.y >= 30.0 - 1e-9)
        );

        let diameter = DimensionEntity::new(
            Measure::Diameter {
                center: p(0.0, 0.0),
                radius: 20.0,
            },
            p(0.0, 5.0),
        );
        let layout = diameter.layout(&standard());
        assert_eq!(layout.text.content, "Ø40.00");
        assert!((layout.value - 40.0).abs() < 1e-12);
        assert_eq!(layout.arrowheads.len(), 2);
        assert!(
            close(layout.lines[0].start, p(0.0, -20.0)) && close(layout.lines[0].end, p(0.0, 20.0))
        );
        assert_eq!(
            diameter.circle_points().map(|(near, far)| (near.y, far.y)),
            Some((20.0, -20.0))
        );
        // Too small a circle for its text: the text goes outside.
        let small = DimensionEntity::new(circle(3.0), p(1.0, 0.0));
        let layout = small.layout(&standard());
        assert!(layout.text.frame().corners().iter().all(|c| c.x > 3.0));
        assert!(
            horizontal(p(0.0, 0.0), p(1.0, 0.0), p(0.0, 1.0))
                .circle_points()
                .is_none()
        );
    }

    #[test]
    fn the_text_can_be_the_drafters_and_the_style_decides_the_value_shown() {
        let dim = horizontal(p(0.0, 0.0), p(12.345, 0.0), p(0.0, 5.0));
        let mut style = standard();
        assert_eq!(dim.content(&style), "12.35");
        style.decimals = 1;
        style.suffix = " mm".to_owned();
        assert_eq!(dim.content(&style), "12.3 mm");
        let worded = dim.clone().saying(Some("TYP".to_owned()));
        assert_eq!(worded.content(&style), "TYP");
        assert_eq!(worded.layout(&style).text.content, "TYP");
        assert_eq!(worded.saying(None).content(&style), "12.3 mm");
        assert_eq!(
            dim.clone().in_style(DimStyleId::from_raw(3)).style,
            DimStyleId::from_raw(3)
        );
        // The drafter's angle is taken as given, upside down or not; it
        // turns with the dimension.
        let turned = dim.clone().turned(Some(FRAC_PI_4 * 5.0));
        assert_eq!(turned.layout(&style).text.rotation, FRAC_PI_4 * 5.0);
        let rotated = turned.transformed(&Transform::rotation(p(0.0, 0.0), 90.0));
        assert!((rotated.text_angle.unwrap() - (FRAC_PI_4 * 7.0)).abs() < 1e-9);
        assert_eq!(
            dim.turned(Some(f64::NAN)).check(),
            Err(DimensionError::NotFinite)
        );
    }

    #[test]
    fn what_cannot_be_laid_out_is_refused() {
        use DimensionError::*;
        let at = |x, y| p(x, y);
        assert_eq!(
            horizontal(at(0.0, 0.0), at(0.0, 0.0), at(0.0, 5.0)).check(),
            Err(Coincident)
        );
        assert_eq!(
            horizontal(at(0.0, 0.0), at(f64::NAN, 0.0), at(0.0, 5.0)).check(),
            Err(NotFinite)
        );
        let turned = DimensionEntity::new(
            Measure::Linear {
                a: at(0.0, 0.0),
                b: at(1.0, 0.0),
                rotation: f64::INFINITY,
            },
            at(0.0, 1.0),
        );
        assert_eq!(turned.check(), Err(NotFinite));
        let aligned = DimensionEntity::new(
            Measure::Aligned {
                a: at(1.0, 1.0),
                b: at(1.0, 1.0),
            },
            at(0.0, 0.0),
        );
        assert_eq!(aligned.check(), Err(Coincident));
        let angle = |a, b, place| {
            DimensionEntity::new(
                Measure::Angular {
                    vertex: at(0.0, 0.0),
                    a,
                    b,
                },
                place,
            )
            .check()
        };
        assert_eq!(
            angle(at(0.0, 0.0), at(1.0, 0.0), at(1.0, 1.0)),
            Err(OnVertex)
        );
        assert_eq!(
            angle(at(1.0, 0.0), at(0.0, 1.0), at(0.0, 0.0)),
            Err(OnVertex)
        );
        assert_eq!(angle(at(1.0, 0.0), at(0.0, 1.0), at(1.0, 1.0)), Ok(()));
        let radius = |radius| {
            DimensionEntity::new(
                Measure::Radius {
                    center: at(0.0, 0.0),
                    radius,
                },
                at(1.0, 0.0),
            )
            .check()
        };
        assert_eq!(radius(0.0), Err(NoRadius));
        assert_eq!(radius(f64::NAN), Err(NoRadius));
        assert_eq!(radius(2.0), Ok(()));
        let diameter = DimensionEntity::new(
            Measure::Diameter {
                center: at(0.0, 0.0),
                radius: -1.0,
            },
            at(1.0, 0.0),
        );
        assert_eq!(diameter.check(), Err(NoRadius));
        assert_eq!(
            Coincident.explain(),
            "The two points of a dimension must be apart."
        );
        assert_eq!(
            Measure::Diameter {
                center: at(0.0, 0.0),
                radius: 1.0
            }
            .kind(),
            "diameter"
        );
    }

    #[test]
    fn the_points_are_what_is_measured_then_the_placement_and_each_may_move() {
        let dim = DimensionEntity::new(
            Measure::Angular {
                vertex: p(0.0, 0.0),
                a: p(10.0, 0.0),
                b: p(0.0, 10.0),
            },
            p(5.0, 5.0),
        );
        assert_eq!(
            dim.points(),
            vec![p(0.0, 0.0), p(10.0, 0.0), p(0.0, 10.0), p(5.0, 5.0)]
        );
        let moved = dim.with_point(1, p(20.0, 0.0)).unwrap();
        assert_eq!(
            moved.measure,
            Measure::Angular {
                vertex: p(0.0, 0.0),
                a: p(20.0, 0.0),
                b: p(0.0, 10.0)
            }
        );
        let placed = dim.with_point(3, p(-5.0, -5.0)).unwrap();
        assert_eq!(placed.line_point, p(-5.0, -5.0));
        assert!((placed.value() - 270.0).abs() < 1e-9);
        assert_eq!(dim.with_point(4, p(0.0, 0.0)), None);
        let radius = DimensionEntity::new(
            Measure::Radius {
                center: p(0.0, 0.0),
                radius: 5.0,
            },
            p(2.0, 0.0),
        );
        assert_eq!(radius.points(), vec![p(0.0, 0.0), p(2.0, 0.0)]);
        assert_eq!(
            radius.with_point(0, p(1.0, 1.0)).unwrap().measure,
            Measure::Radius {
                center: p(1.0, 1.0),
                radius: 5.0
            }
        );
        // Through the entity: a grip per point, and none may make what
        // cannot be laid out.
        let entity = Entity::from(horizontal(p(0.0, 0.0), p(10.0, 0.0), p(0.0, 5.0)));
        let grips = entity.grips();
        assert_eq!(grips.len(), 3);
        assert_eq!(
            grips.iter().map(|grip| grip.kind).collect::<Vec<_>>(),
            [
                GripKind::Definition,
                GripKind::Definition,
                GripKind::Placement
            ]
        );
        assert!(
            entity.grip_moved(1, p(0.0, 0.0)).is_none(),
            "onto the other point"
        );
        let Some(Entity::Dimension(moved)) = entity.grip_moved(2, p(0.0, -7.0)) else {
            panic!("the placement moves");
        };
        assert_eq!(moved.line_point, p(0.0, -7.0));
        assert!(entity.grip_moved(3, p(0.0, 0.0)).is_none());
    }

    #[test]
    fn the_angle_between_two_lines_is_the_one_the_placement_falls_in() {
        // Lines at 0° and 60° through the origin.
        let (vertex, a, b) = (p(0.0, 0.0), p(100.0, 0.0), p(50.0, 86.602_540_378_443_86));
        let at = |place| {
            DimensionEntity::new(Measure::between_lines(vertex, a, b, place), place).value()
        };
        assert!((at(p(60.0, 20.0)) - 60.0).abs() < 1e-9);
        assert!((at(p(-60.0, 20.0)) - 120.0).abs() < 1e-9);
        assert!((at(p(-60.0, -20.0)) - 60.0).abs() < 1e-9);
        assert!((at(p(60.0, -20.0)) - 120.0).abs() < 1e-9);
        // From the lines' points: the same angles, the rays through the
        // farther ends.
        let between = Measure::between_line_segments(
            [p(-10.0, 0.0), p(100.0, 0.0)],
            [
                p(-5.0, -8.660_254_037_844_386),
                p(50.0, 86.602_540_378_443_86),
            ],
            p(60.0, 20.0),
        )
        .unwrap();
        let value = DimensionEntity::new(between, p(60.0, 20.0)).value();
        assert!((value - 60.0).abs() < 1e-6, "{value} from {between:?}");
        assert_eq!(
            Measure::between_line_segments(
                [p(0.0, 0.0), p(10.0, 0.0)],
                [p(0.0, 5.0), p(10.0, 5.0)],
                p(5.0, 2.0)
            ),
            None,
            "parallel"
        );
    }

    #[test]
    fn a_transformed_dimension_measures_the_transformed_points() {
        let dim = horizontal(p(0.0, 0.0), p(10.0, 0.0), p(0.0, 5.0));
        let turned = dim.transformed(&Transform::rotation(p(0.0, 0.0), 90.0));
        let Measure::Linear { a, b, rotation } = turned.measure else {
            panic!("still linear");
        };
        assert!(close(a, p(0.0, 0.0)) && close(b, p(0.0, 10.0)));
        assert!((rotation - FRAC_PI_2).abs() < 1e-12);
        assert!((turned.value() - 10.0).abs() < 1e-9);
        let scaled = dim.transformed(&Transform::scaling(p(0.0, 0.0), 2.0).unwrap());
        assert!((scaled.value() - 20.0).abs() < 1e-9);
        let radius = DimensionEntity::new(
            Measure::Radius {
                center: p(0.0, 0.0),
                radius: 5.0,
            },
            p(2.0, 0.0),
        );
        let scaled = radius.transformed(&Transform::scaling(p(1.0, 0.0), 3.0).unwrap());
        assert_eq!(
            scaled.measure,
            Measure::Radius {
                center: p(-2.0, 0.0),
                radius: 15.0
            }
        );
        // Mirrored, the text still reads the right way up, over the middle.
        let long = horizontal(p(0.0, 0.0), p(100.0, 0.0), p(0.0, 5.0));
        let mirrored = long.transformed(&Transform::mirror(p(0.0, 0.0), p(0.0, 1.0)).unwrap());
        let text = mirrored.layout(&standard()).text;
        assert!(
            text.rotation.abs() < 1e-9 && (text.insertion.x + 50.0).abs() < 1e-9,
            "{text:?}"
        );
        assert_eq!(mirrored.value(), 100.0);
    }

    #[test]
    fn a_dimension_comes_apart_into_what_its_style_laid_out() {
        let entity = Entity::from(horizontal(p(0.0, 0.0), p(100.0, 0.0), p(0.0, 20.0)));
        let parts = entity
            .exploded(Properties::default(), Tables::none())
            .unwrap();
        let kinds: Vec<&str> = parts.iter().map(|(part, _)| part.kind_name()).collect();
        assert_eq!(
            kinds,
            ["line", "line", "line", "polyline", "polyline", "text"]
        );
        let together = parts
            .iter()
            .map(|(part, _)| part.bounds(Tables::none()))
            .reduce(|a, b| a.union(&b))
            .unwrap();
        let whole = entity.bounds(Tables::none());
        assert!(close(together.min(), whole.min()) && close(together.max(), whole.max()));
        // Picked by its lines and its text, not by the space between.
        assert_eq!(entity.distance_to(p(50.0, 20.0), Tables::none()), 0.0);
        assert!(entity.distance_to(p(50.0, 10.0), Tables::none()) > 9.0);
        assert!(entity.primitives(Tables::none()).is_empty(), "not a curve");
    }

    // ── Properties (DESIGN §41) ──

    fn point() -> impl Strategy<Value = Point2> {
        (-1000.0f64..1000.0, -1000.0f64..1000.0).prop_map(|(x, y)| p(x, y))
    }

    fn style() -> impl Strategy<Value = DimStyle> {
        (
            0.5f64..10.0,
            0.5f64..10.0,
            0.0f64..3.0,
            0.0f64..3.0,
            0.0f64..3.0,
            0u8..=8,
            prop::bool::ANY,
        )
            .prop_map(
                |(height, arrow, gap, offset, overshoot, decimals, suffix)| DimStyle {
                    name: "Any".to_owned(),
                    text_height: height,
                    arrow_size: arrow,
                    text_gap: gap,
                    extension_offset: offset,
                    extension_overshoot: overshoot,
                    // Not part of a single dimension's layout.
                    baseline_spacing: 3.75,
                    decimals,
                    suffix: if suffix {
                        " mm".to_owned()
                    } else {
                        String::new()
                    },
                    scale: 1.0,
                    linear_factor: 1.0,
                },
            )
    }

    fn measure() -> impl Strategy<Value = Measure> {
        prop_oneof![
            (point(), point(), -7.0f64..7.0).prop_map(|(a, b, rotation)| Measure::Linear {
                a,
                b,
                rotation
            }),
            (point(), point()).prop_map(|(a, b)| Measure::Aligned { a, b }),
            (point(), point(), point()).prop_map(|(vertex, a, b)| Measure::Angular {
                vertex,
                a,
                b
            }),
            (point(), 0.1f64..500.0)
                .prop_map(|(center, radius)| Measure::Radius { center, radius }),
            (point(), 0.1f64..500.0)
                .prop_map(|(center, radius)| Measure::Diameter { center, radius }),
            (point(), point(), any::<bool>()).prop_map(|(origin, feature, x)| Measure::Ordinate {
                origin,
                feature,
                x
            }),
        ]
    }

    fn dimension() -> impl Strategy<Value = DimensionEntity> {
        (measure(), point())
            .prop_map(|(measure, line_point)| DimensionEntity::new(measure, line_point))
            .prop_filter("can be laid out", |dimension| dimension.check().is_ok())
    }

    fn near(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-6 * (1.0 + a.x.abs() + a.y.abs())
    }

    /// Whether the text's box and the arrowhead share any area. Touching
    /// is not overlapping: the arrowhead is drawn in by a hair first.
    fn overlaps(frame: &TextFrame, arrow: &Arrowhead) -> bool {
        let [tip, b0, b1] = arrow.corners();
        let centroid = p((tip.x + b0.x + b1.x) / 3.0, (tip.y + b0.y + b1.y) / 3.0);
        let inward = |corner: Point2| centroid + (corner - centroid) * 0.999;
        let arrow = Arrowhead {
            tip: inward(tip),
            base: [inward(b0), inward(b1)],
        };
        arrow.corners().iter().any(|corner| frame.contains(*corner))
            || frame.corners().iter().any(|corner| arrow.contains(*corner))
            || frame.edges().iter().any(|edge| {
                arrow.segments().iter().any(|side| {
                    !Primitive::Segment(*edge)
                        .intersect(&Primitive::Segment(*side))
                        .as_slice()
                        .is_empty()
                })
            })
    }

    proptest! {
        /// #327: a style's scale draws a dimension exactly as the same
        /// style with every size multiplied — text, arrows, gaps, lines —
        /// and measures the same value.
        #[test]
        fn a_styles_scale_lays_out_as_its_sizes_multiplied(
            dim in dimension(), style in style(), k in 0.1f64..20.0,
        ) {
            let scaled = DimStyle { scale: k, ..style.clone() };
            let multiplied = DimStyle {
                text_height: style.text_height * k,
                arrow_size: style.arrow_size * k,
                text_gap: style.text_gap * k,
                extension_offset: style.extension_offset * k,
                extension_overshoot: style.extension_overshoot * k,
                baseline_spacing: style.baseline_spacing * k,
                ..style.clone()
            };
            prop_assert_eq!(dim.layout(&scaled), dim.layout(&multiplied));
            prop_assert_eq!(dim.layout(&scaled).value, dim.layout(&style).value);
        }

        #[test]
        fn the_value_is_what_the_points_measure_whatever_the_style(
            dim in dimension(), style in style(),
        ) {
            let layout = dim.layout(&style);
            let value = dim.value();
            prop_assert!((layout.value - value).abs() <= 1e-9 * value.abs().max(1.0));
            match dim.measure {
                Measure::Aligned { a, b } => {
                    let distance = a.distance_to(b);
                    prop_assert!((value - distance).abs() < 1e-9 * distance.max(1.0));
                }
                Measure::Linear { a, b, rotation } => {
                    let along = Vec2::from_polar(1.0, rotation);
                    prop_assert!((value - (b - a).dot(along).abs()).abs() < 1e-6);
                }
                Measure::Angular { vertex, a, b } => {
                    let (ra, rb) = (a - vertex, b - vertex);
                    let between = (ra.x * rb.y - ra.y * rb.x).atan2(ra.dot(rb)).abs().to_degrees();
                    prop_assert!(
                        (value - between).abs() < 1e-6 || (value - (360.0 - between)).abs() < 1e-6,
                        "{value} vs {between}"
                    );
                }
                Measure::Radius { radius, .. } => prop_assert_eq!(value, radius),
                Measure::Diameter { radius, .. } => prop_assert_eq!(value, 2.0 * radius),
                Measure::Ordinate { origin, feature, x } => {
                    let along = if x { feature.x - origin.x } else { feature.y - origin.y };
                    prop_assert_eq!(value, along.abs());
                }
            }
            prop_assert!(layout.text.content.contains(&style.format(value)));
            prop_assert_eq!(layout.text.height, style.text_height);
        }

        #[test]
        fn extension_lines_stand_square_to_the_dimension_line_and_arrowheads_lie_on_it(
            dim in dimension(), style in style(),
        ) {
            let layout = dim.layout(&style);
            let on = |point: Point2, distance: f64| distance < 1e-7 * (1.0 + point.x.abs() + point.y.abs());
            match dim.measure {
                Measure::Linear { .. } | Measure::Aligned { .. } => {
                    let line = *layout.lines.last().unwrap();
                    let along = normalized(line.end - line.start).unwrap();
                    for extension in &layout.lines[..layout.lines.len() - 1] {
                        let direction = normalized(extension.end - extension.start).unwrap();
                        prop_assert!(direction.dot(along).abs() < 1e-6, "{direction:?} along {along:?}");
                    }
                    for arrow in &layout.arrowheads {
                        prop_assert!(on(arrow.tip, line.distance_to_point(arrow.tip)));
                    }
                }
                Measure::Angular { vertex, .. } => {
                    let arc = layout.arcs[0];
                    for arrow in &layout.arrowheads {
                        prop_assert!(on(arrow.tip, (arrow.tip.distance_to(vertex) - arc.radius).abs()));
                    }
                    // Every line is radial (an extension line), its line
                    // through the vertex, or tangent (the line beneath a text
                    // placed past the arc), its line the arc's radius from
                    // it — which, when the arrowheads are outside, starts an
                    // arrow's length past the arc's end, not on it.
                    for line in &layout.lines {
                        let direction = normalized(line.end - line.start).unwrap();
                        let off = line.start - vertex;
                        let apart = (direction.x * off.y - direction.y * off.x).abs();
                        let scale = 1e-7 * (1.0 + arc.radius + off.length());
                        prop_assert!(
                            apart < scale || (apart - arc.radius).abs() < scale,
                            "a line {apart} from the vertex, the arc {}", arc.radius
                        );
                    }
                }
                Measure::Radius { center, radius } | Measure::Diameter { center, radius } => {
                    for arrow in &layout.arrowheads {
                        prop_assert!(on(arrow.tip, (arrow.tip.distance_to(center) - radius).abs()));
                    }
                }
                // An ordinate's leader reaches where it is placed, in one
                // connected run, and has no arrowheads.
                Measure::Ordinate { .. } => {
                    prop_assert!(layout.arrowheads.is_empty());
                    let last = layout.lines.last().unwrap();
                    prop_assert!(on(dim.line_point, last.end.distance_to(dim.line_point)));
                    for pair in layout.lines.windows(2) {
                        prop_assert!(on(pair[0].end, pair[0].end.distance_to(pair[1].start)));
                    }
                }
            }
            for arrow in &layout.arrowheads {
                prop_assert!((arrow.length() - style.arrow_size).abs() < 1e-6 * style.arrow_size);
            }
        }

        #[test]
        fn the_text_box_overlaps_no_arrowhead_and_the_bounds_hold_everything(
            dim in dimension(), style in style(),
        ) {
            let layout = dim.layout(&style);
            let frame = layout.text.frame();
            for arrow in &layout.arrowheads {
                prop_assert!(!overlaps(&frame, arrow), "{arrow:?} vs {:?}", frame.corners());
            }
            let bounds = layout.bounds().expanded(1e-6);
            for segment in layout.segments() {
                prop_assert!(bounds.contains_point(segment.start) && bounds.contains_point(segment.end));
            }
            for corner in frame.corners() {
                prop_assert!(bounds.contains_point(corner));
            }
            for arc in &layout.arcs {
                prop_assert!(bounds.contains_point(arc.start_point()) && bounds.contains_point(arc.mid_point()));
            }
            let readable = layout.text.rotation;
            prop_assert!(readable > -FRAC_PI_2 - 1e-9 && readable <= FRAC_PI_2 + 1e-9);
        }

        #[test]
        fn moving_a_dimension_moves_its_layout(
            dim in dimension(), style in style(), dx in -500.0f64..500.0, dy in -500.0f64..500.0,
        ) {
            let offset = Vec2::new(dx, dy);
            let layout = dim.layout(&style);
            let moved = dim.transformed(&Transform::translation(offset)).layout(&style);
            prop_assert_eq!(moved.lines.len(), layout.lines.len());
            for (before, after) in layout.lines.iter().zip(&moved.lines) {
                prop_assert!(near(before.start + offset, after.start) && near(before.end + offset, after.end));
            }
            for (before, after) in layout.arrowheads.iter().zip(&moved.arrowheads) {
                prop_assert!(near(before.tip + offset, after.tip));
            }
            prop_assert!(near(layout.text.insertion + offset, moved.text.insertion));
            prop_assert_eq!(layout.text.content, moved.text.content);
            prop_assert!((layout.value - moved.value).abs() < 1e-6 * (1.0 + layout.value.abs()));
        }
    }

    /// A style's linear factor shows a length through it, an angle not
    /// (#352): a 2:1 detail of a 1:3 sheet is drawn six times true, and its
    /// 10 mm feature, 60 units long, reads 10.
    #[test]
    fn the_linear_factor_reads_a_length_true_and_leaves_an_angle() {
        let mut style = standard();
        style.linear_factor = 1.0 / 6.0;
        let feature = horizontal(
            Point2::new(0.0, 0.0),
            Point2::new(60.0, 0.0),
            Point2::new(30.0, -10.0),
        );
        let text = feature.layout(&style).text.content;
        assert!(text.starts_with("10.00"), "{text}");
        let angle = DimensionEntity::new(
            Measure::Angular {
                vertex: Point2::new(0.0, 0.0),
                a: Point2::new(10.0, 0.0),
                b: Point2::new(0.0, 10.0),
            },
            Point2::new(7.0, 7.0),
        );
        let text = angle.layout(&style).text.content;
        assert!(text.starts_with("90"), "{text}");
        style.linear_factor = 0.0;
        assert!(style.check().is_err());
    }
}
