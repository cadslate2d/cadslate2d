//! An intent document run through the session (ADR-0037, #253): each
//! compiled operation is its command started and its answers typed, as a
//! user would type them, through the one entry point every input takes.
//! A pick is a click on the object's nearest point; a selection is made by
//! ID as a user's would be. What the engine refuses is reported typed,
//! never guessed around.

use std::fmt;

use cad_core::{EntityId, LengthUnit, Point2};
use cad_intent::{Answer, Compiled, Intent, Pick, Step, Targets};

use crate::{Dirty, KeyAction, Modifiers, PointerButton, Session};

/// What stopped an operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunError {
    /// An ID the document names is not in the drawing.
    UnknownEntity { operation: usize, id: u64 },
    /// The operation acts on the selection, and nothing is selected.
    NothingSelected { operation: usize },
    /// The command refused what it was given and said why — `The lines
    /// are parallel.` — and the drawing is as it was. The command is
    /// cancelled if it was still asking.
    Refused {
        operation: usize,
        command: &'static str,
        reason: String,
    },
    /// The command was still asking when the answers ran out: it needed one
    /// the document does not give. The command is cancelled; the log's last
    /// line says what was asked.
    Incomplete {
        operation: usize,
        command: &'static str,
        prompt: String,
    },
    /// The command is not registered in this session.
    NotRegistered {
        operation: usize,
        command: &'static str,
    },
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownEntity { operation, id } => {
                write!(f, "operation {operation}: no object {id} in the drawing")
            }
            Self::NothingSelected { operation } => {
                write!(f, "operation {operation}: nothing is selected")
            }
            Self::Refused {
                operation,
                command,
                reason,
            } => write!(
                f,
                "operation {operation}: {command} refused: {}",
                reason.trim_end_matches('.')
            ),
            Self::Incomplete {
                operation,
                command,
                prompt,
            } => write!(
                f,
                "operation {operation}: {command} was still asking \"{prompt}\""
            ),
            Self::NotRegistered { operation, command } => {
                write!(f, "operation {operation}: {command} is not a command here")
            }
        }
    }
}

impl std::error::Error for RunError {}

/// What running an operation left in the log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ran {
    pub operation: usize,
    pub command: &'static str,
    /// Whether the drawing changed: an operation asking for what is
    /// already so leaves it false. One that was refused is a
    /// [`RunError::Refused`].
    pub changed: bool,
    /// The log lines the operation wrote, the echoes of its answers included.
    pub log: Vec<String>,
}

impl Session {
    /// Runs every operation of `intent`, in order, stopping at the first
    /// that cannot run. What ran is reported with its log; what stopped it
    /// is typed. The document's units are converted to the drawing's.
    pub fn run_intent(&mut self, intent: &Intent) -> Result<Vec<Ran>, RunError> {
        let scale = unit_scale(
            intent.units.map(cad_intent::Unit::length_unit),
            self.drawing().unit(),
        );
        let mut ran = Vec::new();
        for compiled in cad_intent::compile(intent) {
            ran.push(self.run_compiled(&compiled, scale)?);
        }
        Ok(ran)
    }

    /// One operation: what it made is the selection afterwards, so the next
    /// operation's "selection" is it. A run that stops leaves no command
    /// in progress.
    fn run_compiled(&mut self, compiled: &Compiled, scale: f64) -> Result<Ran, RunError> {
        let operation = compiled.operation;
        let command = compiled.command;
        if !self.is_registered(command) {
            return Err(RunError::NotRegistered { operation, command });
        }
        // The slate is clean: no command in progress, and no selection the
        // command would take as its objects unless the operation names one.
        if self.command_active() {
            self.key(KeyAction::Escape);
        }
        let log_from = self.log_total();
        let revision = self.drawing().revision();
        let first_new = self.drawing().next_entity_id();
        let result = self.run_steps(compiled, scale);
        // The command's own words for what it refused: the last line it
        // wrote that is a sentence, not the echo of a prompt and answer.
        let reason = self
            .log_since(log_from)
            .into_iter()
            .rev()
            .find(|line| line.ends_with('.') && !line.starts_with("Command:"));
        let changed = self.drawing().revision() != revision;
        if self.command_active() {
            let prompt = self.prompt();
            self.key(KeyAction::Escape);
            return Err(result.err().unwrap_or(match reason {
                Some(reason) => RunError::Refused {
                    operation,
                    command,
                    reason,
                },
                None => RunError::Incomplete {
                    operation,
                    command,
                    prompt,
                },
            }));
        }
        result?;
        // A command that goes on after a refusal — OFFSET, TRIM, EXTEND ask
        // for the next object — ends having changed nothing, and said why.
        if let (false, Some(reason)) = (changed, reason) {
            return Err(RunError::Refused {
                operation,
                command,
                reason,
            });
        }
        let made: Vec<EntityId> = (first_new..self.drawing().next_entity_id())
            .map(EntityId::from_raw)
            .filter(|id| self.drawing().entity(*id).is_some())
            .collect();
        if !made.is_empty() {
            self.select_only(&made);
        }
        Ok(Ran {
            operation,
            command,
            changed,
            log: self.log_since(log_from),
        })
    }

    fn run_steps(&mut self, compiled: &Compiled, scale: f64) -> Result<(), RunError> {
        let operation = compiled.operation;
        let command = compiled.command;
        let mut steps = compiled.steps.iter().peekable();
        let selected: Vec<EntityId> = match steps.peek() {
            Some(Step::Select(targets)) => {
                self.select_targets(operation, targets)?;
                steps.next();
                self.selection().iter().collect()
            }
            _ => {
                self.clear_selection();
                Vec::new()
            }
        };
        self.invoke_from_ui(command);
        // A command that chooses its objects from nothing — TRIM's and
        // EXTEND's edges — clears what was selected as it starts: it is
        // chosen again at the command's own prompt.
        if !selected.is_empty() && self.command_active() && self.selection().is_empty() {
            self.select_only(&selected);
        }
        for step in steps {
            if !self.command_active() {
                // The command ended before its answers did: what is left is
                // not asked for, as a text style that fixes the height.
                break;
            }
            match step {
                Step::Select(targets) => {
                    self.select_targets(operation, targets)?;
                }
                Step::Answer { answer, when } => {
                    if let Some(prefix) = when
                        && !self.prompt().starts_with(prefix)
                    {
                        continue;
                    }
                    let text = format_answer(answer, scale);
                    self.submit_text(&text);
                }
                Step::Pick(pick) => {
                    let at = self.pick_point(operation, pick, scale)?;
                    let screen = self.view().world_to_screen(at);
                    self.pointer_down(screen, PointerButton::Primary, Modifiers::NONE);
                    self.pointer_up(screen, PointerButton::Primary, Modifiers::NONE);
                }
            }
        }
        Ok(())
    }

    fn select_targets(&mut self, operation: usize, targets: &Targets) -> Result<Dirty, RunError> {
        match targets {
            Targets::Selection(_) => {
                if self.selection().is_empty() {
                    return Err(RunError::NothingSelected { operation });
                }
                Ok(Dirty::NONE)
            }
            Targets::Ids(ids) => {
                let ids = ids
                    .iter()
                    .map(|&id| self.entity_id(operation, id))
                    .collect::<Result<Vec<EntityId>, RunError>>()?;
                Ok(self.select_only(&ids))
            }
        }
    }

    fn entity_id(&self, operation: usize, id: u64) -> Result<EntityId, RunError> {
        let entity = EntityId::from_raw(id);
        if self.drawing().entity(entity).is_some() {
            Ok(entity)
        } else {
            Err(RunError::UnknownEntity { operation, id })
        }
    }

    /// Where a pick lands: the object's point nearest `near`, or the start
    /// of its first primitive.
    fn pick_point(&self, operation: usize, pick: &Pick, scale: f64) -> Result<Point2, RunError> {
        let id = self.entity_id(operation, pick.id)?;
        let drawing = self.drawing();
        let entity = drawing.entity(id).expect("checked");
        let primitives = entity.primitives(drawing.tables());
        let Some(first) = primitives.first() else {
            return Err(RunError::UnknownEntity {
                operation,
                id: pick.id,
            });
        };
        Ok(match pick.near {
            Some(near) => {
                let near = Point2::new(near[0] * scale, near[1] * scale);
                primitives
                    .iter()
                    .map(|primitive| primitive.closest_point(near))
                    .min_by(|a, b| a.distance_to(near).total_cmp(&b.distance_to(near)))
                    .expect("one primitive at least")
            }
            None => first.closest_point(first.closest_point(Point2::ORIGIN)),
        })
    }
}

/// How many drawing units a document unit is; one when either is unknown.
fn unit_scale(document: Option<LengthUnit>, drawing: Option<LengthUnit>) -> f64 {
    match (document, drawing) {
        (Some(document), Some(drawing)) => document.metres() / drawing.metres(),
        _ => 1.0,
    }
}

/// An answer as the user would type it, lengths and points in the
/// drawing's units.
fn format_answer(answer: &Answer, scale: f64) -> String {
    match answer {
        Answer::Point(p) => format!("{},{}", number(p.x * scale), number(p.y * scale)),
        Answer::Length(v) => number(v * scale),
        Answer::Number(v) | Answer::Angle(v) => number(*v),
        Answer::Keyword(k) => (*k).to_string(),
        Answer::Text(t) => t.clone(),
        Answer::Enter => String::new(),
    }
}

/// A number as typed: no exponent, which the input grammar does not take
/// in a coordinate, and no trailing noise.
fn number(value: f64) -> String {
    let text = format!("{value}");
    if text.contains('e') {
        format!("{value:.9}")
    } else {
        text
    }
}
