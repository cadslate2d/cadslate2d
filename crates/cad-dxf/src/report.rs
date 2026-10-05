//! What an import did and did not bring in (DESIGN §24.2).

use std::collections::BTreeMap;

use cad_core::LengthUnit;

/// The outcome of an import, for the user.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// LINE entities imported.
    pub lines: usize,
    /// CIRCLE entities imported.
    pub circles: usize,
    /// ARC entities imported.
    pub arcs: usize,
    /// POLYLINE and LWPOLYLINE entities imported.
    pub polylines: usize,
    /// Entities that were skipped, by DXF type name, in name order.
    pub unsupported: BTreeMap<String, usize>,
    /// Imported entities that carried properties the drawing cannot hold yet
    /// (a layer other than `0`, a colour, a linetype, an elevation…).
    pub with_dropped_properties: usize,
    /// The drawing unit the file declares, when it is one the drawing can
    /// be in.
    pub unit: Option<LengthUnit>,
    /// The `$INSUNITS` code of a unit the file declares and the drawing
    /// cannot be in — miles, microns — so that the drawing came in unitless.
    pub unsupported_unit: Option<i64>,
}

impl ImportReport {
    /// Most distinct unsupported type names tracked; the rest are pooled, so a
    /// hostile file cannot grow the report without bound.
    pub(crate) const MAX_UNSUPPORTED_KINDS: usize = 64;
    pub(crate) const OTHER: &'static str = "(other)";

    /// True when exporting the imported drawing would lose something the
    /// original file contained. The UI must warn before such an export.
    pub fn has_unsupported_content(&self) -> bool {
        !self.unsupported.is_empty()
            || self.with_dropped_properties > 0
            || self.unsupported_unit.is_some()
    }

    pub fn unsupported_total(&self) -> usize {
        self.unsupported.values().sum()
    }

    /// Entities brought into the drawing.
    pub fn imported_total(&self) -> usize {
        self.lines + self.circles + self.arcs + self.polylines
    }

    pub(crate) fn count_unsupported(&mut self, kind: &str) {
        let known = self.unsupported.contains_key(kind);
        let room = self.unsupported.len() < Self::MAX_UNSUPPORTED_KINDS;
        let key = if known || room { kind } else { Self::OTHER };
        *self.unsupported.entry(key.to_owned()).or_default() += 1;
    }

    /// A short, user-facing summary: one line for what was imported and, when
    /// relevant, one line each for what was not.
    pub fn summary(&self) -> Vec<String> {
        let kinds = [
            (self.lines, "line", "lines"),
            (self.circles, "circle", "circles"),
            (self.arcs, "arc", "arcs"),
            (self.polylines, "polyline", "polylines"),
        ];
        let imported: Vec<String> = kinds
            .into_iter()
            .filter(|(count, ..)| *count > 0)
            .map(|(count, one, many)| counted(count, one, many))
            .collect();

        let mut lines = vec![if imported.is_empty() {
            "Imported nothing: the file has no supported entities.".to_owned()
        } else {
            format!("Imported {}.", imported.join(", "))
        }];
        if !self.unsupported.is_empty() {
            let skipped: Vec<String> = self
                .unsupported
                .iter()
                .map(|(kind, count)| format!("{count} {kind}"))
                .collect();
            lines.push(format!(
                "Not imported (unsupported): {}.",
                skipped.join(", ")
            ));
        }
        if self.with_dropped_properties > 0 {
            let entities = counted(self.with_dropped_properties, "entity", "entities");
            lines.push(format!(
                "Layer, colour, and linetype were dropped from {entities}."
            ));
        }
        if let Some(unit) = self.unit {
            lines.push(format!("Drawing unit: {}.", unit.name()));
        }
        if let Some(code) = self.unsupported_unit {
            lines.push(format!(
                "The file's drawing unit (code {code}) is not supported: the drawing is unitless."
            ));
        }
        lines
    }
}

fn counted(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_import_has_a_one_line_summary_and_no_warning() {
        let report = ImportReport {
            lines: 1,
            ..Default::default()
        };
        assert!(!report.has_unsupported_content());
        assert_eq!(report.summary(), vec!["Imported 1 line."]);
    }

    #[test]
    fn every_imported_kind_is_named() {
        let report = ImportReport {
            lines: 3,
            circles: 1,
            arcs: 2,
            polylines: 1,
            ..Default::default()
        };
        assert_eq!(report.imported_total(), 7);
        assert_eq!(
            report.summary(),
            vec!["Imported 3 lines, 1 circle, 2 arcs, 1 polyline."]
        );
    }

    #[test]
    fn an_import_with_nothing_supported_says_so() {
        let summary = ImportReport::default().summary();
        assert_eq!(
            summary,
            vec!["Imported nothing: the file has no supported entities."]
        );
    }

    #[test]
    fn skipped_entities_are_listed_by_kind() {
        let mut report = ImportReport {
            lines: 12,
            ..Default::default()
        };
        report.count_unsupported("TEXT");
        report.count_unsupported("HATCH");
        report.count_unsupported("HATCH");

        assert!(report.has_unsupported_content());
        assert_eq!(report.unsupported_total(), 3);
        assert_eq!(
            report.summary(),
            vec![
                "Imported 12 lines.",
                "Not imported (unsupported): 2 HATCH, 1 TEXT."
            ]
        );
    }

    #[test]
    fn dropped_properties_alone_make_an_export_lossy() {
        let report = ImportReport {
            lines: 3,
            with_dropped_properties: 2,
            ..Default::default()
        };
        assert!(report.has_unsupported_content());
        let summary = report.summary();
        assert_eq!(
            summary[1],
            "Layer, colour, and linetype were dropped from 2 entities."
        );

        let one = ImportReport {
            circles: 1,
            with_dropped_properties: 1,
            ..Default::default()
        };
        assert_eq!(
            one.summary()[1],
            "Layer, colour, and linetype were dropped from 1 entity."
        );
    }

    #[test]
    fn distinct_unsupported_kinds_are_bounded() {
        let mut report = ImportReport::default();
        for kind in 0..1000 {
            report.count_unsupported(&format!("KIND{kind}"));
        }
        assert_eq!(
            report.unsupported.len(),
            ImportReport::MAX_UNSUPPORTED_KINDS + 1
        );
        assert_eq!(report.unsupported_total(), 1000);
        let pooled = 1000 - ImportReport::MAX_UNSUPPORTED_KINDS;
        assert_eq!(report.unsupported[ImportReport::OTHER], pooled);
    }
}
