//! Exports a DXF twice through a DWG round trip and prints where the two
//! outputs differ, for the determinism test.
fn main() {
    let source = std::env::args().nth(1).expect("a .dxf path");
    let version = match std::env::args().nth(2).as_deref() {
        Some("AC1018") => cad_dwg::Version::R2004,
        Some("AC1021") => cad_dwg::Version::R2007,
        _ => cad_dwg::Version::R2000,
    };
    let bytes = std::fs::read(&source).expect("read");
    let drawing = cad_dxf::import(&bytes).unwrap().drawing;
    let first = cad_dwg::export(&drawing, version).unwrap();
    let back = cad_dwg::import(&first).unwrap().drawing;
    let second = cad_dwg::export(&back, version).unwrap();
    let a = cad_dxf::export(&drawing);
    let b = cad_dxf::export(&back);
    if a != b {
        for (i, (x, y)) in a.lines().zip(b.lines()).enumerate() {
            if x != y {
                println!("dxf line {i}: {x:?} vs {y:?}");
            }
        }
    }
    let diffs: Vec<usize> = (0..first.len().min(second.len()))
        .filter(|&i| first[i] != second[i])
        .collect();
    println!(
        "{} differing bytes; first at {:?}",
        diffs.len(),
        diffs.first()
    );
    if let Some(&at) = diffs.first() {
        println!(
            "first:  {:02x?}",
            &first[at.saturating_sub(8)..(at + 8).min(first.len())]
        );
        println!(
            "second: {:02x?}",
            &second[at.saturating_sub(8)..(at + 8).min(second.len())]
        );
        for (label, bytes) in [("first", &first), ("second", &second)] {
            let file = cad_dwg::File::open(bytes).unwrap();
            for object in file.objects().flatten() {
                if object.offset <= at && at < object.offset + object.data.len() + 4 {
                    println!(
                        "{label}: byte {at} is in object {:x} type {:#x} at {:#x}",
                        object.handle, object.type_code, object.offset
                    );
                    let records = cad_dwg::records(&file).unwrap();
                    let handle = format!("{:X}", object.handle);
                    for record in records
                        .iter()
                        .filter(|r| r.groups.iter().any(|(c, v)| *c == 5 && *v == handle))
                    {
                        println!("  {} {:?}", record.kind, record.groups);
                    }
                }
            }
        }
    }
}
