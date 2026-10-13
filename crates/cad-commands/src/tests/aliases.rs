//! User aliases and settings (#186): ALIASEDIT through the editor, the
//! aliases in use, refusals by line, SETTINGS as a host request, and what
//! the host gives back on start.

use cad_core::LengthUnit;

use crate::testing::Harness;
use crate::{Dirty, HostRequest, Toggle, Toggles};

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

#[test]
fn aliasedit_opens_the_editor_on_the_aliases_and_takes_them_back() {
    let mut h = Harness::new();
    h.enter("ALIASEDIT");
    assert_eq!(
        h.prompt(),
        "Edit the aliases in the editor, one per line as alias, command:"
    );
    assert_eq!(h.session.ui_state().text_editor, Some(String::new()));
    h.enter("LL, LAYER\nCC, circle\n");
    assert_eq!(last(&h), "2 aliases defined.");
    assert_eq!(h.session.ui_state().text_editor, None);
    assert_eq!(h.session.ui_state().user_aliases, "CC, CIRCLE\nLL, LAYER\n");

    // The alias is a command now, and the editor opens on it next time.
    h.enter("ll");
    assert!(
        h.prompt()
            .starts_with("Press Enter to finish or [?/New/Set"),
        "{}",
        h.prompt()
    );
    h.escape();
    h.enter("ALIASEDIT");
    assert_eq!(
        h.session.ui_state().text_editor,
        Some("CC, CIRCLE\nLL, LAYER\n".to_owned())
    );
    h.enter("");
    assert_eq!(h.session.ui_state().user_aliases, "CC, CIRCLE\nLL, LAYER\n");
}

#[test]
fn a_conflict_with_a_built_in_name_is_refused_by_name_and_the_rest_stand() {
    let mut h = Harness::new();
    h.enter("ALIASEDIT");
    h.enter("E, LINE\nERASE, LINE\nLL, LAYER\nQQ, NOPE");
    let log = h.log();
    let tail = &log[log.len() - 4..];
    assert_eq!(
        tail,
        [
            "Line 1: \"E\" is a built-in command name or alias.",
            "Line 2: \"ERASE\" is a built-in command name or alias.",
            "Line 4: there is no command \"NOPE\".",
            "1 alias defined.",
        ]
    );
    h.enter_all(&["L", "0,0", "10,0", ""]);
    h.enter("E");
    assert!(h.prompt().starts_with("Select objects"), "E still erases");
}

#[test]
fn settings_asks_the_host_for_its_panel() {
    let mut h = Harness::new();
    let dirty = h.enter("SETTINGS");
    assert!(dirty.contains(Dirty::HOST));
    assert_eq!(h.session.take_host_request(), Some(HostRequest::Settings));
    assert_eq!(h.prompt(), "Command:");
}

#[test]
fn what_the_host_kept_is_given_back_on_start() {
    let mut h = Harness::new();
    let dirty = h.session.restore_settings(
        Toggles::only(&[Toggle::Ortho, Toggle::Lineweight]),
        Some(LengthUnit::Inches),
        "LL, LAYER\nE, LINE\n",
    );
    assert!(dirty.contains(Dirty::UI));
    let state = h.session.ui_state();
    assert_eq!(
        state.toggles,
        Toggles::only(&[Toggle::Ortho, Toggle::Lineweight]).bits()
    );
    assert_eq!(state.unit, Some("in"));
    assert_eq!(state.user_aliases, "LL, LAYER\n");
    assert_eq!(
        last(&h),
        "Line 2: \"E\" is a built-in command name or alias."
    );
    // The unit was set without a transaction: nothing to undo, nothing unsaved.
    assert!(!state.can_undo);
    assert!(!state.unsaved_changes);
    // A typed length converts in the restored unit.
    h.enter_all(&["L", "0,0", "254mm,0", ""]);
    assert_eq!(h.lines()[0].end.x, 10.0);
}

#[test]
fn a_drawing_with_content_keeps_its_own_unit() {
    let mut h = Harness::new();
    h.enter_all(&["UN", "MM"]);
    h.session
        .restore_settings(Toggles::default(), Some(LengthUnit::Feet), "");
    assert_eq!(h.session.ui_state().unit, Some("mm"));
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "10,0", ""]);
    h.session
        .restore_settings(Toggles::default(), Some(LengthUnit::Feet), "");
    assert_eq!(h.session.ui_state().unit, None);
}

#[test]
fn toggle_bits_from_a_host_are_taken_as_they_are_but_rivals_do_not_both_stay() {
    let bits = Toggle::Ortho.bit() | Toggle::Polar.bit() | Toggle::Grid.bit() | 0xff00;
    let toggles = Toggles::from_bits(bits);
    assert!(toggles.is_on(Toggle::Ortho) && !toggles.is_on(Toggle::Polar));
    assert!(toggles.is_on(Toggle::Grid) && !toggles.is_on(Toggle::Osnap));
    assert_eq!(toggles.bits() & 0xff00, 0);
}
