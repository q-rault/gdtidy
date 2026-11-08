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
fn a_file_that_does_not_parse_is_skipped_not_reported() {
    let dir = project(&[("enemy/enemy.gd", BROKEN), ("ok.gd", VALID)]);
    gdtidy()
        .current_dir(dir.path())
        .arg("lint")
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("enemy/enemy.gd:3:"))
        .stderr(predicate::str::contains(": skipped: "))
        .stderr(predicate::str::contains(
            "0 problem(s) (2 files, 1 skipped)",
        ));
}

#[test]
fn lint_json_lists_skipped_files_apart() {
    let dir = project(&[("a.gd", BROKEN), ("b.gd", VALID)]);
    let output = gdtidy()
        .current_dir(dir.path())
        .args(["lint", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["version"], 1);
    assert_eq!(report["files"], 2);
    assert_eq!(report["diagnostics"].as_array().unwrap().len(), 0);
    assert_eq!(report["skipped"][0]["path"], "a.gd");
    assert_eq!(report["skipped"][0]["reason"], "syntax-error");
    assert_eq!(report["skipped"][0]["line"], 3);
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
        .code(2)
        .stderr(predicate::str::contains("1 skipped"));
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
fn naming_a_file_that_is_not_gdscript_cannot_run() {
    let dir = project(&[("README.md", "# A game\n")]);
    gdtidy()
        .current_dir(dir.path())
        .args(["lint", "README.md"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("not a GDScript (.gd) file"));
}

#[test]
fn a_file_that_is_not_utf8_is_skipped_and_the_run_continues() {
    let dir = project(&[("good.gd", VALID)]);
    fs::write(
        dir.path().join("latin1.gd"),
        b"extends Node\n# caf\xe9\n".as_slice(),
    )
    .unwrap();
    gdtidy()
        .current_dir(dir.path())
        .arg("lint")
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "latin1.gd:2:6: skipped: Script contains invalid unicode (UTF-8)",
        ))
        .stderr(predicate::str::contains(
            "0 problem(s) (2 files, 1 skipped)",
        ));
}

#[test]
fn byte_order_mark_and_crlf_files_parse() {
    let dir = project(&[
        (
            "bom.gd",
            "\u{feff}extends Node\n\n\nfunc f() -> void:\n\tpass\n",
        ),
        (
            "crlf.gd",
            "extends Node\r\n\r\n\r\nfunc f() -> void:\r\n\tpass\r\n",
        ),
        ("empty.gd", ""),
    ]);
    gdtidy()
        .current_dir(dir.path())
        .arg("lint")
        .assert()
        .code(0)
        .stderr(predicate::str::contains("0 problem(s) (3 files)"));
}

#[test]
fn rules_publish_a_concern_for_each_rule() {
    let output = gdtidy()
        .args(["rules", "--format", "json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let rules: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for rule in rules.as_array().unwrap() {
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

// ---- M1: configuration, silencing comments, SARIF ----

/// 97 characters after a tab: 101 columns with a tab of 4, 98 with 1.
const LONG_LINE: &str = "\tprint(\"0123456789012345678901234567890123456789012345678901234567890123456789012345678901234567\")";

fn long_script() -> String {
    format!("extends Node\n\n\nfunc f() -> void:\n{LONG_LINE}\n")
}

#[test]
fn max_line_length_counts_tabs_as_four_columns() {
    let dir = project(&[("a.gd", &long_script())]);
    gdtidy()
        .current_dir(dir.path())
        .arg("lint")
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "a.gd:5:1: warning[max-line-length]: Line is 101 columns long; the limit is 100.",
        ));
}

#[test]
fn gdtidy_toml_sets_options_and_turns_rules_off() {
    let script = long_script();
    let dir = project(&[
        (
            "gdtidy.toml",
            "[lint.rules.max-line-length]\ntab-width = 1\n",
        ),
        ("a.gd", &script),
    ]);
    gdtidy()
        .current_dir(dir.path())
        .arg("lint")
        .assert()
        .code(0);

    let dir = project(&[
        ("gdtidy.toml", "[lint]\ndisable = [\"max-line-length\"]\n"),
        ("a.gd", &script),
    ]);
    gdtidy()
        .current_dir(dir.path())
        .arg("lint")
        .assert()
        .code(0);
}

#[test]
fn the_nearest_gdtidy_toml_applies_and_stops_at_the_project() {
    let dir = project(&[
        // Outside the project: never read for files inside it.
        ("gdtidy.toml", "[lint]\ndisable = [\"max-line-length\"]\n"),
        ("game/project.godot", ""),
        ("game/a.gd", &long_script()),
    ]);
    gdtidy()
        .current_dir(dir.path().join("game"))
        .arg("lint")
        .assert()
        .code(1);
}

#[test]
fn a_mistake_in_gdtidy_toml_cannot_run() {
    let dir = project(&[
        ("gdtidy.toml", "[lint]\ndisable = [\"unused-argument\"]\n"),
        ("a.gd", VALID),
    ]);
    gdtidy()
        .current_dir(dir.path())
        .arg("lint")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("UNUSED_PARAMETER"));
}

#[test]
fn exclude_skips_folders_when_walking_but_not_named_files() {
    let dir = project(&[
        ("gdtidy.toml", "[lint]\nexclude = [\"generated\"]\n"),
        ("generated/a.gd", &long_script()),
    ]);
    gdtidy()
        .current_dir(dir.path())
        .arg("lint")
        .assert()
        .code(0)
        .stderr(predicate::str::contains("(0 files)"));
    gdtidy()
        .current_dir(dir.path())
        .args(["lint", "generated/a.gd"])
        .assert()
        .code(1);
}

#[test]
fn comments_silence_rules_in_every_spelling() {
    for comment in [
        "\t# gdtidy: ignore=max-line-length",
        "\t# gdlint: ignore=max-line-length",
        "\t# gdlint-ignore-next-line max-line-length",
        "\t# gdtidy: disable=max-line-length",
    ] {
        let source = format!("extends Node\n\n\nfunc f() -> void:\n{comment}\n{LONG_LINE}\n");
        let dir = project(&[("a.gd", &source)]);
        gdtidy()
            .current_dir(dir.path())
            .arg("lint")
            .assert()
            .code(0);
    }
}

#[test]
fn an_unknown_rule_in_a_comment_is_reported() {
    let dir = project(&[("a.gd", "extends Node  # gdtidy: ignore=no-such-rule\n")]);
    gdtidy()
        .current_dir(dir.path())
        .arg("lint")
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "warning[unknown-rule-in-comment]: `no-such-rule` is not a rule",
        ));
}

#[test]
fn sarif_output_has_rules_results_and_skipped_files() {
    let dir = project(&[("a.gd", &long_script()), ("b.gd", BROKEN)]);
    let output = gdtidy()
        .current_dir(dir.path())
        .args(["lint", "--format", "sarif"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let sarif: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(sarif["version"], "2.1.0");
    let run = &sarif["runs"][0];
    assert_eq!(run["tool"]["driver"]["name"], "gdtidy");
    assert!(run["tool"]["driver"]["rules"].as_array().unwrap().len() >= 2);
    let result = &run["results"][0];
    assert_eq!(result["ruleId"], "max-line-length");
    let location = &result["locations"][0]["physicalLocation"];
    assert_eq!(location["artifactLocation"]["uri"], "a.gd");
    assert_eq!(location["region"]["startLine"], 5);
    let invocation = &run["invocations"][0];
    assert_eq!(invocation["executionSuccessful"], false);
    assert_eq!(
        invocation["toolExecutionNotifications"][0]["locations"][0]["physicalLocation"]["artifactLocation"]
            ["uri"],
        "b.gd"
    );
}

#[test]
fn json_diagnostics_carry_their_concern() {
    let dir = project(&[("a.gd", &long_script())]);
    let output = gdtidy()
        .current_dir(dir.path())
        .args(["lint", "--format", "json"])
        .output()
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["diagnostics"][0]["concern"], "limit:line-length");
    assert_eq!(report["diagnostics"][0]["fixable"], false);
}

#[test]
fn rules_publish_their_options() {
    let output = gdtidy()
        .args(["rules", "--format", "json"])
        .output()
        .unwrap();
    let rules: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let rule = rules
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "max-line-length")
        .unwrap();
    assert_eq!(rule["options"][0]["name"], "max");
    assert_eq!(rule["options"][0]["type"], "integer");
    assert_eq!(rule["options"][0]["default"], 100);
}

#[test]
fn output_order_does_not_depend_on_threads() {
    let script = long_script();
    let names: Vec<String> = (0..40).map(|i| format!("f{i:02}.gd")).collect();
    let files: Vec<(&str, &str)> = names
        .iter()
        .map(|n| (n.as_str(), script.as_str()))
        .collect();
    let dir = project(&files);
    let run = || {
        gdtidy()
            .current_dir(dir.path())
            .arg("lint")
            .output()
            .unwrap()
            .stdout
    };
    let first = run();
    let lines: Vec<&str> = std::str::from_utf8(&first).unwrap().lines().collect();
    assert_eq!(lines.len(), 40);
    let mut sorted = lines.clone();
    sorted.sort();
    assert_eq!(lines, sorted);
    assert_eq!(run(), first);
}

/// SARIF results link to docs/rules.md, so every rule needs a section there.
#[test]
fn every_rule_is_documented() {
    let docs =
        fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/rules.md")).unwrap();
    let output = gdtidy()
        .args(["rules", "--format", "json"])
        .output()
        .unwrap();
    let rules: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for rule in rules.as_array().unwrap() {
        let id = rule["id"].as_str().unwrap();
        assert!(
            docs.contains(&format!("\n## {id}\n")),
            "docs/rules.md has no section for {id}"
        );
    }
}
