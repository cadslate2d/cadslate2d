//! A proposal (ADR-0037, #254): an intent document run against a copy of
//! the drawing, shown by the overlay as what it would add, change, and
//! remove, and applied to the drawing only on Accept — as one undo — or
//! dropped on Reject. The drawing and its revision are untouched until
//! then, as transient edits never touch the document.

use cad_core::EntityId;
use cad_document::Entity;
use cad_intent::Intent;

use crate::intent::{Ran, RunError};
use crate::{Dirty, OverlayItem, Session};

/// What an intent would do to the drawing, held by the session until the
/// user decides.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub intent: Intent,
    /// Objects the intent makes, as they would be.
    pub added: Vec<(EntityId, Entity)>,
    /// Objects it changes: as they are, and as they would be.
    pub changed: Vec<(EntityId, Entity, Entity)>,
    /// Objects it removes, as they are.
    pub removed: Vec<(EntityId, Entity)>,
    /// What ran, or what stopped it. A run that stopped is still a
    /// proposal: what ran before the stop is what it would do.
    pub outcome: Result<Vec<Ran>, RunError>,
}

/// The proposal as the panel shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalSummary {
    /// One line: what it adds, changes, and removes.
    pub headline: String,
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
    /// One line per operation: its command, and what the log said.
    pub operations: Vec<String>,
    /// What stopped the run, if anything.
    pub error: Option<String>,
}

impl Proposal {
    /// How many objects it adds, changes, and removes, in words.
    pub fn summary(&self) -> ProposalSummary {
        let count = |n: usize, what: &str| match n {
            0 => None,
            1 => Some(format!("{what} 1 object")),
            n => Some(format!("{what} {n} objects")),
        };
        let parts: Vec<String> = [
            count(self.added.len(), "adds"),
            count(self.changed.len(), "changes"),
            count(self.removed.len(), "removes"),
        ]
        .into_iter()
        .flatten()
        .collect();
        let headline = if parts.is_empty() {
            "Changes nothing".to_owned()
        } else {
            let mut text = parts.join(", ");
            if let Some(first) = text.get_mut(..1) {
                first.make_ascii_uppercase();
            }
            text
        };
        let operations = match &self.outcome {
            Ok(ran) => ran.iter().map(describe_ran).collect(),
            Err(_) => Vec::new(),
        };
        ProposalSummary {
            headline,
            added: self.added.len(),
            changed: self.changed.len(),
            removed: self.removed.len(),
            operations,
            error: self.outcome.as_ref().err().map(ToString::to_string),
        }
    }

    /// What the overlay draws: what would be there in the proposal's
    /// style, what would go in the removal's.
    pub fn overlay(&self, out: &mut Vec<OverlayItem>) {
        for (_, entity) in &self.added {
            out.push(OverlayItem::ProposalAdded(entity.clone()));
        }
        for (_, before, after) in &self.changed {
            out.push(OverlayItem::ProposalRemoved(before.clone()));
            out.push(OverlayItem::ProposalAdded(after.clone()));
        }
        for (_, entity) in &self.removed {
            out.push(OverlayItem::ProposalRemoved(entity.clone()));
        }
    }
}

/// `CIRCLE: Imported …`: the command, and the last line the log has of
/// it beyond the echoes of its answers, when there is one.
fn describe_ran(ran: &Ran) -> String {
    let said = ran
        .log
        .iter()
        .rev()
        .find(|line| !line.starts_with("Command: ") && !line.contains(':'))
        .cloned();
    match said {
        Some(said) => format!("{}: {said}", ran.command),
        None => ran.command.to_owned(),
    }
}

impl Session {
    /// Runs `intent` against a copy of the drawing and keeps the result as
    /// the proposal, replacing any before it. The drawing and its revision
    /// are untouched; the overlay shows what would change.
    pub fn propose(&mut self, intent: Intent) -> Dirty {
        let mut scratch = self.scratch();
        let outcome = scratch.run_intent(&intent);
        let before = self.drawing();
        let after = scratch.drawing();
        let mut added = Vec::new();
        let mut changed = Vec::new();
        for (id, entity) in after.entities() {
            match before.entity(id) {
                None => added.push((id, entity.clone())),
                Some(was) if was != entity || before.properties(id) != after.properties(id) => {
                    changed.push((id, was.clone(), entity.clone()));
                }
                Some(_) => {}
            }
        }
        let removed: Vec<(EntityId, Entity)> = before
            .entities()
            .filter(|(id, _)| after.entity(*id).is_none())
            .map(|(id, entity)| (id, entity.clone()))
            .collect();
        self.set_proposal(Some(Proposal {
            intent,
            added,
            changed,
            removed,
            outcome,
        }));
        Dirty::OVERLAY | Dirty::UI
    }

    /// The proposal, if one is waiting.
    pub fn proposal(&self) -> Option<&Proposal> {
        self.proposal_ref().as_ref()
    }

    /// Applies the proposal to the drawing as one undo step, running its
    /// intent again on the drawing itself — the same commands, the same
    /// answers, the same IDs — and drops it. Nothing happens without one.
    pub fn accept_proposal(&mut self) -> Dirty {
        let Some(proposal) = self.take_proposal() else {
            return Dirty::NONE;
        };
        self.begin_undo_group();
        let outcome = self.run_intent(&proposal.intent);
        self.end_undo_group();
        let dirty = Dirty::SCENE | Dirty::OVERLAY | Dirty::UI;
        match outcome {
            Ok(_) => dirty | self.notify("Proposal accepted."),
            Err(error) => dirty | self.notify(format!("Proposal stopped: {error}.")),
        }
    }

    /// Drops the proposal; the drawing was never touched.
    pub fn reject_proposal(&mut self) -> Dirty {
        if self.take_proposal().is_none() {
            return Dirty::NONE;
        }
        Dirty::OVERLAY | Dirty::UI
    }
}
