# U3 — tray/menu-bar, global hotkey, smart-grab consent

Evidence for card `t_1f1d275d`. Everything below was produced on **win11-dev**
(`ryo@10.0.0.191`, Windows 11 Pro 26200, 6 vCPU, autologon console session) on
2026-11-01, from the binary built by `spikes/u3/build.sh`.

## 0. What the binary is

`spikes/u3/u3_spike.rs` includes the **shipped** modules by path
(`src-tauri/src/shell/{geometry,accelerator,hotkey,screen}.rs`) and the shipped
`grab.rs` in the unit-test harness. The only spike-local code is the throwaway window
and the synthetic key press, so the placement and registration code measured below is
the code the app runs.

```text
$ bash spikes/u3/build.sh
== cross-compiling u3_spike.exe (shipped shell modules included by #[path])
-rwxr-xr-x 5350659 out/u3_spike.exe          (bin currently 5,350,659 B)
== Linux unit tests over the same shipped modules
test result: ok. 33 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

`cargo check --locked --offline --target x86_64-pc-windows-gnu` on the crate (which now
enables Tauri's `tray-icon` feature and calls the new modules from `lib.rs`) exits 0.

## 1. Acceptance 1 — hotkey fires, wheel at the cursor, correct monitor

Mode `hotkey`: register the configured chord, synthesise it with `keybd_event`, and let
the registered `WM_HOTKEY` drive the same `screen::summon_rect` → `geometry` path the app
uses.

```json
{"mode":"hotkey","registered":"Ctrl+Alt+Shift+D","describe":"Ctrl+Alt+Shift+D registered"}
{"mode":"hotkey","fired":true,"cursor_before":[640,400],"cursor":[640,400],"wheel":[380,140,900,660],"monitor":0,"cursor_is_centre":true,"inside_work_area":true,"status":"PASS"}
```

`wheel` = `[left, top, right, bottom]`. 520×520 centred on the cursor, entirely inside
the monitor's work area, and the wheel is **shown** at that rect (the spike creates the
region-clipped, `WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW`, topmost window at the same rect
S1 proved, then reads `GetWindowRect` back).

## 2. Multi-monitor geometry — the S1 defect this card had to fix

The VM has **one** monitor (`monitors: 1`, 1280×800, work area 1280×752 — a 48 px
taskbar), so §10.2's negative-coordinate case cannot be produced on this hardware. It is
covered two ways instead of being assumed:

1. **Real, on the VM** — cursor moved to the monitor centre and 5 px from each of the four
   work-area edges, wheel window created, rect read back:

```json
{"mode":"place","monitors":1,"monitor_rects":[{"device":[0,0,1280,800],"work":[0,0,1280,752]}]}
{"mode":"place","label":"monitor0-centre","cursor":[640,400],"wheel":[380,140,900,660],"centre":[640,400],"monitor":0,"work":[0,0,1280,752],"cursor_is_centre":true,"inside_work_area":true,"status":"PASS"}
{"mode":"place","label":"monitor0-top-left+5","cursor":[5,5],"wheel":[0,0,520,520],"centre":[260,260],"monitor":0,"work":[0,0,1280,752],"cursor_is_centre":false,"inside_work_area":true,"status":"PASS"}
{"mode":"place","label":"monitor0-top-right-5","cursor":[1274,5],"wheel":[760,0,1280,520],"centre":[1020,260],"monitor":0,"work":[0,0,1280,752],"cursor_is_centre":false,"inside_work_area":true,"status":"PASS"}
{"mode":"place","label":"monitor0-bottom-left+5","cursor":[5,746],"wheel":[0,232,520,752],"centre":[260,492],"monitor":0,"work":[0,0,1280,752],"cursor_is_centre":false,"inside_work_area":true,"status":"PASS"}
{"mode":"place","label":"monitor0-bottom-right-5","cursor":[1274,746],"wheel":[760,232,1280,752],"centre":[1020,492],"monitor":0,"work":[0,0,1280,752],"cursor_is_centre":false,"inside_work_area":true,"status":"PASS"}
{"mode":"place","summary":"PASS","label_count":5}
{"mode":"place","left_on_screen":true,"hwnd":2556914,"cursor":[1274,746]}
```

Read the two columns together: at the centre the cursor **is** the wheel's centre; 5 px
from an edge both properties cannot hold at once, and the rule chooses *containment*
(the wheel is clamped to `rcWork`, bottom edge 752 = above the taskbar). That is the
§10.2 contract, and it is what the old virtual-screen clamp got wrong.

2. **Synthetic layouts, in unit tests** (`shell/geometry.rs`, run by the harness above):
   a monitor at negative x, a monitor above the primary at negative y, a monitor to the
   right, a tie on a shared edge, and a wheel larger than the work area. Those tests are
   the ones that would fail on the S1 clamp and they pass on the shipped rule.

**Honest gap:** no second physical or virtual display existed on the VM (Microsoft Basic
Display Adapter, single head), so "wheel lands on the *other* monitor" is proven by the
unit tests over synthetic layouts, **not** end-to-end on hardware.

## 3. Acceptance 2 — hotkey conflict detection

Mode `conflict`: this process occupies `Ctrl+Alt+Shift+D` first (the way another
application would), then asks the shipped registrar for the same chord.

```json
{"mode":"conflict","occupier_registered":true,"occupier_error":0}
{"mode":"conflict","attempts":[{"accelerator":"Ctrl+Alt+Shift+D","code":1409}],"status":"PASS fallback=\"Ctrl+Alt+D\""}
```

`1409` is `ERROR_HOTKEY_ALREADY_REGISTERED`. The app does not silently pretend it owns
the chord: the attempt is recorded with the OS error code, the ladder (`accelerator
.conflict_ladder`) yields `Ctrl+Alt+D`, and that chord is what gets persisted so the next
launch does not fight the same conflict. `AllTaken` and `Failed` (non-conflict errors,
which stop the ladder) are unit-tested separately — a bad virtual key must not be retried
eight times.

## 4. Acceptance 3 — smart-grab consent

Proven in `shell/grab.rs` tests (all green in the harness above), because it is a policy
property: `disabling_asks_the_os_nothing_at_all` counts probe calls — exactly 1 after
enabling (with `prompt = true`), still 1 after disabling; `grabbing_while_disabled_reads_no_selection`
asserts the selection source is called **0** times. `a_denied_permission_leaves_the_feature_off_and_says_where_to_grant_it`
covers the macOS wording (System Settings › Privacy & Security › Accessibility) and the
reset so a second attempt asks again.

Windows reports `NotRequired`: reading Explorer's selection is an in-process shell call
as the signed-in user, so there is no consent to obtain. The macOS Accessibility prompt
call site exists as `SystemPermission` but is **not measured** (no macOS host in this
project yet) and reports `Unsupported` rather than faking a flow.

**Gap, stated plainly:** the *selection reader* is not implemented in this build
(`grab::platform_selection_source` returns `Unsupported("reading the Explorer selection
is not implemented yet")`). The consent half — off by default, probe only on enable,
nothing on disable — is complete and tested. The reader (COM `IShellWindows` →
`IShellBrowser` → `IFolderView` → `SHGetNameFromIDList`) is its own card.

## 5. Tray / menu-bar

Implemented in `shell/tray.rs` against Tauri 2's `tray-icon` feature: a tray icon with
`Show conversion wheel`, a `Smart grab the current selection` check item mirrored from
`smart_grab.enabled`, `Settings…`, and `Quit`. It type-checks for Windows
(`cargo check --target x86_64-pc-windows-gnu`, exit 0) but **was not run**: this container
cannot build a Tauri GUI (needs WebKitGTK for the host target, and the wheel/main windows
need a display), and the VM run above exercises the hotkey/placement/consent code, not the
tray shell. Tray behaviour is the residual unmeasured surface of this card, together with
macOS for everything.
