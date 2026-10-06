//! Reads a DXF file and writes it back as CadSlate2D would: for checking
//! the writer's output with other tools.
fn main() {
    let path = std::env::args().nth(1).expect("a DXF file");
    let bytes = std::fs::read(&path).expect("readable");
    let import = cad_dxf::import(&bytes).expect("a valid file");
    print!("{}", cad_dxf::export(&import.drawing));
}
