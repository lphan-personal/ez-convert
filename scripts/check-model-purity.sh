#!/usr/bin/env bash
# Enforce the module boundary from ARCHITECTURE.md §2.1/§2.2.
#
# Two rules, in order of how much they buy:
#
# 1. NOTHING under model/ may import tauri, spawn a process or pull in an async
#    runtime. That is the architecture's own mechanical check (§2.1) and it is
#    what keeps engine construction inside engine/.
# 2. The detection core — types.rs, detect.rs, matrix.rs, test_support.rs — is
#    std-only, no third-party crate at all. That is what lets
#    scripts/test-model.sh compile and test it with plain rustc on a host with
#    no GTK and no serde.
#
# Settings/naming/settings_schema legitimately use serde for our own JSON
# document (`Cargo.toml` explains why that is not a third-party *file parser*),
# so the crate list is allow-listed per file rather than banned globally.
#
# The day this needs to relax, the workspace split happens first (ARCHITECTURE
# §2.1) — not the rule.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
model_dir="$repo_root/src-tauri/src/model"

if [ ! -d "$model_dir" ]; then
  echo "::error::model/ is missing at $model_dir"
  exit 1
fi

mapfile -t files < <(find "$model_dir" -name '*.rs' -type f | sort)
if [ "${#files[@]}" -eq 0 ]; then
  echo "::error::no Rust files under $model_dir"
  exit 1
fi

# Comments are stripped before matching, so documenting the rule (as mod.rs
# does) never trips it.
awk '
  function core_file(name) {
    return name ~ /\/(types|detect|matrix|test_support|mod)\.rs$/
  }
  function strip_comments(line,   i) {
    sub(/^[ \t]+/, "", line)
    if (line ~ /^(\/\/|\/\*|\*)/) return ""
    i = index(line, "//")
    if (i > 0) line = substr(line, 1, i - 1)
    return line
  }
  {
    code = strip_comments($0)
    if (code == "") next

    # 1. forbidden dependencies, everywhere in model/
    #    `std::process::id()` is fine (no child process); spawning is not.
    if (code ~ /(^|[^A-Za-z0-9_-])(tauri|tokio|async_std)/ ||
        code ~ /std[ \t]*::[ \t]*process[ \t]*::[ \t]*Command/) {
      printf "forbidden dependency: %s:%d: %s\n", FILENAME, FNR, $0
      bad = 1
    }

    # 2. imports
    if (code ~ /^[ \t]*(pub[ ]+)?use[ \t]+/) {
      if (code ~ /^[ \t]*(pub[ ]+)?use[ \t]+(std|super|crate|self)[: ]/) next
      # local type/enum imports (`use ControlKind::*;`) are not crates
      if (code ~ /^[ \t]*(pub[ ]+)?use[ \t]+[A-Z][A-Za-z0-9_]*::/) next
      if (core_file(FILENAME)) {
        printf "third-party import in the std-only core: %s:%d: %s\n", FILENAME, FNR, $0
        bad = 1
      } else if (code !~ /^[ \t]*(pub[ ]+)?use[ \t]+(serde|serde_json)[: ]/) {
        printf "unexpected import (not serde/serde_json): %s:%d: %s\n", FILENAME, FNR, $0
        bad = 1
      }
    }
  }
  END { exit bad ? 1 : 0 }
' "${files[@]}"

echo "model purity: ok (no tauri/process/async anywhere; detection core is std-only)"
