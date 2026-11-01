#!/usr/bin/env bash
# Build the U3 acceptance spike for Windows, and run the shipped-module unit tests on
# this (Linux) host. Nothing here is part of the product build.
#
#   ./build.sh            # cross-compile u3_spike.exe + run the Linux unit tests
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
export PATH="$HOME/.cargo/bin:/opt/data/profiles/wright/cache/scratch/mingw-cross/x86_64-w64-mingw32-cross/bin:$PATH"
out="${OUT:-$here/out}"
mkdir -p "$out"

echo "== cross-compiling u3_spike.exe (shipped shell modules included by #[path])"
rustc --edition 2021 -O \
  --target x86_64-pc-windows-gnu \
  -o "$out/u3_spike.exe" \
  "$here/u3_spike.rs"
ls -l "$out/u3_spike.exe"

echo "== Linux unit tests over the same shipped modules"
test_harness="$(mktemp -d)/u3_tests.rs"
{
  echo '#![allow(dead_code)]'
  echo 'pub mod shell {'
  echo "    #[path = \"$repo/src-tauri/src/shell/geometry.rs\"] pub mod geometry;"
  echo "    #[path = \"$repo/src-tauri/src/shell/accelerator.rs\"] pub mod accelerator;"
  echo "    #[path = \"$repo/src-tauri/src/shell/hotkey.rs\"] pub mod hotkey;"
  echo "    #[path = \"$repo/src-tauri/src/shell/screen.rs\"] pub mod screen;"
  echo "    #[path = \"$repo/src-tauri/src/shell/grab.rs\"] pub mod grab;"
  echo '}'
  echo 'fn main() { println!("unit-test harness"); }'
} > "$test_harness"
rustc --edition 2021 --test -o "$out/u3_tests" "$test_harness"
"$out/u3_tests"
