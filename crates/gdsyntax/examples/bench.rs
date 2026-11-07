//! Time the tokenizer and the parser on folders of GDScript files.
//!
//! ```sh
//! cargo run --release -p gdsyntax --example bench -- <folder>...
//! ```

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn gd_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            gd_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "gd") {
            out.push(path);
        }
    }
}

fn main() {
    let mut files = Vec::new();
    for root in std::env::args().skip(1) {
        gd_files(Path::new(&root), &mut files);
    }
    let sources: Vec<String> = files
        .iter()
        .filter_map(|f| std::fs::read_to_string(f).ok())
        .collect();
    let lines: usize = sources.iter().map(|s| s.lines().count()).sum();

    let time = |f: &dyn Fn(&str)| {
        let mut best = Duration::MAX;
        for _ in 0..5 {
            let start = Instant::now();
            for source in &sources {
                f(source);
            }
            best = best.min(start.elapsed());
        }
        best
    };
    let tokenize = time(&|s| {
        std::hint::black_box(gdsyntax::lexer::tokenize(s));
    });
    let parse = time(&|s| {
        std::hint::black_box(gdsyntax::parser::parse(s));
    });
    println!(
        "{} files, {lines} lines: tokenize {tokenize:.1?}, parse {parse:.1?} (best of 5, one thread)",
        sources.len()
    );
}
