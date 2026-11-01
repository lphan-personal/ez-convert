/* Wheel overlay — rendering and interaction.

   The DOM never computes geometry: every angle, radius, font size and hit test
   comes from model.js. Motion comes from spring.js only (no CSS transitions on
   anything the pointer drives — principle 3). */
(function () {
  'use strict';

  var M = window.WheelModel;
  var S = window.WheelSpring;

  /* ---- what a detected type offers (stands in for the Rust matrix) ----- */

  var CATALOGUE = {
    image: [
      { format: 'png', label: 'PNG', sub: 'image' },
      { format: 'jpg', label: 'JPG', sub: 'image' },
      { format: 'webp', label: 'WEBP', sub: 'image' },
      { format: 'avif', label: 'AVIF', sub: 'image' },
      { format: 'heic', label: 'HEIC', sub: 'image' },
      { format: 'tiff', label: 'TIFF', sub: 'image' },
      { format: 'gif', label: 'GIF', sub: 'image' }
    ],
    video: [
      { format: 'mp4', label: 'MP4', sub: 'video' },
      { format: 'mkv', label: 'MKV', sub: 'video' },
      { format: 'mov', label: 'MOV', sub: 'video' },
      { format: 'webm', label: 'WEBM', sub: 'video' },
      { format: 'gif', label: 'GIF', sub: 'video' },
      { format: 'mp3', label: 'MP3', sub: 'audio' }
    ],
    document: [
      { format: 'pdf', label: 'PDF', sub: 'doc' },
      { format: 'txt', label: 'TXT', sub: 'doc' },
      { format: 'md', label: 'MD', sub: 'doc' },
      { format: 'html', label: 'HTML', sub: 'doc' },
      { format: 'csv', label: 'CSV', sub: 'doc' },
      { format: 'rtf', label: 'RTF', sub: 'doc' },
      { format: 'docx', label: 'DOCX', sub: 'doc' },
      { format: 'xlsx', label: 'XLSX', sub: 'doc' }
    ],
    audio: [
      { format: 'mp3', label: 'MP3', sub: 'audio' },
      { format: 'wav', label: 'WAV', sub: 'audio' },
      { format: 'flac', label: 'FLAC', sub: 'audio' },
      { format: 'm4a', label: 'M4A', sub: 'audio' },
      { format: 'ogg', label: 'OGG', sub: 'audio' },
      { format: 'opus', label: 'OPUS', sub: 'audio' }
    ],
    unknown: []
  };

  var KIND_LABEL = {
    image: 'Image detected', video: 'Video detected',
    document: 'Document detected', audio: 'Audio detected',
    unknown: 'Unrecognised file'
  };

  /* ---- elements ------------------------------------------------------- */

  var wheelEl = document.getElementById('wheel');
  var surfaceEl = document.getElementById('surface');
  var hubEl = document.getElementById('hub');
  var hubKicker = document.getElementById('hub-kicker');
  var hubValue = document.getElementById('hub-value');
  var svg = document.getElementById('ring');
  var labelsEl = document.getElementById('labels');
  var logEl = document.getElementById('events');

  var CENTRE = { x: M.DEFAULTS.size / 2, y: M.DEFAULTS.size / 2 };

  /* ---- state ---------------------------------------------------------- */

  var state = {
    kind: 'image',
    layout: null,
    rotation: 0,        // degrees, level-0 content rotation
    selected: 0,
    openLevel: null,    // null | 1
    phase: 'choosing',  // choosing | confirming | converting | done | dismissed
    pressIndex: -1,
    reducedMotion: false,
    reducedTransparency: false
  };

  var events = [];
  var rot = new S.Spring({ damping: 1, response: 0.4, value: 0 });
  var hi = [];      // per-segment highlight opacity springs
  var sc = [];      // per-label scale springs
  var subHi = [];

  function now() { return (window.performance || Date).now(); }

  function log(name, detail) {
    var entry = { t: Math.round(now()), name: name, detail: detail || null };
    events.push(entry);
    var li = document.createElement('li');
    li.textContent = entry.t + ' ms  ' + name + (detail ? '  ' + JSON.stringify(detail) : '');
    logEl.insertBefore(li, logEl.firstChild);
    while (logEl.children.length > 8) logEl.removeChild(logEl.lastChild);
    return entry;
  }

  /* ---- media queries -------------------------------------------------- */

  function mq(query) {
    return window.matchMedia ? window.matchMedia(query).matches : false;
  }

  function readPreferences() {
    state.reducedMotion = mq('(prefers-reduced-motion: reduce)');
    state.reducedTransparency = mq('(prefers-reduced-transparency: reduce)');
  }

  function motion(kind) { return M.motionFor(kind, state.reducedMotion); }

  /* ---- material ------------------------------------------------------- */

  function applyMaterial() {
    var outer = M.materialFor({ reducedTransparency: state.reducedTransparency });
    var hub = M.materialFor({
      reducedTransparency: state.reducedTransparency,
      nested: state.openLevel !== null
    });
    surfaceEl.style.setProperty('--material-bg', outer.background);
    surfaceEl.style.setProperty('--material-blur', outer.blurPx + 'px');
    surfaceEl.style.backdropFilter = outer.blurPx ? 'blur(' + outer.blurPx + 'px) saturate(180%)' : 'none';
    hubEl.style.backdropFilter = hub.blurPx ? 'blur(' + hub.blurPx + 'px) saturate(160%)' : 'none';
    hubEl.style.background = hub.background;
    wheelEl.classList.toggle('ring-open', state.openLevel !== null);
  }

  /* ---- geometry to SVG ------------------------------------------------ */

  function wedgePath(geo, startAngle, endAngle) {
    var large = (endAngle - startAngle) > 180 ? 1 : 0;
    var o1 = M.pointAt(CENTRE, startAngle, geo.outer);
    var o2 = M.pointAt(CENTRE, endAngle, geo.outer);
    var i2 = M.pointAt(CENTRE, endAngle, geo.inner);
    var i1 = M.pointAt(CENTRE, startAngle, geo.inner);
    return 'M' + o1.x.toFixed(2) + ' ' + o1.y.toFixed(2) +
      'A' + geo.outer + ' ' + geo.outer + ' 0 ' + large + ' 1 ' + o2.x.toFixed(2) + ' ' + o2.y.toFixed(2) +
      'L' + i2.x.toFixed(2) + ' ' + i2.y.toFixed(2) +
      'A' + geo.inner + ' ' + geo.inner + ' 0 ' + large + ' 0 ' + i1.x.toFixed(2) + ' ' + i1.y.toFixed(2) +
      'Z';
  }

  function svgEl(name, attrs) {
    var node = document.createElementNS('http://www.w3.org/2000/svg', name);
    Object.keys(attrs || {}).forEach(function (k) { node.setAttribute(k, attrs[k]); });
    return node;
  }

  /**
   * Draw one ring. `group` carries the rotation for level 0 so the whole ring
   * moves as one composited transform (transform only — principle 11).
   */
  function drawRing(ring, opts) {
    var g = svgEl('g', {});
    var geo = ring.geo;
    var base = [], hl = [];
    ring.segments.forEach(function (seg) {
      var d = wedgePath(geo, seg.startAngle, seg.endAngle);
      var basePath = svgEl('path', { d: d, class: 'wedge' });
      var hlPath = svgEl('path', { d: d, class: 'wedge-highlight' });
      basePath.dataset.index = seg.index;
      hlPath.dataset.index = seg.index;
      g.appendChild(basePath);
      g.appendChild(hlPath);
      base.push(basePath);
      hl.push(hlPath);
    });
    // dividers at each boundary
    ring.segments.forEach(function (seg) {
      var a = seg.startAngle;
      var p1 = M.pointAt(CENTRE, a, geo.inner);
      var p2 = M.pointAt(CENTRE, a, geo.outer);
      g.appendChild(svgEl('line', {
        x1: p1.x, y1: p1.y, x2: p2.x, y2: p2.y, class: 'divider'
      }));
    });
    g.appendChild(svgEl('circle', {
      cx: CENTRE.x, cy: CENTRE.y, r: geo.outer - 0.5, class: 'ring-edge'
    }));
    g.appendChild(svgEl('circle', {
      cx: CENTRE.x, cy: CENTRE.y, r: geo.inner + 0.5, class: 'ring-edge'
    }));
    return { group: g, base: base, highlight: hl };
  }

  /* ---- render --------------------------------------------------------- */

  var ringNodes = null;   // { level0: {group, base, highlight}, level1: ... }

  function renderLayout() {
    var layout = M.layout(CATALOGUE[state.kind] || []);
    state.layout = layout;
    // innerHTML is used only to CLEAR these two containers. Every node put back
    // is built with createElement/createElementNS from validated format ids —
    // no string from a file, a locale or the DOM ever reaches an HTML parse.
    svg.innerHTML = '';
    labelsEl.innerHTML = '';
    ringNodes = { level0: null, level1: null };
    hi = [];
    sc = [];
    subHi = [];
    state.openLevel = null;

    if (!layout.ok) {
      hubKicker.textContent = KIND_LABEL[state.kind] || 'Nothing to convert';
      hubValue.textContent = 'No target formats';
      state.selected = -1;
      renderLabelsFor(null);
      applyMaterial();
      log('layout:none', { kind: state.kind });
      return;
    }

    var outer = layout.rings[0];
    var node = drawRing(outer, { rotated: true });
    svg.appendChild(node.group);
    ringNodes.level0 = node;
    outer.group = node.group;

    outer.segments.forEach(function () {
      hi.push(new S.Spring({ damping: 1, response: 0.3, value: 0 }));
      sc.push(new S.Spring({ damping: 1, response: 0.3, value: 1 }));
    });

    state.selected = layout.rings[0].segments.length ? 0 : -1;
    renderLabelsFor(outer);
    updateHub();
    applyMaterial();
    if (layout.unplaced.length) {
      log('layout:unplaced', {
        count: layout.unplaced.length,
        reason: layout.reason,
        formats: layout.unplaced.map(function (t) { return t.format; })
      });
    }
    log('layout', {
      kind: state.kind,
      rings: layout.rings.length,
      segments: layout.rings[0].segments.map(function (s) { return s.kind === 'more' ? 'more' : s.format; })
    });
  }

  /** Build the label nodes for a ring (level 0 or the open sub-ring). */
  function renderLabelsFor(ring) {
    labelsEl.innerHTML = '';
    if (!ring) return;
    ring.segments.forEach(function (seg) {
      var el = document.createElement('div');
      el.className = 'label' + (seg.kind === 'more' ? ' more' : '');
      el.dataset.index = seg.index;
      el.dataset.format = seg.format || 'more';
      var name = document.createElement('span');
      name.className = 'name';
      name.textContent = seg.label;
      name.style.fontSize = ring.geo.fontPx.toFixed(2) + 'px';
      // size-dependent tracking (principle 15) — never one fixed value
      name.style.letterSpacing = M.trackingFor(ring.geo.fontPx);
      el.appendChild(name);
      if (seg.kind !== 'more') {
        var sub = document.createElement('span');
        sub.className = 'sub';
        sub.textContent = seg.sub || '';
        el.appendChild(sub);
      }
      labelsEl.appendChild(el);
    });
  }

  function renderSubRing() {
    var layout = state.layout;
    if (!layout || layout.rings.length < 2) return false;
    var child = layout.rings[1];
    if (M.ringGeometry(child.geo.level, child.segments.length).legible === false) return false;
    // The sub-ring is a second surface inside the hub: material rule applies.
    var node = drawRing(child, { rotated: false });
    node.group.setAttribute('data-sub', '1');
    svg.appendChild(node.group);
    ringNodes.level1 = node;
    state.openLevel = 1;
    child.segments.forEach(function () {
      var s = new S.Spring({ damping: 1, response: 0.3, value: 0 });
      subHi.push(s);
      if (ringNodes.level1.highlight[subHi.length - 1]) {
        ringNodes.level1.highlight[subHi.length - 1].style.opacity = '0';
      }
    });
    renderLabelsFor(child);
    hubKicker.textContent = 'More formats';
    hubValue.textContent = child.segments.length + ' in this ring';
    applyMaterial();
    log('subring:open', { segments: child.segments.map(function (s) { return s.format; }) });
    return true;
  }

  function updateHub() {
    if (!state.layout || !state.layout.ok || state.selected < 0) {
      hubKicker.textContent = KIND_LABEL[state.kind] || 'Nothing to convert';
      hubValue.textContent = 'No target formats';
      return;
    }
    var ring = state.layout.rings[state.openLevel || 0];
    var seg = ring.segments[state.selected];
    hubKicker.textContent = KIND_LABEL[state.kind] || '';
    hubValue.textContent = seg ? (seg.kind === 'more' ? 'More formats' : 'Convert to ' + seg.label) : '';
  }

  /* ---- per-frame ------------------------------------------------------ */

  var last = 0;

  function frame(ts) {
    var t = ts || now();
    var dt = last ? Math.min(0.05, (t - last) / 1000) : 1 / 60;
    last = t;
    readPreferences();
    rot.step(dt);

    if (ringNodes && ringNodes.level0) {
      ringNodes.level0.group.setAttribute('transform',
        'rotate(' + rot.value.toFixed(3) + ' ' + CENTRE.x + ' ' + CENTRE.y + ')');
    }

    // highlight + scale springs
    for (var i = 0; i < hi.length; i++) {
      // pressing counts as selecting: feedback starts on pointer-down, not on
      // release (principle 1)
      var selected = (state.selected === i || state.pressIndex === i) ? 1 : 0;
      if (hi[i].target !== selected) hi[i].setTarget(selected);
      hi[i].step(dt);
      var path = ringNodes.level0.highlight[i];
      if (path) path.style.opacity = hi[i].value.toFixed(3);

      var pressed = state.pressIndex === i && state.phase === 'choosing';
      var target = pressed ? 0.97 : (selected ? 1.05 : 1);
      if (Math.abs(sc[i].target - target) > 1e-4) sc[i].setTarget(target);
      sc[i].step(dt);
    }

    positionLabels(dt);
    positionSubLabels(dt);
    requestAnimationFrame(frame);
  }

  function positionLabels() {
    if (!state.layout || !state.layout.ok) return;
    var ring = state.layout.rings[0];
    var geo = ring.geo;
    var nodes = labelsEl.children;
    for (var i = 0; i < nodes.length; i++) {
      var seg = ring.segments[i];
      if (!seg) continue;
      var angle = seg.midAngle + rot.value;
      var p = M.pointAt(CENTRE, angle, geo.labelRadius);
      var dx = p.x - CENTRE.x;
      var dy = p.y - CENTRE.y;
      var s = sc[i] ? sc[i].value : 1;
      nodes[i].style.transform =
        'translate(-50%,-50%) translate(' + dx.toFixed(2) + 'px,' + dy.toFixed(2) + 'px) scale(' + s.toFixed(4) + ')';
      nodes[i].classList.toggle('selected', state.selected === i && state.openLevel === null);
      nodes[i].style.fontSize = geo.fontPx.toFixed(2) + 'px';
    }
  }

  function positionSubLabels() {
    if (state.openLevel === null || !state.layout || state.layout.rings.length < 2) return;
    var ring = state.layout.rings[1];
    var geo = ring.geo;
    // only the sub-ring labels are in the DOM when it is open
    var nodes = labelsEl.children;
    if (nodes.length !== ring.segments.length) return;
    for (var i = 0; i < nodes.length; i++) {
      var seg = ring.segments[i];
      var p = M.pointAt(CENTRE, seg.midAngle, geo.labelRadius);
      var s = subHi[i] ? subHi[i].value : 1;
      nodes[i].style.transform =
        'translate(-50%,-50%) translate(' + (p.x - CENTRE.x).toFixed(2) + 'px,' +
        (p.y - CENTRE.y).toFixed(2) + 'px) scale(' + s.toFixed(4) + ')';
      nodes[i].style.fontSize = geo.fontPx.toFixed(2) + 'px';
    }
  }

  /* ---- selection ------------------------------------------------------ */

  function ringAt() {
    return state.layout && state.layout.ok
      ? state.layout.rings[state.openLevel || 0]
      : null;
  }

  function rotationFor(ring) {
    return ring && ring.geo.level === 0 ? rot.value : 0;
  }

  function hitAt(point) {
    var ring = ringAt();
    if (!ring) return { kind: 'outside' };
    return M.hitTest(point, ring, rotationFor(ring), CENTRE);
  }

  function select(index) {
    if (index < 0) return;
    state.selected = index;
    updateHub();
    log('select', { index: index, format: ringAt().segments[index].format });
  }

  /* ---- commit --------------------------------------------------------- */

  function commit(index) {
    var ring = ringAt();
    if (!ring) return null;
    var seg = ring.segments[index];
    if (!seg) return null;

    if (seg.kind === 'more') {
      if (renderSubRing()) {
        state.selected = 0;
        updateHub();
        return { kind: 'more', opened: true };
      }
      log('more:refused', { reason: 'no legible sub-ring' });
      return { kind: 'more', opened: false };
    }

    state.phase = 'confirming';
    hubKicker.textContent = 'Converting';
    hubValue.textContent = '→ ' + seg.label;
    log('commit', { format: seg.format, label: seg.label });

    // Segment confirms, then the job is queued: 120 ms (ARCHITECTURE §10.3).
    window.setTimeout(function () {
      if (state.phase !== 'confirming') return;
      state.phase = 'converting';
      log('converting', { format: seg.format });
      window.setTimeout(function () {
        if (state.phase !== 'converting') return;
        state.phase = 'done';
        hubKicker.textContent = 'Converted';
        hubValue.textContent = seg.label + ' file is ready';
        log('converted', { format: seg.format });
      }, 120);
    }, 120);
    return { kind: 'target', format: seg.format };
  }

  function dismiss() {
    state.phase = 'dismissed';
    wheelEl.classList.add('dismissed');
    log('dismiss');
  }

  /* ---- pointer -------------------------------------------------------- */

  var drag = null;

  function unwrap(delta) {
    while (delta > 180) delta -= 360;
    while (delta < -180) delta += 360;
    return delta;
  }

  function pointerPoint(ev) {
    var box = wheelEl.getBoundingClientRect();
    return { x: ev.clientX - box.left, y: ev.clientY - box.top };
  }

  function velocityFrom(samples) {
    if (samples.length < 2) return 0;
    var lastSample = samples[samples.length - 1];
    var cutoff = lastSample.t - 100;
    var first = samples[0];
    for (var i = samples.length - 1; i >= 0; i--) {
      if (samples[i].t <= cutoff) { first = samples[i]; break; }
      first = samples[i];
    }
    var dt = (lastSample.t - first.t) / 1000;
    if (dt <= 0.001) return 0;
    return unwrap(lastSample.a - first.a) / dt;
  }

  wheelEl.addEventListener('pointerdown', function (ev) {
    if (state.phase === 'dismissed') return;
    wheelEl.focus({ preventScroll: true });
    var p = pointerPoint(ev);
    var hit = hitAt(p);
    state.pressIndex = hit.kind === 'segment' ? hit.index : -1;
    drag = {
      id: ev.pointerId,
      startPoint: p,
      startAngle: M.angleOf(p, CENTRE),
      startRotation: rot.value,
      samples: [{ t: now(), a: M.angleOf(p, CENTRE) }],
      moved: false
    };
    // Catching a moving ring must stop it exactly where it is on screen and
    // take over from there: no jump to the target, no leftover velocity.
    rot.jumpTo(rot.value);
    try { wheelEl.setPointerCapture(ev.pointerId); } catch (e) { /* pointer capture optional */ }
    ev.preventDefault();
  });

  wheelEl.addEventListener('pointermove', function (ev) {
    if (!drag || ev.pointerId !== drag.id) return;
    var p = pointerPoint(ev);
    var a = M.angleOf(p, CENTRE);
    var geo = ringAt() ? ringAt().geo : { labelRadius: 192 };
    drag.samples.push({ t: now(), a: a });
    if (drag.samples.length > 24) drag.samples.shift();

    var delta = unwrap(a - drag.startAngle);
    var tangential = Math.abs(delta) * Math.PI / 180 * geo.labelRadius;
    if (!drag.moved && tangential > 10) drag.moved = true;

    if (drag.moved) {
      state.rotation = drag.startRotation + delta;
      rot.jumpTo(state.rotation);
      var hit = hitAt(p);
      if (hit.kind === 'segment') state.selected = hit.index;
      updateHub();
    }
  });

  function endDrag(ev, commitTap) {
    if (!drag || ev.pointerId !== drag.id) return;
    var p = pointerPoint(ev);
    var v = velocityFrom(drag.samples);
    var moved = drag.moved;
    var pressIndex = state.pressIndex;
    try { wheelEl.releasePointerCapture(ev.pointerId); } catch (e) { /* ignore */ }
    drag = null;
    state.pressIndex = -1;
    if (state.phase !== 'choosing') return;

    if (moved) {
      // Momentum: project the fling, snap the ring to the nearest segment
      // boundary, hand the release velocity to a damping-0.8 spring.
      var geo = ringAt().geo;
      var projected = rot.value + M.projectAngle(v, geo.labelRadius);
      var target = Math.round(projected / geo.segAngle) * geo.segAngle;
      var m = motion('momentum');
      if (m.immediate) {
        rot.jumpTo(target);
      } else {
        rot.damping = m.damping;
        rot.response = m.response;
        rot.setTarget(target, v);
      }
      var hit = hitAt(p);
      if (hit.kind === 'segment') state.selected = hit.index;
      updateHub();
      log('release:momentum', {
        velocityDegPerSec: Math.round(v),
        projectedDeg: Math.round(projected),
        targetDeg: Math.round(target),
        moved: true
      });
      return;
    }

    var hit = hitAt(p);
    if (commitTap && hit.kind === 'segment') {
      if (hit.index === pressIndex) {
        // A release that slid off the pressed segment cancels (principle 10).
        commit(hit.index);
        return;
      }
      select(hit.index);
      return;
    }
    if (hit.kind === 'segment') select(hit.index);
  }

  wheelEl.addEventListener('pointerup', function (ev) { endDrag(ev, true); });
  wheelEl.addEventListener('pointercancel', function (ev) { endDrag(ev, false); });

  wheelEl.addEventListener('keydown', function (ev) {
    var ring = ringAt();
    if (!ring) return;
    var geo = ring.geo;
    if (ev.key === 'ArrowRight' || ev.key === 'ArrowLeft') {
      var dir = ev.key === 'ArrowRight' ? 1 : -1;
      var m = motion('settle');
      if (m.immediate) rot.jumpTo(rot.value + dir * geo.segAngle);
      else { rot.damping = m.damping; rot.response = m.response; rot.setTarget(rot.value + dir * geo.segAngle); }
      state.selected = ((state.selected + dir) % ring.segments.length + ring.segments.length) % ring.segments.length;
      updateHub();
      log('key:move', { selected: state.selected });
      ev.preventDefault();
    } else if (ev.key === 'Enter' || ev.key === ' ') {
      commit(state.selected);
      ev.preventDefault();
    } else if (ev.key === 'Escape') {
      dismiss();
      ev.preventDefault();
    }
  });

  document.addEventListener('keydown', function (ev) {
    if (ev.key === 'Escape') dismiss();
  });

  /* ---- drop ----------------------------------------------------------- */

  ['dragenter', 'dragover'].forEach(function (name) {
    wheelEl.addEventListener(name, function (ev) {
      ev.preventDefault();
      ev.dataTransfer.dropEffect = 'copy';
      wheelEl.classList.add('drop-active');
    });
  });

  wheelEl.addEventListener('dragleave', function () { wheelEl.classList.remove('drop-active'); });

  wheelEl.addEventListener('drop', function (ev) {
    ev.preventDefault();
    wheelEl.classList.remove('drop-active');
    var files = ev.dataTransfer && ev.dataTransfer.files ? Array.prototype.slice.call(ev.dataTransfer.files) : [];
    var kind = files.length ? kindFromName(files[0].name) : 'unknown';
    feed(kind);
    var p = pointerPoint(ev);
    var hit = hitAt(p);
    log('drop', { files: files.length, kind: kind, hit: hit.kind === 'segment' ? hit.index : hit.kind });
    if (hit.kind === 'segment') commit(hit.index);
  });

  /* Detection lives in Rust (model/detect.rs, card C1). This prototype maps an
     extension to a demo set so the wheel can be exercised in a browser; it is
     a stand-in, not a detector. */
  function kindFromName(name) {
    var ext = String(name || '').split('.').pop().toLowerCase();
    if (['png', 'jpg', 'jpeg', 'webp', 'avif', 'heic', 'tif', 'tiff', 'gif', 'svg'].indexOf(ext) !== -1) return 'image';
    if (['mp4', 'mkv', 'mov', 'webm'].indexOf(ext) !== -1) return 'video';
    if (['mp3', 'wav', 'flac', 'm4a', 'ogg', 'opus'].indexOf(ext) !== -1) return 'audio';
    if (['docx', 'xlsx', 'pptx', 'rtf', 'csv', 'md', 'html', 'txt', 'pdf', 'doc', 'xls', 'ppt'].indexOf(ext) !== -1) return 'document';
    return 'unknown';
  }

  /* ---- public surface (also what the interaction check drives) -------- */

  function feed(kind) {
    state.kind = CATALOGUE[kind] ? kind : 'unknown';
    state.phase = 'choosing';
    wheelEl.classList.remove('dismissed');
    rot.jumpTo(0);
    renderLayout();
  }

  function geometry() {
    var layout = state.layout;
    if (!layout || !layout.ok) return null;
    return layout.rings.map(function (r) {
      return {
        level: r.geo.level, outer: r.geo.outer, inner: r.geo.inner,
        labelRadius: r.geo.labelRadius, segAngle: r.geo.segAngle,
        fontPx: r.geo.fontPx, legible: r.geo.legible,
        segments: r.segments.map(function (s) {
          return { kind: s.kind, format: s.format, label: s.label, index: s.index, midAngle: s.midAngle };
        })
      };
    });
  }

  window.WHEEL = {
    feed: feed,
    geometry: geometry,
    hitAt: function (x, y) {
      var hit = hitAt({ x: x, y: y });
      return {
        kind: hit.kind,
        index: hit.kind === 'segment' ? hit.index : -1,
        format: hit.kind === 'segment' ? hit.segment.format : null,
        radius: hit.radius
      };
    },
    commitAt: function (x, y, velocityDegPerSec) {
      var ring = ringAt();
      if (!ring) return null;
      var sel = M.selection({ x: x, y: y }, ring, rotationFor(ring), velocityDegPerSec || 0, CENTRE);
      if (sel.index < 0) return { kind: 'none' };
      return commit(sel.index);
    },
    rotation: function () { return rot.value; },
    state: function () {
      return {
        kind: state.kind,
        phase: state.phase,
        selected: state.selected,
        rotation: rot.value,
        openLevel: state.openLevel,
        reducedMotion: state.reducedMotion,
        reducedTransparency: state.reducedTransparency,
        unplaced: state.layout ? state.layout.unplaced.map(function (t) { return t.format; }) : []
      };
    },
    events: function () { return events.slice(); },
    clearEvents: function () { events = []; logEl.innerHTML = ''; },
    log: log,
    commit: commit,
    dismiss: dismiss
  };

  /* ---- boot ----------------------------------------------------------- */

  readPreferences();
  if (window.matchMedia) {
    try {
      window.matchMedia('(prefers-reduced-motion: reduce)').addEventListener('change', applyMaterial);
      window.matchMedia('(prefers-reduced-transparency: reduce)').addEventListener('change', applyMaterial);
    } catch (e) { /* older webview: CSS media query still applies */ }
  }

  document.querySelectorAll('.demo button').forEach(function (button) {
    button.addEventListener('click', function () {
      document.querySelectorAll('.demo button').forEach(function (b) { b.classList.remove('on'); });
      button.classList.add('on');
      feed(button.dataset.kind);
    });
  });

  renderLayout();
  requestAnimationFrame(frame);
})();
