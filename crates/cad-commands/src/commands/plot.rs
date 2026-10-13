//! PLOT (#187): a paper size, an area, a scale, and whether lineweights
//! and colours are plotted; the page itself is `cad-plot`'s to make, and
//! the download the host's. The last settings are the next defaults.
//!
//! On a layout (#194) the paper is the layout's sheet: the prompt offers
//! the sheet as it is, and a paper named here becomes the sheet, kept on
//! the layout's row as one undoable change. The area is the sheet or a
//! window of it; a fit of the sheet is one to one.

use cad_core::{Aabb, Point2, Vec2};
use cad_document::LayoutId;
use cad_input::{ExpectedInput, InputKind, Keyword};

use crate::{
    Command, CommandContext, CommandInput, HostRequest, Paper, PlotArea, PlotScale, PlotSettings,
    Prompt, Step,
};

const PAPERS: [Keyword; 7] = [
    Keyword::new("A4"),
    Keyword::new("A3"),
    Keyword::new("A2"),
    Keyword::new("A1"),
    Keyword::new("A0"),
    Keyword::new("Letter"),
    Keyword::new("Tabloid"),
];
const EXTENTS: Keyword = Keyword::new("Extents");
const WINDOW: Keyword = Keyword::new("Window");
const FIT: Keyword = Keyword::new("Fit");
const YES: Keyword = Keyword::new("Yes");
const NO: Keyword = Keyword::new("No");
const COLOUR: Keyword = Keyword::new("Colour");
const MONOCHROME: Keyword = Keyword::new("Monochrome");

#[derive(Debug)]
enum State {
    Paper,
    Area,
    WindowFirst,
    WindowSecond(Point2),
    Scale,
    Lineweights,
    Colour,
}

#[derive(Debug)]
pub(crate) struct PlotCommand {
    state: State,
    settings: PlotSettings,
    /// The layout plotted, with its name and sheet as the command found
    /// them; `None` on the model.
    layout: Option<(LayoutId, String, Vec2)>,
}

impl PlotCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Paper,
            settings: PlotSettings::default(),
            layout: None,
        }
    }

    /// The paper a sheet is, by name, either way up; or its size.
    fn describe_sheet(size: Vec2) -> String {
        let same = |a: f64, b: f64| (a - b).abs() < 0.05;
        Paper::ALL
            .iter()
            .find(|paper| {
                let (w, h) = paper.size_mm();
                (same(size.x, w) && same(size.y, h)) || (same(size.x, h) && same(size.y, w))
            })
            .map_or_else(
                || format!("{} x {} mm", size.x, size.y),
                |paper| paper.name().to_owned(),
            )
    }

    /// `paper` held as the sheet is: landscape when the sheet is.
    fn sheet_of(paper: Paper, like: Vec2) -> Vec2 {
        let (w, h) = paper.size_mm();
        if like.x > like.y {
            Vec2::new(h, w)
        } else {
            Vec2::new(w, h)
        }
    }

    /// On a layout, a paper named becomes its sheet, kept on the row.
    fn take_paper(&mut self, paper: Paper, ctx: &mut CommandContext<'_>) {
        match &mut self.layout {
            None => self.settings.paper = paper,
            Some((id, _, sheet)) => {
                let size = Self::sheet_of(paper, *sheet);
                if size != *sheet {
                    let mut tx = ctx.transaction();
                    if let Err(error) = tx.set_layout_sheet(*id, size) {
                        ctx.message(format!("The sheet cannot be set: {error}."));
                        return;
                    }
                    tx.commit();
                    *sheet = size;
                }
            }
        }
    }

    /// `1:50`, `1=50`, or `50` is one to fifty; `2:1` is twice the size.
    fn parse_scale(text: &str) -> Option<PlotScale> {
        let text = text.trim();
        if text.is_empty() || FIT.matches(text) {
            return Some(PlotScale::Fit);
        }
        let (paper, drawing) = match text.split_once([':', '=']) {
            Some((paper, drawing)) => (
                paper.trim().parse::<f64>().ok()?,
                drawing.trim().parse::<f64>().ok()?,
            ),
            None => (1.0, text.parse::<f64>().ok()?),
        };
        (paper > 0.0 && drawing > 0.0 && paper.is_finite() && drawing.is_finite())
            .then(|| PlotScale::Ratio(drawing / paper))
    }

    fn describe_scale(scale: PlotScale) -> String {
        match scale {
            PlotScale::Fit => "Fit".to_owned(),
            PlotScale::Ratio(denominator) => format!("1:{denominator}"),
        }
    }

    fn finish(&self, ctx: &mut CommandContext<'_>) -> Step {
        // The layout's sheet is the layout's, not the next model plot's.
        ctx.settings.plot = PlotSettings {
            paper: match self.layout {
                Some(_) => ctx.settings.plot.paper,
                None => self.settings.paper,
            },
            ..self.settings
        };
        ctx.request_host(HostRequest::Plot);
        let (area, paper) = match &self.layout {
            None => (
                match self.settings.area {
                    PlotArea::Extents => "the extents",
                    PlotArea::Window(_) => "the window",
                },
                self.settings.paper.name().to_owned(),
            ),
            Some((_, _, sheet)) => (
                match self.settings.area {
                    PlotArea::Extents => "the sheet",
                    PlotArea::Window(_) => "the window",
                },
                Self::describe_sheet(*sheet),
            ),
        };
        let space = match &self.layout {
            None => "model space".to_owned(),
            Some((_, name, _)) => format!("layout \"{name}\""),
        };
        ctx.message(format!(
            "Plotting {} of {}{} on {}, {}.",
            area,
            space,
            if self.settings.monochrome {
                " in black"
            } else {
                ""
            },
            paper,
            match self.settings.scale {
                PlotScale::Fit => "to fit".to_owned(),
                PlotScale::Ratio(denominator) => format!("at 1:{denominator}"),
            }
        ));
        Step::Finished
    }
}

impl Command for PlotCommand {
    fn start(&mut self, ctx: &mut CommandContext<'_>) -> Step {
        self.settings = ctx.settings.plot;
        if ctx.space != LayoutId::MODEL
            && let Some(layout) = ctx.drawing.layouts().get(ctx.space)
        {
            self.layout = Some((ctx.space, layout.name.clone(), layout.sheet_size()));
        }
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Paper => Prompt::new("Paper size").with_default(match &self.layout {
                None => self.settings.paper.name().to_owned(),
                Some((_, _, sheet)) => Self::describe_sheet(*sheet),
            }),
            State::Area => Prompt::new("Plot area").with_default(match self.settings.area {
                PlotArea::Extents => "Extents",
                PlotArea::Window(_) => "Window",
            }),
            State::WindowFirst => Prompt::new("Specify first corner of the window"),
            State::WindowSecond(_) => Prompt::new("Specify opposite corner"),
            State::Scale => {
                Prompt::new("Scale, as 1:N").with_default(Self::describe_scale(self.settings.scale))
            }
            State::Lineweights => {
                Prompt::new("Plot lineweights?").with_default(if self.settings.lineweights {
                    "Yes"
                } else {
                    "No"
                })
            }
            State::Colour => {
                Prompt::new("Colour or monochrome?").with_default(if self.settings.monochrome {
                    "Monochrome"
                } else {
                    "Colour"
                })
            }
        }
    }

    fn expected(&self) -> ExpectedInput {
        match self.state {
            State::Paper => ExpectedInput::new(InputKind::Nothing).with_keywords(&PAPERS),
            State::Area => ExpectedInput::new(InputKind::Nothing).with_keywords(&[EXTENTS, WINDOW]),
            State::WindowFirst | State::WindowSecond(_) => ExpectedInput::point(),
            State::Scale => ExpectedInput::text().with_keywords(&[FIT]),
            State::Lineweights => ExpectedInput::new(InputKind::Nothing).with_keywords(&[YES, NO]),
            State::Colour => {
                ExpectedInput::new(InputKind::Nothing).with_keywords(&[COLOUR, MONOCHROME])
            }
        }
    }

    /// The corners of a window are places on the screen, not points of
    /// the drawing.
    fn takes_object_snap(&self) -> bool {
        false
    }

    fn step(&mut self, input: CommandInput, ctx: &mut CommandContext<'_>) -> Step {
        match (&self.state, input) {
            (State::Paper, CommandInput::Keyword(keyword)) => {
                if let Some(paper) = Paper::ALL
                    .iter()
                    .zip(PAPERS)
                    .find(|(_, name)| *name == keyword)
                    .map(|(paper, _)| *paper)
                {
                    self.take_paper(paper, ctx);
                }
                self.state = State::Area;
            }
            (State::Paper, CommandInput::Accept) => self.state = State::Area,
            (State::Area, CommandInput::Keyword(WINDOW)) => self.state = State::WindowFirst,
            (State::Area, CommandInput::Keyword(_)) => {
                self.settings.area = PlotArea::Extents;
                self.state = State::Scale;
            }
            (State::Area, CommandInput::Accept) => {
                self.state = match self.settings.area {
                    PlotArea::Extents => State::Scale,
                    PlotArea::Window(_) => State::WindowFirst,
                }
            }
            (State::WindowFirst, CommandInput::Point(first)) => {
                self.state = State::WindowSecond(first);
            }
            (State::WindowSecond(first), CommandInput::Point(second)) => {
                let window = Aabb::from_points(*first, second);
                let size = window.max() - window.min();
                if size.x <= 0.0 || size.y <= 0.0 {
                    ctx.message("A window needs two different corners.");
                    self.state = State::WindowFirst;
                } else {
                    self.settings.area = PlotArea::Window(window);
                    self.state = State::Scale;
                }
            }
            (State::Scale, CommandInput::Keyword(_)) => {
                self.settings.scale = PlotScale::Fit;
                self.state = State::Lineweights;
            }
            (State::Scale, CommandInput::Accept) => self.state = State::Lineweights,
            (State::Scale, CommandInput::Text(text)) => match Self::parse_scale(&text) {
                Some(scale) => {
                    self.settings.scale = scale;
                    self.state = State::Lineweights;
                }
                None => ctx.message("A scale is 1:N — 1:50, 1:100 — or Fit."),
            },
            (State::Lineweights, CommandInput::Keyword(keyword)) => {
                self.settings.lineweights = keyword == YES;
                self.state = State::Colour;
            }
            (State::Lineweights, CommandInput::Accept) => self.state = State::Colour,
            (State::Colour, CommandInput::Keyword(keyword)) => {
                self.settings.monochrome = keyword == MONOCHROME;
                return self.finish(ctx);
            }
            (State::Colour, CommandInput::Accept) => return self.finish(ctx),
            _ => {}
        }
        Step::Continue
    }
}
