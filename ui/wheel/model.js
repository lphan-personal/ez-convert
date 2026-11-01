/* Wheel model — pure geometry, layout and motion decisions for the overlay.
   No DOM, no timers, no globals beyond `module.exports` under node.

   Contract (ARCHITECTURE.md §10.4, SPEC.md §2):
     - hard cap 6 segments per ring, never smaller
     - overflow behind a `more` segment that expands into a sub-ring
     - nothing smaller than the legibility floor (11 pt) — a layout that would
       violate it is REJECTED and its remainder is reported, never shrunk
     - only valid targets appear (the caller passes the filtered list)
     - Apple motion: damping 1.0 / response 0.3-0.4, momentum springs 0.8,
       velocity handoff, momentum projection for segment selection

   Angles are degrees, 0 = 12 o'clock, increasing clockwise. */
(function (root) {
  'use strict';

  /* ---- constants ------------------------------------------------------- */

  var DEFAULTS = {
    size: 520,          // window edge, matches the S1 spike's 520x520 overlay
    padding: 20,        // dead margin between the window edge and the rim
    gap: 16,            // gap between a ring and the ring inside it
    maxSegments: 6,     // SPEC.md §2: hard cap, never smaller
    floorPt: 11,        // legibility floor for a segment label
    minHubRadius: 56,   // the hub must stay big enough to say what is happening
    dpi: 96,
    fontCapPx: 22,      // no label needs to be bigger than this at 520 px
    labelChars: 4,      // longest label we budget for ("WEBP", "DOCX")
    deceleration: 0.998 // Apple's projection deceleration (skill principle 6)
  };

  var FLOOR_PX = DEFAULTS.floorPt * DEFAULTS.dpi / 72; // 14.6667

  /* ---- small maths ----------------------------------------------------- */

  function clamp(v, lo, hi) { return v < lo ? lo : v > hi ? hi : v; }
  function mod360(a) { return ((a % 360) + 360) % 360; }
  function rad(d) { return d * Math.PI / 180; }
  function deg(r) { return r * 180 / Math.PI; }

  /** Clockwise angle of a point around a centre, 0 = 12 o'clock. */
  function angleOf(point, centre) {
    var dx = point.x - centre.x;
    var dy = point.y - centre.y;
    return mod360(deg(Math.atan2(dx, -dy)));
  }

  function distanceOf(point, centre) {
    var dx = point.x - centre.x;
    var dy = point.y - centre.y;
    return Math.sqrt(dx * dx + dy * dy);
  }

  /** Polarity helper for rendering: point at `angle`, `radius` from centre. */
  function pointAt(centre, angleDeg, radius) {
    var a = rad(angleDeg);
    return { x: centre.x + Math.sin(a) * radius, y: centre.y - Math.cos(a) * radius };
  }

  /* ---- typography ------------------------------------------------------ */

  /**
   * Letter-spacing as a function of size — skill principle 15. Large labels
   * tighten, small labels open up. A single fixed tracking value is wrong
   * somewhere by construction.
   */
  function trackingFor(fontPx) {
    if (fontPx >= 20) return '-0.02em';
    if (fontPx >= 16) return '-0.01em';
    if (fontPx >= 14) return '0';
    return '0.01em';
  }

  /* ---- ring geometry --------------------------------------------------- */

  /**
   * Geometry of the ring at `level` for `count` entries. Level 0 is the outer
   * rim; each level sits inside the previous one, separated by `gap`.
   */
  function ringGeometry(level, count, opts) {
    var o = opts || DEFAULTS;
    var outer = o.size / 2 - o.padding;
    var thickness = 96;
    for (var i = 0; i < level; i++) {
      outer = outer - thickness - o.gap;
      thickness = Math.round(thickness * 0.62);
    }
    var inner = outer - thickness;
    var labelRadius = (outer + inner) / 2;
    var segAngle = 360 / Math.max(1, count);
    var arc = 2 * Math.PI * labelRadius * (segAngle / 360);
    // The label must fit both radially and along the arc, with padding.
    var radialFit = thickness * 0.7;
    var arcFit = arc * 0.8 / (o.labelChars * 0.62);
    var fontPx = Math.min(o.fontCapPx, radialFit, arcFit);
    return {
      level: level,
      count: count,
      outer: outer,
      inner: inner,
      thickness: thickness,
      labelRadius: labelRadius,
      segAngle: segAngle,
      arc: arc,
      fontPx: fontPx,
      tracking: trackingFor(fontPx),
      // Legible means both: the label clears the 11 pt floor AND the hole left
      // in the middle is still a hub. A ring can be wide enough for a label and
      // still be unusable because it swallowed the centre.
      legible: inner >= o.minHubRadius && fontPx >= FLOOR_PX
    };
  }

  /* ---- layout ---------------------------------------------------------- */

  function toSegment(entry, index, geo) {
    var mid = index * geo.segAngle + geo.segAngle / 2;
    return {
      index: index,
      kind: entry.kind || 'target',
      id: entry.id || entry.format || ('seg-' + index),
      label: entry.label || String(entry.format || entry.id || '').toUpperCase(),
      format: entry.format || null,
      startAngle: index * geo.segAngle,
      endAngle: (index + 1) * geo.segAngle,
      midAngle: mid
    };
  }

  /**
   * Lay out `targets` into rings.
   *
   * Returns:
   *   { ok, rings:[{ level, geo, segments }], unplaced:[...], reason }
   *
   * `unplaced` is never silent: if the next ring cannot meet the legibility
   * floor, the entries that do not fit are returned with a reason instead of
   * being dropped or rendered illegibly.
   */
  function layout(targets, opts) {
    var o = Object.assign({}, DEFAULTS, opts || {});
    var remaining = (targets || []).slice();
    var rings = [];
    if (!remaining.length) {
      return { ok: false, rings: [], unplaced: [], reason: 'no-targets' };
    }

    var level = 0;
    var guard = 0;
    while (remaining.length && guard++ < 8) {
      var count = Math.min(o.maxSegments, remaining.length);
      var geo = ringGeometry(level, count, o);
      if (!geo.legible) {
        return {
          ok: rings.length > 0,
          rings: rings,
          unplaced: remaining,
          reason: 'legibility-floor'
        };
      }

      var overflow = remaining.length > o.maxSegments;
      if (overflow) {
        var childLevel = level + 1;
        var childCount = Math.min(o.maxSegments, remaining.length - (o.maxSegments - 1));
        if (!ringGeometry(childLevel, childCount, o).legible) {
          // No legal place to put the rest: fill this ring and report the rest.
          var fits = remaining.splice(0, o.maxSegments);
          rings.push({
            level: level,
            geo: ringGeometry(level, o.maxSegments, o),
            segments: fits.map(function (t, i) { return toSegment(t, i, geo); })
          });
          return {
            ok: true,
            rings: rings,
            unplaced: remaining,
            reason: 'legibility-floor'
          };
        }
      }

      var take = overflow ? o.maxSegments - 1 : remaining.length;
      var real = remaining.splice(0, take);
      var segments = real.map(function (t, i) { return toSegment(t, i, geo); });
      if (overflow) {
        segments.push({
          index: segments.length,
          kind: 'more',
          id: 'more',
          label: 'More',
          format: null,
          startAngle: segments.length * geo.segAngle,
          endAngle: (segments.length + 1) * geo.segAngle,
          midAngle: segments.length * geo.segAngle + geo.segAngle / 2
        });
      }
      rings.push({ level: level, geo: geo, segments: segments });
      level++;
      if (level > 2) break;
    }

    return {
      ok: rings.length > 0,
      rings: rings,
      unplaced: remaining,
      reason: remaining.length ? 'legibility-floor' : null
    };
  }

  /* ---- hit testing ----------------------------------------------------- */

  /**
   * Which entry of `ring` is under `point`, given the ring's rotation.
   * `rotation` rotates the content clockwise around the centre.
   *
   * Returns { kind:'segment', index, segment } or { kind:'outside'|'hub' }.
   * The tie at a boundary belongs to the segment that starts there, which is
   * why the arithmetic is a floor over a half-open interval.
   */
  function hitTest(point, ring, rotation, centre) {
    var c = centre || { x: 0, y: 0 };
    var r = distanceOf(point, c);
    if (!ring) return { kind: 'outside', radius: r };
    if (r > ring.geo.outer || r < ring.geo.inner) {
      return { kind: r < ring.geo.inner ? 'hub' : 'outside', radius: r };
    }
    var a = mod360(angleOf(point, c) - (rotation || 0));
    var index = Math.floor(a / ring.geo.segAngle);
    if (index >= ring.segments.length) index = ring.segments.length - 1;
    return {
      kind: 'segment',
      index: index,
      segment: ring.segments[index],
      angleDeg: angleOf(point, c),
      relativeDeg: a,
      radius: r
    };
  }

  /** The `more` entry of a ring, if it has one. */
  function moreSegment(ring) {
    for (var i = 0; i < ring.segments.length; i++) {
      if (ring.segments[i].kind === 'more') return ring.segments[i];
    }
    return null;
  }

  /* ---- momentum -------------------------------------------------------- */

  /**
   * Apple's momentum projection (skill principle 6): exponential deceleration,
   * NOT the textbook v^2/(2a). Returns the distance travelled, in the same
   * units as `velocity` * seconds.
   */
  function project(velocity, deceleration) {
    var d = deceleration === undefined ? DEFAULTS.deceleration : deceleration;
    return (velocity / 1000) * d / (1 - d);
  }

  /** Project an angular release along the ring at `radius`, in degrees. */
  function projectAngle(velocityDegPerSec, radius, deceleration) {
    var pxPerSec = rad(velocityDegPerSec) * radius;
    var travelled = project(pxPerSec, deceleration);
    return deg(travelled / radius);
  }

  /**
   * Segment selection on release: project the release point forward along the
   * arc, then snap to the segment that contains the projected point.
   */
  function selectionOnRelease(point, ring, rotation, velocityDegPerSec, centre) {
    var c = centre || { x: 0, y: 0 };
    var start = angleOf(point, c);
    var geo = ring.geo;
    var projected = start + projectAngle(velocityDegPerSec || 0, geo.labelRadius);
    var a = mod360(projected - (rotation || 0));
    var index = Math.floor(a / geo.segAngle);
    if (index >= ring.segments.length) index = ring.segments.length - 1;
    return {
      index: index,
      segment: ring.segments[index],
      startAngle: start,
      projectedAngle: mod360(projected),
      projectedDegrees: projectAngle(velocityDegPerSec || 0, geo.labelRadius)
    };
  }

  /*
   * Momentum projection must not turn a nudge into a jump. A release under the
   * gesture detection threshold selects what is under the pointer; above it,
   * the projection decides. The threshold is expressed as a distance so the
   * rule is testable in pixels rather than in degrees.
   */
  var FLICK_MIN_PX_PER_SEC = 120;

  function selection(point, ring, rotation, velocityDegPerSec, centre) {
    var geo = ring.geo;
    var pxPerSec = Math.abs(rad(velocityDegPerSec || 0) * geo.labelRadius);
    if (pxPerSec < FLICK_MIN_PX_PER_SEC) {
      var direct = hitTest(point, ring, rotation, centre);
      return {
        index: direct.kind === 'segment' ? direct.index : -1,
        segment: direct.segment || null,
        projectedDegrees: 0,
        flick: false
      };
    }
    var s = selectionOnRelease(point, ring, rotation, velocityDegPerSec, centre);
    s.flick = true;
    return s;
  }

  /* ---- motion parameters ----------------------------------------------- */

  /**
   * Spring parameters by interaction kind (skill principle 4 table).
   * Reduced motion removes the spring entirely rather than shortening it.
   */
  function motionFor(kind, reducedMotion) {
    if (reducedMotion) {
      return { immediate: true, damping: 1, response: 0, velocity: 0 };
    }
    switch (kind) {
      case 'momentum':   // drag release carrying hand velocity
        return { immediate: false, damping: 0.8, response: 0.35 };
      case 'settle':     // segment confirm / label emphasis
        return { immediate: false, damping: 1.0, response: 0.3 };
      case 'reposition': // wheel arrives at the cursor
      default:
        return { immediate: false, damping: 1.0, response: 0.4 };
    }
  }

  /**
   * Material weight for a surface. A ring drawn inside another surface may not
   * stack translucency (skill principle 12): once the sub-ring is open the hub
   * becomes opaque, and reduced-transparency makes every surface solid.
   */
  function materialFor(opts) {
    var o = opts || {};
    if (o.reducedTransparency) {
      return { background: 'rgba(28,28,30,1)', blurPx: 0, solid: true };
    }
    if (o.nested) {
      return { background: 'rgba(28,28,30,0.92)', blurPx: 0, solid: true };
    }
    return { background: 'rgba(28,28,30,0.55)', blurPx: 30, solid: false };
  }

  /* ---- export ---------------------------------------------------------- */

  var API = {
    DEFAULTS: DEFAULTS,
    FLOOR_PX: FLOOR_PX,
    FLICK_MIN_PX_PER_SEC: FLICK_MIN_PX_PER_SEC,
    clamp: clamp,
    mod360: mod360,
    angleOf: angleOf,
    distanceOf: distanceOf,
    pointAt: pointAt,
    trackingFor: trackingFor,
    ringGeometry: ringGeometry,
    layout: layout,
    hitTest: hitTest,
    moreSegment: moreSegment,
    project: project,
    projectAngle: projectAngle,
    selectionOnRelease: selectionOnRelease,
    selection: selection,
    motionFor: motionFor,
    materialFor: materialFor
  };

  if (typeof module !== 'undefined' && module.exports) module.exports = API;
  root.WheelModel = API;
})(typeof globalThis !== 'undefined' ? globalThis : this);
