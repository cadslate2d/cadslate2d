//! LINETYPE, LTSCALE, and LAYER's LType option (#138) through the session.
//! What the linetype table allows is tested in `cad-document`; how a
//! pattern reaches the screen, in `cad-render` and the browser tests.

use cad_core::Point2;
use cad_document::{Linetype, LinetypeId};

use crate::testing::Harness;

const OPTIONS: &str = "Press Enter to finish or [?/Load/Set/Delete]:";
const LOAD: &str =
    "Enter name(s) of linetype(s) to load (Dashed, Hidden, Center, Phantom, Dot, Dashdot):";

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn names(h: &Harness) -> Vec<String> {
    h.session
        .drawing()
        .linetypes()
        .iter()
        .map(|(_, def)| def.name.clone())
        .collect()
}

fn linetypes(h: &Harness) -> Vec<Linetype> {
    h.properties().into_iter().map(|p| p.linetype).collect()
}

fn current(h: &Harness) -> Linetype {
    h.session.drawing().current_linetype()
}

#[test]
fn linetypes_are_loaded_by_name_listed_and_the_command_loops_until_enter() {
    let mut h = Harness::new();
    h.enter("LINETYPE");
    assert_eq!(last(&h), "Current linetype: \"ByLayer\"");
    assert_eq!(h.prompt(), OPTIONS);
    h.enter("L");
    assert_eq!(h.prompt(), LOAD);
    h.enter("dashed, HIDDEN");
    assert_eq!(last(&h), "2 linetypes loaded.");
    assert_eq!(names(&h), vec!["Continuous", "Dashed", "Hidden"]);
    assert_eq!(h.prompt(), OPTIONS);

    h.enter_all(&["Load", "Dashed"]);
    assert_eq!(last(&h), "Linetype \"Dashed\" is already loaded.");
    h.enter_all(&["Load", "Center,Zigzag"]);
    assert_eq!(last(&h), "There is no built-in linetype \"Zigzag\".");
    assert_eq!(names(&h).len(), 3, "nothing half done");

    h.enter("?");
    let log = h.log();
    assert_eq!(
        &log[log.len() - 3..],
        [
            "  \"Continuous\": Solid line",
            "  \"Dashed\": __ __ __ __ __ __ __",
            "  \"Hidden\": _ _ _ _ _ _ _ _ _ _",
        ]
    );
    h.enter("");
    assert_eq!(h.prompt(), "Command:");

    // One command, however many options, is one undo step.
    h.undo();
    assert_eq!(names(&h), vec!["Continuous"]);
    h.redo();
    assert_eq!(names(&h).len(), 3);
}

#[test]
fn new_objects_take_the_current_linetype_and_copies_keep_theirs() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "10,0", ""]);
    h.enter_all(&["LT", "S", "Dashed"]);
    assert_eq!(
        last(&h),
        "Linetype \"Dashed\" is not loaded: load it with LINETYPE first."
    );
    h.enter_all(&["S", "wavy"]);
    assert_eq!(last(&h), "There is no linetype \"wavy\".");
    h.enter_all(&["L", "Dashed", "S", "dashed"]);
    assert_eq!(last(&h), "Current linetype: \"Dashed\"");
    let dashed = Linetype::Named(LinetypeId::from_raw(1));
    assert_eq!(current(&h), dashed);
    h.enter("?");
    assert_eq!(last(&h), "  \"Dashed\": __ __ __ __ __ __ __, current");
    h.enter("");

    h.enter_all(&["L", "0,5", "10,5", "", "C", "50,50", "5"]);
    assert_eq!(linetypes(&h), vec![Linetype::ByLayer, dashed, dashed]);

    // Continuous by name is not ByLayer.
    h.enter_all(&["LTYPE", "Set", "continuous", ""]);
    assert_eq!(current(&h), Linetype::CONTINUOUS);
    h.enter_all(&["LT", "S", "ByBlock", ""]);
    assert_eq!(current(&h), Linetype::ByBlock);
    h.enter_all(&["LT", "S", "bylayer", ""]);
    assert_eq!(current(&h), Linetype::ByLayer);

    // An offset of a dashed line is dashed, whatever is current.
    h.enter_all(&["O", "2"]);
    h.click_world(Point2::new(5.0, 5.0));
    h.click_world(Point2::new(5.0, 20.0));
    h.enter("");
    assert_eq!(linetypes(&h).last(), Some(&dashed));
}

#[test]
fn a_linetype_in_use_is_not_deleted_and_continuous_never_is() {
    let mut h = Harness::new();
    h.enter_all(&["LT", "L", "Dashed,Hidden,Center", ""]);
    // An object with Dashed, a layer with Hidden, and Center for new objects.
    h.enter_all(&[
        "LT", "S", "Dashed", "", "L", "0,0", "10,0", "", "LT", "S", "Center", "",
    ]);
    h.enter_all(&["LA", "N", "walls", "LT", "Hidden", "walls"]);
    assert_eq!(last(&h), "1 layer set to linetype Hidden.");
    h.enter("");

    h.enter_all(&["LT", "D", "Continuous"]);
    assert_eq!(last(&h), "Linetype \"Continuous\" cannot be deleted.");
    h.enter_all(&["D", "Center"]);
    assert_eq!(
        last(&h),
        "Linetype \"Center\" is in use: by an object, a layer, or as the linetype for new objects."
    );
    h.enter_all(&["D", "Hidden"]);
    assert!(last(&h).starts_with("Linetype \"Hidden\" is in use"));
    h.enter_all(&["D", "Dashed"]);
    assert!(last(&h).starts_with("Linetype \"Dashed\" is in use"));
    h.enter_all(&["D", "Zigzag"]);
    assert_eq!(last(&h), "There is no linetype \"Zigzag\".");
    assert_eq!(names(&h).len(), 4);
    h.enter("");

    // Erase the object, set ByLayer, and the layer back to continuous, and
    // all three can go — but all or none.
    h.enter("E");
    h.click_world(Point2::new(5.0, 0.0));
    h.enter("");
    h.enter_all(&["LT", "S", "ByLayer", "D", "Dashed,Hidden"]);
    assert!(last(&h).starts_with("Linetype \"Hidden\" is in use"));
    assert_eq!(names(&h).len(), 4, "nothing half done");
    h.enter("");
    h.enter_all(&["LA", "LT", "Continuous", "walls", ""]);
    h.enter_all(&["LT", "D", "Dashed,Center,Hidden"]);
    assert_eq!(last(&h), "3 linetypes deleted.");
    h.enter("");
    assert_eq!(names(&h), vec!["Continuous"]);
    h.undo();
    assert_eq!(names(&h), vec!["Continuous", "Dashed", "Hidden", "Center"]);
}

#[test]
fn a_layer_is_given_a_linetype_of_its_own_a_built_in_one_loaded_with_it() {
    let mut h = Harness::new();
    h.enter_all(&["LA", "N", "walls", "LT", "ByLayer"]);
    assert_eq!(
        last(&h),
        "A layer needs a linetype of its own, not ByLayer or ByBlock."
    );
    // Asked again, until a linetype or Enter.
    assert_eq!(h.prompt(), "Enter linetype (a loaded name):");
    h.enter("nothing");
    assert_eq!(last(&h), "There is no linetype \"nothing\".");
    // A built-in linetype not loaded yet is loaded with it (ADR-0041 §4).
    h.enter("dashed");
    assert_eq!(h.prompt(), "Enter name(s) of layer(s) for that linetype:");
    h.enter("walls,0");
    assert_eq!(last(&h), "2 layers set to linetype Dashed.");
    h.enter("?");
    let log = h.log();
    assert_eq!(
        &log[log.len() - 2..],
        [
            "  \"0\": colour 7, on, Dashed, current",
            "  \"walls\": colour 7, on, Dashed",
        ]
    );
    h.enter("");
    let dashed = Linetype::Named(LinetypeId::from_raw(1));
    let layers = h.session.drawing().layers();
    assert!(layers.iter().all(|(_, layer)| layer.linetype == dashed));
    // One undo takes the layer, its linetype, and the linetype loaded.
    h.undo();
    assert_eq!(h.session.drawing().linetypes().len(), 1);
}

#[test]
fn ltscale_is_a_positive_factor_with_a_default_and_an_undo() {
    let mut h = Harness::new();
    h.enter("LTSCALE");
    assert_eq!(h.prompt(), "Enter new linetype scale factor <1>:");
    h.enter("0");
    assert_eq!(last(&h), "The linetype scale must be a positive number.");
    assert_eq!(h.prompt(), "Enter new linetype scale factor <1>:");
    h.enter("-2");
    assert_eq!(last(&h), "The linetype scale must be a positive number.");
    h.enter("2.5");
    assert_eq!(last(&h), "Linetype scale: 2.5");
    assert_eq!(h.session.drawing().ltscale(), 2.5);
    assert_eq!(h.prompt(), "Command:");

    h.enter("LTS");
    assert_eq!(h.prompt(), "Enter new linetype scale factor <2.5>:");
    h.enter("");
    assert_eq!(h.session.drawing().ltscale(), 2.5, "Enter keeps the scale");
    h.undo();
    assert_eq!(h.session.drawing().ltscale(), 1.0);
    h.redo();
    assert_eq!(h.session.drawing().ltscale(), 2.5);
}
