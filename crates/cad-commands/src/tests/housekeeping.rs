//! MATCHPROP, PURGE, RENAME (#200): properties copied, unused rows
//! swept, rows renamed — each one undo.

use cad_core::Point2;
use cad_document::{
    Block, BlockReferenceEntity, CircleEntity, Color, DimStyle, DimensionEntity, Drawing, Entity,
    EntityRecord, Layer, LayerId, LineEntity, Linetype, LinetypeDef, LinetypeId, Lineweight,
    Measure, Properties, TextEntity, TextStyle, TextStyleId,
};

use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

/// A drawing with rows to match, purge, and rename: a styled line as a
/// source; a plain circle, two texts, two dimensions; unused rows beside
/// used ones; a block placed and one not.
fn furnished() -> Drawing {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let dashed = tx
        .add_linetype(LinetypeDef::built_in_named("DASHED").unwrap())
        .unwrap();
    let _center = tx
        .add_linetype(LinetypeDef::built_in_named("CENTER").unwrap())
        .unwrap();
    let walls = tx.add_layer(Layer::named("WALLS")).unwrap();
    let _free = tx.add_layer(Layer::named("FREE")).unwrap();
    let titles = tx.add_text_style(TextStyle::named("TITLES")).unwrap();
    let _notes = tx.add_text_style(TextStyle::named("NOTES")).unwrap();
    let arch = tx.add_dim_style(DimStyle::named("ARCH")).unwrap();
    let _civil = tx.add_dim_style(DimStyle::named("CIVIL")).unwrap();
    let part = tx
        .define_block(Block::new(
            "PART",
            Point2::ORIGIN,
            vec![EntityRecord::new(
                Entity::Line(LineEntity::new(p(0.0, 0.0), p(1.0, 0.0))),
                Properties::default(),
            )],
        ))
        .unwrap();
    let _spare = tx
        .define_block(Block::new("SPARE", Point2::ORIGIN, vec![]))
        .unwrap();
    // The source: on WALLS, red, dashed, 0.50 mm.
    tx.insert_with(
        LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)),
        Properties {
            layer: walls,
            color: Color::Index(1),
            linetype: Linetype::Named(dashed),
            lineweight: Lineweight::Hundredths(50),
        },
    )
    .unwrap();
    tx.insert(CircleEntity::new(p(200.0, 0.0), 10.0)).unwrap();
    tx.insert(TextEntity {
        style: titles,
        ..TextEntity::new(p(0.0, 50.0), 8.0, 0.0, "Title")
    })
    .unwrap();
    tx.insert(TextEntity::new(p(100.0, 50.0), 3.0, 0.0, "Note"))
        .unwrap();
    tx.insert(DimensionEntity {
        style: arch,
        ..DimensionEntity::new(
            Measure::Aligned {
                a: p(0.0, 100.0),
                b: p(50.0, 100.0),
            },
            p(25.0, 110.0),
        )
    })
    .unwrap();
    tx.insert(DimensionEntity::new(
        Measure::Aligned {
            a: p(100.0, 100.0),
            b: p(150.0, 100.0),
        },
        p(125.0, 110.0),
    ))
    .unwrap();
    tx.insert(BlockReferenceEntity::new(part, p(200.0, 100.0), 1.0, 0.0))
        .unwrap();
    tx.commit();
    drawing
}

#[test]
fn matchprop_copies_the_sources_properties_onto_the_destinations_as_one_undo() {
    let mut h = Harness::with_drawing(furnished());
    let ids = h.ids();
    let (source, circle, _title, note, dim_a, dim_b, reference) =
        (ids[0], ids[1], ids[2], ids[3], ids[4], ids[5], ids[6]);
    h.enter_all(&["ZOOM", "E", "MATCHPROP"]);
    assert_eq!(h.prompt(), "Select source object:");
    h.click_world(p(50.0, 0.0));
    assert!(
        h.prompt()
            .starts_with("Select destination objects or [Settings/"),
        "{}",
        h.prompt()
    );
    h.click_world(p(210.0, 0.0));
    h.click_world(p(100.0, 50.0));
    h.enter("");
    assert_eq!(
        last(&h),
        "2 objects matched: layer, colour, linetype, lineweight, text style and height, dimension style."
    );
    assert_eq!(h.prompt(), "Command:");
    let from = h.session.drawing().properties(source).unwrap();
    for id in [circle, note] {
        assert_eq!(h.session.drawing().properties(id).unwrap(), from);
    }
    // A line is no text: the note's style and height are its own.
    let Some(Entity::Text(text)) = h.session.drawing().entity(note) else {
        panic!("a text");
    };
    assert_eq!((text.style, text.height), (TextStyleId::FIRST, 3.0));
    h.undo();
    assert_eq!(
        h.session.drawing().properties(circle).unwrap(),
        Properties::default(),
        "one undo"
    );

    // A text source matches a text's style and height; a dimension
    // source a dimension's style; the block reference its properties.
    h.enter("MA");
    h.click_world(p(0.0, 50.0));
    h.click_world(p(100.0, 50.0));
    h.enter("");
    let Some(Entity::Text(text)) = h.session.drawing().entity(note) else {
        panic!("a text");
    };
    let titles = h.session.drawing().text_styles().find("TITLES").unwrap();
    assert_eq!((text.style, text.height), (titles, 8.0));
    h.enter("MA");
    h.click_world(p(25.0, 110.0));
    h.click_world(p(125.0, 110.0));
    h.click_world(p(200.0, 100.0));
    h.enter("");
    let arch = h.session.drawing().dim_styles().find("ARCH").unwrap();
    let Some(Entity::Dimension(dimension)) = h.session.drawing().entity(dim_b) else {
        panic!("a dimension");
    };
    assert_eq!(dimension.style, arch);
    let Some(Entity::Dimension(dimension)) = h.session.drawing().entity(dim_a) else {
        panic!("a dimension");
    };
    assert_eq!(dimension.style, arch, "the source as it was");
    assert_eq!(
        h.session.drawing().properties(reference).unwrap(),
        h.session.drawing().properties(dim_a).unwrap()
    );

    // Settings: only the layer.
    h.undo();
    h.undo();
    h.enter("MA");
    h.click_world(p(50.0, 0.0));
    h.enter("S");
    assert_eq!(
        h.prompt(),
        "Switch a setting, or Enter when done or [Layer/Color/LType/LWeight/Text/Dimension/ALL]:"
    );
    h.enter_all(&["C", "LT", "LW", "T", "D"]);
    assert_eq!(last(&h), "Matching: layer.");
    h.enter("");
    assert!(
        h.prompt()
            .starts_with("Select destination objects or [Settings/")
    );
    h.click_world(p(210.0, 0.0));
    h.enter("");
    assert_eq!(last(&h), "1 object matched: layer.");
    let got = h.session.drawing().properties(circle).unwrap();
    assert_eq!(got.layer, from.layer);
    assert_eq!(
        (got.color, got.linetype),
        (Color::ByLayer, Linetype::ByLayer)
    );
    // Nothing picked: nothing done.
    h.enter_all(&["MA", ""]);
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn purge_sweeps_the_unused_rows_as_one_undo_and_a_question_only_tells() {
    let mut h = Harness::with_drawing(furnished());
    let rows = |h: &Harness| {
        let d = h.session.drawing();
        (
            d.layers().len(),
            d.linetypes().len(),
            d.text_styles().len(),
            d.dim_styles().len(),
            d.blocks().len(),
        )
    };
    let before = rows(&h);
    h.enter("PURGE");
    assert_eq!(
        h.prompt(),
        "Enter type of unused objects to purge or [Blocks/Dimstyles/LAyers/LTypes/STyles/All/?] <All>:"
    );
    h.enter("?");
    assert_eq!(
        last(&h),
        "Unused: 1 block (SPARE), 1 dimension style (CIVIL), 1 layer (FREE), 1 linetype (Center), 1 text style (NOTES)."
    );
    assert_eq!(rows(&h), before, "told, not done");
    h.enter("LA");
    assert_eq!(last(&h), "Purged 1 layer (FREE).");
    assert_eq!(h.prompt(), "Command:");
    assert!(h.session.drawing().layers().find("FREE").is_none());
    assert_eq!(h.session.drawing().linetypes().len(), before.1);
    h.undo();
    assert_eq!(rows(&h), before, "one undo");
    h.enter_all(&["PU", ""]);
    assert_eq!(
        last(&h),
        "Purged 1 block (SPARE), 1 dimension style (CIVIL), 1 layer (FREE), 1 linetype (Center), 1 text style (NOTES)."
    );
    assert_eq!(
        rows(&h),
        (
            before.0 - 1,
            before.1 - 1,
            before.2 - 1,
            before.3 - 1,
            before.4 - 1
        )
    );
    assert!(h.session.drawing().layers().find("WALLS").is_some());
    assert!(h.session.drawing().blocks().find("PART").is_some());
    h.enter_all(&["PU", ""]);
    assert_eq!(last(&h), "Nothing to purge.");
    h.undo();
    assert_eq!(rows(&h), before, "one undo takes every row back");
    assert_eq!(h.session.drawing().linetypes().len(), 3);
    let _ = (LayerId::FIRST, LinetypeId::FIRST);
}

#[test]
fn rename_renames_a_row_of_any_table_and_refuses_what_the_table_does() {
    let mut h = Harness::with_drawing(furnished());
    h.enter_all(&["LAYOUT", "N", "Sheet", "MODEL"]);
    h.enter("RENAME");
    assert_eq!(
        h.prompt(),
        "Enter object type to rename or [Block/Dimstyle/LAyer/LType/Style/LAYOUT]:"
    );
    h.enter("LA");
    assert_eq!(h.prompt(), "Enter old layer name:");
    h.enter("nowhere");
    assert_eq!(last(&h), "There is no layer \"nowhere\".");
    h.enter("walls");
    assert_eq!(h.prompt(), "Enter new layer name:");
    h.enter("free");
    assert_eq!(last(&h), "There is already a layer named \"free\".");
    h.enter("a,b");
    assert!(last(&h).contains("layer"), "{}", last(&h));
    h.enter("Partitions");
    assert_eq!(last(&h), "Layer \"walls\" renamed \"Partitions\".");
    assert_eq!(h.prompt(), "Command:");
    assert!(h.session.drawing().layers().find("Partitions").is_some());
    let source = h.ids()[0];
    let walls = h.session.drawing().layers().find("Partitions").unwrap();
    assert_eq!(h.session.drawing().properties(source).unwrap().layer, walls);
    h.undo();
    assert!(h.session.drawing().layers().find("WALLS").is_some());

    h.enter_all(&["REN", "LA", "0", "Zero"]);
    assert_eq!(last(&h), "That layer cannot be renamed.");
    h.escape();
    h.enter_all(&["REN", "LT", "DASHED", "Broken"]);
    assert_eq!(last(&h), "Linetype \"DASHED\" renamed \"Broken\".");
    h.enter_all(&["REN", "LT", "Continuous", "Solid"]);
    assert_eq!(last(&h), "That linetype cannot be renamed.");
    h.escape();
    h.enter_all(&["REN", "B", "SPARE", "Part"]);
    assert_eq!(last(&h), "There is already a block named \"Part\".");
    h.enter("Extra");
    assert_eq!(last(&h), "Block \"SPARE\" renamed \"Extra\".");
    h.enter_all(&["REN", "D", "CIVIL", "Roads"]);
    assert_eq!(last(&h), "Dimension style \"CIVIL\" renamed \"Roads\".");
    h.enter_all(&["REN", "D", "Standard", "Plain"]);
    assert_eq!(last(&h), "That dimension style cannot be renamed.");
    h.escape();
    h.enter_all(&["REN", "S", "NOTES", "Remarks"]);
    assert_eq!(last(&h), "Text style \"NOTES\" renamed \"Remarks\".");
    h.enter_all(&["REN", "LAYOUT", "Sheet", "Sheet one"]);
    assert_eq!(last(&h), "Layout \"Sheet\" renamed \"Sheet one\".");
    assert_eq!(h.session.ui_state().layouts, vec!["Sheet one"]);
    h.enter_all(&["REN", "LAYOUT", "Model", "Paper"]);
    assert_eq!(last(&h), "That layout cannot be renamed.");
    h.escape();
}
