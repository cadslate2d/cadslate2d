//! VIEW through the session (#235): a view saved and restored — in its
//! space, which the session goes to — deleted and listed; saving and
//! deleting are undone, restoring is not, and ZOOM Previous comes back.

use cad_core::Point2;
use cad_document::LayoutId;

use crate::testing::Harness;

fn said(h: &Harness, message: &str) -> bool {
    h.log().iter().any(|line| line == message)
}

fn close(a: Point2, b: Point2) -> bool {
    a.distance_to(b) < 1e-9
}

#[test]
fn a_view_saved_is_restored_without_changing_the_drawing() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "1000,500", ""]);
    h.enter_all(&["Z", "W", "100,100", "300,250"]);
    let saved = h.session.view().clone();
    h.enter("V");
    assert_eq!(
        h.prompt(),
        "Enter an option or [?/Save/Restore/Delete] <Restore>:"
    );
    h.enter_all(&["S", "Detail A"]);
    assert!(said(&h, "View \"Detail A\" saved."));
    h.enter_all(&["Z", "E"]);
    let revision = h.session.drawing().revision();
    h.enter_all(&["VIEW", "", "detail a"]);
    assert!(said(&h, "View \"detail a\" restored."));
    assert!(close(h.session.view().center(), saved.center()));
    assert!((h.session.view().scale() - saved.scale()).abs() < 1e-9 * saved.scale());
    assert_eq!(
        h.session.drawing().revision(),
        revision,
        "a restore is no drawing change"
    );
    // ZOOM Previous comes back from it; U undoes the save, not the view.
    h.enter_all(&["Z", "P"]);
    assert!(!close(h.session.view().center(), saved.center()));
    h.enter_all(&["V", "?"]);
    assert!(said(&h, "  \"Detail A\" on Model"));
    h.undo();
    assert!(h.session.drawing().views().is_empty(), "the save undone");
}

#[test]
fn a_view_of_a_layout_goes_there_and_names_are_refused_as_they_should_be() {
    let mut h = Harness::new();
    h.enter_all(&["LAYOUT", "N", "Sheet"]);
    let sheet = h.session.space();
    assert_ne!(sheet, LayoutId::MODEL);
    h.enter_all(&["V", "S", "Title"]);
    h.enter_all(&["MODEL"]);
    assert_eq!(h.session.space(), LayoutId::MODEL);
    h.enter_all(&["V", "R", "Title"]);
    assert_eq!(h.session.space(), sheet, "restored on its layout");

    h.enter_all(&["V", "R", "Nothing"]);
    assert!(said(&h, "There is no view \"Nothing\"."));
    assert_eq!(h.prompt(), "Enter view name:");
    h.enter("");
    h.enter_all(&["V", "S", "a<b"]);
    assert_eq!(h.prompt(), "Enter view name to save:", "asked again");
    h.enter_all(&["", "V", "D", "title"]);
    assert!(said(&h, "View \"title\" deleted."));
    assert!(h.session.drawing().views().is_empty());
}
