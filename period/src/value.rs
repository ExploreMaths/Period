use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use num_bigint::BigInt;
use num_traits::cast::FromPrimitive;

use crate::ast::*;
use crate::environment::Environment;

#[derive(Clone)]
pub enum Value {
    Integer(BigInt),
    Number(f64),
    String(String),
    Bool(bool),
    Nothing,
    List(Rc<RefCell<Vec<Value>>>),
    Dict(Rc<RefCell<HashMap<ValueKey, Value>>>),
    Range {
        start: i64,
        stop: i64,
        step: i64,
    },
    Function {
        name: String,
        params: Vec<(String, Option<String>)>,
        return_type: Option<String>,
        body: Vec<Stmt>,
        closure: Rc<RefCell<Environment>>,
        span: Span,
        from_module: bool,
    },
    Class {
        name: String,
        init: Option<Init>,
        methods: HashMap<String, Stmt>,
        from_module: bool,
    },
    Instance {
        class: Box<Value>,
        fields: Rc<RefCell<HashMap<String, Value>>>,
    },
    BuiltIn {
        name: String,
        min_arity: usize,
        max_arity: usize,
        func: fn(&[Value]) -> Result<Value, String>,
    },
    Module {
        name: String,
        env: Rc<RefCell<Environment>>,
    },
    Error {
        message: String,
        line: i64,
        col: i64,
    },
}

impl std::fmt::Debug for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Integer(n) => write!(f, "{}", n),
            Value::Number(n) => write!(f, "{}", n),
            Value::String(s) => write!(f, "\"{}\"", s),
            Value::Bool(b) => write!(f, "{}", if *b { "true" } else { "false" }),
            Value::Nothing => write!(f, "nothing"),
            Value::List(l) => write!(f, "{:?}", l.borrow()),
            Value::Dict(d) => write!(f, "{:?}", d.borrow()),
            Value::Function { name, .. } => write!(f, "<function {}>", name),
            Value::Class { name, .. } => write!(f, "<class {}>", name),
            Value::Instance { class, .. } => write!(f, "<instance of {:?}>", class),
            Value::BuiltIn { name, .. } => write!(f, "<built-in {}>", name),
            Value::Module { name, .. } => write!(f, "<module {}>", name),
            Value::Error { message, .. } => write!(f, "error: {}", message),
            Value::Range { start, stop, step } => write!(f, "range({}, {}, {})", start, stop, step),
        }
    }
}

fn integer_eq_f64(a: &BigInt, b: f64) -> bool {
    if !b.is_finite() {
        return false;
    }
    if b.fract() != 0.0 {
        return false;
    }
    if let Some(i) = BigInt::from_f64(b) {
        a == &i
    } else {
        false
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Integer(a), Value::Integer(b)) => a == b,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::Integer(a), Value::Number(b)) | (Value::Number(b), Value::Integer(a)) => integer_eq_f64(a, *b),
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Nothing, Value::Nothing) => true,
            (Value::List(a), Value::List(b)) => a.borrow().eq(&*b.borrow()),
            (Value::Dict(a), Value::Dict(b)) => a.borrow().eq(&*b.borrow()),
            (Value::Error { message: a, .. }, Value::Error { message: b, .. }) => a == b,
            (
                Value::Range { start: a, stop: b, step: c },
                Value::Range { start: d, stop: e, step: f },
            ) => a == d && b == e && c == f,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ValueKey {
    Integer(BigInt),
    Number(u64),
    String(String),
    Bool(bool),
    Nothing,
}

impl Value {
    pub fn as_key(&self) -> Result<ValueKey, String> {
        match self {
            Value::Integer(n) => Ok(ValueKey::Integer(n.clone())),
            Value::Number(n) => Ok(ValueKey::Number(n.to_bits())),
            Value::String(s) => Ok(ValueKey::String(s.clone())),
            Value::Bool(b) => Ok(ValueKey::Bool(*b)),
            Value::Nothing => Ok(ValueKey::Nothing),
            Value::Range { .. } => Err("range is not hashable".to_string()),
            _ => Err(format!("{} is not hashable", self.type_name())),
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Integer(_) => "integer",
            Value::Number(_) => "number",
            Value::String(_) => "string",
            Value::Bool(_) => "boolean",
            Value::Nothing => "nothing",
            Value::List(_) => "list",
            Value::Dict(_) => "dictionary",
            Value::Function { .. } => "function",
            Value::Class { .. } => "class",
            Value::Instance { .. } => "instance",
            Value::BuiltIn { .. } => "built-in",
            Value::Module { .. } => "module",
            Value::Range { .. } => "range",
            Value::Error { .. } => "error",
        }
    }

}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Integer(n) => write!(f, "{}", n),
            Value::Number(n) => write!(f, "{}", n),
            Value::String(s) => write!(f, "{}", s),
            Value::Bool(b) => write!(f, "{}", if *b { "true" } else { "false" }),
            Value::Nothing => write!(f, "nothing"),
            Value::List(l) => {
                let items: Vec<String> = l.borrow().iter().map(|v| v.to_string()).collect();
                write!(f, "[{}]", items.join(", "))
            }
            Value::Dict(d) => {
                let items: Vec<String> = d.borrow().iter()
                    .map(|(k, v)| format!("{}: {}", k.to_value(), v))
                    .collect();
                write!(f, "{{{}}}", items.join(", "))
            }
            Value::Function { name, .. } => write!(f, "<function {}>", name),
            Value::Class { name, .. } => write!(f, "<class {}>", name),
            Value::Instance { class, .. } => write!(f, "<instance of {:?}>", class),
            Value::BuiltIn { name, .. } => write!(f, "<built-in {}>", name),
            Value::Module { name, .. } => write!(f, "<module {}>", name),
            Value::Range { start, stop, step } => write!(f, "range({}, {}, {})", start, stop, step),
            Value::Error { message, line, col } => write!(f, "{}:{}: {}", line, col, message),
        }
    }
}

impl ValueKey {
    pub fn to_value(&self) -> Value {
        match self {
            ValueKey::Integer(n) => Value::Integer(n.clone()),
            ValueKey::Number(b) => Value::Number(f64::from_bits(*b)),
            ValueKey::String(s) => Value::String(s.clone()),
            ValueKey::Bool(b) => Value::Bool(*b),
            ValueKey::Nothing => Value::Nothing,
        }
    }

}

pub fn range_len(start: i64, stop: i64, step: i64) -> i64 {
    if step == 0 || (step > 0 && start >= stop) || (step < 0 && start <= stop) {
        return 0;
    }
    let diff = if step > 0 { stop - start } else { start - stop };
    let abs_step = step.abs();
    (diff + abs_step - 1) / abs_step
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_equals_number() {
        assert_eq!(Value::Integer(BigInt::from(5)), Value::Number(5.0));
        assert_ne!(Value::Integer(BigInt::from(5)), Value::Number(5.5));
        // Large integers must not lose precision when compared with integral floats.
        assert_ne!(Value::Integer(BigInt::from(i64::MAX)), Value::Number(i64::MAX as f64));
        assert_ne!(Value::Integer(BigInt::from(9_007_199_254_740_993_i64)), Value::Number(9_007_199_254_740_992.0));
        assert_eq!(Value::Integer(BigInt::from(9_007_199_254_740_992_i64)), Value::Number(9_007_199_254_740_992.0));
    }

    #[test]
    fn type_names() {
        assert_eq!(Value::Integer(BigInt::from(1)).type_name(), "integer");
        assert_eq!(Value::Number(1.5).type_name(), "number");
        assert_eq!(Value::String("hi".to_string()).type_name(), "string");
        assert_eq!(Value::Bool(true).type_name(), "boolean");
        assert_eq!(Value::Nothing.type_name(), "nothing");
    }

    #[test]
    fn list_display() {
        let list = Value::List(Rc::new(RefCell::new(vec![
            Value::Integer(BigInt::from(1)),
            Value::Integer(BigInt::from(2)),
        ])));
        assert_eq!(format!("{:?}", list), "[1, 2]");
    }

    #[test]
    fn range_len_calculations() {
        assert_eq!(range_len(0, 10, 1), 10);
        assert_eq!(range_len(0, 10, 2), 5);
        assert_eq!(range_len(10, 0, -2), 5);
        assert_eq!(range_len(0, 0, 1), 0);
    }

    #[test]
    fn value_key_roundtrip() {
        let key = Value::Integer(BigInt::from(42)).as_key().unwrap();
        assert_eq!(key.to_value(), Value::Integer(BigInt::from(42)));
    }
}
