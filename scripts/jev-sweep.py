#!/usr/bin/env python3
"""Sweep Rust symbols past Jev to rank them for restructuring.

PROBE-STAGE TOOL. The question set here is unvalidated beyond the backtest in
`.agents/skills/jev-restructuring/references/questions.md`. Read that file before
trusting a ranking. Promote to `tddy-tools` only once the questions hold up.

Two passes, deliberately separate:

  classify  every function in scope -> P(structural problem) + category, ranked
  propose   top-K only -> a restructure operation, emitted as candidate plan.jsonl

Jev never decides whether a cut is *possible*: `tddy-tools restructure check --deep`
does that, deterministically, against a warm index. Jev only decides whether a cut is
*worth making*.

Nothing here counts. Lengths, nesting and CRAP come from `tddy-tools analyze`; feeding
counts to a model is how this repo once recorded 44 fields and 79 methods for a struct
that had 37 and 46 (see packages/tddy-core/docs/code-issues/god-object-presenter.md).

The key comes from `TYPESAFE_API_KEY`, exported or in the repo-root `.env` (gitignored) --
the same loader semantics `./web-dev` and `./run-vm-testkit` use: an already-set variable
always wins, so `.env` never silently overrides what you exported.

Usage:
    echo 'TYPESAFE_API_KEY=apikey_...' >> .env     # or export it
    scripts/jev-sweep.py classify packages/tddy-core/src --query presenter
    scripts/jev-sweep.py classify packages/tddy-core/src -o sweep.jsonl
    scripts/jev-sweep.py propose sweep.jsonl --top 20
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import sys
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor

API_KEY_ENV = "TYPESAFE_API_KEY"
BASE_URL_ENV = "TYPESAFE_BASE_URL"
DEFAULT_MODEL_ENV = "TYPESAFE_DEFAULT_MODEL"

DEFAULT_BASE_URL = "https://api.typesafe.ai"
DEFAULT_MODEL = "jev-latest"

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent

# --------------------------------------------------------------------------------------
# The criteria. This block is the whole review surface -- it is the repo's house style,
# written down. Change a weight here rather than rewriting a question.
# --------------------------------------------------------------------------------------

W_PROBLEM = 0.60  # P(structural problem) -- the gate, dominates the score
W_DISPATCH = 0.25  # tangled routing is the shape most worth cutting
W_PURE_CONSTRUCTION = -0.55  # long-but-flat builders are the classic false positive

GATE = 0.70  # below this, not a candidate at any score
MIN_CATEGORY_CONFIDENCE = 0.60  # below this, the category is not reportable
MIN_BODY_LINES = 12  # shorter than this, a restructure is not worth a request

# Categories are the taxonomy already in packages/*/docs/code-issues/ filenames.
CATEGORY_CRITERIA = {
    "god_function": "It does several unrelated jobs that share no data with each other.",
    "tangled_dispatch": (
        "It routes many cases, and the routing and the per-case work are interleaved "
        "rather than separated."
    ),
    "long_but_flat": (
        "It is long, but every statement is at the same level and independent of the others."
    ),
    "pure_mapping": "It converts one shape into another with no decisions to make.",
    "none": "It has no structural problem worth acting on.",
}

CLASSIFY_QUESTIONS = {
    "has_structural_problem": {
        "type": "noul",
        "instructions": (
            "The body of `fn` contains decision logic that a reader must trace through "
            "several branches to follow."
        ),
        "criteria": {
            "true": (
                "Following what the function does requires holding several branch "
                "conditions in mind at once."
            ),
            "false": (
                "The body reads straight through, or is a flat sequence of independent "
                "assignments or arms that share no state."
            ),
        },
    },
    "dispatch_shape": {
        "type": "noul",
        "instructions": (
            "`fn` routes to different behaviour based on matching a value, and the arms of "
            "that match contain substantial logic rather than a single call each."
        ),
    },
    "pure_construction": {
        "type": "noul",
        "instructions": (
            "`fn` is mostly assembling a value from its inputs -- field assignments, struct "
            "or enum construction -- with no branching on those inputs."
        ),
    },
    "contiguous_seam": {
        "type": "noul",
        "instructions": (
            "A run of consecutive statements in `fn` operates on values that the rest of the "
            "body does not read afterwards."
        ),
    },
    "category": {
        "type": "choice",
        "instructions": "Which structural problem best describes `fn`, if any?",
        "criteria": CATEGORY_CRITERIA,
    },
}

# Restricted to operations the Rust backend actually supports (plan-schema.md), minus the
# ones this repo has recorded as unusable. `move_module_to_crate` is absent deliberately.
OPERATION_CRITERIA = {
    "extract_method": (
        "Lift a contiguous run of statements out of the body into a private helper, "
        "leaving a call in its place."
    ),
    "extract_module": (
        "Move a group of whole items out of this file into a module of their own. Only "
        "applies when the unit is a group of items, not a single function body."
    ),
    "extract_trait": (
        "Pull a set of methods off an inherent impl into a trait. Only applies to an impl "
        "block."
    ),
    "inline_method": (
        "Fold this function back into its only caller because it adds a name and nothing else."
    ),
    "tests_first": (
        "The unit is too entangled to move safely and has no test covering it; a "
        "restructure would relocate the risk without reducing it."
    ),
    "leave": "No operation is worth applying here.",
}

PROPOSE_QUESTIONS = {
    "worth_acting": {
        "type": "noul",
        "instructions": (
            "Restructuring `fn` would make it easier to read without changing what it does."
        ),
    },
    "safe_without_tests": {
        "type": "noul",
        "instructions": (
            "The behaviour of `fn` is evident enough from its body that a reviewer could "
            "tell whether a mechanical move changed it."
        ),
    },
    "operation": {
        "type": "choice",
        "instructions": "Which restructuring operation best fits `fn`?",
        "criteria": OPERATION_CRITERIA,
    },
}

# --------------------------------------------------------------------------------------
# Symbol extraction. Syntactic and deliberately dumb -- rust-analyzer owns the real
# anchors, via `tddy-tools restructure anchors`, at the point a plan is written.
# --------------------------------------------------------------------------------------

FN_RE = re.compile(r"^(\s*)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?fn\s+([A-Za-z0-9_]+)")


# A `#[cfg(test)]` module is skipped by the body scan below, but an integration test is
# not gated by one -- `tests/foo.rs` is ordinary code to the compiler. Sweeping those
# ranks test helpers alongside production functions and quietly pollutes every finding.
NON_PRODUCTION_DIRS = {"tests", "benches", "examples", "target", "testkit"}


def is_production(rs: pathlib.Path) -> bool:
    return not (
        NON_PRODUCTION_DIRS & set(rs.parts)
        or rs.name.endswith("_test.rs")
        or rs.name.endswith("_tests.rs")
    )


def iter_functions(path: pathlib.Path):
    """Yield (file, name, line, body) for every production fn outside a #[cfg(test)] module."""
    found = sorted(path.rglob("*.rs")) if path.is_dir() else [path]
    for rs in (f for f in found if is_production(f)):
        try:
            lines = rs.read_text(encoding="utf-8", errors="replace").splitlines()
        except OSError:
            continue
        in_test_mod_at = None
        depth_running = 0
        for i, line in enumerate(lines):
            if re.match(r"\s*#\[cfg\(test\)\]", line):
                in_test_mod_at = depth_running
            depth_running += line.count("{") - line.count("}")
            if in_test_mod_at is not None and depth_running <= in_test_mod_at:
                in_test_mod_at = None
            if in_test_mod_at is not None:
                continue
            m = FN_RE.match(line)
            if not m:
                continue
            body, depth, started = [], 0, False
            for ln in lines[i:]:
                body.append(ln)
                depth += ln.count("{") - ln.count("}")
                if "{" in ln:
                    started = True
                if started and depth <= 0:
                    break
                if len(body) > 1200:  # runaway; the brace scan lost the plot
                    break
            if started and len(body) >= MIN_BODY_LINES:
                yield str(rs), m.group(2), i + 1, "\n".join(body)


# --------------------------------------------------------------------------------------
# Transport
# --------------------------------------------------------------------------------------


def load_dotenv(path: pathlib.Path) -> None:
    """Load repo-root `.env`, with the semantics `./web-dev` and `./run-vm-testkit` use.

    An already-set variable always wins, so `.env` never silently overrides an export.
    Split on the first `=` only; strip one layer of surrounding quotes; skip blanks and
    `#` comments. Deliberately no `export FOO=bar` handling -- the shell loaders have
    none either, and one env format in the repo is worth more than a second convenience.
    """
    if not path.is_file():
        return
    for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
        key, sep, value = line.partition("=")
        if not sep or not key or key.lstrip().startswith("#"):
            continue
        for quote in ('"', "'"):
            if value.endswith(quote):
                value = value[:-1]
            if value.startswith(quote):
                value = value[1:]
        os.environ.setdefault(key, value)


def resolve_key() -> str:
    load_dotenv(REPO_ROOT / ".env")
    key = os.environ.get(API_KEY_ENV)
    if key:
        return key
    sys.exit(
        f"{API_KEY_ENV} is not set.\n"
        f"  export {API_KEY_ENV}=apikey_...\n"
        f"or add a line to {REPO_ROOT / '.env'} (gitignored):\n"
        f"  {API_KEY_ENV}=apikey_...\n"
        f"No `export ` prefix, no spaces around `=` -- the repo's .env loaders take the\n"
        f"literal text left of the first `=` as the variable name.\n"
        f"Keys: https://console.typesafe.ai/keys"
    )


RETRY_STATUSES = {429, 500, 502, 503, 504}
MAX_ATTEMPTS = 5


def ask(state: dict, questions: dict, key: str) -> dict:
    """POST one state plus its question set, retrying on rate limits and transient 5xx.

    A repo-wide sweep is thousands of requests against a 1,200/min limit, so 429 is an
    expected outcome rather than an error. `retry-after` wins when the response carries
    one -- the limits move without notice, so the server's number beats our backoff.
    """
    base = os.environ.get(BASE_URL_ENV, DEFAULT_BASE_URL).rstrip("/")
    model = os.environ.get(DEFAULT_MODEL_ENV, DEFAULT_MODEL)
    payload = json.dumps({"state": state, "model": model, "questions": questions}).encode()
    for attempt in range(MAX_ATTEMPTS):
        req = urllib.request.Request(
            f"{base}/v1/systemone",
            data=payload,
            headers={"Authorization": f"Bearer {key}", "Content-Type": "application/json"},
        )
        try:
            with urllib.request.urlopen(req, timeout=120) as resp:
                return json.load(resp)
        except urllib.error.HTTPError as e:
            if e.code not in RETRY_STATUSES or attempt == MAX_ATTEMPTS - 1:
                raise
            delay = float(e.headers.get("retry-after") or 0) or min(2**attempt, 30)
            time.sleep(delay)
        except (urllib.error.URLError, TimeoutError):
            if attempt == MAX_ATTEMPTS - 1:
                raise
            time.sleep(min(2**attempt, 30))
    raise RuntimeError("unreachable")


def noul(ans: dict, k: str) -> float:
    return float(ans.get(k, {}).get("noul", 0.0))


def score_of(ans: dict) -> float:
    return (
        W_PROBLEM * noul(ans, "has_structural_problem")
        + W_DISPATCH * noul(ans, "dispatch_shape")
        + W_PURE_CONSTRUCTION * noul(ans, "pure_construction")
    )


# --------------------------------------------------------------------------------------
# Passes
# --------------------------------------------------------------------------------------


def classify(args, key):
    units = [
        u
        for u in iter_functions(pathlib.Path(args.path))
        if not args.query or args.query.lower() in (u[0] + " " + u[1]).lower()
    ]
    if args.limit:
        units = units[: args.limit]
    print(f"# {len(units)} units in scope", file=sys.stderr)
    if not units:
        return

    tokens = 0
    rows = []

    def one(u):
        f, name, line, body = u
        try:
            out = ask({"fn": body}, CLASSIFY_QUESTIONS, key)
        except urllib.error.HTTPError as e:
            return {"file": f, "symbol": name, "line": line, "error": f"{e.code} {e.reason}"}
        a = out["answers"]
        cat = a["category"]
        return {
            "file": f,
            "symbol": name,
            "line": line,
            "body_lines": len(body.splitlines()),
            "score": round(score_of(a), 3),
            "p_problem": noul(a, "has_structural_problem"),
            "p_dispatch": noul(a, "dispatch_shape"),
            "p_construction": noul(a, "pure_construction"),
            "p_seam": noul(a, "contiguous_seam"),
            "category": cat["choice"] if cat["confidence"] >= MIN_CATEGORY_CONFIDENCE else "unsure",
            "category_confidence": cat["confidence"],
            "input_tokens": out["usage"]["input_tokens"],
        }

    with ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        for r in pool.map(one, units):
            if "error" in r:
                print(f"# {r['symbol']}: {r['error']}", file=sys.stderr)
                continue
            tokens += r.pop("input_tokens")
            rows.append(r)

    rows.sort(key=lambda r: -r["score"])
    out = open(args.out, "w") if args.out else sys.stdout
    for r in rows:
        out.write(json.dumps(r) + "\n")
    if args.out:
        out.close()

    candidates = [r for r in rows if r["p_problem"] >= GATE]
    print(
        f"# {len(candidates)}/{len(rows)} above gate {GATE} | "
        f"{tokens:,} input tokens | ${tokens * 0.042 / 1e6:.4f}",
        file=sys.stderr,
    )


def propose(args, key):
    rows = [json.loads(l) for l in open(args.sweep) if l.strip()]
    rows = [r for r in rows if r["p_problem"] >= GATE][: args.top]
    print(f"# proposing for {len(rows)} units", file=sys.stderr)

    def one(r):
        body = "\n".join(
            pathlib.Path(r["file"]).read_text(errors="replace").splitlines()[
                r["line"] - 1 : r["line"] - 1 + r["body_lines"]
            ]
        )
        out = ask({"fn": body}, PROPOSE_QUESTIONS, key)
        a = out["answers"]
        op = a["operation"]
        return {
            **r,
            "operation": op["choice"],
            "operation_confidence": op["confidence"],
            "operation_probabilities": op["probabilities"],
            "p_worth_acting": noul(a, "worth_acting"),
            "p_safe_without_tests": noul(a, "safe_without_tests"),
        }

    with ThreadPoolExecutor(max_workers=args.concurrency) as pool:
        results = list(pool.map(one, rows))

    for r in results:
        print(json.dumps(r))

    print("\n# Next: these are PROPOSALS, not a plan.", file=sys.stderr)
    print(
        "#   1. `tddy-tools restructure anchors <file> --items <symbol>` for real coordinates\n"
        "#   2. write plan.jsonl, `restructure snapshot`, then `check --deep`\n"
        "#   3. --deep is the only thing that knows whether the seam can be cut",
        file=sys.stderr,
    )


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = p.add_subparsers(dest="cmd", required=True)

    c = sub.add_parser("classify", help="rank every function in scope")
    c.add_argument("path")
    c.add_argument("--query", help="substring filter on file path or symbol name")
    c.add_argument("--limit", type=int)
    c.add_argument("-o", "--out")
    c.add_argument("--concurrency", type=int, default=8)

    pr = sub.add_parser("propose", help="propose an operation for the top-ranked units")
    pr.add_argument("sweep")
    pr.add_argument("--top", type=int, default=20)
    pr.add_argument("--concurrency", type=int, default=8)

    args = p.parse_args()
    {"classify": classify, "propose": propose}[args.cmd](args, resolve_key())


if __name__ == "__main__":
    main()
