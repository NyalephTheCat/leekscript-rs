//! Semantic error codes and helpers for analysis diagnostics.
//!
//! Aligned with leekscript-java's `leekscript.common.Error` where applicable.
//!
//! ## Mapping to LeekScript Java compiler (Error enum)
//!
//! | AnalysisError variant              | Java constant                     | Code |
//! |------------------------------------|-----------------------------------|------|
//! | `BreakOutOfLoop`                   | `BREAK_OUT_OF_LOOP`              | E012 |
//! | `ContinueOutOfLoop`                | `CONTINUE_OUT_OF_LOOP`           | E013 |
//! | `IncludeOnlyInMainBlock`           | `INCLUDE_ONLY_IN_MAIN_BLOCK`     | E014 |
//! | `FunctionOnlyInMainBlock`         | `FUNCTION_ONLY_IN_MAIN_BLOCK`    | E019 |
//! | `VariableNameUnavailable`         | `VARIABLE_NAME_UNAVAILABLE`      | E021 |
//! | `GlobalOnlyInMainBlock`            | `GLOBAL_ONLY_IN_MAIN_BLOCK`      | E027 |
//! | `UnknownVariableOrFunction`        | `UNKNOWN_VARIABLE_OR_FUNCTION`   | E033 |
//! | `DuplicateClassName`              | duplicate class                  | E034 |
//! | `DuplicateFunctionName`           | duplicate function               | E035 |
//! | `WrongArity`                       | wrong argument count             | E036 |
//! | `TypeMismatch`                     | type mismatch / invalid cast     | E037 |
//! | `OptionalParamsOnlyInStandard...` | optional params in user function | E038 |

use sipha::error::SemanticDiagnostic;
use sipha::types::Span;

/// Error codes for semantic validation (mirroring leekscript-java Error enum).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum AnalysisError {
    /// Unknown variable or function name (Java: `UNKNOWN_VARIABLE_OR_FUNCTION`, 33).
    UnknownVariableOrFunction,
    /// Break outside of loop (Java: `BREAK_OUT_OF_LOOP`, 12).
    BreakOutOfLoop,
    /// Continue outside of loop (Java: `CONTINUE_OUT_OF_LOOP`, 13).
    ContinueOutOfLoop,
    /// Variable/name already declared in this scope (Java: `VARIABLE_NAME_UNAVAILABLE`, 21).
    VariableNameUnavailable,
    /// Include only allowed in main block (Java: `INCLUDE_ONLY_IN_MAIN_BLOCK`, 14).
    IncludeOnlyInMainBlock,
    /// Function only in main block (Java: `FUNCTION_ONLY_IN_MAIN_BLOCK`, 19).
    FunctionOnlyInMainBlock,
    /// Global only in main block (Java: `GLOBAL_ONLY_IN_MAIN_BLOCK`, 27).
    GlobalOnlyInMainBlock,
    /// Duplicate class name in main scope (Java: duplicate class).
    DuplicateClassName,
    /// Duplicate function name in main scope (Java: duplicate function).
    DuplicateFunctionName,
    /// Optional/default parameters only allowed in standard functions or methods, not in user-defined top-level functions.
    OptionalParamsOnlyInStandardFunctionsOrMethods,
    /// Function call argument count does not match declaration (arity).
    WrongArity,
    /// Type mismatch (e.g. assignment or argument).
    TypeMismatch,
}

impl AnalysisError {
    /// Short code for diagnostics (e.g. E033).
    #[must_use] 
    pub fn code(self) -> &'static str {
        match self {
            Self::UnknownVariableOrFunction => "E033",
            Self::BreakOutOfLoop => "E012",
            Self::ContinueOutOfLoop => "E013",
            Self::VariableNameUnavailable => "E021",
            Self::IncludeOnlyInMainBlock => "E014",
            Self::FunctionOnlyInMainBlock => "E019",
            Self::GlobalOnlyInMainBlock => "E027",
            Self::DuplicateClassName => "E034",
            Self::DuplicateFunctionName => "E035",
            Self::OptionalParamsOnlyInStandardFunctionsOrMethods => "E038",
            Self::WrongArity => "E036",
            Self::TypeMismatch => "E037",
        }
    }

    /// Human-readable message.
    #[must_use] 
    pub fn message(self) -> &'static str {
        match self {
            Self::UnknownVariableOrFunction => "unknown variable or function",
            Self::BreakOutOfLoop => "break outside of loop",
            Self::ContinueOutOfLoop => "continue outside of loop",
            Self::VariableNameUnavailable => "variable name already used in this scope",
            Self::IncludeOnlyInMainBlock => "include only allowed in main block",
            Self::FunctionOnlyInMainBlock => "function declaration only allowed in main block",
            Self::GlobalOnlyInMainBlock => "global declaration only allowed in main block",
            Self::DuplicateClassName => "duplicate class name",
            Self::DuplicateFunctionName => "duplicate function name",
            Self::OptionalParamsOnlyInStandardFunctionsOrMethods => {
                "optional/default parameters only allowed in standard functions or methods, not in user-defined functions"
            }
            Self::WrongArity => "wrong number of arguments for function call",
            Self::TypeMismatch => "type mismatch",
        }
    }

    /// Build a semantic diagnostic for this error at the given span.
    #[must_use] 
    pub fn at(self, span: Span) -> SemanticDiagnostic {
        SemanticDiagnostic::error(span, self.message()).with_code(self.code())
    }
}

/// Build a wrong-arity diagnostic with expected vs actual counts.
pub fn wrong_arity_at(span: Span, expected: usize, actual: usize) -> SemanticDiagnostic {
    let message = format!(
        "wrong number of arguments (expected {expected}, got {actual})"
    );
    SemanticDiagnostic::error(span, message).with_code(AnalysisError::WrongArity.code())
}

/// Build a type mismatch diagnostic (expected vs got).
pub fn type_mismatch_at(span: Span, expected: &str, got: &str) -> SemanticDiagnostic {
    let message = format!("type mismatch: expected {expected}, got {got}");
    SemanticDiagnostic::error(span, message).with_code(AnalysisError::TypeMismatch.code())
}

/// Build an invalid cast diagnostic.
pub fn invalid_cast_at(span: Span, from: &str, to: &str) -> SemanticDiagnostic {
    let message = format!("invalid cast from {from} to {to}");
    SemanticDiagnostic::error(span, message).with_code(AnalysisError::TypeMismatch.code())
}
