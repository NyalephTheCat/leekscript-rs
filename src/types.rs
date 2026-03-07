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
    /// `Class` or `Class<ClassName>` (the class/metatype value)
    Class(Option<String>),
    /// Instance of a class (e.g. `this` inside a class; type of a variable declared as that class).
    Instance(String),
    /// `Array` or `Array<T>`
    Array(Box<Type>),
    /// `Map` or `Map<K, V>`
    Map(Box<Type>, Box<Type>),
    /// `Set` or `Set<T>`
    Set(Box<Type>),
    /// `Interval` or `Interval<T>`
    Interval(Box<Type>),
    /// Function type: `Function<T1, T2, ... => R>` (0 args: `Function< => R>`)
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
    pub fn instance(class_name: impl Into<String>) -> Self {
        Type::Instance(class_name.into())
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

    /// Type with null removed from a union. For `A | null` returns `A`; for `A` returns `A`.
    #[must_use]
    pub fn non_null(ty: &Type) -> Type {
        match ty {
            Type::Compound(types) => {
                let rest: Vec<Type> = types.iter().filter(|t| !matches!(t, Type::Null)).cloned().collect();
                if rest.is_empty() {
                    Type::null()
                } else if rest.len() == 1 {
                    rest.into_iter().next().unwrap()
                } else {
                    Type::Compound(rest)
                }
            }
            _ => ty.clone(),
        }
    }

    /// Name/code as in source (`getCode()` in Java). Uses Display for full representation.
    #[must_use] 
    pub fn code(&self) -> String {
        self.to_string()
    }

    /// Format for type annotations (e.g. comments). Uses `type?` instead of `type | null`.
    #[must_use]
    pub fn for_annotation(&self) -> String {
        if let Type::Compound(types) = self {
            if types.len() == 2 && types.iter().any(|t| matches!(t, Type::Null)) {
                if let Some(t) = types.iter().find(|t| !matches!(t, Type::Null)) {
                    return format!("{}?", t);
                }
            }
        }
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

    /// Whether a value of type `other` can be assigned to a variable of type `self` (self = target, other = source).
    #[must_use]
    pub fn assignable_from(&self, other: &Type) -> bool {
        if self == other {
            return true;
        }
        match self {
            Type::Any => true,
            Type::Error | Type::Warning => false,
            Type::Real => other.is_number(), // int and real assignable to real
            Type::Compound(types) => types.iter().any(|t| t.assignable_from(other)),
            _ => {
                if matches!(other, Type::Null) {
                    return self.can_be_null()
                        || matches!(self, Type::Compound(_))
                        || matches!(self, Type::Class(_) | Type::Instance(_))
                        || matches!(self, Type::Array(_) | Type::Map(_, _) | Type::Set(_) | Type::Interval(_))
                        || matches!(self, Type::Function { .. });
                }
                match (self, other) {
                    (Type::Array(te), Type::Array(oe)) => te.assignable_from(oe),
                    (Type::Map(tk, tv), Type::Map(ok, ov)) => tk.assignable_from(ok) && tv.assignable_from(ov),
                    (Type::Set(te), Type::Set(oe)) => te.assignable_from(oe),
                    (Type::Interval(te), Type::Interval(oe)) => te.assignable_from(oe),
                    (Type::Class(ta), Type::Class(oa)) => match (ta, oa) {
                        (None, _) => true,
                        (Some(_), None) => false,
                        (Some(a), Some(b)) => a == b,
                    },
                    (Type::Instance(a), Type::Instance(b)) => a == b,
                    _ => false,
                }
            }
        }
    }

    /// Classify the cast from expression type `from` to target type `to`.
    #[must_use]
    pub fn check_cast(from: &Type, to: &Type) -> CastType {
        if from == to {
            return CastType::Equals;
        }
        match (from, to) {
            (Type::Error, _) | (_, Type::Error) => CastType::Incompatible,
            (_, Type::Any) => CastType::Upcast,
            (Type::Int, Type::Real) => CastType::Upcast,
            (Type::Null, to) if to.can_be_null() || matches!(to, Type::Compound(_) | Type::Class(_) | Type::Instance(_) | Type::Array(_) | Type::Map(_, _) | Type::Set(_) | Type::Interval(_) | Type::Function { .. }) => CastType::Upcast,
            (Type::Compound(types), to) if types.iter().any(|t| t == to) => CastType::SafeDowncast,
            (from, Type::Compound(types)) if types.iter().any(|t| from.assignable_from(t)) => CastType::Upcast,
            (Type::Any, _) => CastType::UnsafeDowncast,
            (Type::Real, Type::Int) | (Type::Real, Type::Bool) | (Type::Int, Type::Bool) => CastType::UnsafeDowncast,
            (Type::Class(Some(_)), Type::Class(None)) => CastType::Upcast,
            (Type::Class(None), Type::Class(Some(_))) => CastType::UnsafeDowncast,
            (Type::Class(Some(a)), Type::Class(Some(b))) if a == b => CastType::Equals,
            (Type::Class(Some(_)), Type::Class(Some(_))) => CastType::UnsafeDowncast,
            (Type::Instance(a), Type::Instance(b)) if a == b => CastType::Equals,
            (Type::Instance(_), Type::Instance(_)) => CastType::UnsafeDowncast,
            _ => {
                if to.assignable_from(from) {
                    CastType::Upcast
                } else {
                    CastType::Incompatible
                }
            }
        }
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
            Type::Instance(n) => write!(f, "{n}"),
            Type::Array(t) => write!(f, "Array<{t}>"),
            Type::Map(k, v) => write!(f, "Map<{k}, {v}>"),
            Type::Set(t) => write!(f, "Set<{t}>"),
            Type::Interval(t) => write!(f, "Interval<{t}>"),
            Type::Function { args, return_type } => {
                write!(f, "Function<")?;
                for (i, a) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{a}")?;
                }
                write!(f, " => {return_type}>")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assignable_exact_and_any() {
        assert!(Type::int().assignable_from(&Type::int()));
        assert!(Type::string().assignable_from(&Type::string()));
        assert!(Type::any().assignable_from(&Type::int()));
        assert!(Type::any().assignable_from(&Type::string()));
    }

    #[test]
    fn assignable_numeric() {
        assert!(Type::real().assignable_from(&Type::int()));
        assert!(!Type::int().assignable_from(&Type::real()));
    }

    #[test]
    fn assignable_null() {
        assert!(Type::any().assignable_from(&Type::null()));
        assert!(Type::compound2(Type::real(), Type::null()).assignable_from(&Type::null()));
    }

    #[test]
    fn assignable_compound() {
        let real_or_int = Type::compound2(Type::real(), Type::int());
        assert!(real_or_int.assignable_from(&Type::int()));
        assert!(real_or_int.assignable_from(&Type::real()));
    }

    #[test]
    fn check_cast_equals_and_upcast() {
        assert_eq!(Type::check_cast(&Type::int(), &Type::int()), CastType::Equals);
        assert_eq!(Type::check_cast(&Type::int(), &Type::real()), CastType::Upcast);
        assert_eq!(Type::check_cast(&Type::int(), &Type::any()), CastType::Upcast);
    }

    #[test]
    fn check_cast_incompatible() {
        assert_eq!(
            Type::check_cast(&Type::string(), &Type::int()),
            CastType::Incompatible
        );
    }

    #[test]
    fn check_cast_unsafe_downcast() {
        assert_eq!(
            Type::check_cast(&Type::any(), &Type::int()),
            CastType::UnsafeDowncast
        );
        assert_eq!(
            Type::check_cast(&Type::real(), &Type::int()),
            CastType::UnsafeDowncast
        );
    }

    #[test]
    fn for_annotation_optional_shorthand() {
        assert_eq!(Type::compound2(Type::int(), Type::null()).for_annotation(), "integer?");
        assert_eq!(Type::compound2(Type::null(), Type::real()).for_annotation(), "real?");
        assert_eq!(Type::compound2(Type::string(), Type::null()).for_annotation(), "string?");
        assert_eq!(Type::int().for_annotation(), "integer");
        assert_eq!(
            Type::compound2(Type::real(), Type::int()).for_annotation(),
            "real | integer"
        );
    }
}
