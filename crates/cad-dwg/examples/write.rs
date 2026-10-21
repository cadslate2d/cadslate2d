//! Writes a DXF file as an AC1015 DWG, for looking at the result by hand:
//!
//! ```text
//! cargo run -p cad-dwg --example write -- file.dxf out.dwg
//! ```

fn main() {
    let source = std::env::args().nth(1).expect("a .dxf path");
    let target = std::env::args().nth(2).expect("a .dwg path");
    let bytes = std::fs::read(&source).expect("read");
    let imported = cad_dxf::import(&bytes).unwrap_or_else(|e| panic!("{source}: {e}"));
    let dwg = cad_dwg::export(&imported.drawing).unwrap_or_else(|e| panic!("{source}: {e}"));
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
