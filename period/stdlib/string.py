"""String utilities for Period."""


def join(separator, items):
    return separator.join(str(item) for item in items)


def split(value, separator=None):
    if separator is None:
        return value.split()
    return value.split(separator)


def trim(value):
    return value.strip()


def upper(value):
    return value.upper()


def lower(value):
    return value.lower()


def replace(value, old, new):
    return value.replace(old, new)


def contains(value, substring):
    return substring in value


def substring(value, start, length=None):
    if length is None:
        return value[start:]
    return value[start:start + length]


EXPORTS = {
    "join": (join, ("join with <separator>, <list> -> string", "Concatenate all items in a list into a string, separated by the given separator.")),
    "split": (split, ("split with <value>, [separator] -> list", "Split a string into a list of substrings. If separator is omitted, split on whitespace.")),
    "trim": (trim, ("trim with <value> -> string", "Return a copy of the string with leading and trailing whitespace removed.")),
    "upper": (upper, ("upper with <value> -> string", "Return a copy of the string converted to uppercase.")),
    "lower": (lower, ("lower with <value> -> string", "Return a copy of the string converted to lowercase.")),
    "replace": (replace, ("replace with <value>, <old>, <new> -> string", "Return a copy of the string with all occurrences of old replaced by new.")),
    "contains": (contains, ("contains with <value>, <substring> -> boolean", "Return true if substring is found in value.")),
    "substring": (substring, ("substring with <value>, <start>, [length] -> string", "Return a slice of the string starting at start. If length is given, limit the slice to that many characters.")),
}
