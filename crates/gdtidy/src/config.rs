//! `gdtidy.toml`: which rules run, with which options, on which files.
//!
//! ```toml
//! [lint]
//! disable = ["max-public-methods"]   # rules on by default not to run
//! enable = ["file-name"]             # rules off by default to run
//! exclude = ["reports"]              # folders, relative to this file
//!
//! [lint.rules.max-line-length]
//! max = 130
//! tab-width = 1
//!
//! [fmt]
//! continuation-indent = 1          # see fmt::Options
//! ```
//!
//! The same sections can live in `gdsuite.toml`, next to the other tools'
//! settings, for a project that keeps them all in one file.
//!
//! The settings that apply to a script are the first found walking up from
//! the script's folder: in each folder, a `gdsuite.toml` with a `[lint]` or
//! `[fmt]` section comes first, then `gdtidy.toml`. A `gdtidy.toml` next to
//! such a `gdsuite.toml` is ignored, with a warning: one tool's settings
//! belong in one file, and the two are not merged. The walk stops at a
//! folder with a `project.godot`, so a project never picks up a file from
//! outside it.
//! Anything gdtidy does not know (a rule, an option, a key, a wrong type) is
//! an error, so a typo never silently turns a rule off.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context as _, Result, anyhow, bail};
use regex::Regex;
use toml::{Table, Value};

use crate::rules::{self, OptionKind, Rule};

pub const FILE_NAME: &str = "gdtidy.toml";

/// gdsuite's configuration file, which can hold gdtidy's sections.
pub const SUITE_FILE_NAME: &str = "gdsuite.toml";

/// One option's value.
#[derive(Debug, Clone, PartialEq)]
pub enum OptionValue {
    Integer(i64),
    String(String),
    StringList(Vec<String>),
    Pattern(Pattern),
}

/// A regular expression that must match the whole text, compiled once when
/// the configuration is read.
#[derive(Debug, Clone)]
pub struct Pattern {
    pub source: String,
    regex: Arc<Regex>,
}

impl PartialEq for Pattern {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl Pattern {
    pub fn new(source: &str) -> Result<Self> {
        // gdlint uses Python's `re.fullmatch`; anchoring a group does the same.
        let regex = Regex::new(&format!("^(?:{source})$")).map_err(|e| anyhow!("{e}"))?;
        Ok(Pattern {
            source: source.to_string(),
            regex: Arc::new(regex),
        })
    }

    pub fn is_match(&self, text: &str) -> bool {
        self.regex.is_match(text)
    }
}

/// The options of one rule: its defaults, overridden by the configuration.
#[derive(Debug, Clone, Default)]
pub struct RuleOptions {
    values: HashMap<&'static str, OptionValue>,
}

impl RuleOptions {
    fn defaults(rule: &Rule) -> Self {
        let values = rule
            .options
            .iter()
            .map(|option| {
                let value = match option.kind {
                    OptionKind::Integer(n) => OptionValue::Integer(n),
                    OptionKind::String(s) => OptionValue::String(s.to_string()),
                    OptionKind::Pattern(p) => OptionValue::Pattern(
                        Pattern::new(p).expect("default patterns are valid regular expressions"),
                    ),
                    OptionKind::StringList(list) => {
                        OptionValue::StringList(list.iter().map(|s| s.to_string()).collect())
                    }
                };
                (option.name, value)
            })
            .collect();
        RuleOptions { values }
    }

    /// An integer option. Panics if the rule does not declare it as one,
    /// which is a bug in the rule.
    pub fn integer(&self, name: &str) -> i64 {
        match self.values.get(name) {
            Some(OptionValue::Integer(n)) => *n,
            other => panic!("option {name} is not an integer: {other:?}"),
        }
    }

    pub fn pattern(&self, name: &str) -> &Pattern {
        match self.values.get(name) {
            Some(OptionValue::Pattern(p)) => p,
            other => panic!("option {name} is not a pattern: {other:?}"),
        }
    }

    #[allow(dead_code)] // No rule has a plain string option yet.
    pub fn string(&self, name: &str) -> &str {
        match self.values.get(name) {
            Some(OptionValue::String(s)) => s,
            other => panic!("option {name} is not a string: {other:?}"),
        }
    }

    #[allow(dead_code)] // Used by class-definitions-order (M3).
    pub fn strings(&self, name: &str) -> &[String] {
        match self.values.get(name) {
            Some(OptionValue::StringList(list)) => list,
            other => panic!("option {name} is not a list of strings: {other:?}"),
        }
    }
}

/// The configuration that applies to a set of scripts.
#[derive(Debug)]
pub struct Config {
    /// The folder of the `gdtidy.toml`, or of the project, that `exclude`
    /// paths are relative to. `None` when neither was found.
    pub root: Option<PathBuf>,
    /// The rules that run, in registry order, with their options.
    pub rules: Vec<(&'static Rule, RuleOptions)>,
    pub exclude: Vec<String>,
    /// `[fmt]`: the formatter's settings and its own `exclude`.
    pub fmt: crate::fmt::Options,
    pub fmt_exclude: Vec<String>,
}

impl Config {
    fn defaults(root: Option<PathBuf>) -> Self {
        Config {
            root,
            rules: rules::RULES
                .iter()
                .filter(|r| r.default_enabled)
                .map(|r| (r, RuleOptions::defaults(r)))
                .collect(),
            exclude: Vec::new(),
            fmt: crate::fmt::Options::default(),
            fmt_exclude: Vec::new(),
        }
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.rules.iter().any(|(rule, _)| rule.id == id)
    }

    /// Whether `path` is under one of the `exclude` folders. As in
    /// `.gitignore`, a name alone (`reports`) matches a folder at any depth,
    /// and a path (`tools/gen`, `/reports`) is relative to the root.
    pub fn excludes(&self, path: &Path) -> bool {
        self.excludes_in(&self.exclude, path)
    }

    /// Whether `path` is under one of `[fmt] exclude`'s folders.
    pub fn fmt_excludes(&self, path: &Path) -> bool {
        self.excludes_in(&self.fmt_exclude, path)
    }

    fn excludes_in(&self, exclude: &[String], path: &Path) -> bool {
        let Some(root) = &self.root else {
            return false;
        };
        let Ok(relative) = path.strip_prefix(root) else {
            return false;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        let parts: Vec<&str> = relative.split('/').collect();
        let folders = &parts[..parts.len().saturating_sub(1)];
        exclude.iter().any(|entry| {
            let entry = entry.trim_end_matches('/');
            match entry.strip_prefix('/') {
                None if !entry.contains('/') => folders.contains(&entry),
                rooted => {
                    let entry = rooted.unwrap_or(entry);
                    relative == entry || relative.starts_with(&format!("{entry}/"))
                }
            }
        })
    }

    /// Read a `gdtidy.toml`'s text.
    pub fn parse(text: &str, root: Option<PathBuf>) -> Result<Self> {
        let table: Table = text.parse().map_err(|e| anyhow!("{e}"))?;
        let mut config = Config::defaults(root);
        for (key, value) in &table {
            match key.as_str() {
                "lint" => config.read_lint(as_table(value, "lint")?)?,
                "fmt" => config.read_fmt(as_table(value, "fmt")?)?,
                other => bail!("unknown section `{other}`; expected `lint` or `fmt`"),
            }
        }
        Ok(config)
    }

    /// Read the `[lint]` and `[fmt]` sections of a `gdsuite.toml`, which
    /// also holds other tools' sections; `None` when it has neither.
    pub fn parse_suite(text: &str, root: Option<PathBuf>) -> Result<Option<Self>> {
        let table: Table = text.parse().map_err(|e| anyhow!("{e}"))?;
        if !table.contains_key("lint") && !table.contains_key("fmt") {
            return Ok(None);
        }
        let mut config = Config::defaults(root);
        if let Some(lint) = table.get("lint") {
            config.read_lint(as_table(lint, "lint")?)?;
        }
        if let Some(fmt) = table.get("fmt") {
            config.read_fmt(as_table(fmt, "fmt")?)?;
        }
        Ok(Some(config))
    }

    fn read_fmt(&mut self, fmt: &Table) -> Result<()> {
        for (key, value) in fmt {
            let boolean = || {
                value
                    .as_bool()
                    .ok_or_else(|| anyhow!("`fmt.{key}` must be true or false"))
            };
            match key.as_str() {
                "exclude" => self.fmt_exclude = string_list(value, "fmt.exclude")?,
                "continuation-indent" => {
                    self.fmt.continuation_indent = match value.as_integer() {
                        Some(n @ 1..=4) => n as usize,
                        _ => bail!("`fmt.continuation-indent` must be 1, 2, 3 or 4"),
                    }
                }
                "dict-brace-spaces" => self.fmt.dict_brace_spaces = boolean()?,
                "trailing-commas" => self.fmt.trailing_commas = boolean()?,
                other => bail!(
                    "unknown key `fmt.{other}`; expected `exclude`, `continuation-indent`, \
                     `dict-brace-spaces` or `trailing-commas`"
                ),
            }
        }
        Ok(())
    }

    fn read_lint(&mut self, lint: &Table) -> Result<()> {
        let mut enabled: Vec<&'static Rule> = self.rules.iter().map(|(r, _)| *r).collect();
        let mut options: HashMap<&'static str, RuleOptions> = rules::RULES
            .iter()
            .map(|r| (r.id, RuleOptions::defaults(r)))
            .collect();
        for (key, value) in lint {
            match key.as_str() {
                "disable" => {
                    for id in string_list(value, "lint.disable")? {
                        let rule = known_rule(&id, "lint.disable")?;
                        enabled.retain(|r| r.id != rule.id);
                    }
                }
                "enable" => {
                    for id in string_list(value, "lint.enable")? {
                        let rule = known_rule(&id, "lint.enable")?;
                        if !enabled.iter().any(|r| r.id == rule.id) {
                            enabled.push(rule);
                        }
                    }
                }
                "exclude" => self.exclude = string_list(value, "lint.exclude")?,
                "rules" => {
                    for (id, table) in as_table(value, "lint.rules")? {
                        let rule = known_rule(id, "lint.rules")?;
                        let where_ = format!("lint.rules.{id}");
                        let target = options.get_mut(rule.id).expect("every rule has options");
                        for (name, value) in as_table(table, &where_)? {
                            let option = rule.option(name).ok_or_else(|| {
                                anyhow!("`{where_}`: `{id}` has no option `{name}`")
                            })?;
                            let parsed = match option.kind {
                                OptionKind::Integer(_) => {
                                    OptionValue::Integer(value.as_integer().ok_or_else(|| {
                                        anyhow!("`{where_}.{name}` must be an integer")
                                    })?)
                                }
                                OptionKind::String(_) => OptionValue::String(
                                    value
                                        .as_str()
                                        .ok_or_else(|| {
                                            anyhow!("`{where_}.{name}` must be a string")
                                        })?
                                        .to_string(),
                                ),
                                OptionKind::Pattern(_) => {
                                    let text = value.as_str().ok_or_else(|| {
                                        anyhow!("`{where_}.{name}` must be a string")
                                    })?;
                                    OptionValue::Pattern(Pattern::new(text).map_err(|e| {
                                        anyhow!(
                                            "`{where_}.{name}` is not a valid regular expression: {e}"
                                        )
                                    })?)
                                }
                                OptionKind::StringList(_) => OptionValue::StringList(string_list(
                                    value,
                                    &format!("{where_}.{name}"),
                                )?),
                            };
                            target.values.insert(option.name, parsed);
                        }
                    }
                }
                other => bail!(
                    "unknown key `lint.{other}`; expected `disable`, `enable`, `exclude` or `rules`"
                ),
            }
        }
        // Keep registry order, so output does not depend on the file's order.
        self.rules = rules::RULES
            .iter()
            .filter(|r| enabled.iter().any(|e| e.id == r.id))
            .map(|r| (r, options.remove(r.id).expect("every rule has options")))
            .collect();
        Ok(())
    }
}

fn as_table<'a>(value: &'a Value, key: &str) -> Result<&'a Table> {
    value
        .as_table()
        .ok_or_else(|| anyhow!("`{key}` must be a table"))
}

fn string_list(value: &Value, key: &str) -> Result<Vec<String>> {
    let list = value
        .as_array()
        .ok_or_else(|| anyhow!("`{key}` must be a list of strings"))?;
    list.iter()
        .map(|item| {
            item.as_str()
                .map(str::to_string)
                .ok_or_else(|| anyhow!("`{key}` must be a list of strings"))
        })
        .collect()
}

fn known_rule(id: &str, key: &str) -> Result<&'static Rule> {
    if let Some(rule) = rules::find(id) {
        return Ok(rule);
    }
    if let Some(to) = rules::moved(id) {
        bail!("`{key}`: `{id}` is not a gdtidy lint rule: {to}");
    }
    if rules::PLANNED.contains(&id) {
        bail!("`{key}`: `{id}` is planned but not implemented yet");
    }
    bail!("`{key}`: unknown rule `{id}` (see `gdtidy rules`)")
}

/// Finds and caches the configuration for each folder.
#[derive(Default)]
pub struct Configs {
    by_dir: HashMap<PathBuf, Arc<Config>>,
}

impl Configs {
    /// The configuration that applies to the script at `path`.
    pub fn for_file(&mut self, path: &Path) -> Result<Arc<Config>> {
        let absolute = std::path::absolute(path)
            .with_context(|| format!("cannot resolve {}", path.display()))?;
        let dir = absolute.parent().unwrap_or(Path::new("/")).to_path_buf();
        self.for_dir(&dir)
    }

    fn for_dir(&mut self, dir: &Path) -> Result<Arc<Config>> {
        if let Some(config) = self.by_dir.get(dir) {
            return Ok(config.clone());
        }
        let file = dir.join(FILE_NAME);
        let suite = dir.join(SUITE_FILE_NAME);
        let from_suite = if suite.is_file() {
            let text = std::fs::read_to_string(&suite)
                .with_context(|| format!("cannot read {}", suite.display()))?;
            Config::parse_suite(&text, Some(dir.to_path_buf()))
                .with_context(|| format!("{}", suite.display()))?
        } else {
            None
        };
        let config = if let Some(config) = from_suite {
            if file.is_file() {
                eprintln!(
                    "gdtidy: warning: {} has a [lint] or [fmt] section, so {} is ignored. \
                     Keep gdtidy's settings in one of them.",
                    suite.display(),
                    file.display()
                );
            }
            Arc::new(config)
        } else if file.is_file() {
            let text = std::fs::read_to_string(&file)
                .with_context(|| format!("cannot read {}", file.display()))?;
            Arc::new(
                Config::parse(&text, Some(dir.to_path_buf()))
                    .with_context(|| format!("{}", file.display()))?,
            )
        } else if dir.join("project.godot").is_file() {
            Arc::new(Config::defaults(Some(dir.to_path_buf())))
        } else if let Some(parent) = dir.parent() {
            self.for_dir(parent)?
        } else {
            Arc::new(Config::defaults(None))
        };
        self.by_dir.insert(dir.to_path_buf(), config.clone());
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Config> {
        Config::parse(text, None)
    }

    #[test]
    fn a_gdsuite_toml_section_comes_before_gdtidy_toml() {
        let dir = std::env::temp_dir().join(format!("gdtidy-suite-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("project.godot"), "").unwrap();
        std::fs::write(
            dir.join("gdtidy.toml"),
            "[lint]\ndisable = [\"max-line-length\"]\n",
        )
        .unwrap();
        let script = dir.join("a.gd");

        // A gdsuite.toml without [lint] or [fmt] leaves gdtidy.toml in charge.
        std::fs::write(dir.join("gdsuite.toml"), "[project]\nroot = \".\"\n").unwrap();
        let config = Configs::default().for_file(&script).unwrap();
        assert!(!config.is_enabled("max-line-length"));

        // Other tools' sections are not gdtidy's business.
        std::fs::write(
            dir.join("gdsuite.toml"),
            "[project]\nroot = \".\"\n\n[test]\ntimeout = 5\n\n[fmt]\ntrailing-commas = false\n",
        )
        .unwrap();
        let config = Configs::default().for_file(&script).unwrap();
        // gdtidy.toml is ignored, not merged.
        assert!(config.is_enabled("max-line-length"));
        assert!(!config.fmt.trailing_commas);

        std::fs::write(dir.join("gdsuite.toml"), "[lint]\ndisable = [\"nope\"]\n").unwrap();
        let error = format!("{:#}", Configs::default().for_file(&script).unwrap_err());
        assert!(error.contains("gdsuite.toml"), "{error}");

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn defaults_run_the_rules_that_are_on_by_default() {
        let config = parse("").unwrap();
        assert!(config.is_enabled("max-line-length"));
    }

    #[test]
    fn options_override_defaults() {
        let config = parse("[lint.rules.max-line-length]\nmax = 130\n").unwrap();
        let (_, options) = config
            .rules
            .iter()
            .find(|(rule, _)| rule.id == "max-line-length")
            .unwrap();
        assert_eq!(options.integer("max"), 130);
        assert_eq!(options.integer("tab-width"), 4);
    }

    #[test]
    fn disable_turns_a_rule_off() {
        let config = parse("[lint]\ndisable = [\"max-line-length\"]\n").unwrap();
        assert!(!config.is_enabled("max-line-length"));
    }

    #[test]
    fn mistakes_are_errors() {
        let error = |text: &str| parse(text).unwrap_err().to_string();
        assert!(error("[lint]\ndisable = [\"nope\"]\n").contains("unknown rule `nope`"));
        assert!(error("[lint]\ndisable = [\"unused-argument\"]\n").contains("UNUSED_PARAMETER"));
        assert!(error("[lint.rules.max-line-length]\nmaxx = 1\n").contains("no option `maxx`"));
        assert!(
            error("[lint.rules.max-line-length]\nmax = \"1\"\n").contains("must be an integer")
        );
        assert!(
            error("[lint.rules.class-name]\npattern = \"(\"\n").contains("not a valid regular")
        );
        assert!(error("[lnit]\n").contains("unknown section"));
        assert!(error("[lint]\nignore = []\n").contains("unknown key"));
        assert!(error("[fmt]\nindent = 1\n").contains("unknown key `fmt.indent`"));
        assert!(error("[fmt]\ncontinuation-indent = 0\n").contains("must be 1, 2, 3 or 4"));
        assert!(error("[fmt]\ntrailing-commas = 1\n").contains("true or false"));
    }

    #[test]
    fn fmt_settings() {
        let config = parse(
            "[fmt]\ncontinuation-indent = 1\ndict-brace-spaces = false\nexclude = [\"gen\"]\n",
        )
        .unwrap();
        assert_eq!(config.fmt.continuation_indent, 1);
        assert!(!config.fmt.dict_brace_spaces);
        assert!(config.fmt.trailing_commas);
        assert_eq!(config.fmt_exclude, ["gen"]);
        assert!(config.exclude.is_empty());
    }

    #[test]
    fn patterns_match_the_whole_name() {
        let pattern = Pattern::new("[a-z]+|[A-Z]+").unwrap();
        assert!(pattern.is_match("abc"));
        assert!(pattern.is_match("ABC"));
        assert!(!pattern.is_match("abcDEF"));
    }

    #[test]
    fn exclude_matches_like_gitignore() {
        let mut config =
            parse("[lint]\nexclude = [\"reports\", \"tools/gen/\", \"/build\"]\n").unwrap();
        config.root = Some(PathBuf::from("/p"));
        // A name alone: a folder at any depth, as gdlint's excluded_directories.
        assert!(config.excludes(Path::new("/p/reports/a.gd")));
        assert!(config.excludes(Path::new("/p/src/reports/a.gd")));
        assert!(!config.excludes(Path::new("/p/reports2/a.gd")));
        assert!(!config.excludes(Path::new("/p/reports.gd")));
        // A path: relative to the root.
        assert!(config.excludes(Path::new("/p/tools/gen/b/c.gd")));
        assert!(!config.excludes(Path::new("/p/src/tools/gen/c.gd")));
        assert!(config.excludes(Path::new("/p/build/a.gd")));
        assert!(!config.excludes(Path::new("/p/src/build/a.gd")));
    }
}
