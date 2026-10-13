//! PLOT (#187): a paper size, an area, a scale, and whether lineweights
//! and colours are plotted; the page itself is `cad-plot`'s to make, and
//! the download the host's. The last settings are the next defaults.

use cad_core::{Aabb, Point2};
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
}

impl PlotCommand {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Paper,
            settings: PlotSettings::default(),
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
        ctx.settings.plot = self.settings;
        ctx.request_host(HostRequest::Plot);
        let area = match self.settings.area {
            PlotArea::Extents => "the extents".to_owned(),
            PlotArea::Window(_) => "the window".to_owned(),
        };
        ctx.message(format!(
            "Plotting {} of {} on {}, {}.",
            area,
            if self.settings.monochrome {
                "model space in black"
            } else {
                "model space"
            },
            self.settings.paper.name(),
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
        Step::Continue
    }

    fn prompt(&self) -> Prompt {
        match self.state {
            State::Paper => Prompt::new("Paper size").with_default(self.settings.paper.name()),
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
                    self.settings.paper = paper;
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
