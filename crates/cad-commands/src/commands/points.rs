//! Points (#198): `POINT` places them, `DIVIDE` and `MEASURE` put them
//! along a curve at equal arc length, and `PTYPE` sets the drawing's
//! point style — one setting, every point redrawn by it.
//!
//! A curve is read as a `Path`: a polyline's own, with its arcs; a line,
//! an arc, or a circle as one; an ellipse or a spline as its chords. The
//! points land on the curve by `Path::point_along`, through the arcs by
//! arc length, and are placed with `insert_like` so they take the
//! curve's layer and properties.

use cad_core::{EntityId, Point2};
use cad_document::{Entity, Path, PointEntity, PointStyle};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::ltscale::display;
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

/// The curve an entity is, run from its start; `None` for one that has
/// no length to measure along.
fn path_of(entity: &Entity) -> Option<Path> {
    let straight = |vertices: Vec<Point2>| Path {
        vertices,
        bulges: Vec::new(),
        closed: false,
    };
    match entity {
        Entity::Line(line) => Some(straight(vec![line.start, line.end])),
        Entity::Polyline(polyline) => Some(polyline.path()),
        Entity::Arc(arc) => {
            let geometry = arc.arc();
            Some(Path {
                vertices: vec![geometry.start_point(), geometry.end_point()],
                bulges: vec![(geometry.sweep / 4.0).tan(), 0.0],
                closed: false,
            })
        }
        // Two half-turns from the circle's rightmost point, as the
        // quadrants count.
        Entity::Circle(circle) => Some(Path {
            vertices: vec![
                circle.center + cad_core::Vec2::new(circle.radius, 0.0),
                circle.center - cad_core::Vec2::new(circle.radius, 0.0),
            ],
            bulges: vec![1.0, 1.0],
            closed: true,
        }),
        Entity::Ellipse(ellipse) => Some(straight(ellipse.ellipse().chord_points())),
        Entity::Spline(spline) => {
            let segments = spline.spline().segments();
            let mut vertices: Vec<Point2> = segments.first().map(|s| s.start).into_iter().collect();
            vertices.extend(segments.iter().map(|s| s.end));
            (vertices.len() >= 2).then(|| straight(vertices))
        }
        _ => None,
    }
}

/// Places `points` as entities like `source`, in one transaction.
fn place_like(ctx: &mut CommandContext<'_>, source: EntityId, points: &[Point2]) {
    let mut tx = ctx.transaction();
    for point in points {
        tx.insert_like(source, PointEntity::new(*point))
            .expect("the source is live and was just picked");
    }
    tx.commit();
}

fn placed(count: usize) -> String {
    if count == 1 {
        "1 point placed".to_owned()
    } else {
        format!("{count} points placed")
    }
}

/// `POINT` (`PO`): a point at each place given, until Enter.
#[derive(Debug, Default)]
pub(crate) struct PointCommand;

impl Command for PointCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new("Specify a point")
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::point()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Point(at) => {
                let mut tx = ctx.transaction();
                tx.insert(PointEntity::new(at))
                    .expect("the current layer and linetype exist");
                tx.commit();
                Step::Continue
            }
            CommandInput::Accept => Step::Finished,
            _ => Step::Continue,
        }
    }
}

#[derive(Debug)]
enum AlongState {
    Pick,
    /// The curve picked, run from the end nearest the pick, and its ID.
    Amount {
        source: EntityId,
        path: Path,
    },
}

/// `DIVIDE` (`DIV`): a curve into a number of segments of equal length,
/// a point at each division.
#[derive(Debug)]
pub(crate) struct DivideCommand {
    state: AlongState,
}

impl DivideCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: AlongState::Pick,
        }
    }

    /// The divisions of `path` into `count` segments: the points between
    /// them, and, on a closed curve, the start too.
    fn divisions(path: &Path, count: usize) -> Vec<Point2> {
        let length = path.length();
        let first = if path.closed { 0 } else { 1 };
        (first..count)
            .filter_map(|k| path.point_along(length * k as f64 / count as f64))
            .collect()
    }
}

/// The curve under a pick, run from the end nearest the pick: what
/// `MEASURE` steps along from, and what `DIVIDE` counts along.
fn picked_curve(ctx: &mut CommandContext<'_>, entity: EntityId, at: Point2) -> Option<Path> {
    let Some(path) = ctx.drawing.entity(entity).and_then(path_of) else {
        ctx.message("That object cannot be divided or measured: pick a line, arc, circle, ellipse, spline, or polyline.");
        return None;
    };
    if path.closed {
        return Some(path);
    }
    let (first, last) = (path.vertices[0], path.vertices[path.vertices.len() - 1]);
    Some(if at.distance_to(last) < at.distance_to(first) {
        path.reversed()
    } else {
        path
    })
}

impl Command for DivideCommand {
    fn prompt(&self) -> Prompt {
        match self.state {
            AlongState::Pick => Prompt::new("Select object to divide"),
            AlongState::Amount { .. } => Prompt::new("Enter the number of segments"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            AlongState::Pick => ExpectedInput::new(InputKind::Object),
            AlongState::Amount { .. } => ExpectedInput::factor(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (AlongState::Pick, CommandInput::Pick { entity, at }) => {
                if let Some(path) = picked_curve(ctx, entity, at) {
                    self.state = AlongState::Amount {
                        source: entity,
                        path,
                    };
                }
                Step::Continue
            }
            (AlongState::Amount { source, path }, CommandInput::Distance(count)) => {
                let whole = count.round();
                if !(count.is_finite()
                    && (count - whole).abs() < 1e-9
                    && (2.0..=32767.0).contains(&whole))
                {
                    ctx.message("Enter a whole number from 2 to 32767.");
                    return Step::Continue;
                }
                let count = whole as usize;
                let points = Self::divisions(path, count);
                place_like(ctx, *source, &points);
                ctx.message(format!(
                    "Divided into {count} segments: {}.",
                    placed(points.len())
                ));
                Step::Finished
            }
            (_, CommandInput::Accept) => Step::Finished,
            _ => Step::Continue,
        }
    }
}

/// `MEASURE` (`ME`): points every so far along a curve, from the end
/// nearest the pick.
#[derive(Debug)]
pub(crate) struct MeasureCommand {
    state: AlongState,
}

impl MeasureCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: AlongState::Pick,
        }
    }

    /// Every multiple of `step` along `path`, up to its length.
    fn marks(path: &Path, step: f64) -> Vec<Point2> {
        let length = path.length();
        let count = (length / step).floor() as usize;
        (1..=count)
            .filter_map(|k| path.point_along(step * k as f64))
            .collect()
    }
}

impl Command for MeasureCommand {
    fn prompt(&self) -> Prompt {
        match self.state {
            AlongState::Pick => Prompt::new("Select object to measure"),
            AlongState::Amount { .. } => Prompt::new("Specify length of segment"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            AlongState::Pick => ExpectedInput::new(InputKind::Object),
            AlongState::Amount { .. } => ExpectedInput::distance(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (AlongState::Pick, CommandInput::Pick { entity, at }) => {
                if let Some(path) = picked_curve(ctx, entity, at) {
                    self.state = AlongState::Amount {
                        source: entity,
                        path,
                    };
                }
                Step::Continue
            }
            (AlongState::Amount { source, path }, CommandInput::Distance(step)) => {
                if !(step.is_finite() && step > 0.0) {
                    ctx.message("The segment length must be a positive number.");
                    return Step::Continue;
                }
                let points = Self::marks(path, step);
                if points.is_empty() {
                    ctx.message("The segment is longer than the object.");
                    return Step::Finished;
                }
                place_like(ctx, *source, &points);
                ctx.message(format!(
                    "Measured every {}: {}.",
                    display(step),
                    placed(points.len())
                ));
                Step::Finished
            }
            (_, CommandInput::Accept) => Step::Finished,
            _ => Step::Continue,
        }
    }
}

const DOT: Keyword = Keyword::new("Dot");
const CROSS: Keyword = Keyword::new("Cross");
const X: Keyword = Keyword::new("X");
const TICK: Keyword = Keyword::new("Tick");
const BLANK: Keyword = Keyword::new("Blank");
const CIRCLE: Keyword = Keyword::new("CIrcle");
const SQUARE: Keyword = Keyword::new("SQuare");
const PLAIN: Keyword = Keyword::new("Plain");
const MODES: [Keyword; 8] = [DOT, CROSS, X, TICK, BLANK, CIRCLE, SQUARE, PLAIN];

#[derive(Debug)]
enum PtypeState {
    Mode,
    Size,
}

/// `PTYPE` (`DDPTYPE`): the shape every point is drawn as — a dot, a
/// cross, an X, a tick, or nothing, with or without a circle or a square
/// round it, or the `$PDMODE` number itself — and its size in drawing
/// units. One undoable change.
#[derive(Debug)]
pub(crate) struct PtypeCommand {
    state: PtypeState,
    style: PointStyle,
}

impl PtypeCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: PtypeState::Mode,
            style: PointStyle::default(),
        }
    }

    /// The mode in words: `X in a circle`, `cross`, `dot`.
    pub(crate) fn describe(style: PointStyle) -> String {
        let shape = match style.shape() {
            PointStyle::DOT => "dot",
            PointStyle::NONE => "nothing",
            PointStyle::CROSS => "cross",
            PointStyle::X => "X",
            _ => "tick",
        };
        match (style.has_circle(), style.has_square()) {
            (true, true) => format!("{shape} in a circle and a square"),
            (true, false) => format!("{shape} in a circle"),
            (false, true) => format!("{shape} in a square"),
            (false, false) => shape.to_owned(),
        }
    }

    fn commit(&self, ctx: &mut CommandContext<'_>) -> Step {
        let mut tx = ctx.transaction();
        if let Err(error) = tx.set_point_style(self.style) {
            drop(tx);
            ctx.message(format!("{error}."));
            return Step::Continue;
        }
        tx.commit();
        ctx.message(format!(
            "Point style: {} ({}), size {}.",
            Self::describe(self.style),
            self.style.mode,
            display(self.style.size)
        ));
        Step::Finished
    }
}

impl Command for PtypeCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.style = ctx.drawing.settings().point_style;
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            PtypeState::Mode => Prompt::new("Enter point mode, as a number")
                .with_default(self.style.mode.to_string()),
            PtypeState::Size => {
                Prompt::new("Enter point size").with_default(display(self.style.size))
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            PtypeState::Mode => ExpectedInput::text().with_keywords(&MODES),
            PtypeState::Size => ExpectedInput::factor(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (PtypeState::Mode, CommandInput::Keyword(keyword)) => {
                let rings = self.style.mode & (PointStyle::CIRCLE | PointStyle::SQUARE);
                self.style.mode = match keyword {
                    DOT => PointStyle::DOT | rings,
                    CROSS => PointStyle::CROSS | rings,
                    X => PointStyle::X | rings,
                    TICK => PointStyle::TICK | rings,
                    BLANK => PointStyle::NONE | rings,
                    CIRCLE => self.style.mode ^ PointStyle::CIRCLE,
                    SQUARE => self.style.mode ^ PointStyle::SQUARE,
                    _ => self.style.shape(),
                };
                ctx.message(format!(
                    "Point mode: {} ({}).",
                    Self::describe(self.style),
                    self.style.mode
                ));
                Step::Continue
            }
            (PtypeState::Mode, CommandInput::Text(text)) => {
                match text.trim().parse::<u8>().ok().map(|mode| PointStyle::new(mode, 1.0)) {
                    Some(style) if style.check().is_ok() => {
                        self.style.mode = style.mode;
                        self.state = PtypeState::Size;
                    }
                    _ => ctx.message(
                        "A point mode is 0 (dot), 1 (nothing), 2 (cross), 3 (X), or 4 (tick), with 32 for a circle or 64 for a square round it.",
                    ),
                }
                Step::Continue
            }
            (PtypeState::Mode, CommandInput::Accept) => {
                self.state = PtypeState::Size;
                Step::Continue
            }
            (PtypeState::Size, CommandInput::Distance(size)) => {
                self.style.size = size;
                self.commit(ctx)
            }
            (PtypeState::Size, CommandInput::Accept) => self.commit(ctx),
            _ => Step::Continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_curve_is_read_as_a_path_of_its_own_length() {
        use std::f64::consts::TAU;

        use cad_document::{ArcEntity, CircleEntity, LineEntity, TextEntity};
        let line: Entity = LineEntity::new(Point2::new(0.0, 0.0), Point2::new(3.0, 4.0)).into();
        assert!((path_of(&line).unwrap().length() - 5.0).abs() < 1e-9);
        let circle: Entity = CircleEntity::new(Point2::new(1.0, 1.0), 2.0).into();
        assert!((path_of(&circle).unwrap().length() - TAU * 2.0).abs() < 1e-9);
        let arc: Entity =
            ArcEntity::new(Point2::ORIGIN, 2.0, 0.0, std::f64::consts::FRAC_PI_2).into();
        let path = path_of(&arc).unwrap();
        assert!(
            (path.length() - TAU / 2.0).abs() < 1e-9,
            "{}",
            path.length()
        );
        assert!(
            path.point_along(TAU / 4.0)
                .unwrap()
                .distance_to(Point2::new(2.0_f64.sqrt(), 2.0_f64.sqrt()))
                < 1e-9
        );
        let text: Entity = TextEntity::new(Point2::ORIGIN, 1.0, 0.0, "no").into();
        assert!(path_of(&text).is_none());
    }
}
