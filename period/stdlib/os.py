"""Operating system utilities for Period."""
import os as _os


def list_dir(path="."):
    return _os.listdir(path)


def exists(path):
    return _os.path.exists(path)


def is_file(path):
    return _os.path.isfile(path)


def is_dir(path):
    return _os.path.isdir(path)


def mkdir(path):
    _os.makedirs(path, exist_ok=True)


def remove(path):
    _os.remove(path)


EXPORTS = {
    "list_dir": (list_dir, ("list_dir with [path] -> list", "Return a list of names in the given directory. Defaults to the current directory.")),
    "exists": (exists, ("exists with <path> -> boolean", "Return true if the path exists on the file system.")),
    "is_file": (is_file, ("is_file with <path> -> boolean", "Return true if the path is a regular file.")),
    "is_dir": (is_dir, ("is_dir with <path> -> boolean", "Return true if the path is a directory.")),
    "mkdir": (mkdir, ("mkdir with <path>", "Create a directory and any missing parent directories.")),
    "remove": (remove, ("remove with <path>", "Delete a file. Use with care: deleted files cannot be recovered from here.")),
}
