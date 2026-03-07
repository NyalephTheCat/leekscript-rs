//! CLI types and command runners for the leekscript binary.

use std::fs;
use std::io::Read;
use std::path::Path;

use clap::{Parser, Subcommand};
use sipha::engine::ParseError;
use sipha::red::SyntaxNode;

use leekscript_rs::formatter::{BraceStyle, FormatterOptions, IndentStyle, SemicolonStyle};
use leekscript_rs::{
    analyze, analyze_with_signatures, expand_includes, format, parse, parse_error_to_miette,
    parse_signatures, IncludeError, LineIndex,
};

/// Exit code for successful completion.
const EXIT_SUCCESS: i32 = 0;
/// Exit code for failure (syntax error, I/O error, etc.).
const EXIT_FAILURE: i32 = 1;

/// Default directory for .sig files when no `--stdlib-dir` or `--signatures` are given.
/// Override with env var `LEEKSCRIPT_SIGNATURES_DIR`.
const DEFAULT_SIGNATURES_DIR: &str = "examples/signatures";

#[derive(Parser)]
#[command(name = "leekscript")]
#[command(author, version, about = "Format, validate, and manipulate LeekScript source code")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Format `LeekScript` source files or stdin.
    Format(FormatArgs),
    /// Check syntax and run semantic analysis (scopes, types, deprecations).
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

    /// Wrap expressions in parentheses to make precedence explicit.
    #[arg(long)]
    pub parenthesize_expressions: bool,

    /// Add block comments with inferred types after expressions and variables.
    #[arg(long)]
    pub annotate_types: bool,

    /// When using --annotate-types: directory containing .sig files. All *.sig are loaded. Default: LEEKSCRIPT_SIGNATURES_DIR or examples/signatures.
    #[arg(long, value_name = "DIR")]
    pub stdlib_dir: Option<std::path::PathBuf>,

    /// When using --annotate-types: signature file(s) to load (function/global/class API). May be repeated.
    #[arg(long = "signatures", value_name = "FILE")]
    pub signature_files: Vec<std::path::PathBuf>,

    /// Normalize layout: re-indent, apply brace/semicolon style. Ignores source whitespace and comments.
    #[arg(long)]
    pub canonical: bool,

    /// Indent with "tabs" or "spaces" (default: 4). Used when --canonical.
    #[arg(long, value_name = "tabs|spaces[N]", default_value = "tabs")]
    pub indent: String,

    /// Brace style: "same-line" or "next-line". Used when --canonical.
    #[arg(long, value_name = "STYLE", default_value = "same-line")]
    pub brace_style: String,

    /// Semicolon style: "always" or "omit". Used when --canonical.
    #[arg(long, value_name = "STYLE", default_value = "always")]
    pub semicolon_style: String,
}

#[derive(Parser)]
pub struct ValidateArgs {
    /// Input file (default: stdin).
    #[arg(value_name = "FILE")]
    pub input: Option<std::path::PathBuf>,

    /// Emit machine-readable output (e.g. for editors).
    #[arg(long)]
    pub json: bool,

    /// Path to a directory containing .sig files. All *.sig in the dir are loaded. Default: LEEKSCRIPT_SIGNATURES_DIR or examples/signatures.
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
    IncludeError(IncludeError),
}

/// Read source from file or stdin.
pub fn read_input(file: Option<&Path>) -> Result<String, String> {
    let s = if let Some(path) = file { std::fs::read_to_string(path).map_err(|e| e.to_string())? } else {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).map_err(|e| e.to_string())?;
        s
    };
    Ok(s)
}

/// Read source and parse; centralises filename and miette error reporting.
/// Expands `include(path)` relative to the input file (or cwd when reading from stdin).
pub fn read_and_parse(input: Option<&Path>) -> ParseOutcome {
    let source = match read_input(input) {
        Ok(s) => s,
        Err(e) => return ParseOutcome::IoError(e),
    };
    let source = match expand_includes(&source, input) {
        Ok(expanded) => expanded,
        Err(e) => return ParseOutcome::IncludeError(e),
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
        eprintln!("{report:?}");
    } else {
        eprintln!("leekscript: parse error: {e}");
    }
}

/// Handles a failed parse outcome: reports error (or emits JSON when `json` is true) and returns `EXIT_FAILURE`.
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
                eprintln!("leekscript {command_label}: empty parse result");
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
        ParseOutcome::IncludeError(e) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({ "valid": false, "message": e.to_string() })
                );
            } else {
                eprintln!("leekscript {command_label}: {e}");
            }
            EXIT_FAILURE
        }
        ParseOutcome::IoError(e) => {
            if json {
                println!("{}", serde_json::json!({ "valid": false, "message": e }));
            } else {
                eprintln!("leekscript {command_label}: {e}");
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
                let path = if let Some(p) = &args.input { p.clone() } else {
                    eprintln!("leekscript format: --in-place requires an input file");
                    return EXIT_FAILURE;
                };
                if let Err(e) = std::fs::write(&path, &formatted) {
                    eprintln!("leekscript format: write error: {e}");
                    return EXIT_FAILURE;
                }
                return EXIT_SUCCESS;
            }

            if let Some(ref out_path) = args.output {
                if let Err(e) = std::fs::write(out_path, &formatted) {
                    eprintln!("leekscript format: write error: {e}");
                    return EXIT_FAILURE;
                }
                eprintln!("leekscript format: wrote {} ({} bytes)", out_path.display(), formatted.len());
                return EXIT_SUCCESS;
            }

            print!("{formatted}");
            EXIT_SUCCESS
        }
        ParseOutcome::Empty => handle_parse_failure(ParseOutcome::Empty, input, false, "format"),
        ParseOutcome::ParseError(e, source) => handle_parse_failure(
            ParseOutcome::ParseError(e, source),
            input,
            false,
            "format",
        ),
        ParseOutcome::IncludeError(e) => {
            handle_parse_failure(ParseOutcome::IncludeError(e), input, false, "format")
        }
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
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "sig"))
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

/// If no signature options were given, use the default .sig path: `LEEKSCRIPT_SIGNATURES_DIR` env var if set,
/// else `DEFAULT_SIGNATURES_DIR`, else `leekscript-rs/examples/signatures` (when run from workspace root).
fn default_signature_roots() -> Vec<sipha::red::SyntaxNode> {
    let candidates: Vec<std::path::PathBuf> = if let Some(ref d) = std::env::var_os("LEEKSCRIPT_SIGNATURES_DIR") {
        vec![d.into()]
    } else {
        vec![
            std::path::PathBuf::from(DEFAULT_SIGNATURES_DIR),
            std::path::PathBuf::from("leekscript-rs/examples/signatures"),
        ]
    };
    for dir in candidates {
        if dir.is_dir() {
            let roots = load_signatures_from_dir(&dir);
            if !roots.is_empty() {
                return roots;
            }
        }
    }
    Vec::new()
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
            let line_index = LineIndex::new(source.as_bytes());
            if !args.json {
                for d in &result.diagnostics {
                    if matches!(
                        d.severity,
                        sipha::error::Severity::Warning | sipha::error::Severity::Deprecation
                    ) {
                        eprintln!(
                            "{}",
                            d.format_with_source(source.as_bytes(), &line_index)
                        );
                    }
                }
            }
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
        ParseOutcome::IncludeError(e) => handle_parse_failure(
            ParseOutcome::IncludeError(e),
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
    let signature_roots = if args.annotate_types {
        let mut roots = Vec::new();
        if let Some(ref dir) = args.stdlib_dir {
            roots.extend(load_signatures_from_dir(dir));
        }
        if !args.signature_files.is_empty() {
            roots.extend(load_signatures_from_files(&args.signature_files));
        }
        if roots.is_empty()
            && args.stdlib_dir.is_none()
            && args.signature_files.is_empty()
        {
            roots = default_signature_roots();
        }
        if roots.is_empty() {
            None
        } else {
            Some(roots)
        }
    } else {
        None
    };

    let indent_style = if args.indent.eq_ignore_ascii_case("tabs") {
        IndentStyle::Tabs
    } else if args.indent.eq_ignore_ascii_case("spaces") {
        IndentStyle::Spaces(4)
    } else {
        let lower = args.indent.to_ascii_lowercase();
        if lower.starts_with("spaces") && args.indent.len() >= 6 {
            let suffix = args.indent[6..].trim_start_matches(|c: char| !c.is_ascii_digit());
            let n = suffix.parse().unwrap_or(4);
            IndentStyle::Spaces(n)
        } else {
            IndentStyle::Tabs
        }
    };

    let brace_style = if args.brace_style.eq_ignore_ascii_case("next-line") {
        BraceStyle::NextLine
    } else {
        BraceStyle::SameLine
    };

    let semicolon_style = if args.semicolon_style.eq_ignore_ascii_case("omit") {
        SemicolonStyle::Omit
    } else {
        SemicolonStyle::Always
    };

    FormatterOptions {
        preserve_comments: args.preserve_comments && !args.canonical,
        parenthesize_expressions: args.parenthesize_expressions,
        annotate_types: args.annotate_types,
        signature_roots,
        canonical_format: args.canonical,
        indent_style,
        brace_style,
        semicolon_style,
    }
}
