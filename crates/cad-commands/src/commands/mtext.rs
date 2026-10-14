//! MTEXT: a paragraph of text (DESIGN §5, §11, §32; ADR-0016).
//!
//! Two corners give the paragraph its top left and its width; then the
//! command waits for the words. The host opens its paragraph editor — a
//! narrow, task-specific one (§5) — and what the editor submits arrives
//! through the command line as text with its line breaks, exactly as if it
//! had been typed; headless, it is typed.

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;
use cad_document::{Entity, MTextEntity, PolylineEntity};
use cad_input::{ExpectedInput, Keyword};

use super::ltscale::display;
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const HEIGHT: Keyword = Keyword::new("Height");
const DEFAULT_HEIGHT: f64 = 2.5;

#[derive(Debug)]
enum State {
    FirstCorner,
    Height { first: Option<Point2> },
    OppositeCorner { first: Point2 },
    Words { insertion: Point2, width: f64 },
}

#[derive(Debug)]
pub(crate) struct MTextCommand {
    state: State,
    height: f64,
    /// The current text style as the command found it (#221).
    style: cad_document::TextStyleId,
}

impl MTextCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::FirstCorner,
            height: DEFAULT_HEIGHT,
            style: cad_document::TextStyleId::FIRST,
        }
    }

    /// The paragraph two corners define: the top left is the upper of the
    /// two on the left, and the width is the distance across.
    fn placed(first: Point2, opposite: Point2) -> (Point2, f64) {
        let insertion = Point2::new(first.x.min(opposite.x), first.y.max(opposite.y));
        (insertion, (opposite.x - first.x).abs())
    }
}

impl Command for MTextCommand {
    /// The paragraph takes the current text style, and its fixed height
    /// as the height offered, when it has one (#221).
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.style = ctx.drawing.current_text_style();
        if let Some(style) = ctx.drawing.text_styles().get(self.style)
            && style.height > 0.0
        {
            self.height = style.height;
        }
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match &self.state {
            State::FirstCorner => Prompt::new("Specify first corner"),
            State::Height { .. } => {
                Prompt::new("Specify height").with_default(display(self.height))
            }
            State::OppositeCorner { .. } => Prompt::new("Specify opposite corner"),
            State::Words { .. } => Prompt::new("Enter text in the editor"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::FirstCorner => ExpectedInput::point().with_keywords(&[HEIGHT]),
            State::Height { .. } => ExpectedInput::distance(),
            State::OppositeCorner { .. } => ExpectedInput::point().with_keywords(&[HEIGHT]),
            State::Words { .. } => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::FirstCorner, CommandInput::Point(first)) => {
                self.state = State::OppositeCorner { first };
            }
            (State::FirstCorner, CommandInput::Keyword(_)) => {
                self.state = State::Height { first: None };
            }
            (State::OppositeCorner { first }, CommandInput::Keyword(_)) => {
                self.state = State::Height {
                    first: Some(*first),
                };
            }
            (State::Height { first }, CommandInput::Distance(height)) => {
                if !height.is_finite() || LinearTolerance::DEFAULT.is_zero(height) || height < 0.0 {
                    ctx.message("The height must be greater than zero.");
                    return Step::Continue;
                }
                self.height = height;
                self.state = match first {
                    Some(first) => State::OppositeCorner { first: *first },
                    None => State::FirstCorner,
                };
            }
            (State::Height { first }, CommandInput::Accept) => {
                self.state = match first {
                    Some(first) => State::OppositeCorner { first: *first },
                    None => State::FirstCorner,
                };
            }
            (State::OppositeCorner { first }, CommandInput::Point(opposite)) => {
                let (insertion, width) = Self::placed(*first, opposite);
                self.state = State::Words { insertion, width };
            }
            (State::Words { insertion, width }, CommandInput::Text(content)) => {
                if content.trim().is_empty() {
                    return Step::Finished;
                }
                let text = MTextEntity {
                    style: self.style,
                    ..MTextEntity::new(*insertion, *width, self.height, 0.0, content)
                };
                let mut transaction = ctx.transaction();
                transaction
                    .insert(text)
                    .expect("a new paragraph has the current style, which exists");
                transaction.commit();
                return Step::Finished;
            }
            // Enter with nothing written: no paragraph.
            (State::Words { .. }, CommandInput::Accept) => return Step::Finished,
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            State::OppositeCorner { first } => Some(*first),
            _ => None,
        }
    }

    /// The corners of a window are places on the screen.
    fn takes_object_snap(&self) -> bool {
        !matches!(self.state, State::OppositeCorner { .. })
    }

    /// The box the paragraph will fill, with a sample line in it.
    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let State::OppositeCorner { first } = &self.state {
            let (insertion, width) = Self::placed(*first, cursor);
            out.push(OverlayItem::PreviewEntity(Entity::Polyline(
                PolylineEntity::new(
                    vec![
                        *first,
                        Point2::new(cursor.x, first.y),
                        cursor,
                        Point2::new(first.x, cursor.y),
                    ],
                    true,
                ),
            )));
            if width > 0.0 {
                out.push(OverlayItem::PreviewEntity(Entity::MText(MTextEntity::new(
                    insertion,
                    width,
                    self.height,
                    0.0,
                    "Text",
                ))));
            }
        }
    }

    fn editor(&self) -> Option<&str> {
        matches!(self.state, State::Words { .. }).then_some("")
    }
}
