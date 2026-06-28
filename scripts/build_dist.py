"""Build a distribution of period.exe.

The distribution contains:
  - period.exe        tiny fast-path wrapper (C)
  - period-core.exe   full Rust interpreter / LSP server
  - stdlib/           Period standard library stubs
  - period.ico        Windows icon

Run with:
    python scripts/build_dist.py
"""
from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PERIOD_DIR = ROOT / "period"
TCC_EXE = ROOT / ".tools" / "tcc" / "tcc" / "tcc.exe"
DIST = ROOT / "dist"


def run(cmd: list[str | Path], cwd: Path | None = None) -> None:
    print("$", " ".join(str(c) for c in cmd))
    subprocess.run([str(c) for c in cmd], cwd=cwd, check=True)


def main() -> None:
    if not TCC_EXE.exists():
        print(f"TCC not found at {TCC_EXE}")
        sys.exit(1)

    print("Building release Rust binary...")
    run(["cargo", "build", "--release"], cwd=PERIOD_DIR)

    print("Preparing dist directory...")
    if DIST.exists():
        shutil.rmtree(DIST)
    DIST.mkdir(parents=True)

    shutil.copy(PERIOD_DIR / "target" / "release" / "period.exe", DIST / "period-core.exe")
    shutil.copytree(ROOT / "period" / "stdlib", DIST / "stdlib")
    shutil.copy(ROOT / "assets" / "period.ico", DIST / "period.ico")

    print("Bundling TCC for numeric JIT...")
    shutil.copytree(ROOT / ".tools" / "tcc" / "tcc", DIST / "tcc")

    print("Compiling fast-path wrapper...")
    run([TCC_EXE, PERIOD_DIR / "wrapper.c", "-o", DIST / "period.exe"])

    print(f"Done. Distribution is in {DIST}")


if __name__ == "__main__":
    main()
