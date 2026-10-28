//! Each operation as a registered command and its answers (ADR-0037): what
//! a user would type, typed. The engine runs them through its one entry
//! point; this module knows nothing of the drawing.

use cad_core::Point2;

use crate::{
    Color, Intent, Measure, Operation, Orientation, PaperSize, Pick, Projection, Relation,
    RoofKind, Side, StairKind, StairView, Targets, iso_point, point,
};

/// One answer to a prompt. Lengths and points are in the document's
/// units; the runner converts them to the drawing's and formats them.
#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    /// A point of the drawing.
    Point(Point2),
    /// A length or distance.
    Length(f64),
    /// A plain number: a factor, a count.
    Number(f64),
    /// An angle in degrees.
    Angle(f64),
    /// A keyword, as the prompt offers it.
    Keyword(&'static str),
    /// Free text: a name, a line of text.
    Text(String),
    /// Enter with nothing typed: a default taken, a list ended.
    Enter,
    /// A point in a placed view's own coordinates (#354): where it is on
    /// the sheet the engine works out from the view's placement.
    InView { view: String, point: Point2 },
    /// Where a chain's dimension line goes (#355): on `side` of `points`,
    /// beyond what the operation's view has drawn there — worked out by
    /// the engine, which knows.
    Beyond {
        side: crate::ChainSide,
        points: Vec<Point2>,
    },
}

/// One step of a compiled operation.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Make these the selection before the command starts: the command then
    /// acts on them without asking for objects.
    Select(Targets),
    /// Answer the command's prompt — only if the prompt begins with `when`,
    /// where a prompt may be skipped (a text style that fixes the height
    /// does not ask for one).
    Answer {
        answer: Answer,
        when: Option<&'static str>,
    },
    /// Pick an object where the command asks for one, at its point nearest
    /// `near`.
    Pick(Pick),
    /// Make what the operations of the view `id` made the selection (#351):
    /// before the command starts when it is the first step.
    SelectView(String),
}

/// An operation compiled: the command to start, then the steps.
#[derive(Clone, Debug, PartialEq)]
pub struct Compiled {
    /// The operation's index in the document.
    pub operation: usize,
    /// The operation's `op` name.
    pub op: &'static str,
    /// The registered command, by its name.
    pub command: &'static str,
    pub steps: Vec<Step>,
    /// The view whose operations this is one of, if any (#351).
    pub view: Option<String>,
}

/// The commands the catalogue compiles to: the allowlist. No file, host,
/// or settings command is among them.
pub const COMMANDS: [&str; 54] = [
    "DIMCONTINUE",
    "NOTES",
    "BOMTABLE",
    "BALLOON",
    "VIEWFRAME",
    "LINE",
    "PLINE",
    "SPLINE",
    "HATCH",
    "RECTANG",
    "CIRCLE",
    "ARC",
    "POLYGON",
    "TEXT",
    "MOVE",
    "COPY",
    "ROTATE",
    "SCALE",
    "MIRROR",
    "OFFSET",
    "TRIM",
    "EXTEND",
    "FILLET",
    "CHAMFER",
    "ARRAY",
    "LAYER",
    "DIMSTYLE",
    "CHPROP",
    "DIMLINEAR",
    "DIMALIGNED",
    "DIMRADIUS",
    "DIMDIAMETER",
    "DIMANGULAR",
    "PLATE",
    "SLOT",
    "HOLEPATTERN",
    "BOLTCIRCLE",
    "FLANGE",
    "TANARC",
    "CONCENTRIC",
    "ARRAYPATH",
    "ARRAYGRID",
    "GEOMCONSTRAINT",
    "DIMCONSTRAINT",
    "-PARAMETERS",
    "FLOORPLAN",
    "ERASE",
    "MEMBERS",
    "STAIR",
    "ELLIPSE",
    "ELEVATION",
    "BSECTION",
    "PARTVIEWS",
    "TITLEBLOCK",
];

/// Compiles every operation of a validated document, in order.
pub fn compile(intent: &Intent) -> Vec<Compiled> {
    let operations = &intent.operations;
    let mut out = Vec::new();
    let mut index = 0;
    while index < operations.len() {
        if !matches!(operations[index], Operation::View { .. }) {
            for (command, steps) in commands_of(&operations[index]) {
                out.push(Compiled {
                    operation: index,
                    op: operations[index].name(),
                    command,
                    steps,
                    view: None,
                });
            }
            index += 1;
            continue;
        }
        // A run of views: each one's operations, drawn in its own
        // coordinates, then one VIEWFRAME placing them together (ADR-0041).
        let first = index;
        while index < operations.len() && matches!(operations[index], Operation::View { .. }) {
            index += 1;
        }
        let views = &operations[first..index];
        for (offset, view) in views.iter().enumerate() {
            let Operation::View { id, operations, .. } = view else {
                unreachable!("a run of views")
            };
            for inner in operations {
                for (command, steps) in commands_of(inner) {
                    out.push(Compiled {
                        operation: first + offset,
                        op: inner.name(),
                        command,
                        steps,
                        view: Some(id.clone()),
                    });
                }
            }
        }
        out.push(Compiled {
            operation: first,
            op: "view",
            command: "VIEWFRAME",
            steps: viewframe_steps(views),
            view: None,
        });
    }
    out
}

/// The commands an operation runs: one, but a chain of dimensions — its
/// first dimension, then DIMCONTINUE for the rest (#355).
fn commands_of(operation: &Operation) -> Vec<(&'static str, Vec<Step>)> {
    // A datum is read into the coordinates that name it: it runs nothing.
    if matches!(operation, Operation::Datum { .. }) {
        return Vec::new();
    }
    let Operation::DimChain { points, side } = operation else {
        return vec![steps_of(operation)];
    };
    let all: Vec<Point2> = points.iter().map(|p| point(*p)).collect();
    let mut out = vec![(
        "DIMLINEAR",
        vec![
            at(points[0]),
            at(points[1]),
            answer(Answer::Beyond {
                side: *side,
                points: all,
            }),
        ],
    )];
    if points.len() > 2 {
        let mut steps: Vec<Step> = points[2..].iter().map(|p| at(*p)).collect();
        // Enter at the point asks for another dimension to continue, as
        // typed; Enter again ends.
        steps.push(enter());
        steps.push(enter());
        out.push(("DIMCONTINUE", steps));
    }
    out
}

/// VIEWFRAME's answers for a run of views: each view's objects, its origin
/// at the drawing's origin, its id, title, and placement; another view, or
/// done.
fn viewframe_steps(views: &[Operation]) -> Vec<Step> {
    let mut steps = Vec::new();
    for (k, view) in views.iter().enumerate() {
        let Operation::View {
            id,
            title,
            placed,
            of,
            looking,
            ratio,
            cut,
            ..
        } = view
        else {
            unreachable!("a run of views")
        };
        if k > 0 {
            steps.push(keyword("Yes"));
        }
        steps.push(Step::SelectView(id.clone()));
        if k > 0 {
            // The selection made, the objects prompt is done.
            steps.push(enter());
        }
        steps.push(at([0.0, 0.0]));
        steps.push(answer(Answer::Text(id.clone())));
        steps.push(if title.trim().is_empty() {
            enter()
        } else {
            answer(Answer::Text(title.clone()))
        });
        match placed {
            crate::ViewPlacement::Principal => steps.push(keyword("Principal")),
            crate::ViewPlacement::Free | crate::ViewPlacement::Section => {
                match cut {
                    Some(cut) => {
                        steps.push(keyword("Section"));
                        steps.push(answer(Answer::Text(cut.of.clone())));
                        steps.push(at(cut.from));
                        steps.push(at(cut.to));
                        steps.push(at(cut.toward));
                    }
                    None => steps.push(keyword("Free")),
                }
                steps.push(match ratio {
                    Some(r) => answer(Answer::Text(format!("{r}"))),
                    None => enter(),
                });
            }
            crate::ViewPlacement::Projected => {
                steps.push(keyword("Projected"));
                steps.push(answer(Answer::Text(of.clone().unwrap_or_default())));
                steps.push(keyword(match looking {
                    Some(crate::ViewLooking::FromLeft) => "Left",
                    Some(crate::ViewLooking::FromRight) => "Right",
                    Some(crate::ViewLooking::FromAbove) => "Above",
                    Some(crate::ViewLooking::FromBelow) => "Below",
                    Some(crate::ViewLooking::FromBehind) | None => "Rear",
                }));
            }
        }
    }
    steps.push(keyword("No"));
    steps
}

fn answer(answer: Answer) -> Step {
    Step::Answer { answer, when: None }
}

fn when(prefix: &'static str, answer: Answer) -> Step {
    Step::Answer {
        answer,
        when: Some(prefix),
    }
}

fn at(p: crate::Point) -> Step {
    answer(Answer::Point(point(p)))
}

fn length(value: f64) -> Step {
    answer(Answer::Length(value))
}

fn keyword(word: &'static str) -> Step {
    answer(Answer::Keyword(word))
}

fn enter() -> Step {
    answer(Answer::Enter)
}

fn pick(pick: &Pick) -> Step {
    Step::Pick(pick.clone())
}

fn steps_of(operation: &Operation) -> (&'static str, Vec<Step>) {
    match operation {
        // Compiled with the run of views it is in (`compile`).
        Operation::View { .. } => ("VIEWFRAME", Vec::new()),
        Operation::Line { points } => {
            let mut steps: Vec<Step> = points.iter().map(|p| at(*p)).collect();
            steps.push(enter());
            ("LINE", steps)
        }
        Operation::Polyline { points, closed } => {
            let mut steps: Vec<Step> = points.iter().map(|p| at(*p)).collect();
            steps.push(if *closed { keyword("Close") } else { enter() });
            ("PLINE", steps)
        }
        Operation::BillOfMaterials { parts, title } => {
            let mut steps: Vec<Step> = parts
                .iter()
                .map(|p| {
                    let mut line = format!("{}, {}, {}", p.number, p.name, p.quantity);
                    if !p.material.is_empty() {
                        line.push_str(&format!(", {}", p.material));
                    }
                    answer(Answer::Text(line))
                })
                .collect();
            steps.push(enter());
            steps.push(if title.trim().is_empty() {
                enter()
            } else {
                answer(Answer::Text(title.clone()))
            });
            ("BOMTABLE", steps)
        }
        Operation::Balloon { part, view, at, to } => (
            "BALLOON",
            vec![
                answer(Answer::Text(part.to_string())),
                answer(Answer::InView {
                    view: view.clone(),
                    point: point(*at),
                }),
                answer(Answer::InView {
                    view: view.clone(),
                    point: point(*to),
                }),
                enter(),
            ],
        ),
        // Compiled by `commands_of`: two commands, or none.
        Operation::DimChain { .. } => ("DIMLINEAR", Vec::new()),
        Operation::Datum { .. } => ("LINE", Vec::new()),
        Operation::Notes {
            lines,
            width,
            title,
        } => {
            let mut steps: Vec<Step> = lines
                .iter()
                .map(|l| answer(Answer::Text(l.clone())))
                .collect();
            steps.push(enter());
            steps.push(match width {
                Some(w) => answer(Answer::Number(*w)),
                None => enter(),
            });
            steps.push(if title.trim().is_empty() {
                enter()
            } else {
                answer(Answer::Text(title.clone()))
            });
            ("NOTES", steps)
        }
        Operation::Spline { points } => {
            let mut steps: Vec<Step> = points.iter().map(|p| at(*p)).collect();
            steps.push(enter());
            ("SPLINE", steps)
        }
        Operation::Hatch {
            inside,
            targets,
            pattern,
            angle,
            scale,
        } => {
            // Each answer only where its prompt is: a pattern the table
            // lacks leaves HATCH asking for one, and nothing more is typed
            // into it — the refusal is the pattern's.
            const PICK: &str = "Pick internal point";
            let mut steps = vec![
                keyword("Pattern"),
                answer(Answer::Text(pattern.clone())),
                when(PICK, Answer::Keyword("Angle")),
                when("Specify angle", Answer::Angle(*angle)),
                when(PICK, Answer::Keyword("SCale")),
                when("Specify scale", Answer::Number(*scale)),
            ];
            match (inside, targets) {
                (Some(inside), _) => steps.push(when(PICK, Answer::Point(point(*inside)))),
                (None, Some(targets)) => {
                    steps.push(when(PICK, Answer::Keyword("Select")));
                    steps.push(Step::Select(targets.clone()));
                    steps.push(when("Select boundary", Answer::Enter));
                }
                (None, None) => {}
            }
            ("HATCH", steps)
        }
        Operation::IsoLine { points } => {
            let mut steps: Vec<Step> = points.iter().map(|p| at(iso_point(*p))).collect();
            steps.push(enter());
            ("LINE", steps)
        }
        Operation::IsoPolyline { points, closed } => {
            let mut steps: Vec<Step> = points.iter().map(|p| at(iso_point(*p))).collect();
            steps.push(if *closed { keyword("Close") } else { enter() });
            ("PLINE", steps)
        }
        Operation::IsoCircle {
            center,
            radius,
            plane,
        } => {
            // The ellipse by its centre, the end of its long axis, and the
            // half-length of the short one.
            let (major, ratio) = plane.plane().circle(*radius);
            let c = iso_point(*center);
            (
                "ELLIPSE",
                vec![
                    keyword("Center"),
                    at(c),
                    at([c[0] + major.x, c[1] + major.y]),
                    length(major.length() * ratio),
                ],
            )
        }
        Operation::Rectangle { corner, opposite } => ("RECTANG", vec![at(*corner), at(*opposite)]),
        Operation::Circle { center, radius } => ("CIRCLE", vec![at(*center), length(*radius)]),
        Operation::Arc {
            start,
            through,
            end,
        } => ("ARC", vec![at(*start), at(*through), at(*end)]),
        Operation::Polygon {
            sides,
            center,
            radius,
            circumscribed,
        } => (
            "POLYGON",
            vec![
                answer(Answer::Number(f64::from(*sides))),
                at(*center),
                keyword(if *circumscribed {
                    "Circumscribed about circle"
                } else {
                    "Inscribed in circle"
                }),
                length(*radius),
            ],
        ),
        Operation::Text {
            at: start,
            text,
            height,
            rotation,
        } => (
            "TEXT",
            vec![
                at(*start),
                // Not asked when the current text style fixes the height.
                when(
                    "Specify height",
                    height.map_or(Answer::Enter, Answer::Length),
                ),
                answer(rotation.map_or(Answer::Enter, Answer::Angle)),
                answer(Answer::Text(text.clone())),
            ],
        ),
        Operation::Erase { targets } => ("ERASE", vec![Step::Select(targets.clone())]),
        Operation::Members {
            groups,
            keep_centrelines,
        } => {
            // The first group is the selection the command starts with;
            // each further one is chosen at its prompt and ended by Enter;
            // Enter with none ends the groups.
            let mut steps = Vec::new();
            for (index, group) in groups.iter().enumerate() {
                steps.push(Step::Select(group.targets.clone()));
                if index > 0 {
                    steps.push(enter());
                }
                steps.push(length(group.width));
            }
            steps.push(enter());
            steps.push(keyword(if *keep_centrelines { "Yes" } else { "No" }));
            ("MEMBERS", steps)
        }
        Operation::Move {
            targets,
            displacement,
        } => (
            "MOVE",
            vec![
                Step::Select(targets.clone()),
                at([0.0, 0.0]),
                at(*displacement),
            ],
        ),
        Operation::Copy {
            targets,
            displacement,
        } => (
            "COPY",
            vec![
                Step::Select(targets.clone()),
                at([0.0, 0.0]),
                at(*displacement),
                enter(),
            ],
        ),
        Operation::Rotate {
            targets,
            base,
            angle,
        } => (
            "ROTATE",
            vec![
                Step::Select(targets.clone()),
                at(*base),
                answer(Answer::Angle(*angle)),
            ],
        ),
        Operation::Scale {
            targets,
            base,
            factor,
        } => (
            "SCALE",
            vec![
                Step::Select(targets.clone()),
                at(*base),
                answer(Answer::Number(*factor)),
            ],
        ),
        Operation::Mirror {
            targets,
            axis,
            keep,
        } => (
            "MIRROR",
            vec![
                Step::Select(targets.clone()),
                at(axis[0]),
                at(axis[1]),
                keyword(if *keep { "No" } else { "Yes" }),
            ],
        ),
        Operation::Offset {
            target,
            distance,
            toward,
        } => (
            "OFFSET",
            vec![length(*distance), pick(target), at(*toward), enter()],
        ),
        Operation::Trim { edges, targets } => ("TRIM", edge_steps(edges, targets)),
        Operation::Extend { edges, targets } => ("EXTEND", edge_steps(edges, targets)),
        Operation::Fillet {
            radius,
            first,
            second,
        } => (
            "FILLET",
            vec![
                keyword("Radius"),
                length(*radius),
                pick(first),
                pick(second),
            ],
        ),
        Operation::Chamfer {
            first_distance,
            second_distance,
            first,
            second,
        } => (
            "CHAMFER",
            vec![
                keyword("Distance"),
                length(*first_distance),
                length(*second_distance),
                pick(first),
                pick(second),
            ],
        ),
        Operation::Array {
            targets,
            rows,
            columns,
            row_spacing,
            column_spacing,
        } => (
            "ARRAY",
            vec![
                Step::Select(targets.clone()),
                keyword("Rectangular"),
                answer(Answer::Number(f64::from(*rows))),
                answer(Answer::Number(f64::from(*columns))),
                when(
                    "Specify the distance between rows",
                    Answer::Length(*row_spacing),
                ),
                when(
                    "Specify the distance between columns",
                    Answer::Length(*column_spacing),
                ),
            ],
        ),
        Operation::PolarArray {
            targets,
            center,
            count,
            angle,
            rotate,
        } => (
            "ARRAY",
            vec![
                Step::Select(targets.clone()),
                keyword("Polar"),
                at(*center),
                answer(Answer::Number(f64::from(*count))),
                answer(Answer::Angle(*angle)),
                keyword(if *rotate { "Yes" } else { "No" }),
            ],
        ),
        Operation::DimScale { scale } => (
            "DIMSTYLE",
            vec![keyword("SCale"), answer(Answer::Number(*scale)), enter()],
        ),
        Operation::Layer {
            name,
            current,
            color,
            linetype,
        } => {
            let mut steps = vec![keyword("New"), answer(Answer::Text(name.clone()))];
            if let Some(color) = color {
                steps.push(keyword("Color"));
                steps.push(answer(Answer::Text(match color {
                    Color::Index(index) => index.to_string(),
                    Color::Name(name) => name.clone(),
                })));
                steps.push(answer(Answer::Text(name.clone())));
            }
            if let Some(linetype) = linetype {
                steps.push(keyword("LType"));
                steps.push(answer(Answer::Text(linetype.clone())));
                steps.push(answer(Answer::Text(name.clone())));
            }
            if *current {
                steps.push(keyword("Set"));
                steps.push(answer(Answer::Text(name.clone())));
            }
            steps.push(enter());
            ("LAYER", steps)
        }
        Operation::SetProperties {
            targets,
            layer,
            color,
            linetype,
            lineweight,
        } => {
            let mut steps = vec![Step::Select(targets.clone())];
            if let Some(layer) = layer {
                steps.push(keyword("LAyer"));
                steps.push(answer(Answer::Text(layer.clone())));
            }
            if let Some(color) = color {
                steps.push(keyword("Color"));
                steps.push(answer(Answer::Text(match color {
                    Color::Index(index) => index.to_string(),
                    Color::Name(name) => name.clone(),
                })));
            }
            if let Some(linetype) = linetype {
                steps.push(keyword("LType"));
                steps.push(answer(Answer::Text(linetype.clone())));
            }
            if let Some(lineweight) = lineweight {
                steps.push(keyword("LWeight"));
                steps.push(answer(Answer::Text(lineweight.clone())));
            }
            steps.push(enter());
            ("CHPROP", steps)
        }
        Operation::DimLinear {
            from,
            to,
            at: place,
            orientation,
        } => {
            let mut steps = vec![at(*from), at(*to)];
            match orientation {
                Some(Orientation::Horizontal) => steps.push(keyword("Horizontal")),
                Some(Orientation::Vertical) => steps.push(keyword("Vertical")),
                None => {}
            }
            steps.push(at(*place));
            ("DIMLINEAR", steps)
        }
        Operation::DimAligned {
            from,
            to,
            at: place,
        } => ("DIMALIGNED", vec![at(*from), at(*to), at(*place)]),
        Operation::DimRadius { target, at: place } => ("DIMRADIUS", vec![pick(target), at(*place)]),
        Operation::DimDiameter { target, at: place } => {
            ("DIMDIAMETER", vec![pick(target), at(*place)])
        }
        Operation::DimAngular {
            vertex,
            first,
            second,
            at: place,
        } => (
            "DIMANGULAR",
            vec![enter(), at(*vertex), at(*first), at(*second), at(*place)],
        ),
        Operation::Plate {
            corner,
            width,
            height,
            fillet,
            chamfer,
            hole_diameter,
            hole_offset,
        } => {
            let mut steps = vec![
                at(*corner),
                keyword("Dimensions"),
                length(*width),
                length(*height),
            ];
            match (fillet, chamfer) {
                (Some(r), _) => steps.extend([keyword("Fillet"), length(*r)]),
                (None, Some(d)) => steps.extend([keyword("Chamfer"), length(*d)]),
                (None, None) => steps.push(keyword("Square")),
            }
            match (hole_diameter, hole_offset) {
                (Some(d), Some(o)) => steps.extend([length(*d), length(*o)]),
                _ => steps.push(keyword("None")),
            }
            ("PLATE", steps)
        }
        Operation::Slot {
            center,
            length: between,
            width,
            angle,
        } => (
            "SLOT",
            vec![
                at(*center),
                length(*between),
                length(*width),
                answer(Answer::Angle(*angle)),
            ],
        ),
        Operation::HolePattern {
            first,
            diameter,
            rows,
            columns,
            row_spacing,
            column_spacing,
        } => (
            "HOLEPATTERN",
            vec![
                at(*first),
                length(*diameter),
                answer(Answer::Number(f64::from(*rows))),
                answer(Answer::Number(f64::from(*columns))),
                // Asked only of more than one row, or column.
                when("Specify row spacing", Answer::Length(*row_spacing)),
                when("Specify column spacing", Answer::Length(*column_spacing)),
            ],
        ),
        Operation::BoltCircle {
            center,
            pitch_diameter,
            count,
            hole_diameter,
            start_angle,
        } => (
            "BOLTCIRCLE",
            vec![
                at(*center),
                length(*pitch_diameter),
                answer(Answer::Number(f64::from(*count))),
                length(*hole_diameter),
                answer(Answer::Angle(*start_angle)),
            ],
        ),
        Operation::RoundFlange {
            center,
            outer_diameter,
            bore_diameter,
            pitch_diameter,
            count,
            hole_diameter,
        } => (
            "FLANGE",
            vec![
                keyword("Round"),
                at(*center),
                length(*outer_diameter),
                length(*bore_diameter),
                length(*pitch_diameter),
                answer(Answer::Number(f64::from(*count))),
                length(*hole_diameter),
            ],
        ),
        Operation::RectangularFlange {
            center,
            width,
            height,
            bore_diameter,
            hole_diameter,
            hole_offset,
        } => (
            "FLANGE",
            vec![
                keyword("REctangular"),
                at(*center),
                length(*width),
                length(*height),
                length(*bore_diameter),
                length(*hole_diameter),
                length(*hole_offset),
            ],
        ),
        Operation::TangentCircle {
            first,
            second,
            third,
            radius,
        } => {
            let mut steps = Vec::with_capacity(4);
            match (third, radius) {
                (Some(third), _) => {
                    steps.extend([keyword("TTT"), pick(first), pick(second), pick(third)]);
                }
                (None, radius) => steps.extend([
                    keyword("Ttr"),
                    pick(first),
                    pick(second),
                    length(radius.unwrap_or_default()),
                ]),
            }
            ("CIRCLE", steps)
        }
        Operation::TangentArc {
            first,
            second,
            radius,
        } => ("TANARC", vec![pick(first), pick(second), length(*radius)]),
        Operation::ConcentricCircle { of, radius } => {
            ("CONCENTRIC", vec![pick(of), length(*radius)])
        }
        Operation::PathArray {
            targets,
            path,
            count,
            spacing,
        } => {
            let mut steps = vec![Step::Select(targets.clone()), pick(path)];
            match (count, spacing) {
                (Some(count), _) => steps.push(answer(Answer::Number(f64::from(*count)))),
                (None, spacing) => {
                    steps.extend([keyword("Measure"), length(spacing.unwrap_or_default())]);
                }
            }
            ("ARRAYPATH", steps)
        }
        Operation::GridArray {
            targets,
            inside,
            column_spacing,
            row_spacing,
        } => (
            "ARRAYGRID",
            vec![
                Step::Select(targets.clone()),
                at(*inside),
                length(*column_spacing),
                row_spacing.map_or_else(enter, length),
            ],
        ),
        Operation::Constrain {
            relation,
            first,
            second,
        } => {
            let mut steps = vec![keyword(relation_keyword(*relation)), pick(first)];
            steps.extend(second.as_ref().map(pick));
            ("GEOMCONSTRAINT", steps)
        }
        Operation::ConstrainDimension {
            measure,
            points,
            of,
            and,
            at: place,
            value,
        } => {
            let mut steps = vec![keyword(match measure {
                Measure::Horizontal => "Horizontal",
                Measure::Vertical => "Vertical",
                Measure::Aligned => "ALigned",
                Measure::Angular => "ANgular",
                Measure::Radius => "Radial",
                Measure::Diameter => "Diameter",
            })];
            match (points, of) {
                (Some([a, b]), _) => steps.extend([at(*a), at(*b)]),
                // A line measured end to end: the Object option.
                (None, Some(of)) if measure.is_linear() => steps.extend([enter(), pick(of)]),
                (None, Some(of)) => steps.push(pick(of)),
                (None, None) => {}
            }
            steps.extend(and.as_ref().map(pick));
            steps.push(at(*place));
            steps.push(match (value, measure) {
                (Some(v), Measure::Angular) => answer(Answer::Angle(*v)),
                (Some(v), _) => length(*v),
                (None, _) => enter(),
            });
            ("DIMCONSTRAINT", steps)
        }
        Operation::FloorPlan {
            brief,
            at: place,
            alternative,
        } => {
            let mut steps = vec![
                keyword("Brief"),
                answer(Answer::Text(
                    serde_json::to_string(brief).expect("a brief is plain data"),
                )),
                place.map_or_else(enter, at),
            ];
            for _ in 1..alternative.unwrap_or(1) {
                steps.push(keyword("Next"));
            }
            steps.push(keyword("Accept"));
            ("FLOORPLAN", steps)
        }
        Operation::Elevation {
            plan,
            side,
            roof,
            roof_pitch,
            at: place,
        } => {
            let roof_word = roof.map(|roof| match roof {
                RoofKind::Flat => "Flat",
                RoofKind::Gable => "Gable",
                RoofKind::Hip => "Hip",
            });
            (
                "ELEVATION",
                vec![
                    keyword("Name"),
                    answer(Answer::Text(plan.clone())),
                    keyword(match side {
                        Side::Front => "Front",
                        Side::Back => "Back",
                        Side::Left => "Left",
                        Side::Right => "Right",
                    }),
                    roof_word.map_or_else(enter, keyword),
                    // Asked only for a pitched roof.
                    when(
                        "Specify roof pitch",
                        roof_pitch.map_or(Answer::Enter, Answer::Angle),
                    ),
                    place.map_or_else(enter, at),
                ],
            )
        }
        Operation::BuildingSection {
            plan,
            from,
            to,
            at: place,
        } => (
            "BSECTION",
            vec![
                keyword("Name"),
                answer(Answer::Text(plan.clone())),
                at(*from),
                at(*to),
                place.map_or_else(enter, at),
            ],
        ),
        Operation::PartViews {
            part,
            projection,
            section,
            at: place,
        } => (
            "PARTVIEWS",
            vec![
                keyword("Json"),
                answer(Answer::Text(
                    serde_json::to_string(part).expect("a part is plain data"),
                )),
                keyword(match projection {
                    Projection::First => "First",
                    Projection::Third => "Third",
                }),
                keyword(if *section { "Yes" } else { "No" }),
                place.map_or_else(enter, at),
            ],
        ),
        Operation::TitleBlock {
            paper,
            portrait,
            scale,
            title,
            number,
            drawn_by,
            date,
            sheet,
            projection,
            at: place,
        } => {
            let text = |value: &String| {
                if value.trim().is_empty() {
                    enter()
                } else {
                    answer(Answer::Text(value.clone()))
                }
            };
            (
                "TITLEBLOCK",
                vec![
                    keyword(match paper {
                        PaperSize::A4 => "A4",
                        PaperSize::A3 => "A3",
                        PaperSize::A2 => "A2",
                        PaperSize::A1 => "A1",
                        PaperSize::A0 => "A0",
                    }),
                    keyword(if *portrait { "Portrait" } else { "Landscape" }),
                    // Asked in the model only.
                    when(
                        "Specify the scale",
                        scale.map_or(Answer::Enter, Answer::Number),
                    ),
                    text(title),
                    text(number),
                    text(drawn_by),
                    text(date),
                    text(sheet),
                    keyword(match projection {
                        Projection::First => "First",
                        Projection::Third => "Third",
                    }),
                    place.map_or_else(enter, at),
                ],
            )
        }
        Operation::Stair {
            height,
            width,
            kind,
            newel,
            view,
            at: foot,
            direction,
        } => {
            let mut steps = vec![
                length(*height),
                length(*width),
                keyword(match kind {
                    StairKind::Straight => "Straight",
                    StairKind::L => "L",
                    StairKind::U => "U",
                    StairKind::Helical => "Helical",
                }),
            ];
            if *kind == StairKind::Helical {
                steps.push(newel.map_or_else(enter, length));
            }
            steps.push(keyword(match view {
                StairView::Plan => "Plan",
                StairView::Section => "Section",
            }));
            steps.push(at(*foot));
            steps.push(answer(direction.map_or(Answer::Enter, Answer::Angle)));
            ("STAIR", steps)
        }
        Operation::SetParameter { name, value } => (
            "-PARAMETERS",
            vec![
                keyword("Edit"),
                answer(Answer::Text(name.clone())),
                // An angle's value is in degrees, whatever the length unit.
                if name.trim().to_ascii_lowercase().starts_with("ang") {
                    answer(Answer::Angle(*value))
                } else {
                    length(*value)
                },
            ],
        ),
    }
}

/// GEOMCONSTRAINT's keyword for a relation.
fn relation_keyword(relation: Relation) -> &'static str {
    match relation {
        Relation::Coincident => "Coincident",
        Relation::Collinear => "COLlinear",
        Relation::Concentric => "CONcentric",
        Relation::Fix => "Fix",
        Relation::Parallel => "PArallel",
        Relation::Perpendicular => "Perpendicular",
        Relation::Horizontal => "Horizontal",
        Relation::Vertical => "Vertical",
        Relation::Tangent => "Tangent",
        Relation::Equal => "Equal",
    }
}

/// TRIM and EXTEND: the edges — the selection, or every object with
/// Enter — then each target picked, then Enter.
fn edge_steps(edges: &Option<Targets>, targets: &[Pick]) -> Vec<Step> {
    let mut steps = Vec::with_capacity(targets.len() + 3);
    if let Some(edges) = edges {
        steps.push(Step::Select(edges.clone()));
    }
    steps.push(enter());
    steps.extend(targets.iter().map(pick));
    steps.push(enter());
    steps
}
