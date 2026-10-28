//! VIEWFRAME (ADR-0041, #351): places views on the sheet. Each view is
//! objects already drawn, picked, with the point where the object's
//! reference point is — the view's origin — an id, a title, and how it is
//! placed: the principal view, projected of another view looking from a
//! side, or free. Several views given together are placed together, so a
//! principal view is put where its projected views fit; views already on
//! the sheet never move. One undo.

use cad_core::Point2;
use cad_geometry::composition::{Looking, Role};
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::modify::selection_expected;
use crate::compose::{Cut, NewView, compose, sheet_of, valid_id, views_on};
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const PRINCIPAL: Keyword = Keyword::new("Principal");
const PROJECTED: Keyword = Keyword::new("PRojected");
const FREE: Keyword = Keyword::new("Free");
const SECTION: Keyword = Keyword::new("Section");
const LEFT: Keyword = Keyword::new("Left");
const RIGHT: Keyword = Keyword::new("Right");
const ABOVE: Keyword = Keyword::new("Above");
const BELOW: Keyword = Keyword::new("Below");
const REAR: Keyword = Keyword::new("REar");
const YES: Keyword = Keyword::new("Yes");
const NO: Keyword = Keyword::new("No");

#[derive(Clone, Debug, PartialEq)]
enum State {
    Select,
    Origin,
    Id,
    Title,
    Role,
    Of,
    Looking,
    CutOf,
    CutFrom,
    CutTo,
    CutToward,
    Ratio,
    Another,
}

#[derive(Debug)]
pub(crate) struct ViewFrameCommand {
    state: State,
    views: Vec<NewView>,
    /// The view being given.
    current: NewView,
    /// The principal view's id, on the sheet or among those given.
    principal: Option<String>,
}

impl ViewFrameCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Select,
            views: Vec::new(),
            current: NewView {
                id: String::new(),
                title: String::new(),
                role: Role::Free,
                entities: Vec::new(),
                origin: Point2::ORIGIN,
                ratio: None,
                cut: None,
                parts: Vec::new(),
            },
            principal: None,
        }
    }

    fn take_selection(&mut self, ctx: &mut CommandContext<'_>) -> bool {
        let ids: Vec<_> = ctx.selection.iter().collect();
        ctx.selection.clear();
        if ids.is_empty() {
            return false;
        }
        self.current.entities = ids;
        true
    }

    fn finish_view(&mut self) {
        if self.current.role == Role::Principal {
            self.principal = Some(self.current.id.clone());
        }
        let done = std::mem::replace(
            &mut self.current,
            NewView {
                id: String::new(),
                title: String::new(),
                role: Role::Free,
                entities: Vec::new(),
                origin: Point2::ORIGIN,
                ratio: None,
                cut: None,
                parts: Vec::new(),
            },
        );
        self.views.push(done);
        self.state = State::Another;
    }

    fn place(&self, ctx: &mut CommandContext<'_>) -> Step {
        let space = ctx.space;
        match compose(ctx.drawing, space, &self.views) {
            Ok(ids) => {
                let sheet = sheet_of(ctx.drawing).map(|s| s.group).unwrap_or_default();
                ctx.message(format!(
                    "{} placed on {sheet}: {}.",
                    if ids.len() == 1 {
                        "1 view".to_owned()
                    } else {
                        format!("{} views", ids.len())
                    },
                    ids.join(", ")
                ));
            }
            Err(reason) => ctx.message(reason),
        }
        Step::Finished
    }

    fn taken(&self, id: &str, ctx: &CommandContext<'_>) -> bool {
        self.views.iter().any(|v| v.id == id)
            || sheet_of(ctx.drawing).is_some_and(|sheet| {
                views_on(ctx.drawing, &sheet.group)
                    .iter()
                    .any(|v| v.id == id)
            })
    }
}

impl Command for ViewFrameCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        let Some(sheet) = sheet_of(ctx.drawing) else {
            ctx.message("There is no sheet to place views on: draw one with TITLEBLOCK first.");
            return Step::Finished;
        };
        self.principal = views_on(ctx.drawing, &sheet.group)
            .into_iter()
            .find(|v| v.role == Role::Principal)
            .map(|v| v.id);
        if self.take_selection(ctx) {
            self.state = State::Origin;
        }
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Select => Prompt::new("Select the objects of the view"),
            State::Origin => {
                Prompt::new("Specify the view's origin, where the object's reference point is")
                    .with_default("0,0")
            }
            State::Id => Prompt::new("Enter the view's id"),
            State::Title => {
                Prompt::new("Enter the view's title").with_default(self.current.id.to_uppercase())
            }
            State::Role => {
                Prompt::new("How is it placed?").with_default(if self.principal.is_none() {
                    "Principal"
                } else {
                    "PRojected"
                })
            }
            State::Of => {
                let prompt = Prompt::new("Projected of which view?");
                match &self.principal {
                    Some(id) => prompt.with_default(id.clone()),
                    None => prompt,
                }
            }
            State::Looking => Prompt::new("Looking from?"),
            State::CutOf => Prompt::new("Cut across which view?"),
            State::CutFrom => {
                Prompt::new("Specify where the cut starts, in that view's own coordinates")
            }
            State::CutTo => Prompt::new("Specify where the cut ends"),
            State::CutToward => Prompt::new("Specify a point on the side the arrows look toward"),
            State::Ratio => Prompt::new("Specify the view's scale, paper to real, as 2:1")
                .with_default("the sheet's"),
            State::Another => Prompt::new("Another view?").with_default("No"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Select => selection_expected(),
            State::Origin => ExpectedInput::point(),
            State::Id | State::Title | State::Of | State::Ratio | State::CutOf => {
                ExpectedInput::text()
            }
            State::CutFrom | State::CutTo | State::CutToward => ExpectedInput::point(),
            State::Role => ExpectedInput::new(InputKind::Nothing)
                .with_keywords(&[PRINCIPAL, PROJECTED, FREE, SECTION]),
            State::Looking => ExpectedInput::new(InputKind::Nothing)
                .with_keywords(&[LEFT, RIGHT, ABOVE, BELOW, REAR]),
            State::Another => ExpectedInput::new(InputKind::Nothing).with_keywords(&[YES, NO]),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (self.state.clone(), input) {
            (State::Select, CommandInput::Accept) => {
                if self.take_selection(ctx) {
                    self.state = State::Origin;
                } else if self.views.is_empty() {
                    ctx.message("No objects selected.");
                    return Step::Finished;
                } else {
                    return self.place(ctx);
                }
            }
            (State::Origin, CommandInput::Point(at)) => {
                self.current.origin = at;
                self.state = State::Id;
            }
            (State::Origin, CommandInput::Accept) => self.state = State::Id,
            (State::Id, CommandInput::Text(id)) => {
                let id = id.trim().to_owned();
                if !valid_id(&id) {
                    ctx.message("A view's id is letters, digits, - and _.");
                } else if self.taken(&id, ctx) {
                    ctx.message(format!("There is a view {id} already."));
                } else {
                    self.current.id = id;
                    self.state = State::Title;
                }
            }
            (State::Title, CommandInput::Text(title)) if !title.trim().is_empty() => {
                self.current.title = title.trim().to_owned();
                self.state = State::Role;
            }
            (State::Title, CommandInput::Accept | CommandInput::Text(_)) => {
                self.current.title = self.current.id.to_uppercase();
                self.state = State::Role;
            }
            (State::Role, CommandInput::Accept) if self.principal.is_none() => {
                self.current.role = Role::Principal;
                self.finish_view();
            }
            (State::Role, CommandInput::Keyword(PRINCIPAL)) => {
                if self.principal.is_some() {
                    ctx.message("The sheet has a principal view already.");
                } else {
                    self.current.role = Role::Principal;
                    self.finish_view();
                }
            }
            (State::Role, CommandInput::Keyword(PROJECTED) | CommandInput::Accept) => {
                self.state = State::Of;
            }
            (State::Role, CommandInput::Keyword(FREE)) => {
                self.current.role = Role::Free;
                self.state = State::Ratio;
            }
            (State::Role, CommandInput::Keyword(SECTION)) => {
                self.current.role = Role::Free;
                self.state = State::CutOf;
            }
            (State::CutOf, CommandInput::Text(of)) if !of.trim().is_empty() => {
                self.current.cut = Some(Cut {
                    of: of.trim().to_owned(),
                    from: Point2::ORIGIN,
                    to: Point2::ORIGIN,
                    toward: Point2::ORIGIN,
                });
                self.state = State::CutFrom;
            }
            (State::CutFrom, CommandInput::Point(at)) => {
                if let Some(cut) = &mut self.current.cut {
                    cut.from = at;
                }
                self.state = State::CutTo;
            }
            (State::CutTo, CommandInput::Point(at)) => {
                if let Some(cut) = &mut self.current.cut {
                    cut.to = at;
                }
                self.state = State::CutToward;
            }
            (State::CutToward, CommandInput::Point(at)) => {
                if let Some(cut) = &mut self.current.cut {
                    cut.toward = at;
                }
                self.state = State::Ratio;
            }
            (State::Ratio, CommandInput::Accept) => self.finish_view(),
            (State::Ratio, CommandInput::Text(text)) => match parse_ratio(&text) {
                Some(ratio) => {
                    self.current.ratio = Some(ratio);
                    self.finish_view();
                }
                None => ctx.message("A scale is paper to real, as 2:1 or 1:5, greater than zero."),
            },
            (State::Of, CommandInput::Text(of)) if !of.trim().is_empty() => {
                self.current.role = Role::Projected {
                    of: of.trim().to_owned(),
                    looking: Looking::Left,
                };
                self.state = State::Looking;
            }
            (State::Of, CommandInput::Accept | CommandInput::Text(_)) => {
                if let Some(principal) = self.principal.clone() {
                    self.current.role = Role::Projected {
                        of: principal,
                        looking: Looking::Left,
                    };
                    self.state = State::Looking;
                }
            }
            (State::Looking, CommandInput::Keyword(k)) => {
                let looking = match k {
                    LEFT => Looking::Left,
                    RIGHT => Looking::Right,
                    ABOVE => Looking::Above,
                    BELOW => Looking::Below,
                    _ => Looking::Behind,
                };
                if let Role::Projected { of, .. } = &self.current.role {
                    self.current.role = Role::Projected {
                        of: of.clone(),
                        looking,
                    };
                }
                self.finish_view();
            }
            (State::Another, CommandInput::Keyword(YES)) => self.state = State::Select,
            (State::Another, CommandInput::Keyword(_) | CommandInput::Accept) => {
                return self.place(ctx);
            }
            _ => {}
        }
        Step::Continue
    }
}

/// A scale as a drafter types it: `2:1`, `1:5`, or a number, paper to real.
fn parse_ratio(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let ratio = match text.split_once(':') {
        Some((paper, real)) => {
            paper.trim().parse::<f64>().ok()? / real.trim().parse::<f64>().ok()?
        }
        None => text.parse::<f64>().ok()?,
    };
    (ratio.is_finite() && ratio > 0.0).then_some(ratio)
}
