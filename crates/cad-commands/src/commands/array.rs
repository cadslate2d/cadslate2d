//! ARRAY (#199): copies of a selection laid out in rows and columns, or
//! round a centre. Every copy is a transformed `insert_like` of its
//! source — a block reference, a dimension, a text, a hatch copy as they
//! do under COPY and ROTATE — in one transaction, one undo. The array is
//! not associative: its copies are ordinary entities. While the numbers
//! are typed, the whole array is previewed from the same transforms the
//! commit uses, the one still being asked for read off the pointer.

use cad_core::{Aabb, Point2, Vec2};
use cad_geometry::Transform;
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::ltscale::display;
use super::modify::{SELECT_OBJECTS, adopt_selection, selection_expected};
use crate::command::angle_degrees;
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step, TransientEdit};

const RECTANGULAR: Keyword = Keyword::new("Rectangular");
const POLAR: Keyword = Keyword::new("Polar");
const YES: Keyword = Keyword::new("Yes");
const NO: Keyword = Keyword::new("No");
const COUNT_LIMIT: f64 = 10_000.0;

/// A rectangular array: so many rows and columns, so far apart. A
/// negative spacing goes the other way.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rectangular {
    pub rows: usize,
    pub columns: usize,
    pub row_spacing: f64,
    pub column_spacing: f64,
}

impl Rectangular {
    /// The transform of every copy but the original: column by column
    /// along each row.
    pub fn transforms(&self) -> Vec<Transform> {
        (0..self.rows)
            .flat_map(|row| (0..self.columns).map(move |column| (row, column)))
            .filter(|(row, column)| *row > 0 || *column > 0)
            .map(|(row, column)| {
                Transform::translation(Vec2::new(
                    column as f64 * self.column_spacing,
                    row as f64 * self.row_spacing,
                ))
            })
            .collect()
    }
}

/// A polar array: so many items round a centre, spread over an angle —
/// the whole turn, or part of it, counter-clockwise when positive — each
/// turned as it goes round, or kept upright and moved.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Polar {
    pub center: Point2,
    pub count: usize,
    pub fill_degrees: f64,
    pub rotate_items: bool,
    /// The point an item is placed by when it is not turned: the
    /// selection's centre.
    pub reference: Point2,
}

impl Polar {
    /// The angle between neighbours: the fill over the count round a
    /// whole turn, over one less than the count otherwise.
    pub fn step_degrees(&self) -> f64 {
        if (self.fill_degrees.abs() - 360.0).abs() < 1e-9 {
            self.fill_degrees / self.count as f64
        } else {
            self.fill_degrees / (self.count as f64 - 1.0)
        }
    }

    /// The transform of every item but the original.
    pub fn transforms(&self) -> Vec<Transform> {
        let step = self.step_degrees();
        (1..self.count)
            .map(|i| {
                let turn = Transform::rotation(self.center, step * i as f64);
                if self.rotate_items {
                    turn
                } else {
                    Transform::translation(turn.apply(self.reference) - self.reference)
                }
            })
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Rectangular,
    Polar,
}

#[derive(Debug)]
enum State {
    Selecting,
    Kind,
    Rows,
    Columns,
    RowSpacing,
    ColumnSpacing,
    Center,
    Count,
    Fill,
    Rotate,
}

#[derive(Debug)]
pub(crate) struct ArrayCommand {
    state: State,
    edit: TransientEdit,
    /// The selection's bounds, for the preview's spacings and the polar
    /// array's reference point.
    bounds: Aabb,
    kind: Kind,
    rectangular: Rectangular,
    polar: Polar,
}

impl ArrayCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Selecting,
            edit: TransientEdit::default(),
            bounds: Aabb::from_points(Point2::ORIGIN, Point2::ORIGIN),
            kind: Kind::Rectangular,
            rectangular: Rectangular {
                rows: 1,
                columns: 1,
                row_spacing: 0.0,
                column_spacing: 0.0,
            },
            polar: Polar {
                center: Point2::ORIGIN,
                count: 1,
                fill_degrees: 360.0,
                rotate_items: true,
                reference: Point2::ORIGIN,
            },
        }
    }

    fn adopt(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        match adopt_selection(ctx) {
            Some(edit) => {
                self.bounds = edit
                    .sources()
                    .iter()
                    .filter_map(|id| ctx.drawing.entity(*id))
                    .map(|entity| ctx.drawing.bounds_of(entity))
                    .reduce(|a, b| a.union(&b))
                    .unwrap_or(self.bounds);
                self.polar.reference = self.bounds.center();
                self.edit = edit;
                self.state = State::Kind;
                Step::Continue
            }
            None => Step::Finished,
        }
    }

    /// The transforms the array as it stands commits, or previews.
    fn transforms(&self) -> Vec<Transform> {
        match self.kind {
            Kind::Rectangular => self.rectangular.transforms(),
            Kind::Polar => self.polar.transforms(),
        }
    }

    fn commit(&self, ctx: &mut CommandContext<'_>) -> Step {
        let transforms = self.transforms();
        if transforms.is_empty() {
            ctx.message("An array of one is the selection as it is.");
            ctx.selection.clear();
            return Step::Finished;
        }
        let ids = self.edit.commit_copies_under(ctx.drawing, &transforms);
        ctx.message(match self.kind {
            Kind::Rectangular => format!(
                "{} rows by {} columns: {} copies made.",
                self.rectangular.rows,
                self.rectangular.columns,
                ids.len()
            ),
            Kind::Polar => format!(
                "{} items round {}, {}: {} copies made.",
                self.polar.count,
                display_point(self.polar.center),
                format_fill(self.polar.fill_degrees),
                ids.len()
            ),
        });
        ctx.selection.clear();
        Step::Finished
    }

    /// A count typed: a whole number from `least` up.
    fn count(value: f64, least: f64) -> Option<usize> {
        let whole = value.round();
        (value.is_finite()
            && (value - whole).abs() < 1e-9
            && (least..=COUNT_LIMIT).contains(&whole))
        .then_some(whole as usize)
    }
}

fn display_point(point: Point2) -> String {
    format!("{},{}", display(point.x), display(point.y))
}

fn format_fill(degrees: f64) -> String {
    format!("over {}°", display(degrees))
}

impl Command for ArrayCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if ctx.selection.is_empty() {
            Step::Continue
        } else {
            self.adopt(ctx)
        }
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Selecting => Prompt::new(SELECT_OBJECTS),
            State::Kind => Prompt::new("Enter the type of array").with_default("Rectangular"),
            State::Rows => Prompt::new("Enter the number of rows (---)").with_default("1"),
            State::Columns => Prompt::new("Enter the number of columns (|||)").with_default("1"),
            State::RowSpacing => Prompt::new("Specify the distance between rows"),
            State::ColumnSpacing => Prompt::new("Specify the distance between columns"),
            State::Center => Prompt::new("Specify center point of array"),
            State::Count => Prompt::new("Enter the number of items in the array"),
            State::Fill => {
                Prompt::new("Specify the angle to fill (+=ccw, -=cw)").with_default("360")
            }
            State::Rotate => Prompt::new("Rotate arrayed objects?").with_default("Yes"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Selecting => selection_expected(),
            State::Kind => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&[RECTANGULAR, POLAR])
            }
            State::Rows | State::Columns | State::Count => ExpectedInput::factor(),
            // A spacing may be negative: it is typed, or measured from
            // the selection's corner with the pointer.
            State::RowSpacing | State::ColumnSpacing => ExpectedInput::distance(),
            State::Center => ExpectedInput::point(),
            State::Fill => ExpectedInput::angle(),
            State::Rotate => ExpectedInput::new(InputKind::Nothing).with_keywords(&[YES, NO]),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Selecting, CommandInput::Accept) => return self.adopt(ctx),
            (State::Selecting, _) => {}
            (State::Kind, CommandInput::Keyword(POLAR)) => {
                self.kind = Kind::Polar;
                self.state = State::Center;
            }
            (State::Kind, CommandInput::Keyword(_) | CommandInput::Accept) => {
                self.kind = Kind::Rectangular;
                self.state = State::Rows;
            }
            (State::Rows, CommandInput::Distance(rows)) => match Self::count(rows, 1.0) {
                Some(rows) => {
                    self.rectangular.rows = rows;
                    self.state = State::Columns;
                }
                None => ctx.message("Enter a whole number of rows, 1 or more."),
            },
            (State::Rows, CommandInput::Accept) => self.state = State::Columns,
            (State::Columns, CommandInput::Distance(columns)) => match Self::count(columns, 1.0) {
                Some(columns) => {
                    self.rectangular.columns = columns;
                    match self.after_columns(ctx) {
                        Some(state) => self.state = state,
                        None => return Step::Finished,
                    }
                }
                None => ctx.message("Enter a whole number of columns, 1 or more."),
            },
            (State::Columns, CommandInput::Accept) => match self.after_columns(ctx) {
                Some(state) => self.state = state,
                None => return Step::Finished,
            },
            (State::RowSpacing, CommandInput::Distance(spacing)) => {
                if !spacing.is_finite() || spacing == 0.0 {
                    ctx.message("The distance between rows cannot be zero.");
                } else {
                    self.rectangular.row_spacing = spacing;
                    self.state = if self.rectangular.columns > 1 {
                        State::ColumnSpacing
                    } else {
                        return self.commit(ctx);
                    };
                }
            }
            (State::ColumnSpacing, CommandInput::Distance(spacing)) => {
                if !spacing.is_finite() || spacing == 0.0 {
                    ctx.message("The distance between columns cannot be zero.");
                } else {
                    self.rectangular.column_spacing = spacing;
                    return self.commit(ctx);
                }
            }
            (State::Center, CommandInput::Point(center)) => {
                self.polar.center = center;
                self.state = State::Count;
            }
            (State::Count, CommandInput::Distance(count)) => match Self::count(count, 2.0) {
                Some(count) => {
                    self.polar.count = count;
                    self.state = State::Fill;
                }
                None => ctx.message("Enter a whole number of items, 2 or more."),
            },
            (State::Fill, CommandInput::Angle(degrees)) => {
                if !degrees.is_finite() || degrees == 0.0 || degrees.abs() > 360.0 {
                    ctx.message("The angle to fill is up to 360°, and not zero.");
                } else {
                    self.polar.fill_degrees = degrees;
                    self.state = State::Rotate;
                }
            }
            (State::Fill, CommandInput::Accept) => self.state = State::Rotate,
            (State::Rotate, CommandInput::Keyword(keyword)) => {
                self.polar.rotate_items = keyword == YES;
                return self.commit(ctx);
            }
            (State::Rotate, CommandInput::Accept) => return self.commit(ctx),
            (_, CommandInput::Accept) => {
                ctx.selection.clear();
                return Step::Finished;
            }
            _ => {}
        }
        Step::Continue
    }

    /// The whole array as it stands, the number still being asked for
    /// read off the pointer: a spacing as the pointer's offset from the
    /// selection's corner, the fill as the pointer's angle about the
    /// centre. Nothing before a count is known.
    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        let corner = self.bounds.min();
        let transforms = match (self.kind, &self.state) {
            (Kind::Rectangular, State::RowSpacing) => Rectangular {
                row_spacing: cursor.y - corner.y,
                column_spacing: cursor.x - corner.x,
                ..self.rectangular
            }
            .transforms(),
            (Kind::Rectangular, State::ColumnSpacing) => Rectangular {
                column_spacing: cursor.x - corner.x,
                ..self.rectangular
            }
            .transforms(),
            (Kind::Polar, State::Fill) => {
                let degrees = angle_degrees(cursor - self.polar.center);
                Polar {
                    fill_degrees: if degrees == 0.0 { 360.0 } else { degrees },
                    ..self.polar
                }
                .transforms()
            }
            (Kind::Polar, State::Rotate) => self.polar.transforms(),
            _ => return,
        };
        if self.kind == Kind::Polar {
            out.push(OverlayItem::PreviewSegment {
                start: self.polar.center,
                end: cursor,
            });
        }
        for transform in transforms {
            self.edit.preview(transform, out);
        }
    }

    fn base_point(&self) -> Option<Point2> {
        match self.state {
            State::Count | State::Fill | State::Rotate => Some(self.polar.center),
            State::RowSpacing | State::ColumnSpacing => Some(self.bounds.min()),
            _ => None,
        }
    }
}

impl ArrayCommand {
    /// After the columns: a spacing for whatever there is more than one
    /// of; an array of one is done at once.
    fn after_columns(&mut self, ctx: &mut CommandContext<'_>) -> Option<State> {
        if self.rectangular.rows > 1 {
            Some(State::RowSpacing)
        } else if self.rectangular.columns > 1 {
            Some(State::ColumnSpacing)
        } else {
            ctx.message("An array of one is the selection as it is.");
            ctx.selection.clear();
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rectangular_array_is_every_cell_but_the_first_and_a_polar_one_every_item_but_the_first() {
        let grid = Rectangular {
            rows: 2,
            columns: 3,
            row_spacing: 10.0,
            column_spacing: -5.0,
        };
        let moved: Vec<Point2> = grid
            .transforms()
            .iter()
            .map(|t| t.apply(Point2::ORIGIN))
            .collect();
        assert_eq!(
            moved,
            vec![
                Point2::new(-5.0, 0.0),
                Point2::new(-10.0, 0.0),
                Point2::new(0.0, 10.0),
                Point2::new(-5.0, 10.0),
                Point2::new(-10.0, 10.0),
            ]
        );
        let round = Polar {
            center: Point2::ORIGIN,
            count: 4,
            fill_degrees: 360.0,
            rotate_items: true,
            reference: Point2::new(10.0, 0.0),
        };
        assert_eq!(round.step_degrees(), 90.0);
        assert_eq!(round.transforms().len(), 3);
        let half = Polar {
            fill_degrees: 180.0,
            ..round
        };
        assert_eq!(half.step_degrees(), 60.0);
        let upright = Polar {
            rotate_items: false,
            ..round
        };
        let at = upright.transforms()[0].apply(Point2::new(10.0, 0.0));
        assert!((at.x).abs() < 1e-9 && (at.y - 10.0).abs() < 1e-9);
        let still = upright.transforms()[0].apply(Point2::new(12.0, 0.0));
        assert!(
            (still.x - 2.0).abs() < 1e-9 && (still.y - 10.0).abs() < 1e-9,
            "moved, not turned"
        );
    }
}
