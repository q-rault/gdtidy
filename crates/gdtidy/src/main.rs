mod config;
mod discover;
mod fmt;
mod lint;
mod migrate;
mod rules;
mod sarif;
mod suppress;

use std::{fs, path::PathBuf, process::ExitCode};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use rayon::prelude::*;
use serde::Serialize;

/// Format and lint GDScript, following the official Godot style guide.
#[derive(Parser)]
#[command(name = "gdtidy", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Format GDScript files, following the official style guide
    Fmt(FmtArgs),
    /// Report style and convention problems in GDScript files
    Lint(LintArgs),
    /// List every rule gdtidy can report
    Rules(RulesArgs),
    /// Convert a gdlintrc into a gdtidy.toml next to it
    Migrate(MigrateArgs),
}

#[derive(Args)]
struct MigrateArgs {
    /// The gdlintrc to convert [default: gdlintrc or .gdlintrc here]
    gdlintrc: Option<PathBuf>,
    /// Print the configuration instead of writing gdtidy.toml
    #[arg(long)]
    stdout: bool,
    /// Replace an existing gdtidy.toml
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
struct FmtArgs {
    /// Files or folders to format; `-` reads standard input and writes
    /// standard output
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,
    /// Change nothing; list the files that would change
    #[arg(long)]
    check: bool,
    /// Change nothing; print the changes as a unified diff
    #[arg(long)]
    diff: bool,
    #[arg(long, value_enum, default_value_t = RulesFormat::Text)]
    format: RulesFormat,
}

#[derive(Args)]
struct LintArgs {
    /// Files or folders to lint
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,
    #[arg(long, value_enum, default_value_t = LintFormat::Text)]
    format: LintFormat,
    /// Repair the problems that can be repaired safely, then report the rest
    #[arg(long)]
    fix: bool,
}

#[derive(Args)]
struct RulesArgs {
    #[arg(long, value_enum, default_value_t = RulesFormat::Text)]
    format: RulesFormat,
}

#[derive(Clone, Copy, ValueEnum)]
enum LintFormat {
    Text,
    Json,
    /// SARIF 2.1.0, for GitHub code scanning
    Sarif,
}

#[derive(Clone, Copy, ValueEnum)]
enum RulesFormat {
    Text,
    Json,
}

/// Exit codes, shared with gdtypecheck: no problems, problems, could not run.
const OK: u8 = 0;
const PROBLEMS: u8 = 1;
const FAILED: u8 = 2;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Command::Fmt(args) => fmt(args),
        Command::Lint(args) => run_lint(args),
        Command::Rules(args) => list_rules(args),
        Command::Migrate(args) => run_migrate(args),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("gdtidy: {error:#}");
            ExitCode::from(FAILED)
        }
    }
}

#[derive(Serialize)]
struct FmtReport {
    version: u32,
    files: usize,
    /// Files that were formatted, or with --check or --diff, would be.
    changed: Vec<String>,
    skipped: Vec<lint::Skipped>,
}

enum Formatted {
    Unchanged,
    Changed { before: String, after: String },
    Skipped(lint::Skipped),
}

fn format_bytes(shown: &str, bytes: &[u8], options: &fmt::Options) -> Formatted {
    match fmt::format(bytes, options) {
        Ok(after) if after.as_bytes() == bytes => Formatted::Unchanged,
        Ok(after) => Formatted::Changed {
            before: String::from_utf8_lossy(bytes).into_owned(),
            after,
        },
        Err(failure) => Formatted::Skipped(match failure {
            fmt::Failure::Syntax {
                line,
                column,
                message,
                not_utf8,
            } => lint::Skipped {
                path: shown.to_string(),
                line,
                column,
                reason: if not_utf8 {
                    lint::SkipReason::NotUtf8
                } else {
                    lint::SkipReason::SyntaxError
                },
                message,
            },
            fmt::Failure::Unsafe => lint::Skipped {
                path: shown.to_string(),
                line: 1,
                column: 1,
                reason: lint::SkipReason::Unsafe,
                message: "formatting would have changed the code, so the file was left as it is; \
                          this is a gdtidy bug, please report it"
                    .to_string(),
            },
        }),
    }
}

fn fmt(args: FmtArgs) -> Result<u8> {
    if args.paths.len() == 1 && args.paths[0].as_os_str() == "-" {
        return fmt_stdin(&args);
    }
    let mut configs = config::Configs::default();
    let mut jobs = Vec::new();
    for target in discover::gdscript_files(&args.paths)? {
        let config = configs.for_file(&target.path)?;
        if !target.explicit && config.fmt_excludes(&std::path::absolute(&target.path)?) {
            continue;
        }
        jobs.push((target.path, config));
    }
    let write = !args.check && !args.diff;
    let outcomes: Vec<(String, Formatted)> = jobs
        .par_iter()
        .map(|(path, config)| {
            let shown = discover::display_path(path);
            let bytes = fs::read(path).with_context(|| format!("cannot read {shown}"))?;
            let outcome = format_bytes(&shown, &bytes, &config.fmt);
            if write && let Formatted::Changed { after, .. } = &outcome {
                fs::write(path, after).with_context(|| format!("cannot write {shown}"))?;
            }
            Ok((shown, outcome))
        })
        .collect::<Result<_>>()?;

    let mut report = FmtReport {
        version: 1,
        files: jobs.len(),
        changed: Vec::new(),
        skipped: Vec::new(),
    };
    for (shown, outcome) in &outcomes {
        match outcome {
            Formatted::Unchanged => {}
            Formatted::Changed { before, after } => {
                report.changed.push(shown.clone());
                if args.diff && matches!(args.format, RulesFormat::Text) {
                    print!("{}", unified_diff(shown, before, after));
                }
            }
            Formatted::Skipped(skipped) => report.skipped.push(skipped.clone()),
        }
    }
    match args.format {
        RulesFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        RulesFormat::Text => {
            if args.check {
                for path in &report.changed {
                    println!("{path}");
                }
            }
            for s in &report.skipped {
                eprintln!("{}:{}:{}: skipped: {}", s.path, s.line, s.column, s.message);
            }
            let verb = if write {
                "formatted"
            } else {
                "would be formatted"
            };
            let mut summary = format!(
                "gdtidy: {} file(s) {verb} ({} files",
                report.changed.len(),
                report.files
            );
            if !report.skipped.is_empty() {
                summary += &format!(", {} skipped", report.skipped.len());
            }
            eprintln!("{summary})");
        }
    }
    Ok(if !report.skipped.is_empty() {
        FAILED
    } else if !write && !report.changed.is_empty() {
        PROBLEMS
    } else {
        OK
    })
}

/// `gdtidy fmt -`: for editors. The settings are those of the current folder.
fn fmt_stdin(args: &FmtArgs) -> Result<u8> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes)?;
    let config = config::Configs::default().for_file(std::path::Path::new("stdin.gd"))?;
    match format_bytes("<stdin>", &bytes, &config.fmt) {
        Formatted::Unchanged => {
            if !args.check && !args.diff {
                std::io::Write::write_all(&mut std::io::stdout(), &bytes)?;
            }
            Ok(OK)
        }
        Formatted::Changed { before, after } => {
            if args.diff {
                print!("{}", unified_diff("<stdin>", &before, &after));
            } else if !args.check {
                print!("{after}");
            }
            Ok(if args.check || args.diff {
                PROBLEMS
            } else {
                OK
            })
        }
        Formatted::Skipped(s) => {
            eprintln!("{}:{}:{}: skipped: {}", s.path, s.line, s.column, s.message);
            Ok(FAILED)
        }
    }
}

fn unified_diff(path: &str, before: &str, after: &str) -> String {
    similar::TextDiff::from_lines(before, after)
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string()
}

#[derive(Serialize)]
struct LintReport {
    version: u32,
    files: usize,
    /// Problems `--fix` repaired; they are not in `diagnostics`.
    fixed: usize,
    diagnostics: Vec<lint::Diagnostic>,
    /// Files that could not be checked, such as files with a syntax error.
    skipped: Vec<lint::Skipped>,
}

/// Problems give 1; a file that could not be checked gives 2 (the check did
/// not fully run), whether or not other files had problems.
fn lint_exit_code(report: &LintReport) -> u8 {
    if !report.skipped.is_empty() {
        FAILED
    } else if !report.diagnostics.is_empty() {
        PROBLEMS
    } else {
        OK
    }
}

fn run_lint(args: LintArgs) -> Result<u8> {
    // Configurations are found first, in order, so errors in them are
    // reported the same way every time; the files are then linted in
    // parallel, and the results keep the files' order.
    let mut configs = config::Configs::default();
    let mut jobs = Vec::new();
    for target in discover::gdscript_files(&args.paths)? {
        let config = configs.for_file(&target.path)?;
        if !target.explicit && config.excludes(&std::path::absolute(&target.path)?) {
            continue;
        }
        jobs.push((target.path, config));
    }
    let outcomes: Vec<(lint::Outcome, usize)> = jobs
        .par_iter()
        .map(|(path, config)| {
            let shown = discover::display_path(path);
            let mut bytes = fs::read(path).with_context(|| format!("cannot read {shown}"))?;
            let mut fixed = 0;
            if args.fix
                && let Some((text, count)) = lint::fix_file(shown.clone(), &bytes, config)
            {
                fs::write(path, &text).with_context(|| format!("cannot write {shown}"))?;
                bytes = text.into_bytes();
                fixed = count;
            }
            Ok((lint::lint_file(shown, &bytes, config), fixed))
        })
        .collect::<Result<_>>()?;

    let mut report = LintReport {
        version: 1,
        files: jobs.len(),
        fixed: outcomes.iter().map(|(_, fixed)| fixed).sum(),
        diagnostics: Vec::new(),
        skipped: Vec::new(),
    };
    for (outcome, _) in outcomes {
        match outcome {
            lint::Outcome::Checked(diagnostics) => report.diagnostics.extend(diagnostics),
            lint::Outcome::Skipped(skipped) => report.skipped.push(skipped),
        }
    }

    match args.format {
        LintFormat::Json => println!("{}", serde_json::to_string_pretty(&report)?),
        LintFormat::Sarif => println!(
            "{}",
            serde_json::to_string_pretty(&sarif::report(&report.diagnostics, &report.skipped))?
        ),
        LintFormat::Text => {
            for d in &report.diagnostics {
                println!(
                    "{}:{}:{}: {}[{}]: {}",
                    d.path,
                    d.line,
                    d.column,
                    d.severity.as_str(),
                    d.rule,
                    d.message
                );
            }
            for s in &report.skipped {
                eprintln!("{}:{}:{}: skipped: {}", s.path, s.line, s.column, s.message);
            }
            let mut summary = format!(
                "gdtidy: {} problem(s) ({} files",
                report.diagnostics.len(),
                report.files
            );
            if !report.skipped.is_empty() {
                summary += &format!(", {} skipped", report.skipped.len());
            }
            if report.fixed > 0 {
                summary += &format!(", {} fixed", report.fixed);
            }
            eprintln!("{summary})");
        }
    }
    Ok(lint_exit_code(&report))
}

fn list_rules(args: RulesArgs) -> Result<u8> {
    match args.format {
        RulesFormat::Json => println!("{}", serde_json::to_string_pretty(rules::RULES)?),
        RulesFormat::Text => {
            for rule in rules::RULES {
                let state = if rule.default_enabled { "on" } else { "off" };
                println!("{:<28} {:<4} {}", rule.id, state, rule.summary);
            }
        }
    }
    Ok(OK)
}

fn run_migrate(args: MigrateArgs) -> Result<u8> {
    let source = match args.gdlintrc {
        Some(path) => path,
        None => ["gdlintrc", ".gdlintrc"]
            .into_iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
            .context("no gdlintrc or .gdlintrc in this folder; name the file to convert")?,
    };
    let text =
        fs::read_to_string(&source).with_context(|| format!("cannot read {}", source.display()))?;
    let migration = migrate::migrate(&text).with_context(|| format!("{}", source.display()))?;
    for note in &migration.notes {
        eprintln!("gdtidy: {note}");
    }
    if args.stdout {
        print!("{}", migration.toml);
        return Ok(OK);
    }
    let target = source
        .parent()
        .unwrap_or(std::path::Path::new(""))
        .join(config::FILE_NAME);
    if target.exists() && !args.force {
        anyhow::bail!(
            "{} already exists; use --force to replace it, or --stdout to see the result",
            target.display()
        );
    }
    fs::write(&target, &migration.toml)
        .with_context(|| format!("cannot write {}", target.display()))?;
    eprintln!(
        "gdtidy: wrote {} from {}; the gdlintrc is left as it is",
        target.display(),
        source.display()
    );
    Ok(OK)
}
