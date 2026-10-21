//! Writes a DXF file as a DWG of a version (AC1015 unless told), for looking
//! at the result by hand:
//!
//! ```text
//! cargo run -p cad-dwg --example write -- file.dxf out.dwg [AC1015|AC1018|AC1021]
//! ```

fn version_arg(at: usize) -> cad_dwg::Version {
    match std::env::args().nth(at).as_deref() {
        None | Some("AC1015") => cad_dwg::Version::R2000,
        Some("AC1018") => cad_dwg::Version::R2004,
        Some("AC1021") => cad_dwg::Version::R2007,
        Some(other) => panic!("{other}: not a version this crate writes"),
    }
}

fn main() {
    let source = std::env::args().nth(1).expect("a .dxf path");
    let target = std::env::args().nth(2).expect("a .dwg path");
    let version = version_arg(3);
    let bytes = std::fs::read(&source).expect("read");
    let imported = cad_dxf::import(&bytes).unwrap_or_else(|e| panic!("{source}: {e}"));
    let dwg =
        cad_dwg::export(&imported.drawing, version).unwrap_or_else(|e| panic!("{source}: {e}"));
    std::fs::write(&target, &dwg).expect("write");
    println!("{target}: {} bytes", dwg.len());
    match cad_dwg::import(&dwg) {
        Ok(back) => {
            for line in back.report.summary() {
                println!("  {line}");
            }
        }
        Err(e) => println!("  not read back: {e}"),
    }
}
