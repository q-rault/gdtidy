//! Checks gdsyntax against Godot's own GDScript test scripts, copied under
//! `tests/godot/<version>/`. Each script's `.out` file says what Godot does
//! with it.
//!
//! For now this covers the tokenizer:
//! - every script, valid or not, round-trips byte for byte through the tree;
//! - a script Godot parses gets no tokenizer error;
//! - a script whose expected parser error comes from Godot's tokenizer gets
//!   that same message first.

use std::path::{Path, PathBuf};

/// The start of every error message Godot's tokenizer can produce.
const TOKENIZER_MESSAGES: &[&str] = &[
    "Unterminated string.",
    "Invalid escape in string.",
    "Invalid hexadecimal digit in unicode escape sequence.",
    "Invalid UTF-16 sequence in string",
    "Invisible text direction control character",
    "Multiple underscores cannot be adjacent",
    "Unexpected underscore after",
    "Cannot use a decimal point",
    "Expected exponent value after",
    "Expected hexadecimal digit after",
    "Expected binary digit after",
    "Invalid numeric notation.",
    "Expected annotation identifier after",
    "Closing \"",
    "Mixed use of tabs and spaces for indentation.",
    "Used tab character for indentation",
    "Used space character for indentation",
    "Unindent doesn't match the previous indentation level.",
    "Stray carriage return character",
    "Unexpected carriage return character.",
    "Expected new line after \"\\\".",
    "Invalid character \"",
    "Invalid white space character",
];

/// Godot's tokenizer messages gdsyntax does not produce yet, with the reason.
const NOT_PORTED: &[(&str, &str)] = &[(
    "Identifier \"",
    "the check for identifiers that look like keywords needs ICU's confusable tables",
)];

struct Case {
    path: PathBuf,
    outcome: String,
    message: Option<String>,
}

fn cases(version: &str) -> Vec<Case> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/godot")
        .join(version);
    let mut cases = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|ext| ext != "gd") {
                continue;
            }
            let out = path.with_extension("out");
            let Ok(expected) = std::fs::read_to_string(&out) else {
                // Helper scripts (`*.notest.gd`) and LSP fixtures have no
                // expected output.
                continue;
            };
            let mut lines = expected.lines();
            cases.push(Case {
                path,
                outcome: lines.next().unwrap_or("").trim().to_string(),
                message: lines.next().map(|line| line.trim().to_string()),
            });
        }
    }
    cases.sort_by(|a, b| a.path.cmp(&b.path));
    cases
}

fn is_tokenizer_message(message: &str) -> bool {
    TOKENIZER_MESSAGES
        .iter()
        .any(|start| message.starts_with(start))
}

fn check_tokenizer(version: &str) {
    let cases = cases(version);
    assert!(
        cases.len() > 400,
        "expected Godot's test scripts, found {}",
        cases.len()
    );

    let mut failures = Vec::new();
    let mut tokenizer_cases = 0;
    for case in &cases {
        let source = std::fs::read_to_string(&case.path).unwrap();
        let name = case.path.display();
        let (tree, errors) = gdsyntax::lossless_tree(&source);
        if tree.to_string() != source {
            failures.push(format!("{name}: the tree does not reproduce the source"));
        }
        let first = errors.first().map(|e| e.message.as_str());

        if case.outcome != "GDTEST_PARSER_ERROR" {
            if let Some(message) = first {
                failures.push(format!(
                    "{name}: Godot parses it, but gdsyntax reports \"{message}\""
                ));
            }
            continue;
        }
        let expected = case.message.as_deref().unwrap_or("");
        if NOT_PORTED
            .iter()
            .any(|(start, _)| expected.starts_with(start))
        {
            continue;
        }
        if is_tokenizer_message(expected) {
            tokenizer_cases += 1;
            if first != Some(expected) {
                failures.push(format!(
                    "{name}: expected \"{expected}\", gdsyntax reports {first:?}"
                ));
            }
        } else if let Some(message) = first {
            // The error is the parser's. The tokenizer may only report
            // something here if Godot's tokenizer would too.
            if !is_tokenizer_message(message) {
                failures.push(format!("{name}: unexpected tokenizer error \"{message}\""));
            }
        }
    }

    assert!(
        tokenizer_cases > 5,
        "found only {tokenizer_cases} tokenizer error cases"
    );
    assert!(
        failures.is_empty(),
        "{} of {} Godot {version} scripts disagree:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

#[test]
fn godot_4_7_tokenizer() {
    check_tokenizer("4.7");
}
