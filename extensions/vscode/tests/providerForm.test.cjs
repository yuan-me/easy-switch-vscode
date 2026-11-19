const test = require('node:test');
const assert = require('node:assert/strict');
const Module = require('node:module');

async function formTest(check) {
  let receive, close, disposed = false;
  const replies = [];
  const disposable = () => ({ dispose() {} });
  const panel = { webview: { html: '', onDidReceiveMessage(fn) { receive = fn; return disposable(); }, async postMessage(value) { replies.push(value); } }, onDidDispose(fn) { close = fn; return disposable(); }, dispose() { disposed = true; close(); } };
  const old = Module._load;
  Module._load = function(name, ...args) { return name === 'vscode' ? { ViewColumn: { Active: -1 }, window: { createWebviewPanel() { return panel; } } } : old.call(this, name, ...args); };
  try {
    delete require.cache[require.resolve('../out/providerForm')];
    const { editProviderForm } = require('../out/providerForm');
    await check({ editProviderForm, panel, replies, receive: value => receive(value), close: () => close(), disposed: () => disposed });
  } finally { Module._load = old; delete require.cache[require.resolve('../out/providerForm')]; }
}

test('single-page form escapes metadata and never embeds saved secrets', () => formTest(async p => {
  const result = p.editProviderForm({ name: '<script>bad()</script>', baseUrl: 'https://example.test/v1', model: 'model', nativeFast: true }, true, async () => assert.fail('cancel saved data'));
  for (const id of ['name', 'baseUrl', 'model', 'key', 'nativeFast']) assert.ok(p.panel.webview.html.includes(`id="${id}"`));
  assert.ok(p.panel.webview.html.includes('&lt;script&gt;bad()&lt;/script&gt;'));
  assert.ok(!p.panel.webview.html.includes('<script>bad()'));
  assert.match(p.panel.webview.html, /已保存密钥，留空则不修改/);
  assert.match(p.panel.webview.html, /default-src 'none'/);
  await p.receive({ action: 'cancel' });
  assert.equal(await result, false);
}));

test('invalid messages and save errors retain the form; retry saves normalized values', () => formTest(async p => {
  let count = 0, saved;
  const result = p.editProviderForm(undefined, false, async value => { if (++count === 1) throw Error(`failed with ${value.key}`); saved = value; });
  await p.receive({ action: 'save', name: 123 });
  assert.equal(count, 0);
  const input = { action: 'save', name: ' A ', model: ' model ', baseUrl: ' https://example.test/v1/// ', key: 'synthetic-secret', nativeFast: true };
  await p.receive(input);
  assert.equal(p.disposed(), false);
  assert.equal(p.replies.at(-1).message, 'failed with [已隐藏]');
  await p.receive(input);
  assert.equal(await result, true);
  assert.deepEqual(saved, { name: 'A', model: 'model', baseUrl: 'https://example.test/v1', key: 'synthetic-secret', nativeFast: true });
}));

test('closing the editor cancels without saving', () => formTest(async p => {
  const result = p.editProviderForm(undefined, false, async () => assert.fail('saved'));
  p.close(); assert.equal(await result, false);
}));

test('model lookup can fail and retry without saving or returning the entered key to the page', () => formTest(async p => {
  const request = { action: 'fetchModels', requestId: 1, baseUrl: ' https://example.test/v1 ', key: 'synthetic' };
  let attempts = 0;
  const result = p.editProviderForm(undefined, false, async () => assert.fail('lookup saved the form'), async (url, key) => {
    assert.equal(url, request.baseUrl.trim());
    assert.equal(key, request.key);
    if (++attempts === 1) throw Error('HTTP 401');
    return ['model-a'];
  });
  await p.receive(request);
  await p.receive({ ...request, requestId: 2 });
  assert.deepEqual(p.replies, [
    { action: 'modelResult', requestId: 1, message: 'HTTP 401' },
    { action: 'modelResult', requestId: 2, models: ['model-a'] }
  ]);
  await p.receive({ action: 'cancel' });
  assert.equal(await result, false);
}));
