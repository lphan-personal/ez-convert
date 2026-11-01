#!/usr/bin/env bash
# s3 — LibreOffice document direction matrix.
#
# Proves each direction the app needs (docx->pdf, docx->md, docx->html,
# csv->xlsx, xlsx->csv, html->docx, pptx->pdf) with the output file asserted on
# disk *and* content-checked, plus cold/warm wall times and the no-output trap
# controls. See spikes/s3/run_s3.py for the matrix itself.
#
# Exits non-zero when a required direction produces no valid file, so a silent
# no-output conversion can never pass this spike.

set -euo pipefail

SPIKE_ID="${SPIKE_ID:-s3}"
OUT="spikes/${SPIKE_ID}/out"
mkdir -p "$OUT"

echo "== ${SPIKE_ID}: python verification deps =="
python3 -m pip install --quiet --disable-pip-version-check \
  "python-docx==1.2.0" "openpyxl==3.1.5" "python-pptx==1.0.2" "pypdf==6.21.4"

echo "== ${SPIKE_ID}: libreoffice =="
if ! command -v soffice >/dev/null 2>&1; then
  sudo apt-get update
  sudo apt-get install -y --no-install-recommends \
    libreoffice-writer libreoffice-calc libreoffice-impress \
    fonts-crosextra-carlito fonts-crosextra-caladea
fi
soffice --version | tee "$OUT/soffice-version.txt"

echo "== ${SPIKE_ID}: direction matrix =="
set +e
python3 spikes/s3/run_s3.py \
  --soffice "$(command -v soffice)" \
  --out "$OUT" \
  --reps 3 \
  --controls 2>&1 | tee "$OUT/run.log"
rc=${PIPESTATUS[0]}
set -e

echo "== ${SPIKE_ID}: verdict rc=${rc} =="
[ -f "$OUT/table.md" ] && cat "$OUT/table.md"
exit "$rc"
