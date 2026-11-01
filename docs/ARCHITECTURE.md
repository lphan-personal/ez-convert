# EZ Convert — Architecture

Status: **ratified by the engineering lead, 2026-11-01** (card `A1`, `t_3c0dfcf9`).
Authority chain: `SPEC.md` (stakeholder, locked) → this document (engineering) → code.
Where this document and `SPEC.md` disagree, `SPEC.md` wins and this document is the bug.

Audience: the workers implementing `C1`-`C4` (conversion + output rules), `U1`-`U3` (wheel,
window, tray/hotkey), `P2` (packaging) and `warden` (security/licence review).

Canonical copy: `/opt/data/converter/ARCHITECTURE.md`.
A byte-identical mirror is committed at `docs/ARCHITECTURE.md` in the repo so review and the
CI deny-list gate see it. They must never diverge — `cmp` before every push.

---

## 0. The one rule that shapes everything

**A capability exists when evidence exists, not when code exists.**

Every route in the target matrix (`§8`) carries a `status`:

```rust
enum RouteStatus {
    /// Measured end to end on a named host, with a checked output file and a
    /// committed reproduction. `EvidenceId` points at a file under `evidence/`.
    Proven { evidence: EvidenceId },
    /// The engine is licensed, pinned and understood, but the exact invocation
    /// has not been measured here. Never reachable from the UI.
    Planned { owner: CardId, hypothesis: &'static str },
    /// Measured and rejected (licence, fidelity, or a silent-failure mode).
    Cut { reason: &'static str },
}
```

Consequences, stated plainly because they change what ships:

- The wheel only ever offers targets whose route is `Proven`. A `Planned` route is
  invisible, not greyed out (`SPEC.md` §2 already requires absent-not-greyed).
- A unit test asserts every `Proven` route names an evidence file that exists on disk and
  every `Planned` route names a live card. Adding a target without evidence fails CI.
- At the time of writing, `Proven` covers **six measured image pairs through libvips** and
  **seven document directions through LibreOffice** (`§6.1`, `§6.2`). Audio, video, PDF raster
  input, SVG input, AVIF/GIF/BMP output, legacy `.doc/.xls/.ppt`, and most of the image grid are
  **`Planned`**. That is the honest state of the world after S1-S3, not a scope reduction:
  `C2`, `C3` and `C4` exist to move them.

---

## 1. System overview

```
┌────────────────────────────────────────────────────────────────────────────┐
│ one process: ez-convert.exe (Tauri 2 shell + Rust core)                    │
│                                                                            │
│  ui/ (static HTML/CSS/JS)                                                  │
│    wheel.html ──┐                                                          │
│    window.html ─┴──► shell/ipc.rs ──► job/queue.rs ──► engine/<driver>.rs  │
│                                         │                  │                │
│                       model/detect.rs ◄─┘                  │ argv + env     │
│                       model/matrix.rs                      ▼                │
│                       model/naming.rs               proc.rs (spawn+reap)    │
│                       job/journal.rs                        │                │
└─────────────────────────────────────────────────────────────┼──────────────┘
                                                              │ pipes, no shell
        ┌──────────────┬───────────────┬──────────────┬───────┴────────┬───────────┐
        ▼              ▼               ▼              ▼                ▼           ▼
   vips.exe +      ffmpeg.exe      soffice.exe     ezc-pdf.exe      resvg.exe   (none:
   DLL closure     ffprobe.exe     + LO tree       (PDFium)                      in-process
   (resources)     (externalBin)   (resources)     (externalBin)    (externalBin) Rust crates)
```

Two surfaces, one core (`SPEC.md` §1): the **wheel** overlay and the **window**. Both are
webviews over the same `shell/ipc.rs` command surface; neither does conversion work itself.

Process model, non-negotiable (stakeholder decision + `warden` audit C-3):

- Engines are **separate processes**. The app never loads libvips/libheif/libav\*/LibreOffice
  into its own address space, so the LGPL relink obligation never arises and an engine crash
  is a failed job, not a dead app.
- The app makes **no network calls** at runtime, ever (`SPEC.md` §1). No update check, no
  telemetry, no crash reporting, no font CDN.
- One engine invocation = one job = one child process. Measured spawn cost ≈ 13 ms
  (`media-engine-stack-2026.md` §0), so per-file processes are affordable and keep blast
  radius at one file.

---

## 2. Crate and module layout

### 2.1 Decision: one crate, strict module boundaries (for v1)

The repo keeps a single package (`src-tauri`, lib name `ez_convert_lib`) for v1. A Cargo
workspace split into `crates/ezc-core` + the Tauri shell is **deferred**, because:

1. `cargo test --locked` in `src-tauri` is the existing green CI path; a workspace moves
   `Cargo.lock` to a new root and puts `--locked` at risk for zero functional gain today.
2. The core modules are already Tauri-free by construction (below), so the testability
   benefit of a separate crate is available without the build-system churn.
3. `SPEC.md` §3 needs no CLI surface in v1. An agent-first CLI is the natural trigger for
   the split, and it is not v1 scope.

The boundary is enforced mechanically, not by intention:

```bash
# must print nothing
grep -rn --include=*.rs -E '^\s*use (tauri|tauri_plugin)' src-tauri/src/{model,engine,job,platform}/
```

If that grep ever fires, the split happens first and the feature second.

### 2.2 Target tree

```
src-tauri/src/
├── main.rs                 # thin binary; windows_subsystem attr; calls ez_convert_lib::run()
├── lib.rs                  # Builder, plugin registration, state, command+event registry
│
├── model/                  # pure data + pure rules. No I/O, no tauri, no async.
│   ├── types.rs            # CanonicalType, TargetFormat, Route, JobId, DetectedFile
│   ├── detect.rs           # magic-byte sniffing: &[u8] -> Detected { type, integrity, claimed }
│   ├── matrix.rs           # CanonicalType -> &[Route]; the RouteStatus table (§8)
│   ├── naming.rs           # collision suffixing, output path derivation, path admission
│   ├── errors.rs           # AppError + the user-facing message table (§14)
│   └── settings.rs         # Settings schema, defaults, load/save/migrate (§15)
│
├── engine/                 # the only place a child process is created
│   ├── mod.rs              # Route -> ExecPlan dispatch; timeouts; cancellation token
│   ├── proc.rs             # ChildSpec -> Outcome: scrubbed env, pipes, caps, kill (§6.0)
│   ├── stderr_rules.rs     # "exit 0 is not success" pattern rules (§6.0.4)
│   ├── verify.rs           # per-target output validators (§6.0.5)
│   ├── vips.rs             # argv builder; Op is a closed enum (§6.1)
│   ├── soffice.rs          # argv builder + warm-profile worker (§6.2)
│   ├── ffmpeg.rs           # argv builder + progress parser (§6.3, Planned)
│   ├── pdfium.rs           # talks to the first-party ezc-pdf sidecar (§6.4, Planned)
│   ├── resvg.rs            # svg -> png hop (§6.5, Planned)
│   └── toolpath.rs         # resolve engines under resource_dir, verify version pin
│
├── job/
│   ├── mod.rs              # Job, JobState, JobEvent
│   ├── queue.rs            # admission, batching, ordering, per-engine lanes
│   ├── worker.rs           # worker threads; kill-on-cancel; partial-output cleanup
│   ├── progress.rs         # per-file state; determinate only where the engine reports
│   └── journal.rs          # undo journal (§13)
│
├── platform/               # raw OS calls; tauri-free (pass hwnd as isize)
│   ├── win32.rs            # window styles, regions, monitors, hotkey plumbing helpers
│   ├── monitors.rs         # cursor -> monitor work area, fit-rect calculation
│   └── macos.rs            # (post-Windows; stub that returns Unsupported)
│
└── shell/                  # the only tauri-aware layer
    ├── ipc.rs              # #[tauri::command] fns + event emit helpers
    ├── wheel.rs            # overlay window lifecycle + wheel state machine host (§10)
    ├── main_window.rs      # queue/batch/settings surface host (§11)
    ├── tray.rs             # tray (Windows) / menu bar (macOS)
    └── hotkey.rs           # registration, conflict detection, re-register on change
```

Rules of the tree:

- `model/` is pure. Detection, matrix, naming, settings and error text are unit-testable
  with no filesystem, no window and no engine. This is where most tests live.
- `engine/` owns `std::process::Command` **exclusively**. No other module may spawn.
- `job/` owns filesystem mutation of outputs and the journal. `engine/` writes only inside
  the job's temp directory.
- `shell/` owns tauri types. It translates commands into `job` calls and `job` events into
  window events, and does nothing else.

### 2.3 State owned by the shell

```rust
struct AppState {
    settings: RwLock<Settings>,        // persisted, §15
    queue: Arc<JobQueue>,              // job/queue.rs
    engines: EngineSet,                // resolved + version-checked at startup, §6.7
    wheel: Mutex<WheelState>,          // §10 state machine (authoritative, not the webview)
    window: WindowHandles,             // main + wheel webview handles
    recent: RwLock<RecentUses>,        // MRU targets for smart defaults; never leaves disk
}
```

The wheel's **authoritative** state lives in Rust. The webview renders it and reports
intent (segment hovered, segment picked); it never decides whether the wheel is open.

---

## 3. The pipeline, end to end

```
hotkey ──► shell/hotkey.rs ──► shell/wheel.rs: show at cursor
                                     │
drag from Explorer/Finder ──► Tauri DragDropEvent::Drop { paths }
                                     │
                                     ▼
                    model::detect   ── content sniff, not extension (§7)
                                     │  DetectedFile { type, integrity, claimed_ext }
                                     ▼
                    model::matrix   ── types ∩ user preference ──► valid targets (§8)
                                     │  (multi-file drop = intersection, §8.3)
                                     ▼
                    ui: wheel shows only valid segments (≤6 + "more")
                                     │  user picks a target
                                     ▼
                    job::queue      ── admission, naming, journal intent (§12, §13)
                                     │
                                     ▼
                    engine::run     ── ExecPlan -> proc.rs -> child (§6)
                                     │  stdout/stderr captured, timeout, kill
                                     ▼
                    engine::verify  ── output exists + parses + matches target (§6.0.5)
                                     │
                     ┌───────────────┴───────────────┐
                     ▼                               ▼
              job::journal commit            AppError -> message table (§14)
              toast + 10 s Undo                      │
                     │                               ▼
                     └─────────────► window queue + wheel toast with reason
```

Two invariants fall out of this and are worth stating separately:

1. **Detection happens before any target is offered.** A file whose content is not a
   document never sees document targets, therefore a mislabelled file cannot reach
   LibreOffice at all. This is measurable: today, a text file renamed `broken.docx` makes
   LibreOffice write a 12,388-byte PDF and exit 0 with empty stderr (`§6.2`, measured
   2026-11-01). Content-first detection is the structural fix for that class of bug.
2. **Success is a verified artifact**, never an exit code. See `§6.0.5`.

---

## 4. Threading, concurrency, cancellation

| lane | workers (default) | why |
|---|---|---|
| `vips` | `min(4, cores/2)` | stateless, no shared profile, scales |
| `soffice` | **1** | one warm profile must be serialised (S3 §4/§5.4: cold bootstrap dominates; a warm profile is 1.2-2.3× faster) |
| `ffmpeg` | 2 | CPU-saturating; a long video must not block images |
| `pdfium` | 2 | page rasterisation is bounded per page |
| `resvg` | 1 | single-file, short-lived |

- Core uses **std threads**, not async. Tauri commands are `async` only at the boundary and
  hand work to the queue. No tokio in `model`/`engine`/`job` (keeps the dependency graph and
  the licence surface small).
- Cancellation is a `CancelToken` (atomic bool + child handle). Cancelling a job kills the
  child, waits for exit, and deletes the job's temp directory. A cancelled job never
  produces a visible output.
- Long jobs emit `job:progress` at most every 100 ms; the UI must not be driven by engine
  output rate (a 4 GB video can emit thousands of lines).

---

## 5. State machines

### 5.1 Job

```
Queued ─► Preparing ─► Running ─► Verifying ─► Done
   │          │           │           │
   │          │           │           └─► Failed(Verification)
   │          │           └─► Failed(Engine) | Cancelled | TimedOut
   │          └─► Failed(OutputPath | Journal)
   └─► Failed(Detect | NoRoute)
```

`Verifying` is a real state, not a formality: it is where `§6.0.5` runs. A job can only
reach `Done` after the output passed its validator and the journal committed.

### 5.2 Wheel (detail in `§10`)

```
Hidden ─► Summoning ─► Empty | Choosing ─► Confirming ─► Converting ─► Toast ─► Hidden
                            │                  │
                            └──── Dismissing ◄─┘   (esc / click-away / pick-nothing)
```

---

## 6. Engine invocation contracts

Everything in this section is a contract with the *process*, not with a library. The
acceptance rule for this document applies: **an argv example exists only if a spike
measured it**, and every example below names its evidence. Anything not measured is marked
`PLANNED — do not implement from this document alone` and must be proven by its card before
it appears in the wheel.

### 6.0 The common spawn contract (`engine/proc.rs`)

Every engine spawn goes through one function. Nothing else in the app constructs a
`Command`. The rules below are release-blocking: `warden`'s audit of the S2 dist
(`t_cccf4bf3`, condition C-3) proved each hazard against the real binary.

#### 6.0.1 argv construction

| # | Rule | Proven hazard |
|---|---|---|
| A1 | `argv[0]` is an absolute path resolved from the app's own install root (`resource_dir()` / `externalBin`), canonicalised, and asserted to start with that root. Never `PATH`, never a relative name. | Binary planting; the packaging doc's `sidecar()` resolution caveat |
| A2 | `argv[1]` for `vips.exe`/`vipsheader.exe` is a **literal from a closed Rust enum** (`Op::Copy`, `Op::TiffLoad`, …). User data can never occupy it. | `vips system "cmd.exe /c …"` executed on the build box — a proven arbitrary-command primitive |
| A3 | Every path argument is absolute, canonicalised, and rejected if it begins with `-` (leading-dash filenames are refused rather than escaped). | A file named `--vips-config` is parsed as an option: config printed, no conversion |
| A4 | UNC and device paths (`\\host\share`, `//host/share`, `\\.\`, `\\?\`) are rejected before spawn. | They reach the Win32 file layer directly (SMB/NTLM relay surface) |
| A5 | The output path is derived by us, not by the engine, and canonicalised inside the job's output root. Traversal (`..`) is impossible by construction, and re-checked. | `vips copy t.ppm ../out.ppm` writes outside CWD as-is |
| A6 | Never a shell. No `cmd.exe`, no `sh -c`, no string concatenation of arguments. Filter names with spaces are **one argv element with no quote characters**. | A quoted LibreOffice filter name is handed literal quote bytes and fails with rc=1 |
| A7 | Arguments are enumerated per call site. No wildcard/`args: true` passthrough in the Tauri capability file. | Packaging doc §3.3: argument smuggling (`-f`, `concat:`) |

The builders are pure functions returning `Vec<OsString>`, which makes A2/A3/A7 unit-testable
without spawning anything:

```rust
#[test]
fn user_text_can_never_reach_argv0_or_argv1() {
    for hostile in ["system", "--vips-config", "-o", "\\\\srv\\share\\x.png", "../out.png"] {
        let plan = build_vips(Op::Copy, hostile, "/safe/out.png");
        assert!(plan.is_err() || plan.unwrap().argv[1] == "copy");
    }
}
```

#### 6.0.2 Environment and working directory

Spawn with a **constructed** environment, not an inherited one:

| variable | value | why |
|---|---|---|
| `PATH` | the engine's own directory only | no accidental PATH lookup of a DLL/tool |
| `TMP`/`TEMP` | the job's temp directory | keeps spill files together and cleanable |
| `HOME` (mac/Linux), `USERPROFILE` (Win) | the job's temp dir | LibreOffice writes profile/registry state to HOME if `-env:UserInstallation` is ever missed |
| `VIPS_*` | **absent** | vips reads tunables from the environment |
| `GIO_MODULE_DIR`, `GIO_EXTRA_MODULES`, `GLIB_*` | **absent** | glib/gio can load modules from the environment or `<dist>\gio\modules` |
| `LD_LIBRARY_PATH` / `DYLD_*` | **absent** unless the platform needs it | library substitution |
| `SAL_USE_VCLPLUGIN=svp`, `SAL_DISABLE_OPENCL=1`, `DISPLAY` unset | soffice only | measured in S3: headless, no display, no GPU probing |

CWD: the engine's own directory for `vips` (its DLLs are siblings), the job temp dir for
everything else. Never the bundle directory — on macOS writing there breaks the code
signature (packaging doc §2).

#### 6.0.3 Timeouts, output caps, reaping

- Wall-clock timeout per invocation, enforced by a monitor that kills the child and its
  process group (Windows: job object; POSIX: kill the pid, then sweep orphans by job id).
- Defaults: `vips` 120 s, `ezc-pdf` 120 s, `resvg` 60 s, `soffice` 240 s (S3 used 240 s per
  run and its worst measured cold case was 1217 ms), `ffmpeg` no fixed cap but a stall
  detector (no progress line for 120 s ⇒ kill). **All of these defaults are untested
  against large inputs** — they are safety limits, not performance claims.
- stdout/stderr are captured to bounded buffers (1 MiB each, head + tail kept) and written to
  the job log. A pathological engine cannot exhaust the app's memory.

#### 6.0.4 "Exit 0 is not success" — the stderr rule engine

Measured on the shipped vips dist (`t_65624d81`): a JBIG-compressed TIFF decoded with exit 0,
an output PNG on disk, and a fully black image. For the `tiffload` variant the only signal was
on stderr; for the plain `copy` variant the probe recorded exit 0, 523 bytes and mean pixel
0.000000 — stderr was not even captured, which is exactly why the loader flag is the primary
control and this rule is defence in depth.

```rust
// engine/stderr_rules.rs
const FATAL_PATTERNS: &[(&str, AppErrorKind)] = &[
    ("tiff2vips:", AppErrorKind::DecodeFailed),
    ("heifload:", AppErrorKind::DecodeFailed),
    ("jpeg2vips:", AppErrorKind::DecodeFailed),
    ("pngload:", AppErrorKind::DecodeFailed),
    ("webpload:", AppErrorKind::DecodeFailed),
    ("VIPS-WARNING", AppErrorKind::DecodeFailed),
];
```

A run is **fatal** if any of:

1. exit code ≠ 0, or the child was killed (timeout/cancel), or
2. stderr matches a fatal pattern (even with exit 0 and an output file present), or
3. the expected output file is missing or zero-length, or
4. the output fails its validator (`§6.0.5`).

Rule 2 is subordinate to the loader flag: for TIFF input we *always* pass
`--fail-on error` to the named loader, so the common case fails loudly with exit 1 and no
output. The stderr rule remains as defence in depth for the other loaders, whose `fail-on`
behaviour has not been measured one by one.

**Deliberate non-goal:** we do not try to detect "silent bad pixels" (a black frame, a
half-decoded image). A genuinely black image is valid output, so post-hoc content heuristics
would delete good work. The contract is the loader flag plus the stderr rule, exactly as
recorded on the card.

#### 6.0.5 Output verification (`engine/verify.rs`)

`Done` requires a validator to pass. Validators are cheap and format-specific:

| target | validator |
|---|---|
| any image | magic bytes match the target family (re-uses `model::detect`), plus a decodable header (vips header read or the `image` crate) |
| pdf | `%PDF-` header, trailer present, page count ≥ 1 |
| xlsx/docx/pptx | ZIP with the required part (`xl/workbook.xml`, `word/document.xml`, `ppt/presentation.xml`) |
| csv | parses as delimited text, ≥ 1 row, consistent column count ≥ 1 |
| md/html/txt | valid UTF-8 (or declared encoding), non-empty, no NUL bytes |
| audio/video | `ffprobe` reports a duration > 0 and the expected stream kind (Planned) |

Two things this does **not** catch, stated so nobody assumes otherwise:

- a PDF built from garbage is still a syntactically valid PDF — measured: a text file renamed
  `.docx` produces a 12,388 B, **1-page, valid-looking PDF** (`§6.2` N3), which passes every
  cheap validator. Detection is the guard for that case, and it only works because the file is
  *not* a document by content;
- a lossy re-encode that is visually wrong is still a valid file (fixture-based fidelity tests,
  `§19`, are the guard).

Residual risk, named rather than hidden: a file that passes structural detection (real ZIP,
real `word/document.xml`) but whose XML is corrupt can still yield a plausible-looking PDF that
passes verification. There is no cheap oracle for that in-process; `C3`'s corrupt-input test
covers the cases we can catch (truncated ZIP, plain text renamed, wrong container) and the queue
must report the job as `Done` with the file — claiming otherwise would be the fabrication this
project is trying to avoid.

#### 6.0.6 Egress

No engine needs a network. Verified in `t_cccf4bf3`: no vips loader treats `http://`,
`https://` or `file:` as a URL — they are plain filenames. glib/gio do carry socket/DNS
imports, so:

- the app has no network code and no HTTP dependency;
- sidecars are spawned with no proxy environment and no credentials in env;
- where the platform allows it, deny egress to the engine processes (Windows: no per-process
  firewall rule without elevation, so this is **best effort** and the primary control is
  "the argv can never make it fetch"); `warden` reviews this in the release gate.

### 6.1 libvips — image conversions (`PROVEN`)

Dist: self-built 8.18.7, `VARIANT=spec-aom`, 43 files / 35 binaries / 35,731,757 B, no GPL
components in the link graph (`image quantisation … false`, `PDF load … false`, `modules:
false`), `libaom` linked for AVIF encode. Layout: `vips.exe` + `vipsheader.exe` +
`vipsthumbnail.exe` + 32 DLLs + `licenses/`. **Ship the whole directory**, not the exe:
`vips.exe` is a 62 KB launcher over `libvips-42.dll` (S2 F4).

⚠ **Closed 2026-11-01 (`spikes/s4/EVIDENCE.md` §1).** The AVIF round-trip was originally proven
on the **S2b** dist (37 files / 35,649,889 B); the shipped dist was relinked afterwards by the
libtiff/JBIG rebuild. The unchanged `18-avif-tests.sh` (sha256 `d5314fbf…74224`) was re-run
against the final dist and reports **34 assertions, 0 failures** — jpg→avif→jpg with all four
`exif-ifd0-*` tags and the 3,144 B sRGB ICC surviving, orientation-6 landing at 1152×1728 with no
stale `Orientation` left to double-rotate, avif→jpg via dav1d, HEIC decode unchanged, `--encoder
aom` accepted, 0 banned imports, no `libjbig-0.dll`. The 43-file / 35,824,560 B dist tested is
hash-identical to the S2c manifest (`FINAL-dist-spec-aom-sha256.txt`, 43/43 entries, 0
mismatches). Avif output is `Proven` on the artifact that ships; nothing in the wheel moves.

Measured invocations (Windows 11 Pro 26200, MSYS2/UCRT64 build; evidence: `S2 §1`-`§3`,
`build/windows/EVIDENCE-S2-libvips.md`, `t_65624d81`):

| # | argv (paths elided) | measured result |
|---|---|---|
| V1 | `vips.exe --vips-config` | prints the build report; runnable with `env -i PATH=<dist>;C:\Windows\system32` — the dist is self-contained |
| V2 | `vips.exe copy <in.jpg> <out.png>` | exit 0, PNG written (jpg → png) |
| V3 | `vips.exe copy <in.png> <out.webp>` | exit 0 (png → webp) |
| V4 | `vips.exe copy <in.webp> <out.tif>` | exit 0 (webp → tiff) |
| V5 | `vips.exe copy <in.tif> <out.png>` | exit 0 (tiff → png) |
| V6 | `vips.exe autorot <in-orient6.jpg> <out.jpg>` | 400×200 → 200×400, `exif-ifd0-Orientation: 1`, corner pixels prove 90° CW |
| V7 | `vips.exe autorot <in-orient6.heic> <out.jpg>` | 200×400 → 200×400, orientation 1 — libheif already applied `irot`; autorot is a safe no-op on HEIC |
| V8 | `vips.exe heifsave <in.png> <out.avif> --compression av1 --Q 60 --effort 6 --bitdepth 8 --keep all` | **avif save works** on the `spec-aom` dist: 400×200 → 3,863 B, container brand `ftypavif`; the 2 MP jpg variant → 302,068 B |
| V8b | `vips.exe jpegsave <in.avif> <out.jpg> --Q 95` | avif → jpg via dav1d: geometry kept, EXIF IFD0 kept, 3,144 B sRGB ICC kept |
| V8c | `vips.exe copy <in.heic> <out.avif>` (pre-`libaom` dist) | **failed** with `heifsave: Unsupported compression` — the reason capability must be probed by doing, not by reading `--vips-config` |
| V9 | `vips.exe tiffload <in.tif> <out.png> --fail-on error` | on JBIG-tagged TIFF: exit 1, **no output**, stderr `tiff2vips: ISO JBIG compression support is not configured` |
| V10 | `vips.exe copy <in-jbig.tif> <out.png>` | exit 0, 400×200 PNG written, mean pixel 0.000000 — **silent black frame**. Never use bare `copy` for TIFF input |
| V11 | `vipsheader.exe -f compression <in.tif>` | prints the tag value; JBIG TIFF emits the `tiff2vips:` warning above |

Contract:

```
argv = [ <install>/resources/engines/vips/vips.exe,
         <op>,                                   // closed enum: literal
         <abs in>, <abs out>,                    // absolute, canonical, no leading '-'
         ...op_flags ]                           // literals only
env  = scrubbed (§6.0.2), CWD = <install>/resources/engines/vips
```

- **Op selection is by input type, not by preference.** TIFF input ⇒ `tiffload … --fail-on
  error`. JPEG/PNG/WebP/HEIC/GIF/AVIF input ⇒ the matching named loader once its `fail-on`
  behaviour is measured; until then, `copy` for those types is acceptable **only** because
  the stderr rule (`§6.0.4`) is active, and the measuring task must flip them to named
  loaders with `--fail-on error`.
- **`--vips-fail-on error` does not exist** and `--fail-on` is not accepted by `copy`
  (both rejected as unknown options when measured). The per-loader form is the only guard.
- **Orientation** is handled by `autorot`, never by hand. On the JPEG/EXIF path, honour it
  exactly once: `autorot` bakes the rotation *and* rewrites the tag. On HEIC it is a no-op.
  The double-rotation trap is on the JPEG path.
- **Metadata** is kept by default. What was measured: the autorot hops keep the EXIF block
  (V6/V7 read `exif-ifd0-Orientation` back from the output) and the build reports
  `EXIF metadata support with libexif: true` / `ICC profile support with lcms2: true`. The
  **strip** path is *not* measured on this dist: `C2` must prove which saver option actually
  removes EXIF/ICC (the research note names `--strip`, unverified here) before the "strip
  metadata" toggle ships. Until then the toggle is absent rather than fake.
- Resize/compression flags (`--Q`, `thumbnail` shrink-on-load, size limits) are **not** in
  this contract yet: none has been measured on this dist. `C2` proves them before the
  resize/compress UI appears (§8.2).
- **AVIF output names its saver and passes the measured option set**:
  `heifsave` + `--compression av1 --Q 60 --effort 6 --bitdepth 8 --keep all` (`§6.1` V8).
  Four corrections from the measurement, each of which costs a bug if ignored:
  1. the option is **`effort`** (0-9), not `speed` — `SPEC.md` §9.3 says `speed` and is wrong;
  2. `--keep all` is the default, so metadata survival does **not** depend on passing it;
  3. `bitdepth` defaults to 12 but an 8-bit source stays 8-bit; pass `--bitdepth 8` for
     determinism;
  4. **never use `--encoder` for capability detection** — `--encoder rav1e` on a build
     without rav1e prints a WARNING and returns **rc=0** with a valid fallback file. Same class
     of trap as the JBIG TIFF: exit 0 must never be the capability signal.
- **Capability is probed by doing, not by config.** `--vips-config` reported
  `HEIC/AVIF load/save: true` on the very build where avif save failed (V8c). The startup
  engine check therefore performs one tiny real conversion (a 1×1 PNG → the target format in
  the job temp dir) for each advertised saver and caches the verdict against the engine hash.
  `§6.7` treats a probe failure as `Unavailable`.
- `vipsheader.exe` / `vipsthumbnail.exe` are shipped but not used by the app in v1. If a
  card starts using them, rules A2/A3 apply identically (`vipsthumbnail` takes an operation
  name too).

### 6.2 LibreOffice — document conversions (`PROVEN`, 7 directions, Linux **and** Windows)

Version: **26.8.2.2** (`9dea406ab7695df6325b11b0e19fdca8db4b7276`), the version `SPEC.md` §6
pins. Both legs (Linux x86-64 and Windows 11) ran the same matrix on the same build id.

#### 6.2.1 Which executable to spawn — per platform, measured

| platform | spawn | first run on a **brand-new** profile | notes |
|---|---|---|---|
| Windows | **`program/soffice.com`** | rc=0, 1 130 ms, file written, `convert …` line on stdout | console-subsystem sibling; real blocking pipes — the sidecar contract |
| Windows | `program/soffice.exe` | rc=0, 1 150 ms, file written | GUI subsystem: console behaviour depends on inherited handles. Do not rely on it when `.com` exists |
| Windows | `program/soffice.bin` | **rc=81**, 355 ms, **no output**, empty stdout/stderr | the same trap as Linux; a *second* run on the bootstrapped profile works (rc=0, 739 ms) |
| Linux | **`program/soffice`** | rc=0, 923 ms, file written | `oosplash` wrapper; relaunches after the first-profile crash |
| Linux | `program/soffice.bin` | **rc=81**, 230 ms, **no file** | never call it |

The failure is not "this binary never works" — it is specifically *the first run against a
profile that has never been bootstrapped*, which is exactly the run a freshly installed app
performs for its first user job. There is no `oosplash.exe` on Windows; `soffice.com` is what
plays that role.

The envelope, as measured (`spikes/s3/run_s3.py::invoke`, `spikes/s3/run.ps1`; Linux re-run on
this host 2026-11-01):

```rust
let mut argv: Vec<OsString> = vec![
    soffice_launcher.into(),             // §6.2.1: soffice / soffice.com
    "--headless".into(),
    "--norestore".into(),
    "--nolockcheck".into(),
    "--nodefault".into(),
    "--nologo".into(),
    format!("-env:UserInstallation={}", profile_url).into(),  // §6.2.2 — a URL, not a path
];
if let Some(f) = infilter { argv.push(format!("--infilter={f}").into()); }
argv.push("--convert-to".into());
argv.push(convert_to.into());            // literal from the route table
argv.push("--outdir".into());
argv.push(outdir.into());                // fresh per job
argv.push(input.into());                 // absolute
```

```text
Linux  : HOME=<job temp>, SAL_USE_VCLPLUGIN=svp, SAL_DISABLE_OPENCL=1, DISPLAY unset,
         LD_LIBRARY_PATH=<only for the unpacked TDF tarball; never on Windows/macOS>
Windows: HOME=<job temp>, SAL_DISABLE_OPENCL=1.  Do NOT set SAL_USE_VCLPLUGIN=svp —
         svp is the X11-free *Linux* VCL plugin; the Windows build never needs it.
```

#### 6.2.2 Windows-only details that silently break a correct port

1. **`-env:UserInstallation` wants a URL, not a path.** It must be
   `file:///C:/Users/…/profile` — drive letter, forward slashes, three slashes. The Linux
   spelling (`file://` + `C:\Users\…`) is accepted without error and quietly bootstraps a
   *different* profile, which destroys the cold/warm contract and re-pays the first-run cost on
   every job. Build it with a `file_url()` helper, not string concatenation.
2. **No `SAL_USE_VCLPLUGIN` on Windows** (above).
3. **No `LD_LIBRARY_PATH` concept**; the Windows install is self-contained under
   `C:\Program Files\LibreOffice\program` (or wherever `P2` puts the bundled tree).

#### 6.2.3 Proven directions

Bytes are shown for both legs because they differ (real Calibri/Cambria embed on Windows;
Carlito/Caladea metric clones on Linux). All seven PASS on both platforms with content checks
beyond file existence.

| # | direction | `--convert-to` | `--infilter` | Linux output | Windows output |
|---|---|---|---|---|---|
| D1 | docx → pdf | `pdf:writer_pdf_Export` | — | 26,460 B, 1 page, 612×792 pt | 39,946 B, 1 page, 612×792 pt |
| D2 | docx → md | `md:Markdown` | — | 389 B, real Markdown (heading, table, unicode) | 404 B, real Markdown heading |
| D3 | docx → html | `html:HTML (StarWriter)` | — | 4,064 B, `<h1>`, `<table>`, 13 `<p>` | 4,134 B, same structure, `café` intact |
| D4 | csv → xlsx | `xlsx:Calc MS Excel 2007 XML` | `csv:44,34,76,1,,0,false,true,true` | 5,802 B, numeric cells, UTF-8 | 5,793 B, same |
| D5 | xlsx → csv | `csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,true,false,false,1` | — | 147 B, named **`<stem>-<sheet>.csv`** | 151 B, **`sample-Data.csv`** — same renaming rule |
| D6 | html → docx | `docx:MS Word 2007 XML` | `HTML (StarWriter)` | 6,863 B, real 2×2 Word table | 6,888 B, same |
| D7 | pptx → pdf | `pdf:impress_pdf_Export` | — | 33,957 B, 2 pages, 720×540 pt | 56,471 B, 2 pages, 720×540 pt |

D2's filter is **version-dependent** (26.8.x has it; an older LibreOffice may not). That is one
of the reasons pandoc is not shipped (`§17.3`): a second GPL process for a filter the pinned
version already provides.

Fidelity (docx → pdf, 4-page fixture): every page 612×792 pt, all position markers present,
2,981 chars of live text, 4 image XObjects, fonts embedded — `Carlito-Bold`/`Caladea-Regular`
on Linux, the **real `Calibri-Bold`/`Cambria`** on Windows (which is why the PDF is 50,626 B
there vs 34,607 B on Linux). Same metrics, different outlines: "layout-faithful", never
"pixel-identical", never "Word-identical" (`SPEC.md` §4). **Aptos remains unbundled with no
metric clone** — modern Word documents will reflow; do not claim otherwise.

#### 6.2.4 Windows text layer — measured in real viewers, 2026-11-01

On Windows, `docx → pdf` renders accented Latin correctly and the PDF is correct, but the
**text layer** is degraded: `pypdf 6.21.4` extracts `cafe\x1c` / `naï\x1fve` because the Cambria
subset's `ToUnicode` CMap is missing exactly `0x1C` and `0x1F`. That finding was originally a
library result, so it was tested in **real viewers** (`spikes/s4/EVIDENCE.md` §2) on a fresh
conversion of the S3 fixture (39,946 B, `%PDF-1.7`):

| viewer | renders `café – naïve → 中文` | find `café` | copy / screen-reader text |
|---|---|---|---|
| Chrome 156 (PDFium) | yes | **1 of 1** | `caf[00E9] – na[00EF]ve → 中文` — correct |
| Brave 156 (PDFium) | yes | **1 of 1** | `caf[00E9] – na[00EF]ve → 中文` — correct |
| Edge 156 (Adobe-Acrobat-branded PDF engine) | yes | **1 of 1**, highlighted | `cafe ` — **the `é` is dropped** |
| `pdf.js` 5.7.284 (Firefox's text layer) | — | — | `caf e[1C] – na ï[1F] ve` — same defect as pypdf |

Verdict: **some viewers are affected.** Consequences for this architecture:

- **No validator may depend on text extraction of non-ASCII characters.** `§6.0.5` validates
  PDFs by header + page count, not by text. Confirmed by two independent engines, not one
  library; load-bearing, not incidental.
- The app **must not** advertise "searchable text intact" / "text stays searchable" on Windows.
  Rendering and in-viewer search are fine; copying accented text out is not, outside PDFium.
  This matters for a Vietnamese user base and is a **LibreOffice Windows exporter** behaviour,
  not something the packaging introduces.
- Not measured: Acrobat Reader itself (not installed on the build host) and macOS Preview. Do not
  extend the table above by assumption.


Negative controls, all measured — these define the error model:

| # | situation | rc | stdout/stderr | output |
|---|---|---|---|---|
| N1 | `soffice.bin` on a brand-new profile (Linux **and** Windows) | **81** | crash-with-restart | **none** — the wrapper (`soffice` / `soffice.com`) relaunches, which is why the launcher choice is mandatory |
| N2 | missing input file | **1** | stderr `Error: source file could not be loaded` | none (re-verified today) |
| N3 | plain text renamed `broken.docx` | **0** | stdout claims conversion, stderr empty | **a 12,388 B, 1-page PDF is written** (re-verified today) — the silent-garbage trap |
| N4 | `csv:44,34,76,1` (short token + options) | **1** | `Error: Please verify input parameters… Class:Parameter Code:26` | none |
| N5 | filter name without its type token | 0 | — | a file literally named `<stem>.Text - txt - csv (StarCalc)` |
| N6 | quoted filter name (`csv:"Text - txt - csv (StarCalc)"`) | **1** | no export filter | none |
| N7 | `--outdir` does not exist | 0 | normal convert line | directory is created and the file written |

Operational contract:

- **Always stat/glob the outdir.** LibreOffice renames outputs (`<stem>-<sheet>.csv`, one file
  per sheet with `-1`). The worker creates a fresh per-job outdir, then globs it for the
  expected extension and requires ≥ 1 result; > 1 means a per-sheet export, which is surfaced
  as a multi-output job rather than an error.
- **rc = 0 carries no information.** Success = (rc == 0) ∧ (expected file found) ∧ (validator
  passes, `§6.0.5`). N3 is why the validator exists; N3 is also why detection runs first.
- **One warm profile per worker, serialised.** Profile dir lives under the app cache, created
  on first use, deleted and recreated if a run leaves it locked. Cost of getting this wrong is
  measured on both platforms: Windows **install-cold first conversion 5,111 ms**, second
  fresh-profile run 2,725 ms, warm ~880-892 ms; Linux cold 704-1217 ms, warm 311-990 ms. A
  startup warm-up call keeps the first user conversion from paying bootstrap cost.
- **Batching** (card `C3`): multiple inputs may be passed to one invocation with one
  `--outdir`; one outdir per batch, then outputs are attributed by stem. Only do this when
  every file in the batch shares the same route and outdir — mixed batches are split.
- **`soffice --version`** is the identity check for `§6.7`.

### 6.3 ffmpeg / ffprobe — audio and video (`PLANNED`, card `C2`)

**No contract is documented here, and none may be inferred from this section.** Reasons:
S1-S3 measured no ffmpeg invocation in this project; the research findings in
`media-engine-stack-2026.md` are single-command probes on a different FFmpeg build, not a
verified matrix; and the binary on the dev box is `n9.0.1 --enable-gpl --enable-version3
--enable-libx264 --enable-libx265` — a build that must never ship.

What is fixed now, because it is not an argv detail:

- LGPL-only build; `scripts/license-assert.sh --scan` must pass on the shipped binaries
  (`ffmpeg -version` must contain `--enable-gpl` zero times; no `libx264`/`libx265`).
- ffmpeg **never** converts still images (`SPEC.md` §6 rule 1, measured: EXIF is destroyed
  even with `-map_metadata 0`). A unit test asserts no image route resolves to `ffmpeg`.
- Audio conversions pass explicit `-ar` and sample format derived from the source; Opus is
  48 kHz-only. Sample-rate drift is a known silent-wrongness class.
- Video honours rotation and variable frame rate explicitly (`-fps_mode cfr`), and trims
  declare keyframe-vs-frame accuracy in the UI copy.
- Tags and cover art are merged with the in-process `lofty` crate after the encode; ffmpeg's
  tag mapping is lossy and is not trusted.
- GIF output is `palettegen`/`paletteuse`, fed by a vips-normalised PNG (`SPEC.md` §9 D2).

`C2` must fill in the argv table for: mp3/wav/flac/m4a/aac/ogg/opus, mp4/mkv/mov/webm,
trim, resize, GIF, and prove each with a content-checked fixture. Until then the audio/video
routes are `Planned` and invisible.

### 6.4 PDFium — PDF pages to raster (`PLANNED`, needs a decision recorded in §17)

PDFium has no CLI. `pdfium-render` is a Rust binding that loads `pdfium.dll`/`.dylib` into
the calling process, which collides with two standing rules: "engines are sidecar processes,
never FFI" (`SPEC.md` §6 preamble) and "every parser is an attack surface" (engineering rule
7 — an in-process PDF parser crash takes the whole app down).

**Decision:** ship a first-party sidecar, `ezc-pdf.exe`, that links `pdfium-render` and does
nothing but rasterise pages to PNG in a per-job outdir. The app never loads PDFium. This
keeps the FFI inside a process we can kill, keeps the licence story simple (PDFium is BSD-3,
`pdfium-render` MIT/Apache), and satisfies the letter of the sidecar rule for the app binary.

Contract shape (unproven — `C1`/`C2` must measure and record the exact argv, including page
selection and DPI):

```
ezc-pdf.exe rasterize --input <abs pdf> --outdir <abs dir> --page <n|first> --dpi <int>
```

Not yet decided and deliberately left open: multi-page export policy. v1 rasterises **page
1** by default; a `pdf.page` setting exists (§15) but has no UI promise beyond a number field.

### 6.5 resvg — SVG input (`PLANNED`, card `C2`)

`SPEC.md` §9 D3 requires SVG input-only, rasterised to a lossless PNG and then handed to
vips. The spec text calls resvg a "required v1 sidecar" (§9.1, §9.2) and also a "crate"
(§9.3); **architecture resolves this in favour of the sidecar** (`§17.1`), because an SVG
parser is an attack surface and the process boundary is free here.

- `resvg 0.49.x` (Apache-2.0 OR MIT), pinned; older releases were MPL-2.0 — do not mix.
- Explicit output size is mandatory (width or zoom); no "default size" output, so results
  are deterministic.
- Text-bearing SVG needs bundled fonts; unknown fonts fall back and the UI must not promise
  pixel fidelity.
- The exact argv is **not documented** here: it has not been measured in this project.

### 6.6 In-process Rust crates (no argv, explicit justification required)

| crate | use in v1 | status |
|---|---|---|
| `image` | BMP decode/encode only, via a temp-PNG hop with vips (`SPEC.md` §9 D5) | required by D5; unproven route |
| `calamine` | reading spreadsheet contents for verification/preview | optional; `C3` uses LibreOffice for conversion, so this is verification-only |
| `rust_xlsxwriter`, `comrak`, `htmd` | **not used in v1** — LibreOffice covers every spreadsheet/doc/markdown direction the app advertises today | unused until a proven gap exists |
| `lofty` | audio tag merge after an ffmpeg encode | required by `C2` |

Each addition lands in `docs/LICENSES.md` in the same commit (R6) — `scripts/gen-licenses.py
--check` enforces the Cargo side in CI, and the notice files ship with the installer.

### 6.7 Engine discovery and identity (`engine/toolpath.rs`)

At startup, resolve each engine from the install layout (`§18`), then verify identity once:

| engine | check | action on mismatch |
|---|---|---|
| vips | `vips.exe --version` ⇒ `vips-8.18.7`; `--vips-config` parsed for `image quantisation … false` and `PDF load … false`; **plus a real 1×1 convert per advertised saver** (§6.1 — config cannot be trusted for avif) | engine marked `Unavailable(VersionMismatch)` / `Unavailable(CapabilityProbeFailed)`; that route is hidden from the wheel |
| soffice | the §6.2.1 launcher (`soffice`, `soffice.com`) `--version` ⇒ `LibreOffice 26.8.2` | same |
| ffmpeg | `ffmpeg -version` contains no `--enable-gpl` | **refuse to run it**, surface a licence fault in diagnostics (a GPL binary must never execute as our engine, not even accidentally) |
| pdfium, resvg | version string matches the pin | same |

`Unavailable` is a first-class state: the wheel shows the reason, the window shows a
remediation line, and no job is attempted. Never a silent fallback to a different engine.

---

## 7. Detection: content first, extension never authoritative

`model/detect.rs` is a pure function over a bounded prefix of the file (first 64 KiB, plus
ZIP/CFB directory reads when the container signature matches). It returns:

```rust
struct Detected {
    ty: CanonicalType,          // what the bytes are
    integrity: Integrity,       // Complete | Truncated | Unreadable
    claimed: Option<Ext>,       // what the name said (for diagnostics only)
    detail: DetectedDetail,     // brand/stream/container specifics
}
```

Rules:

1. **Content decides.** The extension is recorded and compared; a mismatch is logged
   (`claimed != detected`) and never changes the outcome. The acceptance fixture is a JPEG
   renamed `.png`, which must resolve to JPEG and offer JPEG's targets.
2. **Container types are validated structurally**, not by their first four bytes:
   - `docx` = ZIP with `[Content_Types].xml` **and** `word/document.xml`;
   - `xlsx` = `xl/workbook.xml`; `pptx` = `ppt/presentation.xml`;
   - `doc` = OLE2/CFB with a `WordDocument` directory entry; `xls` = `Workbook` or `Book`;
     `ppt` = `PowerPoint Document`.
   A ZIP without the required part is not a document; a CFB with none of those entries is
   not an Office file. This is what stops N3 (`§6.2`) from ever reaching LibreOffice.
3. **Truncation is detected cheaply**: JPEG without EOI, PNG without `IEND`, ZIP without an
   end-of-central-directory record, PDF without `%%EOF`. `Truncated` still allows conversion
   attempts but the UI warns first (a truncated JPEG may still decode, a truncated ZIP will
   not).
4. **Text types** (`txt`, `md`, `csv`, `html`, `svg`) have no magic. Resolve in this order,
   on validated UTF-8/UTF-16-with-BOM text with no NUL bytes:
   `svg` (root element `<svg`) → `html` (`<!doctype html`/`<html`) → `csv` (≥ 2 lines with a
   stable delimiter count) → `md` (ATX heading, list markers, fenced block, link syntax) →
   `txt`.
5. **Size and count limits** before any parse: refuse > 2 GiB per input; refuse a declared
   uncompressed ZIP size > 8 GiB (bomb guard); refuse more than 10,000 directory entries.
6. **Unknown is a valid answer.** Unknown bytes ⇒ `ty: Unknown`, no targets, and the wheel
   shows the Empty state with the reason. Never guess.

The signature table is in the code, one row per canonical type, and every row carries a
`fixture:` attribute. Signatures themselves are published format facts; **no row is
`Proven` until `C1`'s fixture test passes**, including the renamed-file and truncated-file
cases its acceptance requires.

### 7.1 What this costs

Detection reads a prefix, not the whole file. Two consequences we accept: a file that is a
valid JPEG header followed by garbage is detected as JPEG and fails at decode (with the
`§6.0.4` stderr rule catching the silent case), and a polyglot file (valid ZIP and valid PDF)
is classified by precedence order, documented in code, not by "whichever parse succeeds".

---

## 8. Target matrix

`model/matrix.rs` is the single source of truth for "what can this file become". The wheel,
the window and the tests all read it. Nothing in the UI hardcodes a format list.

### 8.1 Shape

```rust
struct Route {
    target: TargetFormat,
    engine: Engine,
    status: RouteStatus,          // §0
    plan: fn(&JobCtx) -> ExecPlan // argv builder, pure
}
fn routes_for(ty: CanonicalType) -> &'static [Route];
```

### 8.2 Image types

Only pairs that were actually measured are PROVEN. The four `copy` hops, two `autorot` hops
and the avif save/decode pair in `§6.1` are the whole of it; every other cell is `planned` and
invisible in the wheel until `C2` runs that pair against a fixture.

| input | → jpg | → png | → webp | → tiff | → avif | → gif | → bmp | → heic | → svg | → pdf |
|---|---|---|---|---|---|---|---|---|---|---|
| jpg | **PROVEN** (V6) | **PROVEN** (V2) | planned | planned | **PROVEN** (V8) | planned (ffmpeg) | planned (`image`) | cut (D4) | cut | cut (no writer) |
| png | planned | — | **PROVEN** (V3) | planned | **PROVEN** (V8) | planned | planned | cut | cut | cut |
| webp | planned | planned | — | **PROVEN** (V4) | planned | planned | planned | cut | cut | cut |
| tiff | planned | **PROVEN** (V5)¹ | planned | — | planned | planned | planned | cut | cut | cut |
| heic | **PROVEN** (V7) | planned | planned | planned | planned | planned | planned | cut | cut | cut |
| avif | **PROVEN** (V8b) | planned | planned | planned | — | planned | planned | cut | cut | cut |
| gif | planned | planned | planned | planned | planned | planned | planned | cut | cut | cut |
| bmp | planned | planned | planned | planned | planned | planned | planned | cut | cut | cut |
| svg | planned | planned | planned | planned | planned | planned | planned | cut | cut | cut |
| pdf | planned | planned | planned | planned | planned | planned | planned | cut | cut | cut |

`—` = not offered (identity, or cut by `SPEC.md` §9).

¹ TIFF input must go through `tiffload … --fail-on error` (V9), so the measured pair V5
(`copy`) is the *encode* side only; the route is proven once `C2` re-runs it with the named
loader. JBIG-compressed TIFF fails loudly, which is the intended behaviour (`SPEC.md` §9.2).

What makes the `planned` cells cheap to flip: they are the same `vips` mechanism with a
different saver, and the four measured hops already prove the dist can decode jpg/png/webp/
tiff/heic and encode png/webp/tiff/jpg. `C2`'s fixture matrix (one file per cell) is the work,
not new engine integration. Claiming those cells from "same code path" would violate the
rule in `§0`, so they stay `planned` until each has its own fixture and assertion.

### 8.3 Documents

| input | → pdf | → md | → html | → docx | → xlsx | → csv | → txt |
|---|---|---|---|---|---|---|---|
| docx | **PROVEN** (D1) | **PROVEN** (D2) | **PROVEN** (D3) | — | planned | cut | planned (LO txt filter, unmeasured) |
| pptx | **PROVEN** (D7) | planned | planned | cut | cut | cut | cut |
| xlsx | planned | cut | planned | cut | — | **PROVEN** (D5) | planned (sheet-aware) |
| csv | planned | cut | cut | cut | **PROVEN** (D4) | — | planned (normalise) |
| html | planned | planned | — | **PROVEN** (D6) | cut | cut | planned |
| doc / xls / ppt (legacy) | planned (same filters, unmeasured) | planned | planned | planned | planned | planned | planned |
| rtf | planned | planned | planned | planned | cut | cut | planned |
| md | planned | — | planned | planned | cut | cut | planned |
| txt | planned | cut | cut | planned | cut | cut | — |
| pdf | cut (no raster-to-writer path) | cut | cut | cut | cut | cut | planned (text extraction is a different product) |

**v1 truth:** seven document routes are live. The rest are `planned` with an owner card and
will appear in the wheel the day their card flips them with evidence. `SPEC.md` §3 is the
target state; this table is the current one.

### 8.4 Audio and video

All pairs `planned` (`§6.3`). The UI renders no audio/video segment until `C2` proves a
route. `SPEC.md` §3 lists them as v1 scope — they are v1 scope, gated on measurement, not
re-scoped.

### 8.5 Multi-file and folder drops

- **Multi-file drop:** the wheel shows the **intersection** of valid targets across all
  detected files, with a count badge and a per-file overflow list in the window. Rationale:
  offering a target that fails for 3 of 5 files is worse than offering fewer.
  If the intersection is empty, fall back to the union with a warning ("2 of 5 files can't do
  this") and per-file results — the user can see which.
- **Folder drop:** enumerate **1 level deep** by default, `include_subfolders` opt-in
  (`SPEC.md` §5). Non-matching files inside are counted and reported, never silently dropped.
- **Type mixing:** a drop containing images and documents runs as two routes in one batch;
  the toast reports per-family results.

---

## 9. Output paths, naming, metadata (implementation in `C4`)

Deterministic and collision-free by construction:

1. **Root:** `same_folder` (default) or `same_folder/converted` when the batch is ≥ 3 files
   and the batch-folder setting is on. Both rules are settings, not constants (`SPEC.md` §5).
2. **Name:** `<stem>.<target-ext>`; for per-sheet spreadsheet exports, the engine's own name
   (`<stem>-<sheet>.csv`) is preserved and recorded as a separate output.
3. **Collisions never overwrite.** Probe `stem.ext`, `stem-1.ext`, `stem-2.ext`, … and use
   the first free name. The probe and the later rename are one operation under a per-directory
   lock; a name claimed by a concurrent job is skipped.
4. **Atomic publication:** the engine writes into the job temp directory; the result is
   hash-recorded, then renamed into place on the same volume (rename, not copy). A crash
   mid-conversion leaves the user's folder untouched.
5. **Reserved names and pathological paths** are refused at admission (Windows device names
   `CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`, trailing dots/spaces, path > 260 chars
   unless long-path aware) with an actionable message.
6. **Metadata:** keep by default (EXIF/ICC/dates/audio tags). The per-conversion `strip`
   toggle ships only once the strip path is measured for each engine (`§6.1` for vips,
   `C2` for audio where "strip" means skipping the tag merge). `SPEC.md` §5.

---

## 10. Wheel state machine (implementation in `U1`, `U3`)

### 10.1 What S1 proved (and what it did not)

Host: Windows 11 Pro 26200, session 11, human-driven, no synthetic input
(`evidence/S1-EVIDENCE.md`).

| claim | verdict |
|---|---|
| shows without stealing focus | PASS, 10/10 shows with `fg_before == fg_after` |
| click outside reaches the window behind | PASS (probe counter 0→1 aligned with our `click_outside`) |
| real Explorer drag delivers a path | PASS (`CabinetWClass` source, path on disk) |
| Esc dismisses | PASS — registration of a global `Esc` shortcut while visible |
| button-up outside dismisses | PASS, 9 events, no false dismissals after the grace/guard rules |
| re-summon at the cursor, same monitor | PASS (`cursor == center`, monitor `DISPLAY1`) |
| re-summon on a **second** monitor | **UNPROVEN** — every summon had the cursor on DISPLAY1 |
| focus-free behaviour with a console-less debug build | PASS once `windows_subsystem="windows"` was set — a console window is a focus thief |

Mechanism that made it work (keep it):

- Window: 520×520, `decorations:false`, `transparent:true`, `alwaysOnTop:true`,
  `skipTaskbar:true`, `shadow:false`, `focus:false`, `visible:false`, `dragDropEnabled:true`.
- `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`, `WS_EX_APPWINDOW` cleared; shown with
  `SetWindowPos(HWND_TOPMOST, …, SWP_NOACTIVATE|SWP_SHOWWINDOW)` + `SW_SHOWNOACTIVATE`.
- **Region-clipped to the ring** (`SetWindowRgn` with an elliptical region). Outside the
  circle there is no window at all, so clicks and drops go to whatever is behind — no
  `WS_EX_TRANSPARENT` toggling and no hit-test race mid-drag.
- Esc is a **registered global shortcut while visible**, unregistered on hide. A window that
  never activates never receives `WM_KEYDOWN`.
- Click-away is a tight poll of `GetAsyncKeyState(VK_LBUTTON)` detecting a button-up outside
  the ring (8 ms in the spike; 16 ms is enough in production and must be re-proven), refused
  when (a) within 400 ms of the show, or (b) an OLE drag is in flight. Both refusals are
  logged with the rule that fired.
- Re-assert `WS_EX_NOACTIVATE` on every show: tao's `set_ignore_cursor_events` rewrites
  `GWL_EXSTYLE` from a cached copy and adds `WS_EX_LAYERED`, silently dropping the style.

### 10.2 Defect found while writing this document (must be fixed in `U3`)

The spike centres the window on the cursor and clamps to the **virtual screen**:

```rust
let (vw, vh) = virtual_screen();                       // SM_CXVIRTUALSCREEN
let x = (cx - WIN_W/2).clamp(0, (vw - WIN_W).max(0));  // clamps at 0!
```

On a multi-monitor desktop with a monitor to the **left** of the primary (negative virtual
coordinates) this clamps the wheel onto the primary monitor. It also cannot express "fit
inside the cursor's work area" (taskbar). Production must clamp to the **cursor monitor's
work rect**:

```text
mon = MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST)
GetMonitorInfoW(mon) -> rcMonitor, rcWork
x = clamp(cursor.x - W/2, rcWork.left, rcWork.right  - W)
y = clamp(cursor.y - H/2, rcWork.top,  rcWork.bottom - H)
```

This is the exact gap that leaves S1's e2 UNPROVEN, and it becomes `U3`'s acceptance test
(summon with the cursor on each monitor, including one with negative coordinates).

### 10.3 States and transitions

| state | meaning | exits |
|---|---|---|
| `Hidden` | no window | hotkey ⇒ `Summoning`; (drop is impossible when hidden) |
| `Summoning` | positioned at cursor, ring animating in | auto ⇒ `Empty`/`Choosing`; hotkey ⇒ `Dismissing` (toggle); esc ⇒ `Dismissing` |
| `Empty` | open, no valid targets (unknown type, engine unavailable) | esc/click-away ⇒ `Dismissing`; drop replaces content |
| `Choosing` | detecting done, ≤ 6 segments + optional "more" | pick ⇒ `Confirming`; more ⇒ sub-ring (still `Choosing`); esc/click-away ⇒ `Dismissing`; drop replaces |
| `Confirming` | segment locked, spring settle, target accepted | auto (≤ 120 ms) ⇒ `Converting` |
| `Converting` | jobs queued/running; wheel shows per-file state | all done ⇒ `Toast`; second drop ⇒ queue more, stay `Converting` |
| `Toast` | result + Undo affordance | 10 s elapsed or Undo ⇒ `Dismissing`; click toast ⇒ window |
| `Dismissing` | spring out, then `Hidden` | — |

Rules:

- The **Rust** state machine is authoritative; the webview only renders and reports intent.
  This prevents a dropped `drag_leave` from leaving a phantom wheel.
- **Hotkey toggles**: summon when hidden, dismiss when visible (proven: 10 re-summons).
- **Drops while visible replace** the current file set if not converting; **queue** if
  converting.
- **Never dismiss while a drag is in flight** or within 400 ms of a show.
- While `Converting`, the wheel stays open (progress is the point); it must still be
  focus-free — no modal, no progress dialog.
- **Idle auto-dismiss is not in v1.** No timeout dismissals until one is measured; a wheel
  that vanishes mid-decision is worse than one that waits.

### 10.4 Wheel content and geometry

- Hard cap **6 segments, never smaller** (`SPEC.md` §2). With radius 200 px this is 60° of arc
  per segment at the rim; the legibility floor is a segment label of ≥ 11 pt at the outermost
  ring. `U1` must reject any layout that violates it rather than shrinking type.
- > 6 valid targets ⇒ the 7th slot is a **"more"** segment that expands into a sub-ring
  (still ≤ 6 children per ring, recursing).
- Settings choose the user's preferred segments and order; at show time the list is filtered
  to valid targets, then padded from the MRU list, then from defaults. Ordering is user order
  first, MRU second.
- Apple motion per `SPEC.md` §2 and the `apple-design` skill: springs (damping 1.0,
  response 0.3-0.4; momentum spring 0.8 on release), velocity handoff on drop, momentum
  projection for segment selection, `backdrop-filter` material with the recursive
  transparency rules, `prefers-reduced-motion` and `prefers-reduced-transparency` fallbacks,
  `system-ui` with size-dependent tracking.

### 10.5 macOS differences (not yet measured)

`WS_EX_NOACTIVATE` has no macOS equivalent; the relevant primitives are
`NSWindow` with `canBecomeKeyWindow = false`, `ignoresMouseEvents` for the click-through
regions, and `-[NSWindow setLevel:NSFloatingWindowLevel]`-style stacking. The S1 evidence says
nothing about macOS, so `U1`/`U3` must measure it on a macOS runner before the wheel is
claimed to work there. Do not port the Win32 constants by analogy.

---

## 11. Window surface (implementation in `U2`)

- Normal window: queue table (file, detected type, target, state, size, duration), batch
  controls, settings, diagnostics.
- Drag-and-drop and "Add files" both funnel into the same `job::queue` admission path as the
  wheel, so naming/location/metadata rules cannot diverge between surfaces.
- Settings UI is generated from the schema (`§15`), so a new setting cannot exist without a
  control and vice versa (`U2` acceptance: every `SPEC.md` §5 rule has a visible control).
- No network UI, no account UI, no update UI (no auto-update in v1).

---

## 12. Job model

```rust
struct Job {
    id: JobId,
    inputs: Vec<Input>,        // abs path + Detected + size
    target: TargetFormat,
    route: &'static Route,
    outdir: PathBuf,           // resolved by §9
    state: JobState,           // §5.1
    outputs: Vec<Output>,      // filled at Verifying
    engine: Engine,
    started: Instant,
    log: JobLog,               // bounded stdout/stderr per invocation
}
```

- One job = one (input set, target). Batches are jobs with multiple inputs; per-file failure
  does not fail the batch (the job reports `PartiallyFailed` with per-file reasons).
- Job ids are monotonic per session; the journal (`§13`) records them for undo.
- Every job writes a log to the app cache: argv (paths included — local only), env delta, rc,
  stderr tail, validator verdict, timings. This is the support artifact; it never leaves the
  machine.

---

## 13. Undo journal on disk (`job/journal.rs`)

`SPEC.md` §5: a 10-second Undo that deletes the outputs. Safety beats convenience, so the
journal is written **before** anything is published and undo is a verified, all-or-nothing
delete.

Location: `<cache>/journal/<job_id>-<unix_ms>.json` (cache, not config — it is disposable).
Format:

```json
{
  "schema": 1,
  "job_id": "j_000042",
  "created_at_unix_ms": 1793512000123,
  "state": "pending",
  "inputs": [{ "path": "C:\\Users\\ryo\\Documents\\report.docx", "sha256": "…", "size": 37058 }],
  "outputs": [
    { "path": "C:\\Users\\ryo\\Documents\\report.pdf", "sha256": "…", "size": 26460, "mtime_unix_ms": 1793512001020 }
  ],
  "engine": "soffice 26.8.2",
  "route": "docx2pdf"
}
```

Lifecycle:

1. **Before** the first publication: write `state:"pending"` with inputs and the intended
   output paths, fsync, close.
2. After publication and validation: rewrite with final sizes/hashes, `state:"committed"`.
3. Undo (button, ≤ 10 s): for **every** output — recompute sha256 and compare; require the
   canonical path to equal the recorded one; require it is not one of the inputs. If all
   checks pass, delete; if any fails, delete nothing and report "some files changed since
   conversion — not touching them". Then mark the journal `undone` and remove it.
4. After 10 s: mark `expired`, remove the journal (outputs stay).
5. **On startup:** never delete anything. Any `pending` journal (crash mid-flight) is
   rewritten as `abandoned`, logged, and removed. `committed` journals older than the window
   are removed without action.

The hash check is what makes "deletes only files this job created" true (C4's acceptance):
it cannot delete a user's replacement file, cannot delete an input, and cannot delete through
a path that was re-pointed by a symlink or junction after conversion.

---

## 14. Error model and user-facing messages

One enum, one message table, no ad-hoc strings in the UI.

```rust
enum AppErrorKind {
    DetectUnknown, IntegrityTruncated, NoRouteForPair,
    EngineUnavailable, EngineVersionMismatch, EngineLicenceFault,
    InputUnreadable, InputEncrypted, InputTooLarge,
    PathRejected, OutputPathFailed, DiskFull, PermissionDenied,
    Timeout, Cancelled,
    NoOutput,          // engine "succeeded", produced nothing
    OutputInvalid,     // output failed its validator
    DecodeFailed,      // stderr rule fired (JBIG TIFF, corrupt JPEG, …)
    JournalFailed, PartialBatch,
}
```

Message table (verbatim, user-facing; `{…}` are substituted). Tone rules: say what happened,
say what to do, never blame the user, never show a raw exit code or a stack trace, always
state whether the original file was touched.

| kind | message |
|---|---|
| `DetectUnknown` | "We couldn't tell what kind of file this is, so there's nothing safe to convert it to. The file is unchanged." |
| `IntegrityTruncated` | "This file looks incomplete (`{detail}`). Converting may fail — the original stays as it is." |
| `NoRouteForPair` | "There's no path from `{from}` to `{to}` yet." (internal; should be unreachable from the UI) |
| `EngineUnavailable` | "`{engine}` isn't available, so `{family}` conversions are off. Reinstall EZ Convert to restore it." |
| `EngineVersionMismatch` | "`{engine}` is version `{found}` but this build expects `{expected}`. Conversions are off until it's fixed." |
| `EngineLicenceFault` | "`{engine}` failed its licence check and will not run. This installation is faulty — please reinstall." |
| `InputUnreadable` | "This `{type}` file uses `{feature}`, which this build can't read (`{example}`). Nothing was written." — used for JBIG TIFF: "JBIG-compressed TIFF" |
| `InputEncrypted` | "This file is password-protected. EZ Convert can't open encrypted files. Nothing was written." |
| `InputTooLarge` | "`{name}` is `{size}`, over the `{limit}` limit for a single file." |
| `PathRejected` | "`{name}` can't be used: `{reason}` (network paths aren't supported)." |
| `OutputPathFailed` | "There's no room to write the result next to `{name}`. Try a different output folder." |
| `DiskFull` | "The disk filled up while writing `{name}`. Nothing was left behind." |
| `PermissionDenied` | "Windows didn't let us write to `{folder}`. Choose another output folder in Settings." |
| `Timeout` | "`{name}` took longer than `{limit}`, so it was stopped. The original is unchanged." |
| `Cancelled` | "Cancelled. `{count}` file(s) weren't converted." |
| `NoOutput` | "`{engine}` reported success but wrote no file — nothing was saved. This is a bug; the details are in the log." |
| `OutputInvalid` | "The result didn't pass a check, so it wasn't saved. The original is unchanged." |
| `DecodeFailed` | "`{name}` couldn't be decoded (`{detail}`). It may be damaged. Nothing was written." |
| `JournalFailed` | "Converted, but we couldn't record it for Undo. The output is kept; Undo is unavailable for this file." |
| `PartialBatch` | "`{ok}` of `{total}` files converted. `{failed}` failed — see the queue for details." |

Mapping from measured engine output (grounded, not guessed):

| engine signal | kind |
|---|---|
| soffice rc=1 + stderr `Error: source file could not be loaded` | `DecodeFailed` (or `PermissionDenied` if the path is unreadable) |
| soffice rc=81 | `EngineUnavailable` — the wrapper was bypassed or the install is broken; report as a bug |
| soffice rc=0 + no file in the outdir | `NoOutput` |
| soffice rc=0 + file fails validator | `OutputInvalid` |
| soffice rc=1 + `Please verify input parameters… Code:26` | internal `NoRouteForPair` (never user-reachable; the argv is a literal) |
| vips exit 1 + `tiff2vips: ISO JBIG compression support is not configured` | `InputUnreadable` with feature "JBIG-compressed TIFF" |
| vips exit 0 + stderr matching a `§6.0.4` pattern | `DecodeFailed` |
| engine killed by our timeout | `Timeout` |
| spawn failed with `NotFound` | `EngineUnavailable` |
| spawn failed with `PermissionDenied` | `EngineLicenceFault` if the licence gate caught it, else `PermissionDenied` |

Raw stderr never reaches the UI by default. The window's diagnostics pane shows the job log
on request, with a line saying it stays on this machine.

---

## 15. Settings schema

Store: `<config>/settings.json` (Windows `%APPDATA%\app.ezconvert\`, macOS
`~/Library/Application Support/app.ezconvert/`). Written atomically (temp file + rename),
`schema_version` first, unknown fields preserved across versions.

```json
{
  "schema_version": 1,
  "output": {
    "location": "same_folder",
    "chosen_folder": null,
    "batch_subfolder": "converted",
    "batch_threshold": 3,
    "use_batch_subfolder": true,
    "never_overwrite": true
  },
  "metadata": { "strip": false },
  "wheel": {
    "segments": ["png", "jpg", "pdf", "webp", "mp3", "mp4"],
    "order": ["png", "jpg", "pdf", "webp", "mp3", "mp4"],
    "show_more_ring": true
  },
  "hotkey": { "accelerator": "Ctrl+Alt+Shift+D" },
  "smart_grab": { "enabled": false, "permission_state": "unknown" },
  "folder_drop": { "include_subfolders": false },
  "undo": { "window_seconds": 10 },
  "sound": { "enabled": true },
  "pdf": { "page": 1 },
  "diagnostics": { "capture_engine_stderr": true, "keep_job_logs": 50 },
  "limits": { "max_input_bytes": 2147483648, "max_parallel_images": 4 }
}
```

Rules:

- Every key above maps to a `SPEC.md` requirement or to a safety limit named in this document.
  `wheel.segments`/`order` are user order + preference; at show time they are filtered by
  validity (`§10.4`), so a stale entry never produces an invalid segment.
- `smart_grab.enabled` defaults **false** and is the only setting that can cause a permission
  prompt; that prompt is triggered by the user flipping the toggle, never at launch
  (`SPEC.md` §2).
- Invalid values are repaired on load (clamped, or reset to default) and the repair is logged.
  A corrupt file is copied to `settings.json.bak.<ts>` and defaults are written — never a
  launch failure.
- `never_overwrite` is exposed in the schema but not user-toggleable in v1: `SPEC.md` §5 makes
  "never overwrite" a rule, not a preference. It exists so the invariant has a name in code.

*Update 2026-11-01 (card `U2`, implementation):* two keys were added to make the `SPEC.md` §5
rules actually toggleable, which §15 did not provide for:

- `output.chosen_folder` (default `null`) — `output.location` could already be set to a chosen
  folder, but the schema had nowhere to record *which* folder. `location` switches the rule,
  `chosen_folder` carries the destination; a `chosen_folder` location with no absolute path is
  repaired back to `same_folder`.
- `output.use_batch_subfolder` (default `true`) — `SPEC.md` §5 requires the batch rule to be
  user-toggleable, and a threshold alone cannot turn the rule off.

Both are in `model/settings_schema.rs` with the rest, and the schema/field bijection is a test,
so this section and the code cannot drift silently.

---

## 16. Security posture (release-blocking)

| # | control | where |
|---|---|---|
| S1 | argv[1] literal, paths absolute, no leading `-`, no UNC, no device paths | `engine/proc.rs` + builders, property tests |
| S2 | output path derived and jailed; traversal impossible | `model/naming.rs` |
| S3 | scrubbed env, fixed CWD, no shell, no `cmd.exe` | `engine/proc.rs` |
| S4 | wall-clock timeout + kill + partial-output cleanup | `engine/proc.rs`, `job/worker.rs` |
| S5 | bounded stdout/stderr capture (1 MiB each) | `engine/proc.rs` |
| S6 | content-first detection with structural container validation, size/entry limits | `model/detect.rs` |
| S7 | output validators; `Done` unreachable without one | `engine/verify.rs` |
| S8 | no network code in the app; engines spawned with no proxy/credentials in env | whole app; `warden` gate |
| S9 | licence gate: no GPL/AGPL in the shipped set; `ffmpeg --enable-gpl` count = 0 | `scripts/license-assert.sh`, `audit-licenses.sh` |
| S10 | fuzz corpus for the parsers we own (ZIP/CFB directory reader, detection) and for engine inputs | CI + `warden` |

Threats explicitly out of scope for v1, stated so they are not assumed handled: a user who
runs the app **as administrator** (Windows) hands elevated privilege to a parser crash;
macOS has no direct equivalent of the no-activate window style, so the click-through model
there is unproven; and no engine is sandboxed beyond process isolation plus the argv/env
controls above.

---

## 17. Decisions taken here (and what needs a signature)

These were open or contradictory in the source documents. I decided them so downstream cards
do not each decide differently. Each needs `warden` (security/licence) or the stakeholder
(scope) to object now rather than at release.

| # | decision | rationale | needs |
|---|---|---|---|
| 17.1 | resvg ships as a **sidecar**, not an in-process crate | `SPEC.md` §9.1/§9.2 say sidecar, §9.3 says crate; an SVG parser is an attack surface, and a process boundary costs ~13 ms | `warden` |
| 17.2 | PDFium runs in a **first-party sidecar** (`ezc-pdf`), not in the app | `SPEC.md` §6 says sidecar-process and lists the `pdfium-render` crate; FFI in-process breaks rule 7 (parser crash = app crash) | `warden`, scope note for `P2` |
| 17.3 | **Pandoc is not shipped in v1** | GPL-2 binary, and LibreOffice 26.8.2 has a real Markdown export filter (D2, measured). Fallback for an LO without the filter is `txt`, not a new GPL process | stakeholder (scope) |
| 17.4 | Routes are hidden until **proven** (`§0`) | encodes "evidence over claims" in the data structure; makes the app's advertised scope grow with tests, not with code | stakeholder, because `SPEC.md` §3 reads as a v1 promise |
| 17.5 | Multi-file drop offers the **intersection** of targets | a target that fails for 3 of 5 files is worse than fewer targets; union-with-warning is the fallback | none (UX, reversible) |
| 17.6 | Wheel clamps to the **cursor monitor's work area**, not the virtual screen | fixes the spike defect that leaves S1's cross-monitor case unproven | none (bug fix `U3`) |
| 17.7 | **No idle auto-dismiss** in v1 | an unmeasured dismissal path can only lose work | none |
| 17.8 | Undo is **same-session only**; a crash abandons the journal without deleting | deleting files after a restart is unrecoverable; keeping them is not | none |
| 17.9 | PDF raster input is **page 1** in v1 (`pdf.page` exists in settings) | `SPEC.md` is silent; page 1 is the predictable default and multi-page output is a product decision | stakeholder (minor) |
| 17.10 | AVIF output is **proven on the dist that ships**: `heifsave` + `--compression av1 --Q 60 --effort 6 --bitdepth 8`, EXIF+ICC survive, and the unchanged suite now reports **34/34 on the final 43-file dist** (`spikes/s4/EVIDENCE.md` §1, 2026-11-01). `SPEC.md` §9.3's `speed`→`effort` correction already landed in `t_803b8add` | the parameter name in the spec was wrong; the round-trip is now measured twice, on both dists | none — discharged |

---

## 18. Install layout and packaging hooks (`P2`)

```
<install>/                                  Windows: %LOCALAPPDATA%\EZ Convert\  (NSIS per-user, no UAC)
├── ez-convert.exe
├── ezc-pdf.exe            externalBin     (FFI isolated here)
├── ffmpeg.exe             externalBin     (LGPL build, licence gate)
├── ffprobe.exe            externalBin
├── resvg.exe              externalBin
├── resources/
│   ├── engines/vips/      data+exe: vips.exe, vipsheader.exe, DLL closure, licenses/
│   └── engines/libreoffice/  data: the LibreOffice tree (Windows only)
└── licenses/              THIRD-PARTY-NOTICES.md + per-component licence texts
```

- **Executables via `externalBin`** (Tauri signs them, strips the `-<triple>` suffix, and they
  land next to the app exe). **Multi-file engine trees via `resources`** — `vips.exe` must
  find its 32 DLL siblings, so it is launched by absolute path with CWD set to its own
  directory.
- macOS: LibreOffice **cannot** live inside `Contents/Resources` (Tauri's bundler does not
  sign nested code there and flattens `.app` symlinks). It ships as a separate signed
  **Document Engine Pack** (`SPEC.md` §7); the app probes for it and reports
  `EngineUnavailable` until it is installed.
- Sidecar names are resolved relative to the executable, so a test must run from a real
  install location (and from `/Applications` on macOS), not from `target/release`.
- The vips dist ships its `licenses/` directory (7 files) next to the DLLs — copying the
  sidecar without it is a licence defect, and `audit-licenses.sh` is the gate that catches it.
- No auto-update, no telemetry, no network capability in the Tauri capability set.

---

## 19. Testing strategy

| layer | what | gate |
|---|---|---|
| unit — detection | table-driven over the fixture corpus: every canonical type, a JPEG renamed `.png`, a truncated JPEG/PNG/ZIP, a zero-byte file, a 2 GiB-spoofed header | `cargo test`, fast, no engines |
| unit — matrix | every `Proven` route resolves to an existing evidence file; every `Planned` route names a live card; no image route resolves to ffmpeg | `cargo test` |
| unit — naming | 1,000 simulated collisions produce 1,000 distinct names; reserved names rejected; path length limit | `cargo test` |
| unit — journal | undo removes only recorded outputs; refuses on hash mismatch; refuses to delete an input; startup never deletes | `cargo test` with a temp dir |
| unit — argv builders | property tests for A1-A7: no user string in argv[0]/argv[1], no leading `-` argument, absolute canonical paths only | `cargo test` |
| contract — vips | on a Windows CI job: fetch the pinned dist, run V1/V2/V6/V8/V9/V10 and assert exit codes, output magic, the avif EXIF+ICC round-trip, and that V10's silent-black output is *rejected* by the wrapper | CI (Windows) |
| contract — soffice | on Linux **and** Windows (`soffice` / `soffice.com`, §6.2.1): D1-D7 with content checks (the S3 drivers are the template), plus N2/N3 negative controls asserting `DecodeFailed` / `NoOutput` classification | CI |
| e2e — one per family | drop → detect → matrix → convert → verify → journal → undo, driven through the real `job` API | CI on Windows first |
| fuzz | detection parser, ZIP/CFB directory reader, and engine inputs (JBIG TIFF, malformed HEIC, truncated DOCX) | `cargo-fuzz` + `warden` review |
| packaging | installer builds, app runs from the installed location, engines resolve, `license-assert` and `audit-licenses` pass on the shipped tree | CI |

Corpus discipline (engineering rule 5): no route flips to `Proven` without a committed
fixture (a known input with known output properties) and a test that consumes it.

---

## 20. Honest gaps in this document

1. **ffmpeg has no contract.** Audio/video is `SPEC.md` v1 scope and is `Planned` here. `C2`
   owns the measurement; nothing in this document should be read as an ffmpeg design.
2. **PDFium and resvg argv are unspecified** — the binaries were never built or run in this
   project (`§6.4`, `§6.5`).
3. ~~**AVIF is proven on the S2b dist, not on the final one.**~~ **Closed 2026-11-01.** The
   unchanged suite was re-run against the final dist: 34 assertions, 0 failures, on a directory
   hash-identical to the S2c manifest (`spikes/s4/EVIDENCE.md` §1). The CI vips contract test
   (`§19`) still has to reproduce it, but the measurement now exists on the shipping artifact.
4. **macOS is unmeasured for every contract in this document**, including the wheel
   (`§10.5`). S1-S4 ran on Windows (S1, S2, S4) and Linux (S3).
5. **Windows LibreOffice is measured for the seven document directions** (`spikes/s3/EVIDENCE.md`
   §9, card `t_20ced994`) and now for **real-viewer text fidelity** in Chrome, Edge and Brave
   plus Firefox's `pdf.js` text layer (`spikes/s4/EVIDENCE.md` §2, `§6.2.4`) — which is where the
   accented-text caveat was confirmed as a real cross-engine defect. Still **not** measured:
   Acrobat Reader itself, macOS Preview, Vietnamese stacked diacritics, very large documents, and
   the bundled-tree layout `P2` will ship. Legacy `.doc/.xls/.ppt` are unmeasured on both
   platforms.
6. **Timeouts and concurrency defaults are safety limits, not measurements.** No large file
   has been converted yet.
7. **Legacy binary Office formats** (`.doc/.xls/.ppt`) have no fixture on either platform, so
   their routes are `Planned` even though the same LibreOffice filters should handle them.
8. **Wheel placement on multi-monitor** is fixed by design (`§10.2`) but not yet proven on
   hardware; the only monitor evidence is single-monitor (`DISPLAY1`).

---

## 21. Evidence index

| claim | source |
|---|---|
| vips build, dist contents, import audit, HEIC decode, orientation, the four copy directions | `build/windows/EVIDENCE-S2-libvips.md`; card `t_7d0ad299` |
| JBIG TIFF silent black frame vs `tiffload --fail-on error` | `t_65624d81` (`jbig-c34661-probe.txt`, `S2C-FIX-REPORT.md` §5) |
| avif save/decode, `effort`-not-`speed`, EXIF+ICC survival, `--encoder` rc=0 trap, config-lying trap | `build/windows/EVIDENCE-S2b-avif-encode.md`; card `t_803b8add` (warden-approved) |
| **avif re-proven on the final dist** (34/34, EXIF IFD0 ×4 + 3,144 B ICC through jpg→avif→jpg, orientation-6 at 1152×1728 with no stale tag, dav1d decode, dist hash-identical to S2c) | `spikes/s4/EVIDENCE.md` §1 + `artifacts/avif-final.log`, `artifacts/FINAL-dist-spec-aom-sha256.txt`; card `t_61e55863` |
| **real-viewer text fidelity for Windows `docx → pdf`** (Chrome/Brave/Edge GUI: render, find result, clipboard via `CF_UNICODETEXT`; Edge + `pdf.js` lose `é`, PDFium keeps it) | `spikes/s4/EVIDENCE.md` §2 + `artifacts/*-clipboard.txt`, `artifacts/*-uia*.txt`, `artifacts/*.png`; card `t_61e55863` |
| vips argv hardening (system op, leading `--`, UNC, traversal, env) | `warden` audit `t_cccf4bf3`, condition C-3 |
| 7 document directions, argv, timings, failure modes, fidelity (Linux leg) | `spikes/s3/EVIDENCE.md` §1-§8, `spikes/s3/run_s3.py`, `result.json`; card `t_d910a4c8` |
| the same 7 directions on **Windows**, `soffice.com` vs `soffice.exe` vs `soffice.bin`, file-URL profile rule, real Calibri/Cambria, text-layer degradation, install-cold 5,111 ms | `spikes/s3/EVIDENCE.md` §9, `spikes/s3/run.ps1`; card `t_20ced994` |
| docx→pdf re-run on this host (26,460 B, `%PDF-1.7`), missing-input rc=1, garbage-.docx silent 12,388 B PDF, `<stem>-<sheet>.csv` naming | this card's own re-run, 2026-11-01, `/opt/data/profiles/wright/cache/scratch/a1-probe-*` |
| focus-free overlay, click-through, Explorer drop, Esc/click-away, re-summon | `evidence/S1-EVIDENCE.md`; card `t_37bfdf33` |
| wheel window config that produced those results | `spike/src-tauri/tauri.conf.json`, `src/main.rs`, `src/win32.rs` (card `t_37bfdf33`) |
| engine/sidecar packaging mechanics and signing order | `converter-packaging-strategy.md` |
| ffmpeg build constraints, EXIF destruction, GIF palette route | `media-engine-stack-2026.md` |
| image matrix decisions D1-D5 | `SPEC.md` §9 (card `t_d4b99d30`) |
