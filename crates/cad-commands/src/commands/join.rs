//! JOIN: makes one object out of several (DESIGN §22, §31).
//!
//! Collinear lines become a line, touching arcs of one circle an arc, and
//! lines, arcs, and open polylines that meet end to end a polyline. What is
//! joined is decided by `cad_document::join`; this is only the conversation.

use cad_core::EntityId;
use cad_document::{Entity, JoinError, close_into_circle, join};
use cad_input::{ExpectedInput, Keyword};

use super::modify::selection_expected;
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const CLOSE: Keyword = Keyword::new("Close");

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

/// "1 polyline", "2 polylines".
fn counted(count: usize, noun: &str) -> String {
    let plural = if count == 1 { "" } else { "s" };
    format!("{count} {noun}{plural}")
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

        // The oldest member becomes the result; the others go.
        let mut transaction = ctx.drawing.transaction();
        for (members, entity) in results {
            let mut members = members.into_iter().map(|member| ids[member]);
            if let Some(kept) = members.next() {
                transaction
                    .replace(kept, entity)
                    .expect("selected entities are live");
            }
            for member in members {
                transaction
                    .remove(member)
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
