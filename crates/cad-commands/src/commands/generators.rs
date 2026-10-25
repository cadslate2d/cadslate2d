//! The part generators as commands (#272): PLATE, SLOT, HOLEPATTERN,
//! BOLTCIRCLE, and FLANGE. Each asks for its parameters in turn — a value
//! offered as the default at every prompt — previews the part as it would
//! be made, and makes it, constrained (`cad_document::generate`), as one
//! undo. An impossible part is refused with the reason.
//!
//! One engine serves them: a command is a list of fields, each asked when
//! the answers so far call for it, and a function from the answers to the
//! part.

use std::collections::BTreeMap;

use cad_core::Point2;
use cad_document::generate::{CornerHoles, Corners, Spec};
use cad_input::{ExpectedInput, InputKind, Keyword};

use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

/// What a field asks for.
#[derive(Clone, Copy, Debug)]
enum Ask {
    Point,
    /// A point, or a keyword instead of one.
    PointOr(Keyword),
    Length(f64),
    /// A length, or a keyword for none; none by default.
    LengthOr(Keyword),
    Count(u32),
    /// Degrees.
    Angle(f64),
    /// One of the keywords; the first by default.
    Choice(&'static [Keyword]),
}

/// An answer.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Value {
    Point(Point2),
    Number(f64),
    /// The index of the keyword chosen.
    Choice(usize),
    /// The keyword of a `PointOr` or a `LengthOr`.
    Keyword,
}

type Answers = BTreeMap<&'static str, Value>;

struct Field {
    key: &'static str,
    prompt: &'static str,
    ask: Ask,
    /// Whether the answers so far call for it.
    when: fn(&Answers) -> bool,
}

const fn field(key: &'static str, prompt: &'static str, ask: Ask) -> Field {
    Field {
        key,
        prompt,
        ask,
        when: |_| true,
    }
}

const fn field_when(
    key: &'static str,
    prompt: &'static str,
    ask: Ask,
    when: fn(&Answers) -> bool,
) -> Field {
    Field {
        key,
        prompt,
        ask,
        when,
    }
}

/// A generator: its name for the message, its fields, and the part its
/// answers make.
pub(crate) struct Form {
    name: &'static str,
    fields: &'static [Field],
    build: fn(&Answers) -> Option<Spec>,
}

impl std::fmt::Debug for Form {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

fn point(answers: &Answers, key: &str) -> Option<Point2> {
    match answers.get(key)? {
        Value::Point(p) => Some(*p),
        _ => None,
    }
}

/// A number answered, or the field's default.
fn number(fields: &[Field], answers: &Answers, key: &str) -> Option<f64> {
    match answers.get(key) {
        Some(Value::Number(n)) => Some(*n),
        Some(_) => None,
        None => fields
            .iter()
            .find(|f| f.key == key)
            .and_then(|f| match f.ask {
                Ask::Length(d) | Ask::Angle(d) => Some(d),
                Ask::Count(n) => Some(f64::from(n)),
                _ => None,
            }),
    }
}

fn choice(answers: &Answers, key: &str) -> usize {
    match answers.get(key) {
        Some(Value::Choice(i)) => *i,
        _ => 0,
    }
}

fn count(value: f64) -> u32 {
    value.clamp(0.0, f64::from(u32::MAX)) as u32
}

// ── PLATE ──

const DIMENSIONS: Keyword = Keyword::new("Dimensions");
const NONE: Keyword = Keyword::new("None");
const CORNERS: &[Keyword] = &[
    Keyword::new("Square"),
    Keyword::new("Fillet"),
    Keyword::new("Chamfer"),
];
const SHAPES: &[Keyword] = &[Keyword::new("Round"), Keyword::new("REctangular")];

static PLATE_FIELDS: [Field; 9] = [
    field("first", "Specify first corner", Ask::Point),
    field("other", "Specify other corner", Ask::PointOr(DIMENSIONS)),
    field_when("width", "Specify width", Ask::Length(100.0), |a| {
        a.get("other") == Some(&Value::Keyword)
    }),
    field_when("height", "Specify height", Ask::Length(50.0), |a| {
        a.get("other") == Some(&Value::Keyword)
    }),
    field("corners", "Enter corner type", Ask::Choice(CORNERS)),
    field_when("radius", "Specify fillet radius", Ask::Length(5.0), |a| {
        a.get("corners") == Some(&Value::Choice(1))
    }),
    field_when(
        "distance",
        "Specify chamfer distance",
        Ask::Length(5.0),
        |a| a.get("corners") == Some(&Value::Choice(2)),
    ),
    field("holes", "Specify corner hole diameter", Ask::LengthOr(NONE)),
    field_when(
        "offset",
        "Specify hole offset from the edges",
        Ask::Length(10.0),
        |a| matches!(a.get("holes"), Some(Value::Number(_))),
    ),
];

fn plate(a: &Answers) -> Option<Spec> {
    let f = &PLATE_FIELDS;
    let first = point(a, "first")?;
    let (corner, width, height) = match a.get("other") {
        Some(Value::Point(other)) => (
            Point2::new(first.x.min(other.x), first.y.min(other.y)),
            (other.x - first.x).abs(),
            (other.y - first.y).abs(),
        ),
        _ => (first, number(f, a, "width")?, number(f, a, "height")?),
    };
    let corners = match choice(a, "corners") {
        1 => Corners::Fillet(number(f, a, "radius")?),
        2 => Corners::Chamfer(number(f, a, "distance")?),
        _ => Corners::Square,
    };
    let holes = match a.get("holes") {
        Some(Value::Number(diameter)) => Some(CornerHoles {
            diameter: *diameter,
            offset: number(f, a, "offset")?,
        }),
        _ => None,
    };
    Some(Spec::Plate {
        corner,
        width,
        height,
        corners,
        holes,
    })
}

// ── SLOT ──

static SLOT_FIELDS: [Field; 4] = [
    field("center", "Specify centre", Ask::Point),
    field(
        "length",
        "Specify length between centres",
        Ask::Length(40.0),
    ),
    field("width", "Specify width", Ask::Length(10.0)),
    field("angle", "Specify angle", Ask::Angle(0.0)),
];

fn slot(a: &Answers) -> Option<Spec> {
    let f = &SLOT_FIELDS;
    Some(Spec::Slot {
        center: point(a, "center")?,
        length: number(f, a, "length")?,
        width: number(f, a, "width")?,
        angle: number(f, a, "angle")?.to_radians(),
    })
}

// ── HOLEPATTERN ──

static HOLE_PATTERN_FIELDS: [Field; 6] = [
    field("first", "Specify first hole centre", Ask::Point),
    field("diameter", "Specify hole diameter", Ask::Length(10.0)),
    field("rows", "Enter number of rows", Ask::Count(2)),
    field("columns", "Enter number of columns", Ask::Count(2)),
    field_when(
        "row_spacing",
        "Specify row spacing",
        Ask::Length(25.0),
        |a| a.get("rows") != Some(&Value::Number(1.0)),
    ),
    field_when(
        "column_spacing",
        "Specify column spacing",
        Ask::Length(25.0),
        |a| a.get("columns") != Some(&Value::Number(1.0)),
    ),
];

fn hole_pattern(a: &Answers) -> Option<Spec> {
    let f = &HOLE_PATTERN_FIELDS;
    Some(Spec::HolePattern {
        first: point(a, "first")?,
        diameter: number(f, a, "diameter")?,
        rows: count(number(f, a, "rows")?),
        columns: count(number(f, a, "columns")?),
        row_spacing: number(f, a, "row_spacing")?,
        column_spacing: number(f, a, "column_spacing")?,
    })
}

// ── BOLTCIRCLE ──

static BOLT_CIRCLE_FIELDS: [Field; 5] = [
    field("center", "Specify centre", Ask::Point),
    field("pitch", "Specify pitch circle diameter", Ask::Length(100.0)),
    field("count", "Enter number of holes", Ask::Count(6)),
    field("hole", "Specify hole diameter", Ask::Length(10.0)),
    field("start", "Specify start angle", Ask::Angle(0.0)),
];

fn bolt_circle(a: &Answers) -> Option<Spec> {
    let f = &BOLT_CIRCLE_FIELDS;
    Some(Spec::BoltCircle {
        center: point(a, "center")?,
        pitch_diameter: number(f, a, "pitch")?,
        count: count(number(f, a, "count")?),
        hole_diameter: number(f, a, "hole")?,
        start_angle: number(f, a, "start")?.to_radians(),
    })
}

// ── FLANGE ──

fn round(a: &Answers) -> bool {
    a.get("shape") != Some(&Value::Choice(1))
}

fn rectangular(a: &Answers) -> bool {
    !round(a)
}

static FLANGE_FIELDS: [Field; 10] = [
    field("shape", "Enter flange shape", Ask::Choice(SHAPES)),
    field("center", "Specify centre", Ask::Point),
    field_when("outer", "Specify outer diameter", Ask::Length(150.0), round),
    field_when("width", "Specify width", Ask::Length(120.0), rectangular),
    field_when("height", "Specify height", Ask::Length(120.0), rectangular),
    field("bore", "Specify bore diameter", Ask::Length(50.0)),
    field_when(
        "pitch",
        "Specify pitch circle diameter",
        Ask::Length(110.0),
        round,
    ),
    field_when("count", "Enter number of holes", Ask::Count(8), round),
    field("hole", "Specify hole diameter", Ask::Length(12.0)),
    field_when(
        "offset",
        "Specify hole offset from the edges",
        Ask::Length(15.0),
        rectangular,
    ),
];

fn flange(a: &Answers) -> Option<Spec> {
    let f = &FLANGE_FIELDS;
    let center = point(a, "center")?;
    Some(if round(a) {
        Spec::RoundFlange {
            center,
            outer_diameter: number(f, a, "outer")?,
            bore_diameter: number(f, a, "bore")?,
            pitch_diameter: number(f, a, "pitch")?,
            count: count(number(f, a, "count")?),
            hole_diameter: number(f, a, "hole")?,
        }
    } else {
        Spec::RectangularFlange {
            center,
            width: number(f, a, "width")?,
            height: number(f, a, "height")?,
            bore_diameter: number(f, a, "bore")?,
            hole_diameter: number(f, a, "hole")?,
            offset: number(f, a, "offset")?,
        }
    })
}

pub(crate) static PLATE: Form = Form {
    name: "Plate",
    fields: &PLATE_FIELDS,
    build: plate,
};
pub(crate) static SLOT: Form = Form {
    name: "Slot",
    fields: &SLOT_FIELDS,
    build: slot,
};
pub(crate) static HOLE_PATTERN: Form = Form {
    name: "Hole pattern",
    fields: &HOLE_PATTERN_FIELDS,
    build: hole_pattern,
};
pub(crate) static BOLT_CIRCLE: Form = Form {
    name: "Bolt circle",
    fields: &BOLT_CIRCLE_FIELDS,
    build: bolt_circle,
};
pub(crate) static FLANGE: Form = Form {
    name: "Flange",
    fields: &FLANGE_FIELDS,
    build: flange,
};

/// A generator command: a form and its answers so far.
#[derive(Debug)]
pub(crate) struct GeneratorCommand {
    form: &'static Form,
    answers: Answers,
}

impl GeneratorCommand {
    pub(crate) fn new(form: &'static Form) -> Self {
        Self {
            form,
            answers: Answers::new(),
        }
    }

    fn next(&self) -> Option<&'static Field> {
        self.form
            .fields
            .iter()
            .find(|f| !self.answers.contains_key(f.key) && (f.when)(&self.answers))
    }

    /// The point a length is measured from: the first point answered.
    fn base(&self) -> Option<Point2> {
        self.form
            .fields
            .iter()
            .find_map(|f| point(&self.answers, f.key))
    }

    /// Makes the part; what to say.
    fn make(&self, ctx: &mut CommandContext<'_>) -> String {
        let Some(spec) = (self.form.build)(&self.answers) else {
            return "Not enough to make it.".to_owned();
        };
        let mut transaction = ctx.transaction();
        match spec.place(&mut transaction) {
            Ok(ids) => {
                let constraints = transaction.added_constraints();
                if transaction.commit().refused.is_some() {
                    return String::new();
                }
                format!(
                    "{}: {} objects, {constraints} constraints.",
                    self.form.name,
                    ids.len()
                )
            }
            Err(error) => error.to_string(),
        }
    }
}

impl Command for GeneratorCommand {
    fn start(&mut self, _ctx: &mut CommandContext<'_>) -> Step {
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        let Some(field) = self.next() else {
            return Prompt::new("");
        };
        let prompt = Prompt::new(field.prompt);
        let show = |n: f64| {
            let text = format!("{n:.4}");
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        };
        match field.ask {
            Ask::Length(d) | Ask::Angle(d) => prompt.with_default(show(d)),
            Ask::Count(n) => prompt.with_default(n.to_string()),
            Ask::LengthOr(keyword) => prompt.with_default(keyword.name()),
            Ask::Choice(keywords) => prompt.with_default(keywords[0].name()),
            Ask::Point | Ask::PointOr(_) => prompt,
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.next().map(|f| f.ask) {
            Some(Ask::Point) | None => ExpectedInput::point(),
            Some(Ask::PointOr(keyword)) => ExpectedInput::point().with_keywords(&[keyword]),
            Some(Ask::Length(_)) => ExpectedInput::distance(),
            Some(Ask::LengthOr(keyword)) => ExpectedInput::distance().with_keywords(&[keyword]),
            Some(Ask::Count(_)) => ExpectedInput::factor(),
            Some(Ask::Angle(_)) => ExpectedInput::angle(),
            Some(Ask::Choice(keywords)) => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(keywords)
            }
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let Some(field) = self.next() else {
            return Step::Finished;
        };
        let value = match (field.ask, input) {
            (Ask::Point | Ask::PointOr(_), CommandInput::Point(p)) => Some(Value::Point(p)),
            (Ask::PointOr(_) | Ask::LengthOr(_), CommandInput::Keyword(_)) => Some(Value::Keyword),
            (Ask::LengthOr(_), CommandInput::Accept) => Some(Value::Keyword),
            (Ask::Length(_) | Ask::LengthOr(_), CommandInput::Distance(d))
            | (Ask::Angle(_), CommandInput::Angle(d)) => Some(Value::Number(d)),
            (Ask::Length(d) | Ask::Angle(d), CommandInput::Accept) => Some(Value::Number(d)),
            (Ask::Count(n), CommandInput::Accept) => Some(Value::Number(f64::from(n))),
            (Ask::Count(_), CommandInput::Distance(d)) => {
                if d.fract() == 0.0 && d >= 1.0 && d <= f64::from(u32::MAX) {
                    Some(Value::Number(d))
                } else {
                    ctx.message("Enter a whole number, 1 or more.");
                    None
                }
            }
            (Ask::Choice(keywords), CommandInput::Keyword(k)) => {
                keywords.iter().position(|w| *w == k).map(Value::Choice)
            }
            (Ask::Choice(_), CommandInput::Accept) => Some(Value::Choice(0)),
            _ => None,
        };
        if let Some(value) = value {
            self.answers.insert(field.key, value);
        }
        if self.next().is_some() {
            return Step::Continue;
        }
        let said = self.make(ctx);
        if !said.is_empty() {
            ctx.message(said);
        }
        Step::Finished
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        // The part as it would be with the cursor for the point asked for
        // now, and every value not yet given at its default.
        let mut answers = self.answers.clone();
        if let Some(field) = self.next()
            && matches!(field.ask, Ask::Point | Ask::PointOr(_))
        {
            answers.insert(field.key, Value::Point(cursor));
        }
        if let Some(shapes) = (self.form.build)(&answers).and_then(|spec| spec.shapes().ok()) {
            out.extend(shapes.entities.into_iter().map(OverlayItem::PreviewEntity));
        }
    }

    fn base_point(&self) -> Option<Point2> {
        self.base()
    }
}
