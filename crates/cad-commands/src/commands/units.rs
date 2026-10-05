//! UNITS: says what the drawing's numbers are numbers of (DESIGN §14).
//!
//! The unit is a drawing setting, changed in a transaction and undone like
//! anything else. It rescales nothing: geometry is unit-independent, and
//! the unit is what a typed `25mm` is converted into.

use cad_core::LengthUnit;
use cad_input::{ExpectedInput, InputKind, Keyword};

use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const UNITLESS: Keyword = Keyword::new("Unitless");
/// One keyword per unit: its suffix, in capitals, so that it is typed in
/// full and `M` is metres, not the start of `MM`.
const UNITS: [(Keyword, LengthUnit); 5] = [
    (Keyword::new("MM"), LengthUnit::Millimetres),
    (Keyword::new("CM"), LengthUnit::Centimetres),
    (Keyword::new("M"), LengthUnit::Metres),
    (Keyword::new("IN"), LengthUnit::Inches),
    (Keyword::new("FT"), LengthUnit::Feet),
];

#[derive(Debug)]
pub(crate) struct UnitsCommand {
    /// The unit when the command started, shown as the default.
    current: Option<LengthUnit>,
}

impl UnitsCommand {
    pub(crate) fn new() -> Self {
        Self { current: None }
    }
}

fn keyword_of(unit: Option<LengthUnit>) -> Keyword {
    UNITS
        .iter()
        .find(|(_, candidate)| Some(*candidate) == unit)
        .map_or(UNITLESS, |(keyword, _)| *keyword)
}

impl Command for UnitsCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.current = ctx.drawing.unit();
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Drawing unit?").with_default(keyword_of(self.current).name())
    }

    fn expected(&self) -> ExpectedInput {
        let mut keywords = vec![UNITLESS];
        keywords.extend(UNITS.iter().map(|(keyword, _)| *keyword));
        ExpectedInput::new(InputKind::Nothing).with_keywords(&keywords)
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let CommandInput::Keyword(keyword) = input else {
            // Enter keeps the unit there is.
            return Step::Finished;
        };
        let unit = UNITS
            .iter()
            .find(|(candidate, _)| *candidate == keyword)
            .map(|(_, unit)| *unit);
        if unit != self.current {
            let mut transaction = ctx.drawing.transaction();
            transaction.set_unit(unit);
            transaction.commit();
        }
        ctx.message(match unit {
            Some(unit) => format!(
                "The drawing is in {}. Nothing was rescaled; a typed length such as 25mm is now converted.",
                unit.name()
            ),
            None => "The drawing is unitless: lengths are plain numbers.".to_owned(),
        });
        Step::Finished
    }
}
