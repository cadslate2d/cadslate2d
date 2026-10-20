//! The DWG reader on bytes from strangers (ADR-0036): whatever comes in,
//! the containers, the object map, every object's header, common data,
//! and common handles, the classes, and the header variables must answer
//! with values or a structured error, never a panic.
#![no_main]

use cad_dwg::objects::fixed_type_is_entity;
use cad_dwg::{File, ImportLimits};
use libfuzzer_sys::fuzz_target;

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
