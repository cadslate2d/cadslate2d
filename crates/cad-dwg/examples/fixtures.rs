//! Writes the DWG fixtures (ADR-0036 §3): each DXF fixture named on the
//! command line, through acadrust, in the three versions, into
//! `test-data/dwg/valid/<name>.<version>.dwg`. The source goes through
//! this project's own DXF reader and writer first, so that every row an
//! entity names is defined: a DWG holds rows by handle and has no way to
//! say a name the file never defines, and the writer would drop it. Run
//! deliberately, like the screenshot baselines, and look at what the tests
//! make of the result:
//!
//! ```text
//! cargo run -p cad-dwg --example fixtures -- minimal-line ezdxf-r2010-mixed-content
//! ```

use std::io::Cursor;
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
        let bytes = std::fs::read(&source).unwrap_or_else(|e| panic!("{}: {e}", source.display()));
        let imported = cad_dxf::import(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let normalised = cad_dxf::export(&imported.drawing);
        let doc = DxfReader::from_reader(Cursor::new(normalised.into_bytes()))
            .and_then(|r| r.read())
            .unwrap_or_else(|e| panic!("{name} through acadrust: {e}"));
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
