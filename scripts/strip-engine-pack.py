#!/usr/bin/env python3
"""strip-engine-pack.py — remove the engine-pack content that the licence
decision card t_20fae747 resolved as STRIP.

Runs on an assembled LibreOffice engine pack (the tree that P2 packages into the
installer and that `scripts/dist-license-gate.sh --scan` walks). Three groups go:

  culmus   share/fonts/truetype/{DavidCLM,FrankRuehlCLM,MiriamCLM,MiriamMonoCLM,
           NachlieliCLM}-*            GPL-2.0, and — measured from the pack's own
                                      LICENSE.html and the upstream Culmus README —
           NOT covered by the font-embedding exception. LibreOffice's PDF export
           embeds these font programs (measured: 5 subsets in a converted
           Hebrew document), so shipping them would make every produced PDF
           carry GPL font software. The OFL-1.1 twins (DavidLibre,
           FrankRuhlHofshi, MiriamLibre) stay in the pack and are what LO
           substitutes.

  gallery  share/gallery/**           clipart; no licence statement in the pack's
                                      own LICENSE/LICENSE.html/NOTICE. Not used by
                                      headless conversion.

  help     help/**                    LibreOffice's own help viewer content; not
                                      used by headless conversion (26 MB).

Every removed path is written to a manifest so the packaging step (and the card
audit trail) can show exactly what left the pack.

USAGE
  scripts/strip-engine-pack.py --pack <dir> [--dry-run] [--manifest <file>]

  --pack      pack root, i.e. the directory that holds `program/`, `share/`
              (and `help/`). Refuses to run if `program/soffice` is missing.
  --dry-run   print what would be removed, remove nothing
  --manifest  write `group<TAB>relpath<TAB>bytes` lines to this file

EXIT: 0 stripped (or already clean), 1 pack root not recognised, 2 usage.
"""
from __future__ import annotations

import argparse
import os
import sys

CULMUS_FAMILIES = (
    "DavidCLM",
    "FrankRuehlCLM",
    "MiriamCLM",
    "MiriamMonoCLM",
    "NachlieliCLM",
)


def collect(pack: str) -> list[tuple[str, str, int]]:
    """Return (group, relpath, bytes) for everything the decision strips.

    The data root differs by platform: Windows and Linux packs put it under
    `share/` (`share/fonts/truetype`, `share/gallery`), while the macOS pack —
    built from LibreOffice.app/Contents/Resources — keeps `fonts/truetype` at
    the pack root and has no `share/gallery` at all. Both layouts are collected
    so a decision that says "strip" cannot be silently a no-op on one platform
    (the macOS pack shipped all five GPL-2.0 Culmus families until this was
    fixed, caught by the pack's own post-strip assertion).
    """
    out: list[tuple[str, str, int]] = []

    for fontdir_rel in (("share", "fonts", "truetype"), ("fonts", "truetype")):
        fontdir = os.path.join(pack, *fontdir_rel)
        if not os.path.isdir(fontdir):
            continue
        for name in sorted(os.listdir(fontdir)):
            stem = name.split("-", 1)[0]
            if stem in CULMUS_FAMILIES:
                full = os.path.join(fontdir, name)
                if os.path.isfile(full):
                    out.append(("culmus", os.path.relpath(full, pack),
                                os.path.getsize(full)))

    for group, sub in (("gallery", os.path.join("share", "gallery")),
                       ("gallery", "gallery"),
                       ("help", "help")):
        root = os.path.join(pack, sub)
        if not os.path.isdir(root):
            continue
        for dirpath, _dirnames, filenames in os.walk(root):
            for name in sorted(filenames):
                full = os.path.join(dirpath, name)
                out.append((group, os.path.relpath(full, pack),
                            os.path.getsize(full)))
    return out


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(add_help=True)
    ap.add_argument("--pack", required=True)
    ap.add_argument("--dry-run", action="store_true")
    ap.add_argument("--manifest")
    args = ap.parse_args(argv[1:])

    pack = os.path.abspath(args.pack)
    # The launcher name is platform-decided: `soffice` on POSIX, `soffice.com`
    # (and `soffice.exe`) on Windows. The strip set is path-identical on both,
    # so the Windows pack is a valid input for the same decision.
    launchers = ("soffice", "soffice.com", "soffice.exe")
    if not any(os.path.isfile(os.path.join(pack, "program", name)) for name in launchers):
        print(f"strip-engine-pack: refusing to run: {pack} does not look like a "
              f"LibreOffice engine pack (no program/soffice)", file=sys.stderr)
        return 1

    items = collect(pack)
    by_group: dict[str, list[tuple[str, int]]] = {}
    for group, rel, size in items:
        by_group.setdefault(group, []).append((rel, size))

    for group in ("culmus", "gallery", "help"):
        rows = by_group.get(group, [])
        total = sum(s for _r, s in rows)
        print(f"strip-engine-pack: {group:<8} {len(rows):>5} files "
              f"{total:>10,d} B")

    if args.manifest:
        with open(args.manifest, "w", encoding="utf-8") as fh:
            for group, rel, size in items:
                fh.write(f"{group}\t{rel}\t{size}\n")
        print(f"strip-engine-pack: manifest -> {args.manifest} "
              f"({len(items)} entries)")

    if args.dry_run:
        print("strip-engine-pack: dry run, nothing removed")
        return 0

    removed = 0
    for group, rel, _size in items:
        target = os.path.join(pack, rel)
        try:
            os.remove(target)
            removed += 1
        except FileNotFoundError:  # already gone (idempotent re-run)
            pass

    # Prune the now-empty directory trees (the pack tolerates their absence —
    # proven by the conversion matrix in evidence/engine-pack-licence-decisions.md).
    for sub in (os.path.join("share", "gallery"), "help"):
        root = os.path.join(pack, sub)
        if not os.path.isdir(root):
            continue
        for dirpath, _dirnames, _filenames in os.walk(root, topdown=False):
            try:
                os.rmdir(dirpath)
            except OSError:
                pass  # not empty (something unexpected) — leave it, the gate reports it

    print(f"strip-engine-pack: removed {removed} files")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
