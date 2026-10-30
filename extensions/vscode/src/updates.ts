import * as https from 'node:https';
import * as vscode from 'vscode';

export const repository = 'yuan-me/easy-switch-vscode';
const day = 24 * 60 * 60 * 1000;
export interface Release { version: string; url: string }

function versionParts(value: string): number[] {
  if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(value)) throw new Error('版本号格式不受支持。');
  const parts = value.split('.').map(Number);
  if (parts.some(part => !Number.isSafeInteger(part))) throw new Error('版本号超出范围。');
  return parts;
}
export function newerVersion(candidate: string, installed: string): boolean {
  const next = versionParts(candidate), current = versionParts(installed);
  for (let i = 0; i < 3; i++) {
    if (next[i] !== current[i]) return next[i]! > current[i]!;
  }
  return false;
}
export function parseRelease(value: unknown): Release | undefined {
  if (!value || typeof value !== 'object') throw new Error('GitHub 返回的数据格式无效。');
  const data = value as Record<string, unknown>;
  if (data.draft !== false || data.prerelease !== false) return undefined;
  if (typeof data.tag_name !== 'string' || !data.tag_name.startsWith('v')) return undefined;
  const version = data.tag_name.slice(1);
  versionParts(version);
  if (!Array.isArray(data.assets)) throw new Error('GitHub 未返回安装包列表。');
  const name = `easy-switch-remote-${version}.vsix`;
  const download = `https://github.com/${repository}/releases/download/v${version}/${name}`;
  const asset = data.assets.find(item => item && item.name === name && item.state === 'uploaded'
    && Number.isSafeInteger(item.size) && item.size > 0 && item.browser_download_url === download);
  return asset ? { version, url: `https://github.com/${repository}/releases/tag/v${version}` } : undefined;
}

export function fetchLatestRelease(signal?: AbortSignal): Promise<Release | undefined> {
  // Use the extension host's Node HTTPS stack so VS Code's HTTP proxy handling applies.
  return new Promise((resolve, reject) => {
    const request = https.get(`https://api.github.com/repos/${repository}/releases/latest`, {
      signal,
      headers: { Accept: 'application/vnd.github+json', 'User-Agent': 'easy-switch-vscode', 'X-GitHub-Api-Version': '2022-11-28' }
    }, response => {
      if (response.statusCode === 404) { response.resume(); resolve(undefined); return; }
      if (response.statusCode !== 200) { response.resume(); reject(new Error(`GitHub HTTP ${response.statusCode}`)); return; }
      const chunks: Buffer[] = [];
      let size = 0;
      response.on('data', (chunk: Buffer) => {
        size += chunk.length;
        if (size > 1024 * 1024) { request.destroy(new Error('GitHub 响应过大。')); return; }
        chunks.push(chunk);
      });
      response.on('error', reject);
      response.on('end', () => {
        try { resolve(parseRelease(JSON.parse(Buffer.concat(chunks).toString('utf8')))); }
        catch (error) { reject(error); }
      });
    });
    // Bound DNS, proxy connection and response time as well as socket inactivity.
    const timeout = setTimeout(() => request.destroy(new Error('检查更新超时。')), 15_000);
    timeout.unref();
    request.once('close', () => clearTimeout(timeout));
    request.once('error', reject);
  });
}

export class UpdateChecker {
  private busy = false;
  private disposed = false;
  private readonly abort = new AbortController();
  constructor(private readonly context: vscode.ExtensionContext,
    private readonly fetchRelease: typeof fetchLatestRelease = fetchLatestRelease) {}
  dispose(): void { this.disposed = true; this.abort.abort(); }
  async check(manual = false): Promise<void> {
    if (this.disposed || this.busy) return;
    const state = this.context.globalState;
    const key = `updates:${repository}`;
    if (!manual) {
      if (!vscode.workspace.getConfiguration('easySwitch').get<boolean>('autoCheckUpdates', true)) return;
      const last = state.get<number>(`${key}:lastCheck`);
      if (typeof last === 'number' && Date.now() >= last && Date.now() - last < day) return;
    }
    this.busy = true;
    try {
      await state.update(`${key}:lastCheck`, Date.now());
      const release = await this.fetchRelease(this.abort.signal);
      if (this.disposed) return;
      if (!release) {
        if (manual) await vscode.window.showInformationMessage('GitHub 上还没有可用的正式安装包。');
        return;
      }
      const installed = this.context.extension.packageJSON.version as string;
      if (!newerVersion(release.version, installed)) {
        if (manual) await vscode.window.showInformationMessage(`Easy Switch ${installed} 已是最新正式版。`);
        return;
      }
      if (!manual && state.get<string>(`${key}:notified`) === release.version) return;
      await state.update(`${key}:notified`, release.version);
      const action = await vscode.window.showInformationMessage(`Easy Switch ${release.version} 可用，当前版本 ${installed}。`, '查看更新');
      if (action === '查看更新' && !this.disposed) {
        const opened = await vscode.env.openExternal(vscode.Uri.parse(release.url));
        if (!opened) await vscode.window.showWarningMessage(`无法打开发布页，请访问 ${release.url}`);
      }
    } catch {
      if (manual && !this.disposed) await vscode.window.showWarningMessage('检查更新失败。请检查服务器的网络或代理，然后重试。');
    } finally { this.busy = false; }
  }
}

export function registerUpdates(context: vscode.ExtensionContext): void {
  const updater = new UpdateChecker(context);
  const initial = setTimeout(() => { void updater.check(); }, 10_000);
  const periodic = setInterval(() => { void updater.check(); }, 60 * 60 * 1000);
  initial.unref(); periodic.unref();
  context.subscriptions.push(updater,
    vscode.commands.registerCommand('easySwitch.checkForUpdates', () => updater.check(true)),
    { dispose: () => { clearTimeout(initial); clearInterval(periodic); } });
}
