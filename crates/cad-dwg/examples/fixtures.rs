//! Writes the DWG fixtures (ADR-0036 §3): each DXF fixture named on the
//! command line, through acadrust, in the three versions, into
//! `test-data/dwg/valid/<name>.<version>.dwg`. Run deliberately, like the
//! screenshot baselines, and look at what the tests make of the result:
//!
//! ```text
//! cargo run -p cad-dwg --example fixtures -- minimal-line ezdxf-r2010-mixed-content
//! ```

use std::path::Path;

use acadrust::{DwgWriter, DxfReader, DxfVersion};

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let names: Vec<String> = std::env::args().skip(1).collect();
    if names.is_empty() {
        eprintln!("usage: fixtures <dxf fixture name without .dxf>...");
        std::process::exit(2);
    }
    for name in names {
        let source = root.join("test-data/dxf/valid").join(format!("{name}.dxf"));
        let doc = DxfReader::from_file(&source)
            .and_then(|r| r.read())
            .unwrap_or_else(|e| panic!("{}: {e}", source.display()));
        for version in [DxfVersion::AC1015, DxfVersion::AC1018, DxfVersion::AC1021] {
            let mut doc = doc.clone();
            doc.version = version;
            let bytes = DwgWriter::write_to_vec(&doc)
                .unwrap_or_else(|e| panic!("{name} as {version:?}: {e}"));
            let target = root
                .join("test-data/dwg/valid")
                .join(format!("{name}.{version:?}.dwg"));
            std::fs::write(&target, &bytes).unwrap();
            println!("{}: {} bytes", target.display(), bytes.len());
        }
    }
}
