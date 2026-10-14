//! SELECT and QSELECT (#185): a selection made at the idle prompt with every
//! method a "Select objects" prompt offers, and one made from what the
//! objects are — their kind and their properties.

use cad_document::{Color, Drawing, Entity, Linetype, Lineweight};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::modify::{SELECT_OBJECTS, selection_expected};
use crate::summary::{kind_index, kind_named, kind_names};
use crate::{Command, CommandContext, CommandInput, Prompt, SelectionMethod, Step};

/// SELECT: builds a selection — by clicking, by window or crossing, or
/// with the keywords every selection prompt offers — and keeps it for the
/// next command. Started by a method's name at the idle prompt — `FENCE`,
/// `WPOLYGON`, `CPOLYGON`, `PREVIOUS`, `LAST`, `ALL`, `REMOVE` — it begins
/// with that keyword given (#207).
#[derive(Debug, Default)]
pub(crate) struct SelectCommand {
    opening: Option<Keyword>,
}

impl SelectCommand {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// SELECT with `keyword` — one of the selection keywords — given first.
    pub(crate) fn opening_with(keyword: Keyword) -> Self {
        Self {
            opening: Some(keyword),
        }
    }
}

impl Command for SelectCommand {
    fn start(&mut self, _ctx: &mut CommandContext<'_>) -> Step {
        Step::Continue
    }

    fn take_opening_keyword(&mut self) -> Option<Keyword> {
        self.opening.take()
    }

    fn prompt(&self) -> Prompt {
        Prompt::new(SELECT_OBJECTS)
    }

    fn expected(&self) -> ExpectedInput {
        selection_expected()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match input {
            CommandInput::Accept => {
                ctx.message(format!("{} selected.", ctx.selection.len()));
                Step::Finished
            }
            _ => Step::Continue,
        }
    }
}

const LAYER: Keyword = Keyword::new("Layer");
const COLOR: Keyword = Keyword::new("Color");
const LTYPE: Keyword = Keyword::new("LType");
const LWEIGHT: Keyword = Keyword::new("LWeight");
const STYLE: Keyword = Keyword::new("Style");
const DIMSTYLE: Keyword = Keyword::new("DIMstyle");
const BLOCK: Keyword = Keyword::new("Block");
const APPEND: Keyword = Keyword::new("Append");
const REMOVE: Keyword = Keyword::new("Remove");
const DONE: Keyword = Keyword::new("Done");
const PROPERTIES: [Keyword; 10] = [
    LAYER, COLOR, LTYPE, LWEIGHT, STYLE, DIMSTYLE, BLOCK, APPEND, REMOVE, DONE,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Property {
    Layer,
    Color,
    Linetype,
    Lineweight,
    /// A text's, a paragraph's, or an attribute definition's text style.
    Style,
    /// A dimension's or a leader's dimension style.
    DimStyle,
    /// A block reference's block.
    Block,
}

/// What the objects found do to the selection (#207).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// They are the selection.
    Replace,
    /// They join it.
    Append,
    /// They leave it.
    Remove,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operator {
    Equal,
    NotEqual,
    Less,
    Greater,
}

impl Operator {
    /// The operator the text begins with — `=`, `<>`, `<`, `>` — and the
    /// rest; no operator means equal.
    fn split(text: &str) -> (Self, &str) {
        let text = text.trim();
        for (prefix, operator) in [
            ("<>", Self::NotEqual),
            ("!=", Self::NotEqual),
            ("≠", Self::NotEqual),
            ("<", Self::Less),
            (">", Self::Greater),
            ("=", Self::Equal),
        ] {
            if let Some(rest) = text.strip_prefix(prefix) {
                return (operator, rest.trim());
            }
        }
        (Self::Equal, text)
    }

    fn holds<T: PartialOrd>(self, actual: T, wanted: T) -> bool {
        match self {
            Self::Equal => actual == wanted,
            Self::NotEqual => actual != wanted,
            Self::Less => actual < wanted,
            Self::Greater => actual > wanted,
        }
    }

    fn orders(self) -> bool {
        matches!(self, Self::Less | Self::Greater)
    }
}

/// One condition on a property, as parsed.
#[derive(Clone, Debug, PartialEq)]
enum Test {
    Layer(Operator, String),
    Color(Operator, Color),
    Linetype(Operator, Linetype),
    Lineweight(Operator, Lineweight),
    /// A row of a table, by name: a text style, a dimension style, a block.
    Named(Property, Operator, String),
}

impl Test {
    fn parse(property: Property, text: &str, drawing: &Drawing) -> Result<Self, String> {
        let (operator, value) = Operator::split(text);
        match property {
            Property::Layer => {
                if operator.orders() {
                    return Err("A layer is tested with = or <>.".to_owned());
                }
                let name = value.trim();
                if drawing.layers().find(name).is_none() {
                    return Err(format!("No layer named \"{name}\"."));
                }
                Ok(Self::Layer(operator, name.to_owned()))
            }
            Property::Color => {
                let color = Color::parse(value)
                    .ok_or_else(|| "A colour is 1-255, a name, ByLayer, or ByBlock.".to_owned())?;
                if operator.orders() && !matches!(color, Color::Index(_)) {
                    return Err("< and > compare colour numbers.".to_owned());
                }
                Ok(Self::Color(operator, color))
            }
            Property::Linetype => {
                if operator.orders() {
                    return Err("A linetype is tested with = or <>.".to_owned());
                }
                let linetype = Linetype::parse(value, drawing.linetypes())
                    .ok_or_else(|| format!("No linetype named \"{}\".", value.trim()))?;
                Ok(Self::Linetype(operator, linetype))
            }
            Property::Style | Property::DimStyle | Property::Block => {
                let (noun, exists) = match property {
                    Property::Style => ("text style", drawing.text_styles().find(value).is_some()),
                    Property::DimStyle => (
                        "dimension style",
                        drawing.dim_styles().find(value).is_some(),
                    ),
                    _ => ("block", drawing.blocks().find(value).is_some()),
                };
                if operator.orders() {
                    return Err(format!("A {noun} is tested with = or <>."));
                }
                if !exists {
                    return Err(format!("No {noun} named \"{}\".", value.trim()));
                }
                Ok(Self::Named(property, operator, value.trim().to_owned()))
            }
            Property::Lineweight => {
                let lineweight = Lineweight::parse(value).ok_or_else(|| {
                    "A lineweight is a standard width in millimetres, Default, ByLayer, or ByBlock."
                        .to_owned()
                })?;
                if operator.orders() && !matches!(lineweight, Lineweight::Hundredths(_)) {
                    return Err("< and > compare widths in millimetres.".to_owned());
                }
                Ok(Self::Lineweight(operator, lineweight))
            }
        }
    }

    fn holds(&self, drawing: &Drawing, id: cad_core::EntityId) -> bool {
        let Some(properties) = drawing.properties(id) else {
            return false;
        };
        match self {
            Self::Layer(operator, name) => {
                let actual = drawing.layer_of(id).map_or("", |layer| layer.name.as_str());
                let equal = actual.eq_ignore_ascii_case(name);
                operator.holds(equal, true)
            }
            Self::Color(operator, wanted) => match (operator.orders(), properties.color, wanted) {
                (true, Color::Index(actual), Color::Index(wanted)) => {
                    operator.holds(actual, *wanted)
                }
                (true, ..) => false,
                (false, actual, wanted) => operator.holds(actual == *wanted, true),
            },
            Self::Linetype(operator, wanted) => {
                operator.holds(properties.linetype == *wanted, true)
            }
            // Only an object that has the property is tested by it.
            Self::Named(property, operator, name) => {
                let Some(entity) = drawing.entity(id) else {
                    return false;
                };
                let actual = match (property, entity) {
                    (Property::Style, Entity::Text(text)) => drawing
                        .text_styles()
                        .get(text.style)
                        .map(|row| row.name.as_str()),
                    (Property::Style, Entity::MText(text)) => drawing
                        .text_styles()
                        .get(text.style)
                        .map(|row| row.name.as_str()),
                    (Property::Style, Entity::AttributeDefinition(definition)) => drawing
                        .text_styles()
                        .get(definition.text.style)
                        .map(|row| row.name.as_str()),
                    (Property::DimStyle, Entity::Dimension(dimension)) => drawing
                        .dim_styles()
                        .get(dimension.style)
                        .map(|row| row.name.as_str()),
                    (Property::DimStyle, Entity::Leader(leader)) => drawing
                        .dim_styles()
                        .get(leader.style)
                        .map(|row| row.name.as_str()),
                    (Property::Block, Entity::BlockRef(reference)) => drawing
                        .blocks()
                        .get(reference.block)
                        .map(|row| row.name.as_str()),
                    _ => None,
                };
                actual.is_some_and(|actual| operator.holds(actual.eq_ignore_ascii_case(name), true))
            }
            Self::Lineweight(operator, wanted) => {
                match (operator.orders(), properties.lineweight, wanted) {
                    (true, Lineweight::Hundredths(actual), Lineweight::Hundredths(wanted)) => {
                        operator.holds(actual, *wanted)
                    }
                    (true, ..) => false,
                    (false, actual, wanted) => operator.holds(actual == *wanted, true),
                }
            }
        }
    }
}

#[derive(Debug)]
enum State {
    Kind,
    Property,
    Value(Property),
}

/// QSELECT: the objects of a kind whose properties pass every test given
/// — a layer, colour, linetype, lineweight, text style, dimension style,
/// or block that is, is not, is less than, or is more than a value — and
/// what they do to the selection: replace it, join it, or leave it.
#[derive(Debug)]
pub(crate) struct QselectCommand {
    state: State,
    /// The kind, as an index into the summary's list; `None` for every kind.
    kind: Option<usize>,
    tests: Vec<Test>,
    mode: Mode,
}

impl QselectCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Kind,
            kind: None,
            tests: Vec::new(),
            mode: Mode::Replace,
        }
    }

    /// The shown, unlocked objects of the kind that pass every test.
    fn matching(&self, ctx: &CommandContext<'_>) -> Vec<cad_core::EntityId> {
        let drawing = &*ctx.drawing;
        drawing
            .shown_in(ctx.space)
            .filter(|(id, entity)| {
                !drawing.is_locked(*id)
                    && self.kind.is_none_or(|kind| kind_index(entity) == kind)
                    && self.tests.iter().all(|test| test.holds(drawing, *id))
            })
            .map(|(id, _)| id)
            .collect()
    }

    fn select(&self, ctx: &mut CommandContext<'_>) -> Step {
        let ids = self.matching(ctx);
        match self.mode {
            Mode::Replace => {
                ctx.selection.clear();
                for id in &ids {
                    ctx.selection.add(*id);
                }
            }
            Mode::Append => {
                for id in &ids {
                    ctx.selection.add(*id);
                }
            }
            Mode::Remove => {
                for id in &ids {
                    ctx.selection.remove(*id);
                }
            }
        }
        ctx.selection.set_method(SelectionMethod::QuickSelect);
        ctx.message(match self.mode {
            Mode::Replace => format!("{} found.", ids.len()),
            Mode::Append => format!(
                "{} found and added: {} selected.",
                ids.len(),
                ctx.selection.len()
            ),
            Mode::Remove => format!(
                "{} found and removed: {} selected.",
                ids.len(),
                ctx.selection.len()
            ),
        });
        Step::Finished
    }
}

impl Command for QselectCommand {
    fn start(&mut self, _ctx: &mut CommandContext<'_>) -> Step {
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Kind => Prompt::new("Object kind (line, circle, text, …)").with_default("all"),
            State::Property if self.tests.is_empty() => {
                Prompt::new("Property to test").with_default("none")
            }
            State::Property => Prompt::new("Another property to test").with_default("done"),
            State::Value(Property::Layer) => Prompt::new("Layer name, after = or <>"),
            State::Value(Property::Color) => Prompt::new("Colour, after =, <>, <, or >"),
            State::Value(Property::Linetype) => Prompt::new("Linetype name, after = or <>"),
            State::Value(Property::Lineweight) => Prompt::new("Lineweight, after =, <>, <, or >"),
            State::Value(Property::Style) => Prompt::new("Text style name, after = or <>"),
            State::Value(Property::DimStyle) => Prompt::new("Dimension style name, after = or <>"),
            State::Value(Property::Block) => Prompt::new("Block name, after = or <>"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Kind | State::Value(_) => ExpectedInput::text(),
            State::Property => ExpectedInput::new(InputKind::Nothing).with_keywords(&PROPERTIES),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Kind, CommandInput::Accept) => self.state = State::Property,
            (State::Kind, CommandInput::Text(text)) => {
                let text = text.trim();
                if text.is_empty() || text.eq_ignore_ascii_case("all") {
                    self.kind = None;
                } else if let Some(kind) = kind_named(text) {
                    self.kind = Some(kind);
                } else {
                    ctx.message(format!(
                        "No kind named \"{text}\". The kinds are {}.",
                        kind_names().join(", ")
                    ));
                    return Step::Continue;
                }
                self.state = State::Property;
            }
            (State::Property, CommandInput::Accept | CommandInput::Keyword(DONE)) => {
                return self.select(ctx);
            }
            (State::Property, CommandInput::Keyword(APPEND)) => {
                self.mode = Mode::Append;
                ctx.message("What is found will be added to the selection.");
            }
            (State::Property, CommandInput::Keyword(REMOVE)) => {
                self.mode = Mode::Remove;
                ctx.message("What is found will be taken from the selection.");
            }
            (State::Property, CommandInput::Keyword(keyword)) => {
                self.state = State::Value(match keyword {
                    LAYER => Property::Layer,
                    COLOR => Property::Color,
                    LTYPE => Property::Linetype,
                    STYLE => Property::Style,
                    DIMSTYLE => Property::DimStyle,
                    BLOCK => Property::Block,
                    _ => Property::Lineweight,
                });
            }
            (State::Value(property), CommandInput::Text(text)) => {
                match Test::parse(*property, &text, ctx.drawing) {
                    Ok(test) => {
                        self.tests.push(test);
                        let count = self.matching(ctx).len();
                        ctx.message(format!("{count} match so far."));
                        self.state = State::Property;
                    }
                    Err(message) => ctx.message(message),
                }
            }
            (State::Value(_), CommandInput::Accept) => self.state = State::Property,
            _ => {}
        }
        Step::Continue
    }
}
