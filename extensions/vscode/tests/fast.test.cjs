const test = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const { fastAccess, patchFastSource, fastFiles, prepareFast } = require('../out/fast');
const config = { model_provider: 'easy_switch_vscode', model_catalog_json: '/plugin/fast-models.json', model_providers: { easy_switch_vscode: { requires_openai_auth: false } } };

test('Fast is scoped to the opted-in local provider and does not override other authentication', () => {
  for (const auth of [null, undefined, 'apiKey', 'apikey', 'chatgpt']) assert.equal(fastAccess('local', auth, config, false), true);
  for (const auth of ['personalAccessToken', 'amazonBedrock', 'copilot']) assert.equal(fastAccess('local', auth, config, true), false);
  assert.equal(fastAccess('remote', 'apiKey', config, false), false);
  assert.equal(fastAccess('remote', 'chatgpt', config, true), true);
  assert.equal(fastAccess('local', null, { ...config, profile: 'override' }, true), false);
  assert.equal(fastAccess('local', null, { ...config, model_catalog_json: undefined }, true), false);
  assert.equal(fastAccess('local', null, { ...config, model_providers: {} }, true), false);
  assert.equal(fastAccess('local', 'chatgpt', { model_provider: 'openai' }, true), true);
  assert.equal(fastAccess('local', 'apiKey', { model_provider: 'other' }, false), false);
});

test('queued explicit and default service tiers use current provider config, preserving Copilot exclusion', async () => {
  const fixture = 'function queue(e,{runtime:t}){return{async readServiceTier(){return run(async()=>{if(e.getHostId()===`local`&&await t.isUsingCopilotInference())return!1;let n=await e.getAccount();return n.account?.type===`chatgpt`||n.account?.type===`amazonBedrock`})},async writeServiceTier(){}}};async function explicit(e,u){let{account:t}=await e.getAccount();return t?.type===`chatgpt`||t?.type===`amazonBedrock`?u.serviceTier:null}';
  const context = vm.createContext({ run: fn => fn() });
  vm.runInContext(patchFastSource(fastFiles[2], fixture), context);
  let current = config;
  const manager = { getHostId: () => 'local', getAccount: async () => ({ account: { type: 'apiKey' } }), sendRequest: async () => ({ config: current }) };
  assert.equal(await context.explicit(manager, { serviceTier: 'priority' }), 'priority');
  assert.equal(await context.explicit(manager, { serviceTier: 'default' }), 'default');
  assert.equal(await context.queue(manager, { runtime: { isUsingCopilotInference: async () => false } }).readServiceTier(), true);
  assert.equal(await context.queue(manager, { runtime: { isUsingCopilotInference: async () => true } }).readServiceTier(), false);
  current = { ...config, model_catalog_json: undefined };
  assert.equal(await context.explicit(manager, { serviceTier: 'priority' }), null);
});

test('unknown versions and changed gate structures fail before writes', async () => {
  assert.throws(() => patchFastSource(fastFiles[0], 'changed'), /不匹配/);
  await assert.rejects(prepareFast('/unused', 'future'), /兼容检查/);
});
