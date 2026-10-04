//! What an import did and did not bring in (DESIGN §24.2).

use std::collections::BTreeMap;

/// The outcome of an import, for the user.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// LINE entities imported.
    pub lines: usize,
    /// Entities that were skipped, by DXF type name, in name order.
    pub unsupported: BTreeMap<String, usize>,
    /// Imported entities that carried properties the drawing cannot hold yet
    /// (a layer other than `0`, a colour, a linetype, an elevation…).
    pub with_dropped_properties: usize,
}

impl ImportReport {
    /// Most distinct unsupported type names tracked; the rest are pooled, so a
    /// hostile file cannot grow the report without bound.
    pub(crate) const MAX_UNSUPPORTED_KINDS: usize = 64;
    pub(crate) const OTHER: &'static str = "(other)";

    /// True when exporting the imported drawing would lose something the
    /// original file contained. The UI must warn before such an export.
    pub fn has_unsupported_content(&self) -> bool {
        !self.unsupported.is_empty() || self.with_dropped_properties > 0
    }

    pub fn unsupported_total(&self) -> usize {
        self.unsupported.values().sum()
    }

    pub(crate) fn count_unsupported(&mut self, kind: &str) {
        let known = self.unsupported.contains_key(kind);
        let key = if known || self.unsupported.len() < Self::MAX_UNSUPPORTED_KINDS {
            kind
        } else {
            Self::OTHER
        };
        *self.unsupported.entry(key.to_owned()).or_default() += 1;
    }

    /// A short, user-facing summary: one line for what was imported and, when
    /// relevant, one line each for what was not.
    pub fn summary(&self) -> Vec<String> {
        let mut lines = vec![format!("Imported {}.", plural(self.lines, "line"))];
        if !self.unsupported.is_empty() {
            let kinds: Vec<String> = self
                .unsupported
                .iter()
                .map(|(kind, count)| format!("{count} {kind}"))
                .collect();
            lines.push(format!("Not imported (unsupported): {}.", kinds.join(", ")));
        }
        if self.with_dropped_properties > 0 {
            lines.push(format!(
                "Layer, colour, and linetype were dropped from {}.",
                plural(self.with_dropped_properties, "line")
            ));
        }
        lines
    }
}

fn plural(count: usize, noun: &str) -> String {
    if count == 1 {
        format!("1 {noun}")
    } else {
        format!("{count} {noun}s")
    }
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
    fn skipped_entities_are_listed_by_kind() {
        let mut report = ImportReport {
            lines: 12,
            ..Default::default()
        };
        report.count_unsupported("TEXT");
        report.count_unsupported("CIRCLE");
        report.count_unsupported("CIRCLE");

        assert!(report.has_unsupported_content());
        assert_eq!(report.unsupported_total(), 3);
        assert_eq!(
            report.summary(),
            vec![
                "Imported 12 lines.",
                "Not imported (unsupported): 2 CIRCLE, 1 TEXT."
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
        assert_eq!(
            report.summary()[1],
            "Layer, colour, and linetype were dropped from 2 lines."
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
        assert_eq!(
            report.unsupported[ImportReport::OTHER],
            1000 - ImportReport::MAX_UNSUPPORTED_KINDS
        );
    }
}
