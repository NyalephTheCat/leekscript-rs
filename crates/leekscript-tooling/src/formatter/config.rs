//! Load formatter options from config files (.leekfmt.toml or leekscript.toml [format] section).

use std::path::Path;

use super::options::{BraceStyle, FormatterOptions, IndentStyle, SemicolonStyle};

/// Try to load formatter options from a directory: first `.leekfmt.toml`, then `leekscript.toml` with `[format]` section.
/// Returns `None` if no file is found or parsing fails; caller falls back to `FormatterOptions::default()`.
#[must_use]
pub fn load_formatter_options_from_dir(dir: &Path) -> Option<FormatterOptions> {
    let leekfmt = dir.join(".leekfmt.toml");
    if leekfmt.exists() {
        return load_from_file(&leekfmt);
    }
    let manifest = dir.join("leekscript.toml");
    if manifest.exists() {
        if let Ok(content) = std::fs::read_to_string(&manifest) {
            if let Ok(t) = content.parse::<toml::Table>() {
                if let Some(format_table) = t.get("format").and_then(|v| v.as_table()) {
                    return parse_format_table(format_table);
                }
            }
        }
    }
    None
}

/// Load formatter options from a single config file (e.g. `.leekfmt.toml`) with a top-level `[format]` section.
#[must_use]
pub fn load_formatter_options_from_file(path: &Path) -> Option<FormatterOptions> {
    load_from_file(path)
}

fn load_from_file(path: &Path) -> Option<FormatterOptions> {
    let content = std::fs::read_to_string(path).ok()?;
    let t = content.parse::<toml::Table>().ok()?;
    let format_table = t.get("format").and_then(|v| v.as_table())?;
    parse_format_table(format_table)
}

fn parse_format_table(t: &toml::map::Map<String, toml::Value>) -> Option<FormatterOptions> {
    let indent_style = t
        .get("indent")
        .and_then(|v| v.as_str())
        .map_or(IndentStyle::Tabs, parse_indent_style);

    let brace_style = t
        .get("brace_style")
        .and_then(|v| v.as_str())
        .map_or(BraceStyle::SameLine, parse_brace_style);

    let semicolon_style = t
        .get("semicolon_style")
        .and_then(|v| v.as_str())
        .map_or(SemicolonStyle::Always, parse_semicolon_style);

    let canonical_format = t
        .get("canonical")
        .and_then(toml::Value::as_bool)
        .unwrap_or(false);

    Some(FormatterOptions {
        preserve_comments: t
            .get("preserve_comments")
            .and_then(toml::Value::as_bool)
            .unwrap_or(true),
        parenthesize_expressions: t
            .get("parenthesize_expressions")
            .and_then(toml::Value::as_bool)
            .unwrap_or(false),
        annotate_types: false,
        signature_roots: None,
        canonical_format,
        indent_style,
        brace_style,
        semicolon_style,
    })
}

fn parse_indent_style(s: &str) -> IndentStyle {
    let s = s.trim().to_lowercase();
    if s == "tabs" {
        return IndentStyle::Tabs;
    }
    if s.starts_with("spaces") {
        let n = s
            .trim_start_matches("spaces")
            .trim()
            .parse::<u32>()
            .unwrap_or(4);
        return IndentStyle::Spaces(n);
    }
    IndentStyle::Tabs
}

fn parse_brace_style(s: &str) -> BraceStyle {
    match s.trim().to_lowercase().as_str() {
        "next-line" | "nextline" => BraceStyle::NextLine,
        _ => BraceStyle::SameLine,
    }
}

fn parse_semicolon_style(s: &str) -> SemicolonStyle {
    match s.trim().to_lowercase().as_str() {
        "omit" => SemicolonStyle::Omit,
        _ => SemicolonStyle::Always,
    }
}
