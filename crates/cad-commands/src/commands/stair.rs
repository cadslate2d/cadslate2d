//! STAIR (#331): a stair sized by Blondel's rule, in plan or in section.
//! It asks for the floor-to-floor height and the stair's width — in the
//! drawing's units, as FLOORPLAN asks for lengths — its kind (straight, L,
//! U, helical, and a helix's newel radius), the view, the foot, and the
//! direction it climbs; says how it is sized; and draws it on `STAIR`, one
//! undo. What cannot be sized or would leave no headroom is refused with
//! the reason (`cad_geometry::stair`).

use cad_core::Point2;
use cad_geometry::stair::{Kind, Rule, Stair};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::ltscale::display;
use crate::stair::{describe, draw, entities, explain};
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const STRAIGHT: Keyword = Keyword::new("Straight");
const L: Keyword = Keyword::new("L");
const U: Keyword = Keyword::new("U");
const HELICAL: Keyword = Keyword::new("Helical");
const PLAN: Keyword = Keyword::new("Plan");
const SECTION: Keyword = Keyword::new("Section");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Height,
    Width,
    Kind,
    Newel,
    View,
    Foot,
    Direction,
}

#[derive(Debug)]
pub(crate) struct StairCommand {
    state: State,
    stair: Stair,
    section: bool,
    foot: Point2,
    /// Drawing units to the metre.
    scale: f64,
}

impl StairCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Height,
            stair: Stair {
                kind: Kind::Straight,
                height: 2.8,
                width: 0.9,
                newel: 0.15,
                rule: Rule::default(),
            },
            section: false,
            foot: Point2::ORIGIN,
            scale: 1.0,
        }
    }

    fn metres(&self, length: f64) -> f64 {
        length / self.scale
    }

    fn finish(&self, angle: f64, ctx: &mut CommandContext<'_>) -> Step {
        let laid = if self.section {
            self.stair.section()
        } else {
            self.stair.plan()
        };
        match laid.and_then(|d| self.stair.sizing().map(|s| (d, s))) {
            Ok((laid, sizing)) => {
                // A plan climbs along its own y; a section runs along its x.
                let turn = if self.section {
                    angle
                } else {
                    angle - std::f64::consts::FRAC_PI_2
                };
                let made = entities(&laid, self.foot, turn, self.scale);
                let space = ctx.space;
                match draw(ctx.drawing, space, made) {
                    Ok(_) => ctx.message(describe(&sizing)),
                    Err(error) => ctx.message(error.to_string()),
                }
            }
            Err(error) => ctx.message(explain(error)),
        }
        Step::Finished
    }

    /// The kind given: a helix asks for its newel; the others for the view.
    fn kind(&mut self, kind: Kind, ctx: &mut CommandContext<'_>) -> State {
        self.stair.kind = kind;
        // Refused now, not after the view and the point.
        if kind != Kind::Helical
            && let Err(error) = self.stair.sizing()
        {
            ctx.message(explain(error));
            return State::Height;
        }
        if kind == Kind::Helical {
            State::Newel
        } else {
            State::View
        }
    }
}

impl Command for StairCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.scale = ctx.drawing.unit().map_or(1.0, |unit| 1.0 / unit.metres());
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Height => Prompt::new("Specify floor-to-floor height")
                .with_default(display(self.stair.height * self.scale)),
            State::Width => Prompt::new("Specify stair width")
                .with_default(display(self.stair.width * self.scale)),
            State::Kind => Prompt::new("What kind of stair?").with_default("Straight"),
            State::Newel => Prompt::new("Specify newel radius")
                .with_default(display(self.stair.newel * self.scale)),
            State::View => Prompt::new("Which view?").with_default("Plan"),
            State::Foot => Prompt::new("Specify the foot of the stair").with_default("0,0"),
            // A plan's direction is where it climbs; a section's, where its
            // run goes, the height above it.
            State::Direction => Prompt::new("Specify the direction it climbs")
                .with_default(if self.section { "0" } else { "90" }),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Height | State::Width | State::Newel => ExpectedInput::distance(),
            State::Kind => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&[STRAIGHT, L, U, HELICAL])
            }
            State::View => ExpectedInput::new(InputKind::Nothing).with_keywords(&[PLAN, SECTION]),
            State::Foot => ExpectedInput::point(),
            State::Direction => ExpectedInput::angle(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        self.state = match (self.state, input) {
            (State::Height, CommandInput::Accept) => State::Width,
            (State::Height, CommandInput::Distance(h)) if h.is_finite() && h > 0.0 => {
                self.stair.height = self.metres(h);
                State::Width
            }
            (State::Width, CommandInput::Accept) => State::Kind,
            (State::Width, CommandInput::Distance(w)) if w.is_finite() && w > 0.0 => {
                self.stair.width = self.metres(w);
                State::Kind
            }
            (State::Newel, CommandInput::Distance(r)) if r.is_finite() && r > 0.0 => {
                self.stair.newel = self.metres(r);
                self.newel_checked(ctx)
            }
            (State::Height | State::Width | State::Newel, CommandInput::Distance(_)) => {
                ctx.message("A stair's sizes must be greater than zero.");
                self.state
            }
            (State::Kind, CommandInput::Accept) => self.kind(Kind::Straight, ctx),
            (State::Kind, CommandInput::Keyword(k)) => {
                let kind = match k {
                    k if k == L => Kind::L,
                    k if k == U => Kind::U,
                    k if k == HELICAL => Kind::Helical,
                    _ => Kind::Straight,
                };
                self.kind(kind, ctx)
            }
            (State::Newel, CommandInput::Accept) => self.newel_checked(ctx),
            (State::View, CommandInput::Accept) => State::Foot,
            (State::View, CommandInput::Keyword(k)) => {
                self.section = k == SECTION;
                State::Foot
            }
            (State::Foot, CommandInput::Accept) => State::Direction,
            (State::Foot, CommandInput::Point(foot)) => {
                self.foot = foot;
                State::Direction
            }
            (State::Direction, CommandInput::Accept) => {
                let angle = if self.section {
                    0.0
                } else {
                    std::f64::consts::FRAC_PI_2
                };
                return self.finish(angle, ctx);
            }
            (State::Direction, CommandInput::Angle(degrees)) => {
                return self.finish(degrees.to_radians(), ctx);
            }
            (state, _) => state,
        };
        Step::Continue
    }
}

impl StairCommand {
    fn newel_checked(&mut self, ctx: &mut CommandContext<'_>) -> State {
        match self.stair.sizing() {
            Ok(_) => State::View,
            Err(error) => {
                ctx.message(explain(error));
                State::Width
            }
        }
    }
}
