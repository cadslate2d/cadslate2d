//! JOIN: makes one object out of several (DESIGN §22, §31).
//!
//! Collinear lines become a line, touching arcs of one circle an arc, and
//! lines, arcs, and open polylines that meet end to end a polyline. What is
//! joined is decided by `cad_document::join`; this is only the conversation.

use cad_core::EntityId;
use cad_document::{Entity, JoinError, close_into_circle, join};
use cad_input::{ExpectedInput, Keyword};

use super::modify::{counted, selection_expected};
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

/// `CL`, so that `C` stays the crossing polygon of every selection prompt (#185).
const CLOSE: Keyword = Keyword::new("CLose");

#[derive(Debug)]
pub(crate) struct JoinCommand;

fn explain(error: JoinError) -> &'static str {
    match error {
        JoinError::TooFew => "Select at least two objects to join.",
        JoinError::NothingMeets => "Nothing to join: none of these objects meet end to end.",
        JoinError::ArcsApart => {
            "These arcs are on one circle but do not touch. The Close option makes the whole circle."
        }
        JoinError::NotArcsOfOneCircle => {
            "Close makes a circle out of arcs of one circle; select only those."
        }
    }
}

impl JoinCommand {
    /// Joins the selection, or closes it into a circle. Whatever happens,
    /// the command is over: a refusal says why and changes nothing.
    fn finish(ctx: &mut CommandContext<'_>, close: bool) -> Step {
        let (ids, entities): (Vec<EntityId>, Vec<&Entity>) = ctx
            .selection
            .iter()
            .filter_map(|id| Some((id, ctx.drawing.entity(id)?)))
            .unzip();
        if ids.is_empty() {
            ctx.message("No objects selected.");
            return Step::Finished;
        }

        // Each result, with the selected objects it takes the place of.
        let results: Result<Vec<(Vec<usize>, Entity)>, JoinError> = if close {
            close_into_circle(&entities).map(|circle| vec![((0..ids.len()).collect(), circle)])
        } else {
            join(&entities).map(|joined| {
                joined
                    .into_iter()
                    .map(|result| (result.members, result.entity))
                    .collect()
            })
        };
        let results = match results {
            Ok(results) => results,
            Err(error) => {
                ctx.message(explain(error));
                return Step::Finished;
            }
        };

        let used: usize = results.iter().map(|(members, _)| members.len()).sum();
        let summary = if close {
            format!("{} closed into a circle.", counted(used, "arc"))
        } else {
            let kind = results[0].1.kind_name();
            let made = if results.iter().all(|(_, entity)| entity.kind_name() == kind) {
                counted(results.len(), kind)
            } else {
                counted(results.len(), "object")
            };
            format!("{} joined into {made}.", counted(used, "object"))
        };

        // The oldest member becomes the result; the others go. A constraint
        // on a point that is still an end of the result goes with it; one
        // on a joint, now inside it, goes (#421).
        let mut transaction = ctx.transaction();
        for (members, entity) in results {
            let members: Vec<EntityId> = members.into_iter().map(|member| ids[member]).collect();
            if let Some((kept, others)) = members.split_first() {
                transaction
                    .reshape(*kept, entity, Vec::new(), others)
                    .expect("selected entities are live and appear once");
            }
        }
        transaction.commit();
        ctx.selection.clear();
        ctx.message(summary);
        Step::Finished
    }
}

impl Command for JoinCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            Self::finish(ctx, false)
        }
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Select objects to join")
    }

    fn expected(&self) -> ExpectedInput {
        selection_expected().with_keywords(&[CLOSE])
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Accept => Self::finish(ctx, false),
            CommandInput::Keyword(_) => Self::finish(ctx, true),
            _ => Step::Continue,
        }
    }
}
