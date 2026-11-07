mod discover;
mod lint;
mod rules;

use std::{fs, path::PathBuf, process::ExitCode};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
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
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
}

#[derive(Args)]
struct RulesArgs {
    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
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
}

fn run_lint(args: LintArgs) -> Result<u8> {
    let files = discover::gdscript_files(&args.paths)?;
    let mut diagnostics = Vec::new();
    for file in &files {
        let bytes = fs::read(file)
            .with_context(|| format!("cannot read {}", discover::display_path(file)))?;
        diagnostics.extend(lint::lint_file(file, bytes));
    }

    match args.format {
        Format::Json => {
            let report = LintReport {
                version: 1,
                files: files.len(),
                diagnostics,
            };
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(if report.diagnostics.is_empty() {
                OK
            } else {
                PROBLEMS
            });
        }
        Format::Text => {
            for d in &diagnostics {
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
            eprintln!(
                "gdtidy: {} problem(s) ({} files)",
                diagnostics.len(),
                files.len()
            );
        }
    }
    Ok(if diagnostics.is_empty() { OK } else { PROBLEMS })
}

fn list_rules(args: RulesArgs) -> Result<u8> {
    match args.format {
        Format::Json => println!("{}", serde_json::to_string_pretty(rules::RULES)?),
        Format::Text => {
            for rule in rules::RULES {
                println!(
                    "{:<16} {:<8} {}",
                    rule.id,
                    rule.severity.as_str(),
                    rule.summary
                );
            }
        }
    }
    Ok(OK)
}
