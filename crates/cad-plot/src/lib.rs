//! PDF plots of model space (#187; ADR-0027).
//!
//! A plot is a page: a paper size, a scale or a fit, an area of the
//! drawing — its extents or a window — and whether lineweights and colours
//! are plotted. The page is vector: every entity goes on it through the
//! same walk that fills the committed scene (`cad_render::walk_entity`), so
//! what prints is what is drawn — lines and arcs, a text's strokes, a
//! dimension's layout, a hatch's fill and pattern — with the entity's
//! resolved colour, lineweight, and linetype. Fonts are not embedded: text
//! is its strokes, as on screen (ADR-0016).
//!
//! The writer produces PDF 1.4 as ASCII, one page, one content stream,
//! uncompressed: a reader needs nothing beyond the basic operators.

use std::fmt::Write as _;

use cad_commands::{PlotArea, PlotScale, PlotSettings};
use cad_core::tolerance::LinearTolerance;
use cad_core::{Aabb, Point2, Vec2};
use cad_document::{Drawing, Entity, HatchEntity, Linetype, LinetypeId, Paint, Properties, Weight};
use cad_geometry::{Arc, Primitive};
use cad_render::{GeometrySink, walk_entity};

/// PDF points per millimetre: a point is 1/72 of an inch.
pub const POINTS_PER_MM: f64 = 72.0 / 25.4;
/// The unprinted edge of the page, in millimetres.
pub const MARGIN_MM: f64 = 10.0;
/// The width of a line whose lineweight is the default, or when lineweights
/// are not plotted, in millimetres.
pub const DEFAULT_WIDTH_MM: f64 = 0.25;
/// Where a drawing has nothing to plot: a page is still made, of this
/// many units on a side around the origin.
const EMPTY_EXTENT: f64 = 100.0;

/// The page a plot goes on, and how the drawing is placed on it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Page {
    /// The paper, in points, as it is held: landscape when the area is
    /// wider than tall.
    pub width: f64,
    pub height: f64,
    pub landscape: bool,
    /// The area of the drawing on the page, in drawing units.
    pub area: Aabb,
    /// Points on the page per drawing unit.
    pub scale: f64,
    /// Where the area's centre lands: the page's centre.
    pub center: Point2,
}

impl Page {
    /// Lays the plot out: the area, its orientation, the scale, the fit.
    pub fn new(drawing: &Drawing, settings: &PlotSettings) -> Self {
        let area = match settings.area {
            PlotArea::Window(window) => window,
            PlotArea::Extents => drawing.extents().unwrap_or_else(|| {
                Aabb::from_points(
                    Point2::new(-EMPTY_EXTENT / 2.0, -EMPTY_EXTENT / 2.0),
                    Point2::new(EMPTY_EXTENT / 2.0, EMPTY_EXTENT / 2.0),
                )
            }),
        };
        let size = area.max() - area.min();
        let (paper_width, paper_height) = settings.paper.size_mm();
        let landscape = size.x > size.y;
        let (width_mm, height_mm) = if landscape {
            (paper_height, paper_width)
        } else {
            (paper_width, paper_height)
        };
        let width = width_mm * POINTS_PER_MM;
        let height = height_mm * POINTS_PER_MM;
        let usable = Vec2::new(
            (width_mm - 2.0 * MARGIN_MM) * POINTS_PER_MM,
            (height_mm - 2.0 * MARGIN_MM) * POINTS_PER_MM,
        );
        let mm_per_unit = drawing.unit().map_or(1.0, |unit| unit.metres() * 1000.0);
        let scale = match settings.scale {
            PlotScale::Ratio(denominator) => mm_per_unit / denominator * POINTS_PER_MM,
            PlotScale::Fit => {
                let fit_x = if size.x > 0.0 {
                    usable.x / size.x
                } else {
                    f64::INFINITY
                };
                let fit_y = if size.y > 0.0 {
                    usable.y / size.y
                } else {
                    f64::INFINITY
                };
                let fit = fit_x.min(fit_y);
                if fit.is_finite() { fit } else { POINTS_PER_MM }
            }
        };
        Self {
            width,
            height,
            landscape,
            area,
            scale,
            center: Point2::new(width / 2.0, height / 2.0),
        }
    }

    /// A drawing point on the page, in points.
    pub fn to_page(&self, point: Point2) -> Point2 {
        self.center + (point - self.area.center()) * self.scale
    }

    /// The area on the page, in points.
    pub fn area_on_page(&self) -> Aabb {
        Aabb::from_points(self.to_page(self.area.min()), self.to_page(self.area.max()))
    }

    /// Where a `scale` lands a drawing length, in points: what a lineweight
    /// never is — a lineweight is a width on paper.
    pub fn length(&self, units: f64) -> f64 {
        units * self.scale
    }
}

/// The drawing as a one-page PDF, ASCII throughout.
pub fn plot_pdf(drawing: &Drawing, settings: &PlotSettings) -> String {
    let page = Page::new(drawing, settings);
    let content = content(drawing, settings, &page);
    pdf_file(&page, &content)
}

/// The resolved look of one part: what the scene resolves to a style row.
#[derive(Clone, Debug, PartialEq)]
struct Look {
    rgb: [u8; 3],
    /// In points.
    width: f64,
    /// Dash and gap lengths on the page, in points; empty for continuous.
    dashes: Vec<f64>,
}

fn look_of(
    drawing: &Drawing,
    page: &Page,
    settings: &PlotSettings,
    properties: Properties,
) -> Look {
    let layer = drawing.layers().get(properties.layer);
    let paint = layer.map_or(Paint::Foreground, |layer| properties.color.paint_on(layer));
    let rgb = match paint {
        Paint::Rgb(rgb) if !settings.monochrome => rgb,
        _ => [0, 0, 0],
    };
    let weight = layer.map_or(Weight::Default, |layer| {
        properties.lineweight.weight_on(layer)
    });
    let width_mm = match weight {
        Weight::Hundredths(hundredths) if settings.lineweights => f64::from(hundredths) / 100.0,
        _ => DEFAULT_WIDTH_MM,
    };
    let linetype = layer.map_or(LinetypeId::FIRST, |layer| properties.linetype.row_on(layer));
    let dashes = match drawing.linetypes().get(linetype) {
        Some(def) if !def.is_continuous() => def
            .pattern
            .iter()
            .map(|length| page.length(length * drawing.ltscale()))
            .collect(),
        _ => Vec::new(),
    };
    Look {
        rgb,
        width: width_mm * POINTS_PER_MM,
        dashes,
    }
}

/// The content stream: every shown entity, in ID order, each part with its
/// look, inside a clip to the plotted area.
fn content(drawing: &Drawing, settings: &PlotSettings, page: &Page) -> String {
    let mut out = String::new();
    // Round caps and joins meet at vertices as they do on screen.
    out.push_str("q\n1 J\n1 j\n");
    let clip = page.area_on_page();
    let size = clip.max() - clip.min();
    let _ = writeln!(
        out,
        "{} {} {} {} re W n",
        num(clip.min().x),
        num(clip.min().y),
        num(size.x),
        num(size.y)
    );
    let mut sink = PageSink::new(page);
    let mut current: Option<Look> = None;
    let mut ids: Vec<_> = drawing.shown().map(|(id, _)| id).collect();
    ids.sort_unstable();
    for id in ids {
        let Some(entity) = drawing.entity(id) else {
            continue;
        };
        let properties = drawing.properties(id).unwrap_or_default();
        let parts: Vec<(Entity, Properties)> = match entity {
            Entity::BlockRef(reference) => drawing.blocks().flatten(reference, properties),
            other => vec![(other.clone(), properties)],
        };
        for (part, mut properties) in parts {
            // As on screen: a text, a dimension, and a hatch are drawn
            // continuous, whatever their linetype says.
            if matches!(
                part,
                Entity::Text(_)
                    | Entity::MText(_)
                    | Entity::Dimension(_)
                    | Entity::Hatch(_)
                    | Entity::Viewport(_)
            ) {
                properties.linetype = Linetype::CONTINUOUS;
            }
            let look = look_of(drawing, page, settings, properties);
            if current.as_ref() != Some(&look) {
                sink.flush(&mut out);
                let [red, green, blue] = look.rgb;
                let _ = writeln!(
                    out,
                    "{} {} {} RG {} {} {} rg",
                    num(f64::from(red) / 255.0),
                    num(f64::from(green) / 255.0),
                    num(f64::from(blue) / 255.0),
                    num(f64::from(red) / 255.0),
                    num(f64::from(green) / 255.0),
                    num(f64::from(blue) / 255.0),
                );
                let _ = writeln!(out, "{} w", num(look.width));
                let dashes: Vec<String> = look.dashes.iter().map(|d| num(*d)).collect();
                let _ = writeln!(out, "[{}] 0 d", dashes.join(" "));
                current = Some(look);
            }
            walk_entity(&part, drawing.tables(), &mut sink);
            sink.flush(&mut out);
        }
    }
    sink.flush(&mut out);
    out.push_str("Q\n");
    out
}

/// Builds paths on the page from what the walk hands it. Consecutive
/// pieces that meet end to start continue one subpath, so a polyline's
/// dashes run on through its vertices, as they do on screen.
struct PageSink<'a> {
    page: &'a Page,
    /// The stroke path being built, and where it is (drawing units).
    path: String,
    current: Option<Point2>,
    /// Fill paths, each complete, emitted after the strokes they came with.
    fills: Vec<String>,
}

impl<'a> PageSink<'a> {
    fn new(page: &'a Page) -> Self {
        Self {
            page,
            path: String::new(),
            current: None,
            fills: Vec::new(),
        }
    }

    /// Emits what was built: the strokes as one path, then the fills.
    fn flush(&mut self, out: &mut String) {
        if !self.path.is_empty() {
            out.push_str(&self.path);
            out.push_str("S\n");
            self.path.clear();
        }
        self.current = None;
        for fill in self.fills.drain(..) {
            out.push_str(&fill);
        }
    }

    fn continues_from(&self, point: Point2) -> bool {
        self.current.is_some_and(|current| {
            LinearTolerance::DEFAULT
                .around(&[current, point])
                .coincident(current, point)
        })
    }

    fn move_or_continue(&mut self, start: Point2) {
        if !self.continues_from(start) {
            let at = self.page.to_page(start);
            let _ = writeln!(self.path, "{} {} m", num(at.x), num(at.y));
        }
    }

    fn line_to(path: &mut String, page: &Page, point: Point2) {
        let at = page.to_page(point);
        let _ = writeln!(path, "{} {} l", num(at.x), num(at.y));
    }

    /// An arc as cubic Béziers of at most a quarter turn each, run from
    /// its counter-clockwise start, or from its end back round when not
    /// `forwards`. The current point must be at the arc's first point.
    fn arc_to(path: &mut String, page: &Page, arc: &Arc, forwards: bool) {
        let pieces = (arc.sweep.abs() / std::f64::consts::FRAC_PI_2)
            .ceil()
            .max(1.0) as usize;
        let step = arc.sweep / pieces as f64 * if forwards { 1.0 } else { -1.0 };
        let mut angle = if forwards {
            arc.start
        } else {
            arc.start + arc.sweep
        };
        // The tangent control distance for a circular arc of `step`.
        let k = 4.0 / 3.0 * (step.abs() / 4.0).tan();
        for _ in 0..pieces {
            let next = angle + step;
            let from = arc.point_at(angle);
            let to = arc.point_at(next);
            let direction = step.signum();
            let tangent_from = Vec2::new(-angle.sin(), angle.cos()) * (arc.radius * k * direction);
            let tangent_to = Vec2::new(-next.sin(), next.cos()) * (arc.radius * k * direction);
            let c1 = page.to_page(from + tangent_from);
            let c2 = page.to_page(to - tangent_to);
            let end = page.to_page(to);
            let _ = writeln!(
                path,
                "{} {} {} {} {} {} c",
                num(c1.x),
                num(c1.y),
                num(c2.x),
                num(c2.y),
                num(end.x),
                num(end.y)
            );
            angle = next;
        }
    }

    /// A closed region of primitives as one subpath.
    fn ring(path: &mut String, page: &Page, primitives: &[Primitive]) {
        let mut first = true;
        for primitive in primitives {
            let start = match primitive {
                Primitive::Segment(segment) => segment.start,
                Primitive::Arc(arc) => arc.start_point(),
                Primitive::Circle(circle) => circle.point_at(0.0),
            };
            if first {
                let at = page.to_page(start);
                let _ = writeln!(path, "{} {} m", num(at.x), num(at.y));
                first = false;
            }
            match primitive {
                Primitive::Segment(segment) => Self::line_to(path, page, segment.end),
                Primitive::Arc(arc) => Self::arc_to(path, page, arc, true),
                Primitive::Circle(circle) => Self::arc_to(
                    path,
                    page,
                    &Arc::new(circle.center, circle.radius, 0.0, std::f64::consts::TAU),
                    true,
                ),
            }
        }
        if !first {
            path.push_str("h\n");
        }
    }
}

impl GeometrySink for PageSink<'_> {
    fn segment(&mut self, start: Point2, end: Point2, _distance: f64) {
        self.move_or_continue(start);
        Self::line_to(&mut self.path, self.page, end);
        self.current = Some(end);
    }

    fn arc(&mut self, arc: &Arc, _distance: f64, forwards: bool) {
        let (first, last) = if forwards {
            (arc.start_point(), arc.end_point())
        } else {
            (arc.end_point(), arc.start_point())
        };
        self.move_or_continue(first);
        Self::arc_to(&mut self.path, self.page, arc, forwards);
        self.current = Some(last);
    }

    fn triangle(&mut self, corners: [Point2; 3]) {
        let mut fill = String::new();
        let [a, b, c] = corners.map(|corner| self.page.to_page(corner));
        let _ = writeln!(
            fill,
            "{} {} m {} {} l {} {} l h f",
            num(a.x),
            num(a.y),
            num(b.x),
            num(b.y),
            num(c.x),
            num(c.y)
        );
        self.fills.push(fill);
    }

    /// A solid hatch is its loops, filled by the even-odd rule: one path,
    /// no seams between triangles.
    fn hatch_fill(&mut self, hatch: &HatchEntity) {
        let mut fill = String::new();
        for path in &hatch.loops {
            Self::ring(&mut fill, self.page, &path.primitives());
        }
        if !fill.is_empty() {
            fill.push_str("f*\n");
            self.fills.push(fill);
        }
    }
}

/// A number as PDF reads it: at most three decimals, no exponent, no
/// trailing zeros.
pub fn num(value: f64) -> String {
    let value = if value.is_finite() { value } else { 0.0 };
    let rounded = (value * 1000.0).round() / 1000.0;
    let text = format!("{rounded:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".to_owned()
    } else {
        text.to_owned()
    }
}

/// The file around the content: a catalog, the page tree, the page, the
/// stream, and the information, with a cross-reference table that
/// points at each.
fn pdf_file(page: &Page, content: &str) -> String {
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    let mut object = |out: &mut String, body: &str| {
        offsets.push(out.len());
        let number = offsets.len();
        let _ = write!(out, "{number} 0 obj\n{body}\nendobj\n");
    };
    object(&mut out, "<< /Type /Catalog /Pages 2 0 R >>");
    object(&mut out, "<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    object(
        &mut out,
        &format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents 4 0 R /Resources << >> >>",
            num(page.width),
            num(page.height)
        ),
    );
    object(
        &mut out,
        &format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ),
    );
    object(&mut out, "<< /Producer (CadSlate2D) >>");
    let xref = out.len();
    let _ = writeln!(out, "xref\n0 {}\n0000000000 65535 f ", offsets.len() + 1);
    for offset in &offsets {
        let _ = writeln!(out, "{offset:010} 00000 n ");
    }
    let _ = write!(
        out,
        "trailer\n<< /Size {} /Root 1 0 R /Info {} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        offsets.len() + 1,
        offsets.len()
    );
    out
}

/// The paper's name and the plot's scale, for the command log.
pub fn describe(settings: &PlotSettings) -> String {
    let scale = match settings.scale {
        PlotScale::Fit => "to fit".to_owned(),
        PlotScale::Ratio(denominator) => format!("at 1:{}", num(denominator)),
    };
    format!("{} {scale}", settings.paper.name())
}

#[cfg(test)]
mod tests;
