//! What HATCH and HATCHEDIT share (#168): how a hatch is filled — a
//! pattern from the table, its angle, its scale — and the `Pattern`,
//! `Angle`, and `SCale` options that ask for them. A command holds a
//! [`LookOptions`], asks it first at each prompt, and reads the look it
//! keeps.

use cad_core::Point2;
use cad_document::{HatchEntity, HatchStyle, hatch_pattern, hatch_patterns};
use cad_input::{ExpectedInput, Keyword};

use super::ltscale::display;
use crate::{CommandContext, CommandInput, Prompt};

pub(super) const PATTERN: Keyword = Keyword::new("Pattern");
pub(super) const ANGLE: Keyword = Keyword::new("Angle");
pub(super) const SCALE: Keyword = Keyword::new("SCale");
pub(super) const OPTIONS: [Keyword; 3] = [PATTERN, ANGLE, SCALE];
/// Typed at the pattern prompt, lists the patterns.
const LIST: &str = "?";

/// How a hatch is filled, as the options change it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Look {
    /// A row of `hatch_patterns()`: 0 is `SOLID`.
    pub pattern: usize,
    /// Radians.
    pub angle: f64,
    pub scale: f64,
    /// Where the pattern's lines start: kept from the hatch being edited.
    pub origin: Point2,
}

impl Look {
    /// The look HATCH last used in this session.
    pub(super) fn remembered(ctx: &CommandContext<'_>) -> Self {
        Self {
            pattern: ctx.settings.hatch_pattern.min(hatch_patterns().len() - 1),
            angle: ctx.settings.hatch_angle,
            scale: ctx.settings.hatch_scale,
            origin: Point2::ORIGIN,
        }
    }

    pub(super) fn remember(self, ctx: &mut CommandContext<'_>) {
        ctx.settings.hatch_pattern = self.pattern;
        ctx.settings.hatch_angle = self.angle;
        ctx.settings.hatch_scale = self.scale;
    }

    /// A hatch's own look; a pattern the table lacks reads as `SOLID`.
    pub(super) fn of(hatch: &HatchEntity) -> Self {
        match &hatch.style {
            HatchStyle::Solid => Self {
                pattern: 0,
                angle: 0.0,
                scale: 1.0,
                origin: Point2::ORIGIN,
            },
            HatchStyle::Pattern {
                name,
                angle,
                scale,
                origin,
            } => Self {
                pattern: index_of(name).unwrap_or(0),
                angle: *angle,
                scale: *scale,
                origin: *origin,
            },
        }
    }

    pub(super) fn name(self) -> &'static str {
        &hatch_patterns()[self.pattern].name
    }

    pub(super) fn is_solid(self) -> bool {
        self.pattern == 0
    }

    pub(super) fn style(self) -> HatchStyle {
        if self.is_solid() {
            HatchStyle::Solid
        } else {
            HatchStyle::Pattern {
                name: self.name().to_owned(),
                angle: self.angle,
                scale: self.scale,
                origin: self.origin,
            }
        }
    }
}

fn index_of(name: &str) -> Option<usize> {
    let pattern = hatch_pattern(name)?;
    hatch_patterns()
        .iter()
        .position(|row| std::ptr::eq(row, pattern))
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Asking {
    Nothing,
    Pattern,
    Angle,
    Scale,
}

/// The look being chosen, and the prompt open for one part of it.
#[derive(Debug)]
pub(super) struct LookOptions {
    pub look: Look,
    asking: Asking,
}

impl LookOptions {
    pub(super) fn new(look: Look) -> Self {
        Self {
            look,
            asking: Asking::Nothing,
        }
    }

    /// While an option's prompt is open: what it asks.
    pub(super) fn prompt(&self) -> Option<Prompt> {
        Some(match self.asking {
            Asking::Nothing => return None,
            Asking::Pattern => Prompt::new(format!("Enter a pattern name ({LIST} to list)"))
                .with_default(self.look.name()),
            Asking::Angle => Prompt::new("Specify angle for pattern")
                .with_default(display(self.look.angle.to_degrees())),
            Asking::Scale => {
                Prompt::new("Specify scale for pattern").with_default(display(self.look.scale))
            }
        })
    }

    /// While an option's prompt is open: what it accepts.
    pub(super) fn expected(&self) -> Option<ExpectedInput> {
        Some(match self.asking {
            Asking::Nothing => return None,
            Asking::Pattern => ExpectedInput::text(),
            Asking::Angle => ExpectedInput::angle(),
            Asking::Scale => ExpectedInput::factor(),
        })
    }

    /// Opens the prompt `keyword` names; `false` when it is none of these.
    pub(super) fn open(&mut self, keyword: Keyword) -> bool {
        self.asking = match keyword {
            PATTERN => Asking::Pattern,
            ANGLE => Asking::Angle,
            SCALE => Asking::Scale,
            _ => return false,
        };
        true
    }

    /// Takes an input while an option's prompt is open. `None` when no
    /// prompt is open; `Some(true)` when the look changed, `Some(false)`
    /// when it did not — Enter, a refusal, or the list.
    pub(super) fn step(
        &mut self,
        input: &CommandInput,
        ctx: &mut CommandContext<'_>,
    ) -> Option<bool> {
        let asking = self.asking;
        if asking == Asking::Nothing {
            return None;
        }
        let changed = match (asking, input) {
            (_, CommandInput::Accept) => false,
            (Asking::Pattern, CommandInput::Text(text)) if text.trim() == LIST => {
                for pattern in hatch_patterns() {
                    ctx.message(format!("  {}: {}", pattern.name, pattern.description));
                }
                return Some(false); // the prompt stays open
            }
            (Asking::Pattern, CommandInput::Text(text)) => match index_of(text.trim()) {
                Some(pattern) => {
                    self.look.pattern = pattern;
                    true
                }
                None => {
                    ctx.message(format!("There is no hatch pattern \"{}\".", text.trim()));
                    return Some(false);
                }
            },
            (Asking::Angle, CommandInput::Angle(degrees)) => {
                self.look.angle = degrees.to_radians();
                true
            }
            (Asking::Scale, CommandInput::Distance(scale)) => {
                if !(scale.is_finite() && *scale > 0.0) {
                    ctx.message("The pattern scale must be greater than zero.");
                    return Some(false);
                }
                self.look.scale = *scale;
                true
            }
            _ => return Some(false),
        };
        self.asking = Asking::Nothing;
        Some(changed)
    }
}
