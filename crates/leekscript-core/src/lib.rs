//! LeekScript parser core: syntax, types, grammar, parser, preprocess, doc comments.

pub mod doc_comment;
pub mod grammar;
pub mod parser;
pub mod preprocess;
pub mod syntax;
pub mod types;

pub use doc_comment::DocComment;
pub use grammar::{build_grammar, build_signature_grammar};
pub use parser::{
    parse, parse_error_to_diagnostics, parse_error_to_miette, parse_expression, parse_recovering,
    parse_recovering_multi, parse_signatures, parse_to_doc, parse_tokens, program_literals,
    reparse, reparse_or_parse, TextEdit,
};
pub use preprocess::{
    all_files, build_include_tree, collect_include_path_ranges, IncludeError, IncludeTree,
};
pub use syntax::{is_valid_identifier, Kind, KEYWORDS};
pub use types::{CastType, Type};
