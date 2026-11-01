#!/usr/bin/env bash
# Emit the sha256 manifest for the dist that the avif suite is about to be run against.
set -uo pipefail
D="${D:-/c/build/dist}"
OUT="${OUT:-/c/build/FINAL-dist-spec-aom-sha256.txt}"
cd "$D" || exit 1
COUNT=$(find . -type f | wc -l)
BYTES=$(find . -type f -printf '%s\n' | awk '{s+=$1} END{print s}')
{
  echo "# FINAL vips dist (spec-aom + libtiff-without-JBIG relink, S2c) — manifest"
  echo "# generated: $(date -u +%Y-%m-%dT%H:%M:%SZ) on $(hostname)"
  echo "# dist: $D"
  echo "# files: $COUNT   total_bytes: $BYTES"
  echo "# format: <sha256>  <size_bytes>  <path>"
  find . -type f | LC_ALL=C sort | while read -r f; do
    printf '%s  %10d  %s\n' "$(sha256sum "$f" | cut -d' ' -f1)" "$(stat -c %s "$f")" "$f"
  done
} > "$OUT"
wc -l "$OUT"
