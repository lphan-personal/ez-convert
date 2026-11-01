/* Pure window logic: no DOM, no IPC, no globals beyond `module.exports` when
   running under node. Loaded in the webview by index.html and by
   `ui/tests/logic.test.js` under node:test. */
(function (root) {
  'use strict';

  /** Read a dotted key out of a settings object. */
  function getPath(object, key) {
    if (!object) return undefined;
    return key.split('.').reduce(function (acc, part) {
      return acc == null ? undefined : acc[part];
    }, object);
  }

  /**
   * Group controls into the fieldset order the schema declares.
   * Returns [{ group, controls }] preserving first-use order.
   */
  function groupControls(controls) {
    var groups = [];
    var index = {};
    (controls || []).forEach(function (control) {
      if (!(control.group in index)) {
        index[control.group] = groups.length;
        groups.push({ group: control.group, controls: [] });
      }
      groups[index[control.group]].controls.push(control);
    });
    return groups;
  }

  /** Human summary of the SPEC §5 output rules currently in force. */
  function outputRuleSummary(settings) {
    var output = (settings && settings.output) || {};
    var parts = [];
    if (output.location === 'chosen_folder' && output.chosen_folder) {
      parts.push('Results go to ' + output.chosen_folder + '.');
    } else {
      parts.push('Results land beside the originals.');
    }
    var threshold = Number(output.batch_threshold || 3);
    if (output.use_batch_subfolder === false) {
      parts.push('No batch folder — every result stays next to its original.');
    } else {
      parts.push(threshold + '+ files go to ' + (output.batch_subfolder || 'converted') + '/.');
    }
    parts.push('Existing files are never overwritten (-1, -2…).');
    return parts.join(' ');
  }

  /**
   * Add or remove a wheel segment while keeping `order` a permutation.
   * `cap` is the SPEC.md §2 hard cap of 6.
   */
  function editWheel(wheel, format, present, cap) {
    var limit = cap || 6;
    var segments = (wheel.segments || []).slice();
    var order = (wheel.order || []).slice();
    if (present) {
      if (segments.indexOf(format) !== -1) {
        return { ok: false, reason: 'already on the wheel', wheel: wheel };
      }
      if (segments.length >= limit) {
        return { ok: false, reason: 'the wheel holds ' + limit + ' segments at most', wheel: wheel };
      }
      segments.push(format);
      order.push(format);
    } else {
      segments = segments.filter(function (s) { return s !== format; });
      order = order.filter(function (s) { return s !== format; });
      if (segments.length === 0) {
        return { ok: false, reason: 'keep at least one segment', wheel: wheel };
      }
    }
    return { ok: true, wheel: { segments: segments, order: order } };
  }

  /** Move a segment one slot earlier (-1) or later (+1) in the wheel order. */
  function moveSegment(order, format, delta) {
    var next = (order || []).slice();
    var from = next.indexOf(format);
    if (from === -1) return next;
    var to = from + delta;
    if (to < 0 || to >= next.length) return next;
    next.splice(from, 1);
    next.splice(to, 0, format);
    return next;
  }

  /** Progress model for the bar: percentage plus a label. */
  function progressModel(snapshot) {
    var progress = (snapshot && snapshot.progress) || {};
    var total = Number(progress.total || 0);
    var percent = total === 0 ? 0 : Math.max(0, Math.min(100, Number(progress.percent || 0)));
    return {
      percent: percent,
      label: progress.label || (total === 0 ? 'Nothing queued' : '0 of ' + total + ' finished'),
      complete: total > 0 && percent >= 100
    };
  }

  /**
   * Toast model for the completion surface (SPEC.md §5: toast + Show +
   * 10-second Undo; sound follows the `sound.enabled` setting).
   */
  function toastModel(snapshot, settings, undoSeconds) {
    var progress = (snapshot && snapshot.progress) || {};
    var total = Number(progress.total || 0);
    var done = Number(progress.done || 0);
    var failed = Number(progress.failed || 0) + Number(progress.skipped || 0);
    var complete = total > 0 && Number(progress.percent || 0) >= 100;
    var seconds = Number(undoSeconds || (settings && settings.undo && settings.undo.window_seconds) || 10);
    var detail = failed > 0
      ? done + ' converted · ' + failed + " didn't"
      : done + ' file' + (done === 1 ? '' : 's') + ' converted';
    return {
      visible: complete,
      title: complete ? (failed > 0 ? 'Finished with problems' : 'Conversion complete') : '',
      detail: detail,
      undoLabel: 'Undo (' + seconds + 's)',
      undoSeconds: seconds,
      sound: !!(settings && settings.sound && settings.sound.enabled)
    };
  }

  /** Text for a control's current value, used in tests and aria labels. */
  function describeValue(control, value) {
    if (control.kind === 'bool') return value ? 'on' : 'off';
    if (control.kind === 'target_list') return (value || []).join(' → ');
    if (value === null || value === undefined) return 'not set';
    return String(value);
  }

  /** Turn a DataTransfer-like object into absolute paths, best effort. */
  function pathsFromDataTransfer(dataTransfer) {
    if (!dataTransfer) return [];
    var files = dataTransfer.files;
    if (files && files.length) {
      return Array.prototype.map
        .call(files, function (file) { return file.path || file.name; })
        .filter(Boolean);
    }
    var raw = '';
    if (typeof dataTransfer.getData === 'function') {
      raw = dataTransfer.getData('text/uri-list') || dataTransfer.getData('text/plain') || '';
    }
    return raw
      .split(/\r?\n/)
      .map(function (line) { return line.trim().replace(/^file:\/\//, ''); })
      .filter(function (line) { return line.length > 0 && line.charAt(0) !== '#'; });
  }

  var api = {
    getPath: getPath,
    groupControls: groupControls,
    outputRuleSummary: outputRuleSummary,
    editWheel: editWheel,
    moveSegment: moveSegment,
    progressModel: progressModel,
    toastModel: toastModel,
    describeValue: describeValue,
    pathsFromDataTransfer: pathsFromDataTransfer,
    WHEEL_CAP: 6
  };

  if (typeof module !== 'undefined' && module && module.exports) {
    module.exports = api;
  }
  if (root) {
    root.EZLogic = api;
  }
})(typeof window !== 'undefined' ? window : null);
