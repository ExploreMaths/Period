//! Terminal error-reporting helpers shared by the CLI and REPL.

use crate::ast::Span;

/// Report a source-level error with file location and caret.
pub fn report_source_error(path: &str, source: &str, span: &Span, msg: &str) {
    eprintln!("{}:{}:{}: error: {}", path, span.line, span.col, msg);
    if let Some(src_line) = source.lines().nth(span.line.saturating_sub(1)) {
        eprintln!("    {} | {}", span.line, src_line);
        let indent = 7 + span.col.saturating_sub(1);
        eprintln!("{}^", " ".repeat(indent));
    }
}

/// Report a source-level warning with file location and caret.
pub fn report_source_warning(path: &str, source: &str, span: &Span, msg: &str) {
    eprintln!("{}:{}:{}: warning: {}", path, span.line, span.col, msg);
    if let Some(src_line) = source.lines().nth(span.line.saturating_sub(1)) {
        eprintln!("    {} | {}", span.line, src_line);
        let indent = 7 + span.col.saturating_sub(1);
        eprintln!("{}^", " ".repeat(indent));
    }
}

/// Report a runtime error produced by the interpreter.
pub fn report_runtime_error(path: &str, source: &str, msg: &str, span: Option<&Span>) {
    if let Some(span) = span {
        eprintln!("{}:{}:{}: runtime error: {}", path, span.line, span.col, msg);
        if let Some(src_line) = source.lines().nth(span.line.saturating_sub(1)) {
            eprintln!("    {} | {}", span.line, src_line);
            let indent = 7 + span.col.saturating_sub(1);
            eprintln!("{}^", " ".repeat(indent));
        }
    } else {
        eprintln!("{}: runtime error: {}", path, msg);
    }
}

/// Parse a "lexer/parse error at L:C: reason" string and report it.
pub fn report_parse_error(path: &str, source: &str, msg: &str) {
    let (line, col, reason) = if let Some(rest) = msg.strip_prefix("parse error at ") {
        parse_error_location(rest, msg)
    } else if let Some(rest) = msg.strip_prefix("lexer error at ") {
        parse_error_location(rest, msg)
    } else {
        (1, 1, msg.to_string())
    };

    report_source_error(path, source, &Span { line, col }, &reason);
}

fn parse_error_location(rest: &str, fallback: &str) -> (usize, usize, String) {
    let mut parts = rest.splitn(2, ": ");
    let loc = parts.next().unwrap_or("1:1");
    let mut loc_parts = loc.splitn(2, ':');
    let line: usize = loc_parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let col: usize = loc_parts.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    (line, col, parts.next().unwrap_or(fallback).to_string())
}
