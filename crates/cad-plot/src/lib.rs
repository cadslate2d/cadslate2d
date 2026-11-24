//! PDF plots of model space (#187) and of a layout (#194; ADR-0027).
//!
//! A plot is a page: a paper size, a scale or a fit, an area of the
//! drawing — its extents or a window — and whether lineweights and colours
//! are plotted. A layout's page is its sheet: the paper at one to one,
//! and the model through each viewport at the viewport's scale, clipped
//! to its frame. The page is vector: every entity goes on it through the
//! same walk that fills the committed scene (`cad_render::walk_entity`), so
//! what prints is what is drawn — lines and arcs, a text's strokes, a
//! dimension's layout, a hatch's fill and pattern — with the entity's
//! resolved colour, lineweight, and linetype. Fonts are not embedded: text
//! is its strokes, as on screen (ADR-0016). A picture is embedded as it is
//! held (ADR-0042 §5): a JPEG's bytes, a PNG's compressed samples, each an
//! image of the page's resources that the reader decodes.
//!
//! The writer produces PDF 1.4 as ASCII, one page, one content stream,
//! uncompressed: a reader needs nothing beyond the basic operators, and a
//! picture's bytes are in base 85 (`ASCII85Decode`).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use cad_commands::{PlotArea, PlotScale, PlotSettings};
use cad_core::tolerance::LinearTolerance;
use cad_core::{Aabb, EntityId, Point2, Vec2};
use cad_document::{
    Drawing, Entity, HatchEntity, ImageDef, ImageDefId, ImageFormat, Layout, LayoutArea, LayoutId,
    Linetype, LinetypeId, Paint, Properties, ViewportEntity, Weight, WipeoutEntity, draw_order,
};
use cad_geometry::{Arc, Primitive};
use cad_render::{GeometrySink, ImageQuad, drawn_continuous, viewports_shown, walk_entity};

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

/// Where drawing units land on the page, in points: `to + turned(point -
/// from) * scale`. The page's own, or the model's through a viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// Points on the page per unit placed.
    pub scale: f64,
    pub from: Point2,
    pub to: Point2,
    /// Quarter turns counter-clockwise: a layout plotted turned (#208).
    pub turns: u8,
}

/// `offset` turned `turns` quarter turns counter-clockwise, exactly.
fn turned(offset: Vec2, turns: u8) -> Vec2 {
    match turns % 4 {
        0 => offset,
        1 => Vec2::new(-offset.y, offset.x),
        2 => Vec2::new(-offset.x, -offset.y),
        _ => Vec2::new(offset.y, -offset.x),
    }
}

impl Placement {
    pub fn map(&self, point: Point2) -> Point2 {
        self.to + turned(point - self.from, self.turns) * self.scale
    }

    /// Where a length lands, in points: what a lineweight never is — a
    /// lineweight is a width on paper.
    pub fn length(&self, units: f64) -> f64 {
        units * self.scale
    }

    /// The page point back where it came from.
    pub fn unmap(&self, point: Point2) -> Point2 {
        self.from + turned(point - self.to, (4 - self.turns % 4) % 4) * (1.0 / self.scale)
    }
}

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
    /// Where the area's centre lands.
    pub center: Point2,
    /// Quarter turns counter-clockwise the area is turned on the page.
    pub turns: u8,
    /// What of the page is printed, in points: a layout's printable area
    /// within its margins (#208); for the model's, the area itself.
    pub printable: Option<Aabb>,
}

impl Page {
    /// The page of a space: the model's, or a layout's sheet.
    pub fn in_space(drawing: &Drawing, space: LayoutId, settings: &PlotSettings) -> Self {
        match drawing.layouts().get(space) {
            Some(layout) if space != LayoutId::MODEL => {
                Self::of_layout(layout, drawing.extents_in(space))
            }
            _ => Self::new(drawing, settings),
        }
    }

    /// A layout's page is its sheet (#194), plotted by the layout's own
    /// settings (#208): the paper its sheet is, turned a quarter turn on
    /// the page when they say so; printed within the sheet's margins; the
    /// area the sheet, what is on it (`extents`, or the sheet when it has
    /// nothing), or a window of it; the scale its own — a paper millimetre
    /// per `scale` units — or a fit within the margins. The sheet plots
    /// where it lies on the paper; a window or the extents, centred in
    /// the printable area.
    pub fn of_layout(layout: &Layout, extents: Option<Aabb>) -> Self {
        let plot = layout.plot_settings();
        let sheet = layout.sheet();
        let (area, centred) = match plot.area {
            LayoutArea::Layout => (sheet, false),
            LayoutArea::Extents => (extents.unwrap_or(sheet), true),
            LayoutArea::Window(window) => (window, true),
        };
        let paper = layout.sheet_size();
        let turns = plot.turns % 4;
        let upright = Vec2::new(paper.x * POINTS_PER_MM, paper.y * POINTS_PER_MM);
        let (width, height) = if turns % 2 == 1 {
            (upright.y, upright.x)
        } else {
            (upright.x, upright.y)
        };
        let page_center = Point2::new(width / 2.0, height / 2.0);
        // The printable area on the upright sheet, turned with it.
        let margins = layout.printable_area();
        let sheet_center = Point2::new(upright.x / 2.0, upright.y / 2.0);
        let on_page = |point: Point2| {
            page_center
                + turned(
                    Point2::new(point.x * POINTS_PER_MM, point.y * POINTS_PER_MM) - sheet_center,
                    turns,
                )
        };
        let printable = Aabb::from_points(on_page(margins.min()), on_page(margins.max()));
        let size = area.max() - area.min();
        let scale = match plot.scale {
            Some(millimetres) => millimetres * POINTS_PER_MM,
            None => {
                let room = margins.max() - margins.min();
                Self::fit(
                    Vec2::new(room.x * POINTS_PER_MM, room.y * POINTS_PER_MM),
                    size,
                )
            }
        };
        let center = if centred {
            printable.center()
        } else {
            on_page(area.center())
        };
        Self {
            width,
            height,
            landscape: width > height,
            area,
            scale,
            center,
            turns,
            printable: Some(printable),
        }
    }

    /// Points per unit at which `size` fills `usable`; a millimetre per
    /// unit for an area with no size.
    fn fit(usable: Vec2, size: Vec2) -> f64 {
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

    /// Lays the model's plot out: the area, its orientation, the scale,
    /// the fit.
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
            PlotScale::Fit => Self::fit(usable, size),
        };
        Self {
            width,
            height,
            landscape,
            area,
            scale,
            center: Point2::new(width / 2.0, height / 2.0),
            turns: 0,
            printable: None,
        }
    }

    /// How the plotted space lands on the page.
    pub fn placement(&self) -> Placement {
        Placement {
            scale: self.scale,
            from: self.area.center(),
            to: self.center,
            turns: self.turns,
        }
    }

    /// What the page clips to, in points: the area, within the printable
    /// area when there is one.
    pub fn clip(&self) -> Aabb {
        let area = self.area_on_page();
        match self.printable {
            Some(printable) => Aabb::from_points(
                Point2::new(
                    area.min().x.max(printable.min().x),
                    area.min().y.max(printable.min().y),
                ),
                Point2::new(
                    area.max()
                        .x
                        .min(printable.max().x)
                        .max(area.min().x.max(printable.min().x)),
                    area.max()
                        .y
                        .min(printable.max().y)
                        .max(area.min().y.max(printable.min().y)),
                ),
            ),
            None => area,
        }
    }

    /// How the model lands on the page through a viewport of the plotted
    /// layout: at the viewport's scale on the paper, then the paper's.
    pub fn through(&self, viewport: &ViewportEntity) -> Placement {
        Placement {
            scale: viewport.scale * self.scale,
            from: viewport.view_center,
            to: self.to_page(viewport.center),
            turns: self.turns,
        }
    }

    /// A drawing point on the page, in points.
    pub fn to_page(&self, point: Point2) -> Point2 {
        self.placement().map(point)
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

/// Model space as a one-page PDF, ASCII throughout.
pub fn plot_pdf(drawing: &Drawing, settings: &PlotSettings) -> String {
    plot_pdf_in(drawing, LayoutId::MODEL, settings)
}

/// A space as a one-page PDF: the model, or a layout's sheet with the
/// model through its viewports (#194).
pub fn plot_pdf_in(drawing: &Drawing, space: LayoutId, settings: &PlotSettings) -> String {
    let page = Page::in_space(drawing, space, settings);
    let (content, pictures) = content(drawing, space, settings, &page);
    pdf_file(&page, &content, &pictures)
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
    placement: &Placement,
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
            .map(|length| placement.length(length * drawing.ltscale()))
            .collect(),
        _ => Vec::new(),
    };
    Look {
        rgb,
        width: width_mm * POINTS_PER_MM,
        dashes,
    }
}

/// The content stream: every shown entity of the space, in the drawing
/// order (ADR-0032),
/// each part with its look, inside a clip to the plotted area. On a
/// layout, for each viewport that is on, the model's entities through it,
/// inside a clip to its frame; then the paper's entities at the page's
/// placement over them all, as the screen draws them (#413) — a viewport
/// itself draws nothing. With it, the pictures the stream draws.
fn content(
    drawing: &Drawing,
    space: LayoutId,
    settings: &PlotSettings,
    page: &Page,
) -> (String, Pictures) {
    let mut out = String::new();
    // Round caps and joins meet at vertices as they do on screen.
    out.push_str("q\n1 J\n1 j\n");
    clip(&mut out, page.clip());
    let mut painter = Painter {
        drawing,
        settings,
        current: None,
        pictures: Pictures::default(),
    };
    let ids = painted(drawing, drawing.shown_in(space).map(|(id, _)| id));
    // The windows the screen draws the model through, by its one rule.
    let viewports: Vec<ViewportEntity> = if space == LayoutId::MODEL {
        Vec::new()
    } else {
        viewports_shown(drawing, space).collect()
    };
    if !viewports.is_empty() {
        let model = painted(drawing, drawing.shown().map(|(id, _)| id));
        for viewport in viewports {
            out.push_str("q\n");
            let frame = viewport.frame();
            clip(
                &mut out,
                Aabb::from_points(page.to_page(frame.min()), page.to_page(frame.max())),
            );
            painter.current = None;
            // What of the model shows through it, for its construction lines.
            let window = Aabb::from_points(
                viewport.model_point(frame.min()),
                viewport.model_point(frame.max()),
            );
            painter.space(&mut out, &model, page.through(&viewport), window);
            out.push_str("Q\n");
            painter.current = None;
        }
    }
    // The paper's own objects lie over every viewport.
    painter.space(&mut out, &ids, page.placement(), page.area);
    out.push_str("Q\n");
    (out, painter.pictures)
}

/// The entities in the order they are painted, first beneath: the
/// drawing order, as the screen draws it (#226).
fn painted(drawing: &Drawing, ids: impl Iterator<Item = EntityId>) -> Vec<EntityId> {
    let mut ids: Vec<_> = ids
        .filter_map(|id| {
            let record = drawing.record(id)?;
            Some(draw_order::paint_key(
                id,
                record.order,
                record.entity.is_fill(),
            ))
        })
        .collect();
    ids.sort_unstable();
    ids.into_iter().map(|(_, _, id)| id).collect()
}

/// `re W n`: everything after is clipped to the rectangle, in points.
fn clip(out: &mut String, rectangle: Aabb) {
    let size = rectangle.max() - rectangle.min();
    let _ = writeln!(
        out,
        "{} {} {} {} re W n",
        num(rectangle.min().x),
        num(rectangle.min().y),
        num(size.x),
        num(size.y)
    );
}

/// Draws entities at a placement, setting the look as it changes.
struct Painter<'a> {
    drawing: &'a Drawing,
    settings: &'a PlotSettings,
    /// The look the stream is set to, if any.
    current: Option<Look>,
    /// The pictures drawn so far.
    pictures: Pictures,
}

impl Painter<'_> {
    /// The entities `ids` at `placement`; `area`, in the space's units,
    /// is what the page shows of it, which a construction line is clipped
    /// to (ADR-0031).
    fn space(&mut self, out: &mut String, ids: &[EntityId], placement: Placement, area: Aabb) {
        let drawing = self.drawing;
        let tier_of = |id: &EntityId| drawing.record(*id).map_or(0, |record| record.order);
        // A tier at a time (ADR-0032), as the screen draws it: every fill of
        // the tier — a block's, a multileader note's — beneath every
        // picture of it, and those beneath every stroke (#412, ADR-0042).
        // Each part is walked once; what it fills, shows, and strokes are
        // painted in three passes.
        for tier in ids.chunk_by(|a, b| tier_of(a) == tier_of(b)) {
            let mut fills: Vec<(Look, String)> = Vec::new();
            let mut shown = String::new();
            let mut strokes: Vec<(Look, String)> = Vec::new();
            for id in tier {
                let Some(entity) = drawing.entity(*id) else {
                    continue;
                };
                let properties = drawing.properties(*id).unwrap_or_default();
                let parts: Vec<(Entity, Properties)> = match entity {
                    Entity::BlockRef(reference) => drawing.blocks().flatten(reference, properties),
                    other => vec![(other.clone(), properties)],
                };
                for (part, mut properties) in parts {
                    // A viewport is a window, not a thing on the paper: what
                    // it shows is plotted, its frame is not.
                    if matches!(part, Entity::Viewport(_)) {
                        continue;
                    }
                    // As on screen: what is not a curve is drawn continuous,
                    // whatever its linetype says.
                    if drawn_continuous(&part) {
                        properties.linetype = Linetype::CONTINUOUS;
                    }
                    let look = look_of(drawing, &placement, self.settings, properties);
                    let mut sink = PageSink::new(placement);
                    walk_entity(&part, drawing.tables(), &mut sink);
                    let construction = match &part {
                        Entity::XLine(line) => line.clipped(&area),
                        Entity::Ray(ray) => ray.clipped(&area),
                        _ => None,
                    };
                    if let Some(piece) = construction {
                        sink.segment(piece.start, piece.end, 0.0);
                    }
                    // A picture the page cannot hold — missing — is its
                    // frame, as on screen.
                    for image in std::mem::take(&mut sink.images) {
                        match self.pictures.name(drawing, image.def) {
                            Some(name) => {
                                shown.push_str(&self.pictures.paint(&placement, &image, name));
                            }
                            None => sink.outline(&image),
                        }
                    }
                    let (filled, stroked) = sink.take();
                    if !filled.is_empty() {
                        fills.push((look.clone(), filled));
                    }
                    if !stroked.is_empty() {
                        strokes.push((look, stroked));
                    }
                }
            }
            for (look, paths) in fills {
                self.set_look(out, look);
                out.push_str(&paths);
            }
            // A picture keeps the look: it is drawn inside `q` … `Q`.
            out.push_str(&shown);
            for (look, paths) in strokes {
                self.set_look(out, look);
                out.push_str(&paths);
            }
        }
    }

    /// Sets the stream's look to `look`, unless it is set so already.
    fn set_look(&mut self, out: &mut String, look: Look) {
        if self.current.as_ref() == Some(&look) {
            return;
        }
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
        self.current = Some(look);
    }
}

/// Builds paths on the page from what the walk hands it. Consecutive
/// pieces that meet end to start continue one subpath, so a polyline's
/// dashes run on through its vertices, as they do on screen.
struct PageSink {
    placement: Placement,
    /// The stroke path being built, and where it is (drawing units).
    path: String,
    current: Option<Point2>,
    /// Fill paths, each complete, emitted after the strokes they came with.
    fills: Vec<String>,
    /// Wipeouts' masks, in the paper's white, emitted before the strokes
    /// they came with — a wipeout's frame — so as not to cover them.
    masks: Vec<String>,
    /// The pictures the walk handed it, for the painter to embed.
    images: Vec<ImageQuad>,
}

impl PageSink {
    fn new(placement: Placement) -> Self {
        Self {
            placement,
            path: String::new(),
            current: None,
            fills: Vec::new(),
            masks: Vec::new(),
            images: Vec::new(),
        }
    }

    /// A picture's frame, stroked: what the page shows of one it cannot
    /// hold.
    fn outline(&mut self, image: &ImageQuad) {
        let corners = image.corners();
        for (i, corner) in corners.iter().enumerate() {
            self.segment(*corner, corners[(i + 1) % 4], 0.0);
        }
    }

    /// What was built, as two parts of the stream (#412): the areas —
    /// wipeouts' masks, then fills — and the strokes, as one path.
    fn take(self) -> (String, String) {
        let areas: String = self.masks.into_iter().chain(self.fills).collect();
        let strokes = if self.path.is_empty() {
            String::new()
        } else {
            self.path + "S\n"
        };
        (areas, strokes)
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
            let at = self.placement.map(start);
            let _ = writeln!(self.path, "{} {} m", num(at.x), num(at.y));
        }
    }

    fn line_to(path: &mut String, page: &Placement, point: Point2) {
        let at = page.map(point);
        let _ = writeln!(path, "{} {} l", num(at.x), num(at.y));
    }

    /// An arc as cubic Béziers of at most a quarter turn each, run from
    /// its counter-clockwise start, or from its end back round when not
    /// `forwards`. The current point must be at the arc's first point.
    fn arc_to(path: &mut String, page: &Placement, arc: &Arc, forwards: bool) {
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
            let c1 = page.map(from + tangent_from);
            let c2 = page.map(to - tangent_to);
            let end = page.map(to);
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
    fn ring(path: &mut String, page: &Placement, primitives: &[Primitive]) {
        let mut first = true;
        for primitive in primitives {
            let start = match primitive {
                Primitive::Segment(segment) => segment.start,
                Primitive::Arc(arc) => arc.start_point(),
                Primitive::Circle(circle) => circle.point_at(0.0),
            };
            if first {
                let at = page.map(start);
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

impl GeometrySink for PageSink {
    fn segment(&mut self, start: Point2, end: Point2, _distance: f64) {
        self.move_or_continue(start);
        Self::line_to(&mut self.path, &self.placement, end);
        self.current = Some(end);
    }

    fn arc(&mut self, arc: &Arc, _distance: f64, forwards: bool) {
        let (first, last) = if forwards {
            (arc.start_point(), arc.end_point())
        } else {
            (arc.end_point(), arc.start_point())
        };
        self.move_or_continue(first);
        Self::arc_to(&mut self.path, &self.placement, arc, forwards);
        self.current = Some(last);
    }

    fn triangle(&mut self, corners: [Point2; 3]) {
        let mut fill = String::new();
        let [a, b, c] = corners.map(|corner| self.placement.map(corner));
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

    /// A wipeout's mask is its boundary filled in the paper's white (#226),
    /// the look around it kept.
    fn mask(&mut self, wipeout: &WipeoutEntity) {
        let mut mask = String::from("q 1 1 1 rg\n");
        Self::ring(&mut mask, &self.placement, &wipeout.primitives());
        mask.push_str("f\nQ\n");
        self.masks.push(mask);
    }

    fn image(&mut self, image: &ImageQuad) {
        self.images.push(*image);
    }

    /// A solid hatch is its loops, filled by the even-odd rule: one path,
    /// no seams between triangles.
    fn hatch_fill(&mut self, hatch: &HatchEntity) {
        let mut fill = String::new();
        for path in &hatch.loops {
            Self::ring(&mut fill, &self.placement, &path.primitives());
        }
        if !fill.is_empty() {
            fill.push_str("f*\n");
            self.fills.push(fill);
        }
    }
}

/// The pictures a plot embeds (ADR-0042 §5): each definition drawn once
/// an image of the page's resources, made the first time it is drawn, and
/// each fade drawn a graphics state of them.
#[derive(Default)]
struct Pictures {
    /// By the definition's raw ID: its image, or `None` when the page
    /// cannot hold its picture.
    images: BTreeMap<u32, Option<Picture>>,
    /// The fades drawn, in percent.
    fades: BTreeSet<u8>,
}

/// One embedded picture: an image XObject.
struct Picture {
    /// Its dictionary, up to `/Length`.
    dictionary: String,
    /// Its bytes in base 85, ended by `~>`.
    data: String,
}

impl Pictures {
    /// The resource name of the picture of definition `def`, made the first
    /// time it is asked for; `None` when it is missing.
    fn name(&mut self, drawing: &Drawing, def: u32) -> Option<String> {
        self.images
            .entry(def)
            .or_insert_with(|| {
                drawing
                    .image_defs()
                    .get(ImageDefId::from_raw(def))
                    .and_then(picture)
            })
            .as_ref()
            .map(|_| image_name(def))
    }

    /// The stream drawing `image`, called `name`, at `placement`: the unit
    /// square the reader maps a picture to made its frame on the page —
    /// lower-left corner, bottom edge, left edge, the picture's first row
    /// at the top, as on screen — then, faded, painted over in the paper's
    /// white at the fade's opacity. The frame is the picture's edge, so
    /// nothing outside it is drawn.
    fn paint(&mut self, placement: &Placement, image: &ImageQuad, name: String) -> String {
        let corner = placement.map(image.corner);
        let across = placement.map(image.corner + image.across) - corner;
        let up = placement.map(image.corner + image.up) - corner;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "q\n{} {} {} {} {} {} cm\n/{name} Do",
            num(across.x),
            num(across.y),
            num(up.x),
            num(up.y),
            num(corner.x),
            num(corner.y)
        );
        if image.fade > 0 {
            self.fades.insert(image.fade);
            let _ = writeln!(out, "/{} gs 1 1 1 rg 0 0 1 1 re f", fade_name(image.fade));
        }
        out.push_str("Q\n");
        out
    }

    /// The pictures embedded, by their resource names.
    fn embedded(&self) -> impl Iterator<Item = (String, &Picture)> {
        self.images
            .iter()
            .filter_map(|(def, picture)| Some((image_name(*def), picture.as_ref()?)))
    }

    /// The page's resources: its images, by object number from `first`, and
    /// its fades.
    fn resources(&self, first: usize) -> String {
        let mut out = String::from("<<");
        let images: Vec<String> = self
            .embedded()
            .enumerate()
            .map(|(i, (name, _))| format!("/{name} {} 0 R", first + i))
            .collect();
        if !images.is_empty() {
            let _ = write!(out, " /XObject << {} >>", images.join(" "));
        }
        if !self.fades.is_empty() {
            let fades: Vec<String> = self
                .fades
                .iter()
                .map(|fade| {
                    format!(
                        "/{} << /ca {} >>",
                        fade_name(*fade),
                        num(f64::from(*fade) / 100.0)
                    )
                })
                .collect();
            let _ = write!(out, " /ExtGState << {} >>", fades.join(" "));
        }
        out.push_str(" >>");
        out
    }
}

fn image_name(def: u32) -> String {
    format!("Im{def}")
}

fn fade_name(fade: u8) -> String {
    format!("Fade{fade}")
}

/// The image a definition is embedded as: a JPEG's bytes as they are
/// (`DCTDecode`), a PNG's compressed samples with the predictor its rows
/// are filtered by (`FlateDecode`, `Predictor 15`), neither decoded here;
/// `None` when its picture is missing.
fn picture(def: &ImageDef) -> Option<Picture> {
    let colours = match def.components()? {
        1 => ("/DeviceGray", 1),
        _ => ("/DeviceRGB", 3),
    };
    let (filter, parameters, data) = match def.format {
        ImageFormat::Jpeg => ("/DCTDecode", String::new(), ascii85(&def.bytes)),
        ImageFormat::Png => (
            "/FlateDecode",
            format!(
                " /DecodeParms [null << /Predictor 15 /Colors {} /BitsPerComponent 8 /Columns {} >>]",
                colours.1, def.width
            ),
            ascii85(&def.png_samples()?),
        ),
    };
    Some(Picture {
        dictionary: format!(
            "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {} /BitsPerComponent 8 /Filter [/ASCII85Decode {filter}]{parameters}",
            def.width, def.height, colours.0
        ),
        data,
    })
}

/// `bytes` in base 85, as PDF's `ASCII85Decode` reads them: four bytes to
/// five characters from `!`, a group of four zeros `z`, the last group's
/// `n` bytes `n + 1` characters, in lines of 75, ended by `~>`.
pub fn ascii85(bytes: &[u8]) -> String {
    const LINE: usize = 75;
    let mut out = String::with_capacity(bytes.len() / 4 * 5 + bytes.len() / 60 + 8);
    let mut column = 0;
    for group in bytes.chunks(4) {
        let mut word = [0u8; 4];
        word[..group.len()].copy_from_slice(group);
        let mut value = u32::from_be_bytes(word);
        let mut digits = [0u8; 5];
        let count = if group.len() == 4 && value == 0 {
            digits[0] = b'z';
            1
        } else {
            for digit in digits.iter_mut().rev() {
                *digit = b'!' + (value % 85) as u8;
                value /= 85;
            }
            group.len() + 1
        };
        for digit in &digits[..count] {
            out.push(char::from(*digit));
            column += 1;
            if column == LINE {
                out.push('\n');
                column = 0;
            }
        }
    }
    out.push_str("~>");
    out
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
/// stream, the pictures it draws, and the information, with a
/// cross-reference table that points at each.
fn pdf_file(page: &Page, content: &str, pictures: &Pictures) -> String {
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
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Contents 4 0 R /Resources {} >>",
            num(page.width),
            num(page.height),
            // The pictures follow the content stream, object 4.
            pictures.resources(5)
        ),
    );
    object(
        &mut out,
        &format!(
            "<< /Length {} >>\nstream\n{content}endstream",
            content.len()
        ),
    );
    for (_, picture) in pictures.embedded() {
        object(
            &mut out,
            &format!(
                "{} /Length {} >>\nstream\n{}\nendstream",
                picture.dictionary,
                picture.data.len(),
                picture.data
            ),
        );
    }
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
