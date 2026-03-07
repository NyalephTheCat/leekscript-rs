//! CLI types and command runners for the leekscript binary.

use std::fs;
use std::io::Read;
use std::path::Path;

use clap::{Parser, Subcommand};
use sipha::engine::ParseError;
use sipha::red::SyntaxNode;

use leekscript_rs::formatter::FormatterOptions;
use leekscript_rs::{
    analyze, analyze_with_signatures, format, parse, parse_error_to_miette, parse_signatures,
    LineIndex,
};

/// Exit code for successful completion.
const EXIT_SUCCESS: i32 = 0;
/// Exit code for failure (syntax error, I/O error, etc.).
const EXIT_FAILURE: i32 = 1;

#[derive(Parser)]
#[command(name = "leekscript")]
#[command(author, version, about = "Format, validate, and manipulate LeekScript source code")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Format LeekScript source files or stdin.
    Format(FormatArgs),
    /// Check syntax and optionally run analyses (stub).
    Validate(ValidateArgs),
}

#[derive(Parser)]
pub struct FormatArgs {
    /// Input file (default: stdin).
    #[arg(value_name = "FILE")]
    pub input: Option<std::path::PathBuf>,

    /// Output file (default: stdout). Use with --in-place for overwriting.
    #[arg(short, long, value_name = "FILE")]
    pub output: Option<std::path::PathBuf>,

    /// Overwrite input file with output (ignores --output).
    #[arg(long)]
    pub in_place: bool,

    /// Check if output would differ from input; exit 1 if so (no write).
    #[arg(long)]
    pub check: bool,

    /// Include comments and whitespace in output (default: true).
    #[arg(long, default_value = "true")]
    pub preserve_comments: bool,
}

#[derive(Parser)]
pub struct ValidateArgs {
    /// Input file (default: stdin).
    #[arg(value_name = "FILE")]
    pub input: Option<std::path::PathBuf>,

    /// Emit machine-readable output (e.g. for editors).
    #[arg(long)]
    pub json: bool,

    /// Path to a directory containing .sig files (e.g. stdlib). All *.sig in the dir are loaded.
    #[arg(long, value_name = "DIR")]
    pub stdlib_dir: Option<std::path::PathBuf>,

    /// Signature file(s) to load (function/global/class API). May be repeated.
    #[arg(long = "signatures", value_name = "FILE")]
    pub signature_files: Vec<std::path::PathBuf>,
}

/// Result of reading input and parsing (used by format and validate).
pub enum ParseOutcome {
    Success(String, SyntaxNode),
    Empty,
    ParseError(ParseError, String),
    IoError(String),
}

/// Read source from file or stdin.
pub fn read_input(file: Option<&Path>) -> Result<String, String> {
    let s = match file {
        Some(path) => std::fs::read_to_string(path).map_err(|e| e.to_string())?,
        None => {
            let mut s = String::new();
            std::io::stdin().read_to_string(&mut s).map_err(|e| e.to_string())?;
            s
        }
    };
    Ok(s)
}

/// Read source and parse; centralises filename and miette error reporting.
pub fn read_and_parse(input: Option<&Path>) -> ParseOutcome {
    let source = match read_input(input) {
        Ok(s) => s,
        Err(e) => return ParseOutcome::IoError(e),
    };
    match parse(&source) {
        Ok(Some(root)) => ParseOutcome::Success(source, root),
        Ok(None) => ParseOutcome::Empty,
        Err(e) => ParseOutcome::ParseError(e, source),
    }
}

fn filename_from_input(input: Option<&Path>) -> &str {
    input
        .and_then(|p| p.to_str())
        .unwrap_or("<stdin>")
}

fn report_parse_error(e: &ParseError, source: &str, filename: &str) {
    if let Some(report) = parse_error_to_miette(e, source, filename) {
        eprintln!("{:?}", report);
    } else {
        eprintln!("leekscript: parse error: {}", e);
    }
}

/// Handles a failed parse outcome: reports error (or emits JSON when `json` is true) and returns EXIT_FAILURE.
fn handle_parse_failure(
    outcome: ParseOutcome,
    input: Option<&Path>,
    json: bool,
    command_label: &str,
) -> i32 {
    match outcome {
        ParseOutcome::Success(_, _) => unreachable!("handle_parse_failure only for failures"),
        ParseOutcome::Empty => {
            if json {
                println!("{}", serde_json::json!({ "valid": false, "message": "empty parse" }));
            } else {
                eprintln!("leekscript {}: empty parse result", command_label);
            }
            EXIT_FAILURE
        }
        ParseOutcome::ParseError(e, source) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "valid": false, "message": e.to_string() })
                );
            } else {
                report_parse_error(&e, &source, filename_from_input(input));
            }
            EXIT_FAILURE
        }
        ParseOutcome::IoError(e) => {
            if json {
                println!("{}", serde_json::json!({ "valid": false, "message": e }));
            } else {
                eprintln!("leekscript {}: {}", command_label, e);
            }
            EXIT_FAILURE
        }
    }
}

pub fn run_format(args: &FormatArgs) -> i32 {
    let input = args.input.as_deref();
    let outcome = read_and_parse(input);

    match outcome {
        ParseOutcome::Success(source, root) => {
            let options = formatter_options_from_args(args);
            let formatted = format(&root, &options);

            if args.check {
                if source != formatted {
                    eprintln!(
                        "leekscript format: output would differ from input (use --in-place to apply)"
                    );
                    return EXIT_FAILURE;
                }
                return EXIT_SUCCESS;
            }

            if args.in_place {
                let path = match &args.input {
                    Some(p) => p.clone(),
                    None => {
                        eprintln!("leekscript format: --in-place requires an input file");
                        return EXIT_FAILURE;
                    }
                };
                if let Err(e) = std::fs::write(&path, &formatted) {
                    eprintln!("leekscript format: write error: {}", e);
                    return EXIT_FAILURE;
                }
                return EXIT_SUCCESS;
            }

            if let Some(ref out_path) = args.output {
                if let Err(e) = std::fs::write(out_path, &formatted) {
                    eprintln!("leekscript format: write error: {}", e);
                    return EXIT_FAILURE;
                }
                return EXIT_SUCCESS;
            }

            print!("{}", formatted);
            EXIT_SUCCESS
        }
        ParseOutcome::Empty => handle_parse_failure(ParseOutcome::Empty, input, false, "format"),
        ParseOutcome::ParseError(e, source) => handle_parse_failure(
            ParseOutcome::ParseError(e, source),
            input,
            false,
            "format",
        ),
        ParseOutcome::IoError(e) => {
            handle_parse_failure(ParseOutcome::IoError(e), input, false, "format")
        }
    }
}

/// Load signature AST roots from a directory (all *.sig files).
fn load_signatures_from_dir(dir: &Path) -> Vec<sipha::red::SyntaxNode> {
    let mut roots = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return roots;
    };
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().map_or(false, |e| e == "sig"))
        .collect();
    files.sort_by_key(|p| p.as_os_str().to_owned());
    for path in files {
        if let Ok(s) = fs::read_to_string(&path) {
            if let Ok(Some(node)) = parse_signatures(&s) {
                roots.push(node);
            }
        }
    }
    roots
}

/// Load signature AST roots from explicit file paths.
fn load_signatures_from_files(paths: &[std::path::PathBuf]) -> Vec<sipha::red::SyntaxNode> {
    let mut roots = Vec::new();
    for path in paths {
        if let Ok(s) = fs::read_to_string(path) {
            if let Ok(Some(node)) = parse_signatures(&s) {
                roots.push(node);
            }
        }
    }
    roots
}

/// If no signature options were given, try default examples/signatures directory (stdlib_*.sig).
fn default_signature_roots() -> Vec<sipha::red::SyntaxNode> {
    let default_dir = Path::new("examples/signatures");
    if default_dir.is_dir() {
        load_signatures_from_dir(default_dir)
    } else {
        Vec::new()
    }
}

pub fn run_validate(args: &ValidateArgs) -> i32 {
    let input = args.input.as_deref();
    match read_and_parse(input) {
        ParseOutcome::Success(source, root) => {
            let mut signature_roots = Vec::new();
            if let Some(ref dir) = args.stdlib_dir {
                signature_roots.extend(load_signatures_from_dir(dir));
            }
            if !args.signature_files.is_empty() {
                signature_roots.extend(load_signatures_from_files(&args.signature_files));
            }
            if signature_roots.is_empty()
                && args.stdlib_dir.is_none()
                && args.signature_files.is_empty()
            {
                signature_roots = default_signature_roots();
            }
            let result = if signature_roots.is_empty() {
                analyze(&root)
            } else {
                analyze_with_signatures(&root, &signature_roots)
            };
            if result.has_errors() {
                if args.json {
                    let messages: Vec<String> = result
                        .diagnostics
                        .iter()
                        .filter(|d| d.severity == sipha::error::Severity::Error)
                        .map(|d| d.message.clone())
                        .collect();
                    println!(
                        "{}",
                        serde_json::json!({ "valid": false, "errors": messages })
                    );
                } else {
                    let line_index = LineIndex::new(source.as_bytes());
                    for d in &result.diagnostics {
                        if d.severity == sipha::error::Severity::Error {
                            eprintln!(
                                "{}",
                                d.format_with_source(source.as_bytes(), &line_index)
                            );
                        }
                    }
                }
                return EXIT_FAILURE;
            }
            if args.json {
                println!("{}", serde_json::json!({ "valid": true }));
            }
            EXIT_SUCCESS
        }
        ParseOutcome::Empty => handle_parse_failure(ParseOutcome::Empty, input, args.json, "validate"),
        ParseOutcome::ParseError(e, source) => handle_parse_failure(
            ParseOutcome::ParseError(e, source),
            input,
            args.json,
            "validate",
        ),
        ParseOutcome::IoError(e) => {
            handle_parse_failure(ParseOutcome::IoError(e), input, args.json, "validate")
        }
    }
}

fn formatter_options_from_args(args: &FormatArgs) -> FormatterOptions {
    FormatterOptions {
        preserve_comments: args.preserve_comments,
    }
}
