//! Proptest strategies for valid documents, for this crate's property
//! tests and for the engine's (`features = ["testing"]`).

use proptest::prelude::*;

use crate::validate::same_point;
use crate::{
    Brief, BriefRoom, BriefTerrain, Color, Intent, Measure, MemberGroup, Operation, Orientation,
    Pick, Point, Relation, SelectionWord, StairKind, StairView, Targets, Unit, VERSION,
};

fn differ(a: Point, b: Point) -> bool {
    !same_point(a, b)
}

/// A finite coordinate in a workable range.
pub fn coordinate() -> impl Strategy<Value = f64> {
    (-1000i32..=1000, 0u8..100)
        .prop_map(|(whole, hundredths)| f64::from(whole) + f64::from(hundredths) / 100.0)
}

pub fn point() -> impl Strategy<Value = Point> {
    (coordinate(), coordinate()).prop_map(|(x, y)| [x, y])
}

/// Points with no two consecutive the same.
pub fn points(least: usize) -> impl Strategy<Value = Vec<Point>> {
    proptest::collection::vec(point(), least..least + 6)
        .prop_filter("consecutive points differ", |points| {
            points.windows(2).all(|pair| differ(pair[0], pair[1]))
        })
}

pub fn positive() -> impl Strategy<Value = f64> {
    (1u32..=5000).prop_map(|n| f64::from(n) / 10.0)
}

pub fn angle() -> impl Strategy<Value = f64> {
    (-360i32..=360).prop_map(f64::from)
}

/// An object ID the context might have given out.
pub fn id() -> impl Strategy<Value = u64> {
    1u64..=40
}

pub fn targets() -> impl Strategy<Value = Targets> {
    prop_oneof![
        Just(Targets::Selection(SelectionWord::Selection)),
        proptest::collection::vec(id(), 1..4).prop_map(Targets::Ids),
    ]
}

pub fn pick() -> impl Strategy<Value = Pick> {
    (id(), proptest::option::of(point())).prop_map(|(id, near)| Pick { id, near })
}

pub fn name() -> impl Strategy<Value = String> {
    "[A-Za-z][A-Za-z0-9 _-]{0,15}".prop_filter("not blank", |s| !s.trim().is_empty())
}

pub fn text() -> impl Strategy<Value = String> {
    "[A-Za-z0-9 .,!?-]{1,40}".prop_filter("not blank", |s| !s.trim().is_empty())
}

/// Two points that differ in both coordinates: a rectangle's corners.
fn corners() -> impl Strategy<Value = (Point, Point)> {
    (point(), point()).prop_filter("a rectangle has width and height", |(a, b)| {
        (a[0] - b[0]).abs() > 0.5 && (a[1] - b[1]).abs() > 0.5
    })
}

fn three_distinct() -> impl Strategy<Value = (Point, Point, Point)> {
    (point(), point(), point()).prop_filter("three distinct points", |(a, b, c)| {
        differ(*a, *b) && differ(*b, *c) && differ(*a, *c)
    })
}

/// One valid operation of any kind.
pub fn operation() -> impl Strategy<Value = Operation> {
    prop_oneof![
        points(2).prop_map(|points| Operation::Line { points }),
        (points(3), any::<bool>())
            .prop_map(|(points, closed)| Operation::Polyline { points, closed }),
        points(2).prop_map(|points| Operation::Spline { points }),
        (1.0..2000.0f64).prop_map(|value| Operation::Datum {
            name: "d".to_owned(),
            axis: crate::DatumAxis::Height,
            value,
            views: Vec::new(),
        }),
        points(3).prop_map(|points| Operation::DimChain {
            points,
            side: crate::ChainSide::Below,
        }),
        (
            proptest::collection::vec(text(), 1..4),
            proptest::option::of(10.0..200.0f64)
        )
            .prop_map(|(lines, width)| Operation::Notes {
                lines,
                width,
                title: String::new(),
            }),
        (1u32..50, 1u32..5).prop_map(|(number, quantity)| Operation::BillOfMaterials {
            parts: vec![crate::BomPart {
                number,
                name: "Shell".to_owned(),
                quantity,
                material: String::new(),
            }],
            title: String::new(),
        }),
        (1u32..50, point(), point()).prop_map(|(part, at, to)| Operation::Balloon {
            part,
            view: "front".to_owned(),
            at,
            to,
        }),
        (corners(), prop_oneof![Just(0u8), Just(1), Just(2)]).prop_map(
            |((corner, opposite), kind)| {
                let (placed, of, looking) = match kind {
                    0 => (crate::ViewPlacement::Principal, None, None),
                    1 => (
                        crate::ViewPlacement::Projected,
                        Some("front".to_owned()),
                        Some(crate::ViewLooking::FromLeft),
                    ),
                    _ => (crate::ViewPlacement::Free, None, None),
                };
                Operation::View {
                    id: format!("v{kind}"),
                    title: String::new(),
                    placed,
                    of,
                    looking,
                    ratio: (kind == 2).then_some(2.0),
                    cut: None,
                    operations: vec![Operation::Rectangle { corner, opposite }],
                }
            }
        ),
        (
            prop_oneof![point().prop_map(Ok), targets().prop_map(Err)],
            prop_oneof![Just("SOLID"), Just("ANSI31"), Just("EARTH")],
            angle(),
            positive()
        )
            .prop_map(|(boundary, pattern, angle, scale)| {
                let (inside, targets) = match boundary {
                    Ok(inside) => (Some(inside), None),
                    Err(targets) => (None, Some(targets)),
                };
                Operation::Hatch {
                    inside,
                    targets,
                    pattern: pattern.to_owned(),
                    angle,
                    scale,
                }
            }),
        corners().prop_map(|(corner, opposite)| Operation::Rectangle { corner, opposite }),
        (point(), positive()).prop_map(|(center, radius)| Operation::Circle { center, radius }),
        three_distinct().prop_map(|(start, through, end)| Operation::Arc {
            start,
            through,
            end
        }),
        (3u32..=12, point(), positive(), any::<bool>()).prop_map(
            |(sides, center, radius, circumscribed)| Operation::Polygon {
                sides,
                center,
                radius,
                circumscribed,
            }
        ),
        (
            point(),
            text(),
            proptest::option::of(positive()),
            proptest::option::of(angle())
        )
            .prop_map(|(at, text, height, rotation)| Operation::Text {
                at,
                text,
                height,
                rotation,
            }),
        (targets(), point()).prop_map(|(targets, displacement)| Operation::Move {
            targets,
            displacement
        }),
        (targets(), point()).prop_map(|(targets, displacement)| Operation::Copy {
            targets,
            displacement
        }),
        (targets(), point(), angle()).prop_map(|(targets, base, angle)| Operation::Rotate {
            targets,
            base,
            angle
        }),
        (targets(), point(), positive()).prop_map(|(targets, base, factor)| Operation::Scale {
            targets,
            base,
            factor
        }),
        (targets(), point(), point(), any::<bool>())
            .prop_filter("an axis has two points", |(_, a, b, _)| differ(*a, *b))
            .prop_map(|(targets, a, b, keep)| Operation::Mirror {
                targets,
                axis: [a, b],
                keep,
            }),
        (pick(), positive(), point()).prop_map(|(target, distance, toward)| Operation::Offset {
            target,
            distance,
            toward
        }),
        (
            proptest::option::of(targets()),
            proptest::collection::vec(pick(), 1..3)
        )
            .prop_map(|(edges, targets)| Operation::Trim { edges, targets }),
        (
            proptest::option::of(targets()),
            proptest::collection::vec(pick(), 1..3)
        )
            .prop_map(|(edges, targets)| Operation::Extend { edges, targets }),
        (positive(), pick(), pick()).prop_map(|(radius, first, second)| Operation::Fillet {
            radius,
            first,
            second
        }),
        (positive(), positive(), pick(), pick()).prop_map(
            |(first_distance, second_distance, first, second)| Operation::Chamfer {
                first_distance,
                second_distance,
                first,
                second,
            }
        ),
        (targets(), 1u32..=4, 1u32..=4, coordinate(), coordinate())
            .prop_filter("an array makes copies", |(_, rows, columns, _, _)| rows
                * columns
                > 1)
            .prop_map(|(targets, rows, columns, row_spacing, column_spacing)| {
                Operation::Array {
                    targets,
                    rows,
                    columns,
                    row_spacing,
                    column_spacing,
                }
            }),
        (targets(), point(), 2u32..=12, angle(), any::<bool>())
            .prop_filter("a turn", |(_, _, _, angle, _)| angle.abs() > 0.5)
            .prop_map(
                |(targets, center, count, angle, rotate)| Operation::PolarArray {
                    targets,
                    center,
                    count,
                    angle,
                    rotate,
                }
            ),
        (
            name(),
            any::<bool>(),
            proptest::option::of((1u16..=255).prop_map(Color::Index)),
            proptest::option::of(Just("CENTER".to_owned()))
        )
            .prop_map(|(name, current, color, linetype)| Operation::Layer {
                name,
                current,
                color,
                linetype,
            }),
        (0.1f64..20.0).prop_map(|scale| Operation::DimScale { scale }),
        (
            targets(),
            proptest::option::of(name()),
            proptest::option::of(prop_oneof![
                (1u16..=255).prop_map(Color::Index),
                Just(Color::Name("red".into()))
            ]),
            proptest::option::of(Just("Continuous".to_string())),
            proptest::option::of(Just("0.5".to_string())),
        )
            .prop_filter("something to set", |(_, l, c, t, w)| {
                l.is_some() || c.is_some() || t.is_some() || w.is_some()
            })
            .prop_map(|(targets, layer, color, linetype, lineweight)| {
                Operation::SetProperties {
                    targets,
                    layer,
                    color,
                    linetype,
                    lineweight,
                }
            }),
        (
            point(),
            point(),
            point(),
            proptest::option::of(prop_oneof![
                Just(Orientation::Horizontal),
                Just(Orientation::Vertical)
            ])
        )
            .prop_filter("two points to measure", |(a, b, _, _)| differ(*a, *b))
            .prop_map(|(from, to, at, orientation)| Operation::DimLinear {
                from,
                to,
                at,
                orientation,
            }),
        (point(), point(), point())
            .prop_filter("two points to measure", |(a, b, _)| differ(*a, *b))
            .prop_map(|(from, to, at)| Operation::DimAligned { from, to, at }),
        (pick(), point()).prop_map(|(target, at)| Operation::DimRadius { target, at }),
        (pick(), point()).prop_map(|(target, at)| Operation::DimDiameter { target, at }),
        (three_distinct(), point()).prop_map(|((vertex, first, second), at)| {
            Operation::DimAngular {
                vertex,
                first,
                second,
                at,
            }
        }),
        part(),
        placement(),
        constraint(),
        floor_plan(),
        stair(),
        isometric(),
        elevation(),
        part_views(),
        (
            prop_oneof![
                Just(crate::PaperSize::A4),
                Just(crate::PaperSize::A3),
                Just(crate::PaperSize::A0),
            ],
            any::<bool>(),
            proptest::option::of(1.0..200.0f64),
            "[A-Z ]{0,12}",
            prop_oneof![
                Just(crate::Projection::First),
                Just(crate::Projection::Third)
            ],
            proptest::option::of(point()),
        )
            .prop_map(|(paper, portrait, scale, title, projection, at)| {
                Operation::TitleBlock {
                    paper,
                    portrait,
                    scale,
                    title,
                    number: String::new(),
                    drawn_by: String::new(),
                    date: String::new(),
                    sheet: String::new(),
                    projection,
                    at,
                }
            }),
        (
            prop_oneof![Just("PLAN-1"), Just("PLAN-2")],
            point(),
            point(),
            proptest::option::of(point()),
        )
            .prop_filter("apart", |(_, a, b, _)| differ(*a, *b))
            .prop_map(|(plan, from, to, at)| Operation::BuildingSection {
                plan: plan.to_owned(),
                from,
                to,
                at,
            }),
        targets().prop_map(|targets| Operation::Erase { targets }),
        (
            proptest::collection::vec((targets(), 0.5f64..50.0), 1..4),
            any::<bool>(),
        )
            .prop_map(|(groups, keep_centrelines)| Operation::Members {
                groups: groups
                    .into_iter()
                    .map(|(targets, width)| MemberGroup { targets, width })
                    .collect(),
                keep_centrelines,
            }),
    ]
}

/// A façade (#337) of a plan that may not be there: most name none, and
/// the engine says so.
fn elevation() -> impl Strategy<Value = Operation> {
    (
        prop_oneof![Just("PLAN-1"), Just("PLAN-2"), Just("ELEV-1")],
        prop_oneof![
            Just(crate::Side::Front),
            Just(crate::Side::Back),
            Just(crate::Side::Left),
            Just(crate::Side::Right),
        ],
        proptest::option::of(prop_oneof![
            Just(crate::RoofKind::Flat),
            Just(crate::RoofKind::Gable),
            Just(crate::RoofKind::Hip),
        ]),
        proptest::option::of(5.0..70.0f64),
        proptest::option::of(point()),
    )
        .prop_map(|(plan, side, roof, roof_pitch, at)| Operation::Elevation {
            plan: plan.to_owned(),
            side,
            roof,
            roof_pitch,
            at,
        })
}

/// A part's views (#339): a box with holes, or a turned part; holes may
/// fall outside, and the engine says so.
fn part_views() -> impl Strategy<Value = Operation> {
    let hole = (
        prop_oneof![
            Just(crate::PartAxis::X),
            Just(crate::PartAxis::Y),
            Just(crate::PartAxis::Z),
        ],
        point(),
        1.0..20.0f64,
    )
        .prop_map(|(axis, at, diameter)| crate::PartHole { axis, at, diameter });
    let prism = (
        (10.0..200.0f64, 10.0..200.0f64, 5.0..100.0f64),
        proptest::collection::vec(hole, 0..3),
    )
        .prop_map(|((w, h, depth), holes)| crate::PartDescription {
            profile: Some(vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]]),
            depth: Some(depth),
            holes,
            ..crate::PartDescription::default()
        });
    let turned = (
        proptest::collection::vec((5.0..80.0f64, 5.0..80.0f64), 1..4),
        proptest::option::of(0.5..4.0f64),
    )
        .prop_map(|(steps, bore)| crate::PartDescription {
            steps: Some(steps.into_iter().map(|(d, l)| [d, l]).collect()),
            bore,
            ..crate::PartDescription::default()
        });
    (
        prop_oneof![prism, turned],
        prop_oneof![
            Just(crate::Projection::First),
            Just(crate::Projection::Third)
        ],
        any::<bool>(),
        proptest::option::of(point()),
    )
        .prop_map(|(part, projection, section, at)| Operation::PartViews {
            part,
            projection,
            section,
            at,
        })
}

/// A point on the isometric axes.
fn iso_point() -> impl Strategy<Value = crate::IsoPoint> {
    (-500.0..500.0f64, -500.0..500.0f64, -500.0..500.0f64).prop_map(|(x, y, z)| [x, y, z])
}

/// Isometric lines, polylines, and circles (#336), their points never
/// twice on one place of the page in a row.
fn isometric() -> impl Strategy<Value = Operation> {
    let apart = |points: &Vec<crate::IsoPoint>| {
        points
            .windows(2)
            .all(|w| differ(crate::iso_point(w[0]), crate::iso_point(w[1])))
    };
    let plane = prop_oneof![
        Just(crate::IsoPlane::Left),
        Just(crate::IsoPlane::Top),
        Just(crate::IsoPlane::Right),
    ];
    prop_oneof![
        proptest::collection::vec(iso_point(), 2..6)
            .prop_filter("apart", apart)
            .prop_map(|points| Operation::IsoLine { points }),
        (proptest::collection::vec(iso_point(), 3..6), any::<bool>())
            .prop_filter("apart", move |(points, _)| apart(points))
            .prop_map(|(points, closed)| Operation::IsoPolyline { points, closed }),
        (iso_point(), 0.5..100.0f64, plane).prop_map(|(center, radius, plane)| {
            Operation::IsoCircle {
                center,
                radius,
                plane,
            }
        }),
    ]
}

/// A stair (#331): of any kind, in plan or section; many sizes are
/// refused — a helix too tight for headroom, a newel wider than the
/// stair — and the engine says why.
fn stair() -> impl Strategy<Value = Operation> {
    (
        (0.5..6.0f64, 0.3..3.0f64),
        prop_oneof![
            Just(StairKind::Straight),
            Just(StairKind::L),
            Just(StairKind::U),
            Just(StairKind::Helical),
        ],
        proptest::option::of(0.05..1.0f64),
        prop_oneof![Just(StairView::Plan), Just(StairView::Section)],
        point(),
        proptest::option::of(-360.0..360.0f64),
    )
        .prop_map(
            |((height, width), kind, newel, view, at, direction)| Operation::Stair {
                height,
                width,
                kind,
                newel: newel.filter(|_| kind == StairKind::Helical),
                view,
                at,
                direction,
            },
        )
}

/// A floor plan (#282): a brief of one to six rooms on a plot, some of
/// them side by side; many cannot be built, and the engine says why.
fn floor_plan() -> impl Strategy<Value = Operation> {
    (
        (8.0..40.0f64, 8.0..40.0f64),
        proptest::collection::vec(3.0..40.0f64, 1..7),
        proptest::collection::vec((0..6usize, 0..6usize), 0..4),
        proptest::option::of(point()),
        (
            proptest::option::of(1u32..=3),
            proptest::option::of(0.6..2.5f64),
            (0..7usize, proptest::option::of(2.4..4.0f64)),
        ),
    )
        .prop_map(
            |((width, depth), areas, pairs, at, (alternative, windows, (stair, storey_height)))| {
                let rooms: Vec<BriefRoom> = areas
                    .iter()
                    .enumerate()
                    .map(|(i, area)| BriefRoom {
                        name: format!("room {i}"),
                        area: *area,
                        min_width: None,
                        max_aspect: None,
                        exterior: false,
                        entrance: false,
                        stair: i == stair,
                    })
                    .collect();
                let n = rooms.len();
                let adjacent = pairs
                    .into_iter()
                    .map(|(a, b)| (a % n, b % n))
                    .filter(|(a, b)| a != b)
                    .map(|(a, b)| [format!("room {a}"), format!("room {b}")])
                    .collect();
                Operation::FloorPlan {
                    brief: Brief {
                        terrain: BriefTerrain { width, depth },
                        setbacks: None,
                        rooms,
                        adjacent,
                        max_circulation: None,
                        windows,
                        storey_height,
                        building: None,
                    },
                    at,
                    alternative,
                }
            },
        )
}

/// A constraint or a parameter (#274); the picks are arbitrary, so most
/// find nothing to relate, and the engine says so.
fn constraint() -> impl Strategy<Value = Operation> {
    let relation = prop_oneof![
        Just(Relation::Coincident),
        Just(Relation::Collinear),
        Just(Relation::Concentric),
        Just(Relation::Fix),
        Just(Relation::Parallel),
        Just(Relation::Perpendicular),
        Just(Relation::Horizontal),
        Just(Relation::Vertical),
        Just(Relation::Tangent),
        Just(Relation::Equal),
    ];
    let measure = prop_oneof![
        Just(Measure::Horizontal),
        Just(Measure::Vertical),
        Just(Measure::Aligned),
        Just(Measure::Angular),
        Just(Measure::Radius),
        Just(Measure::Diameter),
    ];
    prop_oneof![
        (relation, pick(), pick()).prop_map(|(relation, first, second)| Operation::Constrain {
            relation,
            first,
            second: (!relation.takes_one()).then_some(second),
        }),
        (
            measure,
            point(),
            point(),
            pick(),
            pick(),
            point(),
            proptest::option::of(positive()),
            any::<bool>()
        )
            .prop_filter("two points", |(_, a, b, ..)| differ(*a, *b))
            .prop_map(|(measure, a, b, of, and, at, value, by_points)| {
                let (points, of, and) = match measure {
                    m if m.is_linear() && by_points => (Some([a, b]), None, None),
                    m if m.is_linear() => (None, Some(of), None),
                    Measure::Angular => (None, Some(of), Some(and)),
                    _ => (None, Some(of), None),
                };
                Operation::ConstrainDimension {
                    measure,
                    points,
                    of,
                    and,
                    at,
                    value,
                }
            }),
        (
            prop_oneof![Just("d1"), Just("dia1"), Just("ang1"), Just("width")],
            positive()
        )
            .prop_map(|(name, value)| Operation::SetParameter {
                name: name.to_owned(),
                value
            }),
    ]
}

/// A placement (#273); the picks are arbitrary, so many find nothing to
/// touch, and the engine says so.
fn placement() -> impl Strategy<Value = Operation> {
    prop_oneof![
        (pick(), pick(), positive()).prop_map(|(first, second, radius)| {
            Operation::TangentCircle {
                first,
                second,
                third: None,
                radius: Some(radius),
            }
        }),
        (pick(), pick(), pick()).prop_map(|(first, second, third)| Operation::TangentCircle {
            first,
            second,
            third: Some(third),
            radius: None,
        }),
        (pick(), pick(), positive()).prop_map(|(first, second, radius)| Operation::TangentArc {
            first,
            second,
            radius
        }),
        (pick(), positive()).prop_map(|(of, radius)| Operation::ConcentricCircle { of, radius }),
        (targets(), pick(), 2u32..=12).prop_map(|(targets, path, count)| Operation::PathArray {
            targets,
            path,
            count: Some(count),
            spacing: None,
        }),
        (targets(), pick(), positive()).prop_map(|(targets, path, spacing)| {
            Operation::PathArray {
                targets,
                path,
                count: None,
                spacing: Some(spacing),
            }
        }),
        (
            targets(),
            point(),
            positive(),
            proptest::option::of(positive())
        )
            .prop_map(|(targets, inside, column_spacing, row_spacing)| {
                Operation::GridArray {
                    targets,
                    inside,
                    column_spacing,
                    row_spacing,
                }
            }),
    ]
}

/// A part (#272), any of them; many are impossible as parts — holes too
/// big for the plate — and the engine refuses those, as it should.
fn part() -> impl Strategy<Value = Operation> {
    prop_oneof![
        (
            point(),
            positive(),
            positive(),
            proptest::option::of(positive()),
            proptest::option::of((positive(), positive()))
        )
            .prop_map(|(corner, width, height, fillet, holes)| Operation::Plate {
                corner,
                width,
                height,
                fillet,
                chamfer: None,
                hole_diameter: holes.map(|(d, _)| d),
                hole_offset: holes.map(|(_, o)| o),
            }),
        (point(), positive(), positive(), angle()).prop_map(|(center, length, width, angle)| {
            Operation::Slot {
                center,
                length,
                width,
                angle,
            }
        }),
        (
            point(),
            positive(),
            1u32..4,
            1u32..4,
            positive(),
            positive()
        )
            .prop_map(
                |(first, diameter, rows, columns, row_spacing, column_spacing)| {
                    Operation::HolePattern {
                        first,
                        diameter,
                        rows,
                        columns,
                        row_spacing,
                        column_spacing,
                    }
                }
            ),
        (point(), positive(), 2u32..12, positive(), angle()).prop_map(
            |(center, pitch_diameter, count, hole_diameter, start_angle)| Operation::BoltCircle {
                center,
                pitch_diameter,
                count,
                hole_diameter,
                start_angle,
            }
        ),
        (
            point(),
            positive(),
            positive(),
            positive(),
            2u32..12,
            positive()
        )
            .prop_map(
                |(center, outer_diameter, bore_diameter, pitch_diameter, count, hole_diameter)| {
                    Operation::RoundFlange {
                        center,
                        outer_diameter,
                        bore_diameter,
                        pitch_diameter,
                        count,
                        hole_diameter,
                    }
                }
            ),
        (
            point(),
            positive(),
            positive(),
            positive(),
            positive(),
            positive()
        )
            .prop_map(
                |(center, width, height, bore_diameter, hole_diameter, hole_offset)| {
                    Operation::RectangularFlange {
                        center,
                        width,
                        height,
                        bore_diameter,
                        hole_diameter,
                        hole_offset,
                    }
                }
            ),
    ]
}

/// A valid document of up to `most` operations.
pub fn intent(most: usize) -> impl Strategy<Value = Intent> {
    (
        proptest::option::of(prop_oneof![
            Just(Unit::Mm),
            Just(Unit::Cm),
            Just(Unit::M),
            Just(Unit::In),
            Just(Unit::Ft)
        ]),
        proptest::collection::vec(operation(), 0..=most),
    )
        .prop_map(|(units, mut operations)| {
            // Each view its own id.
            for (index, operation) in operations.iter_mut().enumerate() {
                match operation {
                    Operation::View { id, .. } => *id = format!("{id}-{index}"),
                    Operation::Datum { name, .. } => *name = format!("{name}-{index}"),
                    _ => {}
                }
            }
            Intent {
                version: VERSION,
                units,
                operations,
            }
        })
}
