//! The layer tools (#213) through the session: each tool by picking, the
//! refusals the layer table makes, LAYISO's two settings and LAYUNISO
//! putting back only what is as LAYISO left it, one undo per run.

use cad_core::Point2;
use cad_document::Layer;

use crate::testing::Harness;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn layer<'a>(h: &'a Harness, name: &str) -> &'a Layer {
    let layers = h.session.drawing().layers();
    layers
        .get(
            layers
                .find(name)
                .unwrap_or_else(|| panic!("no layer {name}")),
        )
        .unwrap()
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

fn said(h: &Harness, message: &str) -> bool {
    h.log().iter().any(|line| line == message)
}

/// Layers `0`, `walls`, `doors`, and `notes`, a line on each at y = 0,
/// 20, 40, 60; `0` current.
fn drawing() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["LA", "N", "walls,doors,notes", ""]);
    for (name, y) in [("0", 0), ("walls", 20), ("doors", 40), ("notes", 60)] {
        h.enter_all(&["LA", "S", name, ""]);
        h.enter_all(&["L", &format!("0,{y}"), &format!("100,{y}"), ""]);
    }
    h.enter_all(&["LA", "S", "0", ""]);
    assert_eq!(h.prompt(), "Command:");
    h
}

/// Each named layer's `on`.
fn on(h: &Harness, names: &[&str]) -> Vec<bool> {
    names.iter().map(|name| layer(h, name).on).collect()
}

#[test]
fn layiso_switches_off_the_other_layers_and_layuniso_puts_them_back() {
    let mut h = drawing();
    h.enter("LAYISO");
    assert_eq!(
        h.prompt(),
        "Select objects on the layer(s) to be isolated or [Settings/Fence/WPolygon/CPolygon/Previous/Last/ALL/Remove]:"
    );
    h.click_world(p(50.0, 20.0));
    h.click_world(p(50.0, 40.0));
    h.enter("");
    assert!(said(&h, "2 layers are isolated: \"walls\", \"doors\"."));
    assert_eq!(
        on(&h, &["0", "walls", "doors", "notes"]),
        [false, true, true, false]
    );
    assert_eq!(current(&h), "walls", "the current layer was not kept");
    assert!(h.session.selection().is_empty());

    h.enter("LAYUNISO");
    assert!(said(&h, "2 layers restored."));
    assert_eq!(on(&h, &["0", "walls", "doors", "notes"]), [true; 4]);
    assert_eq!(current(&h), "walls", "the current layer stays");
    h.enter("LAYUNISO");
    assert!(said(&h, "LAYISO has isolated no layers to restore."));
}

#[test]
fn layuniso_keeps_a_change_made_since_and_layiso_takes_a_selection_first() {
    let mut h = drawing();
    // Noun first: the selection is isolated at once.
    h.click_world(p(50.0, 0.0));
    h.enter("LAYISO");
    assert!(said(&h, "Layer \"0\" is isolated."));
    assert_eq!(h.prompt(), "Command:");
    // `notes` switched on and off again by hand: LAYUNISO leaves it.
    h.enter_all(&["LA", "ON", "notes", "", "LA", "Lock", "notes", ""]);
    h.enter_all(&["LAYOFF"]);
    h.click_world(p(50.0, 60.0));
    assert!(said(&h, "That object is on the locked layer \"notes\"."));
    h.escape();
    h.enter("LAYUNISO");
    assert!(said(&h, "2 layers restored."));
    assert_eq!(on(&h, &["walls", "doors", "notes"]), [true; 3]);
    assert!(layer(&h, "notes").locked, "its own change kept");

    // Undone, nothing is as LAYISO left it.
    h.click_world(p(50.0, 0.0));
    h.enter("LAYISO");
    h.undo();
    h.enter("LAYUNISO");
    assert!(said(
        &h,
        "No layer is as LAYISO left it: nothing to restore."
    ));
}

#[test]
fn layiso_can_lock_the_other_layers_instead() {
    let mut h = drawing();
    h.enter_all(&["LAYISO", "S"]);
    assert_eq!(
        h.prompt(),
        "Enter setting for layers not isolated or [Off/Lock] <Off>:"
    );
    h.enter("L");
    h.click_world(p(50.0, 20.0));
    h.enter("");
    let locked = |h: &Harness| -> Vec<bool> {
        ["0", "walls", "doors", "notes"]
            .iter()
            .map(|name| layer(h, name).locked)
            .collect()
    };
    assert_eq!(locked(&h), [true, false, true, true]);
    assert_eq!(on(&h, &["0", "walls", "doors", "notes"]), [true; 4]);
    h.enter("LAYUNISO");
    assert_eq!(locked(&h), [false; 4]);
}

#[test]
fn layoff_switches_off_each_picked_layer_until_enter_as_one_undo() {
    let mut h = drawing();
    h.enter("LAYOFF");
    assert_eq!(h.prompt(), "Select an object on the layer to switch off:");
    h.click_world(p(50.0, 20.0));
    assert!(said(&h, "Layer \"walls\" is off."));
    h.click_world(p(50.0, 40.0));
    // The current layer is refused.
    h.click_world(p(50.0, 0.0));
    assert!(said(
        &h,
        "Layer \"0\" is the current layer: it cannot be switched off, frozen, or deleted."
    ));
    h.enter("");
    assert_eq!(
        on(&h, &["0", "walls", "doors", "notes"]),
        [true, false, false, true]
    );
    h.undo();
    assert_eq!(on(&h, &["walls", "doors"]), [true, true], "one undo");
    h.redo();

    h.enter("LAYON");
    assert!(said(&h, "All layers are on."));
    assert_eq!(on(&h, &["0", "walls", "doors", "notes"]), [true; 4]);
}

#[test]
fn layfrz_freezes_and_laythw_thaws_without_touching_on_and_off() {
    let mut h = drawing();
    h.enter_all(&["LA", "OFF", "notes", ""]);
    h.enter("LAYFRZ");
    assert_eq!(h.prompt(), "Select an object on the layer to freeze:");
    h.click_world(p(50.0, 20.0));
    h.enter("");
    assert!(said(&h, "Layer \"walls\" is frozen."));
    assert!(layer(&h, "walls").frozen && layer(&h, "walls").on);
    h.click_world(p(50.0, 20.0));
    assert!(h.session.selection().is_empty(), "nothing there to pick");
    h.enter_all(&["LA", "F", "doors", ""]);
    h.enter("LAYTHW");
    assert!(said(&h, "All layers are thawed."));
    assert!(!layer(&h, "walls").frozen && !layer(&h, "doors").frozen);
    assert!(!layer(&h, "notes").on, "thawing switches nothing on");
}

#[test]
fn laylck_and_layulk_lock_and_unlock_and_laymcur_makes_a_layer_current() {
    let mut h = drawing();
    h.enter("LAYLCK");
    assert_eq!(h.prompt(), "Select an object on the layer to lock:");
    h.click_world(p(50.0, 20.0));
    assert!(said(&h, "Layer \"walls\" is locked."));
    assert_eq!(h.prompt(), "Command:", "one pick");
    assert!(layer(&h, "walls").locked);

    // An object on a locked layer is what LAYULK picks.
    h.enter("LAYULK");
    h.click_world(p(50.0, 20.0));
    assert!(said(&h, "Layer \"walls\" is unlocked."));
    assert!(!layer(&h, "walls").locked);

    h.enter("LAYMCUR");
    assert_eq!(
        h.prompt(),
        "Select an object whose layer will become current:"
    );
    h.click_world(p(50.0, 40.0));
    assert!(said(&h, "Layer \"doors\" is current."));
    assert_eq!(current(&h), "doors");
    h.undo();
    assert_eq!(current(&h), "0");
}
