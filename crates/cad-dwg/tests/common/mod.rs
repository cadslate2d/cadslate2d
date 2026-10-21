//! What the writer and reader tests share: the drawing as lines of text.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use cad_document::{Drawing, Entity, Linetype};

pub fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every valid DWG fixture with the DXF it was written from.
pub fn pairs() -> Vec<(String, Vec<u8>, Vec<u8>)> {
    let dir = root().join("test-data/dwg/valid");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "dwg"))
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("crafted-"))
        })
        .collect();
    files.sort();
    assert!(!files.is_empty());
    files
        .into_iter()
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let source = name.split('.').next().unwrap();
            let dxf = root()
                .join("test-data/dxf/valid")
                .join(format!("{source}.dxf"));
            (name, fs::read(&path).unwrap(), fs::read(&dxf).unwrap())
        })
        .collect()
}

/// Floats to six decimals, so that a degree that went through radians
/// and back compares equal.
pub fn rounded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(|c: char| c.is_ascii_digit() || c == '-') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e'))
            .unwrap_or(rest.len());
        let token = &rest[..end];
        match token.parse::<f64>() {
            Ok(value) if token.contains('.') || token.contains('e') => {
                let r = (value * 1e6).round() / 1e6;
                out.push_str(&format!("{r}"));
            }
            _ => out.push_str(token),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// A drawing as lines of text: its settings, its tables by name, its
/// blocks, and its entities in order, every id resolved to a name.
pub fn describe(drawing: &Drawing) -> Vec<String> {
    let mut lines = Vec::new();
    let layer_name = |id| {
        drawing
            .layers()
            .get(id)
            .map(|l| l.name.clone())
            .unwrap_or_default()
    };
    let linetype_name = |linetype: Linetype| match linetype {
        Linetype::ByLayer => "ByLayer".to_owned(),
        Linetype::ByBlock => "ByBlock".to_owned(),
        Linetype::Named(id) => drawing
            .linetypes()
            .get(id)
            .map(|l| l.name.clone())
            .unwrap_or_default(),
    };
    let describe_entity = |entity: &Entity| -> String {
        let mut text = format!("{entity:?}");
        // Ids inside an entity — a reference's block, a text's style —
        // named instead, since the two imports number rows differently.
        for (id, style) in drawing.text_styles().iter() {
            text = text.replace(
                &format!("style: {id:?},"),
                &format!("style: {},", style.name),
            );
        }
        for (id, block) in drawing.blocks().iter() {
            text = text.replace(
                &format!("block: {id:?},"),
                &format!("block: {},", block.name),
            );
        }
        rounded(&text)
    };
    // The current layer and linetype are left out: the independent writer
    // of the fixtures does not carry them into a DWG.
    lines.push(format!(
        "settings unit={:?} color={:?} lineweight={:?} ltscale={}",
        drawing.unit(),
        drawing.current_color(),
        drawing.current_lineweight(),
        drawing.ltscale()
    ));
    let mut layers: Vec<String> = drawing
        .layers()
        .iter()
        .map(|(_, l)| {
            format!(
                "layer {} color={:?} linetype={} lineweight={:?} on={} frozen={} locked={}",
                l.name,
                l.color,
                linetype_name(l.linetype),
                l.lineweight,
                l.on,
                l.frozen,
                l.locked
            )
        })
        .collect();
    layers.sort();
    lines.extend(layers);
    let mut linetypes: Vec<String> = drawing
        .linetypes()
        .iter()
        .map(|(_, l)| {
            rounded(&format!(
                "linetype {} {:?} {:?}",
                l.name, l.description, l.pattern
            ))
        })
        .collect();
    linetypes.sort();
    lines.extend(linetypes);
    let mut styles: Vec<String> = drawing
        .text_styles()
        .iter()
        .map(|(_, s)| rounded(&format!("text style {s:?}")))
        .collect();
    styles.sort();
    lines.extend(styles);
    let mut dims: Vec<String> = drawing
        .dim_styles()
        .iter()
        .map(|(_, s)| rounded(&format!("dim style {s:?}")))
        .collect();
    dims.sort();
    lines.extend(dims);
    let mut blocks: Vec<String> = drawing
        .blocks()
        .iter()
        .map(|(_, b)| {
            let entities: Vec<String> = b
                .entities
                .iter()
                .map(|r| {
                    format!(
                        "{} layer={} color={:?} linetype={} lineweight={:?}",
                        describe_entity(&r.entity),
                        layer_name(r.properties.layer),
                        r.properties.color,
                        linetype_name(r.properties.linetype),
                        r.properties.lineweight
                    )
                })
                .collect();
            rounded(&format!(
                "block {} base={:?} [{}]",
                b.name,
                b.base,
                entities.join("; ")
            ))
        })
        .collect();
    blocks.sort();
    lines.extend(blocks);
    let mut mleader_styles: Vec<String> = drawing
        .mleader_styles()
        .iter()
        .map(|(_, s)| rounded(&format!("multileader style {s:?}")))
        .collect();
    mleader_styles.sort();
    lines.extend(mleader_styles);
    let layout_name = |id| {
        drawing
            .layouts()
            .get(id)
            .map(|l| l.name.clone())
            .unwrap_or_default()
    };
    // A layout: its name, sheet, plot, and the file's block — not the
    // kept groups, which each reader keeps in its own form.
    for (_, layout) in drawing.layouts().iter() {
        lines.push(rounded(&format!(
            "layout {} paper={:?} plot={:?} block={} kept_entities={}",
            layout.name,
            layout.paper_size,
            layout.plot,
            layout.block_name,
            layout.kept_entities.len()
        )));
    }
    let mut views: Vec<String> = drawing
        .views()
        .iter()
        .map(|(_, v)| {
            rounded(&format!(
                "view {} space={} center={:?} width={} height={}",
                v.name,
                layout_name(v.space),
                v.center,
                v.width,
                v.height
            ))
        })
        .collect();
    views.sort();
    lines.extend(views);
    let mut states: Vec<String> = drawing
        .layer_states()
        .iter()
        .map(|(_, s)| rounded(&format!("layer state {s:?}")))
        .collect();
    states.sort();
    lines.extend(states);
    // A group's members by their place among the records.
    let places: Vec<_> = drawing.records().map(|(id, _)| id).collect();
    let mut groups: Vec<String> = drawing
        .groups()
        .iter()
        .map(|(_, g)| {
            let members: Vec<String> = g
                .members
                .iter()
                .map(|id| {
                    places
                        .iter()
                        .position(|p| p == id)
                        .map_or_else(|| "?".to_owned(), |at| at.to_string())
                })
                .collect();
            format!(
                "group {} description={:?} selectable={} members={}",
                g.name,
                g.description,
                g.selectable,
                members.join(",")
            )
        })
        .collect();
    groups.sort();
    lines.extend(groups);
    for (_, record) in drawing.records() {
        lines.push(format!(
            "entity {} layer={} color={:?} linetype={} lineweight={:?} space={} order={}",
            describe_entity(&record.entity),
            layer_name(record.properties.layer),
            record.properties.color,
            linetype_name(record.properties.linetype),
            record.properties.lineweight,
            layout_name(record.space),
            record.order
        ));
    }
    lines
}
