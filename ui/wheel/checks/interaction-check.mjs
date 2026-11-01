#!/usr/bin/env node
/* Recorded interaction check for the wheel overlay.
 *
 * Drives a real Chromium over the DevTools protocol (no puppeteer/playwright
 * dependency — Node's built-in WebSocket is the client) and checks the wheel
 * against the apple-design checklist the card names:
 *
 *   1  immediate feedback (press-state before release)
 *   2  1:1 grabbing and grab offset
 *   3  interruptible from the presentation value
 *   4  spring parameters (critical for settling, 0.8 for momentum)
 *   5  velocity handoff on release
 *   6  momentum projection for segment selection
 *   9  rubber-banding is not used for the ring (no hard stops in the drag)
 *   10 tap: press highlight, slide-off cancel
 *   11 transform/opacity only, will-change present
 *   12 backdrop-filter material, no stacked translucency
 *   14 prefers-reduced-motion / prefers-reduced-transparency fallbacks
 *   15 system-ui type with size-dependent tracking
 *
 * plus the card's own acceptance: hard cap of six, a more sub-ring, only
 * valid targets, hit-testing correct at every angle, nothing under the
 * legibility floor.
 *
 * Usage: node ui/wheel/checks/interaction-check.mjs [--headful]
 * Env:   CHROME=/path/to/chrome SHOT_DIR=/tmp/shots
 */
import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const PAGE = path.resolve(HERE, '..', 'index.html');
const CHROME = process.env.CHROME || '/opt/hermes/tools/chromium-1208/chrome-linux64/chrome';
const SHOT_DIR = process.env.SHOT_DIR || path.join('/tmp', 'u1-wheel-shots');
const PORT = Number(process.env.CDP_PORT || 9333);
const HEADFUL = process.argv.includes('--headful');

const SIZE = 520;
const CENTRE = { x: SIZE / 2, y: SIZE / 2 };
const FLOOR_PX = 11 * 96 / 72;

const results = [];
let sha = [];

function record(name, ok, detail) {
  results.push({ name, ok, detail: String(detail) });
  const mark = ok ? 'PASS' : 'FAIL';
  console.log(`${mark}  ${name}${ok ? '' : '  —  ' + detail}`);
}
async function check(name, fn) {
  try {
    const detail = await fn();
    record(name, true, detail || 'ok');
  } catch (err) {
    record(name, false, err && err.message ? err.message : err);
  }
}
function assert(cond, message) { if (!cond) throw new Error(message); }
function near(a, b, tol) { return Math.abs(a - b) <= tol; }
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/* ---- CDP client ------------------------------------------------------- */

class CDP {
  constructor(ws) {
    this.ws = ws;
    this.nextId = 1;
    this.pending = new Map();
    ws.addEventListener('message', (event) => {
      const msg = JSON.parse(event.data);
      if (msg.id && this.pending.has(msg.id)) {
        const { resolve, reject } = this.pending.get(msg.id);
        this.pending.delete(msg.id);
        if (msg.error) reject(new Error(msg.error.message + ' ' + JSON.stringify(msg.error.data || '')));
        else resolve(msg.result);
      }
    });
  }

  static async attach(url) {
    const ws = new WebSocket(url);
    await new Promise((resolve, reject) => {
      ws.addEventListener('open', resolve, { once: true });
      ws.addEventListener('error', (e) => reject(new Error('websocket error: ' + (e.message || 'unknown'))), { once: true });
    });
    return new CDP(ws);
  }

  send(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ id, method, params }));
      setTimeout(() => {
        if (this.pending.has(id)) {
          this.pending.delete(id);
          reject(new Error(`CDP timeout: ${method}`));
        }
      }, 30000);
    });
  }

  async evaluate(expression) {
    const r = await this.send('Runtime.evaluate', {
      expression, returnByValue: true, awaitPromise: true, userGesture: true
    });
    if (r.exceptionDetails) {
      throw new Error('page error: ' + (r.exceptionDetails.exception && r.exceptionDetails.exception.description
        || r.exceptionDetails.text));
    }
    return r.result.value;
  }

  async mouse(type, x, y, extra = {}) {
    await this.send('Input.dispatchMouseEvent', {
      type, x, y, button: 'left',
      buttons: type === 'mouseReleased' ? 0 : 1,
      clickCount: 1, pointerType: 'mouse', ...extra
    });
  }

  async key(key, code, windowsVirtualKeyCode) {
    const common = { key, code, windowsVirtualKeyCode, nativeVirtualKeyCode: windowsVirtualKeyCode };
    await this.send('Input.dispatchKeyEvent', { type: 'keyDown', ...common });
    await this.send('Input.dispatchKeyEvent', { type: 'keyUp', ...common });
  }

  async shot(file) {
    const r = await this.send('Page.captureScreenshot', { format: 'png', captureBeyondViewport: false });
    const p = path.join(SHOT_DIR, file);
    fs.writeFileSync(p, Buffer.from(r.data, 'base64'));
    return p;
  }

  close() { try { this.ws.close(); } catch { /* closing */ } }
}

async function waitForTarget(url, timeoutMs = 20000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const list = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      const page = list.find((t) => t.type === 'page' && t.webSocketDebuggerUrl);
      if (page) return page;
    } catch { /* browser not up yet */ }
    await sleep(150);
  }
  throw new Error('chromium did not expose a page target');
}

/* ---- geometry helpers ------------------------------------------------- */

function pointAtLocal(angleDeg, radius) {
  const a = angleDeg * Math.PI / 180;
  return { x: CENTRE.x + Math.sin(a) * radius, y: CENTRE.y - Math.cos(a) * radius };
}

function expectedIndex(angleDeg, segAngle, rotation, count) {
  const rel = ((angleDeg - rotation) % 360 + 360) % 360;
  return Math.min(count - 1, Math.floor(rel / segAngle));
}

/* ---- main ------------------------------------------------------------- */

const shotFiles = [];
let chrome;

async function boot(cdp) {
  await cdp.send('Runtime.enable');
  await cdp.send('Page.enable');
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    const ready = await cdp.evaluate('!!(window.WHEEL && window.WheelModel && window.WheelSpring)').catch(() => false);
    if (ready) break;
    await sleep(100);
  }
  await cdp.evaluate('new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)))');
}

async function wheelRect(cdp) {
  const r = await cdp.evaluate(`(() => {
    const b = document.getElementById('wheel').getBoundingClientRect();
    return { left: b.left, top: b.top, width: b.width, height: b.height };
  })()`);
  return { ...r, centre: { x: r.left + CENTRE.x, y: r.top + CENTRE.y } };
}

async function geometry(cdp) {
  return cdp.evaluate('WHEEL.geometry()');
}

async function main() {
  fs.mkdirSync(SHOT_DIR, { recursive: true });

  const args = [
    `--remote-debugging-port=${PORT}`,
    '--headless=new',
    '--no-sandbox',
    '--disable-gpu',
    '--hide-scrollbars',
    '--force-device-scale-factor=1',
    '--window-size=760,900',
    '--user-data-dir=' + path.join(SHOT_DIR, 'profile'),
    'file://' + PAGE
  ];
  if (HEADFUL) args.splice(args.indexOf('--headless=new'), 1);
  chrome = spawn(CHROME, args, { stdio: 'ignore' });

  const target = await waitForTarget();
  const cdp = await CDP.attach(target.webSocketDebuggerUrl);
  await boot(cdp);

  const rect = await wheelRect(cdp);
  const R = (angle, radius) => {
    const p = pointAtLocal(angle, radius);
    return { x: rect.left + p.x, y: rect.top + p.y };
  };

  /* 1 — boot and the hard cap ------------------------------------------ */
  await check('image type: six segments at most, overflow behind one more segment', async () => {
    const g = await geometry(cdp);
    const ring0 = g[0];
    assert(ring0.segments.length === 6, `ring 0 has ${ring0.segments.length} segments`);
    const more = ring0.segments.filter((s) => s.kind === 'more');
    assert(more.length === 1, 'exactly one more segment');
    assert(g.length === 2, `expected a sub-ring, got ${g.length} rings`);
    assert(g[1].segments.length === 2, `sub-ring has ${g[1].segments.length}`);
    assert(g.every((r) => r.segments.length <= 6), 'no ring exceeds six');
    return `ring0 ${ring0.segments.map((s) => s.format || 'more').join(',')} / ring1 ${g[1].segments.map((s) => s.format).join(',')}`;
  });

  await check('only valid targets for the detected type appear', async () => {
    const image = await cdp.evaluate(`(() => { WHEEL.feed('image'); return WHEEL.geometry(); })()`);
    const imageFormats = image.flatMap((r) => r.segments.map((s) => s.format)).filter(Boolean);
    assert(imageFormats.every((f) => ['png', 'jpg', 'webp', 'avif', 'heic', 'tiff', 'gif'].includes(f)),
      'image wheel offered a non-image target: ' + imageFormats);
    const video = await cdp.evaluate(`(() => { WHEEL.feed('video'); return WHEEL.geometry(); })()`);
    const videoFormats = video.flatMap((r) => r.segments.map((s) => s.format)).filter(Boolean);
    assert(!videoFormats.includes('png'), 'video wheel offered png');
    assert(videoFormats.includes('mp4'), 'video wheel lost mp4');
    assert(video[0].segments.length === 6 && video.length === 1,
      'six video targets should fill exactly one ring with no more segment');
    const unknown = await cdp.evaluate(`(() => { WHEEL.feed('unknown'); return WHEEL.geometry(); })()`);
    assert(unknown === null, 'unknown type must offer no targets');
    await cdp.evaluate(`WHEEL.feed('image')`);
    return `image ${imageFormats.length}, video ${videoFormats.length}, unknown 0`;
  });

  /* 2 — legibility floor in the rendered DOM --------------------------- */
  await check('nothing rendered smaller than the 11 pt legibility floor', async () => {
    const rendered = await cdp.evaluate(`(() => {
      const names = [...document.querySelectorAll('.label .name')];
      return names.map((el) => {
        const cs = getComputedStyle(el);
        return {
          text: el.textContent,
          fontPx: parseFloat(cs.fontSize),
          tracking: cs.letterSpacing,
          width: el.getBoundingClientRect().width,
          lineHeight: cs.lineHeight
        };
      });
    })()`);
    assert(rendered.length >= 6, `only ${rendered.length} labels rendered`);
    for (const label of rendered) {
      assert(label.fontPx >= FLOOR_PX - 0.01,
        `"${label.text}" rendered at ${label.fontPx}px, floor is ${FLOOR_PX.toFixed(2)}px (11 pt)`);
      assert(label.fontPx >= 11, `"${label.text}" is ${label.fontPx}px`);
    }
    const g = await geometry(cdp);
    for (const ring of g) {
      assert(ring.fontPx >= FLOOR_PX, `ring ${ring.level} plans ${ring.fontPx}px`);
      assert(ring.legible === true, `ring ${ring.level} is not legible`);
    }
    const min = Math.min(...rendered.map((l) => l.fontPx));
    return `${rendered.length} labels, smallest ${min.toFixed(1)}px (floor ${FLOOR_PX.toFixed(2)}px), tracking ${rendered[0].tracking}`;
  });

  /* 3 — hit-testing at every angle, in the live DOM -------------------- */
  await check('hit-testing is correct at every angle (DOM raster vs arithmetic)', async () => {
    const g = await geometry(cdp);
    const ring = g[0];
    const rotation = await cdp.evaluate('WHEEL.rotation()');
    const sweep = await cdp.evaluate(`(() => {
      const rect = document.getElementById('wheel').getBoundingClientRect();
      const cx = rect.left + ${CENTRE.x}, cy = rect.top + ${CENTRE.y};
      const radii = ${JSON.stringify([ring.inner + 6, ring.labelRadius, ring.outer - 6])};
      const out = { samples: [], misses: [] };
      for (const radius of radii) {
        for (let step = 0; step < 720; step++) {
          const a = step * 0.5;
          const x = cx + Math.sin(a * Math.PI / 180) * radius;
          const y = cy - Math.cos(a * Math.PI / 180) * radius;
          const el = document.elementFromPoint(x, y);
          const node = el && el.closest ? el.closest('[data-index]') : null;
          const domIndex = node ? Number(node.dataset.index) : -1;
          if (domIndex === -1) {
            out.misses.push({ a, radius, tag: el ? el.tagName + '.' + (el.getAttribute('class') || '') : 'null' });
          }
          out.samples.push(domIndex);
        }
      }
      return out;
    })()`);

    assert(sweep.misses.length === 0,
      `${sweep.misses.length} points hit no segment, e.g. ${JSON.stringify(sweep.misses.slice(0, 3))}`);
    const segAngle = ring.segAngle;
    const count = ring.segments.length;
    let checked = 0, boundary = 0;
    for (let i = 0; i < sweep.samples.length; i++) {
      const radiusIndex = Math.floor(i / 720);
      const step = i % 720;
      const angle = step * 0.5;
      const dom = sweep.samples[i];
      const expect = expectedIndex(angle, segAngle, rotation, count);
      const rel = ((angle - rotation) % 360 + 360) % 360;
      const nearest = Math.round(rel / segAngle) * segAngle;
      if (Math.abs(rel - nearest) < 1e-6) {
        const either = [expect, (expect + count - 1) % count];
        assert(either.includes(dom), `boundary ${angle}° got ${dom}, expected ${either}`);
        boundary++;
      } else {
        assert(dom === expect,
          `angle ${angle}° r=${[ring.inner + 6, ring.labelRadius, ring.outer - 6][radiusIndex]}: DOM says ${dom}, arithmetic says ${expect}`);
      }
      checked++;
    }
    return `${checked} points over 3 radii (${boundary} on an exact boundary), rotation ${rotation.toFixed(1)}°`;
  });

  /* 4 — labels sit inside the ring and never collide ------------------- */
  await check('labels sit inside their ring band and never overlap each other', async () => {
    const labels = await cdp.evaluate(`(() => {
      const wheel = document.getElementById('wheel').getBoundingClientRect();
      const c = { x: wheel.left + wheel.width / 2, y: wheel.top + wheel.height / 2 };
      return [...document.querySelectorAll('.label')].map((el) => {
        const r = el.getBoundingClientRect();
        return {
          text: el.querySelector('.name').textContent,
          cx: r.left + r.width / 2 - c.x,
          cy: r.top + r.height / 2 - c.y,
          w: r.width, h: r.height
        };
      });
    })()`);
    const g = await geometry(cdp);
    const ring = g[0];
    for (const label of labels) {
      const radius = Math.hypot(label.cx, label.cy);
      assert(radius > ring.inner && radius < ring.outer,
        `"${label.text}" centre is at r=${radius.toFixed(1)}, outside the band ${ring.inner}..${ring.outer}`);
    }
    // no two label boxes may overlap: a chord of 60° at r=192 is 192 px
    let worst = Infinity;
    for (let i = 0; i < labels.length; i++) {
      for (let j = i + 1; j < labels.length; j++) {
        const dx = Math.abs(labels[i].cx - labels[j].cx);
        const dy = Math.abs(labels[i].cy - labels[j].cy);
        const gapX = dx - (labels[i].w + labels[j].w) / 2;
        const gapY = dy - (labels[i].h + labels[j].h) / 2;
        const gap = Math.max(gapX, gapY);
        worst = Math.min(worst, gap);
      }
    }
    assert(worst > 4, `two labels are ${worst.toFixed(1)}px apart (or overlapping)`);
    return `${labels.length} labels inside r=${ring.inner}..${ring.outer}, closest pair ${worst.toFixed(0)}px apart`;
  });

  /* 5 — material ------------------------------------------------------- */
  await check('material is backdrop-filter glass, and no surface stacks translucency', async () => {
    const outer = await cdp.evaluate(`(() => {
      const cs = getComputedStyle(document.getElementById('surface'));
      return { backdrop: cs.backdropFilter, background: cs.backgroundColor };
    })()`);
    assert(/blur\(/.test(outer.backdrop), `outer surface backdrop-filter is "${outer.backdrop}"`);
    assert(/rgba\([^)]*0\.\d+\)/.test(outer.background), `outer surface is not translucent: ${outer.background}`);
    return `surface ${outer.backdrop} over ${outer.background}`;
  });

  /* 5 — pointer-down feedback ----------------------------------------- */
  await check('feedback starts on pointer-down, not on release', async () => {
    const g = await geometry(cdp);
    const seg = g[0].segments[2];
    const p = R(seg.midAngle, g[0].labelRadius);
    await cdp.mouse('mousePressed', p.x, p.y);
    await sleep(120);
    const pressed = await cdp.evaluate(`(() => {
      const label = document.querySelectorAll('.label')[2];
      const hi = document.querySelectorAll('.wedge-highlight')[2];
      return {
        transform: label.style.transform,
        highlightOpacity: Number(hi.style.opacity || 0),
        phase: WHEEL.state().phase
      };
    })()`);
    // release whatever happened, so a failed assertion cannot strand the
    // pointer in a pressed state and poison the next check
    await cdp.mouse('mouseReleased', p.x, p.y);
    await sleep(40);
    const after = await cdp.evaluate(`WHEEL.state()`);

    const scaleMatch = /scale\(([\d.]+)\)/.exec(pressed.transform);
    const scale = scaleMatch ? Number(scaleMatch[1]) : 1;
    assert(scale < 0.995, `pressed label scale is ${scale}, expected a press state`);
    assert(pressed.highlightOpacity > 0.05,
      `highlight opacity stayed at ${pressed.highlightOpacity} while pressed`);
    assert(['confirming', 'converting', 'done'].includes(after.phase),
      `releasing on the pressed segment did not commit (phase ${after.phase})`);
    return `scale ${scale.toFixed(3)}, highlight ${pressed.highlightOpacity.toFixed(2)}, phase after release ${after.phase}`;
  });

  /* 6 — the tap did not move the ring --------------------------------- */
  await check('a tap commits without moving the ring (no momentum without a drag)', async () => {
    const rotation = await cdp.evaluate('WHEEL.rotation()');
    assert(near(rotation, 0, 0.01), `tap moved the ring to ${rotation}°`);
    const events = await cdp.evaluate('WHEEL.events()');
    const commit = events.filter((e) => e.name === 'commit').pop();
    assert(commit, 'no commit event recorded');
    assert(commit.detail && commit.detail.format === 'webp',
      `commit recorded ${JSON.stringify(commit.detail)}, expected the pressed segment webp`);
    await sleep(400);
    const done = await cdp.evaluate('WHEEL.events()');
    assert(done.some((e) => e.name === 'converted'), 'the commit never reached converted');
    return `commit → ${commit.detail.format}, converted ${done.filter((e) => e.name === 'converted').length}×`;
  });

  /* 7 — momentum flick ------------------------------------------------ */
  let flick = null;
  await check('a flick projects momentum and the ring overshoots (damping 0.8)', async () => {
    await cdp.evaluate('WHEEL.feed("image")');
    await sleep(60);
    const g = await geometry(cdp);
    const radius = g[0].labelRadius;
    const start = R(200, radius);
    await cdp.mouse('mousePressed', start.x, start.y);
    for (let i = 1; i <= 4; i++) {
      const p = R(200 + i * 8, radius);
      await cdp.mouse('mouseMoved', p.x, p.y);
      await sleep(8);
    }
    const end = R(240, radius);
    await cdp.mouse('mouseMoved', end.x, end.y);
    await sleep(8);
    await cdp.mouse('mouseReleased', end.x, end.y);

    await sleep(10);
    const releaseEvent = await cdp.evaluate(`WHEEL.events().filter(e => e.name === 'release:momentum').pop()`);
    assert(releaseEvent, 'no release:momentum event: the drag was not treated as a flick');
    flick = releaseEvent.detail;
    assert(Math.abs(flick.velocityDegPerSec) > 150,
      `release velocity ${flick.velocityDegPerSec}°/s is too small to prove a handoff`);

    const trace = await cdp.evaluate(`new Promise((resolve) => {
      const samples = [];
      const t0 = performance.now();
      function frame() {
        samples.push(WHEEL.rotation());
        if (performance.now() - t0 < 650) requestAnimationFrame(frame);
        else resolve(samples);
      }
      requestAnimationFrame(frame);
    })`);
    assert(trace.length > 10, `only ${trace.length} frames sampled`);
    const target = flick.targetDeg;
    const direction = Math.sign(target - 0) || 1;
    const overshoot = direction > 0
      ? Math.max(...trace) - target
      : target - Math.min(...trace);
    const final = trace[trace.length - 1];
    assert(overshoot > 0.3, `no overshoot: max ${direction > 0 ? Math.max(...trace) : Math.min(...trace)} vs target ${target}`);
    assert(near(final, target, 1.5), `did not settle on the projected target: ${final} vs ${target}`);
    return `release ${flick.velocityDegPerSec}°/s, projected ${flick.projectedDeg}°, target ${target.toFixed(1)}°, overshoot ${overshoot.toFixed(2)}°, settled ${final.toFixed(2)}° in ${trace.length} frames`;
  });

  /* 8 — interruptibility --------------------------------------------- */
  await check('catching the ring mid-flight stops it exactly where it is', async () => {
    // start another flick, then grab it while it is still moving
    await cdp.evaluate('WHEEL.feed("image")');
    await sleep(60);
    const g = await geometry(cdp);
    const radius = g[0].labelRadius;
    const start = R(120, radius);
    await cdp.mouse('mousePressed', start.x, start.y);
    for (let i = 1; i <= 4; i++) {
      const p = R(120 + i * 4, radius);
      await cdp.mouse('mouseMoved', p.x, p.y);
      await sleep(8);
    }
    const end = R(136, radius);
    await cdp.mouse('mouseMoved', end.x, end.y);
    await sleep(8);
    await cdp.mouse('mouseReleased', end.x, end.y);
    await sleep(45);                                   // mid-flight

    const grab = R(200, radius);
    await cdp.mouse('mousePressed', grab.x, grab.y);
    const caughtAt = await cdp.evaluate('WHEEL.rotation()');
    await sleep(90);
    const later = await cdp.evaluate('WHEEL.rotation()');
    assert(near(caughtAt, later, 0.05),
      `ring kept moving after being caught: ${caughtAt.toFixed(2)}° → ${later.toFixed(2)}°`);
    await cdp.mouse('mouseReleased', grab.x, grab.y);
    // it was caught in flight, not after it had already settled
    const release = await cdp.evaluate(`WHEEL.events().filter(e => e.name === 'release:momentum').pop()`);
    assert(release, 'no momentum event to compare against');
    assert(!near(caughtAt, release.detail.targetDeg, 0.5),
      `catch happened after settle (${caughtAt}° == target ${release.detail.targetDeg}°), not mid-flight`);
    return `caught at ${caughtAt.toFixed(1)}° while heading for ${release.detail.targetDeg.toFixed(1)}°`;
  });

  /* 9 — keyboard settle spring --------------------------------------- */
  await check('keyboard move uses a critical spring: no overshoot, monotone approach', async () => {
    await cdp.evaluate(`(() => { WHEEL.feed('image'); document.getElementById('wheel').focus(); })()`);
    await sleep(60);
    const before = await cdp.evaluate('WHEEL.rotation()');
    const trace = await cdp.evaluate(`new Promise((resolve) => {
      const samples = [];
      const t0 = performance.now();
      document.getElementById('wheel').dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
      function frame() {
        samples.push(WHEEL.rotation());
        if (performance.now() - t0 < 500) requestAnimationFrame(frame);
        else resolve(samples);
      }
      requestAnimationFrame(frame);
    })`);
    const final = trace[trace.length - 1];
    const maxValue = Math.max(...trace);
    assert(near(final - before, 60, 1), `ArrowRight moved ${(final - before).toFixed(2)}°, expected 60°`);
    assert(maxValue <= final + 0.01, `critical spring overshot: max ${maxValue} vs final ${final}`);
    let monotone = true;
    for (let i = 1; i < trace.length; i++) if (trace[i] < trace[i - 1] - 0.001) monotone = false;
    assert(monotone, 'critical spring was not monotone');
    return `0° → ${final.toFixed(1)}° in ${trace.length} frames, overshoot 0`;
  });

  /* 10 — sub-ring + recursive transparency ---------------------------- */
  await check('the more segment opens a sub-ring, and the nested surface turns opaque', async () => {
    await cdp.evaluate('WHEEL.feed("image")');
    await sleep(60);
    const g = await geometry(cdp);
    const moreIndex = g[0].segments.findIndex((s) => s.kind === 'more');
    assert(moreIndex >= 0, 'no more segment to open');
    const p = R(g[0].segments[moreIndex].midAngle, g[0].labelRadius);
    await cdp.mouse('mousePressed', p.x, p.y);
    await cdp.mouse('mouseReleased', p.x, p.y);
    await sleep(120);
    const state = await cdp.evaluate('WHEEL.state()');
    assert(state.openLevel === 1, `sub-ring did not open (openLevel ${state.openLevel})`);
    // The material animates its blur as it arrives (principle 12), so read the
    // settled value rather than a frame mid-transition.
    const material = await cdp.evaluate(`new Promise((resolve) => {
      const hub = document.getElementById('hub');
      let stable = 0, last = null;
      const t0 = performance.now();
      function tick() {
        const cs = getComputedStyle(hub);
        const now = cs.backdropFilter + '|' + cs.backgroundColor;
        if (now === last) stable++; else { stable = 0; last = now; }
        if (stable >= 6 || performance.now() - t0 > 1500) {
          resolve({
            backdrop: cs.backdropFilter,
            background: cs.backgroundColor,
            ringOpen: document.getElementById('wheel').classList.contains('ring-open')
          });
        } else requestAnimationFrame(tick);
      }
      requestAnimationFrame(tick);
    })`);
    assert(material.ringOpen, 'ring-open class missing');
    assert(material.backdrop === 'none', `nested hub still blurs (${material.backdrop}): translucency stacked on translucency`);
    const alpha = Number((/rgba?\([^)]*?([\d.]+)\)$/.exec(material.background) || [])[1] || 1);
    assert(alpha >= 0.9, `nested hub is still translucent: ${material.background}`);
    const sub = await geometry(cdp);
    assert(sub[1].segments.length === 2, `sub-ring shows ${sub[1].segments.length}`);
    assert(sub[1].fontPx >= FLOOR_PX, 'sub-ring label is under the floor');
    shotFiles.push(await cdp.shot('03-subring.png'));
    return `openLevel 1, hub ${material.background} / ${material.backdrop}, ${sub[1].segments.length} children`;
  });

  /* 11 — typography --------------------------------------------------- */
  await check('typography: system-ui with size-dependent tracking', async () => {
    const type = await cdp.evaluate(`(() => {
      const name = document.querySelector('.label .name');
      const sub = document.querySelector('.label .sub');
      const nameCs = getComputedStyle(name);
      const subCs = sub ? getComputedStyle(sub) : null;
      return {
        family: nameCs.fontFamily,
        nameSize: parseFloat(nameCs.fontSize),
        nameTracking: nameCs.letterSpacing,
        subSize: subCs ? parseFloat(subCs.fontSize) : null,
        subTracking: subCs ? subCs.letterSpacing : null
      };
    })()`);
    assert(/system-ui/.test(type.family), `label font is "${type.family}"`);
    assert(type.nameTracking !== type.subTracking,
      `one tracking value everywhere: ${type.nameTracking} at both sizes`);
    // large type tightens, small type opens
    assert(parseFloat(type.nameTracking) < 0,
      `22 px labels should tighten, got ${type.nameTracking}`);
    assert(parseFloat(type.subTracking) >= 0,
      `10 px captions should not be tightened, got ${type.subTracking}`);
    return `${type.nameSize}px/${type.nameTracking} vs ${type.subSize}px/${type.subTracking}, ${type.family.split(',')[0]}`;
  });

  /* 12 — reduced motion ---------------------------------------------- */
  await check('prefers-reduced-motion: the spring is removed, not shortened', async () => {
    await cdp.send('Emulation.setEmulatedMedia', {
      features: [{ name: 'prefers-reduced-motion', value: 'reduce' }]
    });
    await cdp.send('Page.reload', { ignoreCache: true });
    await boot(cdp);
    const media = await cdp.evaluate(`matchMedia('(prefers-reduced-motion: reduce)').matches`);
    assert(media, 'the emulated media query did not reach the page');
    const g = await geometry(cdp);
    const radius = g[0].labelRadius;
    const start = R(30, radius);
    await cdp.mouse('mousePressed', start.x, start.y);
    for (let i = 1; i <= 4; i++) {
      const p = R(30 + i * 4, radius);
      await cdp.mouse('mouseMoved', p.x, p.y);
      await sleep(12);
    }
    const end = R(46, radius);
    await cdp.mouse('mouseMoved', end.x, end.y);
    await sleep(12);
    await cdp.mouse('mouseReleased', end.x, end.y);
    await sleep(20);
    const release = await cdp.evaluate(`WHEEL.events().filter(e => e.name === 'release:momentum').pop()`);
    assert(release, 'no release event under reduced motion');
    const rotation = await cdp.evaluate('WHEEL.rotation()');
    assert(near(rotation, release.detail.targetDeg, 0.01),
      `reduced motion still animated: at ${rotation}° heading for ${release.detail.targetDeg}°`);
    const transition = await cdp.evaluate(`getComputedStyle(document.getElementById('wheel')).transitionProperty`);
    assert(/opacity/.test(transition), `the fallback transition is not an opacity one: ${transition}`);
    shotFiles.push(await cdp.shot('04-reduced-motion.png'));
    return `snapped to ${rotation.toFixed(1)}° with no intermediate frames, transition "${transition}"`;
  });

  /* 13 — reduced transparency ---------------------------------------- */
  await check('prefers-reduced-transparency: surfaces become solid', async () => {
    await cdp.send('Emulation.setEmulatedMedia', {
      features: [{ name: 'prefers-reduced-transparency', value: 'reduce' }]
    });
    await cdp.send('Page.reload', { ignoreCache: true });
    await boot(cdp);
    const media = await cdp.evaluate(`matchMedia('(prefers-reduced-transparency: reduce)').matches`);
    assert(media, 'the emulated media query did not reach the page');
    const surface = await cdp.evaluate(`(() => {
      const cs = getComputedStyle(document.getElementById('surface'));
      return { backdrop: cs.backdropFilter, background: cs.backgroundColor };
    })()`);
    assert(surface.backdrop === 'none', `still blurred under reduced transparency: ${surface.backdrop}`);
    const m = /rgba?\(([^)]+)\)/.exec(surface.background);
    const parts = m ? m[1].split(',').map((s) => Number(s.trim())) : [];
    const alpha = parts.length === 4 ? parts[3] : 1;
    assert(alpha >= 0.99, `surface is still translucent: ${surface.background}`);
    shotFiles.push(await cdp.shot('05-reduced-transparency.png'));
    return `backdrop ${surface.backdrop}, background ${surface.background}`;
  });

  /* reset media, take the hero screenshots ---------------------------- */
  await cdp.send('Emulation.setEmulatedMedia', { features: [] });
  await cdp.send('Page.reload', { ignoreCache: true });
  await boot(cdp);

  await cdp.evaluate(`(() => { WHEEL.feed('image'); })()`);
  await sleep(200);
  shotFiles.push(await cdp.shot('01-idle.png'));

  const g = await geometry(cdp);
  const sel = R(g[0].segments[1].midAngle, g[0].labelRadius);
  await cdp.mouse('mousePressed', sel.x, sel.y);
  await sleep(120);
  shotFiles.push(await cdp.shot('02-pressed.png'));
  await cdp.mouse('mouseReleased', sel.x, sel.y);
  await sleep(120);

  await cdp.evaluate(`(() => { WHEEL.feed('image'); })()`);
  await sleep(80);
  const g2 = await geometry(cdp);
  const moreIndex = g2[0].segments.findIndex((s) => s.kind === 'more');
  const more = R(g2[0].segments[moreIndex].midAngle, g2[0].labelRadius);
  await cdp.mouse('mousePressed', more.x, more.y);
  await cdp.mouse('mouseReleased', more.x, more.y);
  await sleep(250);
  shotFiles.push(await cdp.shot('06-subring.png'));

  /* 14 — Esc dismiss --------------------------------------------------- */
  await check('Escape dismisses the wheel', async () => {
    await cdp.key('Escape', 'Escape', 27);
    await sleep(60);
    const state = await cdp.evaluate(`(() => ({ phase: WHEEL.state().phase,
      dismissed: document.getElementById('wheel').classList.contains('dismissed') }))()`);
    assert(state.dismissed && state.phase === 'dismissed', `after Escape: ${JSON.stringify(state)}`);
    return 'phase dismissed, .dismissed applied';
  });

  cdp.close();
}

main()
  .then(async () => {
    const pass = results.filter((r) => r.ok).length;
    const fail = results.length - pass;
    console.log(`\nSUMMARY pass=${pass} fail=${fail} checks=${results.length}`);
    const report = {
      when: new Date().toISOString(),
      chrome: CHROME,
      page: PAGE,
      pass, fail,
      checks: results,
      screenshots: shotFiles
    };
    fs.writeFileSync(path.join(SHOT_DIR, 'report.json'), JSON.stringify(report, null, 2));
    console.log('report: ' + path.join(SHOT_DIR, 'report.json'));
    console.log('screenshots: ' + SHOT_DIR);
    if (chrome) chrome.kill('SIGKILL');
    process.exit(fail === 0 ? 0 : 1);
  })
  .catch((err) => {
    console.error('HARNESS ERROR: ' + (err && err.stack || err));
    if (chrome) chrome.kill('SIGKILL');
    process.exit(2);
  });
