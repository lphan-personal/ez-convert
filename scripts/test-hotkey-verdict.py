#!/usr/bin/env python3
"""test-hotkey-verdict.py — prove the hotkey verdict checker in both directions.

The checker is what decides a card from a JSON file, so it gets the same treatment
as `png-nonblank.py`: known inputs with known answers. It runs on Linux, where the
registration itself cannot.

Run: python3 scripts/test-hotkey-verdict.py
"""

import json
import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
CHECKER = os.path.join(HERE, "hotkey-verdict.py")

sys.path.insert(0, HERE)
from importlib import import_module  # noqa: E402

verdict = import_module("hotkey-verdict").verdict


def facts(**overrides):
    hotkey = {
        "configured": "Ctrl+Alt+Shift+D",
        "configured_canonical": "Ctrl+Alt+Shift+D",
        "registered": "Ctrl+Alt+Shift+D",
        "status": "Ctrl+Alt+Shift+D registered",
        "state": "registered",
        "matches_configured": True,
        "carbon": {"key_code": 2, "modifiers": 6656},
        "input_posting_trusted": False,
        "presses": [{"method": "cg-post-own-pid", "delivered": False}],
        "summons_before": 3,
        "summons_after": 3,
        "delivered_by": None,
        "note": "no synthetic key event reached the hot key manager",
    }
    hotkey.update(overrides)
    return {"ok": True, "hotkey": hotkey}


CASES = []


def case(name, expected_ok):
    def register(function):
        CASES.append((name, function, expected_ok))
        return function

    return register


@case("a registered chord that was not pressed still passes", True)
def _(payload):
    return payload


@case("a chord taken by someone else fails", False)
def _(payload):
    payload["hotkey"]["registered"] = "Ctrl+Alt+Shift+E"
    payload["hotkey"]["matches_configured"] = False
    return payload


@case("a failed registration fails even if the string matches", False)
def _(payload):
    payload["hotkey"]["state"] = "failed"
    payload["hotkey"]["matches_configured"] = False
    return payload


@case("an all-taken ladder fails", False)
def _(payload):
    payload["hotkey"]["state"] = "all-taken"
    payload["hotkey"]["registered"] = None
    payload["hotkey"]["matches_configured"] = False
    return payload


@case("a delivered press is reported as delivered", True)
def _(payload):
    payload["hotkey"]["delivered_by"] = "cg-post-own-pid"
    payload["hotkey"]["presses"] = [{"method": "cg-post-own-pid", "delivered": True}]
    payload["hotkey"]["summons_after"] = 4
    return payload


@case("a linux probe with no hotkey block is not a macOS measurement", None)
def _(payload):
    return {"ok": True}


def run_cases():
    failures = 0
    for name, build, expected in CASES:
        ok, lines = verdict(build(facts()))
        if ok != expected:
            failures += 1
            print(f"FAIL {name}: expected ok={expected}, got {ok}\n  " + "\n  ".join(lines))
        else:
            last = lines[-1]
            print(f"ok   {name} -> {last}")
    return failures


def run_cli():
    """The exit code is what CI branches on, so it is checked too."""
    failures = 0
    with tempfile.TemporaryDirectory() as scratch:
        good = os.path.join(scratch, "good.json")
        bad = os.path.join(scratch, "bad.json")
        with open(good, "w", encoding="utf-8") as handle:
            json.dump(facts(), handle)
        broken = facts()
        broken["hotkey"]["registered"] = "Ctrl+Alt+Shift+E"
        broken["hotkey"]["matches_configured"] = False
        with open(bad, "w", encoding="utf-8") as handle:
            json.dump(broken, handle)

        for path, expected in ((good, 0), (bad, 1)):
            result = subprocess.run(
                [sys.executable, CHECKER, path],
                capture_output=True,
                text=True,
                check=False,
            )
            status = "ok  " if result.returncode == expected else "FAIL"
            if result.returncode != expected:
                failures += 1
            print(f"{status} cli {os.path.basename(path)} exit={result.returncode} (want {expected})")

        missing = subprocess.run(
            [sys.executable, CHECKER, os.path.join(scratch, "absent.json")],
            capture_output=True,
            text=True,
            check=False,
        )
        if missing.returncode != 2:
            failures += 1
            print(f"FAIL cli absent facts exit={missing.returncode} (want 2)")
        else:
            print("ok   cli absent facts exit=2")
    return failures


if __name__ == "__main__":
    total = run_cases() + run_cli()
    print(f"hotkey-verdict: {len(CASES) + 3 - total}/{len(CASES) + 3} checks passed")
    sys.exit(1 if total else 0)
