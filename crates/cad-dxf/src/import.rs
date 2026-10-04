//! DXF import: a tolerant, defensive ASCII reader.
//!
//! The reader accepts any DXF release, because it only looks at the ENTITIES
//! section and at LINE records, which have not changed since R12. Everything
//! else is counted and reported.

use cad_core::Point2;
use cad_document::{Drawing, Entity, LineEntity};

use crate::{DxfError, ImportReport};

/// Start of every binary DXF file.
const BINARY_SENTINEL: &[u8] = b"AutoCAD Binary DXF";
/// Records that belong to the entity before them (POLYLINE vertices, INSERT
/// attributes) and are not counted on their own.
const SUBORDINATE_RECORDS: [&str; 3] = ["VERTEX", "SEQEND", "ATTRIB"];
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
                if self.entities.len() >= self.limits.max_entities {
                    return Err(DxfError::TooManyEntities {
                        limit: self.limits.max_entities,
                    });
                }
                self.entities.push(Entity::Line(body.line(record.line)?));
                self.report.lines += 1;
                if body.has_dropped_properties {
                    self.report.with_dropped_properties += 1;
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
}

#[derive(Clone, Copy, Debug)]
enum RecordContext {
    Entities,
    Blocks,
}

/// The groups of one record that matter to this reader.
#[derive(Debug, Default)]
struct RecordBody {
    x0: Option<f64>,
    y0: Option<f64>,
    x1: Option<f64>,
    y1: Option<f64>,
    has_dropped_properties: bool,
    name_is_reserved: bool,
}

impl RecordBody {
    fn absorb(&mut self, group: Group<'_>) -> Result<(), DxfError> {
        match group.code {
            10 => self.x0 = Some(real(group)?),
            20 => self.y0 = Some(real(group)?),
            11 => self.x1 = Some(real(group)?),
            21 => self.y1 = Some(real(group)?),
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
        let need = |value: Option<f64>, group| {
            value.ok_or(DxfError::MissingCoordinate {
                line: at,
                entity: "LINE",
                group,
            })
        };
        Ok(LineEntity::new(
            Point2::new(need(self.x0, 10)?, need(self.y0, 20)?),
            Point2::new(need(self.x1, 11)?, need(self.y1, 21)?),
        ))
    }
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
            .map(|(_, Entity::Line(line))| *line)
            .collect()
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
        let mut body = vec![(0, "CIRCLE"), (10, "0"), (20, "0"), (40, "5")];
        body.extend_from_slice(&LINE_0_0_TO_10_5);
        body.extend_from_slice(&[(0, "TEXT"), (1, "hello"), (0, "CIRCLE"), (40, "1")]);
        // A polyline's vertices belong to it and are not counted separately.
        body.extend_from_slice(&[
            (0, "POLYLINE"),
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
            .map(|(k, v)| (k.as_str(), *v))
            .collect();
        assert_eq!(counts, vec![("CIRCLE", 2), ("POLYLINE", 1), ("TEXT", 1)]);
        assert!(import.report.has_unsupported_content());
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
                imported.drawing.entities().map(|(id, Entity::Line(line))| (id.raw(), *line)).collect();
            reread.sort_by_key(|(id, _)| *id);
            let reread: Vec<LineEntity> = reread.into_iter().map(|(_, line)| line).collect();
            prop_assert_eq!(reread, original);
            prop_assert!(!imported.report.has_unsupported_content());
            // And a second trip changes nothing.
            prop_assert_eq!(export(&imported.drawing), text);
        }
    }
}
