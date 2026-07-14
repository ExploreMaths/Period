"""Build a distribution of Period.

The distribution contains:
  - period.exe        Period interpreter / LSP server
  - stdlib/           Period standard library
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
DIST = ROOT / "dist"
SET_VERSION = ROOT / "scripts" / "set_version.py"


def run(cmd: list[str | Path], cwd: Path | None = None) -> None:
    print("$", " ".join(str(c) for c in cmd))
    subprocess.run([str(c) for c in cmd], cwd=cwd, check=True)


def main() -> None:
    print("Synchronising version numbers with git tag...")
    run(["python", SET_VERSION])

    print("Building release Rust binary...")
    run(["cargo", "build", "--release"], cwd=PERIOD_DIR)

    print("Preparing dist directory...")
    if DIST.exists():
        shutil.rmtree(DIST)
    DIST.mkdir(parents=True)

    release = PERIOD_DIR / "target" / "release"
    shutil.copy(release / "period.exe", DIST / "period.exe")
    shutil.copytree(ROOT / "period" / "stdlib", DIST / "stdlib")
    shutil.copy(ROOT / "assets" / "period.ico", DIST / "period.ico")

    print(f"Done. Distribution is in {DIST}")


if __name__ == "__main__":
    main()
