import { createHash } from 'node:crypto';
import * as fs from 'node:fs/promises';
import * as path from 'node:path';

// These assets were checked against Codex 26.1021.22027. Updates require a new audit.
export const fastFiles = [
  'use-service-tier-availability-e114aaca7bc0.js',
  'read-service-tier-for-request-dbd681591676.js',
  'src-622b307a9baf.js',
] as const;

export function fastAccess(host: string, auth: string | null | undefined, config: any, original: boolean): boolean {
  if (host !== 'local') return original;
  if (!config) return false;
  if (config.model_provider !== 'easy_switch_vscode') return original;
  return (auth == null || ['apiKey', 'apikey', 'chatgpt', 'chatgptAuthTokens'].includes(auth))
    && config.profile == null
    && config.model_providers?.easy_switch_vscode?.requires_openai_auth === false
    && typeof config.model_catalog_json === 'string'
    && config.model_catalog_json.endsWith('/fast-models.json');
}

const allow = `async function esvAllow(manager,auth,original){
  if(manager.getHostId()!=="local")return original;
  const {config}=await manager.sendRequest("config/read",{includeLayers:false,cwd:null},{priority:"critical"});
  return fastAccess(manager.getHostId(),auth,config,original);
}`;

export function patchFastSource(name: string, source: string): string {
  const replacements: [string, string][] = name === fastFiles[0] ? [
    ['import{c as l,i as u}', 'import{b as esvConfigQuery,c as l,i as u}'],
    ['l=c?.authMethod===`chatgpt`||c?.authMethod===`personalAccessToken`||c?.authMethod===`amazonBedrock`,',
      'l=fastAccess(a,c?.authMethod,i(esvConfigQuery,a).data?.config,c?.authMethod===`chatgpt`||c?.authMethod===`personalAccessToken`||c?.authMethod===`amazonBedrock`),'],
  ] : name === fastFiles[1] ? [
    ['if(n!==`chatgpt`&&n!==`personalAccessToken`&&n!==`amazonBedrock`)return null;',
      'if(!await esvAllow(s(e,t),n,n===`chatgpt`||n===`personalAccessToken`||n===`amazonBedrock`))return null;'],
  ] : name === fastFiles[2] ? [
    ['return n.account?.type===`chatgpt`||n.account?.type===`amazonBedrock`})},async writeServiceTier(',
      'return await esvAllow(e,n.account?.type,n.account?.type===`chatgpt`||n.account?.type===`amazonBedrock`)})},async writeServiceTier('],
    ['return t?.type===`chatgpt`||t?.type===`amazonBedrock`?u.serviceTier:null',
      'return await esvAllow(e,t?.type,t?.type===`chatgpt`||t?.type===`amazonBedrock`)?u.serviceTier:null'],
  ] : [];
  if (!replacements.length) throw new Error('未知 Fast 模块。');
  for (const [before, after] of replacements) {
    if (source.split(before).length !== 2) throw new Error(`Codex Fast 结构不匹配：${name}。未覆盖文件。`);
    source = source.replace(before, after);
  }
  return `${source}\n/* Easy Switch: scoped Fast support */\n${fastAccess.toString()}\n${name === fastFiles[0] ? '' : allow}\n`;
}

const hashes: Record<string, string> = {
  [fastFiles[0]]: '9b7a547140be9fe30a7cfdb4ee01cab04ef20a1c242c9725cceff1629f4e0631',
  [fastFiles[1]]: '280cec40c1af0fe6123b4034cf2e21a8877d1110a93a43eb74e8e1f8bb428c9e',
  [fastFiles[2]]: 'e4a82a9ee669473490d25c1f2b6f7b60abc4a1b193acb1f9c0a92e074dc81eb5',
};
const hash = (text: string) => createHash('sha256').update(text).digest('hex');

export async function prepareFast(extensionPath: string, version: string, restore = false): Promise<() => Promise<void>> {
  if (version !== '26.1021.22027') throw new Error('当前 Codex 界面尚未通过 Fast 兼容检查。请关闭服务商 Fast 设置后重试。');
  const directory = path.join(extensionPath, 'webview', 'assets');
  const backup = path.join(directory, '.easy-switch-fast');
  for (const folder of [extensionPath, path.join(extensionPath, 'webview'), directory, backup]) {
    const info = await fs.lstat(folder).catch(error => {
      if (folder === backup && error.code === 'ENOENT') return undefined;
      throw error;
    });
    if (info && (!info.isDirectory() || info.isSymbolicLink())) throw new Error('Codex Fast 目录不是普通目录，未修改。');
  }
  const changes: { file: string; original: string; current: string; next: string }[] = [];
  for (const name of fastFiles) {
    const file = path.join(directory, name);
    const info = await fs.lstat(file);
    if (!info.isFile() || info.isSymbolicLink()) throw new Error('Codex Fast 模块不是普通文件，未修改。');
    const current = await fs.readFile(file, 'utf8');
    let original = current;
    if (hash(current) !== hashes[name]) {
      original = await fs.readFile(path.join(backup, name), 'utf8');
      if (hash(original) !== hashes[name] || current !== patchFastSource(name, original)) throw new Error('Codex 文件已被其他工具修改，请重新安装官方扩展后再启用 Fast。');
    }
    if (hash(original) !== hashes[name]) throw new Error('Codex 文件与已验证版本不一致，未应用 Fast。');
    changes.push({ file, original, current, next: restore ? original : patchFastSource(name, original) });
  }
  return async () => {
    await fs.mkdir(backup, { recursive: true });
    for (const change of changes) {
      if (change.current === change.next) continue;
      if (await fs.readFile(change.file, 'utf8') !== change.current) throw new Error('检查后 Codex 文件发生变化，已停止 Fast 更新。请重试。');
      const saved = path.join(backup, path.basename(change.file));
      await fs.writeFile(saved, change.original, { flag: 'wx' }).catch(async error => {
        if (error.code !== 'EEXIST' || await fs.readFile(saved, 'utf8') !== change.original) throw error;
      });
      const temp = `${change.file}.easy-switch.tmp`;
      await fs.writeFile(temp, change.next, { flag: 'wx' });
      await fs.rename(temp, change.file);
    }
  };
}
