#!/usr/bin/env python3
"""Check furniture library templates (format 1) against the spec in
docs/licenses/furniture-library.md.

Usage: python3 tools/furniture-lint.py [file.json ...]
With no arguments every crates/carve-core/src/furniture/library/*.json is checked.

Checks: keys, kinds and ids; every expression evaluates at the default values,
at all minimums, at all maximums, at each parameter's own minimum and maximum
and at every choice option and toggle state; each panel box has exactly one
axis equal to its material thickness and positive sizes; no two panel boxes
overlap in volume (touching is fine, pairs whose connection class defaults to
crossLap may cross); every panel touches or crosses another panel.
Exit code 0 when every file passes, 1 otherwise.
"""

import json
import math
import re
import sys
from pathlib import Path

TOL = 0.01
LIBRARY = Path(__file__).resolve().parent.parent / "crates/carve-core/src/furniture/library"

TOP_REQUIRED = {
    "format", "id", "name", "category", "description", "license", "author",
    "source", "credit", "tags", "params", "derived", "materials", "panels",
}
TOP_OPTIONAL = {"connections", "jointClasses", "hardware"}
CATEGORIES = {
    "Storage", "Tables", "Seating", "Beds", "Desks", "Kitchen", "Outdoor",
    "Shop", "Kids", "Decor",
}
LICENSES = {"CC0-1.0", "CC-BY-4.0", "CC-BY-SA-4.0", "PD"}
KINDS = {"length", "thickness", "count", "number", "angle", "toggle", "choice"}
PARAM_KEYS = {"key", "label", "kind", "default", "min", "max", "options", "group", "step"}
PANEL_KEYS = {"id", "name", "material", "box", "grain", "features", "repeat", "when", "outline"}
BOX_AXES = ("x", "y", "z", "w", "d", "h")
GRAINS = {"long", "short", "none"}
EDGES = {"bottom", "top", "front", "back", "left", "right"}
FEATURES = {
    "notch", "cutout", "hole", "radius", "arch", "handle", "pocket",
    "shelfPins", "hingeCups", "slideHoles",
}
# Feature fields that hold words rather than expressions.
WORD_FIELDS = {"kind", "edge", "at", "face", "shape"}
# Feature fields that must come out positive.
POSITIVE_FIELDS = {"w", "h", "d", "r", "rise", "diameter", "depth", "pitch", "rows", "count"}
JOINTS = {
    "none", "dado", "stoppedDado", "tabSlot", "mortiseTenon", "wedgedTenon",
    "kerfTenon", "dowel", "captiveNut", "camLock", "twistLock", "rabbet",
    "finger", "cornerDovetail", "crossLap", "dovetail", "butterfly", "puzzle",
    "groove",
}
KEBAB = re.compile(r"^[a-z0-9]+(-[a-z0-9]+)*$")
IDENT = re.compile(r"^[a-z][a-z0-9_]*$")
# Text the project keeps out of every file (built so this file passes too).
BANNED = [chr(0x2014), "As" + "pire", "Vec" + "tric"]


class EvalError(Exception):
    pass


def tokenize(text):
    out = []
    i = 0
    while i < len(text):
        c = text[i]
        if c in " \t":
            i += 1
        elif c in "+-*/^()":
            out.append(("op", c))
            i += 1
        elif c == "{":
            end = text.find("}", i)
            if end < 0:
                raise EvalError("unclosed {")
            out.append(("name", text[i + 1:end].strip()))
            i = end + 1
        elif c.isdigit() or c == ".":
            m = re.match(r"\d*\.?\d+([eE][+-]?\d+)?|\d+\.", text[i:])
            if not m:
                raise EvalError(f"bad number at {text[i:]!r}")
            out.append(("num", float(m.group(0))))
            i += len(m.group(0))
        else:
            raise EvalError(f"unexpected {c!r} in {text!r} (names go in braces)")
    return out


def evaluate(text, values):
    """The value of `text` as the kernel's evaluator reads it."""
    if isinstance(text, (int, float)) and not isinstance(text, bool):
        return float(text)
    if not isinstance(text, str):
        raise EvalError(f"expression must be a string, got {text!r}")
    toks = tokenize(text)
    if not toks:
        raise EvalError("empty expression")
    pos = [0]

    def peek(op):
        return pos[0] < len(toks) and toks[pos[0]] == ("op", op)

    def expr():
        left = term()
        while peek("+") or peek("-"):
            minus = peek("-")
            pos[0] += 1
            right = term()
            left = left - right if minus else left + right
        return left

    def term():
        left = unary()
        while peek("*") or peek("/"):
            divide = peek("/")
            pos[0] += 1
            right = unary()
            if divide and right == 0:
                raise EvalError(f"division by zero in {text!r}")
            left = left / right if divide else left * right
        return left

    def unary():
        if peek("-") or peek("+"):
            minus = peek("-")
            pos[0] += 1
            v = unary()
            return -v if minus else v
        return power()

    def power():
        base = atom()
        if not peek("^"):
            return base
        pos[0] += 1
        return base ** unary()

    def atom():
        if pos[0] >= len(toks):
            raise EvalError(f"{text!r} ends early")
        kind, val = toks[pos[0]]
        pos[0] += 1
        if kind == "num":
            return val
        if kind == "name":
            if val not in values:
                raise EvalError(f"unknown name {{{val}}}")
            return values[val]
        if val == "(":
            inner = expr()
            if not peek(")"):
                raise EvalError(f"missing ) in {text!r}")
            pos[0] += 1
            return inner
        raise EvalError(f"unexpected {val!r} in {text!r}")

    value = expr()
    if pos[0] != len(toks):
        raise EvalError(f"trailing text in {text!r}")
    if isinstance(value, complex) or not math.isfinite(value):
        raise EvalError(f"{text!r} is not a finite number")
    return float(value)


def eval_name_template(name, values):
    """Names may hold {expr} pieces where bare words read values ({i+1})."""
    def repl(m):
        inner = m.group(1)
        braced = re.sub(r"[a-z_][a-z0-9_]*", lambda w: "{" + w.group(0) + "}", inner)
        return str(int(round(evaluate(braced, values))))
    return re.sub(r"\{([^}]*)\}", repl, name)


def pattern_matches(pattern, panel_id):
    for alt in pattern.split("|"):
        regex = "^" + re.escape(alt).replace("\\*", ".*") + "$"
        if re.match(regex, panel_id):
            return True
    return False


class Linter:
    def __init__(self, path):
        self.path = path
        self.errors = []
        self.params = {}

    def err(self, msg):
        if msg not in self.errors:
            self.errors.append(msg)

    def run(self):
        raw = self.path.read_text(encoding="utf-8")
        for word in BANNED:
            if word in raw:
                self.err(f"banned text {word!r} in file")
        try:
            tpl = json.loads(raw)
        except json.JSONDecodeError as e:
            self.err(f"not valid JSON: {e}")
            return self.errors
        self.tpl = tpl
        self.check_structure()
        if self.errors:
            return self.errors
        for label, values in self.scenarios():
            self.check_scenario(label, values)
        return self.errors

    # Structure -------------------------------------------------------------

    def check_structure(self):
        t = self.tpl
        missing = TOP_REQUIRED - t.keys()
        if missing:
            self.err(f"missing keys: {sorted(missing)}")
        extra = t.keys() - TOP_REQUIRED - TOP_OPTIONAL
        if extra:
            self.err(f"unknown keys: {sorted(extra)}")
        if self.errors:
            return
        if t["format"] != 1:
            self.err("format must be 1")
        if not KEBAB.match(t["id"]):
            self.err(f"id {t['id']!r} is not kebab case")
        stem = self.path.name.removesuffix(".carvefurniture.json").removesuffix(".json")
        if t["id"] != stem:
            self.err(f"id {t['id']!r} does not match file name {stem!r}")
        if t["category"] not in CATEGORIES:
            self.err(f"category {t['category']!r} not one of {sorted(CATEGORIES)}")
        if t["license"] not in LICENSES:
            self.err(f"license {t['license']!r} not one of {sorted(LICENSES)}")
        if t["license"].startswith("CC-BY") and not t["credit"]:
            self.err("CC BY licenses need a credit line")
        if t["license"] != "CC0-1.0" and not t["source"]:
            self.err("a template that follows an outside design needs its source URL")
        for k in ("name", "description", "author"):
            if not isinstance(t[k], str) or not t[k].strip():
                self.err(f"{k} must be a non-empty string")
        if not isinstance(t["tags"], list) or not all(isinstance(x, str) for x in t["tags"]):
            self.err("tags must be a list of strings")

        for p in t["params"]:
            self.check_param(p)
        if not isinstance(t["derived"], dict):
            self.err("derived must be an object")
            return
        for k in t["derived"]:
            if not IDENT.match(k):
                self.err(f"derived name {k!r} is not a plain name")
            if k in self.params:
                self.err(f"derived {k!r} shadows a parameter")
        for k, m in t["materials"].items():
            if not IDENT.match(k):
                self.err(f"material key {k!r} is not a plain name")
            if set(m.keys()) != {"label", "thickness"}:
                self.err(f"material {k!r} needs exactly label and thickness")

        ids = set()
        for p in t["panels"]:
            extra = p.keys() - PANEL_KEYS
            if extra:
                self.err(f"panel {p.get('id')!r}: unknown keys {sorted(extra)}")
            for k in ("id", "name", "material", "box"):
                if k not in p:
                    self.err(f"panel {p.get('id')!r}: missing {k}")
            pid = p.get("id", "")
            if not re.match(r"^[a-z][a-z0-9_]*$", pid):
                self.err(f"panel id {pid!r} must be lower case letters, digits and _")
            if pid in ids:
                self.err(f"panel id {pid!r} used twice")
            ids.add(pid)
            if p.get("material") not in t["materials"]:
                self.err(f"panel {pid!r}: material {p.get('material')!r} not defined")
            if set(p.get("box", {}).keys()) != set(BOX_AXES):
                self.err(f"panel {pid!r}: box needs exactly {BOX_AXES}")
            if p.get("grain", "long") not in GRAINS:
                self.err(f"panel {pid!r}: grain {p['grain']!r}")
            for f in p.get("features", []):
                if f.get("kind") not in FEATURES:
                    self.err(f"panel {pid!r}: feature kind {f.get('kind')!r}")
                if "edge" in f and f["edge"] not in EDGES:
                    self.err(f"panel {pid!r}: feature edge {f['edge']!r}")
            for pt in p.get("outline", []):
                if not isinstance(pt, list) or len(pt) not in (2, 3):
                    self.err(f"panel {pid!r}: outline point {pt!r}")
            if "outline" in p and len(p["outline"]) < 3:
                self.err(f"panel {pid!r}: outline needs three or more points")

        classes = t.get("jointClasses", {})
        for k, c in classes.items():
            if set(c.keys()) - {"label", "default"} or "default" not in c:
                self.err(f"joint class {k!r} needs label and default")
            elif c["default"] not in JOINTS:
                self.err(f"joint class {k!r}: unknown joint {c['default']!r}")
        used = set()
        for r in t.get("connections", []):
            if set(r.keys()) != {"a", "b", "class"}:
                self.err(f"connection rule {r!r} needs exactly a, b, class")
                continue
            for side in ("a", "b"):
                for alt in r[side].split("|"):
                    if "*" not in alt and alt not in ids:
                        self.err(f"connection rule names unknown panel {alt!r}")
            if r["class"] not in classes and r["class"] != "other":
                self.err(f"connection rule class {r['class']!r} not in jointClasses")
            used.add(r["class"])
        for k in classes:
            if k not in used:
                self.err(f"joint class {k!r} is not used by any connection rule")
        for h in t.get("hardware", []):
            if set(h.keys()) - {"name", "count", "note"} or "name" not in h or "count" not in h:
                self.err(f"hardware row {h!r} needs name, count and optional note")

    def check_param(self, p):
        extra = p.keys() - PARAM_KEYS
        if extra:
            self.err(f"param {p.get('key')!r}: unknown keys {sorted(extra)}")
        key = p.get("key", "")
        if not IDENT.match(key) or key == "i":
            self.err(f"param key {key!r} is not a plain name")
        if key in self.params:
            self.err(f"param key {key!r} used twice")
        self.params[key] = p
        kind = p.get("kind")
        if kind not in KINDS:
            self.err(f"param {key!r}: kind {kind!r}")
            return
        if not p.get("label"):
            self.err(f"param {key!r}: missing label")
        d = p.get("default")
        if kind == "choice":
            opts = p.get("options")
            if not isinstance(opts, list) or len(opts) < 2:
                self.err(f"param {key!r}: choice needs two or more options")
            elif d not in opts:
                self.err(f"param {key!r}: default {d!r} not an option")
            elif any(not IDENT.match(o) for o in opts):
                self.err(f"param {key!r}: options must be plain names")
            return
        if not isinstance(d, (int, float)) or isinstance(d, bool):
            self.err(f"param {key!r}: default must be a number")
            return
        if kind == "toggle":
            if d not in (0, 1):
                self.err(f"param {key!r}: toggle default must be 0 or 1")
            return
        lo, hi = p.get("min"), p.get("max")
        if kind != "thickness" and (lo is None or hi is None):
            self.err(f"param {key!r}: needs min and max")
        if lo is not None and hi is not None and not lo <= d <= hi:
            self.err(f"param {key!r}: default {d} outside {lo}..{hi}")
        if kind == "count" and any(v is not None and v != int(v) for v in (d, lo, hi)):
            self.err(f"param {key!r}: counts are whole numbers")

    # Scenarios -------------------------------------------------------------

    def scenarios(self):
        base = {k: p["default"] for k, p in self.params.items()}
        yield "defaults", dict(base)
        lows, highs = dict(base), dict(base)
        for k, p in self.params.items():
            if "min" in p:
                lows[k] = p["min"]
            if "max" in p:
                highs[k] = p["max"]
        yield "all min", lows
        yield "all max", highs
        for k, p in self.params.items():
            if p["kind"] == "choice":
                for o in p["options"]:
                    yield f"{k}={o}", {**base, k: o}
            elif p["kind"] == "toggle":
                for v in (0, 1):
                    yield f"{k}={v}", {**base, k: v}
            else:
                for b in ("min", "max"):
                    if b in p:
                        yield f"{k}={b} {p[b]}", {**base, k: p[b]}

    def resolve(self, chosen):
        values = {}
        for k, p in self.params.items():
            v = chosen[k]
            if p["kind"] == "choice":
                for o in p["options"]:
                    values[f"{k}_{o}"] = 1.0 if o == v else 0.0
                values[k] = float(p["options"].index(v))
            else:
                values[k] = float(v)
        pending = dict(self.tpl["derived"])
        while pending:
            progressed = False
            for k, e in list(pending.items()):
                try:
                    values[k] = evaluate(e, values)
                except EvalError as ex:
                    if "unknown name" in str(ex):
                        continue
                    raise EvalError(f"derived {k}: {ex}")
                del pending[k]
                progressed = True
            if not progressed:
                k, e = next(iter(pending.items()))
                try:
                    evaluate(e, values)
                except EvalError as ex:
                    raise EvalError(f"derived {k}: {ex}")
        return values

    def check_scenario(self, label, chosen):
        where = f"[{label}]"
        try:
            values = self.resolve(chosen)
        except EvalError as e:
            self.err(f"{where} {e}")
            return
        thick = {}
        for k, m in self.tpl["materials"].items():
            try:
                thick[k] = evaluate(m["thickness"], values)
            except EvalError as e:
                self.err(f"{where} material {k}: {e}")
                return
            if thick[k] <= 0:
                self.err(f"{where} material {k}: thickness {thick[k]} not positive")

        boxes = []
        for p in self.tpl["panels"]:
            pid = p["id"]
            try:
                n = 1
                if "repeat" in p:
                    rv = evaluate(p["repeat"], values)
                    if abs(rv - round(rv)) > 1e-9 or rv < 0:
                        self.err(f"{where} panel {pid}: repeat {rv} is not a whole count")
                    n = int(round(rv))
                for i in range(n):
                    v = dict(values)
                    if "repeat" in p:
                        v["i"] = float(i)
                    if "when" in p and evaluate(p["when"], v) <= 0:
                        continue
                    eval_name_template(p["name"], v)
                    b = {a: evaluate(p["box"][a], v) for a in BOX_AXES}
                    key = f"{pid}#{i}" if "repeat" in p else pid
                    self.check_box(where, key, b, thick[p["material"]])
                    self.check_features(where, key, p, v)
                    for pt in p.get("outline", []):
                        for e in pt[:2]:
                            evaluate(e, v)
                        if len(pt) == 3:
                            evaluate(pt[2]["bulge"] if isinstance(pt[2], dict) else pt[2], v)
                    boxes.append((pid, i, b))
            except EvalError as e:
                self.err(f"{where} panel {pid}: {e}")
        for h in self.tpl.get("hardware", []):
            try:
                c = evaluate(h["count"], values)
                if c < 0:
                    self.err(f"{where} hardware {h['name']}: count {c} negative")
            except EvalError as e:
                self.err(f"{where} hardware {h['name']}: {e}")
        self.check_overlaps(where, boxes)

    def check_features(self, where, key, p, v):
        for f in p.get("features", []):
            if "when" in f and evaluate(f["when"], v) <= 0:
                continue
            for fk, fv in f.items():
                if fk in WORD_FIELDS or fk == "when":
                    continue
                if isinstance(fv, bool) or not isinstance(fv, (str, int, float)):
                    continue
                fval = evaluate(fv, v)
                if fk in POSITIVE_FIELDS and fval <= 0:
                    self.err(f"{where} panel {key}: feature {f['kind']} {fk} = {fval:g} not positive")

    def check_box(self, where, key, b, t):
        sizes = (b["w"], b["d"], b["h"])
        for axis, s in zip("wdh", sizes):
            if s <= TOL:
                self.err(f"{where} panel {key}: {axis} = {s:g} not positive")
        equal = [axis for axis, s in zip("wdh", sizes) if abs(s - t) <= TOL]
        if len(equal) != 1:
            self.err(f"{where} panel {key}: {len(equal)} axes equal the material thickness {t:g} "
                     f"(w {b['w']:g}, d {b['d']:g}, h {b['h']:g})")

    def cross_allowed(self, a, b):
        classes = self.tpl.get("jointClasses", {})
        for r in self.tpl.get("connections", []):
            hit = (pattern_matches(r["a"], a) and pattern_matches(r["b"], b)) or \
                  (pattern_matches(r["a"], b) and pattern_matches(r["b"], a))
            if hit:
                c = classes.get(r["class"])
                return c is not None and c["default"] == "crossLap"
        return False

    def check_overlaps(self, where, boxes):
        touched = [False] * len(boxes)
        for i in range(len(boxes)):
            for j in range(i + 1, len(boxes)):
                (pa, ia, a), (pb, ib, b) = boxes[i], boxes[j]
                gaps = []
                for o, s in (("x", "w"), ("y", "d"), ("z", "h")):
                    lo = max(a[o], b[o])
                    hi = min(a[o] + a[s], b[o] + b[s])
                    gaps.append(hi - lo)
                if min(gaps) >= -TOL:
                    touched[i] = touched[j] = True
                if all(g > TOL for g in gaps) and not self.cross_allowed(pa, pb):
                    self.err(f"{where} panels {pa}#{ia} and {pb}#{ib} overlap "
                             f"({gaps[0]:.2f} x {gaps[1]:.2f} x {gaps[2]:.2f} mm)")
        if len(boxes) > 1:
            for (pid, i, _), t in zip(boxes, touched):
                if not t and not self.hinged(pid):
                    self.err(f"{where} panel {pid}#{i} touches no other panel")

    def hinged(self, pid):
        """A hinged door (a panel with hinge cups) stands clear of the
        carcass by its reveal, so it may touch nothing."""
        for p in self.tpl.get("panels", []):
            if p.get("id") == pid:
                return any(f.get("kind") == "hingeCups" for f in p.get("features", []))
        return False


def main(argv):
    files = [Path(a) for a in argv] or sorted(LIBRARY.glob("*.json"))
    if not files:
        print("no template files found")
        return 1
    failed = 0
    ids = {}
    for f in files:
        errors = Linter(f).run()
        try:
            tid = json.loads(f.read_text(encoding="utf-8")).get("id")
            if tid in ids:
                errors.append(f"id {tid!r} also used by {ids[tid].name}")
            ids[tid] = f
        except (json.JSONDecodeError, AttributeError):
            pass
        if errors:
            failed += 1
            print(f"FAIL {f.name}")
            for e in errors[:40]:
                print(f"  {e}")
            if len(errors) > 40:
                print(f"  ... {len(errors) - 40} more")
        else:
            print(f"ok   {f.name}")
    print(f"{len(files) - failed} of {len(files)} templates pass")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
