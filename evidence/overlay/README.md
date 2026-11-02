# Overlay acceptance evidence — run 41491695772 @ 2ad4563

Workflow: `.github/workflows/overlay-measure.yml` (jobs `overlay (macOS arm64)` and
`overlay (Windows x64)`). Both jobs built the app from this branch, ran the portable unit
tests, and then ran the app in `--overlay-probe` mode on a real desktop.

The app measured itself: `summon::summon` — the same function the global hotkey, the tray
item and the `wheel_summon` command call — driven from a worker thread exactly as the
hotkey callback does, then the window state, the placement payload, the monitor list, the
click-through boundary and the summon→paint time were read back and written as JSON. The
harness photographed the screen while the ring was up, cropped the capture to the wheel's
rect and checked the crop (`scripts/png-nonblank.py`).

## Verdicts

| Platform | `ok` | summon → visible | cold (first, pre-warm) | screenshot | asserted checks |
|---|---|---|---|---|---|
| macOS 15 arm64 (`macos-latest`, 1024×768, scale 1) | **true** | **44 ms** | 50 ms | captured, ring visible | 15/15 |
| Windows Server 2025 x64 (1024×768) | **true** | **45 ms** | 62 ms | captured, ring visible | 15/15 |

## macOS facts (`macos/overlay-facts.json`)

- screen 1024×768, work area `0,31–1024,708` — the menu bar's 31 px is excluded, i.e. the
  Cocoa → virtual-desktop flip and `NSScreen.visibleFrame` both came through correctly.
- cursor `(10,10)` → wheel `(0,31,520,520)`: centred on the cursor horizontally and
  clamped to the work-area top (the cursor was over the menu bar), on monitor 0.
- window: created, visible, `focused=false`, `always_on_top=true`, `decorated=false`,
  outer position `(0,31)`, size `520×520`, scale 1.0 — position and size agree with the
  placement payload.
- main window focus: `true` before, `true` after.
- click-through (sampled by moving the window under the fixed pointer, because this
  session refuses `CGWarpMouseCursorPosition`): pointer on the disc →
  `ignoresMouseEvents=false`; pointer in the window's corner →
  `ignoresMouseEvents=true`.
- screenshot crop (`macos/overlay-shot.json`): 520×520, luminance stddev 55.1, centre vs
  corner delta 262.1, 99.91 % of pixels differ from the corner — not a blank capture, and
  not a capture of an empty desktop.

## Windows facts (`windows/overlay-facts.json`)

- cursor `(512,384)` → wheel `(252,124,520,520)`, monitor 0 — centred on the cursor.
- window: created, visible, `always_on_top=true`, `decorated=false`, position and size
  agree with the placement payload.
- click-through: `GetWindowRgn` reports `COMPLEXREGION` (3, i.e. the ellipse), and
  `WindowFromPoint` + `GetAncestor` at the window's corner answers with **another**
  window (the desktop), not ours.
- non-activating: `WS_EX_NOACTIVATE` read back from the window's ex-style.
- main window focus: unchanged.

## What is recorded but not asserted

- `overlay-not-key` on Windows reads `focused=true`. A runner with no console has no
  foreground window, so that reading describes the session, not the window; the asserted
  Windows check is the `WS_EX_NOACTIVATE` style bit plus "the main window keeps focus".
  On macOS the same check passes directly (`focused=false`).

## Reproduce

```bash
gh workflow run overlay-measure.yml --ref overlay/wheel-window
# or, on a machine with a desktop:
BIN=src-tauri/target/release/ez-convert bash scripts/overlay-probe.sh overlay-evidence
```
