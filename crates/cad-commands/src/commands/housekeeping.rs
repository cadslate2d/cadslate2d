//! Housekeeping (#200): `MATCHPROP` copies one object's properties onto
//! others, `PURGE` sweeps the unused rows out of the tables, and `RENAME`
//! renames a row of any table. Each is one transaction through the
//! document's builders, which hold the rules: what is permanent, what is
//! current, what is in use, and what a name may be.

use cad_core::EntityId;
use cad_document::{Entity, Properties, PurgeScope, Purged, TransactionError};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::modify::selection_expected;
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

// ── MATCHPROP ──

const SETTINGS: Keyword = Keyword::new("Settings");
const LAYER: Keyword = Keyword::new("Layer");
const COLOR: Keyword = Keyword::new("Color");
const LTYPE: Keyword = Keyword::new("LType");
const LWEIGHT: Keyword = Keyword::new("LWeight");
const TEXT: Keyword = Keyword::new("Text");
const DIMENSION: Keyword = Keyword::new("Dimension");
const ALL: Keyword = Keyword::new("ALL");
const MATCHED: [Keyword; 7] = [LAYER, COLOR, LTYPE, LWEIGHT, TEXT, DIMENSION, ALL];

/// Which of the source's properties are copied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Matched {
    layer: bool,
    color: bool,
    linetype: bool,
    lineweight: bool,
    /// A text's style and height, where both are texts.
    text: bool,
    /// A dimension's style, where both are dimensions.
    dimension: bool,
}

impl Matched {
    const ALL: Self = Self {
        layer: true,
        color: true,
        linetype: true,
        lineweight: true,
        text: true,
        dimension: true,
    };

    fn describe(self) -> String {
        let on: Vec<&str> = [
            (self.layer, "layer"),
            (self.color, "colour"),
            (self.linetype, "linetype"),
            (self.lineweight, "lineweight"),
            (self.text, "text style and height"),
            (self.dimension, "dimension style"),
        ]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
        if on.is_empty() {
            "nothing".to_owned()
        } else {
            on.join(", ")
        }
    }
}

#[derive(Debug)]
enum MatchState {
    Source,
    Destinations,
    Settings,
}

/// `MATCHPROP` (`MA`): a source object, then the objects to make like it.
#[derive(Debug)]
pub(crate) struct MatchpropCommand {
    state: MatchState,
    source: Option<EntityId>,
    matched: Matched,
}

impl MatchpropCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: MatchState::Source,
            source: None,
            matched: Matched::ALL,
        }
    }

    /// `entity` made like `source` in what is matched: the properties,
    /// and the geometry where a text or dimension style is copied.
    fn made_like(
        matched: Matched,
        source: &Entity,
        from: Properties,
        entity: &Entity,
        to: Properties,
    ) -> (Option<Entity>, Properties) {
        let properties = Properties {
            layer: if matched.layer { from.layer } else { to.layer },
            color: if matched.color { from.color } else { to.color },
            linetype: if matched.linetype {
                from.linetype
            } else {
                to.linetype
            },
            lineweight: if matched.lineweight {
                from.lineweight
            } else {
                to.lineweight
            },
        };
        let text_of = |entity: &Entity| match entity {
            Entity::Text(text) => Some((text.style, text.height)),
            Entity::MText(text) => Some((text.style, text.height)),
            Entity::AttributeDefinition(definition) => {
                Some((definition.text.style, definition.text.height))
            }
            _ => None,
        };
        let entity = match (entity, matched.text.then(|| text_of(source)).flatten()) {
            (Entity::Text(text), Some((style, height))) => {
                Some(Entity::Text(cad_document::TextEntity {
                    style,
                    height,
                    ..text.clone()
                }))
            }
            (Entity::MText(text), Some((style, height))) => {
                Some(Entity::MText(cad_document::MTextEntity {
                    style,
                    height,
                    ..text.clone()
                }))
            }
            (Entity::AttributeDefinition(definition), Some((style, height))) => {
                let mut definition = definition.clone();
                definition.text.style = style;
                definition.text.height = height;
                Some(Entity::AttributeDefinition(definition))
            }
            (Entity::Dimension(dimension), _) if matched.dimension => match source {
                Entity::Dimension(from) if from.style != dimension.style => {
                    Some(Entity::Dimension(cad_document::DimensionEntity {
                        style: from.style,
                        ..dimension.clone()
                    }))
                }
                _ => None,
            },
            _ => None,
        };
        (entity, properties)
    }

    fn apply(&self, ctx: &mut CommandContext<'_>) -> Step {
        let Some(source) = self.source else {
            return Step::Finished;
        };
        let (Some(from_entity), Some(from)) = (
            ctx.drawing.entity(source).cloned(),
            ctx.drawing.properties(source),
        ) else {
            ctx.message("The source object is gone.");
            return Step::Finished;
        };
        let destinations: Vec<EntityId> = ctx.selection.iter().filter(|id| *id != source).collect();
        if destinations.is_empty() {
            ctx.message("No objects selected.");
            return Step::Finished;
        }
        let records: Vec<(EntityId, Entity, Properties)> = destinations
            .iter()
            .filter_map(|id| {
                Some((
                    *id,
                    ctx.drawing.entity(*id)?.clone(),
                    ctx.drawing.properties(*id)?,
                ))
            })
            .collect();
        let mut changed = 0;
        let mut tx = ctx.transaction();
        for (id, entity, to) in &records {
            let (geometry, properties) =
                Self::made_like(self.matched, &from_entity, from, entity, *to);
            let outcome = match geometry {
                Some(geometry) => tx.replace_with(*id, geometry, properties),
                None if properties != *to => tx.set_properties(*id, properties),
                None => continue,
            };
            if outcome.is_ok() {
                changed += 1;
            }
        }
        tx.commit();
        ctx.selection.clear();
        ctx.message(format!(
            "{} matched: {}.",
            if changed == 1 {
                "1 object".to_owned()
            } else {
                format!("{changed} objects")
            },
            self.matched.describe()
        ));
        Step::Finished
    }
}

impl Command for MatchpropCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        // The source is asked for first, whatever was selected.
        ctx.selection.clear();
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            MatchState::Source => Prompt::new("Select source object"),
            MatchState::Destinations => Prompt::new("Select destination objects"),
            MatchState::Settings => Prompt::new("Switch a setting, or Enter when done"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            MatchState::Source => ExpectedInput::new(InputKind::Object),
            MatchState::Destinations => selection_expected().with_keywords(&[SETTINGS]),
            MatchState::Settings => ExpectedInput::new(InputKind::Nothing).with_keywords(&MATCHED),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (MatchState::Source, CommandInput::Pick { entity, .. }) => {
                self.source = Some(entity);
                ctx.selection.clear();
                self.state = MatchState::Destinations;
            }
            (MatchState::Source, CommandInput::Accept) => return Step::Finished,
            (MatchState::Destinations, CommandInput::Keyword(_)) => {
                self.state = MatchState::Settings;
            }
            (MatchState::Destinations, CommandInput::Accept) => return self.apply(ctx),
            (MatchState::Settings, CommandInput::Keyword(keyword)) => {
                let m = &mut self.matched;
                match keyword {
                    LAYER => m.layer = !m.layer,
                    COLOR => m.color = !m.color,
                    LTYPE => m.linetype = !m.linetype,
                    LWEIGHT => m.lineweight = !m.lineweight,
                    TEXT => m.text = !m.text,
                    DIMENSION => m.dimension = !m.dimension,
                    _ => *m = Matched::ALL,
                }
                ctx.message(format!("Matching: {}.", self.matched.describe()));
            }
            (MatchState::Settings, CommandInput::Accept) => self.state = MatchState::Destinations,
            _ => {}
        }
        Step::Continue
    }
}

// ── PURGE ──

const BLOCKS: Keyword = Keyword::new("Blocks");
const DIMSTYLES: Keyword = Keyword::new("Dimstyles");
const LAYERS: Keyword = Keyword::new("LAyers");
const LTYPES: Keyword = Keyword::new("LTypes");
const STYLES: Keyword = Keyword::new("STyles");
const EVERYTHING: Keyword = Keyword::new("All");
const LIST: Keyword = Keyword::new("?");
const PURGEABLE: [Keyword; 7] = [BLOCKS, DIMSTYLES, LAYERS, LTYPES, STYLES, EVERYTHING, LIST];

/// `PURGE` (`PU`): every unused row of one table, or of all, swept out
/// as one change; `?` says what would go.
#[derive(Debug, Default)]
pub(crate) struct PurgeCommand;

/// "2 layers (A, B), 1 linetype (Center)".
fn describe_purged(purged: &Purged) -> String {
    let part = |rows: &[String], one: &str, many: &str| {
        (!rows.is_empty()).then(|| {
            format!(
                "{} {} ({})",
                rows.len(),
                if rows.len() == 1 { one } else { many },
                rows.join(", ")
            )
        })
    };
    [
        part(&purged.blocks, "block", "blocks"),
        part(&purged.dim_styles, "dimension style", "dimension styles"),
        part(&purged.layers, "layer", "layers"),
        part(&purged.linetypes, "linetype", "linetypes"),
        part(&purged.text_styles, "text style", "text styles"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ")
}

fn scope_of(keyword: Keyword) -> PurgeScope {
    match keyword {
        BLOCKS => PurgeScope {
            blocks: true,
            ..PurgeScope::NONE
        },
        DIMSTYLES => PurgeScope {
            dim_styles: true,
            ..PurgeScope::NONE
        },
        LAYERS => PurgeScope {
            layers: true,
            ..PurgeScope::NONE
        },
        LTYPES => PurgeScope {
            linetypes: true,
            ..PurgeScope::NONE
        },
        STYLES => PurgeScope {
            text_styles: true,
            ..PurgeScope::NONE
        },
        _ => PurgeScope::ALL,
    }
}

impl Command for PurgeCommand {
    fn prompt(&self) -> Prompt {
        Prompt::new("Enter type of unused objects to purge").with_default("All")
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::new(InputKind::Nothing).with_keywords(&PURGEABLE)
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let scope = match input {
            CommandInput::Keyword(LIST) => {
                // A dry run: a transaction abandoned says what would go.
                let mut tx = ctx.transaction();
                let would = tx.purge(PurgeScope::ALL);
                drop(tx);
                ctx.message(if would.is_empty() {
                    "Nothing to purge.".to_owned()
                } else {
                    format!("Unused: {}.", describe_purged(&would))
                });
                return Step::Continue;
            }
            CommandInput::Keyword(keyword) => scope_of(keyword),
            CommandInput::Accept => PurgeScope::ALL,
            _ => return Step::Continue,
        };
        let mut tx = ctx.transaction();
        let purged = tx.purge(scope);
        tx.commit();
        ctx.message(if purged.is_empty() {
            "Nothing to purge.".to_owned()
        } else {
            format!("Purged {}.", describe_purged(&purged))
        });
        Step::Finished
    }
}

// ── RENAME ──

const BLOCK: Keyword = Keyword::new("Block");
const DIMSTYLE: Keyword = Keyword::new("Dimstyle");
const LAYER_ROW: Keyword = Keyword::new("LAyer");
const LTYPE_ROW: Keyword = Keyword::new("LType");
const STYLE: Keyword = Keyword::new("Style");
const LAYOUT: Keyword = Keyword::new("LAYOUT");
const RENAMEABLE: [Keyword; 6] = [BLOCK, DIMSTYLE, LAYER_ROW, LTYPE_ROW, STYLE, LAYOUT];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableKind {
    Block,
    DimStyle,
    Layer,
    Linetype,
    TextStyle,
    Layout,
}

impl TableKind {
    fn of(keyword: Keyword) -> Self {
        match keyword {
            BLOCK => Self::Block,
            DIMSTYLE => Self::DimStyle,
            LAYER_ROW => Self::Layer,
            LTYPE_ROW => Self::Linetype,
            STYLE => Self::TextStyle,
            _ => Self::Layout,
        }
    }

    fn noun(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::DimStyle => "dimension style",
            Self::Layer => "layer",
            Self::Linetype => "linetype",
            Self::TextStyle => "text style",
            Self::Layout => "layout",
        }
    }

    /// Whether a row by this name exists.
    fn exists(self, ctx: &CommandContext<'_>, name: &str) -> bool {
        let drawing = &*ctx.drawing;
        match self {
            Self::Block => drawing.blocks().find(name).is_some(),
            Self::DimStyle => drawing.dim_styles().find(name).is_some(),
            Self::Layer => drawing.layers().find(name).is_some(),
            Self::Linetype => drawing.linetypes().find(name).is_some(),
            Self::TextStyle => drawing.text_styles().find(name).is_some(),
            Self::Layout => drawing.layouts().find(name).is_some(),
        }
    }

    /// Renames the row `old` to `new`, through the builder of its table.
    fn rename(
        self,
        ctx: &mut CommandContext<'_>,
        old: &str,
        new: &str,
    ) -> Result<(), TransactionError> {
        let drawing = &*ctx.drawing;
        let found = match self {
            Self::Block => drawing.blocks().find(old).map(|id| id.raw()),
            Self::DimStyle => drawing.dim_styles().find(old).map(|id| id.raw()),
            Self::Layer => drawing.layers().find(old).map(|id| id.raw()),
            Self::Linetype => drawing.linetypes().find(old).map(|id| id.raw()),
            Self::TextStyle => drawing.text_styles().find(old).map(|id| id.raw()),
            Self::Layout => drawing.layouts().find(old).map(|id| id.raw()),
        };
        let raw = found.expect("the name was checked to exist");
        let mut tx = ctx.drawing.transaction();
        match self {
            Self::Block => tx.rename_block(cad_document::BlockId::from_raw(raw), new)?,
            Self::DimStyle => tx.rename_dim_style(cad_document::DimStyleId::from_raw(raw), new)?,
            Self::Layer => tx.rename_layer(cad_document::LayerId::from_raw(raw), new)?,
            Self::Linetype => tx.rename_linetype(cad_document::LinetypeId::from_raw(raw), new)?,
            Self::TextStyle => {
                tx.rename_text_style(cad_document::TextStyleId::from_raw(raw), new)?
            }
            Self::Layout => tx.rename_layout(cad_document::LayoutId::from_raw(raw), new)?,
        }
        tx.commit();
        Ok(())
    }

    /// Why the rename was refused, in the user's terms.
    fn explain(self, error: TransactionError, new: &str) -> String {
        use TransactionError as E;
        match error {
            E::PermanentLayer
            | E::PermanentLinetype
            | E::PermanentTextStyle
            | E::PermanentDimStyle
            | E::PermanentBlock
            | E::PermanentLayout => format!("That {} cannot be renamed.", self.noun()),
            E::DuplicateLayerName(_)
            | E::DuplicateLinetypeName(_)
            | E::DuplicateTextStyleName(_)
            | E::DuplicateDimStyleName(_)
            | E::DuplicateBlockName(_)
            | E::DuplicateLayoutName(_) => {
                format!("There is already a {} named \"{new}\".", self.noun())
            }
            E::BadLayerName(error)
            | E::BadTextStyleName(error)
            | E::BadDimStyleName(error)
            | E::BadBlockName(error)
            | E::BadLayoutName(error) => error.explain(self.noun()),
            other => format!("{other}."),
        }
    }
}

#[derive(Debug)]
enum RenameState {
    Kind,
    Old(TableKind),
    New(TableKind, String),
}

/// `RENAME` (`REN`): a row of any table, by its old and new names.
#[derive(Debug)]
pub(crate) struct RenameCommand {
    state: RenameState,
}

impl RenameCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: RenameState::Kind,
        }
    }
}

impl Command for RenameCommand {
    fn prompt(&self) -> Prompt {
        match &self.state {
            RenameState::Kind => Prompt::new("Enter object type to rename"),
            RenameState::Old(kind) => Prompt::new(format!("Enter old {} name", kind.noun())),
            RenameState::New(kind, _) => Prompt::new(format!("Enter new {} name", kind.noun())),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            RenameState::Kind => ExpectedInput::new(InputKind::Nothing).with_keywords(&RENAMEABLE),
            _ => ExpectedInput::text(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (RenameState::Kind, CommandInput::Keyword(keyword)) => {
                self.state = RenameState::Old(TableKind::of(keyword));
            }
            (RenameState::Old(kind), CommandInput::Text(old)) => {
                let old = old.trim().to_owned();
                if kind.exists(ctx, &old) {
                    self.state = RenameState::New(*kind, old);
                } else {
                    ctx.message(format!("There is no {} \"{old}\".", kind.noun()));
                }
            }
            (RenameState::New(kind, old), CommandInput::Text(new)) => {
                let (kind, old) = (*kind, old.clone());
                let new = new.trim().to_owned();
                match kind.rename(ctx, &old, &new) {
                    Ok(()) => {
                        let mut noun = kind.noun().to_owned();
                        noun[..1].make_ascii_uppercase();
                        ctx.message(format!("{noun} \"{old}\" renamed \"{new}\"."));
                        return Step::Finished;
                    }
                    Err(error) => ctx.message(kind.explain(error, &new)),
                }
            }
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }
}
