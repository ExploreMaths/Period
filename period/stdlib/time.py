"""Time utilities for Period."""
import time as _time


def now(format_str=None):
    if format_str is None:
        return _time.time()
    return _time.strftime(format_str, _time.localtime())


def sleep(seconds):
    _time.sleep(seconds)


def format(timestamp=None, format_str="%Y-%m-%d %H:%M:%S"):
    if timestamp is None:
        timestamp = _time.time()
    return _time.strftime(format_str, _time.localtime(timestamp))


EXPORTS = {
    "now": (now, ("now with [format] -> number|string", "Return the current time as a Unix timestamp. If a format string is given, return a formatted local-time string instead.")),
    "sleep": (sleep, ("sleep with <seconds>", "Pause execution for the given number of seconds.")),
    "format": (format, ("format with [timestamp], [format] -> string", "Format a Unix timestamp as a human-readable string. Defaults to the current time and the format '%Y-%m-%d %H:%M:%S'.")),
}
