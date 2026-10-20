//! The fixtures under `test-data/dwg/` (DESIGN §41, ADR-0036): every valid
//! one opens with its sections, its object map, and every object's common
//! data; every malformed one is refused with a structured error. Rows of
//! `docs/dwg-matrix.md` name these tests.

use std::fs;
use std::path::{Path, PathBuf};

use cad_dwg::objects::{fixed_type_is_entity, types};
use cad_dwg::{DwgError, File, ImportLimits, SectionName, Version};

fn fixtures(kind: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/dwg")
        .join(kind)
}

fn each(kind: &str) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<_> = fs::read_dir(fixtures(kind))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "dwg"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no {kind} fixtures");
    files
        .into_iter()
        .map(|p| {
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(&p).unwrap(),
            )
        })
        .collect()
}

fn version_of(name: &str) -> Version {
    if name.contains("AC1015") {
        Version::R2000
    } else if name.contains("AC1018") {
        Version::R2004
    } else {
        Version::R2007
    }
}

fn fixture(name: &str) -> Vec<u8> {
    fs::read(fixtures("valid").join(name)).unwrap()
}

const MIXED: [&str; 3] = [
    "ezdxf-r2010-mixed-content.AC1015.dwg",
    "ezdxf-r2010-mixed-content.AC1018.dwg",
    "ezdxf-r2010-mixed-content.AC1021.dwg",
];

const MINIMAL: [&str; 3] = [
    "minimal-line.AC1015.dwg",
    "minimal-line.AC1018.dwg",
    "minimal-line.AC1021.dwg",
];

#[test]
fn every_valid_fixture_opens_with_its_sections_and_an_increasing_object_map() {
    for (name, bytes) in each("valid") {
        let file = File::open(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(file.version(), version_of(&name), "{name}");
        let names: Vec<SectionName> = file.sections().iter().map(|s| s.name).collect();
        let expected: &[SectionName] = match file.version() {
            Version::R2000 => &[
                SectionName::Header,
                SectionName::Classes,
                SectionName::Handles,
            ],
            _ => &[
                SectionName::Header,
                SectionName::Classes,
                SectionName::Handles,
                SectionName::Objects,
                SectionName::SummaryInfo,
                SectionName::Preview,
                SectionName::AppInfo,
                SectionName::FileDepList,
                SectionName::RevHistory,
                SectionName::ObjFreeSpace,
                SectionName::Template,
                SectionName::AuxHeader,
            ],
        };
        for want in expected {
            assert!(
                names.contains(want),
                "{name}: no {want:?} section in {names:?}"
            );
        }
        for section in file.sections() {
            if section.name != SectionName::Objects {
                assert!(
                    !section.data.is_empty(),
                    "{name}: {:?} is empty",
                    section.name
                );
            }
        }
        assert!(!file.object_bytes().is_empty(), "{name}: no objects");
        let map = file.map();
        assert!(map.len() > 20, "{name}: {} objects", map.len());
        for pair in map.windows(2) {
            assert!(
                pair[0].handle < pair[1].handle,
                "{name}: handles not increasing"
            );
        }
    }
}

#[test]
fn every_object_of_every_fixture_decodes_its_header_common_data_and_handles() {
    for (name, bytes) in each("valid") {
        let file = File::open(&bytes).unwrap();
        let classes = file.classes().unwrap();
        let mut lines = 0;
        let mut layers = 0;
        for object in file.objects() {
            let object = object.unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(
                object.crc_matches,
                "{name}: object {:x} fails its CRC",
                object.handle
            );
            let (header, mut streams) = file
                .open_object(&object)
                .unwrap_or_else(|e| panic!("{name}: object {:x}: {e}", object.handle));
            assert_eq!(
                header.handle.value, object.handle,
                "{name}: the object's handle is the map's"
            );
            let Some(is_entity) = fixed_type_is_entity(object.type_code).or_else(|| {
                classes
                    .iter()
                    .find(|c| c.number == object.type_code)
                    .map(|c| c.is_entity())
            }) else {
                assert!(
                    name.starts_with("crafted-unknown-class"),
                    "{name}: type {:#x} has no class",
                    object.type_code
                );
                continue;
            };
            if is_entity {
                let common = file
                    .entity_common(&mut streams, &object)
                    .unwrap_or_else(|e| panic!("{name}: entity {:x}: {e}", object.handle));
                let handles = file
                    .entity_handles(&mut streams, &common)
                    .unwrap_or_else(|e| panic!("{name}: entity {:x} handles: {e}", object.handle));
                assert!(
                    !handles.layer.is_null(),
                    "{name}: entity {:x} has no layer",
                    object.handle
                );
                assert!(matches!(common.entmode, 0..=2));
                lines += usize::from(object.type_code == types::LINE);
            } else {
                let common = file
                    .object_common(&mut streams, &object)
                    .unwrap_or_else(|e| panic!("{name}: object {:x}: {e}", object.handle));
                file.object_handles(&mut streams, &common)
                    .unwrap_or_else(|e| panic!("{name}: object {:x} handles: {e}", object.handle));
                layers += usize::from(object.type_code == types::LAYER);
            }
        }
        let want_lines = match name.split('.').next().unwrap() {
            "minimal-line" => 1,
            "ezdxf-r2010-mixed-content" => 4,
            _ => lines,
        };
        assert_eq!(lines, want_lines, "{name}: lines");
        assert!(layers >= 1, "{name}: layers");
    }
}

#[test]
fn the_classes_of_every_fixture_name_their_dxf_kinds() {
    for (name, bytes) in each("valid") {
        let file = File::open(&bytes).unwrap();
        let classes = file.classes().unwrap_or_else(|e| panic!("{name}: {e}"));
        for class in &classes {
            assert!(
                class.number >= types::FIRST_CLASS,
                "{name}: class {class:?}"
            );
            assert!(!class.dxf_name.is_empty(), "{name}: class {class:?}");
            assert!(
                matches!(class.item_class_id, 0x1f2 | 0x1f3),
                "{name}: class {class:?}"
            );
        }
        for object in file.objects() {
            let object = object.unwrap();
            if object.type_code >= types::FIRST_CLASS && !name.starts_with("crafted-unknown-class")
            {
                assert!(
                    classes.iter().any(|c| c.number == object.type_code),
                    "{name}: type {:#x} has no class among {classes:?}",
                    object.type_code
                );
            }
        }
    }
}

#[test]
fn the_header_variables_of_every_fixture_name_the_spaces_and_the_current_layer() {
    for (name, bytes) in each("valid") {
        let file = File::open(&bytes).unwrap();
        let header = file.header().unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(header.crc_matches, "{name}: the header's CRC");
        let map = file.map();
        for var in [
            "CLAYER",
            "TEXTSTYLE",
            "CELTYPE",
            "DIMSTYLE",
            "BLOCK_CONTROL",
            "LAYER_CONTROL",
            "DICTIONARY_NAMED_OBJECTS",
            "BLOCK_RECORD_MODEL_SPACE",
            "BLOCK_RECORD_PAPER_SPACE",
            "LTYPE_BYLAYER",
            "LTYPE_CONTINUOUS",
        ] {
            let handle = header
                .handle(var)
                .unwrap_or_else(|| panic!("{name}: no {var}"));
            assert!(
                map.iter().any(|e| e.handle == handle.value),
                "{name}: {var} = {handle:?} is not in the object map"
            );
        }
        assert!(matches!(header.int("INSUNITS"), Some(0..=20)), "{name}");
        assert!(header.real("LTSCALE").is_some_and(|s| s > 0.0), "{name}");
        assert!(header.int("HANDSEED").is_none() && header.handle("HANDSEED").is_some());
        assert!(header.text("FINGERPRINTGUID").is_some(), "{name}");
        assert!(header.color("CECOLOR").is_some(), "{name}");
        assert!(header.get("LUNITS").is_some(), "{name}");
        assert!(header.values().len() > 200, "{name}");
    }
}

#[test]
fn every_malformed_fixture_is_refused_with_the_error_its_name_says() {
    for (name, bytes) in each("malformed") {
        let error = match File::open(&bytes) {
            Err(e) => e,
            Ok(file) => file
                .objects()
                .find_map(Result::err)
                .unwrap_or_else(|| panic!("{name} should be refused")),
        };
        let ok = if name.starts_with("truncated") {
            matches!(error, DwgError::Truncated { .. } | DwgError::Corrupt { .. })
        } else if name.starts_with("version") {
            matches!(error, DwgError::UnsupportedVersion { .. })
        } else if name.starts_with("bad-page-checksum") || name.starts_with("bad-header-crc") {
            matches!(error, DwgError::Checksum { .. })
        } else if name.starts_with("fuzz-") {
            // A fuzzer finding: refused somehow, never a panic or an
            // allocation the file asked for.
            true
        } else {
            panic!("{name}: no expectation for this fixture");
        };
        assert!(ok, "{name}: {error:?}");
    }
}

#[test]
fn a_version_this_crate_does_not_read_is_named_in_the_error() {
    let mut later = fixture("minimal-line.AC1015.dwg");
    later[..6].copy_from_slice(b"AC1027");
    let error = File::open(&later).err().unwrap();
    assert_eq!(
        error,
        DwgError::UnsupportedVersion {
            version: "AC1027".into()
        }
    );
    let message = error.to_string();
    assert!(
        message.contains("AC1027") && message.contains("AC1021"),
        "{message}"
    );
    assert_eq!(File::open(b"  0\nSECTION").err(), Some(DwgError::NotDwg));
}

#[test]
fn a_file_over_the_byte_limit_is_refused() {
    let bytes = fixture("minimal-line.AC1015.dwg");
    let limits = ImportLimits {
        max_bytes: 100,
        ..ImportLimits::default()
    };
    assert_eq!(
        File::open_with_limits(&bytes, &limits).err(),
        Some(DwgError::FileTooLarge {
            bytes: bytes.len(),
            limit: 100
        })
    );
}

#[test]
fn too_many_sections_are_refused() {
    for name in MINIMAL {
        let bytes = fixture(name);
        let limits = ImportLimits {
            max_sections: 2,
            ..ImportLimits::default()
        };
        assert!(
            matches!(
                File::open_with_limits(&bytes, &limits).err(),
                Some(DwgError::TooMany { limit: 2, .. })
            ),
            "{name}"
        );
    }
}

#[test]
fn too_many_pages_are_refused() {
    for name in &MINIMAL[1..] {
        let bytes = fixture(name);
        let limits = ImportLimits {
            max_pages: 3,
            ..ImportLimits::default()
        };
        assert!(
            matches!(
                File::open_with_limits(&bytes, &limits).err(),
                Some(DwgError::TooMany { limit: 3, .. })
            ),
            "{name}"
        );
    }
}

#[test]
fn a_section_over_the_byte_limit_is_refused() {
    for name in &MINIMAL[1..] {
        let bytes = fixture(name);
        let limits = ImportLimits {
            max_section_bytes: 64,
            ..ImportLimits::default()
        };
        assert!(
            matches!(
                File::open_with_limits(&bytes, &limits).err(),
                Some(DwgError::TooMany { limit: 64, .. })
            ),
            "{name}"
        );
    }
}

#[test]
fn too_many_objects_are_refused() {
    for name in MINIMAL {
        let bytes = fixture(name);
        let limits = ImportLimits {
            max_objects: 5,
            ..ImportLimits::default()
        };
        assert_eq!(
            File::open_with_limits(&bytes, &limits).err(),
            Some(DwgError::TooMany {
                what: "objects",
                limit: 5
            }),
            "{name}"
        );
    }
}

#[test]
fn an_object_over_the_byte_limit_is_refused() {
    for name in MINIMAL {
        let bytes = fixture(name);
        let limits = ImportLimits {
            max_object_bytes: 8,
            ..ImportLimits::default()
        };
        let file = File::open_with_limits(&bytes, &limits).unwrap();
        let refused = file.objects().any(|o| {
            matches!(
                o,
                Err(DwgError::TooMany {
                    what: "bytes in an object",
                    limit: 8
                })
            )
        });
        assert!(refused, "{name}");
    }
}

#[test]
fn a_string_over_the_byte_limit_is_refused() {
    for name in MIXED {
        let bytes = fixture(name);
        let limits = ImportLimits {
            max_string_bytes: 1,
            ..ImportLimits::default()
        };
        let file = File::open_with_limits(&bytes, &limits).unwrap();
        // The class names are longer than a byte.
        assert_eq!(
            file.classes().err(),
            Some(DwgError::TooMany {
                what: "bytes in a string",
                limit: 1
            }),
            "{name}"
        );
    }
}

#[test]
fn too_many_classes_are_refused() {
    for name in MIXED {
        let bytes = fixture(name);
        let file = File::open(&bytes).unwrap();
        let count = file.classes().unwrap().len();
        assert!(count >= 1, "{name}: {count} classes");
        let limits = ImportLimits {
            max_classes: count - 1,
            ..ImportLimits::default()
        };
        let file = File::open_with_limits(&bytes, &limits).unwrap();
        assert_eq!(
            file.classes().err(),
            Some(DwgError::TooMany {
                what: "classes",
                limit: count - 1
            }),
            "{name}"
        );
    }
}

/// `docs/dwg-matrix.md` names, in backticks, the tests (`a_name()`) and
/// fixtures (`a-file.dwg`) that back each row. A row cannot outlive what
/// backs it: every name must exist.
#[test]
fn the_matrix_names_only_tests_and_fixtures_that_exist() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let matrix = fs::read_to_string(root.join("docs/dwg-matrix.md")).unwrap();
    let sources: String = [
        "src/bits.rs",
        "src/crc.rs",
        "src/lz77.rs",
        "src/version.rs",
        "src/r2000.rs",
        "src/r2004.rs",
        "src/r2007.rs",
        "src/sections.rs",
        "src/objects.rs",
        "src/classes.rs",
        "src/header.rs",
        "tests/fixtures.rs",
        "tests/drawing.rs",
    ]
    .iter()
    .map(|file| fs::read_to_string(root.join("crates/cad-dwg").join(file)).unwrap())
    .collect();
    let mut checked = 0;
    for name in matrix.split('`').skip(1).step_by(2) {
        if let Some(test) = name.strip_suffix("()").filter(|test| !test.is_empty()) {
            assert!(
                sources.contains(&format!("fn {test}(")),
                "the matrix names a test that does not exist: {test}"
            );
            checked += 1;
        } else if name.ends_with(".dwg") {
            let exists = ["valid", "malformed"]
                .iter()
                .any(|kind| fixtures(kind).join(name).is_file());
            assert!(
                exists,
                "the matrix names a fixture that does not exist: {name}"
            );
            checked += 1;
        }
    }
    assert!(
        checked > 20,
        "the matrix names {checked} tests and fixtures"
    );
}
