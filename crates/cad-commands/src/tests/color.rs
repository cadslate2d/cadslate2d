//! COLOR (#135) through the session: the colour new objects are given.
//! Reading a colour as typed, and the palette, are tested in
//! `cad-document/src/aci.rs`.

use cad_core::Point2;
use cad_document::{Color, LayerId, Paint, Properties};

use crate::testing::Harness;

const PROMPT: &str = "Enter colour for new objects (1-255, a name, ByLayer, or ByBlock)";

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn colours(h: &Harness) -> Vec<Color> {
    h.properties().into_iter().map(|p| p.color).collect()
}

#[test]
fn new_objects_take_the_current_colour_and_old_ones_keep_theirs() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "10,0", ""]);
    h.enter("COLOR");
    assert_eq!(h.prompt(), format!("{PROMPT} <ByLayer>:"));
    h.enter("red");
    assert_eq!(last(&h), "New objects are red (colour 1).");
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(h.session.drawing().current_color(), Color::Index(1));

    h.enter_all(&[
        "L", "0,5", "10,5", "", "C", "50,50", "5", "REC", "80,80", "90,90",
    ]);
    assert_eq!(
        colours(&h),
        vec![
            Color::ByLayer,
            Color::Index(1),
            Color::Index(1),
            Color::Index(1)
        ]
    );

    h.enter("COL");
    assert_eq!(h.prompt(), format!("{PROMPT} <red>:"));
    h.enter("42");
    assert_eq!(last(&h), "New objects are colour 42.");
    h.enter_all(&["COLOUR", "ByLayer"]);
    assert_eq!(last(&h), "New objects take their layer's colour.");
    h.enter_all(&["COLOR", "byblock"]);
    assert_eq!(
        last(&h),
        "New objects take the colour of the block they are in."
    );
    assert_eq!(h.session.drawing().current_color(), Color::ByBlock);
}

#[test]
fn the_current_colour_is_undone_and_enter_keeps_it() {
    let mut h = Harness::new();
    h.enter_all(&["COLOR", "blue"]);
    let revision = h.session.drawing().revision();
    h.enter_all(&["COLOR", ""]);
    assert_eq!(h.prompt(), "Command:");
    h.enter_all(&["COLOR", "5"]);
    assert_eq!(
        h.session.drawing().revision(),
        revision,
        "no transaction for no change"
    );
    assert_eq!(last(&h), "New objects are blue (colour 5).");

    h.undo();
    assert_eq!(h.session.drawing().current_color(), Color::ByLayer);
    h.redo();
    assert_eq!(h.session.drawing().current_color(), Color::Index(5));
}

#[test]
fn text_that_is_not_a_colour_is_explained_and_asked_for_again() {
    let mut h = Harness::new();
    h.enter("COLOR");
    for bad in ["0", "256", "purple", "2.5", "1,2"] {
        h.enter(bad);
        assert_eq!(
            last(&h),
            "A colour is a number from 1 to 255, or red, yellow, green, cyan, blue, magenta, or white. Or ByLayer, or ByBlock.",
            "{bad}"
        );
        assert_eq!(h.prompt(), format!("{PROMPT} <ByLayer>:"));
    }
    h.enter("Magenta");
    assert_eq!(h.session.drawing().current_color(), Color::Index(6));
}

#[test]
fn an_object_made_from_another_keeps_that_ones_colour() {
    let mut h = Harness::new();
    h.enter_all(&["COLOR", "red", "L", "0,0", "100,0", "", "COLOR", "green"]);
    h.enter_all(&["O", "5"]);
    h.click_world(Point2::new(50.0, 0.0));
    h.click_world(Point2::new(50.0, 30.0));
    h.enter("");
    assert_eq!(
        colours(&h),
        vec![Color::Index(1), Color::Index(1)],
        "red, not green"
    );
}

/// The issue's criterion: an object coloured ByLayer follows a change of
/// its layer's colour; an object with a colour of its own does not.
#[test]
fn by_layer_follows_the_layer_and_an_objects_own_colour_does_not() {
    let mut h = Harness::new();
    h.enter_all(&["LA", "N", "walls", "C", "blue", "walls", "S", "walls", ""]);
    h.enter_all(&["L", "0,0", "10,0", ""]); // ByLayer
    h.enter_all(&["COLOR", "red", "L", "0,5", "10,5", ""]); // its own red
    let walls = h.session.drawing().layers().find("walls").unwrap();
    assert_ne!(walls, LayerId::FIRST);
    assert_eq!(
        h.properties(),
        vec![
            Properties::on(walls),
            Properties {
                color: Color::Index(1),
                ..Properties::on(walls)
            }
        ]
    );

    // What each is painted with, resolved against its layer as it is now.
    let paints = |h: &Harness| -> Vec<Paint> {
        let drawing = h.session.drawing();
        h.ids()
            .into_iter()
            .map(|id| {
                let layer = drawing.layer_of(id).expect("its layer exists");
                drawing
                    .properties(id)
                    .expect("it exists")
                    .color
                    .paint_on(layer)
            })
            .collect()
    };
    assert_eq!(
        paints(&h),
        vec![Paint::Rgb([0, 0, 255]), Paint::Rgb([255, 0, 0])]
    );

    h.enter_all(&["LA", "C", "green", "walls", ""]);
    assert_eq!(
        paints(&h),
        vec![Paint::Rgb([0, 255, 0]), Paint::Rgb([255, 0, 0])],
        "only the ByLayer line followed"
    );
}
