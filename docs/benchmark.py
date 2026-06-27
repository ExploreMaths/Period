"""Benchmark a longer-running computation across Period, Python and Node.js.

Run with:
    python docs/benchmark.py

Outputs a JSON object that can be pasted into docs/index.html for the
performance chart.  The benchmark sums the integers from 1 to N so that
startup overhead is dwarfed by actual execution time.
"""
from __future__ import annotations

import json
import subprocess
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
PERIOD_EXE = REPO / "period" / "target" / "debug" / "period.exe"
N = 1_000_000

PROGRAMS = {
    "Period": (
        PERIOD_EXE,
        f"""let sum be 0.
let i be 1.
while i <= {N} repeat:
    set sum to sum + i.
    set i to i + 1.
show sum.
""",
        ".period",
    ),
    "Python": (
        "python",
        f"""s = 0
for i in range(1, {N + 1}):
    s += i
print(s)
""",
        ".py",
    ),
    "Node.js": (
        "node",
        f"""let s = 0;
for (let i = 1; i <= {N}; i++) {{
    s += i;
}}
console.log(s);
""",
        ".js",
    ),
}


def run_benchmark(name: str, executable: str | Path, source: str, ext: str, runs: int = 5) -> float:
    with tempfile.NamedTemporaryFile(mode="w", suffix=ext, delete=False) as f:
        f.write(source)
        f.flush()
        src_path = Path(f.name)

    cmd = [str(executable), str(src_path)]
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
            if subprocess.run([exe, "--version"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode != 0:
                print(f"{exe} not available, skipping {name}")
                continue
        results[name] = run_benchmark(name, exe, source, ext)

    print(json.dumps(results, indent=2))


if __name__ == "__main__":
    main()
