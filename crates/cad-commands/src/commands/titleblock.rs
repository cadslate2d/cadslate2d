//! TITLEBLOCK (#340, ADR-0040): a sheet's border and title block. The
//! paper and its orientation; in the model, the scale the sheet is drawn
//! at; the title, the drawing's number, who drew it, the date, the sheet,
//! and the projection; and where the sheet's lower left corner goes. On a
//! layout it is drawn in paper millimetres; in the model at the scale, in
//! the drawing's unit. One undo, on `TITLE`, in a group `TITLE-n` holding
//! its fields (`crate::titleblock`).

use cad_core::Point2;
use cad_document::LayoutId;
use cad_input::{ExpectedInput, InputKind, Keyword};

use super::ltscale::display;
use crate::settings::Paper;
use crate::titleblock::{Fields, GROUP, LAYER, description, entities};
use crate::views::draw_group;
use crate::{Command, CommandContext, CommandInput, OverlayItem, Prompt, Step};

const A4: Keyword = Keyword::new("A4");
const A3: Keyword = Keyword::new("A3");
const A2: Keyword = Keyword::new("A2");
const A1: Keyword = Keyword::new("A1");
const A0: Keyword = Keyword::new("A0");
const LANDSCAPE: Keyword = Keyword::new("Landscape");
const PORTRAIT: Keyword = Keyword::new("Portrait");
const FIRST: Keyword = Keyword::new("First");
const THIRD: Keyword = Keyword::new("Third");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Paper,
    Orientation,
    Scale,
    Title,
    Number,
    DrawnBy,
    Date,
    Sheet,
    Projection,
    Corner,
}

#[derive(Debug)]
pub(crate) struct TitleBlockCommand {
    state: State,
    paper: Paper,
    landscape: bool,
    /// 1:N.
    ratio: f64,
    fields: Fields,
    /// On a layout's paper: drawn in millimetres, at 1.
    on_paper: bool,
    /// Drawing units to the millimetre at 1:1.
    unit_per_mm: f64,
}

impl TitleBlockCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Paper,
            paper: Paper::A3,
            landscape: true,
            ratio: 1.0,
            fields: Fields {
                sheet: "1/1".into(),
                first_angle: true,
                ..Fields::default()
            },
            on_paper: false,
            unit_per_mm: 1.0,
        }
    }

    /// Drawing units to a millimetre of paper.
    fn scale(&self) -> f64 {
        if self.on_paper {
            1.0
        } else {
            self.ratio * self.unit_per_mm
        }
    }

    fn fields(&self) -> Fields {
        Fields {
            scale: format!("1:{}", display(self.ratio)),
            ..self.fields.clone()
        }
    }

    fn finish(&self, at: Point2, ctx: &mut CommandContext<'_>) -> Step {
        let fields = self.fields();
        let made = entities(self.paper, self.landscape, &fields, at, self.scale())
            .into_iter()
            .map(|e| (0, e))
            .collect();
        let space = ctx.space;
        match draw_group(
            ctx.drawing,
            space,
            &[LAYER],
            made,
            GROUP,
            description(self.paper, self.landscape, &fields, at, self.scale()),
        ) {
            Ok((_, name)) => ctx.message(format!(
                "{} {} sheet drawn as {name}, at {}.",
                self.paper.name(),
                if self.landscape {
                    "landscape"
                } else {
                    "portrait"
                },
                fields.scale
            )),
            Err(error) => ctx.message(error.to_string()),
        }
        Step::Finished
    }
}

impl Command for TitleBlockCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.on_paper = ctx.space != LayoutId::MODEL;
        // A unitless drawing is taken in millimetres.
        self.unit_per_mm = ctx.drawing.unit().map_or(1.0, |unit| 0.001 / unit.metres());
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        let text = |label: &str, value: &str| {
            let prompt = Prompt::new(label);
            if value.is_empty() {
                prompt
            } else {
                prompt.with_default(value)
            }
        };
        match self.state {
            State::Paper => Prompt::new("Paper size?").with_default(self.paper.name()),
            State::Orientation => Prompt::new("Orientation?").with_default("Landscape"),
            State::Scale => Prompt::new("Specify the scale, 1:N").with_default(display(self.ratio)),
            State::Title => text("Enter the title", &self.fields.title),
            State::Number => text("Enter the drawing's number", &self.fields.number),
            State::DrawnBy => text("Enter who drew it", &self.fields.drawn_by),
            State::Date => text("Enter the date", &self.fields.date),
            State::Sheet => text("Enter the sheet", &self.fields.sheet),
            State::Projection => Prompt::new("Which projection?").with_default("First"),
            State::Corner => {
                Prompt::new("Specify the sheet's lower left corner").with_default("0,0")
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Paper => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&[A4, A3, A2, A1, A0])
            }
            State::Orientation => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&[LANDSCAPE, PORTRAIT])
            }
            State::Scale => ExpectedInput::factor(),
            State::Title | State::Number | State::DrawnBy | State::Date | State::Sheet => {
                ExpectedInput::text()
            }
            State::Projection => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&[FIRST, THIRD])
            }
            State::Corner => ExpectedInput::point(),
        }
    }

    fn preview(&self, cursor: Point2, out: &mut Vec<OverlayItem>) {
        if self.state == State::Corner {
            for entity in entities(
                self.paper,
                self.landscape,
                &self.fields(),
                cursor,
                self.scale(),
            ) {
                out.push(OverlayItem::PreviewEntity(entity));
            }
        }
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        let next = |state: State, on_paper: bool| match state {
            State::Paper => State::Orientation,
            State::Orientation if on_paper => State::Title,
            State::Orientation => State::Scale,
            State::Scale => State::Title,
            State::Title => State::Number,
            State::Number => State::DrawnBy,
            State::DrawnBy => State::Date,
            State::Date => State::Sheet,
            State::Sheet => State::Projection,
            State::Projection | State::Corner => State::Corner,
        };
        match (self.state, input) {
            (State::Paper, CommandInput::Keyword(k)) => {
                self.paper = match k {
                    k if k == A4 => Paper::A4,
                    k if k == A2 => Paper::A2,
                    k if k == A1 => Paper::A1,
                    k if k == A0 => Paper::A0,
                    _ => Paper::A3,
                };
            }
            (State::Orientation, CommandInput::Keyword(k)) => self.landscape = k == LANDSCAPE,
            (State::Scale, CommandInput::Distance(n)) => {
                if n.is_finite() && n > 0.0 {
                    self.ratio = n;
                } else {
                    ctx.message("The scale must be greater than zero.");
                    return Step::Continue;
                }
            }
            (State::Title, CommandInput::Text(t)) => self.fields.title = t.trim().to_owned(),
            (State::Number, CommandInput::Text(t)) => self.fields.number = t.trim().to_owned(),
            (State::DrawnBy, CommandInput::Text(t)) => self.fields.drawn_by = t.trim().to_owned(),
            (State::Date, CommandInput::Text(t)) => self.fields.date = t.trim().to_owned(),
            (State::Sheet, CommandInput::Text(t)) => self.fields.sheet = t.trim().to_owned(),
            (State::Projection, CommandInput::Keyword(k)) => self.fields.first_angle = k == FIRST,
            (State::Corner, CommandInput::Point(at)) => return self.finish(at, ctx),
            (State::Corner, CommandInput::Accept) => return self.finish(Point2::ORIGIN, ctx),
            (_, CommandInput::Accept) => {}
            _ => return Step::Continue,
        }
        self.state = next(self.state, self.on_paper);
        Step::Continue
    }
}
