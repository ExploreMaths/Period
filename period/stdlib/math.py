"""Math utilities for Period."""
import math as _math


def sin(x):
    return _math.sin(x)


def cos(x):
    return _math.cos(x)


def tan(x):
    return _math.tan(x)


def sqrt(x):
    return _math.sqrt(x)


def floor(x):
    return _math.floor(x)


def ceil(x):
    return _math.ceil(x)


def abs(x):
    return _math.fabs(x)


def round(x):
    return _math.floor(x + 0.5)


pi = _math.pi
e = _math.e


# Exported names. Value may be wrapped with a documentation entry:
#   name: (value, (signature, docstring))
#   name: (value, docstring)
#   name: value
EXPORTS = {
    "sin": (sin, ("sin with <x> -> number", "Return the sine of x (x in radians).")),
    "cos": (cos, ("cos with <x> -> number", "Return the cosine of x (x in radians).")),
    "tan": (tan, ("tan with <x> -> number", "Return the tangent of x (x in radians).")),
    "sqrt": (sqrt, ("sqrt with <x> -> number", "Return the square root of x.")),
    "floor": (floor, ("floor with <x> -> number", "Return the largest integer less than or equal to x.")),
    "ceil": (ceil, ("ceil with <x> -> number", "Return the smallest integer greater than or equal to x.")),
    "abs": (abs, ("abs with <x> -> number", "Return the absolute value of x.")),
    "round": (round, ("round with <x> -> number", "Return x rounded to the nearest integer.")),
    "pi": (pi, "The ratio of a circle's circumference to its diameter (3.14159...)."),
    "e": (e, "The base of the natural logarithm (2.71828...)."),
}
