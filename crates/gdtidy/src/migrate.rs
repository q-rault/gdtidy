//! `gdtidy migrate`: convert a gdlintrc into the `[lint]` section of a
//! `gdtidy.toml` that reports the same problems, and a gdformatrc into its
//! `[fmt]` section.
//!
//! Kept rules carry their values over; values equal to gdtidy's defaults
//! are left out, so the file only says what the project changed. The one
//! exception is the tab width: gdlint counts a tab as 1 column and gdtidy as
//! 4, so the converted file always says which one the project used. Rules
//! that moved to another tool are dropped with a note saying where.

use anyhow::{Context as _, Result, anyhow, bail};
use yaml_rust2::{Yaml, YamlLoader};

use crate::config::{Config, Pattern};
use crate::rules::{self, OptionKind};

/// gdlint's rules that take a pattern, and gdtidy's option for it.
const PATTERN_RULES: &[&str] = &[
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
];

/// gdlint's rules that take a number, which becomes gdtidy's `max`.
const LIMIT_RULES: &[&str] = &[
    "max-returns",
    "function-arguments-number",
    "max-public-methods",
    "max-file-lines",
    "max-line-length",
];

/// gdlint's rules that take no value: on unless disabled.
const FLAG_RULES: &[&str] = &[
    "duplicated-load",
    "comparison-with-itself",
    "no-elif-return",
    "no-else-return",
    "unnecessary-pass",
];

/// What a conversion gives.
pub struct Migration {
    /// One section of `gdtidy.toml`, starting with its header.
    pub toml: String,
    /// What the user should know: dropped rules and where they went,
    /// entries gdtidy does not need, and anything not understood.
    pub notes: Vec<String>,
}

/// One `[lint.rules.<id>]` table: option names and TOML values.
type RuleTable = (String, Vec<(String, String)>);

pub fn migrate(gdlintrc: &str) -> Result<Migration> {
    let docs = YamlLoader::load_from_str(gdlintrc).map_err(|e| anyhow!("not valid YAML: {e}"))?;
    let root = match docs.into_iter().next() {
        Some(Yaml::Hash(hash)) => hash,
        None | Some(Yaml::Null) => Default::default(),
        Some(_) => bail!("expected `key: value` entries at the top level"),
    };
    let mut notes = Vec::new();
    let mut disable: Vec<String> = Vec::new();
    let mut exclude: Vec<String> = Vec::new();
    let mut tables: Vec<RuleTable> = Vec::new();
    let mut tab_width: i64 = 1;

    for (key, value) in &root {
        let Some(key) = key.as_str() else {
            notes.push(format!("ignored a key that is not text: {key:?}"));
            continue;
        };
        match key {
            "disable" => {
                for id in strings(value, key)? {
                    if rules::find(&id).is_some() {
                        disable.push(id);
                    } else if let Some(to) = rules::moved(&id) {
                        notes.push(format!("{id} (disabled): no longer gdtidy's, {to}"));
                    } else {
                        notes.push(format!("{id} (disabled): not a gdlint rule, ignored"));
                    }
                }
            }
            "excluded_directories" => {
                for dir in strings(value, key)? {
                    let dir = dir.trim_end_matches('/').to_string();
                    if dir.starts_with('.') || dir == "addons" {
                        notes.push(format!(
                            "{dir} (excluded): gdtidy always skips hidden folders and addons"
                        ));
                    } else {
                        exclude.push(dir);
                    }
                }
            }
            "tab-characters" => {
                tab_width = value
                    .as_i64()
                    .ok_or_else(|| anyhow!("`tab-characters` must be a number"))?;
            }
            "class-definitions-order" => {
                let sections = strings(value, key)?;
                if !is_default(key, "sections", &OptionValueRef::List(&sections)) {
                    tables.push((
                        key.to_string(),
                        vec![("sections".to_string(), toml_list(&sections))],
                    ));
                }
            }
            id if PATTERN_RULES.contains(&id) => {
                let pattern = value
                    .as_str()
                    .ok_or_else(|| anyhow!("`{id}` must be a regular expression"))?;
                Pattern::new(pattern).with_context(|| {
                    format!("`{id}`: gdtidy cannot use this regular expression")
                })?;
                if !is_default(id, "pattern", &OptionValueRef::Text(pattern)) {
                    tables.push((
                        id.to_string(),
                        vec![("pattern".to_string(), toml_string(pattern))],
                    ));
                }
            }
            id if LIMIT_RULES.contains(&id) => {
                let max = value
                    .as_i64()
                    .ok_or_else(|| anyhow!("`{id}` must be a number"))?;
                if !is_default(id, "max", &OptionValueRef::Integer(max)) {
                    tables.push((id.to_string(), vec![("max".to_string(), max.to_string())]));
                }
            }
            id if FLAG_RULES.contains(&id) => {}
            id => match rules::moved(id) {
                Some(to) => notes.push(format!("{id}: no longer gdtidy's, {to}")),
                None => notes.push(format!("{id}: not a gdlint setting, ignored")),
            },
        }
    }

    // Report lines the same length as gdlint did, unless the rule is off.
    if !disable.iter().any(|id| id == "max-line-length") {
        let at = match tables.iter().position(|(id, _)| id == "max-line-length") {
            Some(at) => at,
            None => {
                tables.push(("max-line-length".to_string(), Vec::new()));
                tables.len() - 1
            }
        };
        tables[at]
            .1
            .push(("tab-width".to_string(), tab_width.to_string()));
    }

    // Write rules in gdtidy's order, so the file reads like `gdtidy rules`.
    let position = |id: &str| rules::RULES.iter().position(|r| r.id == id);
    tables.sort_by_key(|(id, _)| position(id));
    disable.sort_by_key(|id| position(id));
    disable.dedup();

    let mut toml = String::from("[lint]\n");
    if !disable.is_empty() {
        toml += &format!("disable = {}\n", toml_list(&disable));
    }
    if !exclude.is_empty() {
        toml += &format!("exclude = {}\n", toml_list(&exclude));
    }
    for (id, options) in &tables {
        toml += &format!("\n[lint.rules.{id}]\n");
        for (name, value) in options {
            toml += &format!("{name} = {value}\n");
        }
    }
    // The result must be a configuration gdtidy accepts.
    Config::parse(&toml, None).context("the converted configuration is not valid")?;
    Ok(Migration { toml, notes })
}

/// Convert a gdformatrc into a `[fmt]` section. gdformat writes one-line
/// dictionaries as `{"a": 1}`, so the section turns brace spaces off to keep
/// the project's look.
pub fn migrate_gdformatrc(gdformatrc: &str) -> Result<Migration> {
    let docs = YamlLoader::load_from_str(gdformatrc).map_err(|e| anyhow!("not valid YAML: {e}"))?;
    let root = match docs.into_iter().next() {
        Some(Yaml::Hash(hash)) => hash,
        None | Some(Yaml::Null) => Default::default(),
        Some(_) => bail!("expected `key: value` entries at the top level"),
    };
    let mut notes = Vec::new();
    let mut exclude = Vec::new();
    for (key, value) in &root {
        match key.as_str() {
            Some("excluded_directories") => {
                for dir in strings(value, "excluded_directories")? {
                    let dir = dir.trim_end_matches('/').to_string();
                    if dir.starts_with('.') || dir == "addons" {
                        notes.push(format!(
                            "{dir} (excluded): gdtidy always skips hidden folders and addons"
                        ));
                    } else {
                        exclude.push(dir);
                    }
                }
            }
            Some("line_length") => notes.push(
                "line_length: gdtidy fmt keeps your line breaks and does not wrap lines; \
                 max-line-length in [lint] reports long ones"
                    .to_string(),
            ),
            Some("use_spaces") => {
                if !matches!(value, Yaml::Boolean(false) | Yaml::Null) {
                    notes.push(
                        "use_spaces: gdtidy fmt always indents with tabs, as the style guide asks"
                            .to_string(),
                    );
                }
            }
            Some("safety_checks") => {}
            Some(other) => notes.push(format!("{other}: not a gdformat setting, ignored")),
            None => notes.push(format!("ignored a key that is not text: {key:?}")),
        }
    }
    let mut toml = String::from("[fmt]\n");
    if !exclude.is_empty() {
        toml += &format!("exclude = {}\n", toml_list(&exclude));
    }
    toml += "# gdformat writes one-line dictionaries without spaces: {\"a\": 1}.\n";
    toml += "dict-brace-spaces = false\n";
    Config::parse(&toml, None).context("the converted configuration is not valid")?;
    Ok(Migration { toml, notes })
}

enum OptionValueRef<'a> {
    Text(&'a str),
    Integer(i64),
    List(&'a [String]),
}

/// Whether `value` is gdtidy's default for that option.
fn is_default(rule: &str, option: &str, value: &OptionValueRef) -> bool {
    let Some(kind) = rules::find(rule)
        .and_then(|r| r.option(option))
        .map(|o| o.kind)
    else {
        return false;
    };
    match (kind, value) {
        (OptionKind::Pattern(d) | OptionKind::String(d), OptionValueRef::Text(v)) => d == *v,
        (OptionKind::Integer(d), OptionValueRef::Integer(v)) => d == *v,
        (OptionKind::StringList(d), OptionValueRef::List(v)) => {
            d.len() == v.len() && d.iter().zip(v.iter()).all(|(a, b)| a == b)
        }
        _ => false,
    }
}

/// A list of strings: a YAML list, or a set (`!!set`, read as a mapping to
/// nulls), as gdlint writes `excluded_directories`.
fn strings(value: &Yaml, key: &str) -> Result<Vec<String>> {
    let items: Vec<&Yaml> = match value {
        Yaml::Array(items) => items.iter().collect(),
        Yaml::Hash(hash) => hash.keys().collect(),
        Yaml::Null => Vec::new(),
        _ => bail!("`{key}` must be a list"),
    };
    items
        .into_iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| anyhow!("`{key}` must be a list of names"))
        })
        .collect()
}

/// A TOML string; a literal string (no escapes) when it can be one, which
/// keeps regular expressions readable.
fn toml_string(text: &str) -> String {
    if !text.contains(['\'', '\n', '\r']) {
        format!("'{text}'")
    } else {
        toml::Value::String(text.to_string()).to_string()
    }
}

fn toml_list(items: &[String]) -> String {
    let items: Vec<String> = items.iter().map(|s| toml_string(s)).collect();
    format!("[{}]", items.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_gdlintrc_only_keeps_the_tab_width() {
        let dumped = "class-definitions-order:\n- tools\n- classnames\n- extends\n- docstrings\n- signals\n- enums\n- consts\n- staticvars\n- exports\n- pubvars\n- prvvars\n- onreadypubvars\n- onreadyprvvars\n- others\nclass-name: ([A-Z][a-z0-9]*)+\ncomparison-with-itself: null\ndisable: []\nenum-element-name: '[A-Z][A-Z0-9]*(_[A-Z0-9]+)*'\nexcluded_directories: !!set\n  .git: null\nexpression-not-assigned: null\nmax-line-length: 100\ntab-characters: 1\ntrailing-whitespace: null\nunused-argument: null\n";
        let migration = migrate(dumped).unwrap();
        assert_eq!(
            migration.toml,
            "[lint]\n\n[lint.rules.max-line-length]\ntab-width = 1\n"
        );
        assert!(
            migration
                .notes
                .iter()
                .any(|n| n.starts_with("unused-argument:") && n.contains("UNUSED_PARAMETER"))
        );
    }

    #[test]
    fn values_disables_and_exclusions_carry_over() {
        let gdlintrc = "excluded_directories: !!set\n  .git: null\n  addons: null\n  reports: null\nmax-line-length: 130\ntab-characters: 4\nmax-returns: 3\nfunction-name: '[a-z]+'\ndisable:\n  - no-else-return\n  - unused-argument\nclass-definitions-order:\n- tools\n- others\n";
        let migration = migrate(gdlintrc).unwrap();
        assert_eq!(
            migration.toml,
            "[lint]\ndisable = ['no-else-return']\nexclude = ['reports']\n\n[lint.rules.function-name]\npattern = '[a-z]+'\n\n[lint.rules.class-definitions-order]\nsections = ['tools', 'others']\n\n[lint.rules.max-returns]\nmax = 3\n\n[lint.rules.max-line-length]\nmax = 130\ntab-width = 4\n"
        );
        assert_eq!(migration.notes.len(), 3, "{:?}", migration.notes);
    }

    #[test]
    fn a_disabled_line_length_needs_no_tab_width() {
        let migration = migrate("disable: [max-line-length]\n").unwrap();
        assert!(!migration.toml.contains("tab-width"));
        assert!(migration.toml.contains("disable = ['max-line-length']"));
    }

    #[test]
    fn a_gdformatrc_becomes_the_fmt_section() {
        let gdformatrc = "excluded_directories: !!set\n  .git: null\n  addons: null\n  generated: null\nline_length: 100\nsafety_checks: true\nuse_spaces: false\n";
        let migration = migrate_gdformatrc(gdformatrc).unwrap();
        assert_eq!(
            migration.toml,
            "[fmt]\nexclude = ['generated']\n# gdformat writes one-line dictionaries without spaces: {\"a\": 1}.\ndict-brace-spaces = false\n"
        );
        assert_eq!(migration.notes.len(), 3, "{:?}", migration.notes);
        assert!(migration.notes[2].starts_with("line_length:"));
        assert!(migrate_gdformatrc("use_spaces: 4\n").unwrap().notes[0].contains("tabs"));
    }

    #[test]
    fn mistakes_are_errors() {
        assert!(migrate("function-name: '('\n").is_err());
        assert!(migrate("max-returns: many\n").is_err());
        assert!(migrate("- a\n- b\n").is_err());
    }
}
