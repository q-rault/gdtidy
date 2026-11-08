//! Comparison with gdlint, off by default. Run it with:
//!
//! ```sh
//! GDTIDY_PARITY=1 GDTIDY_PARITY_CORPUS="<folder>;<folder>" cargo test --test parity -- --nocapture
//! ```
//!
//! For every `.gd` file under the folders (skipping hidden folders and
//! `addons`, as gdtidy does), both tools run with their defaults: gdlint
//! from an empty folder, so no gdlintrc applies, and gdtidy as is. Folders
//! must not hold a `gdtidy.toml`. `GDLINT` names the gdlint to run (default:
//! `gdlint` on the PATH).
//!
//! The test fails if gdlint reports something gdtidy does not. What only
//! gdtidy reports is printed, to check against docs/gdlint-differences.md.
//! `max-line-length` is left out: its tab width differs on purpose.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

/// gdlint's rules that gdtidy keeps, with the same default values.
const RULES: &[&str] = &[
    "function-name",
    "class-name",
    "sub-class-name",
    "signal-name",
    "class-variable-name",
    "class-load-variable-name",
    "function-variable-name",
    "function-preload-variable-name",
    "function-argument-name",
    "loop-variable-name",
    "enum-name",
    "enum-element-name",
    "constant-name",
    "load-constant-name",
    "class-definitions-order",
    "max-returns",
    "function-arguments-number",
    "max-public-methods",
    "max-file-lines",
    "duplicated-load",
    "comparison-with-itself",
    "no-elif-return",
    "no-else-return",
    "unnecessary-pass",
];

/// (file, line, rule) → how many times it is reported.
type Findings = BTreeMap<(PathBuf, usize, String), usize>;

fn gd_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if path.is_dir() {
            if !name.starts_with('.') && name != "addons" {
                gd_files(&path, out);
            }
        } else if name.ends_with(".gd") {
            out.push(path);
        }
    }
}

fn gdtidy(files: &[PathBuf]) -> (Findings, Vec<PathBuf>) {
    let mut found = Findings::new();
    let mut skipped = Vec::new();
    for chunk in files.chunks(100) {
        let output = Command::new(env!("CARGO_BIN_EXE_gdtidy"))
            .args(["lint", "--format", "json"])
            .args(chunk)
            .output()
            .unwrap();
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        for s in report["skipped"].as_array().unwrap() {
            skipped.push(PathBuf::from(s["path"].as_str().unwrap()));
        }
        for d in report["diagnostics"].as_array().unwrap() {
            let rule = d["rule"].as_str().unwrap();
            if RULES.contains(&rule) {
                let key = (
                    normalize(Path::new(d["path"].as_str().unwrap())),
                    d["line"].as_u64().unwrap() as usize,
                    rule.to_string(),
                );
                *found.entry(key).or_default() += 1;
            }
        }
    }
    (found, skipped.iter().map(|p| normalize(p)).collect())
}

/// gdlint's findings, and the files it could not parse.
fn gdlint(files: &[PathBuf], empty: &Path) -> (Findings, Vec<PathBuf>) {
    let program = std::env::var("GDLINT").unwrap_or_else(|_| "gdlint".to_string());
    let found = Mutex::new(Findings::new());
    let failed = Mutex::new(Vec::new());
    let next = Mutex::new(0usize);
    std::thread::scope(|scope| {
        for _ in 0..8 {
            scope.spawn(|| {
                loop {
                    let index = {
                        let mut next = next.lock().unwrap();
                        *next += 1;
                        *next - 1
                    };
                    let Some(file) = files.get(index) else { break };
                    let output = Command::new(&program)
                        .arg(file)
                        .current_dir(empty)
                        .output()
                        .expect("cannot run gdlint; set GDLINT");
                    let text = String::from_utf8_lossy(&output.stdout).to_string()
                        + &String::from_utf8_lossy(&output.stderr);
                    if [
                        "Traceback",
                        "Unexpected",
                        "Previous tokens",
                        "Expected one of",
                    ]
                    .iter()
                    .any(|marker| text.contains(marker))
                    {
                        failed.lock().unwrap().push(normalize(file));
                        continue;
                    }
                    let mut found = found.lock().unwrap();
                    for line in text.lines() {
                        // path:line: Error: message (rule)
                        let Some((head, rest)) = line.trim_end().split_once(": Error: ") else {
                            continue;
                        };
                        let Some((path, number)) = head.rsplit_once(':') else {
                            continue;
                        };
                        let Some(rule) = rest
                            .rsplit_once(" (")
                            .and_then(|(_, r)| r.strip_suffix(')'))
                        else {
                            continue;
                        };
                        if RULES.contains(&rule) {
                            let key = (
                                normalize(Path::new(path)),
                                number.parse().unwrap(),
                                rule.to_string(),
                            );
                            *found.entry(key).or_default() += 1;
                        }
                    }
                }
            });
        }
    });
    (found.into_inner().unwrap(), failed.into_inner().unwrap())
}

fn normalize(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap();
    PathBuf::from(absolute.to_string_lossy().replace('\\', "/").to_lowercase())
}

/// What is in `a` and not in `b`, counting repeats.
fn minus(a: &Findings, b: &Findings) -> Vec<(PathBuf, usize, String)> {
    let mut out = Vec::new();
    for (key, count) in a {
        for _ in *b.get(key).unwrap_or(&0)..*count {
            out.push(key.clone());
        }
    }
    out
}

#[test]
fn same_findings_as_gdlint() {
    if std::env::var("GDTIDY_PARITY").as_deref() != Ok("1") {
        eprintln!("skipped: set GDTIDY_PARITY=1 and GDTIDY_PARITY_CORPUS to run");
        return;
    }
    let corpus = std::env::var("GDTIDY_PARITY_CORPUS").expect("set GDTIDY_PARITY_CORPUS");
    let separator = if cfg!(windows) { ';' } else { ':' };
    let mut files = Vec::new();
    for folder in corpus.split(separator).filter(|f| !f.is_empty()) {
        gd_files(Path::new(folder), &mut files);
    }
    files.sort();
    assert!(!files.is_empty(), "no .gd files in GDTIDY_PARITY_CORPUS");
    let empty = tempfile::TempDir::new().unwrap();

    let (tidy, skipped) = gdtidy(&files);
    let (lint, failed) = gdlint(&files, empty.path());
    let left_out =
        |key: &(PathBuf, usize, String)| skipped.contains(&key.0) || failed.contains(&key.0);
    let only_gdlint: Vec<_> = minus(&lint, &tidy)
        .into_iter()
        .filter(|k| !left_out(k))
        .collect();
    let only_gdtidy: Vec<_> = minus(&tidy, &lint)
        .into_iter()
        .filter(|k| !left_out(k))
        .collect();

    let count = |f: &Findings| f.values().sum::<usize>();
    eprintln!(
        "{} files; gdtidy skipped {}, gdlint could not parse {}; findings: gdtidy {}, gdlint {}",
        files.len(),
        skipped.len(),
        failed.len(),
        count(&tidy),
        count(&lint)
    );
    let mut by_rule: BTreeMap<&str, usize> = BTreeMap::new();
    for (_, _, rule) in &only_gdtidy {
        *by_rule.entry(rule).or_default() += 1;
    }
    eprintln!("only gdtidy ({}): {by_rule:?}", only_gdtidy.len());
    for (path, line, rule) in &only_gdtidy {
        eprintln!("  + {}:{line} {rule}", path.display());
    }
    let missing: Vec<String> = only_gdlint
        .iter()
        .map(|(path, line, rule)| format!("{}:{line} {rule}", path.display()))
        .collect();
    assert!(
        missing.is_empty(),
        "gdlint reports, gdtidy does not:\n{}",
        missing.join("\n")
    );
}
