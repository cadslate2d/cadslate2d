//! BLOCK, INSERT, EXPLODE: making, placing, and taking apart blocks
//! (DESIGN §11, §32; ADR-0017).
//!
//! BLOCK asks for a name, a base point, and the objects, which it replaces
//! with one reference to the new block; a name already taken asks before
//! the block is redefined. INSERT asks for a name, the insertion point, a
//! scale, and a rotation, previewing the reference at the pointer. EXPLODE
//! takes a selection apart one level; what comes apart into what is
//! decided by `Entity::exploded` in `cad-document`. Each is one
//! transaction.

use cad_core::{EntityId, Point2};
use cad_document::{
    Block, BlockId, BlockReferenceEntity, Entity, EntityRecord, ExplodeError, Properties,
    TransactionError, check_name,
};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::modify::{NOTHING_SELECTED, SELECT_OBJECTS, counted, selection_expected};
use crate::command::angle_degrees;
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const YES: Keyword = Keyword::new("Yes");
const NO: Keyword = Keyword::new("No");
const ENTER_NAME: &str = "Enter block name";
const BAD_SCALE_FACTOR: &str = "The scale factor must be greater than zero.";

/// The block called `name`, if there is one.
fn find(ctx: &CommandContext<'_>, name: &str) -> Option<BlockId> {
    ctx.drawing.blocks().find(name)
}

/// Why the block table refused a change, in the user's terms.
fn explain(ctx: &CommandContext<'_>, name: &str, error: TransactionError) -> String {
    let named = |id: BlockId| {
        ctx.drawing
            .blocks()
            .get(id)
            .map_or_else(|| id.to_string(), |block| block.name.clone())
    };
    match error {
        TransactionError::BadBlockName(error) => error.explain("block"),
        TransactionError::DuplicateBlockName(id) => {
            format!("There is already a block \"{}\".", named(id))
        }
        TransactionError::BlockCycle(_) => format!("Block \"{name}\" cannot contain itself."),
        TransactionError::PermanentBlock => {
            format!("\"{}\" is the drawing itself.", Block::MODEL_SPACE)
        }
        other => other.to_string(),
    }
}

// ── BLOCK ──

#[derive(Debug)]
enum BlockState {
    Name,
    /// The name is taken: redefine that block, or ask for another name.
    Confirm {
        id: BlockId,
        name: String,
    },
    BasePoint {
        /// The block to redefine, or none for a new one.
        id: Option<BlockId>,
        name: String,
    },
    Selecting {
        id: Option<BlockId>,
        name: String,
        base: Point2,
    },
}

#[derive(Debug)]
pub(crate) struct BlockCommand {
    state: BlockState,
}

impl BlockCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: BlockState::Name,
        }
    }

    /// Asks for the base point of `name`, which is `id` when it exists.
    fn take_name(&mut self, ctx: &mut CommandContext<'_>, name: String) {
        if let Err(error) = check_name(&name) {
            ctx.message(error.explain("block"));
            return;
        }
        self.state = match find(ctx, &name) {
            Some(id) => BlockState::Confirm { id, name },
            None => BlockState::BasePoint { id: None, name },
        };
    }

    /// With a base point: defines the block from the selection if there is
    /// one, or asks for the objects.
    fn take_base(&mut self, ctx: &mut CommandContext<'_>, base: Point2) -> Step {
        let (BlockState::BasePoint { id, name } | BlockState::Selecting { id, name, .. }) =
            &self.state
        else {
            return Step::Continue;
        };
        let (id, name) = (*id, name.clone());
        if ctx.selection.is_empty() {
            self.state = BlockState::Selecting { id, name, base };
            return Step::Continue;
        }
        Self::define(ctx, id, &name, base)
    }

    /// Makes the block out of the selection, which one reference to it
    /// replaces, in one transaction. A refusal changes nothing.
    fn define(ctx: &mut CommandContext<'_>, id: Option<BlockId>, name: &str, base: Point2) -> Step {
        let ids: Vec<EntityId> = ctx.selection.iter().collect();
        if ids.is_empty() {
            ctx.message(NOTHING_SELECTED);
            return Step::Finished;
        }
        let records: Vec<EntityRecord> = ids
            .iter()
            .filter_map(|id| {
                Some(EntityRecord::new(
                    ctx.drawing.entity(*id)?.clone(),
                    ctx.drawing.properties(*id)?,
                ))
            })
            .collect();
        let block = Block::new(name, base, records);

        let result = {
            let mut transaction = ctx.drawing.transaction();
            let defined = match id {
                Some(id) => transaction.redefine_block(id, block).map(|()| id),
                None => transaction.define_block(block),
            };
            defined.map(|id| {
                for entity in &ids {
                    transaction
                        .remove(*entity)
                        .expect("selected entities are live and appear once");
                }
                transaction
                    .insert(BlockReferenceEntity::new(id, base, 1.0, 0.0))
                    .expect("the block is defined in this transaction");
                transaction.commit();
            })
        };
        match result {
            Ok(()) => {
                let verb = if id.is_some() { "redefined" } else { "defined" };
                ctx.message(format!(
                    "Block \"{name}\" {verb} from {}.",
                    counted(ids.len(), "object")
                ));
                ctx.selection.clear();
            }
            Err(error) => {
                let message = explain(ctx, name, error);
                ctx.message(message);
            }
        }
        Step::Finished
    }
}

impl Command for BlockCommand {
    fn prompt(&self) -> Prompt {
        match &self.state {
            BlockState::Name => Prompt::new(ENTER_NAME),
            BlockState::Confirm { name, .. } => {
                Prompt::new(format!("Block \"{name}\" already exists. Redefine it?"))
                    .with_default("No")
            }
            BlockState::BasePoint { .. } => Prompt::new("Specify base point"),
            BlockState::Selecting { .. } => Prompt::new(SELECT_OBJECTS),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            BlockState::Name => ExpectedInput::text(),
            BlockState::Confirm { .. } => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&[YES, NO])
            }
            BlockState::BasePoint { .. } => ExpectedInput::point(),
            BlockState::Selecting { .. } => selection_expected(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (BlockState::Name, CommandInput::Text(name)) => self.take_name(ctx, name),
            (BlockState::Name, CommandInput::Accept) => return Step::Finished,
            (BlockState::Confirm { id, name }, CommandInput::Keyword(keyword))
                if keyword == YES =>
            {
                self.state = BlockState::BasePoint {
                    id: Some(*id),
                    name: name.clone(),
                };
            }
            // No, or Enter for the default: another name, then.
            (BlockState::Confirm { .. }, CommandInput::Keyword(_) | CommandInput::Accept) => {
                self.state = BlockState::Name;
            }
            (BlockState::BasePoint { .. }, CommandInput::Point(base)) => {
                return self.take_base(ctx, base);
            }
            (BlockState::BasePoint { .. }, CommandInput::Accept) => return Step::Finished,
            (BlockState::Selecting { id, name, base }, CommandInput::Accept) => {
                let (id, name, base) = (*id, name.clone(), *base);
                return Self::define(ctx, id, &name, base);
            }
            _ => {}
        }
        Step::Continue
    }
}

// ── INSERT ──

#[derive(Debug)]
enum InsertState {
    Name,
    Point {
        block: BlockId,
    },
    Scale {
        block: BlockId,
        insertion: Point2,
    },
    Rotation {
        block: BlockId,
        insertion: Point2,
        scale: f64,
    },
}

#[derive(Debug)]
pub(crate) struct InsertCommand {
    state: InsertState,
}

impl InsertCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: InsertState::Name,
        }
    }

    /// Lists the blocks there are to insert.
    fn list(ctx: &mut CommandContext<'_>) {
        let names: Vec<&str> = ctx
            .drawing
            .blocks()
            .iter()
            .filter(|(id, _)| *id != BlockId::FIRST)
            .map(|(_, block)| block.name.as_str())
            .collect();
        ctx.message(if names.is_empty() {
            "There are no blocks in this drawing.".to_owned()
        } else {
            format!("Blocks: {}.", names.join(", "))
        });
    }

    fn insert(
        ctx: &mut CommandContext<'_>,
        block: BlockId,
        insertion: Point2,
        scale: f64,
        degrees: f64,
    ) -> Step {
        let mut transaction = ctx.drawing.transaction();
        transaction
            .insert(BlockReferenceEntity::new(
                block,
                insertion,
                scale,
                degrees.to_radians(),
            ))
            .expect("the block was found and the scale checked");
        transaction.commit();
        Step::Finished
    }
}

fn is_scale(factor: f64) -> bool {
    factor.is_finite() && factor > 0.0
}

impl Command for InsertCommand {
    fn prompt(&self) -> Prompt {
        match &self.state {
            InsertState::Name => Prompt::new(ENTER_NAME),
            InsertState::Point { .. } => Prompt::new("Specify insertion point"),
            InsertState::Scale { .. } => Prompt::new("Specify scale factor").with_default("1"),
            InsertState::Rotation { .. } => Prompt::new("Specify rotation angle").with_default("0"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            InsertState::Name => ExpectedInput::text(),
            InsertState::Point { .. } => ExpectedInput::point(),
            // A factor is a plain number: `2mm` is not a scale.
            InsertState::Scale { .. } => ExpectedInput::factor(),
            InsertState::Rotation { .. } => ExpectedInput::angle(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (InsertState::Name, CommandInput::Text(name)) if name == "?" => Self::list(ctx),
            (InsertState::Name, CommandInput::Text(name)) => match find(ctx, &name) {
                Some(block) => self.state = InsertState::Point { block },
                None => ctx.message(format!("There is no block \"{name}\".")),
            },
            (InsertState::Name, CommandInput::Accept) => return Step::Finished,
            (InsertState::Point { block }, CommandInput::Point(insertion)) => {
                self.state = InsertState::Scale {
                    block: *block,
                    insertion,
                };
            }
            (InsertState::Point { .. }, CommandInput::Accept) => return Step::Finished,
            (InsertState::Scale { block, insertion }, CommandInput::Distance(scale)) => {
                if !is_scale(scale) {
                    ctx.message(BAD_SCALE_FACTOR);
                    return Step::Continue;
                }
                self.state = InsertState::Rotation {
                    block: *block,
                    insertion: *insertion,
                    scale,
                };
            }
            (InsertState::Scale { block, insertion }, CommandInput::Accept) => {
                self.state = InsertState::Rotation {
                    block: *block,
                    insertion: *insertion,
                    scale: 1.0,
                };
            }
            (
                InsertState::Rotation {
                    block,
                    insertion,
                    scale,
                },
                CommandInput::Angle(degrees),
            ) => return Self::insert(ctx, *block, *insertion, *scale, degrees),
            (
                InsertState::Rotation {
                    block,
                    insertion,
                    scale,
                },
                CommandInput::Accept,
            ) => return Self::insert(ctx, *block, *insertion, *scale, 0.0),
            _ => {}
        }
        Step::Continue
    }

    /// The reference as a click would place it: at the pointer, then at
    /// the insertion point with the scale the pointer's distance gives,
    /// then turned towards the pointer.
    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        let reference = match &self.state {
            InsertState::Name => return,
            InsertState::Point { block } => BlockReferenceEntity::new(*block, cursor, 1.0, 0.0),
            InsertState::Scale { block, insertion } => {
                out.push(OverlayItem::PreviewSegment {
                    start: *insertion,
                    end: cursor,
                });
                let scale = cursor.distance_to(*insertion);
                let scale = if is_scale(scale) { scale } else { 1.0 };
                BlockReferenceEntity::new(*block, *insertion, scale, 0.0)
            }
            InsertState::Rotation {
                block,
                insertion,
                scale,
            } => {
                out.push(OverlayItem::PreviewSegment {
                    start: *insertion,
                    end: cursor,
                });
                let degrees = angle_degrees(cursor - *insertion);
                BlockReferenceEntity::new(*block, *insertion, *scale, degrees.to_radians())
            }
        };
        out.push(OverlayItem::PreviewEntity(Entity::BlockRef(reference)));
    }

    fn base_point(&self) -> Option<Point2> {
        match &self.state {
            InsertState::Scale { insertion, .. } | InsertState::Rotation { insertion, .. } => {
                Some(*insertion)
            }
            InsertState::Name | InsertState::Point { .. } => None,
        }
    }
}

// ── EXPLODE ──

/// What an object comes apart into, or why it does not.
type Exploded = Result<Vec<(Entity, Properties)>, ExplodeError>;

#[derive(Debug)]
pub(crate) struct ExplodeCommand;

impl ExplodeCommand {
    /// Takes the selection apart, in one transaction; what has no parts is
    /// left as it is and named.
    fn explode(ctx: &mut CommandContext<'_>) -> Step {
        let ids: Vec<EntityId> = ctx.selection.iter().collect();
        if ids.is_empty() {
            ctx.message(NOTHING_SELECTED);
            return Step::Finished;
        }

        // Each selected object with its parts, or the kind that has none.
        let outcomes: Vec<(EntityId, &'static str, Exploded)> = ids
            .iter()
            .filter_map(|&id| {
                let entity = ctx.drawing.entity(id)?;
                let properties = ctx.drawing.properties(id)?;
                Some((
                    id,
                    entity.kind_name(),
                    entity.exploded(properties, ctx.drawing.tables()),
                ))
            })
            .collect();

        let mut exploded: Vec<&'static str> = Vec::new();
        let mut refused: Vec<&'static str> = Vec::new();
        let mut parts = 0;
        let mut transaction = ctx.drawing.transaction();
        for (id, kind, outcome) in outcomes {
            match outcome {
                Ok(pieces) => {
                    transaction
                        .remove(id)
                        .expect("selected entities are live and appear once");
                    for (entity, properties) in pieces {
                        transaction
                            .insert_with(entity, properties)
                            .expect("the parts of a live entity are on tables the drawing has");
                        parts += 1;
                    }
                    exploded.push(kind);
                }
                Err(ExplodeError::Unsupported(kind)) => {
                    if !refused.contains(&kind) {
                        refused.push(kind);
                    }
                }
            }
        }
        transaction.commit();

        if !exploded.is_empty() {
            let kind = exploded[0];
            let what = if exploded.iter().all(|other| *other == kind) {
                counted(exploded.len(), kind)
            } else {
                counted(exploded.len(), "object")
            };
            ctx.message(format!(
                "{what} exploded into {}.",
                counted(parts, "object")
            ));
        }
        for kind in refused {
            ctx.message(format!("A {kind} cannot be exploded."));
        }
        ctx.selection.clear();
        Step::Finished
    }
}

impl Command for ExplodeCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            Self::explode(ctx)
        }
    }

    fn prompt(&self) -> Prompt {
        Prompt::new(SELECT_OBJECTS)
    }

    fn expected(&self) -> ExpectedInput {
        selection_expected()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Accept => Self::explode(ctx),
            _ => Step::Continue,
        }
    }
}
