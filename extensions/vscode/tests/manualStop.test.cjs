const test = require('node:test');
const assert = require('node:assert/strict');
const Module = require('node:module');
const vm = require('node:vm');

async function panelTest(check) {
  let receive, close, settings, disposed = false;
  const subscription = () => ({ dispose() {} });
  const panel = { webview: { html: '', onDidReceiveMessage(fn) { receive = fn; return subscription(); } }, onDidDispose(fn) { close = fn; return subscription(); }, dispose() { disposed = true; close(); } };
  const old = Module._load;
  Module._load = function(name, ...args) { return name === 'vscode' ? { ViewColumn: { Active: -1 }, window: { createWebviewPanel(_, title, column, options) { settings = options; return panel; } } } : old.call(this, name, ...args); };
  try {
    delete require.cache[require.resolve('../out/manualStop')];
    const { confirmManualStop } = require('../out/manualStop');
    await check({ confirmManualStop, panel, receive: value => receive(value), close: () => close(), settings: () => settings, disposed: () => disposed });
  } finally { Module._load = old; delete require.cache[require.resolve('../out/manualStop')]; }
}

test('warning uses theme-aware colored text, escapes details and requires an explicit button click', () => panelTest(async p => {
  const result = p.confirmManualStop('切换 <test>', '</div><script>bad()</script>', '切换并重载');
  const html = p.panel.webview.html;
  assert.match(html, /color: var\(--vscode-errorForeground/);
  assert.match(html, /仅停止生成或关闭侧边栏/);
  assert.match(html, /不会调用 sudo/);
  assert.match(html, /role="alert"/);
  assert.match(html, /&lt;script&gt;bad\(\)&lt;\/script&gt;/);
  assert.ok(!html.includes('<script>bad()'));
  assert.match(html, /default-src 'none'/);
  assert.deepEqual(p.settings().localResourceRoots, []);
  const elements = Object.fromEntries(['cancel', 'continue'].map(id => [id, { addEventListener(_, fn) { this.click = fn; } }]));
  const script = html.match(/<script nonce="[^"]+">([\s\S]*?)<\/script>/)[1];
  vm.runInNewContext(script, { acquireVsCodeApi: () => ({ postMessage: p.receive }), document: { getElementById: id => elements[id] } });
  elements.continue.click();
  assert.equal(await result, true); assert.equal(p.disposed(), true);
}));

test('cancel button and closing the page both cancel; unknown messages cannot confirm', () => panelTest(async p => {
  let result = p.confirmManualStop('修复', '原件保留', '修复');
  p.receive({ action: 'unknown' }); p.receive({ action: 'cancel' }); assert.equal(await result, false);
  result = p.confirmManualStop('恢复', '核对备份', '恢复');
  p.close(); assert.equal(await result, false);
}));
