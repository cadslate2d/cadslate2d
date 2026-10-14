//! LENGTHEN (#201): a line, an arc, or an open polyline made longer or
//! shorter at the end nearer the pick — by a delta, to a percentage, to
//! a total, by an angle for an arc, or dynamically, the end following
//! the pointer along the object's own line or circle. The operation is
//! `Entity::lengthened`; the command only asks and previews.

use cad_core::{EntityId, Point2};
use cad_document::{Entity, LengthenError, Lengthening};
use cad_input::{ExpectedInput, InputKind, Keyword};

use crate::summary::{degrees, number};
use crate::{Command, CommandContext, CommandInput, HoveredObject, OverlayItem, Prompt, Step};

const DELTA: Keyword = Keyword::new("DElta");
const PERCENT: Keyword = Keyword::new("Percent");
const TOTAL: Keyword = Keyword::new("Total");
const DYNAMIC: Keyword = Keyword::new("DYnamic");
const MODES: [Keyword; 4] = [DELTA, PERCENT, TOTAL, DYNAMIC];
const ANGLE: Keyword = Keyword::new("Angle");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Delta,
    Percent,
    Total,
}

#[derive(Debug)]
enum State {
    /// An object to measure, or a mode.
    Measure,
    /// The amount for a mode; `angle` once `Angle` was chosen.
    Amount { mode: Mode, angle: bool },
    /// Objects to change by `how`, until Enter.
    Change(Lengthening),
    /// Dynamic: an object to drag the end of.
    DynamicPick,
    /// Dynamic: the end nearer `pick` of `id` — the object as it was
    /// picked — follows the pointer.
    DynamicEnd {
        id: EntityId,
        pick: Point2,
        entity: Entity,
    },
}

#[derive(Debug)]
pub(crate) struct LengthenCommand {
    state: State,
}

impl LengthenCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Measure,
        }
    }

    fn explain(error: LengthenError) -> String {
        match error {
            LengthenError::Unsupported(kind) => {
                let article = if kind.starts_with(['a', 'e', 'i', 'o', 'u']) {
                    "An"
                } else {
                    "A"
                };
                format!("{article} {kind} cannot be lengthened.")
            }
            LengthenError::Vanishes => "That would leave nothing of the object.".to_owned(),
            LengthenError::FullCircle => "An arc cannot reach its full circle.".to_owned(),
            LengthenError::NotAnArc => "Only an arc has an angle to change.".to_owned(),
            LengthenError::BadAmount => "The amount must be a positive number.".to_owned(),
        }
    }

    /// The object as the dynamic drag would leave it: its end nearer the
    /// pick carried to the point of its own line or circle nearest the
    /// cursor.
    fn dragged(entity: &Entity, pick: Point2, cursor: Point2) -> Result<Entity, LengthenError> {
        let delta = match entity {
            Entity::Line(line) => {
                let along = line.end - line.start;
                let length = along.length();
                if length == 0.0 {
                    return Err(LengthenError::Vanishes);
                }
                let direction = along * (1.0 / length);
                let from_end = pick.distance_to(line.end) <= pick.distance_to(line.start);
                if from_end {
                    (cursor - line.end).dot(direction)
                } else {
                    -(cursor - line.start).dot(direction)
                }
            }
            Entity::Arc(arc) => {
                let geometry = arc.arc();
                let from_end = pick.distance_to(geometry.end_point())
                    <= pick.distance_to(geometry.start_point());
                let offset = cursor - geometry.center;
                let angle = offset.y.atan2(offset.x);
                // How far round from the tip the cursor's direction is, in
                // the direction of growth, taken the short way.
                let turn = if from_end {
                    cad_geometry::normalize_angle(angle - geometry.end())
                } else {
                    cad_geometry::normalize_angle(geometry.start - angle)
                };
                let turn = if turn > std::f64::consts::PI {
                    turn - std::f64::consts::TAU
                } else {
                    turn
                };
                turn * geometry.radius
            }
            Entity::Polyline(polyline) if !polyline.closed => {
                // The end segment, straight or an arc, dragged like a line
                // or an arc of its own.
                let path = polyline.path();
                let from_end = polyline.vertices.last().is_some_and(|last| {
                    pick.distance_to(*last) <= pick.distance_to(polyline.vertices[0])
                });
                let primitives = path.primitives();
                let primitive = if from_end {
                    primitives.last()
                } else {
                    primitives.first()
                }
                .ok_or(LengthenError::Vanishes)?;
                let piece: Entity = match primitive {
                    cad_geometry::Primitive::Segment(segment) => {
                        cad_document::LineEntity::new(segment.start, segment.end).into()
                    }
                    cad_geometry::Primitive::Arc(arc) => cad_document::ArcEntity::from(*arc).into(),
                    cad_geometry::Primitive::Circle(_) => return Err(LengthenError::Vanishes),
                };
                let tip = if from_end {
                    polyline.vertices[polyline.vertices.len() - 1]
                } else {
                    polyline.vertices[0]
                };
                let dragged = Self::dragged(&piece, tip, cursor)?;
                dragged.curve_length().unwrap_or(0.0) - piece.curve_length().unwrap_or(0.0)
            }
            other => return Err(LengthenError::Unsupported(other.kind_name())),
        };
        entity.lengthened(pick, Lengthening::Delta(delta))
    }

    fn change(ctx: &mut CommandContext<'_>, id: EntityId, result: Result<Entity, LengthenError>) {
        match result {
            Ok(entity) => {
                let mut tx = ctx.transaction();
                tx.replace(id, entity).expect("the picked object is live");
                tx.commit();
            }
            Err(error) => ctx.message(Self::explain(error)),
        }
    }
}

impl Command for LengthenCommand {
    fn prompt(&self) -> Prompt {
        match &self.state {
            State::Measure => Prompt::new("Select an object to measure"),
            State::Amount {
                mode: Mode::Delta,
                angle: false,
            } => Prompt::new("Enter delta length"),
            State::Amount {
                mode: Mode::Delta,
                angle: true,
            } => Prompt::new("Enter delta angle"),
            State::Amount {
                mode: Mode::Percent,
                ..
            } => Prompt::new("Enter percentage length"),
            State::Amount {
                mode: Mode::Total,
                angle: false,
            } => Prompt::new("Specify total length"),
            State::Amount {
                mode: Mode::Total,
                angle: true,
            } => Prompt::new("Specify total angle"),
            State::Change(_) | State::DynamicPick => Prompt::new("Select an object to change"),
            State::DynamicEnd { .. } => Prompt::new("Specify new end point"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match &self.state {
            State::Measure => ExpectedInput::new(InputKind::Object).with_keywords(&MODES),
            State::Amount {
                mode: Mode::Delta | Mode::Total,
                angle: false,
            } => ExpectedInput::distance().with_keywords(&[ANGLE]),
            State::Amount { angle: true, .. } => ExpectedInput::angle(),
            State::Amount {
                mode: Mode::Percent,
                ..
            } => ExpectedInput::factor(),
            State::Change(_) | State::DynamicPick => ExpectedInput::new(InputKind::Object),
            State::DynamicEnd { .. } => ExpectedInput::point(),
        }
    }

    fn takes_object_snap(&self) -> bool {
        // The new end lies on the object's own line or circle: a snap would
        // only pull it off.
        !matches!(self.state, State::DynamicEnd { .. })
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Measure, CommandInput::Pick { entity, .. }) => {
                match ctx.drawing.entity(entity) {
                    Some(picked @ Entity::Arc(arc)) => {
                        let geometry = arc.arc();
                        ctx.message(format!(
                            "Current length: {}, included angle: {}",
                            number(picked.curve_length().unwrap_or(0.0)),
                            degrees(geometry.sweep)
                        ));
                    }
                    Some(picked) => match picked.curve_length() {
                        Some(length) => ctx.message(format!("Current length: {}", number(length))),
                        None => ctx.message(Self::explain(LengthenError::Unsupported(
                            picked.kind_name(),
                        ))),
                    },
                    None => {}
                }
            }
            (State::Measure, CommandInput::Keyword(keyword)) => {
                self.state = match keyword {
                    DELTA => State::Amount {
                        mode: Mode::Delta,
                        angle: false,
                    },
                    PERCENT => State::Amount {
                        mode: Mode::Percent,
                        angle: false,
                    },
                    TOTAL => State::Amount {
                        mode: Mode::Total,
                        angle: false,
                    },
                    _ => State::DynamicPick,
                };
            }
            (State::Amount { mode, .. }, CommandInput::Keyword(_)) => {
                self.state = State::Amount {
                    mode: *mode,
                    angle: true,
                };
            }
            (State::Amount { mode, angle }, CommandInput::Distance(amount)) => {
                let how = match (mode, angle) {
                    (Mode::Delta, false) => Lengthening::Delta(amount),
                    (Mode::Percent, _) => Lengthening::Percent(amount),
                    (Mode::Total, false) => Lengthening::Total(amount),
                    (_, true) => return Step::Continue,
                };
                if let Lengthening::Percent(p) | Lengthening::Total(p) = how
                    && !(p.is_finite() && p > 0.0)
                {
                    ctx.message(Self::explain(LengthenError::BadAmount));
                    return Step::Continue;
                }
                self.state = State::Change(how);
            }
            (State::Amount { mode, angle: true }, CommandInput::Angle(amount)) => {
                self.state = State::Change(match mode {
                    Mode::Total => Lengthening::TotalAngle(amount),
                    _ => Lengthening::Angle(amount),
                });
            }
            (State::Change(how), CommandInput::Pick { entity, at }) => {
                let how = *how;
                let result = ctx
                    .drawing
                    .entity(entity)
                    .map(|picked| picked.lengthened(at, how));
                if let Some(result) = result {
                    Self::change(ctx, entity, result);
                }
            }
            (State::DynamicPick, CommandInput::Pick { entity, at }) => {
                match ctx.drawing.entity(entity).cloned() {
                    Some(picked) if picked.curve_length().is_some() => {
                        self.state = State::DynamicEnd {
                            id: entity,
                            pick: at,
                            entity: picked,
                        };
                    }
                    Some(picked) => {
                        ctx.message(Self::explain(LengthenError::Unsupported(
                            picked.kind_name(),
                        )));
                    }
                    None => {}
                }
            }
            (State::DynamicEnd { id, pick, entity }, CommandInput::Point(to)) => {
                let (id, result) = (*id, Self::dragged(entity, *pick, to));
                Self::change(ctx, id, result);
                self.state = State::DynamicPick;
            }
            (_, CommandInput::Accept) => return Step::Finished,
            _ => {}
        }
        Step::Continue
    }

    /// The object as a pick would leave it.
    fn preview_pick(&self, hovered: &HoveredObject<'_>, out: &mut Vec<OverlayItem>) {
        if let State::Change(how) = &self.state
            && let Ok(entity) = hovered.entity.lengthened(hovered.at, *how)
        {
            out.push(OverlayItem::PreviewEntity(entity));
        }
    }

    /// Dynamic: the object as the pointer would leave it.
    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if let State::DynamicEnd { pick, entity, .. } = &self.state
            && let Ok(dragged) = Self::dragged(entity, *pick, cursor)
        {
            out.push(OverlayItem::PreviewEntity(dragged));
        }
    }

    /// While an end is dragged, the object is not a snap target.
    fn transient_sources(&self) -> &[EntityId] {
        match &self.state {
            State::DynamicEnd { id, .. } => std::slice::from_ref(id),
            _ => &[],
        }
    }
}
