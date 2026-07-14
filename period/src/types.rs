//! Shared type-system definitions used by the static type checker and LSP.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    Integer,
    Number,
    String,
    Boolean,
    Nothing,
    List(Box<Type>),
    Dict(Box<Type>, Box<Type>),
    Function(Vec<Type>, Box<Type>),
    Class(String),
    Instance(String),
    Module(String),
    Range,
    Error,
    /// Gradual-typing escape hatch: explicit `anything` annotation.
    /// Compatible with every type; checked at runtime instead.
    Anything,
    /// A value whose type is not statically known because no annotation was
    /// given. Unlike `Anything`, this does not satisfy concrete type
    /// requirements, so using an unannotated value in a typed position is an
    /// error.
    Unknown,
    /// Union of two or more types, written `a or b` or `a, b or c`.
    Union(Vec<Type>),
}

impl Type {
    pub fn name(&self) -> String {
        match self {
            Type::Integer => "integer".to_string(),
            Type::Number => "number".to_string(),
            Type::String => "string".to_string(),
            Type::Boolean => "boolean".to_string(),
            Type::Nothing => "nothing".to_string(),
            Type::List(t) => format!("list of {}", t.name()),
            Type::Dict(k, v) => format!("dictionary of {} to {}", k.name(), v.name()),
            Type::Function(args, ret) => {
                let args = args.iter().map(|a| a.name()).collect::<Vec<_>>().join(", ");
                format!("function({}) -> {}", args, ret.name())
            }
            Type::Class(n) => format!("class {}", n),
            Type::Instance(n) => n.clone(),
            Type::Module(n) => format!("module {}", n),
            Type::Range => "range".to_string(),
            Type::Error => "<error>".to_string(),
            Type::Anything => "anything".to_string(),
            Type::Unknown => "unknown".to_string(),
            Type::Union(members) => {
                let names: Vec<String> = members.iter().map(|m| m.name()).collect();
                if let Some((last, head)) = names.split_last()
                    && !head.is_empty() {
                        return format!("{} or {}", head.join(", "), last);
                    }
                names.into_iter().next().unwrap_or_default()
            }
        }
    }

    pub fn is_subtype(&self, other: &Type) -> bool {
        match (self, other) {
            // Explicit `anything` is compatible with every type, and every type
            // is compatible with explicit `anything`.
            (Type::Anything, _) => true,
            (_, Type::Anything) => true,
            // `unknown` represents a missing annotation. It can hold any value,
            // so every type is a subtype of unknown, but unknown itself is not
            // a subtype of any concrete type.
            (_, Type::Unknown) => true,
            (Type::Unknown, _) => false,
            (Type::Error, _) => true,
            (_, Type::Error) => true,
            (Type::Union(members), other) => members.iter().all(|m| m.is_subtype(other)),
            (this, Type::Union(members)) => members.iter().any(|m| this.is_subtype(m)),
            (Type::Integer, Type::Number) => true,
            (Type::List(a), Type::List(b)) => a.is_subtype(b),
            (Type::Dict(ak, av), Type::Dict(bk, bv)) => ak.is_subtype(bk) && av.is_subtype(bv),
            (Type::Instance(a), Type::Instance(b)) => a == b,
            (Type::Function(a_args, a_ret), Type::Function(b_args, b_ret)) => {
                a_args.len() == b_args.len()
                    && b_args.iter().zip(a_args.iter()).all(|(b, a)| b.is_subtype(a))
                    && a_ret.is_subtype(b_ret)
            }
            _ => self == other,
        }
    }
}

/// Parse a Period type annotation string into a `Type`.
///
/// Supports unions in the language's list style: `a or b`, or `a, b or c`
/// for three or more members. Also supports function types:
/// `function() -> nothing`, `function(integer) -> boolean`,
/// `function(integer, string) -> number`, etc.
pub fn parse_type_ann(ann: &str) -> Type {
    let ann = ann.trim();
    if let Some(ty) = parse_function_type(ann) {
        return ty;
    }
    let members: Vec<&str> = ann
        .split(" or ")
        .flat_map(|seg| seg.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if members.len() > 1 {
        return Type::Union(members.iter().map(|m| parse_type_ann(m)).collect());
    }
    let parts: Vec<&str> = ann.split_whitespace().collect();
    if parts.is_empty() {
        return Type::Anything;
    }
    match parts[0] {
        "integer" => Type::Integer,
        "number" => Type::Number,
        "string" => Type::String,
        "boolean" => Type::Boolean,
        "nothing" => Type::Nothing,
        "list" if parts.len() >= 3 && parts[1] == "of" => {
            Type::List(Box::new(parse_type_ann(&parts[2..].join(" "))))
        }
        "dictionary" if parts.len() >= 5 && parts[1] == "of" && parts[3] == "to" => {
            Type::Dict(
                Box::new(parse_type_ann(parts[2])),
                Box::new(parse_type_ann(&parts[4..].join(" "))),
            )
        }
        "range" => Type::Range,
        "anything" => Type::Anything,
        name => Type::Instance(name.to_string()),
    }
}

/// Try to parse a function type annotation of the form
/// `function(<args>) -> <ret>`. Returns `None` if the input does not match.
fn parse_function_type(s: &str) -> Option<Type> {
    let s = s.trim();
    let keyword = "function";
    if !s.starts_with(keyword) {
        return None;
    }
    let rest = s[keyword.len()..].trim_start();
    if !rest.starts_with('(') {
        return None;
    }
    let rest = &rest[1..];
    let mut paren_depth = 1usize;
    let mut close_idx = 0usize;
    for (idx, c) in rest.char_indices() {
        match c {
            '(' => paren_depth += 1,
            ')' => {
                paren_depth = paren_depth.saturating_sub(1);
                if paren_depth == 0 {
                    close_idx = idx;
                    break;
                }
            }
            _ => {}
        }
    }
    if paren_depth != 0 {
        return None;
    }
    let args_str = &rest[..close_idx];
    let after = rest[close_idx + 1..].trim_start();
    if !after.starts_with("->") {
        return None;
    }
    let ret_str = after[2..].trim();
    let args: Vec<Type> = if args_str.trim().is_empty() {
        Vec::new()
    } else {
        split_top_level(args_str, ',')
            .iter()
            .map(|a| parse_type_ann(a))
            .collect()
    };
    let ret = parse_type_ann(ret_str);
    Some(Type::Function(args, Box::new(ret)))
}

/// Split a string by a delimiter, but only at the top level of parentheses.
fn split_top_level(s: &str, delim: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    for (idx, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            d if d == delim && depth == 0 => {
                parts.push(&s[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    if start < s.len() {
        parts.push(&s[start..]);
    }
    parts
}

/// Summary information about a class used for static type checking.
#[derive(Default)]
pub struct ClassInfo {
    pub fields: HashMap<String, Type>,
    pub methods: HashMap<String, Type>,
    /// Types of `init` parameters, in declaration order. These determine the
    /// arity of `new ... with ...`, which is separate from the class fields.
    pub init_params: Vec<Type>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_primitive_types() {
        assert_eq!(parse_type_ann("integer"), Type::Integer);
        assert_eq!(parse_type_ann("number"), Type::Number);
        assert_eq!(parse_type_ann("string"), Type::String);
        assert_eq!(parse_type_ann("boolean"), Type::Boolean);
        assert_eq!(parse_type_ann("nothing"), Type::Nothing);
        assert_eq!(parse_type_ann("anything"), Type::Anything);
    }

    #[test]
    fn parse_compound_types() {
        assert_eq!(
            parse_type_ann("list of number"),
            Type::List(Box::new(Type::Number))
        );
        assert_eq!(
            parse_type_ann("dictionary of string to number"),
            Type::Dict(Box::new(Type::String), Box::new(Type::Number))
        );
    }

    #[test]
    fn parse_class_instance_type() {
        assert_eq!(parse_type_ann("Person"), Type::Instance("Person".to_string()));
    }

    #[test]
    fn parse_function_types() {
        assert_eq!(
            parse_type_ann("function() -> nothing"),
            Type::Function(vec![], Box::new(Type::Nothing))
        );
        assert_eq!(
            parse_type_ann("function(integer) -> boolean"),
            Type::Function(vec![Type::Integer], Box::new(Type::Boolean))
        );
        assert_eq!(
            parse_type_ann("function(integer, string) -> number"),
            Type::Function(vec![Type::Integer, Type::String], Box::new(Type::Number))
        );
        assert_eq!(
            parse_type_ann("function(integer) -> boolean or string"),
            Type::Function(
                vec![Type::Integer],
                Box::new(Type::Union(vec![Type::Boolean, Type::String]))
            )
        );
        assert_eq!(
            parse_type_ann("function(function(integer) -> boolean) -> nothing"),
            Type::Function(
                vec![Type::Function(vec![Type::Integer], Box::new(Type::Boolean))],
                Box::new(Type::Nothing)
            )
        );
    }

    #[test]
    fn integer_is_subtype_of_number() {
        assert!(Type::Integer.is_subtype(&Type::Number));
        assert!(!Type::Number.is_subtype(&Type::Integer));
    }

    #[test]
    fn list_subtyping() {
        let int_list = Type::List(Box::new(Type::Integer));
        let num_list = Type::List(Box::new(Type::Number));
        assert!(int_list.is_subtype(&num_list));
        assert!(!num_list.is_subtype(&int_list));
    }

    #[test]
    fn anything_absorbs_everything() {
        assert!(Type::Integer.is_subtype(&Type::Anything));
        assert!(Type::Anything.is_subtype(&Type::Integer));
    }

    #[test]
    fn type_names() {
        assert_eq!(Type::Integer.name(), "integer");
        assert_eq!(
            Type::List(Box::new(Type::Integer)).name(),
            "list of integer"
        );
    }

    #[test]
    fn parse_union_types() {
        assert_eq!(
            parse_type_ann("number or string"),
            Type::Union(vec![Type::Number, Type::String])
        );
        assert_eq!(
            parse_type_ann("integer, number or string"),
            Type::Union(vec![Type::Integer, Type::Number, Type::String])
        );
        assert_eq!(
            parse_type_ann("list of integer or string"),
            Type::Union(vec![Type::List(Box::new(Type::Integer)), Type::String])
        );
    }

    #[test]
    fn union_names() {
        assert_eq!(
            Type::Union(vec![Type::Number, Type::String]).name(),
            "number or string"
        );
        assert_eq!(
            Type::Union(vec![Type::Integer, Type::Number, Type::String]).name(),
            "integer, number or string"
        );
    }

    #[test]
    fn union_subtyping() {
        let u = Type::Union(vec![Type::Number, Type::String]);
        // A value matches a union if it matches any member.
        assert!(Type::Integer.is_subtype(&u)); // integer <: number
        assert!(Type::String.is_subtype(&u));
        assert!(!Type::Boolean.is_subtype(&u));
        // A union is a subtype only if every member is.
        assert!(Type::Union(vec![Type::Integer, Type::Number]).is_subtype(&Type::Number));
        assert!(!u.is_subtype(&Type::Number));
        // Union vs union.
        assert!(Type::Union(vec![Type::Integer, Type::String]).is_subtype(&u));
    }
}
