"""JSON parsing and serialization for Period."""
import json as _json


def parse(text):
    return _json.loads(text)


def stringify(value):
    return _json.dumps(value, ensure_ascii=False)


EXPORTS = {
    "parse": (parse, ("parse with <json> -> any", "Parse a JSON string into a Period value (list, dictionary, string, number, boolean, or nothing).")),
    "stringify": (stringify, ("stringify with <value> -> string", "Serialize a Period value into a JSON string.")),
}
