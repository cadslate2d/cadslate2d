//! DXF export (ADR-0005: ASCII DXF, release 12).
//!
//! The file has a HEADER with the drawing's settings, TABLES with its
//! linetypes, text styles, and layers, BLOCKS with its block definitions
//! when it has any, and ENTITIES, each with its layer and — when they are
//! not its layer's — its colour and linetype. R12 has no lineweights:
//! those are named in the [`ExportReport`], never silently dropped (§24.2).

use std::fmt::Write;

use cad_core::{EntityId, LengthUnit};
use cad_document::{
    ArcEntity, BlockId, BlockReferenceEntity, BlockTable, CircleEntity, Color, Drawing,
    DrawingSettings, Entity, EntityRecord, Justification, LINE_SPACING, Layer, LineEntity,
    Linetype, LinetypeDef, LinetypeTable, Lineweight, PolylineEntity, Properties, TextEntity,
    TextStyle, TextStyleTable,
};

use crate::properties::{
    LAYER_LOCKED, LINETYPE_ALIGNMENT_A, LINETYPE_BY_BLOCK, LINETYPE_BY_LAYER, color_to_dxf,
    pattern_to_dxf,
};

/// The DXF release written, as stored in `$ACADVER`. R12 is the simplest
/// release every CAD tool reads and needs no handles or subclass markers.
const ACADVER_R12: &str = "AC1009";

/// What an export could not write (§24.2): the baseline release has no
/// lineweights, so every one that is not `ByLayer` on an object, or not
/// `Default` on a layer, is lost.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExportReport {
    /// Entities with a lineweight of their own.
    pub lineweights_on_entities: usize,
    /// Layers with a lineweight other than the default.
    pub lineweights_on_layers: usize,
}

impl ExportReport {
    /// Whether the file holds less than the drawing did.
    pub fn is_lossy(&self) -> bool {
        self.lineweights_on_entities + self.lineweights_on_layers > 0
    }

    /// A short, user-facing account of what was lost; empty when nothing was.
    pub fn summary(&self) -> Vec<String> {
        if !self.is_lossy() {
            return Vec::new();
        }
        let counted = |count: usize, one: &str, many: &str| {
            format!("{count} {}", if count == 1 { one } else { many })
        };
        let carriers: Vec<String> = [
            (self.lineweights_on_entities, "object", "objects"),
            (self.lineweights_on_layers, "layer", "layers"),
        ]
        .into_iter()
        .filter(|(count, ..)| *count > 0)
        .map(|(count, one, many)| counted(count, one, many))
        .collect();
        vec![format!(
            "Lineweights are not written in DXF R12: dropped from {}.",
            carriers.join(" and ")
        )]
    }
}

/// The file and the account of what it could not hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Export {
    pub text: String,
    pub report: ExportReport,
}

/// Serialises the drawing as ASCII DXF.
///
/// Output is deterministic: entities are written in `EntityId` order, and
/// coordinates use the shortest decimal form that reads back to exactly the
/// same `f64`.
pub fn export(drawing: &Drawing) -> String {
    export_with_report(drawing).text
}

/// Serialises the drawing as ASCII DXF, and says what the file could not hold.
pub fn export_with_report(drawing: &Drawing) -> Export {
    let mut out = String::new();
    let mut report = ExportReport::default();
    let linetypes = drawing.linetypes();

    section(&mut out, "HEADER", |out| {
        group(out, 9, "$ACADVER");
        group(out, 1, ACADVER_R12);
        // The drawing unit. `$INSUNITS` is newer than R12; it is written
        // all the same so that a drawing keeps its unit when saved and
        // reopened (ADR-0005, amended). A unitless drawing writes nothing,
        // and so stays a plain R12 file.
        if let Some(unit) = drawing.unit() {
            group(out, 9, "$INSUNITS");
            group(out, 70, &insunits_code(unit).to_string());
        }
        // The settings for new objects, when they are not a new drawing's.
        let settings = drawing.settings();
        let defaults = DrawingSettings::default();
        if settings.current_layer != defaults.current_layer
            && let Some(layer) = drawing.layers().get(settings.current_layer)
        {
            group(out, 9, "$CLAYER");
            group(out, 8, &layer.name);
        }
        if settings.current_color != defaults.current_color {
            group(out, 9, "$CECOLOR");
            group(out, 62, &color_to_dxf(settings.current_color).to_string());
        }
        if settings.current_linetype != defaults.current_linetype {
            group(out, 9, "$CELTYPE");
            group(out, 6, &linetype_name(settings.current_linetype, linetypes));
        }
        // Exactly the default is left out; any other value is written as it is.
        #[allow(clippy::float_cmp)]
        if settings.ltscale != defaults.ltscale {
            group(out, 9, "$LTSCALE");
            real(out, 40, settings.ltscale);
        }
    });
    section(&mut out, "TABLES", |out| {
        table(out, "LTYPE", linetypes.len(), |out| {
            for (_, def) in linetypes.iter() {
                write_linetype(out, def);
            }
        });
        table(out, "STYLE", drawing.text_styles().len(), |out| {
            for (_, style) in drawing.text_styles().iter() {
                write_text_style(out, style);
            }
        });
        table(out, "LAYER", drawing.layers().len(), |out| {
            for (_, layer) in drawing.layers().iter() {
                write_layer(out, layer, linetypes);
                if layer.lineweight != Lineweight::Default {
                    report.lineweights_on_layers += 1;
                }
            }
        });
    });
    // Block definitions, in ID order, each with its entities; a drawing
    // without any writes no section, and so stays the file it was.
    let blocks = drawing.blocks();
    if blocks.len() > 1 {
        section(&mut out, "BLOCKS", |out| {
            for (_, block) in blocks.iter().filter(|(id, _)| *id != BlockId::FIRST) {
                group(out, 0, "BLOCK");
                group(out, 8, Layer::ZERO);
                group(out, 2, &block.name);
                group(out, 70, "0");
                real(out, 10, block.base.x);
                real(out, 20, block.base.y);
                group(out, 3, &block.name);
                for record in block.entities.iter() {
                    write_record(out, record, drawing, &mut report);
                }
                group(out, 0, "ENDBLK");
                group(out, 8, Layer::ZERO);
            }
        });
    }
    section(&mut out, "ENTITIES", |out| {
        let mut records: Vec<(EntityId, &EntityRecord)> = drawing.records().collect();
        records.sort_unstable_by_key(|(id, _)| *id);
        for (_, record) in records {
            write_record(out, record, drawing, &mut report);
        }
    });
    group(&mut out, 0, "EOF");
    Export { text: out, report }
}

/// One entity with its properties, as a record of ENTITIES or of a block.
fn write_record(
    out: &mut String,
    record: &EntityRecord,
    drawing: &Drawing,
    report: &mut ExportReport,
) {
    let linetypes = drawing.linetypes();
    let properties = &record.properties;
    let layer = drawing
        .layers()
        .get(properties.layer)
        .map_or(Layer::ZERO, |layer| layer.name.as_str());
    if properties.lineweight != Lineweight::ByLayer {
        report.lineweights_on_entities += 1;
    }
    let common = |out: &mut String| write_properties(out, layer, properties, linetypes);
    match &*record.entity {
        Entity::Line(line) => write_line(out, line, common),
        Entity::Circle(circle) => write_circle(out, circle, common),
        Entity::Arc(arc) => write_arc(out, arc, common),
        Entity::Polyline(polyline) => write_polyline(out, polyline, layer, common),
        Entity::Text(text) => write_text(out, text, drawing.text_styles(), common),
        Entity::BlockRef(reference) => write_insert(out, reference, drawing.blocks(), common),
        // R12 has no MTEXT: a paragraph is written as one TEXT per
        // line, as it is set.
        Entity::MText(text) => {
            let lines = text.lines();
            for (index, line) in lines.iter().enumerate() {
                let start = text.insertion
                    - text_up(text.rotation) * (text.height * (1.0 + LINE_SPACING * index as f64));
                let single = TextEntity::new(start, text.height, text.rotation, line.clone());
                write_text(out, &single, drawing.text_styles(), |out| {
                    write_properties(out, layer, properties, linetypes);
                });
            }
        }
    }
}

/// Group 6 for a linetype: `BYLAYER`, `BYBLOCK`, or the row's name.
fn linetype_name(linetype: Linetype, table: &LinetypeTable) -> String {
    match linetype {
        Linetype::ByLayer => LINETYPE_BY_LAYER.to_owned(),
        Linetype::ByBlock => LINETYPE_BY_BLOCK.to_owned(),
        Linetype::Named(id) => table.get(id).map_or_else(
            || LinetypeDef::CONTINUOUS.to_owned(),
            |def| def.name.clone(),
        ),
    }
}

/// The groups every entity carries after its type: its layer, and its
/// colour and linetype when they are its own. Lineweights have no group
/// in R12.
fn write_properties(out: &mut String, layer: &str, properties: &Properties, table: &LinetypeTable) {
    group(out, 8, layer);
    if properties.linetype != Linetype::ByLayer {
        group(out, 6, &linetype_name(properties.linetype, table));
    }
    if properties.color != Color::ByLayer {
        group(out, 62, &color_to_dxf(properties.color).to_string());
    }
}

/// An LTYPE row: name, flags, description, alignment, then the pattern as
/// its count, its length, and each element with gaps negative.
fn write_linetype(out: &mut String, def: &LinetypeDef) {
    group(out, 0, "LTYPE");
    group(out, 2, &def.name);
    group(out, 70, "0");
    group(out, 3, &def.description);
    group(out, 72, &LINETYPE_ALIGNMENT_A.to_string());
    group(out, 73, &def.pattern.len().to_string());
    real(out, 40, def.period());
    for element in pattern_to_dxf(&def.pattern) {
        real(out, 49, element);
    }
}

/// A STYLE row: name, flags, no fixed height, a width factor of one, no
/// oblique angle, and the stroke font every style draws with.
fn write_text_style(out: &mut String, style: &TextStyle) {
    group(out, 0, "STYLE");
    group(out, 2, &style.name);
    group(out, 70, "0");
    real(out, 40, 0.0);
    real(out, 41, 1.0);
    real(out, 50, 0.0);
    group(out, 71, "0");
    real(out, 42, 2.5);
    group(out, 3, "txt");
}

/// A LAYER row: name, flags, colour — negative when the layer is off, the
/// R12 way — and linetype.
fn write_layer(out: &mut String, layer: &Layer, table: &LinetypeTable) {
    group(out, 0, "LAYER");
    group(out, 2, &layer.name);
    group(
        out,
        70,
        &(if layer.locked { LAYER_LOCKED } else { 0 }).to_string(),
    );
    let color = match layer.color {
        Color::Index(index) => i64::from(index),
        // A layer's colour is what ByLayer means: an indirection here is
        // written as the colour a new layer has.
        Color::ByLayer | Color::ByBlock => color_to_dxf(Layer::DEFAULT_COLOR),
    };
    group(
        out,
        62,
        &(if layer.on { color } else { -color }).to_string(),
    );
    let linetype = match layer.linetype {
        Linetype::Named(_) => layer.linetype,
        Linetype::ByLayer | Linetype::ByBlock => Linetype::CONTINUOUS,
    };
    group(out, 6, &linetype_name(linetype, table));
}

/// The `$INSUNITS` code of a unit.
pub(crate) const fn insunits_code(unit: LengthUnit) -> i64 {
    match unit {
        LengthUnit::Inches => 1,
        LengthUnit::Feet => 2,
        LengthUnit::Millimetres => 4,
        LengthUnit::Centimetres => 5,
        LengthUnit::Metres => 6,
    }
}

fn write_line(out: &mut String, line: &LineEntity, properties: impl FnOnce(&mut String)) {
    group(out, 0, "LINE");
    properties(out);
    real(out, 10, line.start.x);
    real(out, 20, line.start.y);
    real(out, 11, line.end.x);
    real(out, 21, line.end.y);
}

fn write_circle(out: &mut String, circle: &CircleEntity, properties: impl FnOnce(&mut String)) {
    group(out, 0, "CIRCLE");
    properties(out);
    real(out, 10, circle.center.x);
    real(out, 20, circle.center.y);
    real(out, 40, circle.radius);
}

/// DXF arcs run counter-clockwise from the start angle to the end angle, in
/// degrees — the same convention as the document, in different units.
fn write_arc(out: &mut String, arc: &ArcEntity, properties: impl FnOnce(&mut String)) {
    group(out, 0, "ARC");
    properties(out);
    real(out, 10, arc.center.x);
    real(out, 20, arc.center.y);
    real(out, 40, arc.radius);
    real(out, 50, arc.start_angle.to_degrees());
    real(out, 51, arc.end_angle.to_degrees());
}

/// The unit vector up from a baseline at `rotation`.
fn text_up(rotation: f64) -> cad_core::Vec2 {
    cad_core::Vec2::new(-rotation.sin(), rotation.cos())
}

/// A TEXT: its insertion point, height, content, rotation, style, and —
/// for centred and right-justified text — the horizontal justification
/// (group 72) with the alignment point in groups 11 and 21, which is where
/// the text is justified to. A left-justified text writes neither.
fn write_text(
    out: &mut String,
    text: &TextEntity,
    styles: &TextStyleTable,
    properties: impl FnOnce(&mut String),
) {
    group(out, 0, "TEXT");
    properties(out);
    // For a justified text the first point is the baseline start, as DXF
    // readers expect, and the alignment point is the insertion.
    let start = text.start();
    real(out, 10, start.x);
    real(out, 20, start.y);
    real(out, 40, text.height);
    group(out, 1, &text.content);
    if text.rotation != 0.0 {
        real(out, 50, text.rotation.to_degrees());
    }
    let style = styles
        .get(text.style)
        .map_or(TextStyle::STANDARD, |style| style.name.as_str());
    if style != TextStyle::STANDARD {
        group(out, 7, style);
    }
    let justification = match text.justification {
        Justification::Left => None,
        Justification::Center => Some(1),
        Justification::Right => Some(2),
    };
    if let Some(code) = justification {
        group(out, 72, &code.to_string());
        real(out, 11, text.insertion.x);
        real(out, 21, text.insertion.y);
    }
}

/// An INSERT: the block by name, the insertion point, the scale along X,
/// Y, and Z when it is not one — a mirrored reference has its Y scale
/// negated, which is how DXF says it — and the rotation in degrees when
/// it is not zero.
fn write_insert(
    out: &mut String,
    reference: &BlockReferenceEntity,
    blocks: &BlockTable,
    properties: impl FnOnce(&mut String),
) {
    group(out, 0, "INSERT");
    properties(out);
    let name = blocks
        .get(reference.block)
        .map_or("", |block| block.name.as_str());
    group(out, 2, name);
    real(out, 10, reference.insertion.x);
    real(out, 20, reference.insertion.y);
    // Exactly one is no scale at all.
    #[allow(clippy::float_cmp)]
    if reference.scale != 1.0 || reference.mirrored {
        let y = if reference.mirrored {
            -reference.scale
        } else {
            reference.scale
        };
        real(out, 41, reference.scale);
        real(out, 42, y);
        real(out, 43, reference.scale);
    }
    if reference.rotation != 0.0 {
        real(out, 50, reference.rotation.to_degrees());
    }
}

/// R12 has no LWPOLYLINE: a polyline is a POLYLINE header, one VERTEX per
/// point, and a closing SEQEND (ADR-0005). A vertex that starts an arc
/// segment carries its bulge in group 42; straight ones carry none.
fn write_polyline(
    out: &mut String,
    polyline: &PolylineEntity,
    layer: &str,
    properties: impl FnOnce(&mut String),
) {
    /// "Vertices follow" — required on every R12 polyline.
    const VERTICES_FOLLOW: &str = "1";
    /// Bit 1 of the polyline flags: closed.
    const CLOSED: &str = "1";
    const OPEN: &str = "0";

    group(out, 0, "POLYLINE");
    properties(out);
    group(out, 66, VERTICES_FOLLOW);
    group(out, 70, if polyline.closed { CLOSED } else { OPEN });
    for (vertex, bulge) in polyline.vertices.iter().zip(polyline.bulges()) {
        group(out, 0, "VERTEX");
        // A vertex is on its polyline's layer and has no properties of its own.
        group(out, 8, layer);
        real(out, 10, vertex.x);
        real(out, 20, vertex.y);
        if bulge.abs() > 0.0 {
            real(out, 42, bulge);
        }
    }
    group(out, 0, "SEQEND");
}

fn section(out: &mut String, name: &str, body: impl FnOnce(&mut String)) {
    group(out, 0, "SECTION");
    group(out, 2, name);
    body(out);
    group(out, 0, "ENDSEC");
}

/// One table of the TABLES section; group 70 is the number of rows.
fn table(out: &mut String, name: &str, rows: usize, body: impl FnOnce(&mut String)) {
    group(out, 0, "TABLE");
    group(out, 2, name);
    group(out, 70, &rows.to_string());
    body(out);
    group(out, 0, "ENDTAB");
}

/// One group: the code right-aligned in three columns, then the value.
fn group(out: &mut String, code: i32, value: &str) {
    writeln!(out, "{code:>3}\n{value}").expect("writing to a String cannot fail");
}

fn real(out: &mut String, code: i32, value: f64) {
    group(out, code, &format_real(value));
}

/// The shortest decimal that parses back to exactly `value`, always with a
/// decimal point so readers treat it as a real.
fn format_real(value: f64) -> String {
    let mut text = value.to_string();
    if !text.contains('.') {
        text.push_str(".0");
    }
    text
}

#[cfg(test)]
mod tests {
    use cad_core::Point2;
    use proptest::prelude::*;

    use super::*;

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Entity {
        Entity::Line(LineEntity::new(Point2::new(x0, y0), Point2::new(x1, y1)))
    }

    #[test]
    fn an_empty_drawing_is_a_valid_file_with_its_three_permanent_rows() {
        let expected = "  0\nSECTION\n  2\nHEADER\n  9\n$ACADVER\n  1\nAC1009\n  0\nENDSEC\n\
                        \x20 0\nSECTION\n  2\nTABLES\n\
                        \x20 0\nTABLE\n  2\nLTYPE\n 70\n1\n\
                        \x20 0\nLTYPE\n  2\nContinuous\n 70\n0\n  3\nSolid line\n 72\n65\n 73\n0\n 40\n0.0\n\
                        \x20 0\nENDTAB\n\
                        \x20 0\nTABLE\n  2\nSTYLE\n 70\n1\n\
                        \x20 0\nSTYLE\n  2\nStandard\n 70\n0\n 40\n0.0\n 41\n1.0\n 50\n0.0\n 71\n0\n 42\n2.5\n  3\ntxt\n\
                        \x20 0\nENDTAB\n\
                        \x20 0\nTABLE\n  2\nLAYER\n 70\n1\n\
                        \x20 0\nLAYER\n  2\n0\n 70\n0\n 62\n7\n  6\nContinuous\n\
                        \x20 0\nENDTAB\n  0\nENDSEC\n\
                        \x20 0\nSECTION\n  2\nENTITIES\n  0\nENDSEC\n  0\nEOF\n";
        let export = export_with_report(&Drawing::new());
        assert_eq!(export.text, expected);
        assert!(!export.report.is_lossy());
        assert!(export.report.summary().is_empty());
    }

    #[test]
    fn a_line_is_written_with_its_layer_and_four_coordinates() {
        let dxf = export(&Drawing::from_entities([line(0.0, 0.0, 100.0, 50.5)]));
        let entity = "  0\nLINE\n  8\n0\n 10\n0.0\n 20\n0.0\n 11\n100.0\n 21\n50.5\n";
        assert!(dxf.contains(entity), "{dxf}");
    }

    #[test]
    fn circles_and_arcs_are_written_with_angles_in_degrees() {
        use std::f64::consts::{FRAC_PI_2, PI};
        let dxf = export(&Drawing::from_entities([
            Entity::Circle(CircleEntity::new(Point2::new(1.0, 2.0), 3.5)),
            Entity::Arc(ArcEntity::new(Point2::new(0.0, 0.0), 10.0, FRAC_PI_2, PI)),
        ]));
        assert!(
            dxf.contains("  0\nCIRCLE\n  8\n0\n 10\n1.0\n 20\n2.0\n 40\n3.5\n"),
            "{dxf}"
        );
        assert!(
            dxf.contains(
                "  0\nARC\n  8\n0\n 10\n0.0\n 20\n0.0\n 40\n10.0\n 50\n90.0\n 51\n180.0\n"
            ),
            "{dxf}"
        );
    }

    /// #115: a bulge is written on the vertex that starts the arc segment,
    /// and nowhere else — straight polylines are written as before.
    #[test]
    fn bulges_are_written_as_group_42_on_their_vertices() {
        let slot = PolylineEntity::with_bulges(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(10.0, 0.0),
                Point2::new(10.0, 4.0),
                Point2::new(0.0, 4.0),
            ],
            vec![0.0, 1.0, 0.0, -0.5],
            true,
        );
        let dxf = export(&Drawing::from_entities([Entity::Polyline(slot)]));
        // Bulges are the only group 42 among the entities (a STYLE row has one too).
        let entities = &dxf[dxf.find("ENTITIES").unwrap()..];
        assert_eq!(entities.matches("\n 42\n").count(), 2);
        assert!(dxf.contains("VERTEX\n  8\n0\n 10\n10.0\n 20\n0.0\n 42\n1.0\n"));
        assert!(dxf.contains("VERTEX\n  8\n0\n 10\n0.0\n 20\n4.0\n 42\n-0.5\n"));
        assert!(dxf.contains("VERTEX\n  8\n0\n 10\n0.0\n 20\n0.0\n  0\nVERTEX"));
    }

    #[test]
    fn a_polyline_is_written_as_polyline_vertex_seqend() {
        let triangle = PolylineEntity::new(
            vec![
                Point2::new(0.0, 0.0),
                Point2::new(4.0, 0.0),
                Point2::new(4.0, 3.0),
            ],
            true,
        );
        let dxf = export(&Drawing::from_entities([Entity::Polyline(triangle)]));
        let expected = "  0\nPOLYLINE\n  8\n0\n 66\n1\n 70\n1\n\
                        \x20 0\nVERTEX\n  8\n0\n 10\n0.0\n 20\n0.0\n\
                        \x20 0\nVERTEX\n  8\n0\n 10\n4.0\n 20\n0.0\n\
                        \x20 0\nVERTEX\n  8\n0\n 10\n4.0\n 20\n3.0\n\
                        \x20 0\nSEQEND\n";
        assert!(dxf.contains(expected), "{dxf}");
        assert!(!dxf.contains("LWPOLYLINE"), "R12 has no LWPOLYLINE");
    }

    #[test]
    fn output_is_deterministic_and_ordered_by_entity_id() {
        let mut drawing = Drawing::new();
        let mut ids = Vec::new();
        for x in [1.0, 2.0, 3.0, 4.0] {
            let mut tx = drawing.transaction();
            ids.push(
                tx.insert(LineEntity::new(Point2::new(x, 0.0), Point2::new(x, 1.0)))
                    .unwrap(),
            );
            tx.commit();
        }
        // Removing from the middle reorders dense storage; export must not care.
        let mut tx = drawing.transaction();
        tx.remove(ids[1]).unwrap();
        tx.commit();

        let dxf = export(&drawing);
        let xs: Vec<&str> = dxf
            .lines()
            .skip_while(|l| *l != "ENTITIES")
            .filter(|l| l.ends_with(".0"))
            .collect();
        assert_eq!(
            xs,
            vec![
                "1.0", "0.0", "1.0", "1.0", "3.0", "0.0", "3.0", "1.0", "4.0", "0.0", "4.0", "1.0"
            ]
        );
        assert_eq!(export(&drawing), dxf);
    }

    #[test]
    fn reals_always_carry_a_decimal_point() {
        assert_eq!(format_real(100.0), "100.0");
        assert_eq!(format_real(-0.5), "-0.5");
        assert_eq!(format_real(707.1067811865476), "707.1067811865476");
        assert_eq!(format_real(1e21), "1000000000000000000000.0");
    }

    // ── layers, colours, linetypes, lineweights (#139) ──

    /// Layers `walls` (dashed, colour 1, locked) and `hidden` (off), the
    /// linetypes Dashed and Dot, and lines with every kind of property.
    fn styled() -> Drawing {
        use cad_core::Point2;
        use cad_document::{Layer, LayerId, LinetypeId};

        let (walls, hidden) = (LayerId::from_raw(1), LayerId::from_raw(2));
        let (dashed, dot) = (LinetypeId::from_raw(1), LinetypeId::from_raw(2));
        let layers = [
            Layer {
                color: Color::Index(1),
                linetype: Linetype::Named(dashed),
                locked: true,
                ..Layer::named("walls")
            },
            Layer {
                on: false,
                lineweight: Lineweight::Hundredths(50),
                ..Layer::named("hidden")
            },
        ];
        let linetypes = [
            LinetypeDef::built_in_named("Dashed").unwrap(),
            LinetypeDef::built_in_named("Dot").unwrap(),
        ];
        let at = |y: f64| Entity::Line(LineEntity::new(Point2::new(0.0, y), Point2::new(10.0, y)));
        let entities = [
            (at(0.0), Properties::default()),
            (at(1.0), Properties::on(walls)),
            (
                at(2.0),
                Properties {
                    color: Color::Index(5),
                    linetype: Linetype::Named(dot),
                    lineweight: Lineweight::Hundredths(35),
                    ..Properties::on(hidden)
                },
            ),
            (
                at(3.0),
                Properties {
                    color: Color::ByBlock,
                    linetype: Linetype::ByBlock,
                    ..Properties::default()
                },
            ),
            (
                at(4.0),
                Properties {
                    linetype: Linetype::CONTINUOUS,
                    lineweight: Lineweight::Default,
                    ..Properties::on(walls)
                },
            ),
        ];
        Drawing::from_parts(layers, linetypes, entities)
            .unwrap()
            .with_settings(DrawingSettings {
                current_layer: walls,
                current_color: Color::Index(3),
                current_linetype: Linetype::Named(dot),
                current_lineweight: Lineweight::Hundredths(25),
                ltscale: 20.0,
                unit: None,
            })
            .unwrap()
    }

    #[test]
    fn the_tables_are_written_with_patterns_switches_and_linetypes() {
        let dxf = export(&styled());
        let tables = dxf
            .split("TABLES\n")
            .nth(1)
            .and_then(|rest| rest.split("  0\nENDSEC").next())
            .unwrap();
        assert_eq!(
            tables,
            "  0\nTABLE\n  2\nLTYPE\n 70\n3\n\
             \x20 0\nLTYPE\n  2\nContinuous\n 70\n0\n  3\nSolid line\n 72\n65\n 73\n0\n 40\n0.0\n\
             \x20 0\nLTYPE\n  2\nDashed\n 70\n0\n  3\n__ __ __ __ __ __ __\n 72\n65\n 73\n2\n 40\n0.75\n 49\n0.5\n 49\n-0.25\n\
             \x20 0\nLTYPE\n  2\nDot\n 70\n0\n  3\n. . . . . . . . . . . .\n 72\n65\n 73\n2\n 40\n0.25\n 49\n0.0\n 49\n-0.25\n\
             \x20 0\nENDTAB\n\
             \x20 0\nTABLE\n  2\nSTYLE\n 70\n1\n\
             \x20 0\nSTYLE\n  2\nStandard\n 70\n0\n 40\n0.0\n 41\n1.0\n 50\n0.0\n 71\n0\n 42\n2.5\n  3\ntxt\n\
             \x20 0\nENDTAB\n\
             \x20 0\nTABLE\n  2\nLAYER\n 70\n3\n\
             \x20 0\nLAYER\n  2\n0\n 70\n0\n 62\n7\n  6\nContinuous\n\
             \x20 0\nLAYER\n  2\nwalls\n 70\n4\n 62\n1\n  6\nDashed\n\
             \x20 0\nLAYER\n  2\nhidden\n 70\n0\n 62\n-7\n  6\nContinuous\n\
             \x20 0\nENDTAB\n"
        );
    }

    #[test]
    fn an_entity_carries_its_layer_and_its_own_colour_and_linetype_only() {
        let dxf = export(&styled());
        let entities: Vec<&str> = dxf
            .split("ENTITIES\n")
            .nth(1)
            .unwrap()
            .split("  0\nLINE\n")
            .skip(1)
            .map(|line| line.split(" 10\n").next().unwrap())
            .collect();
        assert_eq!(
            entities,
            vec![
                "  8\n0\n",
                "  8\nwalls\n",
                "  8\nhidden\n  6\nDot\n 62\n5\n",
                "  8\n0\n  6\nBYBLOCK\n 62\n0\n",
                "  8\nwalls\n  6\nContinuous\n",
            ]
        );
    }

    #[test]
    fn the_header_carries_the_settings_that_are_not_a_new_drawings() {
        let dxf = export(&styled());
        let header = dxf.split("  0\nENDSEC").next().unwrap();
        assert!(header.contains("  9\n$CLAYER\n  8\nwalls\n"), "{header}");
        assert!(header.contains("  9\n$CECOLOR\n 62\n3\n"));
        assert!(header.contains("  9\n$CELTYPE\n  6\nDot\n"));
        assert!(header.contains("  9\n$LTSCALE\n 40\n20.0\n"));
        assert!(!header.contains("$CELWEIGHT"), "R12 has no lineweights");
        let plain = export(&Drawing::new());
        for variable in ["$CLAYER", "$CECOLOR", "$CELTYPE", "$LTSCALE"] {
            assert!(!plain.contains(variable), "{variable} is the default");
        }
    }

    #[test]
    fn lineweights_are_named_in_the_report_not_silently_dropped() {
        let export = export_with_report(&styled());
        assert!(!export.text.contains("\n370\n"), "no group 370 in R12");
        assert_eq!(
            export.report,
            ExportReport {
                lineweights_on_entities: 2,
                lineweights_on_layers: 1,
            }
        );
        assert!(export.report.is_lossy());
        assert_eq!(
            export.report.summary(),
            vec!["Lineweights are not written in DXF R12: dropped from 2 objects and 1 layer."]
        );
        let only_layers = ExportReport {
            lineweights_on_entities: 0,
            lineweights_on_layers: 2,
        };
        assert_eq!(
            only_layers.summary(),
            vec!["Lineweights are not written in DXF R12: dropped from 2 layers."]
        );
    }

    #[test]
    fn everything_r12_holds_survives_export_import_export_byte_for_byte() {
        let first = export(&styled());
        let back = crate::import(first.as_bytes()).unwrap();
        assert_eq!(back.report.layers, 2);
        assert_eq!(back.report.linetypes, 2);
        assert_eq!(
            back.report.created_layers + back.report.created_linetypes,
            0
        );
        assert!(!back.report.has_unsupported_content());
        let drawing = &back.drawing;
        let walls = drawing.layers().find("walls").unwrap();
        let walls_row = drawing.layers().get(walls).unwrap();
        assert!(walls_row.locked && walls_row.on);
        assert_eq!(walls_row.color, Color::Index(1));
        let hidden = drawing
            .layers()
            .get(drawing.layers().find("hidden").unwrap())
            .unwrap();
        assert!(!hidden.on);
        assert_eq!(hidden.lineweight, Lineweight::Default, "lost, as reported");
        assert_eq!(drawing.current_layer(), walls);
        assert_eq!(drawing.ltscale(), 20.0);
        assert_eq!(export(drawing), first);
    }

    // ── text (#143) ──

    #[test]
    fn blocks_are_written_as_a_blocks_section_and_inserts_and_survive_a_round_trip() {
        use cad_document::{Block, BlockReferenceEntity, EntityRecord, Properties};
        let mut drawing = Drawing::new();
        let mut tx = drawing.transaction();
        let chair = tx
            .define_block(Block::new(
                "Chair",
                Point2::new(1.0, 2.0),
                vec![EntityRecord::new(
                    LineEntity::new(Point2::ORIGIN, Point2::new(10.0, 0.0)),
                    Properties {
                        color: Color::ByBlock,
                        ..Properties::default()
                    },
                )],
            ))
            .unwrap();
        let pair = tx
            .define_block(Block::new(
                "Pair",
                Point2::ORIGIN,
                vec![
                    EntityRecord::new(
                        BlockReferenceEntity::new(chair, Point2::ORIGIN, 1.0, 0.0),
                        Properties::default(),
                    ),
                    EntityRecord::new(
                        BlockReferenceEntity::new(chair, Point2::new(20.0, 0.0), 1.0, 0.0),
                        Properties::default(),
                    ),
                ],
            ))
            .unwrap();
        tx.insert(BlockReferenceEntity::new(
            pair,
            Point2::new(100.0, 0.0),
            1.0,
            0.0,
        ))
        .unwrap();
        // Mirrored, twice the size, a quarter turn, in red.
        tx.insert_with(
            BlockReferenceEntity {
                mirrored: true,
                ..BlockReferenceEntity::new(
                    chair,
                    Point2::new(0.0, 50.0),
                    2.0,
                    std::f64::consts::FRAC_PI_2,
                )
            },
            Properties {
                color: Color::Index(1),
                ..Properties::default()
            },
        )
        .unwrap();
        tx.commit();

        let first = export_with_report(&drawing);
        assert!(!first.report.is_lossy());
        let blocks = first
            .text
            .split("  2\nBLOCKS\n")
            .nth(1)
            .and_then(|rest| rest.split("  0\nENDSEC").next())
            .expect("a BLOCKS section");
        assert_eq!(
            blocks,
            "  0\nBLOCK\n  8\n0\n  2\nChair\n 70\n0\n 10\n1.0\n 20\n2.0\n  3\nChair\n\
             \x20 0\nLINE\n  8\n0\n 62\n0\n 10\n0.0\n 20\n0.0\n 11\n10.0\n 21\n0.0\n\
             \x20 0\nENDBLK\n  8\n0\n\
             \x20 0\nBLOCK\n  8\n0\n  2\nPair\n 70\n0\n 10\n0.0\n 20\n0.0\n  3\nPair\n\
             \x20 0\nINSERT\n  8\n0\n  2\nChair\n 10\n0.0\n 20\n0.0\n\
             \x20 0\nINSERT\n  8\n0\n  2\nChair\n 10\n20.0\n 20\n0.0\n\
             \x20 0\nENDBLK\n  8\n0\n"
        );
        assert!(first.text.contains(
            "  0\nINSERT\n  8\n0\n 62\n1\n  2\nChair\n 10\n0.0\n 20\n50.0\n 41\n2.0\n 42\n-2.0\n 43\n2.0\n 50\n90.0\n"
        ));

        let back = crate::import(first.text.as_bytes()).unwrap();
        assert!(!back.report.has_unsupported_content());
        assert_eq!((back.report.blocks, back.report.block_references), (2, 2));
        let mirrored = back
            .drawing
            .entities()
            .filter_map(|(_, entity)| match entity {
                Entity::BlockRef(reference) if reference.mirrored => Some(reference.clone()),
                _ => None,
            })
            .next()
            .expect("the mirrored chair");
        assert_eq!(mirrored.scale, 2.0);
        assert!((mirrored.rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert_eq!(export(&back.drawing), first.text, "byte-identical");

        // A drawing with no blocks writes no BLOCKS section.
        assert!(!export(&Drawing::new()).contains("BLOCKS"));
    }

    #[test]
    fn a_text_is_written_with_its_insertion_height_words_angle_and_justification() {
        use cad_core::Point2;
        use cad_document::{Justification, TextEntity};

        let plain = TextEntity::new(Point2::new(1.0, 2.0), 2.5, 0.0, "Hello world");
        let turned = TextEntity::new(
            Point2::new(10.0, 0.0),
            5.0,
            std::f64::consts::FRAC_PI_2,
            "Up",
        )
        .justified(Justification::Center);
        let right = TextEntity::new(Point2::new(50.0, 0.0), 5.0, 0.0, "End")
            .justified(Justification::Right);
        let start = right.start();
        let dxf = export(&Drawing::from_entities([
            Entity::Text(plain),
            Entity::Text(turned.clone()),
            Entity::Text(right),
        ]));
        assert!(
            dxf.contains(
                "  0\nTEXT\n  8\n0\n 10\n1.0\n 20\n2.0\n 40\n2.5\n  1\nHello world\n  0\n"
            ),
            "{dxf}"
        );
        // A centred text: the alignment point is the insertion, group 72 is 1,
        // and the first point is where the baseline starts.
        let centred_start = turned.start();
        assert!(
            dxf.contains(&format!(
                " 10\n{}\n 20\n{}\n 40\n5.0\n  1\nUp\n 50\n90.0\n 72\n1\n 11\n10.0\n 21\n0.0\n",
                format_real(centred_start.x),
                format_real(centred_start.y)
            )),
            "{dxf}"
        );
        assert!(
            dxf.contains(&format!(
                " 10\n{}\n 20\n0.0\n 40\n5.0\n  1\nEnd\n 72\n2\n 11\n50.0\n 21\n0.0\n",
                format_real(start.x)
            )),
            "{dxf}"
        );
        assert!(!dxf.contains("  7\n"), "the Standard style is the default");
    }

    proptest! {
        #[test]
        fn formatted_reals_parse_back_exactly(value in proptest::num::f64::NORMAL | proptest::num::f64::ZERO) {
            prop_assert_eq!(format_real(value).parse::<f64>().unwrap(), value);
        }
    }
}
