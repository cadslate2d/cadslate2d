//! ARRAY (#199): rectangular and polar copies of every kind, previewed
//! from the same transforms the commit uses, one transaction, one undo.

use std::f64::consts::{FRAC_PI_2, PI};

use cad_core::{EntityId, Point2, Vec2};
use cad_document::{
    ArcEntity, Block, BlockReferenceEntity, CircleEntity, DimensionEntity, Drawing, EllipseEntity,
    Entity, EntityRecord, HatchEntity, LineEntity, MTextEntity, Measure, Path, PointEntity,
    PolylineEntity, Properties, SplineEntity, TextEntity,
};
use cad_geometry::Transform;
use proptest::prelude::*;

use crate::commands::{Polar, Rectangular};
use crate::testing::Harness;
use crate::{Dirty, OverlayItem};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

/// One of every kind, within (0, 0) to (120, 120).
fn everything() -> Drawing {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let part = tx
        .define_block(Block::new(
            "Part",
            Point2::ORIGIN,
            vec![EntityRecord::new(
                Entity::Line(LineEntity::new(p(0.0, 0.0), p(5.0, 5.0))),
                Properties::default(),
            )],
        ))
        .unwrap();
    let entities: [Entity; 12] = [
        LineEntity::new(p(0.0, 0.0), p(30.0, 0.0)).into(),
        CircleEntity::new(p(50.0, 10.0), 8.0).into(),
        ArcEntity::new(p(80.0, 10.0), 8.0, 0.0, PI).into(),
        PolylineEntity::with_bulges(
            vec![p(100.0, 0.0), p(115.0, 0.0), p(115.0, 15.0)],
            vec![0.0, 1.0, 0.0],
            true,
        )
        .into(),
        TextEntity::new(p(0.0, 40.0), 5.0, 0.0, "Ab").into(),
        MTextEntity::new(p(40.0, 40.0), 30.0, 4.0, 0.0, "Two\nlines").into(),
        BlockReferenceEntity::new(part, p(90.0, 40.0), 2.0, FRAC_PI_2).into(),
        DimensionEntity::new(
            Measure::Aligned {
                a: p(0.0, 70.0),
                b: p(30.0, 70.0),
            },
            p(15.0, 80.0),
        )
        .into(),
        HatchEntity::solid(vec![Path {
            vertices: vec![p(50.0, 70.0), p(70.0, 70.0), p(70.0, 90.0), p(50.0, 90.0)],
            bulges: vec![],
            closed: true,
        }])
        .into(),
        EllipseEntity::new(p(100.0, 80.0), Vec2::new(10.0, 0.0), 0.5).into(),
        SplineEntity::through(&[p(0.0, 100.0), p(20.0, 115.0), p(40.0, 100.0)])
            .unwrap()
            .into(),
        PointEntity::new(p(60.0, 110.0)).into(),
    ];
    for entity in entities {
        tx.insert(entity).unwrap();
    }
    tx.commit();
    drawing
}

/// The entities beyond `sources`, in ID order, as `(id, entity)`.
fn copies(h: &Harness, sources: &[EntityId]) -> Vec<(EntityId, Entity)> {
    let mut all: Vec<(EntityId, Entity)> = h
        .session
        .drawing()
        .shown()
        .filter(|(id, _)| !sources.contains(id))
        .map(|(id, entity)| (id, entity.clone()))
        .collect();
    all.sort_by_key(|(id, _)| *id);
    all
}

/// Every source under every transform, in the order the commit makes them.
fn expected_copies(h: &Harness, sources: &[EntityId], transforms: &[Transform]) -> Vec<Entity> {
    transforms
        .iter()
        .flat_map(|transform| {
            sources.iter().map(move |id| {
                h.session
                    .drawing()
                    .entity(*id)
                    .unwrap()
                    .transformed(transform)
            })
        })
        .collect()
}

fn select_everything(h: &mut Harness) -> Vec<EntityId> {
    h.enter_all(&["ZOOM", "E"]);
    h.select_box(p(-10.0, -10.0), p(130.0, 130.0));
    let sources = h.ids();
    assert_eq!(h.session.selection().len(), sources.len());
    sources
}

#[test]
fn a_rectangular_array_copies_every_kind_with_its_properties_in_one_undo() {
    let mut h = Harness::with_drawing(everything());
    let sources = select_everything(&mut h);
    assert_eq!(sources.len(), 12);
    h.enter("ARRAY");
    assert_eq!(
        h.prompt(),
        "Enter the type of array or [Rectangular/Polar] <Rectangular>:"
    );
    h.enter("");
    assert_eq!(h.prompt(), "Enter the number of rows (---) <1>:");
    h.enter("2");
    assert_eq!(h.prompt(), "Enter the number of columns (|||) <1>:");
    h.enter("3");
    assert_eq!(h.prompt(), "Specify the distance between rows:");
    h.enter("200");
    assert_eq!(h.prompt(), "Specify the distance between columns:");
    let dirty = h.enter("-150");
    assert!(dirty.contains(Dirty::SCENE));
    assert_eq!(last(&h), "2 rows by 3 columns: 60 copies made.");
    assert_eq!(h.prompt(), "Command:");
    assert!(h.session.selection().is_empty());
    let grid = Rectangular {
        rows: 2,
        columns: 3,
        row_spacing: 200.0,
        column_spacing: -150.0,
    };
    let made = copies(&h, &sources);
    let expected = expected_copies(&h, &sources, &grid.transforms());
    assert_eq!(made.len(), expected.len());
    for ((id, made), expected) in made.iter().zip(&expected) {
        assert_eq!(made, expected, "copy {id}");
    }
    // Each copy has its source's properties: the block reference's layer.
    let properties = h.session.drawing().properties(made[6].0).unwrap();
    assert_eq!(
        properties,
        h.session.drawing().properties(sources[6]).unwrap()
    );
    h.undo();
    assert_eq!(
        h.session.drawing().shown().count(),
        12,
        "one undo takes every copy"
    );
    h.redo();
    assert_eq!(
        h.session.drawing().shown().count(),
        72,
        "and one redo brings them back"
    );
}

#[test]
fn a_polar_array_turns_every_kind_round_the_centre_or_keeps_it_upright() {
    let mut h = Harness::with_drawing(everything());
    let sources = select_everything(&mut h);
    h.enter_all(&["AR", "P"]);
    assert_eq!(h.prompt(), "Specify center point of array:");
    h.enter("200,200");
    assert_eq!(h.prompt(), "Enter the number of items in the array:");
    h.enter("5");
    assert_eq!(h.prompt(), "Specify the angle to fill (+=ccw, -=cw) <360>:");
    h.enter("");
    assert_eq!(h.prompt(), "Rotate arrayed objects? [Yes/No] <Yes>:");
    h.enter("");
    assert_eq!(
        last(&h),
        "5 items round 200,200, over 360°: 48 copies made."
    );
    let round = Polar {
        center: p(200.0, 200.0),
        count: 5,
        fill_degrees: 360.0,
        rotate_items: true,
        reference: Point2::ORIGIN,
    };
    let made = copies(&h, &sources);
    let expected = expected_copies(&h, &sources, &round.transforms());
    for ((id, made), expected) in made.iter().zip(&expected) {
        assert_eq!(made, expected, "copy {id}");
    }
    h.undo();

    // Not rotated: moved round, the same way up — the text's rotation,
    // the block's, and the arc's angles as they were.
    let sources = select_everything(&mut h);
    h.enter_all(&["AR", "P", "200,200", "4", "180", "N"]);
    assert_eq!(
        last(&h),
        "4 items round 200,200, over 180°: 36 copies made."
    );
    let made = copies(&h, &sources);
    for (_, entity) in &made {
        match entity {
            Entity::Text(text) => assert_eq!(text.rotation, 0.0),
            Entity::BlockRef(reference) => assert_eq!(reference.rotation, FRAC_PI_2),
            Entity::Arc(arc) => assert_eq!((arc.start_angle, arc.end_angle), (0.0, PI)),
            _ => {}
        }
    }
    // Three steps of 60° over the half turn: the circle's copies on a
    // circle about the centre, 60° apart.
    let centres: Vec<Point2> = made
        .iter()
        .filter_map(|(_, e)| match e {
            Entity::Circle(c) => Some(c.center),
            _ => None,
        })
        .collect();
    assert_eq!(centres.len(), 3);
    let reference = h
        .session
        .drawing()
        .entity(sources[1])
        .map(|e| match e {
            Entity::Circle(c) => c.center,
            _ => unreachable!(),
        })
        .unwrap();
    // The selection's bounds' centre is the reference every item is
    // placed by, so the circle's centre moves as that point does.
    let bounds_centre = sources
        .iter()
        .map(|id| {
            let drawing = h.session.drawing();
            drawing.bounds_of(drawing.entity(*id).unwrap())
        })
        .reduce(|a, b| a.union(&b))
        .unwrap()
        .center();
    for (i, centre) in centres.iter().enumerate() {
        let turn = Transform::rotation(p(200.0, 200.0), 60.0 * (i + 1) as f64);
        let want = reference + (turn.apply(bounds_centre) - bounds_centre);
        assert!(
            centre.distance_to(want) < 1e-9,
            "{centre:?} is not {want:?}"
        );
    }
}

#[test]
fn the_preview_is_the_array_the_commit_makes_and_touches_nothing() {
    let mut h = Harness::with_entities([CircleEntity::new(p(10.0, 10.0), 5.0).into()]);
    let id = h.ids()[0];
    h.click_world(p(15.0, 10.0));
    h.enter_all(&["ARRAY", "R", "2", "2", "40"]);
    assert_eq!(h.prompt(), "Specify the distance between columns:");
    let revision = h.session.drawing().revision();
    // The pointer 60 to the right of the selection's corner (5, 5).
    for x in 10..60 {
        let dirty = h.move_to_world(p(5.0 + f64::from(x), 5.0));
        assert!(!dirty.contains(Dirty::SCENE));
    }
    h.move_to_world(p(65.0, 5.0));
    let previewed: Vec<Transform> = h
        .session
        .overlay()
        .into_iter()
        .filter_map(|item| match item {
            OverlayItem::PreviewTransformed { ids, transform } if ids == vec![id] => {
                Some(transform)
            }
            _ => None,
        })
        .collect();
    assert_eq!(previewed.len(), 3);
    assert_eq!(h.session.drawing().revision(), revision);
    h.enter("60");
    let made = copies(&h, &[id]);
    assert_eq!(made.len(), 3);
    for ((_, copy), transform) in made.iter().zip(&previewed) {
        let source = h.session.drawing().entity(id).unwrap();
        assert_eq!(copy, &source.transformed(transform), "as previewed");
    }
    // Polar: the fill from the pointer's angle about the centre.
    h.undo();
    h.click_world(p(15.0, 10.0));
    h.enter_all(&["ARRAY", "P", "0,0", "3"]);
    h.move_to_world(p(0.0, 50.0));
    let previewed: Vec<Transform> = h
        .session
        .overlay()
        .into_iter()
        .filter_map(|item| match item {
            OverlayItem::PreviewTransformed { transform, .. } => Some(transform),
            _ => None,
        })
        .collect();
    assert_eq!(
        previewed,
        Polar {
            center: Point2::ORIGIN,
            count: 3,
            fill_degrees: 90.0,
            rotate_items: true,
            reference: p(10.0, 10.0),
        }
        .transforms()
    );
    h.enter_all(&["90", ""]);
    let made = copies(&h, &[id]);
    assert_eq!(made.len(), 2);
    assert_eq!(
        made[1].1,
        h.session
            .drawing()
            .entity(id)
            .unwrap()
            .transformed(&previewed[1])
    );
}

#[test]
fn counts_spacings_and_fills_are_checked_and_an_array_of_one_is_nothing() {
    let mut h = Harness::with_entities([LineEntity::new(p(0.0, 0.0), p(10.0, 0.0)).into()]);
    h.click_world(p(5.0, 0.0));
    h.enter_all(&["ARRAY", "", "0"]);
    assert_eq!(last(&h), "Enter a whole number of rows, 1 or more.");
    h.enter("1.5");
    assert_eq!(last(&h), "Enter a whole number of rows, 1 or more.");
    h.enter_all(&["2", "0"]);
    assert_eq!(last(&h), "Enter a whole number of columns, 1 or more.");
    h.enter_all(&["1", "0"]);
    assert_eq!(last(&h), "The distance between rows cannot be zero.");
    h.enter("25");
    assert_eq!(last(&h), "2 rows by 1 columns: 1 copies made.");
    assert_eq!(h.session.drawing().shown().count(), 2);
    h.undo();
    // One row, one column: nothing to make.
    h.click_world(p(5.0, 0.0));
    h.enter_all(&["ARRAY", "", "", ""]);
    assert_eq!(last(&h), "An array of one is the selection as it is.");
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.session.drawing().shown().count(), 1);
    // Polar refusals.
    h.click_world(p(5.0, 0.0));
    h.enter_all(&["ARRAY", "P", "0,0", "1"]);
    assert_eq!(last(&h), "Enter a whole number of items, 2 or more.");
    h.enter_all(&["3", "0"]);
    assert_eq!(last(&h), "The angle to fill is up to 360°, and not zero.");
    h.enter("400");
    assert_eq!(last(&h), "The angle to fill is up to 360°, and not zero.");
    h.enter_all(&["-90", "Y"]);
    assert_eq!(last(&h), "3 items round 0,0, over -90°: 2 copies made.");
    // Nothing selected: asked, and Enter with nothing finishes.
    h.enter_all(&["ARRAY", ""]);
    assert_eq!(last(&h), "No objects selected.");
}

proptest! {
    /// #199: a polar array of n items about a centre has every item at
    /// the same distance from it and n equal angles between neighbours.
    #[test]
    fn a_polar_array_is_n_items_at_one_distance_and_equal_angles(
        cx in -100.0..100.0_f64,
        cy in -100.0..100.0_f64,
        count in 2..13usize,
        fill in prop_oneof![Just(360.0), Just(-360.0), -350.0..-10.0_f64, 10.0..350.0_f64],
        rotate in any::<bool>(),
    ) {
        let center = p(cx, cy);
        let mut h = Harness::with_entities([CircleEntity::new(p(150.0, 20.0), 5.0).into()]);
        h.enter_all(&["ZOOM", "E"]);
        h.click_world(p(155.0, 20.0));
        h.enter_all(&["ARRAY", "P", &format!("{cx},{cy}"), &count.to_string(), &format!("{fill}"), if rotate { "Y" } else { "N" }]);
        let mut centres: Vec<Point2> = h
            .session
            .drawing()
            .shown()
            .filter_map(|(_, e)| match e {
                Entity::Circle(c) => Some(c.center),
                _ => None,
            })
            .collect();
        prop_assert_eq!(centres.len(), count);
        let distance = p(150.0, 20.0).distance_to(center);
        for c in &centres {
            prop_assert!((c.distance_to(center) - distance).abs() < 1e-6, "{c:?}");
        }
        // The angles about the centre, in the order made, step evenly.
        let step = if fill.abs() == 360.0 { fill / count as f64 } else { fill / (count as f64 - 1.0) };
        let first = centres.remove(0);
        let angle_of = |c: Point2| (c.y - center.y).atan2(c.x - center.x).to_degrees();
        let base = angle_of(first);
        for (i, c) in centres.iter().enumerate() {
            let want = (base + step * (i + 1) as f64).rem_euclid(360.0);
            let got = angle_of(*c).rem_euclid(360.0);
            let apart = (got - want).abs();
            prop_assert!(apart < 1e-6 || (360.0 - apart) < 1e-6, "{got} is not {want}");
        }
    }
}
