import { spawn } from 'node:child_process';
import * as readline from 'node:readline';
import type { Provider } from './storage';
import type { Target } from './environment';
export interface Operation { id: string; state: string; description: string; fileCount: number }
export interface RepairPlan { fingerprint: string; fileCount: number; items: { path: string; line: number; detail: string; missingImage: boolean }[] }
export interface HelperResult { protocolVersion: 1; activeProviderId: string | null; operations?: Operation[]; sessionCount?: number | null; repairPlan?: RepairPlan; repairedFileCount?: number; configFingerprint?: string | null; committed?: boolean; home?: string; sqliteHome?: string | null; officialModel?: string | null }
export interface Progress { type: 'progress'; stage: string; detail: string; completed?: number; total?: number }
export interface HelperRequest extends Target {
  protocolVersion: 1; action: 'inspect' | 'switch' | 'recover' | 'repair'; storeRoot: string;
  provider?: Provider | { id: 'official' }; apiKey?: string; journalKey?: string;
  operationId?: string; expectedConfigFingerprint?: string; projectDirs?: string[]; officialModel?: string;
  repairFingerprint?: string;
}
export function runHelper(binary: string, request: HelperRequest, onProgress: (value: Progress) => void = () => {}): Promise<HelperResult> {
  return new Promise((resolve, reject) => {
    const child = spawn(binary, [], { stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true });
    let result: HelperResult | undefined;
    let failure: string | undefined;
    let settled = false;
    const secrets = [request.apiKey, request.journalKey].filter((s): s is string => !!s);
    const safe = (value: string) => secrets.reduce((s, secret) => s.split(secret).join('[已隐藏]'), value).slice(0, 1200);
    const done = (error?: Error) => {
      if (settled) return; settled = true;
      if (error) reject(error); else if (result) resolve(result); else reject(new Error('切换工具未返回完整结果，请刷新检查中断操作。'));
    };
    child.on('error', () => done(new Error('无法启动 Linux 切换工具。请确认 VSIX 包含对应架构的工具及执行权限。')));
    // Child stderr is never exposed because a dependency may include input in an error.
    child.stderr.on('data', () => {});
    child.stdin.on('error', () => { failure ??= '切换工具输入通道中断，请刷新检查操作状态。'; });
    const lines = readline.createInterface({ input: child.stdout, crlfDelay: Infinity });
    let received = 0;
    lines.on('line', line => {
      received += line.length;
      if (line.length > 8 * 1024 * 1024 || received > 16 * 1024 * 1024) { failure = '切换工具输出超过安全限制，请检查操作状态。'; return; }
      try {
        const value: unknown = JSON.parse(line);
        if (!value || typeof value !== 'object') throw new Error();
        const event = value as Record<string, unknown>;
        if (event.type !== 'result' && line.length > 64 * 1024) throw new Error();
        if (event.type === 'progress' && typeof event.stage === 'string' && typeof event.detail === 'string') onProgress({ type: 'progress', stage: safe(event.stage), detail: safe(event.detail), completed: typeof event.completed === 'number' ? event.completed : undefined, total: typeof event.total === 'number' ? event.total : undefined });
        else if (event.type === 'error' && typeof event.message === 'string') failure = safe(event.message);
        else if (event.type === 'result' && event.protocolVersion === 1 && (event.activeProviderId === null || typeof event.activeProviderId === 'string')) {
          if (result) throw new Error();
          if (event.repairPlan !== undefined) {
            const p = event.repairPlan as RepairPlan;
            if (!p || typeof p.fingerprint !== 'string' || !/^[a-fA-F0-9]{64}$/.test(p.fingerprint) || !Number.isSafeInteger(p.fileCount) || p.fileCount < 1 || !Array.isArray(p.items) || !p.items.length || p.items.some(i => !i || typeof i.path !== 'string' || typeof i.detail !== 'string' || !Number.isSafeInteger(i.line) || i.line < 1 || typeof i.missingImage !== 'boolean')) throw new Error();
            event.repairPlan = { ...p, items: p.items.map(i => ({ ...i, path: safe(i.path), detail: safe(i.detail) })) };
          }
          result = event as unknown as HelperResult;
        } else throw new Error();
      } catch { failure = '切换工具返回了无法识别的数据，请刷新检查操作状态。'; }
    });
    child.on('close', code => done(failure ? new Error(failure) : code !== 0 ? new Error('切换工具未正常结束，请先处理可能的中断操作。') : undefined));
    child.stdin.end(JSON.stringify(request) + '\n');
  });
}
