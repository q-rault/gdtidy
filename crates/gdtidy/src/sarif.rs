//! SARIF 2.1.0 output, as GitHub code scanning reads it.

use serde_json::{Value, json};

use crate::lint::{Diagnostic, Skipped};
use crate::rules::RULES;

pub fn report(diagnostics: &[Diagnostic], skipped: &[Skipped]) -> Value {
    let rules: Vec<Value> = RULES
        .iter()
        .map(|rule| {
            json!({
                "id": rule.id,
                "shortDescription": { "text": rule.summary },
                "helpUri": format!("https://github.com/q-rault/gdtidy/blob/main/docs/rules.md#{}", rule.id),
                "defaultConfiguration": { "level": "warning", "enabled": rule.default_enabled },
                "properties": { "concern": rule.concern, "fixable": rule.fixable },
            })
        })
        .collect();
    let results: Vec<Value> = diagnostics
        .iter()
        .map(|d| {
            json!({
                "ruleId": d.rule,
                "ruleIndex": RULES.iter().position(|r| r.id == d.rule),
                "level": "warning",
                "message": { "text": d.message },
                "locations": [location(&d.path, d.line, d.column, Some((d.end_line, d.end_column)))],
            })
        })
        .collect();
    let notifications: Vec<Value> = skipped
        .iter()
        .map(|s| {
            json!({
                "level": "error",
                "descriptor": { "id": "skipped" },
                "message": { "text": format!("Not checked: {}", s.message) },
                "locations": [location(&s.path, s.line, s.column, None)],
            })
        })
        .collect();
    json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": { "driver": {
                "name": "gdtidy",
                "version": env!("CARGO_PKG_VERSION"),
                "informationUri": "https://github.com/q-rault/gdtidy",
                "rules": rules,
            }},
            "columnKind": "unicodeCodePoints",
            "results": results,
            "invocations": [{
                "executionSuccessful": skipped.is_empty(),
                "toolExecutionNotifications": notifications,
            }],
        }],
    })
}

fn location(path: &str, line: usize, column: usize, end: Option<(usize, usize)>) -> Value {
    let mut region = json!({ "startLine": line, "startColumn": column });
    if let Some((end_line, end_column)) = end {
        region["endLine"] = json!(end_line);
        region["endColumn"] = json!(end_column);
    }
    json!({ "physicalLocation": {
        "artifactLocation": { "uri": path },
        "region": region,
    }})
}
