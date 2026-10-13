//! ATTDEF and ATTEDIT (#183): an attribute definition — a block's variable
//! text — made like a text with a tag, a prompt, and a default; and the
//! values of one reference's attributes changed, each asked for with its
//! definition's prompt and the value it has.

use cad_core::Point2;
use cad_document::{AttributeDefinitionEntity, BlockReferenceEntity, Entity, TextEntity};
use cad_input::{ExpectedInput, InputKind};

use crate::commands::block::attribute_prompt;
use crate::{Command, CommandContext, CommandInput, HoveredObject, OverlayItem, Prompt, Step};

/// The height offered when none has been given yet, as TEXT offers it.
const DEFAULT_HEIGHT: f64 = 2.5;

#[derive(Debug)]
enum DefineState {
    Tag,
    Prompt {
        tag: String,
    },
    Default {
        tag: String,
        prompt: String,
    },
    Start {
        tag: String,
        prompt: String,
        default: String,
    },
    Height {
        tag: String,
        prompt: String,
        default: String,
        start: Point2,
    },
    Rotation {
        tag: String,
        prompt: String,
        default: String,
        start: Point2,
        height: f64,
    },
}

#[derive(Debug)]
pub(crate) struct AttdefCommand {
    state: DefineState,
}

impl AttdefCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: DefineState::Tag,
        }
    }
}

fn display(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

impl Command for AttdefCommand {
    fn prompt(&self) -> Prompt {
        match &self.state {
            DefineState::Tag => Prompt::new("Enter attribute tag name"),
            DefineState::Prompt { .. } => {
                Prompt::new("Enter attribute prompt").with_default("none")
            }
            DefineState::Default { .. } => {
                Prompt::new("Enter default attribute value").with_default("none")
            }
            DefineState::Start { .. } => Prompt::new("Specify start point"),
            DefineState::Height { .. } => {
                Prompt::new("Specify height").with_default(display(DEFAULT_HEIGHT))
            }
            DefineState::Rotation { .. } => Prompt::new("Specify rotation angle").with_default("0"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            DefineState::Tag | DefineState::Prompt { .. } | DefineState::Default { .. } => {
                ExpectedInput::text()
            }
            DefineState::Start { .. } => ExpectedInput::point(),
            DefineState::Height { .. } => ExpectedInput::distance(),
            DefineState::Rotation { .. } => ExpectedInput::new(InputKind::Angle),
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            DefineState::Height { start, .. } | DefineState::Rotation { start, .. } => Some(*start),
            _ => None,
        }
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let DefineState::Height { tag, start, .. } = &self.state {
            let height = cursor.distance_to(*start);
            if height > 0.0 {
                out.push(OverlayItem::PreviewEntity(Entity::Text(TextEntity::new(
                    *start,
                    height,
                    0.0,
                    tag.clone(),
                ))));
            }
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, DefineState::Tag);
        self.state = match (state, input) {
            (DefineState::Tag, CommandInput::Text(tag)) => {
                // A tag is one word: classic drafting writes spaces as underscores.
                let tag = tag.trim().replace(' ', "_").to_uppercase();
                if tag.is_empty() {
                    ctx.message("A tag needs a name.");
                    DefineState::Tag
                } else {
                    DefineState::Prompt { tag }
                }
            }
            (DefineState::Tag, CommandInput::Accept) => return Step::Finished,
            (DefineState::Prompt { tag }, CommandInput::Text(prompt)) => {
                DefineState::Default { tag, prompt }
            }
            (DefineState::Prompt { tag }, CommandInput::Accept) => DefineState::Default {
                tag,
                prompt: String::new(),
            },
            (DefineState::Default { tag, prompt }, CommandInput::Text(default)) => {
                DefineState::Start {
                    tag,
                    prompt,
                    default,
                }
            }
            (DefineState::Default { tag, prompt }, CommandInput::Accept) => DefineState::Start {
                tag,
                prompt,
                default: String::new(),
            },
            (
                DefineState::Start {
                    tag,
                    prompt,
                    default,
                },
                CommandInput::Point(start),
            ) => DefineState::Height {
                tag,
                prompt,
                default,
                start,
            },
            (
                DefineState::Height {
                    tag,
                    prompt,
                    default,
                    start,
                },
                input,
            ) => {
                let height = match input {
                    CommandInput::Distance(height) => height,
                    CommandInput::Accept => DEFAULT_HEIGHT,
                    _ => {
                        self.state = DefineState::Height {
                            tag,
                            prompt,
                            default,
                            start,
                        };
                        return Step::Continue;
                    }
                };
                if !(height.is_finite() && height > 0.0) {
                    ctx.message("The height must be a positive length.");
                    self.state = DefineState::Height {
                        tag,
                        prompt,
                        default,
                        start,
                    };
                    return Step::Continue;
                }
                DefineState::Rotation {
                    tag,
                    prompt,
                    default,
                    start,
                    height,
                }
            }
            (
                DefineState::Rotation {
                    tag,
                    prompt,
                    default,
                    start,
                    height,
                },
                input,
            ) => {
                let degrees = match input {
                    CommandInput::Angle(degrees) => degrees,
                    CommandInput::Accept => 0.0,
                    _ => {
                        self.state = DefineState::Rotation {
                            tag,
                            prompt,
                            default,
                            start,
                            height,
                        };
                        return Step::Continue;
                    }
                };
                let text = TextEntity::new(start, height, degrees.to_radians(), default);
                let definition = AttributeDefinitionEntity::new(tag, prompt, text);
                let mut transaction = ctx.transaction();
                transaction
                    .insert(definition)
                    .expect("an attribute definition has the standard style");
                transaction.commit();
                return Step::Finished;
            }
            (state, _) => state,
        };
        Step::Continue
    }
}

#[derive(Debug)]
enum EditState {
    Pick,
    Values {
        id: cad_core::EntityId,
        reference: BlockReferenceEntity,
        asked: Vec<AttributeDefinitionEntity>,
        index: usize,
    },
}

#[derive(Debug)]
pub(crate) struct AtteditCommand {
    state: EditState,
}

impl AtteditCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: EditState::Pick,
        }
    }
}

impl Command for AtteditCommand {
    fn prompt(&self) -> Prompt {
        match &self.state {
            EditState::Pick => Prompt::new("Select block reference"),
            EditState::Values {
                reference,
                asked,
                index,
                ..
            } => attribute_prompt(&asked[*index], reference.attribute(&asked[*index].tag)),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            EditState::Pick => ExpectedInput::object(),
            EditState::Values { .. } => ExpectedInput::text(),
        }
    }

    fn preview_pick(&self, hovered: &HoveredObject<'_>, out: &mut Vec<OverlayItem>) {
        if matches!(hovered.entity, Entity::BlockRef(_)) {
            out.push(OverlayItem::Highlight(hovered.id));
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, EditState::Pick);
        match (state, input) {
            (EditState::Pick, CommandInput::Pick { entity: id, .. }) => {
                let Some(Entity::BlockRef(reference)) = ctx.drawing.entity(id).cloned() else {
                    ctx.message("That is not a block reference.");
                    return Step::Continue;
                };
                let asked: Vec<AttributeDefinitionEntity> = ctx
                    .drawing
                    .blocks()
                    .get(reference.block)
                    .map(|block| block.attribute_definitions().cloned().collect())
                    .unwrap_or_default();
                if asked.is_empty() {
                    ctx.message("That block has no attributes.");
                    return Step::Continue;
                }
                self.state = EditState::Values {
                    id,
                    reference,
                    asked,
                    index: 0,
                };
                Step::Continue
            }
            (EditState::Pick, CommandInput::Accept) => Step::Finished,
            (EditState::Pick, _) => Step::Continue,
            (
                EditState::Values {
                    id,
                    reference,
                    asked,
                    index,
                },
                input,
            ) => {
                let definition = &asked[index];
                let reference = match input {
                    CommandInput::Text(value) => reference.with_attribute(&definition.tag, &value),
                    // Enter keeps the value it has, or takes the default.
                    CommandInput::Accept => {
                        let value = reference
                            .attribute(&definition.tag)
                            .unwrap_or(&definition.text.content)
                            .to_owned();
                        reference.with_attribute(&definition.tag, &value)
                    }
                    _ => {
                        self.state = EditState::Values {
                            id,
                            reference,
                            asked,
                            index,
                        };
                        return Step::Continue;
                    }
                };
                if index + 1 < asked.len() {
                    self.state = EditState::Values {
                        id,
                        reference,
                        asked,
                        index: index + 1,
                    };
                    return Step::Continue;
                }
                let mut transaction = ctx.transaction();
                if transaction.replace(id, reference).is_err() {
                    drop(transaction);
                    ctx.message("The reference is gone.");
                    return Step::Finished;
                }
                transaction.commit();
                Step::Finished
            }
        }
    }
}
