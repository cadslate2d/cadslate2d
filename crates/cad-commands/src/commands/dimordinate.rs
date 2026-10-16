//! DIMORDINATE and QDIM (#227): dimensions from a datum, and a row of
//! dimensions placed at once.
//!
//! `DIMORDINATE` (`DOR`) takes a feature and where its leader ends: the X
//! of the feature from the datum when the leader is dragged up or down,
//! its Y when across, or as `Xdatum` or `Ydatum` say. The datum is the
//! origin until `Datum` moves it, for the session. `QDIM` takes a
//! selection, finds the ends and centres of its lines, arcs, circles,
//! polylines, and points, and dimensions them along the line placed —
//! `Continuous`, `Baseline` from the first or from a `Datum point`, or
//! `Ordinate` from the datum, which `Datum point` moves too —
//! horizontally when placed above or below them, vertically when beside;
//! one transaction, one undo.

use std::f64::consts::FRAC_PI_2;

use cad_core::tolerance::LinearTolerance;
use cad_core::{Aabb, Point2};
use cad_document::{DimStyleId, DimensionEntity, Drawing, Entity, Measure};
use cad_input::{ExpectedInput, Keyword};

use super::dimtext::{ANGLE, TEXT, TextOptions, commit};
use super::modify::{NOTHING_SELECTED, counted, selection_expected};
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const DATUM: Keyword = Keyword::new("Datum");
const X_DATUM: Keyword = Keyword::new("Xdatum");
const Y_DATUM: Keyword = Keyword::new("Ydatum");
const LEADER_OPTIONS: [Keyword; 4] = [X_DATUM, Y_DATUM, TEXT, ANGLE];

/// Whether an ordinate's leader dragged from `feature` to `end` measures
/// the X: dragged further up or down than across.
fn measures_x(feature: Point2, end: Point2) -> bool {
    let drag = end - feature;
    drag.y.abs() >= drag.x.abs()
}

#[derive(Debug)]
enum OrdinateState {
    Feature,
    Datum,
    Leader(Point2),
}

#[derive(Debug)]
pub(crate) struct DimOrdinateCommand {
    state: OrdinateState,
    /// The axis forced by `Xdatum` (true) or `Ydatum`, if either.
    axis: Option<bool>,
    options: TextOptions,
    style: DimStyleId,
    datum: Point2,
}

impl DimOrdinateCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: OrdinateState::Feature,
            axis: None,
            options: TextOptions::new(),
            style: DimStyleId::FIRST,
            datum: Point2::ORIGIN,
        }
    }

    /// The dimension a leader ending at `end` makes: what the preview
    /// shows and the click commits.
    fn dimension(&self, feature: Point2, end: Point2) -> DimensionEntity {
        let measure = Measure::Ordinate {
            origin: self.datum,
            feature,
            x: self.axis.unwrap_or_else(|| measures_x(feature, end)),
        };
        self.options
            .apply(DimensionEntity::new(measure, end).in_style(self.style))
    }
}

impl Command for DimOrdinateCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.style = ctx.drawing.current_dim_style();
        self.datum = ctx.settings.ordinate_datum;
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        if let Some(prompt) = self.options.prompt() {
            return prompt;
        }
        match self.state {
            OrdinateState::Feature => Prompt::new("Specify feature location"),
            OrdinateState::Datum => Prompt::new("Specify datum point")
                .with_default(format!("{},{}", self.datum.x, self.datum.y)),
            OrdinateState::Leader(_) => Prompt::new("Specify leader endpoint"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        if let Some(expected) = self.options.expected() {
            return expected;
        }
        match self.state {
            OrdinateState::Feature => ExpectedInput::point().with_keywords(&[DATUM]),
            OrdinateState::Datum => ExpectedInput::point(),
            OrdinateState::Leader(_) => ExpectedInput::point().with_keywords(&LEADER_OPTIONS),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        if self.options.step(&input) {
            return Step::Continue;
        }
        match (&self.state, input) {
            (OrdinateState::Feature, CommandInput::Point(feature)) => {
                self.state = OrdinateState::Leader(feature);
            }
            (OrdinateState::Feature, CommandInput::Keyword(_)) => {
                self.state = OrdinateState::Datum;
            }
            (OrdinateState::Datum, CommandInput::Point(datum)) => {
                self.datum = datum;
                ctx.settings.ordinate_datum = datum;
                ctx.message(format!(
                    "Ordinates are measured from {},{}.",
                    datum.x, datum.y
                ));
                self.state = OrdinateState::Feature;
            }
            (OrdinateState::Datum, CommandInput::Accept) => self.state = OrdinateState::Feature,
            (OrdinateState::Leader(feature), CommandInput::Point(end)) => {
                return commit(self.dimension(*feature, end), ctx);
            }
            (OrdinateState::Leader(feature), CommandInput::Keyword(keyword)) => {
                let feature = *feature;
                match keyword {
                    X_DATUM => self.axis = Some(true),
                    Y_DATUM => self.axis = Some(false),
                    other => {
                        let measured =
                            TextOptions::measured(&self.dimension(feature, feature), ctx);
                        self.options.open(other, || measured);
                    }
                }
            }
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let OrdinateState::Leader(feature) = self.state {
            let dimension = self.dimension(feature, cursor);
            if dimension.check().is_ok() {
                out.push(OverlayItem::PreviewEntity(dimension.into()));
            }
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match self.state {
            OrdinateState::Leader(feature) => Some(feature),
            _ => None,
        }
    }
}

const CONTINUOUS: Keyword = Keyword::new("Continuous");
const BASELINE: Keyword = Keyword::new("Baseline");
const ORDINATE: Keyword = Keyword::new("Ordinate");
const DATUM_POINT: Keyword = Keyword::new("Datum point");
const QDIM_OPTIONS: [Keyword; 4] = [CONTINUOUS, BASELINE, ORDINATE, DATUM_POINT];

/// How QDIM dimensions the points it found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Row {
    Continuous,
    Baseline,
    Ordinate,
}

/// The ends and centres QDIM dimensions on an object: a line's ends; an
/// arc's ends and centre; a circle's or an ellipse's centre; a
/// polyline's vertices and the centres of its arc segments; a point.
fn points_of(entity: &Entity) -> Vec<Point2> {
    match entity {
        Entity::Line(line) => vec![line.start, line.end],
        Entity::Arc(arc) => {
            let arc = arc.arc();
            vec![arc.start_point(), arc.end_point(), arc.center]
        }
        Entity::Circle(circle) => vec![circle.center],
        Entity::Ellipse(ellipse) => vec![ellipse.center],
        Entity::Polyline(polyline) => {
            let mut points = polyline.vertices.clone();
            points.extend(
                polyline
                    .primitives()
                    .filter_map(|primitive| match primitive {
                        cad_geometry::Primitive::Arc(arc) => Some(arc.center),
                        _ => None,
                    }),
            );
            points
        }
        Entity::Point(point) => vec![point.position],
        _ => Vec::new(),
    }
}

/// The dimensions QDIM makes of `points` placed at `place`: one per
/// coordinate along the axis they measure — of the points sharing one,
/// the nearest the dimension line — in order along it.
fn row(
    points: &[Point2],
    place: Point2,
    row: Row,
    datum: Option<Point2>,
    ordinate_datum: Point2,
    style: (DimStyleId, f64),
) -> Vec<DimensionEntity> {
    let Some(extent) = points
        .iter()
        .map(|point| Aabb::from_points(*point, *point))
        .reduce(|a, b| a.union(&b))
    else {
        return Vec::new();
    };
    let beyond = |low: f64, high: f64, at: f64| (low - at).max(at - high).max(0.0);
    let out_x = beyond(extent.min().x, extent.max().x, place.x);
    let out_y = beyond(extent.min().y, extent.max().y, place.y);
    // Placed above or below: horizontal, measuring along X.
    let horizontal = out_y >= out_x;
    let along = |p: Point2| if horizontal { p.x } else { p.y };
    let across = |p: Point2| if horizontal { p.y } else { p.x };
    let mut ordered: Vec<Point2> = points.to_vec();
    ordered.sort_by(|a, b| {
        along(*a).total_cmp(&along(*b)).then(
            (across(*a) - across(place))
                .abs()
                .total_cmp(&(across(*b) - across(place)).abs()),
        )
    });
    let tolerance = LinearTolerance::DEFAULT.around(&ordered);
    ordered.dedup_by(|later, kept| tolerance.eq(along(*later), along(*kept)));
    let (style_id, spacing) = style;
    let rotation = if horizontal { 0.0 } else { FRAC_PI_2 };
    let linear = |a: Point2, b: Point2, line_point: Point2| {
        DimensionEntity::new(Measure::Linear { a, b, rotation }, line_point).in_style(style_id)
    };
    match row {
        Row::Continuous => ordered
            .windows(2)
            .map(|pair| linear(pair[0], pair[1], place))
            .collect(),
        Row::Baseline => {
            // From the datum point, when one was given; else the first.
            let base = datum.unwrap_or(ordered[0]);
            // Each further out, away from what is measured.
            let outwards = if across(place) < (across(extent.min()) + across(extent.max())) / 2.0 {
                -1.0
            } else {
                1.0
            };
            ordered
                .iter()
                .filter(|point| !tolerance.eq(along(**point), along(base)))
                .enumerate()
                .map(|(n, point)| {
                    let step = outwards * spacing * n as f64;
                    let line_point = if horizontal {
                        Point2::new(place.x, place.y + step)
                    } else {
                        Point2::new(place.x + step, place.y)
                    };
                    linear(base, *point, line_point)
                })
                .collect()
        }
        Row::Ordinate => ordered
            .iter()
            .map(|feature| {
                let end = if horizontal {
                    Point2::new(feature.x, place.y)
                } else {
                    Point2::new(place.x, feature.y)
                };
                DimensionEntity::new(
                    Measure::Ordinate {
                        origin: datum.unwrap_or(ordinate_datum),
                        feature: *feature,
                        x: horizontal,
                    },
                    end,
                )
                .in_style(style_id)
            })
            .collect(),
    }
}

#[derive(Debug)]
enum QdimState {
    Select,
    Place,
    Datum,
}

#[derive(Debug)]
pub(crate) struct QdimCommand {
    state: QdimState,
    row: Row,
    points: Vec<Point2>,
    datum: Option<Point2>,
    ordinate_datum: Point2,
    style: (DimStyleId, f64),
}

impl QdimCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: QdimState::Select,
            row: Row::Continuous,
            points: Vec::new(),
            datum: None,
            ordinate_datum: Point2::ORIGIN,
            style: (DimStyleId::FIRST, 0.0),
        }
    }

    /// The selection's ends and centres, or the message that it has none.
    fn gather(&mut self, ctx: &mut CommandContext<'_>) -> bool {
        let drawing: &Drawing = ctx.drawing;
        self.points = ctx
            .selection
            .iter()
            .filter_map(|id| drawing.entity(id))
            .flat_map(points_of)
            .collect();
        ctx.selection.clear();
        if self.points.is_empty() {
            ctx.message("Nothing selected has ends or centres to dimension.");
            return false;
        }
        true
    }

    fn row_at(&self, place: Point2) -> Vec<DimensionEntity> {
        row(
            &self.points,
            place,
            self.row,
            self.datum,
            self.ordinate_datum,
            self.style,
        )
    }
}

impl Command for QdimCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        let style = ctx.drawing.current_dim_style();
        self.style = (
            style,
            ctx.drawing.tables().dim_style(style).baseline_spacing,
        );
        self.ordinate_datum = ctx.settings.ordinate_datum;
        if ctx.selection.is_empty() {
            return Step::Continue;
        }
        if self.gather(ctx) {
            self.state = QdimState::Place;
            Step::Continue
        } else {
            Step::Finished
        }
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            QdimState::Select => Prompt::new("Select geometry to dimension"),
            QdimState::Place => {
                Prompt::new("Specify dimension line position").with_default(match self.row {
                    Row::Continuous => "Continuous",
                    Row::Baseline => "Baseline",
                    Row::Ordinate => "Ordinate",
                })
            }
            QdimState::Datum => Prompt::new("Select new datum point"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            QdimState::Select => selection_expected(),
            QdimState::Place => ExpectedInput::point().with_keywords(&QDIM_OPTIONS),
            QdimState::Datum => ExpectedInput::point(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (QdimState::Select, CommandInput::Accept) => {
                if ctx.selection.is_empty() {
                    ctx.message(NOTHING_SELECTED);
                    return Step::Finished;
                }
                if !self.gather(ctx) {
                    return Step::Finished;
                }
                self.state = QdimState::Place;
            }
            (QdimState::Place, CommandInput::Point(place)) => {
                let dimensions = self.row_at(place);
                if dimensions.is_empty() {
                    ctx.message("There are not points enough apart to dimension.");
                    return Step::Continue;
                }
                let count = dimensions.len();
                let mut transaction = ctx.transaction();
                for dimension in dimensions {
                    if dimension.check().is_ok() {
                        transaction
                            .insert(dimension)
                            .expect("a dimension in the current style");
                    }
                }
                transaction.commit();
                ctx.message(format!("{} made.", counted(count, "dimension")));
                return Step::Finished;
            }
            (QdimState::Place, CommandInput::Keyword(DATUM_POINT)) => {
                self.state = QdimState::Datum;
            }
            (QdimState::Place, CommandInput::Keyword(keyword)) => {
                self.row = match keyword {
                    BASELINE => Row::Baseline,
                    ORDINATE => Row::Ordinate,
                    _ => Row::Continuous,
                };
            }
            // The datum of a baseline, and of ordinates from now on.
            (QdimState::Datum, CommandInput::Point(datum)) => {
                self.datum = Some(datum);
                ctx.settings.ordinate_datum = datum;
                self.state = QdimState::Place;
            }
            (QdimState::Datum, CommandInput::Accept) => self.state = QdimState::Place,
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if !matches!(self.state, QdimState::Place) {
            return;
        }
        for dimension in self.row_at(cursor) {
            if dimension.check().is_ok() {
                out.push(OverlayItem::PreviewEntity(dimension.into()));
            }
        }
    }
}
