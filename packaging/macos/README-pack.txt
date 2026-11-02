EZ Convert — macOS Engine Pack
=============================

EZ Convert converts files entirely on this Mac: no network calls, no telemetry.
The conversion engines (libvips, ffmpeg, LibreOffice) are too large to live in
the app itself, so they ship in this pack and are installed once.

Install
-------
1. Open "Install Engine Pack.command" (double-click it).
   If macOS refuses, right-click it and choose Open.
2. It checks that this pack matches this Mac's architecture (arm64 or x86_64) —
   a mismatched pack is refused, nothing is copied — then verifies every file
   against MANIFEST.sha256 and copies the engines to
   ~/Library/Application Support/app.ezconvert.desktop/engines
3. Open EZ Convert.

To check the installation from Terminal:

  "/Applications/EZ Convert.app/Contents/MacOS/ez-convert" --diagnostics --fixtures .

To uninstall, delete the folder:

  ~/Library/Application Support/app.ezconvert.desktop

What is inside
--------------
  engines/vips     libvips 8.18.x + its libraries (LGPL-2.1-or-later)
  engines/ffmpeg   ffmpeg/ffprobe, built without GPL components (LGPL-3.0)
  engines/soffice  LibreOffice, separate process (MPL-2.0)

Licence texts and the exact build record (ffmpeg configure output, LibreOffice
image digest, the list of stripped files, and the licence-gate scan) are in
licenses/. The app never links these engines: each one is a separate process.

MANIFEST.json names the pack (id, version, architecture), the version of every
engine, the install root, and the SHA-256 of MANIFEST.sha256 — MANIFEST.sha256
in turn covers MANIFEST.json. The installer checks the hashes before copying
anything; to re-check an installed pack on macOS:

  bash scripts/verify-engine-pack.sh --pack \
    "$HOME/Library/Application Support/app.ezconvert.desktop"

The pack is signed ad-hoc for now. With an Apple Developer ID it will be signed
with a Developer ID Application certificate, with hardened runtime and a secure
timestamp, and notarized — docs/MACOS-PACKAGING.md §4 has the exact order.
