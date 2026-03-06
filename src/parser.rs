//! Parser entry points: build grammar once and parse source.

use std::sync::OnceLock;

use sipha::engine::{Engine, ParseError, ParseOutput};
use sipha::insn::ParseGraph;
use sipha::red::SyntaxNode;

use crate::grammar::{build_expression_grammar, build_grammar, build_program_grammar};

type BuiltAndGraph = (sipha::builder::BuiltGraph, ParseGraph);

fn cache_grammar<F>(storage: &'static OnceLock<BuiltAndGraph>, build: F) -> &'static BuiltAndGraph
where
    F: FnOnce() -> sipha::builder::BuiltGraph,
{
    storage.get_or_init(|| {
        let built = build();
        let graph = built.as_graph();
        (built, graph)
    })
}

/// Cached (BuiltGraph, ParseGraph) for token stream parsing (Phase 1).
fn token_stream_built_and_graph() -> &'static BuiltAndGraph {
    static STORAGE: OnceLock<BuiltAndGraph> = OnceLock::new();
    cache_grammar(&STORAGE, build_grammar)
}

/// Cached (BuiltGraph, ParseGraph) for expression-only parsing (Phase 2).
fn expression_built_and_graph() -> &'static BuiltAndGraph {
    static STORAGE: OnceLock<BuiltAndGraph> = OnceLock::new();
    cache_grammar(&STORAGE, build_expression_grammar)
}

/// Cached (BuiltGraph, ParseGraph) for program parsing (Phase 3/4).
fn program_built_and_graph() -> &'static BuiltAndGraph {
    static STORAGE: OnceLock<BuiltAndGraph> = OnceLock::new();
    cache_grammar(&STORAGE, build_program_grammar)
}

fn parse_to_syntax_root(
    source: &str,
    get_graph: fn() -> &'static BuiltAndGraph,
) -> Result<Option<SyntaxNode>, ParseError> {
    let (_, graph) = get_graph();
    let mut engine = Engine::new();
    let out = engine.parse(graph, source.as_bytes())?;
    Ok(out.syntax_root(source.as_bytes()))
}

/// Parse source as a token stream (Phase 1 lexer).
///
/// Returns the sipha parse output; use `.syntax_root(source.as_bytes())` to get
/// the root syntax node, or `.tree_events` for the raw event list.
pub fn parse_tokens(source: &str) -> Result<ParseOutput, ParseError> {
    let (_, graph) = token_stream_built_and_graph();
    let mut engine = Engine::new();
    engine.parse(graph, source.as_bytes())
}

/// Parse source as a program (Phase 3/4: list of statements).
///
/// Returns the program root node (NODE_ROOT with statement children).
/// For token stream only, use [`parse_tokens`].
pub fn parse(source: &str) -> Result<Option<SyntaxNode>, ParseError> {
    parse_to_syntax_root(source, program_built_and_graph)
}

/// Parse source as a single expression (Phase 2).
///
/// Uses a dedicated expression grammar (primary: number, string, identifier, parenthesized expr).
pub fn parse_expression(source: &str) -> Result<Option<SyntaxNode>, ParseError> {
    parse_to_syntax_root(source, expression_built_and_graph)
}
