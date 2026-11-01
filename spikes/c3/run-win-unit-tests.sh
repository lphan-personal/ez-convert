#!/usr/bin/env bash
# Cross-compile the engine+model unit tests for Windows and run them on win11-dev.
# Ad-hoc: the CI matrix runs `cargo test` natively on windows-latest, but this box
# cannot run cargo for the Tauri crate, so the rustc harness is what we have.
set -euo pipefail
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
src="$repo/src-tauri/src"
work="${TMPDIR:-/tmp}/ezc-c3-win-tests"
mkdir -p "$work"
export PATH="$HOME/.cargo/bin:/opt/data/profiles/wright/cache/scratch/mingw-cross/x86_64-w64-mingw32-cross/bin:$PATH"

cat >"$work/harness.rs" <<EOF
#![allow(dead_code)]
pub mod model {
    #[path = "$src/model/types.rs"]
    pub mod types;
    #[path = "$src/model/detect.rs"]
    pub mod detect;
    #[path = "$src/model/test_support.rs"]
    pub mod test_support;
    #[path = "$src/model/matrix.rs"]
    pub mod matrix;
}
pub mod engine {
    #[path = "$src/engine/proc.rs"]
    pub mod proc;
    #[path = "$src/engine/stderr_rules.rs"]
    pub mod stderr_rules;
    #[path = "$src/engine/vips.rs"]
    pub mod vips;
    #[path = "$src/engine/ffmpeg.rs"]
    pub mod ffmpeg;
    #[path = "$src/engine/plan.rs"]
    pub mod plan;
    #[path = "$src/engine/soffice.rs"]
    pub mod soffice;
    #[path = "$src/engine/verify.rs"]
    pub mod verify;
    #[path = "$src/engine/convert.rs"]
    pub mod convert;
    #[path = "$src/engine/doc.rs"]
    pub mod doc;
    pub mod tags {
        use std::path::Path;
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct MergeReport { pub items: usize, pub pictures: usize, pub tag_type: Option<()>, pub source_untagged: bool }
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum TagError { Unavailable }
        impl std::fmt::Display for TagError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "lofty not linked") }
        }
        pub fn merge(_a: &Path, _b: &Path) -> Result<MergeReport, TagError> { Err(TagError::Unavailable) }
    }
}
EOF

EZC_REPO_ROOT="C:/ezc/repo" rustc --edition 2021 --test --target x86_64-pc-windows-gnu \
  -C linker=x86_64-w64-mingw32-gcc -o "$work/engine-tests.exe" "$work/harness.rs"

# The model tests read fixtures and generated docs through EZC_REPO_ROOT, so the
# tree they need has to exist on the guest at that exact path.
ssh_opts=(-i /opt/data/.ssh/win11-dev -o StrictHostKeyChecking=no)
tar -C "$repo" -cf - fixtures docs evidence spikes src-tauri/src | ssh "${ssh_opts[@]}" ryo@10.0.0.191 'mkdir C:\ezc\repo 2>nul & tar -xf - -C C:/ezc/repo'
scp "${ssh_opts[@]}" "$work/engine-tests.exe" ryo@10.0.0.191:C:/ezc/engine-tests.exe
ssh "${ssh_opts[@]}" ryo@10.0.0.191 "C:\\ezc\\engine-tests.exe --test-threads=4"
