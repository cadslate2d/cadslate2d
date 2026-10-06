//! What every dimension command shares (#164, #165): the `Text` and
//! `Angle` options — words in place of the measured value, and an angle
//! for the text — and the one transaction that commits a dimension. A
//! command holds a [`TextOptions`], asks it first at each prompt, and
//! applies it to the dimension it makes.

use cad_document::DimensionEntity;
use cad_input::{ExpectedInput, Keyword};

use crate::{CommandContext, CommandInput, Prompt, Step};

pub(super) const TEXT: Keyword = Keyword::new("Text");
pub(super) const ANGLE: Keyword = Keyword::new("Angle");
/// The options of a command that offers nothing else.
pub(super) const OPTIONS: [Keyword; 2] = [TEXT, ANGLE];

#[derive(Debug)]
enum Asking {
    Nothing,
    /// The words; the measured text is the default.
    Text {
        measured: String,
    },
    Angle,
}

/// The drafter's text and text angle for the dimension being made, and
/// the prompt open for one of them.
#[derive(Debug)]
pub(super) struct TextOptions {
    pub text: Option<String>,
    pub angle: Option<f64>,
    asking: Asking,
}

impl TextOptions {
    pub(super) fn new() -> Self {
        Self {
            text: None,
            angle: None,
            asking: Asking::Nothing,
        }
    }

    /// While an option's own prompt is open: what it asks.
    pub(super) fn prompt(&self) -> Option<Prompt> {
        match &self.asking {
            Asking::Nothing => None,
            Asking::Text { measured } => {
                Some(Prompt::new("Enter dimension text").with_default(measured.clone()))
            }
            Asking::Angle => Some(Prompt::new("Specify angle of dimension text").with_default("0")),
        }
    }

    /// While an option's own prompt is open: what it accepts.
    pub(super) fn expected(&self) -> Option<ExpectedInput> {
        match &self.asking {
            Asking::Nothing => None,
            Asking::Text { .. } => Some(ExpectedInput::text()),
            Asking::Angle => Some(ExpectedInput::angle()),
        }
    }

    /// Opens the prompt `keyword` names, when it is one of these options;
    /// `measured` is what the text would say, the default for `Text`.
    pub(super) fn open(&mut self, keyword: Keyword, measured: impl FnOnce() -> String) -> bool {
        match keyword {
            TEXT => {
                self.asking = Asking::Text {
                    measured: measured(),
                };
            }
            ANGLE => self.asking = Asking::Angle,
            _ => return false,
        }
        true
    }

    /// Takes an input while an option's prompt is open: `true` when it
    /// was taken. Words or an angle set the option and Enter clears it;
    /// either closes the prompt. Anything else is ignored and the prompt
    /// stays open.
    pub(super) fn step(&mut self, input: &CommandInput) -> bool {
        match (&self.asking, input) {
            (Asking::Nothing, _) => return false,
            (Asking::Text { .. }, CommandInput::Text(words)) => self.text = Some(words.clone()),
            (Asking::Text { .. }, CommandInput::Accept) => self.text = None,
            (Asking::Angle, CommandInput::Angle(degrees)) => {
                self.angle = Some(degrees.to_radians());
            }
            (Asking::Angle, CommandInput::Accept) => self.angle = None,
            _ => return true,
        }
        self.asking = Asking::Nothing;
        true
    }

    /// The dimension with these options applied.
    pub(super) fn apply(&self, dimension: DimensionEntity) -> DimensionEntity {
        dimension.saying(self.text.clone()).turned(self.angle)
    }

    /// What `dimension`'s text would say without these options: the
    /// default offered for `Text`.
    pub(super) fn measured(dimension: &DimensionEntity, ctx: &CommandContext<'_>) -> String {
        let style = ctx.drawing.tables().dim_style(dimension.style);
        DimensionEntity {
            text_override: None,
            ..dimension.clone()
        }
        .content(style)
    }
}

/// Commits `dimension` as one transaction and finishes, or explains why
/// the drawing refused it and keeps asking.
pub(super) fn commit(dimension: DimensionEntity, ctx: &mut CommandContext<'_>) -> Step {
    let mut transaction = ctx.drawing.transaction();
    match transaction.insert(dimension) {
        Ok(_) => {
            transaction.commit();
            Step::Finished
        }
        Err(error) => {
            drop(transaction);
            ctx.message(error.to_string());
            Step::Continue
        }
    }
}
