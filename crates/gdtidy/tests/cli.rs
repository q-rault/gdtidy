use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn gdtidy() -> Command {
    Command::cargo_bin("gdtidy").unwrap()
}

fn project(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    for (path, content) in files {
        let path = dir.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    dir
}

const VALID: &str = "extends Node\n\n\nfunc _ready() -> void:\n\tpass\n";
const BROKEN: &str = "extends Node\n\nfunc f(:\n\tpass\n";

#[test]
fn lint_passes_on_valid_code() {
    let dir = project(&[("player.gd", VALID)]);
    gdtidy()
        .current_dir(dir.path())
        .arg("lint")
        .assert()
        .code(0)
        .stderr(predicate::str::contains("0 problem(s) (1 files)"));
}

#[test]
fn lint_reports_a_file_that_does_not_parse() {
    let dir = project(&[("enemy/enemy.gd", BROKEN)]);
    gdtidy()
        .current_dir(dir.path())
        .arg("lint")
        .assert()
        .code(1)
        .stdout(predicate::str::starts_with("enemy/enemy.gd:3:"))
        .stdout(predicate::str::contains("error[parse-error]"));
}

#[test]
fn lint_json_lists_diagnostics() {
    let dir = project(&[("a.gd", BROKEN), ("b.gd", VALID)]);
    let output = gdtidy()
        .current_dir(dir.path())
        .args(["lint", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["version"], 1);
    assert_eq!(report["files"], 2);
    assert_eq!(report["diagnostics"][0]["rule"], "parse-error");
    assert_eq!(report["diagnostics"][0]["path"], "a.gd");
}

#[test]
fn walking_skips_addons_hidden_ignored_and_nested_projects() {
    let dir = project(&[
        ("main.gd", VALID),
        ("addons/plugin/broken.gd", BROKEN),
        (".godot/cache.gd", BROKEN),
        ("ignored/.gdignore", ""),
        ("ignored/broken.gd", BROKEN),
        ("nested/project.godot", ""),
        ("nested/broken.gd", BROKEN),
    ]);
    gdtidy()
        .current_dir(dir.path())
        .arg("lint")
        .assert()
        .code(0)
        .stderr(predicate::str::contains("(1 files)"));
}

#[test]
fn a_file_named_directly_is_always_linted() {
    let dir = project(&[("addons/plugin/broken.gd", BROKEN)]);
    gdtidy()
        .current_dir(dir.path())
        .args(["lint", "addons/plugin/broken.gd"])
        .assert()
        .code(1);
}

#[test]
fn a_missing_target_cannot_run() {
    let dir = project(&[]);
    gdtidy()
        .current_dir(dir.path())
        .args(["lint", "nowhere"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no such file or folder"));
}

#[test]
fn rules_publish_a_concern_for_each_rule() {
    let output = gdtidy()
        .args(["rules", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let rules: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rules = rules.as_array().unwrap();
    assert!(!rules.is_empty());
    for rule in rules {
        assert!(rule["id"].is_string());
        assert!(rule["concern"].is_string());
    }
}

#[test]
fn fmt_says_it_is_not_implemented() {
    let dir = project(&[("main.gd", VALID)]);
    gdtidy()
        .current_dir(dir.path())
        .args(["fmt", "--check"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("not implemented"));
}
