//! TEXT and TEXTEDIT: a single line of text, and changing what a text says
//! (DESIGN §11, §32; ADR-0016).
//!
//! TEXT asks for the start point — or a justification first — the height,
//! the rotation, and then the text itself, typed with the text input kind,
//! so spaces are part of it. TEXTEDIT picks a text and asks for its new
//! content; Enter keeps the old. Picked a paragraph, it opens the host's
//! editor on the old words instead (see `mtext.rs`).

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;
use cad_document::{Entity, Justification, TextEntity};
use cad_input::{ExpectedInput, Keyword};

use super::ltscale::display;
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const JUSTIFY: Keyword = Keyword::new("Justify");
const LEFT: Keyword = Keyword::new("Left");
const CENTER: Keyword = Keyword::new("Center");
const RIGHT: Keyword = Keyword::new("Right");
const JUSTIFICATIONS: [Keyword; 3] = [LEFT, CENTER, RIGHT];
/// The height offered when none has been given yet.
const DEFAULT_HEIGHT: f64 = 2.5;

#[derive(Debug)]
enum State {
    Start,
    Justification,
    Height {
        start: Point2,
    },
    Rotation {
        start: Point2,
        height: f64,
    },
    Content {
        start: Point2,
        height: f64,
        rotation: f64,
    },
}

#[derive(Debug)]
pub(crate) struct TextCommand {
    state: State,
    justification: Justification,
}

impl TextCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Start,
            justification: Justification::Left,
        }
    }

    fn text(&self, start: Point2, height: f64, rotation: f64, content: &str) -> TextEntity {
        TextEntity::new(start, height, rotation, content).justified(self.justification)
    }
}

impl Command for TextCommand {
    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Start => Prompt::new("Specify start point of text"),
            State::Justification => Prompt::new("Enter justification"),
            State::Height { .. } => {
                Prompt::new("Specify height").with_default(display(DEFAULT_HEIGHT))
            }
            State::Rotation { .. } => {
                Prompt::new("Specify rotation angle of text").with_default("0")
            }
            State::Content { .. } => Prompt::new("Enter text"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Start => ExpectedInput::point().with_keywords(&[JUSTIFY]),
            State::Justification => {
                ExpectedInput::new(cad_input::InputKind::Nothing).with_keywords(&JUSTIFICATIONS)
            }
            State::Height { .. } => ExpectedInput::distance(),
            State::Rotation { .. } => ExpectedInput::angle(),
            State::Content { .. } => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Start, CommandInput::Point(start)) => self.state = State::Height { start },
            (State::Start, CommandInput::Keyword(_)) => self.state = State::Justification,
            (State::Start, CommandInput::Accept) => return Step::Finished,
            (State::Justification, CommandInput::Keyword(keyword)) => {
                self.justification = match keyword {
                    CENTER => Justification::Center,
                    RIGHT => Justification::Right,
                    _ => Justification::Left,
                };
                self.state = State::Start;
            }
            (State::Justification, CommandInput::Accept) => self.state = State::Start,
            (State::Height { start }, CommandInput::Distance(height)) => {
                if !height.is_finite() || LinearTolerance::DEFAULT.is_zero(height) || height < 0.0 {
                    ctx.message("The height must be greater than zero.");
                    return Step::Continue;
                }
                self.state = State::Rotation {
                    start: *start,
                    height,
                };
            }
            (State::Height { start }, CommandInput::Accept) => {
                self.state = State::Rotation {
                    start: *start,
                    height: DEFAULT_HEIGHT,
                };
            }
            (State::Rotation { start, height }, CommandInput::Angle(degrees)) => {
                self.state = State::Content {
                    start: *start,
                    height: *height,
                    rotation: degrees.to_radians(),
                };
            }
            (State::Rotation { start, height }, CommandInput::Accept) => {
                self.state = State::Content {
                    start: *start,
                    height: *height,
                    rotation: 0.0,
                };
            }
            (
                State::Content {
                    start,
                    height,
                    rotation,
                },
                CommandInput::Text(content),
            ) => {
                let text = self.text(*start, *height, *rotation, &content);
                let mut transaction = ctx.transaction();
                transaction
                    .insert(text)
                    .expect("a new text has the standard style");
                transaction.commit();
                return Step::Finished;
            }
            // Enter with nothing typed: no text.
            (State::Content { .. }, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            State::Height { start } | State::Rotation { start, .. } => Some(*start),
            _ => None,
        }
    }

    /// The box the text will fill, as the height and rotation are chosen.
    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        let sample = match &self.state {
            State::Height { start } => {
                let height = start.distance_to(cursor);
                (height > 0.0).then(|| self.text(*start, height, 0.0, "Text"))
            }
            State::Rotation { start, height } => {
                let offset = cursor - *start;
                let rotation = if offset.length() > 0.0 {
                    offset.y.atan2(offset.x)
                } else {
                    0.0
                };
                Some(self.text(*start, *height, rotation, "Text"))
            }
            _ => None,
        };
        if let Some(text) = sample {
            out.push(OverlayItem::PreviewEntity(Entity::Text(text)));
        }
    }
}

// ── TEXTEDIT ──

#[derive(Debug)]
enum EditState {
    Pick,
    /// A single-line text: the new words are typed at the prompt.
    Content {
        id: cad_core::EntityId,
        old: String,
    },
    /// A paragraph: the host's editor opens with the old words.
    Paragraph {
        id: cad_core::EntityId,
        old: String,
    },
    /// A leader's note (#205): the new words are typed at the prompt.
    Note {
        id: cad_core::EntityId,
        old: String,
    },
}

#[derive(Debug)]
pub(crate) struct TextEditCommand {
    state: EditState,
}

impl TextEditCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: EditState::Pick,
        }
    }
}

impl Command for TextEditCommand {
    fn prompt(&self) -> Prompt {
        match &self.state {
            EditState::Pick => Prompt::new("Select text to edit"),
            EditState::Content { old, .. } => {
                Prompt::new("Enter new text").with_default(old.clone())
            }
            EditState::Paragraph { .. } => Prompt::new("Edit the text in the editor"),
            EditState::Note { old, .. } => {
                Prompt::new("Enter new leader text").with_default(old.clone())
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            EditState::Pick => ExpectedInput::object(),
            EditState::Content { .. } | EditState::Paragraph { .. } | EditState::Note { .. } => {
                ExpectedInput::text()
            }
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (EditState::Pick, CommandInput::Pick { entity, .. }) => {
                match ctx.drawing.entity(entity) {
                    Some(Entity::Text(text)) => {
                        self.state = EditState::Content {
                            id: entity,
                            old: text.content.clone(),
                        };
                    }
                    Some(Entity::MText(text)) => {
                        self.state = EditState::Paragraph {
                            id: entity,
                            old: text.content.clone(),
                        };
                    }
                    // A leader's note is edited like a text (#205); an
                    // empty note takes the note away.
                    Some(Entity::Leader(leader)) => {
                        self.state = EditState::Note {
                            id: entity,
                            old: leader.text.clone().unwrap_or_default(),
                        };
                    }
                    Some(other) => {
                        ctx.message(format!("That is a {}, not a text.", other.kind_name()));
                    }
                    None => {}
                }
            }
            (EditState::Pick, CommandInput::Accept) => return Step::Finished,
            (EditState::Content { id, old }, CommandInput::Text(content)) => {
                if content != *old
                    && let Some(Entity::Text(text)) = ctx.drawing.entity(*id)
                {
                    let edited = text.saying(content);
                    let mut transaction = ctx.transaction();
                    transaction
                        .replace(*id, edited)
                        .expect("the picked text is live and keeps its style");
                    transaction.commit();
                }
                return Step::Finished;
            }
            (EditState::Paragraph { id, old }, CommandInput::Text(content)) => {
                if content != *old
                    && !content.trim().is_empty()
                    && let Some(Entity::MText(text)) = ctx.drawing.entity(*id)
                {
                    let edited = text.saying(content);
                    let mut transaction = ctx.transaction();
                    transaction
                        .replace(*id, edited)
                        .expect("the picked paragraph is live and keeps its style");
                    transaction.commit();
                }
                return Step::Finished;
            }
            (EditState::Note { id, old }, CommandInput::Text(content)) => {
                if content != *old
                    && let Some(Entity::Leader(leader)) = ctx.drawing.entity(*id)
                {
                    let edited = leader.clone().saying(Some(content));
                    let mut transaction = ctx.transaction();
                    transaction
                        .replace(*id, edited)
                        .expect("the picked leader is live and keeps its style");
                    transaction.commit();
                }
                return Step::Finished;
            }
            (
                EditState::Content { .. } | EditState::Paragraph { .. } | EditState::Note { .. },
                CommandInput::Accept,
            ) => {
                return Step::Finished;
            }
            _ => {}
        }
        Step::Continue
    }

    fn editor(&self) -> Option<&str> {
        match &self.state {
            EditState::Paragraph { old, .. } => Some(old),
            _ => None,
        }
    }
}
