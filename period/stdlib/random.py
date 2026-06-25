"""Random number utilities for Period."""
import random as _random


def random(seed=None):
    if seed is not None:
        _random.seed(seed)
    return _random.random()


def randint(min, max):
    return _random.randint(int(min), int(max))


def choice(seq):
    return _random.choice(seq)


def shuffle(seq):
    result = list(seq)
    _random.shuffle(result)
    return result


EXPORTS = {
    "random": (random, ("random with [seed] -> number", "Return a random floating-point number in the range [0.0, 1.0). An optional seed makes the sequence reproducible.")),
    "randint": (randint, ("randint with <min>, <max> -> integer", "Return a random integer N such that min <= N <= max.")),
    "choice": (choice, ("choice with <list> -> any", "Return a random element from a non-empty list.")),
    "shuffle": (shuffle, ("shuffle with <list> -> list", "Return a new list with the elements shuffled randomly.")),
}
