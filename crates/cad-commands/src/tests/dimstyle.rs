//! DIMSTYLE (#162) through the session: every option, every refusal, one
//! undo each. What the table allows is tested in `cad-document`.

use cad_document::{DimStyle, DimStyleId};

use crate::testing::Harness;

const OPTIONS: &str = "Press Enter to finish or [?/New/Set/Rename/Delete/Height/Arrow/Gap/Offset/Extension/Precision/SUffix]:";

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

fn current(h: &Harness) -> DimStyle {
    let drawing = h.session.drawing();
    drawing
        .dim_styles()
        .get(drawing.current_dim_style())
        .unwrap()
        .clone()
}

fn names(h: &Harness) -> Vec<String> {
    h.session
        .drawing()
        .dim_styles()
        .iter()
        .map(|(_, style)| style.name.clone())
        .collect()
}

#[test]
fn the_command_reports_lists_and_loops_until_enter() {
    let mut h = Harness::new();
    h.enter("DIMSTYLE");
    assert_eq!(
        last(&h),
        "Current dimension style \"Standard\": text 2.5, arrows 2.5, gap 0.625, offset 0.625, overshoot 1.25, 2 decimals"
    );
    assert_eq!(h.prompt(), OPTIONS);
    h.enter("?");
    assert_eq!(
        last(&h),
        "  \"Standard\": text 2.5, arrows 2.5, gap 0.625, offset 0.625, overshoot 1.25, 2 decimals, current"
    );
    assert_eq!(h.prompt(), OPTIONS);
    h.enter("");
    assert_eq!(h.prompt(), "Command:");
    // The aliases.
    h.enter("D");
    assert_eq!(h.prompt(), OPTIONS);
    h.enter("");
}

#[test]
fn new_copies_the_current_style_and_sets_it_current_and_one_undo_reverts_both() {
    let mut h = Harness::new();
    h.enter_all(&["DIMSTYLE", "H", "5", "N"]);
    assert_eq!(h.prompt(), "Enter name for new dimension style:");
    h.enter("Arch 1");
    assert_eq!(
        last(&h),
        "Dimension style \"Arch 1\" made from \"Standard\" and set current."
    );
    assert_eq!(h.prompt(), OPTIONS);
    assert_eq!(names(&h), ["Standard", "Arch 1"]);
    assert_eq!(current(&h).name, "Arch 1");
    assert_eq!(
        current(&h).text_height,
        5.0,
        "the copy has the current values"
    );
    h.enter("");
    // One invocation of the command is one undo step, however many options
    // it ran: the new style and the height change go together.
    h.undo();
    assert_eq!(names(&h), ["Standard"]);
    assert_eq!(current(&h).name, "Standard");
    assert_eq!(current(&h).text_height, 2.5);
    h.redo();
    assert_eq!(names(&h), ["Standard", "Arch 1"]);
    assert_eq!(current(&h).text_height, 5.0);
}

#[test]
fn new_refuses_a_taken_or_bad_name_and_changes_nothing() {
    let mut h = Harness::new();
    h.enter_all(&["DIMSTYLE", "N", "standard"]);
    assert_eq!(last(&h), "There is already a dimension style \"Standard\".");
    assert_eq!(names(&h), ["Standard"]);
    h.enter_all(&["N", "a/b"]);
    assert_eq!(last(&h), "A dimension style name cannot contain \"/\".");
    h.enter_all(&["N", ""]);
    assert_eq!(h.prompt(), OPTIONS, "Enter at the name goes back");
    assert_eq!(names(&h), ["Standard"]);
}

#[test]
fn set_rename_and_delete_with_their_refusals() {
    let mut h = Harness::new();
    h.enter_all(&["DIMSTYLE", "N", "Arch", "N", "Civil", "S", "Arch"]);
    assert_eq!(last(&h), "Current dimension style: \"Arch\".");
    assert_eq!(current(&h).name, "Arch");
    h.enter_all(&["S", "Nope"]);
    assert_eq!(last(&h), "There is no dimension style \"Nope\".");

    h.enter_all(&["R", "civil"]);
    assert_eq!(h.prompt(), "Enter new name:");
    h.enter("Site");
    assert_eq!(last(&h), "Dimension style \"Civil\" renamed \"Site\".");
    assert_eq!(names(&h), ["Standard", "Arch", "Site"]);
    h.enter_all(&["R", "Site", "ARCH"]);
    assert_eq!(last(&h), "There is already a dimension style \"Arch\".");
    h.enter_all(&["R", "Standard", "Std"]);
    assert_eq!(
        last(&h),
        "Dimension style \"Standard\" cannot be deleted or renamed."
    );
    h.enter_all(&["R", "Nope"]);
    assert_eq!(last(&h), "There is no dimension style \"Nope\".");
    assert_eq!(h.prompt(), OPTIONS);

    // The current style cannot be deleted; Standard never; a list is all or nothing.
    h.enter_all(&["D", "Arch"]);
    assert_eq!(
        last(&h),
        "Dimension style \"Arch\" is current: set another first."
    );
    h.enter_all(&["D", "Standard"]);
    assert_eq!(
        last(&h),
        "Dimension style \"Standard\" cannot be deleted or renamed."
    );
    h.enter_all(&["D", "Site, Nope"]);
    assert_eq!(last(&h), "There is no dimension style \"Nope\".");
    assert_eq!(names(&h), ["Standard", "Arch", "Site"]);
    h.enter("");
    // A fresh invocation: its undo brings back exactly what it deleted.
    h.enter_all(&["DIMSTYLE", "S", "Standard", "D", "Arch, Site"]);
    assert_eq!(last(&h), "2 dimension styles deleted.");
    assert_eq!(names(&h), ["Standard"]);
    h.enter("");
    h.undo();
    assert_eq!(names(&h), ["Standard", "Arch", "Site"], "one undo");
    assert_eq!(current(&h).name, "Arch", "and the current style with them");
}

#[test]
fn the_variables_change_the_current_style_with_the_current_value_as_default() {
    let mut h = Harness::new();
    h.enter_all(&["UNITS", "MM", "DIMSTYLE", "H"]);
    assert_eq!(h.prompt(), "Enter text height <2.5>:");
    h.enter("3.5");
    assert_eq!(last(&h), "\"Standard\": text height 3.5.");
    assert_eq!(current(&h).text_height, 3.5);
    h.enter("A");
    assert_eq!(h.prompt(), "Enter arrow size <2.5>:");
    h.enter("2mm");
    assert_eq!(current(&h).arrow_size, 2.0, "a length takes a unit suffix");
    h.enter_all(&["G", "1", "O", "0", "E", "2"]);
    let style = current(&h);
    assert_eq!(
        (
            style.text_gap,
            style.extension_offset,
            style.extension_overshoot
        ),
        (1.0, 0.0, 2.0)
    );
    h.enter("P");
    assert_eq!(h.prompt(), "Enter number of decimals (0 to 8) <2>:");
    h.enter("3");
    assert_eq!(last(&h), "\"Standard\": 3 decimals.");
    h.enter("SU");
    assert_eq!(h.prompt(), "Enter text after the value (\".\" for none):");
    h.enter("mm");
    assert_eq!(last(&h), "\"Standard\": suffix \"mm\".");
    assert_eq!(current(&h).suffix, "mm");
    h.enter("SU");
    assert_eq!(
        h.prompt(),
        "Enter text after the value (\".\" for none) <mm>:"
    );
    h.enter(".");
    assert_eq!(current(&h).suffix, "");
    // Enter at a variable keeps it.
    h.enter_all(&["H", ""]);
    assert_eq!(h.prompt(), OPTIONS);
    assert_eq!(current(&h).text_height, 3.5);
    h.enter("");
    // The whole invocation is one undo step; the next invocation another.
    h.enter_all(&["DIMSTYLE", "SU", "in", ""]);
    assert_eq!(current(&h).suffix, "in");
    h.undo();
    assert_eq!(current(&h).suffix, "");
    assert_eq!(current(&h).text_height, 3.5);
}

#[test]
fn the_variables_refuse_what_a_dimension_cannot_be_drawn_by() {
    let mut h = Harness::new();
    h.enter_all(&["DIMSTYLE", "H", "0"]);
    assert_eq!(last(&h), "The text height must be greater than zero.");
    assert_eq!(current(&h).text_height, 2.5);
    h.enter_all(&["G", "-1"]);
    assert_eq!(last(&h), "The text gap cannot be negative.");
    h.enter_all(&["P", "9"]);
    assert_eq!(last(&h), "A dimension shows at most 8 decimals.");
    h.enter_all(&["P", "2.5"]);
    assert_eq!(last(&h), "A dimension shows at most 8 decimals.");
    assert_eq!(current(&h).decimals, 2);
    // A count is a plain number: a length is refused at the prompt.
    h.enter_all(&["P", "2mm"]);
    assert_eq!(h.prompt(), "Enter number of decimals (0 to 8) <2>:");
    h.enter("");
    assert_eq!(h.prompt(), OPTIONS);
}

#[test]
fn a_style_change_is_one_row_and_the_current_style_survives_undo() {
    let mut h = Harness::new();
    h.enter_all(&["DIMSTYLE", "N", "Arch", ""]);
    let arch = h.session.drawing().dim_styles().find("Arch").unwrap();
    assert_eq!(arch, DimStyleId::from_raw(1));
    assert_eq!(h.session.drawing().current_dim_style(), arch);
    let revision = h.session.drawing().revision();
    h.enter_all(&["DIMSTYLE", "H", "4", ""]);
    assert_ne!(h.session.drawing().revision(), revision);
    assert_eq!(current(&h).text_height, 4.0);
    assert_eq!(
        h.session
            .drawing()
            .dim_styles()
            .get(DimStyleId::FIRST)
            .unwrap()
            .text_height,
        2.5,
        "Standard is untouched"
    );
}
