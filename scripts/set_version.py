"""Synchronise project version numbers with the latest git tag.

Reads the most recent git tag (e.g. v2.0.0-beta.1), strips the leading 'v',
and writes that version into:
  - period/Cargo.toml
  - installer/period.iss
  - vscode-extension/package.json
  - vscode-extension/package-lock.json (if present)

Run manually:
    python scripts/set_version.py

Or import and call set_version_from_tag().
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def get_git_version() -> str:
    try:
        tag = subprocess.run(
            ["git", "describe", "--tags", "--abbrev=0"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            check=True,
        ).stdout.strip()
    except subprocess.CalledProcessError as e:
        raise RuntimeError(f"could not read git tag: {e.stderr or e.stdout}") from e

    if tag.startswith("v"):
        tag = tag[1:]
    return tag


def set_cargo_version(version: str) -> None:
    path = ROOT / "period" / "Cargo.toml"
    text = path.read_text(encoding="utf-8")
    new_text = re.sub(r'^version\s*=\s*"[^"]*"', f'version = "{version}"', text, flags=re.MULTILINE)
    if new_text == text:
        return
    path.write_text(new_text, encoding="utf-8")
    print(f"Set period/Cargo.toml version to {version}")


def set_installer_version(version: str) -> None:
    path = ROOT / "installer" / "period.iss"
    text = path.read_text(encoding="utf-8")
    new_text = re.sub(r'#define MyAppVersion "[^"]*"', f'#define MyAppVersion "{version}"', text)
    if new_text == text:
        return
    path.write_text(new_text, encoding="utf-8")
    print(f"Set installer/period.iss version to {version}")


def set_extension_version(version: str) -> None:
    pkg_path = ROOT / "vscode-extension" / "package.json"
    pkg = json.loads(pkg_path.read_text(encoding="utf-8"))
    if pkg.get("version") == version:
        return
    pkg["version"] = version
    pkg_path.write_text(json.dumps(pkg, indent=2) + "\n", encoding="utf-8")
    print(f"Set vscode-extension/package.json version to {version}")

    lock_path = ROOT / "vscode-extension" / "package-lock.json"
    if not lock_path.exists():
        return
    lock = json.loads(lock_path.read_text(encoding="utf-8"))
    if lock.get("version") == version and lock.get("packages", {}).get("", {}).get("version") == version:
        return
    lock["version"] = version
    if "" in lock.get("packages", {}):
        lock["packages"][""]["version"] = version
    lock_path.write_text(json.dumps(lock, indent=2) + "\n", encoding="utf-8")
    print(f"Set vscode-extension/package-lock.json version to {version}")


def set_version_from_tag() -> str:
    version = get_git_version()
    set_cargo_version(version)
    set_installer_version(version)
    set_extension_version(version)
    return version


def main() -> int:
    try:
        version = set_version_from_tag()
    except RuntimeError as e:
        print(f"error: {e}", file=sys.stderr)
        return 1
    print(f"Project version synchronized to {version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
