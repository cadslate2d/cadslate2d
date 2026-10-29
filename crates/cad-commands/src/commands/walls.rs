//! WALLS (#358): walls drawn with their thickness, one object. By hand, a
//! thickness and a chain of centreline points — `Close` back to the first;
//! or whole as JSON (`Json`): the walls, each its centreline and
//! thickness, the openings in them — doors, windows, plain gaps — and the
//! layers the doors and windows go on. The faces are drawn where wall meets
//! no wall, the openings cut with their jambs, on the current layer; a
//! door's leaf and swing and a window's glass on their layers; all of it in
//! a group — `WALLS-1`, the first number free — holding what it was drawn
//! from, as one undo (`cad_geometry::walls`).

use cad_core::Point2;
use cad_document::{ArcEntity, Group, Layer, LineEntity, Properties};
use cad_geometry::walls::{self, Kind, Opening, Wall, WallsError};
use cad_input::{ExpectedInput, Keyword};
use cad_intent::{Hinge, OpeningKind, Swing, WallLine, WallsSpec};

use super::ltscale::display;
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const JSON: Keyword = Keyword::new("Json");
const CLOSE: Keyword = Keyword::new("Close");
const UNDO: Keyword = Keyword::new("Undo");

/// The prefix of the group walls are kept in: `WALLS-1`, `WALLS-2`, …
pub const WALLS_GROUP: &str = "WALLS-";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Thickness,
    Json,
    /// The walls given; where their origin goes.
    Base,
    First,
    Next,
}

#[derive(Debug)]
pub(crate) struct WallsCommand {
    state: State,
    thickness: Option<f64>,
    points: Vec<Point2>,
    spec: Option<WallsSpec>,
    /// Drawing units to the metre.
    scale: f64,
}

impl WallsCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Thickness,
            thickness: None,
            points: Vec::new(),
            spec: None,
            scale: 1.0,
        }
    }

    /// 200 mm in the drawing's units.
    fn standard(&self) -> f64 {
        0.2 * self.scale
    }

    fn chain(&self, closed: bool) -> WallsSpec {
        let t = self.thickness.unwrap_or_else(|| self.standard());
        let mut walls: Vec<WallLine> = self
            .points
            .windows(2)
            .map(|pair| WallLine {
                from: [pair[0].x, pair[0].y],
                to: [pair[1].x, pair[1].y],
                thickness: t,
            })
            .collect();
        if closed && self.points.len() > 2 {
            let (first, last) = (self.points[0], self.points[self.points.len() - 1]);
            walls.push(WallLine {
                from: [last.x, last.y],
                to: [first.x, first.y],
                thickness: t,
            });
        }
        WallsSpec {
            units: None,
            walls,
            openings: Vec::new(),
            door_layer: None,
            window_layer: None,
        }
    }
}

/// Why walls were refused, in the user's words.
fn explain(error: WallsError) -> String {
    match error {
        WallsError::NoLength(i) => format!("Wall {i} has no length."),
        WallsError::BadThickness(i) => format!("Wall {i}'s thickness must be greater than zero."),
        WallsError::NoWall(i) => format!("Opening {i} names no wall."),
        WallsError::OffTheWall(i) => {
            format!(
                "Opening {i} is not wholly on its wall: its middle `at` along the wall from its start, and half its width either side, must lie on it."
            )
        }
        WallsError::Overlap(i) => format!("Opening {i} overlaps another on its wall."),
    }
}

/// Draws `spec`, its origin at `base`, as one transaction, or says why not.
fn draw(spec: &WallsSpec, base: Point2, ctx: &mut CommandContext<'_>) -> Result<String, String> {
    // The spec's lengths in the drawing's units.
    let factor = match (spec.units, ctx.drawing.unit()) {
        (Some(from), Some(to)) => from.length_unit().metres() / to.metres(),
        _ => 1.0,
    };
    let point = |p: [f64; 2]| base + cad_core::Vec2::new(p[0] * factor, p[1] * factor);
    let walls: Vec<Wall> = spec
        .walls
        .iter()
        .map(|w| Wall {
            from: point(w.from),
            to: point(w.to),
            thickness: w.thickness * factor,
        })
        .collect();
    let openings: Vec<Opening> = spec
        .openings
        .iter()
        .map(|o| Opening {
            wall: o.wall,
            at: o.at * factor,
            width: o.width * factor,
            kind: match o.kind {
                OpeningKind::Door => Kind::Door,
                OpeningKind::Window => Kind::Window,
                OpeningKind::Opening => Kind::Opening,
            },
            left: o.swing == Swing::Left,
            hinge_at_start: o.hinge == Hinge::Start,
        })
        .collect();
    let drawn = walls::walls(&walls, &openings).map_err(explain)?;
    let taken: Vec<String> = ctx
        .drawing
        .groups()
        .iter()
        .map(|(_, g)| g.name.clone())
        .collect();
    let found = |name: &Option<String>| {
        name.as_ref()
            .map(|n| (n.clone(), ctx.drawing.layers().find(n)))
    };
    let (doors, windows) = (found(&spec.door_layer), found(&spec.window_layer));
    let current = ctx.drawing.current_layer();
    // What they were drawn from: a revision replaces them whole.
    let description = serde_json::to_string(spec).expect("walls are plain data");
    let mut transaction = ctx.transaction();
    let mut layer_of = |wanted: Option<(String, Option<cad_document::LayerId>)>| -> Result<cad_document::LayerId, String> {
        match wanted {
            None => Ok(current),
            Some((_, Some(id))) => Ok(id),
            Some((name, None)) => transaction.add_layer(Layer::named(name)).map_err(|e| e.to_string()),
        }
    };
    let (door_layer, window_layer) = (layer_of(doors)?, layer_of(windows)?);
    let on = |layer| Properties {
        layer,
        ..Properties::default()
    };
    let mut made = Vec::new();
    for s in &drawn.faces {
        made.push(
            transaction
                .insert_with(LineEntity::new(s.start, s.end), on(current))
                .map_err(|e| e.to_string())?,
        );
    }
    for s in &drawn.leaves {
        made.push(
            transaction
                .insert_with(LineEntity::new(s.start, s.end), on(door_layer))
                .map_err(|e| e.to_string())?,
        );
    }
    for arc in &drawn.swings {
        made.push(
            transaction
                .insert_with(
                    ArcEntity::new(arc.center, arc.radius, arc.start, arc.end),
                    on(door_layer),
                )
                .map_err(|e| e.to_string())?,
        );
    }
    for s in &drawn.glass {
        made.push(
            transaction
                .insert_with(LineEntity::new(s.start, s.end), on(window_layer))
                .map_err(|e| e.to_string())?,
        );
    }
    let name = (1..)
        .map(|n| format!("{WALLS_GROUP}{n}"))
        .find(|name| !taken.contains(name))
        .expect("a number is free");
    let mut group = Group::named(name.clone(), made);
    group.description = description;
    transaction.add_group(group).map_err(|e| e.to_string())?;
    transaction.commit();
    Ok(format!(
        "{} walls drawn as {name}: {} faces, {} doors, {} windows.",
        walls.len(),
        drawn.faces.len(),
        drawn.swings.len(),
        drawn.glass.len() / 2
    ))
}

impl Command for WallsCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        // A unitless drawing is taken in millimetres.
        self.scale = ctx
            .drawing
            .unit()
            .map_or(1000.0, |unit| 1.0 / unit.metres());
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Thickness => Prompt::new("Specify wall thickness")
                .with_default(display(self.thickness.unwrap_or_else(|| self.standard()))),
            State::Json => Prompt::new("Enter the walls as JSON"),
            State::Base => Prompt::new("Specify base point").with_default("0,0"),
            State::First => Prompt::new("Specify start point"),
            State::Next => Prompt::new("Specify next point"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Thickness => ExpectedInput::distance().with_keywords(&[JSON]),
            State::Json => ExpectedInput::text(),
            State::Base => ExpectedInput::point(),
            State::First => ExpectedInput::point(),
            State::Next if self.points.len() > 2 => {
                ExpectedInput::point().with_keywords(&[CLOSE, UNDO])
            }
            State::Next => ExpectedInput::point().with_keywords(&[UNDO]),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        self.state = match (self.state, input) {
            (State::Thickness, CommandInput::Keyword(k)) if k == JSON => State::Json,
            (State::Thickness, CommandInput::Accept) => State::First,
            (State::Thickness, CommandInput::Distance(t)) if t.is_finite() && t > 0.0 => {
                self.thickness = Some(t);
                State::First
            }
            (State::Thickness, CommandInput::Distance(_)) => {
                ctx.message("A wall's thickness must be greater than zero.");
                State::Thickness
            }
            (State::Json, CommandInput::Text(text)) => {
                match serde_json::from_str::<WallsSpec>(&text) {
                    Ok(spec) => {
                        self.spec = Some(spec);
                        State::Base
                    }
                    Err(error) => {
                        ctx.message(format!("Those are not walls: {error}."));
                        return Step::Finished;
                    }
                }
            }
            (State::Base, input @ (CommandInput::Point(_) | CommandInput::Accept)) => {
                let base = match input {
                    CommandInput::Point(p) => p,
                    _ => Point2::ORIGIN,
                };
                if let Some(spec) = self.spec.take() {
                    match draw(&spec, base, ctx) {
                        Ok(said) => ctx.message(said),
                        Err(why) => ctx.message(why),
                    }
                }
                return Step::Finished;
            }
            (State::First, CommandInput::Point(p)) => {
                self.points.push(p);
                State::Next
            }
            (State::Next, CommandInput::Point(p)) => {
                self.points.push(p);
                State::Next
            }
            (State::Next, CommandInput::Keyword(k)) if k == UNDO => {
                self.points.pop();
                if self.points.is_empty() {
                    State::First
                } else {
                    State::Next
                }
            }
            (State::Next, CommandInput::Keyword(k)) if k == CLOSE => {
                let spec = self.chain(true);
                match draw(&spec, Point2::ORIGIN, ctx) {
                    Ok(said) => ctx.message(said),
                    Err(why) => ctx.message(why),
                }
                return Step::Finished;
            }
            (State::Next, CommandInput::Accept) => {
                if self.points.len() > 1 {
                    let spec = self.chain(false);
                    match draw(&spec, Point2::ORIGIN, ctx) {
                        Ok(said) => ctx.message(said),
                        Err(why) => ctx.message(why),
                    }
                }
                return Step::Finished;
            }
            (state, _) => state,
        };
        Step::Continue
    }
}
