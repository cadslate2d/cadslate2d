//! DIMCONSTRAINT and -PARAMETERS (#268, ADR-0038): dimensional constraints,
//! which drive the geometry — change the value, and the shape follows.
//!
//! DIMCONSTRAINT asks for a form — Linear, Horizontal, Vertical, Aligned,
//! Angular, Radial, Diameter — or is started with one (`DCLINEAR` …), then
//! for what it measures: two points (or a line, with the Object option),
//! two lines, or an arc or circle; then where its dimension line is shown;
//! then its value, the measured one by default. A value typed is the
//! constraint's, and the geometry settles to it in the same undo. Names are
//! AutoCAD's: `d1` for a distance, `ang1` an angle, `rad1` a radius, `dia1`
//! a diameter, each the first number free.
//!
//! -PARAMETERS edits a constraint's value by name — the PARAMETERS
//! manager's work, at the command line — renames or deletes one, or lists
//! them.

use cad_core::{EntityId, Point2};
use cad_document::{Axis, Constraint, Entity, Kind, Part, Reference, TransactionError};
use cad_geometry::constraint::Verdict;
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::constrain::{edge_at, point_near};
use super::dimlinear::auto_rotation;
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

const LINEAR: Keyword = Keyword::new("LInear");
const HORIZONTAL: Keyword = Keyword::new("Horizontal");
const VERTICAL: Keyword = Keyword::new("Vertical");
const ALIGNED: Keyword = Keyword::new("ALigned");
const ANGULAR: Keyword = Keyword::new("ANgular");
const RADIAL: Keyword = Keyword::new("Radial");
const DIAMETER: Keyword = Keyword::new("Diameter");
const OBJECT: Keyword = Keyword::new("Object");

const FORMS: [Keyword; 7] = [
    LINEAR, HORIZONTAL, VERTICAL, ALIGNED, ANGULAR, RADIAL, DIAMETER,
];

/// The forms of dimensional constraint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Form {
    /// Horizontal or vertical, as the dimension line is placed.
    Linear,
    Horizontal,
    Vertical,
    Aligned,
    Angular,
    Radial,
    Diameter,
}

impl Form {
    fn from_keyword(keyword: Keyword) -> Self {
        match keyword {
            k if k == LINEAR => Self::Linear,
            k if k == HORIZONTAL => Self::Horizontal,
            k if k == VERTICAL => Self::Vertical,
            k if k == ANGULAR => Self::Angular,
            k if k == RADIAL => Self::Radial,
            k if k == DIAMETER => Self::Diameter,
            _ => Self::Aligned,
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Self::Angular => "ang",
            Self::Radial => "rad",
            Self::Diameter => "dia",
            _ => "d",
        }
    }
}

#[derive(Debug)]
enum State {
    Form,
    /// Picking what it measures: `references` so far.
    Picking {
        references: Vec<Reference>,
    },
    /// A line picked with the Object option: its two ends, measured.
    Object,
    Place {
        references: Vec<Reference>,
    },
    Value {
        constraint: Constraint,
    },
}

#[derive(Debug)]
pub(crate) struct DimConstraintCommand {
    form: Form,
    state: State,
}

impl DimConstraintCommand {
    /// DIMCONSTRAINT: asks for the form.
    pub(crate) fn new() -> Self {
        Self {
            form: Form::Aligned,
            state: State::Form,
        }
    }

    /// `DCLINEAR` and the rest.
    pub(crate) fn of(form: Form) -> Self {
        Self {
            form,
            state: State::Picking {
                references: Vec::new(),
            },
        }
    }

    fn wants_points(&self) -> bool {
        matches!(
            self.form,
            Form::Linear | Form::Horizontal | Form::Vertical | Form::Aligned
        )
    }
}

/// The first `prefix` and number no constraint is called, `d3`.
fn free_name(ctx: &CommandContext<'_>, prefix: &str) -> String {
    (1..)
        .map(|n| format!("{prefix}{n}"))
        .find(|name| ctx.drawing.constraints().find(name).is_none())
        .expect("a free name")
}

fn position(ctx: &CommandContext<'_>, reference: Reference) -> Option<Point2> {
    reference
        .part
        .position_on(ctx.drawing.entity(reference.entity)?)
}

/// A straight edge's two ends, as they are.
fn edge_ends(ctx: &CommandContext<'_>, reference: Reference) -> Option<(Point2, Point2)> {
    match (ctx.drawing.entity(reference.entity)?, reference.part) {
        (Entity::Line(line), Part::Edge) => Some((line.start, line.end)),
        (Entity::Polyline(p), Part::Segment(n)) => {
            let n = n as usize;
            Some((p.vertices[n], p.vertices[(n + 1) % p.vertices.len()]))
        }
        _ => None,
    }
}

/// `120`, `2.5`: as short as the value allows.
fn display(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

impl DimConstraintCommand {
    /// The constraint the picks and the placement make, at the measured
    /// value.
    fn measured(
        &self,
        references: &[Reference],
        place: Point2,
        ctx: &CommandContext<'_>,
    ) -> Option<Constraint> {
        let name = free_name(ctx, self.form.prefix());
        let kind = match self.form {
            Form::Linear | Form::Horizontal | Form::Vertical | Form::Aligned => {
                let (a, b) = (position(ctx, references[0])?, position(ctx, references[1])?);
                let axis = match self.form {
                    Form::Horizontal => Axis::Horizontal,
                    Form::Vertical => Axis::Vertical,
                    Form::Aligned => Axis::Aligned,
                    _ if auto_rotation(a, b, place) == 0.0 => Axis::Horizontal,
                    _ => Axis::Vertical,
                };
                let value = match axis {
                    Axis::Horizontal => b.x - a.x,
                    Axis::Vertical => b.y - a.y,
                    Axis::Aligned => a.distance_to(b),
                };
                Kind::Distance { axis, value }
            }
            Form::Angular => {
                let (a0, a1) = edge_ends(ctx, references[0])?;
                let (b0, b1) = edge_ends(ctx, references[1])?;
                let (da, db) = (a1 - a0, b1 - b0);
                let value = (da.x * db.y - da.y * db.x).atan2(da.x * db.x + da.y * db.y);
                Kind::Angle { value }
            }
            Form::Radial | Form::Diameter => {
                let radius = match ctx.drawing.entity(references[0].entity)? {
                    Entity::Circle(c) => c.radius,
                    Entity::Arc(a) => a.radius,
                    _ => return None,
                };
                if self.form == Form::Radial {
                    Kind::Radius { value: radius }
                } else {
                    Kind::Diameter {
                        value: 2.0 * radius,
                    }
                }
            }
        };
        Some(Constraint::dimensional(name, kind, references.to_vec()).placed(place))
    }

    /// Adds the constraint at its value; what to say.
    fn apply(constraint: Constraint, ctx: &mut CommandContext<'_>) -> String {
        let mut transaction = ctx.transaction();
        match transaction.assess(&constraint).map(|a| a.verdict) {
            Some(Verdict::Redundant) => {
                return "The geometry is already fully determined there: it would over-constrain it."
                    .to_owned();
            }
            Some(Verdict::Conflicting) => {
                return "It contradicts the other constraints.".to_owned();
            }
            _ => {}
        }
        let said = describe(&constraint);
        match transaction.add_constraint(constraint) {
            Ok(_) => {
                // A refusal of the settled edit is logged by the session.
                if transaction.commit().refused.is_some() {
                    String::new()
                } else {
                    said
                }
            }
            Err(TransactionError::BadConstraint(why)) => sentence(&why.to_string()),
            Err(other) => sentence(&other.to_string()),
        }
    }
}

fn sentence(text: &str) -> String {
    let mut out = text[..1].to_uppercase();
    out.push_str(&text[1..]);
    out.push('.');
    out
}

/// `d1 = 120`, `ang1 = 30°`.
fn describe(constraint: &Constraint) -> String {
    match constraint.kind {
        Kind::Angle { value } => format!("{} = {}°", constraint.name, display(value.to_degrees())),
        ref kind => format!(
            "{} = {}",
            constraint.name,
            display(kind.value().unwrap_or_default())
        ),
    }
}

/// The value typed for `constraint`, in its own sign: a horizontal
/// distance measured leftwards stays leftwards.
fn with_typed(constraint: &Constraint, typed: f64) -> Kind {
    let value = match constraint.kind {
        Kind::Angle { value } => typed.to_radians().abs().copysign(value),
        Kind::Distance {
            axis: Axis::Horizontal | Axis::Vertical,
            value,
        } => typed.abs().copysign(value),
        _ => typed,
    };
    constraint.kind.with_value(value)
}

impl Command for DimConstraintCommand {
    fn start(&mut self, _ctx: &mut CommandContext<'_>) -> Step {
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Form => Prompt::new("Enter constraint form").with_default("ALigned"),
            State::Picking { references } => match (self.form, references.len()) {
                (Form::Angular, 0) => Prompt::new("Select first line"),
                (Form::Angular, _) => Prompt::new("Select second line"),
                (Form::Radial | Form::Diameter, _) => Prompt::new("Select arc or circle"),
                (_, 0) => Prompt::new("Specify first constraint point").with_default("Object"),
                _ => Prompt::new("Specify second constraint point"),
            },
            State::Object => Prompt::new("Select object"),
            State::Place { .. } => Prompt::new("Specify dimension line location"),
            State::Value { constraint } => {
                let current = match constraint.kind {
                    Kind::Angle { value } => display(value.to_degrees().abs()),
                    ref kind => display(kind.value().unwrap_or_default().abs()),
                };
                Prompt::new(format!("Enter value for {}", constraint.name)).with_default(current)
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Form => ExpectedInput::new(InputKind::Nothing).with_keywords(&FORMS),
            State::Picking { references } if self.wants_points() => {
                if references.is_empty() {
                    ExpectedInput::point().with_keywords(&[OBJECT])
                } else {
                    ExpectedInput::point()
                }
            }
            State::Picking { .. } | State::Object => ExpectedInput::object(),
            State::Place { .. } => ExpectedInput::point(),
            State::Value { constraint } => {
                if matches!(constraint.kind, Kind::Angle { .. }) {
                    ExpectedInput::angle()
                } else {
                    ExpectedInput::distance()
                }
            }
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, State::Form);
        self.state = match (state, input) {
            (State::Form, CommandInput::Keyword(keyword)) => {
                *self = Self::of(Form::from_keyword(keyword));
                return Step::Continue;
            }
            (State::Form, CommandInput::Accept) => {
                *self = Self::of(Form::Aligned);
                return Step::Continue;
            }
            (State::Form, _) => State::Form,
            (State::Picking { references }, CommandInput::Keyword(_) | CommandInput::Accept)
                if references.is_empty() && self.wants_points() =>
            {
                State::Object
            }
            (State::Object, CommandInput::Pick { entity, at }) => {
                match line_ends(ctx, entity, at) {
                    Some(ends) => State::Place {
                        references: ends.to_vec(),
                    },
                    None => {
                        ctx.message("Pick a line or a straight polyline segment.");
                        State::Object
                    }
                }
            }
            (State::Picking { mut references }, CommandInput::Point(at)) if self.wants_points() => {
                match point_near(ctx, at, &references) {
                    Some(reference) => references.push(reference),
                    None => ctx.message("No end, centre, or vertex there."),
                }
                if references.len() == 2 {
                    State::Place { references }
                } else {
                    State::Picking { references }
                }
            }
            (State::Picking { mut references }, CommandInput::Pick { entity, at })
                if !self.wants_points() =>
            {
                let edge = ctx
                    .drawing
                    .entity(entity)
                    .and_then(|e| edge_at(e, entity, at));
                let wanted = match self.form {
                    Form::Angular => edge.filter(|r| edge_ends(ctx, *r).is_some()),
                    _ => edge.filter(|r| {
                        matches!(
                            ctx.drawing.entity(r.entity),
                            Some(Entity::Circle(_) | Entity::Arc(_))
                        )
                    }),
                };
                match wanted {
                    Some(reference) if !references.contains(&reference) => {
                        references.push(reference);
                    }
                    _ if self.form == Form::Angular => ctx.message("Pick a line."),
                    _ => ctx.message("Pick an arc or a circle."),
                }
                let needed = if self.form == Form::Angular { 2 } else { 1 };
                if references.len() == needed {
                    State::Place { references }
                } else {
                    State::Picking { references }
                }
            }
            (State::Place { references }, CommandInput::Point(place)) => {
                match self.measured(&references, place, ctx) {
                    Some(constraint) => {
                        let said = describe(&constraint);
                        let value = said.split(" = ").nth(1).unwrap_or_default().to_owned();
                        ctx.message(format!("Dimension text = {value}"));
                        State::Value { constraint }
                    }
                    None => {
                        ctx.message("Those cannot be measured.");
                        return Step::Finished;
                    }
                }
            }
            (State::Value { constraint }, input) => {
                let typed = match input {
                    CommandInput::Accept => None,
                    CommandInput::Distance(value) | CommandInput::Angle(value) => Some(value),
                    _ => {
                        self.state = State::Value { constraint };
                        return Step::Continue;
                    }
                };
                let mut constraint = constraint;
                if let Some(typed) = typed {
                    constraint.kind = with_typed(&constraint, typed);
                }
                let said = Self::apply(constraint, ctx);
                if !said.is_empty() {
                    ctx.message(said);
                }
                return Step::Finished;
            }
            (state, _) => state,
        };
        Step::Continue
    }
}

/// The two ends of a line, or of the polyline segment nearest `at`.
fn line_ends(ctx: &CommandContext<'_>, entity: EntityId, at: Point2) -> Option<[Reference; 2]> {
    let edge = edge_at(ctx.drawing.entity(entity)?, entity, at)?;
    match (ctx.drawing.entity(entity)?, edge.part) {
        (Entity::Line(_), _) => Some([
            Reference::new(entity, Part::Start),
            Reference::new(entity, Part::End),
        ]),
        (Entity::Polyline(p), Part::Segment(n)) => Some([
            Reference::new(entity, Part::Vertex(n)),
            Reference::new(entity, Part::Vertex((n + 1) % p.vertices.len() as u32)),
        ]),
        _ => None,
    }
}

// ── -PARAMETERS ──

const EDIT: Keyword = Keyword::new("Edit");
const RENAME: Keyword = Keyword::new("Rename");
const DELETE: Keyword = Keyword::new("Delete");
const LIST: Keyword = Keyword::new("?");

#[derive(Debug)]
enum ParamState {
    Option,
    Name(Keyword),
    Value(cad_document::ConstraintId),
    NewName(cad_document::ConstraintId),
}

/// -PARAMETERS: a dimensional constraint's value edited by name, as one
/// undo the geometry follows; renamed; deleted; or every one listed.
#[derive(Debug)]
pub(crate) struct ParametersCommand {
    state: ParamState,
}

impl ParametersCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: ParamState::Option,
        }
    }
}

impl Command for ParametersCommand {
    fn start(&mut self, _ctx: &mut CommandContext<'_>) -> Step {
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match &self.state {
            ParamState::Option => Prompt::new("Enter an option").with_default("Edit"),
            ParamState::Name(k) if *k == RENAME => Prompt::new("Enter name of parameter to rename"),
            ParamState::Name(k) if *k == DELETE => Prompt::new("Enter name of parameter to delete"),
            ParamState::Name(_) => Prompt::new("Enter name of parameter to edit"),
            ParamState::Value(_) => Prompt::new("Enter value"),
            ParamState::NewName(_) => Prompt::new("Enter new name"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            ParamState::Option => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&[EDIT, RENAME, DELETE, LIST])
            }
            ParamState::Name(_) | ParamState::NewName(_) => ExpectedInput::text(),
            ParamState::Value(_) => ExpectedInput::distance(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let state = std::mem::replace(&mut self.state, ParamState::Option);
        match (state, input) {
            (ParamState::Option, CommandInput::Keyword(k)) if k == LIST => {
                let mut rows: Vec<String> = ctx
                    .drawing
                    .constraints()
                    .iter()
                    .filter(|(_, c)| c.kind.is_dimensional())
                    .map(|(_, c)| describe(c))
                    .collect();
                if rows.is_empty() {
                    rows.push("The drawing has no parameters.".to_owned());
                }
                for row in rows {
                    ctx.message(row);
                }
                Step::Finished
            }
            (ParamState::Option, CommandInput::Keyword(k)) => {
                self.state = ParamState::Name(k);
                Step::Continue
            }
            (ParamState::Option, CommandInput::Accept) => {
                self.state = ParamState::Name(EDIT);
                Step::Continue
            }
            (ParamState::Name(k), CommandInput::Text(name)) => {
                let found = ctx.drawing.constraints().find(name.trim()).filter(|id| {
                    ctx.drawing
                        .constraints()
                        .get(*id)
                        .is_some_and(|c| c.kind.is_dimensional())
                });
                let Some(id) = found else {
                    ctx.message(format!("No parameter called \"{}\".", name.trim()));
                    return Step::Finished;
                };
                if k == DELETE {
                    let mut transaction = ctx.transaction();
                    transaction
                        .remove_constraint(id)
                        .expect("a constraint of the drawing");
                    transaction.commit();
                    ctx.message(format!("{} deleted.", name.trim()));
                    return Step::Finished;
                }
                self.state = if k == RENAME {
                    ParamState::NewName(id)
                } else {
                    ParamState::Value(id)
                };
                Step::Continue
            }
            (ParamState::Value(id), CommandInput::Distance(typed)) => {
                let Some(mut constraint) = ctx.drawing.constraints().get(id).cloned() else {
                    return Step::Finished;
                };
                constraint.kind = with_typed(&constraint, typed);
                let said = describe(&constraint);
                let mut transaction = ctx.transaction();
                match transaction.replace_constraint(id, constraint) {
                    Ok(()) => {
                        if transaction.commit().refused.is_none() {
                            ctx.message(said);
                        }
                    }
                    Err(error) => ctx.message(sentence(&error.to_string())),
                }
                Step::Finished
            }
            (ParamState::NewName(id), CommandInput::Text(name)) => {
                let Some(mut constraint) = ctx.drawing.constraints().get(id).cloned() else {
                    return Step::Finished;
                };
                let name = name.trim().to_owned();
                if cad_document::check_name(&name).is_err() {
                    ctx.message(format!("\"{name}\" cannot be a parameter's name."));
                    return Step::Finished;
                }
                constraint.name.clone_from(&name);
                let mut transaction = ctx.transaction();
                match transaction.replace_constraint(id, constraint) {
                    Ok(()) => {
                        transaction.commit();
                        ctx.message(format!("Renamed {name}."));
                    }
                    Err(TransactionError::DuplicateConstraintName(_)) => {
                        ctx.message(format!("A parameter is already called \"{name}\"."));
                    }
                    Err(error) => ctx.message(sentence(&error.to_string())),
                }
                Step::Finished
            }
            (state, _) => {
                self.state = state;
                Step::Continue
            }
        }
    }
}
