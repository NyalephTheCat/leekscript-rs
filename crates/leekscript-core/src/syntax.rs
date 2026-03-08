//! Syntax and token kinds for the `LeekScript` grammar.
//!
//! Uses an enum with [`sipha::SyntaxKinds`] so discriminants are 0, 1, 2, … automatically.

use sipha::types::FromSyntaxKind;
use sipha::SyntaxKinds;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, SyntaxKinds)]
#[repr(u16)]
pub enum Kind {
    // Trivia (0–2)
    TriviaWs,
    TriviaLineComment,
    TriviaBlockComment,
    // Tokens: literals and identifiers (3–5)
    TokNumber,
    TokString,
    TokIdent,
    // Keywords (6–48)
    KwAbstract,
    KwAnd,
    KwAs,
    KwBreak,
    KwClass,
    KwConst,
    KwContinue,
    KwDo,
    KwElse,
    KwFalse,
    KwFor,
    KwFunction,
    KwGlobal,
    KwIf,
    KwIn,
    KwInclude,
    KwLet,
    KwNew,
    KwNot,
    KwNull,
    KwOr,
    KwReturn,
    KwTrue,
    KwVar,
    KwWhile,
    KwXor,
    KwFinal,
    KwConstructor,
    KwExtends,
    KwStatic,
    KwPublic,
    KwPrivate,
    KwProtected,
    KwThis,
    KwSuper,
    KwInstanceof,
    KwTry,
    KwCatch,
    KwSwitch,
    KwCase,
    KwDefault,
    KwThrow,
    KwReserved,
    // Tokens: operators and punctuation (49–63)
    TokOp,
    TokArrow,
    TokDotDot,
    TokDot,
    TokColon,
    TokComma,
    TokSemi,
    TokParenL,
    TokParenR,
    TokBracketL,
    TokBracketR,
    TokBraceL,
    TokBraceR,
    TokLemnisate,
    TokPi,
    // End of input (64)
    TokEof,
    // Nodes (65+)
    NodeRoot,
    NodeTokenStream,
    NodeExpr,
    NodePrimaryExpr,
    NodeBinaryExpr,
    NodeBinaryLevel, // Wraps full "left op right" for one precedence level (add, mul, etc.)
    NodeUnaryExpr,
    NodeCallExpr,
    NodeMemberExpr,
    NodeIndexExpr,
    NodeArray,
    NodeMap,
    NodeMapPair,
    NodeObject,
    NodeObjectPair,
    NodeSet,
    NodeStmt,
    NodeBlock,
    NodeVarDecl,
    NodeIfStmt,
    NodeWhileStmt,
    NodeForStmt,
    NodeForInStmt,
    NodeDoWhileStmt,
    NodeReturnStmt,
    NodeBreakStmt,
    NodeContinueStmt,
    NodeExprStmt,
    NodeFunctionDecl,
    NodeClassDecl,
    NodeInclude,
    NodeConstructorDecl,
    NodeInterval,
    NodeClassField,
    NodeAsCast,
    NodeAnonFn,
    NodeTypeAnnot,
    NodeParam,
    NodeTypeExpr,
    NodeTypeParams,
    // Signature file nodes (for stdlib / API signature files, not LeekScript source)
    NodeSigFile,
    NodeSigFunction,
    NodeSigClass,
    NodeSigMethod,
    NodeSigConstructor,
    NodeSigField,
    NodeSigGlobal,
    NodeSigParam,
    /// Doxygen-style doc block after a function/global in .sig (/// lines or /** */ block).
    NodeSigDocBlock,
    /// Doc line token: `///` plus rest of line (for NodeSigDocBlock).
    TokSigDocLine,
    /// Block doc token: `/**` ... `*/` (for NodeSigDocBlock).
    TokSigDocBlock,
}

/// sipha uses this kind for a wrapper root when the grammar produces a single root node.
pub const SYNTHETIC_ROOT: sipha::types::SyntaxKind = u16::MAX;

/// LeekScript keywords (for completion and tooling). Sorted for display.
pub const KEYWORDS: &[&str] = &[
    "abstract",
    "and",
    "as",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "constructor",
    "continue",
    "default",
    "do",
    "else",
    "extends",
    "false",
    "final",
    "for",
    "function",
    "global",
    "if",
    "in",
    "include",
    "instanceof",
    "let",
    "new",
    "not",
    "null",
    "or",
    "private",
    "protected",
    "public",
    "reserved",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "var",
    "while",
    "xor",
];

/// Returns true if `name` is a valid LeekScript identifier (non-empty, starts with letter or
/// underscore, rest alphanumeric or underscore). Used e.g. for rename validation in LSP.
#[must_use]
pub fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::is_valid_identifier;

    #[test]
    fn valid_identifiers() {
        assert!(is_valid_identifier("x"));
        assert!(is_valid_identifier("_private"));
        assert!(is_valid_identifier("foo_bar"));
        assert!(is_valid_identifier("Cell"));
    }

    #[test]
    fn invalid_identifiers() {
        assert!(!is_valid_identifier(""));
        assert!(!is_valid_identifier("123"));
        assert!(!is_valid_identifier("bad name"));
        assert!(!is_valid_identifier("x-y"));
    }
}

/// Field id for the "rhs" named child (right-hand side of binary expressions in the expression grammar).
/// Use with [`sipha::red::SyntaxNode::field_by_id`] on a `NodeBinaryExpr` to get the right operand.
pub const FIELD_RHS: sipha::types::FieldId = 0;

/// Human-readable name for a syntax kind (for diagnostics and debugging).
pub fn kind_name(kind: sipha::types::SyntaxKind) -> &'static str {
    if kind == SYNTHETIC_ROOT {
        return "ROOT";
    }
    Kind::from_syntax_kind(kind).map_or("?", kind_name_enum)
}

fn kind_name_enum(k: Kind) -> &'static str {
    match k {
        Kind::TokNumber => "NUMBER",
        Kind::TokString => "STRING",
        Kind::TokIdent => "IDENT",
        Kind::TokOp => "OP",
        Kind::TokParenL => "(",
        Kind::TokParenR => ")",
        Kind::TokDotDot => "..",
        Kind::NodeTokenStream => "TOKEN_STREAM",
        Kind::NodeRoot => "ROOT",
        Kind::NodeExpr => "EXPR",
        Kind::NodeExprStmt => "EXPR_STMT",
        Kind::NodeVarDecl => "VAR_DECL",
        Kind::NodeIfStmt => "IF_STMT",
        Kind::NodeWhileStmt => "WHILE_STMT",
        Kind::NodeBlock => "BLOCK",
        Kind::NodeReturnStmt => "RETURN_STMT",
        Kind::NodeForStmt => "FOR_STMT",
        Kind::NodeForInStmt => "FOR_IN_STMT",
        Kind::NodeDoWhileStmt => "DO_WHILE_STMT",
        Kind::NodeFunctionDecl => "FUNCTION_DECL",
        Kind::NodeClassDecl => "CLASS_DECL",
        Kind::NodeConstructorDecl => "CONSTRUCTOR_DECL",
        Kind::NodeClassField => "CLASS_FIELD",
        Kind::NodeInterval => "INTERVAL",
        Kind::NodeInclude => "INCLUDE",
        Kind::NodeArray => "ARRAY",
        Kind::NodeMap => "MAP",
        Kind::NodeMapPair => "MAP_PAIR",
        Kind::NodeObject => "OBJECT",
        Kind::NodeObjectPair => "OBJECT_PAIR",
        Kind::NodeSet => "SET",
        Kind::NodeTypeExpr => "TYPE_EXPR",
        Kind::NodeTypeParams => "TYPE_PARAMS",
        Kind::NodeAsCast => "AS_CAST",
        Kind::NodeSigFile => "SIG_FILE",
        Kind::NodeSigFunction => "SIG_FUNCTION",
        Kind::NodeSigClass => "SIG_CLASS",
        Kind::NodeSigMethod => "SIG_METHOD",
        Kind::NodeSigConstructor => "SIG_CONSTRUCTOR",
        Kind::NodeSigField => "SIG_FIELD",
        Kind::NodeSigGlobal => "SIG_GLOBAL",
        Kind::NodeSigParam => "SIG_PARAM",
        Kind::NodeSigDocBlock => "SIG_DOC_BLOCK",
        Kind::TokSigDocLine => "SIG_DOC_LINE",
        Kind::TokSigDocBlock => "SIG_DOC_BLOCK_TOKEN",
        _ => "?",
    }
}
