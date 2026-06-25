"""Input/output utilities for Period."""
from pathlib import Path


def read(path: str) -> str:
    return Path(path).read_text(encoding="utf-8")


def write(path: str, content: str) -> None:
    Path(path).write_text(content, encoding="utf-8")


EXPORTS = {
    "read": (read, ("read with <path> -> string", "Read the contents of a file as a string.")),
    "write": (write, ("write with <path>, <content>", "Write a string to a file.")),
}
