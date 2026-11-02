/* Overlay bridge — the wheel page is a rendered surface, this file is its connection
 * to the real window (`src-tauri/src/shell/overlay.rs`).
 *
 * When the page is opened in a browser (the prototype in `ui/wheel/README.md`) there
 * is no `__TAURI__` and this file returns immediately: nothing below runs, and the
 * demo chrome stays. In the shipped overlay it:
 *
 *   - switches the page into overlay mode (transparent, no demo chrome);
 *   - listens for `wheel://summon` and re-feeds the ring for the detected kind;
 *   - opens and closes with the one animation primitive (`spring.js`), scaling and
 *     fading the whole wheel, and honours `prefers-reduced-motion` by snapping;
 *   - acknowledges the first painted frame (`overlay_rendered`) so the app can
 *     measure summon → visible in milliseconds;
 *   - hands Esc and a click outside the disc back to Rust, which owns the window.
 *
 * No geometry is computed here beyond "is this point inside the disc", which is the
 * same rule `overlay::point_in_disc` implements on the Rust side.
 */
(function (root) {
  'use strict';

  var tauri = root.__TAURI__;
  if (!tauri || !tauri.core || !root.document) return;

  var core = tauri.core;
  var event = tauri.event;
  var doc = root.document;
  var wheelEl = doc.getElementById('wheel');
  if (!wheelEl) return;

  doc.documentElement.classList.add('overlay');

  var S = root.WheelSpring;
  var motion = { fade: null, scale: null, raf: 0, last: 0, opened: false };

  function reducedMotion() {
    return !!(root.WHEEL && root.WHEEL.state().reducedMotion);
  }

  function animate(open) {
    motion.opened = open;
    if (!S || reducedMotion()) {
      // Reduced motion: the state change is immediate, opacity only.
      wheelEl.style.opacity = open ? '1' : '0';
      wheelEl.style.transform = 'none';
      return;
    }
    if (!motion.fade) {
      // Apple's two designer parameters, not mass/stiffness: a critically damped
      // open (no bounce — the wheel is a tool, not a toy) and a slightly quicker
      // close.
      motion.fade = new S.Spring({ damping: 1, response: 0.28, value: open ? 0 : 1 });
      motion.scale = new S.Spring({ damping: 0.86, response: 0.32, value: open ? 0.94 : 1 });
    }
    motion.fade.setTarget(open ? 1 : 0);
    motion.scale.setTarget(open ? 1 : 0.94);
    motion.last = root.performance.now();
    if (!motion.raf) motion.raf = root.requestAnimationFrame(step);
  }

  function step(now) {
    var dt = Math.min(0.05, (now - motion.last) / 1000);
    motion.last = now;
    motion.fade.step(dt);
    motion.scale.step(dt);
    wheelEl.style.opacity = motion.fade.value.toFixed(3);
    wheelEl.style.transform = 'scale(' + motion.scale.value.toFixed(4) + ')';
    if (motion.fade.settled(0.01) && motion.scale.settled(0.001)) {
      motion.raf = 0;
      return;
    }
    motion.raf = root.requestAnimationFrame(step);
  }

  /** The summoned ring is on screen: tell Rust how long it took. Two frames, because
   *  the first one only schedules the paint. */
  function announceRendered() {
    root.requestAnimationFrame(function () {
      root.requestAnimationFrame(function () {
        core.invoke('overlay_rendered').catch(function () {});
      });
    });
  }

  function summon(payload) {
    wheelEl.classList.remove('dismissed');
    // Until detection lands (card C1) the overlay shows the image catalogue; the
    // payload is the placement decision, which the window itself already applied.
    if (root.WHEEL) root.WHEEL.feed((payload && payload.kind) || 'image');
    animate(true);
    announceRendered();
  }

  function dismissed() {
    animate(false);
    // Let the close spring finish before the window goes away; Rust hides the
    // window on `overlay_closed` and has its own timeout if this never arrives.
    var wait = reducedMotion() ? 0 : 260;
    root.setTimeout(function () {
      core.invoke('overlay_closed').catch(function () {});
    }, wait);
  }

  if (event && event.listen) {
    // Kept as a bonus for a build that grants `core:event`, but not relied on: this app
    // grants no capability for the event API, so the listeners below would never fire
    // (measured — the probe got its placement only through the handshake at the bottom).
    // Rust calls `__wheelSummon`/`__wheelDismiss` directly instead.
    try {
      event.listen('wheel://summon', function (message) {
        summon(message && message.payload);
      }).catch(function () {});
      event.listen('wheel://dismiss', dismissed).catch(function () {});
    } catch (e) { /* older webview, or no permission: the direct calls cover it */ }
  }

  // The two entry points Rust uses (`shell/overlay.rs::dispatch`). A plain global
  // function: native injection from the host does not need a permission.
  root.__wheelSummon = summon;
  root.__wheelDismiss = dismissed;

  // Esc: Rust watches the key globally (the window is deliberately non-activating),
  // but when the overlay does have the keyboard this is the immediate path.
  doc.addEventListener('keydown', function (ev) {
    if (ev.key !== 'Escape') return;
    ev.preventDefault();
    ev.stopPropagation();
    core.invoke('overlay_dismiss').catch(function () {});
  }, true);

  // A click inside the square but outside the disc. On Windows the window is shaped
  // to the ellipse so this never fires there; on macOS the Rust watcher toggles
  // click-through, and this is the guard for the frame where the toggle is still in
  // flight.
  doc.addEventListener('pointerdown', function (ev) {
    var box = wheelEl.getBoundingClientRect();
    var dx = ev.clientX - (box.left + box.width / 2);
    var dy = ev.clientY - (box.top + box.height / 2);
    var radius = Math.min(box.width, box.height) / 2;
    if (dx * dx + dy * dy > radius * radius) {
      core.invoke('overlay_dismiss').catch(function () {});
    }
  }, true);

  // A summon that arrived before this listener existed is not lost: Rust keeps the
  // last placement and answers with it.
  core.invoke('overlay_ready').then(function (payload) {
    if (payload) summon(payload);
  }).catch(function () {});
})(typeof globalThis !== 'undefined' ? globalThis : this);
