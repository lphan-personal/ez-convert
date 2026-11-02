#!/usr/bin/env python3
"""Regenerate docs/LICENSES.md from the locked dependency graph.

Policy R6 (docs/PRIOR_ART.md): licenses are additive, never silent — every new
dependency lands here the moment it is added. This script reads the truth from
`cargo metadata` instead of anyone transcribing it by hand.

Usage:
    python3 scripts/gen-licenses.py          # writes docs/LICENSES.md
    python3 scripts/gen-licenses.py --check  # exit 1 if the file is stale

Requires: cargo on PATH, src-tauri/Cargo.lock committed.
"""

import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
MANIFEST = os.path.join(ROOT, "src-tauri", "Cargo.toml")
OUT = os.path.join(ROOT, "docs", "LICENSES.md")

# Direct dependencies of ours (the ones a human actually chose).
DIRECT = {"ez-convert", "tauri", "tauri-build", "tauri-plugin-dialog", "serde", "serde_json",
          "lofty", "sha2",
          # macOS-only (src-tauri/Cargo.toml `[target.'cfg(target_os = "macos")']`):
          # the overlay's cursor/monitor/click-through AppKit surface (shell/screen.rs,
          # shell/overlay.rs). Pinned as direct deps with features; already in the graph
          # transitively via tao/tauri.
          "objc2", "objc2-app-kit", "objc2-foundation"}


def metadata():
    raw = subprocess.run(
        ["cargo", "metadata", "--format-version", "1",
         "--manifest-path", MANIFEST],
        capture_output=True, text=True, check=True,
    ).stdout
    return json.loads(raw)


def esc(s):
    return (s or "UNKNOWN").replace("|", "\\|").replace("\n", " ")


def render():
    meta = metadata()
    packages = meta["packages"]
    workspace = set(meta.get("workspace_members", []))

    rows = []
    for p in packages:
        is_ours = p["id"] in workspace
        rows.append({
            "name": p["name"],
            "version": p["version"],
            "license": ("own code - licence TBD (stakeholder decision)" if is_ours
                        else esc(p.get("license") or (", ".join(p.get("license_file") or []) or "UNKNOWN"))),
            "direct": p["name"] in DIRECT or is_ours,
            "ours": is_ours,
        })
    rows.sort(key=lambda r: (r["name"].lower(), r["version"]))

    direct = [r for r in rows if r["direct"]]
    rest = [r for r in rows if not r["direct"]]

    lines = []
    lines.append("# Third-party licenses")
    lines.append("")
    lines.append("Policy R6 (`docs/PRIOR_ART.md`): licenses are additive, never silent — a new")
    lines.append("dependency lands here the moment it is added. **Generated, do not hand-edit:**")
    lines.append("")
    lines.append("```bash")
    lines.append("python3 scripts/gen-licenses.py          # regenerate")
    lines.append("python3 scripts/gen-licenses.py --check  # CI drift check")
    lines.append("```")
    lines.append("")
    lines.append(f"Total packages in the locked graph: **{len(rows)}** "
                 f"({len(direct)} direct/workspace, {len(rest)} transitive).")
    lines.append("")
    lines.append("## Direct dependencies")
    lines.append("")
    lines.append("| Crate | Version | License |")
    lines.append("|---|---|---|")
    for r in direct:
        lines.append(f"| `{r['name']}` | {r['version']} | {r['license']} |")
    lines.append("")
    lines.append("## Transitive dependencies")
    lines.append("")
    lines.append("| Crate | Version | License |")
    lines.append("|---|---|---|")
    for r in rest:
        lines.append(f"| `{r['name']}` | {r['version']} | {r['license']} |")
    lines.append("")
    lines.append("## Bundled engines (not Rust dependencies)")
    lines.append("")
    lines.append("| Engine | License posture | How it ships |")
    lines.append("|---|---|---|")
    lines.append("| ffmpeg | LGPL-only builds required (`--enable-gpl` / `--enable-nonfree` "
                 "rejected by `scripts/license-assert.sh`) | Separate-process sidecar. Never "
                 "statically linked, never bundled into the app binary. |")
    lines.append("| vips | LGPL-2.1-or-later (GPL-only loaders disabled in the build) | "
                 "Separate-process sidecar (`vips`, `vipsheader`). Never linked. |")
    lines.append("| LibreOffice (`soffice`) | MPL-2.0 | Separate-process sidecar. The TDF "
                 "release ships unmodified at the version pinned in `SPEC.md` §6; its "
                 "`NOTICE`/`LICENSE` files travel with the installer. |")
    lines.append("| MSVC runtime (`concrt140`, `msvcp140*`, `vcruntime140*`) | Microsoft "
                 "Visual C++ Redistributable terms (VS 2022 REDIST list: unmodified copies "
                 "of the `VC\\redist` files, distributable with the program) | Copied "
                 "unmodified into the Windows install root by "
                 "`bundle.windows.bundleVCRuntime`; per-file pins in "
                 "`docs/shipped-inventory.tsv`, terms in "
                 "`docs/licenses/THIRD-PARTY-NOTICES.md` section 9. |")
    lines.append("")
    lines.append("The licence gate runs in CI on every push "
                 "(`license-assert` job) and fails the build on a GPL/nonfree ffmpeg.")
    lines.append("")
    return "\n".join(lines)


def main():
    text = render()
    if "--check" in sys.argv:
        if not os.path.exists(OUT):
            print("docs/LICENSES.md is missing — run scripts/gen-licenses.py", file=sys.stderr)
            return 1
        current = open(OUT, encoding="utf-8").read()
        if current != text:
            print("docs/LICENSES.md is stale — run scripts/gen-licenses.py", file=sys.stderr)
            return 1
        print("docs/LICENSES.md is current")
        return 0
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w", encoding="utf-8") as f:
        f.write(text)
    print(f"wrote {OUT}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
