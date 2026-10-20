//! The DWG reader on bytes from strangers (ADR-0036): whatever comes in,
//! the containers, the object map, every object's header, common data,
//! and common handles, the classes, the header variables, and the whole
//! import into a drawing must answer with values or a structured error,
//! never a panic.
#![no_main]

use cad_dwg::objects::fixed_type_is_entity;
use cad_dwg::{File, ImportLimits};
use libfuzzer_sys::fuzz_target;

/// Smaller than the DXF reader's defaults, for the same reason.
fn dxf_limits() -> cad_dxf::ImportLimits {
    cad_dxf::ImportLimits {
        max_bytes: 1 << 20,
        max_entities: 10_000,
        max_table_rows: 1_000,
        max_record_groups: 200_000,
        max_value_bytes: 4 * 1024,
        ..cad_dxf::ImportLimits::default()
    }
}

/// Smaller than the defaults, so a run spends its time on structure
/// rather than on allocating what a header claims.
fn limits() -> ImportLimits {
    ImportLimits {
        max_bytes: 1 << 20,
        max_section_bytes: 1 << 22,
        max_objects: 100_000,
        max_object_bytes: 1 << 20,
        max_string_bytes: 4 * 1024,
        max_classes: 1_000,
        ..ImportLimits::default()
    }
}

fuzz_target!(|data: &[u8]| {
    let _ = cad_dwg::import_with_limits(data, &limits(), &dxf_limits());
    let Ok(file) = File::open_with_limits(data, &limits()) else {
        return;
    };
    let classes = file.classes().unwrap_or_default();
    let _ = file.header();
    for object in file.objects().flatten() {
        let Ok((_header, mut streams)) = file.open_object(&object) else {
            continue;
        };
        let is_entity = fixed_type_is_entity(object.type_code).unwrap_or_else(|| {
            classes
                .iter()
                .any(|c| c.number == object.type_code && c.is_entity())
        });
        if is_entity {
            if let Ok(common) = file.entity_common(&mut streams, &object) {
                let _ = file.entity_handles(&mut streams, &common);
            }
        } else if let Ok(common) = file.object_common(&mut streams, &object) {
            let _ = file.object_handles(&mut streams, &common);
        }
    }
});
