# U1 evidence — wheel overlay

Card: `t_7a09ae2b` · Repo commit: this one · Host: Linux 7.0.14-26-pve (container)
Browser: Chromium 1208 (`/opt/hermes/tools/chromium-1208/chrome-linux64/chrome`), headless,
760×900, device scale 1.

Everything below is transcribed from a run on this host. The check is committed
(`ui/wheel/checks/interaction-check.mjs`) so it can be re-run, and it exits non-zero on
any failure.

## 1. Unit tests — `node --test ui/wheel/tests`

```
ℹ tests 28
ℹ pass 28
ℹ fail 0
ℹ skipped 0
```

The 28 cover: the six-segment cap at every target count 1…40, the `more` segment and the
sub-ring, the remainder being reported (`unplaced` + `reason`) instead of dropped, the
legibility floor and the hub-radius clause, size-dependent tracking, the exhaustive
720-angle × 3-radius × 7-rotation hit-test sweep, boundary ownership, hub/outside
rejection, momentum projection (exponential form), the flick threshold, angular
projection, spring critical-damping / overshoot / handoff / retarget / settle / response,
reduced motion, the Apple parameter table, material nesting, and ring nesting geometry.

## 2. Recorded interaction check — `node ui/wheel/checks/interaction-check.mjs`

```
PASS  image type: six segments at most, overflow behind one more segment
PASS  only valid targets for the detected type appear
PASS  nothing rendered smaller than the 11 pt legibility floor
PASS  hit-testing is correct at every angle (DOM raster vs arithmetic)
PASS  labels sit inside their ring band and never overlap each other
PASS  material is backdrop-filter glass, and no surface stacks translucency
PASS  feedback starts on pointer-down, not on release
PASS  a tap commits without moving the ring (no momentum without a drag)
PASS  a flick projects momentum and the ring overshoots (damping 0.8)
PASS  catching the ring mid-flight stops it exactly where it is
PASS  keyboard move uses a critical spring: no overshoot, monotone approach
PASS  the more segment opens a sub-ring, and the nested surface turns opaque
PASS  typography: system-ui with size-dependent tracking
PASS  prefers-reduced-motion: the spring is removed, not shortened
PASS  prefers-reduced-transparency: surfaces become solid
PASS  Escape dismisses the wheel

SUMMARY pass=16 fail=0 checks=16
```

| check | measured |
|---|---|
| cap + overflow | `ring0 png,jpg,webp,avif,heic,more / ring1 tiff,gif` — 6 entries in ring 0, 2 in the sub-ring |
| valid targets only | `image 7, video 6, unknown 0`; video never offers png, unknown offers no ring at all |
| legibility floor | 6 labels, smallest **22.0 px** (floor 14.67 px = 11 pt); planned `fontPx` ≥ floor for every ring |
| hit-testing at every angle | **2160 points** (720 angles × 3 radii) through `document.elementFromPoint`, 18 on an exact boundary; every point matched the arithmetic |
| label layout | all 6 label centres inside r=144…240; closest pair 114 px apart (no collisions) |
| material | `blur(30px) saturate(1.8)` over `rgba(28,28,30,0.55)` |
| pointer-down feedback | pressed label scale **0.977**, highlight opacity **0.78**, before release |
| tap commit | `commit → webp`, then `converted` |
| flick | release **319 °/s** → projected **193°** → snap target **180°** → **1.48° overshoot** → settled 179.99° in 33 frames |
| interrupt | ring caught at **28.6°** while heading for 60.0°, and it stayed at 28.6° (moved 0.00° over the next 90 ms) |
| keyboard settle | 0° → 60.0° in 25 frames, **overshoot 0** (damping 1.0) |
| sub-ring | `openLevel 1`, hub `rgba(28,28,30,0.92)` / `backdrop-filter: none`, 2 children |
| typography | 22 px / −0.44 px tracking vs 10 px / +0.1 px; `system-ui` first |
| reduced motion | snapped to 120.0° with no intermediate frames; fallback `transition-property: opacity` |
| reduced transparency | `backdrop-filter: none`, `rgb(22,22,24)` (alpha 1) |
| Escape | phase `dismissed`, `.dismissed` applied |

Screenshots (written next to `report.json` in `$SHOT_DIR`, attached to the card):
`01-idle.png`, `02-pressed.png`, `03-subring.png`, `04-reduced-motion.png`,
`05-reduced-transparency.png`, `06-subring.png`.

## 3. What the checks are actually asserting

- **Hit-testing.** `model.hitTest` is the only implementation; the check compares it
  against `document.elementFromPoint`, i.e. the pixels the browser will really deliver a
  drag to. Two real defects were found this way and fixed: 1 px divider/edge strokes and
  the notch were winning `elementFromPoint` over the wedge underneath (they are
  `pointer-events: none` now), and the press state was not raising the highlight because
  only `selected` was wired (`pressIndex` counts too).
- **Momentum.** The check reads the release event the app itself logs (velocity,
  projected angle, snap target) and then samples `requestAnimationFrame` for 650 ms,
  asserting both that the ring passed the target (damping 0.8 overshoot) and that it
  settled on the projected segment. A tap is checked separately and must not move the
  ring at all.
- **Interruptibility.** A flick is started, then the ring is grabbed 45 ms later; the
  check asserts the rotation is frozen within 0.05° and that the catch point is not the
  target — i.e. it was caught mid-flight, from the presentation value.
- **Reduced motion / transparency.** Both are driven through CDP
  `Emulation.setEmulatedMedia`, not by editing CSS, so `matchMedia` and the media queries
  are exercised the way the OS would exercise them.

## 4. Gaps — stated, not papered over

- **macOS unmeasured.** Nothing here has run in WebKit or on a Mac. `ARCHITECTURE.md`
  §10.5 applies: do not port the Win32 constants by analogy.
- **Overlay chrome is `U3`.** No-activate window, `SetWindowRgn` clipping, global hotkey,
  click-away guard and multi-monitor clamping are not in this card or this prototype.
- **Detection is stubbed here.** The demo buttons pick a catalogue; the real type → target
  list is `model/matrix.rs` (`C1`) and the wheel is written to consume it as an argument.
- **No icons.** Segments are text labels. An icon set was not requested and is not drawn.
- **Sound/haptics** (skill principle 13) are `SPEC.md` §5 settings, not part of the wheel
  rendering here.
