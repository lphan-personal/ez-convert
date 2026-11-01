/* EZ Convert — window wiring (DOM + IPC). Pure decisions live in logic.js.
   No framework, no bundler: the webview loads this file directly. */
(function () {
  'use strict';

  var logic = window.EZLogic;
  var doc = document;

  // ------------------------------------------------------------------ IPC ---
  function tauriInvoke(cmd, args) {
    return window.__TAURI__.core.invoke(cmd, args);
  }

  function createApi() {
    if (window.__EZ_MOCK__) return window.__EZ_MOCK__;
    if (!(window.__TAURI__ && window.__TAURI__.core)) return null;

    return {
      settings_view: function () { return tauriInvoke('settings_view'); },
      settings_apply: function (key, value) {
        return tauriInvoke('settings_apply', { key: key, value: value });
      },
      settings_apply_many: function (edits) {
        return tauriInvoke('settings_apply_many', { edits: edits });
      },
      settings_reload: function () { return tauriInvoke('settings_reload'); },
      queue_add_paths: function (paths) {
        return tauriInvoke('queue_add_paths', { paths: paths });
      },
      queue_snapshot: function () { return tauriInvoke('queue_snapshot'); },
      queue_set_target_all: function (target) {
        return tauriInvoke('queue_set_target_all', { target: target });
      },
      queue_set_target: function (id, target) {
        return tauriInvoke('queue_set_target', { id: id, target: target });
      },
      queue_set_strip: function (id, strip) {
        return tauriInvoke('queue_set_strip', { id: id, strip: strip });
      },
      queue_set_strip_all: function (strip) {
        return tauriInvoke('queue_set_strip_all', { strip: strip });
      },
      queue_remove: function (id) { return tauriInvoke('queue_remove', { id: id }); },
      queue_clear: function () { return tauriInvoke('queue_clear'); },
      undo_last: function () { return tauriInvoke('undo_last'); },
      app_info: function () { return tauriInvoke('app_info'); },
      dialog_open: function (options) {
        return tauriInvoke('plugin:dialog|open', { options: options });
      },
      on_drag_drop: function (handler) {
        var webview = window.__TAURI__.webview;
        if (!webview) return;
        webview.getCurrentWebview().onDragDropEvent(function (event) {
          var payload = event && event.payload;
          if (payload && payload.type === 'drop' && payload.paths && payload.paths.length) {
            handler(payload.paths);
          }
        });
      }
    };
  }

  var api = createApi();
  var state = {
    view: null,
    queue: null,
    admission: null,
    info: null,
    toast: { timer: null, interval: null }
  };

  function el(id) { return doc.getElementById(id); }

  function setNotice(text) {
    var node = el('admission');
    node.hidden = !text;
    node.textContent = text || '';
  }

  // ------------------------------------------------------------- settings ---
  function controlValue(key) {
    return logic.getPath(state.view && state.view.settings, key);
  }

  function renderSettings(view) {
    var host = el('settings-groups');
    host.textContent = '';
    var groups = logic.groupControls(view.controls || []);

    groups.forEach(function (group) {
      var fieldset = doc.createElement('fieldset');
      var legend = doc.createElement('legend');
      legend.textContent = group.group;
      fieldset.appendChild(legend);

      if (group.group === 'Where files go') {
        var summary = doc.createElement('div');
        summary.className = 'note';
        summary.id = 'rule-summary';
        summary.textContent = logic.outputRuleSummary(view.settings);
        fieldset.appendChild(summary);
      }

      group.controls.forEach(function (control) {
        fieldset.appendChild(renderControl(control));
      });

      if (group.group === 'Feedback') {
        fieldset.appendChild(soundRow());
      }

      host.appendChild(fieldset);
    });
  }

  function renderControl(control) {
    var wrap = doc.createElement('div');
    wrap.className = 'control';
    wrap.setAttribute('data-key', control.key);

    var meta = doc.createElement('div');
    meta.className = 'meta';
    var label = doc.createElement('div');
    label.className = 'label';
    label.id = 'label-' + control.key;
    label.textContent = control.label;
    meta.appendChild(label);
    var help = doc.createElement('div');
    help.className = 'help';
    help.textContent = control.help;
    meta.appendChild(help);
    var spec = doc.createElement('div');
    spec.className = 'spec';
    spec.textContent = control.spec;
    meta.appendChild(spec);
    wrap.appendChild(meta);

    var field = doc.createElement('div');
    field.className = 'field';
    var value = controlValue(control.key);

    if (control.kind === 'bool') {
      field.appendChild(switchControl(control, value === true, false));
    } else if (control.kind === 'enum') {
      var select = doc.createElement('select');
      select.id = 'ctl-' + control.key;
      select.setAttribute('aria-labelledby', 'label-' + control.key);
      (control.options || []).forEach(function (option) {
        var node = doc.createElement('option');
        node.value = option;
        node.textContent = option.replace(/_/g, ' ');
        if (option === value) node.selected = true;
        select.appendChild(node);
      });
      select.addEventListener('change', function () {
        applySetting(control.key, select.value);
      });
      field.appendChild(select);
    } else if (control.kind === 'int') {
      var num = doc.createElement('input');
      num.type = 'number';
      num.id = 'ctl-' + control.key;
      num.setAttribute('aria-labelledby', 'label-' + control.key);
      if (control.min !== undefined && control.min !== null) num.min = control.min;
      if (control.max !== undefined && control.max !== null) num.max = control.max;
      num.value = value === null || value === undefined ? '' : String(value);
      num.addEventListener('change', function () {
        applySetting(control.key, Number(num.value));
      });
      field.appendChild(num);
      if (control.unit) {
        var unit = doc.createElement('span');
        unit.className = 'unit';
        unit.textContent = control.unit;
        field.appendChild(unit);
      }
    } else if (control.kind === 'path') {
      var input = doc.createElement('input');
      input.type = 'text';
      input.id = 'ctl-' + control.key;
      input.setAttribute('aria-labelledby', 'label-' + control.key);
      input.placeholder = 'Absolute folder path';
      input.value = value || '';
      input.addEventListener('change', function () {
        applySetting(control.key, input.value.trim() === '' ? null : input.value.trim());
      });
      field.appendChild(input);
      var browse = doc.createElement('button');
      browse.type = 'button';
      browse.className = 'ghost';
      browse.textContent = 'Browse…';
      browse.addEventListener('click', function () {
        pickFolder().then(function (picked) {
          if (picked) applySetting(control.key, picked);
        });
      });
      field.appendChild(browse);
    } else if (control.kind === 'text') {
      var text = doc.createElement('input');
      text.type = 'text';
      text.id = 'ctl-' + control.key;
      text.setAttribute('aria-labelledby', 'label-' + control.key);
      text.value = value || '';
      text.addEventListener('change', function () {
        applySetting(control.key, text.value);
      });
      field.appendChild(text);
    } else if (control.kind === 'target_list') {
      if (control.key === 'wheel.order') {
        field.appendChild(wheelOrderSummary(control));
      } else {
        field.appendChild(wheelEditor(control));
      }
    } else if (control.kind === 'locked') {
      var locked = switchControl(control, true, true);
      field.appendChild(locked);
      var note = doc.createElement('span');
      note.className = 'locked';
      note.textContent = 'locked by the spec';
      field.appendChild(note);
    } else {
      var readout = doc.createElement('span');
      readout.className = 'badge';
      readout.id = 'ctl-' + control.key;
      readout.textContent = String(value === null || value === undefined ? 'unknown' : value);
      field.appendChild(readout);
    }

    wrap.appendChild(field);
    return wrap;
  }

  function switchControl(control, checked, disabled) {
    var label = doc.createElement('label');
    label.className = 'switch';
    var input = doc.createElement('input');
    input.type = 'checkbox';
    input.checked = !!checked;
    input.disabled = !!disabled;
    input.id = 'ctl-' + control.key;
    input.setAttribute('aria-labelledby', 'label-' + control.key);
    if (!disabled) {
      input.addEventListener('change', function () {
        applySetting(control.key, input.checked);
      });
    }
    var text = doc.createElement('span');
    text.textContent = '';
    label.appendChild(input);
    label.appendChild(text);
    return label;
  }

  // ------------------------------------------------------- wheel layout -----
  function wheelEditor(control) {
    var wrap = doc.createElement('div');
    wrap.className = 'wheel-editor';
    wrap.id = 'wheel-editor';

    var wheel = controlValue('wheel') || { segments: [], order: [] };
    var order = wheel.order || wheel.segments || [];

    var slots = doc.createElement('div');
    slots.className = 'slots';
    slots.id = 'wheel-slots';
    order.forEach(function (format, index) {
      var slot = doc.createElement('span');
      slot.className = 'slot';
      slot.setAttribute('data-format', format);
      var num = doc.createElement('span');
      num.className = 'index';
      num.textContent = String(index + 1);
      slot.appendChild(num);
      var name = doc.createElement('span');
      name.textContent = format;
      slot.appendChild(name);

      var up = doc.createElement('button');
      up.type = 'button';
      up.textContent = '↑';
      up.title = 'Move earlier';
      up.addEventListener('click', function () { moveWheel(format, -1); });
      slot.appendChild(up);

      var down = doc.createElement('button');
      down.type = 'button';
      down.textContent = '↓';
      down.title = 'Move later';
      down.addEventListener('click', function () { moveWheel(format, 1); });
      slot.appendChild(down);

      var off = doc.createElement('button');
      off.type = 'button';
      off.textContent = '×';
      off.title = 'Remove from the wheel';
      off.addEventListener('click', function () { toggleWheel(format, false); });
      slot.appendChild(off);

      slots.appendChild(slot);
    });
    wrap.appendChild(slots);

    var add = doc.createElement('div');
    add.className = 'add';
    var select = doc.createElement('select');
    select.id = 'wheel-add';
    select.setAttribute('aria-label', 'Add a format to the wheel');
    (control.options || []).forEach(function (option) {
      if (order.indexOf(option) !== -1) return;
      var node = doc.createElement('option');
      node.value = option;
      node.textContent = option;
      select.appendChild(node);
    });
    add.appendChild(select);
    var button = doc.createElement('button');
    button.type = 'button';
    button.className = 'ghost';
    button.id = 'wheel-add-button';
    button.textContent = 'Add segment';
    button.addEventListener('click', function () {
      if (select.value) toggleWheel(select.value, true);
    });
    add.appendChild(button);
    wrap.appendChild(add);

    var note = doc.createElement('div');
    note.className = 'note' + (order.length >= logic.WHEEL_CAP ? ' warn' : '');
    note.id = 'wheel-note';
    note.textContent = order.length + ' of ' + logic.WHEEL_CAP +
      ' segments' + (order.length >= logic.WHEEL_CAP ? ' — the wheel never shrinks below 6 slots' : '');
    wrap.appendChild(note);
    return wrap;
  }

  function wheelOrderSummary(control) {
    var wrap = doc.createElement('div');
    wrap.className = 'wheel-editor';
    var wheel = controlValue('wheel') || { order: [], segments: [] };
    var order = wheel.order || wheel.segments || [];
    var slots = doc.createElement('div');
    slots.className = 'slots';
    slots.id = 'wheel-order';
    order.forEach(function (format, index) {
      var slot = doc.createElement('span');
      slot.className = 'slot';
      slot.setAttribute('data-format', format);
      var num = doc.createElement('span');
      num.className = 'index';
      num.textContent = String(index + 1);
      slot.appendChild(num);
      var name = doc.createElement('span');
      name.textContent = format;
      slot.appendChild(name);
      slots.appendChild(slot);
    });
    wrap.appendChild(slots);
    var note = doc.createElement('div');
    note.className = 'note';
    note.textContent = 'Set by the wheel editor above; clockwise from the top.';
    wrap.appendChild(note);
    return wrap;
  }

  function toggleWheel(format, present) {
    var wheel = controlValue('wheel') || { segments: [], order: [] };
    var result = logic.editWheel(wheel, format, present, logic.WHEEL_CAP);
    if (!result.ok) {
      setNotice(result.reason);
      return;
    }
    applyWheel(result.wheel);
  }

  function moveWheel(format, delta) {
    var wheel = controlValue('wheel') || { segments: [], order: [] };
    applyWheel({
      segments: wheel.segments,
      order: logic.moveSegment(wheel.order || wheel.segments, format, delta)
    });
  }

  function applyWheel(wheel) {
    api.settings_apply_many([
      ['wheel.segments', wheel.segments],
      ['wheel.order', wheel.order]
    ]).then(function (view) {
      state.view = view;
      renderSettings(view);
    }).catch(function (err) {
      setNotice(String(err));
    });
  }

  // ------------------------------------------------------------ sound ------
  function soundRow() {
    var wrap = doc.createElement('div');
    wrap.className = 'control';
    var meta = doc.createElement('div');
    meta.className = 'meta';
    var label = doc.createElement('div');
    label.className = 'label';
    label.textContent = 'Test the sound';
    meta.appendChild(label);
    var help = doc.createElement('div');
    help.className = 'help';
    help.textContent = 'Plays the completion chime, if it is turned on.';
    meta.appendChild(help);
    wrap.appendChild(meta);

    var field = doc.createElement('div');
    field.className = 'field';
    var button = doc.createElement('button');
    button.type = 'button';
    button.id = 'sound-test';
    button.textContent = 'Play';
    button.addEventListener('click', function () {
      var on = playChime();
      setNotice(on ? 'Chime played.' : 'Sound is off, so nothing played.');
    });
    field.appendChild(button);
    wrap.appendChild(field);
    return wrap;
  }

  function playChime() {
    var settings = state.view && state.view.settings;
    if (!settings || !settings.sound || !settings.sound.enabled) return false;
    try {
      var Ctor = window.AudioContext || window.webkitAudioContext;
      if (!Ctor) return false;
      var ctx = new Ctor();
      var osc = ctx.createOscillator();
      var gain = ctx.createGain();
      osc.type = 'sine';
      osc.frequency.value = 660;
      gain.gain.setValueAtTime(0.0001, ctx.currentTime);
      gain.gain.exponentialRampToValueAtTime(0.08, ctx.currentTime + 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + 0.28);
      osc.connect(gain).connect(ctx.destination);
      osc.start();
      osc.stop(ctx.currentTime + 0.3);
      return true;
    } catch (err) {
      return false;
    }
  }

  // ------------------------------------------------------------ settings ---
  function applySetting(key, value) {
    return api.settings_apply(key, value).then(function (view) {
      state.view = view;
      renderSettings(view);
      renderRepairs(view.repairs);
      return refreshQueue();
    }).catch(function (err) {
      setNotice(String(err));
    });
  }

  function pickFolder() {
    if (!api.dialog_open) return Promise.resolve(null);
    return api.dialog_open({ directory: true, multiple: false, title: 'Choose a folder' })
      .then(function (chosen) {
        if (Array.isArray(chosen)) return chosen[0] || null;
        return chosen || null;
      })
      .catch(function () { return null; });
  }

  // --------------------------------------------------------------- queue ---
  function refreshQueue() {
    return api.queue_snapshot().then(function (snapshot) {
      state.queue = snapshot;
      renderQueue(snapshot);
    });
  }

  function renderQueue(snapshot) {
    var items = (snapshot && snapshot.items) || [];
    var body = el('queue-body');
    body.textContent = '';

    el('queue-count').textContent = items.length === 1 ? '1 file' : items.length + ' files';
    el('queue-empty').hidden = items.length > 0;

    items.forEach(function (item) {
      body.appendChild(queueRow(item));
    });

    var progress = logic.progressModel(snapshot);
    el('progress-fill').style.width = progress.percent + '%';
    el('progress-label').textContent = progress.label;

    var batch = doc.getElementById('batch-strip');
    if (batch) {
      var anyStrip = items.some(function (item) { return item.strip; });
      batch.checked = anyStrip;
    }

    var rule = el('rule-summary');
    if (rule && state.view) rule.textContent = logic.outputRuleSummary(state.view.settings);

    renderToast(snapshot);
  }

  function queueRow(item) {
    var tr = doc.createElement('tr');
    tr.setAttribute('data-id', String(item.id));
    tr.setAttribute('data-state', item.state);

    var file = doc.createElement('td');
    var fileBox = doc.createElement('div');
    fileBox.className = 'file';
    var name = doc.createElement('span');
    name.className = 'name';
    name.textContent = item.name;
    fileBox.appendChild(name);
    var path = doc.createElement('span');
    path.className = 'path';
    path.textContent = item.dir + '  ·  ' + item.sizeText;
    fileBox.appendChild(path);
    file.appendChild(fileBox);
    tr.appendChild(file);

    var detected = doc.createElement('td');
    detected.textContent = item.detected;
    tr.appendChild(detected);

    var target = doc.createElement('td');
    var select = doc.createElement('select');
    select.setAttribute('aria-label', 'Target for ' + item.name);
    var none = doc.createElement('option');
    none.value = '';
    none.textContent = '—';
    select.appendChild(none);
    ((state.info && state.info.targetFormats) || []).forEach(function (format) {
      var option = doc.createElement('option');
      option.value = format;
      option.textContent = format;
      if (format === item.target) option.selected = true;
      select.appendChild(option);
    });
    select.addEventListener('change', function () {
      api.queue_set_target(item.id, select.value || null).then(function (snapshot) {
        state.queue = snapshot;
        renderQueue(snapshot);
      });
    });
    target.appendChild(select);
    tr.appendChild(target);

    var out = doc.createElement('td');
    var outBox = doc.createElement('div');
    outBox.className = 'out';
    outBox.textContent = item.previewOut ? item.previewOut : 'choose a target';
    if (item.renamed) {
      var flag = doc.createElement('span');
      flag.className = 'renamed';
      flag.textContent = ' · renamed to avoid overwrite';
      outBox.appendChild(flag);
    }
    out.appendChild(outBox);
    tr.appendChild(out);

    var status = doc.createElement('td');
    var badge = doc.createElement('span');
    badge.className = 'badge ' + item.state;
    badge.textContent = item.stateDetail ? item.state + ' · ' + item.stateDetail : item.state;
    status.appendChild(badge);
    tr.appendChild(status);

    var actions = doc.createElement('td');
    var row = doc.createElement('div');
    row.className = 'row-actions';

    var stripLabel = doc.createElement('label');
    stripLabel.className = 'switch';
    stripLabel.title = 'Strip metadata for this file';
    var strip = doc.createElement('input');
    strip.type = 'checkbox';
    strip.checked = !!item.strip;
    strip.addEventListener('change', function () {
      api.queue_set_strip(item.id, strip.checked).then(function (snapshot) {
        state.queue = snapshot;
        renderQueue(snapshot);
      });
    });
    stripLabel.appendChild(strip);
    row.appendChild(stripLabel);

    var remove = doc.createElement('button');
    remove.type = 'button';
    remove.className = 'ghost icon-btn';
    remove.textContent = '✕';
    remove.title = 'Remove from the queue';
    remove.addEventListener('click', function () {
      api.queue_remove(item.id).then(function (snapshot) {
        state.queue = snapshot;
        renderQueue(snapshot);
      });
    });
    row.appendChild(remove);
    actions.appendChild(row);
    tr.appendChild(actions);

    return tr;
  }

  function renderAdmission(report) {
    var node = el('admission');
    if (!report) {
      node.hidden = true;
      return;
    }
    node.textContent = '';
    var head = doc.createElement('div');
    head.textContent = report.added + ' added' +
      (report.foldersExpanded ? ' · ' + report.foldersExpanded + ' folder(s) read' : '') +
      (report.skipped.length ? ' · ' + report.skipped.length + ' skipped' : '');
    node.appendChild(head);
    if (report.skipped.length) {
      var list = doc.createElement('ul');
      report.skipped.forEach(function (entry) {
        var li = doc.createElement('li');
        li.textContent = entry.path + ' — ' + entry.reason;
        list.appendChild(li);
      });
      node.appendChild(list);
    }
    node.hidden = false;
  }

  // --------------------------------------------------------------- toast ---
  function renderToast(snapshot) {
    if (!state.view) return;
    var model = logic.toastModel(snapshot, state.view.settings, state.view.settings.undo.window_seconds);
    var toast = el('toast');
    if (!model.visible) {
      hideToast();
      return;
    }
    if (state.toast.shownFor === snapshot.progress.total) return;
    state.toast.shownFor = snapshot.progress.total;

    toast.hidden = false;
    el('toast-title').textContent = model.title;
    el('toast-detail').textContent = model.detail;
    el('toast-undo').textContent = model.undoLabel;

    if (model.sound) playChime();

    var remaining = model.undoSeconds;
    clearInterval(state.toast.interval);
    state.toast.interval = setInterval(function () {
      remaining -= 1;
      if (remaining <= 0) {
        clearInterval(state.toast.interval);
        hideToast();
        return;
      }
      el('toast-undo').textContent = 'Undo (' + remaining + 's)';
    }, 1000);
  }

  function hideToast() {
    clearInterval(state.toast.interval);
    state.toast.shownFor = null;
    el('toast').hidden = true;
  }

  function wireToast() {
    el('toast-undo').addEventListener('click', function () {
      api.undo_last().then(function (outcome) {
        el('toast-detail').textContent = outcome.message;
        el('toast-undo').disabled = true;
        clearInterval(state.toast.interval);
        setTimeout(hideToast, 4000);
      }).catch(function (err) {
        el('toast-detail').textContent = String(err);
      });
    });

    el('toast-show').addEventListener('click', function () {
      var table = doc.querySelector('.tablewrap');
      if (table && table.scrollIntoView) table.scrollIntoView({ behavior: 'smooth', block: 'center' });
      var rows = doc.querySelectorAll('#queue-body tr[data-state="done"]');
      Array.prototype.forEach.call(rows, function (row) {
        if (typeof row.animate !== 'function') return;
        row.animate(
          [{ background: 'rgba(95,211,141,0.22)' }, { background: 'transparent' }],
          { duration: 1200, easing: 'ease-out' }
        );
      });
      hideToast();
    });
  }

  // ---------------------------------------------------------- diagnostics --
  function renderInfo(info) {
    var list = el('diag-list');
    list.textContent = '';
    if (!info) return;
    var rows = [
      ['Version', info.version],
      ['Settings file', info.settingsPath],
      ['Config folder', info.configDir],
      ['Formats offered', String((info.targetFormats || []).length)],
      ['Uptime', info.uptimeSeconds + 's']
    ];
    rows.forEach(function (pair) {
      var dt = doc.createElement('dt');
      dt.textContent = pair[0];
      var dd = doc.createElement('dd');
      dd.textContent = pair[1];
      list.appendChild(dt);
      list.appendChild(dd);
    });
  }

  function renderRepairs(repairs) {
    var node = el('repairs');
    if (!repairs || !repairs.length) {
      node.hidden = true;
      return;
    }
    node.textContent = '';
    var head = doc.createElement('div');
    head.textContent = 'Settings repaired on load (kept in memory, not shown twice):';
    node.appendChild(head);
    var list = doc.createElement('ul');
    repairs.forEach(function (line) {
      var li = doc.createElement('li');
      li.textContent = line;
      list.appendChild(li);
    });
    node.appendChild(list);
    node.hidden = false;
  }

  // ------------------------------------------------------------ intake -----
  function handleDroppedPaths(paths) {
    if (!paths || !paths.length) return Promise.resolve();
    return api.queue_add_paths(paths).then(function (result) {
      state.queue = result.queue;
      state.admission = result.report;
      renderQueue(result.queue);
      renderAdmission(result.report);
    }).catch(function (err) {
      setNotice(String(err));
    });
  }

  function wireDrop() {
    var zone = el('dropzone');

    ['dragenter', 'dragover'].forEach(function (name) {
      zone.addEventListener(name, function (event) {
        event.preventDefault();
        zone.classList.add('over');
      });
    });
    ['dragleave', 'dragend'].forEach(function (name) {
      zone.addEventListener(name, function () { zone.classList.remove('over'); });
    });
    zone.addEventListener('drop', function (event) {
      event.preventDefault();
      zone.classList.remove('over');
      handleDroppedPaths(logic.pathsFromDataTransfer(event.dataTransfer));
    });

    doc.addEventListener('dragover', function (event) { event.preventDefault(); });
    doc.addEventListener('drop', function (event) {
      if (zone.contains(event.target)) return;
      event.preventDefault();
      handleDroppedPaths(logic.pathsFromDataTransfer(event.dataTransfer));
    });

    // Tauri's native drag-drop channel (the webview intercepts OS drops).
    if (api.on_drag_drop) api.on_drag_drop(handleDroppedPaths);
  }

  function wireBatchBar() {
    el('target-select').addEventListener('change', function () {
      var select = el('target-select');
      if (!select.value) return;
      api.queue_set_target_all(select.value).then(function (snapshot) {
        state.queue = snapshot;
        renderQueue(snapshot);
      });
    });

    el('batch-strip').addEventListener('change', function () {
      api.queue_set_strip_all(el('batch-strip').checked).then(function (snapshot) {
        state.queue = snapshot;
        renderQueue(snapshot);
      });
    });

    el('clear').addEventListener('click', function () {
      api.queue_clear().then(function (snapshot) {
        state.queue = snapshot;
        state.admission = null;
        renderQueue(snapshot);
        renderAdmission(null);
      });
    });

    el('add-files').addEventListener('click', function () {
      if (!api.dialog_open) return setNotice('File picking needs the desktop app.');
      api.dialog_open({ multiple: true, directory: false, title: 'Add files' })
        .then(function (chosen) {
          var paths = chosen == null ? [] : (Array.isArray(chosen) ? chosen : [chosen]);
          return handleDroppedPaths(paths);
        })
        .catch(function (err) { setNotice(String(err)); });
    });

    el('add-folder').addEventListener('click', function () {
      if (!api.dialog_open) return setNotice('Folder picking needs the desktop app.');
      api.dialog_open({ multiple: true, directory: true, title: 'Add folders' })
        .then(function (chosen) {
          var paths = chosen == null ? [] : (Array.isArray(chosen) ? chosen : [chosen]);
          return handleDroppedPaths(paths);
        })
        .catch(function (err) { setNotice(String(err)); });
    });
  }

  function fillTargets(info) {
    var select = el('target-select');
    (info.targetFormats || []).forEach(function (format) {
      var option = doc.createElement('option');
      option.value = format;
      option.textContent = format;
      select.appendChild(option);
    });
  }

  // --------------------------------------------------------------- boot ----
  function boot() {
    if (!api) {
      setNotice('Open EZ Convert as an app: the window needs the local backend.');
      return Promise.resolve(state);
    }
    wireBatchBar();
    wireToast();
    wireDrop();

    return api.settings_view()
      .then(function (view) {
        state.view = view;
        renderSettings(view);
        renderRepairs(view.repairs);
        return api.app_info();
      })
      .then(function (info) {
        state.info = info;
        fillTargets(info);
        renderInfo(info);
        return api.queue_snapshot();
      })
      .then(function (snapshot) {
        state.queue = snapshot;
        renderQueue(snapshot);
        return state;
      })
      .catch(function (err) {
        setNotice('Startup failed: ' + err);
        return state;
      });
  }

  // Exposed for the UI test harness (and for the wheel window in U3).
  window.__ezApp = {
    boot: boot,
    handleDroppedPaths: handleDroppedPaths,
    applySetting: applySetting,
    refreshQueue: refreshQueue,
    playChime: playChime,
    state: state,
    api: api
  };

  if (doc.readyState === 'loading') {
    doc.addEventListener('DOMContentLoaded', function () {
      // `__ezBootPromise` lets the test harness await the first render instead
      // of polling. Harmless in the app.
      window.__ezBootPromise = boot();
    });
  } else {
    window.__ezBootPromise = boot();
  }
})();
