//! Opens every `.dwg` under the directories given and prints what the
//! import makes of each: the version, the report's summary, or the error.
//! For drawings that are not fixtures — LibreDWG's samples, a user's
//! files. `scripts/dwg-sweep.sh` wraps it.
//!
//! ```text
//! cargo run --release -p cad-dwg --example sweep -- /path/to/drawings
//! ```

use std::path::{Path, PathBuf};

fn dwg_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            dwg_files(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("dwg"))
        {
            out.push(path);
        }
    }
}

fn main() {
    let mut files = Vec::new();
    for arg in std::env::args().skip(1) {
        let path = PathBuf::from(arg);
        if path.is_dir() {
            dwg_files(&path, &mut files);
        } else {
            files.push(path);
        }
    }
    let (mut opened, mut refused) = (0, 0);
    for path in files {
        let bytes = std::fs::read(&path).expect("read");
        let started = std::time::Instant::now();
        match cad_dwg::import(&bytes) {
            Ok(import) => {
                opened += 1;
                println!(
                    "{}: {:?} in {:?}",
                    path.display(),
                    cad_dwg::Version::detect(&bytes)
                        .map(|v| v.id())
                        .unwrap_or("?"),
                    started.elapsed()
                );
                for line in import.report.summary() {
                    println!("  {line}");
                }
            }
            Err(error) => {
                refused += 1;
                println!("{}: refused: {error}", path.display());
            }
        }
    }
    println!("{opened} opened, {refused} refused");
}
