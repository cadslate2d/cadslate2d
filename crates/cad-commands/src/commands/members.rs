//! MEMBERS (#329): centrelines made into members with a width — a truss's
//! chords and webs as steel sections, a frame's beams and posts. The lines
//! are chosen in groups, each with its width; every group's members meet
//! every other's as `cad_geometry::members` joins them — continuous
//! through a joint along them, butted against another's face, mitred where
//! they end together — and each becomes a closed polyline with its
//! centreline's properties. The centrelines go unless they are kept. One
//! invocation is one undo.

use cad_core::EntityId;
use cad_core::tolerance::LinearTolerance;
use cad_document::{Entity, PolylineEntity};
use cad_geometry::Member;
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::ltscale::display;
use super::modify::{counted, selection_expected};
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const YES: Keyword = Keyword::new("Yes");
const NO: Keyword = Keyword::new("No");

#[derive(Debug)]
enum State {
    /// The centrelines of a group, or Enter when they are all chosen.
    Select,
    Width(Vec<EntityId>),
    Keep,
}

#[derive(Debug)]
pub(crate) struct MembersCommand {
    state: State,
    /// Each centreline chosen and its width: a line chosen again takes the
    /// later width.
    chosen: Vec<(EntityId, f64)>,
    /// The width last given: the next group's default.
    width: Option<f64>,
}

impl MembersCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Select,
            chosen: Vec::new(),
            width: None,
        }
    }

    /// The lines of the selection as a group; what is not a line is left out
    /// and said.
    fn take_group(&mut self, ctx: &mut CommandContext<'_>) -> Option<Vec<EntityId>> {
        let ids: Vec<EntityId> = ctx.selection.iter().collect();
        ctx.selection.clear();
        let lines: Vec<EntityId> = ids
            .iter()
            .copied()
            .filter(|id| matches!(ctx.drawing.entity(*id), Some(Entity::Line(_))))
            .collect();
        let others = ids.len() - lines.len();
        if others > 0 {
            ctx.message(format!(
                "Only lines are made members: {} left out.",
                counted(others, "other object")
            ));
        }
        (!lines.is_empty()).then_some(lines)
    }

    fn commit(&self, keep: bool, ctx: &mut CommandContext<'_>) -> Step {
        let centres: Vec<(EntityId, Member)> = self
            .chosen
            .iter()
            .filter_map(|(id, width)| match ctx.drawing.entity(*id) {
                Some(Entity::Line(line)) => Some((
                    *id,
                    Member {
                        centre: line.segment(),
                        width: *width,
                    },
                )),
                _ => None,
            })
            .collect();
        let points: Vec<cad_core::Point2> = centres
            .iter()
            .flat_map(|(_, m)| [m.centre.start, m.centre.end])
            .collect();
        let tolerance = LinearTolerance::DEFAULT.around(&points);
        let members: Vec<Member> = centres.iter().map(|(_, m)| *m).collect();
        let outlines = match cad_geometry::members::outlines(&members, tolerance) {
            Ok(outlines) => outlines,
            Err(_) => {
                ctx.message("A member has no length or no width.");
                return Step::Finished;
            }
        };
        let mut transaction = ctx.transaction();
        for outline in &outlines {
            // The first centreline it was made from gives its properties.
            let source = centres[outline.sources[0]].0;
            transaction
                .insert_like(source, PolylineEntity::new(outline.ring.clone(), true))
                .expect("the centreline was just read");
        }
        if !keep {
            for (id, _) in &centres {
                transaction
                    .remove(*id)
                    .expect("the centreline was just read");
            }
        }
        transaction.commit();
        ctx.message(format!(
            "{} made from {}.",
            counted(outlines.len(), "member"),
            counted(centres.len(), "line")
        ));
        Step::Finished
    }
}

impl Command for MembersCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        // Lines selected beforehand are the first group.
        if !ctx.selection.is_empty()
            && let Some(group) = self.take_group(ctx)
        {
            self.state = State::Width(group);
        }
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Select if self.chosen.is_empty() => {
                Prompt::new("Select the centrelines of members")
            }
            State::Select => Prompt::new(
                "Select members of another width, or press Enter when they are all chosen",
            ),
            State::Width(_) => {
                let prompt = Prompt::new("Specify member width");
                match self.width {
                    Some(width) => prompt.with_default(display(width)),
                    None => prompt,
                }
            }
            State::Keep => Prompt::new("Keep the centrelines?").with_default("No"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Select => selection_expected(),
            State::Width(_) => ExpectedInput::distance(),
            State::Keep => ExpectedInput::new(InputKind::Nothing).with_keywords(&[YES, NO]),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, State::Select);
        self.state = match (state, input) {
            (State::Select, CommandInput::Accept) => {
                if ctx.selection.is_empty() {
                    if self.chosen.is_empty() {
                        ctx.message("No lines chosen.");
                        return Step::Finished;
                    }
                    State::Keep
                } else {
                    match self.take_group(ctx) {
                        Some(group) => State::Width(group),
                        None => State::Select,
                    }
                }
            }
            (State::Width(group), CommandInput::Accept) => match self.width {
                Some(width) => self.add(group, width),
                None => State::Width(group),
            },
            (State::Width(group), CommandInput::Distance(width)) => {
                if width.is_finite() && width > 0.0 {
                    self.width = Some(width);
                    self.add(group, width)
                } else {
                    ctx.message("A member's width must be greater than zero.");
                    State::Width(group)
                }
            }
            (State::Keep, CommandInput::Keyword(k)) => return self.commit(k == YES, ctx),
            (State::Keep, CommandInput::Accept) => return self.commit(false, ctx),
            (state, _) => state,
        };
        Step::Continue
    }
}

impl MembersCommand {
    fn add(&mut self, group: Vec<EntityId>, width: f64) -> State {
        for id in group {
            self.chosen.retain(|(chosen, _)| *chosen != id);
            self.chosen.push((id, width));
        }
        State::Select
    }
}
