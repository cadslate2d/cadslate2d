//! WIPEOUT (#226): a polygon that masks what is beneath it.
//!
//! The corners are picked, Enter closing them, or `Polyline` takes a
//! closed polyline of straight segments, which may then be erased.
//! `Frames` shows or hides every wipeout's frame: a drawing setting, one
//! undo. A wipeout is made over everything the drawing holds
//! (`TransactionBuilder::insert_on_top`), so what is drawn next — the note
//! — lies over it (ADR-0032).

use cad_core::Point2;
use cad_document::{Entity, WipeoutEntity};
use cad_input::{ExpectedInput, Keyword};

use super::keywords::UNDO;
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const FRAMES: Keyword = Keyword::new("Frames");
const POLYLINE: Keyword = Keyword::new("Polyline");
const OPTIONS: [Keyword; 2] = [FRAMES, POLYLINE];
const ON: Keyword = Keyword::new("ON");
const OFF: Keyword = Keyword::new("OFF");
const YES: Keyword = Keyword::new("Yes");
const NO: Keyword = Keyword::new("No");

#[derive(Debug)]
enum State {
    /// The first corner, or an option.
    Start,
    /// The next corner; Enter closes the polygon.
    Next(Vec<Point2>),
    Frames,
    Polyline,
    /// Whether to erase the polyline a wipeout was made from.
    Erase(cad_core::EntityId),
}

#[derive(Debug)]
pub(crate) struct WipeoutCommand {
    state: State,
}

impl WipeoutCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Start,
        }
    }

    /// Adds the wipeout over everything, or says why there is none.
    fn make(ctx: &mut CommandContext<'_>, corners: Vec<Point2>) -> bool {
        match WipeoutEntity::new(corners) {
            Ok(wipeout) => {
                let mut transaction = ctx.transaction();
                transaction
                    .insert_on_top(wipeout)
                    .expect("a wipeout of finite corners");
                transaction.commit();
                true
            }
            Err(error) => {
                ctx.message(format!("No wipeout: {error}."));
                false
            }
        }
    }
}

impl Command for WipeoutCommand {
    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Start => Prompt::new("Specify first point").with_default("Polyline"),
            State::Next(points) if points.len() < 3 => Prompt::new("Specify next point"),
            State::Next(_) => Prompt::new("Specify next point").with_default("close"),
            State::Frames => Prompt::new("Enter mode"),
            State::Polyline => Prompt::new("Select a closed polyline"),
            State::Erase(_) => Prompt::new("Erase polyline?").with_default("No"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Start => ExpectedInput::point().with_keywords(&OPTIONS),
            State::Next(points) if points.len() > 1 => {
                ExpectedInput::point().with_keywords(&[UNDO])
            }
            State::Next(_) => ExpectedInput::point(),
            State::Frames => {
                ExpectedInput::new(cad_input::InputKind::Nothing).with_keywords(&[ON, OFF])
            }
            State::Polyline => ExpectedInput::object(),
            State::Erase(_) => {
                ExpectedInput::new(cad_input::InputKind::Nothing).with_keywords(&[YES, NO])
            }
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, State::Start);
        self.state = match (state, input) {
            (State::Start, CommandInput::Point(point)) => State::Next(vec![point]),
            (State::Start, CommandInput::Keyword(FRAMES)) => State::Frames,
            (State::Start, CommandInput::Keyword(_) | CommandInput::Accept) => State::Polyline,
            (State::Next(mut points), CommandInput::Point(point)) => {
                points.push(point);
                State::Next(points)
            }
            (State::Next(mut points), CommandInput::Keyword(_)) => {
                points.pop();
                State::Next(points)
            }
            (State::Next(points), CommandInput::Accept) if points.len() >= 3 => {
                if Self::make(ctx, points.clone()) {
                    return Step::Finished;
                }
                State::Next(points)
            }
            (State::Next(points), CommandInput::Accept) => {
                ctx.message("A wipeout needs three points at least.");
                State::Next(points)
            }
            (State::Frames, CommandInput::Keyword(keyword)) => {
                let shown = keyword == ON;
                let mut transaction = ctx.transaction();
                transaction.set_wipeout_frame(shown);
                transaction.commit();
                ctx.message(if shown {
                    "Wipeout frames are shown."
                } else {
                    "Wipeout frames are hidden."
                });
                return Step::Finished;
            }
            (State::Polyline, CommandInput::Pick { entity, .. }) => {
                match ctx.drawing.entity(entity) {
                    Some(Entity::Polyline(polyline)) if !polyline.closed => {
                        ctx.message("The polyline is open: a wipeout needs a closed one.");
                        State::Polyline
                    }
                    Some(Entity::Polyline(polyline)) if polyline.has_arcs() => {
                        ctx.message("A wipeout's edges are straight: the polyline has arcs.");
                        State::Polyline
                    }
                    Some(Entity::Polyline(polyline)) => {
                        if Self::make(ctx, polyline.vertices.clone()) {
                            State::Erase(entity)
                        } else {
                            State::Polyline
                        }
                    }
                    Some(other) => {
                        ctx.message(format!(
                            "A {} is not a polyline: pick a closed polyline.",
                            other.kind_name()
                        ));
                        State::Polyline
                    }
                    None => State::Polyline,
                }
            }
            (State::Polyline, CommandInput::Accept) => return Step::Finished,
            (State::Erase(polyline), CommandInput::Keyword(YES)) => {
                let mut transaction = ctx.transaction();
                transaction
                    .remove(polyline)
                    .expect("the polyline just picked");
                transaction.commit();
                return Step::Finished;
            }
            (State::Erase(_), CommandInput::Keyword(_) | CommandInput::Accept) => {
                return Step::Finished;
            }
            (state, _) => state,
        };
        Step::Continue
    }

    /// The polygon as it would close at the pointer.
    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        let State::Next(points) = &self.state else {
            return;
        };
        let mut ring = points.clone();
        ring.push(cursor);
        let n = ring.len();
        for i in 0..n {
            if n == 2 && i == 1 {
                break;
            }
            out.push(OverlayItem::PreviewSegment {
                start: ring[i],
                end: ring[(i + 1) % n],
            });
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            State::Next(points) => points.last().copied(),
            _ => None,
        }
    }
}
