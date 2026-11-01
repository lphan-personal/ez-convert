# Wheel overlay (U1)

The radial target picker for EZ Convert (`SPEC.md` §1/§2, `ARCHITECTURE.md` §10.4).
This directory is a runnable prototype: open `index.html` in any Chromium, or run the
recorded interaction check below.

```
ui/wheel/
  index.html      overlay surface (no inline script or style: CSP default-src 'self')
  wheel.css       material, typography, reduced-motion / reduced-transparency fallbacks
  model.js        pure geometry, layout, hit-testing, momentum, legibility floor
  spring.js       the only animation primitive (damping ratio + response)
  wheel.js        rendering and pointer handling; no geometry of its own
  tests/          node --test, no DOM, no dependencies
  checks/         real-Chromium interaction check over the DevTools protocol
```

## Run it

```bash
# unit tests — pure model and spring
node --test ui/wheel/tests

# the recorded interaction check (real Chromium, no npm install)
CHROME=/opt/hermes/tools/chromium-1208/chrome-linux64/chrome \
  node ui/wheel/checks/interaction-check.mjs
```

The check prints one line per assertion, writes `report.json` and six PNG screenshots
to `$SHOT_DIR` (default `/tmp/u1-wheel-shots`), and exits non-zero on any failure.

## The rules this prototype enforces

**Hard cap of six, never smaller.** `model.layout()` never returns a ring with more than
six entries. A seventh target turns the sixth slot into a `more` segment that opens a
sub-ring. Capacity is reported, not guessed: with twelve targets the second ring fills
and the remainder comes back in `unplaced` with `reason: 'legibility-floor'` — it is not
silently dropped and it is not rendered too small to read.

**Legibility floor (11 pt = 14.67 px at 96 dpi).** A ring is legal only if
`fontPx >= FLOOR_PX` **and** the hole it leaves is still a hub (`inner >= 56 px`).
That second clause is why a third ring is refused even when it would have only two
segments: a ring wide enough for a label can still swallow the centre, and the hub is
where the wheel says what it is doing. The model rejects the layout; the type never
shrinks to fit.

**Hit-testing at every angle.** One function, `model.hitTest(point, ring, rotation)`,
is the only answer to "which segment is under the pointer". Segments are half-open
intervals `[start, end)` so a boundary belongs to exactly one segment; rotation is
subtracted before the floor, so a spun ring hit-tests identically to a still one. The
check sweeps 2160 points (720 angles × 3 radii) through the live DOM and compares
`elementFromPoint` against the arithmetic.

**Only valid targets.** `wheel.js` never filters: the caller hands it the list the
matrix says is offered for the detected type (`model/matrix.rs`, card C1). An empty
list yields no segments and a hub that says so. The demo buttons in `index.html` select
a *stub* catalogue — real detection is Rust-side and is not part of this card.

**Apple motion, one primitive.**
- `spring.js` takes damping ratio and response, not stiffness/mass: `omega = 2*pi/response`,
  `k = omega^2`, `c = 2*damping*omega`, integrated at 1/240 s fixed substeps.
- Settling and emphasis are critically damped (1.0 / 0.3–0.4). Drag release is 0.8 / 0.35,
  so a thrown ring overshoots slightly — overshoot is the physical consequence of the
  velocity the hand gave it, which is why it is only allowed there.
- Retargeting never resets the value; it adopts the release velocity verbatim
  (`setTarget(target, velocity)`), so there is no seam between gesture and animation.
- Grabbing a moving ring calls `jumpTo(presentedValue)`: it stops exactly where it is on
  screen with zero residual velocity, then follows the pointer 1:1 from the grab position.
- Momentum projection uses the exponential form, `(v/1000) * d / (1 - d)` with `d = 0.998`,
  applied along the arc. A release below 120 px/s at the rim is a placement, not a flick.

**Material.** The outer rim is `backdrop-filter: blur(30px) saturate(180%)` over
`rgba(28,28,30,.55)`. The sub-ring is drawn *inside* the hub, so opening it turns the hub
opaque and removes its blur — two translucent surfaces stacked is the one thing the
material rules forbid. `prefers-reduced-transparency` makes every surface solid;
`prefers-reduced-motion` removes the spring entirely (state change is immediate) and
leaves only an opacity fallback; `prefers-contrast: more` goes near-solid with an outline.

**Type.** `system-ui` first, size-dependent tracking from `model.trackingFor()`: −0.02em at
22 px, −0.01em at 16 px, 0 at 14 px, +0.01em below. Values are written from the model, so
a label can never be rendered at a size the floor calculation did not see.

## What this prototype does not do

- The Win32 overlay chrome (no-activate window, region clipping, global hotkey, click-away
  guard) belongs to `U3`. This is a page, not the overlay.
- macOS is unmeasured: none of the material or motion behaviour here has been run in
  WebKit or on a Mac.
- Detection and the route matrix are Rust-side (`C1`); the demo catalogue here is a stub.
- Segment glyphs are text labels only. Icons were not in scope.
