# C3 — document pipeline spike harness

The document engine is LibreOffice, spawned as a sidecar. This directory holds the
end-to-end matrix that proves the pipeline on the two legs the product ships on, and
the negative controls the card's acceptance criteria name.

The Rust code under test is the product code — `engine/soffice.rs` (argv, launcher,
profile URL, warm-up) and `engine/doc.rs` (batch → attribute → verify). Nothing here
re-implements it.

## Run

```bash
spikes/c3/run-e2e.sh                    # linux: vendored TDF tree on this box
spikes/c3/run-e2e.sh --windows          # cross-compile, copy to win11-dev, run over ssh
spikes/c3/run-e2e.sh --measure-legacy   # also measure directions the matrix does not advertise
```

Results land in `/opt/data/profiles/wright/cache/scratch/c3-*.log` and, edited into prose,
in `evidence/c3-document.md`.

## What the harness needs

| leg | requirement |
|---|---|
| linux | `EZC_SOFFICE_DIR` (default: `/opt/data/converter/vendor/libreoffice-26.8.2/opt/libreoffice26.8/program`) and `EZC_ENGINE_LIB_DIR` (default: `/opt/data/converter/vendor/lo-deps/usr/lib/x86_64-linux-gnu`, the X11 libs `oosplash` links). |
| windows | a **Linux-hosted** mingw cross toolchain at `MINGW_BIN` (default `.../mingw-cross/x86_64-w64-mingw32-cross/bin`). Fetch once: `curl -sSL https://musl.cc/x86_64-w64-mingw32-cross.tgz \| tar -xz -C <dir>`. Note: the winlibs zip is a *Windows-hosted* toolchain and cannot run on Linux. |
| windows | ssh to `WIN_HOST` (default `ryo@10.0.0.191`) with `WIN_KEY` (default `/opt/data/.ssh/win11-dev`), and LibreOffice installed at `C:\Program Files\LibreOffice\program`. |

## Why cross-compile instead of a PowerShell script

The Windows risk is not "can LibreOffice convert" — S3 answered that. The risk is the
**launcher rule** (`soffice.com`, not `soffice.exe`/`.bin`), the **profile URL**
(`file:///C:/…`, not `file://` + `C:\…`) and the **output attribution** (`<stem>-<sheet>.csv`).
Those are our code. Running the same Rust function on the guest is the only way to prove
them; a shell re-implementation would prove the shell script.

## Harness layout

```
spikes/c3/e2e.rs        the matrix: 7 advertised directions, batching, controls, legacy sweep
spikes/c3/run-e2e.sh    rustc harness generation (no cargo, no Tauri, no GTK) + both legs
```

`run-e2e.sh` generates a `main.rs` that includes the real modules by path — the same trick
`scripts/test-engine.sh` uses, because the crate depends on Tauri and this box has no
GTK/WebKit.

## Traps this harness hit, recorded so the next card does not

- **`Text file busy` (os error 26)** when two harness runs share the build directory: the
  first run execs the binary while the second recompiles it. The build dir is now `$$`-scoped.
- **A leftover `soffice.bin` holds a profile lock.** Kill stale engines between runs; the
  pipeline's own answer is a per-job profile plus `--nolockcheck`, but the harness workdir
  is shared across cases.
- **`env_clear` and the `soffice` wrapper.** On POSIX `soffice` is a shell script that calls
  `dirname`, `basename`, `ls`, `sed`; a PATH containing only the engine directory makes it
  fail before it reaches `soffice.bin`. `proc::run` therefore constructs
  `<engine dir>:/usr/bin:/bin` for this engine — still constructed, never inherited.
