//! waddle: the command-line tool over `waddle-core`.
//!
//! It reads one DocLang, DocLang archive or docling JSON file, hands the
//! document to the library, and writes one package beside the input or where
//! `-o` says. It follows the fleet's agent-facing CLI contract: exit 0 for
//! success including a run that dropped nodes with warnings, 1 for an input
//! it cannot read or represent, 2 for a wrong command line; `-` for stdin;
//! colour only on a terminal.
//!
//! Author: David M. Anderson
//! Built with AI assistance (Claude, Anthropic)

#![deny(unsafe_code)]

mod assets;
mod input;
mod output;

use std::collections::BTreeMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use waddle_core::{Target, odt};

const EXIT_CODES: &str = "\
Exit codes:
  0  the package was written, or --dry-run listed what would be; dropped nodes
     are reported on stderr and are not a failure
  1  the input could not be read, or under --strict could not be represented
  2  the command line was wrong";

/// Write an ODT or DOCX from a DocLang or docling JSON document.
#[derive(Parser, Debug)]
#[command(version, about, after_help = EXIT_CODES)]
struct Cli {
    /// DocLang (.dclg), DocLang archive (.dclx) or docling JSON; `-` reads stdin
    input: PathBuf,

    /// Output format: odt or docx
    #[arg(long, value_name = "FORMAT")]
    to: Target,

    /// Output file, or a directory to write into; the default is beside the input
    #[arg(short, long, value_name = "PATH")]
    output: Option<PathBuf>,

    /// List what would be written and what would be dropped, and write nothing
    #[arg(long)]
    dry_run: bool,

    /// Refuse to write if anything would be dropped
    #[arg(long)]
    strict: bool,

    /// When to colour diagnostics
    #[arg(long, value_enum, default_value_t = ColorMode::Auto)]
    color: ColorMode,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
enum ColorMode {
    Auto,
    Always,
    Never,
}

impl ColorMode {
    /// Colour on `stderr`, where every diagnostic goes. `auto` needs a
    /// terminal and no `NO_COLOR`; `always` overrides `NO_COLOR`, because a
    /// caller who named the flag has said what they want.
    fn enabled(self) -> bool {
        match self {
            ColorMode::Always => true,
            ColorMode::Never => false,
            ColorMode::Auto => {
                std::io::stderr().is_terminal()
                    && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
            }
        }
    }
}

/// The failure exit codes; success is `ExitCode::SUCCESS`.
enum Failure {
    /// The command was well-formed and the input was not: exit 1.
    Input(String),
    /// The command line itself was wrong: exit 2.
    Usage(String),
}

/// What a run has to say on stderr before it says whether it wrote.
struct Report {
    colored: bool,
    /// Each distinct warning line with how often it occurred.
    lines: BTreeMap<String, usize>,
}

impl Report {
    fn add(&mut self, line: String) {
        *self.lines.entry(line).or_insert(0) += 1;
    }

    fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    fn print(&self, verb: &str) {
        if self.lines.is_empty() {
            return;
        }
        let total: usize = self.lines.values().sum();
        let (yellow, reset) = if self.colored {
            ("\x1b[33m", "\x1b[0m")
        } else {
            ("", "")
        };
        eprintln!(
            "{yellow}waddle:{reset} {total} {} {verb}:",
            if total == 1 { "node" } else { "nodes" }
        );
        for (line, count) in &self.lines {
            if *count > 1 {
                eprintln!("  {line} ({count})");
            } else {
                eprintln!("  {line}");
            }
        }
    }
}

fn run(cli: &Cli) -> Result<(), Failure> {
    let from_stdin = cli.input.as_os_str() == "-";
    let input = input::read(&cli.input).map_err(Failure::Input)?;
    let mut doc = input::convert(&input).map_err(Failure::Input)?;
    let mut report = Report {
        colored: cli.color.enabled(),
        lines: BTreeMap::new(),
    };
    for problem in assets::resolve(&mut doc, &input) {
        report.add(problem);
    }
    let out = match cli.to {
        Target::Odt => odt::write(&doc).map_err(|e| Failure::Input(e.to_string()))?,
        Target::Docx => {
            return Err(Failure::Input(
                "writing docx is not implemented".to_string(),
            ));
        }
    };
    for warning in &out.warnings {
        report.add(warning.to_string());
    }
    let input_path: Option<&Path> = (!from_stdin).then_some(cli.input.as_path());
    let destination = output::destination(
        input_path,
        &input.name,
        cli.output.as_deref(),
        cli.to.extension(),
    )
    .map_err(Failure::Usage)?;
    let destination = output::available(&destination);

    if cli.strict && !report.is_empty() {
        report.print("would be dropped or degraded");
        return Err(Failure::Input(format!(
            "not writing {}: --strict and the document cannot be carried whole",
            destination.display()
        )));
    }
    if cli.dry_run {
        println!("would write {}", destination.display());
        report.print("would be dropped or degraded");
        return Ok(());
    }
    std::fs::write(&destination, &out.bytes).map_err(|e| {
        Failure::Input(format!(
            "cannot write {}: {}",
            destination.display(),
            input::reason(&e)
        ))
    })?;
    println!("{}", destination.display());
    report.print("dropped or degraded");
    Ok(())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let (red, reset) = if cli.color.enabled() {
        ("\x1b[31m", "\x1b[0m")
    } else {
        ("", "")
    };
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Input(message)) => {
            eprintln!("{red}waddle:{reset} {message}");
            ExitCode::from(1)
        }
        Err(Failure::Usage(message)) => {
            eprintln!("{red}waddle:{reset} {message}");
            ExitCode::from(2)
        }
    }
}
