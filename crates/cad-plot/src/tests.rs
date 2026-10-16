//! What a plot carries (ADR-0027): the page, the scale, every entity kind,
//! the lineweights at their width, the colours, the dashes, the clip; and
//! the property that every primitive lands inside the page.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use cad_commands::{Paper, PlotArea, PlotScale, PlotSettings};
use cad_core::{Aabb, LengthUnit, Point2, Vec2};
use cad_document::{
    ArcEntity, Block, BlockReferenceEntity, CircleEntity, Color, DimensionEntity, Drawing,
    EllipseEntity, Entity, EntityRecord, HatchEntity, HatchStyle, Layer, LayoutId, LineEntity,
    Linetype, LinetypeDef, Lineweight, MTextEntity, Measure, Path, PolylineEntity, Properties,
    SplineEntity, TextEntity, ViewportEntity,
};
use proptest::prelude::*;

use super::*;

fn p(x: f64, y: f64) -> Point2 {
    Point2::new(x, y)
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
    LineEntity::new(p(x0, y0), p(x1, y1)).into()
}

fn settings() -> PlotSettings {
    PlotSettings::default()
}

/// A layout's plot, pulled apart (#194): what the paper draws, outside
/// any viewport, and what each viewport's block draws — the lines
/// between its `q` and `Q`, its clip first.
struct LayoutPlot {
    paper: String,
    viewports: Vec<String>,
}

fn pull_apart(content: &str) -> LayoutPlot {
    let mut plot = LayoutPlot {
        paper: String::new(),
        viewports: Vec::new(),
    };
    let mut depth = 0;
    for line in content.lines() {
        match line {
            "q" => {
                depth += 1;
                if depth == 2 {
                    plot.viewports.push(String::new());
                }
            }
            "Q" => depth -= 1,
            _ if depth >= 2 => {
                let block = plot.viewports.last_mut().expect("inside a block");
                block.push_str(line);
                block.push('\n');
            }
            _ => {
                plot.paper.push_str(line);
                plot.paper.push('\n');
            }
        }
    }
    plot
}

/// The rectangle a block's first line clips to.
fn clip_of(block: &str) -> Aabb {
    let first = block.lines().next().expect("a clip");
    let values: Vec<f64> = first
        .split_whitespace()
        .take(4)
        .map(|v| v.parse().expect("a clip number"))
        .collect();
    assert!(first.ends_with(" re W n"), "a clip first: {first}");
    Aabb::from_points(
        p(values[0], values[1]),
        p(values[0] + values[2], values[1] + values[3]),
    )
}

fn close(a: Point2, b: Point2) -> bool {
    (a.x - b.x).abs() < 0.002 && (a.y - b.y).abs() < 0.002
}

/// A drawing with a model line from the origin to (100, 0), a layout
/// with the sheet given, and the viewports onto the model.
fn layout_drawing(sheet: Option<Vec2>, viewports: &[ViewportEntity]) -> (Drawing, LayoutId) {
    let mut drawing = Drawing::from_entities([line(0.0, 0.0, 100.0, 0.0)]);
    let mut tx = drawing.transaction();
    let mut layout = cad_document::Layout::named("Sheet");
    if let Some(size) = sheet {
        layout = layout.with_sheet(size);
    }
    let sheet = tx.add_layout(layout).unwrap();
    for viewport in viewports {
        tx.insert_in(sheet, *viewport).unwrap();
    }
    tx.commit();
    (drawing, sheet)
}

/// The content stream of a plot: between `stream` and `endstream`.
fn content_of(pdf: &str) -> &str {
    let start = pdf.find("stream\n").expect("a stream") + "stream\n".len();
    let end = pdf.find("endstream").expect("an end");
    &pdf[start..end]
}

/// What a content stream draws: the points of its path operators, the
/// widths set, the dash arrays, and the stroke colours.
#[derive(Debug, Default)]
struct Ops {
    /// Every point a `m`, `l`, or `c` lands on or passes through — for a
    /// curve, samples along it.
    points: Vec<Point2>,
    widths: Vec<f64>,
    dashes: Vec<Vec<f64>>,
    strokes: Vec<[f64; 3]>,
    operators: Vec<String>,
}

fn parse(content: &str) -> Ops {
    let mut ops = Ops::default();
    let mut stack: Vec<f64> = Vec::new();
    let mut array: Option<Vec<f64>> = None;
    let mut current = Point2::ORIGIN;
    for token in content.split_whitespace() {
        if let Some(rest) = token.strip_prefix('[') {
            let (rest, closed) = match rest.strip_suffix(']') {
                Some(inner) => (inner, true),
                None => (rest, false),
            };
            let mut values = Vec::new();
            if !rest.is_empty() {
                values.push(rest.parse().expect("a number in a dash array"));
            }
            if closed {
                ops.dashes.push(values);
            } else {
                array = Some(values);
            }
            continue;
        }
        if let Some(rest) = token.strip_suffix(']') {
            let mut values = array.take().unwrap_or_default();
            if !rest.is_empty() {
                values.push(rest.parse().expect("a number in a dash array"));
            }
            ops.dashes.push(values);
            continue;
        }
        if let Some(values) = array.as_mut() {
            values.push(token.parse().expect("a number in a dash array"));
            continue;
        }
        if let Ok(value) = token.parse::<f64>() {
            stack.push(value);
            continue;
        }
        match token {
            "m" | "l" => {
                let point = p(stack[0], stack[1]);
                ops.points.push(point);
                current = point;
            }
            "c" => {
                let c1 = p(stack[0], stack[1]);
                let c2 = p(stack[2], stack[3]);
                let end = p(stack[4], stack[5]);
                for sample in 0..=8 {
                    let t = f64::from(sample) / 8.0;
                    let u = 1.0 - t;
                    let point = Point2::ORIGIN
                        + (current - Point2::ORIGIN) * (u * u * u)
                        + (c1 - Point2::ORIGIN) * (3.0 * u * u * t)
                        + (c2 - Point2::ORIGIN) * (3.0 * u * t * t)
                        + (end - Point2::ORIGIN) * (t * t * t);
                    ops.points.push(point);
                }
                current = end;
            }
            "w" => ops.widths.push(stack[0]),
            "RG" => ops.strokes.push([stack[0], stack[1], stack[2]]),
            _ => {}
        }
        ops.operators.push(token.to_owned());
        stack.clear();
    }
    ops
}

fn media_box(pdf: &str) -> Aabb {
    let start = pdf.find("/MediaBox [").expect("a media box") + "/MediaBox [".len();
    let end = start + pdf[start..].find(']').expect("a closed box");
    let values: Vec<f64> = pdf[start..end]
        .split_whitespace()
        .map(|value| value.parse().unwrap())
        .collect();
    Aabb::from_points(p(values[0], values[1]), p(values[2], values[3]))
}

#[test]
fn the_page_is_landscape_for_a_wide_area_and_fits_it_within_the_margins() {
    let drawing = Drawing::from_entities([line(0.0, 0.0, 400.0, 100.0)]);
    let page = Page::new(&drawing, &settings());
    assert!(page.landscape);
    assert!((page.width - 297.0 * POINTS_PER_MM).abs() < 1e-9);
    assert!((page.height - 210.0 * POINTS_PER_MM).abs() < 1e-9);
    // 400 units across 277 printable millimetres: the width decides.
    assert!((page.scale - 277.0 * POINTS_PER_MM / 400.0).abs() < 1e-9);
    let on_page = page.area_on_page();
    assert!((on_page.min().x - MARGIN_MM * POINTS_PER_MM).abs() < 1e-6);
    assert!((on_page.max().x - (297.0 - MARGIN_MM) * POINTS_PER_MM).abs() < 1e-6);
    assert!((on_page.center().y - page.height / 2.0).abs() < 1e-6);

    let tall = Drawing::from_entities([line(0.0, 0.0, 10.0, 50.0)]);
    let page = Page::new(&tall, &settings());
    assert!(!page.landscape);
    assert!((page.width - 210.0 * POINTS_PER_MM).abs() < 1e-9);
}

#[test]
fn a_ratio_scale_lands_lengths_at_their_paper_size() {
    let drawing = Drawing::from_entities([line(0.0, 0.0, 1000.0, 0.0)])
        .with_unit(Some(LengthUnit::Millimetres));
    let page = Page::new(
        &drawing,
        &PlotSettings {
            paper: Paper::A3,
            scale: PlotScale::Ratio(50.0),
            ..settings()
        },
    );
    // A metre at 1:50 is 20 mm on paper.
    assert!((page.length(1000.0) - 20.0 * POINTS_PER_MM).abs() < 1e-9);
    // In metres, the same line of one unit is a metre.
    let metres =
        Drawing::from_entities([line(0.0, 0.0, 1.0, 0.0)]).with_unit(Some(LengthUnit::Metres));
    let page = Page::new(
        &metres,
        &PlotSettings {
            scale: PlotScale::Ratio(50.0),
            ..settings()
        },
    );
    assert!((page.length(1.0) - 20.0 * POINTS_PER_MM).abs() < 1e-9);
    // A unitless drawing is taken in millimetres.
    let unitless = Drawing::from_entities([line(0.0, 0.0, 1000.0, 0.0)]);
    let page = Page::new(
        &unitless,
        &PlotSettings {
            scale: PlotScale::Ratio(50.0),
            ..settings()
        },
    );
    assert!((page.length(1000.0) - 20.0 * POINTS_PER_MM).abs() < 1e-9);
}

#[test]
fn an_empty_drawing_is_still_a_page() {
    let pdf = plot_pdf(&Drawing::new(), &settings());
    assert!(pdf.starts_with("%PDF-1.4\n"));
    assert!(pdf.ends_with("%%EOF\n"));
    assert!(!Page::new(&Drawing::new(), &settings()).landscape);
}

fn weighted(lineweight: Lineweight) -> Drawing {
    Drawing::from_parts(
        [],
        [],
        [(
            line(0.0, 0.0, 100.0, 0.0),
            Properties {
                lineweight,
                ..Properties::default()
            },
        )],
    )
    .unwrap()
}

#[test]
fn lineweights_measure_on_the_page_at_their_width() {
    let half = weighted(Lineweight::Hundredths(50));
    let ops = parse(content_of(&plot_pdf(&half, &settings())));
    assert_eq!(ops.widths, vec![1.417], "0.50 mm in points");
    // Whatever the scale: a lineweight is a width on paper.
    let ops = parse(content_of(&plot_pdf(
        &half,
        &PlotSettings {
            scale: PlotScale::Ratio(1000.0),
            ..settings()
        },
    )));
    assert_eq!(ops.widths, vec![1.417]);
    let default = weighted(Lineweight::Default);
    let ops = parse(content_of(&plot_pdf(&default, &settings())));
    assert_eq!(ops.widths, vec![0.709], "the default is 0.25 mm");
    // Lineweights not plotted: everything at the default.
    let ops = parse(content_of(&plot_pdf(
        &half,
        &PlotSettings {
            lineweights: false,
            ..settings()
        },
    )));
    assert_eq!(ops.widths, vec![0.709]);
}

fn coloured() -> Drawing {
    Drawing::from_parts(
        [Layer {
            color: Color::Index(5),
            ..Layer::named("blue")
        }],
        [],
        [
            (
                line(0.0, 0.0, 100.0, 0.0),
                Properties {
                    color: Color::Index(1),
                    ..Properties::default()
                },
            ),
            (
                line(0.0, 10.0, 100.0, 10.0),
                Properties::on(cad_document::LayerId::from_raw(1)),
            ),
            (line(0.0, 20.0, 100.0, 20.0), Properties::default()),
        ],
    )
    .unwrap()
}

#[test]
fn colours_are_the_palettes_and_the_foreground_is_black_or_everything_is() {
    let ops = parse(content_of(&plot_pdf(&coloured(), &settings())));
    assert_eq!(
        ops.strokes,
        vec![[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0]],
        "red by the object, blue by the layer, colour 7 black on paper"
    );
    let ops = parse(content_of(&plot_pdf(
        &coloured(),
        &PlotSettings {
            monochrome: true,
            ..settings()
        },
    )));
    assert_eq!(ops.strokes, vec![[0.0, 0.0, 0.0]], "one look for all three");
}

#[test]
fn a_dashed_linetype_becomes_a_dash_array_scaled_to_the_page_and_runs_on_through_a_polyline() {
    let dashed = LinetypeDef::built_in_named("DASHED").unwrap();
    let pattern = dashed.pattern.clone();
    let drawing = Drawing::from_parts(
        [],
        [dashed],
        [(
            Entity::Polyline(PolylineEntity::new(
                vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 50.0)],
                false,
            )),
            Properties {
                linetype: Linetype::Named(cad_document::LinetypeId::from_raw(1)),
                ..Properties::default()
            },
        )],
    )
    .unwrap();
    let plot = PlotSettings {
        scale: PlotScale::Ratio(2.0),
        ..settings()
    };
    let page = Page::new(&drawing, &plot);
    let ops = parse(content_of(&plot_pdf(&drawing, &plot)));
    let expected: Vec<f64> = pattern
        .iter()
        .map(|length| num(page.length(*length)).parse().unwrap())
        .collect();
    assert_eq!(ops.dashes, vec![expected]);
    // One `m`, two `l`: the polyline is one subpath, so the dashes run on.
    assert_eq!(
        ops.operators.iter().filter(|op| *op == "m").count(),
        1,
        "{:?}",
        ops.operators
    );
    assert_eq!(ops.operators.iter().filter(|op| *op == "l").count(), 2);
    // A text is plotted continuous whatever its linetype.
    let text = Drawing::from_parts(
        [],
        [LinetypeDef::built_in_named("DASHED").unwrap()],
        [(
            Entity::Text(TextEntity::new(p(0.0, 0.0), 10.0, 0.0, "Dash")),
            Properties {
                linetype: Linetype::Named(cad_document::LinetypeId::from_raw(1)),
                ..Properties::default()
            },
        )],
    )
    .unwrap();
    let ops = parse(content_of(&plot_pdf(&text, &settings())));
    assert_eq!(ops.dashes, vec![Vec::<f64>::new()]);
}

#[test]
fn a_window_clips_the_page_to_the_area() {
    let drawing = Drawing::from_entities([line(-1000.0, -1000.0, 1000.0, 1000.0)]);
    let window = Aabb::from_points(p(0.0, 0.0), p(100.0, 50.0));
    let plot = PlotSettings {
        area: PlotArea::Window(window),
        ..settings()
    };
    let pdf = plot_pdf(&drawing, &plot);
    let page = Page::new(&drawing, &plot);
    assert_eq!(page.area, window);
    let clip = page.area_on_page();
    let size = clip.max() - clip.min();
    assert!(pdf.contains(&format!(
        "{} {} {} {} re W n\n",
        num(clip.min().x),
        num(clip.min().y),
        num(size.x),
        num(size.y)
    )));
}

/// One of everything the drawing can hold.
fn everything() -> Drawing {
    let mut drawing =
        Drawing::from_parts([], [LinetypeDef::built_in_named("DASHED").unwrap()], []).unwrap();
    let mut tx = drawing.transaction();
    let part = tx
        .define_block(Block::new(
            "Part",
            Point2::ORIGIN,
            vec![EntityRecord::new(
                line(0.0, 0.0, 10.0, 10.0),
                Properties::default(),
            )],
        ))
        .unwrap();
    for entity in [
        line(0.0, 0.0, 100.0, 0.0),
        CircleEntity::new(p(150.0, 50.0), 30.0).into(),
        ArcEntity::new(p(250.0, 50.0), 30.0, 0.0, PI).into(),
        EllipseEntity::new(p(350.0, 50.0), Vec2::new(40.0, 0.0), 0.5).into(),
        SplineEntity::through(&[p(0.0, 100.0), p(50.0, 150.0), p(100.0, 100.0)])
            .unwrap()
            .into(),
        PolylineEntity::with_bulges(
            vec![p(150.0, 100.0), p(250.0, 100.0), p(250.0, 150.0)],
            vec![0.0, 1.0, 0.0],
            true,
        )
        .into(),
        TextEntity::new(p(300.0, 120.0), 10.0, 0.0, "Plot").into(),
        MTextEntity::new(p(300.0, 160.0), 80.0, 8.0, 0.0, "Two\nlines").into(),
        BlockReferenceEntity::new(part, p(0.0, 200.0), 2.0, FRAC_PI_2).into(),
        DimensionEntity::new(
            Measure::Linear {
                a: p(100.0, 200.0),
                b: p(200.0, 200.0),
                rotation: 0.0,
            },
            p(150.0, 230.0),
        )
        .into(),
        HatchEntity::solid(vec![Path {
            vertices: vec![
                p(300.0, 200.0),
                p(360.0, 200.0),
                p(360.0, 260.0),
                p(300.0, 260.0),
            ],
            bulges: vec![0.0, 0.0, 0.0, 0.0],
            closed: true,
        }])
        .into(),
        HatchEntity {
            loops: vec![Path {
                vertices: vec![p(400.0, 200.0), p(460.0, 200.0), p(460.0, 260.0)],
                bulges: vec![0.0, 0.5, 0.0],
                closed: true,
            }],
            style: HatchStyle::Pattern {
                name: "ANSI31".to_owned(),
                angle: 0.0,
                scale: 1.0,
                origin: Point2::ORIGIN,
            },
        }
        .into(),
    ] {
        tx.insert(entity).unwrap();
    }
    tx.commit();
    drawing
}

#[test]
fn every_entity_kind_lands_on_the_page() {
    let drawing = everything();
    let pdf = plot_pdf(&drawing, &settings());
    let content = content_of(&pdf);
    let ops = parse(content);
    assert!(content.contains(" c\n"), "arcs as curves");
    assert!(
        content.contains("f*\n"),
        "the solid hatch filled by even-odd"
    );
    assert!(
        content.contains("h f\n") || content.contains(" l\n"),
        "something stroked"
    );
    assert!(
        ops.points.len() > 200,
        "a text's strokes and a pattern's lines"
    );
    let page = media_box(&pdf);
    for point in &ops.points {
        assert!(page.contains_point(*point), "{point:?} is off the page");
    }
    // Twelve entities, some sharing a look: fewer width settings than entities.
    assert!(!ops.widths.is_empty());
}

#[test]
fn the_cross_reference_table_points_at_every_object() {
    let pdf = plot_pdf(&everything(), &settings());
    let xref_at: usize = pdf
        .rsplit("startxref\n")
        .next()
        .unwrap()
        .lines()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(pdf[xref_at..].starts_with("xref\n0 6\n"));
    let table = &pdf[xref_at + "xref\n0 6\n".len()..];
    for (index, entry) in table.lines().take(6).enumerate() {
        let offset: usize = entry[..10].parse().unwrap();
        if index == 0 {
            assert_eq!(entry, "0000000000 65535 f ");
        } else {
            assert!(
                pdf[offset..].starts_with(&format!("{index} 0 obj\n")),
                "object {index} at {offset}"
            );
        }
    }
    let length: usize = pdf
        .split("/Length ")
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(content_of(&pdf).len(), length);
    assert!(
        pdf.is_ascii(),
        "ASCII throughout: a text file to any reader"
    );
}

#[test]
fn numbers_are_short_and_plain() {
    assert_eq!(num(0.0), "0");
    assert_eq!(num(-0.0004), "0");
    assert_eq!(num(1.5), "1.5");
    assert_eq!(num(1234.56789), "1234.568");
    assert_eq!(num(1e-7), "0");
    assert_eq!(num(f64::NAN), "0");
}

#[test]
fn the_plot_is_described_for_the_log() {
    assert_eq!(describe(&settings()), "A4 to fit");
    assert_eq!(
        describe(&PlotSettings {
            paper: Paper::Letter,
            scale: PlotScale::Ratio(50.0),
            ..settings()
        }),
        "Letter at 1:50"
    );
}

/// Writes a plot of everything to `target/plot-sample.pdf`, for a reader
/// to open: `cargo test -p cad-plot -- --ignored sample`.
#[test]
#[ignore]
fn the_sample_plot_for_a_reader() {
    let pdf = plot_pdf(&everything(), &settings());
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/plot-sample.pdf");
    std::fs::write(&path, pdf).unwrap();
}

/// #206 (ADR-0031): a construction line plots clipped to the plotted
/// area — every point it draws is on the page within the margins — and
/// leaves the extents, and so the page's fit, to the rest.
#[test]
fn a_construction_line_plots_clipped_to_the_area_and_leaves_the_fit_alone() {
    let mut drawing = Drawing::from_entities([line(0.0, 0.0, 100.0, 50.0)]);
    let plain = Page::new(&drawing, &settings());
    let mut tx = drawing.transaction();
    tx.insert(cad_document::XLineEntity::through(p(0.0, 25.0), p(1.0, 25.0)).unwrap())
        .unwrap();
    tx.commit();
    let page = Page::new(&drawing, &settings());
    assert_eq!(page, plain, "the fit is the line's");
    let pdf = plot_pdf(&drawing, &settings());
    let ops = parse(content_of(&pdf));
    assert_eq!(
        ops.points.len(),
        4,
        "the line and the xline, two points each"
    );
    let margin = MARGIN_MM * POINTS_PER_MM;
    let media = media_box(&pdf);
    let printable = Aabb::from_points(
        media.min() + Vec2::new(margin, margin),
        media.max() - Vec2::new(margin, margin),
    )
    .expanded(0.01);
    for point in &ops.points {
        assert!(printable.contains_point(*point), "{point:?}");
    }
    // The xline spans the plotted area's width at y = 25.
    let mid = page.to_page(p(50.0, 25.0)).y;
    assert!(
        ops.points
            .iter()
            .filter(|point| (point.y - mid).abs() < 0.01)
            .count()
            == 2
    );
}

/// #194: a layout plots on its sheet at one to one — the title line where
/// it is on the paper, the sheet the page — and each viewport that is on
/// draws the model at its own scale, clipped to its frame; the frame
/// itself is not drawn, and a viewport that is off draws nothing.
#[test]
fn a_layout_plots_its_paper_at_one_to_one_and_the_model_through_each_viewport_at_its_scale() {
    let full = ViewportEntity::new(p(100.0, 150.0), 150.0, 100.0, p(50.0, 0.0), 1.0);
    let half = ViewportEntity::new(p(300.0, 150.0), 100.0, 100.0, p(50.0, 0.0), 0.5);
    let mut off = ViewportEntity::new(p(300.0, 40.0), 50.0, 50.0, p(50.0, 0.0), 2.0);
    off.on = false;
    let (mut drawing, sheet) = layout_drawing(None, &[full, half, off]);
    let mut tx = drawing.transaction();
    tx.insert_in(sheet, line(10.0, 10.0, 200.0, 10.0)).unwrap();
    tx.commit();

    let pdf = plot_pdf_in(&drawing, sheet, &settings());
    let mm = POINTS_PER_MM;
    assert!(
        close(media_box(&pdf).max(), p(420.0 * mm, 297.0 * mm)),
        "the sheet is the page"
    );
    let plot = pull_apart(content_of(&pdf));
    let paper = parse(&plot.paper);
    assert_eq!(paper.points.len(), 2, "the title line alone: no frames");
    assert!(close(paper.points[0], p(10.0 * mm, 10.0 * mm)));
    assert!(close(paper.points[1], p(200.0 * mm, 10.0 * mm)));
    assert_eq!(plot.viewports.len(), 2, "the one that is off draws nothing");

    let first = clip_of(&plot.viewports[0]);
    assert!(close(first.min(), p(25.0 * mm, 100.0 * mm)));
    assert!(close(first.max(), p(175.0 * mm, 200.0 * mm)));
    let ops = parse(&plot.viewports[0]);
    assert_eq!(ops.points.len(), 2);
    assert!(
        close(ops.points[0], p(50.0 * mm, 150.0 * mm)),
        "{:?}",
        ops.points
    );
    assert!(close(ops.points[1], p(150.0 * mm, 150.0 * mm)));

    let second = clip_of(&plot.viewports[1]);
    assert!(close(second.min(), p(250.0 * mm, 100.0 * mm)));
    assert!(close(second.max(), p(350.0 * mm, 200.0 * mm)));
    let ops = parse(&plot.viewports[1]);
    assert_eq!(ops.points.len(), 2);
    assert!(
        close(ops.points[0], p(275.0 * mm, 150.0 * mm)),
        "{:?}",
        ops.points
    );
    assert!(close(ops.points[1], p(325.0 * mm, 150.0 * mm)));
    // The look is set afresh inside each block: a stroke colour each.
    assert_eq!(ops.strokes.len(), 1);

    // The model's own plot is as it was: one line, no blocks.
    let model = pull_apart(content_of(&plot_pdf(&drawing, &settings())));
    assert!(model.viewports.is_empty());
    assert_eq!(parse(&model.paper).points.len(), 2);
}

/// #194, #208: the page is the sheet the layout's row says — set here,
/// or A3 landscape for one made here — whatever paper the settings name;
/// a layout made here plots its whole sheet at one to one, printed within
/// its margins.
#[test]
fn a_layouts_page_is_its_sheet_at_its_own_scale_within_its_margins() {
    let (drawing, sheet) = layout_drawing(Some(Vec2::new(210.0, 297.0)), &[]);
    let plot = PlotSettings {
        paper: Paper::A0,
        ..settings()
    };
    let page = Page::in_space(&drawing, sheet, &plot);
    assert!(!page.landscape);
    assert!((page.width - 210.0 * POINTS_PER_MM).abs() < 1e-9);
    assert!((page.height - 297.0 * POINTS_PER_MM).abs() < 1e-9);
    assert!((page.scale - POINTS_PER_MM).abs() < 1e-9, "one to one");
    assert!(
        close(page.to_page(Point2::ORIGIN), Point2::ORIGIN),
        "where it lies"
    );
    // Printed within the margins a layout made here has: 7.5, 20, 7.5, 20.
    let clip = page.clip();
    assert!(close(
        clip.min(),
        p(7.5 * POINTS_PER_MM, 20.0 * POINTS_PER_MM)
    ));
    assert!(close(
        clip.max(),
        p(202.5 * POINTS_PER_MM, 277.0 * POINTS_PER_MM)
    ));
    let pdf = plot_pdf_in(&drawing, sheet, &plot);
    assert!(pdf.contains(&format!(
        "{} {} {} {} re W n\n",
        num(clip.min().x),
        num(clip.min().y),
        num(clip.max().x - clip.min().x),
        num(clip.max().y - clip.min().y)
    )));
    assert_eq!(
        Page::in_space(&drawing, LayoutId::MODEL, &plot),
        Page::new(&drawing, &plot)
    );
}

/// #208: a layout set to plot a window, its extents, at a scale, to fit,
/// or turned, plots so; the settings travel on the row.
#[test]
fn a_layouts_plot_settings_shape_the_page() {
    use cad_document::{LayoutArea, LayoutPlot};
    let set = |plot: LayoutPlot| {
        let (mut drawing, sheet) = layout_drawing(Some(Vec2::new(297.0, 210.0)), &[]);
        let mut tx = drawing.transaction();
        tx.insert_in(sheet, line(100.0, 100.0, 140.0, 120.0))
            .unwrap();
        tx.set_layout_plot(sheet, plot).unwrap();
        tx.commit();
        let page = Page::in_space(&drawing, sheet, &settings());
        (drawing, sheet, page)
    };
    // Half scale: a paper millimetre per two units.
    let (_, _, page) = set(LayoutPlot {
        scale: Some(0.5),
        ..LayoutPlot::default()
    });
    assert!((page.scale - POINTS_PER_MM / 2.0).abs() < 1e-9);
    // A window, fitted into the printable area: 282 × 170 mm of room.
    let window = Aabb::from_points(p(100.0, 100.0), p(140.0, 120.0));
    let (_, _, page) = set(LayoutPlot {
        area: LayoutArea::Window(window),
        scale: None,
        turns: 0,
    });
    assert_eq!(page.area, window);
    let fit = (282.0 / 40.0_f64).min(170.0 / 20.0) * POINTS_PER_MM;
    assert!((page.scale - fit).abs() < 1e-9, "{} {fit}", page.scale);
    let printable = page.printable.unwrap();
    assert!(
        close(page.to_page(window.center()), printable.center()),
        "centred"
    );
    // The extents: what is on the paper.
    let (_, _, page) = set(LayoutPlot {
        area: LayoutArea::Extents,
        scale: Some(1.0),
        turns: 0,
    });
    assert_eq!(page.area, window, "the line is all there is");
    // A quarter turn: the page is held the other way, and the sheet's
    // corners go round counter-clockwise.
    let (drawing, sheet, page) = set(LayoutPlot {
        turns: 1,
        ..LayoutPlot::default()
    });
    assert!((page.width - 210.0 * POINTS_PER_MM).abs() < 1e-9);
    assert!((page.height - 297.0 * POINTS_PER_MM).abs() < 1e-9);
    assert!(close(
        page.to_page(Point2::ORIGIN),
        p(210.0 * POINTS_PER_MM, 0.0)
    ));
    assert!(close(
        page.to_page(p(297.0, 210.0)),
        p(0.0, 297.0 * POINTS_PER_MM)
    ));
    let pdf = plot_pdf_in(&drawing, sheet, &settings());
    assert!(pdf.contains("/MediaBox [0 0 595.276 841.89]"));
    let ops = parse(&pull_apart(content_of(&pdf)).paper);
    // The line from (100, 100) to (140, 120) turned: x = 210 − y, y = x.
    assert!(
        close(
            ops.points[0],
            p(110.0 * POINTS_PER_MM, 100.0 * POINTS_PER_MM)
        ),
        "{:?}",
        ops.points
    );
    assert!(close(
        ops.points[1],
        p(90.0 * POINTS_PER_MM, 140.0 * POINTS_PER_MM)
    ));
}

/// #208: a layout's plot settings as a file gives them — the margins
/// (40–43), the scale (142 over 143, or a fit), the plot type and window
/// (74; 48, 49, 140, 141), the rotation (73) — as the layouts fixture's
/// `Layout1` has them: a whole A3 sheet at one to one within margins of
/// 7.5, 20, 7.5, 20.
#[test]
fn a_layouts_kept_plot_settings_are_read_from_its_groups() {
    use cad_document::{Layout, LayoutArea, LayoutPlot};
    let groups = |pairs: &[(i32, &str)]| -> Vec<(i32, String)> {
        pairs
            .iter()
            .map(|(code, value)| (*code, (*value).to_owned()))
            .collect()
    };
    let fixture = Layout {
        kept: groups(&[
            (40, "7.5"),
            (41, "20.0"),
            (42, "7.5"),
            (43, "20.0"),
            (44, "420.0"),
            (45, "297.0"),
            (48, "0.0"),
            (49, "0.0"),
            (140, "0.0"),
            (141, "0.0"),
            (142, "1.0"),
            (143, "1.0"),
            (70, "0"),
            (73, "0"),
            (74, "5"),
            (75, "16"),
        ]),
        ..Layout::named("Layout1")
    };
    assert_eq!(fixture.plot_settings(), LayoutPlot::default());
    let page = Page::of_layout(&fixture, None);
    assert!((page.width - 420.0 * POINTS_PER_MM).abs() < 1e-9);
    assert!((page.scale - POINTS_PER_MM).abs() < 1e-9);
    assert!(close(
        page.clip().min(),
        p(7.5 * POINTS_PER_MM, 20.0 * POINTS_PER_MM)
    ));
    let turned_window = Layout {
        kept: groups(&[
            (44, "420.0"),
            (45, "297.0"),
            (48, "10.0"),
            (49, "20.0"),
            (140, "110.0"),
            (141, "70.0"),
            (142, "1.0"),
            (143, "4.0"),
            (70, "0"),
            (73, "3"),
            (74, "4"),
            (75, "0"),
        ]),
        ..Layout::named("Detail")
    };
    let plot = turned_window.plot_settings();
    assert_eq!(
        plot.area,
        LayoutArea::Window(Aabb::from_points(p(10.0, 20.0), p(110.0, 70.0)))
    );
    assert_eq!(plot.scale, Some(0.25));
    assert_eq!(plot.turns, 3);
    let fit = Layout {
        kept: groups(&[
            (44, "420.0"),
            (45, "297.0"),
            (70, "16"),
            (74, "1"),
            (75, "0"),
        ]),
        ..Layout::named("Fit")
    };
    assert_eq!(fit.plot_settings().scale, None);
    assert_eq!(fit.plot_settings().area, LayoutArea::Extents);
}

fn any_entity() -> impl Strategy<Value = Entity> {
    let coordinate = -500.0..500.0;
    let radius = 1.0..200.0;
    prop_oneof![
        (
            coordinate.clone(),
            coordinate.clone(),
            coordinate.clone(),
            coordinate.clone()
        )
            .prop_map(|(x0, y0, x1, y1)| line(x0, y0, x1, y1)),
        (coordinate.clone(), coordinate.clone(), radius.clone())
            .prop_map(|(x, y, r)| CircleEntity::new(p(x, y), r).into()),
        (
            coordinate.clone(),
            coordinate.clone(),
            radius.clone(),
            0.0..TAU,
            0.1..TAU
        )
            .prop_map(|(x, y, r, start, sweep)| ArcEntity::new(
                p(x, y),
                r,
                start,
                start + sweep
            )
            .into()),
        (
            prop::collection::vec((coordinate.clone(), coordinate.clone(), -1.0..1.0), 2..6),
            any::<bool>()
        )
            .prop_map(|(vertices, closed)| {
                let bulges = vertices.iter().map(|(_, _, bulge)| *bulge).collect();
                PolylineEntity::with_bulges(
                    vertices.into_iter().map(|(x, y, _)| p(x, y)).collect(),
                    bulges,
                    closed,
                )
                .into()
            }),
        (coordinate.clone(), coordinate, 1.0..50.0, 0.0..TAU)
            .prop_map(|(x, y, height, angle)| TextEntity::new(p(x, y), height, angle, "Ag").into()),
    ]
}

proptest! {
    /// Every point a plot draws is on the page, within its margins, at
    /// any scale that fits — the page is laid out from the extents, and
    /// the extents hold every primitive.
    #[test]
    fn every_primitive_lands_inside_the_page(
        entities in prop::collection::vec(any_entity(), 1..8),
        paper in prop::sample::select(Paper::ALL.to_vec()),
    ) {
        let drawing = Drawing::from_entities(entities);
        let plot = PlotSettings { paper, ..settings() };
        let pdf = plot_pdf(&drawing, &plot);
        let page = media_box(&pdf);
        let margin = MARGIN_MM * POINTS_PER_MM;
        // A quarter-turn Bézier bulges 0.027% of its radius outside the
        // arc, and an arc can be as large as the page: on A0 that is most
        // of a point, so the slack is that share of the page's size.
        let slack = 0.5_f64.max(3.0e-4 * (page.max().x - page.min().x).max(page.max().y - page.min().y));
        let printable = Aabb::from_points(
            page.min() + Vec2::new(margin, margin),
            page.max() - Vec2::new(margin, margin),
        )
        .expanded(slack);
        let ops = parse(content_of(&pdf));
        prop_assert!(!ops.points.is_empty());
        for point in &ops.points {
            prop_assert!(printable.contains_point(*point), "{point:?} outside {printable:?}");
        }
    }

    /// #194: on a layout, every point the paper draws is on the sheet —
    /// the page — and every viewport that is on draws the model inside a
    /// clip to its frame, on the page, with every point it draws a model
    /// point at the viewport's scale: mapped back, it is inside the
    /// model's extents.
    #[test]
    fn every_primitive_of_a_layout_lands_on_the_sheet_and_the_model_through_each_viewport(
        model in prop::collection::vec(any_entity(), 1..6),
        paper in prop::collection::vec((0.0..420.0, 0.0..297.0, 0.0..420.0, 0.0..297.0), 1..4),
        viewports in prop::collection::vec(
            (0.0..320.0, 0.0..197.0, 20.0..100.0, 20.0..100.0, -500.0..500.0, -500.0..500.0, 0.05..5.0, any::<bool>()),
            0..4,
        ),
    ) {
        let windows: Vec<ViewportEntity> = viewports
            .iter()
            .map(|&(x, y, w, h, vx, vy, scale, on)| {
                let mut viewport =
                    ViewportEntity::new(p(x + w / 2.0, y + h / 2.0), w, h, p(vx, vy), scale);
                viewport.on = on;
                viewport
            })
            .collect();
        let mut drawing = Drawing::from_entities(model);
        let extents = drawing.extents().expect("a model with something in it");
        let mut tx = drawing.transaction();
        let sheet = tx.add_layout(cad_document::Layout::named("Sheet")).unwrap();
        for window in &windows {
            tx.insert_in(sheet, *window).unwrap();
        }
        for (x0, y0, x1, y1) in paper {
            tx.insert_in(sheet, line(x0, y0, x1, y1)).unwrap();
        }
        tx.commit();

        let pdf = plot_pdf_in(&drawing, sheet, &settings());
        let media = media_box(&pdf);
        prop_assert!(close(media.max(), p(420.0 * POINTS_PER_MM, 297.0 * POINTS_PER_MM)));
        let page = media.expanded(0.5);
        let plot = pull_apart(content_of(&pdf));
        let paper = parse(&plot.paper);
        prop_assert!(!paper.points.is_empty());
        for point in &paper.points {
            prop_assert!(page.contains_point(*point), "{point:?} off the sheet {page:?}");
        }
        let on: Vec<&ViewportEntity> = windows.iter().filter(|w| w.on).collect();
        prop_assert_eq!(plot.viewports.len(), on.len());
        let laid = Page::in_space(&drawing, sheet, &settings());
        for (block, viewport) in plot.viewports.iter().zip(on) {
            let clip = clip_of(block);
            prop_assert!(page.contains_point(clip.min()) && page.contains_point(clip.max()));
            let frame = viewport.frame();
            prop_assert!(close(clip.min(), laid.to_page(frame.min())));
            prop_assert!(close(clip.max(), laid.to_page(frame.max())));
            let through = laid.through(viewport);
            // Rounding to a thousandth of a point, and a quarter-turn
            // Bézier's bulge, back in model units.
            let within = extents.expanded(1.0 / through.scale + 0.1);
            let ops = parse(block);
            prop_assert!(!ops.points.is_empty());
            for point in &ops.points {
                let model = through.unmap(*point);
                prop_assert!(within.contains_point(model), "{model:?} outside {within:?}");
            }
        }
    }
}

/// #226 (ADR-0032): the page is painted in the drawing order — a line
/// brought to the front is painted last — and a wipeout's mask is the
/// paper's white, painted over what is beneath it and under its frame.
#[test]
fn the_page_is_painted_in_the_drawing_order_and_a_wipeout_masks_in_white() {
    let mut drawing = coloured();
    let red = drawing
        .records()
        .find(|(_, record)| record.properties.color == Color::Index(1))
        .map(|(id, _)| id)
        .unwrap();
    let mut tx = drawing.transaction();
    tx.set_order(red, 1).unwrap();
    tx.commit();
    let ops = parse(content_of(&plot_pdf(&drawing, &settings())));
    assert_eq!(
        ops.strokes,
        vec![[0.0, 0.0, 1.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]],
        "the red line last, over the others"
    );

    let mut tx = drawing.transaction();
    tx.insert_on_top(
        cad_document::WipeoutEntity::new(vec![p(10.0, -5.0), p(60.0, -5.0), p(60.0, 25.0)])
            .unwrap(),
    )
    .unwrap();
    tx.commit();
    let pdf = plot_pdf(&drawing, &settings());
    let content = content_of(&pdf);
    let mask = content.find("q 1 1 1 rg\n").expect("a white mask");
    let red_line = content.find("1 0 0 RG").expect("the red line");
    assert!(red_line < mask, "the mask over what is beneath it");
    let frame = content[mask..].find("S\n").expect("its frame, stroked");
    assert!(
        content[mask..mask + frame].contains("f\nQ\n"),
        "the mask, then the frame"
    );
    let page = media_box(&pdf);
    for point in &parse(content).points {
        assert!(page.contains_point(*point), "{point:?} is off the page");
    }
}
