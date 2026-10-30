const test = require('node:test');
const assert = require('node:assert/strict');
const Module = require('node:module');
const https = require('node:https');
const { EventEmitter } = require('node:events');
const messages = [], opened = [];
let automatic = true, choice;
const vscode = {
  workspace: { getConfiguration: () => ({ get: () => automatic }) },
  window: {
    showInformationMessage: async (...args) => { messages.push(args); return choice; },
    showWarningMessage: async text => { messages.push([text]); }
  },
  Uri: { parse: value => value }, env: { openExternal: async value => { opened.push(value); return true; } }
};
const originalLoad = Module._load;
Module._load = function(name, ...args) { return name === 'vscode' ? vscode : originalLoad.call(this, name, ...args); };
const { repository, newerVersion, parseRelease, fetchLatestRelease, UpdateChecker } = require('../out/updates');
Module._load = originalLoad;
const release = version => ({ tag_name: `v${version}`, draft: false, prerelease: false, html_url: 'https://untrusted.invalid', assets: [{ name: `easy-switch-remote-${version}.vsix`, state: 'uploaded', size: 100,
  browser_download_url: `https://github.com/${repository}/releases/download/v${version}/easy-switch-remote-${version}.vsix` }] });
function context() {
  messages.length = opened.length = 0; automatic = true; choice = undefined;
  const state = new Map();
  return { extension: { packageJSON: { version: '0.1.4' } }, globalState: {
    get: key => state.get(key), update: async (key, value) => { state.set(key, value); }
  }, state };
}
test('numeric stable versions reject rollback, prerelease and malformed numbers', () => {
  assert.ok(newerVersion('0.1.10', '0.1.9'));
  assert.ok(newerVersion('1.0.0', '0.99.99'));
  assert.equal(newerVersion('0.1.4', '0.1.4'), false);
  assert.equal(newerVersion('0.1.3', '0.1.4'), false);
  for (const bad of ['0.1.5-beta', '01.2.3', '1.2', '999999999999999999999.0.0']) assert.throws(() => newerVersion(bad, '0.1.4'));
});
test('only a stable release with its exact uploaded VSIX produces a trusted link', () => {
  assert.deepEqual(parseRelease(release('0.1.5')), { version: '0.1.5', url: `https://github.com/${repository}/releases/tag/v0.1.5` });
  for (const flag of ['draft', 'prerelease']) assert.equal(parseRelease({ ...release('0.1.5'), [flag]: true }), undefined);
  for (const patch of [{ name: 'desktop.exe' }, { state: 'new' }, { size: 0 }, { browser_download_url: 'https://evil.invalid/package.vsix' }]) {
    const data = release('0.1.5'); Object.assign(data.assets[0], patch); assert.equal(parseRelease(data), undefined);
  }
  assert.equal(parseRelease({ ...release('0.1.5'), assets: [] }), undefined);
  assert.throws(() => parseRelease(null));
});
test('automatic checks are throttled and dismissed versions do not notify again', async () => {
  const ctx = context(); let calls = 0;
  const checker = new UpdateChecker(ctx, async () => { calls++; return parseRelease(release('0.1.5')); });
  await checker.check(); await checker.check(); assert.equal(calls, 1); assert.equal(messages.length, 1);
  ctx.state.set(`updates:${repository}:lastCheck`, 0);
  await checker.check(); assert.equal(calls, 2); assert.equal(messages.length, 1); assert.equal(opened.length, 0);
  checker.dispose();
});
test('manual checks work with automatic checks off and open only after a click', async () => {
  const ctx = context(); automatic = false; choice = '查看更新'; let calls = 0;
  const checker = new UpdateChecker(ctx, async () => { calls++; return parseRelease(release('0.1.5')); });
  await checker.check(); assert.equal(calls, 0);
  await checker.check(true); assert.deepEqual(opened, [`https://github.com/${repository}/releases/tag/v0.1.5`]);
  checker.dispose();
});
test('missing releases, latest version and failures have distinct manual messages; automatic failures stay quiet', async () => {
  for (const [response, expected] of [[undefined, '还没有'], [parseRelease(release('0.1.4')), '最新正式版']]) {
    const checker = new UpdateChecker(context(), async () => response);
    await checker.check(true); assert.ok(messages[0][0].includes(expected)); checker.dispose();
  }
  const checker = new UpdateChecker(context(), async () => { throw Error('sensitive transport details'); });
  await checker.check(); assert.equal(messages.length, 0);
  await checker.check(true); assert.ok(messages[0][0].includes('检查更新失败')); assert.ok(!messages[0][0].includes('sensitive'));
  checker.dispose();
});
test('concurrent checks share the busy guard; disposal aborts and suppresses notifications', async () => {
  const ctx = context(); let complete, signal, calls = 0;
  const checker = new UpdateChecker(ctx, async value => { calls++; signal = value; return new Promise(resolve => { complete = resolve; }); });
  const pending = checker.check(true); await new Promise(resolve => setImmediate(resolve));
  await checker.check(true); assert.equal(calls, 1);
  checker.dispose(); assert.equal(signal.aborted, true);
  complete(parseRelease(release('0.1.5'))); await pending; assert.equal(messages.length, 0);
});
test('HTTPS sends no credentials, rejects redirects and oversized or malformed replies', async () => {
  const saved = https.get;
  try {
    for (const [status, body, expected] of [[200, JSON.stringify(release('0.1.5')), 'ok'], [404, '', 'none'], [302, '', 'error'], [403, '', 'error'], [200, '{', 'error'], [200, 'x'.repeat(1024 * 1024 + 1), 'error']]) {
      https.get = (url, options, callback) => {
        assert.equal(url, `https://api.github.com/repos/${repository}/releases/latest`);
        assert.deepEqual(Object.keys(options.headers).sort(), ['Accept', 'User-Agent', 'X-GitHub-Api-Version']);
        const req = new EventEmitter(); req.destroy = error => { req.emit('error', error); req.emit('close'); };
        queueMicrotask(() => {
          const response = new EventEmitter(); response.statusCode = status; response.resume = () => {};
          callback(response); response.emit('data', Buffer.from(body)); response.emit('end'); req.emit('close');
        });
        return req;
      };
      if (expected === 'error') await assert.rejects(fetchLatestRelease());
      else assert.equal((await fetchLatestRelease())?.version, expected === 'ok' ? '0.1.5' : undefined);
    }
  } finally { https.get = saved; }
});
