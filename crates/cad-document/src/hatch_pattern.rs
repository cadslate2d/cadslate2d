//! The hatch patterns (DESIGN §34; #168): the classic set, shipped as
//! data in the `.pat` format drafting programs use, metric, with `SOLID`
//! as the permanent first row. A pattern is line families
//! ([`LineFamily`]); a hatch names one and places it at an angle and a
//! scale. The table is the program's, not the drawing's: a hatch that
//! names a pattern the table lacks cannot be stored.

use std::sync::OnceLock;

use cad_core::Point2;
use cad_geometry::LineFamily;

use crate::names::same_name;

/// A hatch pattern: a name, and the families its lines come in. `SOLID`
/// has none: it is filled.
#[derive(Clone, Debug, PartialEq)]
pub struct HatchPattern {
    pub name: String,
    pub description: String,
    pub families: Vec<LineFamily>,
}

impl HatchPattern {
    /// The name of the fill that is not lines.
    pub const SOLID: &'static str = "SOLID";

    pub fn is_solid(&self) -> bool {
        self.families.is_empty()
    }
}

/// The patterns, in millimetres, as `acadiso.pat` gives them: a `*NAME,
/// description` line, then one line per family — angle in degrees, the
/// origin's x and y, the shift along and the spacing across, then the
/// dashes.
const PATTERNS: &str = "\
*ANSI31, ANSI iron, brick, stone masonry
45, 0,0, 0,3.175
*ANSI32, ANSI steel
45, 0,0, 0,9.525
45, 4.49013,0, 0,9.525
*ANSI33, ANSI bronze, brass, copper
45, 0,0, 0,6.35
45, 4.49013,0, 0,6.35, 3.175,-1.5875
*ANSI34, ANSI plastic, rubber
45, 0,0, 0,19.05
45, 4.49013,0, 0,19.05
45, 8.98026,0, 0,19.05
45, 13.4704,0, 0,19.05
*ANSI35, ANSI fire brick, refractory material
45, 0,0, 0,6.35
45, 4.49013,0, 0,6.35, 7.9375,-1.5875,0,-1.5875
*ANSI36, ANSI marble, slate, glass
45, 0,0, 5.55625,3.175, 7.9375,-1.5875,0,-1.5875
*ANSI37, ANSI lead, zinc, magnesium, sound/heat/elec insulation
45, 0,0, 0,3.175
135, 0,0, 0,3.175
*ANSI38, ANSI aluminum
45, 0,0, 0,3.175
135, 0,0, 6.35,3.175, 7.9375,-4.7625
*BRICK, Brick or masonry-type surface
0, 0,0, 0,6.35
90, 0,0, 0,12.7, 6.35,-6.35
90, 6.35,0, 0,12.7, -6.35,6.35
*DOTS, A series of dots
0, 0,0, 0.79375,1.5875, 0,-1.5875
*EARTH, Earth or ground (subterranean)
0, 0,0, 6.35,6.35, 6.35,-6.35
0, 0,2.38125, 6.35,6.35, 6.35,-6.35
0, 0,4.7625, 6.35,6.35, 6.35,-6.35
90, 0.79375,5.55625, 6.35,6.35, 6.35,-6.35
90, 3.175,5.55625, 6.35,6.35, 6.35,-6.35
90, 5.55625,5.55625, 6.35,6.35, 6.35,-6.35
*GRASS, Grass area
90, 0,0, 17.9605,17.9605, 4.7625,-31.1585
45, 0,0, 0,25.4, 4.7625,-20.6375
135, 0,0, 0,25.4, 4.7625,-20.6375
*HONEY, Honeycomb pattern
0, 0,0, 4.7625,2.74963, 3.175,-6.35
120, 0,0, 4.7625,2.74963, 3.175,-6.35
60, 0,0, 4.7625,2.74963, -6.35,3.175
*LINE, Parallel horizontal lines
0, 0,0, 0,3.175
*NET, Horizontal / vertical grid
0, 0,0, 0,3.175
90, 0,0, 0,3.175
*NET3, Network pattern 0-60-120
0, 0,0, 0,3.175
60, 0,0, 0,3.175
120, 0,0, 0,3.175
*STEEL, Steel material
45, 0,0, 0,3.175
45, 0,1.5875, 0,3.175
";

/// Why a `.pat` text could not be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatternSyntax {
    pub line: usize,
}

/// Reads patterns in the `.pat` format. Blank lines and `;` comments are
/// skipped.
pub fn parse_patterns(text: &str) -> Result<Vec<HatchPattern>, PatternSyntax> {
    let mut patterns: Vec<HatchPattern> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let error = PatternSyntax { line: index + 1 };
        if let Some(header) = line.strip_prefix('*') {
            let (name, description) = header.split_once(',').unwrap_or((header, ""));
            patterns.push(HatchPattern {
                name: name.trim().to_owned(),
                description: description.trim().to_owned(),
                families: Vec::new(),
            });
            continue;
        }
        let numbers: Vec<f64> = line
            .split(',')
            .map(|word| word.trim().parse::<f64>())
            .collect::<Result<_, _>>()
            .map_err(|_| error)?;
        let pattern = patterns.last_mut().ok_or(error)?;
        let [angle, x, y, shift, spacing, dashes @ ..] = numbers.as_slice() else {
            return Err(error);
        };
        pattern.families.push(LineFamily {
            angle: angle.to_radians(),
            origin: Point2::new(*x, *y),
            shift: *shift,
            spacing: *spacing,
            dashes: dashes.to_vec(),
        });
    }
    Ok(patterns)
}

/// Every pattern, `SOLID` first, then the classic set in name order.
pub fn hatch_patterns() -> &'static [HatchPattern] {
    static TABLE: OnceLock<Vec<HatchPattern>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = vec![HatchPattern {
            name: HatchPattern::SOLID.to_owned(),
            description: "Solid fill".to_owned(),
            families: Vec::new(),
        }];
        table.extend(parse_patterns(PATTERNS).expect("the bundled patterns are well formed"));
        table
    })
}

/// The pattern called `name`, whatever its case.
pub fn hatch_pattern(name: &str) -> Option<&'static HatchPattern> {
    hatch_patterns()
        .iter()
        .find(|pattern| same_name(&pattern.name, name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_has_solid_first_and_the_classic_set() {
        let names: Vec<&str> = hatch_patterns().iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names[0], "SOLID");
        for name in [
            "ANSI31", "ANSI32", "ANSI33", "ANSI34", "ANSI35", "ANSI36", "ANSI37", "ANSI38",
            "BRICK", "DOTS", "EARTH", "GRASS", "HONEY", "LINE", "NET", "NET3", "STEEL",
        ] {
            assert!(names.contains(&name), "{name}");
        }
        assert!(hatch_pattern("solid").unwrap().is_solid());
        let ansi31 = hatch_pattern("ansi31").unwrap();
        assert_eq!(ansi31.families.len(), 1);
        assert!((ansi31.families[0].angle - 45f64.to_radians()).abs() < 1e-12);
        assert_eq!(ansi31.families[0].spacing, 3.175);
        assert_eq!(
            hatch_pattern("ANSI38").unwrap().families[1].dashes,
            [7.9375, -4.7625]
        );
        assert!(hatch_pattern("NOPE").is_none());
        // Every family's lines are apart: none would draw nothing.
        assert!(
            hatch_patterns()
                .iter()
                .flat_map(|p| &p.families)
                .all(|family| family.spacing > 0.0)
        );
    }

    #[test]
    fn a_pat_text_is_read_and_a_bad_one_refused_by_line() {
        let patterns = parse_patterns("; a comment\n*ONE, a test\n0, 1,2, 3,4, 5,-6\n\n").unwrap();
        assert_eq!(patterns.len(), 1);
        assert_eq!(patterns[0].description, "a test");
        assert_eq!(patterns[0].families[0].dashes, [5.0, -6.0]);
        assert_eq!(
            parse_patterns("0, 0,0, 0,1"),
            Err(PatternSyntax { line: 1 }),
            "no header"
        );
        assert_eq!(
            parse_patterns("*A\n0, 0,0, 0"),
            Err(PatternSyntax { line: 2 }),
            "too few"
        );
        assert_eq!(
            parse_patterns("*A\n0, x,0, 0,1"),
            Err(PatternSyntax { line: 2 }),
            "not a number"
        );
    }
}
