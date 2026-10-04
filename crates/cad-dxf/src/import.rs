//! DXF import: a tolerant, defensive ASCII reader.
//!
//! The reader accepts any DXF release, because it only looks at the ENTITIES
//! section and at LINE, CIRCLE, ARC, and polyline records. Everything else is
//! counted and reported.

use cad_core::Point2;
use cad_document::{ArcEntity, CircleEntity, Drawing, Entity, LineEntity, PolylineEntity};

use crate::{DxfError, ImportReport};

/// Start of every binary DXF file.
const BINARY_SENTINEL: &[u8] = b"AutoCAD Binary DXF";
/// Records that belong to the entity before them (POLYLINE vertices, INSERT
/// attributes) and are not counted on their own.
const SUBORDINATE_RECORDS: [&str; 1] = ["ATTRIB"];
/// POLYLINE flag bits for 3D polylines, meshes, and polyface meshes.
const POLYLINE_NOT_2D: i64 = 8 | 16 | 32 | 64;
/// Polyline flag bit: closed.
const POLYLINE_CLOSED: i64 = 1;
/// Report key for supported entity types with geometry the drawing cannot hold.
const INVALID: &str = "(invalid geometry)";
/// Longest type name kept in the report.
const MAX_KIND_LENGTH: usize = 32;

/// Bounds on what an import may consume. Imported files are untrusted (§43).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportLimits {
    pub max_bytes: usize,
    pub max_entities: usize,
}

impl Default for ImportLimits {
    fn default() -> Self {
        Self {
            max_bytes: 256 * 1024 * 1024,
            max_entities: 2_000_000,
        }
    }
}

/// A successfully imported drawing and the account of what was skipped.
#[derive(Debug)]
pub struct Import {
    pub drawing: Drawing,
    pub report: ImportReport,
}

/// Imports an ASCII DXF file with the default limits.
pub fn import(bytes: &[u8]) -> Result<Import, DxfError> {
    import_with_limits(bytes, &ImportLimits::default())
}

/// Imports an ASCII DXF file into a fresh drawing with an empty history.
pub fn import_with_limits(bytes: &[u8], limits: &ImportLimits) -> Result<Import, DxfError> {
    if bytes.len() > limits.max_bytes {
        return Err(DxfError::FileTooLarge {
            bytes: bytes.len(),
            limit: limits.max_bytes,
        });
    }
    if bytes.starts_with(BINARY_SENTINEL) {
        return Err(DxfError::BinaryDxf);
    }
    // Pre-2007 files use a legacy code page. Only text content could be
    // affected, and none is imported, so lossy decoding is safe here.
    let text = String::from_utf8_lossy(bytes);

    let mut reader = Reader {
        groups: Groups::new(&text),
        limits,
        entities: Vec::new(),
        report: ImportReport::default(),
        open_polyline: None,
    };
    reader.read_file()?;
    Ok(Import {
        drawing: Drawing::from_entities(reader.entities),
        report: reader.report,
    })
}

/// One DXF group: a code and its value, with the line the code was on.
#[derive(Clone, Copy, Debug)]
struct Group<'a> {
    code: i32,
    value: &'a str,
    line: usize,
}

impl Group<'_> {
    fn is(&self, code: i32, value: &str) -> bool {
        self.code == code && self.value == value
    }
}

/// Iterates over the groups of a file, two lines at a time.
struct Groups<'a> {
    lines: std::str::Lines<'a>,
    line: usize,
}

impl<'a> Groups<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            lines: text.lines(),
            line: 0,
        }
    }

    /// The next group, or `None` at a clean end of input.
    fn next(&mut self) -> Result<Option<Group<'a>>, DxfError> {
        let Some(code_text) = self.lines.next() else {
            return Ok(None);
        };
        self.line += 1;
        let line = self.line;
        let code = code_text
            .trim()
            .parse::<i32>()
            .map_err(|_| DxfError::InvalidGroupCode { line })?;
        let value = self.lines.next().ok_or(DxfError::UnexpectedEnd { line })?;
        self.line += 1;
        Ok(Some(Group {
            code,
            value: value.trim(),
            line,
        }))
    }

    /// The next group; the file must not end here.
    fn expect(&mut self) -> Result<Group<'a>, DxfError> {
        self.next()?.ok_or(DxfError::UnexpectedEnd {
            line: self.line + 1,
        })
    }
}

struct Reader<'a> {
    groups: Groups<'a>,
    limits: &'a ImportLimits,
    entities: Vec<Entity>,
    report: ImportReport,
    /// An R12-style POLYLINE whose VERTEX records are still being read.
    open_polyline: Option<PolylineBuilder>,
}

/// Collects a polyline that may span several records.
#[derive(Debug, Default)]
struct PolylineBuilder {
    vertices: Vec<Point2>,
    /// One per vertex: the bulge of the segment that starts there.
    bulges: Vec<f64>,
    closed: bool,
    /// Not a 2D polyline at all (mesh, 3D polyline): already reported; its
    /// vertices are swallowed.
    unsupported: bool,
    has_dropped_properties: bool,
}

impl<'a> Reader<'a> {
    fn read_file(&mut self) -> Result<(), DxfError> {
        loop {
            let group = self.groups.expect()?;
            if group.is(0, "EOF") {
                return Ok(());
            }
            if group.is(0, "SECTION") {
                let name = self.groups.expect()?;
                match name.value {
                    "ENTITIES" if name.code == 2 => self.read_records(RecordContext::Entities)?,
                    "BLOCKS" if name.code == 2 => self.read_records(RecordContext::Blocks)?,
                    _ => self.skip_section()?,
                }
            }
            // Anything else between sections (comments, stray groups) is ignored.
        }
    }

    fn skip_section(&mut self) -> Result<(), DxfError> {
        while !self.groups.expect()?.is(0, "ENDSEC") {}
        Ok(())
    }

    /// Reads the `0`-delimited records of a section up to its `ENDSEC`.
    fn read_records(&mut self, context: RecordContext) -> Result<(), DxfError> {
        let mut record = self.groups.expect()?;
        loop {
            if record.code != 0 {
                // Groups before the first record of the section: ignore.
                record = self.groups.expect()?;
                continue;
            }
            if record.value == "ENDSEC" {
                return Ok(());
            }
            let mut body = RecordBody::default();
            let next = loop {
                let group = self.groups.expect()?;
                if group.code == 0 {
                    break group;
                }
                body.absorb(group)?;
            };
            self.finish_record(context, record, &body)?;
            record = next;
        }
    }

    fn finish_record(
        &mut self,
        context: RecordContext,
        record: Group<'_>,
        body: &RecordBody,
    ) -> Result<(), DxfError> {
        match (context, record.value) {
            (RecordContext::Entities, "LINE") => {
                let line = body.line(record.line)?;
                self.report.lines += 1;
                self.import(Entity::Line(line), body)?;
            }
            (RecordContext::Entities, "CIRCLE") => match body.circle(record.line)? {
                Some(circle) => {
                    self.report.circles += 1;
                    self.import(Entity::Circle(circle), body)?;
                }
                None => self.report.count_unsupported(INVALID),
            },
            (RecordContext::Entities, "ARC") => match body.arc(record.line)? {
                Some(arc) => {
                    self.report.arcs += 1;
                    self.import(Entity::Arc(arc), body)?;
                }
                None => self.report.count_unsupported(INVALID),
            },
            (RecordContext::Entities, "LWPOLYLINE") => {
                let builder = PolylineBuilder {
                    vertices: body.points(record.line, "LWPOLYLINE")?,
                    closed: body.flags & POLYLINE_CLOSED != 0,
                    bulges: body.bulges.clone(),
                    unsupported: false,
                    has_dropped_properties: body.has_dropped_properties,
                };
                self.finish_polyline(builder)?;
            }
            (RecordContext::Entities, "POLYLINE") => {
                let unsupported = body.flags & POLYLINE_NOT_2D != 0;
                if unsupported {
                    self.report.count_unsupported("POLYLINE");
                }
                self.open_polyline = Some(PolylineBuilder {
                    closed: body.flags & POLYLINE_CLOSED != 0,
                    unsupported,
                    has_dropped_properties: body.has_dropped_properties,
                    ..Default::default()
                });
            }
            (RecordContext::Entities, "VERTEX") => {
                let open = self
                    .open_polyline
                    .as_mut()
                    .filter(|polyline| !polyline.unsupported);
                if let Some(polyline) = open {
                    polyline
                        .vertices
                        .extend(body.points(record.line, "VERTEX")?);
                    polyline.bulges.extend(&body.bulges);
                }
            }
            (RecordContext::Entities, "SEQEND") => {
                let finished = self
                    .open_polyline
                    .take()
                    .filter(|polyline| !polyline.unsupported);
                if let Some(polyline) = finished {
                    self.finish_polyline(polyline)?;
                }
            }
            (RecordContext::Entities, kind) if !SUBORDINATE_RECORDS.contains(&kind) => {
                self.report.count_unsupported(&kind_name(kind));
            }
            // A user block definition, as opposed to the model- and paper-space
            // containers every complete file has.
            (RecordContext::Blocks, "BLOCK") if !body.name_is_reserved => {
                self.report.count_unsupported("BLOCK");
            }
            _ => {}
        }
        Ok(())
    }

    fn import(&mut self, entity: Entity, body: &RecordBody) -> Result<(), DxfError> {
        self.push(entity, body.has_dropped_properties)
    }

    fn push(&mut self, entity: Entity, has_dropped_properties: bool) -> Result<(), DxfError> {
        if self.entities.len() >= self.limits.max_entities {
            return Err(DxfError::TooManyEntities {
                limit: self.limits.max_entities,
            });
        }
        self.entities.push(entity);
        if has_dropped_properties {
            self.report.with_dropped_properties += 1;
        }
        Ok(())
    }

    fn finish_polyline(&mut self, polyline: PolylineBuilder) -> Result<(), DxfError> {
        if polyline.vertices.len() < 2 {
            self.report.count_unsupported(INVALID);
        } else {
            self.report.polylines += 1;
            let entity = Entity::Polyline(PolylineEntity::with_bulges(
                polyline.vertices,
                polyline.bulges,
                polyline.closed,
            ));
            self.push(entity, polyline.has_dropped_properties)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
enum RecordContext {
    Entities,
    Blocks,
}

/// The groups of one record that matter to this reader.
#[derive(Debug, Default)]
struct RecordBody {
    /// Every group 10 and 20 of the record, in order: one point for most
    /// entities, one per vertex for an LWPOLYLINE.
    xs: Vec<f64>,
    ys: Vec<f64>,
    /// Group 70: entity flags.
    flags: i64,
    /// Group 42 of each vertex, in step with `xs`: the bulge of the polyline
    /// segment that starts there. Zero where the vertex has none.
    bulges: Vec<f64>,
    x0: Option<f64>,
    y0: Option<f64>,
    x1: Option<f64>,
    y1: Option<f64>,
    radius: Option<f64>,
    start_degrees: Option<f64>,
    end_degrees: Option<f64>,
    has_dropped_properties: bool,
    name_is_reserved: bool,
}

impl RecordBody {
    fn absorb(&mut self, group: Group<'_>) -> Result<(), DxfError> {
        match group.code {
            10 => {
                let x = real(group)?;
                self.x0.get_or_insert(x);
                self.xs.push(x);
                self.bulges.push(0.0);
            }
            20 => {
                let y = real(group)?;
                self.y0.get_or_insert(y);
                self.ys.push(y);
            }
            // A bulge belongs to the vertex whose coordinates precede it.
            42 => {
                let bulge = real(group)?;
                if let Some(last) = self.bulges.last_mut() {
                    *last = bulge;
                }
            }
            // Constant width of an LWPOLYLINE: dropped.
            43 => self.has_dropped_properties |= real(group)? != 0.0,
            70 => self.flags = group.value.parse().unwrap_or(0),
            11 => self.x1 = Some(real(group)?),
            21 => self.y1 = Some(real(group)?),
            40 => self.radius = Some(real(group)?),
            50 => self.start_degrees = Some(real(group)?),
            51 => self.end_degrees = Some(real(group)?),
            2 => self.name_is_reserved = is_structural_block(group.value),
            // Layer: only the default layer can be represented.
            8 => self.has_dropped_properties |= group.value != "0",
            // Linetype, colour, thickness, linetype scale: dropped.
            6 | 39 | 48 | 62 => self.has_dropped_properties = true,
            // Elevation: a 2D drawing keeps none. Zero is the default and loses nothing.
            30 | 31 => self.has_dropped_properties |= real(group)? != 0.0,
            _ => {}
        }
        Ok(())
    }

    fn line(&self, at: usize) -> Result<LineEntity, DxfError> {
        let need = |value, group| required(value, at, "LINE", group);
        Ok(LineEntity::new(
            Point2::new(need(self.x0, 10)?, need(self.y0, 20)?),
            Point2::new(need(self.x1, 11)?, need(self.y1, 21)?),
        ))
    }

    /// Every point of the record. Each X must have its Y.
    fn points(&self, at: usize, entity: &'static str) -> Result<Vec<Point2>, DxfError> {
        if self.ys.len() < self.xs.len() {
            return Err(DxfError::MissingCoordinate {
                line: at,
                entity,
                group: 20,
            });
        }
        if self.xs.len() < self.ys.len() {
            return Err(DxfError::MissingCoordinate {
                line: at,
                entity,
                group: 10,
            });
        }
        Ok(self
            .xs
            .iter()
            .zip(&self.ys)
            .map(|(x, y)| Point2::new(*x, *y))
            .collect())
    }

    /// `None` for a circle the drawing cannot hold: a radius that is not positive.
    fn circle(&self, at: usize) -> Result<Option<CircleEntity>, DxfError> {
        let need = |value, group| required(value, at, "CIRCLE", group);
        let center = Point2::new(need(self.x0, 10)?, need(self.y0, 20)?);
        let radius = need(self.radius, 40)?;
        Ok((radius > 0.0).then(|| CircleEntity::new(center, radius)))
    }

    /// DXF arcs run counter-clockwise from group 50 to group 51, in degrees.
    fn arc(&self, at: usize) -> Result<Option<ArcEntity>, DxfError> {
        let need = |value, group| required(value, at, "ARC", group);
        let center = Point2::new(need(self.x0, 10)?, need(self.y0, 20)?);
        let radius = need(self.radius, 40)?;
        let start = need(self.start_degrees, 50)?.to_radians();
        let end = need(self.end_degrees, 51)?.to_radians();
        Ok((radius > 0.0).then(|| ArcEntity::new(center, radius, start, end)))
    }
}

fn required(
    value: Option<f64>,
    line: usize,
    entity: &'static str,
    group: i32,
) -> Result<f64, DxfError> {
    value.ok_or(DxfError::MissingCoordinate {
        line,
        entity,
        group,
    })
}

/// Block names that are part of the file's structure rather than user
/// content: `*Model_Space`, `*Paper_Space`, anonymous `*…` blocks, and the
/// `$MODEL_SPACE` / `$PAPER_SPACE` spelling used by R12 writers.
fn is_structural_block(name: &str) -> bool {
    name.starts_with('*')
        || name.eq_ignore_ascii_case("$MODEL_SPACE")
        || name.eq_ignore_ascii_case("$PAPER_SPACE")
}

fn real(group: Group<'_>) -> Result<f64, DxfError> {
    match group.value.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(value),
        _ => Err(DxfError::InvalidNumber {
            line: group.line + 1,
            group: group.code,
        }),
    }
}

/// A report-safe type name: bounded length, printable ASCII only.
fn kind_name(kind: &str) -> String {
    let clean: String = kind
        .chars()
        .filter(|c| c.is_ascii_graphic())
        .take(MAX_KIND_LENGTH)
        .collect();
    if clean.is_empty() {
        "(unnamed)".to_owned()
    } else {
        clean.to_ascii_uppercase()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::export;

    /// Builds a file from `code value` pairs, one group per pair.
    fn dxf(groups: &[(i32, &str)]) -> String {
        groups
            .iter()
            .map(|(code, value)| format!("{code:>3}\n{value}\n"))
            .collect()
    }

    fn entities(body: &[(i32, &str)]) -> String {
        let mut groups = vec![(0, "SECTION"), (2, "ENTITIES")];
        groups.extend_from_slice(body);
        groups.extend_from_slice(&[(0, "ENDSEC"), (0, "EOF")]);
        dxf(&groups)
    }

    const LINE_0_0_TO_10_5: [(i32, &str); 6] = [
        (0, "LINE"),
        (8, "0"),
        (10, "0.0"),
        (20, "0.0"),
        (11, "10.0"),
        (21, "5.0"),
    ];

    fn lines_of(import: &Import) -> Vec<LineEntity> {
        import
            .drawing
            .entities()
            .filter_map(|(_, entity)| entity.as_line().copied())
            .collect()
    }

    /// Every imported entity, in file order.
    fn entities_of(import: &Import) -> Vec<Entity> {
        let mut entities: Vec<_> = import
            .drawing
            .entities()
            .map(|(id, entity)| (id, entity.clone()))
            .collect();
        entities.sort_by_key(|(id, _)| *id);
        entities.into_iter().map(|(_, entity)| entity).collect()
    }

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> LineEntity {
        LineEntity::new(Point2::new(x0, y0), Point2::new(x1, y1))
    }

    #[test]
    fn imports_a_line() {
        let import = import(entities(&LINE_0_0_TO_10_5).as_bytes()).unwrap();
        assert_eq!(lines_of(&import), vec![line(0.0, 0.0, 10.0, 5.0)]);
        assert_eq!(
            import.report,
            ImportReport {
                lines: 1,
                ..Default::default()
            }
        );
        assert!(!import.report.has_unsupported_content());
    }

    #[test]
    fn import_starts_a_fresh_drawing_with_no_history() {
        let import = import(entities(&LINE_0_0_TO_10_5).as_bytes()).unwrap();
        assert!(!import.drawing.can_undo());
        assert_eq!(import.drawing.len(), 1);
    }

    #[test]
    fn tolerates_crlf_padding_exponents_and_missing_sections() {
        let text = "  0\r\nSECTION\r\n  2\r\nENTITIES\r\n  0\r\nLINE\r\n 10\r\n  1.0E+1\r\n 20\r\n-2.5e0\r\n 11\r\n 3\r\n 21\r\n.5\r\n  0\r\nENDSEC\r\n  0\r\nEOF\r\n";
        let import = import(text.as_bytes()).unwrap();
        assert_eq!(lines_of(&import), vec![line(10.0, -2.5, 3.0, 0.5)]);
    }

    #[test]
    fn a_file_with_no_entities_section_is_an_empty_drawing() {
        let import = import(
            dxf(&[
                (0, "SECTION"),
                (2, "HEADER"),
                (9, "$ACADVER"),
                (1, "AC1027"),
                (0, "ENDSEC"),
                (0, "EOF"),
            ])
            .as_bytes(),
        )
        .unwrap();
        assert!(import.drawing.is_empty());
    }

    #[test]
    fn unsupported_entities_are_counted_by_kind_and_supported_ones_still_import() {
        let mut body = vec![(0, "ELLIPSE"), (10, "0"), (20, "0"), (40, "5")];
        body.extend_from_slice(&LINE_0_0_TO_10_5);
        body.extend_from_slice(&[(0, "TEXT"), (1, "hello"), (0, "ELLIPSE"), (40, "1")]);
        // A 3D polyline is unsupported; its vertices belong to it and are not counted separately.
        body.extend_from_slice(&[
            (0, "POLYLINE"),
            (70, "8"),
            (0, "VERTEX"),
            (10, "1"),
            (0, "VERTEX"),
            (0, "SEQEND"),
        ]);

        let import = import(entities(&body).as_bytes()).unwrap();
        assert_eq!(import.report.lines, 1);
        let counts: Vec<(&str, usize)> = import
            .report
            .unsupported
            .iter()
            .map(|(kind, count)| (kind.as_str(), *count))
            .collect();
        assert_eq!(counts, vec![("ELLIPSE", 2), ("POLYLINE", 1), ("TEXT", 1)]);
        assert!(import.report.has_unsupported_content());
    }

    #[test]
    fn imports_circles_and_arcs_with_angles_converted_from_degrees() {
        use std::f64::consts::{FRAC_PI_2, PI};
        let body = [
            (0, "CIRCLE"),
            (8, "0"),
            (10, "1.5"),
            (20, "-2"),
            (40, "3"),
            (0, "ARC"),
            (8, "0"),
            (10, "0"),
            (20, "0"),
            (40, "10"),
            (50, "90"),
            (51, "180"),
        ];
        let import = import(entities(&body).as_bytes()).unwrap();

        assert_eq!(
            (
                import.report.lines,
                import.report.circles,
                import.report.arcs
            ),
            (0, 1, 1)
        );
        assert!(!import.report.has_unsupported_content());
        let imported = entities_of(&import);
        assert_eq!(
            imported[0],
            Entity::Circle(CircleEntity::new(Point2::new(1.5, -2.0), 3.0))
        );
        let Entity::Arc(arc) = &imported[1] else {
            panic!("expected an arc, got {:?}", imported[1])
        };
        assert_eq!((arc.center, arc.radius), (Point2::ORIGIN, 10.0));
        assert!((arc.start_angle - FRAC_PI_2).abs() < 1e-12 && (arc.end_angle - PI).abs() < 1e-12);
    }

    fn polylines_of(import: &Import) -> Vec<PolylineEntity> {
        entities_of(import)
            .into_iter()
            .filter_map(|entity| match entity {
                Entity::Polyline(polyline) => Some(polyline),
                _ => None,
            })
            .collect()
    }

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn imports_r12_polylines_from_their_vertex_records() {
        let body = [
            (0, "POLYLINE"),
            (8, "0"),
            (66, "1"),
            (70, "1"),
            (0, "VERTEX"),
            (8, "0"),
            (10, "0"),
            (20, "0"),
            (0, "VERTEX"),
            (8, "0"),
            (10, "4"),
            (20, "0"),
            (0, "VERTEX"),
            (8, "0"),
            (10, "4"),
            (20, "3"),
            (0, "SEQEND"),
        ];
        let import =
            import(entities(&[body.as_slice(), &LINE_0_0_TO_10_5].concat()).as_bytes()).unwrap();
        assert_eq!((import.report.polylines, import.report.lines), (1, 1));
        assert!(!import.report.has_unsupported_content());
        assert_eq!(
            polylines_of(&import),
            vec![PolylineEntity::new(
                vec![p(0.0, 0.0), p(4.0, 0.0), p(4.0, 3.0)],
                true
            )]
        );
    }

    #[test]
    fn imports_lwpolylines_from_their_repeated_coordinates() {
        let open = [
            (0, "LWPOLYLINE"),
            (90, "3"),
            (70, "0"),
            (10, "0"),
            (20, "0"),
            (10, "1"),
            (20, "0"),
            (10, "1"),
            (20, "1"),
        ];
        let import = import(entities(&open).as_bytes()).unwrap();
        assert_eq!(
            polylines_of(&import),
            vec![PolylineEntity::new(
                vec![p(0.0, 0.0), p(1.0, 0.0), p(1.0, 1.0)],
                false
            )]
        );
    }

    /// #115: group 42 is read in both encodings, on the vertex it follows.
    #[test]
    fn polylines_with_arc_segments_keep_their_bulges() {
        let lightweight = [
            (0, "LWPOLYLINE"),
            (70, "1"),
            (10, "0"),
            (20, "0"),
            (10, "10"),
            (20, "0"),
            (42, "1.0"),
            (10, "10"),
            (20, "4"),
            (10, "0"),
            (20, "4"),
            (42, "-0.5"),
        ];
        let classic = [
            (0, "POLYLINE"),
            (70, "0"),
            (0, "VERTEX"),
            (10, "0"),
            (20, "0"),
            (42, "1.0"),
            (0, "VERTEX"),
            (10, "2"),
            (20, "0"),
            (0, "SEQEND"),
        ];
        let import =
            import(entities(&[lightweight.as_slice(), &classic].concat()).as_bytes()).unwrap();
        assert_eq!(import.report.polylines, 2);
        assert!(import.report.unsupported.is_empty());
        assert_eq!(
            polylines_of(&import),
            vec![
                PolylineEntity::with_bulges(
                    vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 4.0), p(0.0, 4.0)],
                    vec![0.0, 1.0, 0.0, -0.5],
                    true
                ),
                PolylineEntity::with_bulges(vec![p(0.0, 0.0), p(2.0, 0.0)], vec![1.0], false),
            ]
        );
    }

    #[test]
    fn an_absurd_bulge_never_becomes_unbounded_geometry() {
        let with_bulge = |bulge: &'static str| {
            let body = [
                (0, "LWPOLYLINE"),
                (10, "0"),
                (20, "0"),
                (42, bulge),
                (10, "1"),
                (20, "0"),
            ];
            import(entities(&body).as_bytes())
        };
        // Finite but far too large to square: imported, as a straight segment.
        let import = with_bulge("1e200").unwrap();
        let bounds = Entity::Polyline(polylines_of(&import)[0].clone()).bounds();
        assert!(bounds.min().is_finite() && bounds.max().is_finite());
        // Not a finite number, or not a number: a malformed file, as for a coordinate.
        for bulge in ["1e400", "round"] {
            assert!(
                matches!(
                    with_bulge(bulge),
                    Err(DxfError::InvalidNumber { group: 42, .. })
                ),
                "bulge {bulge}"
            );
        }
    }

    /// #115: export → import → export is byte-identical for bulged polylines.
    #[test]
    fn a_bulged_polyline_survives_a_round_trip_exactly() {
        let slot = PolylineEntity::with_bulges(
            vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 4.0), p(0.0, 4.0)],
            vec![0.0, 1.0, 0.0, 2f64.sqrt() - 1.0],
            true,
        );
        let open = PolylineEntity::with_bulges(
            vec![p(0.1, 0.2), p(7.3, -9.1), p(20.0, 0.0)],
            vec![-0.333_333_333_333_333_3, 0.0],
            false,
        );
        let drawing = Drawing::from_entities([
            Entity::Polyline(slot.clone()),
            Entity::Polyline(open.clone()),
        ]);
        let text = export(&drawing);
        let reread = import(text.as_bytes()).unwrap();
        assert_eq!(polylines_of(&reread), vec![slot, open]);
        assert_eq!(export(&reread.drawing), text);
    }

    #[test]
    fn a_zero_bulge_is_a_straight_segment() {
        let body = [
            (0, "LWPOLYLINE"),
            (70, "1"),
            (10, "0"),
            (20, "0"),
            (42, "0.0"),
            (10, "1"),
            (20, "0"),
            (10, "1"),
            (20, "1"),
        ];
        let import = import(entities(&body).as_bytes()).unwrap();
        assert_eq!(import.report.polylines, 1);
        assert!(polylines_of(&import)[0].closed);
    }

    #[test]
    fn degenerate_and_malformed_polylines() {
        let one_vertex = [(0, "LWPOLYLINE"), (10, "0"), (20, "0")];
        let import_one = import(entities(&one_vertex).as_bytes()).unwrap();
        assert_eq!(import_one.report.unsupported.get(INVALID), Some(&1));

        let missing_y = entities(&[(0, "LWPOLYLINE"), (10, "0"), (20, "0"), (10, "1")]);
        assert_eq!(
            import(missing_y.as_bytes()).unwrap_err(),
            DxfError::MissingCoordinate {
                line: 5,
                entity: "LWPOLYLINE",
                group: 20
            }
        );
    }

    #[test]
    fn a_polyline_survives_a_round_trip_exactly() {
        let original = PolylineEntity::new(
            vec![p(0.1, 0.2), p(707.1067811865476, -3.5), p(1e6, 2.5e-3)],
            true,
        );
        let text = export(&Drawing::from_entities([Entity::Polyline(
            original.clone(),
        )]));
        let reread = import(text.as_bytes()).unwrap();
        assert_eq!(polylines_of(&reread), vec![original]);
        assert_eq!(export(&reread.drawing), text);
    }

    #[test]
    fn curves_with_a_radius_that_is_not_positive_are_reported_not_imported() {
        let body = [
            (0, "CIRCLE"),
            (10, "0"),
            (20, "0"),
            (40, "0"),
            (0, "ARC"),
            (10, "0"),
            (20, "0"),
            (40, "-1"),
            (50, "0"),
            (51, "90"),
        ];
        let import = import(entities(&body).as_bytes()).unwrap();
        assert!(import.drawing.is_empty());
        assert_eq!(import.report.unsupported.get(INVALID), Some(&2));
    }

    #[test]
    fn curves_missing_required_groups_are_malformed() {
        let no_radius = entities(&[(0, "CIRCLE"), (10, "0"), (20, "0")]);
        assert_eq!(
            import(no_radius.as_bytes()).unwrap_err(),
            DxfError::MissingCoordinate {
                line: 5,
                entity: "CIRCLE",
                group: 40
            }
        );
        let no_end = entities(&[(0, "ARC"), (10, "0"), (20, "0"), (40, "1"), (50, "0")]);
        assert_eq!(
            import(no_end.as_bytes()).unwrap_err(),
            DxfError::MissingCoordinate {
                line: 5,
                entity: "ARC",
                group: 51
            }
        );
    }

    /// Angles pass through degrees, so they come back within floating-point
    /// rounding rather than bit for bit; everything else is exact.
    #[test]
    fn circles_and_arcs_survive_a_round_trip() {
        let original = vec![
            Entity::Circle(CircleEntity::new(Point2::new(12.5, -7.25), 0.1)),
            Entity::Arc(ArcEntity::new(
                Point2::new(-3.0, 4.0),
                707.1067811865476,
                0.3,
                5.9,
            )),
            Entity::Arc(ArcEntity::new(Point2::ORIGIN, 1.0, 6.0, 0.25)),
        ];
        let text = export(&Drawing::from_entities(original.clone()));
        let reread = entities_of(&import(text.as_bytes()).unwrap());

        assert_eq!(reread[0], original[0]);
        for (read, written) in reread.iter().zip(&original).skip(1) {
            let (Entity::Arc(read), Entity::Arc(written)) = (read, written) else {
                panic!("expected arcs")
            };
            assert_eq!((read.center, read.radius), (written.center, written.radius));
            assert!((read.start_angle - written.start_angle).abs() < 1e-12);
            assert!((read.end_angle - written.end_angle).abs() < 1e-12);
        }
    }

    #[test]
    fn properties_the_drawing_cannot_hold_are_reported() {
        let on_layer = [
            (0, "LINE"),
            (8, "WALLS"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "1"),
        ];
        let coloured = [
            (0, "LINE"),
            (8, "0"),
            (62, "1"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "1"),
        ];
        let elevated = [
            (0, "LINE"),
            (10, "0"),
            (20, "0"),
            (30, "7.5"),
            (11, "1"),
            (21, "1"),
            (31, "0.0"),
        ];
        let flat = [
            (0, "LINE"),
            (10, "0"),
            (20, "0"),
            (30, "0.0"),
            (11, "1"),
            (21, "1"),
            (31, "0"),
        ];
        let body = [
            on_layer.as_slice(),
            &coloured,
            &elevated,
            &flat,
            &LINE_0_0_TO_10_5,
        ]
        .concat();

        let import = import(entities(&body).as_bytes()).unwrap();
        assert_eq!(import.report.lines, 5);
        assert_eq!(import.report.with_dropped_properties, 3);
    }

    #[test]
    fn user_blocks_are_reported_but_structural_ones_are_not() {
        let text = dxf(&[
            (0, "SECTION"),
            (2, "BLOCKS"),
            (0, "BLOCK"),
            (2, "*Model_Space"),
            (0, "ENDBLK"),
            (0, "BLOCK"),
            (2, "DOOR"),
            (0, "LINE"),
            (10, "0"),
            (20, "0"),
            (11, "1"),
            (21, "1"),
            (0, "ENDBLK"),
            (0, "ENDSEC"),
            (0, "EOF"),
        ]);
        let import = import(text.as_bytes()).unwrap();
        assert!(
            import.drawing.is_empty(),
            "geometry inside block definitions is not model space"
        );
        assert_eq!(import.report.unsupported.get("BLOCK"), Some(&1));
    }

    #[test]
    fn structural_block_names_are_recognised_in_every_spelling() {
        for name in [
            "*Model_Space",
            "*Paper_Space",
            "*U12",
            "$MODEL_SPACE",
            "$Paper_Space",
        ] {
            assert!(is_structural_block(name), "{name}");
        }
        for name in ["DOOR", "MODEL_SPACE", "$TITLE"] {
            assert!(!is_structural_block(name), "{name}");
        }
    }

    #[test]
    fn binary_dxf_is_rejected_with_a_clear_message() {
        let error = import(b"AutoCAD Binary DXF\r\n\x1a\0\0\0").unwrap_err();
        assert_eq!(error, DxfError::BinaryDxf);
        assert!(error.to_string().contains("save as ASCII"));
    }

    #[test]
    fn malformed_files_return_structured_errors() {
        let cases: Vec<(String, DxfError)> = vec![
            (String::new(), DxfError::UnexpectedEnd { line: 1 }),
            (
                "hello\nworld\n".into(),
                DxfError::InvalidGroupCode { line: 1 },
            ),
            ("  0\n".into(), DxfError::UnexpectedEnd { line: 1 }),
            // No EOF marker: the file was truncated.
            (
                dxf(&[(0, "SECTION"), (2, "ENTITIES"), (0, "ENDSEC")]),
                DxfError::UnexpectedEnd { line: 7 },
            ),
            (
                dxf(&[(0, "SECTION"), (2, "ENTITIES"), (0, "LINE"), (10, "1")]),
                DxfError::UnexpectedEnd { line: 9 },
            ),
            (
                entities(&[(0, "LINE"), (10, "abc"), (20, "0"), (11, "1"), (21, "1")]),
                DxfError::InvalidNumber { line: 8, group: 10 },
            ),
            (
                entities(&[(0, "LINE"), (10, "inf"), (20, "0"), (11, "1"), (21, "1")]),
                DxfError::InvalidNumber { line: 8, group: 10 },
            ),
            (
                entities(&[(0, "LINE"), (10, "NaN"), (20, "0"), (11, "1"), (21, "1")]),
                DxfError::InvalidNumber { line: 8, group: 10 },
            ),
            (
                entities(&[(0, "LINE"), (10, "0"), (20, "0"), (11, "1")]),
                DxfError::MissingCoordinate {
                    line: 5,
                    entity: "LINE",
                    group: 21,
                },
            ),
        ];
        for (text, expected) in cases {
            assert_eq!(import(text.as_bytes()).unwrap_err(), expected, "{text:?}");
        }
    }

    #[test]
    fn limits_are_enforced() {
        let text = entities(
            &[
                LINE_0_0_TO_10_5.as_slice(),
                &LINE_0_0_TO_10_5,
                &LINE_0_0_TO_10_5,
            ]
            .concat(),
        );

        let too_big = ImportLimits {
            max_bytes: 10,
            ..Default::default()
        };
        assert_eq!(
            import_with_limits(text.as_bytes(), &too_big).unwrap_err(),
            DxfError::FileTooLarge {
                bytes: text.len(),
                limit: 10
            }
        );

        let too_many = ImportLimits {
            max_entities: 2,
            ..Default::default()
        };
        assert_eq!(
            import_with_limits(text.as_bytes(), &too_many).unwrap_err(),
            DxfError::TooManyEntities { limit: 2 }
        );

        let exact = ImportLimits {
            max_entities: 3,
            ..Default::default()
        };
        assert_eq!(
            import_with_limits(text.as_bytes(), &exact)
                .unwrap()
                .report
                .lines,
            3
        );
    }

    #[test]
    fn hostile_type_names_are_sanitised() {
        let long = "X".repeat(10_000);
        let import =
            import(entities(&[(0, long.as_str()), (0, "we\u{202e}ird name")]).as_bytes()).unwrap();
        let kinds: Vec<&String> = import.report.unsupported.keys().collect();
        assert_eq!(kinds, vec!["WEIRDNAME", &"X".repeat(MAX_KIND_LENGTH)]);
    }

    #[test]
    fn non_utf8_bytes_do_not_stop_an_import() {
        let mut bytes = b"999\ncaf\xe9\n".to_vec();
        bytes.extend_from_slice(entities(&LINE_0_0_TO_10_5).as_bytes());
        assert_eq!(import(&bytes).unwrap().report.lines, 1);
    }

    fn coordinate() -> impl Strategy<Value = f64> {
        prop_oneof![-1.0e9..1.0e9_f64, proptest::num::f64::NORMAL]
    }

    proptest! {
        /// Imported files are untrusted: no input may panic.
        #[test]
        fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
            let _ = import(&bytes);
        }

        #[test]
        fn arbitrary_dxf_shaped_text_never_panics(
            groups in proptest::collection::vec((0..1100i32, "[A-Z0-9.*$ -]{0,12}"), 0..60)
        ) {
            let text: String = groups.iter().map(|(code, value)| format!("{code}\n{value}\n")).collect();
            let _ = import(text.as_bytes());
        }

        /// Export → import reproduces every coordinate bit for bit.
        /// #115: any polyline, bulged or not, survives export and import
        /// exactly, and a second export is byte-identical.
        #[test]
        fn polylines_with_bulges_round_trip_exactly(
            vertices in proptest::collection::vec((coordinate(), coordinate()), 2..8),
            bulges in proptest::collection::vec(prop_oneof![Just(0.0), -4.0..4.0f64], 0..8),
            closed in any::<bool>(),
        ) {
            let vertices: Vec<Point2> = vertices.into_iter().map(|(x, y)| p(x, y)).collect();
            let original = PolylineEntity::with_bulges(vertices, bulges, closed);
            let text = export(&Drawing::from_entities([Entity::Polyline(original.clone())]));
            let imported = import(text.as_bytes()).unwrap();
            prop_assert_eq!(polylines_of(&imported), vec![original]);
            prop_assert!(!imported.report.has_unsupported_content());
            prop_assert_eq!(export(&imported.drawing), text);
        }

        #[test]
        fn export_then_import_is_exact(
            coordinates in proptest::collection::vec((coordinate(), coordinate(), coordinate(), coordinate()), 0..20)
        ) {
            let original: Vec<LineEntity> =
                coordinates.iter().map(|&(x0, y0, x1, y1)| line(x0, y0, x1, y1)).collect();
            let drawing = Drawing::from_entities(original.iter().map(|line| Entity::Line(*line)));

            let text = export(&drawing);
            let imported = import(text.as_bytes()).unwrap();

            let mut reread: Vec<(u64, LineEntity)> =
                imported
                    .drawing
                    .entities()
                    .filter_map(|(id, entity)| Some((id.raw(), *entity.as_line()?)))
                    .collect();
            reread.sort_by_key(|(id, _)| *id);
            let reread: Vec<LineEntity> = reread.into_iter().map(|(_, line)| line).collect();
            prop_assert_eq!(reread, original);
            prop_assert!(!imported.report.has_unsupported_content());
            // And a second trip changes nothing.
            prop_assert_eq!(export(&imported.drawing), text);
        }
    }
}
