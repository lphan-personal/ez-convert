# The overlay window

The wheel is not a page in the main window. It is a second, transparent, always-on-top
window that appears under the pointer when the user presses the hotkey, picks a target
and disappears. This document is what that window is, how it is built, and how it is
proven.

Implementation: `src-tauri/src/shell/overlay.rs` (the window), `shell/screen.rs` (where
the pointer and the screens are), `ui/wheel/overlay.js` (the page's side of it),
`shell/probe.rs` + `scripts/overlay-probe.sh` (the measurement).

## Why it is not in `tauri.conf.json`

It was, once: one window, label `main`, and `summon()` looked up a window labelled
`wheel` that nothing ever created. That lookup returned `None` on every platform, so
the "position it and show it" branch was dead code and Ctrl+Alt+Shift+D produced
nothing at all. The window is now built at runtime because every property that makes
it an overlay is a decision, not config:

| Property | Why |
|---|---|
| `transparent`, `decorations(false)`, `shadow(false)` | the ring must be the only painted pixel; a title bar or a drop shadow would draw a window around it |
| `always_on_top`, `visible_on_all_workspaces` | it must be reachable while the user is dragging a file in another app, including full-screen apps |
| `skip_taskbar`, `resizable(false)`, `closable(false)` | it is a surface, not a document |
| `focusable(false)`, `focused(false)` | **the main window never loses focus.** On macOS `show()` is `makeKeyAndOrderFront`, which would steal the key window from the app the user is working in; a non-focusable window is ordered front without becoming key, and still receives mouse events |
| `accept_first_mouse(true)` | the first click selects a segment instead of merely activating the window |
| `background_throttling(Disabled)` | the window is created hidden at startup so the first press is a paint, not a webview construction — a throttled hidden webview would miss the summon event |

It is created in `.setup()` (`shell::overlay::install`) and reused for the process
lifetime.

## Click-through outside the disc

The window is a 520×520 square; the wheel is the disc inscribed in it. The corners must
belong to whatever is underneath:

- **Windows**: the window is *shaped* with an elliptic `SetWindowRgn`. The window
  server itself refuses clicks outside the ellipse, so the click goes to the window
  below with no hook, no polling and no permission. Read back with `GetWindowRgn`
  (an ellipse reports `COMPLEXREGION`) and with `WindowFromPoint` + `GetAncestor` at a
  corner, which must answer with *another* window.
- **macOS**: `NSWindow.ignoresMouseEvents` is toggled by a 16 ms watcher that compares
  the pointer with the disc. AppKit has no per-region hit-testing on a plain `NSWindow`,
  so this is the mechanism that exists; the probe reads the flag back through
  `with_webview` → `NSWindow.ignoresMouseEvents`.

## Dismissal

| Trigger | Mechanism |
|---|---|
| Esc | the watcher polls the key globally — `GetAsyncKeyState(VK_ESCAPE)` on Windows, `CGEventSourceKeyState` on macOS. No event tap, no Accessibility grant. The page also handles `keydown` when it does hold the keyboard |
| click-away | a primary-button press outside the disc (`GetAsyncKeyState` / `NSEvent.pressedMouseButtons`) |
| close | the window is not hidden on the spot: `wheel://dismiss` starts the close spring in the page, the page calls `overlay_closed`, and a 320 ms fallback hides the window if the page cannot answer. A re-summon inside that window is not hidden by the previous dismissal (a summon generation counter) |

`Focused(false)` is deliberately **not** a dismissal reason: the overlay is
non-activating, so "not focused" is its normal state.

## Motion

Open and close use the one animation primitive the wheel already has
(`ui/wheel/spring.js`): damping ratio and response, not stiffness/mass. Open is
critically damped at 0.28 s with a 0.86/0.32 scale; close is the same in reverse.
`prefers-reduced-motion` removes the spring entirely — the state change is immediate,
with opacity only, as a legibility fallback.

## macOS cursor and monitors

Cocoa reports geometry with the origin at the **bottom-left of the primary screen** and
`y` growing **upward**; everything else in this codebase (and `tauri::Position`) uses
the Win32 convention, origin top-left, `y` down. The conversion is a reflection about
the primary screen's top edge:

```
y_virtual = reference_max_y - y_cocoa - height_cocoa
```

`reference_max_y` is the primary screen's `frame.maxY` (its own height, because the
primary's Cocoa origin is `(0, 0)`). The reflection makes negative-origin monitors —
a screen left of, or above, the primary — come out right with no special case, and the
existing nearest-monitor placement rule (`shell/geometry.rs`) then does the rest.

`NSScreen` is `MainThreadOnly`, so the query is marshalled onto the main thread when the
hotkey thread asks (`shell/summon.rs`); off the main thread `summon_rect` answers `None`
rather than inventing a geometry, and the payload says why (`grab_note`).

macOS points vs Windows pixels: the placement payload is logical on macOS (520 pt stays
520 pt on a Retina display) and physical on Windows, and the window is positioned with
the matching `LogicalPosition`/`PhysicalPosition`.

## The measurement

`ez-convert --overlay-probe <facts.json> [--overlay-hold <seconds>]`:

1. waits for the hidden overlay webview to load;
2. calls `summon::summon` from a **worker thread** — the same function, and the same
   thread situation, as the global hotkey callback;
3. waits for the page's first painted frame (`overlay_rendered`), which gives
   **summon → visible in ms**;
4. records the window (visible, position, size, scale, always-on-top, decorated, key
   status), the placement payload, the monitor list, and whether the main window kept
   focus;
5. warps the pointer to the disc centre and to a corner, reading back the click-through
   state at both, then puts the pointer back;
6. writes the JSON, touches `<facts>.ready`, holds the ring on screen for the
   screenshot, and exits 0 only when every asserted check passed.

`scripts/overlay-probe.sh` runs it, photographs the screen (`screencapture` /
`System.Drawing.CopyFromScreen`), crops the capture to the wheel's rect and checks that
the crop is not blank and that its centre differs from its corner
(`scripts/png-nonblank.py`, proved in both directions by
`scripts/test-png-nonblank.py`). A screenshot the runner cannot take is recorded, never
asserted. On Windows the capture writes **three frames a second apart** and keeps them all:
`Add-Type -AssemblyName System.Windows.Forms` is a cold start that can outlast a short
`--overlay-hold`, and a frame taken after the dismissal photographs the desktop — a
maximised console window is not blank and not uniform, so the crop check alone cannot tell
it from a wheel. The Windows jobs hold the ring for 25 s for the same reason.

CI: `.github/workflows/overlay-measure.yml` (macOS arm64 + Windows, **build tree**) and
`.github/workflows/windows-installer.yml` (the **installed** NSIS build, `%LOCALAPPDATA%\EZ
Convert\`), publishing the facts, the frames and the capture to the `overlay-status` /
`ci-windows-installer` branches.

## What is *not* proven by this

- The **global hotkey on macOS** is still `Failed` in `shell/hotkey.rs` (the macOS
  adapter is unimplemented). The probe drives the summon path the hotkey calls, not the
  chord itself. Tray → Summon and the IPC command reach the overlay today.
- Per-segment interaction *inside* the ring under the shaped window (mouse capture over
  a region-clipped HWND) is asserted only by the click-through samples, not by a
  scripted drag.
