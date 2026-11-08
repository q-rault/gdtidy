//! Formatter fixtures: `tests/fmt/<case>.in.gd` must format to
//! `<case>.out.gd`, and `<case>.out.gd` must not change. A `gdtidy.toml` in
//! a case's folder applies to it.

use std::fs;
use std::path::Path;

use assert_cmd::Command;

fn format(path: &Path) -> String {
    let output = Command::cargo_bin("gdtidy")
        .unwrap()
        .current_dir(path.parent().unwrap())
        .args(["fmt", "-"])
        .write_stdin(fs::read(path).unwrap())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn cases(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            cases(&path, out);
        } else if path.to_string_lossy().ends_with(".in.gd") {
            out.push(path);
        }
    }
}

#[test]
fn every_case_formats_as_expected_and_is_stable() {
    let mut inputs = Vec::new();
    cases(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fmt"),
        &mut inputs,
    );
    assert!(!inputs.is_empty());
    let mut failures = Vec::new();
    for input in inputs {
        let expected_path = input.with_file_name(
            input
                .file_name()
                .unwrap()
                .to_string_lossy()
                .replace(".in.gd", ".out.gd"),
        );
        let expected = fs::read_to_string(&expected_path).unwrap();
        let got = format(&input);
        if got != expected {
            failures.push(format!(
                "{}:\n--- expected\n{expected}\n--- got\n{got}",
                input.display()
            ));
        }
        let again = format(&expected_path);
        if again != expected {
            failures.push(format!(
                "{} changes when formatted:\n{again}",
                expected_path.display()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
