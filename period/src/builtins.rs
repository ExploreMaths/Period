use std::cell::RefCell;
use std::io::{self, Write};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use num_bigint::BigInt;
use num_traits::cast::ToPrimitive;

use crate::environment::Environment;
use crate::value::{range_len, BuiltInValue, Value};

pub fn install_builtins(env: &Environment) {
    env.define_untyped("length", Value::BuiltIn(Box::new(BuiltInValue { name: "length".to_string(), min_arity: 1, max_arity: 1, func: builtin_length })));
    env.define_untyped("string", Value::BuiltIn(Box::new(BuiltInValue { name: "string".to_string(), min_arity: 1, max_arity: 1, func: builtin_string })));
    env.define_untyped("number", Value::BuiltIn(Box::new(BuiltInValue { name: "number".to_string(), min_arity: 1, max_arity: 1, func: builtin_number })));
    env.define_untyped("integer", Value::BuiltIn(Box::new(BuiltInValue { name: "integer".to_string(), min_arity: 1, max_arity: 1, func: builtin_integer })));
    env.define_untyped("boolean", Value::BuiltIn(Box::new(BuiltInValue { name: "boolean".to_string(), min_arity: 1, max_arity: 1, func: builtin_boolean })));
    env.define_untyped("type", Value::BuiltIn(Box::new(BuiltInValue { name: "type".to_string(), min_arity: 1, max_arity: 1, func: builtin_type })));
    env.define_untyped("input", Value::BuiltIn(Box::new(BuiltInValue { name: "input".to_string(), min_arity: 0, max_arity: 0, func: builtin_input })));
    env.define_untyped("range", Value::BuiltIn(Box::new(BuiltInValue { name: "range".to_string(), min_arity: 1, max_arity: 3, func: builtin_range })));
    env.define_untyped("error", Value::BuiltIn(Box::new(BuiltInValue { name: "error".to_string(), min_arity: 1, max_arity: 1, func: builtin_error })));
}

fn builtin_length(args: &[Value]) -> Result<Value, String> {
    match &args[0] {
        Value::String(s) => Ok(Value::integer(s.len() as i64)),
        Value::List(l) => Ok(Value::integer(l.borrow().len() as i64)),
        Value::Dict(d) => Ok(Value::integer(d.borrow().len() as i64)),
        Value::Range { start, stop, step } => Ok(Value::integer(range_len(*start, *stop, *step))),
        _ => Err("Cannot get length".to_string()),
    }
}

fn builtin_string(args: &[Value]) -> Result<Value, String> {
    Ok(Value::String(args[0].to_string()))
}

fn builtin_number(args: &[Value]) -> Result<Value, String> {
    match &args[0] {
        Value::Integer(n) => Ok(Value::Number(n.to_f64())),
        Value::Number(n) => Ok(Value::Number(*n)),
        Value::String(s) => s.parse::<f64>().map(Value::Number).or_else(|_| s.parse::<BigInt>().map(Value::big_integer)).map_err(|_| "Cannot convert to number".to_string()),
        Value::Bool(true) => Ok(Value::Number(1.0)),
        Value::Bool(false) => Ok(Value::Number(0.0)),
        _ => Err("Cannot convert to number".to_string()),
    }
}

fn builtin_integer(args: &[Value]) -> Result<Value, String> {
    match &args[0] {
        Value::Integer(n) => Ok(Value::Integer(n.clone())),
        Value::Number(n) => Ok(Value::integer(*n as i64)),
        Value::String(s) => s.parse::<BigInt>().map(Value::big_integer).map_err(|_| "Cannot convert to integer".to_string()),
        Value::Bool(true) => Ok(Value::integer(1)),
        Value::Bool(false) => Ok(Value::integer(0)),
        _ => Err("Cannot convert to integer".to_string()),
    }
}

fn builtin_boolean(args: &[Value]) -> Result<Value, String> {
    let b = match &args[0] {
        Value::Bool(b) => *b,
        Value::Integer(n) => !n.is_zero(),
        Value::Number(n) => *n != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Nothing => false,
        Value::List(l) => !l.borrow().is_empty(),
        Value::Dict(d) => !d.borrow().is_empty(),
        Value::Range { start, stop, step } => (*step > 0 && start < stop) || (*step < 0 && start > stop),
        _ => true,
    };
    Ok(Value::Bool(b))
}

fn builtin_type(args: &[Value]) -> Result<Value, String> {
    Ok(Value::String(args[0].type_name().to_string()))
}

fn builtin_input(_: &[Value]) -> Result<Value, String> {
    let mut s = String::new();
    io::stdout().flush().unwrap();
    io::stdin().read_line(&mut s).map_err(|e| e.to_string())?;
    Ok(Value::String(s.trim_end().to_string()))
}

fn builtin_error(args: &[Value]) -> Result<Value, String> {
    Err(match &args[0] { Value::String(s) => s.clone(), v => v.to_string() })
}

fn builtin_range(args: &[Value]) -> Result<Value, String> {
    let to_i = |v: &Value| match v {
        Value::Integer(n) => n.to_i64().ok_or_else(|| "range argument too large to iterate over".to_string()),
        Value::Number(n) => {
            if n.fract() != 0.0 {
                return Err("range arguments must be whole numbers".to_string());
            }
            Ok(*n as i64)
        }
        _ => Err("range args must be integers".to_string()),
    };
    let (start, stop, step) = match args.len() {
        1 => (0, to_i(&args[0])?, 1),
        2 => (to_i(&args[0])?, to_i(&args[1])?, 1),
        3 => (to_i(&args[0])?, to_i(&args[1])?, to_i(&args[2])?),
        _ => unreachable!(),
    };
    if step == 0 { return Err("range step cannot be zero".to_string()); }
    Ok(Value::Range { start, stop, step })
}

fn make_module(values: Vec<(&str, Value)>) -> Rc<RefCell<Environment>> {
    let env = Environment::new();
    for (name, value) in values { env.borrow().define_untyped(name, value); }
    env
}

pub fn make_math_module() -> Rc<RefCell<Environment>> {
    macro_rules! unary_float {
        ($name:ident, $f:path) => {
            fn $name(args: &[Value]) -> Result<Value, String> {
                let n = match &args[0] { Value::Integer(i) => i.to_f64(), Value::Number(n) => *n, _ => return Err("expected number".to_string()) };
                Ok(Value::Number($f(n)))
            }
        };
    }
    unary_float!(sin_fn, f64::sin);
    unary_float!(cos_fn, f64::cos);
    unary_float!(tan_fn, f64::tan);
    unary_float!(sqrt_fn, f64::sqrt);
    unary_float!(abs_fn, f64::abs);
    unary_float!(floor_fn, f64::floor);
    unary_float!(ceil_fn, f64::ceil);
    make_module(vec![
        ("sin", Value::BuiltIn(Box::new(BuiltInValue { name: "sin".to_string(), min_arity: 1, max_arity: 1, func: sin_fn }))),
        ("cos", Value::BuiltIn(Box::new(BuiltInValue { name: "cos".to_string(), min_arity: 1, max_arity: 1, func: cos_fn }))),
        ("tan", Value::BuiltIn(Box::new(BuiltInValue { name: "tan".to_string(), min_arity: 1, max_arity: 1, func: tan_fn }))),
        ("sqrt", Value::BuiltIn(Box::new(BuiltInValue { name: "sqrt".to_string(), min_arity: 1, max_arity: 1, func: sqrt_fn }))),
        ("abs", Value::BuiltIn(Box::new(BuiltInValue { name: "abs".to_string(), min_arity: 1, max_arity: 1, func: abs_fn }))),
        ("floor", Value::BuiltIn(Box::new(BuiltInValue { name: "floor".to_string(), min_arity: 1, max_arity: 1, func: floor_fn }))),
        ("ceil", Value::BuiltIn(Box::new(BuiltInValue { name: "ceil".to_string(), min_arity: 1, max_arity: 1, func: ceil_fn }))),
        ("pi", Value::Number(std::f64::consts::PI)),
    ])
}

pub fn make_random_module() -> Rc<RefCell<Environment>> {
    static SEED: AtomicU64 = AtomicU64::new(0);
    fn random_fn(_: &[Value]) -> Result<Value, String> {
        let mut seed = SEED.load(Ordering::Relaxed);
        if seed == 0 {
            seed = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos() as u64;
        }
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        SEED.store(seed, Ordering::Relaxed);
        let r = ((seed >> 33) as f64) / ((1u64 << 31) as f64);
        Ok(Value::Number(r))
    }
    make_module(vec![
        ("random", Value::BuiltIn(Box::new(BuiltInValue { name: "random".to_string(), min_arity: 0, max_arity: 0, func: random_fn }))),
    ])
}

pub fn make_string_module() -> Rc<RefCell<Environment>> {
    make_module(vec![
        ("upper", Value::BuiltIn(Box::new(BuiltInValue { name: "upper".to_string(), min_arity: 1, max_arity: 1, func: |args| {
            match &args[0] { Value::String(s) => Ok(Value::String(s.to_uppercase())), _ => Err("expected string".to_string()) }
        }}))),
        ("lower", Value::BuiltIn(Box::new(BuiltInValue { name: "lower".to_string(), min_arity: 1, max_arity: 1, func: |args| {
            match &args[0] { Value::String(s) => Ok(Value::String(s.to_lowercase())), _ => Err("expected string".to_string()) }
        }}))),
        ("trim", Value::BuiltIn(Box::new(BuiltInValue { name: "trim".to_string(), min_arity: 1, max_arity: 1, func: |args| {
            match &args[0] { Value::String(s) => Ok(Value::String(s.trim().to_string())), _ => Err("expected string".to_string()) }
        }}))),
        ("split", Value::BuiltIn(Box::new(BuiltInValue { name: "split".to_string(), min_arity: 2, max_arity: 2, func: |args| {
            match (&args[0], &args[1]) {
                (Value::String(s), Value::String(delim)) => {
                    let parts: Vec<Value> = s.split(delim).map(|p| Value::String(p.to_string())).collect();
                    Ok(Value::List(Rc::new(RefCell::new(parts))))
                }
                _ => Err("expected string and delimiter".to_string())
            }
        }}))),
        ("contains", Value::BuiltIn(Box::new(BuiltInValue { name: "contains".to_string(), min_arity: 2, max_arity: 2, func: |args| {
            match (&args[0], &args[1]) {
                (Value::String(s), Value::String(sub)) => Ok(Value::Bool(s.contains(sub))),
                _ => Err("expected string and substring".to_string())
            }
        }}))),
        ("starts_with", Value::BuiltIn(Box::new(BuiltInValue { name: "starts_with".to_string(), min_arity: 2, max_arity: 2, func: |args| {
            match (&args[0], &args[1]) {
                (Value::String(s), Value::String(prefix)) => Ok(Value::Bool(s.starts_with(prefix))),
                _ => Err("expected string and prefix".to_string())
            }
        }}))),
        ("ends_with", Value::BuiltIn(Box::new(BuiltInValue { name: "ends_with".to_string(), min_arity: 2, max_arity: 2, func: |args| {
            match (&args[0], &args[1]) {
                (Value::String(s), Value::String(suffix)) => Ok(Value::Bool(s.ends_with(suffix))),
                _ => Err("expected string and suffix".to_string())
            }
        }}))),
        ("replace", Value::BuiltIn(Box::new(BuiltInValue { name: "replace".to_string(), min_arity: 3, max_arity: 3, func: |args| {
            match (&args[0], &args[1], &args[2]) {
                (Value::String(s), Value::String(from), Value::String(to)) => Ok(Value::String(s.replace(from, to))),
                _ => Err("expected string, from, and to".to_string())
            }
        }}))),
        ("slice", Value::BuiltIn(Box::new(BuiltInValue { name: "slice".to_string(), min_arity: 2, max_arity: 2, func: |args| {
            match (&args[0], &args[1]) {
                (Value::String(s), Value::Integer(start)) => {
                    let start = start.to_i64().ok_or("slice start too large")?;
                    let start = if start < 0 { s.len().saturating_sub((-start) as usize) } else { start as usize };
                    let start = start.min(s.len());
                    Ok(Value::String(s[start..].to_string()))
                }
                _ => Err("expected string and integer start".to_string())
            }
        }}))),
        ("substring", Value::BuiltIn(Box::new(BuiltInValue { name: "substring".to_string(), min_arity: 3, max_arity: 3, func: |args| {
            match (&args[0], &args[1], &args[2]) {
                (Value::String(s), Value::Integer(start), Value::Integer(end)) => {
                    let start = start.to_i64().ok_or("slice start too large")?;
                    let start = if start < 0 { s.len().saturating_sub((-start) as usize) } else { start as usize };
                    let end = end.to_i64().ok_or("slice end too large")?;
                    let end = if end < 0 { s.len().saturating_sub((-end) as usize) } else { end as usize };
                    let end = end.min(s.len());
                    let start = start.min(end);
                    Ok(Value::String(s[start..end].to_string()))
                }
                _ => Err("expected string, integer start, and integer end".to_string())
            }
        }}))),
    ])
}

pub fn make_time_module() -> Rc<RefCell<Environment>> {
    make_module(vec![
        ("now", Value::BuiltIn(Box::new(BuiltInValue { name: "now".to_string(), min_arity: 0, max_arity: 0, func: |_| {
            Ok(Value::Number(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64()))
        }}))),
    ])
}
