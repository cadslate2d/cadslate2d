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
    pub ellipses: usize,
    pub splines: usize,
    /// Attribute definitions of the drawing itself: a block's are its own.
    pub attribute_definitions: usize,
    /// Viewports of the layouts (#192).
    pub viewports: usize,
    /// Points (#198).
    pub points: usize,
    /// Leaders (#205).
    pub leaders: usize,
    /// Xlines and rays (#206).
    pub construction_lines: usize,
    /// Whether the file gave its points a size relative to the screen —
    /// `$PDSIZE` zero or negative — which the drawing does not hold: its
    /// points are drawn at the default size (#198).
    pub point_size_relative: bool,
    /// Layouts with something on their paper — entities of the drawing
    /// since #191, or records kept as the file held them — by name. An
    /// empty layout is read without a word.
    pub layouts: Vec<String>,
    /// POLYLINE and LWPOLYLINE entities imported.
    pub polylines: usize,
    /// TEXT and MTEXT entities imported.
    pub texts: usize,
    /// INSERT entities imported into the drawing itself (those inside
    /// block definitions are part of their block).
    pub block_references: usize,
    /// DIMENSION entities imported.
    pub dimensions: usize,
    /// HATCH entities imported.
    pub hatches: usize,
    /// Entities that were skipped, by DXF type name, in name order.
    pub unsupported: BTreeMap<String, usize>,
    /// Imported entities that carried properties the drawing cannot hold
    /// (an elevation, a thickness, a width, a linetype scale of their own,
    /// a colour or lineweight that is not one).
    pub with_dropped_properties: usize,
    /// Layers besides `0` in the drawing after the import.
    pub layers: usize,
    /// Linetypes besides `Continuous` in the drawing after the import.
    pub linetypes: usize,
    /// Layers that an entity or the header named but the file did not
    /// define, made with a new layer's settings.
    pub created_layers: usize,
    /// Linetypes that an entity or a layer named but the file did not
    /// define, made continuous.
    pub created_linetypes: usize,
    /// Text styles besides `Standard` in the drawing after the import.
    pub text_styles: usize,
    /// Text styles a text named but the file did not define.
    pub created_text_styles: usize,
    /// Dimension styles besides `Standard` in the drawing after the import.
    pub dim_styles: usize,
    /// Dimension styles a dimension named but the file did not define.
    pub created_dim_styles: usize,
    /// Block definitions in the drawing after the import.
    pub blocks: usize,
    /// Groups read (#214).
    pub groups: usize,
    /// MTEXT entities whose formatting — fonts, heights, colours,
    /// stacking — was dropped, their words and breaks kept.
    pub formatting_dropped: usize,
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
            || self.formatting_dropped > 0
            || self.unsupported_unit.is_some()
    }

    pub fn unsupported_total(&self) -> usize {
        self.unsupported.values().sum()
    }

    /// Entities brought into the drawing.
    pub fn imported_total(&self) -> usize {
        self.lines
            + self.circles
            + self.arcs
            + self.ellipses
            + self.splines
            + self.attribute_definitions
            + self.polylines
            + self.texts
            + self.block_references
            + self.dimensions
            + self.hatches
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
            (self.ellipses, "ellipse", "ellipses"),
            (self.splines, "spline", "splines"),
            (
                self.attribute_definitions,
                "attribute definition",
                "attribute definitions",
            ),
            (self.polylines, "polyline", "polylines"),
            (self.texts, "text", "texts"),
            (self.block_references, "block reference", "block references"),
            (self.dimensions, "dimension", "dimensions"),
            (self.hatches, "hatch", "hatches"),
            (self.viewports, "viewport", "viewports"),
            (self.points, "point", "points"),
            (self.leaders, "leader", "leaders"),
            (
                self.construction_lines,
                "construction line",
                "construction lines",
            ),
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
        if !self.layouts.is_empty() {
            lines.push(format!(
                "{}: {}.",
                counted(self.layouts.len(), "layout", "layouts"),
                self.layouts.join(", ")
            ));
        }
        lines.extend(self.unsupported_line());
        // Said only when there are points for it to matter to.
        if self.point_size_relative && self.points > 0 {
            lines.push(
                "The points' size is relative to the screen in the file, which is not held: they are drawn at the default size."
                    .to_owned(),
            );
        }
        if self.layers
            + self.linetypes
            + self.text_styles
            + self.dim_styles
            + self.blocks
            + self.groups
            > 0
        {
            let tables: Vec<String> = [
                (self.layers, "layer", "layers"),
                (self.linetypes, "linetype", "linetypes"),
                (self.text_styles, "text style", "text styles"),
                (self.dim_styles, "dimension style", "dimension styles"),
                (self.blocks, "block", "blocks"),
                (self.groups, "group", "groups"),
            ]
            .into_iter()
            .filter(|(count, ..)| *count > 0)
            .map(|(count, one, many)| counted(count, one, many))
            .collect();
            lines.push(format!("With {}.", joined(&tables)));
        }
        if self.created_layers
            + self.created_linetypes
            + self.created_text_styles
            + self.created_dim_styles
            > 0
        {
            let created: Vec<String> = [
                (self.created_layers, "layer", "layers"),
                (self.created_linetypes, "linetype", "linetypes"),
                (self.created_text_styles, "text style", "text styles"),
                (
                    self.created_dim_styles,
                    "dimension style",
                    "dimension styles",
                ),
            ]
            .into_iter()
            .filter(|(count, ..)| *count > 0)
            .map(|(count, one, many)| counted(count, one, many))
            .collect();
            lines.push(format!(
                "Named but not defined in the file, so created: {}.",
                joined(&created)
            ));
        }
        lines.extend(self.formatting_line());
        lines.extend(self.dropped_line());
        if let Some(unit) = self.unit {
            lines.push(format!("Drawing unit: {}.", unit.name()));
        }
        lines.extend(self.unit_line());
        lines
    }

    /// What an export of the imported drawing will not include: the lines
    /// of the summary that name content the drawing could not hold. Empty
    /// exactly when `has_unsupported_content` is false (§24.2).
    pub fn losses(&self) -> Vec<String> {
        [
            self.unsupported_line(),
            self.formatting_line(),
            self.dropped_line(),
            self.unit_line(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    fn unsupported_line(&self) -> Option<String> {
        if self.unsupported.is_empty() {
            return None;
        }
        let skipped: Vec<String> = self
            .unsupported
            .iter()
            .map(|(kind, count)| format!("{count} {kind}"))
            .collect();
        Some(format!(
            "Not imported (unsupported): {}.",
            skipped.join(", ")
        ))
    }

    fn formatting_line(&self) -> Option<String> {
        (self.formatting_dropped > 0).then(|| {
            let texts = counted(self.formatting_dropped, "paragraph", "paragraphs");
            format!(
                "Formatting — fonts, heights, colours, stacking — was dropped from {texts}; the words and line breaks were kept."
            )
        })
    }

    fn dropped_line(&self) -> Option<String> {
        (self.with_dropped_properties > 0).then(|| {
            let entities = counted(self.with_dropped_properties, "entity", "entities");
            format!("Properties the drawing cannot hold were dropped from {entities}.")
        })
    }

    fn unit_line(&self) -> Option<String> {
        self.unsupported_unit.map(|code| {
            format!(
                "The file's drawing unit (code {code}) is not supported: the drawing is unitless."
            )
        })
    }
}

fn counted(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// "a", "a and b", "a, b, and c".
fn joined(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [rest @ .., last] => format!("{}, and {last}", rest.join(", ")),
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
    fn the_losses_are_the_summary_lines_that_name_what_was_not_held() {
        let mut report = ImportReport {
            lines: 2,
            layers: 1,
            with_dropped_properties: 1,
            unsupported_unit: Some(9),
            ..Default::default()
        };
        report.count_unsupported("HATCH");
        let losses = report.losses();
        assert_eq!(
            losses,
            vec![
                "Not imported (unsupported): 1 HATCH.",
                "Properties the drawing cannot hold were dropped from 1 entity.",
                "The file's drawing unit (code 9) is not supported: the drawing is unitless.",
            ]
        );
        assert!(losses.iter().all(|line| report.summary().contains(line)));
        assert!(
            ImportReport {
                lines: 1,
                layers: 2,
                ..Default::default()
            }
            .losses()
            .is_empty()
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
            "Properties the drawing cannot hold were dropped from 2 entities."
        );

        let one = ImportReport {
            circles: 1,
            with_dropped_properties: 1,
            ..Default::default()
        };
        assert_eq!(
            one.summary()[1],
            "Properties the drawing cannot hold were dropped from 1 entity."
        );
    }

    #[test]
    fn tables_are_counted_and_rows_the_file_did_not_define_are_named() {
        let report = ImportReport {
            lines: 1,
            layers: 2,
            linetypes: 1,
            created_layers: 1,
            ..Default::default()
        };
        assert!(!report.has_unsupported_content(), "nothing was lost");
        assert_eq!(
            report.summary(),
            vec![
                "Imported 1 line.",
                "With 2 layers and 1 linetype.",
                "Named but not defined in the file, so created: 1 layer."
            ]
        );
        let only_linetypes = ImportReport {
            linetypes: 3,
            created_linetypes: 2,
            ..Default::default()
        };
        assert_eq!(
            only_linetypes.summary()[1..],
            [
                "With 3 linetypes.",
                "Named but not defined in the file, so created: 2 linetypes."
            ]
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
