//! LAYER (#133) through the session: every option, names with spaces,
//! several names at once, and every refusal. What the layer table allows
//! is tested in `cad-document/src/drawing.rs`; what off and locked mean
//! for drafting, in `layers.rs`.

use cad_core::Point2;
use cad_document::{Color, Layer, LayerId, Properties};

use crate::testing::Harness;

const OPTIONS: &str =
    "Press Enter to finish or [?/New/Set/Color/LWeight/LType/ON/OFF/Lock/Unlock/Rename/Delete]:";

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn layer<'a>(h: &'a Harness, name: &str) -> &'a Layer {
    let layers = h.session.drawing().layers();
    let id = layers
        .find(name)
        .unwrap_or_else(|| panic!("no layer {name}"));
    layers.get(id).unwrap()
}

fn layer_names(h: &Harness) -> Vec<String> {
    h.session
        .drawing()
        .layers()
        .iter()
        .map(|(_, layer)| layer.name.clone())
        .collect()
}

fn current(h: &Harness) -> String {
    let drawing = h.session.drawing();
    drawing
        .layers()
        .get(drawing.current_layer())
        .unwrap()
        .name
        .clone()
}

/// A session with layers `walls` and `doors` besides `0`.
fn with_layers() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["LA", "N", "walls,doors", ""]);
    assert_eq!(h.prompt(), "Command:");
    h
}

#[test]
fn the_command_says_which_layer_is_current_and_loops_until_enter() {
    let mut h = Harness::new();
    h.enter("LAYER");
    assert!(h.log().contains(&"Current layer: \"0\"".to_owned()));
    assert_eq!(h.prompt(), OPTIONS);
    h.enter("N");
    assert_eq!(h.prompt(), "Enter name(s) for new layer(s):");
    h.enter("walls");
    assert_eq!(last(&h), "1 layer created.");
    assert_eq!(h.prompt(), OPTIONS, "back to the options");
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
    assert_eq!(layer_names(&h), ["0", "walls"]);
}

#[test]
fn new_makes_several_layers_at_once_and_names_may_have_spaces() {
    let mut h = Harness::new();
    h.enter_all(&["LA", "New"]);
    assert!(
        !h.session.ui_state().space_accepts,
        "at a name prompt a space is part of the name"
    );
    h.enter("Ground floor, walls ,A-DOOR");
    assert_eq!(last(&h), "3 layers created.");
    assert!(
        h.session.ui_state().space_accepts,
        "and submits again after"
    );
    h.enter("");
    assert_eq!(layer_names(&h), ["0", "Ground floor", "walls", "A-DOOR"]);
    let ground = layer(&h, "ground FLOOR");
    assert!(ground.on && !ground.locked);
    assert_eq!(ground.color, Color::Index(7));

    // One transaction for the option, one undo step for the command.
    h.undo();
    assert_eq!(layer_names(&h), ["0"]);
    h.redo();
    assert_eq!(layer_names(&h).len(), 4);
}

#[test]
fn new_refuses_bad_and_taken_names_and_then_makes_none_of_the_list() {
    let mut h = with_layers();
    let cases = [
        ("windows,WALLS", "There is already a layer \"walls\"."),
        ("a*b", "A layer name cannot contain \"*\"."),
        ("roof,,eaves", "A layer name cannot be empty."),
        ("0", "There is already a layer \"0\"."),
        (
            &"x".repeat(256),
            "A layer name can have at most 255 characters.",
        ),
    ];
    for (typed, message) in cases {
        h.enter_all(&["LA", "N", typed]);
        assert_eq!(last(&h), message, "{typed}");
        assert_eq!(h.prompt(), OPTIONS);
        h.enter("");
        assert_eq!(
            layer_names(&h),
            ["0", "walls", "doors"],
            "{typed}: none made"
        );
    }
    // A name given twice is one layer.
    h.enter_all(&["LA", "N", "roof, Roof", ""]);
    assert_eq!(layer_names(&h), ["0", "walls", "doors", "roof"]);
}

#[test]
fn set_makes_a_layer_current_and_new_objects_go_on_it() {
    let mut h = with_layers();
    h.enter_all(&["LA", "S"]);
    assert_eq!(h.prompt(), "Enter name of layer to make current:");
    h.enter("WALLS");
    assert_eq!(last(&h), "Current layer: \"walls\"");
    h.enter("");
    assert_eq!(current(&h), "walls");

    h.enter_all(&["L", "0,0", "10,0", ""]);
    let walls = h.session.drawing().layers().find("walls").unwrap();
    assert_eq!(h.properties(), vec![Properties::on(walls)]);

    h.undo(); // the line
    h.undo(); // the LAYER command
    assert_eq!(current(&h), "0");

    h.enter_all(&["LA", "S", "nowhere"]);
    assert_eq!(last(&h), "There is no layer \"nowhere\".");
    h.enter_all(&["OFF", "doors", "S", "doors"]);
    assert_eq!(
        last(&h),
        "Layer \"doors\" is off: switch it on before making it current."
    );
    h.enter("");
    assert_eq!(current(&h), "0");
}

#[test]
fn color_gives_layers_a_colour_from_the_palette() {
    let mut h = with_layers();
    h.enter_all(&["LA", "C"]);
    assert_eq!(h.prompt(), "Enter colour (1-255 or a name):");
    for bad in ["0", "256", "2.5", "-3", "purple"] {
        h.enter(bad);
        assert_eq!(
            last(&h),
            "A colour is a number from 1 to 255, or red, yellow, green, cyan, blue, magenta, or white.",
            "{bad}"
        );
        assert_eq!(h.prompt(), "Enter colour (1-255 or a name):");
    }
    for reference in ["ByLayer", "byblock"] {
        h.enter(reference);
        assert_eq!(
            last(&h),
            "A layer needs a colour of its own, not ByLayer or ByBlock."
        );
    }
    h.enter("3");
    assert_eq!(h.prompt(), "Enter name(s) of layer(s) for colour 3:");
    h.enter("walls, 0");
    assert_eq!(last(&h), "2 layers set to colour 3.");
    h.enter("");
    assert_eq!(layer(&h, "walls").color, Color::Index(3));
    assert_eq!(layer(&h, "0").color, Color::Index(3));
    assert_eq!(layer(&h, "doors").color, Color::Index(7));

    // A name is the same as its number.
    h.enter_all(&["LA", "C", "Blue", "doors", ""]);
    assert_eq!(layer(&h, "doors").color, Color::Index(5));

    h.enter_all(&["LA", "C", "1", "walls,nowhere"]);
    assert_eq!(last(&h), "There is no layer \"nowhere\".");
    h.enter("");
    assert_eq!(
        layer(&h, "walls").color,
        Color::Index(3),
        "none of the list changed"
    );
}

#[test]
fn lweight_gives_layers_a_standard_lineweight() {
    use cad_document::Lineweight;

    let mut h = with_layers();
    h.enter_all(&["LA", "LW"]);
    assert_eq!(h.prompt(), "Enter lineweight (mm or Default):");
    h.enter("0.26");
    assert!(
        last(&h).starts_with("A lineweight is one of the standard widths in millimetres: 0, 0.05,")
    );
    assert!(last(&h).ends_with("2, or 2.11. Or Default."));
    h.enter("ByLayer");
    assert_eq!(
        last(&h),
        "A layer needs a lineweight of its own, not ByLayer or ByBlock."
    );
    assert_eq!(h.prompt(), "Enter lineweight (mm or Default):");
    h.enter("0.5");
    assert_eq!(
        h.prompt(),
        "Enter name(s) of layer(s) for lineweight 0.50 mm:"
    );
    h.enter("walls,doors");
    assert_eq!(last(&h), "2 layers set to lineweight 0.50 mm.");
    h.enter_all(&["LW", "default", "doors", "?"]);
    let log = h.log();
    assert_eq!(
        &log[log.len() - 2..],
        [
            "  \"walls\": colour 7, on, 0.50 mm",
            "  \"doors\": colour 7, on"
        ]
    );
    h.enter("");
    assert_eq!(layer(&h, "walls").lineweight, Lineweight::Hundredths(50));
    assert_eq!(layer(&h, "doors").lineweight, Lineweight::Default);
    assert_eq!(layer(&h, "0").lineweight, Lineweight::Default);
    h.undo();
    assert_eq!(
        layer(&h, "walls").lineweight,
        Lineweight::Default,
        "one step"
    );
}

#[test]
fn on_off_lock_and_unlock_switch_the_layers_named() {
    let mut h = with_layers();
    h.enter_all(&["LA", "OFF"]);
    assert_eq!(h.prompt(), "Enter name(s) of layer(s) to switch off:");
    h.enter("walls,doors");
    assert_eq!(last(&h), "2 layers switched off.");
    assert!(!layer(&h, "walls").on && !layer(&h, "doors").on);

    h.enter_all(&["ON", "walls"]);
    assert_eq!(last(&h), "1 layer switched on.");
    h.enter_all(&["L", "doors, 0"]);
    assert_eq!(last(&h), "2 layers locked.");
    h.enter_all(&["U", "0"]);
    assert_eq!(last(&h), "1 layer unlocked.");
    h.enter("");

    let state = |name: &str| (layer(&h, name).on, layer(&h, name).locked);
    assert_eq!(state("0"), (true, false));
    assert_eq!(state("walls"), (true, false));
    assert_eq!(state("doors"), (false, true));

    // The current layer cannot be switched off — and then nothing in the list is.
    h.enter_all(&["LA", "OFF", "walls,0"]);
    assert_eq!(
        last(&h),
        "Layer \"0\" is the current layer: it cannot be switched off or deleted."
    );
    h.enter("");
    assert!(layer(&h, "walls").on);

    // "O" alone is neither ON nor OFF.
    h.enter_all(&["LA", "O"]);
    assert_eq!(h.prompt(), OPTIONS);
    assert_eq!(last(&h), "An option keyword is required.");
    h.escape();
}

#[test]
fn switching_a_layer_off_hides_its_objects_from_the_pointer() {
    let mut h = with_layers();
    h.enter_all(&[
        "LA", "S", "walls", "", "L", "0,0", "100,0", "", "LA", "S", "0", "OFF", "walls", "",
    ]);
    h.click_world(Point2::new(50.0, 0.0));
    h.escape();
    assert!(h.session.selection().is_empty(), "nothing there to pick");

    h.enter_all(&["LA", "ON", "walls", ""]);
    h.click_world(Point2::new(50.0, 0.0));
    assert_eq!(h.session.selection().len(), 1);
}

#[test]
fn rename_keeps_the_layer_and_everything_on_it() {
    let mut h = with_layers();
    h.enter_all(&["LA", "S", "walls", "", "L", "0,0", "10,0", ""]);
    let walls = h.session.drawing().layers().find("walls").unwrap();

    h.enter_all(&["LA", "R"]);
    assert_eq!(h.prompt(), "Enter name of layer to rename:");
    h.enter("walls");
    assert_eq!(h.prompt(), "Enter new name:");
    h.enter("Outer walls");
    assert_eq!(last(&h), "Layer \"walls\" renamed \"Outer walls\".");
    h.enter("");
    assert_eq!(
        h.session.drawing().layers().find("outer walls"),
        Some(walls)
    );
    assert_eq!(
        h.properties(),
        vec![Properties::on(walls)],
        "the line did not change"
    );
    assert_eq!(current(&h), "Outer walls");

    let refusals = [
        (
            &["R", "0", "base"][..],
            "Layer \"0\" cannot be renamed or deleted.",
        ),
        (
            &["R", "doors", "OUTER WALLS"],
            "There is already a layer \"Outer walls\".",
        ),
        (&["R", "doors", "a/b"], "A layer name cannot contain \"/\"."),
        (&["R", "nowhere"], "There is no layer \"nowhere\"."),
    ];
    for (typed, message) in refusals {
        h.enter("LA");
        h.enter_all(typed);
        assert_eq!(last(&h), message, "{typed:?}");
        assert_eq!(h.prompt(), OPTIONS, "{typed:?}");
        h.enter("");
    }
    assert_eq!(layer_names(&h), ["0", "Outer walls", "doors"]);
}

#[test]
fn delete_removes_empty_layers_and_refuses_the_rest() {
    let mut h = with_layers();
    h.enter_all(&[
        "LA",
        "N",
        "spare,extra",
        "S",
        "walls",
        "",
        "L",
        "0,0",
        "10,0",
        "",
    ]);
    h.enter_all(&["LA", "S", "doors"]);

    let refusals = [
        ("0", "Layer \"0\" cannot be renamed or deleted."),
        ("walls", "Layer \"walls\" has objects on it."),
        (
            "doors",
            "Layer \"doors\" is the current layer: it cannot be switched off or deleted.",
        ),
        ("nowhere", "There is no layer \"nowhere\"."),
        ("spare,walls", "Layer \"walls\" has objects on it."),
    ];
    for (typed, message) in refusals {
        h.enter("D");
        assert_eq!(h.prompt(), "Enter name(s) of layer(s) to delete:");
        h.enter(typed);
        assert_eq!(last(&h), message, "{typed}");
    }
    assert_eq!(
        layer_names(&h).len(),
        5,
        "nothing was deleted, not even `spare`"
    );

    h.enter_all(&["D", "spare, extra"]);
    assert_eq!(last(&h), "2 layers deleted.");
    h.enter("");
    assert_eq!(layer_names(&h), ["0", "walls", "doors"]);
    h.undo();
    assert_eq!(layer_names(&h).len(), 5);
    assert_eq!(
        current(&h),
        "walls",
        "the whole command is one step: Set and Delete are undone together"
    );
}

#[test]
fn the_question_mark_lists_every_layer_with_its_state() {
    let mut h = with_layers();
    h.enter_all(&[
        "LA", "C", "1", "walls", "OFF", "doors", "L", "doors", "S", "walls", "?",
    ]);
    let log = h.log();
    assert_eq!(
        &log[log.len() - 3..],
        [
            "  \"0\": colour 7, on",
            "  \"walls\": colour 1, on, current",
            "  \"doors\": colour 7, off, locked",
        ]
    );
    assert_eq!(h.prompt(), OPTIONS);
}

#[test]
fn enter_at_a_name_prompt_goes_back_to_the_options_and_escape_cancels() {
    let mut h = with_layers();
    let revision = h.session.drawing().revision();
    for option in ["N", "S", "C", "LW", "ON", "OFF", "L", "U", "R", "D"] {
        h.enter_all(&["LA", option]);
        assert_ne!(h.prompt(), OPTIONS, "{option}");
        h.enter("");
        assert_eq!(h.prompt(), OPTIONS, "{option}");
        h.escape();
        assert_eq!(h.prompt(), "Command:");
    }
    assert_eq!(h.session.drawing().revision(), revision);
    assert!(h.session.drawing().layers().get(LayerId::FIRST).is_some());

    // A click at a name prompt is not a name.
    h.enter_all(&["LA", "N"]);
    h.click_world(Point2::new(5.0, 5.0));
    assert_eq!(h.prompt(), "Enter name(s) for new layer(s):");
}
