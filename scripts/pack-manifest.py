#!/usr/bin/env python3
"""pack-manifest.py — the engine pack's version + checksum manifest.

`MANIFEST.sha256` answers "are these bytes the bytes that were built?". It does
not answer "which pack is this, which engines does it hold, which architecture,
and where is it supposed to be installed?" — that is this file's job, and
`MANIFEST.json` is what the installer, the CI verifier and a support question
read.

TWO FILES, ONE SELF-CONSISTENT PAIR
-----------------------------------
`seal` writes both and the ordering is deliberate (a manifest that hashes itself
is circular, so it is solved explicitly rather than by leaving a hole):

  1. hash every regular file except the two manifest files -> MANIFEST.sha256
  2. files_list_sha256 = sha256(MANIFEST.sha256 as written)
  3. write MANIFEST.json, carrying that `files_list_sha256`
  4. append MANIFEST.json's own hash line to MANIFEST.sha256

So MANIFEST.json names the hash of the list *as it was before step 4*, and
MANIFEST.sha256 (after step 4) covers MANIFEST.json. `verify` reproduces exactly
that: it removes the MANIFEST.json line, re-hashes the remaining bytes, and
compares with `checksums.files_list_sha256`; then it checks every file,
MANIFEST.json included, against its line. Nothing is trusted and nothing is
double-hashed.

`seal-artifact` writes the sidecar next to the published DMG. A file inside the
DMG cannot contain the DMG's own digest, so the artifact digest lives beside it:

  EZ-Convert-Engine-Pack-arm64.dmg
  EZ-Convert-Engine-Pack-arm64.dmg.sha256
  EZ-Convert-Engine-Pack-arm64.dmg.manifest.json

USAGE
  scripts/pack-manifest.py seal --pack <dir> --arch arm64|x86_64
                                [--pack-version 1.0.0]
                                [--engine NAME=VERSION ...]
                                [--source NAME=sha256:HEX | NAME=file:PATH]
                                [--created-at ISO8601]
  scripts/pack-manifest.py seal-artifact --dmg <file> --pack-manifest <json>
                                         [--out <file>] [--created-at ISO8601]
  scripts/pack-manifest.py verify --pack <dir> [--expect-arch ARCH] [--quiet]

EXIT: 0 ok, 1 a check failed, 2 usage.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
from datetime import datetime, timezone
from typing import NoReturn

MANIFEST_JSON = "MANIFEST.json"
MANIFEST_SHA = "MANIFEST.sha256"
SCHEMA = 1
PACK_NAME = "EZ Convert Engine Pack"
# Where each engine's entry point lives inside the pack. The app resolves these
# paths itself (src-tauri/src/engine/proc.rs); recording them here is what makes
# the manifest checkable against the tree instead of a prose claim.
ENGINE_ENTRIES = {
    "soffice": "engines/soffice/LibreOffice.app/Contents/MacOS/soffice",
    "ffmpeg": "engines/ffmpeg/ffmpeg",
    "vips": "engines/vips/vips",
}
INSTALL_ROOT = "~/Library/Application Support/app.ezconvert.desktop"
ENV_OVERRIDE = "EZC_ENGINE_PACK"


def die(msg: str, code: int = 1) -> NoReturn:
    print(msg, file=sys.stderr)
    raise SystemExit(code)


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_file(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def walk_tree(pack: str):
    """Return (files, symlinks) as posix rel paths; symlinks are never followed.

    `Resources -> .` in the LibreOffice tree is a symlink to a *directory*, so it
    arrives via os.walk's `dirs`, not `files`. Both cases are recorded: the whole
    point of the inventory is that an unpack (zip, tar, DMG, ditto) that flattens
    a symlink changes this list.
    """
    files: list[str] = []
    symlinks: list[list[str]] = []
    for root, dirs, names in os.walk(pack, followlinks=False):
        for d in list(dirs):
            full = os.path.join(root, d)
            if os.path.islink(full):
                symlinks.append([os.path.relpath(full, pack).replace(os.sep, "/"),
                                 os.readlink(full)])
                dirs.remove(d)
        for name in names:
            full = os.path.join(root, name)
            rel = os.path.relpath(full, pack).replace(os.sep, "/")
            if os.path.islink(full):
                symlinks.append([rel, os.readlink(full)])
            else:
                files.append(rel)
    files.sort()
    symlinks.sort()
    return files, symlinks


def read_manifest_list(path: str) -> list[bytes]:
    with open(path, "rb") as fh:
        return fh.read().splitlines(keepends=True)


def line_path(line: bytes) -> str:
    # "hex  ./rel/path" -> "./rel/path"
    parts = line.rstrip(b"\n").split(b"  ", 1)
    if len(parts) != 2:
        die(f"malformed line in {MANIFEST_SHA}: {line!r}")
    return parts[1].decode("utf-8", "surrogateescape")


def cmd_seal(args: argparse.Namespace) -> int:
    pack = os.path.abspath(args.pack)
    if not os.path.isdir(os.path.join(pack, "engines")):
        die(f"not a pack root (no engines/): {pack}")

    for stale in (MANIFEST_JSON, MANIFEST_SHA):
        p = os.path.join(pack, stale)
        if os.path.exists(p):
            os.remove(p)

    files, symlinks = walk_tree(pack)
    if not files:
        die(f"pack root has no files: {pack}")

    lines: list[bytes] = []
    for rel in files:
        lines.append(f"{sha256_file(os.path.join(pack, rel))}  ./{rel}\n".encode())

    list_path = os.path.join(pack, MANIFEST_SHA)
    with open(list_path, "wb") as fh:
        fh.writelines(lines)
    files_list_sha256 = sha256_file(list_path)

    engines: dict[str, dict] = {}
    for name, version in args.engine:
        entry = ENGINE_ENTRIES.get(name)
        if entry is None:
            die(f"unknown engine {name!r}; known: {', '.join(sorted(ENGINE_ENTRIES))}", 2)
        full = os.path.join(pack, entry)
        if not os.path.exists(full):
            # Fail closed: a manifest that lists an engine the pack does not have
            # is worse than no manifest — it would make a broken pack look whole.
            die(f"engine {name} has no entry point at {entry} in {pack}")
        prefix = os.path.dirname(entry) + "/"
        owned = [f for f in files if f == entry or f.startswith(prefix)]
        engines[name] = {
            "version": version,
            "entry": entry,
            "files": len(owned),
            "bytes": sum(os.path.getsize(os.path.join(pack, f)) for f in owned),
        }

    sources: dict[str, str] = {}
    for name, spec in args.source:
        kind, _, value = spec.partition(":")
        if kind == "sha256":
            if len(value) != 64 or any(c not in "0123456789abcdef" for c in value):
                die(f"source {name}: sha256 must be 64 lowercase hex chars, got {value!r}", 2)
            sources[name] = value
        elif kind == "file":
            if not os.path.isfile(value):
                die(f"source {name}: no such file {value}")
            sources[name] = sha256_file(value)
        else:
            die(f"source {name}: expected sha256:<hex> or file:<path>, got {spec!r}", 2)

    manifest = {
        "manifest_version": SCHEMA,
        "pack": {
            "name": PACK_NAME,
            "id": f"ezconvert-engine-pack-macos-{args.arch}",
            "version": args.pack_version,
            "platform": "macos",
            "arch": args.arch,
            "install_root": INSTALL_ROOT,
            "engines_dir": "engines",
            "env_override": ENV_OVERRIDE,
            "created_at": args.created_at or now_iso(),
        },
        "engines": engines,
        "checksums": {
            "algorithm": "sha256",
            "files_list": MANIFEST_SHA,
            "files_list_sha256": files_list_sha256,
            "files_count": len(files),
            "bytes_total": sum(os.path.getsize(os.path.join(pack, f)) for f in files),
            "self_in_files_list": True,
        },
        "sources": sources,
        "symlinks": [{"path": p, "target": t} for p, t in symlinks],
    }
    manifest_path = os.path.join(pack, MANIFEST_JSON)
    with open(manifest_path, "w", encoding="utf-8") as fh:
        json.dump(manifest, fh, indent=2, sort_keys=True)
        fh.write("\n")

    with open(list_path, "ab") as fh:
        fh.write(f"{sha256_file(manifest_path)}  ./{MANIFEST_JSON}\n".encode())

    print(f"manifest: pack {manifest['pack']['id']} version {args.pack_version} "
          f"({len(files) + 1} files, {len(symlinks)} symlinks)")
    for name in sorted(engines):
        e = engines[name]
        print(f"manifest: engine {name} {e['version']} -> {e['entry']} "
              f"({e['files']} files)")
    return 0


def cmd_seal_artifact(args: argparse.Namespace) -> int:
    if not os.path.isfile(args.dmg):
        die(f"no DMG at {args.dmg}")
    with open(args.pack_manifest, "rb") as fh:
        try:
            pack = json.load(fh)
        except json.JSONDecodeError as exc:
            die(f"{args.pack_manifest} is not JSON: {exc}")
    out = args.out or f"{args.dmg}.manifest.json"
    artifact = {
        "artifact_manifest_version": SCHEMA,
        "artifact": {
            "name": os.path.basename(args.dmg),
            "bytes": os.path.getsize(args.dmg),
            "sha256": sha256_file(args.dmg),
        },
        "pack": {
            "id": pack["pack"]["id"],
            "version": pack["pack"]["version"],
            "arch": pack["pack"]["arch"],
        },
        # Hash of the pack's own MANIFEST.json: the sidecar is what lets a
        # downloaded artifact be tied to the tree it carries without unpacking
        # anything first.
        "pack_manifest_sha256": sha256_file(args.pack_manifest),
        "pack_manifest_files_list_sha256": pack["checksums"]["files_list_sha256"],
        "created_at": args.created_at or now_iso(),
    }
    with open(out, "w", encoding="utf-8") as fh:
        json.dump(artifact, fh, indent=2, sort_keys=True)
        fh.write("\n")
    print(f"artifact manifest: {os.path.basename(out)} "
          f"sha256={artifact['artifact']['sha256']} "
          f"bytes={artifact['artifact']['bytes']}")
    return 0


def cmd_verify(args: argparse.Namespace) -> int:
    pack = os.path.abspath(args.pack)
    failures: list[str] = []

    def fail(msg: str) -> None:
        failures.append(msg)

    mpath = os.path.join(pack, MANIFEST_JSON)
    lpath = os.path.join(pack, MANIFEST_SHA)
    for p in (mpath, lpath):
        if not os.path.isfile(p):
            die(f"missing {os.path.basename(p)} in {pack}")

    with open(mpath, "rb") as fh:
        try:
            manifest = json.load(fh)
        except json.JSONDecodeError as exc:
            die(f"{MANIFEST_JSON} is not JSON: {exc}")

    if manifest.get("manifest_version") != SCHEMA:
        fail(f"manifest_version is {manifest.get('manifest_version')}, expected {SCHEMA}")
    pmeta = manifest.get("pack", {})
    if args.expect_arch and pmeta.get("arch") != args.expect_arch:
        fail(f"manifest arch is {pmeta.get('arch')!r}, expected {args.expect_arch!r}")

    # 1. the self-referential part: the list without MANIFEST.json's own line.
    raw = read_manifest_list(lpath)
    manifest_self = None
    rest: list[bytes] = []
    for line in raw:
        if line_path(line) == f"./{MANIFEST_JSON}":
            manifest_self = line.rstrip(b"\n").split(b"  ", 1)[0].decode()
        else:
            rest.append(line)
    if manifest_self is None:
        fail(f"{MANIFEST_SHA} does not cover {MANIFEST_JSON}")
    else:
        if sha256_file(mpath) != manifest_self:
            fail(f"{MANIFEST_JSON} does not match its line in {MANIFEST_SHA}")
        recomputed = hashlib.sha256(b"".join(rest)).hexdigest()
        recorded = manifest.get("checksums", {}).get("files_list_sha256")
        if recomputed != recorded:
            fail(f"{MANIFEST_SHA} changed since MANIFEST.json was written "
                 f"(recorded {recorded}, recomputed {recomputed})")

    # 2. every file in the list, and nothing on disk that is not in the list.
    listed: set[str] = set()
    for line in raw:
        rel = line_path(line)
        listed.add(os.path.normpath(rel))
        target = os.path.join(pack, rel)
        if not os.path.isfile(target):
            fail(f"listed but missing: {rel}")
            continue
        want = line.rstrip(b"\n").split(b"  ", 1)[0].decode()
        if sha256_file(target) != want:
            fail(f"checksum mismatch: {rel}")

    files, symlinks = walk_tree(pack)
    on_disk = {os.path.normpath("./" + f) for f in files}
    on_disk.discard(os.path.normpath("./" + MANIFEST_SHA))
    for extra in sorted(on_disk - listed):
        fail(f"not in {MANIFEST_SHA}: {extra}")

    # 3. the symlink inventory — this is the "unpacks with symlinks intact" check.
    recorded_links = {s["path"]: s["target"] for s in manifest.get("symlinks", [])}
    disk_links = {p: t for p, t in symlinks}
    for path, target in sorted(recorded_links.items()):
        if path not in disk_links:
            fail(f"symlink gone (flattened by the unpack?): {path} -> {target}")
        elif disk_links[path] != target:
            fail(f"symlink retargeted: {path} -> {disk_links[path]!r}, "
                 f"recorded {target!r}")
    for path in sorted(set(disk_links) - set(recorded_links)):
        fail(f"symlink not in the manifest: {path} -> {disk_links[path]}")
    for path in sorted(recorded_links):
        full = os.path.join(pack, path)
        # A dangling symlink passes os.path.exists() as False, which is exactly
        # the state a bad copy produces — check the link itself.
        if os.path.exists(full) and not os.path.islink(full):
            fail(f"recorded symlink is now a regular file: {path}")

    # 4. engines: version recorded, entry point present, size not silently short.
    for name, meta in sorted(manifest.get("engines", {}).items()):
        entry = meta.get("entry", "")
        full = os.path.join(pack, entry)
        if not os.path.exists(full):
            fail(f"engine {name} missing its entry point {entry}")
        prefix = os.path.dirname(entry) + "/"
        owned = [f for f in files if f == entry or f.startswith(prefix)]
        if len(owned) != meta.get("files"):
            fail(f"engine {name}: {len(owned)} files on disk, manifest says {meta.get('files')}")

    if failures:
        print(f"verify: FAIL ({len(failures)} problem(s)) in {pack}", file=sys.stderr)
        for f in failures[:40]:
            print(f"  - {f}", file=sys.stderr)
        if len(failures) > 40:
            print(f"  ... {len(failures) - 40} more", file=sys.stderr)
        return 1

    if not args.quiet:
        print(f"verify: OK {pmeta.get('id')} version {pmeta.get('version')} "
              f"arch {pmeta.get('arch')}")
        print(f"verify: {len(raw) - 1} files hashed, {MANIFEST_JSON} covered by the list")
        print(f"verify: {len(disk_links)} symlinks intact")
        for name in sorted(manifest.get("engines", {})):
            e = manifest["engines"][name]
            print(f"verify: engine {name} {e['version']} at {e['entry']}")
    return 0


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(add_help=True, description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)

    seal = sub.add_parser("seal", help="write MANIFEST.json + MANIFEST.sha256 for a pack tree")
    seal.add_argument("--pack", required=True)
    seal.add_argument("--arch", required=True, choices=["arm64", "x86_64"])
    seal.add_argument("--pack-version", default=os.environ.get("PACK_VERSION", "1.0.0"))
    seal.add_argument("--engine", action="append", default=[], metavar="NAME=VERSION",
                      help="repeatable; NAME is soffice|ffmpeg|vips")
    seal.add_argument("--source", action="append", default=[], metavar="NAME=sha256:HEX|file:PATH",
                      help="repeatable; provenance digests recorded in the manifest")
    seal.add_argument("--created-at", default="")
    seal.set_defaults(func=cmd_seal)

    art = sub.add_parser("seal-artifact", help="write the sidecar manifest next to a DMG")
    art.add_argument("--dmg", required=True)
    art.add_argument("--pack-manifest", required=True)
    art.add_argument("--out", default="")
    art.add_argument("--created-at", default="")
    art.set_defaults(func=cmd_seal_artifact)

    ver = sub.add_parser("verify", help="check a pack tree against its own manifests")
    ver.add_argument("--pack", required=True)
    ver.add_argument("--expect-arch", default="")
    ver.add_argument("--quiet", action="store_true")
    ver.set_defaults(func=cmd_verify)

    args = ap.parse_args(argv)
    if getattr(args, "engine", None):
        parsed = []
        for item in args.engine:
            name, _, version = item.partition("=")
            if not name or not version:
                die(f"--engine expects NAME=VERSION, got {item!r}", 2)
            parsed.append((name, version))
        args.engine = parsed
    if getattr(args, "source", None):
        parsed_sources = []
        for item in args.source:
            name, _, spec = item.partition("=")
            if not name or not spec:
                die(f"--source expects NAME=sha256:HEX|file:PATH, got {item!r}", 2)
            parsed_sources.append((name, spec))
        args.source = parsed_sources
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
