//! # leekscript-rs
//!
//! A LeekScript parser implemented with [sipha](https://docs.rs/sipha).

pub mod grammar;
pub mod parser;
pub mod syntax;
pub mod tree_display;
pub mod types;

pub use grammar::build_grammar;
pub use parser::{parse, parse_expression, parse_tokens};
pub use tree_display::{format_syntax_tree, print_syntax_tree, TreeDisplayOptions};
pub use types::{CastType, Type};
