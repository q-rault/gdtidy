use std::fs;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn gdtidy() -> Command {
    Command::cargo_bin("gdtidy").unwrap()
}

fn project(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().unwrap();
    // A real project root, so configuration lookup never climbs out of the
    // temporary folder into whatever the machine has above it.
    fs::write(dir.path().join("project.godot"), "").unwrap();
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
fn fmt_check_lists_files_and_changes_nothing() {
    let messy = "extends Node
func f(a,b):
    return a+b
";
    let dir = project(&[("main.gd", VALID), ("messy.gd", messy)]);
    gdtidy()
        .current_dir(dir.path())
        .args(["fmt", "--check"])
        .assert()
        .code(1)
        .stdout(
            "messy.gd
",
        )
        .stderr(predicate::str::contains("1 file(s) would be formatted"));
    assert_eq!(
        fs::read_to_string(dir.path().join("messy.gd")).unwrap(),
        messy
    );
}

#[test]
fn fmt_rewrites_files_in_place() {
    let dir = project(&[(
        "main.gd",
        "extends Node
func f(a,b):
	return a+b",
    )]);
    gdtidy()
        .current_dir(dir.path())
        .arg("fmt")
        .assert()
        .success()
        .stderr(predicate::str::contains("1 file(s) formatted (1 files)"));
    assert_eq!(
        fs::read_to_string(dir.path().join("main.gd")).unwrap(),
        "extends Node


func f(a, b):
	return a + b
"
    );
    gdtidy()
        .current_dir(dir.path())
        .args(["fmt", "--check"])
        .assert()
        .success();
}

#[test]
fn fmt_diff_prints_a_unified_diff() {
    let dir = project(&[(
        "main.gd", "var a=1
",
    )]);
    gdtidy()
        .current_dir(dir.path())
        .args(["fmt", "--diff"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("--- a/main.gd"))
        .stdout(predicate::str::contains(
            "-var a=1
+var a = 1",
        ));
}

#[test]
fn fmt_skips_files_with_a_syntax_error() {
    let dir = project(&[("main.gd", VALID), ("broken.gd", BROKEN)]);
    let output = gdtidy()
        .current_dir(dir.path())
        .args(["fmt", "--check", "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["files"], 2);
    assert_eq!(report["changed"], serde_json::json!([]));
    assert_eq!(report["skipped"][0]["path"], "broken.gd");
    assert_eq!(report["skipped"][0]["reason"], "syntax-error");
    assert_eq!(
        fs::read_to_string(dir.path().join("broken.gd")).unwrap(),
        BROKEN
    );
}

#[test]
fn fmt_exclude_is_its_own() {
    let dir = project(&[
        (
            "gdtidy.toml",
            "[fmt]
exclude = [\"gen\"]
",
        ),
        (
            "gen/a.gd", "var a=1
",
        ),
        (
            "b.gd", "var b=1
",
        ),
    ]);
    gdtidy()
        .current_dir(dir.path())
        .args(["fmt", "--check"])
        .assert()
        .code(1)
        .stdout(
            "b.gd
",
        );
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

/// SARIF results link to the rules page of the docs site, so every rule needs a
/// section in docs/content/docs/rules.mdx.
#[test]
fn every_rule_is_documented() {
    let docs = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/content/docs/rules.mdx"
    ))
    .unwrap();
    let output = gdtidy()
        .args(["rules", "--format", "json"])
        .output()
        .unwrap();
    let rules: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for rule in rules.as_array().unwrap() {
        let id = rule["id"].as_str().unwrap();
        assert!(
            docs.contains(&format!("\n## {id}\n")),
            "docs/content/docs/rules.mdx has no section for {id}"
        );
    }
}

#[test]
fn fix_repairs_what_is_safe_and_reports_the_rest() {
    let source = "extends Node\n#comment\n\n\nfunc f(x: int) -> void:\n\tpass\n\tprint(x)\n\tif x:\n\t\tpass  # keep this comment\n\t\tprint(x)\n";
    let dir = project(&[
        ("gdtidy.toml", "[lint]\nenable = [\"comment-space\"]\n"),
        ("a.gd", source),
    ]);
    gdtidy()
        .current_dir(dir.path())
        .args(["lint", "--fix"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains(
            "a.gd:8:3: warning[unnecessary-pass]",
        ))
        .stderr(predicate::str::contains("1 problem(s) (1 files, 2 fixed)"));
    let fixed = fs::read_to_string(dir.path().join("a.gd")).unwrap();
    assert_eq!(
        fixed,
        "extends Node\n# comment\n\n\nfunc f(x: int) -> void:\n\tprint(x)\n\tif x:\n\t\tpass  # keep this comment\n\t\tprint(x)\n"
    );
    // A second run has nothing left to fix.
    gdtidy()
        .current_dir(dir.path())
        .args(["lint", "--fix"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("1 problem(s) (1 files)"));
    assert_eq!(fs::read_to_string(dir.path().join("a.gd")).unwrap(), fixed);
}

#[test]
fn fix_keeps_crlf_line_ends_and_files_without_a_final_newline() {
    let dir = project(&[(
        "a.gd",
        "extends Node\r\n\r\n\r\nfunc f() -> void:\r\n\tpass\r\n\tprint(1)",
    )]);
    gdtidy()
        .current_dir(dir.path())
        .args(["lint", "--fix"])
        .assert()
        .code(0);
    assert_eq!(
        fs::read_to_string(dir.path().join("a.gd")).unwrap(),
        "extends Node\r\n\r\n\r\nfunc f() -> void:\r\n\tprint(1)"
    );
}

#[test]
fn lint_without_fix_changes_nothing() {
    let source = "extends Node\n\n\nfunc f() -> void:\n\tpass\n\tprint(1)\n";
    let dir = project(&[("a.gd", source)]);
    gdtidy()
        .current_dir(dir.path())
        .arg("lint")
        .assert()
        .code(1);
    assert_eq!(fs::read_to_string(dir.path().join("a.gd")).unwrap(), source);
}

// ---- migrate ----

#[test]
fn migrate_converts_gdlintrc_and_gdformatrc_together() {
    let dir = project(&[
        ("gdlintrc", "max-line-length: 130\n"),
        (".gdformatrc", "line_length: 130\n"),
    ]);
    gdtidy()
        .current_dir(dir.path())
        .arg("migrate")
        .assert()
        .success()
        .stderr(predicate::str::contains("line_length:"))
        .stderr(predicate::str::contains(
            "wrote gdtidy.toml from gdlintrc and .gdformatrc",
        ));
    let toml = fs::read_to_string(dir.path().join("gdtidy.toml")).unwrap();
    assert!(toml.starts_with("# Converted from gdlintrc and .gdformatrc"));

    // Named with a path, the header still gives only the file names.
    fs::remove_file(dir.path().join("gdtidy.toml")).unwrap();
    gdtidy()
        .current_dir(dir.path())
        .arg("migrate")
        .arg(dir.path().join("gdlintrc"))
        .assert()
        .success();
    let named = fs::read_to_string(dir.path().join("gdtidy.toml")).unwrap();
    assert!(named.starts_with("# Converted from gdlintrc by"), "{named}");
    assert!(toml.contains("[lint.rules.max-line-length]\nmax = 130"));
    assert!(toml.contains("[fmt]\n"));
    assert!(toml.contains("dict-brace-spaces = false"));
}

#[test]
fn migrate_adds_only_the_missing_section() {
    let lint_only = "[lint]\ndisable = [\"max-returns\"]\n";
    let dir = project(&[
        ("gdtidy.toml", lint_only),
        ("gdlintrc", "max-line-length: 130\n"),
        ("gdformatrc", ""),
    ]);
    gdtidy()
        .current_dir(dir.path())
        .arg("migrate")
        .assert()
        .success()
        .stderr(predicate::str::contains("already has [lint]"));
    let toml = fs::read_to_string(dir.path().join("gdtidy.toml")).unwrap();
    assert!(toml.starts_with(lint_only));
    assert!(toml.contains("[fmt]\n"));
    assert!(!toml.contains("max = 130"));
    // Nothing left to add.
    gdtidy()
        .current_dir(dir.path())
        .arg("migrate")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("already has every section"));
}

#[test]
fn gdsuite_toml_sections_apply_and_a_gdtidy_toml_beside_them_is_ignored() {
    let script = long_script();
    let dir = project(&[
        (
            "gdsuite.toml",
            "[project]\nroot = \".\"\n\n[test]\ntimeout = 5\n\n[lint]\ndisable = [\"max-line-length\"]\n",
        ),
        ("a.gd", &script),
    ]);
    gdtidy()
        .current_dir(dir.path())
        .arg("lint")
        .assert()
        .code(0);

    fs::write(dir.path().join("gdtidy.toml"), "[lint]\n").unwrap();
    gdtidy()
        .current_dir(dir.path())
        .arg("lint")
        .assert()
        .code(0)
        .stderr(predicate::str::contains("gdtidy.toml is ignored"));
}

#[test]
fn migrate_writes_into_gdsuite_toml_when_the_project_has_one() {
    let suite = "[project]\nroot = \".\"\n\n[test]\ntimeout = 5\n";
    let dir = project(&[
        ("gdsuite.toml", suite),
        ("gdlintrc", "max-line-length: 130\n"),
        ("gdformatrc", ""),
    ]);
    gdtidy()
        .current_dir(dir.path())
        .arg("migrate")
        .assert()
        .success()
        .stderr(predicate::str::contains(
            "wrote gdsuite.toml from gdlintrc and gdformatrc",
        ));
    assert!(!dir.path().join("gdtidy.toml").exists());
    let toml = fs::read_to_string(dir.path().join("gdsuite.toml")).unwrap();
    assert!(toml.starts_with(suite), "{toml}");
    assert!(toml.contains("# Converted from gdlintrc and gdformatrc by `gdtidy migrate`."));
    assert!(
        toml.contains("[lint.rules.max-line-length]\nmax = 130"),
        "{toml}"
    );
    assert!(toml.contains("[fmt]\n"), "{toml}");

    // Sections already in gdsuite.toml are kept, even with --force.
    gdtidy()
        .current_dir(dir.path())
        .arg("migrate")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("already has every section"));
    gdtidy()
        .current_dir(dir.path())
        .args(["migrate", "--force"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("remove it to convert"));
    assert_eq!(
        fs::read_to_string(dir.path().join("gdsuite.toml")).unwrap(),
        toml
    );
}
