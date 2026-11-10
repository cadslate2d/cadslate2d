//! The writer (ADR-0036 §4, #315): every DXF fixture, exported as a DWG
//! of this crate's, reads back as the same drawing, writes the same bytes
//! again, and is read by the independent reader with nothing lost.

use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use cad_document::Drawing;
use cad_dwg::Version;

mod common;

/// The versions the writer writes.
const VERSIONS: [Version; 3] = [Version::R2000, Version::R2004, Version::R2007];

/// Every source in every version, named by both.
fn cases(sources: &[(String, Drawing)]) -> Vec<(Version, String, &Drawing)> {
    VERSIONS
        .iter()
        .flat_map(|version| {
            sources
                .iter()
                .map(move |(name, drawing)| (*version, format!("{name}.{}", version.id()), drawing))
        })
        .collect()
}
use common::{describe, root};

/// Every DXF fixture this project's reader takes, with its name.
fn sources() -> Vec<(String, Drawing)> {
    let dir = root().join("test-data/dxf/valid");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "dxf"))
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|path| {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let bytes = fs::read(&path).unwrap();
            let import = cad_dxf::import(&bytes).ok()?;
            Some((name, import.drawing))
        })
        .collect()
}

/// The drawing as the exporter's walk leaves it: what a DWG is written
/// from, read back through the DXF of the same walk.
fn normalised(drawing: &Drawing) -> Drawing {
    cad_dxf::import(cad_dxf::export(drawing).as_bytes())
        .unwrap()
        .drawing
}

fn diff(a: &[String], b: &[String]) -> String {
    let mut out = String::new();
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        if x != y {
            out.push_str(&format!("  line {i}:\n    ours: {x}\n    theirs: {y}\n"));
        }
    }
    if a.len() != b.len() {
        out.push_str(&format!(
            "  ours has {} lines, theirs {}\n",
            a.len(),
            b.len()
        ));
        for x in a.iter().skip(b.len()) {
            out.push_str(&format!("    ours only: {x}\n"));
        }
        for y in b.iter().skip(a.len()) {
            out.push_str(&format!("    theirs only: {y}\n"));
        }
    }
    out
}

/// Export → import → export is byte-identical, and the drawing read back
/// is the drawing written, for every fixture.
#[test]
fn every_fixture_round_trips_through_our_dwg_byte_for_byte() {
    let mut failures = Vec::new();
    let sources = sources();
    for (version, name, drawing) in cases(&sources) {
        let first = cad_dwg::export(drawing, version).unwrap_or_else(|e| panic!("{name}: {e}"));
        if Version::detect(&first) != Ok(version) {
            failures.push(format!("{name}: not a file of its version"));
        }
        let back = match cad_dwg::import(&first) {
            Ok(back) => back,
            Err(e) => {
                failures.push(format!("{name}: not read back: {e}"));
                continue;
            }
        };
        let kept = reported(drawing, version);
        let expected: Vec<String> = describe(&normalised(drawing))
            .into_iter()
            .map(without_fit_points)
            .filter(&kept)
            .collect();
        let got: Vec<String> = describe(&back.drawing)
            .into_iter()
            .map(without_fit_points)
            .filter(&kept)
            .collect();
        if got != expected {
            failures.push(format!(
                "{name}: the drawing read back differs\n{}",
                diff(&got, &expected)
            ));
        }
        let second = cad_dwg::export(&back.drawing, version).unwrap();
        if second != first {
            failures.push(format!(
                "{name}: the second export differs ({} vs {} bytes)",
                second.len(),
                first.len()
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// What an AC1015 file cannot hold: a spline is by its control points or
/// by its fit points (§20.4.40), so one with both keeps its control
/// points, which are its geometry, and its fit points are read back as
/// none. The lines are compared without them.
fn without_fit_points(line: String) -> String {
    match (line.find("fit: ["), line.find("] })")) {
        (Some(from), Some(to)) if from < to => format!("{}fit: [{}", &line[..from], &line[to..]),
        _ => line,
    }
}

/// What the independent reader cannot carry back into a DXF of its own,
/// by fixture and a substring of the lines it changes, dropped from both
/// sides: a difference of its DXF, not of this crate's DWG, each checked
/// against LibreDWG's reading of the same file.
const READER_LOSSES: &[(&str, &str)] = &[
    // acadrust's DXF gives every vertex a width of zero (40, 41), which
    // overrides the constant width (43) it also writes; LibreDWG reads
    // the constant width from our file.
    ("wide-polylines", "Polyline("),
    ("multileaders", "_ARCHTICK"),
    ("ordinate-dimensions", "_ARCHTICK"),
    // acadrust's DXF says a viewport is off (68) whatever the status
    // flags (90) of our file say; LibreDWG's DXF says it is on.
    ("layouts", "Viewport("),
];

/// Keeps the lines of what `export_losses` does not name for `drawing`: a
/// current multileader style it says is lost (#431) is not compared, and
/// one it does not say is lost is.
fn reported(drawing: &Drawing, version: Version) -> impl Fn(&String) -> bool {
    let style_lost = cad_dwg::export_losses(drawing, version)
        .iter()
        .any(|loss| loss.contains("multileader style"));
    move |line: &String| !(style_lost && line.starts_with("current multileader style"))
}

/// Keeps the lines no loss listed for `name` touches.
fn keeper(name: &str, losses: &[(&str, &str)]) -> impl Fn(&String) -> bool {
    let patterns: Vec<&str> = losses
        .iter()
        .filter(|(fixture, _)| name.contains(fixture))
        .map(|(_, pattern)| *pattern)
        .collect();
    move |line: &String| !patterns.iter().any(|pattern| line.contains(pattern))
}

/// `drawing` as our DWG of `version`, read by acadrust, written back by it
/// as a DXF, and read by ours: what an independent reader makes of it.
fn through_acadrust(drawing: &Drawing, version: Version) -> Result<Drawing, String> {
    let dwg = cad_dwg::export(drawing, version).map_err(|e| e.to_string())?;
    let document = acadrust::DwgReader::from_stream(Cursor::new(dwg))
        .read()
        .map_err(|e| format!("acadrust refuses it: {e}"))?;
    let dxf = acadrust::DxfWriter::new(&document)
        .write_to_vec()
        .map_err(|e| format!("acadrust cannot write it back: {e}"))?;
    cad_dxf::import(&dxf)
        .map(|import| import.drawing)
        .map_err(|e| format!("acadrust's DXF is refused: {e}"))
}

/// The independent reader reads every fixture's DWG, and the DXF it
/// writes of it makes the same drawing: nothing lost.
#[test]
fn acadrust_reads_our_output_with_nothing_lost() {
    let mut failures = Vec::new();
    let sources = sources();
    for (version, name, drawing) in cases(&sources) {
        let theirs = match through_acadrust(drawing, version) {
            Ok(theirs) => theirs,
            Err(e) => {
                failures.push(format!("{name}: {e}"));
                continue;
            }
        };
        let keep = keeper(&name, READER_LOSSES);
        let kept = reported(drawing, version);
        let expected: Vec<String> = describe(&normalised(drawing))
            .into_iter()
            .map(without_fit_points)
            .filter(&keep)
            .filter(&kept)
            .collect();
        let got: Vec<String> = describe(&theirs)
            .into_iter()
            .map(without_fit_points)
            .filter(&keep)
            .filter(&kept)
            .collect();
        if got != expected {
            failures.push(format!(
                "{name}: through acadrust differs\n{}",
                diff(&got, &expected)
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// #319: a spline drawn through points — its control data is what a
/// spline through them has — keeps its fit points in every version, and
/// nothing is reported lost; one whose control data is its own loses the
/// fit points it also carries, and the loss is named before the export.
#[test]
fn a_spline_through_points_keeps_them_and_one_with_control_data_of_its_own_is_reported() {
    use cad_core::Point2;
    use cad_document::{Drawing, Entity, SplineEntity};
    let points = [
        Point2::new(0.0, 0.0),
        Point2::new(10.0, 5.0),
        Point2::new(20.0, -3.0),
        Point2::new(35.0, 8.0),
    ];
    let through = SplineEntity::through(&points).unwrap();
    let mut own = through.clone();
    own.control[1] = Point2::new(11.0, 6.0);
    for version in VERSIONS {
        let drawing = Drawing::from_entities([Entity::Spline(through.clone())]);
        assert_eq!(
            cad_dwg::export_losses(&drawing, version),
            Vec::<String>::new()
        );
        let bytes = cad_dwg::export(&drawing, version).unwrap();
        let back = cad_dwg::import(&bytes).unwrap().drawing;
        let (_, entity) = back.entities().next().expect("the spline");
        match entity {
            Entity::Spline(spline) => {
                assert_eq!(spline.fit, points, "{version:?}");
                assert_eq!(spline.control, through.control, "{version:?}");
            }
            other => panic!("{other:?}"),
        }

        let drawing = Drawing::from_entities([Entity::Spline(own.clone())]);
        let losses = cad_dwg::export_losses(&drawing, version);
        assert_eq!(losses.len(), 1, "{losses:?}");
        assert!(
            losses[0].starts_with("1 spline with control points of their own"),
            "{losses:?}"
        );
        let back = cad_dwg::import(&cad_dwg::export(&drawing, version).unwrap())
            .unwrap()
            .drawing;
        let (_, entity) = back.entities().next().expect("the spline");
        match entity {
            Entity::Spline(spline) => {
                assert!(spline.fit.is_empty(), "{version:?}");
                assert_eq!(spline.control, own.control, "{version:?}");
            }
            other => panic!("{other:?}"),
        }
    }
}

/// #431: the writer carries no current multileader style, so one but
/// Standard is named before the save, and the file opens with Standard.
#[test]
fn a_current_multileader_style_a_dwg_cannot_hold_is_named_before_the_save() {
    use cad_document::{MLeaderStyle, MLeaderStyleId};
    let mut drawing = Drawing::new();
    for version in VERSIONS {
        assert!(
            cad_dwg::export_losses(&drawing, version).is_empty(),
            "{version:?}"
        );
    }
    let mut tx = drawing.transaction();
    let notes = tx
        .add_mleader_style(MLeaderStyle::named("Notes ML"))
        .unwrap();
    tx.set_current_mleader_style(notes).unwrap();
    tx.commit();
    for version in VERSIONS {
        let losses = cad_dwg::export_losses(&drawing, version);
        assert_eq!(losses.len(), 1, "{version:?}: {losses:?}");
        assert!(losses[0].contains("\"Notes ML\""), "{losses:?}");
        let back = cad_dwg::import(&cad_dwg::export(&drawing, version).unwrap())
            .unwrap()
            .drawing;
        assert_eq!(
            back.settings().current_mleader_style,
            MLeaderStyleId::FIRST,
            "{version:?}: as said"
        );
    }
}

/// #428: names and text outside ASCII come back as they were from our own
/// DWG in every version — a layer, a text style, a block, a layout, and a
/// text holding a character Windows-1252 has, one it does not, one beyond
/// the first 65,536, and a literal `\U+` — with nothing reported.
#[test]
fn non_ascii_names_and_text_round_trip_in_every_version() {
    use cad_core::Point2;
    use cad_document::{
        Block, BlockReferenceEntity, Entity, EntityRecord, Layer, Layout, LineEntity, Properties,
        TextEntity, TextStyle,
    };
    let p = Point2::new;
    let content = "Größe ⌀ 5 — 𝄞 \\U+0041";
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let walls = tx.add_layer(Layer::named("Wände")).unwrap();
    let style = tx.add_text_style(TextStyle::named("Ação")).unwrap();
    let door = tx
        .define_block(Block::new(
            "Tür",
            p(0.0, 0.0),
            vec![EntityRecord::new(
                Entity::Line(LineEntity::new(p(0.0, 0.0), p(1.0, 0.0))),
                Properties::default(),
            )],
        ))
        .unwrap();
    tx.add_layout(Layout::named("Sheet A1 ü")).unwrap();
    let on_walls = Properties {
        layer: walls,
        ..Properties::default()
    };
    let mut text = TextEntity::new(p(0.0, 0.0), 2.5, 0.0, content);
    text.style = style;
    tx.insert_with(Entity::Text(text), on_walls).unwrap();
    tx.insert_with(
        Entity::BlockRef(BlockReferenceEntity::new(door, p(10.0, 0.0), 1.0, 0.0)),
        on_walls,
    )
    .unwrap();
    tx.commit();

    // What a reader holds: the names in four tables, and the one text.
    let held = |back: &Drawing, by: &str, content: &str| {
        let layers: Vec<&str> = back.layers().iter().map(|(_, l)| l.name.as_str()).collect();
        assert!(layers.contains(&"Wände"), "{by}: {layers:?}");
        assert!(back.text_styles().find("Ação").is_some(), "{by}");
        assert!(back.blocks().find("Tür").is_some(), "{by}");
        assert!(
            back.layouts().iter().any(|(_, l)| l.name == "Sheet A1 ü"),
            "{by}"
        );
        let texts: Vec<&str> = back
            .entities()
            .filter_map(|(_, e)| match e {
                Entity::Text(t) => Some(t.content.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, [content], "{by}");
    };
    for version in VERSIONS {
        assert_eq!(
            cad_dwg::export_losses(&drawing, version),
            Vec::<String>::new(),
            "{version:?}"
        );
        // An independent reader reads the same names and text (#428).
        let theirs = through_acadrust(&drawing, version).unwrap_or_else(|e| panic!("{e}"));
        // acadrust's DXF writes a literal `\U+` as it is, which a DXF
        // reader then decodes: the text is compared up to it.
        let before_the_literal = content.trim_end_matches("\\U+0041");
        let theirs_text = theirs
            .entities()
            .find_map(|(_, e)| match e {
                Entity::Text(t) => Some(t.content.clone()),
                _ => None,
            })
            .expect("the text");
        assert!(
            theirs_text.starts_with(before_the_literal),
            "{version:?} through acadrust: {theirs_text:?}"
        );
        held(
            &theirs,
            &format!("{version:?} through acadrust"),
            &theirs_text,
        );
        let bytes = cad_dwg::export(&drawing, version).unwrap();
        let import = cad_dwg::import(&bytes).unwrap();
        let back = &import.drawing;
        held(back, &format!("{version:?}"), content);
        let names = |rows: Vec<&str>| rows.into_iter().map(str::to_owned).collect::<Vec<_>>();
        assert!(
            names(back.layers().iter().map(|(_, l)| l.name.as_str()).collect())
                .contains(&"Wände".to_owned()),
            "{version:?}: {:?}",
            import.report
        );
        assert!(back.text_styles().find("Ação").is_some(), "{version:?}");
        assert!(back.blocks().find("Tür").is_some(), "{version:?}");
        assert!(
            back.layouts().iter().any(|(_, l)| l.name == "Sheet A1 ü"),
            "{version:?}"
        );
        let texts: Vec<&TextEntity> = back
            .entities()
            .filter_map(|(_, e)| match e {
                Entity::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(texts.len(), 1, "{version:?}");
        assert_eq!(texts[0].content, content, "{version:?}");
        assert_eq!(
            back.entities()
                .filter(|(_, e)| matches!(e, Entity::BlockRef(_)))
                .count(),
            1,
            "{version:?}: the reference to Tür"
        );
        let said = import.report.summary();
        assert!(
            !said
                .iter()
                .any(|line| line.contains("Not imported") || line.contains("dropped")),
            "{version:?}: {said:?}"
        );
    }
}

/// #430: a line break a DWG's length-counted text holds is read as a space,
/// as a DXF's is, and reported; the DXF saved from the drawing it opens
/// reads, and saves again as it was.
#[test]
fn a_line_break_in_dwg_text_keeps_the_dxf_saved_from_it_readable() {
    use cad_core::Point2;
    use cad_document::{Entity, TextEntity};
    let drawing = Drawing::from_entities([Entity::Text(TextEntity::new(
        Point2::new(0.0, 0.0),
        1.0,
        0.0,
        "first\nsecond",
    ))]);
    let content = |drawing: &Drawing| {
        drawing
            .entities()
            .find_map(|(_, e)| match e {
                Entity::Text(t) => Some(t.content.clone()),
                _ => None,
            })
            .expect("the text")
    };
    for version in VERSIONS {
        let opened = cad_dwg::import(&cad_dwg::export(&drawing, version).unwrap()).unwrap();
        assert_eq!(content(&opened.drawing), "first second", "{version:?}");
        assert_eq!(opened.report.with_dropped_properties, 1, "{version:?}");
        let saved = cad_dxf::export_bytes(&opened.drawing);
        let reopened = cad_dxf::import(&saved)
            .unwrap_or_else(|e| panic!("{version:?}: the DXF saved from it is refused: {e}"));
        assert_eq!(content(&reopened.drawing), "first second", "{version:?}");
        assert_eq!(
            cad_dxf::export_bytes(&reopened.drawing),
            saved,
            "{version:?}: saved again, the same file"
        );
    }
}

/// #429: a multileader read from a DWG keeps its colour and lineweight,
/// stays in its group, and leaves its space's draw order as it was — its
/// common groups go before its subclass marker, where a file has them.
#[test]
fn a_multileader_keeps_its_properties_group_and_space_s_draw_order() {
    use cad_core::Point2;
    use cad_document::{
        Color, Entity, Group, LineEntity, Lineweight, MLeaderContent, MLeaderEntity, Properties,
        WipeoutEntity,
    };
    let p = Point2::new;
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let line = tx
        .insert(LineEntity::new(p(0.0, 0.0), p(100.0, 0.0)))
        .unwrap();
    let red = Properties {
        color: Color::Index(1),
        lineweight: Lineweight::Hundredths(50),
        ..Properties::default()
    };
    let leader = tx
        .insert_with(
            Entity::MLeader(MLeaderEntity::new(
                p(10.0, 0.0),
                p(30.0, 20.0),
                MLeaderContent::Text("Note".into()),
            )),
            red,
        )
        .unwrap();
    tx.insert_on_top(WipeoutEntity::new(vec![p(40.0, -5.0), p(60.0, -5.0), p(50.0, 5.0)]).unwrap())
        .unwrap();
    tx.add_group(Group::named("G", vec![line, leader])).unwrap();
    tx.commit();

    // Each entity's kind, tier, colour, and lineweight; the group's kinds.
    let described = |drawing: &Drawing| {
        let entities: Vec<(String, i16, Color, Lineweight)> = drawing
            .records()
            .map(|(_, r)| {
                (
                    r.entity.kind_name().to_owned(),
                    r.order,
                    r.properties.color,
                    r.properties.lineweight,
                )
            })
            .collect();
        let groups: Vec<(String, Vec<String>)> = drawing
            .groups()
            .iter()
            .map(|(_, g)| {
                let mut kinds: Vec<String> = g
                    .members
                    .iter()
                    .filter_map(|id| drawing.entity(*id))
                    .map(|e| e.kind_name().to_owned())
                    .collect();
                kinds.sort();
                (g.name.clone(), kinds)
            })
            .collect();
        (entities, groups)
    };
    let expected = described(&drawing);
    assert_eq!(expected.0[1].2, Color::Index(1), "{expected:?}");
    assert_eq!(expected.0[2].1, 1, "the wipeout on top");
    for version in VERSIONS {
        let bytes = cad_dwg::export(&drawing, version).unwrap();
        let back = cad_dwg::import(&bytes).unwrap().drawing;
        assert_eq!(described(&back), expected, "{version:?}");
    }
}
