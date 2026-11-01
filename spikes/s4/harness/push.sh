#!/usr/bin/env bash
# push.sh — base64-pipe one or more local files to C:\build\s4 on Ryo-PC (scp is unreliable there).
set -euo pipefail
KEY=/opt/data/.ssh/pc_ed25519
trap 'rm -f /tmp/push.$$.b64' EXIT
for f in "$@"; do
  base=$(basename "$f")
  base64 -w0 "$f" > /tmp/push.$$.b64
  ssh -i "$KEY" -o BatchMode=yes -o StrictHostKeyChecking=no ryoin@10.0.0.103 \
      "C:\msys64\usr\bin\bash.exe -lc \"cat > /c/build/s4/$base.b64\"" < /tmp/push.$$.b64
  ssh -i "$KEY" -o BatchMode=yes -o StrictHostKeyChecking=no ryoin@10.0.0.103 \
      "C:\msys64\usr\bin\bash.exe -lc \"cd /c/build/s4 && base64 -d $base.b64 > $base && rm -f $base.b64 && ls -l $base\""
done
