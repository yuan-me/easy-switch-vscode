const test = require('node:test');
const assert = require('node:assert/strict');
const path = require('node:path');
const { runHelper, progressMessage } = require('../out/helper');
test('subtask percentages use measured counts and never fabricate unknown progress', () => {
  const event = { type: 'progress', stage: '修复会话', detail: '处理文件' };
  assert.equal(progressMessage({ ...event, completed: 7, total: 10 }), '修复会话：处理文件 · 70%（7/10）');
  assert.match(progressMessage({ ...event, completed: 10, total: 10 }), /100%/);
  assert.match(progressMessage({ ...event, completed: 0, total: 10 }), /0%/);
  for (const counts of [{}, { completed: 0, total: 0 }, { completed: 2, total: 1 }, { completed: -1, total: 10 }, { completed: NaN, total: 10 }]) assert.equal(progressMessage({ ...event, ...counts }), '修复会话：处理文件');
});
const request = { protocolVersion: 1, action: 'inspect', home: '/synthetic', storeRoot: '/synthetic/store', apiKey: 'test-api-secret', journalKey: 'test-journal-secret', officialModel: 'official-model' };
async function fixture(callback) {
  const before = process.env.NODE_OPTIONS;
  process.env.NODE_OPTIONS = `--require ${JSON.stringify(path.join(__dirname, 'fixtures', 'helper.cjs'))}`;
  try { return await callback(); }
  finally { if (before === undefined) delete process.env.NODE_OPTIONS; else process.env.NODE_OPTIONS = before; }
}
test('helper secrets are sent through stdin with no command-line arguments; progress is redacted', () => fixture(async () => {
  const progress = [];
  const result = await runHelper(process.execPath, request, item => progress.push(item));
  assert.equal(result.hasKey, true);
  assert.deepEqual(result.childArgs, []);
  assert.equal(result.officialModel, request.officialModel);
  assert.equal(progress.length, 1);
  assert.ok(!progress[0].detail.includes(request.apiKey));
}));
test('structured helper errors cannot expose supplied secrets', () => fixture(async () => {
  await assert.rejects(runHelper(process.execPath, { ...request, action: 'switch' }), error => {
    assert.ok(!error.message.includes(request.apiKey));
    assert.ok(!error.message.includes(request.journalKey));
    assert.match(error.message, /已隐藏/);
    return true;
  });
}));
test('missing protocol version is rejected instead of showing a false success', () => fixture(async () => {
  await assert.rejects(runHelper(process.execPath, { ...request, operationId: 'malformed' }), /无法识别/);
}));

test('large repair reports retain all paths and redact request secrets', () => fixture(async () => {
  const result = await runHelper(process.execPath, { ...request, operationId: 'repair-report' });
  assert.equal(result.repairPlan.items.length, 500);
  assert.ok(result.repairPlan.items[499].path.includes('499-'));
  assert.ok(!JSON.stringify(result).includes(request.apiKey));
  assert.ok(!JSON.stringify(result).includes(request.journalKey));
}));

test('malformed repair reports are rejected', () => fixture(async () => {
  await assert.rejects(runHelper(process.execPath, { ...request, operationId: 'bad-report' }), /无法识别/);
}));

test('settings initialization previews validate counts and redact secrets', () => fixture(async () => {
  const result = await runHelper(process.execPath, { ...request, operationId: 'settings-report' });
  assert.equal(result.settingsPlan.fileCount, 1);
  assert.ok(!JSON.stringify(result).includes(request.apiKey));
  await assert.rejects(runHelper(process.execPath, { ...request, operationId: 'bad-settings' }), /无法识别/);
}));
