//! PLOT (#187): the prompts and their defaults, the window, the scale,
//! the refusals, and the request to the host.

use cad_core::{Aabb, Point2, Vec2};

use crate::testing::Harness;
use crate::{Dirty, HostRequest, Paper, PlotArea, PlotScale, PlotSettings};

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn last(h: &Harness) -> String {
    h.log().last().cloned().unwrap_or_default()
}

#[test]
fn plot_asks_for_everything_in_order_and_asks_the_host_for_the_page() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "100,0", ""]);
    h.enter("PLOT");
    assert_eq!(
        h.prompt(),
        "Paper size or [A4/A3/A2/A1/A0/Letter/Tabloid] <A4>:"
    );
    h.enter("A3");
    assert_eq!(h.prompt(), "Plot area or [Extents/Window] <Extents>:");
    h.enter("W");
    assert_eq!(h.prompt(), "Specify first corner of the window:");
    h.enter("0,0");
    assert_eq!(h.prompt(), "Specify opposite corner:");
    h.enter("50,20");
    assert_eq!(h.prompt(), "Scale, as 1:N or [Fit] <Fit>:");
    h.enter("1:50");
    assert_eq!(h.prompt(), "Plot lineweights? [Yes/No] <Yes>:");
    h.enter("N");
    assert_eq!(
        h.prompt(),
        "Colour or monochrome? [Colour/Monochrome] <Colour>:"
    );
    let dirty = h.enter("M");
    assert!(dirty.contains(Dirty::HOST));
    assert_eq!(h.session.take_host_request(), Some(HostRequest::Plot));
    assert_eq!(
        last(&h),
        "Plotting the window of model space in black on A3, at 1:50."
    );
    assert_eq!(
        h.session.settings().plot,
        PlotSettings {
            paper: Paper::A3,
            scale: PlotScale::Ratio(50.0),
            area: PlotArea::Window(Aabb::from_points(p(0.0, 0.0), p(50.0, 20.0))),
            lineweights: false,
            monochrome: true,
        }
    );
    assert_eq!(h.prompt(), "Command:");

    // The last settings are the next defaults: Enter keeps each.
    h.enter("PDFOUT");
    assert_eq!(
        h.prompt(),
        "Paper size or [A4/A3/A2/A1/A0/Letter/Tabloid] <A3>:"
    );
    h.enter("");
    assert_eq!(h.prompt(), "Plot area or [Extents/Window] <Window>:");
    h.enter("E");
    assert_eq!(h.prompt(), "Scale, as 1:N or [Fit] <1:50>:");
    h.enter("");
    assert_eq!(h.prompt(), "Plot lineweights? [Yes/No] <No>:");
    h.enter("");
    assert_eq!(
        h.prompt(),
        "Colour or monochrome? [Colour/Monochrome] <Monochrome>:"
    );
    h.enter("");
    assert_eq!(
        last(&h),
        "Plotting the extents of model space in black on A3, at 1:50."
    );
    assert_eq!(h.session.settings().plot.area, PlotArea::Extents);
}

#[test]
fn the_defaults_plot_the_extents_to_fit_on_a4_in_colour() {
    let mut h = Harness::new();
    h.enter_all(&["PLOT", "", "", "", "", ""]);
    assert_eq!(h.session.settings().plot, PlotSettings::default());
    assert_eq!(
        last(&h),
        "Plotting the extents of model space on A4, to fit."
    );
    assert_eq!(h.session.take_host_request(), Some(HostRequest::Plot));
}

#[test]
fn scales_are_read_in_every_spelling_and_nonsense_is_refused() {
    let mut h = Harness::new();
    h.enter_all(&["PLOT", "", "E"]);
    h.enter("fifty");
    assert_eq!(last(&h), "A scale is 1:N — 1:50, 1:100 — or Fit.");
    assert_eq!(h.prompt(), "Scale, as 1:N or [Fit] <Fit>:");
    h.enter("1:0");
    assert_eq!(last(&h), "A scale is 1:N — 1:50, 1:100 — or Fit.");
    h.enter_all(&["2:1", "", ""]);
    assert_eq!(h.session.settings().plot.scale, PlotScale::Ratio(0.5));
    h.enter_all(&["PLOT", "", "E", "100", "", ""]);
    assert_eq!(h.session.settings().plot.scale, PlotScale::Ratio(100.0));
    h.enter_all(&["PLOT", "", "E", "1=20", "", ""]);
    assert_eq!(h.session.settings().plot.scale, PlotScale::Ratio(20.0));
    h.enter_all(&["PLOT", "", "E", "F", "", ""]);
    assert_eq!(h.session.settings().plot.scale, PlotScale::Fit);
}

#[test]
fn a_window_needs_two_different_corners_and_takes_no_object_snap() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "100,0", ""]);
    h.enter_all(&["PLOT", "", "W", "10,10", "10,10"]);
    assert_eq!(last(&h), "A window needs two different corners.");
    assert_eq!(h.prompt(), "Specify first corner of the window:");
    // A click near the line's end is the place clicked, not the endpoint.
    h.click_world(p(98.0, 3.0));
    h.click_world(p(-20.0, -20.0));
    assert_eq!(
        h.session.settings().plot.area,
        PlotArea::Extents,
        "not yet: the command is still running"
    );
    h.enter_all(&["", "", ""]);
    assert_eq!(
        h.session.settings().plot.area,
        PlotArea::Window(Aabb::from_points(p(-20.0, -20.0), p(98.0, 3.0)))
    );
}

/// #194: on a layout, PLOT offers the sheet as the paper — A3 for a
/// layout made here — and a paper named becomes the sheet, kept on the
/// row as one undoable change; the model's own paper is not touched.
#[test]
fn on_a_layout_the_paper_is_the_sheet_and_a_paper_named_becomes_it() {
    let mut h = Harness::new();
    h.enter_all(&["L", "0,0", "100,0", ""]);
    h.enter_all(&["LAYOUT", "N", "Sheet one"]);
    let sheet = h.session.space();
    h.enter("PLOT");
    assert_eq!(
        h.prompt(),
        "Paper size or [A4/A3/A2/A1/A0/Letter/Tabloid] <A3>:"
    );
    h.enter("A4");
    assert_eq!(h.prompt(), "Plot area or [Layout/Extents/Window] <Layout>:");
    h.enter("");
    assert_eq!(h.prompt(), "Scale, as 1:N or [Fit] <1:1>:");
    h.enter("");
    assert_eq!(h.prompt(), "Plot rotation in degrees <0>:");
    h.enter_all(&["", "", ""]);
    assert_eq!(h.session.take_host_request(), Some(HostRequest::Plot));
    assert_eq!(
        last(&h),
        "Plotting the sheet of layout \"Sheet one\" on A4, at 1:1."
    );
    let row = h.session.drawing().layouts().get(sheet).unwrap();
    assert_eq!(
        row.sheet_size(),
        Vec2::new(297.0, 210.0),
        "landscape, as the sheet was"
    );
    assert_eq!(
        h.session.settings().plot.paper,
        Paper::A4,
        "the default, untouched"
    );
    assert_eq!(h.prompt(), "Command:");

    // The sheet is the next default; Enter keeps it and changes nothing.
    let revision = h.session.drawing().revision();
    h.enter("PLOT");
    assert_eq!(
        h.prompt(),
        "Paper size or [A4/A3/A2/A1/A0/Letter/Tabloid] <A4>:"
    );
    h.enter_all(&["", "", "", "", "", "M"]);
    assert_eq!(h.session.drawing().revision(), revision);
    assert_eq!(
        last(&h),
        "Plotting the sheet of layout \"Sheet one\" in black on A4, at 1:1."
    );

    // One undo takes the sheet back; the model plots as before.
    h.undo();
    let row = h.session.drawing().layouts().get(sheet).unwrap();
    assert_eq!(row.sheet_size(), Vec2::new(420.0, 297.0));
    h.enter("MODEL");
    h.enter_all(&["PLOT", "", "", "", "", "C"]);
    assert_eq!(
        last(&h),
        "Plotting the extents of model space on A4, to fit."
    );
}

/// #208: on a layout, PLOT offers the layout's own area, scale, and
/// rotation, and a choice made keeps them on its row, undone as one.
#[test]
fn on_a_layout_the_area_scale_and_rotation_are_the_layouts_and_kept_on_it() {
    use cad_document::{LayoutArea, LayoutPlot};
    let mut h = Harness::new();
    h.enter_all(&["LAYOUT", "N", "Sheet", "L", "10,10", "100,50", ""]);
    let sheet = h.session.space();
    h.enter_all(&["PLOT", "", "W", "0,0", "200,100", "1:2", "45"]);
    assert_eq!(last(&h), "The rotation is 0, 90, 180, or 270 degrees.");
    h.enter_all(&["90", "", ""]);
    assert_eq!(
        last(&h),
        "Plotting the window of layout \"Sheet\" on A3, at 1:2, turned 90°."
    );
    let kept = h
        .session
        .drawing()
        .layouts()
        .get(sheet)
        .unwrap()
        .plot_settings();
    assert_eq!(
        kept,
        LayoutPlot {
            area: LayoutArea::Window(Aabb::from_points(p(0.0, 0.0), p(200.0, 100.0))),
            scale: Some(0.5),
            turns: 1,
        }
    );
    // The next PLOT offers them; Enter keeps them and changes nothing.
    let revision = h.session.drawing().revision();
    h.enter("PLOT");
    h.enter("");
    assert_eq!(h.prompt(), "Plot area or [Layout/Extents/Window] <Window>:");
    h.enter("");
    assert_eq!(h.prompt(), "Scale, as 1:N or [Fit] <1:2>:");
    h.enter("");
    assert_eq!(h.prompt(), "Plot rotation in degrees <90>:");
    h.enter_all(&["", "", ""]);
    assert_eq!(h.session.drawing().revision(), revision);
    // Extents to fit, back upright.
    h.enter_all(&["PLOT", "", "E", "F", "0", "", ""]);
    assert_eq!(
        last(&h),
        "Plotting the extents of layout \"Sheet\" on A3, to fit."
    );
    h.undo();
    assert_eq!(
        h.session
            .drawing()
            .layouts()
            .get(sheet)
            .unwrap()
            .plot_settings(),
        kept,
        "one undo"
    );
}
