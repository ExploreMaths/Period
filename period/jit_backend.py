"""Numba-based JIT backend for numeric Period programs.

Translates supported programs into Python functions decorated with
``@numba.njit`` and executes them.  The first run pays a compilation cost;
subsequent runs (with ``cache=True``) reuse compiled machine code.
"""
from __future__ import annotations

import hashlib
import os
import tempfile
from typing import Any, List, Set, Tuple

from . import ast_nodes as ast


class JITUnsupportedError(Exception):
    """Raised when the JIT backend cannot handle an AST node."""

    def __init__(self, message: str, node: ast.Node):
        super().__init__(message)
        self.node = node


class _JITTranspiler:
    """AST -> Numba-compatible Python source."""

    def __init__(self):
        self.indent = 0
        self.lines: List[str] = []
        self._scope_stack: List[Set[str]] = []

    def _in_function(self) -> bool:
        return bool(self._scope_stack)

    def _locals(self) -> Set[str]:
        return self._scope_stack[-1] if self._scope_stack else set()

    def _add_local(self, name: str) -> None:
        if self._scope_stack:
            self._scope_stack[-1].add(name)

    def _is_local(self, name: str) -> bool:
        return name in self._locals()

    def _write(self, text: str) -> None:
        self.lines.append("    " * self.indent + text)

    def _expr(self, expr: ast.Expr) -> str:
        if isinstance(expr, ast.NumberLiteral):
            return repr(expr.value)
        if isinstance(expr, ast.BooleanLiteral):
            return "True" if expr.value else "False"
        if isinstance(expr, ast.NothingLiteral):
            return "None"
        if isinstance(expr, ast.VariableExpr):
            return expr.name
        if isinstance(expr, ast.UnaryExpr):
            operand = self._expr(expr.operand)
            if expr.operator == "-":
                return f"(-{operand})"
            if expr.operator == "not":
                return f"(not {operand})"
            raise JITUnsupportedError(f"Unary '{expr.operator}'.", expr)
        if isinstance(expr, ast.BinaryExpr):
            return self._binary(expr)
        if isinstance(expr, ast.CallExpr):
            return self._call(expr)
        raise JITUnsupportedError(
            f"Expression {type(expr).__name__} not supported.", expr
        )

    def _binary(self, expr: ast.BinaryExpr) -> str:
        op = expr.operator
        if op == "and":
            return f"({self._expr(expr.left)} and {self._expr(expr.right)})"
        if op == "or":
            return f"({self._expr(expr.left)} or {self._expr(expr.right)})"
        left = self._expr(expr.left)
        right = self._expr(expr.right)
        if op == "==":
            return f"({left} == {right})"
        if op == "!=":
            return f"({left} != {right})"
        if op == "<=":
            return f"({left} <= {right})"
        if op == ">=":
            return f"({left} >= {right})"
        if op == "**":
            return f"({left} ** {right})"
        return f"({left} {op} {right})"

    def _call(self, expr: ast.CallExpr) -> str:
        if isinstance(expr.callee, ast.VariableExpr) and expr.callee.name == "range":
            args = ", ".join(self._expr(a) for a in expr.arguments)
            return f"range({args})"
        if isinstance(expr.callee, ast.VariableExpr):
            callee = expr.callee.name
        else:
            callee = self._expr(expr.callee)
        args = ", ".join(self._expr(a) for a in expr.arguments)
        return f"{callee}({args})"

    def transpile(self, program: ast.Program) -> str:
        self._write("import numba")
        self._write("")
        for stmt in program.statements:
            if isinstance(stmt, ast.DefineStmt):
                self._compile_function(stmt)
        self._write("@numba.njit(cache=True)")
        self._write("def __period_run():")
        self.indent += 1
        for stmt in program.statements:
            if not isinstance(stmt, ast.DefineStmt):
                self._stmt(stmt)
        self.indent -= 1
        self._write("")
        self._write("__period_run()")
        return "\n".join(self.lines)

    def _stmt(self, stmt: ast.Stmt) -> None:
        if isinstance(stmt, ast.ExpressionStmt):
            self._write(self._expr(stmt.expression))
            return

        if isinstance(stmt, ast.LetStmt):
            init = self._expr(stmt.initializer)
            if stmt.type_annotation and stmt.is_default_initialization:
                init = self._default_for_type(stmt.type_annotation)
            self._add_local(stmt.name)
            self._write(f"{stmt.name} = {init}")
            return

        if isinstance(stmt, ast.SetStmt):
            target = stmt.target
            if isinstance(target, ast.VariableExpr):
                self._write(f"{target.name} = {self._expr(stmt.value)}")
                return
            raise JITUnsupportedError(
                f"Assignment target {type(target).__name__} not supported.", stmt
            )

        if isinstance(stmt, ast.ShowStmt):
            self._write(f"print({self._expr(stmt.expression)})")
            return

        if isinstance(stmt, ast.IfStmt):
            self._write(f"if {self._expr(stmt.condition)}:")
            self.indent += 1
            for s in stmt.then_branch:
                self._stmt(s)
            self.indent -= 1
            if stmt.else_branch:
                self._write("else:")
                self.indent += 1
                for s in stmt.else_branch:
                    self._stmt(s)
                self.indent -= 1
            return

        if isinstance(stmt, ast.WhileStmt):
            self._write(f"while {self._expr(stmt.condition)}:")
            self.indent += 1
            for s in stmt.body:
                self._stmt(s)
            self.indent -= 1
            return

        if isinstance(stmt, ast.ForStmt):
            if not self._is_range_call(stmt.iterable):
                raise JITUnsupportedError(
                    "Only 'for ... in range with ...' is supported.", stmt
                )
            args = ", ".join(self._expr(a) for a in stmt.iterable.arguments)
            self._write(f"for {stmt.variable} in range({args}):")
            self.indent += 1
            for s in stmt.body:
                self._stmt(s)
            self.indent -= 1
            return

        if isinstance(stmt, ast.ReturnStmt):
            value = "None" if stmt.value is None else self._expr(stmt.value)
            self._write(f"return {value}")
            return

        if isinstance(stmt, ast.DefineStmt):
            # Top-level functions are emitted separately before __period_run.
            return

        if isinstance(stmt, ast.BlockStmt):
            for s in stmt.statements:
                self._stmt(s)
            return

        raise JITUnsupportedError(
            f"Statement {type(stmt).__name__} not supported.", stmt
        )

    def _compile_function(self, stmt: ast.DefineStmt) -> None:
        params = ", ".join(stmt.parameters)
        self._write("@numba.njit(cache=True)")
        self._write(f"def {stmt.name}({params}):")
        self.indent += 1
        self._scope_stack.append(set(stmt.parameters))
        for body_stmt in stmt.body:
            self._stmt(body_stmt)
        self.indent -= 1
        self._scope_stack.pop()
        self._write("")

    @staticmethod
    def _default_for_type(type_name: str) -> str:
        defaults = {
            "string": '""',
            "number": "0",
            "integer": "0",
            "boolean": "False",
            "list": "[]",
            "dictionary": "{}",
        }
        return defaults.get(type_name, "None")

    @staticmethod
    def _is_range_call(expr: ast.Expr) -> bool:
        return (
            isinstance(expr, ast.CallExpr)
            and isinstance(expr.callee, ast.VariableExpr)
            and expr.callee.name == "range"
        )


def transpile(program: ast.Program) -> str:
    """Return Numba-compatible Python source."""
    return _JITTranspiler().transpile(program)


def run(program: ast.Program) -> Tuple[bool, List[str], str]:
    """Run *program* with Numba JIT.

    Returns ``(success, output_lines, error_message)``.
    """
    try:
        source = transpile(program)
    except JITUnsupportedError as exc:
        return False, [], str(exc)

    # Write the generated source to a real file so Numba's cache locator can
    # persist compiled machine code across process runs.
    source_hash = hashlib.sha256(source.encode("utf-8")).hexdigest()[:16]
    cache_dir = os.path.join(tempfile.gettempdir(), "period_jit_cache", source_hash)
    os.makedirs(cache_dir, exist_ok=True)
    cache_path = os.path.join(cache_dir, "__period_jit_module.py")
    with open(cache_path, "w", encoding="utf-8") as f:
        f.write(source)

    namespace: dict = {}
    try:
        with open(cache_path, "r", encoding="utf-8") as f:
            code = compile(f.read(), cache_path, "exec")
        exec(code, namespace)
    except Exception as exc:
        return False, [], f"{type(exc).__name__}: {exc}"

    return True, [], ""
