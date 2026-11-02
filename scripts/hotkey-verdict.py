#!/usr/bin/env python3
"""hotkey-verdict.py — decide the macOS global-hotkey measurement from the facts.

The app writes the facts (`--overlay-probe`, `src-tauri/src/shell/probe.rs`); this
script reads them and answers two questions out loud:

1. **Acceptance:** does the chord the OS holds equal the chord `settings.json` asks
   for? (`hotkey.registered` == canonical spelling of `hotkey.configured`, and the
   registration state is `registered`.) A failure here is a real failure: exit 1.
2. **Pressure:** did a synthetic key event reach the hot key manager and summon the
   wheel? A macOS CI runner usually refuses input posting (TCC). That case is
   REPORTED, not failed — the card allows asserting only the registration when the
   session cannot inject input, and inventing a pass would be worse than saying so.

Usage: python3 scripts/hotkey-verdict.py <facts.json>
Exit:  0 the acceptance checks pass; 1 they do not; 2 the facts have no hotkey block.
"""

import json
import sys


def verdict(facts):
    """Return (ok, lines). Pure, so the checker itself can be tested."""
    lines = []
    hotkey = facts.get("hotkey")
    if not isinstance(hotkey, dict):
        return None, ["HOTKEY_VERDICT ok=false reason=no-hotkey-block"]

    configured = hotkey.get("configured")
    registered = hotkey.get("registered")
    state = hotkey.get("state")
    matches = bool(hotkey.get("matches_configured"))

    lines.append(
        f"HOTKEY configured={configured!r} registered={registered!r} state={state}"
    )
    carbon = hotkey.get("carbon") or {}
    lines.append(
        f"HOTKEY carbon_key_code={carbon.get('key_code')} "
        f"carbon_modifiers={carbon.get('modifiers')} "
        f"status={hotkey.get('status')!r}"
    )

    presses = hotkey.get("presses") or []
    delivered_by = hotkey.get("delivered_by")
    for press in presses:
        lines.append(
            f"HOTKEY press={press.get('method')} "
            f"delivered={str(press.get('delivered')).lower()}"
        )
    lines.append(
        f"HOTKEY summons={hotkey.get('summons_before')}->{hotkey.get('summons_after')} "
        f"delivered_by={delivered_by} "
        f"input_posting_trusted={hotkey.get('input_posting_trusted')}"
    )
    if hotkey.get("note"):
        lines.append(f"HOTKEY note={hotkey['note']}")

    ok = bool(matches) and state == "registered"
    if ok and delivered_by:
        lines.append(f"HOTKEY_VERDICT ok=true chord-press=delivered-by-{delivered_by}")
    elif ok:
        attempted = len(presses)
        lines.append(
            "HOTKEY_VERDICT ok=true chord-press=not-asserted "
            f"attempted={attempted} (registration and the summon path the probe "
            "measures are what was asserted; see note)"
        )
    else:
        lines.append(
            f"HOTKEY_VERDICT ok=false reason=registered-mismatch "
            f"matches_configured={str(matches).lower()} state={state}"
        )
    return ok, lines


def main(argv):
    if len(argv) != 2:
        print("usage: hotkey-verdict.py <facts.json>", file=sys.stderr)
        return 2
    try:
        with open(argv[1], encoding="utf-8") as handle:
            facts = json.load(handle)
    except (OSError, ValueError) as error:
        print(f"HOTKEY_VERDICT ok=false reason=cannot-read-facts: {error}")
        return 2
    ok, lines = verdict(facts)
    print("\n".join(lines))
    if ok is None:
        return 2
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
