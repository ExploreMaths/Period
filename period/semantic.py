"""Simple semantic analysis for Period."""
from typing import List, Set

from . import ast_nodes as ast
from .errors import Diagnostic

BUILTINS: Set[str] = {
    "length",
    "string",
    "number",
    "type",
    "input",
}


class SemanticChecker:
    """Walks the AST and reports undefined variable usages."""

    def __init__(self):
        self.diagnostics: List[Diagnostic] = []
        self.scopes: List[Set[str]] = []

    def check(self, program: ast.Program) -> List[Diagnostic]:
        self.diagnostics = []
        self.scopes = [set(BUILTINS)]

        # First pass: register all top-level function names so they can be
        # referenced before their definition.
        for stmt in program.statements:
            if isinstance(stmt, ast.DefineStmt):
                self.scopes[0].add(stmt.name)

        for stmt in program.statements:
            self._visit_stmt(stmt)

        return self.diagnostics

    def _declare(self, name: str):
        if self.scopes:
            self.scopes[-1].add(name)

    def _is_defined(self, name: str) -> bool:
        for scope in reversed(self.scopes):
            if name in scope:
                return True
        return False

    def _error(self, name: str, span):
        self.diagnostics.append(
            Diagnostic(
                f"Undefined variable '{name}'.",
                span,
                "error",
            )
        )

    def _visit_stmt(self, stmt: ast.Stmt):
        if isinstance(stmt, ast.ExpressionStmt):
            self._visit_expr(stmt.expression)
        elif isinstance(stmt, ast.LetStmt):
            self._visit_expr(stmt.initializer)
            self._declare(stmt.name)
        elif isinstance(stmt, ast.SetStmt):
            self._visit_expr(stmt.value)
            self._visit_set_target(stmt.target)
        elif isinstance(stmt, ast.ShowStmt):
            self._visit_expr(stmt.expression)
        elif isinstance(stmt, ast.BlockStmt):
            self._scope(self._visit_stmts, stmt.statements)
        elif isinstance(stmt, ast.IfStmt):
            self._visit_expr(stmt.condition)
            self._scope(self._visit_stmts, stmt.then_branch)
            self._scope(self._visit_stmts, stmt.else_branch)
        elif isinstance(stmt, ast.WhileStmt):
            self._visit_expr(stmt.condition)
            self._scope(self._visit_stmts, stmt.body)
        elif isinstance(stmt, ast.ReturnStmt):
            if stmt.value is not None:
                self._visit_expr(stmt.value)
        elif isinstance(stmt, ast.DefineStmt):
            self._scope(self._visit_function_body, stmt)

    def _visit_stmts(self, statements: List[ast.Stmt]):
        for stmt in statements:
            self._visit_stmt(stmt)

    def _visit_function_body(self, stmt: ast.DefineStmt):
        for param in stmt.parameters:
            self._declare(param)
        self._visit_stmts(stmt.body)

    def _scope(self, fn, *args):
        self.scopes.append(set())
        try:
            fn(*args)
        finally:
            self.scopes.pop()

    def _visit_set_target(self, target: ast.Expr):
        if isinstance(target, ast.VariableExpr):
            if not self._is_defined(target.name):
                self._error(target.name, target.span)
        else:
            self._visit_expr(target)

    def _visit_expr(self, expr: ast.Expr):
        if isinstance(expr, ast.VariableExpr):
            if not self._is_defined(expr.name):
                self._error(expr.name, expr.span)
        elif isinstance(expr, ast.BinaryExpr):
            self._visit_expr(expr.left)
            self._visit_expr(expr.right)
        elif isinstance(expr, ast.UnaryExpr):
            self._visit_expr(expr.operand)
        elif isinstance(expr, ast.CallExpr):
            self._visit_expr(expr.callee)
            for arg in expr.arguments:
                self._visit_expr(arg)
        elif isinstance(expr, ast.IndexExpr):
            self._visit_expr(expr.object)
            self._visit_expr(expr.index)
        elif isinstance(expr, ast.ListExpr):
            for el in expr.elements:
                self._visit_expr(el)
        elif isinstance(expr, ast.DictExpr):
            for key, value in expr.pairs:
                self._visit_expr(key)
                self._visit_expr(value)
