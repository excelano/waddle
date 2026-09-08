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

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use waddle_core::Target;

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
    /// The command was well-formed and the input was not.
    Input(String),
}

fn run(cli: &Cli) -> Result<(), Failure> {
    let from_stdin = cli.input.as_os_str() == "-";
    if !from_stdin && !cli.input.is_file() {
        return Err(Failure::Input(format!(
            "cannot read {}: no such file",
            cli.input.display()
        )));
    }
    Err(Failure::Input(format!(
        "writing {} is not implemented",
        cli.to
    )))
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Failure::Input(message)) => {
            let (red, reset) = if cli.color.enabled() {
                ("\x1b[31m", "\x1b[0m")
            } else {
                ("", "")
            };
            eprintln!("{red}waddle:{reset} {message}");
            ExitCode::from(1)
        }
    }
}
