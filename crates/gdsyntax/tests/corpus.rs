//! Checks gdsyntax on real GDScript projects that are not part of the
//! repository. Point `GDSYNTAX_CORPUS` at one or more folders (separated
//! like `PATH`) and every `.gd` file under them must:
//!
//! - round-trip byte for byte through the tree, and
//! - parse without errors, since real projects only contain code Godot
//!   loads. Folders known to hold invalid scripts on purpose can be left out
//!   with `GDSYNTAX_CORPUS_EXCLUDE`, a comma-separated list of path fragments.
//!
//! ```sh
//! GDSYNTAX_CORPUS=../my-game cargo test -p gdsyntax --test corpus -- --nocapture
//! ```
//!
//! Without the variable the test does nothing, so a clean checkout and CI
//! never depend on files outside the repository.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn gd_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .is_some_and(|name| name == ".godot" || name == ".git")
            {
                continue;
            }
            gd_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "gd") {
            out.push(path);
        }
    }
}

#[test]
fn corpus_round_trips_without_errors() {
    let Some(roots) = std::env::var_os("GDSYNTAX_CORPUS") else {
        eprintln!("GDSYNTAX_CORPUS is not set; skipping");
        return;
    };
    let exclude: Vec<String> = std::env::var("GDSYNTAX_CORPUS_EXCLUDE")
        .map(|v| v.split(',').map(|s| s.trim().replace('\\', "/")).collect())
        .unwrap_or_default();

    let mut files = Vec::new();
    for root in std::env::split_paths(&roots) {
        gd_files(&root, &mut files);
    }
    files.retain(|file| {
        let shown = file.to_string_lossy().replace('\\', "/");
        !exclude
            .iter()
            .any(|part| !part.is_empty() && shown.contains(part.as_str()))
    });
    assert!(!files.is_empty(), "no .gd files under GDSYNTAX_CORPUS");

    let mut failures = Vec::new();
    let (mut lines, mut elapsed) = (0, Duration::ZERO);
    for file in &files {
        let Ok(source) = std::fs::read_to_string(file) else {
            continue; // Not UTF-8: Godot would not load it either.
        };
        lines += source.lines().count();
        let start = Instant::now();
        let parse = gdsyntax::parser::parse(&source);
        elapsed += start.elapsed();
        let tree = gdsyntax::SyntaxNode::new_root(parse.green);
        let errors = parse.errors;
        if tree.to_string() != source {
            failures.push(format!(
                "{}: the tree does not reproduce the source",
                file.display()
            ));
        }
        for error in errors {
            let line = source[..error.start].matches('\n').count() + 1;
            failures.push(format!("{}:{line}: {}", file.display(), error.message));
        }
    }
    eprintln!(
        "{} files, {lines} lines, parsed in {elapsed:.1?}, {} problems",
        files.len(),
        failures.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
