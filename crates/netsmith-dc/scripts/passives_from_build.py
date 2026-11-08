#!/usr/bin/env python3
"""Print island lines (the format of src/text.rs) for chosen parts of a built board.

    python3 -I passives_from_build.py <build-output.txt> <catalog.csv>... -- R8 R9 F1 D2

`<build-output.txt>` is what `netsmith build` prints: its bill of materials (reference, kind,
value, footprint, LCSC number) and its nets (`net NAME: R8.1, Q1.3, ...`). Each reference's two
pins are looked up in the nets; the part's value comes from the catalog row with its LCSC number
(the resistance with its tolerance, a fuse's resistance, a bead's DC resistance, a diode's
forward drop). A part with no row, or a value with no tolerance, is said in a comment. Pins:
two-terminal parts 1 and 2; a Schottky diode has its anode on pin 2 and its cathode on pin 1.
Nothing here reads a project's source: only what the tool exports.
"""
import csv
import re
import sys


def value(text):
    t = text.replace(" ", "").replace("+/-", "+-")
    t = re.sub(r"(?<=[0-9a-zA-Z.])ohm", "", t)
    t = re.sub(r"(?<=[0-9.])V", "", t)
    return t


def main(argv):
    if "--" not in argv:
        sys.exit(__doc__)
    cut = argv.index("--")
    log, catalogs, refs = argv[0], argv[1:cut], argv[cut + 1:]
    bom, nets = {}, {}
    for line in open(log, encoding="utf-8"):
        m = re.match(r"^  net (\S+): (.*)$", line.rstrip("\n"))
        if m:
            for pin in m.group(2).split(", "):
                nets[pin] = m.group(1)
            continue
        parts = re.split(r"\s{2,}", line.strip())
        if len(parts) >= 5 and re.match(r"^[A-Z]+[0-9]+$", parts[0]):
            bom[parts[0]] = parts
    rows = {}
    for path in catalogs:
        with open(path, encoding="utf-8", newline="") as f:
            for row in csv.DictReader(f):
                if row.get("lcsc"):
                    rows.setdefault(row["lcsc"], row)
    for ref in refs:
        if ref not in bom:
            print(f"# {ref}: not in the bill of materials")
            continue
        kind = bom[ref][1]
        lcsc = bom[ref][4].split()[0]
        row = rows.get(lcsc)
        pin = lambda n: nets.get(f"{ref}.{n}", f"?{ref}.{n}")
        if row is None:
            print(f"# {ref} {kind} {bom[ref][2]}: no catalog row for {lcsc}")
            continue
        if kind == "Schottky":
            v = value(row["forward_voltage"])
            print(f"D {ref.lower()} {pin(2)} {pin(1)} {v}   # {row.get('mpn', '')}, {lcsc}")
            continue
        column = {"Resistor": "resistance", "Fuse": "resistance", "FerriteBead": "dc_resistance"}[kind]
        v = value(row[column])
        note = "" if ("+-" in v or ".." in v) else "  (no tolerance in the row)"
        print(f"R {ref.lower()} {pin(1)} {pin(2)} {v}   # {kind} {row.get('mpn', '')}, {lcsc}{note}")


main(sys.argv[1:])
