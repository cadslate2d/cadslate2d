//! DIMBASELINE and DIMCONTINUE (DESIGN §34; #212): dimensions chained
//! from another, one more for every point until Enter.
//!
//! The chain starts from the newest dimension in the space, or from one
//! picked (`Select`, or Enter at the point prompt): from its first
//! extension line for a baseline chain, from its second for a continued
//! one, or from the extension line nearer the pick. Each point then makes
//! a dimension of the same kind and style (`DimensionEntity::chained`): a
//! continued one on the same dimension line, running on from the last; a
//! baseline one from the same origin, its line the style's baseline
//! spacing further out than the last. `Undo` takes the last one back. A
//! radius or a diameter cannot be chained. The whole run is one undo.

use cad_core::tolerance::{AngularTolerance, LinearTolerance};
use cad_core::{EntityId, Point2};
use cad_document::{Chain, DimensionEntity, Entity, Measure};
use cad_input::{ExpectedInput, Keyword};

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const UNDO: Keyword = Keyword::new("Undo");
const SELECT: Keyword = Keyword::new("Select");

/// Which chain the command makes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Baseline,
    Continue,
}

impl Mode {
    /// The extension line of a new dimension the next one chains from:
    /// the shared origin, or the far end.
    fn next_origin(self) -> usize {
        match self {
            Self::Baseline => 0,
            Self::Continue => 1,
        }
    }
}

/// What the next dimension is chained to: a dimension and which of its
/// extension lines, 0 or 1.
#[derive(Clone, Debug)]
struct Base {
    dimension: DimensionEntity,
    origin: usize,
    /// Its style's baseline spacing, as the chain started.
    spacing: f64,
}

impl Base {
    fn from(&self) -> Point2 {
        self.dimension.measure.points()[match self.dimension.measure {
            Measure::Angular { .. } => self.origin + 1,
            _ => self.origin,
        }]
    }
}

#[derive(Debug, PartialEq, Eq)]
enum State {
    /// The next point, or an option.
    Next,
    /// A dimension to chain from.
    Select,
}

#[derive(Debug)]
pub(crate) struct DimChainCommand {
    mode: Mode,
    state: State,
    base: Option<Base>,
    /// What this run made, each with the base it was chained to, for
    /// `Undo`.
    made: Vec<(EntityId, Base)>,
}

impl DimChainCommand {
    /// DIMBASELINE.
    pub(crate) fn baseline() -> Self {
        Self::with(Mode::Baseline)
    }

    /// DIMCONTINUE.
    pub(crate) fn continued() -> Self {
        Self::with(Mode::Continue)
    }

    fn with(mode: Mode) -> Self {
        Self {
            mode,
            state: State::Select,
            base: None,
            made: Vec::new(),
        }
    }

    /// The dimension a point at `to` would make: what the preview shows
    /// and the point commits. `None` when it cannot be one.
    fn next(&self, to: Point2) -> Option<DimensionEntity> {
        let base = self.base.as_ref()?;
        let chain = match self.mode {
            Mode::Baseline => Chain::Baseline(base.spacing),
            Mode::Continue => Chain::Continue,
        };
        base.dimension.chained(base.origin, to, chain)
    }

    /// Chains from `dimension`'s extension line `origin`, or says why not.
    fn take(
        &mut self,
        ctx: &CommandContext<'_>,
        dimension: &DimensionEntity,
        origin: Option<usize>,
    ) -> Result<(), String> {
        if dimension.nearer_origin(dimension.line_point).is_none() {
            let kind = dimension.measure.kind();
            let article = if kind.starts_with(['a', 'e', 'i', 'o', 'u']) {
                "An"
            } else {
                "A"
            };
            return Err(format!(
                "{article} {kind} dimension cannot be chained: pick a linear, aligned, or angular one."
            ));
        }
        self.base = Some(Base {
            dimension: dimension.clone(),
            origin: origin.unwrap_or(match self.mode {
                Mode::Baseline => 0,
                Mode::Continue => 1,
            }),
            spacing: ctx
                .drawing
                .dim_styles()
                .get(dimension.style)
                .map_or(0.0, |style| style.scaled().baseline_spacing),
        });
        self.state = State::Next;
        Ok(())
    }

    fn select_prompt(&self) -> &'static str {
        match self.mode {
            Mode::Baseline => "Select base dimension",
            Mode::Continue => "Select continued dimension",
        }
    }
}

/// Whether a dimension measures nothing: a length or an angle of zero.
fn measures_nothing(dimension: &DimensionEntity) -> bool {
    let value = dimension.value();
    match dimension.measure {
        Measure::Angular { .. } => AngularTolerance::DEFAULT.is_zero(value.to_radians()),
        _ => LinearTolerance::DEFAULT
            .around(&dimension.measure.points())
            .is_zero(value),
    }
}

impl Command for DimChainCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        // The newest dimension in the space: the one last made.
        let newest = ctx
            .drawing
            .shown_in(ctx.space)
            .filter_map(|(id, entity)| match entity {
                Entity::Dimension(dimension) => Some((id, dimension)),
                _ => None,
            })
            .max_by_key(|(id, _)| *id)
            .map(|(_, dimension)| dimension.clone());
        if let Some(dimension) = newest
            && let Err(why) = self.take(ctx, &dimension, None)
        {
            ctx.message(why);
        }
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Next => {
                Prompt::new("Specify second extension line origin").with_default("select")
            }
            State::Select => Prompt::new(self.select_prompt()),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Next if self.made.is_empty() => ExpectedInput::point().with_keywords(&[SELECT]),
            State::Next => ExpectedInput::point().with_keywords(&[UNDO, SELECT]),
            State::Select => ExpectedInput::object(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Next, CommandInput::Point(to)) => {
                let Some(dimension) = self.next(to) else {
                    ctx.message("The point must be away from the extension line's origin.");
                    return Step::Continue;
                };
                if let Err(error) = dimension.check() {
                    ctx.message(error.explain());
                    return Step::Continue;
                }
                if measures_nothing(&dimension) {
                    ctx.message("The point must be off the extension line it chains from.");
                    return Step::Continue;
                }
                let mut transaction = ctx.transaction();
                match transaction.insert(dimension.clone()) {
                    Ok(id) => {
                        transaction.commit();
                        let before = self.base.take().expect("a point is asked for with a base");
                        self.base = Some(Base {
                            dimension,
                            origin: self.mode.next_origin(),
                            spacing: before.spacing,
                        });
                        self.made.push((id, before));
                    }
                    Err(error) => {
                        drop(transaction);
                        ctx.message(error.to_string());
                    }
                }
            }
            (State::Next, CommandInput::Keyword(UNDO)) => {
                if let Some((id, before)) = self.made.pop() {
                    let mut transaction = ctx.transaction();
                    if transaction.remove(id).is_ok() {
                        transaction.commit();
                    }
                    self.base = Some(before);
                }
            }
            (State::Next, CommandInput::Keyword(_) | CommandInput::Accept) => {
                self.state = State::Select;
            }
            (State::Select, CommandInput::Pick { entity, at }) => {
                match ctx.drawing.entity(entity) {
                    Some(Entity::Dimension(dimension)) => {
                        let dimension = dimension.clone();
                        if let Err(why) = self.take(ctx, &dimension, dimension.nearer_origin(at)) {
                            ctx.message(why);
                        }
                    }
                    Some(other) => {
                        let kind = other.kind_name();
                        ctx.message(format!("A {kind} is not a dimension."));
                    }
                    None => {}
                }
            }
            (State::Select, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if self.state != State::Next {
            return;
        }
        if let Some(dimension) = self.next(cursor)
            && dimension.check().is_ok()
            && !measures_nothing(&dimension)
        {
            out.push(OverlayItem::PreviewEntity(dimension.into()));
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match self.state {
            State::Next => self.base.as_ref().map(Base::from),
            State::Select => None,
        }
    }
}
