//! LAYERSTATE through the session (#228): save, restore, delete, rename,
//! list, the refusals, and one undo each.

use crate::testing::Harness;

fn said(h: &Harness, message: &str) -> bool {
    h.log().iter().any(|line| line == message)
}

fn shown(h: &Harness, name: &str) -> bool {
    let layers = h.session.drawing().layers();
    layers.get(layers.find(name).unwrap()).unwrap().is_shown()
}

/// A plan and a ceiling on layers of their own.
fn layered() -> Harness {
    let mut h = Harness::new();
    h.enter_all(&["LAYER", "N", "PLAN", "N", "CEILING", ""]);
    h
}

#[test]
fn a_state_saved_is_restored_as_one_undo_and_leaves_other_layers_alone() {
    let mut h = layered();
    h.enter_all(&["LAYER", "OFF", "CEILING", ""]);
    h.enter("LAS");
    assert_eq!(
        h.prompt(),
        "Enter an option or [?/Save/Restore/Delete/REName] <Restore>:"
    );
    h.enter_all(&["S", "Plan view"]);
    assert!(said(&h, "Layer state \"Plan view\" saved: 3 layers."));

    // The other way round, and a layer made since.
    h.enter_all(&[
        "LAYER", "ON", "CEILING", "OFF", "PLAN", "N", "LATER", "OFF", "LATER", "",
    ]);
    h.enter_all(&["LAYERSTATE", "", "plan view"]);
    assert!(said(
        &h,
        "Layer state \"plan view\" restored: 2 layers changed."
    ));
    assert!(shown(&h, "PLAN"));
    assert!(!shown(&h, "CEILING"));
    assert!(!shown(&h, "LATER"), "not in the state: left as it was");
    h.undo();
    assert!(!shown(&h, "PLAN") && shown(&h, "CEILING"), "one undo");
    h.redo();

    h.enter_all(&["LAS", "?"]);
    assert!(said(&h, "  \"Plan view\": 3 layers"));
    h.enter_all(&["LAS", "REN", "Plan view", "Floor"]);
    assert!(said(&h, "Layer state \"Plan view\" renamed \"Floor\"."));
    h.enter_all(&["LAS", "D", "floor"]);
    assert!(said(&h, "Layer state \"floor\" deleted."));
    assert!(h.session.drawing().layer_states().is_empty());
    h.undo();
    assert_eq!(h.session.drawing().layer_states().len(), 1);
}

#[test]
fn saving_again_overwrites_and_names_are_refused_as_they_should_be() {
    let mut h = layered();
    h.enter_all(&["LAS", "S", "Plan"]);
    h.enter_all(&["LAYER", "OFF", "PLAN", ""]);
    h.enter_all(&["LAS", "S", "PLAN"]);
    assert_eq!(h.session.drawing().layer_states().len(), 1, "saved over");
    h.enter_all(&["LAYER", "ON", "PLAN", ""]);
    h.enter_all(&["LAS", "R", "Plan"]);
    assert!(!shown(&h, "PLAN"), "the second save is what is restored");

    h.enter_all(&["LAS", "R", "Nothing"]);
    assert!(said(&h, "There is no layer state \"Nothing\"."));
    assert_eq!(h.prompt(), "Enter name of layer state:");
    h.enter("");
    h.enter_all(&["LAS", "S", "a<b"]);
    assert_eq!(h.prompt(), "Enter new layer state name:", "asked again");
    h.enter_all(&["", "LAS", "S", "Ceiling", "LAS", "REN", "Ceiling", "plan"]);
    assert!(said(&h, "There is already a layer state of that name."));
    h.enter("");
    h.enter_all(&["LAS", "?"]);
    assert!(said(&h, "  \"Ceiling\": 3 layers"));
}
