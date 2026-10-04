//! The command abstraction (DESIGN §15).
//!
//! Every command is an explicit state machine (State pattern). It consumes
//! typed and pointer input through the same [`Command::step`], so a point is a
//! point no matter where it came from, and a toolbar button can never bypass
//! the command architecture.

use std::fmt::Debug;

use cad_core::{EntityId, Point2, Vec2};
use cad_document::Drawing;
use cad_input::{ExpectedInput, Keyword};

use crate::log::CommandLog;
use crate::{DraftingSettings, HostRequest, OverlayItem, Selection};

/// One resolved input handed to the active command.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CommandInput {
    /// A world point — already resolved from typed coordinates or the pointer.
    Point(Point2),
    /// A length, typed or measured from the command's base point.
    Distance(f64),
    /// An angle in degrees, typed or measured from the command's base point.
    Angle(f64),
    /// One of the option keywords the command offered.
    Keyword(Keyword),
    /// Enter or Space on an empty line.
    Accept,
}

/// The angle, in degrees counter-clockwise from +X in `[0, 360)`, that a
/// point makes with a base point it is `offset` away from. This is how a
/// click is read where an angle is expected.
pub(crate) fn angle_degrees(offset: Vec2) -> f64 {
    offset.y.atan2(offset.x).to_degrees().rem_euclid(360.0)
}

/// What a command asks for, before formatting.
///
/// Commands never write `[Close/Undo]` or `<default>` by hand: the session
/// renders the option list from [`Command::expected`], so the brackets always
/// match what is actually accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    /// The request, without options or a trailing colon: `Specify next point`.
    pub message: String,
    /// What Enter will use, shown as `<…>`.
    pub default: Option<String>,
}

impl Prompt {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            default: None,
        }
    }

    pub fn with_default(mut self, default: impl Into<String>) -> Self {
        self.default = Some(default.into());
        self
    }

    /// The full prompt line, e.g. `Specify next point or [Close/Undo]:`.
    pub fn render(&self, expected: &ExpectedInput) -> String {
        let mut line = self.message.clone();
        if !expected.keywords.is_empty() {
            let names: Vec<&str> = expected
                .keywords
                .iter()
                .map(|keyword| keyword.name())
                .collect();
            // `Specify next point or [Close/Undo]`, but `Erase source objects? [Yes/No]`.
            let joiner = if line.ends_with('?') { " " } else { " or " };
            line.push_str(&format!("{joiner}[{}]", names.join("/")));
        }
        if let Some(default) = &self.default {
            line.push_str(&format!(" <{default}>"));
        }
        line.push(':');
        line
    }
}

/// What a command wants after consuming an input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Keep the command active and prompt again.
    Continue,
    /// The command is complete; return to the idle prompt.
    Finished,
}

/// What a command may touch while it runs.
///
/// The drawing is mutable only through its transaction API, so a command
/// cannot change the document without producing an undo step.
#[derive(Debug)]
pub struct CommandContext<'a> {
    pub drawing: &'a mut Drawing,
    pub selection: &'a mut Selection,
    /// How the drafting aids behave; session state, never undoable.
    pub settings: &'a mut DraftingSettings,
    pub(crate) log: &'a mut CommandLog,
    pub(crate) host_request: &'a mut Option<HostRequest>,
}

impl CommandContext<'_> {
    /// Writes a line to the command log.
    pub fn message(&mut self, text: impl Into<String>) {
        self.log.push(text.into());
    }

    /// Asks the host to do something a command cannot: show a file picker,
    /// offer a download. The host picks the request up after this step.
    pub fn request_host(&mut self, request: HostRequest) {
        *self.host_request = Some(request);
    }
}

/// An interactive command.
pub trait Command: Debug {
    /// Called once when the command is invoked. Commands that need no input
    /// do their work here and return [`Step::Finished`].
    fn start(&mut self, _ctx: &mut CommandContext<'_>) -> Step {
        Step::Continue
    }

    /// The request for the current state, for example `Specify next point`.
    fn prompt(&self) -> Prompt;

    /// What the current state accepts. Drives input resolution (§13.3).
    fn expected(&self) -> ExpectedInput;

    /// Consumes one input and advances the state machine.
    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step;

    /// Describes, semantically, what the command would create if the next
    /// point were `cursor`. The preview is transient: it lives in the overlay
    /// and never touches the document (§17).
    fn preview(&self, _cursor: Point2, _out: &mut Vec<OverlayItem>) {}

    /// The point the current input is measured from, if any: the anchor of
    /// the rubber band. It gives meaning to direct-distance entry, and to a
    /// point supplied where a distance or an angle is expected.
    fn base_point(&self) -> Option<Point2> {
        None
    }

    /// The point ORTHO and POLAR constrain the pointer from: the anchor of a
    /// rubber-band line. It is the base point unless the next point is not
    /// reached along such a line — the opposite corner of a rectangle.
    fn tracking_base(&self) -> Option<Point2> {
        self.base_point()
    }

    /// The entities this command is transiently editing. While an edit is in
    /// progress they must not be offered as snap targets at their old
    /// positions (DESIGN §18.2).
    fn transient_sources(&self) -> &[EntityId] {
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angles_are_measured_counter_clockwise_from_x_in_degrees() {
        for (x, y, expected) in [
            (1.0, 0.0, 0.0),
            (0.0, 2.0, 90.0),
            (-3.0, 0.0, 180.0),
            (0.0, -1.0, 270.0),
            (1.0, 1.0, 45.0),
        ] {
            assert!(
                (angle_degrees(Vec2::new(x, y)) - expected).abs() < 1e-9,
                "({x}, {y})"
            );
        }
    }

    #[test]
    fn a_question_lists_its_answers_without_or() {
        let expected = ExpectedInput::new(cad_input::InputKind::Nothing)
            .with_keywords(&[Keyword::new("Yes"), Keyword::new("No")]);
        assert_eq!(
            Prompt::new("Erase source objects?")
                .with_default("No")
                .render(&expected),
            "Erase source objects? [Yes/No] <No>:"
        );
    }

    #[test]
    fn a_plain_prompt_ends_with_a_colon() {
        assert_eq!(
            Prompt::new("Specify first point").render(&ExpectedInput::point()),
            "Specify first point:"
        );
    }

    #[test]
    fn options_are_rendered_from_the_expectation() {
        let expected =
            ExpectedInput::point().with_keywords(&[Keyword::new("Close"), Keyword::new("Undo")]);
        assert_eq!(
            Prompt::new("Specify next point").render(&expected),
            "Specify next point or [Close/Undo]:"
        );
    }

    #[test]
    fn a_default_is_shown_in_angle_brackets_after_the_options() {
        let expected = ExpectedInput::distance().with_keywords(&[Keyword::new("Diameter")]);
        let prompt = Prompt::new("Specify radius of circle").with_default("10.0000");
        assert_eq!(
            prompt.render(&expected),
            "Specify radius of circle or [Diameter] <10.0000>:"
        );
        assert_eq!(
            Prompt::new("Specify radius")
                .with_default("5")
                .render(&ExpectedInput::distance()),
            "Specify radius <5>:"
        );
    }
}
