const test = require('node:test');
const assert = require('node:assert/strict');
const { providersHtml } = require('../out/providersView');
const provider = { id: 'a', name: 'win99', model: 'model', nativeFast: true };

test('green Fast status follows applied config, not a saved opt-in', () => {
  const pending = providersHtml([provider], 'a', false, false, 'test');
  assert.ok(!pending.includes('class="fast"'));
  assert.ok(pending.includes('Fast 待启用'));
  const active = providersHtml([provider], 'a', true, false, 'test');
  assert.match(active, /class="fast"[^>]*>Fast 已启用/);
  assert.match(active, /color:var\(--vscode-testing-iconPassed/);
  const edited = providersHtml([{ ...provider, nativeFast: false }], 'a', true, false, 'test');
  assert.ok(edited.includes('Fast 已启用'));
  assert.ok(edited.includes('设置待应用'));
  assert.ok(!providersHtml([provider], 'official', true, false, 'test').includes('class="fast"'));
});

test('provider names are escaped and busy actions are disabled', () => {
  const html = providersHtml([{ ...provider, name: '<script>bad()</script>' }], 'a', true, true, 'test');
  assert.ok(!html.includes('<script>bad()'));
  assert.ok(html.includes('&lt;script&gt;bad()&lt;/script&gt;'));
  for (const button of html.matchAll(/<button[^>]*>/g)) assert.ok(button[0].includes('disabled'));
});
