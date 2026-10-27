//! PARTVIEWS (#339, ADR-0040): a part's standard views from its
//! description — typed as JSON, or a drawn front view: a closed polyline
//! of straight edges and the circles of holes through it, with a depth.
//! First- or third-angle projection, the front view sectioned or not, and
//! where it goes; drawn on the `PART-*` layers — hidden and centre lines
//! in their linetypes, LTSCALE set for the part's size if the drawing
//! still has the default — one undo, in a group `PART-n` holding the part
//! (`crate::partviews`).

use cad_core::{EntityId, Point2};
use cad_document::{DrawingSettings, Entity};
use cad_geometry::views::{Axis, Hole, Part, PartError, views};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::ltscale::display;
use super::modify::selection_expected;
use crate::partviews::{
    GROUP, LAYERS, Projection, description, entities, ltscale_for, parse, size,
};
use crate::views::draw_group_styled;
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const JSON: Keyword = Keyword::new("Json");
const SELECT: Keyword = Keyword::new("Select");
const FIRST: Keyword = Keyword::new("First");
const THIRD: Keyword = Keyword::new("Third");
const YES: Keyword = Keyword::new("Yes");
const NO: Keyword = Keyword::new("No");

#[derive(Clone, Copy, Debug)]
enum State {
    How,
    Json,
    Select,
    Depth,
    Projection,
    Section,
    Place,
}

/// A front view drawn by hand: its outline, and its circles' centres and
/// radii.
type DrawnFront = (Vec<Point2>, Vec<(Point2, f64)>);

#[derive(Debug)]
pub(crate) struct PartViewsCommand {
    state: State,
    part: Option<Part>,
    /// A drawn front view's outline and circles, waiting for a depth.
    drawn: Option<DrawnFront>,
    projection: Projection,
    section: bool,
}

impl PartViewsCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::How,
            part: None,
            drawn: None,
            projection: Projection::First,
            section: false,
        }
    }

    /// A drawn front view from the selection: its closed polyline of
    /// straight edges, and its circles.
    fn take_selection(&mut self, ctx: &mut CommandContext<'_>) -> State {
        let ids: Vec<EntityId> = ctx.selection.iter().collect();
        ctx.selection.clear();
        let mut outline = None;
        let mut circles = Vec::new();
        for id in ids {
            match ctx.drawing.entity(id) {
                Some(Entity::Polyline(p))
                    if p.closed && outline.is_none() && p.bulges().all(|b| b == 0.0) =>
                {
                    outline = Some(p.vertices.clone());
                }
                Some(Entity::Circle(c)) => circles.push((c.center, c.radius)),
                _ => {}
            }
        }
        match outline {
            Some(outline) => {
                self.drawn = Some((outline, circles));
                State::Depth
            }
            None => {
                ctx.message(
                    "Select a closed polyline of straight edges: the front view's outline.",
                );
                State::Select
            }
        }
    }

    /// Why a part cannot be drawn, in the user's words.
    fn explain(error: PartError) -> String {
        match error {
            PartError::Profile => "The part's profile needs three corners and an area.".into(),
            PartError::NotPositive(what) => format!("The part's {what} must be greater than zero."),
            PartError::HoleOutside(i) => format!("Hole {} is not inside the part.", i + 1),
            PartError::Bore => "The bore must be narrower than every step.".into(),
        }
    }

    fn finish(&self, at: Point2, ctx: &mut CommandContext<'_>) -> Step {
        let Some(part) = &self.part else {
            return Step::Finished;
        };
        let views = match views(part, self.section) {
            Ok(views) => views,
            Err(error) => {
                ctx.message(Self::explain(error));
                return Step::Finished;
            }
        };
        let made = entities(&views, self.projection, at);
        // Dashes the part's size can show, unless LTSCALE was set already.
        let default = DrawingSettings::DEFAULT_LTSCALE.to_bits();
        let ltscale = (ctx.drawing.ltscale().to_bits() == default)
            .then(|| ltscale_for(size(&views)))
            .filter(|scale| scale.to_bits() != default);
        let space = ctx.space;
        match draw_group_styled(
            ctx.drawing,
            space,
            &LAYERS,
            made,
            GROUP,
            description(part, self.projection, self.section, at),
            ltscale,
        ) {
            Ok((_, name)) => {
                let shown = if views.top.is_some() {
                    "front, top, and right side"
                } else {
                    "front and right side"
                };
                let scale = ltscale.map_or(String::new(), |s| {
                    format!(
                        " LTSCALE set to {} for its hidden and centre lines.",
                        display(s)
                    )
                });
                ctx.message(format!(
                    "Part views drawn as {name}: {shown}, {}-angle.{scale}",
                    self.projection.name()
                ));
            }
            Err(error) => ctx.message(error.to_string()),
        }
        Step::Finished
    }

    /// The part checked: refused now, with the reason, rather than after
    /// the questions.
    fn checked(&mut self, part: Part, ctx: &mut CommandContext<'_>, again: State) -> State {
        match views(&part, false) {
            Ok(_) => {
                self.part = Some(part);
                State::Projection
            }
            Err(error) => {
                ctx.message(Self::explain(error));
                again
            }
        }
    }
}

impl Command for PartViewsCommand {
    fn prompt(&self) -> Prompt {
        match self.state {
            State::How => Prompt::new("Describe the part").with_default("Json"),
            State::Json => Prompt::new("Enter the part as JSON"),
            State::Select => Prompt::new("Select the front view's outline and its holes"),
            State::Depth => Prompt::new("Specify the part's depth"),
            State::Projection => Prompt::new("Which projection?").with_default("First"),
            State::Section => Prompt::new("Section the front view?").with_default("No"),
            State::Place => {
                Prompt::new("Specify the front view's lower left corner").with_default("0,0")
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::How => ExpectedInput::new(InputKind::Nothing).with_keywords(&[JSON, SELECT]),
            State::Json => ExpectedInput::text(),
            State::Select => selection_expected(),
            State::Depth => ExpectedInput::distance(),
            State::Projection => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&[FIRST, THIRD])
            }
            State::Section => ExpectedInput::new(InputKind::Nothing).with_keywords(&[YES, NO]),
            State::Place => ExpectedInput::point(),
        }
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let (State::Place, Some(part)) = (self.state, &self.part)
            && let Ok(views) = views(part, self.section)
        {
            for (_, entity) in entities(&views, self.projection, cursor) {
                out.push(OverlayItem::PreviewEntity(entity));
            }
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        self.state = match (self.state, input) {
            (State::How, CommandInput::Keyword(k)) if k == SELECT => State::Select,
            (State::How, CommandInput::Keyword(_) | CommandInput::Accept) => State::Json,
            (State::Json, CommandInput::Text(text)) => match parse(&text) {
                Ok(part) => self.checked(part, ctx, State::Json),
                Err(why) => {
                    ctx.message(why);
                    State::Json
                }
            },
            (State::Select, CommandInput::Accept) => {
                if ctx.selection.is_empty() {
                    State::Select
                } else {
                    self.take_selection(ctx)
                }
            }
            (State::Depth, CommandInput::Distance(depth)) => {
                let (profile, circles) = self.drawn.clone().unwrap_or_default();
                let holes = circles
                    .into_iter()
                    .map(|(c, r)| Hole {
                        axis: Axis::Y,
                        at: [c.x, c.y],
                        diameter: 2.0 * r,
                    })
                    .collect();
                let part = Part::Prism {
                    profile,
                    depth,
                    holes,
                };
                self.checked(part, ctx, State::Depth)
            }
            (State::Projection, CommandInput::Keyword(k)) => {
                self.projection = if k == THIRD {
                    Projection::Third
                } else {
                    Projection::First
                };
                State::Section
            }
            (State::Projection, CommandInput::Accept) => State::Section,
            (State::Section, CommandInput::Keyword(k)) => {
                self.section = k == YES;
                State::Place
            }
            (State::Section, CommandInput::Accept) => State::Place,
            (State::Place, CommandInput::Point(at)) => return self.finish(at, ctx),
            (State::Place, CommandInput::Accept) => return self.finish(Point2::ORIGIN, ctx),
            (state, _) => state,
        };
        Step::Continue
    }
}
