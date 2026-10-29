//! An intent document run through the session (ADR-0037, #253): each
//! compiled operation is its command started and its answers typed, as a
//! user would type them, through the one entry point every input takes.
//! A pick is a click on the object's nearest point — where a point is
//! asked for, the point `near` typed; a selection is made by
//! ID as a user's would be. What the engine refuses is reported typed,
//! never guessed around.

use std::fmt;

use cad_core::{EntityId, LengthUnit, Point2, Vec2};
use cad_intent::{Answer, Compiled, Intent, Pick, Step, Targets};

use cad_input::InputKind;

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
    /// What it made.
    pub made: Vec<EntityId>,
    /// Why it was passed over, when it was: a hatch in a sheet's view
    /// whose point no closed outline surrounds (#358). The rest runs.
    pub skipped: Option<String>,
}

/// How far apart views are drawn before they are placed: far beyond any
/// view's size, in drawing units.
const STAGING: f64 = 1.0e6;

/// Past the widest sheet at 1:1, in millimetres: an A0's 1189 and room.
const SHEET_REACH: f64 = 2500.0;

/// Between a sheet and what the drawing held before it, in millimetres of
/// paper.
const SHEET_GAP: f64 = 50.0;

/// The paper a document names, as the title block command takes it.
fn paper_of(paper: cad_intent::PaperSize) -> crate::settings::Paper {
    use crate::settings::Paper;
    match paper {
        cad_intent::PaperSize::A4 => Paper::A4,
        cad_intent::PaperSize::A3 => Paper::A3,
        cad_intent::PaperSize::A2 => Paper::A2,
        cad_intent::PaperSize::A1 => Paper::A1,
        cad_intent::PaperSize::A0 => Paper::A0,
    }
}

/// Where an operation draws: moved by `offset` when it is a view's, and
/// where each view of the document was drawn.
struct Place<'a> {
    offset: Vec2,
    staging: &'a [(String, Vec2)],
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
        // What each view's operations made, for its VIEWFRAME (#351).
        let mut views: Vec<(String, Vec<EntityId>)> = Vec::new();
        let compiled_all = cad_intent::compile(intent);
        // Each view drawn in a place of its own, far from the others', so
        // a view's operations — a hatch's region, a trim's edges, a snap —
        // meet only its own objects before VIEWFRAME places them (#358).
        // Clear of the sheet too: a title block at 1:N is that many times
        // its paper, and an A0 sheet at 1:1000 reached into the first view
        // staged a million units away, its border and table cutting the
        // hatches of the parcels there. Each view a step past what the
        // drawing holds, the step past the largest sheet the document draws.
        let largest_sheet = intent
            .operations
            .iter()
            .filter_map(|op| match op {
                cad_intent::Operation::TitleBlock { scale: n, .. } => Some(n.unwrap_or(1.0)),
                _ => None,
            })
            .fold(1.0_f64, f64::max);
        let step = STAGING.max(SHEET_REACH * largest_sheet * scale);
        let past = self
            .drawing()
            .extents()
            .map_or(0.0, |extents| extents.max().x.max(0.0));
        let mut staging: Vec<(String, Vec2)> = Vec::new();
        for compiled in &compiled_all {
            if let Some(view) = &compiled.view
                && !staging.iter().any(|(id, _)| id == view)
            {
                let k = staging.len() as f64 + 1.0;
                staging.push((view.clone(), Vec2::new(past + step * k, 0.0)));
            }
        }
        // A sheet is drawn beside what the drawing holds, never on it: a
        // second sheet — the upper floor of a house whose ground floor is
        // drawn — names its corner as the first did, at the origin (#358).
        let held = self.drawing().extents();
        let unit_per_mm = self
            .drawing()
            .unit()
            .map_or(1.0, |unit| 0.001 / unit.metres());
        let beside = |op: &cad_intent::Operation| -> Option<Vec2> {
            let cad_intent::Operation::TitleBlock {
                paper,
                portrait,
                scale: n,
                at,
                ..
            } = op
            else {
                return None;
            };
            let held = held?;
            let per_mm = n.unwrap_or(1.0) * unit_per_mm;
            let (w, h) = crate::titleblock::sheet(paper_of(*paper), !*portrait);
            let [x, y] = at.unwrap_or([0.0, 0.0]);
            let corner = Point2::new(x * scale, y * scale);
            let sheet =
                cad_core::Aabb::from_points(corner, corner + Vec2::new(w * per_mm, h * per_mm));
            sheet
                .intersects(&held)
                .then(|| Vec2::new(held.max().x + SHEET_GAP * per_mm - corner.x, 0.0))
        };
        for compiled in compiled_all {
            let moved = (compiled.view.is_none() && compiled.command == "TITLEBLOCK")
                .then(|| intent.operations.get(compiled.operation).and_then(beside))
                .flatten();
            let offset = compiled
                .view
                .as_ref()
                .and_then(|v| staging.iter().find(|(id, _)| id == v))
                .map_or(moved.unwrap_or(Vec2::new(0.0, 0.0)), |(_, at)| *at);
            let place = Place {
                offset,
                staging: &staging,
            };
            let done = match self.run_compiled(&compiled, scale, &views, &place) {
                Ok(done) => done,
                // A hatch in a sheet's view that finds no closed outline is
                // passed over, and said: one missing outline does not lose
                // the sheet (#358). Anything else stops the run, as before.
                Err(RunError::Refused {
                    operation,
                    command: "HATCH",
                    reason,
                }) if compiled.view.is_some() && reason.starts_with("No closed boundary") => {
                    let view = compiled.view.clone().unwrap_or_default();
                    Ran {
                        operation,
                        command: "HATCH",
                        changed: false,
                        log: Vec::new(),
                        made: Vec::new(),
                        skipped: Some(format!(
                            "a hatch in view {view}: no closed outline surrounds its point"
                        )),
                    }
                }
                Err(error) => return Err(error),
            };
            let mut done = done;
            if moved.is_some() {
                done.log.push(
                    "The sheet is drawn to the right of what the drawing held, clear of it."
                        .to_owned(),
                );
            }
            if let Some(view) = &compiled.view {
                match views.iter_mut().find(|(id, _)| id == view) {
                    Some((_, made)) => made.extend(&done.made),
                    None => views.push((view.clone(), done.made.clone())),
                }
            }
            ran.push(done);
        }
        Ok(ran)
    }

    /// One operation: what it made is the selection afterwards, so the next
    /// operation's "selection" is it. A run that stops leaves no command
    /// in progress.
    fn run_compiled(
        &mut self,
        compiled: &Compiled,
        scale: f64,
        views: &[(String, Vec<EntityId>)],
        place: &Place<'_>,
    ) -> Result<Ran, RunError> {
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
        // An operation that acts on no objects and makes none — a layer, a
        // setting — leaves the selection as it was, so "the selection"
        // after it is still what the one before made (#358).
        let selected_before: Vec<EntityId> = self.selection().iter().collect();
        let acts_on_objects = compiled
            .steps
            .iter()
            .any(|step| matches!(step, Step::Select(_) | Step::SelectView(_) | Step::Pick(_)));
        let result = self.run_steps(compiled, scale, views, place);
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
        } else if !acts_on_objects {
            let still: Vec<EntityId> = selected_before
                .into_iter()
                .filter(|id| self.drawing().entity(*id).is_some())
                .collect();
            self.select_only(&still);
        }
        Ok(Ran {
            operation,
            command,
            changed,
            log: self.log_since(log_from),
            made,
            skipped: None,
        })
    }

    fn run_steps(
        &mut self,
        compiled: &Compiled,
        scale: f64,
        views: &[(String, Vec<EntityId>)],
        place: &Place<'_>,
    ) -> Result<(), RunError> {
        // A point of the operation, in the drawing, where it is drawn.
        let drawn = |p: Point2| Point2::new(p.x * scale, p.y * scale) + place.offset;
        // What a view's operations made, still there.
        let view_made = |session: &Self, id: &str| -> Vec<EntityId> {
            views
                .iter()
                .find(|(view, _)| view == id)
                .map(|(_, made)| {
                    made.iter()
                        .copied()
                        .filter(|e| session.drawing().entity(*e).is_some())
                        .collect()
                })
                .unwrap_or_default()
        };
        let operation = compiled.operation;
        let command = compiled.command;
        let mut steps = compiled.steps.iter().peekable();
        // "The selection" is what was selected when the operation began —
        // what the last one made — even where a command clears it before
        // asking for objects, as HATCH's Select does.
        let before: Vec<EntityId> = self.selection().iter().collect();
        let selected: Vec<EntityId> = match steps.peek() {
            Some(Step::Select(targets)) => {
                self.select_targets(operation, targets)?;
                steps.next();
                self.selection().iter().collect()
            }
            Some(Step::SelectView(id)) => {
                let made = view_made(self, id);
                if made.is_empty() {
                    return Err(RunError::NothingSelected { operation });
                }
                self.select_only(&made);
                steps.next();
                made
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
                Step::SelectView(id) => {
                    let made = view_made(self, id);
                    if made.is_empty() {
                        return Err(RunError::NothingSelected { operation });
                    }
                    self.select_only(&made);
                }
                Step::Select(Targets::Selection(_)) if self.selection().is_empty() => {
                    if before.is_empty() {
                        return Err(RunError::NothingSelected { operation });
                    }
                    self.select_only(&before);
                }
                Step::Select(targets) => {
                    self.select_targets(operation, targets)?;
                }
                Step::Answer { answer, when } => {
                    if let Some(prefix) = when
                        && !self.prompt().starts_with(prefix)
                    {
                        continue;
                    }
                    let text = match answer {
                        Answer::Point(p) => format_answer(&Answer::Point(drawn(*p)), 1.0),
                        Answer::Staging(view) => {
                            let at = place
                                .staging
                                .iter()
                                .find(|(id, _)| id == view)
                                .map_or(Point2::ORIGIN, |(_, by)| Point2::ORIGIN + *by);
                            format_answer(&Answer::Point(at), 1.0)
                        }
                        Answer::InView { view, point } => {
                            let at = self.in_view(operation, command, view, *point, scale)?;
                            format_answer(&Answer::Point(at), 1.0)
                        }
                        Answer::Beyond { side, points } => {
                            let made = compiled
                                .view
                                .as_ref()
                                .map(|id| view_made(self, id))
                                .unwrap_or_default();
                            let points: Vec<Point2> = points.iter().map(|p| drawn(*p)).collect();
                            let at = self.beyond(&made, *side, &points, 1.0);
                            format_answer(&Answer::Point(at), 1.0)
                        }
                        _ => format_answer(answer, scale),
                    };
                    self.submit_text(&text);
                }
                // Where an end, a centre, or a vertex is asked for, the
                // pick is the point it is at: typed, as exact as a snap.
                Step::Pick(pick) if self.expected().kind == InputKind::Point => {
                    let at = match pick.near {
                        Some(near) => drawn(Point2::new(near[0], near[1])),
                        None => self.pick_point(operation, pick, scale, place.offset)?,
                    };
                    self.entity_id(operation, pick.id)?;
                    self.submit_text(&format_answer(&Answer::Point(at), 1.0));
                }
                Step::Pick(pick) => {
                    let at = self.pick_point(operation, pick, scale, place.offset)?;
                    let screen = self.view().world_to_screen(at);
                    self.pointer_down(screen, PointerButton::Primary, Modifiers::NONE);
                    self.pointer_up(screen, PointerButton::Primary, Modifiers::NONE);
                }
            }
        }
        Ok(())
    }

    /// Where `point`, in the placed view `view`'s own coordinates, is on
    /// the sheet: the view's origin, the point in the drawing's units scaled
    /// by the view's factor (ADR-0041 §4).
    fn in_view(
        &self,
        operation: usize,
        command: &'static str,
        view: &str,
        point: Point2,
        scale: f64,
    ) -> Result<Point2, RunError> {
        let refused = |reason: String| RunError::Refused {
            operation,
            command,
            reason,
        };
        let sheet = crate::compose::sheet_of(self.drawing())
            .ok_or_else(|| refused("There is no sheet.".to_owned()))?;
        let record = crate::compose::views_on(self.drawing(), &sheet.group)
            .into_iter()
            .find(|v| v.id == view)
            .ok_or_else(|| refused(format!("There is no view {view} on the sheet.")))?;
        let k = record.ratio.map_or(1.0, |r| r * sheet.frame.units_per_mm);
        Ok(record.origin + (point - Point2::ORIGIN) * (scale * k))
    }

    /// Where a chain's dimension line goes (#355): on `side` of the
    /// points, 10 paper millimetres beyond everything the view has drawn
    /// there — its geometry and the chains already on that side.
    fn beyond(
        &self,
        drawn: &[EntityId],
        side: cad_intent::ChainSide,
        points: &[Point2],
        scale: f64,
    ) -> Point2 {
        let points: Vec<Point2> = points
            .iter()
            .map(|p| Point2::new(p.x * scale, p.y * scale))
            .collect();
        let tables = self.drawing().tables();
        let extent = drawn
            .iter()
            .filter_map(|id| self.drawing().entity(*id))
            .filter(|e| !e.is_unbounded())
            .map(|e| e.bounds(tables))
            .chain(points.iter().map(|p| cad_core::Aabb::from_points(*p, *p)))
            .reduce(|a, b| a.union(&b))
            .expect("a chain has points");
        let unit = crate::compose::sheet_of(self.drawing()).map_or(1.0, |s| s.frame.units_per_mm);
        let off = 10.0 * unit;
        // Over the first dimension, between its two points: DIMLINEAR
        // turns by where the line's point is, and a point off to one side —
        // the middle of a long chain — made it vertical, measuring nothing,
        // and DIMCONTINUE refused the rest (#358).
        let mid = Point2::new(
            (points[0].x + points[1].x) / 2.0,
            (points[0].y + points[1].y) / 2.0,
        );
        match side {
            cad_intent::ChainSide::Below => Point2::new(mid.x, extent.min().y - off),
            cad_intent::ChainSide::Above => Point2::new(mid.x, extent.max().y + off),
            cad_intent::ChainSide::Left => Point2::new(extent.min().x - off, mid.y),
            cad_intent::ChainSide::Right => Point2::new(extent.max().x + off, mid.y),
        }
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
    fn pick_point(
        &self,
        operation: usize,
        pick: &Pick,
        scale: f64,
        offset: Vec2,
    ) -> Result<Point2, RunError> {
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
                let near = Point2::new(near[0] * scale, near[1] * scale) + offset;
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
        // Resolved by the session, which knows where the view went and what
        // it has drawn.
        Answer::InView { point, .. } => {
            format!("{},{}", number(point.x * scale), number(point.y * scale))
        }
        Answer::Staging(_) => "0,0".to_owned(),
        Answer::Beyond { points, .. } => points.first().map_or_else(String::new, |p| {
            format!("{},{}", number(p.x * scale), number(p.y * scale))
        }),
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
