"""Benchmark simple "Hello, World!" runtime across Period, Python and Node.js.

Run with:
    python docs/benchmark.py

Outputs a JSON object that can be pasted into docs/index.html for the
performance chart.
"""
from __future__ import annotations

import json
import subprocess
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
PERIOD_EXE = REPO / "period" / "target" / "debug" / "period.exe"

PROGRAMS = {
    "Period": (
        PERIOD_EXE,
        'show "Hello, World!".',
        ".period",
    ),
    "Python": (
        "python",
        'print("Hello, World!")',
        ".py",
    ),
    "Node.js": (
        "node",
        'console.log("Hello, World!");',
        ".js",
    ),
}


def run_benchmark(name: str, executable: str | Path, source: str, ext: str, runs: int = 15) -> float:
    with tempfile.NamedTemporaryFile(mode="w", suffix=ext, delete=False) as f:
        f.write(source)
        f.flush()
        src_path = Path(f.name)

    cmd = [str(executable), str(src_path)] if ext != ".py" and ext != ".js" else [str(executable), str(src_path)]
    # Warm-up run.
    subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

    times: list[float] = []
    for _ in range(runs):
        start = time.perf_counter()
        subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        times.append(time.perf_counter() - start)

    src_path.unlink(missing_ok=True)
    # Drop the slowest outlier and average the rest.
    times.sort()
    return sum(times[:-1]) / len(times[:-1]) * 1000


def main() -> None:
    if not PERIOD_EXE.exists():
        print(f"Period executable not found at {PERIOD_EXE}")
        return

    results: dict[str, float] = {}
    for name, (exe, source, ext) in PROGRAMS.items():
        if isinstance(exe, str):
            # Verify interpreter exists.
            if subprocess.run([exe, "--version"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode != 0:
                print(f"{exe} not available, skipping {name}")
                continue
        results[name] = run_benchmark(name, exe, source, ext)

    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
