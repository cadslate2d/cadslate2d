//! SHEETLAYOUT (#358): the sheet composed in the model, as a paper-space
//! layout too. A composed sheet is drawn in the model at its scale — its
//! frame and title block 1:N times their paper size — which prints at
//! scale only when the plot is set to 1:N by hand. This makes the layout
//! that holds it: named after the drawing's number, on the sheet's paper
//! and orientation, with one viewport over the whole paper showing the
//! sheet at exactly 1:N; the layout plots its whole sheet at one to one.
//! The model is left as it is. One undo; a sheet that has its layout
//! already is said so.

use cad_core::{Point2, Vec2};
use cad_document::{Entity, Layer, Layout, Properties, ViewportEntity};
use cad_input::{ExpectedInput, InputKind};

use crate::compose::sheet_of;
use crate::settings::Paper;
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

/// The layer a sheet's viewport is on.
pub const VIEWPORTS: &str = "VIEWPORTS";

#[derive(Debug)]
pub(crate) struct SheetLayoutCommand;

impl SheetLayoutCommand {
    pub(crate) fn new() -> Self {
        Self
    }
}

/// The layout of the drawing's sheet, made; what is said of it, or why not.
fn make(ctx: &mut CommandContext<'_>) -> Result<String, String> {
    let sheet = sheet_of(ctx.drawing)
        .ok_or("There is no sheet to lay out: compose one on a title block first.")?;
    let record: serde_json::Value = ctx
        .drawing
        .groups()
        .iter()
        .find(|(_, g)| g.name == sheet.group)
        .and_then(|(_, g)| serde_json::from_str(&g.description).ok())
        .ok_or("The sheet's title block keeps no record of its paper.")?;
    let paper = Paper::ALL
        .into_iter()
        .find(|p| Some(p.name()) == record["paper"].as_str())
        .ok_or("The sheet's paper is not one a layout has.")?;
    let landscape = record["landscape"].as_bool().unwrap_or(true);
    let at = Point2::new(
        record["at"][0].as_f64().unwrap_or(0.0),
        record["at"][1].as_f64().unwrap_or(0.0),
    );
    let per_mm = sheet.frame.units_per_mm;
    let (w, h) = crate::titleblock::sheet(paper, landscape);
    // Named after the drawing's number, or which sheet it is.
    let number = record["number"].as_str().unwrap_or("").trim().to_owned();
    let name = if number.is_empty() {
        let n = sheet
            .group
            .strip_prefix(crate::titleblock::GROUP)
            .unwrap_or("1");
        format!("Sheet {n}")
    } else {
        number
    };
    if ctx.drawing.layouts().find(&name).is_some() {
        return Err(format!("{name} is a layout already."));
    }
    let found = ctx.drawing.layers().find(VIEWPORTS);
    let mut transaction = ctx.drawing.transaction();
    let layout = transaction
        .add_layout(Layout::named(name.clone()))
        .map_err(|e| e.to_string())?;
    transaction
        .set_layout_sheet(layout, Vec2::new(w, h))
        .map_err(|e| e.to_string())?;
    let layer = match found {
        Some(id) => id,
        None => transaction
            .add_layer(Layer::named(VIEWPORTS))
            .map_err(|e| e.to_string())?,
    };
    // The whole paper, showing the sheet's middle at a paper millimetre
    // to every `per_mm` drawing units: 1:N.
    let viewport = ViewportEntity::new(
        Point2::new(w / 2.0, h / 2.0),
        w,
        h,
        at + Vec2::new(w * per_mm / 2.0, h * per_mm / 2.0),
        1.0 / per_mm,
    );
    transaction
        .insert_with_in(
            layout,
            Entity::Viewport(viewport),
            Properties {
                layer,
                ..Properties::default()
            },
        )
        .map_err(|e| e.to_string())?;
    transaction.commit();
    Ok(format!(
        "Layout {name}: {} {}, the sheet through one viewport at {}.",
        paper.name(),
        if landscape { "landscape" } else { "portrait" },
        crate::compose::ratio_text(1.0 / per_mm)
    ))
}

impl Command for SheetLayoutCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        match make(ctx) {
            Ok(said) | Err(said) => ctx.message(said),
        }
        Step::Finished
    }

    fn prompt(&self) -> Prompt {
        Prompt::new("Laying out the sheet")
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::new(InputKind::Nothing)
    }

    fn step(&mut self, _input: CommandInput, _ctx: &mut CommandContext<'_>) -> Step {
        Step::Finished
    }
}
