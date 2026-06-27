"""Experimental C backend for numeric/loop Period code.

Translates a subset of Period to C, compiles with the host C compiler,
and runs the resulting executable. Falls back to the interpreter for
unsupported constructs.
"""
import os
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

from . import ast_nodes as ast


class CBackendError(Exception):
    """Raised when the C backend cannot handle a construct."""

    pass


_C_RUNTIME = r'''#include <stdio.h>
#include <stdint.h>

typedef int64_t period_int;
typedef double period_float;

static period_int period_abs(period_int x) { return x < 0 ? -x : x; }

static period_int _period_ipow(period_int base, period_int exp) {
    period_int result = 1;
    while (exp > 0) {
        if (exp & 1) result *= base;
        base *= base;
        exp >>= 1;
    }
    return result;
}
'''


class _CTranspiler:
    """Generate C source from a Period AST."""

    def __init__(self):
        self.functions: List[str] = []
        self.main_body: List[str] = []
        self._indent = "    "
        self._local_vars: set = set()

    def transpile(self, program: ast.Program) -> str:
        for stmt in program.statements:
            self._emit_top_level(stmt)
        return self._assemble()

    def _assemble(self) -> str:
        lines = [_C_RUNTIME]
        if self.functions:
            lines.extend(self.functions)
        lines.append("int main(void) {")
        lines.extend(self.main_body)
        lines.append("    return 0;")
        lines.append("}")
        return "\n".join(lines)

    def _emit_top_level(self, stmt: ast.Stmt):
        if isinstance(stmt, ast.DefineStmt):
            self.functions.append(self._transpile_function(stmt))
        elif isinstance(stmt, ast.ClassStmt):
            raise CBackendError("Classes are not supported by the C backend.")
        elif isinstance(stmt, ast.ImportStmt):
            raise CBackendError("Imports are not supported by the C backend.")
        else:
            self._emit_block_stmt(stmt, self.main_body, 1)

    def _transpile_function(self, stmt: ast.DefineStmt) -> str:
        params = ", ".join(f"period_int {p}" for p in stmt.parameters)
        body_lines: List[str] = []
        body_lines.append(f"static __forceinline period_int {stmt.name}({params}) {{")
        local_vars = set(stmt.parameters)
        for s in stmt.body:
            self._emit_block_stmt(s, body_lines, 1, local_vars)
        if not self._ends_with_return(stmt.body):
            body_lines.append("    return 0;")
        body_lines.append("}")
        return "\n".join(body_lines)

    def _ends_with_return(self, body: List[ast.Stmt]) -> bool:
        if not body:
            return False
        return isinstance(body[-1], ast.ReturnStmt)

    def _emit_block_stmt(
        self,
        stmt: ast.Stmt,
        out: List[str],
        level: int,
        local_vars: Optional[set] = None,
    ):
        if local_vars is None:
            local_vars = set()
        indent = self._indent * level
        if isinstance(stmt, ast.ExpressionStmt):
            expr = self._transpile_expr(stmt.expression)
            out.append(f"{indent}{expr};")
        elif isinstance(stmt, ast.LetStmt):
            expr = self._transpile_expr(stmt.initializer)
            out.append(f"{indent}period_int {stmt.name} = {expr};")
            local_vars.add(stmt.name)
        elif isinstance(stmt, ast.SetStmt):
            compound = self._try_compound_assignment(stmt.target, stmt.value)
            if compound:
                out.append(f"{indent}{compound};")
            else:
                target = self._transpile_expr(stmt.target)
                value = self._transpile_expr(stmt.value)
                out.append(f"{indent}{target} = {value};")
        elif isinstance(stmt, ast.ShowStmt):
            expr = self._transpile_expr(stmt.expression)
            out.append(f"{indent}printf(\"%lld\\n\", (long long){expr});")
        elif isinstance(stmt, ast.IfStmt):
            cond = self._transpile_expr(stmt.condition)
            out.append(f"{indent}if ({cond}) {{")
            for s in stmt.then_branch:
                self._emit_block_stmt(s, out, level + 1, local_vars)
            if stmt.else_branch:
                out.append(f"{indent}}} else {{")
                for s in stmt.else_branch:
                    self._emit_block_stmt(s, out, level + 1, local_vars)
            out.append(f"{indent}}}")
        elif isinstance(stmt, ast.WhileStmt):
            cond = self._transpile_expr(stmt.condition)
            out.append(f"{indent}while ({cond}) {{")
            for s in stmt.body:
                self._emit_block_stmt(s, out, level + 1, local_vars)
            out.append(f"{indent}}}")
        elif isinstance(stmt, ast.ForStmt):
            iterable = self._try_numeric_range(stmt.iterable)
            if iterable is None:
                raise CBackendError("Only 'range' iterables are supported by the C backend.")
            start, stop, step = iterable
            var = stmt.variable
            local_vars.add(var)
            out.append(f"{indent}for (period_int {var} = {start}; {var} < {stop}; {var} += {step}) {{")
            for s in stmt.body:
                self._emit_block_stmt(s, out, level + 1, local_vars)
            out.append(f"{indent}}}")
        elif isinstance(stmt, ast.ReturnStmt):
            if stmt.value is not None:
                expr = self._transpile_expr(stmt.value)
                out.append(f"{indent}return {expr};")
            else:
                out.append(f"{indent}return 0;")
        else:
            raise CBackendError(f"Statement type {type(stmt).__name__} is not supported by the C backend.")

    def _try_compound_assignment(self, target: ast.Expr, value: ast.Expr) -> Optional[str]:
        if not isinstance(target, ast.VariableExpr):
            return None
        if not isinstance(value, ast.BinaryExpr):
            return None
        if value.operator == "**":
            return None
        if not isinstance(value.left, ast.VariableExpr) or value.left.name != target.name:
            return None
        op_map = {"+": "+=", "-": "-=", "*": "*=", "/": "/=", "%": "%="}
        if value.operator not in op_map:
            return None
        right = self._transpile_expr(value.right)
        return f"{target.name} {op_map[value.operator]} {right}"

    def _try_numeric_range(self, expr: ast.Expr) -> Optional[Tuple[str, str, str]]:
        """If expr is a call to range with numeric args, return (start, stop, step) C expressions."""
        if not isinstance(expr, ast.CallExpr):
            return None
        callee = expr.callee
        if not isinstance(callee, ast.VariableExpr) or callee.name != "range":
            return None
        args = [self._transpile_expr(a) for a in expr.arguments]
        if len(args) == 1:
            return "0", args[0], "1"
        if len(args) == 2:
            return args[0], args[1], "1"
        if len(args) == 3:
            return args[0], args[1], args[2]
        return None

    def _transpile_expr(self, expr: ast.Expr) -> str:
        if isinstance(expr, ast.NumberLiteral):
            if isinstance(expr.value, float) and not expr.value.is_integer():
                return repr(expr.value)
            return repr(int(expr.value))
        if isinstance(expr, ast.BooleanLiteral):
            return "1" if expr.value else "0"
        if isinstance(expr, ast.NothingLiteral):
            return "0"
        if isinstance(expr, ast.VariableExpr):
            return expr.name
        if isinstance(expr, ast.BinaryExpr):
            left = self._transpile_expr(expr.left)
            right = self._transpile_expr(expr.right)
            op = expr.operator
            if op == "and":
                op = "&&"
            elif op == "or":
                op = "||"
            elif op == "==":
                op = "=="
            elif op == "!=":
                op = "!="
            elif op == "**":
                # Use integer exponentiation helper.
                return f"_period_ipow(({left}), ({right}))"
            return f"({left} {op} {right})"
        if isinstance(expr, ast.UnaryExpr):
            operand = self._transpile_expr(expr.operand)
            if expr.operator == "not":
                return f"(!{operand})"
            return f"({expr.operator}{operand})"
        if isinstance(expr, ast.CallExpr):
            callee = self._transpile_expr(expr.callee)
            args = ", ".join(self._transpile_expr(a) for a in expr.arguments)
            return f"{callee}({args})"
        if isinstance(expr, ast.IndexExpr):
            obj = self._transpile_expr(expr.object)
            idx = self._transpile_expr(expr.index)
            return f"({obj})[{idx}]"
        if isinstance(expr, ast.PropertyExpr):
            obj = self._transpile_expr(expr.object)
            return f"({obj}).{expr.name}"
        if isinstance(expr, ast.NewExpr):
            raise CBackendError("'new' expressions are not supported by the C backend.")
        if isinstance(expr, ast.TellExpr):
            raise CBackendError("'tell' expressions are not supported by the C backend.")
        if isinstance(expr, ast.QualifiedExpr):
            raise CBackendError("Module-qualified names are not supported by the C backend.")
        if isinstance(expr, ast.ListExpr):
            raise CBackendError("Lists are not supported by the C backend.")
        if isinstance(expr, ast.DictExpr):
            raise CBackendError("Dictionaries are not supported by the C backend.")
        if isinstance(expr, ast.StringLiteral):
            raise CBackendError("Strings are not supported by the C backend.")
        if isinstance(expr, ast.InputExpr):
            raise CBackendError("Input is not supported by the C backend.")
        raise CBackendError(f"Expression type {type(expr).__name__} is not supported by the C backend.")


def _find_windows_kits() -> Tuple[Optional[str], Optional[str]]:
    base = Path(r"C:\Program Files (x86)\Windows Kits\10")
    if not base.exists():
        base = Path(r"C:\Program Files\Windows Kits\10")
    if not base.exists():
        return None, None
    include_root = base / "Include"
    lib_root = base / "Lib"
    if not include_root.exists() or not lib_root.exists():
        return None, None
    versions = [d.name for d in include_root.iterdir() if d.is_dir()]
    if not versions:
        return None, None
    versions.sort(key=lambda s: tuple(int(x) for x in s.split(".")), reverse=True)
    version = versions[0]
    include = str(include_root / version)
    lib = str(lib_root / version)
    return include, lib


def find_cl() -> Optional[Path]:
    """Locate a usable MSVC cl.exe on Windows."""
    if sys.platform != "win32":
        return shutil.which("gcc") or shutil.which("clang") or None
    candidates = [
        Path(r"C:\Program Files (x86)\Microsoft Visual Studio"),
        Path(r"C:\Program Files\Microsoft Visual Studio"),
    ]
    for root in candidates:
        if not root.exists():
            continue
        for cl in root.rglob(r"VC\Tools\MSVC\*\bin\Hostx64\x64\cl.exe"):
            return cl
    return None


def compile_c(c_source: str, output_exe: Path) -> Tuple[bool, str]:
    """Compile generated C source to an executable."""
    cl = find_cl()
    if cl is None:
        return False, "No C compiler found. Install MSVC, gcc, or clang."

    with tempfile.NamedTemporaryFile("w", suffix=".c", delete=False) as f:
        f.write(c_source)
        c_path = Path(f.name)

    try:
        if sys.platform == "win32" and "cl.exe" in str(cl).lower():
            include_dir = (cl.parents[3] / "include").as_posix()
            lib_dir = (cl.parents[3] / "lib" / "x64").as_posix()
            win_include, win_lib = _find_windows_kits()
            if not win_include or not win_lib:
                return False, "Windows SDK not found."
            cmd = [
                str(cl),
                "/O2",
                f"/I{include_dir}",
                f"/I{win_include}/ucrt",
                f"/I{win_include}/um",
                f"/I{win_include}/shared",
                f"/Fe:{output_exe.as_posix()}",
                c_path.as_posix(),
                "/link",
                f"/LIBPATH:{lib_dir}",
                f"/LIBPATH:{win_lib}/ucrt/x64",
                f"/LIBPATH:{win_lib}/um/x64",
            ]
            env = os.environ.copy()
            env["MSYS2_ARG_CONV_EXCL"] = "*"
            proc = subprocess.run(cmd, capture_output=True, text=True, env=env)
        else:
            cmd = [str(cl), "-O2", "-o", str(output_exe), str(c_path)]
            proc = subprocess.run(cmd, capture_output=True, text=True)

        if proc.returncode != 0:
            return False, proc.stdout + proc.stderr
        return True, ""
    finally:
        c_path.unlink(missing_ok=True)


def run_native(program: ast.Program) -> Tuple[bool, str, str]:
    """Transpile, compile, and run the program. Returns (ok, stdout, stderr)."""
    transpiler = _CTranspiler()
    try:
        c_source = transpiler.transpile(program)
    except CBackendError as exc:
        return False, "", str(exc)

    with tempfile.TemporaryDirectory() as tmpdir:
        exe = Path(tmpdir) / "period_native.exe"
        ok, err = compile_c(c_source, exe)
        if not ok:
            return False, "", err
        proc = subprocess.run([str(exe)], capture_output=True, text=True)
        return True, proc.stdout, proc.stderr
