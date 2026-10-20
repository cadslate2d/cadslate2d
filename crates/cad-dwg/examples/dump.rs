//! Prints what the crate makes of a DWG file: each object's handle, type,
//! and the record it becomes, or why it could not be decoded; then the
//! records as the DXF reader receives them. For looking at a file by hand:
//!
//! ```text
//! cargo run -p cad-dwg --example dump -- file.dwg [KIND]
//! ```

use cad_dwg::objects::{fixed_type_is_entity, types};
use cad_dwg::{File, entities, tables};

fn main() {
    let path = std::env::args().nth(1).expect("a .dwg path");
    let only = std::env::args().nth(2);
    let bytes = std::fs::read(&path).expect("read");
    let file = File::open(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"));
    println!("{path}: {:?}", file.version());
    if let Ok(header) = file.header() {
        for name in [
            "CLAYER",
            "CELTYPE",
            "TEXTSTYLE",
            "DIMSTYLE",
            "INSUNITS",
            "LTSCALE",
            "CECOLOR",
            "FLAGS",
        ] {
            println!("header {name} = {:?}", header.get(name));
        }
    }
    let classes = file.classes().unwrap_or_default();
    for class in &classes {
        println!(
            "class {} = {} ({})",
            class.number,
            class.dxf_name,
            if class.is_entity() {
                "entity"
            } else {
                "object"
            }
        );
    }
    for object in file.objects() {
        let object = match object {
            Ok(o) => o,
            Err(e) => {
                println!("object: {e}");
                continue;
            }
        };
        let class = classes.iter().find(|c| c.number == object.type_code);
        let is_entity = fixed_type_is_entity(object.type_code)
            .unwrap_or_else(|| class.is_some_and(|c| c.is_entity()));
        let (_, mut streams) = match file.open_object(&object) {
            Ok(opened) => opened,
            Err(e) => {
                println!(
                    "{:x} type {:#x}: open: {e}",
                    object.handle, object.type_code
                );
                continue;
            }
        };
        if is_entity {
            match entities::decode(
                &file,
                &object,
                &mut streams,
                class.map(|c| c.dxf_name.as_str()),
            ) {
                Ok(d) => {
                    if only.as_deref().is_none_or(|k| k == d.kind) {
                        println!(
                            "{:x} {} entmode={} owner={:?} next={:?} layer={:x} first={:?} last={:?} owned={:?} groups={:?}",
                            d.handle,
                            d.kind,
                            d.entmode,
                            d.owner,
                            d.next,
                            d.layer,
                            d.first,
                            d.last,
                            d.owned,
                            d.groups.0
                        );
                    }
                }
                Err(e) => println!(
                    "{:x} type {:#x} ({:?}): {e}",
                    object.handle,
                    object.type_code,
                    class.map(|c| &c.dxf_name)
                ),
            }
            continue;
        }
        let common = match file.object_common(&mut streams, &object) {
            Ok(c) => c,
            Err(e) => {
                println!(
                    "{:x} type {:#x}: common: {e}",
                    object.handle, object.type_code
                );
                continue;
            }
        };
        let handle = object.handle;
        let result =
            match object.type_code {
                types::LAYER => {
                    tables::layer(&file, &mut streams, &common, handle).map(|r| format!("{r:?}"))
                }
                types::LTYPE => {
                    tables::linetype(&file, &mut streams, &common, handle).map(|r| format!("{r:?}"))
                }
                types::STYLE => tables::text_style(&file, &mut streams, &common, handle)
                    .map(|r| format!("{r:?}")),
                types::DIMSTYLE => tables::dim_style(&file, &mut streams, &common, handle)
                    .map(|r| format!("{r:?}")),
                types::BLOCK_HEADER => tables::block_header(&file, &mut streams, &common, handle)
                    .map(|r| format!("{r:?}")),
                other => Ok(format!(
                    "object type {other:#x} ({:?})",
                    class.map(|c| &c.dxf_name)
                )),
            };
        match result {
            Ok(text) if only.is_none() || text.contains(only.as_deref().unwrap_or("")) => {
                println!("{handle:x} {text}")
            }
            Ok(_) => {}
            Err(e) => println!("{handle:x} type {:#x}: {e}", object.type_code),
        }
    }
}
