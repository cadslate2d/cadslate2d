//! Inquiry (#197): `DIST`, `ID`, `AREA`, and `LIST` answer a question in
//! the log and change nothing — none commits a transaction, so the
//! revision does not move and `U` after them undoes the edit before.
//!
//! Lengths carry the drawing's unit suffix when it has one; an area is in
//! that unit squared. Numbers are four decimals, like the status bar's
//! readout and the Properties panel.

use std::f64::consts::TAU;

use cad_core::{EntityId, Point2};
use cad_document::{Entity, LayoutId};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::modify::{NOTHING_SELECTED, SELECT_OBJECTS, selection_expected};
use crate::summary::{degrees, geometry_of, kind_index, kind_names, number};
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

/// A length with the drawing's unit after it, when it has one.
fn length(ctx: &CommandContext<'_>, value: f64) -> String {
    match ctx.drawing.unit() {
        Some(unit) => format!("{} {}", number(value), unit.suffix()),
        None => number(value),
    }
}

/// An area in the drawing's unit squared, when it has one.
fn area(ctx: &CommandContext<'_>, value: f64) -> String {
    match ctx.drawing.unit() {
        Some(unit) => format!("{} {}²", number(value), unit.suffix()),
        None => number(value),
    }
}

/// `DIST` (`DI`): two points; the distance between them, the delta in X
/// and Y, and the angle from the first to the second.
#[derive(Debug, Default)]
pub(crate) struct DistCommand {
    first: Option<Point2>,
}

impl DistCommand {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

impl Command for DistCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new(if self.first.is_none() {
            "Specify first point"
        } else {
            "Specify second point"
        })
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::point()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (self.first, input) {
            (None, CommandInput::Point(first)) => {
                self.first = Some(first);
                Step::Continue
            }
            (Some(first), CommandInput::Point(second)) => {
                let delta = second - first;
                ctx.message(format!(
                    "Distance = {}, Delta X = {}, Delta Y = {}, Angle = {}",
                    length(ctx, delta.length()),
                    length(ctx, delta.x),
                    length(ctx, delta.y),
                    degrees(delta.y.atan2(delta.x).rem_euclid(TAU)),
                ));
                Step::Finished
            }
            (_, CommandInput::Accept) => Step::Finished,
            _ => Step::Continue,
        }
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let Some(first) = self.first {
            out.push(OverlayItem::PreviewSegment {
                start: first,
                end: cursor,
            });
        }
    }

    fn base_point(&self) -> Option<Point2> {
        self.first
    }
}

/// `ID`: a point, reported.
#[derive(Debug, Default)]
pub(crate) struct IdCommand;

impl Command for IdCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new("Specify point")
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::point()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Point(at) => {
                ctx.message(format!(
                    "X = {}, Y = {}",
                    length(ctx, at.x),
                    length(ctx, at.y)
                ));
                Step::Finished
            }
            CommandInput::Accept => Step::Finished,
            _ => Step::Continue,
        }
    }
}

const OBJECT: Keyword = Keyword::new("Object");

#[derive(Debug)]
enum AreaState {
    /// The first corner, or `Object`.
    First,
    /// The corners so far; Enter closes the polygon.
    Corners(Vec<Point2>),
    /// One object to measure.
    Object,
}

/// `AREA` (`AA`): the area and perimeter of a polygon given corner by
/// corner, closed by Enter, or of one closed object.
#[derive(Debug)]
pub(crate) struct AreaCommand {
    state: AreaState,
}

impl AreaCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: AreaState::First,
        }
    }

    /// The area and perimeter of a closed object, or why it has none.
    fn measure(entity: &Entity) -> Result<(f64, f64), &'static str> {
        const NOT_CLOSED: &str = "That object is not closed.";
        match entity {
            Entity::Circle(circle) => Ok((
                std::f64::consts::PI * circle.radius * circle.radius,
                TAU * circle.radius,
            )),
            Entity::Polyline(polyline) if polyline.closed => {
                let path = polyline.path();
                Ok((path.area().abs(), path.length()))
            }
            Entity::Polyline(_) => Err(NOT_CLOSED),
            Entity::Ellipse(entity) if entity.is_full() => {
                // The perimeter as the ellipse is read: its chords.
                let ellipse = entity.ellipse();
                let chords = ellipse.chord_points();
                let perimeter: f64 = chords
                    .windows(2)
                    .map(|pair| pair[0].distance_to(pair[1]))
                    .sum();
                Ok((ellipse.area(), perimeter))
            }
            Entity::Ellipse(_) => Err(NOT_CLOSED),
            Entity::Hatch(hatch) => {
                let perimeter: f64 = hatch.loops.iter().map(|path| path.length()).sum();
                Ok((hatch.area(), perimeter))
            }
            _ => Err("An area needs a closed polyline, a circle, an ellipse, or a hatch."),
        }
    }

    fn report(ctx: &mut CommandContext<'_>, area_value: f64, perimeter: f64) {
        ctx.message(format!(
            "Area = {}, Perimeter = {}",
            area(ctx, area_value),
            length(ctx, perimeter)
        ));
    }
}

impl Command for AreaCommand {
    fn prompt(&self) -> Prompt {
        match &self.state {
            AreaState::First => Prompt::new("Specify first corner point"),
            AreaState::Corners(_) => Prompt::new("Specify next point"),
            AreaState::Object => Prompt::new("Select object"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            AreaState::First => ExpectedInput::point().with_keywords(&[OBJECT]),
            AreaState::Corners(_) => ExpectedInput::point(),
            AreaState::Object => ExpectedInput::new(InputKind::Object),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&mut self.state, input) {
            (AreaState::First, CommandInput::Keyword(_)) => self.state = AreaState::Object,
            (AreaState::First, CommandInput::Point(first)) => {
                self.state = AreaState::Corners(vec![first]);
            }
            (AreaState::First, CommandInput::Accept) => return Step::Finished,
            (AreaState::Corners(corners), CommandInput::Point(next)) => corners.push(next),
            (AreaState::Corners(corners), CommandInput::Accept) => {
                if corners.len() < 3 {
                    ctx.message("An area needs at least three points.");
                    return Step::Finished;
                }
                let path = cad_geometry::Path {
                    vertices: corners.clone(),
                    bulges: Vec::new(),
                    closed: true,
                };
                Self::report(ctx, path.area().abs(), path.length());
                return Step::Finished;
            }
            (AreaState::Object, CommandInput::Pick { entity, .. }) => {
                let Some(entity) = ctx.drawing.entity(entity) else {
                    return Step::Continue;
                };
                match Self::measure(entity) {
                    Ok((area_value, perimeter)) => {
                        Self::report(ctx, area_value, perimeter);
                        return Step::Finished;
                    }
                    Err(why) => ctx.message(why),
                }
            }
            (AreaState::Object, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    /// The polygon so far, and its closing edge back to the first corner.
    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let AreaState::Corners(corners) = &self.state {
            let mut points = corners.clone();
            points.push(cursor);
            for pair in points.windows(2) {
                out.push(OverlayItem::PreviewSegment {
                    start: pair[0],
                    end: pair[1],
                });
            }
            if corners.len() >= 2 {
                out.push(OverlayItem::PreviewSegment {
                    start: cursor,
                    end: corners[0],
                });
            }
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            AreaState::Corners(corners) => corners.last().copied(),
            _ => None,
        }
    }

    /// The corners of a polygon are places on the screen — but an area is
    /// usually asked of something drawn, so object snap stays on.
    fn takes_object_snap(&self) -> bool {
        true
    }
}

/// `LIST` (`LI`): every selected object, its kind, properties, space, and
/// what defines it, as the Properties panel names them.
#[derive(Debug, Default)]
pub(crate) struct ListCommand;

impl ListCommand {
    fn list(ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            ctx.message(NOTHING_SELECTED);
            return Step::Finished;
        }
        let mut ids: Vec<EntityId> = ctx.selection.iter().collect();
        ids.sort_unstable();
        for id in ids {
            let Some(entity) = ctx.drawing.entity(id).cloned() else {
                continue;
            };
            let entity = &entity;
            let properties = ctx.drawing.properties(id).unwrap_or_default();
            let layer = ctx
                .drawing
                .layers()
                .get(properties.layer)
                .map_or_else(String::new, |layer| layer.name.clone());
            let space = match ctx.drawing.space_of(id) {
                Some(space) if space != LayoutId::MODEL => ctx
                    .drawing
                    .layouts()
                    .get(space)
                    .map_or_else(String::new, |layout| format!("layout \"{}\"", layout.name)),
                _ => "model space".to_owned(),
            };
            let kind = kind_names()[kind_index(entity)].to_uppercase();
            let headline = format!(
                "{kind}  Layer: \"{layer}\"  Colour: {}  Linetype: {}  Lineweight: {}  Space: {space}",
                properties.color.describe(),
                properties.linetype.describe(ctx.drawing.linetypes()),
                properties.lineweight.describe(),
            );
            let rows = geometry_of(entity, ctx.drawing);
            ctx.message(headline);
            for (label, value) in rows {
                ctx.message(format!("  {label}: {value}"));
            }
        }
        // Listed, the selection is spent, as an edited one is.
        ctx.selection.clear();
        Step::Finished
    }
}

impl Command for ListCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            Self::list(ctx)
        }
    }

    fn prompt(&self) -> Prompt {
        Prompt::new(SELECT_OBJECTS)
    }

    fn expected(&self) -> ExpectedInput {
        selection_expected()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Accept => Self::list(ctx),
            _ => Step::Continue,
        }
    }
}
