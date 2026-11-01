// Window logic tests — no dependencies, run with `node --test ui/tests`.
//
// These cover the pure decisions in ui/logic.js and the DOM contract in
// ui/index.html. The interaction test (clicking a generated control and
// watching behaviour change) needs a DOM; it runs in the card's evidence
// harness with jsdom and is not committed here because the repo keeps the UI
// build-step-free (README: "no node/webview build step").

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const logic = require('../logic.js');

const UI = path.join(__dirname, '..');
const indexHtml = fs.readFileSync(path.join(UI, 'index.html'), 'utf8');

test('settings controls are grouped in schema order without dropping any', () => {
  const controls = [
    { key: 'a.one', group: 'Output' },
    { key: 'b.two', group: 'Wheel' },
    { key: 'a.three', group: 'Output' }
  ];
  const groups = logic.groupControls(controls);
  assert.deepEqual(groups.map((g) => g.group), ['Output', 'Wheel']);
  assert.deepEqual(groups[0].controls.map((c) => c.key), ['a.one', 'a.three']);
  assert.equal(groups.reduce((n, g) => n + g.controls.length, 0), controls.length);
});

test('SPEC §5 output rules read back as plain language', () => {
  const settings = {
    output: {
      location: 'same_folder',
      chosen_folder: null,
      batch_threshold: 3,
      batch_subfolder: 'converted',
      use_batch_subfolder: true
    }
  };
  const summary = logic.outputRuleSummary(settings);
  assert.match(summary, /beside the originals/);
  assert.match(summary, /3\+ files go to converted\//);
  assert.match(summary, /never overwritten/);

  const off = logic.outputRuleSummary({
    output: { location: 'same_folder', batch_threshold: 3, use_batch_subfolder: false }
  });
  assert.match(off, /No batch folder/);

  const chosen = logic.outputRuleSummary({
    output: { location: 'chosen_folder', chosen_folder: '/Users/ryo/out' }
  });
  assert.match(chosen, /\/Users\/ryo\/out/);
});

test('the wheel hard cap of 6 is enforced, not rendered smaller', () => {
  let wheel = { segments: ['png', 'jpg', 'pdf', 'webp', 'mp3', 'mp4'], order: [] };
  const full = logic.editWheel(wheel, 'gif', true, 6);
  assert.equal(full.ok, false);
  assert.match(full.reason, /6 segments at most/);

  wheel = { segments: ['png'], order: ['png'] };
  const added = logic.editWheel(wheel, 'jpg', true, 6);
  assert.equal(added.ok, true);
  assert.deepEqual(added.wheel.segments, ['png', 'jpg']);
  assert.deepEqual(added.wheel.order, ['png', 'jpg']);

  const removed = logic.editWheel(added.wheel, 'png', false, 6);
  assert.deepEqual(removed.wheel.segments, ['jpg']);
  assert.deepEqual(removed.wheel.order, ['jpg']);

  const empty = logic.editWheel({ segments: ['png'], order: ['png'] }, 'png', false, 6);
  assert.equal(empty.ok, false, 'never leaves the wheel with no segments');
});

test('moving a segment keeps every other segment and its order', () => {
  const order = ['png', 'jpg', 'pdf'];
  assert.deepEqual(logic.moveSegment(order, 'pdf', -1), ['png', 'pdf', 'jpg']);
  assert.deepEqual(logic.moveSegment(order, 'png', 1), ['jpg', 'png', 'pdf']);
  assert.deepEqual(logic.moveSegment(order, 'png', -1), order, 'first stays first');
  assert.deepEqual(logic.moveSegment(order, 'pdf', 1), order, 'last stays last');
  assert.deepEqual(logic.moveSegment(order, 'nope', 1), order);
});

test('progress is a percentage of terminal work with a readable label', () => {
  assert.deepEqual(logic.progressModel({ progress: { total: 0 } }), {
    percent: 0,
    label: 'Nothing queued',
    complete: false
  });
  const half = logic.progressModel({ progress: { total: 4, percent: 50, label: '2 of 4 finished' } });
  assert.equal(half.percent, 50);
  assert.equal(half.complete, false);
  const done = logic.progressModel({ progress: { total: 3, percent: 100, label: '3 of 3 converted' } });
  assert.equal(done.complete, true);
  assert.equal(done.percent, 100);
});

test('toast offers Show + a counting Undo and follows the sound setting', () => {
  const snapshot = {
    progress: { total: 3, done: 3, failed: 0, skipped: 0, percent: 100 }
  };
  const on = logic.toastModel(snapshot, { sound: { enabled: true }, undo: { window_seconds: 10 } });
  assert.equal(on.visible, true);
  assert.equal(on.title, 'Conversion complete');
  assert.equal(on.undoLabel, 'Undo (10s)');
  assert.equal(on.sound, true);

  const off = logic.toastModel(snapshot, { sound: { enabled: false }, undo: { window_seconds: 4 } });
  assert.equal(off.sound, false);
  assert.equal(off.undoLabel, 'Undo (4s)');

  const partial = logic.toastModel(
    { progress: { total: 3, done: 2, failed: 1, percent: 100 } },
    { sound: { enabled: true }, undo: { window_seconds: 10 } }
  );
  assert.equal(partial.title, 'Finished with problems');
  assert.match(partial.detail, /2 converted · 1 didn't/);

  const running = logic.toastModel({ progress: { total: 3, percent: 40 } }, {});
  assert.equal(running.visible, false);
});

test('dropped paths are read from a DataTransfer-like object', () => {
  assert.deepEqual(
    logic.pathsFromDataTransfer({ files: [{ path: 'C:\\a\\b.png' }, { name: 'c.jpg' }] }),
    ['C:\\a\\b.png', 'c.jpg']
  );
  assert.deepEqual(
    logic.pathsFromDataTransfer({ getData: () => 'file:///Users/ryo/a.png\nfile:///Users/ryo/b.pdf' }),
    ['/Users/ryo/a.png', '/Users/ryo/b.pdf']
  );
  assert.deepEqual(logic.pathsFromDataTransfer(null), []);
});

test('index.html carries every anchor the window wiring owns', () => {
  const ids = [
    'dropzone', 'add-files', 'add-folder', 'queue-count', 'target-select',
    'batch-strip', 'clear', 'progress-fill', 'progress-label', 'queue-body',
    'queue-empty', 'admission', 'settings-groups', 'diag-list', 'repairs',
    'toast', 'toast-title', 'toast-detail', 'toast-show', 'toast-undo'
  ];
  for (const id of ids) {
    assert.match(indexHtml, new RegExp(`id="${id}"`), `index.html is missing #${id}`);
  }
  assert.match(indexHtml, /src="logic\.js"/);
  assert.match(indexHtml, /src="app\.js"/);
  assert.doesNotMatch(indexHtml, /<script>/, 'no inline script: CSP is default-src self');
  assert.doesNotMatch(indexHtml, /style="/, 'no inline style attributes');
});

test('every SPEC §5 rule has a visible element in the window', () => {
  // Multi-file/folder intake, output location, batch rule, never-overwrite,
  // metadata strip, undo window and sound are all reachable from the window.
  const required = [
    [/Drop files or folders/, 'intake surface'],
    [/Include subfolders/, 'folder depth control is described on the surface'],
    [/Strip metadata/, 'per-batch metadata toggle'],
    [/Convert all to/, 'batch target control'],
    [/Undo/, 'toast undo control'],
    [/Show/, 'toast show control']
  ];
  for (const [pattern, what] of required) {
    assert.match(indexHtml, pattern, `missing ${what}`);
  }
});
