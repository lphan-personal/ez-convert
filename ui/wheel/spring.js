/* Spring solver — one animation primitive for the whole wheel.

   Apple exposes two designer parameters, not mass/stiffness/damping
   (skill principle 4): damping ratio and response (seconds). Both map onto a
   unit-mass spring:

       omega = 2*pi / response        natural frequency
       k     = omega^2                stiffness
       c     = 2 * damping * omega    damping coefficient

   A spring is interruptible by construction: retargeting keeps the current
   value and velocity, which is what makes a mid-flight catch feel seamless
   (principle 3) and gives velocity handoff on release (principle 5).

   No DOM. Shared by the wheel and by node tests. */
(function (root) {
  'use strict';

  var SUBSTEP = 1 / 240; // fixed integration step, well under one frame

  function Spring(spec) {
    spec = spec || {};
    this.damping = spec.damping === undefined ? 1 : spec.damping;
    this.response = spec.response === undefined ? 0.4 : spec.response;
    this.value = spec.value === undefined ? 0 : spec.value;
    this.target = spec.target === undefined ? this.value : spec.target;
    this.velocity = spec.velocity || 0;
    this.immediate = !!spec.immediate;
    if (this.immediate) this.value = this.target;
  }

  Spring.prototype.omega = function () {
    return this.response > 0 ? (2 * Math.PI) / this.response : Infinity;
  };

  /** Retarget without a seam: value and velocity carry over. */
  Spring.prototype.setTarget = function (target, velocity) {
    this.target = target;
    if (velocity !== undefined && velocity !== null) this.velocity = velocity;
    if (this.immediate) {
      this.value = target;
      this.velocity = 0;
    }
    return this;
  };

  /** Snap with no animation (reduced motion, or an off-screen reset). */
  Spring.prototype.jumpTo = function (value) {
    this.value = value;
    this.target = value;
    this.velocity = 0;
    return this;
  };

  Spring.prototype.acceleration = function () {
    var omega = this.omega();
    if (!isFinite(omega)) return 0;
    var k = omega * omega;
    var c = 2 * this.damping * omega;
    return -k * (this.value - this.target) - c * this.velocity;
  };

  /** Advance by `dt` seconds using fixed substeps (stable for any response). */
  Spring.prototype.step = function (dt) {
    if (this.immediate) {
      this.value = this.target;
      this.velocity = 0;
      return this;
    }
    var remaining = Math.max(0, dt);
    while (remaining > 0) {
      var h = Math.min(SUBSTEP, remaining);
      // Semi-implicit Euler: velocity first, then position.
      this.velocity += this.acceleration() * h;
      this.value += this.velocity * h;
      remaining -= h;
    }
    return this;
  };

  Spring.prototype.settled = function (epsilon) {
    var e = epsilon === undefined ? 0.1 : epsilon;
    return Math.abs(this.value - this.target) < e &&
      Math.abs(this.velocity) < e * 10;
  };

  /** Run to rest, returning per-frame samples — used by the checks. */
  Spring.prototype.trace = function (dt, maxSeconds) {
    var frames = [];
    var t = 0;
    var limit = maxSeconds || 5;
    while (t < limit) {
      frames.push({ t: t, value: this.value, velocity: this.velocity });
      if (this.settled()) break;
      this.step(dt);
      t += dt;
    }
    return frames;
  };

  var API = { Spring: Spring, SUBSTEP: SUBSTEP };

  if (typeof module !== 'undefined' && module.exports) module.exports = API;
  root.WheelSpring = API;
})(typeof globalThis !== 'undefined' ? globalThis : this);
