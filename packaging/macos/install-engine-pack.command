#!/bin/bash
# Install Engine Pack.command — copy the EZ Convert engine pack into place.
#
# Double-click this file on the mounted "EZ Convert Engine Pack" volume, or run
# it from Terminal:
#
#   bash "/Volumes/EZ Convert Engine Pack/Install Engine Pack.command"
#
# It verifies every file against MANIFEST.sha256 and only then copies the tree
# to:
#
#   ~/Library/Application Support/app.ezconvert.desktop/engines
#
# which is where the app looks for engines it does not bundle
# (docs/MACOS-PACKAGING.md §2). Re-running it replaces the pack; nothing else
# on the machine is touched.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
packroot="$HOME/Library/Application Support/app.ezconvert.desktop"
dest="$packroot/engines"
payload="$here/payload"

[ -d "$payload/engines" ] || {
  echo "not a pack volume: $payload/engines is missing" >&2
  exit 1
}

echo "verifying $(wc -l < "$payload/MANIFEST.sha256" | tr -d ' ') files..."
if ! ( cd "$payload" && shasum -a 256 -c MANIFEST.sha256 --quiet ); then
  echo "verification FAILED — the pack is damaged; nothing was installed" >&2
  exit 1
fi

mkdir -p "$packroot"
if [ -d "$dest" ]; then
  backup="$dest.replaced-$(date +%Y%m%d-%H%M%S)"
  echo "existing pack moved to $backup"
  mv "$dest" "$backup"
fi
# ditto, not cp -R: the LibreOffice tree has symlinks and extended attributes.
# payload/engines holds vips/, ffmpeg/ and soffice/, which is exactly what
# app.ezconvert.desktop/engines must contain.
ditto "$payload/engines" "$dest"
ditto "$payload/licenses" "$packroot/licenses"
cp "$payload/MANIFEST.sha256" "$packroot/MANIFEST.sha256"

printf 'installed: %s\n' "$dest"
for probe in vips/vips ffmpeg/ffmpeg soffice/program/soffice; do
  if [ -f "$dest/$probe" ]; then
    printf '  present  %s\n' "$probe"
  else
    printf '  MISSING  %s\n' "$probe"
  fi
done
echo
echo "Open EZ Convert. If macOS blocks the unsigned app, right-click it and choose Open once."
