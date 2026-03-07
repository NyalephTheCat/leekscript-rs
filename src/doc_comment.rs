//! Doxygen-style comment parsing and association with declarations.
//!
//! Parses comment content (from trivia) into structured fields (brief, description,
//! @param, @return, @deprecated, etc.) and builds a map from declaration span to docs.

use std::collections::HashMap;

use sipha::red::{SyntaxElement, SyntaxNode, SyntaxToken};
use sipha::types::{FromSyntaxKind, IntoSyntaxKind};

use crate::syntax::Kind;

/// Structured documentation parsed from a Doxygen-style comment.
#[derive(Clone, Debug, Default)]
pub struct DocComment {
    /// Short summary (e.g. from @brief or first line).
    pub brief: Option<String>,
    /// Main description body.
    pub description: String,
    /// Parameter name -> description.
    pub params: Vec<(String, String)>,
    /// Return value description.
    pub returns: Option<String>,
    /// Deprecation message if @deprecated is present.
    pub deprecated: Option<String>,
    /// @see references.
    pub see: Vec<String>,
    /// @since version or similar.
    pub since: Option<String>,
}

/// Strip comment markers and normalize line prefixes to get raw content.
fn strip_comment_markers(raw: &str, is_block: bool) -> String {
    let mut out = String::new();
    let lines: Vec<&str> = raw.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if is_block {
            // Block: strip /* from first line, */ from last, and leading * from each line
            let content = if i == 0 {
                trimmed.strip_prefix("/*").unwrap_or(trimmed).trim_start()
            } else {
                trimmed
            };
            let content = if i == lines.len() - 1 {
                content.strip_suffix("*/").unwrap_or(content).trim_end()
            } else {
                content
            };
            let content = content
                .strip_prefix('*')
                .map(|s| s.trim_start())
                .unwrap_or(content)
                .trim();
            if !content.is_empty() || !out.is_empty() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(content);
            }
        } else {
            // Line comment: strip //, ///, //!
            let content = trimmed
                .trim_start_matches('/')
                .trim_start_matches('!')
                .trim_start_matches(' ')
                .trim_start();
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(content);
        }
    }
    out
}

/// Parse a single raw comment string (already stripped of delimiters) for block vs line.
fn parse_comment_content(content: &str, is_block: bool) -> DocComment {
    let normalized = strip_comment_markers(content, is_block);
    parse_normalized_content(&normalized)
}

/// Parse normalized (marker-stripped) content into DocComment.
fn parse_normalized_content(s: &str) -> DocComment {
    let mut doc = DocComment::default();
    let mut description_lines: Vec<&str> = Vec::new();
    let mut in_description = true;

    for line in s.lines() {
        let line = line.trim();

        if line.is_empty() {
            if in_description && !description_lines.is_empty() {
                in_description = false; // blank line ends description paragraph
            }
            continue;
        }

        let (tag, after) = if let Some(after) = line.strip_prefix("@brief").or_else(|| line.strip_prefix("\\brief")) {
            ("brief", after.trim())
        } else if let Some(after) = line.strip_prefix("@param").or_else(|| line.strip_prefix("\\param")) {
            ("param", after.trim())
        } else if let Some(after) = line.strip_prefix("@return").or_else(|| line.strip_prefix("\\return")) {
            ("return", after.trim())
        } else if let Some(after) = line.strip_prefix("@returns").or_else(|| line.strip_prefix("\\returns")) {
            ("return", after.trim())
        } else if let Some(after) = line.strip_prefix("@deprecated").or_else(|| line.strip_prefix("\\deprecated")) {
            ("deprecated", after.trim())
        } else if let Some(after) = line.strip_prefix("@see").or_else(|| line.strip_prefix("\\see")) {
            ("see", after.trim())
        } else if let Some(after) = line.strip_prefix("@since").or_else(|| line.strip_prefix("\\since")) {
            ("since", after.trim())
        } else {
            if in_description {
                description_lines.push(line);
            }
            continue;
        };

        in_description = false;

        match tag {
            "brief" => doc.brief = Some(after.to_string()),
            "return" => doc.returns = Some(after.to_string()),
            "deprecated" => doc.deprecated = Some(after.to_string()),
            "since" => doc.since = Some(after.to_string()),
            "param" => {
                // First word is param name, rest is description
                let mut it = after.splitn(2, char::is_whitespace);
                let name = it.next().unwrap_or("").trim().to_string();
                let desc = it.next().unwrap_or("").trim().to_string();
                if !name.is_empty() {
                    doc.params.push((name, desc));
                }
            }
            "see" => doc.see.push(after.to_string()),
            _ => {}
        }
    }

    doc.description = description_lines.join("\n").trim().to_string();
    if doc.brief.is_none() && !doc.description.is_empty() {
        let first_para = doc.description.split("\n\n").next().unwrap_or(&doc.description);
        doc.brief = Some(first_para.replace('\n', " ").trim().to_string());
    }
    doc
}

/// Parse one or more raw comment strings (e.g. from multiple preceding trivia tokens).
pub fn parse_doc_comment(parts: &[String]) -> Option<DocComment> {
    if parts.is_empty() {
        return None;
    }
    let combined = parts.join("\n");
    let trimmed = combined.trim();
    if trimmed.is_empty() {
        return None;
    }
    let is_block = trimmed.starts_with("/*");
    let doc = parse_comment_content(trimmed, is_block);
    if doc.brief.is_none() && doc.description.is_empty() && doc.params.is_empty()
        && doc.returns.is_none() && doc.deprecated.is_none() && doc.see.is_empty() && doc.since.is_none()
    {
        return None;
    }
    Some(doc)
}

/// Declaration node kinds we attach doc comments to.
const DOC_DECL_KINDS: [Kind; 6] = [
    Kind::NodeClassDecl,
    Kind::NodeFunctionDecl,
    Kind::NodeVarDecl,
    Kind::NodeConstructorDecl,
    Kind::NodeClassField,
    Kind::NodeInclude,
];

fn is_comment_trivia(kind: Kind) -> bool {
    kind == Kind::TriviaLineComment || kind == Kind::TriviaBlockComment
}

/// Collect contiguous comment trivia tokens that immediately precede `node`.
/// Uses the node's own [`leading_trivia`](sipha::red::SyntaxNode::leading_trivia) when the tree
/// builder has attached preceding trivia to the node (see sipha green tree). Otherwise walks up
/// to an ancestor that has preceding sibling comment tokens.
fn preceding_comment_tokens(node: &SyntaxNode, root: &SyntaxNode) -> Option<Vec<SyntaxToken>> {
    let leading = node.leading_trivia();
    let comments: Vec<SyntaxToken> = leading
        .into_iter()
        .filter(|t| {
            Kind::from_syntax_kind(t.kind()).map_or(false, is_comment_trivia)
        })
        .collect();
    if !comments.is_empty() {
        return Some(comments);
    }
    let mut current = node.clone();
    loop {
        let parent = current.ancestors(root).into_iter().next()?;
        let children: Vec<SyntaxElement> = parent.children().collect();
        let pos = children.iter().position(|e| {
            e.as_node().map_or(false, |n| {
                n.offset() == current.offset() && n.kind() == current.kind()
            })
        })?;
        let mut comments = Vec::new();
        for i in (0..pos).rev() {
            let el = &children[i];
            if let Some(tok) = el.as_token() {
                if let Some(k) = Kind::from_syntax_kind(tok.kind()) {
                    if is_comment_trivia(k) {
                        comments.push(tok.clone());
                    } else if k != Kind::TriviaWs {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        comments.reverse();
        if !comments.is_empty() {
            return Some(comments);
        }
        if parent.offset() == root.offset() && parent.kind() == root.kind() {
            return None;
        }
        current = parent;
    }
}

/// Build a map from declaration (start_byte, end_byte) to parsed documentation.
pub fn build_doc_map(root: &SyntaxNode) -> HashMap<(u32, u32), DocComment> {
    let mut map = HashMap::new();
    for kind in DOC_DECL_KINDS {
        for node in root.find_all_nodes(kind.into_syntax_kind()) {
            let Some(tokens) = preceding_comment_tokens(&node, root) else {
                continue;
            };
            if tokens.is_empty() {
                continue;
            }
            let parts: Vec<String> = tokens.iter().map(|t| t.text().to_string()).collect();
            if let Some(doc_comment) = parse_doc_comment(&parts) {
                let span = node.text_range();
                map.insert((span.start, span.end), doc_comment);
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use sipha::types::IntoSyntaxKind;

    use crate::parse;
    use crate::syntax::Kind;

    use super::{build_doc_map, parse_comment_content, preceding_comment_tokens, DocComment};

    /// Asserts that a Doxygen block comment above a class is attached to that class in the doc map.
    #[test]
    fn test_doc_comment_attached_to_class() {
        let source = r#"
/**
 * @brief Represents a position or object in the game world by cell ID and coordinates.
 *
 * The Cell class allows you to create a cell either using a unique ID or X/Y coordinates.
 */
class Cell {
    integer id;
}
"#;
        let root = parse(source).ok().flatten().expect("parse should succeed");
        let doc_map = build_doc_map(&root);

        let class_nodes: Vec<_> = root.find_all_nodes(Kind::NodeClassDecl.into_syntax_kind());
        let class_node = class_nodes
            .into_iter()
            .next()
            .expect("there should be one class decl");
        let span = class_node.text_range();
        let key = (span.start, span.end);

        let doc = doc_map
            .get(&key)
            .expect("doc_map should contain an entry for the class declaration span");
        assert_eq!(
            doc.brief.as_deref(),
            Some("Represents a position or object in the game world by cell ID and coordinates."),
            "Doxygen @brief should be attached to the class"
        );
        assert!(
            doc.brief.is_some() || !doc.description.is_empty(),
            "doc should have brief or description"
        );
    }

    #[test]
    fn test_parse_block_brief_param_return() {
        let s = r#"
 * Brief line.
 *
 * More description here.
 * @param x The first argument.
 * @param y The second.
 * @return The result.
"#;
        let doc: DocComment = parse_comment_content(&format!("/*{}*/", s.trim()), true);
        assert_eq!(doc.brief.as_deref(), Some("Brief line."));
        assert!(doc.description.contains("Brief line."));
        assert_eq!(doc.params.len(), 2);
        assert_eq!(doc.params[0].0, "x");
        assert_eq!(doc.params[0].1, "The first argument.");
        assert_eq!(doc.returns.as_deref(), Some("The result."));
    }

    #[test]
    fn test_parse_line_comment() {
        let s = "/// Brief.\n/// @param a desc";
        let doc = parse_comment_content(s, false);
        assert_eq!(doc.brief.as_deref(), Some("Brief."));
        assert_eq!(doc.params.len(), 1);
        assert_eq!(doc.params[0].0, "a");
    }

    /// Asserts that a Doxygen block comment immediately before a top-level function
    /// is attached to that function in the doc map.
    #[test]
    fn test_doc_comment_attached_to_function() {
        let source = r#"
/**
 * @brief Computes the sum of two numbers.
 * @param a First operand.
 * @param b Second operand.
 * @return The sum.
 */
function add(a, b) -> integer {
    return a + b;
}
"#;
        let root = parse(source).ok().flatten().expect("parse should succeed");
        let doc_map = build_doc_map(&root);

        let func_nodes: Vec<_> = root.find_all_nodes(Kind::NodeFunctionDecl.into_syntax_kind());
        let func_node = func_nodes
            .into_iter()
            .next()
            .expect("there should be one function decl");
        let span = func_node.text_range();
        let key = (span.start, span.end);

        let doc = doc_map
            .get(&key)
            .expect("doc_map should contain an entry for the function declaration span; ensure preceding comments are attached");
        assert_eq!(
            doc.brief.as_deref(),
            Some("Computes the sum of two numbers."),
            "Doxygen @brief should be attached to the function"
        );
        assert_eq!(doc.params.len(), 2, "expected @param a and @param b");
        assert_eq!(doc.params[0].0, "a");
        assert_eq!(doc.params[1].0, "b");
        assert_eq!(doc.returns.as_deref(), Some("The sum."));
    }

    // --- Trivia attachment tests ---

    /// When a declaration has no preceding comment, it has no doc and `preceding_comment_tokens` returns None.
    #[test]
    fn test_trivia_not_attached_when_no_comment() {
        let source = r#"
function no_doc() {
    return 0;
}
"#;
        let root = parse(source).ok().flatten().expect("parse should succeed");
        let doc_map = build_doc_map(&root);

        let func_nodes: Vec<_> = root.find_all_nodes(Kind::NodeFunctionDecl.into_syntax_kind());
        let func_node = func_nodes.into_iter().next().expect("one function decl");
        let span = func_node.text_range();
        let key = (span.start, span.end);

        assert!(
            doc_map.get(&key).is_none(),
            "decl with no preceding comment should not be in doc_map"
        );
        let tokens = preceding_comment_tokens(&func_node, &root);
        assert!(
            tokens.is_none() || tokens.as_ref().map(|t| t.is_empty()).unwrap_or(false),
            "preceding_comment_tokens should return None or empty for decl with no comment"
        );
    }

    /// Comment is attached only to the immediately following declaration; the next decl has no doc.
    #[test]
    fn test_trivia_attached_only_to_immediately_following_decl() {
        let source = r#"
/**
 * @brief Only for first.
 */
function first() { return 1; }

function second() { return 2; }
"#;
        let root = parse(source).ok().flatten().expect("parse should succeed");
        let doc_map = build_doc_map(&root);

        let func_nodes: Vec<_> = root.find_all_nodes(Kind::NodeFunctionDecl.into_syntax_kind());
        let (first, second) = {
            let mut it = func_nodes.into_iter();
            let a = it.next().expect("first");
            let b = it.next().expect("second");
            (a, b)
        };

        let key_first = (first.text_range().start, first.text_range().end);
        let key_second = (second.text_range().start, second.text_range().end);

        let doc_first = doc_map.get(&key_first).expect("first function should have doc");
        assert_eq!(doc_first.brief.as_deref(), Some("Only for first."));

        assert!(
            doc_map.get(&key_second).is_none(),
            "second function should not get the comment; trivia attached only to immediately following decl"
        );
        let tokens_second = preceding_comment_tokens(&second, &root);
        assert!(
            tokens_second.is_none() || tokens_second.as_ref().map(|t| t.is_empty()).unwrap_or(true),
            "preceding_comment_tokens(second) should be None or empty"
        );
    }

    /// Parser attaches preceding comment to the node (leading trivia or preceding sibling); we find it via preceding_comment_tokens.
    #[test]
    fn test_trivia_leading_comment_found_for_decl() {
        let source = r#"
/// Doc for foo.
function foo() { return 0; }
"#;
        let root = parse(source).ok().flatten().expect("parse should succeed");
        let func_nodes: Vec<_> = root.find_all_nodes(Kind::NodeFunctionDecl.into_syntax_kind());
        let func_node = func_nodes.into_iter().next().expect("one function decl");

        let tokens = preceding_comment_tokens(&func_node, &root).expect("should find preceding comment tokens");
        assert!(!tokens.is_empty(), "should have at least one comment token");
        let text: String = tokens.iter().map(|t| t.text().to_string()).collect();
        assert!(
            text.contains("Doc for foo"),
            "trivia attached to decl should contain the comment text; got: {:?}",
            text
        );

        let doc_map = build_doc_map(&root);
        let span = func_node.text_range();
        let key = (span.start, span.end);
        let doc = doc_map.get(&key).expect("doc_map should have entry for foo");
        assert_eq!(doc.brief.as_deref(), Some("Doc for foo."));
    }

    /// Multiple consecutive line comments are all collected and merged into one doc for the following decl.
    #[test]
    fn test_trivia_multiple_line_comments_attached_to_same_decl() {
        let source = r#"
/// First line.
/// Second line.
/// @param x desc
function f(x) { return x; }
"#;
        let root = parse(source).ok().flatten().expect("parse should succeed");
        let func_nodes: Vec<_> = root.find_all_nodes(Kind::NodeFunctionDecl.into_syntax_kind());
        let func_node = func_nodes.into_iter().next().expect("one function decl");

        let tokens = preceding_comment_tokens(&func_node, &root).expect("should find preceding comment tokens");
        assert_eq!(tokens.len(), 3, "should have three /// comment tokens");

        let doc_map = build_doc_map(&root);
        let span = func_node.text_range();
        let key = (span.start, span.end);
        let doc = doc_map.get(&key).expect("doc_map should have entry for f");
        assert!(doc.brief.as_deref().map(|b| b.contains("First line")).unwrap_or(false));
        assert!(doc.description.contains("Second line."));
        assert_eq!(doc.params.len(), 1);
        assert_eq!(doc.params[0].0, "x");
        assert_eq!(doc.params[0].1, "desc");
    }
}
