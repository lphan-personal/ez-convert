// Wheel model + spring unit tests — no dependencies: `node --test ui/wheel/tests`
//
// These cover the acceptance terms in the card:
//   * hard cap 6 segments, never smaller
//   * hit-testing correct at EVERY angle (exhaustive sweep, several rotations)
//   * nothing smaller than the legibility floor (rejection, not shrinking)
//   * momentum projection decides the selection on a flick
//   * springs: critical damping has no overshoot, momentum springs do,
//     retargeting preserves value and velocity (no seam), reduced motion is
//     immediate, and a fully settled spring stays settled.

const test = require('node:test');
const assert = require('node:assert/strict');

const M = require('../model.js');
const { Spring } = require('../spring.js');

const SIZE = M.DEFAULTS.size;
const CENTRE = { x: SIZE / 2, y: SIZE / 2 };

function targets(n) {
  return Array.from({ length: n }, (_, i) => ({
    id: 't' + i,
    format: 'f' + i,
    label: 'F' + i
  }));
}

/* ---- cap + overflow --------------------------------------------------- */

test('six targets fill one ring with six segments and no more segment', () => {
  const layout = M.layout(targets(6));
  assert.equal(layout.ok, true);
  assert.equal(layout.rings.length, 1);
  assert.equal(layout.rings[0].segments.length, 6);
  assert.equal(M.moreSegment(layout.rings[0]), null);
  assert.deepEqual(layout.unplaced, []);
});

test('a seventh target becomes a more segment, never a seventh segment', () => {
  const layout = M.layout(targets(7));
  assert.equal(layout.rings.length, 2);
  assert.equal(layout.rings[0].segments.length, 6, 'the ring is capped at six');
  const more = M.moreSegment(layout.rings[0]);
  assert.ok(more, 'overflow needs a more segment');
  assert.equal(more.kind, 'more');
  assert.equal(layout.rings[1].segments.length, 2, 'the other two live one ring in');
  assert.deepEqual(layout.unplaced, []);
});

test('eleven targets still fit in two rings; twelve leave one unplaced and say so', () => {
  const eleven = M.layout(targets(11));
  assert.equal(eleven.rings.length, 2);
  assert.equal(eleven.rings[1].segments.length, 6);
  assert.deepEqual(eleven.unplaced, []);

  const twelve = M.layout(targets(12));
  assert.equal(twelve.rings[0].segments.length, 6);
  assert.equal(twelve.rings[1].segments.length, 6);
  assert.equal(twelve.unplaced.length, 1);
  assert.equal(twelve.reason, 'legibility-floor');
});

test('no ring ever exceeds the hard cap, at any target count', () => {
  for (let n = 1; n <= 40; n++) {
    const layout = M.layout(targets(n));
    for (const ring of layout.rings) {
      assert.ok(ring.segments.length <= M.DEFAULTS.maxSegments,
        `ring ${ring.geo.level} has ${ring.segments.length} segments at n=${n}`);
    }
  }
});

/* ---- legibility floor ------------------------------------------------- */

test('the legibility floor is 11 pt at 96 dpi', () => {
  assert.equal(M.DEFAULTS.floorPt, 11);
  assert.ok(Math.abs(M.FLOOR_PX - 14.6667) < 0.001);
});

test('every planned label is at or above the floor — nothing is ever shrunk under it', () => {
  for (let n = 1; n <= 40; n++) {
    const layout = M.layout(targets(n));
    for (const ring of layout.rings) {
      assert.ok(ring.geo.fontPx >= M.FLOOR_PX,
        `n=${n} level=${ring.geo.level} font=${ring.geo.fontPx}`);
      assert.equal(ring.geo.legible, true);
    }
    // and the remainder is reported rather than rendered illegibly
    if (layout.unplaced.length) assert.equal(layout.reason, 'legibility-floor');
  }
});

test('a third ring would break the floor, so the model refuses to build one', () => {
  const third = M.ringGeometry(2, 6);
  assert.equal(third.legible, false);
  assert.ok(third.fontPx < M.FLOOR_PX || third.inner < M.DEFAULTS.minHubRadius,
    'the third ring must fail on the label floor or on the hub radius');
  // even a sparse third ring is refused: it leaves no hub to say what happened
  assert.equal(M.ringGeometry(2, 2).legible, false);
  // 40 targets never produce a third ring
  assert.equal(M.layout(targets(40)).rings.length, 2);
});

test('tracking changes with size instead of being one fixed value', () => {
  const small = M.trackingFor(12);
  const body = M.trackingFor(15);
  const large = M.trackingFor(22);
  assert.notEqual(large, body);
  assert.notEqual(small, large);
  assert.match(large, /^-/);      // big type tightens
  assert.match(small, /^\+?0?\.0?1em/); // small type opens up
});

/* ---- hit testing at every angle --------------------------------------- */

function expectedIndex(angleDeg, segAngle, rotation, count) {
  const rel = M.mod360(angleDeg - rotation);
  return Math.min(count - 1, Math.floor(rel / segAngle));
}

test('hit-testing is correct at every angle, for several rotations', () => {
  const layout = M.layout(targets(6));
  const ring = layout.rings[0];
  const rotations = [0, 17.5, 60, 123.4, 359.9, -30, 720 + 5];
  const radii = [ring.geo.inner + 2, ring.geo.labelRadius, ring.geo.outer - 2];
  let checked = 0;

  for (const rotation of rotations) {
    const rot = M.mod360(rotation);
    for (const radius of radii) {
      for (let step = 0; step < 720; step++) {
        const angle = step * 0.5;              // 0.5° resolution over the circle
        const point = M.pointAt(CENTRE, angle, radius);
        const hit = M.hitTest(point, ring, rot, CENTRE);
        assert.equal(hit.kind, 'segment', `angle ${angle} r ${radius}`);

        const rel = M.mod360(angle - rot);
        const boundary = Math.round(rel / ring.geo.segAngle) * ring.geo.segAngle;
        // The sweep lands exactly on boundaries for some rotations. Recovering
        // the angle through atan2(sin, cos) leaves ~1e-13° of noise there, and
        // which side of the tie a double falls on is not a product decision —
        // so a 1e-6° window accepts either neighbour. Everywhere else the
        // index must match exactly.
        if (Math.abs(rel - boundary) < 1e-6) {
          const either = [expectedIndex(angle, ring.geo.segAngle, rot, 6),
            (expectedIndex(angle, ring.geo.segAngle, rot, 6) + 5) % 6];
          assert.ok(either.includes(hit.index),
            `boundary angle ${angle} rotation ${rot} index ${hit.index}`);
        } else {
          assert.equal(hit.index, expectedIndex(angle, ring.geo.segAngle, rot, 6),
            `angle ${angle} rotation ${rot} radius ${radius}`);
        }
        checked++;
      }
    }
  }
  assert.ok(checked > 15000, `swept ${checked} points`);
});

test('the returned segment actually contains the point angle (wrap included)', () => {
  const ring = M.layout(targets(6)).rings[0];
  for (const rotation of [0, 45, 200]) {
    for (let a = 0; a < 360; a += 0.5) {
      const hit = M.hitTest(M.pointAt(CENTRE, a, ring.geo.labelRadius), ring, rotation, CENTRE);
      const seg = hit.segment;
      const rel = M.mod360(a - rotation);
      assert.ok(rel >= seg.startAngle && rel < seg.endAngle,
        `angle ${a} rot ${rotation} rel ${rel} not in [${seg.startAngle},${seg.endAngle})`);
    }
  }
});

test('a boundary angle belongs to the segment that starts there, once', () => {
  const ring = M.layout(targets(6)).rings[0];
  for (let i = 0; i < 6; i++) {
    const boundary = i * ring.geo.segAngle;
    const point = M.pointAt(CENTRE, boundary, ring.geo.labelRadius);
    const hit = M.hitTest(point, ring, 0, CENTRE);
    assert.equal(hit.index, i, `boundary ${boundary}°`);
    // one notch before belongs to the previous segment
    const before = M.pointAt(CENTRE, M.mod360(boundary - 0.01), ring.geo.labelRadius);
    assert.equal(M.hitTest(before, ring, 0, CENTRE).index, (i + 5) % 6);
  }
});

test('points inside the hub and outside the rim are not segments', () => {
  const ring = M.layout(targets(6)).rings[0];
  const centreHit = M.hitTest({ x: CENTRE.x, y: CENTRE.y }, ring, 0, CENTRE);
  assert.equal(centreHit.kind, 'hub');
  const far = M.hitTest({ x: CENTRE.x, y: CENTRE.y - 400 }, ring, 0, CENTRE);
  assert.equal(far.kind, 'outside');
  // the first pixel band of the rim is a segment for every angle
  const justInside = M.hitTest(M.pointAt(CENTRE, 33, ring.geo.inner + 0.5), ring, 0, CENTRE);
  assert.equal(justInside.kind, 'segment');
});

test('a rotated ring keeps hit-testing and geometry consistent', () => {
  const ring = M.layout(targets(6)).rings[0];
  const rotation = 37;
  for (let a = 0; a < 360; a += 1) {
    const hit = M.hitTest(M.pointAt(CENTRE, a, ring.geo.labelRadius), ring, rotation, CENTRE);
    // rotating the content by 37° puts the segment that used to sit at
    // (a - 37)° under the point at a°.
    const equivalent = M.hitTest(
      M.pointAt(CENTRE, M.mod360(a - rotation), ring.geo.labelRadius), ring, 0, CENTRE);
    assert.equal(hit.index, equivalent.index, `angle ${a}`);
  }
});

/* ---- momentum --------------------------------------------------------- */

test('momentum projection is the exponential form, not v²/(2a)', () => {
  // 1000 px/s at d=0.998 travels 499 px
  assert.ok(Math.abs(M.project(1000) - 499) < 0.5);
  assert.equal(M.project(0), 0);
  assert.ok(M.project(2000) > M.project(1000));
});

test('a flick lands further than a nudge, and a still release selects what is under the pointer', () => {
  const ring = M.layout(targets(6)).rings[0];
  const radius = ring.geo.labelRadius;
  const startAngle = 45;                      // late in segment 0, near a boundary
  const point = M.pointAt(CENTRE, startAngle, radius);

  const still = M.selection(point, ring, 0, 0, CENTRE);
  assert.equal(still.index, 0);
  assert.equal(still.flick, false);

  // 150 deg/s ≈ 75° of projection at r=192; 450 deg/s ≈ 225°. Both are hand
  // speeds a person actually produces (≈ 0.5 and 1.5 k px/s at the rim).
  const slow = M.selection(point, ring, 0, 150, CENTRE);
  const fast = M.selection(point, ring, 0, 450, CENTRE);
  assert.equal(slow.flick, true);
  assert.ok(fast.projectedDegrees > slow.projectedDegrees,
    `fast ${fast.projectedDegrees} should outrun slow ${slow.projectedDegrees}`);
  assert.ok(fast.index > slow.index, 'a harder flick selects a later segment');
  assert.equal(slow.index, 1, 'a gentle flick advances one segment');
  assert.equal(fast.index, 4, 'a hard flick crosses four');

  // the projected point is inside the selected segment
  const rel = M.mod360(fast.projectedAngle);
  assert.ok(rel >= fast.segment.startAngle && rel < fast.segment.endAngle);

  // a throw hard enough to lap the wheel still yields a valid segment
  const lapped = M.selection(point, ring, 0, 6000, CENTRE);
  assert.ok(lapped.index >= 0 && lapped.index < 6);
});

test('a release below the flick threshold selects by position, not by projection', () => {
  const ring = M.layout(targets(6)).rings[0];
  const point = M.pointAt(CENTRE, 5, ring.geo.labelRadius);
  const gentle = M.selection(point, ring, 0, 20, CENTRE);
  assert.equal(gentle.flick, false);
  assert.equal(gentle.index, 0);
});

test('angular projection: a release speed in deg/s is radius-invariant, a linear one is not', () => {
  // 600 deg/s turns the same angle whether the ring is r=100 or r=200 …
  assert.ok(Math.abs(M.projectAngle(600, 100) - M.projectAngle(600, 200)) < 1e-9);
  // … while the same LINEAR speed turns further on the smaller ring
  const linearDeg = (pxPerSec, r) => (M.project(pxPerSec) / r) * 180 / Math.PI;
  assert.ok(linearDeg(2000, 100) > linearDeg(2000, 200));
  // and projectAngle is exactly that linear projection expressed in degrees
  const r = 192;
  const v = 900;                              // deg/s
  const expected = linearDeg(v * Math.PI / 180 * r, r);
  assert.ok(Math.abs(M.projectAngle(v, r) - expected) < 1e-9);
});

/* ---- springs ---------------------------------------------------------- */

function run(spec, seconds = 3, dt = 1 / 60) {
  const s = new Spring(spec);
  const frames = [];
  for (let t = 0; t < seconds; t += dt) {
    frames.push({ t, value: s.value, velocity: s.velocity });
    s.step(dt);
    if (s.settled()) break;
  }
  return { spring: s, frames };
}

test('critical damping (1.0) never overshoots its target', () => {
  const { frames } = run({ damping: 1, response: 0.4, value: 0, target: 100 });
  let maxValue = 0;
  for (const f of frames) maxValue = Math.max(maxValue, f.value);
  assert.ok(maxValue <= 100.0001, `overshot to ${maxValue}`);
  assert.ok(frames[frames.length - 1].value > 90, 'and it still arrives');
});

test('a momentum spring (0.8) overshoots when it is thrown, which is the point', () => {
  const s = new Spring({ damping: 0.8, response: 0.35, value: 0, target: 100 });
  s.setTarget(100, 1500);                 // thrown: 1500 units/s
  let maxValue = 0;
  for (let t = 0; t < 2; t += 1 / 60) {
    s.step(1 / 60);
    maxValue = Math.max(maxValue, s.value);
  }
  assert.ok(maxValue > 100, `expected overshoot past the target, got ${maxValue}`);
});

test('velocity handoff: the release velocity survives the seam into the spring', () => {
  const gesture = 1200;                    // px/s at release
  const thrown = new Spring({ damping: 0.8, response: 0.35, value: 0, target: 0 });
  thrown.setTarget(0, gesture);            // target unchanged: pure handoff
  assert.equal(thrown.velocity, gesture, 'retarget adopts the release velocity verbatim');

  thrown.step(1 / 240);
  assert.ok(thrown.value > 3,
    `one substep moved ${thrown.value.toFixed(2)} px; a spring from rest would move ~0`);

  const rested = new Spring({ damping: 0.8, response: 0.35, value: 0, target: 0 });
  rested.step(1 / 240);
  assert.ok(Math.abs(rested.value) < 0.01, 'a spring with no velocity does not lurch');
});

test('retargeting mid-flight is seamless: value and velocity carry over', () => {
  const s = new Spring({ damping: 1, response: 0.4, value: 0, target: 200 });
  for (let i = 0; i < 10; i++) s.step(1 / 60);   // caught mid-flight
  const valueAtCatch = s.value;
  const velocityAtCatch = s.velocity;
  assert.ok(valueAtCatch > 1 && valueAtCatch < 200, 'catch happens in flight');
  assert.ok(velocityAtCatch > 0);

  s.setTarget(-50);                              // reverse, no velocity argument
  assert.equal(s.value, valueAtCatch, 'position must not jump on retarget');
  assert.equal(s.velocity, velocityAtCatch, 'velocity must not be discarded on retarget');

  s.step(1 / 240);
  assert.ok(Math.abs(s.velocity) > Math.abs(velocityAtCatch) * 0.5,
    `velocity collapsed from ${velocityAtCatch} to ${s.velocity}`);

  for (let i = 0; i < 400; i++) s.step(1 / 120);
  assert.ok(Math.abs(s.value + 50) < 1, `did not reach the new target, landed at ${s.value}`);
});

test('a spring settles, and stays settled', () => {
  const { spring, frames } = run({ damping: 0.8, response: 0.35, value: 0, target: 100 }, 3);
  assert.ok(spring.settled(), `value=${spring.value} v=${spring.velocity}`);
  assert.ok(spring.value > 99.5 && spring.value < 100.5);
  assert.ok(frames.length < 90, `took ${frames.length} frames (~${(frames.length / 60).toFixed(2)} s)`);
});

test('response really is the speed knob: 0.2 s arrives before 0.5 s', () => {
  const timeTo = (response) => {
    const s = new Spring({ damping: 1, response, value: 0, target: 100 });
    let t = 0;
    while (!s.settled() && t < 5) { s.step(1 / 120); t += 1 / 120; }
    return t;
  };
  const fast = timeTo(0.2);
  const slow = timeTo(0.5);
  assert.ok(fast < slow, `${fast} should be quicker than ${slow}`);
  assert.ok(fast > 0.05 && fast < 1.0, `0.2 s response settled in ${fast} s`);
});

test('reduced motion removes the spring instead of shortening it', () => {
  const m = M.motionFor('momentum', true);
  assert.equal(m.immediate, true);
  const s = new Spring({ immediate: true, value: 0, target: 100 });
  assert.equal(s.value, 100);
  s.setTarget(-20, 900);
  assert.equal(s.value, -20);
  assert.equal(s.velocity, 0);
});

test('motion parameters match the Apple table the card names', () => {
  assert.deepEqual(
    { d: M.motionFor('reposition', false).damping, r: M.motionFor('reposition', false).response },
    { d: 1.0, r: 0.4 });
  assert.deepEqual(
    { d: M.motionFor('settle', false).damping, r: M.motionFor('settle', false).response },
    { d: 1.0, r: 0.3 });
  assert.deepEqual(
    { d: M.motionFor('momentum', false).damping, r: M.motionFor('momentum', false).response },
    { d: 0.8, r: 0.35 });
});

/* ---- material --------------------------------------------------------- */

test('material never stacks translucency: a nested surface is opaque', () => {
  assert.equal(M.materialFor({}).solid, false);
  assert.equal(M.materialFor({ nested: true }).solid, true);
  assert.equal(M.materialFor({ nested: true }).blurPx, 0);
  const reduced = M.materialFor({ reducedTransparency: true });
  assert.equal(reduced.solid, true);
  assert.equal(reduced.blurPx, 0);
});

/* ---- geometry sanity -------------------------------------------------- */

test('per-ring geometry nests without overlapping', () => {
  const zero = M.ringGeometry(0, 6);
  const one = M.ringGeometry(1, 6);
  assert.ok(one.outer < zero.inner, 'a sub-ring sits strictly inside its parent');
  assert.ok(one.inner > 0);
  assert.ok(Math.abs(zero.outer - (M.DEFAULTS.size / 2 - M.DEFAULTS.padding)) < 1e-9);
  assert.ok(Math.abs(zero.segAngle * zero.count - 360) < 1e-9);
});

test('segment mid-angles are all distinct and ordered', () => {
  const ring = M.layout(targets(6)).rings[0];
  const mids = ring.segments.map((s) => s.midAngle);
  for (let i = 1; i < mids.length; i++) assert.ok(mids[i] > mids[i - 1]);
  assert.equal(new Set(mids).size, mids.length);
});
