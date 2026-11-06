// Run against an installed official extension; only disposable copies are modified.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const vm = require('node:vm');
const { execFileSync } = require('node:child_process');
const { fastFiles, prepareFast } = require('../out/fast');

(async () => {
  const official = process.argv[2];
  assert(official, 'pass the official Codex extension directory');
  const root = await fs.mkdtemp(path.join(os.tmpdir(), 'easy-switch-fast-proof-'));
  const assets = path.join(root, 'webview', 'assets');
  await fs.mkdir(assets, { recursive: true });
  try {
    const originals = new Map();
    for (const name of fastFiles) {
      const text = await fs.readFile(path.join(official, 'webview', 'assets', name), 'utf8');
      originals.set(name, text);
      await fs.writeFile(path.join(assets, name), text);
    }
    const apply = await prepareFast(root, '26.1021.22027');
    for (const [name, text] of originals) assert.equal(await fs.readFile(path.join(assets, name), 'utf8'), text);
    await apply();
    await (await prepareFast(root, '26.1021.22027'))();
    const patched = new Map();
    for (const name of fastFiles) {
      const source = await fs.readFile(path.join(assets, name), 'utf8');
      execFileSync(process.execPath, ['--check', '--input-type=module'], { input: source });
      patched.set(name, source);
    }
    const plain = source => source.replace(/import\{[^;]+?from"[^"]+";/g, '').replace(/export\{[^;]+;/g, '');
    const initial = await fs.readFile(path.join(official, 'webview', 'assets', 'app-initial-13f932cb2f13.js'), 'utf8');
    const mapper = initial.match(/function Vq\(e\)\{[^}]+\}\}/)[0];
    const normalize = vm.runInNewContext(`(${mapper})`);
    let config = { model_provider: 'easy_switch_vscode', model_catalog_json: '/plugin/fast-models.json', model_providers: { easy_switch_vscode: { requires_openai_auth: false } } };
    let auth = 'apiKey', requirements = { requirements: null };
    const query = Symbol('config');
    const ui = vm.createContext({ m: { c: () => [] }, s: null, u: null, o: () => 'local', d: () => ({ authMethod: auth }), esvConfigQuery: query,
      i: q => ({ data: q === query ? { config } : requirements, isPending: false }), n: normalize });
    vm.runInContext(plain(patched.get(fastFiles[0])), ui);
    assert.equal(ui.p().serviceTierAccess.fast, true);
    requirements = { requirements: { featureRequirements: { fast_mode: false } } };
    assert.equal(ui.p().serviceTierAccess.fast, false);
    config = { ...config, model_catalog_json: undefined };
    assert.equal(ui.p().serviceTierAccess, null);
    config = { model_provider: 'openai' }; auth = 'chatgpt'; requirements = { requirements: null };
    assert.equal(ui.p().serviceTierAccess.fast, true);

    const manager = { getHostId: () => 'local', sendRequest: async () => ({ config }) };
    const request = vm.createContext({ v: async () => auth, s: () => manager, h: async () => requirements, m: null, a: normalize });
    vm.runInContext(plain(patched.get(fastFiles[1])), request);
    const scope = { query: { setData() {} } };
    assert.equal((await request.x(scope, 'local')).fast, true);
    config = { model_provider: 'easy_switch_vscode' }; auth = 'apiKey';
    assert.equal(await request.x(scope, 'local'), null);

    const changed = path.join(assets, fastFiles[0]);
    await fs.appendFile(changed, '\n// external change');
    await assert.rejects(prepareFast(root, '26.1021.22027', true), /其他工具修改/);
    await fs.writeFile(changed, patched.get(fastFiles[0]));
    await (await prepareFast(root, '26.1021.22027', true))();
    for (const [name, text] of originals) assert.equal(await fs.readFile(path.join(assets, name), 'utf8'), text);
    console.log('PASS: three real modules parse; native UI and request gates honor Fast opt-in and restrictions; preparation is read-only; install is repeatable; restore is byte-exact; foreign edits are rejected.');
  } finally {
    assert.equal(path.dirname(root), os.tmpdir());
    assert(path.basename(root).startsWith('easy-switch-fast-proof-'));
    await fs.rm(root, { recursive: true, force: true });
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
