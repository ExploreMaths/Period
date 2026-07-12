# Changelog

## Unreleased

### Added

### Changed

### Fixed

## 2.0.0-beta.7 (2026-07-12)

### Fixed

- Lexer no longer panics or misreads tokens on lines containing multi-byte characters (e.g. Chinese identifiers): `read_identifier` and `read_number` collected token text by slicing the source line with character-based column numbers used as byte indices, and now collect characters directly instead. (issue #7)
- The LSP server no longer offers completions while typing inside a `--` comment. (issue #8)

### Quality

- Added regression tests for issues #5–#8: parse errors report a source location instead of a Rust panic, compact `show("...")` calls exit with code 0, non-ASCII identifiers compile and run, and comment lines return an empty completion list.

## 2.0.0 (2026-07-07)

This release consolidates the six 2.0.0-beta releases into the first stable 2.x line. The interpreter was completely redesigned between 1.x and 2.0.0-beta.1, and the beta cycle added a bytecode compiler/VM, a package manager, and finally a native Cranelift JIT compiler.

### Breaking redesign (from 2.0.0-beta.1)

- Unified syntax and runtime semantics under a single tree-walking Rust interpreter; removed the C/JIT backend, cached DLL generation, and bundled TCC.
- Restored case-insensitive keywords (`let`, `Let`, and `LET` are equivalent).
- Unified property access and method call syntax to `the <property> of <object>` and `tell <object> to <method>`.
- Relative imports now require POSIX-style paths (`./helper`, `../utils/helper`).
- Added a static type checker that validates annotated parameters, return values, and variables before execution.
- Structured error values expose `message`, `line`, and `col` properties.

### Added

- **Native JIT compilation (beta.5–beta.6)**:
  - Cranelift-based integer-only JIT with closed-form and periodic loop optimisations.
  - Generic Cranelift JIT compiler (`period/src/jit_generic.rs`) that compiles nearly all Period programs to native code by default, with automatic fallback to the bytecode VM for unsupported constructs.
  - Fast-path numeric loops in the `period.exe` wrapper for common summation/counting patterns.
  - `period_run` now routes all programs through the JIT path first before falling back to the bytecode VM.
- **Bytecode compiler and VM (beta.4)**: all supported language constructs are compiled to a compact instruction set and executed on a stack machine, including nested functions with upvalues, classes, `for`-in iteration, `try`/`catch`, `import`/`export`, qualified module access, and file I/O.
- **Package manager (beta.3)**: `period init`, `period install`, `period update`, and `period publish` with registry resolution, version constraints, transitive dependencies, and `period.lock` generation.
- **Optional compact syntax** that coexists with the English forms: `obj.prop`, `obj.method(args)`, `f(args)`, and `new Class(args)`.
- New `error` built-in for raising runtime errors with a custom message.
- New `integer with <value>` and `boolean with <value>` built-in conversion functions.
- Expanded standard library: `string`, `list`, `path`, and `test` modules.
- Static type-checker signatures for `math`, `string`, `random`, `time`, `path`, and `test` modules.
- Simple return-type inference for functions without an explicit `returns` annotation.
- Source spans attached to list, dictionary, call, index, property, `new`, `tell`, `qualified`, unary, and literal expressions.
- Arbitrary-precision integer support using `num-bigint`.
- New examples: `examples/compact.period`, `examples/tests.period`, and `examples/factorial.period`.
- Benchmarking infrastructure: `docs/benchmark_long.py` with server-mode benchmarking, SVG chart rendering, and a Chart.js homepage chart.

### Changed

- `Value::Integer` is now split into a tagged small-integer variant (`i64`) and a `BigInt` fallback, with hot arithmetic paths mutating small integers in place.
- VM local-variable slots moved from `Rc<RefCell<Value>>` to plain `Vec<Value>`; only captured variables are boxed. Large `Value` variants are now heap-allocated.
- Static type inference for unannotated integer arithmetic now infers `integer` instead of `number`.
- `period publish` no longer supports `--push`/`--remote`/`--message`; it only writes files to the local registry directory and prints manual git steps.
- `Value::Class` methods are stored as first-class callable values so the bytecode VM and tree-walking interpreter share the same class representation.

### Fixed

- Lexer, parser, and interpreter no longer panic on invalid input; parse and runtime errors are reported once with source locations.
- Function and method call arguments are parsed as full expressions separated by commas.
- Integer arithmetic uses `BigInt` and no longer overflows.
- Mixed `integer`/`number` equality and comparisons use exact integer arithmetic when possible.
- `0 ** -1` reports `Division by zero` instead of returning `inf`.
- Circular imports and self-imports are detected and reported with source locations.
- Class fields assigned inside `init` are visible to the static type checker.
- Accessing a method as a property is a static and runtime error.
- Local module imports are validated statically; missing files and import cycles report source locations.
- Standard-library source and interface modules are recognised by both the runtime and the static checker.
- Functions and methods with explicit return types are checked for return coverage on every control-flow path.
- LSP diagnostics and hover use correct lexical scope and source positions.
- Terminal error caret is aligned with the exact source column and underlines the whole quoted token.
- Duplicate-definition warnings are emitted once per symbol and point to correct source locations.
- `cargo clippy` now runs clean (remaining `result_large_err` lints are allowed at crate root).

### Quality

- Codebase modularized into focused modules: `value`, `types`, `environment`, `builtins`, `reporting`, `semantic`, and `type_checker`.
- Full test suite: 69 Rust unit tests, 55 Python integration tests, 15 example programs, and VS Code grammar tests.

### Documentation

- Rewrote `docs/docs.html`, `docs/examples.html`, and `docs/about.html` to match the redesigned language.
- Added a Compact Syntax section to `docs/docs.html`.
- Updated `README.md` and `docs/docs.html` to describe arbitrary-precision integers, exact comparison, boolean-only conditions, and first-error-only parsing.
- Updated `docs/about.html` and `README.md` to present Period as an educational language with optional compact syntax.
- Updated the VS Code extension README and LSP hover docs to use POSIX-style relative imports.

## 1.0.6 (2026-07-01)

### What's new

- Removed `.` from the LSP completion trigger characters, so typing a statement terminator no longer pops up unwanted autocomplete suggestions in the VS Code extension.
- Split GitHub Release note generation into its own workflow to prevent the same notes from being appended three times (once per platform job).

### Full commit

`v1.0.6`

## 1.0.5 (2026-07-01)

### What's new

- Fixed local/relative module imports (`import .helper.`) being incorrectly rejected by the pre-runtime semantic check introduced in 1.0.4.
- Fixed the REPL and file mode crashing with no output when given lexer-invalid input such as `..`; they now report a friendly parse error instead.
- Added a cross-platform CI workflow (`.github/workflows/ci.yml`) that runs `cargo test`, all example programs, and an expanded integration test suite on every push and pull request.

### Full commit

`v1.0.5`

## 1.0.4 (2026-07-01)

> **Note:** The C/JIT backend, bundled TCC, and numeric fast-path described in this release were removed in the Unreleased redesign. The current implementation uses a single Rust interpreter for all programs.

### What's new

- Added Linux `.deb` (`period-{version}-amd64.deb`) and macOS `.pkg` (`period-{version}-macos.pkg`) installers to GitHub Releases.
- Added Linux and macOS release tarballs to GitHub Releases, shipping the `period` binary, standard library, docs, examples, README, and license.
- Added a Windows portable ZIP archive (`period-{version}-windows.zip`) to GitHub Releases alongside the installer and VS Code extension.
- Windows installer now builds the full distribution via `scripts/build_dist.py`, including the fast-path wrapper, `period-core.exe`, bundled TCC, and standard library.
- JIT compiler auto-selection: numeric programs are compiled to a cached DLL using the best available C compiler (Clang, GCC, or MSVC), falling back to the bundled TCC.
- General 8x loop unrolling for pure numeric `while` loops.
- New `benchmark_long.py` workload: count numbers divisible by 3 or 5.
- Website copy updated to match the current Rust/JIT/LSP implementation.
- Package manager documentation removed from the site; the feature remains experimental.

## 1.0.3 (2026-06-28)

> **Note:** The C/JIT backend mentioned in this release was removed in the Unreleased redesign.

### What's new

- Runtime and compile-time errors now print the offending source line with a caret (`^`), similar to Python.
- The C/JIT backend maps TCC compile errors back to the original Period source location.
- Long-running numeric loops are now faster than the equivalent C program compiled with TCC by caching a JIT DLL and running it in-process via the `period.exe` wrapper.
- Updated `docs/index.html` performance chart to use `benchmark_long.py` results with 1M and 5M iteration bars.

## 1.0.2 (2026-06-27)

> **Note:** Keyword case enforcement mentioned in this release was later reverted; the current implementation treats keywords as case-insensitive.

### What's new

- Bumped the VS Code: extension to v1.0.2.
- Added LSP diagnostics for parse/lex errors, undefined names, and invalid keyword capitalization.
- Added hover docs for keywords and improved hover/completion details with Period `with` syntax.
- Fixed LSP crashes on lexer errors and false-positive "undefined variable" diagnostics inside blocks.
- Enforced lowercase keywords and restricted plain imports to built-in/stdlib modules.
- Exposed built-in modules as loadable `stdlib/` `.period` wrappers and added `.periodi` interface files.
- Added `...` placeholder expression/body for stub/interface files.
- Allowed docstrings without a trailing `.` inside block bodies.
- Improved VS Code: syntax highlighting for module names, exported functions, and keyword capitalization.
- Fixed lexer panic on Windows CRLF line endings.

## 1.0.1 (2026-06-27)

> **Note:** The C/JIT numeric fast-path and keyword case enforcement mentioned in this release were removed in the Unreleased redesign.

### What's new

- Added a Rust-based LSP server (`period --lsp`).
- Hover information for variables, functions, classes, modules, and built-ins.
- Auto-completion for local symbols, built-ins, and module exports.
- Simple type inference based on function return-type annotations and literal kinds.
- Docstrings are now preserved and shown in hover popups.
- Diagnostics for parse/lex errors and undefined names.
- Fixed LSP server startup when the VS Code: client passes extra stdio flags.
- Fixed lexer panic on Windows CRLF line endings.
- Numeric fast-path now falls back to the interpreter when `rustc` is not available.
- Fixed false-positive "undefined variable" diagnostics for variables defined earlier in the same block (e.g. inside `while`/`if` bodies).
- Improved hover: variable/function signature is shown as a syntax-highlighted `period` code block on the first line, variables defined inside blocks (e.g. inside `while`) also show hover, and keywords like `show` now have hover docs.
- Fixed hover token-length matching for multi-character keywords (`show`, `returns`, etc.).
- Restored `period/stdlib/` as a directory of loadable modules. `list` and `text` are implemented as `.period` source files; `math`, `random`, `string`, and `time` are native modules with `.periodi` stub files for documentation and IDE support.
- Added support for `.periodi` interface files: they are parsed by the LSP for completions/hover but ignored by the runtime, similar to Python `.pyi` stubs. Function bodies can be written as `...`.
- Fixed syntax gaps found in docs.html audit:
  - Keywords and reserved words must be lowercase; any capitalization (e.g. `Let` or `LET`) is a lexer error.
  - `true`/`false` are now boolean values and `nothing` is the nothing value, not numbers.
  - Zero-argument built-ins like `input` can be used without `with`.
  - `import` with a plain name resolves to built-in or standard-library modules only; local files must be imported with a relative path (`.helper`, `..helper`) or from a `lib/` folder.
  - Updated the grammar reference and module list in `docs/docs.html` to match the Rust implementation.
- Fixed an LSP server crash when lexing files containing invalid keyword casing; such errors are now reported as diagnostics instead of crashing the server.
- Updated VS Code: syntax highlighting so module names in `import` / `from` statements are colored green, and common functions exported by built-in/standard-library modules (e.g. `sin`, `upper`, `sum`) are colored yellow.
- The Windows installer now registers `.periodi` files as "Period Interface File" with the Period icon and open command.
- The VS Code: extension now associates `.periodi` files with the Period language and contributes a "Period Icons" file icon theme for `.period` / `.periodi` files.
- Zero-argument user-defined functions are now auto-called when used as values, matching zero-argument built-ins.
- A leading string literal inside a block is now treated as a docstring and does not require a trailing `.`, enabling stub/interface files to declare documentation before `...`.
- `...` can now be used as an expression placeholder, so `.periodi` stubs can write `let pi be ... .` as well as `...` statement bodies.
- The installer now uninstalls the old VS Code extension before installing the new one, preventing version-downgrade issues.

### Full commit

`760a43e`

## 1.0.0 (2026-06-27)

> **Note:** The numeric fast-path compiler was removed in the Unreleased redesign; all programs now run through the single Rust interpreter.

### What's new

- First stable release of Period.
- Complete rewrite of the language implementation in Rust under the `period/` crate.
- Lexer, parser, interpreter, numeric fast-path compiler, CLI, and Windows installer are all Rust-based.
- Removed the previous Python implementation and build tooling.
- Numeric programs are automatically translated to Rust and compiled to native code.
- Full interpreter support for strings, lists, dictionaries, classes, functions, imports, and built-in modules.

### Full commit

`04e01e9`
