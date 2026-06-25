"""Semantic analysis for Period, including identifier kind resolution."""
from typing import Dict, List, Optional, Set, Tuple

from . import ast_nodes as ast
from .errors import Diagnostic, SourceSpan

BUILTINS: Set[str] = {
    "length",
    "string",
    "number",
    "type",
    "input",
    "show",
}

# Recognized Period type names. They are treated as predefined identifiers and
# highlighted as classes. They do not have to be runtime built-ins.
TYPE_NAMES: Set[str] = {
    "any",
    "never",
    "nothing",
    "boolean",
    "integer",
    "number",
    "string",
    "list",
    "dictionary",
    "function",
    "class",
}

# Token kinds used for LSP semantic highlighting.
TOKEN_FUNCTION = "function"
TOKEN_CLASS = "class"
TOKEN_VARIABLE = "variable"
TOKEN_PARAMETER = "parameter"
TOKEN_PROPERTY = "property"
TOKEN_METHOD = "method"
TOKEN_BUILTIN = "builtin"


class SemanticChecker:
    """Walks the AST and reports undefined variable usages."""

    def __init__(self):
        self.diagnostics: List[Diagnostic] = []
        self.scopes: List[Dict[str, str]] = []
        self.tokens: List[Tuple[SourceSpan, str, bool]] = []
        self.filename: str = "<stdin>"
        self.local_names: Set[str] = set()
        self.imported_names: Dict[str, List[str]] = {}
        self.imported_kinds: Dict[str, Dict[str, str]] = {}
        self.imported_modules: Set[str] = set()

    def check(self, program: ast.Program, filename: str = "<stdin>") -> List[Diagnostic]:
        self.diagnostics = []
        self.tokens = []
        self.scopes = [self._builtin_scope()]
        self.filename = filename
        self.local_names = set()
        self.imported_names = {}
        self.imported_kinds = {}
        self.imported_modules = set()

        # First pass: register all top-level function and class names so they can
        # be referenced before their definition.
        for stmt in program.statements:
            if isinstance(stmt, ast.DefineStmt):
                self.scopes[0][stmt.name] = TOKEN_FUNCTION
                self.local_names.add(stmt.name)
            elif isinstance(stmt, ast.ClassStmt):
                self.scopes[0][stmt.name] = TOKEN_CLASS
                self.local_names.add(stmt.name)

        for stmt in program.statements:
            self._visit_stmt(stmt)

        return self.diagnostics

    def semantic_tokens(self, program: ast.Program, filename: str = "<stdin>") -> List[Tuple[SourceSpan, str, bool]]:
        """Return a list of (span, kind, is_declaration) tuples for semantic highlighting."""
        self.check(program, filename)
        return self.tokens

    def _builtin_scope(self) -> Dict[str, str]:
        scope = {name: TOKEN_BUILTIN for name in BUILTINS}
        for name in TYPE_NAMES:
            scope[name] = TOKEN_CLASS
        return scope

    def _declare(self, name: str, kind: str):
        if self.scopes:
            self.scopes[-1][name] = kind

    def _declare_local(self, name: str, kind: str):
        self._declare(name, kind)
        self.local_names.add(name)

    def _lookup(self, name: str) -> Optional[str]:
        for scope in reversed(self.scopes):
            if name in scope:
                return scope[name]
        return None

    def _is_defined(self, name: str) -> bool:
        return self._lookup(name) is not None

    def _error(self, name: str, span: SourceSpan):
        self.diagnostics.append(
            Diagnostic(
                f"Undefined variable '{name}'.",
                span,
                "error",
            )
        )

    def _validate_type_annotation(self, name: str, span: SourceSpan):
        """Report a diagnostic if the type name is not a known type or class."""
        if self._is_valid_type(name):
            return
        self.diagnostics.append(
            Diagnostic(
                f"Unknown type '{name}'.",
                span,
                "error",
            )
        )

    def _is_valid_type(self, name: str) -> bool:
        """Return True if the name refers to a known type or class."""
        if name in TYPE_NAMES:
            return True
        return self._lookup(name) == TOKEN_CLASS

    def _infer_expr_type(self, expr: ast.Expr) -> Optional[str]:
        """Statically infer the Period type name of an expression, if obvious."""
        if isinstance(expr, ast.NumberLiteral):
            return "number"
        if isinstance(expr, ast.StringLiteral):
            return "string"
        if isinstance(expr, ast.BooleanLiteral):
            return "boolean"
        if isinstance(expr, ast.NothingLiteral):
            return "nothing"
        if isinstance(expr, ast.ListExpr):
            return "list"
        if isinstance(expr, ast.DictExpr):
            return "dictionary"
        if isinstance(expr, ast.NewExpr):
            if isinstance(expr.class_expr, ast.VariableExpr):
                return f"instance of {expr.class_expr.name}"
            return "instance"
        if isinstance(expr, ast.VariableExpr):
            if expr.name in TYPE_NAMES:
                return expr.name
        return None

    def _check_let_type(self, stmt: ast.LetStmt):
        """Report a diagnostic if the initializer doesn't match the declared type."""
        expected = stmt.type_annotation
        if expected == "any":
            return
        actual = self._infer_expr_type(stmt.initializer)
        if actual is None:
            return
        if expected == actual:
            return
        # 'number' accepts integer values.
        if expected == "number" and actual == "integer":
            return
        # Class annotations match instances of that class.
        if actual == f"instance of {expected}":
            return
        self.diagnostics.append(
            Diagnostic(
                f"Type mismatch: expected '{expected}' but got '{actual}'.",
                stmt.initializer.span,
                "error",
            )
        )

    def _add_token(self, span: SourceSpan, kind: str, is_declaration: bool = False):
        self.tokens.append((span, kind, is_declaration))

    def _visit_stmt(self, stmt: ast.Stmt):
        if isinstance(stmt, ast.ExpressionStmt):
            self._visit_expr(stmt.expression)
        elif isinstance(stmt, ast.LetStmt):
            self._visit_expr(stmt.initializer)
            if stmt.type_annotation_span is not None:
                self._add_token(stmt.type_annotation_span, TOKEN_CLASS)
            if stmt.type_annotation is not None:
                self._validate_type_annotation(
                    stmt.type_annotation,
                    stmt.type_annotation_span or stmt.span,
                )
                if self._is_valid_type(stmt.type_annotation):
                    self._check_let_type(stmt)
            self._declare_local(stmt.name, TOKEN_VARIABLE)
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
            self._add_token(stmt.name_span, TOKEN_FUNCTION, is_declaration=True)
            self._declare_local(stmt.name, TOKEN_FUNCTION)
            self._scope(self._visit_function_body, stmt)
        elif isinstance(stmt, ast.ClassStmt):
            self._add_token(stmt.name_span, TOKEN_CLASS, is_declaration=True)
            self._declare_local(stmt.name, TOKEN_CLASS)
            self._visit_class_body(stmt)
        elif isinstance(stmt, ast.ImportStmt):
            self._visit_import(stmt)
        elif isinstance(stmt, ast.InitStmt):
            # Init outside a class body is a parse/runtime concern; nothing to check here.
            pass

    def _visit_import(self, stmt: ast.ImportStmt):
        from .module_loader import resolve_module

        for module_path, module_span in zip(stmt.module_paths, stmt.module_spans):
            self._add_token(module_span, TOKEN_CLASS)
            resolved = resolve_module(module_path, self.filename)
            if resolved is None:
                self.diagnostics.append(
                    Diagnostic(
                        f"Module '{module_path}' not found.",
                        module_span,
                        "error",
                    )
                )
                continue

            self.imported_modules.add(module_path)

            if isinstance(resolved, str):
                exports = self._collect_builtin_module_exports(resolved)
            else:
                exports = self._collect_file_module_exports(resolved)

            for export_name, kind in exports:
                modules = self.imported_names.setdefault(export_name, [])
                if module_path not in modules:
                    modules.append(module_path)
                self.imported_kinds.setdefault(export_name, {})[module_path] = kind

                if len(modules) == 1 and export_name not in self.local_names:
                    self._declare(export_name, kind)
                elif len(modules) > 1:
                    # Name is ambiguous across imports; remove it from scopes.
                    for scope in self.scopes:
                        scope.pop(export_name, None)

    def _collect_builtin_module_exports(self, name: str) -> List[Tuple[str, str]]:
        import importlib

        exports: List[Tuple[str, str]] = []
        try:
            mod = importlib.import_module(f"period.stdlib.{name}")
        except Exception as exc:
            self.diagnostics.append(
                Diagnostic(
                    f"Could not load built-in module '{name}': {exc}.",
                    SourceSpan(1, 1, 1),
                    "error",
                )
            )
            return exports

        for export_name, entry in getattr(mod, "EXPORTS", {}).items():
            if isinstance(entry, tuple):
                value = entry[0]
            else:
                value = entry
            kind = TOKEN_FUNCTION if callable(value) else TOKEN_VARIABLE
            exports.append((export_name, kind))
        return exports

    def _collect_file_module_exports(self, path) -> List[Tuple[str, str]]:
        from .lexer import Lexer
        from .parser import Parser

        exports: List[Tuple[str, str]] = []
        source = path.read_text(encoding="utf-8")
        lexer = Lexer(source, str(path))
        tokens = lexer.scan()
        diagnostics = list(lexer.diagnostics)

        parser = Parser(tokens, source, str(path))
        program = parser.parse()
        diagnostics.extend(parser.diagnostics)

        if diagnostics:
            for diag in diagnostics:
                self.diagnostics.append(diag)
            return exports

        for s in program.statements:
            if isinstance(s, ast.DefineStmt):
                exports.append((s.name, TOKEN_FUNCTION))
            elif isinstance(s, ast.ClassStmt):
                exports.append((s.name, TOKEN_CLASS))
            elif isinstance(s, ast.LetStmt):
                exports.append((s.name, TOKEN_VARIABLE))
        return exports

    def _visit_stmts(self, statements: List[ast.Stmt]):
        for stmt in statements:
            self._visit_stmt(stmt)

    def _visit_function_body(self, stmt: ast.DefineStmt):
        for param, param_type in zip(stmt.parameters, stmt.parameter_types):
            self._declare_local(param, TOKEN_PARAMETER)
        for param_type, param_type_span in zip(
            stmt.parameter_types, stmt.parameter_type_spans
        ):
            if param_type_span is not None:
                self._add_token(param_type_span, TOKEN_CLASS)
            if param_type is not None:
                self._validate_type_annotation(param_type, param_type_span)
        if stmt.return_type is not None and stmt.return_type_span is not None:
            self._validate_type_annotation(stmt.return_type, stmt.return_type_span)
            self._add_token(stmt.return_type_span, TOKEN_CLASS)
        self._visit_stmts(stmt.body)

    def _visit_class_body(self, stmt: ast.ClassStmt):
        for member in stmt.body:
            if isinstance(member, ast.InitStmt):
                self._scope(self._visit_init_body, member)
            elif isinstance(member, ast.DefineStmt):
                self._add_token(member.name_span, TOKEN_METHOD, is_declaration=True)
                self._scope(self._visit_method_body, member)

    def _visit_init_body(self, stmt: ast.InitStmt):
        self._declare_local("this", TOKEN_VARIABLE)
        for param, param_type in zip(stmt.parameters, stmt.parameter_types):
            self._declare_local(param, TOKEN_PARAMETER)
        for param_type, param_type_span in zip(
            stmt.parameter_types, stmt.parameter_type_spans
        ):
            if param_type_span is not None:
                self._add_token(param_type_span, TOKEN_CLASS)
            if param_type is not None:
                self._validate_type_annotation(param_type, param_type_span)
        self._visit_stmts(stmt.body)

    def _visit_method_body(self, stmt: ast.DefineStmt):
        self._declare_local("this", TOKEN_VARIABLE)
        for param, param_type in zip(stmt.parameters, stmt.parameter_types):
            self._declare_local(param, TOKEN_PARAMETER)
        for param_type, param_type_span in zip(
            stmt.parameter_types, stmt.parameter_type_spans
        ):
            if param_type_span is not None:
                self._add_token(param_type_span, TOKEN_CLASS)
            if param_type is not None:
                self._validate_type_annotation(param_type, param_type_span)
        if stmt.return_type is not None and stmt.return_type_span is not None:
            self._validate_type_annotation(stmt.return_type, stmt.return_type_span)
            self._add_token(stmt.return_type_span, TOKEN_CLASS)
        self._visit_stmts(stmt.body)

    def _scope(self, fn, *args):
        self.scopes.append({})
        try:
            fn(*args)
        finally:
            self.scopes.pop()

    def _visit_set_target(self, target: ast.Expr):
        if isinstance(target, ast.VariableExpr):
            if not self._is_defined(target.name):
                self._error(target.name, target.span)
        elif isinstance(target, ast.PropertyExpr):
            self._visit_expr(target.object)
            self._add_token(target.span, TOKEN_PROPERTY)
        else:
            self._visit_expr(target)

    def _visit_expr(self, expr: ast.Expr):
        if isinstance(expr, ast.VariableExpr):
            if not self._is_defined(expr.name):
                if (
                    expr.name in self.imported_names
                    and len(self.imported_names[expr.name]) > 1
                    and expr.name not in self.local_names
                ):
                    modules = ", ".join(self.imported_names[expr.name])
                    self.diagnostics.append(
                        Diagnostic(
                            f"Ambiguous name '{expr.name}' imported from multiple modules: {modules}. Use '{expr.name} from <module>'.",
                            expr.span,
                            "error",
                        )
                    )
                else:
                    self._error(expr.name, expr.span)
            kind = self._lookup(expr.name) or TOKEN_VARIABLE
            self._add_token(expr.span, kind)
        elif isinstance(expr, ast.QualifiedExpr):
            self._visit_qualified_expr(expr)
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
        elif isinstance(expr, ast.PropertyExpr):
            self._visit_expr(expr.object)
            if isinstance(expr.object, ast.VariableExpr):
                kind = self._lookup(expr.object.name)
                if kind in (TOKEN_BUILTIN, TOKEN_FUNCTION, TOKEN_CLASS):
                    self.diagnostics.append(
                        Diagnostic(
                            f"Cannot access property on {kind}.",
                            expr.span,
                            "error",
                        )
                    )
            self._add_token(expr.span, TOKEN_PROPERTY)
        elif isinstance(expr, ast.NewExpr):
            self._visit_expr(expr.class_expr)
            for arg in expr.arguments:
                self._visit_expr(arg)
        elif isinstance(expr, ast.TellExpr):
            self._visit_expr(expr.object)
            self._add_token(expr.span, TOKEN_METHOD)
            for arg in expr.arguments:
                self._visit_expr(arg)

    def _visit_qualified_expr(self, expr: ast.QualifiedExpr):
        if expr.module not in self.imported_modules:
            self.diagnostics.append(
                Diagnostic(
                    f"Module '{expr.module}' has not been imported.",
                    expr.module_span or expr.span,
                    "error",
                )
            )
            return

        kinds = self.imported_kinds.get(expr.name, {})
        if expr.module not in kinds:
            self.diagnostics.append(
                Diagnostic(
                    f"Module '{expr.module}' does not export '{expr.name}'.",
                    expr.span,
                    "error",
                )
            )
            return

        self._add_token(expr.name_span or expr.span, kinds[expr.module])
        self._add_token(expr.module_span or expr.span, TOKEN_CLASS)
