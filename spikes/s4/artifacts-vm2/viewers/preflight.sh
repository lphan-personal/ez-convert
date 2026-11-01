#!/usr/bin/env bash
# preflight.sh — what the avif suite needs, on the win11-dev VM
set -u
echo "bash        : $BASH_VERSION  ($(uname -s))"
echo "python3     : $(python3 --version 2>&1)  -> $(command -v python3)"
echo "objdump     : $(objdump --version 2>&1 | head -1)  -> $(command -v objdump)"
for t in stat awk sed grep od head ls mkdir rm date sha256sum base64; do
  printf '%-10s %s\n' "$t" "$(command -v $t || echo MISSING)"
done
echo "dist files  : $(ls /c/build/dist | wc -l)"
echo "fixtures    : $(ls /c/build/fx)"
echo "icc         : $(stat -c %s '/c/Windows/System32/spool/drivers/color/sRGB Color Space Profile.icm' 2>&1) bytes"
echo "objdump on vips.exe:"; objdump -p /c/build/dist/vips.exe | grep -c 'DLL Name:'
