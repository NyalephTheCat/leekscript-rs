//! `LeekScript` type system.
//!
//! Mirrors the type hierarchy from leekscript-java (leekscript.common.Type and subclasses)
//! so that tooling can support the same features: primitives, generics, compound types, etc.

use std::fmt;

/// Primitive and built-in types matching leekscript-java's Type constants.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// Error type (invalid type)
    Error,
    /// Warning type (e.g. unknown member on any)
    Warning,
    /// `void` – no value
    Void,
    /// `any` – top type
    Any,
    /// `null`
    Null,
    /// `boolean`
    Bool,
    /// `integer`
    Int,
    /// `real`
    Real,
    /// `string`
    String,
    /// `Object`
    Object,
    /// `Class` or `Class<ClassName>`
    Class(Option<String>),
    /// `Array` or `Array<T>`
    Array(Box<Type>),
    /// `Map` or `Map<K, V>`
    Map(Box<Type>, Box<Type>),
    /// `Set` or `Set<T>`
    Set(Box<Type>),
    /// `Interval` or `Interval<T>`
    Interval(Box<Type>),
    /// Function type: `(T1, T2, ...) => R`
    Function {
        args: Vec<Type>,
        return_type: Box<Type>,
    },
    /// Union: `T1 | T2 | ...`
    Compound(Vec<Type>),
}

impl Type {
    /// Predefined type constants matching leekscript-java.
    #[must_use] 
    pub const fn error() -> Self {
        Type::Error
    }
    #[must_use] 
    pub const fn warning() -> Self {
        Type::Warning
    }
    #[must_use] 
    pub const fn void() -> Self {
        Type::Void
    }
    #[must_use] 
    pub const fn any() -> Self {
        Type::Any
    }
    #[must_use] 
    pub const fn null() -> Self {
        Type::Null
    }
    #[must_use] 
    pub const fn bool() -> Self {
        Type::Bool
    }
    #[must_use] 
    pub const fn int() -> Self {
        Type::Int
    }
    #[must_use] 
    pub const fn real() -> Self {
        Type::Real
    }
    #[must_use] 
    pub const fn string() -> Self {
        Type::String
    }
    #[must_use] 
    pub const fn object() -> Self {
        Type::Object
    }
    #[must_use] 
    pub fn class(name: Option<String>) -> Self {
        Type::Class(name)
    }
    #[must_use] 
    pub fn array(element: Type) -> Self {
        Type::Array(Box::new(element))
    }
    #[must_use] 
    pub fn map(key: Type, value: Type) -> Self {
        Type::Map(Box::new(key), Box::new(value))
    }
    #[must_use] 
    pub fn set(element: Type) -> Self {
        Type::Set(Box::new(element))
    }
    #[must_use] 
    pub fn interval(element: Type) -> Self {
        Type::Interval(Box::new(element))
    }
    #[must_use] 
    pub fn function(args: Vec<Type>, return_type: Type) -> Self {
        Type::Function {
            args,
            return_type: Box::new(return_type),
        }
    }
    #[must_use] 
    pub fn compound(types: Vec<Type>) -> Self {
        Type::Compound(types)
    }

    /// Build compound from two types (e.g. T1 | T2).
    #[must_use] 
    pub fn compound2(a: Type, b: Type) -> Self {
        Type::Compound(vec![a, b])
    }

    /// Name/code as in source (`getCode()` in Java). Uses Display for full representation.
    #[must_use] 
    pub fn code(&self) -> String {
        self.to_string()
    }

    #[must_use] 
    pub fn is_primitive_number(&self) -> bool {
        matches!(self, Type::Int | Type::Real)
    }
    #[must_use] 
    pub fn is_number(&self) -> bool {
        matches!(self, Type::Int | Type::Real)
    }
    #[must_use] 
    pub fn is_array(&self) -> bool {
        matches!(self, Type::Array(_))
    }
    #[must_use] 
    pub fn is_map(&self) -> bool {
        matches!(self, Type::Map(_, _))
    }
    #[must_use] 
    pub fn can_be_null(&self) -> bool {
        matches!(self, Type::Any | Type::Null)
    }
    #[must_use] 
    pub fn is_primitive(&self) -> bool {
        matches!(self, Type::Int | Type::Bool | Type::Real)
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Error => write!(f, "error"),
            Type::Warning => write!(f, "warning"),
            Type::Void => write!(f, "void"),
            Type::Any => write!(f, "any"),
            Type::Null => write!(f, "null"),
            Type::Bool => write!(f, "boolean"),
            Type::Int => write!(f, "integer"),
            Type::Real => write!(f, "real"),
            Type::String => write!(f, "string"),
            Type::Object => write!(f, "Object"),
            Type::Class(None) => write!(f, "Class"),
            Type::Class(Some(n)) => write!(f, "Class<{n}>"),
            Type::Array(t) => write!(f, "Array<{t}>"),
            Type::Map(k, v) => write!(f, "Map<{k}, {v}>"),
            Type::Set(t) => write!(f, "Set<{t}>"),
            Type::Interval(t) => write!(f, "Interval<{t}>"),
            Type::Function { args, return_type } => {
                write!(f, "(")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{a}")?;
                }
                write!(f, ") => {return_type}")
            }
            Type::Compound(types) => {
                for (i, t) in types.iter().enumerate() {
                    if i > 0 {
                        write!(f, " | ")?;
                    }
                    write!(f, "{t}")?;
                }
                Ok(())
            }
        }
    }
}

/// Cast compatibility between types (matches Java `CastType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CastType {
    Equals,
    Upcast,
    SafeDowncast,
    UnsafeDowncast,
    Incompatible,
}
