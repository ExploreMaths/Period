"""Benchmark simple "Hello, World!" startup-to-output time across several languages.

Run with:
    python docs/benchmark.py

Outputs a JSON object that can be pasted into docs/index.html for the
performance chart.
"""
from __future__ import annotations

import json
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
PERIOD_EXE = REPO / "dist" / "period.exe"
TCC_EXE = REPO / ".tools" / "tcc" / "tcc" / "tcc.exe"

Program = tuple[list[str], str, str]

PROGRAMS: dict[str, Program] = {
    "Period": (
        [str(PERIOD_EXE)],
        'show "Hello, World!".',
        ".period",
    ),
    "Python": (
        ["python"],
        'print("Hello, World!")',
        ".py",
    ),
    "Node.js": (
        ["node"],
        'console.log("Hello, World!");',
        ".js",
    ),
    "Perl": (
        ["perl"],
        'print "Hello, World!\\n";',
        ".pl",
    ),
    "PowerShell": (
        ["powershell", "-ExecutionPolicy", "Bypass", "-File"],
        "Write-Host 'Hello, World!'",
        ".ps1",
    ),
    "Bash": (
        ["bash"],
        'echo "Hello, World!"',
        ".sh",
    ),
    "C": (
        [str(TCC_EXE)],
        '#include <stdio.h>\nint main(void) { puts("Hello, World!"); return 0; }',
        ".c",
    ),
}


def run_benchmark(
    name: str,
    command_prefix: list[str],
    source: str,
    ext: str,
    runs: int = 10,
) -> float | None:
    # Sanity check: the first executable in the prefix must exist on PATH or as a file.
    first = command_prefix[0]
    if not (shutil.which(first) or Path(first).exists()):
        print(f"{first} not available, skipping {name}")
        return None

    with tempfile.NamedTemporaryFile(mode="w", suffix=ext, delete=False) as f:
        f.write(source)
        f.flush()
        src_path = Path(f.name)

    # C needs to be compiled once; the timer then measures only the binary runtime.
    if ext == ".c":
        exe_path = src_path.with_suffix(".exe")
        compile_cmd = command_prefix + [str(src_path), "-o", str(exe_path)]
        compile_result = subprocess.run(
            compile_cmd,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
        )
        if compile_result.returncode != 0:
            print(f"Failed to compile {name}:\n{compile_result.stderr}")
            src_path.unlink(missing_ok=True)
            return None
        cmd = [str(exe_path)]
    else:
        cmd = command_prefix + [str(src_path)]

    # Warm-up run.
    subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    times: list[float] = []
    for _ in range(runs):
        start = time.perf_counter()
        subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        times.append(time.perf_counter() - start)

    src_path.unlink(missing_ok=True)
    if ext == ".c":
        exe_path.unlink(missing_ok=True)

    # Drop the slowest outlier and average the rest.
    times.sort()
    return sum(times[:-1]) / len(times[:-1]) * 1000


def main() -> None:
    if not PERIOD_EXE.exists():
        print(f"Period executable not found at {PERIOD_EXE}")
        return

    results: dict[str, float] = {}
    for name, (prefix, source, ext) in PROGRAMS.items():
        value = run_benchmark(name, prefix, source, ext)
        if value is not None:
            results[name] = value

    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
