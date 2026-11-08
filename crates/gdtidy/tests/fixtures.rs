//! Per-rule fixtures: every `.gd` file in `tests/fixtures` is linted with
//! the rules its folder's `gdtidy.toml` turns on, and a line must be reported exactly for the rules its
//! `# expect: rule-a, rule-b` comment lists (a rule twice for two findings).

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use assert_cmd::Command;

/// line → rules reported on it, sorted.
type Findings = BTreeMap<usize, Vec<String>>;

fn expected(source: &str) -> Findings {
    let mut found = Findings::new();
    for (index, line) in source.lines().enumerate() {
        if let Some((_, list)) = line.split_once("# expect:") {
            let mut rules: Vec<String> = list.split(',').map(|r| r.trim().to_string()).collect();
            rules.sort();
            found.insert(index + 1, rules);
        }
    }
    found
}

fn reported(path: &Path) -> Findings {
    let output = Command::cargo_bin("gdtidy")
        .unwrap()
        .args(["lint", "--format", "json"])
        .arg(path)
        .output()
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{}: {e}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    assert!(
        report["skipped"].as_array().unwrap().is_empty(),
        "{} was skipped: {}",
        path.display(),
        report["skipped"]
    );
    let mut found = Findings::new();
    for diagnostic in report["diagnostics"].as_array().unwrap() {
        let line = diagnostic["line"].as_u64().unwrap() as usize;
        let rule = diagnostic["rule"].as_str().unwrap().to_string();
        found.entry(line).or_default().push(rule);
    }
    for rules in found.values_mut() {
        rules.sort();
    }
    found
}

/// The `.gd` files under `dir`. A folder can hold its own `gdtidy.toml`
/// to try a rule with other options.
fn gd_files(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(gd_files(&path));
        } else if path.extension().is_some_and(|e| e == "gd") {
            files.push(path);
        }
    }
    files.sort();
    files
}

#[test]
fn fixtures_report_what_they_expect() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut failures = Vec::new();
    let mut count = 0;
    for path in gd_files(&dir) {
        count += 1;
        let source = fs::read_to_string(&path).unwrap();
        let (expected, reported) = (expected(&source), reported(&path));
        let name = path.strip_prefix(&dir).unwrap().to_string_lossy();
        for line in expected.keys().chain(reported.keys()) {
            let (want, got) = (expected.get(line), reported.get(line));
            if want != got {
                failures.push(format!(
                    "{name}:{line}: expected {want:?}, reported {got:?}"
                ));
            }
        }
    }
    assert!(count > 0, "no fixtures in {}", dir.display());
    failures.dedup();
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
