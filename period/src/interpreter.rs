use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

use crate::ast::{Program, Span};
use crate::builtins::{
    install_builtins, make_math_module, make_random_module, make_string_module, make_system_module,
    make_time_module,
};
use crate::compiler;
use crate::environment::Environment;
use crate::lexer::{Lexer, TokenKind};
use crate::parser::Parser;
use crate::value::{ModuleValue, Value};
use crate::vm;

#[derive(Debug)]
pub enum Control {
    #[allow(dead_code)]
    Return(Value, Span),
    Error(String),
    RuntimeError(String, Span),
}

pub struct Interpreter {
    pub(crate) env: Rc<RefCell<Environment>>,
    pub output: Vec<String>,
    pub(crate) modules: RefCell<HashMap<String, Rc<RefCell<Environment>>>>,
    /// Modules currently being loaded; used to detect circular imports.
    loading_modules: RefCell<HashSet<String>>,
    pub(crate) silent: bool,
    current_path: Option<PathBuf>,
    /// True while interpreting a module file; marks top-level functions/classes
    /// as originating from a module so their runtime errors can be mapped to
    /// the user's call site.
    pub(crate) loading_module: bool,
}

impl Interpreter {
    pub fn new() -> Self {
        let env = Environment::new();
        install_builtins(&env.borrow());
        Self {
            env,
            output: Vec::new(),
            modules: RefCell::new(HashMap::new()),
            loading_modules: RefCell::new(HashSet::new()),
            silent: false,
            current_path: None,
            loading_module: false,
        }
    }

    pub fn set_current_path(&mut self, path: impl Into<PathBuf>) {
        self.current_path = Some(path.into());
    }

    pub(crate) fn resolve_path(&self, path: &str) -> PathBuf {
        let p = PathBuf::from(path);
        if p.is_absolute() {
            return p;
        }
        if let Some(current) = &self.current_path {
            let dir = if current.is_file() {
                current.parent().unwrap_or(current)
            } else {
                current
            };
            return dir.join(p);
        }
        p
    }

    pub fn interpret(&mut self, program: &Program, force_globals: bool) -> Result<(), Control> {
        // Single execution path: compile to bytecode, then run on the VM.
        let main = compiler::Compiler::compile_program(&program.statements, false, force_globals)
            .map_err(|e| Control::Error(format!("compilation error: {}", e)))?;
        let main = std::rc::Rc::new(main);
        vm::Vm::new(self, main).run()
    }

    pub fn is_truthy(value: &Value) -> bool {
        match value {
            Value::Nothing => false,
            Value::Bool(b) => *b,
            Value::Integer(n) => !n.is_zero(),
            Value::Number(n) => *n != 0.0,
            Value::String(s) => !s.is_empty(),
            Value::List(l) => !l.borrow().is_empty(),
            Value::Dict(d) => !d.borrow().is_empty(),
            _ => true,
        }
    }

    pub(crate) fn import_module(&mut self, path: &str, span: &Span) -> Result<(), Control> {
        if self.modules.borrow().contains_key(path) {
            return Ok(());
        }
        if !self.loading_modules.borrow_mut().insert(path.to_string()) {
            return Err(Control::RuntimeError(
                format!(
                    "Circular import detected: '{}' is already being loaded",
                    path
                ),
                span.clone(),
            ));
        }

        let result = self.import_module_inner(path, span);
        self.loading_modules.borrow_mut().remove(path);
        result
    }

    fn import_module_inner(&mut self, path: &str, span: &Span) -> Result<(), Control> {
        let env = if let Some(file) = find_module_file(path, self.current_path.as_deref()) {
            self.load_period_module(path, &file)?
        } else {
            match path {
                "math" => make_math_module(),
                "random" => make_random_module(),
                "string" => make_string_module(),
                "system" => make_system_module(),
                "time" => make_time_module(),
                _ => {
                    return Err(Control::RuntimeError(
                        format!("Module '{}' not found", path),
                        span.clone(),
                    ));
                }
            }
        };

        self.modules
            .borrow_mut()
            .insert(path.to_string(), env.clone());
        let exposed_name = path.rsplit('/').next().unwrap_or(path);
        self.env.borrow().define_untyped(
            exposed_name,
            Value::Module(Box::new(ModuleValue {
                name: path.to_string(),
                env: env.clone(),
            })),
        );
        let exports = env.borrow().exported_names();
        let filter = !exports.is_empty();
        for (name, value, type_ann) in env.borrow().entries() {
            if !filter || exports.contains(&name) {
                self.env.borrow().define(&name, value, type_ann);
            }
        }
        Ok(())
    }

    fn run_compiled_module_main(
        &mut self,
        main: Rc<crate::bytecode::CompiledFunction>,
    ) -> Result<(), Control> {
        crate::vm::Vm::new(self, main).run()
    }

    fn load_period_module(
        &mut self,
        name: &str,
        path: &std::path::Path,
    ) -> Result<Rc<RefCell<Environment>>, Control> {
        let source = fs::read_to_string(path)
            .map_err(|e| Control::Error(format!("Cannot read module '{}': {}", name, e)))?;
        let program = parse_module(&source).map_err(|errors| {
            Control::Error(format!("Module '{}':\n{}", name, errors.join("\n")))
        })?;

        let builtins = Environment::new();
        install_builtins(&builtins.borrow());
        let module_env = Environment::with_parent(builtins);

        let old_env = self.env.clone();
        let old_silent = self.silent;
        let old_loading = self.loading_module;
        self.env = module_env.clone();
        self.silent = true;
        self.loading_module = true;
        let main = Rc::new(
            compiler::Compiler::compile_program(&program.statements, true, false)
                .map_err(|e| Control::Error(format!("Module '{}': {}", name, e.0)))?,
        );
        let result = self.run_compiled_module_main(main);
        self.env = old_env;
        self.silent = old_silent;
        self.loading_module = old_loading;

        result?;
        Ok(module_env)
    }
}

fn stdlib_locations() -> Vec<PathBuf> {
    let mut locs = Vec::new();
    if let Ok(v) = env::var("PERIOD_STDLIB") {
        locs.push(PathBuf::from(v));
    }
    if let Ok(exe) = env::current_exe()
        && let Some(parent) = exe.parent()
    {
        locs.push(parent.join("stdlib"));
        // Development layout: binary next to a `period` project directory.
        locs.push(parent.join("period").join("stdlib"));
        // FHS-style install layout (e.g. /usr/local/bin/period -> /usr/local/share/period/stdlib)
        if let Some(grandparent) = parent.parent() {
            locs.push(grandparent.join("share").join("period").join("stdlib"));
        }
        // Rust cargo development layout: binary is at period/target/<profile>/period,
        // stdlib is at the repository root or under period/stdlib.
        if parent
            .file_name()
            .map(|n| n == "debug" || n == "release")
            .unwrap_or(false)
            && let Some(repo) = parent
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
        {
            locs.push(repo.join("stdlib"));
            locs.push(repo.join("period").join("stdlib"));
        }
    }
    if let Ok(cwd) = env::current_dir() {
        locs.push(cwd.join("stdlib"));
        // Development layout: run from the repo root while stdlib lives under `period/`.
        locs.push(cwd.join("period").join("stdlib"));
    }
    locs
}

fn find_module_file(module: &str, current_path: Option<&std::path::Path>) -> Option<PathBuf> {
    module_file_candidates(module, current_path)
        .into_iter()
        .find(|candidate| candidate.is_file())
}

fn module_file_candidates(module: &str, current_path: Option<&std::path::Path>) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if module.starts_with("./") || module.starts_with("../") {
        // Relative POSIX-style paths: ./helper or ../utils/helper.
        if let Some(current) = current_path {
            let dir = if current.is_file() {
                current.parent().unwrap_or(current)
            } else {
                current
            };
            let local_path = dir.join(module);
            candidates.push(local_path.with_extension("period"));
            candidates.push(dir.join("lib").join(module).with_extension("period"));
        }
    } else {
        // Plain module names resolve to installed packages, the standard library,
        // or built-in modules. If a lockfile exists, prefer its listed packages.
        let project_root = current_path
            .and_then(|p| p.parent())
            .map(PathBuf::from)
            .or_else(|| env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));
        if let Some(path) = crate::package_manager::package_path_in(module, &project_root) {
            candidates.push(project_root.join(path));
        }
        let file = format!("{}.period", module);
        if let Ok(cwd) = env::current_dir() {
            candidates.push(cwd.join("period_packages").join(&file));
        }
        for loc in stdlib_locations() {
            candidates.push(loc.join(&file));
        }
    }

    candidates
}

fn parse_module(source: &str) -> Result<Program, Vec<String>> {
    let mut lexer = Lexer::new(source);
    let mut tokens = Vec::new();
    loop {
        let t = lexer.next_token().map_err(|e| vec![e])?;
        let eof = matches!(t.kind, TokenKind::Eof);
        tokens.push(t);
        if eof {
            break;
        }
    }
    Parser::new(tokens).parse_program()
}
