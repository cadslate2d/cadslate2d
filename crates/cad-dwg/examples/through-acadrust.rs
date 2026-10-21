//! Writes a DXF as our DWG, reads that through acadrust, and writes
//! acadrust's DXF of it: what the independent reader made of ours.
//!
//! ```text
//! cargo run -p cad-dwg --example through-acadrust -- file.dxf out.dwg out.dxf [AC1015|AC1018]
//! ```
use std::io::Cursor;

fn main() {
    let source = std::env::args().nth(1).expect("a .dxf path");
    let dwg_path = std::env::args().nth(2).expect("a .dwg path");
    let dxf_path = std::env::args().nth(3).expect("a .dxf output path");
    let version = match std::env::args().nth(4).as_deref() {
        Some("AC1018") => cad_dwg::Version::R2004,
        _ => cad_dwg::Version::R2000,
    };
    let bytes = std::fs::read(&source).expect("read");
    let drawing = cad_dxf::import(&bytes).unwrap().drawing;
    let dwg = cad_dwg::export(&drawing, version).unwrap();
    std::fs::write(&dwg_path, &dwg).unwrap();
    let document = acadrust::DwgReader::from_stream(Cursor::new(dwg))
        .read()
        .unwrap();
    let dxf = acadrust::DxfWriter::new(&document).write_to_vec().unwrap();
    std::fs::write(&dxf_path, &dxf).unwrap();
    println!("{dxf_path}: {} bytes", dxf.len());
}
