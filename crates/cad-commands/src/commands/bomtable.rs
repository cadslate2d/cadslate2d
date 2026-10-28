//! BOMTABLE and BALLOON (ADR-0041 §5, #354). BOMTABLE takes the parts of a
//! bill of materials — number, name, quantity, material — one per line,
//! draws them as a table sized in paper millimetres, and places it on the
//! sheet as a view. BALLOON points at a part by its number: a leader from a
//! point on the part to a circle with the number, refused for a part no
//! bill on the sheet has; a balloon that would sit on another moves out
//! along its leader. Each one undo.

use cad_core::{EntityId, Point2};
use cad_document::{Entity, Group, LayerId, Properties};
use cad_geometry::composition::Role;
use cad_input::ExpectedInput;
use serde_json::json;

use crate::bom::{BALLOON, BALLOON_GROUP, BALLOON_LAYER, Part, TABLE_LAYER, balloon, table};
use crate::compose::{NewView, compose, parts_on, sheet_of, views_on};
use crate::{Command, CommandContext, CommandInput, Prompt, Step};

/// The layer `name`, made in `transaction` when the drawing has none.
fn layer(
    transaction: &mut cad_document::TransactionBuilder<'_>,
    found: Option<LayerId>,
    name: &str,
) -> Result<LayerId, String> {
    match found {
        Some(id) => Ok(id),
        None => transaction
            .add_layer(crate::compose::sheet_layer(name))
            .map_err(|e| e.to_string()),
    }
}

#[derive(Debug, PartialEq)]
enum TableState {
    Part,
    Title,
}

#[derive(Debug)]
pub(crate) struct BomTableCommand {
    state: TableState,
    parts: Vec<Part>,
}

impl BomTableCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: TableState::Part,
            parts: Vec::new(),
        }
    }

    fn draw(&self, title: String, ctx: &mut CommandContext<'_>) -> Step {
        let Some(sheet) = sheet_of(ctx.drawing) else {
            ctx.message("There is no sheet to place a bill of materials on: draw one with TITLEBLOCK first.");
            return Step::Finished;
        };
        let unit = sheet.frame.units_per_mm;
        let taken: Vec<String> = views_on(ctx.drawing, &sheet.group)
            .into_iter()
            .map(|v| v.id)
            .collect();
        let id = (1..)
            .map(|n| {
                if n == 1 {
                    "bom".to_owned()
                } else {
                    format!("bom-{n}")
                }
            })
            .find(|id| !taken.contains(id))
            .expect("a free id");
        let space = ctx.space;
        let view = NewView {
            id: id.clone(),
            title,
            role: Role::Free,
            entities: Vec::new(),
            origin: Point2::ORIGIN,
            ratio: None,
            cut: None,
            parts: self.parts.clone(),
            fresh: table(&self.parts, unit),
            fresh_layer: TABLE_LAYER,
        };
        match compose(ctx.drawing, space, &[view]) {
            Ok(_) => ctx.message(format!(
                "A bill of materials of {} placed on {} as {id}.",
                if self.parts.len() == 1 {
                    "1 part".to_owned()
                } else {
                    format!("{} parts", self.parts.len())
                },
                sheet.group
            )),
            // Nothing was drawn: the table is drawn only where it fits.
            Err(reason) => ctx.message(reason),
        }
        Step::Finished
    }
}

impl Command for BomTableCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        if sheet_of(ctx.drawing).is_none() {
            ctx.message("There is no sheet to place a bill of materials on: draw one with TITLEBLOCK first.");
            return Step::Finished;
        }
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            TableState::Part if self.parts.is_empty() => {
                Prompt::new("Enter a part: its number, name, quantity, and material")
            }
            TableState::Part => {
                Prompt::new("Enter the next part, or press Enter when all are given")
            }
            TableState::Title => {
                Prompt::new("Enter the table's title").with_default("BILL OF MATERIALS")
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        ExpectedInput::text()
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (TableState::Part, CommandInput::Text(text)) if !text.trim().is_empty() => {
                match Part::parse(&text) {
                    Ok(part) => {
                        let on_sheet = sheet_of(ctx.drawing)
                            .map(|s| parts_on(ctx.drawing, &s.group))
                            .unwrap_or_default();
                        if self
                            .parts
                            .iter()
                            .chain(&on_sheet)
                            .any(|p| p.number == part.number)
                        {
                            ctx.message(format!(
                                "Part {} is in a bill of materials already.",
                                part.number
                            ));
                        } else {
                            self.parts.push(part);
                        }
                    }
                    Err(reason) => ctx.message(reason),
                }
            }
            (TableState::Part, CommandInput::Accept | CommandInput::Text(_)) => {
                if self.parts.is_empty() {
                    ctx.message("No parts given.");
                    return Step::Finished;
                }
                self.state = TableState::Title;
            }
            (TableState::Title, CommandInput::Text(text)) if !text.trim().is_empty() => {
                return self.draw(text.trim().to_owned(), ctx);
            }
            (TableState::Title, _) => return self.draw("BILL OF MATERIALS".to_owned(), ctx),
            _ => {}
        }
        Step::Continue
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum BalloonState {
    Part,
    On(u32),
    At(u32, Point2),
}

#[derive(Debug)]
pub(crate) struct BalloonCommand {
    state: BalloonState,
}

impl BalloonCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: BalloonState::Part,
        }
    }

    fn draw(&self, number: u32, on: Point2, at: Point2, ctx: &mut CommandContext<'_>) {
        let Some(sheet) = sheet_of(ctx.drawing) else {
            return;
        };
        let unit = sheet.frame.units_per_mm;
        // Clear of the balloons already there: moved out along its leader.
        let others: Vec<Point2> = ctx
            .drawing
            .groups()
            .iter()
            .filter(|(_, g)| g.name.starts_with(BALLOON_GROUP))
            .filter_map(|(_, g)| {
                let v: serde_json::Value = serde_json::from_str(&g.description).ok()?;
                Some(Point2::new(v["at"][0].as_f64()?, v["at"][1].as_f64()?))
            })
            .collect();
        let clear = 2.0 * BALLOON * unit + unit;
        let away = at - on;
        let length = away.x.hypot(away.y);
        let step = if length > 0.0 {
            away * (clear / length)
        } else {
            cad_core::Vec2::new(clear, 0.0)
        };
        let mut at = at;
        for _ in 0..20 {
            if others.iter().all(|o| o.distance_to(at) >= clear) {
                break;
            }
            at = at + step;
        }
        let name = (1..)
            .map(|k| format!("{BALLOON_GROUP}{number}-{k}"))
            .find(|n| !ctx.drawing.groups().iter().any(|(_, g)| &g.name == n))
            .expect("a free name");
        let found = ctx.drawing.layers().find(BALLOON_LAYER);
        let mut transaction = ctx.transaction();
        let made: Result<Vec<EntityId>, String> = (|| {
            let on_layer = layer(&mut transaction, found, BALLOON_LAYER)?;
            balloon(number, on, at, unit)
                .into_iter()
                .map(|entity: Entity| {
                    transaction
                        .insert_with(
                            entity,
                            Properties {
                                layer: on_layer,
                                ..Properties::default()
                            },
                        )
                        .map_err(|e| e.to_string())
                })
                .collect()
        })();
        match made {
            Ok(members) => {
                let mut group = Group::named(name, members);
                group.description =
                    json!({"part": number, "at": [at.x, at.y], "sheet": sheet.group}).to_string();
                group.selectable = false;
                if transaction.add_group(group).is_ok() {
                    transaction.commit();
                    ctx.message(format!("Part {number} ballooned."));
                }
            }
            Err(reason) => ctx.message(reason),
        }
    }
}

impl Command for BalloonCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        let Some(sheet) = sheet_of(ctx.drawing) else {
            ctx.message("There is no sheet: draw one with TITLEBLOCK first.");
            return Step::Finished;
        };
        if parts_on(ctx.drawing, &sheet.group).is_empty() {
            ctx.message("The sheet has no bill of materials: make one with BOMTABLE first.");
            return Step::Finished;
        }
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            BalloonState::Part => Prompt::new("Enter the part's number, or press Enter to finish"),
            BalloonState::On(_) => Prompt::new("Specify the point on the part"),
            BalloonState::At(..) => Prompt::new("Specify where the balloon goes"),
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            BalloonState::Part => ExpectedInput::text(),
            _ => ExpectedInput::point(),
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (self.state, input) {
            (BalloonState::Part, CommandInput::Text(text)) if !text.trim().is_empty() => {
                let parts = sheet_of(ctx.drawing)
                    .map(|s| parts_on(ctx.drawing, &s.group))
                    .unwrap_or_default();
                match text.trim().parse::<u32>() {
                    Ok(n) if parts.iter().any(|p| p.number == n) => {
                        self.state = BalloonState::On(n)
                    }
                    _ => ctx.message(format!(
                        "No bill of materials on the sheet has part {}.",
                        text.trim()
                    )),
                }
            }
            (BalloonState::Part, _) => return Step::Finished,
            (BalloonState::On(n), CommandInput::Point(on)) => self.state = BalloonState::At(n, on),
            (BalloonState::At(n, on), CommandInput::Point(at)) => {
                self.draw(n, on, at, ctx);
                self.state = BalloonState::Part;
            }
            _ => {}
        }
        Step::Continue
    }
}
