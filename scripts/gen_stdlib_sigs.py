#!/usr/bin/env python3
"""Generate LeekScript signature files from functions.json and constants.json.

Run from leekscript-rs root or with cwd anywhere; paths are relative to this script.
Reads functions.json and constants.json from the same directory as this script.
Writes stdlib_functions.sig and stdlib_constants.sig to examples/signatures/.
"""

import json
import os
import sys

# Type code (int or str in JSON) -> LeekScript type string for signatures.
# Bare Array, Map, Set are shorthand for Array<any>, Map<any, any>, Set<any>.
FUNC_TYPE = {
    -1: "any",
    0: "void",
    1: "real|integer",
    2: "string",
    3: "boolean",
    4: "Array",
    5: "Function",
    6: "integer",
    7: "real",
    8: "Map",
    9: "Set",
    10: "Interval",
    41: "Array<integer>", # Technically real|integer
    42: "string",
    44: "Array<Array>",
    46: "Array<integer>",
    96: "Set<integer>",
    806: "Map<any, integer>",
}

CONST_TYPE = {
    1: "integer",
    2: "string",
    4: "Array",
    6: "integer",
    7: "real",
}


def type_str(code, context=None):
    """Return LeekScript type string for a type code. If unknown and context given, report to stderr."""
    if isinstance(code, str):
        code = int(code)
    result = FUNC_TYPE.get(code, "any")
    if result == "any" and code not in FUNC_TYPE and context is not None:
        print(f"Unknown type {code} — add to FUNC_TYPE. Used in: {context}", file=sys.stderr)
    return result


def const_type_str(c, context=None):
    """Return LeekScript type string for a constant. If unknown and context given, report to stderr."""
    t = c.get("type", 1)
    if isinstance(t, str):
        t = int(t)
    # E, PI, Infinity, NaN are real
    if t == 1 and c.get("name") in ("E", "PI", "Infinity", "NaN"):
        return "real"
    if t == 1 and "." in str(c.get("value", "")):
        return "real"
    result = CONST_TYPE.get(t, "any")
    if result == "any" and t not in CONST_TYPE and context is not None:
        print(f"Unknown type {t} — add to CONST_TYPE. Used in: {context}", file=sys.stderr)
    return result


def param_list(names, types, optional=None, function_name=None):
    """Format params as 'type paramName' or 'type paramName?' when argument can be omitted."""
    if not names:
        return ""
    optional = optional or []
    parts = []
    for i, (n, t) in enumerate(zip(names, types)):
        ctx = f"function {function_name}, param '{n}'" if function_name else None
        typ = type_str(t, context=ctx)
        name = n
        if i < len(optional) and optional[i]:
            name = f"{n}?"
        parts.append(f"{typ} {name}")
    return ", ".join(parts)


def main():
    scripts_dir = os.path.dirname(os.path.abspath(__file__))
    out_dir = os.path.join(scripts_dir, "..", "examples", "signatures")
    out_dir = os.path.normpath(out_dir)
    os.makedirs(out_dir, exist_ok=True)

    functions_path = os.path.join(scripts_dir, "functions.json")
    constants_path = os.path.join(scripts_dir, "constants.json")

    if not os.path.isfile(functions_path):
        print(f"Error: {functions_path} not found", file=sys.stderr)
        sys.exit(1)
    if not os.path.isfile(constants_path):
        print(f"Error: {constants_path} not found", file=sys.stderr)
        sys.exit(1)

    with open(functions_path, encoding="utf-8") as f:
        data = json.load(f)

    lines = [
        "// LeekScript standard library — function signatures",
        "// Generated from functions.json. Do not edit by hand.",
        "",
    ]
    for fn in data["functions"]:
        name = fn["name"]
        names = fn.get("arguments_names", [])
        types = fn.get("arguments_types", [])
        optional = fn.get("optional", [])
        ret = fn.get("return_type", -1)
        params = param_list(names, types, optional, function_name=name)
        ret_s = type_str(ret, context=f"function {name}, return type")
        if ret_s == "void":
            lines.append(f"function {name}({params}) -> void")
        else:
            lines.append(f"function {name}({params}) -> {ret_s}")
        lines.append("")

    with open(os.path.join(out_dir, "stdlib_functions.sig"), "w", encoding="utf-8") as f:
        f.write("\n".join(lines))

    with open(constants_path, encoding="utf-8") as f:
        const_data = json.load(f)

    lines = [
        "// LeekScript standard library — global constants",
        "// Generated from constants.json. Do not edit by hand.",
        "",
    ]
    for c in const_data["constants"]:
        name = c["name"]
        typ = const_type_str(c, context=f"constant '{name}'")
        lines.append(f"global {typ} {name}")
        lines.append("")

    with open(os.path.join(out_dir, "stdlib_constants.sig"), "w", encoding="utf-8") as f:
        f.write("\n".join(lines))

    print("Wrote", os.path.join(out_dir, "stdlib_functions.sig"))
    print("Wrote", os.path.join(out_dir, "stdlib_constants.sig"))


if __name__ == "__main__":
    main()
