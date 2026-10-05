//! DXF export (ADR-0005: ASCII DXF, release 12).

use std::fmt::Write;

use cad_core::{EntityId, LengthUnit};
use cad_document::{ArcEntity, CircleEntity, Drawing, Entity, LineEntity, PolylineEntity};

/// The DXF release written, as stored in `$ACADVER`. R12 is the simplest
/// release every CAD tool reads and needs no handles or subclass markers.
const ACADVER_R12: &str = "AC1009";
/// Everything lives on the default layer until layers exist (M4).
const DEFAULT_LAYER: &str = "0";

/// Serialises the drawing as ASCII DXF.
///
/// Output is deterministic: entities are written in `EntityId` order, and
/// coordinates use the shortest decimal form that reads back to exactly the
/// same `f64`.
pub fn export(drawing: &Drawing) -> String {
    let mut out = String::new();

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
    });
    section(&mut out, "ENTITIES", |out| {
        let mut entities: Vec<(EntityId, &Entity)> = drawing.entities().collect();
        entities.sort_unstable_by_key(|(id, _)| *id);
        for (_, entity) in entities {
            match entity {
                Entity::Line(line) => write_line(out, line),
                Entity::Circle(circle) => write_circle(out, circle),
                Entity::Arc(arc) => write_arc(out, arc),
                Entity::Polyline(polyline) => write_polyline(out, polyline),
            }
        }
    });
    group(&mut out, 0, "EOF");
    out
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

fn write_line(out: &mut String, line: &LineEntity) {
    group(out, 0, "LINE");
    group(out, 8, DEFAULT_LAYER);
    real(out, 10, line.start.x);
    real(out, 20, line.start.y);
    real(out, 11, line.end.x);
    real(out, 21, line.end.y);
}

fn write_circle(out: &mut String, circle: &CircleEntity) {
    group(out, 0, "CIRCLE");
    group(out, 8, DEFAULT_LAYER);
    real(out, 10, circle.center.x);
    real(out, 20, circle.center.y);
    real(out, 40, circle.radius);
}

/// DXF arcs run counter-clockwise from the start angle to the end angle, in
/// degrees — the same convention as the document, in different units.
fn write_arc(out: &mut String, arc: &ArcEntity) {
    group(out, 0, "ARC");
    group(out, 8, DEFAULT_LAYER);
    real(out, 10, arc.center.x);
    real(out, 20, arc.center.y);
    real(out, 40, arc.radius);
    real(out, 50, arc.start_angle.to_degrees());
    real(out, 51, arc.end_angle.to_degrees());
}

/// R12 has no LWPOLYLINE: a polyline is a POLYLINE header, one VERTEX per
/// point, and a closing SEQEND (ADR-0005). A vertex that starts an arc
/// segment carries its bulge in group 42; straight ones carry none.
fn write_polyline(out: &mut String, polyline: &PolylineEntity) {
    /// "Vertices follow" — required on every R12 polyline.
    const VERTICES_FOLLOW: &str = "1";
    /// Bit 1 of the polyline flags: closed.
    const CLOSED: &str = "1";
    const OPEN: &str = "0";

    group(out, 0, "POLYLINE");
    group(out, 8, DEFAULT_LAYER);
    group(out, 66, VERTICES_FOLLOW);
    group(out, 70, if polyline.closed { CLOSED } else { OPEN });
    for (vertex, bulge) in polyline.vertices.iter().zip(polyline.bulges()) {
        group(out, 0, "VERTEX");
        group(out, 8, DEFAULT_LAYER);
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
    fn an_empty_drawing_is_a_valid_empty_file() {
        let expected = "  0\nSECTION\n  2\nHEADER\n  9\n$ACADVER\n  1\nAC1009\n  0\nENDSEC\n\
                        \x20 0\nSECTION\n  2\nENTITIES\n  0\nENDSEC\n  0\nEOF\n";
        assert_eq!(export(&Drawing::new()), expected);
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
        assert_eq!(dxf.matches("\n 42\n").count(), 2);
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
            ids.push(tx.insert(LineEntity::new(Point2::new(x, 0.0), Point2::new(x, 1.0))));
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

    proptest! {
        #[test]
        fn formatted_reals_parse_back_exactly(value in proptest::num::f64::NORMAL | proptest::num::f64::ZERO) {
            prop_assert_eq!(format_real(value).parse::<f64>().unwrap(), value);
        }
    }
}
