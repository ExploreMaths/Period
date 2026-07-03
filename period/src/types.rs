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
    Unknown,
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
            Type::Unknown => "<unknown>".to_string(),
        }
    }

    pub fn is_subtype(&self, other: &Type) -> bool {
        match (self, other) {
            (Type::Unknown, _) => true,
            (_, Type::Unknown) => true,
            (Type::Error, _) => true,
            (_, Type::Error) => true,
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
pub fn parse_type_ann(ann: &str) -> Type {
    let parts: Vec<&str> = ann.split_whitespace().collect();
    if parts.is_empty() {
        return Type::Unknown;
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
        name => Type::Instance(name.to_string()),
    }
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
    fn unknown_absorbs_everything() {
        assert!(Type::Integer.is_subtype(&Type::Unknown));
        assert!(Type::Unknown.is_subtype(&Type::Integer));
    }

    #[test]
    fn type_names() {
        assert_eq!(Type::Integer.name(), "integer");
        assert_eq!(
            Type::List(Box::new(Type::Integer)).name(),
            "list of integer"
        );
    }
}
