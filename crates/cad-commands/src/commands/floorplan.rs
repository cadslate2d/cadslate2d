//! FLOORPLAN (#281, ADR-0039): a floor plan from a brief — the terrain,
//! the rooms and their areas, which rooms open onto each other — asked at
//! the command line, or given whole as JSON (`Brief`), or read from a spec
//! file (`File`, whose text the host types in as the brief). The search's
//! best plans are previewed in turn — `Next`, `Previous` — and the one
//! accepted is drawn as one undo: ordinary walls, doors, and labels on
//! their layers (`crate::floorplan`). A brief with no plan is refused with
//! the reasons: proved impossible, or not found. Asked for a width, every
//! room gets a window on each outside wall (#324).

use cad_core::Point2;
use cad_input::{ExpectedInput, Keyword};
use cad_layout::{Layout, Refusal, Room, Setbacks, Spec, Terrain, alternatives};

use super::ltscale::display;
use crate::floorplan::{draw, entities};
use crate::{Command, CommandContext, CommandInput, HostRequest, OverlayItem, Prompt, Step};

const BRIEF: Keyword = Keyword::new("Brief");
const FILE: Keyword = Keyword::new("File");
const NEXT: Keyword = Keyword::new("Next");
const PREVIOUS: Keyword = Keyword::new("Previous");
const ACCEPT: Keyword = Keyword::new("Accept");

/// How many plans are offered.
const ALTERNATIVES: usize = 3;

#[derive(Debug)]
enum State {
    Width,
    Depth,
    /// A room's name, or Enter when they are all named.
    RoomName,
    RoomArea(String),
    /// Two rooms that open onto each other, or Enter.
    Pair,
    /// The windows' width, or Enter for none.
    Windows,
    /// The brief whole, as JSON.
    Brief,
    /// Where the terrain's lower left corner goes.
    Corner,
    Choosing {
        plans: Vec<Layout>,
        index: usize,
    },
}

/// `FLOORPLAN`: a plan from a brief, previewed and chosen.
#[derive(Debug)]
pub(crate) struct FloorPlanCommand {
    state: State,
    spec: Spec,
    origin: Point2,
    /// Drawing units to the metre.
    scale: f64,
}

impl FloorPlanCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Width,
            spec: Spec {
                terrain: Terrain {
                    width: 15.0,
                    depth: 20.0,
                },
                setbacks: Setbacks::default(),
                rooms: Vec::new(),
                adjacent: Vec::new(),
                exterior_wall: 0.25,
                interior_wall: 0.15,
                door: 0.9,
                max_circulation: 0.15,
                windows: None,
                seed: 0,
            },
            origin: Point2::ORIGIN,
            scale: 1.0,
        }
    }

    /// A length typed, in drawing units, in metres.
    fn metres(&self, length: f64) -> f64 {
        length / self.scale
    }

    /// The plans for the brief, or what stopped them, said.
    fn plan(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        match alternatives(&self.spec, ALTERNATIVES) {
            Ok(plans) => {
                ctx.message(format!(
                    "{} found. {}",
                    if plans.len() == 1 {
                        "1 plan".to_owned()
                    } else {
                        format!("{} plans", plans.len())
                    },
                    describe(&plans[0], 0, plans.len())
                ));
                self.state = State::Choosing { plans, index: 0 };
                Step::Continue
            }
            Err(refusal) => {
                ctx.message(match &refusal {
                    Refusal::Infeasible { .. } => format!("This cannot be built: {refusal}"),
                    _ => refusal.to_string(),
                });
                Step::Finished
            }
        }
    }
}

/// What a plan is, in a line: its place among the plans, and the
/// circulation it adds, if it adds any.
fn describe(plan: &Layout, index: usize, of: usize) -> String {
    let circulation = plan.circulation.map_or(String::new(), |c| {
        format!(" It adds {:.2} m² of circulation.", plan.rooms[c].area())
    });
    format!("Plan {} of {of}.{circulation}", index + 1)
}

impl Command for FloorPlanCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.scale = ctx.drawing.unit().map_or(1.0, |unit| 1.0 / unit.metres());
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Width => Prompt::new("Specify terrain width")
                .with_default(display(self.spec.terrain.width * self.scale)),
            State::Depth => Prompt::new("Specify terrain depth")
                .with_default(display(self.spec.terrain.depth * self.scale)),
            State::RoomName if self.spec.rooms.is_empty() => Prompt::new("Enter a room's name"),
            State::RoomName => {
                Prompt::new("Enter a room's name, or press Enter when they are all named")
            }
            State::RoomArea(name) => Prompt::new(format!("Specify the {name}'s floor area in m²")),
            State::Pair => Prompt::new(
                "Enter two rooms that open onto each other as name/name, or press Enter",
            ),
            State::Windows => {
                Prompt::new("Specify the windows' width, or press Enter for no windows")
            }
            State::Brief => Prompt::new("Enter the brief as JSON"),
            State::Corner => {
                Prompt::new("Specify the terrain's lower left corner").with_default("0,0")
            }
            State::Choosing { plans, index } => {
                Prompt::new(format!("Plan {} of {}", index + 1, plans.len())).with_default("Accept")
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Width => ExpectedInput::distance().with_keywords(&[BRIEF, FILE]),
            State::Depth | State::Windows => ExpectedInput::distance(),
            State::RoomName | State::Pair | State::Brief => ExpectedInput::text(),
            State::RoomArea(_) => ExpectedInput::factor(),
            State::Corner => ExpectedInput::point(),
            State::Choosing { .. } => ExpectedInput::new(cad_input::InputKind::Nothing)
                .with_keywords(&[NEXT, PREVIOUS, ACCEPT]),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, State::Width);
        self.state = match (state, input) {
            (State::Width, CommandInput::Keyword(k)) if k == BRIEF => State::Brief,
            (State::Width, CommandInput::Keyword(k)) if k == FILE => {
                ctx.request_host(HostRequest::OpenBrief);
                State::Brief
            }
            (State::Width, CommandInput::Accept) => State::Depth,
            (State::Width | State::Depth, CommandInput::Distance(length))
                if !(length.is_finite() && length > 0.0) =>
            {
                ctx.message("The terrain's sides must be longer than zero.");
                return Step::Continue;
            }
            (State::Width, CommandInput::Distance(width)) => {
                self.spec.terrain.width = self.metres(width);
                State::Depth
            }
            (State::Depth, CommandInput::Distance(depth)) => {
                self.spec.terrain.depth = self.metres(depth);
                State::RoomName
            }
            (State::Depth, CommandInput::Accept) => State::RoomName,
            (State::RoomName, CommandInput::Text(name)) => {
                let name = name.trim().to_owned();
                if self.spec.room(&name).is_some() {
                    ctx.message(format!("A room is already called {name}."));
                    State::RoomName
                } else {
                    State::RoomArea(name)
                }
            }
            (State::RoomName, CommandInput::Accept) if self.spec.rooms.is_empty() => {
                ctx.message("A plan needs a room at least.");
                State::RoomName
            }
            (State::RoomName, CommandInput::Accept) => State::Pair,
            (State::RoomArea(name), CommandInput::Distance(area)) => {
                if area.is_finite() && area > 0.0 {
                    self.spec.rooms.push(Room::new(name, area));
                    State::RoomName
                } else {
                    ctx.message("A room's area must be greater than zero.");
                    State::RoomArea(name)
                }
            }
            (State::Pair, CommandInput::Text(text)) => {
                let names: Vec<&str> = text.split('/').map(str::trim).collect();
                match names.as_slice() {
                    [a, b]
                        if a != b && self.spec.room(a).is_some() && self.spec.room(b).is_some() =>
                    {
                        self.spec.adjacent.push([(*a).to_owned(), (*b).to_owned()]);
                    }
                    _ => ctx.message("Name two different rooms of the brief, as kitchen/living."),
                }
                State::Pair
            }
            (State::Pair, CommandInput::Accept) => State::Windows,
            (State::Windows, CommandInput::Accept) => State::Corner,
            (State::Windows, CommandInput::Distance(width)) if width.is_finite() && width > 0.0 => {
                self.spec.windows = Some(self.metres(width));
                State::Corner
            }
            (State::Windows, CommandInput::Distance(_)) => {
                ctx.message("A window's width must be greater than zero.");
                State::Windows
            }
            (State::Brief, CommandInput::Text(text)) => match Spec::from_json(&text) {
                Ok(spec) => {
                    self.spec = spec;
                    State::Corner
                }
                Err(error) => {
                    ctx.message(error.to_string());
                    State::Brief
                }
            },
            (State::Corner, input @ (CommandInput::Point(_) | CommandInput::Accept)) => {
                if let CommandInput::Point(corner) = input {
                    self.origin = corner;
                }
                return self.plan(ctx);
            }
            (State::Choosing { plans, index }, CommandInput::Keyword(k))
                if k == NEXT || k == PREVIOUS =>
            {
                let n = plans.len();
                let index = if k == NEXT {
                    (index + 1) % n
                } else {
                    (index + n - 1) % n
                };
                ctx.message(describe(&plans[index], index, n));
                State::Choosing { plans, index }
            }
            (State::Choosing { plans, index }, CommandInput::Keyword(_) | CommandInput::Accept) => {
                let plan = &plans[index];
                match draw(
                    ctx.drawing,
                    ctx.space,
                    &self.spec,
                    plan,
                    self.origin,
                    self.scale,
                    index as u32 + 1,
                ) {
                    Ok(made) => ctx.message(format!(
                        "Plan {} drawn: {} rooms, {} objects.",
                        index + 1,
                        plan.rooms.len(),
                        made.len()
                    )),
                    Err(error) => ctx.message(error.to_string()),
                }
                return Step::Finished;
            }
            (state, _) => state,
        };
        Step::Continue
    }

    fn preview(&self, _cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let State::Choosing { plans, index } = &self.state {
            let terrain = self.spec.terrain;
            let o = self.origin;
            let (w, d) = (terrain.width * self.scale, terrain.depth * self.scale);
            let corners = [
                o,
                Point2::new(o.x + w, o.y),
                Point2::new(o.x + w, o.y + d),
                Point2::new(o.x, o.y + d),
            ];
            for k in 0..4 {
                out.push(OverlayItem::PreviewSegment {
                    start: corners[k],
                    end: corners[(k + 1) % 4],
                });
            }
            for (_, entity) in entities(&self.spec, &plans[*index], self.origin, self.scale) {
                out.push(OverlayItem::PreviewEntity(entity));
            }
        }
    }
}
