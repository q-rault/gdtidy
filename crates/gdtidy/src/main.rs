mod config;
mod discover;
mod lint;
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
    /// Format GDScript files (not implemented yet)
    Fmt(FmtArgs),
    /// Report style and convention problems in GDScript files
    Lint(LintArgs),
    /// List every rule gdtidy can report
    Rules(RulesArgs),
}

#[derive(Args)]
struct FmtArgs {
    /// Files or folders to format
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,
    /// Report files that would change, without writing them
    #[arg(long)]
    check: bool,
}

#[derive(Args)]
struct LintArgs {
    /// Files or folders to lint
    #[arg(default_value = ".")]
    paths: Vec<PathBuf>,
    #[arg(long, value_enum, default_value_t = LintFormat::Text)]
    format: LintFormat,
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
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("gdtidy: {error:#}");
            ExitCode::from(FAILED)
        }
    }
}

fn fmt(args: FmtArgs) -> Result<u8> {
    let _ = (args.paths, args.check);
    anyhow::bail!("formatting is not implemented yet")
}

#[derive(Serialize)]
struct LintReport {
    version: u32,
    files: usize,
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
    let outcomes: Vec<lint::Outcome> = jobs
        .par_iter()
        .map(|(path, config)| {
            let shown = discover::display_path(path);
            let bytes = fs::read(path).with_context(|| format!("cannot read {shown}"))?;
            Ok(lint::lint_file(shown, &bytes, config))
        })
        .collect::<Result<_>>()?;

    let mut report = LintReport {
        version: 1,
        files: jobs.len(),
        diagnostics: Vec::new(),
        skipped: Vec::new(),
    };
    for outcome in outcomes {
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
