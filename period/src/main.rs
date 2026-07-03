#![allow(clippy::result_large_err)]

mod ast;
mod builtins;
mod environment;
mod interpreter;
mod lexer;
mod lsp;
mod parser;
mod reporting;
mod semantic;
mod type_checker;
mod types;
mod value;

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process::{self, Command};

fn install_package(name: &str) -> Result<(), String> {
    let packages_dir = std::path::PathBuf::from("period_packages");
    fs::create_dir_all(&packages_dir).map_err(|e| format!("cannot create period_packages: {}", e))?;

    let (url, filename) = if name.starts_with("http://") || name.starts_with("https://") {
        let filename = name.rsplit('/').next().unwrap_or(name);
        let filename = if filename.is_empty() { "package.period" } else { filename };
        (name.to_string(), filename.to_string())
    } else {
        let registry = env::var("PERIOD_REGISTRY")
            .unwrap_or_else(|_| "https://raw.githubusercontent.com/ExploreMaths/period-packages/main".to_string());
        let url = format!("{}/{}.period", registry.trim_end_matches('/'), name);
        (url, format!("{}.period", name))
    };

    let out_path = packages_dir.join(&filename);
    let status = Command::new("curl")
        .args(["-fsSL", &url, "-o", out_path.to_str().unwrap()])
        .status()
        .map_err(|e| format!("failed to run curl: {}", e))?;
    if !status.success() {
        return Err(format!("could not download '{}'", url));
    }
    println!("Installed {} -> {}", name, out_path.display());
    Ok(())
}

fn main() {
    // In release builds, suppress Rust's default panic backtrace so users see our
    // own friendly error messages. In debug builds keep the default hook so
    // developers get a backtrace when RUST_BACKTRACE=1.
    if cfg!(not(debug_assertions)) {
        std::panic::set_hook(Box::new(|info| {
            eprintln!("period: an internal error occurred");
            if let Some(msg) = info.payload().downcast_ref::<&str>() {
                eprintln!("details: {}", msg);
            } else if let Some(msg) = info.payload().downcast_ref::<String>() {
                eprintln!("details: {}", msg);
            }
            eprintln!("Set RUST_BACKTRACE=1 and run a debug build for more information.");
        }));
    }

    let args: Vec<String> = env::args().collect();
    if args.iter().any(|a| a == "--version" || a == "-v") {
        println!("period {}", env!("CARGO_PKG_VERSION"));
        process::exit(0);
    }
    if args.iter().any(|a| a == "--lsp") {
        if let Err(e) = lsp::run() {
            eprintln!("lsp error: {}", e);
            process::exit(1);
        }
        return;
    }
    if args.len() >= 2 && args[1] == "install" {
        if args.len() != 3 {
            eprintln!("usage: period install <package-or-url>");
            process::exit(1);
        }
        if let Err(e) = install_package(&args[2]) {
            eprintln!("install error: {}", e);
            process::exit(1);
        }
        return;
    }
    if args.len() == 1 {
        if let Err(e) = run_repl() {
            eprintln!("repl error: {}", e);
            process::exit(1);
        }
        return;
    }
    if args.len() != 2 {
        eprintln!("usage: period <file.period>");
        process::exit(1);
    }
    let path = &args[1];
    let source = fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("cannot read {}: {}", path, e);
        process::exit(1);
    });

    let program = match parse_source(&source) {
        Ok(p) => p,
        Err(msg) => {
            reporting::report_parse_error(path, &source, &msg);
            process::exit(1);
        }
    };

    // Semantic check before compilation so source-level errors are reported
    // with Period source locations instead of leaking raw C compiler output.
    let current_path = std::env::current_dir().ok().map(|cwd| cwd.join(path));
    let (sem_errors, sem_warnings) = semantic::program_diagnostics(&program, current_path.as_deref());
    for (span, msg) in sem_warnings {
        reporting::report_source_warning(path, &source, &span, &msg);
    }
    if !sem_errors.is_empty() {
        for (span, msg) in sem_errors {
            reporting::report_source_error(path, &source, &span, &msg);
        }
        process::exit(1);
    }

    let mut tc = type_checker::TypeChecker::new();
    let (type_errors, type_warnings) = tc.check(&program);
    for (span, msg) in type_warnings {
        reporting::report_source_warning(path, &source, &span, &msg);
    }
    if !type_errors.is_empty() {
        for (span, msg) in type_errors {
            reporting::report_source_error(path, &source, &span, &msg);
        }
        process::exit(1);
    }

    run_interpreter(&program, PathBuf::from(path), &source);
}

fn run_interpreter(program: &ast::Program, path: PathBuf, source: &str) -> ! {
    let mut interp = interpreter::Interpreter::new();
    interp.set_current_path(path.clone());
    if let Err(ctrl) = interp.interpret(program) {
        let path_str = path.to_string_lossy().to_string();
        match ctrl {
            interpreter::Control::RuntimeError(msg, span) => {
                reporting::report_runtime_error(&path_str, source, &msg, Some(&span));
            }
            interpreter::Control::Error(msg) => {
                reporting::report_runtime_error(&path_str, source, &msg, None);
            }
            _ => {
                eprintln!("{}: runtime error: {:?}", path_str, ctrl);
            }
        }
        process::exit(1);
    }
    process::exit(0);
}

fn parse_source(source: &str) -> Result<ast::Program, String> {
    let mut lexer = lexer::Lexer::new(source);
    let mut tokens = Vec::new();
    loop {
        let t = lexer.next_token()?;
        let eof = matches!(t.kind, lexer::TokenKind::Eof);
        tokens.push(t);
        if eof {
            break;
        }
    }
    parser::Parser::new(tokens).parse_program()
}

fn run_repl() -> Result<(), Box<dyn std::error::Error>> {
    println!("Period REPL. Type 'exit.' or 'quit.' to leave, or Ctrl+C.");
    let stdin = io::stdin();
    let mut interp = interpreter::Interpreter::new();
    if let Ok(cwd) = env::current_dir() {
        interp.set_current_path(cwd.clone());
    }
    let mut buffer = String::new();
    let mut repl_history: Vec<ast::Stmt> = Vec::new();

    loop {
        let prompt = if buffer.is_empty() { ">>> " } else { "... " };
        print!("{}", prompt);
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            println!();
            break;
        }
        if line.ends_with('\n') {
            line.pop();
        }
        if line.ends_with('\r') {
            line.pop();
        }

        if buffer.is_empty() && (line == "exit." || line == "quit.") {
            break;
        }

        if !buffer.is_empty() {
            buffer.push('\n');
        }
        buffer.push_str(&line);

        let trimmed = buffer.trim();
        if trimmed.is_empty() {
            buffer.clear();
            continue;
        }
        if !trimmed.ends_with('.') {
            continue;
        }

        match parse_source(&buffer) {
            Ok(program) => {
                let current_path = env::current_dir().ok();
                let mut trial_history = repl_history.clone();
                trial_history.extend(program.statements.clone());
                let trial_program = ast::Program { statements: trial_history };
                let mut had_error = false;
                let (sem_errors, sem_warnings) = semantic::program_diagnostics(&trial_program, current_path.as_deref());
                for (span, msg) in sem_warnings {
                    reporting::report_source_warning("<repl>", &buffer, &span, &msg);
                }
                for (span, msg) in sem_errors {
                    reporting::report_source_error("<repl>", &buffer, &span, &msg);
                    had_error = true;
                }
                if !had_error {
                    let mut tc = type_checker::TypeChecker::new();
                    let (type_errors, type_warnings) = tc.check(&trial_program);
                    for (span, msg) in type_warnings {
                        reporting::report_source_warning("<repl>", &buffer, &span, &msg);
                    }
                    for (span, msg) in type_errors {
                        reporting::report_source_error("<repl>", &buffer, &span, &msg);
                        had_error = true;
                    }
                }
                if !had_error {
                    repl_history.extend(program.statements.clone());
                    if let Err(ctrl) = interp.interpret(&program) {
                        match ctrl {
                            interpreter::Control::RuntimeError(msg, span) => {
                                reporting::report_runtime_error("<repl>", &buffer, &msg, Some(&span));
                            }
                            interpreter::Control::Error(msg) => {
                                reporting::report_runtime_error("<repl>", &buffer, &msg, None);
                            }
                            _ => eprintln!("runtime error: {:?}", ctrl),
                        }
                    }
                }
                buffer.clear();
            }
            Err(msg) => {
                reporting::report_parse_error("<repl>", &buffer, &msg);
                buffer.clear();
            }
        }
    }

    Ok(())
}
