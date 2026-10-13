//! SELECT and QSELECT (#185): a selection made at the idle prompt with every
//! method a "Select objects" prompt offers, and one made from what the
//! objects are — their kind and their properties.

use cad_document::{Color, Drawing, Linetype, Lineweight};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::modify::{SELECT_OBJECTS, selection_expected};
use crate::summary::{kind_index, kind_named, kind_names};
use crate::{Command, CommandContext, CommandInput, Prompt, SelectionMethod, Step};

/// SELECT: builds a selection — by clicking, by window or crossing, or
/// with the keywords every selection prompt offers — and keeps it for the
/// next command.
#[derive(Debug)]
pub(crate) struct SelectCommand;

impl Command for SelectCommand {
    fn start(&mut self, _ctx: &mut CommandContext<'_>) -> Step {
        Step::Continue
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
const PROPERTIES: [Keyword; 4] = [LAYER, COLOR, LTYPE, LWEIGHT];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Property {
    Layer,
    Color,
    Linetype,
    Lineweight,
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

/// QSELECT: the objects of a kind, with a property that is, is not,
/// is less than, or is more than a value. The result is the selection.
#[derive(Debug)]
pub(crate) struct QselectCommand {
    state: State,
    /// The kind, as an index into the summary's list; `None` for every kind.
    kind: Option<usize>,
}

impl QselectCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Kind,
            kind: None,
        }
    }

    fn select(&self, ctx: &mut CommandContext<'_>, test: Option<Test>) -> Step {
        let drawing = &*ctx.drawing;
        let ids: Vec<_> = drawing
            .shown_in(ctx.space)
            .filter(|(id, entity)| {
                !drawing.is_locked(*id)
                    && self.kind.is_none_or(|kind| kind_index(entity) == kind)
                    && test.as_ref().is_none_or(|test| test.holds(drawing, *id))
            })
            .map(|(id, _)| id)
            .collect();
        ctx.selection.clear();
        for id in &ids {
            ctx.selection.add(*id);
        }
        ctx.selection.set_method(SelectionMethod::QuickSelect);
        ctx.message(format!("{} found.", ids.len()));
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
            State::Property => Prompt::new("Property to test").with_default("none"),
            State::Value(Property::Layer) => Prompt::new("Layer name, after = or <>"),
            State::Value(Property::Color) => Prompt::new("Colour, after =, <>, <, or >"),
            State::Value(Property::Linetype) => Prompt::new("Linetype name, after = or <>"),
            State::Value(Property::Lineweight) => Prompt::new("Lineweight, after =, <>, <, or >"),
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
            (State::Property, CommandInput::Accept) => return self.select(ctx, None),
            (State::Property, CommandInput::Keyword(keyword)) => {
                self.state = State::Value(match keyword {
                    LAYER => Property::Layer,
                    COLOR => Property::Color,
                    LTYPE => Property::Linetype,
                    _ => Property::Lineweight,
                });
            }
            (State::Value(property), CommandInput::Text(text)) => {
                match Test::parse(*property, &text, ctx.drawing) {
                    Ok(test) => return self.select(ctx, Some(test)),
                    Err(message) => ctx.message(message),
                }
            }
            _ => {}
        }
        Step::Continue
    }
}
